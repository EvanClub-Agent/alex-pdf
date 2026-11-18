use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
  extract::multipart::MultipartError,
  extract::Multipart,
  extract::Query,
  extract::State,
  http::{header, StatusCode},
  response::{IntoResponse, Response},
  Json,
};
use futures_util::StreamExt;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tracing::{error, info, warn};
use utoipa::{IntoParams, ToSchema};

use crate::http_server::MAX_FILE_SIZE;
use crate::settings;
use crate::state::{AppState, FileResult, TaskState};
use crate::wps_com;
use crate::wps_com::process_kill::KillTracker;
use crate::wps_com::types::{BatchItem, ConversionError, ConversionResult, DocumentKind, Engine};

#[derive(Debug, Deserialize, IntoParams)]
pub struct ConvertQuery {
  #[serde(default)]
  pub r#async: bool,
  pub response: Option<String>,
  /// 可选: 指定本次转换使用的引擎 (auto / wps / office)，省略则使用全局设置。
  pub engine: Option<Engine>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SingleConvertResponse {
  pub success: bool,
  pub task_id: Option<String>,
  pub input_file: String,
  pub output_file: Option<String>,
  pub file_size: Option<u64>,
  #[schema(value_type = Option<u64>)]
  pub duration_ms: Option<u128>,
  pub message: String,
  pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BatchConvertResponse {
  pub success: bool,
  pub task_id: Option<String>,
  pub total: usize,
  pub succeeded: usize,
  pub failed: usize,
  #[schema(value_type = Option<u64>)]
  pub duration_ms: Option<u128>,
  pub results: Option<Vec<FileResult>>,
  pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConvertFromUrlRequest {
  pub url: String,
  pub output_name: Option<String>,
  pub output_folder: Option<String>,
  /// 任务终态后回调的 URL；服务端会 POST 任务最新快照。
  pub webhook_url: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConvertBatchFromUrlsRequest {
  pub urls: Vec<String>,
  pub output_folder: Option<String>,
  pub webhook_url: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
  pub success: bool,
  pub message: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);
type HandlerResult<T> = Result<Json<T>, ApiError>;
type ServiceResult<T> = Result<T, ApiError>;
type BinaryHandlerResult = Result<Response, ApiError>;

struct UploadedFile {
  original_name: String,
  path: PathBuf,
}

#[utoipa::path(
  post,
  path = "/api/convert-from-url",
  tag = "convert",
  params(ConvertQuery),
  request_body = ConvertFromUrlRequest,
  responses(
    (status = 200, description = "转换完成", body = SingleConvertResponse),
    (status = 400, description = "参数错误", body = ErrorResponse),
  )
)]
pub async fn convert_from_url(
  State(state): State<Arc<AppState>>,
  Query(query): Query<ConvertQuery>,
  Json(payload): Json<ConvertFromUrlRequest>,
) -> BinaryHandlerResult {
  let url = payload.url.trim().to_string();
  if url.is_empty() {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "文档下载链接不能为空".to_string(),
      }),
    ));
  }

  let uploaded = download_remote_file(&url).await?;
  let kind = document_kind_from_name(&uploaded.original_name)?;
  let prog_id = detect_wps(kind, query.engine).await?;
  let output_dir = output_dir_from_input(payload.output_folder)?;
  let output_file_name = payload
    .output_name
    .as_deref()
    .and_then(sanitize_file_name)
    .map(normalize_output_name)
    .unwrap_or_else(|| default_output_name(&uploaded.original_name));
  let output_path = output_dir.join(output_file_name);

  let task_id = state.create_task(1);
  if let Some(webhook) = payload.webhook_url.as_ref() {
    state.update_task(task_id, |t| t.webhook_url = Some(webhook.clone()));
  }
  let input_name = uploaded.original_name.clone();
  let wants_file_response = wants_file_response(&query)?;

  if query.r#async {
    if wants_file_response {
      return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
          success: false,
          message: "异步模式不支持直接返回 PDF 文件流".to_string(),
        }),
      ));
    }
    spawn_single_task(
      state.clone(),
      task_id,
      kind,
      prog_id,
      uploaded,
      output_path,
    );

    return Ok(Json(SingleConvertResponse {
      success: true,
      task_id: Some(task_id.to_string()),
      input_file: input_name,
      output_file: None,
      file_size: None,
      duration_ms: None,
      message: "下载并转换任务已开始".to_string(),
      error: None,
    })
    .into_response());
  }

  let _permit = state.acquire_conversion_permit().await;
  let cancel_flag = state.get_cancel_flag(task_id);
  let result = convert_one(kind, &prog_id, uploaded, output_path, cancel_flag).await;
  state.update_task(task_id, |task| match &result {
    Ok(file_result) => {
      task.completed_files = 1;
      task.results = vec![file_result.clone()];
      task.finalize(0);
    }
    Err(err) => {
      task.finalize_failed(err.to_string(), err.code.clone());
    }
  });
  state.record_history(task_id);

  match result {
    Ok(file_result) => {
      if wants_file_response {
        return pdf_file_response(&file_result);
      }

      Ok(Json(SingleConvertResponse {
        success: true,
        task_id: Some(task_id.to_string()),
        input_file: input_name,
        output_file: file_result.output,
        file_size: file_result.size,
        duration_ms: file_result.duration_ms,
        message: "下载并转换成功".to_string(),
        error: None,
      })
      .into_response())
    }
    Err(err) => Err((
      StatusCode::INTERNAL_SERVER_ERROR,
      Json(ErrorResponse {
        success: false,
        message: err.to_string(),
      }),
    )),
  }
}

