pub mod backup;
pub mod clean;
pub mod commands;
pub mod db;
pub mod error;
pub mod fs;
pub mod migrate;
pub mod paths;
pub mod process;
pub mod scan;
pub mod settings;
pub mod types;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()
        .ok();

    if let Err(e) = paths::ensure_app_dirs() {
        log::warn!("ensure_app_dirs: {e}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::dry_run,
            commands::execute,
            commands::check_process,
            commands::get_backups,
            commands::restore,
            commands::delete_backup,
            commands::get_settings,
            commands::set_settings,
            commands::get_data_root,
            commands::storage_info,
            commands::migrate_preflight,
            commands::migrate_execute,
            commands::migrate_undo,
            commands::migrate_discard_backup,
            commands::export_diagnostics,
        ])
        .setup(|_app| {
            #[cfg(debug_assertions)]
            {
                let devtools = _app.get_webview_window("main").expect("no main window");
                devtools.open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running opencode-cleaner");
}
