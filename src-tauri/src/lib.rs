//! Thin Tauri shell: exposes core functionality as commands and forwards core events.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!());
    if let Err(err) = result {
        tracing::error!("failed to run Questkeep: {err}");
        std::process::exit(1);
    }
}
