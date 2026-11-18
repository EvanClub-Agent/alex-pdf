use axum::{extract::State, Json};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering as AtomicOrdering;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::AsyncWriteExt;

use crate::state::{AppState, UpdateProgress, UpdateStatus};

const UPDATE_URL_ENV: &str = "PDF_CONVERTER_UPDATE_URL";
const UPDATE_DIR: &str = "pdf-converter-update";
const PART_SUFFIX: &str = ".part";
const META_SUFFIX: &str = ".meta.json";
const PENDING_INSTALL_FILE: &str = "pending-install.json";

#[derive(Debug, Deserialize)]
pub struct RemoteUpdateInfo {
  pub version: String,
  pub url: String,
  pub notes: Option<String>,
  #[serde(rename = "pubDate")]
  pub pub_date: Option<String>,
  pub sha256: String,
  #[serde(rename = "fileSize", default)]
  pub file_size: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateCheckResponse {
  pub success: bool,
  pub current_version: String,
  pub latest_version: Option<String>,
  pub update_available: bool,
  pub download_url: Option<String>,
  pub notes: Option<String>,
  pub pub_date: Option<String>,
  pub sha256: Option<String>,
  pub error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateInstallRequest {
  pub url: String,
  pub sha256: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateInstallResponse {
  pub success: bool,
  pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PartMeta {
  url: String,
  sha256: Option<String>,
  total: u64,
  /// 写入时单调递增的辅助字段，用于将来扩展。
  saved_at_unix: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct PendingInstall {
  pub expected_version: String,
  pub started_at_unix: i64,
  pub installer_path: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PendingInstallResponse {
  pub pending: Option<PendingInstall>,
  pub current_version: String,
  /// 当 expected_version > current_version 时认为安装失败需要回滚提示。
  pub failed: bool,
}

// --------------------------------------------------------------------------- //
//   /api/update/check
// --------------------------------------------------------------------------- //

#[utoipa::path(
  get,
  path = "/api/update/check",
  tag = "update",
  responses((status = 200, description = "更新信息", body = UpdateCheckResponse))
)]
pub async fn check_update() -> Json<UpdateCheckResponse> {
  let current_version = env!("CARGO_PKG_VERSION").to_string();

  match fetch_update_info().await {
    Ok(remote) => {
      let update_available =
        compare_versions(&remote.version, &current_version) == Ordering::Greater;
      Json(UpdateCheckResponse {
        success: true,
        current_version,
        latest_version: Some(remote.version),
        update_available,
        download_url: Some(remote.url),
        notes: remote.notes,
        pub_date: remote.pub_date,
        sha256: Some(remote.sha256),
        error: None,
      })
    }
    Err(err) => Json(UpdateCheckResponse {
      success: false,
      current_version,
      latest_version: None,
      update_available: false,
      download_url: None,
      notes: None,
      pub_date: None,
      sha256: None,
      error: Some(err),
    }),
  }
}

// --------------------------------------------------------------------------- //
//   /api/update/install  (异步任务，支持续传)
// --------------------------------------------------------------------------- //

#[utoipa::path(
  post,
  path = "/api/update/install",
  tag = "update",
  request_body = UpdateInstallRequest,
  responses((status = 200, description = "安装请求受理", body = UpdateInstallResponse))
)]
pub async fn install_update(
  State(state): State<Arc<AppState>>,
  Json(payload): Json<UpdateInstallRequest>,
) -> Json<UpdateInstallResponse> {
  let remote = match fetch_update_info().await {
    Ok(remote) => remote,
    Err(message) => return Json(UpdateInstallResponse { success: false, message }),
  };
  if compare_versions(&remote.version, env!("CARGO_PKG_VERSION")) != Ordering::Greater
    || payload.url != remote.url
    || payload.sha256.as_ref().is_some_and(|sha| !sha.eq_ignore_ascii_case(&remote.sha256))
  {
    return Json(UpdateInstallResponse {
      success: false,
      message: "安装请求与更新服务的最新版本不符，请重新检查更新".to_string(),
    });
  }

  // 已经在跑或已经就绪 → 不重复启动。
  {
    let mut progress = state.update_progress.lock().await;
    if matches!(
      progress.status,
      UpdateStatus::Downloading | UpdateStatus::Verifying
    ) {
      return Json(UpdateInstallResponse {
        success: true,
        message: "下载已在进行中".to_string(),
      });
    }
    if matches!(progress.status, UpdateStatus::Ready) && progress.installer_path.as_ref()
      .and_then(|p| Path::new(p).file_stem())
      .is_some_and(|stem| stem == std::ffi::OsStr::new(&format!("AlexPDF-{}", remote.sha256))) {
      return Json(UpdateInstallResponse {
        success: true,
        message: "安装包已就绪".to_string(),
      });
    }
    state.update_cancel_flag.store(false, AtomicOrdering::SeqCst);
    *progress = UpdateProgress { status: UpdateStatus::Downloading, ..Default::default() };
  }

  let state_clone = state.clone();
  tokio::spawn(async move {
    match download_with_progress(&state_clone, &remote.url, &remote.sha256).await {
      Ok(path) => {
        let mut progress = state_clone.update_progress.lock().await;
        progress.status = UpdateStatus::Ready;
        progress.installer_path = Some(path.display().to_string());
        progress.error = None;
      }
      Err(DownloadError::Cancelled) => {
        let mut progress = state_clone.update_progress.lock().await;
        progress.status = UpdateStatus::Cancelled;
        progress.error = Some("已取消下载（下次可继续）".to_string());
      }
      Err(DownloadError::Other(message)) => {
        let mut progress = state_clone.update_progress.lock().await;
        progress.status = UpdateStatus::Failed;
        progress.error = Some(message);
      }
    }
  });

  Json(UpdateInstallResponse {
    success: true,
    message: "已开始下载".to_string(),
  })
}

// --------------------------------------------------------------------------- //
//   /api/update/progress, /cancel, /launch
// --------------------------------------------------------------------------- //

#[utoipa::path(
  get,
  path = "/api/update/progress",
  tag = "update",
  responses((status = 200, description = "下载进度", body = UpdateProgress))
)]
pub async fn update_progress(State(state): State<Arc<AppState>>) -> Json<UpdateProgress> {
  Json(state.update_progress.lock().await.clone())
}

