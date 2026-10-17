use crate::error::{CleanerError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DataPaths {
    pub data_root: PathBuf,
    pub cache_root: PathBuf,
    pub db_path: PathBuf,
}

impl DataPaths {
    pub fn log_dir(&self) -> PathBuf {
        self.data_root.join("log")
    }
    pub fn snapshot_dir(&self) -> PathBuf {
        self.data_root.join("snapshot")
    }
    pub fn session_diff_dir(&self) -> PathBuf {
        self.data_root.join("storage").join("session_diff")
    }
    pub fn tool_output_dir(&self) -> PathBuf {
        self.data_root.join("tool-output")
    }
    pub fn repos_dir(&self) -> PathBuf {
        self.data_root.join("repos")
    }
    pub fn cache_packages_dir(&self) -> PathBuf {
        self.cache_root.join("packages")
    }
    pub fn cache_bin_dir(&self) -> PathBuf {
        self.cache_root.join("bin")
    }
    pub fn cache_models_file(&self) -> PathBuf {
        self.cache_root.join("models.json")
    }
    pub fn session_diff_path(&self, sid: &str) -> PathBuf {
        self.session_diff_dir().join(format!("{sid}.json"))
    }
    pub fn snapshot_project_dir(&self, project_id: &str) -> PathBuf {
        self.snapshot_dir().join(project_id)
    }
}

fn xdg_data_home() -> PathBuf {
    if let Some(v) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(v)
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("share")
    }
}

fn xdg_cache_home() -> PathBuf {
    if let Some(v) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(v)
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".cache")
    }
}

pub fn resolve() -> Result<DataPaths> {
    let data_root = xdg_data_home().join("opencode");
    let cache_root = xdg_cache_home().join("opencode");
    let db_path = data_root.join("opencode.db");

    if !data_root.exists() {
        return Err(CleanerError::DataDirNotFound(
            data_root.to_string_lossy().to_string(),
        ));
    }
    Ok(DataPaths {
        data_root,
        cache_root,
        db_path,
    })
}

/// App's own data directory for backups, logs, settings.
pub fn app_data_dir() -> PathBuf {
    xdg_data_home().join("opencode-cleaner")
}

pub fn backups_dir() -> PathBuf {
    app_data_dir().join("backups")
}

pub fn app_logs_dir() -> PathBuf {
    app_data_dir().join("logs")
}

pub fn settings_file() -> PathBuf {
    app_data_dir().join("settings.json")
}

pub fn ensure_app_dirs() -> Result<()> {
    for d in [app_data_dir(), backups_dir(), app_logs_dir()] {
        if !d.exists() {
            std::fs::create_dir_all(&d)?;
        }
    }
    Ok(())
}

/// Best-effort free disk space at a path (bytes). Returns 0 on failure (caller
/// then skips the pre-clean disk-space guard, which is non-critical).
pub fn free_space(p: &Path) -> u64 {
    free_space_impl(p)
}

#[cfg(unix)]
fn free_space_impl(p: &Path) -> u64 {
    let out = match std::process::Command::new("df")
        .arg("-k")
        .arg(p)
        .output()
    {
        Ok(o) => o,
        Err(_) => return 0,
    };
    let s = String::from_utf8_lossy(&out.stdout);
    // second line: fs 1K-blocks used avail capacity mountedon
    let line = match s.lines().nth(1) {
        Some(l) => l,
        None => return 0,
    };
    let avail_kb: u64 = line.split_whitespace().nth(3).and_then(|x| x.parse().ok()).unwrap_or(0);
    avail_kb.saturating_mul(1024)
}

#[cfg(windows)]
fn free_space_impl(_p: &Path) -> u64 {
    // Best-effort via PowerShell; non-critical. Returns 0 on failure.
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command",
            &format!("(Get-PSDrive -PSProvider FileSystem | Where-Object {{ $_.DisplayRoot -eq $null }} | Select-Object -First 1).Free")])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().parse().unwrap_or(0),
        Err(_) => 0,
    }
}