/// 上传单文件转换为 PDF。multipart/form-data 字段：file (必填) / output_name (可选)
#[utoipa::path(
  post,
  path = "/api/convert",
  tag = "convert",
  params(ConvertQuery),
  responses(
    (status = 200, description = "转换完成", body = SingleConvertResponse),
    (status = 400, description = "参数错误", body = ErrorResponse),
  )
)]
pub async fn convert_single(
  State(state): State<Arc<AppState>>,
  Query(query): Query<ConvertQuery>,
  mut multipart: Multipart,
) -> BinaryHandlerResult {
  let mut output_name: Option<String> = None;
  let mut files: Vec<UploadedFile> = Vec::new();

  while let Some(field) = multipart.next_field().await.map_err(map_multipart_err)? {
    let name = field.name().unwrap_or_default().to_string();
    if name == "output_name" {
      output_name = Some(field.text().await.map_err(map_multipart_err)?);
      continue;
    }

    if name == "file" {
    let uploaded = store_uploaded_file(field).await?;
      files.push(uploaded);
    }
  }

  let uploaded = match files.pop() {
    Some(file) => file,
    None => {
      return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
          success: false,
          message: "缺少上传文件".to_string(),
        }),
      ));
    }
  };

  let kind = document_kind_from_name(&uploaded.original_name)?;
  let prog_id = detect_wps(kind, query.engine).await?;
  let output_dir = output_dir_from_input(None)?;
  let output_file_name = output_name
    .as_deref()
    .and_then(sanitize_file_name)
    .map(normalize_output_name)
    .unwrap_or_else(|| default_output_name(&uploaded.original_name));
  let output_path = output_dir.join(output_file_name);

  let task_id = state.create_task(1);
  let input_name = uploaded.original_name.clone();
  let wants_file_response = wants_file_response(&query)?;

  if query.r#async {
    if wants_file_response {
      return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
          success: false,
          message: "异步模式不支持直接返回 PDF 文件流".to_string(),
        }),
      ));
    }
    spawn_single_task(
      state.clone(),
      task_id,
      kind,
      prog_id,
      uploaded,
      output_path,
    );

    return Ok(Json(SingleConvertResponse {
      success: true,
      task_id: Some(task_id.to_string()),
      input_file: input_name,
      output_file: None,
      file_size: None,
      duration_ms: None,
      message: "转换任务已开始".to_string(),
      error: None,
    })
    .into_response());
  }

  let _permit = state.acquire_conversion_permit().await;
  let cancel_flag = state.get_cancel_flag(task_id);
  let result = convert_one(kind, &prog_id, uploaded, output_path, cancel_flag).await;
  state.update_task(task_id, |task| match &result {
    Ok(file_result) => {
      task.completed_files = 1;
      task.results = vec![file_result.clone()];
      task.finalize(0);
    }
    Err(err) => {
      task.finalize_failed(err.to_string(), err.code.clone());
    }
  });
  state.record_history(task_id);

  match result {
    Ok(file_result) => {
      if wants_file_response {
        return pdf_file_response(&file_result);
      }

      Ok(Json(SingleConvertResponse {
        success: true,
        task_id: Some(task_id.to_string()),
        input_file: input_name,
        output_file: file_result.output,
        file_size: file_result.size,
        duration_ms: file_result.duration_ms,
        message: "转换成功".to_string(),
        error: None,
      })
      .into_response())
    }
    Err(err) => Err((
      StatusCode::INTERNAL_SERVER_ERROR,
      Json(ErrorResponse {
        success: false,
        message: err.to_string(),
      }),
    )),
  }
}

