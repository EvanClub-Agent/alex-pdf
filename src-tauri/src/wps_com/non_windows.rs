use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::process_kill::KillTracker;
use super::types::{BatchItem, ConversionError, ConversionResult, DocumentKind, Engine, EngineProbe};

pub fn check_wps_com(_kind: DocumentKind) -> Result<String, ConversionError> {
  Err(ConversionError::ComUnavailable("Office/WPS COM 仅支持 Windows".to_string()))
}

pub fn check_engine(_engine: Engine, _kind: DocumentKind) -> Result<String, ConversionError> {
  Err(ConversionError::ComUnavailable("Office/WPS COM 仅支持 Windows".to_string()))
}

pub fn probe_engines(_kind: DocumentKind) -> Vec<EngineProbe> {
  vec![
    EngineProbe {
      engine: "wps",
      available: false,
      prog_id: None,
      version: None,
      error: Some("仅支持 Windows".to_string()),
    },
    EngineProbe {
      engine: "office",
      available: false,
      prog_id: None,
      version: None,
      error: Some("仅支持 Windows".to_string()),
    },
  ]
}

pub fn get_wps_version(_prog_id: &str) -> Result<String, ConversionError> {
  Err(ConversionError::ComUnavailable("Office/WPS COM 仅支持 Windows".to_string()))
}

pub fn convert_to_pdf(
  _kind: DocumentKind,
  _prog_id: &str,
  _input_path: &Path,
  _output_path: &Path,
  _tracker: Option<&KillTracker>,
) -> Result<ConversionResult, ConversionError> {
  Err(ConversionError::ComUnavailable("Office/WPS COM 仅支持 Windows".to_string()))
}

pub fn convert_batch_to_pdf<F>(
  _kind: DocumentKind,
  _prog_id: &str,
  _items: &[BatchItem],
  _cancel: Option<&AtomicBool>,
  _tracker: &KillTracker,
  _on_progress: F,
) -> Result<Vec<Result<ConversionResult, ConversionError>>, ConversionError>
where
  F: FnMut(usize, usize, &BatchItem),
{
  Err(ConversionError::ComUnavailable("Office/WPS COM 仅支持 Windows".to_string()))
}
