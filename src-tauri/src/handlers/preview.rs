use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::settings;
use crate::state::AppState;

use super::convert::ErrorResponse;

#[derive(Debug, Deserialize, IntoParams)]
pub struct PreviewQuery {
    pub path: String,
}

#[utoipa::path(
  get,
  path = "/api/preview",
  tag = "preview",
  params(PreviewQuery),
  responses(
    (status = 200, description = "PDF 二进制", content_type = "application/pdf"),
    (status = 400, description = "参数错误", body = ErrorResponse),
    (status = 403, description = "路径不在允许范围", body = ErrorResponse),
    (status = 404, description = "文件不存在", body = ErrorResponse),
  )
)]
pub async fn preview_pdf(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PreviewQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let path = PathBuf::from(&query.path);

    if path.extension().and_then(|e| e.to_str()).unwrap_or("") != "pdf" {
        return Err(reject(
            StatusCode::BAD_REQUEST,
            "仅支持预览 PDF 文件",
        ));
    }

    if !path.is_absolute() {
        return Err(reject(StatusCode::BAD_REQUEST, "路径必须是绝对路径"));
    }

    let canonical = match path.canonicalize() {
        Ok(p) => strip_unc(p),
        Err(_) => {
            return Err(reject(StatusCode::NOT_FOUND, "文件不存在"));
        }
    };

    if !is_allowed(&state, &canonical) {
        return Err(reject(
            StatusCode::FORBIDDEN,
            "不允许预览该路径，仅支持任务产物或输出目录内的 PDF",
        ));
    }

    let data = tokio::fs::read(&canonical).await.map_err(|err| {
        reject(StatusCode::NOT_FOUND, &format!("无法读取文件: {}", err))
    })?;

    let file_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("preview.pdf");

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", file_name),
            ),
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
    // 1) 命中任务产物精确文件
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

    // 2) 命中允许的输出目录前缀
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
    if let Some(dir) = settings::read_default_output_dir() {
        dirs.push(PathBuf::from(dir));
    }
    dirs.push(settings::fallback_output_dir());
    // 旧版本的临时输出目录，存在历史产物时也允许预览。
    dirs.push(std::env::temp_dir().join("wps-pdf-output"));
    dirs
}

/// Office (Word/Excel) COM 与 std::fs 在某些路径下返回 \\?\ 扩展长路径前缀，比对前剥离。
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