#[utoipa::path(
  post,
  path = "/api/convert-batch-from-urls",
  tag = "convert",
  params(ConvertQuery),
  request_body = ConvertBatchFromUrlsRequest,
  responses(
    (status = 200, description = "批量转换结果", body = BatchConvertResponse),
    (status = 400, description = "参数错误", body = ErrorResponse),
  )
)]
pub async fn convert_batch_from_urls(
  State(state): State<Arc<AppState>>,
  Query(query): Query<ConvertQuery>,
  Json(payload): Json<ConvertBatchFromUrlsRequest>,
) -> HandlerResult<BatchConvertResponse> {
  let urls = payload
    .urls
    .into_iter()
    .map(|url| url.trim().to_string())
    .filter(|url| !url.is_empty())
    .collect::<Vec<_>>();

  if urls.is_empty() {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "没有可下载的文档链接".to_string(),
      }),
    ));
  }

  let total_files = urls.len();
  let task_id = state.create_task(total_files);
  if let Some(webhook) = payload.webhook_url.as_ref() {
    state.update_task(task_id, |t| t.webhook_url = Some(webhook.clone()));
  }

  if query.r#async {
    spawn_batch_from_urls_task(
      state.clone(),
      task_id,
      urls,
      payload.output_folder,
      query.engine,
    );

    return Ok(Json(BatchConvertResponse {
      success: true,
      task_id: Some(task_id.to_string()),
      total: total_files,
      succeeded: 0,
      failed: 0,
      duration_ms: None,
      results: None,
      message: "下载并批量转换任务已开始".to_string(),
    }));
  }

  let _permit = state.acquire_conversion_permit().await;
  let results =
    download_and_convert_many(&state, task_id, urls, payload.output_folder, query.engine).await?;
  let (succeeded, failed) = summarize_results(&results);

  state.update_task(task_id, |task| {
    task.results = results.clone();
    task.finalize(failed);
  });
  state.record_history(task_id);

  Ok(Json(BatchConvertResponse {
    success: failed == 0,
    task_id: Some(task_id.to_string()),
    total: results.len(),
    succeeded,
    failed,
    duration_ms: None,
    results: Some(results),
    message: if failed == 0 {
      "下载并批量转换完成".to_string()
    } else {
      "部分链接转换失败".to_string()
    },
  }))
}

/// 批量上传文件转换。multipart/form-data 字段：files[] / output_folder
#[utoipa::path(
  post,
  path = "/api/convert-batch",
  tag = "convert",
  params(ConvertQuery),
  responses(
    (status = 200, description = "批量转换结果", body = BatchConvertResponse),
    (status = 400, description = "参数错误", body = ErrorResponse),
  )
)]
pub async fn convert_batch(
  State(state): State<Arc<AppState>>,
  Query(query): Query<ConvertQuery>,
  mut multipart: Multipart,
) -> HandlerResult<BatchConvertResponse> {
  let mut output_folder: Option<String> = None;
  let mut files: Vec<UploadedFile> = Vec::new();

  while let Some(field) = multipart.next_field().await.map_err(map_multipart_err)? {
    let name = field.name().unwrap_or_default().to_string();
    match name.as_str() {
      "output_folder" => {
        output_folder = Some(field.text().await.map_err(map_multipart_err)?);
      }
      "files" | "files[]" => {
        let uploaded = store_uploaded_file(field).await?;
        files.push(uploaded);
      }
      _ => {}
    }
  }

  if files.is_empty() {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "没有可转换的文件".to_string(),
      }),
    ));
  }

  let kind = detect_batch_kind(&files)?;
  let prog_id = detect_wps(kind, query.engine).await?;
  let output_dir = output_dir_from_input(output_folder)?;
  let total_files = files.len();
  let task_id = state.create_task(total_files);

  if query.r#async {
    spawn_batch_task(state.clone(), task_id, kind, prog_id, files, output_dir);

    return Ok(Json(BatchConvertResponse {
      success: true,
      task_id: Some(task_id.to_string()),
      total: total_files,
      succeeded: 0,
      failed: 0,
      duration_ms: None,
      results: None,
      message: "转换任务已开始".to_string(),
    }));
  }

  let _permit = state.acquire_conversion_permit().await;
  let results = convert_many(&state, task_id, kind, &prog_id, files, output_dir).await;
  let (succeeded, failed) = summarize_results(&results);

  state.update_task(task_id, |task| {
    task.results = results.clone();
    task.finalize(failed);
  });
  state.record_history(task_id);

  Ok(Json(BatchConvertResponse {
    success: failed == 0,
    task_id: Some(task_id.to_string()),
    total: results.len(),
    succeeded,
    failed,
    duration_ms: None,
    results: Some(results),
    message: if failed == 0 {
      "转换完成".to_string()
    } else {
      "部分文件转换失败".to_string()
    },
  }))
}

fn spawn_single_task(
  state: Arc<AppState>,
  task_id: uuid::Uuid,
  kind: DocumentKind,
  prog_id: String,
  file: UploadedFile,
  output_path: PathBuf,
) {
  tokio::spawn(async move {
    let _permit = state.acquire_conversion_permit().await;
    let cancel_flag = state.get_cancel_flag(task_id);
    let result = convert_one(kind, &prog_id, file, output_path, cancel_flag).await;
    state.update_task(task_id, |task| match &result {
      Ok(file_result) => {
        task.completed_files = 1;
        task.results = vec![file_result.clone()];
        task.finalize(0);
      }
      Err(err) => {
        task.finalize_failed(err.to_string(), err.code.clone());
      }
    });
    state.record_history(task_id);
  });
}

