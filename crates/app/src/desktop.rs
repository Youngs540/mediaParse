//! Tauri wiring for the desktop shell.
//!
//! Kept behind the `desktop` feature so the application layer can be tested
//! on machines without the webkit/GTK development packages.

use tauri::Manager;

use crate::commands::handler;
use crate::state::AppState;

/// Build the Tauri application with managed state and the command surface.
pub fn builder() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let paths = mediaparse_infra::AppPaths::resolve();
            let state = AppState::new(paths)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(handler())
}
