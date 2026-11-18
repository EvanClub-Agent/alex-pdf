#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use tracing::error;

use alexpdf::desktop;
use alexpdf::http_server;
use alexpdf::state::AppState;
use alexpdf::watcher;

fn main() {
  tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();

  let app_state = Arc::new(AppState::new());

  tauri::Builder::default()
    .manage(app_state.clone())
    .setup(move |_app| {
      let state = app_state.clone();
      tauri::async_runtime::spawn(async move {
        if let Err(err) = http_server::run(state).await {
          error!("HTTP server error: {}", err);
        }
      });
      let watcher_state = app_state.clone();
      tauri::async_runtime::spawn(async move {
        watcher::run(watcher_state).await;
      });
      desktop::init_tray(_app)?;
      desktop::apply_startup_window_behavior(_app)?;
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      desktop::get_desktop_settings,
      desktop::set_launch_on_startup,
      desktop::set_close_to_tray,
      desktop::set_silent_start_on_login,
      desktop::set_engine,
      desktop::get_api_token,
      desktop::set_api_token_enabled,
      desktop::regenerate_api_token,
      desktop::hide_main_window,
      desktop::show_main_window,
      desktop::exit_application,
      desktop::reveal_in_explorer,
    ])
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_shell::init())
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
