use crate::backup;
use crate::clean;
use crate::error::CleanerError;
use crate::paths::{self, DataPaths};
use crate::scan;
use crate::settings;
use crate::types::{
    AppSettings, BackupInfo, CleanError, CleanPreview, CleanResult, CleanSelection, ProcessStatus,
    ScanReport,
};
use std::sync::Mutex as StdMutex;
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub write_lock: tokio::sync::Mutex<()>,
    pub _marker: StdMutex<()>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            write_lock: tokio::sync::Mutex::new(()),
            _marker: StdMutex::new(()),
        }
    }
}

fn estr(e: CleanerError) -> String {
    e.to_string()
}

type CmdResult<T> = std::result::Result<T, String>;

fn resolve_paths() -> CmdResult<DataPaths> {
    paths::resolve().map_err(estr)
}

#[tauri::command]
pub async fn scan(app: AppHandle) -> CmdResult<ScanReport> {
    let p = resolve_paths()?;
    let report = tokio::task::spawn_blocking(move || {
        scan::scan(&p, |prog| {
            let _ = app.emit(
                "scan-progress",
                serde_json::json!({
                    "dir": prog.label,
                    "current": prog.current,
                    "total": prog.total,
                    "label": prog.label,
                }),
            );
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(estr)?;
    Ok(report)
}

#[tauri::command]
pub async fn dry_run(selection: CleanSelection) -> CmdResult<CleanPreview> {
    let p = resolve_paths()?;
    tokio::task::spawn_blocking(move || clean::dry_run(&p, &selection))
        .await
        .map_err(|e| e.to_string())?
        .map_err(estr)
}

#[tauri::command]
pub async fn execute(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: CleanSelection,
) -> CmdResult<CleanResult> {
    let _guard = state.write_lock.lock().await;
    let p = resolve_paths()?;
    let s = settings::load();
    let inner = tokio::task::spawn_blocking(move || {
        clean::execute(&p, &selection, &s, |prog| {
            let _ = app.emit(
                "clean-progress",
                serde_json::json!({
                    "phase": prog.phase,
                    "current": prog.current,
                    "total": prog.total,
                }),
            );
        })
    })
    .await
    .map_err(|e| e.to_string())?;
    let result = inner.map_err(estr)?;
    if result.freed_bytes == 0 && !result.errors.is_empty() {
        Err(result
            .errors
            .first()
            .map(|e: &CleanError| e.message.clone())
            .unwrap_or_else(|| "unknown error".into()))
    } else {
        Ok(result)
    }
}

#[tauri::command]
pub async fn check_process() -> CmdResult<ProcessStatus> {
    let st = tokio::task::spawn_blocking(crate::process::check)
        .await
        .map_err(|e| e.to_string())?;
    Ok(st)
}

#[tauri::command]
pub async fn get_backups() -> CmdResult<Vec<BackupInfo>> {
    tokio::task::spawn_blocking(backup::list)
        .await
        .map_err(|e| e.to_string())?
        .map_err(estr)
}

#[tauri::command]
pub async fn restore(app: AppHandle, backup_id: String) -> CmdResult<()> {
    let p = resolve_paths()?;
    tokio::task::spawn_blocking(move || crate::backup::restore(&p, &backup_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(estr)?;
    let _ = app.emit("restore-done", ());
    Ok(())
}

#[tauri::command]
pub async fn delete_backup(backup_id: String) -> CmdResult<()> {
    tokio::task::spawn_blocking(move || backup::delete(&backup_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(estr)
}

#[tauri::command]
pub async fn get_settings() -> CmdResult<AppSettings> {
    Ok(tokio::task::spawn_blocking(settings::load)
        .await
        .map_err(|e| e.to_string())?)
}

#[tauri::command]
pub async fn set_settings(settings: AppSettings) -> CmdResult<AppSettings> {
    tokio::task::spawn_blocking(move || settings::save(&settings))
        .await
        .map_err(|e| e.to_string())?
        .map_err(estr)
}

#[tauri::command]
pub async fn get_data_root() -> CmdResult<String> {
    let p = resolve_paths()?;
    Ok(p.data_root.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn export_diagnostics(_app: AppHandle) -> CmdResult<String> {
    let app_dir = crate::paths::app_data_dir();
    let logs = crate::paths::app_logs_dir();
    let zip_path = app_dir.join("diagnostics.zip");
    let file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut tar = tar::Builder::new(enc);
    let _ = tar.append_dir_all("logs", &logs);
    let _ = tar.finish();
    Ok(zip_path.to_string_lossy().to_string())
}
