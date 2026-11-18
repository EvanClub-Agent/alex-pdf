use serde::Serialize;
use tauri::{
  menu::{Menu, MenuItem, PredefinedMenuItem},
  tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
  AppHandle, Manager, Runtime,
};

use crate::settings::{self, DesktopSettings};
use crate::wps_com::types::Engine;

const MAIN_WINDOW_LABEL: &str = "main";
const AUTOSTART_ARG: &str = "--autostart";
#[cfg(target_os = "windows")]
const AUTOSTART_REG_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const AUTOSTART_VALUE_NAME: &str = "PDFConverterTool";
const TRAY_OPEN_ID: &str = "tray_open";
const TRAY_HIDE_ID: &str = "tray_hide";
const TRAY_QUIT_ID: &str = "tray_quit";

#[derive(Debug, Serialize)]
pub struct DesktopSettingsResponse {
  pub launch_on_startup: bool,
  pub close_to_tray: bool,
  pub silent_start_on_login: bool,
  pub engine: Engine,
}

#[derive(Debug, Serialize)]
pub struct ApiTokenResponse {
  pub enabled: bool,
  pub token: Option<String>,
}

pub fn init_tray<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
  let open_item = MenuItem::with_id(app, TRAY_OPEN_ID, "显示主窗口", true, None::<&str>)?;
  let hide_item = MenuItem::with_id(app, TRAY_HIDE_ID, "隐藏到托盘", true, None::<&str>)?;
  let quit_item = MenuItem::with_id(app, TRAY_QUIT_ID, "退出", true, None::<&str>)?;
  let separator = PredefinedMenuItem::separator(app)?;
  let menu = Menu::with_items(app, &[&open_item, &hide_item, &separator, &quit_item])?;

  let mut tray_builder = TrayIconBuilder::with_id("main-tray")
    .menu(&menu)
    .tooltip("AlexPDF")
    .show_menu_on_left_click(false)
    .on_menu_event(|app, event| match event.id().as_ref() {
      TRAY_OPEN_ID => {
        let _ = show_main_window_impl(app);
      }
      TRAY_HIDE_ID => {
        let _ = hide_main_window_impl(app);
      }
      TRAY_QUIT_ID => exit_application_impl(app),
      _ => {}
    })
    .on_tray_icon_event(|tray, event| {
      if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Down,
        ..
      } = event
      {
        let _ = toggle_main_window_impl(tray.app_handle());
      }
    });

  if let Some(icon) = app.default_window_icon().cloned() {
    tray_builder = tray_builder.icon(icon);
  }

  tray_builder.build(app)?;
  Ok(())
}

pub fn apply_startup_window_behavior<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
  if launched_from_autostart() && settings::read_desktop_settings().silent_start_on_login {
    hide_main_window_impl(app.app_handle())?;
  }
  Ok(())
}

#[tauri::command]
pub fn get_desktop_settings() -> Result<DesktopSettingsResponse, String> {
  build_desktop_settings_response()
}

#[tauri::command]
pub fn set_launch_on_startup(enabled: bool) -> Result<DesktopSettingsResponse, String> {
  set_launch_on_startup_impl(enabled)?;
  build_desktop_settings_response()
}

#[tauri::command]
pub fn set_close_to_tray(enabled: bool) -> Result<DesktopSettingsResponse, String> {
  update_desktop_settings(|settings| settings.close_to_tray = enabled)
}

#[tauri::command]
pub fn set_silent_start_on_login(enabled: bool) -> Result<DesktopSettingsResponse, String> {
  update_desktop_settings(|settings| settings.silent_start_on_login = enabled)
}

#[tauri::command]
pub fn set_engine(engine: Engine) -> Result<DesktopSettingsResponse, String> {
  update_desktop_settings(|settings| settings.engine = engine)
}

#[tauri::command]
pub fn get_api_token() -> Result<ApiTokenResponse, String> {
  let current = settings::read_desktop_settings();
  Ok(ApiTokenResponse {
    enabled: current.api_token.is_some(),
    token: current.api_token,
  })
}

#[tauri::command]
pub fn set_api_token_enabled(enabled: bool) -> Result<ApiTokenResponse, String> {
  let mut current = settings::read_desktop_settings();
  if enabled {
    if current.api_token.is_none() {
      current.api_token = Some(settings::generate_api_token());
    }
  } else {
    current.api_token = None;
  }
  settings::write_desktop_settings(&current).map_err(|err| err.to_string())?;
  Ok(ApiTokenResponse {
    enabled: current.api_token.is_some(),
    token: current.api_token,
  })
}

#[tauri::command]
pub fn regenerate_api_token() -> Result<ApiTokenResponse, String> {
  let mut current = settings::read_desktop_settings();
  current.api_token = Some(settings::generate_api_token());
  settings::write_desktop_settings(&current).map_err(|err| err.to_string())?;
  Ok(ApiTokenResponse {
    enabled: true,
    token: current.api_token,
  })
}

