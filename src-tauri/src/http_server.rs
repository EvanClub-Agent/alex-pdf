use std::sync::Arc;

use axum::{
  extract::{DefaultBodyLimit, Request},
  middleware::{self, Next},
  response::{Html, IntoResponse, Response},
  routing::{get, post},
  Json, Router,
};
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};

use crate::openapi::ApiDoc;
use crate::settings;
use utoipa::OpenApi;

use crate::handlers::{
  convert::convert_batch,
  convert::convert_batch_from_urls,
  convert::convert_from_url,
  convert::convert_single,
  diagnostics::get_diagnostics,
  health::{check_wps, get_engine_version, health_check},
  preview::preview_pdf,
  result::get_result,
  settings::{get_settings, update_settings},
  status::{cancel_task, get_queue, get_status, get_task_report_csv, list_tasks},
  update::{
    cancel_update, check_update, clear_pending_install, install_update, launch_update,
    pending_install_status, update_progress,
  },
};
use crate::state::AppState;

pub const SERVER_PORT: u16 = 24568;
pub const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;
pub const MAX_BODY_SIZE: usize = 120 * 1024 * 1024;

async fn serve_web_console() -> Html<&'static str> {
  Html(include_str!("web/index.html"))
}

async fn serve_swagger_ui() -> Html<&'static str> {
  Html(include_str!("web/docs.html"))
}

async fn serve_openapi_json() -> impl IntoResponse {
  let body = ApiDoc::openapi()
    .to_pretty_json()
    .unwrap_or_else(|err| format!("{{\"error\":\"{}\"}}", err));
  ([(axum::http::header::CONTENT_TYPE, "application/json")], body)
}

/// 若 token 已在桌面设置中开启，则要求所有 /api/* 请求携带 `Authorization: Bearer <token>`。
/// CORS 预检（OPTIONS）不带 Authorization 是合法的，直接放行。
async fn auth_middleware(req: Request, next: Next) -> Response {
  let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok());
  if !matches!(host, Some("127.0.0.1:24568" | "localhost:24568"))
    || req.headers().get(header::ORIGIN).is_some_and(|origin| !trusted_origin(origin))
  {
    return StatusCode::FORBIDDEN.into_response();
  }
  if req.method() == Method::OPTIONS {
    return next.run(req).await;
  }

  // 网页控制台静态首页与 OpenAPI 文档不需要鉴权（公开接口契约）。
  let path = req.uri().path();
  if path == "/" || path == "/api/docs" || path == "/api/openapi.json" {
    return next.run(req).await;
  }

  let Some(expected) = settings::read_api_token() else {
    return next.run(req).await;
  };

  let provided = req
    .headers()
    .get("authorization")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.strip_prefix("Bearer "))
    .map(str::trim);

  if provided != Some(expected.as_str()) {
    return (
      StatusCode::UNAUTHORIZED,
      Json(serde_json::json!({
        "success": false,
        "message": "缺少或无效的 API token",
      })),
    )
      .into_response();
  }

  next.run(req).await
}

pub async fn run(state: Arc<AppState>) -> anyhow::Result<()> {
  let app = Router::new()
    .route("/", get(serve_web_console))
    .route("/api/docs", get(serve_swagger_ui))
    .route("/api/openapi.json", get(serve_openapi_json))
    .route("/health", get(health_check))
    .route("/api/settings", get(get_settings).put(update_settings))
    .route("/api/check-wps", get(check_wps))
    .route("/api/diagnostics", get(get_diagnostics))
    .route("/api/engine-version", get(get_engine_version))
    .route("/api/convert", post(convert_single))
    .route("/api/convert-from-url", post(convert_from_url))
    .route("/api/convert-batch-from-urls", post(convert_batch_from_urls))
    .route("/api/convert-batch", post(convert_batch))
    .route("/api/preview", get(preview_pdf))
    .route("/api/result/:task_id", get(get_result))
    .route("/api/update/check", get(check_update))
    .route("/api/update/install", post(install_update))
    .route("/api/update/progress", get(update_progress))
    .route("/api/update/cancel", post(cancel_update))
    .route("/api/update/launch", post(launch_update))
    .route("/api/update/pending-install", get(pending_install_status))
    .route("/api/update/clear-pending-install", post(clear_pending_install))
    .route("/api/tasks", get(list_tasks))
    .route("/api/queue", get(get_queue))
    .route("/api/tasks/:task_id/report.csv", get(get_task_report_csv))
    .route("/api/status/:task_id", get(get_status))
    .route("/api/cancel/:task_id", post(cancel_task))
    .layer(middleware::from_fn(auth_middleware))
    .layer(cors_layer())
    .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
    .with_state(state);

  let listener = TcpListener::bind(("127.0.0.1", SERVER_PORT)).await?;
  axum::serve(listener, app).await?;
  Ok(())
}

fn cors_layer() -> CorsLayer {
  CorsLayer::new()
    .allow_origin(AllowOrigin::predicate(|origin, _| trusted_origin(origin)))
    .allow_methods([
      Method::GET,
      Method::POST,
      Method::PUT,
      Method::DELETE,
      Method::OPTIONS,
      Method::PATCH,
    ])
    .allow_headers([
      HeaderName::from_static("content-type"),
      HeaderName::from_static("authorization"),
      HeaderName::from_static("accept"),
      HeaderName::from_static("range"),
    ])
}

fn trusted_origin(value: &HeaderValue) -> bool {
  let origin = value.as_bytes();
  matches!(origin, b"http://127.0.0.1:24568" | b"http://localhost:24568"
    | b"tauri://localhost" | b"http://tauri.localhost" | b"https://tauri.localhost")
    || (cfg!(debug_assertions) && matches!(origin, b"http://localhost:5173" | b"http://127.0.0.1:5173"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn accepts_only_desktop_and_local_browser_origins() {
    for origin in ["http://tauri.localhost", "http://127.0.0.1:24568"] {
      assert!(trusted_origin(&HeaderValue::from_static(origin)));
    }
    for origin in ["null", "https://example.com", "http://localhost:8800"] {
      assert!(!trusted_origin(&HeaderValue::from_static(origin)));
    }
  }
}
