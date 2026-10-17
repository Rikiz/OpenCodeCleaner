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
///
/// On Windows this is `%LOCALAPPDATA%\opencode-cleaner` rather than
/// `$XDG_DATA_HOME\opencode-cleaner`: relocating opencode's own data (e.g. onto
/// another volume via a directory junction) must not drag the cleaner's backups
/// and settings along with it. See ADR-010.
///
/// `OPENCODE_CLEANER_HOME` overrides both, so the integration tests can keep
/// fixtures inside their temp dir on every platform.
pub fn app_data_dir() -> PathBuf {
    if let Some(v) = std::env::var_os("OPENCODE_CLEANER_HOME") {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .unwrap_or_else(xdg_data_home)
            .join("opencode-cleaner")
    }
    #[cfg(not(windows))]
    {
        xdg_data_home().join("opencode-cleaner")
    }
}

/// Pre-ADR-010 location, still holding data for Windows users who ran a build
/// from before the move. Both live under `%USERPROFILE%` so the hand-off is a
/// same-volume rename.
#[cfg(windows)]
fn legacy_app_data_dir() -> PathBuf {
    xdg_data_home().join("opencode-cleaner")
}

/// One-time hand-off of the app's own data from the legacy Windows location.
/// Rename when possible (instant, atomic); otherwise copy. Never overwrites an
/// existing new-location dir, never deletes the legacy copy on failure.
#[cfg(windows)]
fn adopt_legacy_app_dir() {
    let legacy = legacy_app_data_dir();
    let new = app_data_dir();
    if !legacy.exists() || new.exists() {
        if legacy.exists() && new.exists() {
            log::warn!(
                "both legacy ({}) and current ({}) app data dirs exist; keeping both",
                legacy.display(),
                new.display()
            );
        }
        return;
    }
    if let Some(parent) = new.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                log::warn!("cannot create {}: {e}", parent.display());
                return;
            }
        }
    }
    match std::fs::rename(&legacy, &new) {
        Ok(()) => log::info!("moved app data {} -> {}", legacy.display(), new.display()),
        Err(e) => {
            // Different volume, or the rename is not permitted: fall back to a
            // copy and keep the source untouched.
            log::warn!("rename {} -> {} failed ({e}); copying instead", legacy.display(), new.display());
            if let Err(e2) = copy_dir_all(&legacy, &new) {
                log::warn!("copy of legacy app data failed: {e2}");
            }
        }
    }
}

#[cfg(windows)]
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
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
    #[cfg(windows)]
    adopt_legacy_app_dir();
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
fn free_space_impl(p: &Path) -> u64 {
    // Per-volume via GetDiskFreeSpaceExW: the previous PowerShell/PSDrive
    // approach ignored `p` entirely and always reported the system drive,
    // which is wrong whenever opencode lives on another volume. Falls back to
    // 0 (caller then skips the non-critical space guard) on any failure.
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut root: Vec<u16> = p.as_os_str().encode_wide().collect();
    root.push(0);
    let mut free_to_caller: u64 = 0;
    // SAFETY: `root` is NUL-terminated and outlives the call; the two null
    // out-params are optional (documented as such) so null pointers are valid.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            root.as_ptr(),
            &mut free_to_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        0
    } else {
        free_to_caller
    }
}