#[utoipa::path(
  post,
  path = "/api/update/cancel",
  tag = "update",
  responses((status = 200, description = "已请求取消", body = UpdateInstallResponse))
)]
pub async fn cancel_update(State(state): State<Arc<AppState>>) -> Json<UpdateInstallResponse> {
  let progress = state.update_progress.lock().await.clone();
  if !matches!(
    progress.status,
    UpdateStatus::Downloading | UpdateStatus::Verifying
  ) {
    return Json(UpdateInstallResponse {
      success: false,
      message: "当前没有进行中的下载".to_string(),
    });
  }
  state.update_cancel_flag.store(true, AtomicOrdering::SeqCst);
  Json(UpdateInstallResponse {
    success: true,
    message: "已发送取消信号，进度文件保留以便续传".to_string(),
  })
}

#[utoipa::path(
  post,
  path = "/api/update/launch",
  tag = "update",
  responses((status = 200, description = "已启动安装程序", body = UpdateInstallResponse))
)]
pub async fn launch_update(State(state): State<Arc<AppState>>) -> Json<UpdateInstallResponse> {
  let installer_path = {
    let progress = state.update_progress.lock().await;
    if !matches!(progress.status, UpdateStatus::Ready) {
      return Json(UpdateInstallResponse {
        success: false,
        message: "安装包尚未就绪".to_string(),
      });
    }
    progress.installer_path.clone()
  };

  let path = match installer_path {
    Some(p) => PathBuf::from(p),
    None => {
      return Json(UpdateInstallResponse {
        success: false,
        message: "找不到安装包路径".to_string(),
      });
    }
  };

  // 写入 pending-install 标记。下次启动时如果版本没升上去，会触发回滚提示。
  let pending = PendingInstall {
    expected_version: env!("CARGO_PKG_VERSION").to_string(),
    started_at_unix: now_unix(),
    installer_path: path.display().to_string(),
  };
  // expected_version 写当前版本看起来反直觉 —— 但是我们这里其实想记录"安装前版本"，
  // 这样下次启动如果运行的还是同一个版本（== expected_version），就说明安装失败了。
  if let Err(err) = write_pending_install(&pending) {
    tracing::warn!("write pending-install marker failed: {}", err);
  }

  match spawn_installer(&path) {
    Ok(()) => {
      let mut progress = state.update_progress.lock().await;
      progress.status = UpdateStatus::Launched;
      Json(UpdateInstallResponse {
        success: true,
        message: "已启动安装程序".to_string(),
      })
    }
    Err(err) => {
      // 安装包没拉起来 → 直接清除 pending 标记，避免下次误报。
      let _ = std::fs::remove_file(pending_install_path());
      Json(UpdateInstallResponse {
        success: false,
        message: err,
      })
    }
  }
}

