//! 监听文件夹自动转换。
//!
//! 周期 (默认 5s) 扫描 settings 中的 watch_folders；对每个支持的源文件做"稳定性判定"——
//! 连续两次扫描 mtime + size 完全一致才视为写入完成，避免边写边转。稳定后异步触发一次
//! wps_com::convert_to_pdf，结果按桌面任务状态机走（state.tasks + history）。
//!
//! 不引入 notify crate；轮询足以覆盖小型批处理场景，避免跨平台 inotify / kqueue 适配工作。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::settings::{self, WatchFolder};
use crate::state::{AppState, FileResult, TaskState};
use crate::wps_com;
use crate::wps_com::types::DocumentKind;

const POLL_INTERVAL_SECS: u64 = 5;
const STABLE_AFTER_SECS: u64 = 5;

#[derive(Debug, Clone)]
struct Tracked {
  size: u64,
  mtime_unix_ms: i128,
  /// 第一次稳定的时刻；保证 size+mtime 至少持续 STABLE_AFTER_SECS 后才转。
  stable_since: Option<Instant>,
  processed: bool,
}

pub async fn run(state: Arc<AppState>) {
  let mut tracked: HashMap<PathBuf, Tracked> = HashMap::new();
  let mut tick = tokio::time::interval(Duration::from_secs(POLL_INTERVAL_SECS));
  loop {
    tick.tick().await;
    let folders = settings::read_desktop_settings().watch_folders;
    if folders.is_empty() {
      // 配置清空后保留 tracked 缓存即可，不会泄漏；下次扫到也不会处理已 processed 的项。
      continue;
    }
    for folder in folders {
      if let Err(err) = scan_folder(&state, &folder, &mut tracked).await {
        warn!("watcher scan {} failed: {}", folder.src, err);
      }
    }
  }
}

async fn scan_folder(
  state: &Arc<AppState>,
  folder: &WatchFolder,
  tracked: &mut HashMap<PathBuf, Tracked>,
) -> std::io::Result<()> {
  let src = PathBuf::from(&folder.src);
  if !src.is_dir() {
    return Ok(());
  }
  let dst = PathBuf::from(&folder.dst);
  std::fs::create_dir_all(&dst)?;

  for entry in std::fs::read_dir(&src)? {
    let entry = entry?;
    let path = entry.path();
    if !path.is_file() {
      continue;
    }
    let ext = path
      .extension()
      .and_then(|e| e.to_str())
      .unwrap_or("")
      .to_ascii_lowercase();
    if DocumentKind::from_extension(&ext).is_none() {
      continue;
    }
    let metadata = match entry.metadata() {
      Ok(m) => m,
      Err(_) => continue,
    };
    let size = metadata.len();
    let mtime_unix_ms = metadata
      .modified()
      .ok()
      .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
      .map(|d| d.as_millis() as i128)
      .unwrap_or(0);

    let key = path.clone();
    let now = Instant::now();
    let entry_state = tracked.entry(key.clone()).or_insert(Tracked {
      size,
      mtime_unix_ms,
      stable_since: None,
      processed: false,
    });

    if entry_state.processed {
      continue;
    }
    if entry_state.size != size || entry_state.mtime_unix_ms != mtime_unix_ms {
      // 文件还在被写入；重置稳定计时。
      entry_state.size = size;
      entry_state.mtime_unix_ms = mtime_unix_ms;
      entry_state.stable_since = None;
      continue;
    }
    let stable_since = entry_state.stable_since.get_or_insert(now);
    if now.duration_since(*stable_since) < Duration::from_secs(STABLE_AFTER_SECS) {
      continue;
    }

    // 稳定够久，触发转换。
    entry_state.processed = true;
    let state = Arc::clone(state);
    let src_path = path.clone();
    let dst_dir = dst.clone();
    let delete_source = folder.delete_source;
    tokio::spawn(async move {
      run_conversion(state, src_path, dst_dir, delete_source).await;
    });
  }
  Ok(())
}

async fn run_conversion(
  state: Arc<AppState>,
  src: PathBuf,
  dst_dir: PathBuf,
  delete_source: bool,
) {
  let file_name = src
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("")
    .to_string();
  let kind = match src
    .extension()
    .and_then(|e| e.to_str())
    .and_then(DocumentKind::from_extension)
  {
    Some(k) => k,
    None => return,
  };
  let stem = src
    .file_stem()
    .and_then(|s| s.to_str())
    .unwrap_or("output");
  let target_path = dst_dir.join(format!("{}.pdf", stem));
  let strategy = settings::read_collision_strategy();
  let final_path = match settings::resolve_output_path(&target_path, strategy) {
    Some(p) => p,
    None => {
      info!("watcher: skip existing {}", target_path.display());
      return;
    }
  };

  let task_id = state.create_task(1);
  state.update_task(task_id, |t| {
    t.status = TaskState::Processing;
    t.current_file = Some(file_name.clone());
  });

  let engine = settings::read_desktop_settings().engine;
  let prog_id = match tokio::task::spawn_blocking(move || wps_com::check_engine(engine, kind)).await {
    Ok(Ok(id)) => id,
    Ok(Err(err)) => {
      state.update_task(task_id, |t| {
        t.finalize_failed(err.to_string(), err.code().to_string());
      });
      state.record_history(task_id);
      return;
    }
    Err(err) => {
      state.update_task(task_id, |t| {
        t.finalize_failed(err.to_string(), "worker_panicked".to_string());
      });
      state.record_history(task_id);
      return;
    }
  };

  let src_for_blocking = src.clone();
  let final_path_for_blocking = final_path.clone();
  let result = tokio::task::spawn_blocking(move || {
    wps_com::convert_to_pdf(kind, &prog_id, &src_for_blocking, &final_path_for_blocking, None)
  })
  .await;

  match result {
    Ok(Ok(conversion)) => {
      state.update_task(task_id, |t| {
        t.completed_files = 1;
        t.results = vec![FileResult {
          file: file_name.clone(),
          status: "success".to_string(),
          output: Some(conversion.output.display().to_string()),
          size: Some(conversion.file_size),
          duration_ms: Some(conversion.duration_ms),
          error: None,
          error_code: None,
        }];
        t.finalize(0);
      });
      if delete_source {
        let _ = tokio::fs::remove_file(&src).await;
      }
    }
    Ok(Err(err)) => {
      state.update_task(task_id, |t| {
        t.results = vec![FileResult {
          file: file_name,
          status: "failed".to_string(),
          output: None,
          size: None,
          duration_ms: None,
          error: Some(err.to_string()),
          error_code: Some(err.code().to_string()),
        }];
        t.finalize_failed(err.to_string(), err.code().to_string());
      });
    }
    Err(err) => {
      state.update_task(task_id, |t| {
        t.finalize_failed(err.to_string(), "worker_panicked".to_string());
      });
    }
  }
  state.record_history(task_id);
  let _ = final_path; // 抑制 unused 警告
  let _: &Path = &dst_dir;
}