fn spawn_batch_task(
  state: Arc<AppState>,
  task_id: uuid::Uuid,
  kind: DocumentKind,
  prog_id: String,
  files: Vec<UploadedFile>,
  output_dir: PathBuf,
) {
  tokio::spawn(async move {
    let _permit = state.acquire_conversion_permit().await;
    let results = convert_many(&state, task_id, kind, &prog_id, files, output_dir).await;
    let (_succeeded, failed) = summarize_results(&results);
    state.update_task(task_id, |task| {
      task.results = results;
      task.finalize(failed);
    });
    state.record_history(task_id);
  });
}

fn spawn_batch_from_urls_task(
  state: Arc<AppState>,
  task_id: uuid::Uuid,
  urls: Vec<String>,
  output_folder: Option<String>,
  engine: Option<Engine>,
) {
  tokio::spawn(async move {
    let _permit = state.acquire_conversion_permit().await;
    match download_and_convert_many(&state, task_id, urls, output_folder, engine).await {
      Ok(results) => {
        let (_succeeded, failed) = summarize_results(&results);
        state.update_task(task_id, |task| {
          task.results = results;
          task.finalize(failed);
        });
      }
      Err((_, Json(err))) => {
        state.update_task(task_id, |task| {
          task.finalize_failed(err.message, "unknown".to_string());
        });
      }
    }
    state.record_history(task_id);
  });
}

async fn download_and_convert_many(
  state: &Arc<AppState>,
  task_id: uuid::Uuid,
  urls: Vec<String>,
  output_folder: Option<String>,
  engine: Option<Engine>,
) -> ServiceResult<Vec<FileResult>> {
  state.update_task(task_id, |task| {
    task.status = TaskState::Processing;
    task.current_file = Some("准备下载文件".to_string());
  });

  let mut files = Vec::with_capacity(urls.len());
  for url in urls {
    state.update_task(task_id, |task| {
      task.current_file = Some(format!("下载中: {}", url));
    });

    match download_remote_file(&url).await {
      Ok(file) => files.push(file),
      Err(err) => {
        cleanup_uploaded_files(&files).await;
        return Err(err);
      }
    }
  }

  let kind = match detect_batch_kind(&files) {
    Ok(kind) => kind,
    Err(err) => {
      cleanup_uploaded_files(&files).await;
      return Err(err);
    }
  };
  let prog_id = detect_wps(kind, engine).await?;
  let output_dir = output_dir_from_input(output_folder)?;

  Ok(convert_many(state, task_id, kind, &prog_id, files, output_dir).await)
}

async fn convert_many(
  state: &Arc<AppState>,
  task_id: uuid::Uuid,
  kind: DocumentKind,
  prog_id: &str,
  files: Vec<UploadedFile>,
  output_dir: PathBuf,
) -> Vec<FileResult> {
  state.update_task(task_id, |task| {
    task.status = TaskState::Processing;
  });

  // 输出冲突处理：跳过的项不进入 wps_com 队列，但记入最终结果。
  let strategy = settings::read_collision_strategy();
  let mut items: Vec<BatchItem> = Vec::with_capacity(files.len());
  let mut skipped_results: Vec<FileResult> = Vec::new();
  for file in files {
    let output_name = default_output_name(&file.original_name);
    let target = output_dir.join(&output_name);
    match settings::resolve_output_path(&target, strategy) {
      Some(resolved) => items.push(BatchItem {
        file_name: file.original_name,
        input: file.path,
        output: resolved,
      }),
      None => {
        let _ = std::fs::remove_file(&file.path);
        skipped_results.push(FileResult {
          file: file.original_name,
          status: "skipped".to_string(),
          output: Some(target.display().to_string()),
          size: None,
          duration_ms: None,
          error: Some("输出文件已存在，按设置跳过".to_string()),
          error_code: Some("skipped_existing".to_string()),
        });
      }
    }
  }

  let prog_id = prog_id.to_string();
  let cancel_flag = state.get_cancel_flag(task_id);
  let state_clone = Arc::clone(state);
  let killer = Arc::new(KillTracker::capture());
  let worker_tracker = killer.clone();

  enum BatchOutcome {
    Results(Vec<BatchItem>, Vec<Result<crate::wps_com::types::ConversionResult, crate::wps_com::types::ConversionError>>),
    Fatal(Vec<BatchItem>, crate::wps_com::types::ConversionError),
  }

  let blocking_handle = tokio::task::spawn_blocking(move || {
    let result = wps_com::convert_batch_to_pdf(
      kind,
      &prog_id,
      &items,
      cancel_flag.as_deref(),
      &worker_tracker,
      |completed, total, item| {
        state_clone.update_task(task_id, |task| {
          task.current_file = Some(item.file_name.clone());
          task.completed_files = completed;
          task.progress = progress_for(completed, total);
        });
      },
    );

    for item in &items {
      let _ = std::fs::remove_file(&item.input);
    }

    match result {
      Ok(results) => BatchOutcome::Results(items, results),
      Err(err) => BatchOutcome::Fatal(items, err),
    }
  });

  // 监控批量任务期间的取消信号；触发后强杀 COM 子进程，让 spawn_blocking 立即解绑。
  let cancel_watcher = state.get_cancel_flag(task_id);
  let watcher = tokio::spawn(async move {
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    loop {
      tick.tick().await;
      if cancel_watcher
        .as_ref()
        .map_or(false, |flag| flag.load(Ordering::SeqCst))
      {
        let killer = killer.clone();
        let _ = tokio::task::spawn_blocking(move || {
          if let Some(killed) = killer.terminate_owned_process() {
            tracing::warn!("batch cancel terminated processes: {:?}", killed);
          }
        })
        .await;
        break;
      }
    }
  });

  let outcome = blocking_handle.await;
  watcher.abort();
  let mut results = skipped_results;

  match outcome {
    Ok(BatchOutcome::Results(items, results_raw)) => {
      for (item, item_result) in items.into_iter().zip(results_raw.into_iter()) {
        match item_result {
          Ok(conversion) => results.push(FileResult {
            file: item.file_name,
            status: "success".to_string(),
            output: Some(conversion.output.display().to_string()),
            size: Some(conversion.file_size),
            duration_ms: Some(conversion.duration_ms),
            error: None,
            error_code: None,
          }),
          Err(err) => results.push(FileResult {
            file: item.file_name,
            status: "failed".to_string(),
            output: None,
            size: None,
            duration_ms: None,
            error: Some(err.to_string()),
            error_code: Some(err.code().to_string()),
          }),
        }
      }
    }
    Ok(BatchOutcome::Fatal(items, err)) => {
      let code = err.code().to_string();
      for item in items {
        results.push(FileResult {
          file: item.file_name,
          status: "failed".to_string(),
          output: None,
          size: None,
          duration_ms: None,
          error: Some(err.to_string()),
          error_code: Some(code.clone()),
        });
      }
    }
    Err(err) => {
      results.push(FileResult {
        file: "unknown".to_string(),
        status: "failed".to_string(),
        output: None,
        size: None,
        duration_ms: None,
        error: Some(err.to_string()),
        error_code: Some("worker_panicked".to_string()),
      });
    }
  }

  results
}

