use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use std::mem::ManuallyDrop;

use windows::core::{ComInterface, BSTR, GUID, PCWSTR};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{
  CLSCTX_LOCAL_SERVER, CLSIDFromProgID, CoCreateInstance, CoFreeUnusedLibraries, CoInitializeEx,
  CoUninitialize,
  COINIT_APARTMENTTHREADED, DISPPARAMS, DISPATCH_FLAGS, EXCEPINFO, IDispatch,
};
use windows::Win32::System::Variant::{
  VariantClear, VARIANT, VT_BSTR, VT_BOOL, VT_DISPATCH, VT_I4, VT_UNKNOWN,
};

use super::process_kill::KillTracker;
use super::types::{
  classify_export_error, BatchItem, ConversionError, ConversionResult, DocumentKind, Engine,
  EngineProbe,
};

const WPS_WORD_PROG_IDS: [&str; 4] = [
  "WPS.Application",
  "KWPS.Application",
  "Ket.Application",
  "Kingsoft.WPS.Application",
];
const WPS_EXCEL_PROG_IDS: [&str; 3] = [
  "ET.Application",
  "KET.Application",
  "Kingsoft.ET.Application",
];
const OFFICE_WORD_PROG_IDS: [&str; 1] = ["Word.Application"];
const OFFICE_EXCEL_PROG_IDS: [&str; 1] = ["Excel.Application"];
const WD_EXPORT_FORMAT_PDF: i32 = 17;
const XL_EXPORT_FORMAT_PDF: i32 = 0;
const DISPATCH_METHOD: DISPATCH_FLAGS = DISPATCH_FLAGS(0x1);
const DISPATCH_PROPERTYGET: DISPATCH_FLAGS = DISPATCH_FLAGS(0x2);
const DISPATCH_PROPERTYPUT: DISPATCH_FLAGS = DISPATCH_FLAGS(0x4);
const DISPID_PROPERTYPUT: i32 = -3;

fn prog_ids_for(engine: Engine, kind: DocumentKind) -> Vec<&'static str> {
  match engine {
    Engine::Wps => match kind {
      DocumentKind::Word => WPS_WORD_PROG_IDS.to_vec(),
      DocumentKind::Excel => WPS_EXCEL_PROG_IDS.to_vec(),
    },
    Engine::Office => match kind {
      DocumentKind::Word => OFFICE_WORD_PROG_IDS.to_vec(),
      DocumentKind::Excel => OFFICE_EXCEL_PROG_IDS.to_vec(),
    },
    Engine::Auto => match kind {
      DocumentKind::Word => {
        let mut ids = WPS_WORD_PROG_IDS.to_vec();
        ids.extend_from_slice(&OFFICE_WORD_PROG_IDS);
        ids
      }
      DocumentKind::Excel => {
        let mut ids = WPS_EXCEL_PROG_IDS.to_vec();
        ids.extend_from_slice(&OFFICE_EXCEL_PROG_IDS);
        ids
      }
    },
  }
}

pub fn check_wps_com(kind: DocumentKind) -> Result<String, ConversionError> {
  check_engine(Engine::Auto, kind)
}

pub fn check_engine(engine: Engine, kind: DocumentKind) -> Result<String, ConversionError> {
  for prog_id in prog_ids_for(engine, kind) {
    if clsid_from_progid(prog_id).is_ok() {
      return Ok(prog_id.to_string());
    }
  }
  Err(ConversionError::ComUnavailable("Office/WPS COM 组件未注册".to_string()))
}

/// 仅通过注册表查询判断引擎是否安装。不会启动 COM 服务器。
/// 这是启动时引擎识别的快速路径 —— 毫秒级完成。
pub fn probe_engines(kind: DocumentKind) -> Vec<EngineProbe> {
  [Engine::Wps, Engine::Office]
    .into_iter()
    .map(|engine| match check_engine(engine, kind) {
      Ok(prog_id) => EngineProbe {
        engine: engine.label(),
        available: true,
        prog_id: Some(prog_id),
        version: None,
        error: None,
      },
      Err(err) => EngineProbe {
        engine: engine.label(),
        available: false,
        prog_id: None,
        version: None,
        error: Some(err.to_string()),
      },
    })
    .collect()
}

pub fn get_wps_version(prog_id: &str) -> Result<String, ConversionError> {
  // Query the running automation object for its version string.
  let _com = ComGuard::new()?;
  let app = AppGuard::new(prog_id)?;
  let variant = invoke_get(app.inner(), "Version")?;
  variant_to_string(variant).or_else(|_| Ok("unknown".to_string()))
}

