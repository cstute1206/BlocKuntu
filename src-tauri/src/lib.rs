use std::path::PathBuf;

#[path = "../../src/main.rs"]
mod focus_hosts;

fn config_path(value: Option<String>) -> Option<PathBuf> {
    value
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

#[tauri::command]
fn dashboard_json(config_path: Option<String>) -> Result<String, String> {
    let explicit = config_path(config_path);
    focus_hosts::gui_dashboard_json_for_config(explicit.as_deref()).map_err(|err| err.to_string())
}

#[tauri::command]
fn rebuild_hosts(config_path: Option<String>) -> Result<(), String> {
    let explicit = config_path(config_path);
    focus_hosts::gui_rebuild_for_config(explicit.as_deref()).map_err(|err| err.to_string())
}

#[tauri::command]
fn close_current(config_path: Option<String>) -> Result<String, String> {
    let explicit = config_path(config_path);
    focus_hosts::gui_close_current_for_config(explicit.as_deref()).map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            dashboard_json,
            rebuild_hosts,
            close_current
        ])
        .run(tauri::generate_context!())
        .expect("error while running BlocKuntu Tauri application");
}