struct ConversionFailure {
  message: String,
  code: String,
  file_name: Option<String>,
}

impl std::fmt::Display for ConversionFailure {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(name) = &self.file_name {
      write!(f, "{} ({})", self.message, name)
    } else {
      write!(f, "{}", self.message)
    }
  }
}

async fn convert_one(
  kind: DocumentKind,
  prog_id: &str,
  file: UploadedFile,
  output_path: PathBuf,
  cancel: Option<Arc<AtomicBool>>,
) -> Result<FileResult, ConversionFailure> {
  let file_name = file.original_name.clone();
  let input_path = file.path.clone();
  let cleanup_path = input_path.clone();
  let prog_id = prog_id.to_string();
  let timeout_secs = settings::read_conversion_timeout_secs();
  // 已存在的进程不归本次转换任务所有。
  let killer = Arc::new(KillTracker::capture());

  // 输出冲突策略：overwrite 直写；skip 直接返回；auto_number 找下一个可用名。
  let strategy = settings::read_collision_strategy();
  let resolved = match settings::resolve_output_path(&output_path, strategy) {
    Some(p) => p,
    None => {
      let _ = tokio::fs::remove_file(&cleanup_path).await;
      return Ok(FileResult {
        file: file_name,
        status: "skipped".to_string(),
        output: Some(output_path.display().to_string()),
        size: None,
        duration_ms: None,
        error: Some("输出文件已存在，按设置跳过".to_string()),
        error_code: Some("skipped_existing".to_string()),
      });
    }
  };
  let output_path = resolved;

  let worker_tracker = killer.clone();
  let result = run_with_timeout(timeout_secs, cancel, killer, move || {
    wps_com::convert_to_pdf(kind, &prog_id, &input_path, &output_path, Some(&worker_tracker))
  })
  .await;

  match result {
    Ok(conversion) => {
      info!("Converted {} -> {}", conversion.input.display(), conversion.output.display());
      let _ = tokio::fs::remove_file(&cleanup_path).await;
      Ok(FileResult {
        file: file_name,
        status: "success".to_string(),
        output: Some(conversion.output.display().to_string()),
        size: Some(conversion.file_size),
        duration_ms: Some(conversion.duration_ms),
        error: None,
        error_code: None,
      })
    }
    Err(err) => {
      if matches!(err, ConversionError::Timeout(_)) {
        warn!("Conversion timed out: {}", err);
      } else {
        error!("Conversion failed: {}", err);
      }
      Err(ConversionFailure {
        message: err.to_string(),
        code: err.code().to_string(),
        file_name: Some(file_name),
      })
    }
  }
}