pub fn convert_to_pdf(
  kind: DocumentKind,
  prog_id: &str,
  input_path: &Path,
  output_path: &Path,
  tracker: Option<&KillTracker>,
) -> Result<ConversionResult, ConversionError> {
  // Each conversion runs on its own thread with a dedicated COM apartment.
  let _com = ComGuard::new()?;
  let app = AppGuard::new(prog_id)?;

  if let Some(tracker) = tracker {
    track_app_process(app.inner(), tracker);
  }

  // Keep WPS invisible and silent for conversion.
  invoke_put(app.inner(), "Visible", variant_from_bool(false))?;
  invoke_put(app.inner(), "DisplayAlerts", variant_from_i32(0))?;

  let result = convert_with_app(kind, app.inner(), input_path, output_path);
  result
}

pub fn convert_batch_to_pdf<F>(
  kind: DocumentKind,
  prog_id: &str,
  items: &[BatchItem],
  cancel: Option<&AtomicBool>,
  tracker: &KillTracker,
  mut on_progress: F,
) -> Result<Vec<Result<ConversionResult, ConversionError>>, ConversionError>
where
  F: FnMut(usize, usize, &BatchItem),
{
  if items.is_empty() {
    return Ok(Vec::new());
  }

  let _com = ComGuard::new()?;
  let app = AppGuard::new(prog_id)?;

  track_app_process(app.inner(), tracker);
  invoke_put(app.inner(), "Visible", variant_from_bool(false))?;
  invoke_put(app.inner(), "DisplayAlerts", variant_from_i32(0))?;

  let total = items.len();
  let mut results = Vec::with_capacity(total);

  for (index, item) in items.iter().enumerate() {
    // 在每个文件开始前检查取消信号；同步 COM 调用本身不能中断，
    // 只能在文件之间响应。剩余文件标记为 Cancelled。
    if cancel.map_or(false, |flag| flag.load(Ordering::SeqCst)) {
      for _ in index..total {
        results.push(Err(ConversionError::Cancelled));
      }
      return Ok(results);
    }
    let result = convert_with_app(kind, app.inner(), &item.input, &item.output);
    results.push(result);
    on_progress(index + 1, total, item);
  }

  Ok(results)
}

// Office exposes its top-level window as a COM long. Some WPS versions omit Hwnd.
fn track_app_process(app: &IDispatch, tracker: &KillTracker) {
  if let Ok(mut window) = invoke_get(app, "Hwnd") {
    unsafe {
      let value = &*window.Anonymous.Anonymous;
      if value.vt == VT_I4 {
        tracker.track_window(windows::Win32::Foundation::HWND(value.Anonymous.lVal as isize));
      }
      let _ = VariantClear(&mut window);
    }
  }
}

fn convert_with_app(
  kind: DocumentKind,
  app: &IDispatch,
  input_path: &Path,
  output_path: &Path,
) -> Result<ConversionResult, ConversionError> {
  if !input_path.exists() {
    return Err(ConversionError::FileNotFound(input_path.display().to_string()));
  }

  let start = Instant::now();
  let input_abs = strip_unc_prefix(input_path.canonicalize()?);
  let output_abs = strip_unc_prefix(to_abs_path(output_path)?);

  let input_str = input_abs.display().to_string();
  let output_str = output_abs.display().to_string();

  match kind {
    DocumentKind::Word => {
      let documents_variant = invoke_get(app, "Documents")?;
      let documents = variant_to_dispatch(documents_variant)?;
      let doc_variant = invoke_method(&documents, "Open", vec![variant_from_str(&input_str)])
        .map_err(reclassify_export_error)?;
      let doc = variant_to_dispatch(doc_variant)?;
      let export_result = invoke_method(
        &doc,
        "ExportAsFixedFormat",
        vec![variant_from_str(&output_str), variant_from_i32(WD_EXPORT_FORMAT_PDF)],
      )
      .map(|_| ())
      .map_err(reclassify_export_error);
      let _ = invoke_method(&doc, "Close", Vec::new());
      export_result?;
    }
    DocumentKind::Excel => {
      let workbooks_variant = invoke_get(app, "Workbooks")?;
      let workbooks = variant_to_dispatch(workbooks_variant)?;
      let workbook_variant =
        invoke_method(&workbooks, "Open", vec![variant_from_str(&input_str)])
          .map_err(reclassify_export_error)?;
      let workbook = variant_to_dispatch(workbook_variant)?;
      let export_result = invoke_method(
        &workbook,
        "ExportAsFixedFormat",
        vec![variant_from_i32(XL_EXPORT_FORMAT_PDF), variant_from_str(&output_str)],
      )
      .map(|_| ())
      .map_err(reclassify_export_error);
      let _ = invoke_method(&workbook, "Close", vec![variant_from_bool(false)]);
      export_result?;
    }
  }

  let duration_ms = start.elapsed().as_millis();
  let file_size = std::fs::metadata(&output_abs).map(|m| m.len()).unwrap_or(0);

  Ok(ConversionResult {
    input: input_abs,
    output: output_abs,
    file_size,
    duration_ms,
  })
}

