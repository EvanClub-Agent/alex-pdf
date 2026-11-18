use chrono::{DateTime, Local};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
  Pending,
  Processing,
  Completed,
  Failed,
  Cancelled,
}

impl TaskState {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Pending => "pending",
      Self::Processing => "processing",
      Self::Completed => "completed",
      Self::Failed => "failed",
      Self::Cancelled => "cancelled",
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FileResult {
  pub file: String,
  pub status: String,
  pub output: Option<String>,
  pub size: Option<u64>,
  #[schema(value_type = Option<u64>)]
  pub duration_ms: Option<u128>,
  pub error: Option<String>,
  /// 稳定的错误分类码，便于前端按类型展示文案。
  pub error_code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TaskInfo {
  pub id: Uuid,
  pub status: TaskState,
  pub progress: u8,
  pub total_files: usize,
  pub completed_files: usize,
  pub current_file: Option<String>,
  pub results: Vec<FileResult>,
  pub error: Option<String>,
  pub error_code: Option<String>,
  pub started_at: DateTime<Local>,
  pub cancel_flag: Arc<AtomicBool>,
  /// 任务终态后回调的 URL（POST JSON）。失败会指数退避重试 3 次。
  pub webhook_url: Option<String>,
}

impl TaskInfo {
  pub fn new(id: Uuid, total_files: usize) -> Self {
    Self {
      id,
      status: TaskState::Pending,
      progress: 0,
      total_files,
      completed_files: 0,
      current_file: None,
      results: Vec::new(),
      error: None,
      error_code: None,
      started_at: Local::now(),
      cancel_flag: Arc::new(AtomicBool::new(false)),
      webhook_url: None,
    }
  }

  pub fn is_cancelled(&self) -> bool {
    self.cancel_flag.load(Ordering::SeqCst)
  }

  /// 在任务结束时统一设置最终状态。若期间用户请求过取消，则覆盖为 Cancelled。
  pub fn finalize(&mut self, failed: usize) {
    self.progress = 100;
    if self.cancel_flag.load(Ordering::SeqCst) {
      self.status = TaskState::Cancelled;
      if self.error.is_none() {
        self.error = Some("任务已取消".to_string());
        self.error_code = Some("cancelled".to_string());
      }
    } else if failed == 0 {
      self.status = TaskState::Completed;
    } else {
      self.status = TaskState::Failed;
    }
  }

  /// 单文件失败的终止：写入 error/code，再走 finalize。
  pub fn finalize_failed(&mut self, message: String, code: String) {
    self.error = Some(message);
    self.error_code = Some(code);
    self.finalize(1);
  }

  pub fn to_response(&self) -> TaskStatusResponse {
    TaskStatusResponse {
      task_id: self.id.to_string(),
      status: self.status.clone(),
      progress: self.progress,
      current_file: self.current_file.clone(),
      total_files: self.total_files,
      completed_files: self.completed_files,
      results: if self.results.is_empty() {
        None
      } else {
        Some(self.results.clone())
      },
      error: self.error.clone(),
      error_code: self.error_code.clone(),
      started_at: self.started_at.to_rfc3339(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TaskStatusResponse {
  pub task_id: String,
  pub status: TaskState,
  pub progress: u8,
  pub current_file: Option<String>,
  pub total_files: usize,
  pub completed_files: usize,
  pub results: Option<Vec<FileResult>>,
  pub error: Option<String>,
  pub error_code: Option<String>,
  pub started_at: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UpdateStatus {
  #[default]
  Idle,
  Downloading,
  Verifying,
  Ready,
  Launched,
  Cancelled,
  Failed,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct UpdateProgress {
  pub status: UpdateStatus,
  pub downloaded: u64,
  pub total: u64,
  pub installer_path: Option<String>,
  pub error: Option<String>,
  /// 客户端可据此判断是否为断点续传，UI 上做"续传中..."提示。
  pub is_resume: bool,
}

pub struct AppState {
  pub tasks: DashMap<Uuid, TaskInfo>,
  conversion_limit: Arc<Semaphore>,
  pub update_progress: Arc<Mutex<UpdateProgress>>,
  /// 取消下载的协作信号；下载循环在每个 chunk 后检查。
  pub update_cancel_flag: Arc<AtomicBool>,
  pub history: Arc<crate::history::HistoryStore>,
}

impl AppState {
  pub fn new() -> Self {
    Self {
      tasks: DashMap::new(),
      conversion_limit: Arc::new(Semaphore::new(1)),
      update_progress: Arc::new(Mutex::new(UpdateProgress::default())),
      update_cancel_flag: Arc::new(AtomicBool::new(false)),
      history: Arc::new(crate::history::HistoryStore::new()),
    }
  }

  pub fn create_task(&self, total_files: usize) -> Uuid {
    let id = Uuid::new_v4();
    self.tasks.insert(id, TaskInfo::new(id, total_files));
    id
  }

  pub fn get_cancel_flag(&self, id: Uuid) -> Option<Arc<AtomicBool>> {
    self.tasks.get(&id).map(|entry| entry.cancel_flag.clone())
  }

  /// 标记任务为取消并返回是否真正进行了状态切换。
  /// 若任务已完成 / 已失败 / 已取消，则只设置 flag 但不改变最终状态。
  pub fn request_cancel(&self, id: Uuid) -> bool {
    let Some(mut entry) = self.tasks.get_mut(&id) else {
      return false;
    };
    entry.cancel_flag.store(true, Ordering::SeqCst);
    if matches!(entry.status, TaskState::Pending | TaskState::Processing) {
      entry.status = TaskState::Cancelled;
      entry.error = Some("任务已取消".to_string());
      entry.error_code = Some("cancelled".to_string());
      true
    } else {
      false
    }
  }

  pub fn update_task<F>(&self, id: Uuid, updater: F)
  where
    F: FnOnce(&mut TaskInfo),
  {
    if let Some(mut entry) = self.tasks.get_mut(&id) {
      updater(&mut entry);
    }
  }

  pub fn get_task_response(&self, id: Uuid) -> Option<TaskStatusResponse> {
    self.tasks.get(&id).map(|entry| entry.to_response())
  }

  /// 任务进入终态后调用：把当前快照异步写入历史文件，并按需触发 webhook 回调。
  pub fn record_history(self: &Arc<Self>, id: Uuid) {
    let Some(snapshot) = self.get_task_response(id) else {
      return;
    };
    let webhook_url = self
      .tasks
      .get(&id)
      .and_then(|t| t.webhook_url.clone());
    let history = self.history.clone();
    tokio::spawn(async move {
      history.append(&snapshot).await;
      if let Some(url) = webhook_url {
        crate::webhook::deliver(&url, &snapshot).await;
      }
    });
  }

  pub async fn acquire_conversion_permit(&self) -> OwnedSemaphorePermit {
    self.conversion_limit
      .clone()
      .acquire_owned()
      .await
      .expect("conversion permit")
  }
}