// --------------------------------------------------------------------------- //
//   /api/update/pending-install  +  /clear-pending-install
// --------------------------------------------------------------------------- //

#[utoipa::path(
  get,
  path = "/api/update/pending-install",
  tag = "update",
  responses((status = 200, description = "待安装状态", body = PendingInstallResponse))
)]
pub async fn pending_install_status() -> Json<PendingInstallResponse> {
  let current_version = env!("CARGO_PKG_VERSION").to_string();
  let pending = read_pending_install();
  let failed = match &pending {
    Some(p) => p.expected_version == current_version,
    None => false,
  };
  Json(PendingInstallResponse {
    pending,
    current_version,
    failed,
  })
}

#[utoipa::path(
  post,
  path = "/api/update/clear-pending-install",
  tag = "update",
  responses((status = 200, description = "已清除", body = UpdateInstallResponse))
)]
pub async fn clear_pending_install() -> Json<UpdateInstallResponse> {
  match std::fs::remove_file(pending_install_path()) {
    Ok(()) => Json(UpdateInstallResponse {
      success: true,
      message: "已清除".to_string(),
    }),
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Json(UpdateInstallResponse {
      success: true,
      message: "无需清除".to_string(),
    }),
    Err(err) => Json(UpdateInstallResponse {
      success: false,
      message: err.to_string(),
    }),
  }
}

// --------------------------------------------------------------------------- //
//   下载主体（带 Range 续传 + 取消信号）
// --------------------------------------------------------------------------- //

enum DownloadError {
  Cancelled,
  Other(String),
}

impl From<String> for DownloadError {
  fn from(s: String) -> Self {
    DownloadError::Other(s)
  }
}

