use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::settings;
use crate::state::{AppState, FileResult, TaskState};
use crate::wps_com;
use crate::wps_com::types::{DocumentKind, EngineProbe};

#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticsResponse {
  pub version: String,
  pub engine: EngineSection,
  pub tasks: TasksSection,
  pub storage: StorageSection,
  pub security: SecuritySection,
  pub conversion_timeout_secs: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EngineSection {
  pub configured: String,
  pub probes: Vec<EngineProbe>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TasksSection {
  pub active: usize,
  pub recent_failures: Vec<RecentFailure>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecentFailure {
  pub task_id: String,
  pub file: String,
  pub error: Option<String>,
  pub error_code: Option<String>,
  pub started_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct StorageSection {
  pub default_output_dir: Option<String>,
  pub fallback_output_dir: String,
  pub free_bytes: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SecuritySection {
  pub api_token_enabled: bool,
}

#[utoipa::path(
  get,
  path = "/api/diagnostics",
  tag = "health",
  responses((status = 200, description = "服务诊断聚合", body = DiagnosticsResponse))
)]
pub async fn get_diagnostics(State(state): State<Arc<AppState>>) -> Json<DiagnosticsResponse> {
  let configured = settings::read_desktop_settings().engine;
  let probes =
    tokio::task::spawn_blocking(|| wps_com::probe_engines(DocumentKind::Word))
      .await
      .unwrap_or_default();

  let active = state
    .tasks
    .iter()
    .filter(|entry| {
      matches!(entry.status, TaskState::Pending | TaskState::Processing)
    })
    .count();

  // 最近 20 个历史任务里挑出失败/含失败子项的，最多展示 10 条。
  let recent = state.history.recent(20, None).await;
  let recent_failures: Vec<RecentFailure> = recent
    .into_iter()
    .flat_map(|task| failures_from_task(task))
    .take(10)
    .collect();

  let default_dir = settings::read_default_output_dir();
  let fallback = settings::fallback_output_dir();
  let free_bytes = disk_free_bytes(default_dir.as_deref().map(std::path::Path::new).unwrap_or(&fallback));

  Json(DiagnosticsResponse {
    version: env!("CARGO_PKG_VERSION").to_string(),
    engine: EngineSection {
      configured: configured.label().to_string(),
      probes,
    },
    tasks: TasksSection {
      active,
      recent_failures,
    },
    storage: StorageSection {
      default_output_dir: default_dir,
      fallback_output_dir: fallback.display().to_string(),
      free_bytes,
    },
    security: SecuritySection {
      api_token_enabled: settings::read_api_token().is_some(),
    },
    conversion_timeout_secs: settings::read_conversion_timeout_secs(),
  })
}

fn failures_from_task(task: crate::state::TaskStatusResponse) -> Vec<RecentFailure> {
  let task_id = task.task_id.clone();
  let started_at = task.started_at.clone();
  let mut out = Vec::new();
  if matches!(task.status, TaskState::Failed | TaskState::Cancelled) && task.error.is_some() {
    out.push(RecentFailure {
      task_id: task_id.clone(),
      file: task.current_file.clone().unwrap_or_else(|| "<task>".to_string()),
      error: task.error.clone(),
      error_code: task.error_code.clone(),
      started_at: started_at.clone(),
    });
  }
  if let Some(results) = task.results {
    for FileResult { file, status, error, error_code, .. } in results {
      if status != "success" && error.is_some() {
        out.push(RecentFailure {
          task_id: task_id.clone(),
          file,
          error,
          error_code,
          started_at: started_at.clone(),
        });
      }
    }
  }
  out
}

#[cfg(windows)]
fn disk_free_bytes(path: &std::path::Path) -> Option<u64> {
  use std::os::windows::ffi::OsStrExt;
  use windows::core::PCWSTR;
  use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

  let wide: Vec<u16> = path
    .as_os_str()
    .encode_wide()
    .chain(std::iter::once(0))
    .collect();
  let mut free = 0u64;
  let result = unsafe {
    GetDiskFreeSpaceExW(
      PCWSTR(wide.as_ptr()),
      Some(&mut free as *mut u64),
      None,
      None,
    )
  };
  if result.is_ok() {
    Some(free)
  } else {
    None
  }
}

#[cfg(not(windows))]
fn disk_free_bytes(_path: &std::path::Path) -> Option<u64> {
  None
}
