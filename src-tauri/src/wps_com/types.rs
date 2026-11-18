use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
  #[default]
  Auto,
  Wps,
  Office,
}

impl Engine {
  pub fn label(&self) -> &'static str {
    match self {
      Self::Auto => "auto",
      Self::Wps => "wps",
      Self::Office => "office",
    }
  }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EngineProbe {
  #[schema(value_type = String)]
  pub engine: &'static str,
  pub available: bool,
  pub prog_id: Option<String>,
  pub version: Option<String>,
  pub error: Option<String>,
}

#[derive(Debug)]
pub struct ConversionResult {
  pub input: PathBuf,
  pub output: PathBuf,
  pub file_size: u64,
  pub duration_ms: u128,
}

#[derive(Debug, Clone)]
pub struct BatchItem {
  pub file_name: String,
  pub input: PathBuf,
  pub output: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
  Word,
  Excel,
}

impl DocumentKind {
  pub fn from_extension(ext: &str) -> Option<Self> {
    match ext.to_ascii_lowercase().as_str() {
      "doc" | "docx" => Some(Self::Word),
      "xls" | "xlsx" => Some(Self::Excel),
      _ => None,
    }
  }

  pub fn label(&self) -> &'static str {
    match self {
      Self::Word => "word",
      Self::Excel => "excel",
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
  /// Office / WPS 引擎未安装，或 COM 初始化失败。
  #[error("Office/WPS COM unavailable: {0}")]
  ComUnavailable(String),
  #[error("input file not found: {0}")]
  FileNotFound(String),
  #[error("password protected: {0}")]
  PasswordProtected(String),
  #[error("file corrupted: {0}")]
  FileCorrupted(String),
  #[error("unsupported format: {0}")]
  UnsupportedFormat(String),
  /// COM 调用通过、但 ExportAsFixedFormat 等导出过程失败。
  #[error("export failed: {0}")]
  ExportFailed(String),
  #[error("conversion timed out after {0}s")]
  Timeout(u64),
  #[error("conversion cancelled")]
  Cancelled,
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
  #[error("conversion failed: {0}")]
  Unknown(String),
}

impl ConversionError {
  /// 稳定的字符串错误码，前端按此分类展示文案。
  pub fn code(&self) -> &'static str {
    match self {
      Self::ComUnavailable(_) => "com_unavailable",
      Self::FileNotFound(_) => "file_not_found",
      Self::PasswordProtected(_) => "password_protected",
      Self::FileCorrupted(_) => "file_corrupted",
      Self::UnsupportedFormat(_) => "unsupported_format",
      Self::ExportFailed(_) => "export_failed",
      Self::Timeout(_) => "timeout",
      Self::Cancelled => "cancelled",
      Self::Io(_) => "io_error",
      Self::Unknown(_) => "unknown",
    }
  }
}

/// 根据 COM 报错文本粗分类。HRESULT 文本不稳定，能匹配则细分，匹配不到回退到 ExportFailed。
pub fn classify_export_error(message: &str) -> ConversionError {
  let lower = message.to_ascii_lowercase();
  let has = |needle: &str| lower.contains(needle);

  if has("password") || has("密码") || has("0x800a1730") {
    return ConversionError::PasswordProtected(message.to_string());
  }
  if has("corrupt") || has("damage") || has("损坏") || has("已损坏") {
    return ConversionError::FileCorrupted(message.to_string());
  }
  if has("unsupported") || has("not supported") || has("not recognized") || has("不支持") || has("无法识别") {
    return ConversionError::UnsupportedFormat(message.to_string());
  }
  ConversionError::ExportFailed(message.to_string())
}
