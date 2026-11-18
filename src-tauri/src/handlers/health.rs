use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::settings;
use crate::state::AppState;
use crate::wps_com;
use crate::wps_com::types::{DocumentKind, Engine, EngineProbe};

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
  pub status: String,
  pub version: String,
  pub wps_available: bool,
  pub wps_prog_id: Option<String>,
  pub engine: Engine,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckWpsResponse {
  pub available: bool,
  pub prog_id: Option<String>,
  pub version: Option<String>,
  pub error: Option<String>,
  pub engine: Engine,
  pub engines: Vec<EngineProbe>,
}

#[utoipa::path(
  get,
  path = "/health",
  tag = "health",
  responses((status = 200, description = "服务健康", body = HealthResponse))
)]
pub async fn health_check(State(_state): State<Arc<AppState>>) -> Json<HealthResponse> {
  let engine = settings::read_desktop_settings().engine;
  let (available, prog_id) = detect_basic(engine).await;

  Json(HealthResponse {
    status: "ok".to_string(),
    version: env!("CARGO_PKG_VERSION").to_string(),
    wps_available: available,
    wps_prog_id: prog_id,
    engine,
  })
}

#[utoipa::path(
  get,
  path = "/api/check-wps",
  tag = "health",
  responses((status = 200, description = "WPS / Office 引擎探测结果", body = CheckWpsResponse))
)]
pub async fn check_wps(State(_state): State<Arc<AppState>>) -> Json<CheckWpsResponse> {
  // 快速路径: 只走注册表查询，不实例化 Office/WPS COM 服务器。
  // 版本号通过独立的 /api/engine-version 按需获取。
  let engine = settings::read_desktop_settings().engine;

  let probes =
    tokio::task::spawn_blocking(|| wps_com::probe_engines(DocumentKind::Word))
      .await
      .unwrap_or_default();

  let result =
    tokio::task::spawn_blocking(move || wps_com::check_engine(engine, DocumentKind::Word)).await;

  match result {
    Ok(Ok(prog_id)) => Json(CheckWpsResponse {
      available: true,
      prog_id: Some(prog_id),
      version: None,
      error: None,
      engine,
      engines: probes,
    }),
    Ok(Err(err)) => Json(CheckWpsResponse {
      available: false,
      prog_id: None,
      version: None,
      error: Some(err.to_string()),
      engine,
      engines: probes,
    }),
    Err(err) => Json(CheckWpsResponse {
      available: false,
      prog_id: None,
      version: None,
      error: Some(err.to_string()),
      engine,
      engines: probes,
    }),
  }
}

#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
pub struct EngineVersionQuery {
  pub engine: Engine,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EngineVersionResponse {
  pub engine: Engine,
  pub available: bool,
  pub version: Option<String>,
  pub error: Option<String>,
}

/// 慢速路径: 实例化 COM 服务器读取版本号。仅在用户主动点击"获取版本"时调用。
#[utoipa::path(
  get,
  path = "/api/engine-version",
  tag = "health",
  params(EngineVersionQuery),
  responses((status = 200, description = "引擎版本", body = EngineVersionResponse))
)]
pub async fn get_engine_version(
  axum::extract::Query(query): axum::extract::Query<EngineVersionQuery>,
) -> Json<EngineVersionResponse> {
  let engine = query.engine;
  let result = tokio::task::spawn_blocking(move || {
    let prog_id = wps_com::check_engine(engine, DocumentKind::Word)?;
    let version = wps_com::get_wps_version(&prog_id)?;
    Ok::<_, crate::wps_com::types::ConversionError>(version)
  })
  .await;

  match result {
    Ok(Ok(version)) => Json(EngineVersionResponse {
      engine,
      available: true,
      version: Some(version),
      error: None,
    }),
    Ok(Err(err)) => Json(EngineVersionResponse {
      engine,
      available: false,
      version: None,
      error: Some(err.to_string()),
    }),
    Err(err) => Json(EngineVersionResponse {
      engine,
      available: false,
      version: None,
      error: Some(err.to_string()),
    }),
  }
}

async fn detect_basic(engine: Engine) -> (bool, Option<String>) {
  let result =
    tokio::task::spawn_blocking(move || wps_com::check_engine(engine, DocumentKind::Word)).await;
  match result {
    Ok(Ok(prog_id)) => (true, Some(prog_id)),
    _ => (false, None),
  }
}
