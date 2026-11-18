use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::settings::{
  self, CollisionStrategy, WatchFolder, MAX_CONVERSION_TIMEOUT_SECS, MIN_CONVERSION_TIMEOUT_SECS,
};

#[derive(Debug, Serialize, ToSchema)]
pub struct SettingsResponse {
  pub default_output_dir: Option<String>,
  pub conversion_timeout_secs: u64,
  pub conversion_timeout_min_secs: u64,
  pub conversion_timeout_max_secs: u64,
  pub output_collision_strategy: CollisionStrategy,
  pub watch_folders: Vec<WatchFolder>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateSettingsRequest {
  pub conversion_timeout_secs: Option<u64>,
  pub output_collision_strategy: Option<CollisionStrategy>,
  pub watch_folders: Option<Vec<WatchFolder>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UpdateSettingsErrorResponse {
  pub success: bool,
  pub message: String,
}

#[utoipa::path(
  get,
  path = "/api/settings",
  tag = "settings",
  responses((status = 200, description = "当前设置", body = SettingsResponse))
)]
pub async fn get_settings() -> Json<SettingsResponse> {
  Json(build_response())
}

#[utoipa::path(
  put,
  path = "/api/settings",
  tag = "settings",
  request_body = UpdateSettingsRequest,
  responses(
    (status = 200, description = "更新后的设置", body = SettingsResponse),
    (status = 400, description = "参数错误", body = UpdateSettingsErrorResponse),
  )
)]
pub async fn update_settings(Json(payload): Json<UpdateSettingsRequest>) -> impl IntoResponse {
  if let Some(secs) = payload.conversion_timeout_secs {
    if !(MIN_CONVERSION_TIMEOUT_SECS..=MAX_CONVERSION_TIMEOUT_SECS).contains(&secs) {
      return (
        StatusCode::BAD_REQUEST,
        Json(UpdateSettingsErrorResponse {
          success: false,
          message: format!(
            "转换超时必须在 {}-{} 秒之间",
            MIN_CONVERSION_TIMEOUT_SECS, MAX_CONVERSION_TIMEOUT_SECS
          ),
        }),
      )
        .into_response();
    }

    let mut current = settings::read_desktop_settings();
    current.conversion_timeout_secs = secs;
    if let Err(err) = settings::write_desktop_settings(&current) {
      return (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(UpdateSettingsErrorResponse {
          success: false,
          message: err.to_string(),
        }),
      )
        .into_response();
    }
  }

  if let Some(strategy) = payload.output_collision_strategy {
    let mut current = settings::read_desktop_settings();
    current.output_collision_strategy = strategy;
    if let Err(err) = settings::write_desktop_settings(&current) {
      return (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(UpdateSettingsErrorResponse {
          success: false,
          message: err.to_string(),
        }),
      )
        .into_response();
    }
  }

  if let Some(folders) = payload.watch_folders {
    let mut current = settings::read_desktop_settings();
    current.watch_folders = folders;
    if let Err(err) = settings::write_desktop_settings(&current) {
      return (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(UpdateSettingsErrorResponse {
          success: false,
          message: err.to_string(),
        }),
      )
        .into_response();
    }
  }

  Json(build_response()).into_response()
}

fn build_response() -> SettingsResponse {
  let cur = settings::read_desktop_settings();
  SettingsResponse {
    default_output_dir: settings::read_default_output_dir(),
    conversion_timeout_secs: settings::read_conversion_timeout_secs(),
    conversion_timeout_min_secs: MIN_CONVERSION_TIMEOUT_SECS,
    conversion_timeout_max_secs: MAX_CONVERSION_TIMEOUT_SECS,
    output_collision_strategy: cur.output_collision_strategy,
    watch_folders: cur.watch_folders,
  }
}
