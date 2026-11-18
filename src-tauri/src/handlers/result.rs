//! 按 task_id 取转换产物的下载接口。外部系统不必拿本机绝对路径，
//! 凭任务 ID + index 即可下载 PDF。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
  extract::{Path as AxumPath, Query, State},
  http::{header, StatusCode},
  response::IntoResponse,
  Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::state::AppState;

use super::convert::ErrorResponse;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ResultQuery {
  #[serde(default)]
  pub index: Option<usize>,
  /// 缺省 attachment（强制下载），传 inline 在浏览器内联预览。
  pub disposition: Option<String>,
}

#[utoipa::path(
  get,
  path = "/api/result/{task_id}",
  tag = "tasks",
  params(
    ("task_id" = String, Path, description = "Task UUID"),
    ResultQuery,
  ),
  responses(
    (status = 200, description = "PDF 二进制", content_type = "application/pdf"),
    (status = 400, description = "参数错误", body = ErrorResponse),
    (status = 403, description = "路径越权", body = ErrorResponse),
    (status = 404, description = "任务或结果不存在", body = ErrorResponse),
  )
)]
pub async fn get_result(
  AxumPath(task_id): AxumPath<String>,
  Query(query): Query<ResultQuery>,
  State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
  let id = uuid::Uuid::parse_str(&task_id)
    .map_err(|_| reject(StatusCode::BAD_REQUEST, "无效的 task_id"))?;

  let task = state
    .get_task_response(id)
    .ok_or_else(|| reject(StatusCode::NOT_FOUND, "任务不存在"))?;

  let results = task
    .results
    .as_ref()
    .ok_or_else(|| reject(StatusCode::NOT_FOUND, "任务尚无产物"))?;

  let index = query.index.unwrap_or(0);
  let item = results
    .get(index)
    .ok_or_else(|| reject(StatusCode::NOT_FOUND, "下标越界"))?;

  let output_str = item
    .output
    .as_ref()
    .ok_or_else(|| reject(StatusCode::NOT_FOUND, "该项无输出文件"))?;

  let path = PathBuf::from(output_str);
  let canonical = path
    .canonicalize()
    .map_err(|_| reject(StatusCode::NOT_FOUND, "文件不存在或已被移动"))?;
  let canonical = strip_unc(canonical);

  if !is_allowed(&state, &canonical) {
    return Err(reject(StatusCode::FORBIDDEN, "路径不在允许范围"));
  }

  let data = tokio::fs::read(&canonical)
    .await
    .map_err(|err| reject(StatusCode::NOT_FOUND, &format!("读取失败: {}", err)))?;

  let file_name = canonical
    .file_name()
    .and_then(|s| s.to_str())
    .unwrap_or("result.pdf");
  let disposition = match query.disposition.as_deref() {
    Some("inline") => format!("inline; filename=\"{}\"", file_name),
    _ => format!("attachment; filename=\"{}\"", file_name),
  };

  Ok((
    [
      (header::CONTENT_TYPE, "application/pdf".to_string()),
      (header::CONTENT_DISPOSITION, disposition),
    ],
    data,
  ))
}

fn reject(status: StatusCode, message: &str) -> (StatusCode, Json<ErrorResponse>) {
  (
    status,
    Json(ErrorResponse {
      success: false,
      message: message.to_string(),
    }),
  )
}

fn is_allowed(state: &AppState, canonical: &Path) -> bool {
  // 复用 preview 的白名单逻辑：任务产物精确文件 + 输出目录前缀。
  for entry in state.tasks.iter() {
    for result in &entry.results {
      if let Some(output) = result.output.as_ref() {
        let candidate = PathBuf::from(output);
        if let Ok(canon) = candidate.canonicalize() {
          if strip_unc(canon) == canonical {
            return true;
          }
        }
      }
    }
  }
  for dir in allowed_dirs() {
    if let Ok(canon) = dir.canonicalize() {
      if canonical.starts_with(strip_unc(canon)) {
        return true;
      }
    }
  }
  false
}

fn allowed_dirs() -> Vec<PathBuf> {
  let mut dirs = Vec::new();
  if let Some(dir) = crate::settings::read_default_output_dir() {
    dirs.push(PathBuf::from(dir));
  }
  dirs.push(crate::settings::fallback_output_dir());
  dirs.push(std::env::temp_dir().join("wps-pdf-output"));
  dirs
}

fn strip_unc(path: PathBuf) -> PathBuf {
  let s = path.display().to_string();
  if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
    return PathBuf::from(format!(r"\\{}", rest));
  }
  if let Some(rest) = s.strip_prefix(r"\\?\") {
    return PathBuf::from(rest);
  }
  path
}
