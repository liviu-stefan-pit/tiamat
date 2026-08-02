pub mod app;
pub mod contracts;
pub mod cursor;
pub mod db;
pub mod intake;
pub mod planner;
pub mod process;
pub mod recovery;
pub mod scheduler;
pub mod security;
pub mod verification;
pub mod workspace;

use app::commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::validate_contract_json,
            commands::orchestrator_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