/// 在独立线程跑同步 COM 转换；同时监听超时与取消信号。
///
/// 超时或取消时：
/// 1. 立即向调用方返回错误（用户体验：永远不会卡在 processing）；
/// 2. 仅终止已确认属于本次 COM 应用的进程；
///    被杀进程持有的 IDispatch::Invoke 会以 RPC_E_DISCONNECTED 返回，worker 线程随之退出。
async fn run_with_timeout<F>(
  timeout_secs: u64,
  cancel: Option<Arc<AtomicBool>>,
  killer: Arc<KillTracker>,
  work: F,
) -> Result<ConversionResult, ConversionError>
where
  F: FnOnce() -> Result<ConversionResult, ConversionError> + Send + 'static,
{
  let (tx, rx) = oneshot::channel();
  std::thread::spawn(move || {
    let result = work();
    let _ = tx.send(result);
  });

  let timeout = tokio::time::sleep(Duration::from_secs(timeout_secs));
  tokio::pin!(timeout);
  tokio::pin!(rx);
  let mut tick = tokio::time::interval(Duration::from_millis(250));

  let outcome: Result<ConversionResult, ConversionError> = loop {
    tokio::select! {
      result = &mut rx => {
        break match result {
          Ok(r) => r,
          Err(_) => Err(ConversionError::Unknown(
            "conversion worker thread dropped".to_string(),
          )),
        };
      }
      _ = &mut timeout => break Err(ConversionError::Timeout(timeout_secs)),
      _ = tick.tick() => {
        if cancel.as_ref().map_or(false, |flag| flag.load(Ordering::SeqCst)) {
          break Err(ConversionError::Cancelled);
        }
      }
    }
  };

  if matches!(outcome, Err(ConversionError::Timeout(_)) | Err(ConversionError::Cancelled)) {
    let killer = killer.clone();
    // 强杀放到 blocking 池，避免阻塞 tokio 调度。
    let _ = tokio::task::spawn_blocking(move || {
      if let Some(killed) = killer.terminate_owned_process() {
        tracing::warn!("terminated stale WPS/Office processes: {:?}", killed);
      }
    })
    .await;
  }

  outcome
}

fn summarize_results(results: &[FileResult]) -> (usize, usize) {
  let mut succeeded = 0;
  let mut failed = 0;
  for item in results {
    if item.status == "success" {
      succeeded += 1;
    } else {
      failed += 1;
    }
  }
  (succeeded, failed)
}

fn progress_for(completed: usize, total: usize) -> u8 {
  if total == 0 {
    return 0;
  }
  ((completed as f32 / total as f32) * 100.0).round() as u8
}

async fn detect_wps(kind: DocumentKind, override_engine: Option<Engine>) -> ServiceResult<String> {
  let engine = override_engine.unwrap_or_else(|| crate::settings::read_desktop_settings().engine);
  let result = tokio::task::spawn_blocking(move || wps_com::check_engine(engine, kind)).await;
  match result {
    Ok(Ok(prog_id)) => Ok(prog_id),
    Ok(Err(err)) => Err((
      StatusCode::SERVICE_UNAVAILABLE,
      Json(ErrorResponse {
        success: false,
        message: engine_unavailable_message(engine, &err),
      }),
    )),
    Err(err) => Err((
      StatusCode::INTERNAL_SERVER_ERROR,
      Json(ErrorResponse {
        success: false,
        message: err.to_string(),
      }),
    )),
  }
}

fn engine_unavailable_message(engine: Engine, err: &crate::wps_com::types::ConversionError) -> String {
  match engine {
    Engine::Wps => format!("WPS 引擎不可用: {} — 请安装 WPS 或在设置中切换到 Microsoft Office", err),
    Engine::Office => format!("Microsoft Office 引擎不可用: {} — 请安装 Word/Excel 或在设置中切换到 WPS", err),
    Engine::Auto => format!("未检测到 WPS 或 Microsoft Office: {}", err),
  }
}

fn sanitize_file_name(name: &str) -> Option<String> {
  let candidate = Path::new(name)
    .file_name()
    .and_then(|v| v.to_str())
    .map(|v| v.trim())?;
  if candidate.is_empty() {
    return None;
  }
  Some(candidate.to_string())
}

fn default_output_name(input_name: &str) -> String {
  let stem = Path::new(input_name)
    .file_stem()
    .and_then(|v| v.to_str())
    .unwrap_or("document");
  format!("{}.pdf", stem)
}

fn document_kind_from_name(name: &str) -> ServiceResult<DocumentKind> {
  let ext = Path::new(name)
    .extension()
    .and_then(|v| v.to_str())
    .unwrap_or("");
  DocumentKind::from_extension(ext).ok_or_else(|| {
    (
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: format!("不支持的文件类型: {}", name),
      }),
    )
  })
}