async fn download_with_progress(
  state: &Arc<AppState>,
  url: &str,
  expected_sha256: &str,
) -> Result<PathBuf, DownloadError> {
  let dir = std::env::temp_dir().join(UPDATE_DIR);
  tokio::fs::create_dir_all(&dir)
    .await
    .map_err(|err| err.to_string())?;

  let extension = installer_extension(url)?;
  let file_name = format!("AlexPDF-{}.{}", expected_sha256, extension);
  let final_path = dir.join(&file_name);
  let part_path = dir.join(format!("{}{}", file_name, PART_SUFFIX));
  let meta_path = dir.join(format!("{}{}", file_name, META_SUFFIX));

  // 检查可不可以续传
  let mut resume_from: u64 = 0;
  let saved_meta = read_meta(&meta_path);
  if let Some(meta) = saved_meta.as_ref() {
    if meta.url == url && meta.sha256.as_deref() == Some(expected_sha256) {
      if let Ok(metadata) = tokio::fs::metadata(&part_path).await {
        let part_len = metadata.len();
        if part_len > 0 && part_len < meta.total {
          resume_from = part_len;
        }
      }
    } else {
      // URL/sha 变了 → 旧的不能用了，清理。
      let _ = tokio::fs::remove_file(&part_path).await;
      let _ = tokio::fs::remove_file(&meta_path).await;
    }
  }

  let client = reqwest::Client::new();
  let mut request = client.get(url);
  if resume_from > 0 {
    request = request.header(RANGE, format!("bytes={}-", resume_from));
  }
  let response = request
    .send()
    .await
    .map_err(|err| err.to_string())?;

  let status = response.status();
  let mut hasher = Sha256::new();
  let total: u64;
  let is_resume_effective: bool;
  let mut downloaded: u64;

  match status {
    StatusCode::PARTIAL_CONTENT => {
      // 服务端接受了 Range
      total = parse_total_from_content_range(&response).unwrap_or_else(|| {
        // 退化：Content-Length + resume_from
        response.content_length().unwrap_or(0) + resume_from
      });
      downloaded = resume_from;
      is_resume_effective = resume_from > 0;
      // 之前累积的字节也要喂给 hasher
      if resume_from > 0 {
        feed_hasher_from_part(&part_path, &mut hasher).await?;
      }
    }
    StatusCode::OK => {
      // 服务端不支持 Range（或我们没要 Range）→ 全量重下
      total = response.content_length().unwrap_or(0);
      downloaded = 0;
      is_resume_effective = false;
      // 删除旧的 .part
      let _ = tokio::fs::remove_file(&part_path).await;
    }
    other if other.is_success() => {
      total = response.content_length().unwrap_or(0);
      downloaded = 0;
      is_resume_effective = false;
      let _ = tokio::fs::remove_file(&part_path).await;
    }
    other => {
      return Err(format!("下载失败 {}", other).into());
    }
  }

  // 写入/更新 meta
  let meta = PartMeta {
    url: url.to_string(),
    sha256: Some(expected_sha256.to_owned()),
    total,
    saved_at_unix: now_unix(),
  };
  if let Err(err) = write_meta(&meta_path, &meta) {
    tracing::warn!("write meta file failed: {}", err);
  }

  {
    let mut progress = state.update_progress.lock().await;
    progress.total = total;
    progress.downloaded = downloaded;
    progress.is_resume = is_resume_effective;
  }

  // 打开文件：续传时 append，否则 create/truncate。
  let mut file = if is_resume_effective {
    tokio::fs::OpenOptions::new()
      .append(true)
      .open(&part_path)
      .await
      .map_err(|err| err.to_string())?
  } else {
    tokio::fs::File::create(&part_path)
      .await
      .map_err(|err| err.to_string())?
  };

  let mut last_emit: u64 = downloaded;
  let mut stream = response.bytes_stream();

  while let Some(chunk) = stream.next().await {
    if state.update_cancel_flag.load(AtomicOrdering::SeqCst) {
      // 把当前进度落到磁盘并退出 —— .part 保留供下次续传。
      let _ = file.flush().await;
      return Err(DownloadError::Cancelled);
    }

    let chunk = chunk.map_err(|err| err.to_string())?;
    hasher.update(&chunk);
    file.write_all(&chunk).await.map_err(|err| err.to_string())?;
    downloaded += chunk.len() as u64;

    if downloaded - last_emit > 256 * 1024 {
      last_emit = downloaded;
      let mut progress = state.update_progress.lock().await;
      progress.downloaded = downloaded;
    }
  }
  file.flush().await.map_err(|err| err.to_string())?;
  drop(file);

  {
    let mut progress = state.update_progress.lock().await;
    progress.downloaded = downloaded;
    progress.status = UpdateStatus::Verifying;
  }

  let actual = hex_lower(&hasher.finalize());
  if actual != expected_sha256 {
    let _ = tokio::fs::remove_file(&part_path).await;
    let _ = tokio::fs::remove_file(&meta_path).await;
    return Err(format!("SHA256 校验失败 (期望 {}, 实际 {})", short(expected_sha256), short(&actual)).into());
  }

  // 收尾：.part → 实际文件 ；删除 meta
  if final_path.exists() {
    let _ = tokio::fs::remove_file(&final_path).await;
  }
  tokio::fs::rename(&part_path, &final_path)
    .await
    .map_err(|err| err.to_string())?;
  let _ = tokio::fs::remove_file(&meta_path).await;

  Ok(final_path)
}

async fn feed_hasher_from_part(path: &Path, hasher: &mut Sha256) -> Result<(), DownloadError> {
  use tokio::io::AsyncReadExt;
  let mut file = tokio::fs::File::open(path)
    .await
    .map_err(|err| err.to_string())?;
  let mut buf = vec![0u8; 1024 * 1024];
  loop {
    let n = file.read(&mut buf).await.map_err(|err| err.to_string())?;
    if n == 0 {
      break;
    }
    hasher.update(&buf[..n]);
  }
  Ok(())
}

fn parse_total_from_content_range(response: &reqwest::Response) -> Option<u64> {
  let header = response.headers().get(CONTENT_RANGE)?.to_str().ok()?;
  // bytes <start>-<end>/<total>
  let slash = header.rfind('/')?;
  let total_str = &header[slash + 1..];
  total_str.parse::<u64>().ok()
}

// --------------------------------------------------------------------------- //
//   meta 持久化 / pending-install 持久化
// --------------------------------------------------------------------------- //

fn read_meta(path: &Path) -> Option<PartMeta> {
  let raw = std::fs::read_to_string(path).ok()?;
  serde_json::from_str(&raw).ok()
}

fn write_meta(path: &Path, meta: &PartMeta) -> Result<(), String> {
  let json = serde_json::to_string_pretty(meta).map_err(|err| err.to_string())?;
  std::fs::write(path, json).map_err(|err| err.to_string())
}

fn pending_install_path() -> PathBuf {
  std::env::temp_dir().join(UPDATE_DIR).join(PENDING_INSTALL_FILE)
}

fn read_pending_install() -> Option<PendingInstall> {
  let raw = std::fs::read_to_string(pending_install_path()).ok()?;
  serde_json::from_str(&raw).ok()
}