#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
  hide_main_window_impl(&app).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
  show_main_window_impl(&app).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn exit_application(app: AppHandle) {
  exit_application_impl(&app);
}

#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
  reveal_in_explorer_impl(&path)
}

#[cfg(target_os = "windows")]
fn reveal_in_explorer_impl(path: &str) -> Result<(), String> {
  use std::path::Path;
  let target = Path::new(path);
  if !target.exists() {
    return Err(format!("路径不存在: {}", path));
  }
  let result = if target.is_dir() {
    std::process::Command::new("explorer").arg(path).spawn()
  } else {
    std::process::Command::new("explorer")
      .arg(format!("/select,{}", path))
      .spawn()
  };
  result.map(|_| ()).map_err(|err| err.to_string())
}

#[cfg(target_os = "macos")]
fn reveal_in_explorer_impl(path: &str) -> Result<(), String> {
  use std::path::Path;
  let target = Path::new(path);
  if !target.exists() {
    return Err(format!("路径不存在: {}", path));
  }
  let mut cmd = std::process::Command::new("open");
  if target.is_file() {
    cmd.arg("-R");
  }
  cmd.arg(path).spawn().map(|_| ()).map_err(|err| err.to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn reveal_in_explorer_impl(_path: &str) -> Result<(), String> {
  Err("当前平台不支持".to_string())
}

fn build_desktop_settings_response() -> Result<DesktopSettingsResponse, String> {
  let settings = settings::read_desktop_settings();
  Ok(DesktopSettingsResponse {
    launch_on_startup: is_launch_on_startup_enabled()?,
    close_to_tray: settings.close_to_tray,
    silent_start_on_login: settings.silent_start_on_login,
    engine: settings.engine,
  })
}

fn update_desktop_settings<F>(mutator: F) -> Result<DesktopSettingsResponse, String>
where
  F: FnOnce(&mut DesktopSettings),
{
  let mut desktop_settings = settings::read_desktop_settings();
  mutator(&mut desktop_settings);
  settings::write_desktop_settings(&desktop_settings).map_err(|err| err.to_string())?;
  build_desktop_settings_response()
}

fn hide_main_window_impl<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
    let _ = window.unminimize();
    window.set_skip_taskbar(true)?;
    window.hide()?;
  }
  Ok(())
}

fn show_main_window_impl<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
    window.set_skip_taskbar(false)?;
    let _ = window.unminimize();
    window.show()?;
    let _ = window.set_focus();
  }
  Ok(())
}

fn toggle_main_window_impl<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
    if window.is_visible().unwrap_or(false) {
      hide_main_window_impl(app)?;
    } else {
      show_main_window_impl(app)?;
    }
  }
  Ok(())
}

fn exit_application_impl<R: Runtime>(app: &AppHandle<R>) {
  app.exit(0);
}

fn launched_from_autostart() -> bool {
  std::env::args().any(|arg| arg == AUTOSTART_ARG)
}

#[cfg(target_os = "windows")]
fn is_launch_on_startup_enabled() -> Result<bool, String> {
  let output = run_reg(["query", AUTOSTART_REG_KEY, "/v", AUTOSTART_VALUE_NAME])?;
  Ok(output.status.success())
}

#[cfg(not(target_os = "windows"))]
fn is_launch_on_startup_enabled() -> Result<bool, String> {
  Ok(false)
}

#[cfg(target_os = "windows")]
fn set_launch_on_startup_impl(enabled: bool) -> Result<(), String> {
  if enabled {
    let current_exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let command = format!("\"{}\" {}", current_exe.display(), AUTOSTART_ARG);
    let output = run_reg([
      "add",
      AUTOSTART_REG_KEY,
      "/v",
      AUTOSTART_VALUE_NAME,
      "/t",
      "REG_SZ",
      "/d",
      &command,
      "/f",
    ])?;
    if output.status.success() {
      Ok(())
    } else {
      Err(read_command_error(&output))
    }
  } else {
    let output = run_reg([
      "delete",
      AUTOSTART_REG_KEY,
      "/v",
      AUTOSTART_VALUE_NAME,
      "/f",
    ])?;
    if output.status.success() || is_missing_registry_value(&output) {
      Ok(())
    } else {
      Err(read_command_error(&output))
    }
  }
}

#[cfg(not(target_os = "windows"))]
fn set_launch_on_startup_impl(_enabled: bool) -> Result<(), String> {
  Err("开机自启仅支持 Windows".to_string())
}

#[cfg(target_os = "windows")]
fn run_reg<const N: usize>(args: [&str; N]) -> Result<std::process::Output, String> {
  std::process::Command::new("reg")
    .args(args)
    .output()
    .map_err(|err| err.to_string())
}

#[cfg(target_os = "windows")]
fn read_command_error(output: &std::process::Output) -> String {
  let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
  if stderr.is_empty() {
    "系统未返回具体错误信息".to_string()
  } else {
    stderr
  }
}

#[cfg(target_os = "windows")]
fn is_missing_registry_value(output: &std::process::Output) -> bool {
  let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
  stderr.contains("unable to find")
}
