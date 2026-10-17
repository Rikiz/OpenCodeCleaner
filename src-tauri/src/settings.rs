use crate::error::{CleanerError, Result};
use crate::paths::settings_file;
use crate::types::AppSettings;
use std::fs;

pub fn load() -> AppSettings {
    let p = settings_file();
    match fs::read_to_string(&p) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save(settings: &AppSettings) -> Result<AppSettings> {
    let p = settings_file();
    if let Some(parent) = p.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| CleanerError::Io(e.to_string()))?;
    fs::write(&p, json)?;
    Ok(settings.clone())
}