fn detect_batch_kind(files: &[UploadedFile]) -> ServiceResult<DocumentKind> {
  let mut kind: Option<DocumentKind> = None;
  for file in files {
    let next = document_kind_from_name(&file.original_name)?;
    match kind {
      None => kind = Some(next),
      Some(existing) if existing == next => {}
      Some(_) => {
        return Err((
          StatusCode::BAD_REQUEST,
          Json(ErrorResponse {
            success: false,
            message: "批量转换暂不支持混合 Word/Excel 类型".to_string(),
          }),
        ));
      }
    }
  }
  Ok(kind.unwrap_or(DocumentKind::Word))
}

fn normalize_output_name(name: String) -> String {
  let path = Path::new(&name);
  let ext = path.extension().and_then(|v| v.to_str()).unwrap_or("");
  if ext.eq_ignore_ascii_case("pdf") {
    return name;
  }
  let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("output");
  format!("{}.pdf", stem)
}

fn output_dir_from_input(output_folder: Option<String>) -> ServiceResult<PathBuf> {
  let folder = output_folder.or_else(crate::settings::read_default_output_dir);
  if let Some(folder) = folder {
    return ensure_output_dir(folder);
  }
  let fallback = settings::fallback_output_dir();
  std::fs::create_dir_all(&fallback).map_err(map_io_err)?;
  Ok(fallback)
}

fn ensure_output_dir(folder: String) -> ServiceResult<PathBuf> {
  let folder = folder.trim();
  if folder.is_empty() {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "输出目录不能为空".to_string(),
      }),
    ));
  }

  let path = PathBuf::from(folder);
  if !path.is_absolute() {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "输出目录必须是绝对路径".to_string(),
      }),
    ));
  }

  std::fs::create_dir_all(&path).map_err(|err| {
    (
      StatusCode::INTERNAL_SERVER_ERROR,
      Json(ErrorResponse {
        success: false,
        message: err.to_string(),
      }),
    )
  })?;

  Ok(path)
}

async fn store_uploaded_file(
  mut field: axum::extract::multipart::Field<'_>,
) -> ServiceResult<UploadedFile> {
  let file_name = field.file_name().unwrap_or("upload.docx").to_string();
  let ext = Path::new(&file_name)
    .extension()
    .and_then(|v| v.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();

  if DocumentKind::from_extension(&ext).is_none() {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: format!("不支持的文件类型: {}", file_name),
      }),
    ));
  }

  let temp_dir = std::env::temp_dir().join("wps-pdf-upload");
  tokio::fs::create_dir_all(&temp_dir)
    .await
    .map_err(map_io_err)?;

  let sanitized = sanitize_file_name(&file_name).unwrap_or_else(|| "upload.docx".to_string());
  let temp_name = format!("{}_{}", uuid::Uuid::new_v4(), sanitized);
  let temp_path = temp_dir.join(temp_name);

  let mut file = tokio::fs::File::create(&temp_path)
    .await
    .map_err(map_io_err)?;

  let mut size = 0usize;
  while let Some(chunk) = field.chunk().await.map_err(map_multipart_err)? {
    size += chunk.len();
    if size > MAX_FILE_SIZE {
      return Err((
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ErrorResponse {
          success: false,
          message: format!("文件过大: {}", file_name),
        }),
      ));
    }
    file.write_all(&chunk).await.map_err(map_io_err)?;
  }
  file.flush().await.map_err(map_io_err)?;

  Ok(UploadedFile {
    original_name: file_name,
    path: temp_path,
  })
}

async fn download_remote_file(url: &str) -> ServiceResult<UploadedFile> {
  let parsed_url = reqwest::Url::parse(url).map_err(|_| {
    (
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "无效的下载链接".to_string(),
      }),
    )
  })?;

  if !matches!(parsed_url.scheme(), "http" | "https") {
    return Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: "仅支持 http/https 下载链接".to_string(),
      }),
    ));
  }

  let response = reqwest::Client::new()
    .get(parsed_url)
    .send()
    .await
    .map_err(map_download_err)?;

  if !response.status().is_success() {
    return Err((
      StatusCode::BAD_GATEWAY,
      Json(ErrorResponse {
        success: false,
        message: format!("下载失败: {}", response.status()),
      }),
    ));
  }

  if response
    .content_length()
    .is_some_and(|content_length| content_length > MAX_FILE_SIZE as u64)
  {
    return Err((
      StatusCode::PAYLOAD_TOO_LARGE,
      Json(ErrorResponse {
        success: false,
        message: "远程文件过大".to_string(),
      }),
    ));
  }

  let file_name = resolve_remote_file_name(url, &response)?;
  let temp_dir = upload_temp_dir();
  tokio::fs::create_dir_all(&temp_dir)
    .await
    .map_err(map_io_err)?;

  let sanitized = sanitize_file_name(&file_name).unwrap_or_else(|| "download.docx".to_string());
  let temp_name = format!("{}_{}", uuid::Uuid::new_v4(), sanitized);
  let temp_path = temp_dir.join(temp_name);

  let mut file = tokio::fs::File::create(&temp_path)
    .await
    .map_err(map_io_err)?;

  let mut size = 0usize;
  let mut stream = response.bytes_stream();
  while let Some(chunk) = stream.next().await {
    let chunk = chunk.map_err(map_download_err)?;
    size += chunk.len();
    if size > MAX_FILE_SIZE {
      return Err((
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ErrorResponse {
          success: false,
          message: format!("远程文件过大: {}", file_name),
        }),
      ));
    }
    file.write_all(&chunk).await.map_err(map_io_err)?;
  }
  file.flush().await.map_err(map_io_err)?;

  Ok(UploadedFile {
    original_name: file_name,
    path: temp_path,
  })
}