struct ComGuard;

impl ComGuard {
  fn new() -> Result<Self, ConversionError> {
    unsafe {
      CoInitializeEx(None, COINIT_APARTMENTTHREADED)
        .map_err(|e| ConversionError::ComUnavailable(format!("CoInitializeEx failed: {}", e)))?;
    }
    Ok(Self)
  }
}

impl Drop for ComGuard {
  fn drop(&mut self) {
    unsafe {
      CoUninitialize();
    }
  }
}

fn create_app(prog_id: &str) -> Result<IDispatch, ConversionError> {
  let clsid = clsid_from_progid(prog_id)?;
  let app: IDispatch = unsafe {
    CoCreateInstance(&clsid, None, CLSCTX_LOCAL_SERVER).map_err(map_com_err)?
  };
  Ok(app)
}

fn clsid_from_progid(prog_id: &str) -> Result<GUID, ConversionError> {
  let wide = to_wide(prog_id);
  let clsid = unsafe { CLSIDFromProgID(PCWSTR(wide.as_ptr())) }.map_err(map_com_err)?;
  Ok(clsid)
}

fn invoke_get(dispatch: &IDispatch, name: &str) -> Result<VARIANT, ConversionError> {
  let dispid = get_dispid(dispatch, name)?;
  // Office expects PROPERTYGET | METHOD; bare PROPERTYGET fails on some Word/Excel hosts.
  let flags = DISPATCH_FLAGS(DISPATCH_PROPERTYGET.0 | DISPATCH_METHOD.0);
  invoke(dispatch, dispid, flags, Vec::new())
}

fn invoke_put(dispatch: &IDispatch, name: &str, value: VARIANT) -> Result<(), ConversionError> {
  let dispid = get_dispid(dispatch, name)?;
  let _ = invoke(dispatch, dispid, DISPATCH_PROPERTYPUT, vec![value])?;
  Ok(())
}

fn invoke_method(
  dispatch: &IDispatch,
  name: &str,
  args: Vec<VARIANT>,
) -> Result<VARIANT, ConversionError> {
  let dispid = get_dispid(dispatch, name)?;
  invoke(dispatch, dispid, DISPATCH_METHOD, args)
}

fn invoke(
  dispatch: &IDispatch,
  dispid: i32,
  flags: DISPATCH_FLAGS,
  mut args: Vec<VARIANT>,
) -> Result<VARIANT, ConversionError> {
  // COM expects arguments in reverse order.
  args.reverse();

  let mut result = VARIANT::default();
  let mut excep = EXCEPINFO::default();
  let mut arg_err = 0u32;

  // Property puts must pass DISPID_PROPERTYPUT as a named argument.
  let mut dispid_named = DISPID_PROPERTYPUT;
  let params = DISPPARAMS {
    rgvarg: if args.is_empty() {
      std::ptr::null_mut()
    } else {
      args.as_mut_ptr()
    },
    rgdispidNamedArgs: if flags.0 & DISPATCH_PROPERTYPUT.0 != 0 {
      &mut dispid_named
    } else {
      std::ptr::null_mut()
    },
    cArgs: args.len() as u32,
    cNamedArgs: if flags.0 & DISPATCH_PROPERTYPUT.0 != 0 {
      1
    } else {
      0
    },
  };

  unsafe {
    dispatch
      .Invoke(
        dispid,
        &GUID::zeroed(),
        0,
        flags,
        &params,
        Some(&mut result as *mut VARIANT),
        Some(&mut excep as *mut EXCEPINFO),
        Some(&mut arg_err as *mut u32),
      )
      .map_err(map_com_err)?;
  }

  unsafe {
    for arg in args.iter_mut() {
      let _ = VariantClear(arg);
    }
  }

  Ok(result)
}

fn get_dispid(dispatch: &IDispatch, name: &str) -> Result<i32, ConversionError> {
  let mut dispid = 0i32;
  let wide = to_wide(name);
  let names = [PCWSTR(wide.as_ptr())];
  unsafe {
    dispatch
      .GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, 0, &mut dispid)
      .map_err(map_com_err)?;
  }
  Ok(dispid)
}

