use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::wps_com::types::Engine;

/// 输出 PDF 与已有同名文件冲突时的处理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CollisionStrategy {
  /// 直接覆盖。
  Overwrite,
  /// 在文件名后追加 _1 / _2 …，找到第一个可用名。
  #[default]
  AutoNumber,
  /// 跳过转换，原文件保留。
  Skip,
}

const APP_DATA_DIR: &str = "com.wordtopdf.converter";
const OUTPUT_DIR_FILE: &str = "default_output_dir.txt";
const DESKTOP_SETTINGS_FILE: &str = "desktop_settings.json";

pub const DEFAULT_CONVERSION_TIMEOUT_SECS: u64 = 120;
pub const MIN_CONVERSION_TIMEOUT_SECS: u64 = 10;
pub const MAX_CONVERSION_TIMEOUT_SECS: u64 = 1800;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WatchFolder {
  pub src: String,
  pub dst: String,
  #[serde(default)]
  pub delete_source: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopSettings {
  pub close_to_tray: bool,
  pub silent_start_on_login: bool,
  pub engine: Engine,
  pub conversion_timeout_secs: u64,
  /// 启用本地 HTTP API 鉴权时的 Bearer token；为 None 表示禁用鉴权。
  pub api_token: Option<String>,
  pub output_collision_strategy: CollisionStrategy,
  pub watch_folders: Vec<WatchFolder>,
}

impl Default for DesktopSettings {
  fn default() -> Self {
    Self {
      close_to_tray: true,
      silent_start_on_login: true,
      engine: Engine::Auto,
      conversion_timeout_secs: DEFAULT_CONVERSION_TIMEOUT_SECS,
      api_token: None,
      output_collision_strategy: CollisionStrategy::AutoNumber,
      watch_folders: Vec::new(),
    }
  }
}

pub fn read_collision_strategy() -> CollisionStrategy {
  read_desktop_settings().output_collision_strategy
}

/// 解决输出冲突：根据策略给出最终目标路径，或返回 None 表示跳过。
pub fn resolve_output_path(target: &std::path::Path, strategy: CollisionStrategy) -> Option<PathBuf> {
  if !target.exists() {
    return Some(target.to_path_buf());
  }
  match strategy {
    CollisionStrategy::Overwrite => Some(target.to_path_buf()),
    CollisionStrategy::Skip => None,
    CollisionStrategy::AutoNumber => {
      let stem = target
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
      let ext = target.extension().and_then(|s| s.to_str()).unwrap_or("pdf");
      let parent = target.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
      for n in 1..1000 {
        let candidate = parent.join(format!("{}_{}.{}", stem, n, ext));
        if !candidate.exists() {
          return Some(candidate);
        }
      }
      // 极端情况兜底：仍返回 target，让上层走 overwrite 行为。
      Some(target.to_path_buf())
    }
  }
}

pub fn generate_api_token() -> String {
  let mut s = Uuid::new_v4().simple().to_string();
  s.push_str(&Uuid::new_v4().simple().to_string());
  s
}

pub fn read_api_token() -> Option<String> {
  read_desktop_settings()
    .api_token
    .filter(|t| !t.is_empty())
}

pub fn read_conversion_timeout_secs() -> u64 {
  let raw = read_desktop_settings().conversion_timeout_secs;
  raw.clamp(MIN_CONVERSION_TIMEOUT_SECS, MAX_CONVERSION_TIMEOUT_SECS)
}

/// 当用户未传 `output_folder` 且未设置默认输出目录时使用的回退目录。
/// 优先使用 `<我的文档>/PDFConverter`，回退到用户主目录下的 `PDFConverter`，
/// 再不行才用系统临时目录。这样产物文件不会被系统临时清理误删。
pub fn fallback_output_dir() -> PathBuf {
  if let Some(profile) = std::env::var_os("USERPROFILE") {
    return PathBuf::from(profile).join("Documents").join("PDFConverter");
  }
  if let Some(home) = std::env::var_os("HOME") {
    return PathBuf::from(home).join("PDFConverter");
  }
  std::env::temp_dir().join("wps-pdf-output")
}

pub fn read_default_output_dir() -> Option<String> {
  let path = default_output_dir_path()?;
  let bytes = std::fs::read(path).ok()?;
  // NSIS writes UTF-16LE with a BOM. Settings written by AlexPDF use UTF-8.
  let contents = if let Some(encoded) = bytes.strip_prefix(&[0xff, 0xfe]) {
    let pairs = encoded.chunks_exact(2);
    if !pairs.remainder().is_empty() {
      return None;
    }
    let units: Vec<_> = pairs.map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
    String::from_utf16(&units).ok()?
  } else {
    String::from_utf8(bytes).ok()?
  };
  let trimmed = contents.trim();
  if trimmed.is_empty() {
    None
  } else {
    Some(trimmed.to_string())
  }
}

pub fn read_desktop_settings() -> DesktopSettings {
  let Some(path) = desktop_settings_path() else {
    return DesktopSettings::default();
  };

  std::fs::read_to_string(path)
    .ok()
    .and_then(|contents| serde_json::from_str::<DesktopSettings>(&contents).ok())
    .unwrap_or_default()
}

pub fn write_desktop_settings(settings: &DesktopSettings) -> Result<()> {
  let Some(path) = desktop_settings_path() else {
    return Ok(());
  };

  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)
      .with_context(|| format!("failed to create settings directory: {}", parent.display()))?;
  }

  let content = serde_json::to_string_pretty(settings)?;
  std::fs::write(&path, content)
    .with_context(|| format!("failed to write settings file: {}", path.display()))?;
  Ok(())
}

fn default_output_dir_path() -> Option<PathBuf> {
  app_data_dir().map(|dir| dir.join(OUTPUT_DIR_FILE))
}

fn desktop_settings_path() -> Option<PathBuf> {
  app_data_dir().map(|dir| dir.join(DESKTOP_SETTINGS_FILE))
}

fn app_data_dir() -> Option<PathBuf> {
  if !cfg!(target_os = "windows") {
    return None;
  }
  let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("LOCALAPPDATA"))?;
  Some(PathBuf::from(base).join(APP_DATA_DIR))
}