fn write_pending_install(pending: &PendingInstall) -> Result<(), String> {
  let path = pending_install_path();
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
  }
  let json = serde_json::to_string_pretty(pending).map_err(|err| err.to_string())?;
  std::fs::write(&path, json).map_err(|err| err.to_string())
}

// --------------------------------------------------------------------------- //
//   工具函数
// --------------------------------------------------------------------------- //

fn now_unix() -> i64 {
  SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

fn hex_lower(bytes: &[u8]) -> String {
  let mut s = String::with_capacity(bytes.len() * 2);
  for b in bytes {
    s.push_str(&format!("{:02x}", b));
  }
  s
}

fn short(s: &str) -> String {
  if s.len() <= 12 {
    s.to_string()
  } else {
    format!("{}…", &s[..12])
  }
}

async fn fetch_update_info() -> Result<RemoteUpdateInfo, String> {
  let configured = std::env::var(UPDATE_URL_ENV).map_err(|_| format!("未配置更新地址 {UPDATE_URL_ENV}"))?;
  let update_url = trusted_update_url(configured.trim())?;
  let client = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(std::time::Duration::from_secs(15))
    .build().map_err(|err| err.to_string())?;
  let response = client.get(update_url).send()
    .await
    .map_err(|err| err.to_string())?;
  if !response.status().is_success() {
    return Err(format!("更新服务返回 {}", response.status()));
  }
  let mut info: RemoteUpdateInfo = response.json().await.map_err(|err| err.to_string())?;
  installer_extension(&info.url)?;
  if info.sha256.len() != 64 || !info.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Err("更新清单缺少有效的 SHA256 校验值".into());
  }
  info.sha256.make_ascii_lowercase();
  Ok(info)
}

fn trusted_update_url(input: &str) -> Result<reqwest::Url, String> {
  let url = reqwest::Url::parse(input).map_err(|_| "更新地址格式无效".to_string())?;
  if url.scheme() == "https" || (url.scheme() == "http"
    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
  {
    return Ok(url);
  }
  Err("远程更新地址必须使用 HTTPS，本机测试可使用 HTTP".into())
}

fn installer_extension(input: &str) -> Result<&'static str, String> {
  let url = trusted_update_url(input)?;
  match Path::new(url.path()).extension().and_then(|s| s.to_str()) {
    Some(ext) if ext.eq_ignore_ascii_case("exe") => Ok("exe"),
    Some(ext) if ext.eq_ignore_ascii_case("msi") => Ok("msi"),
    _ => Err("更新清单中的下载地址必须指向 .exe 或 .msi 安装包".into()),
  }
}

#[cfg(target_os = "windows")]
fn spawn_installer(path: &Path) -> Result<(), String> {
  use std::os::windows::process::CommandExt;

  const DETACHED_PROCESS: u32 = 0x00000008;
  const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

  let mut cmd = if path.extension().is_some_and(|ext| ext == "msi") {
    let mut cmd = std::process::Command::new("msiexec.exe");
    cmd.arg("/i").arg(path);
    cmd
  } else {
    std::process::Command::new(path)
  };
  cmd
    .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
    .spawn()
    .map(|_| ())
    .map_err(|err| err.to_string())
}

#[cfg(not(target_os = "windows"))]
fn spawn_installer(_path: &Path) -> Result<(), String> {
  Err("仅支持 Windows 安装程序".to_string())
}

fn compare_versions(a: &str, b: &str) -> Ordering {
  let parse = |s: &str| {
    s.split('.')
      .map(|part| {
        part
          .chars()
          .take_while(|c| c.is_ascii_digit())
          .collect::<String>()
          .parse::<u64>()
          .unwrap_or(0)
      })
      .collect::<Vec<_>>()
  };

  let mut left = parse(a);
  let mut right = parse(b);
  let max_len = left.len().max(right.len());
  left.resize(max_len, 0);
  right.resize(max_len, 0);
  left.cmp(&right)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn manifest_transport_and_installer_type() {
    assert!(trusted_update_url("https://updates.example.com/update.json").is_ok());
    assert!(trusted_update_url("http://127.0.0.1:8800/update.json").is_ok());
    assert!(trusted_update_url("http://updates.example.com/update.json").is_err());
    assert!(trusted_update_url("").is_err());
    assert_eq!(installer_extension("https://example.com/AlexPDF.EXE?download=1").unwrap(), "exe");
    assert!(installer_extension("https://example.com/release.zip").is_err());
  }
}