fn variant_to_dispatch(mut variant: VARIANT) -> Result<IDispatch, ConversionError> {
  let inner = unsafe { &*variant.Anonymous.Anonymous };
  let vt = inner.vt;
  if vt == VT_DISPATCH {
    let dispatch = unsafe { &*inner.Anonymous.pdispVal };
    let result = dispatch
      .clone()
      .ok_or_else(|| ConversionError::Unknown("IDispatch pointer missing".to_string()))?;
    unsafe {
      let _ = VariantClear(&mut variant);
    }
    return Ok(result);
  }

  if vt == VT_UNKNOWN {
    let unknown = unsafe { &*inner.Anonymous.punkVal };
    let unknown_ref = unknown.as_ref().ok_or_else(|| {
      ConversionError::Unknown("IUnknown pointer missing".to_string())
    })?;
    let result: IDispatch = unknown_ref.cast().map_err(|err| {
      ConversionError::Unknown(format!("IUnknown -> IDispatch cast failed: {}", err))
    })?;
    unsafe {
      let _ = VariantClear(&mut variant);
    }
    return Ok(result);
  }

  Err(ConversionError::Unknown(format!(
    "VARIANT is not an IDispatch (vt=0x{:04x})",
    vt.0
  )))
}

fn variant_to_string(mut variant: VARIANT) -> Result<String, ConversionError> {
  let inner = unsafe { &*variant.Anonymous.Anonymous };
  if inner.vt != VT_BSTR {
    return Err(ConversionError::Unknown(
      "VARIANT is not a string".to_string(),
    ));
  }
  let bstr = unsafe { &*inner.Anonymous.bstrVal };
  let value = bstr.to_string();
  unsafe {
    let _ = VariantClear(&mut variant);
  }
  Ok(value)
}

fn variant_from_str(value: &str) -> VARIANT {
  let bstr = BSTR::from(value);
  let mut variant = VARIANT::default();
  unsafe {
    let inner = &mut *variant.Anonymous.Anonymous;
    inner.vt = VT_BSTR;
    inner.Anonymous.bstrVal = ManuallyDrop::new(bstr);
  }
  variant
}

fn variant_from_bool(value: bool) -> VARIANT {
  let mut variant = VARIANT::default();
  let bool_val = if value { VARIANT_BOOL(-1) } else { VARIANT_BOOL(0) };
  unsafe {
    let inner = &mut *variant.Anonymous.Anonymous;
    inner.vt = VT_BOOL;
    inner.Anonymous.boolVal = bool_val;
  }
  variant
}

fn variant_from_i32(value: i32) -> VARIANT {
  let mut variant = VARIANT::default();
  unsafe {
    let inner = &mut *variant.Anonymous.Anonymous;
    inner.vt = VT_I4;
    inner.Anonymous.lVal = value;
  }
  variant
}

fn to_wide(value: &str) -> Vec<u16> {
  let mut wide: Vec<u16> = value.encode_utf16().collect();
  wide.push(0);
  wide
}

fn to_abs_path(path: &Path) -> Result<PathBuf, ConversionError> {
  if path.is_absolute() {
    Ok(path.to_path_buf())
  } else {
    Ok(std::env::current_dir()?.join(path))
  }
}

// Office (Word/Excel) COM rejects \\?\ extended-length paths returned by canonicalize().
fn strip_unc_prefix(path: PathBuf) -> PathBuf {
  let s = path.display().to_string();
  if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
    return PathBuf::from(format!(r"\\{}", rest));
  }
  if let Some(rest) = s.strip_prefix(r"\\?\") {
    return PathBuf::from(rest);
  }
  path
}

fn map_com_err(err: windows::core::Error) -> ConversionError {
  ConversionError::Unknown(err.to_string())
}

/// 把通用 COM 错误重分类为 PasswordProtected / FileCorrupted / UnsupportedFormat / ExportFailed。
fn reclassify_export_error(err: ConversionError) -> ConversionError {
  match err {
    ConversionError::Unknown(message) | ConversionError::ExportFailed(message) => {
      classify_export_error(&message)
    }
    other => other,
  }
}

struct AppGuard {
  app: IDispatch,
}

impl AppGuard {
  fn new(prog_id: &str) -> Result<Self, ConversionError> {
    let app = create_app(prog_id)?;
    Ok(Self { app })
  }

  fn inner(&self) -> &IDispatch {
    &self.app
  }
}

impl Drop for AppGuard {
  fn drop(&mut self) {
    let _ = invoke_method(&self.app, "Quit", Vec::new());
    unsafe {
      CoFreeUnusedLibraries();
    }
  }
}
