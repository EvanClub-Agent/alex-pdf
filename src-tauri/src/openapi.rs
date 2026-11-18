//! 由 utoipa 自动生成的 OpenAPI 描述。所有 handler 处的 `#[utoipa::path]` 与
//! 这里 `paths()` 列表汇合，schema 通过 `components(schemas(...))` 注册。

use utoipa::OpenApi;

use crate::handlers::{
  convert, diagnostics, health, preview, result, settings, status, update,
};
use crate::settings::{CollisionStrategy, WatchFolder};
use crate::state::{FileResult, TaskState, TaskStatusResponse, UpdateProgress, UpdateStatus};
use crate::wps_com::types::{Engine, EngineProbe};

#[derive(OpenApi)]
#[openapi(
  info(
    title = "AlexPDF · 本地 HTTP API",
    version = "1.0.0",
    description = "Tauri 桌面应用内嵌的本地 HTTP 服务。\n开启 API token 后所有 /api/* 请求需要携带 `Authorization: Bearer <token>`。"
  ),
  paths(
    health::health_check,
    health::check_wps,
    health::get_engine_version,
    diagnostics::get_diagnostics,
    settings::get_settings,
    settings::update_settings,
    convert::convert_single,
    convert::convert_from_url,
    convert::convert_batch,
    convert::convert_batch_from_urls,
    status::get_status,
    status::cancel_task,
    status::list_tasks,
    status::get_queue,
    status::get_task_report_csv,
    preview::preview_pdf,
    result::get_result,
    update::check_update,
    update::install_update,
    update::update_progress,
    update::cancel_update,
    update::launch_update,
    update::pending_install_status,
    update::clear_pending_install,
  ),
  components(schemas(
    CollisionStrategy,
    WatchFolder,
    Engine,
    EngineProbe,
    TaskState,
    FileResult,
    TaskStatusResponse,
    UpdateStatus,
    UpdateProgress,
    health::HealthResponse,
    health::CheckWpsResponse,
    health::EngineVersionResponse,
    diagnostics::DiagnosticsResponse,
    diagnostics::EngineSection,
    diagnostics::TasksSection,
    diagnostics::RecentFailure,
    diagnostics::StorageSection,
    diagnostics::SecuritySection,
    settings::SettingsResponse,
    settings::UpdateSettingsRequest,
    settings::UpdateSettingsErrorResponse,
    convert::SingleConvertResponse,
    convert::BatchConvertResponse,
    convert::ConvertFromUrlRequest,
    convert::ConvertBatchFromUrlsRequest,
    convert::ErrorResponse,
    status::CancelTaskResponse,
    status::ListTasksResponse,
    status::QueueCounts,
    status::QueueResponse,
    update::UpdateCheckResponse,
    update::UpdateInstallRequest,
    update::UpdateInstallResponse,
    update::PendingInstall,
    update::PendingInstallResponse,
  )),
  tags(
    (name = "health", description = "健康检查与引擎探测"),
    (name = "settings", description = "服务端设置"),
    (name = "convert", description = "文档转换"),
    (name = "tasks", description = "任务状态/历史/取消"),
    (name = "preview", description = "预览转换产物"),
    (name = "update", description = "桌面应用更新分发"),
  ),
)]
pub struct ApiDoc;
