use crate::db;
use crate::error::{CleanerError, Result};
use crate::fs as dfs;
use crate::paths::DataPaths;
use crate::types::{DirKey, DirUsage, ScanReport, SessionUsage};
use std::path::PathBuf;

pub struct Progress {
    pub current: usize,
    pub total: usize,
    pub label: String,
}

pub fn scan<F>(paths: &DataPaths, mut on_progress: F) -> Result<ScanReport>
where
    F: FnMut(Progress),
{
    // === directory scan ===
    let dir_entries: Vec<(&str, DirKey, PathBuf, bool)> = vec![
        ("Logs", DirKey::Log, paths.log_dir(), true),
        (
            "Session Diffs",
            DirKey::SessionDiff,
            paths.session_diff_dir(),
            true,
        ),
        ("Snapshots", DirKey::Snapshot, paths.snapshot_dir(), true),
        (
            "Tool Outputs",
            DirKey::ToolOutput,
            paths.tool_output_dir(),
            true,
        ),
        ("Repos", DirKey::Repos, paths.repos_dir(), true),
        (
            "Cache Packages",
            DirKey::CachePackages,
            paths.cache_packages_dir(),
            true,
        ),
        ("Cache Binaries", DirKey::CacheBin, paths.cache_bin_dir(), true),
        (
            "Models Cache",
            DirKey::CacheModels,
            paths.cache_models_file(),
            true,
        ),
    ];
    let total = dir_entries.len() + 1;
    let mut directories = Vec::with_capacity(dir_entries.len());
    for (i, (label, key, path, safe)) in dir_entries.iter().enumerate() {
        on_progress(Progress {
            current: i + 1,
            total,
            label: label.to_string(),
        });
        let stat = dfs::dir_stat(path);
        directories.push(DirUsage {
            key: *key,
            label: label.to_string(),
            path: path.to_string_lossy().to_string(),
            size_bytes: stat.size_bytes,
            file_count: stat.file_count,
            safe_while_running: *safe,
        });
    }

    // === DB scan ===
    on_progress(Progress {
        current: dir_entries.len() + 1,
        total,
        label: "Database".to_string(),
    });
    let opencode_version;
    let mut sessions: Vec<SessionUsage> = match db::open_readonly(paths) {
        Ok(conn) => {
            if let Err(e) = db::ensure_tables_exist(&conn) {
                return Err(e);
            }
            opencode_version = db::detect_opencode_version();
            db::scan_sessions(&conn)?
        }
        Err(CleanerError::DataDirNotFound(_)) => {
            opencode_version = None;
            Vec::new()
        }
        Err(e) => return Err(e),
    };

    // annotate has_diff
    for s in sessions.iter_mut() {
        s.has_diff = paths.session_diff_path(&s.id).exists();
    }

    let session_total_bytes = sessions.iter().map(|s| s.size_bytes).sum();
    let dir_total: u64 = directories.iter().map(|d| d.size_bytes).sum();
    let db_bytes = db::db_file_bytes(paths);
    let total_bytes = dir_total + db_bytes;

    Ok(ScanReport {
        data_root: paths.data_root.to_string_lossy().to_string(),
        cache_root: paths.cache_root.to_string_lossy().to_string(),
        total_bytes,
        total_mb: bytes_to_mb(total_bytes),
        directories,
        sessions,
        session_total_bytes,
        scanned_at: chrono::Utc::now().timestamp_millis(),
        opencode_version,
    })
}

pub fn bytes_to_mb(b: u64) -> f64 {
    (b as f64) / (1024.0 * 1024.0)
}