async fn cleanup_uploaded_files(files: &[UploadedFile]) {
  for file in files {
    let _ = tokio::fs::remove_file(&file.path).await;
  }
}

fn resolve_remote_file_name(url: &str, response: &reqwest::Response) -> ServiceResult<String> {
  let header_name = response
    .headers()
    .get(CONTENT_DISPOSITION)
    .and_then(|value| value.to_str().ok())
    .and_then(extract_filename_from_content_disposition);

  let url_name = reqwest::Url::parse(url)
    .ok()
    .and_then(|parsed| {
      parsed
        .path_segments()
        .and_then(|segments| segments.last().map(|segment| segment.to_string()))
    })
    .filter(|name| !name.trim().is_empty());

  let mut file_name = header_name
    .or(url_name)
    .and_then(|name| sanitize_file_name(&name))
    .unwrap_or_else(|| "download".to_string());

  let ext = Path::new(&file_name)
    .extension()
    .and_then(|value| value.to_str())
    .unwrap_or("")
    .to_ascii_lowercase();

  if DocumentKind::from_extension(&ext).is_none() {
    if let Some(inferred_ext) = response
      .headers()
      .get(CONTENT_TYPE)
      .and_then(|value| value.to_str().ok())
      .and_then(extension_from_content_type)
    {
      let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
      file_name = format!("{}.{}", stem, inferred_ext);
    }
  }

  document_kind_from_name(&file_name)?;
  Ok(file_name)
}

fn extract_filename_from_content_disposition(value: &str) -> Option<String> {
  value.split(';').map(str::trim).find_map(|part| {
    if let Some(raw) = part.strip_prefix("filename*=") {
      let decoded = raw.split("''").last().unwrap_or(raw);
      return Some(decoded.trim_matches('"').to_string());
    }
    part
      .strip_prefix("filename=")
      .map(|raw| raw.trim_matches('"').to_string())
  })
}

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
  let mime = content_type
    .split(';')
    .next()
    .unwrap_or("")
    .trim()
    .to_ascii_lowercase();

  match mime.as_str() {
    "application/msword" => Some("doc"),
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
    "application/vnd.ms-excel" => Some("xls"),
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
    _ => None,
  }
}

fn upload_temp_dir() -> PathBuf {
  std::env::temp_dir().join("wps-pdf-upload")
}

fn wants_file_response(query: &ConvertQuery) -> ServiceResult<bool> {
  match query.response.as_deref() {
    None | Some("json") => Ok(false),
    Some("file") => Ok(true),
    Some(other) => Err((
      StatusCode::BAD_REQUEST,
      Json(ErrorResponse {
        success: false,
        message: format!("不支持的 response 参数: {}", other),
      }),
    )),
  }
}

fn pdf_file_response(file_result: &FileResult) -> BinaryHandlerResult {
  let output = file_result.output.as_ref().ok_or_else(|| {
    (
      StatusCode::INTERNAL_SERVER_ERROR,
      Json(ErrorResponse {
        success: false,
        message: "未找到转换后的 PDF 文件".to_string(),
      }),
    )
  })?;

  let output_path = PathBuf::from(output);
  let file_name = output_path
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or("output.pdf")
    .to_string();

  let data = std::fs::read(&output_path).map_err(|err| {
    (
      StatusCode::INTERNAL_SERVER_ERROR,
      Json(ErrorResponse {
        success: false,
        message: format!("读取 PDF 文件失败: {}", err),
      }),
    )
  })?;

  Ok((
    [
      (header::CONTENT_TYPE, "application/pdf".to_string()),
      (
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", file_name),
      ),
    ],
    data,
  )
    .into_response())
}

fn map_multipart_err(err: MultipartError) -> ApiError {
  (
    StatusCode::BAD_REQUEST,
    Json(ErrorResponse {
      success: false,
      message: err.to_string(),
    }),
  )
}

fn map_io_err(err: std::io::Error) -> ApiError {
  (
    StatusCode::INTERNAL_SERVER_ERROR,
    Json(ErrorResponse {
      success: false,
      message: err.to_string(),
    }),
  )
}

fn map_download_err(err: reqwest::Error) -> ApiError {
  (
    StatusCode::BAD_GATEWAY,
    Json(ErrorResponse {
      success: false,
      message: err.to_string(),
    }),
  )
}
