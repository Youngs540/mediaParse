// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mediaparse_app::desktop::builder()
        .run(tauri::generate_context!())
        .expect("error while running the mediaParse desktop application");
}
