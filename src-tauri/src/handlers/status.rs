use std::sync::Arc;

use axum::{
  extract::{Path, Query, State},
  http::{header, StatusCode},
  response::IntoResponse,
  Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::state::{AppState, TaskStatusResponse};

#[utoipa::path(
  get,
  path = "/api/status/{task_id}",
  tag = "tasks",
  params(("task_id" = String, Path, description = "Task UUID")),
  responses(
    (status = 200, description = "任务状态", body = TaskStatusResponse),
    (status = 404, description = "任务不存在"),
  )
)]
pub async fn get_status(
  Path(task_id): Path<String>,
  State(state): State<Arc<AppState>>,
) -> Result<Json<TaskStatusResponse>, StatusCode> {
  let id = uuid::Uuid::parse_str(&task_id).map_err(|_| StatusCode::BAD_REQUEST)?;
  match state.get_task_response(id) {
    Some(status) => Ok(Json(status)),
    None => Err(StatusCode::NOT_FOUND),
  }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CancelTaskResponse {
  pub success: bool,
  pub task_id: String,
  pub cancelled: bool,
  pub message: String,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListTasksQuery {
  pub limit: Option<usize>,
  pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListTasksResponse {
  pub total: usize,
  pub tasks: Vec<TaskStatusResponse>,
}

#[utoipa::path(
  get,
  path = "/api/tasks",
  tag = "tasks",
  params(ListTasksQuery),
  responses((status = 200, description = "历史任务", body = ListTasksResponse))
)]
pub async fn list_tasks(
  Query(query): Query<ListTasksQuery>,
  State(state): State<Arc<AppState>>,
) -> Json<ListTasksResponse> {
  let limit = query.limit.unwrap_or(50).clamp(1, 500);
  let tasks = state.history.recent(limit, query.status.as_deref()).await;
  Json(ListTasksResponse {
    total: tasks.len(),
    tasks,
  })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QueueCounts {
  pub pending: usize,
  pub processing: usize,
  pub completed: usize,
  pub failed: usize,
  pub cancelled: usize,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QueueResponse {
  pub counts: QueueCounts,
  pub active: Vec<TaskStatusResponse>,
}

#[utoipa::path(
  get,
  path = "/api/tasks/{task_id}/report.csv",
  tag = "tasks",
  params(("task_id" = String, Path, description = "Task UUID")),
  responses(
    (status = 200, description = "CSV 报告", content_type = "text/csv"),
    (status = 404, description = "任务不存在"),
  )
)]
pub async fn get_task_report_csv(
  Path(task_id): Path<String>,
  State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, StatusCode> {
  let id = uuid::Uuid::parse_str(&task_id).map_err(|_| StatusCode::BAD_REQUEST)?;
  let task = state
    .get_task_response(id)
    .ok_or(StatusCode::NOT_FOUND)?;

  let mut csv = String::from("file,status,output,size,duration_ms,error_code,error\n");
  if let Some(results) = task.results {
    for r in results {
      csv.push_str(&format!(
        "{},{},{},{},{},{},{}\n",
        csv_escape(&r.file),
        csv_escape(&r.status),
        csv_escape(r.output.as_deref().unwrap_or("")),
        r.size.map(|v| v.to_string()).unwrap_or_default(),
        r.duration_ms.map(|v| v.to_string()).unwrap_or_default(),
        csv_escape(r.error_code.as_deref().unwrap_or("")),
        csv_escape(r.error.as_deref().unwrap_or("")),
      ));
    }
  }

  let filename = format!("task-{}-report.csv", &task_id[..8.min(task_id.len())]);
  Ok((
    [
      (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
      (
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename),
      ),
    ],
    csv,
  ))
}

fn csv_escape(s: &str) -> String {
  if s.contains(',') || s.contains('"') || s.contains('\n') {
    let escaped = s.replace('"', "\"\"");
    format!("\"{}\"", escaped)
  } else {
    s.to_string()
  }
}

#[utoipa::path(
  get,
  path = "/api/queue",
  tag = "tasks",
  responses((status = 200, description = "任务队列概览", body = QueueResponse))
)]
pub async fn get_queue(State(state): State<Arc<AppState>>) -> Json<QueueResponse> {
  let mut counts = QueueCounts {
    pending: 0,
    processing: 0,
    completed: 0,
    failed: 0,
    cancelled: 0,
  };
  let mut active: Vec<TaskStatusResponse> = Vec::new();
  for entry in state.tasks.iter() {
    match entry.status {
      crate::state::TaskState::Pending => {
        counts.pending += 1;
        active.push(entry.to_response());
      }
      crate::state::TaskState::Processing => {
        counts.processing += 1;
        active.push(entry.to_response());
      }
      crate::state::TaskState::Completed => counts.completed += 1,
      crate::state::TaskState::Failed => counts.failed += 1,
      crate::state::TaskState::Cancelled => counts.cancelled += 1,
    }
  }
  // 活跃任务按开始时间倒序，最多 50 条避免响应过大
  active.sort_by(|a, b| b.started_at.cmp(&a.started_at));
  active.truncate(50);
  Json(QueueResponse { counts, active })
}

#[utoipa::path(
  post,
  path = "/api/cancel/{task_id}",
  tag = "tasks",
  params(("task_id" = String, Path, description = "Task UUID")),
  responses(
    (status = 200, description = "取消请求受理", body = CancelTaskResponse),
    (status = 404, description = "任务不存在"),
  )
)]
pub async fn cancel_task(
  Path(task_id): Path<String>,
  State(state): State<Arc<AppState>>,
) -> Result<Json<CancelTaskResponse>, StatusCode> {
  let id = uuid::Uuid::parse_str(&task_id).map_err(|_| StatusCode::BAD_REQUEST)?;
  if state.get_task_response(id).is_none() {
    return Err(StatusCode::NOT_FOUND);
  }
  let switched = state.request_cancel(id);
  Ok(Json(CancelTaskResponse {
    success: true,
    task_id,
    cancelled: switched,
    message: if switched {
      "已请求取消任务".to_string()
    } else {
      "任务已结束，仅记录取消标记".to_string()
    },
  }))
}
