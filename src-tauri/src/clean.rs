use crate::backup;
use crate::db;
use crate::error::{CleanerError, Result};
use crate::fs as dfs;
use crate::paths::DataPaths;
use crate::process;
use crate::scan;
use crate::types::*;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Resolve which sessions match the selection (fresh query against DB).
pub fn resolve_sessions(
    paths: &DataPaths,
    sel: &CleanSelection,
) -> Result<Vec<SessionUsage>> {
    let mut sessions = match db::open_readonly(paths) {
        Ok(conn) => db::scan_sessions(&conn)?,
        Err(CleanerError::DataDirNotFound(_)) => Vec::new(),
        Err(e) => return Err(e),
    };

    // filters
    let now_ms = now_ms();
    if let Some(days) = sel.older_than_days {
        let cutoff = now_ms.saturating_sub((days as i64) * 86_400_000);
        sessions.retain(|s| s.last_active < cutoff);
    }
    if let Some(min_mb) = sel.min_size_mb {
        let min_bytes = (min_mb * 1024.0 * 1024.0) as u64;
        sessions.retain(|s| s.size_bytes >= min_bytes);
    }
    if !sel.project_ids.is_empty() {
        sessions.retain(|s| sel.project_ids.iter().any(|p| p == &s.project_id));
    }

    if let Some(n) = sel.keep_recent {
        // keep most recently active n, select the rest
        sessions.sort_by_key(|s| std::cmp::Reverse(s.last_active));
        if (n as usize) < sessions.len() {
            sessions = sessions.split_off(n as usize);
        } else {
            sessions.clear();
        }
        Ok(sessions)
    } else if !sel.session_ids.is_empty() {
        let idset: std::collections::HashSet<&String> = sel.session_ids.iter().collect();
        sessions.retain(|s| idset.contains(&s.id));
        Ok(sessions)
    } else {
        Ok(Vec::new())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn dir_for_key(paths: &DataPaths, key: DirKeySel) -> std::path::PathBuf {
    match key {
        DirKeySel::Log => paths.log_dir(),
        DirKeySel::SessionDiff => paths.session_diff_dir(),
        DirKeySel::Snapshot => paths.snapshot_dir(),
        DirKeySel::ToolOutput => paths.tool_output_dir(),
        DirKeySel::Repos => paths.repos_dir(),
        DirKeySel::CachePackages => paths.cache_packages_dir(),
        DirKeySel::CacheBin => paths.cache_bin_dir(),
        DirKeySel::CacheModels => paths.cache_models_file(),
    }
}

pub fn dry_run(paths: &DataPaths, sel: &CleanSelection) -> Result<CleanPreview> {
    validate(sel)?;
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut backup_bytes = 0u64;

    // sessions
    let sessions = resolve_sessions(paths, sel)?;
    if !sessions.is_empty() {
        let size: u64 = sessions.iter().map(|s| s.size_bytes).sum();
        backup_bytes += size + 1024;
        items.push(CleanPreviewItem {
            category: CleanCategory::SessionData,
            label: format!("{} sessions", sessions.len()),
            size_bytes: size,
            count: sessions.len() as u32,
        });
        // session diff files
        let diff_size: u64 = sessions
            .iter()
            .map(|s| dfs::file_size(&paths.session_diff_path(&s.id)))
            .sum();
        backup_bytes += diff_size;
        if diff_size > 0 {
            items.push(CleanPreviewItem {
                category: CleanCategory::SessionDiff,
                label: "session undo diffs".into(),
                size_bytes: diff_size,
                count: sessions.iter().filter(|s| s.has_diff).count() as u32,
            });
        }
        warnings.push(format!(
            "将删除 {} 个会话及其 undo 数据（不可恢复，已自动备份）",
            sessions.len()
        ));
    }

    // directories
    for key in &sel.directories {
        let path = dir_for_key(paths, *key);
        let stat = dfs::dir_stat(&path);
        backup_bytes += stat.size_bytes;
        items.push(CleanPreviewItem {
            category: dir_category(*key),
            label: key.label().to_string(),
            size_bytes: stat.size_bytes,
            count: stat.file_count as u32,
        });
    }

    let total_freed_bytes: u64 = items.iter().map(|i| i.size_bytes).sum();
    Ok(CleanPreview {
        items,
        total_freed_bytes,
        total_freed_mb: scan::bytes_to_mb(total_freed_bytes),
        affected_session_count: sessions.len() as u32,
        backup_estimated_bytes: backup_bytes,
        warnings,
    })
}

fn dir_category(k: DirKeySel) -> CleanCategory {
    match k {
        DirKeySel::Log => CleanCategory::Log,
        DirKeySel::SessionDiff => CleanCategory::SessionDiff,
        DirKeySel::Snapshot => CleanCategory::Snapshot,
        DirKeySel::ToolOutput => CleanCategory::ToolOutput,
        DirKeySel::Repos => CleanCategory::Repos,
        DirKeySel::CachePackages | DirKeySel::CacheBin | DirKeySel::CacheModels => {
            CleanCategory::Cache
        }
    }
}

fn validate(sel: &CleanSelection) -> Result<()> {
    if sel.keep_recent.is_some() && !sel.session_ids.is_empty() {
        return Err(CleanerError::InvalidSelection(
            "keep_recent 与 session_ids 互斥".into(),
        ));
    }
    Ok(())
}

/// Execute a clean. Emits progress via callback.
pub fn execute<F>(
    paths: &DataPaths,
    sel: &CleanSelection,
    settings: &AppSettings,
    mut on_progress: F,
) -> Result<CleanResult>
where
    F: FnMut(ExecuteProgress),
{
    validate(sel)?;
    let start = Instant::now();
    let mut errors: Vec<CleanError> = Vec::new();

    let sessions = resolve_sessions(paths, sel)?;
    let needs_db = !sessions.is_empty();

    // process guard for DB ops
    if needs_db {
        let st = process::check();
        if st.running {
            return Err(CleanerError::ProcessRunning(st.processes));
        }
    }

    // disk space check
    let needed = estimate_backup_size(paths, sel, &sessions);
    if needed > 0 {
        let avail = crate::paths::free_space(&paths.data_root);
        if avail > 0 && needed > (avail * 3 / 2) {
            return Err(CleanerError::InsufficientSpace {
                needed,
                available: avail,
            });
        }
    }

    // phase: backup
    let mut backup_path = None;
    if (!sessions.is_empty()) || !sel.directories.is_empty() {
        on_progress(ExecuteProgress::phase("backup", 0, 2));
        match backup::create(paths, &sessions, &sel.directories) {
            Ok(p) => {
                backup_path = Some(p.to_string_lossy().to_string());
                backup::prune_old(settings.backup_retention)?;
            }
            Err(e) => {
                return Err(CleanerError::BackupFailed(e.to_string()));
            }
        }
        on_progress(ExecuteProgress::phase("backup", 2, 2));
    }

    // phase: delete DB sessions
    let mut freed = 0u64;
    let mut vacuum_reclaimed = 0u64;
    if needs_db {
        on_progress(ExecuteProgress::phase("deleting", 0, sessions.len()));
        match db::open_readwrite(paths) {
            Ok(mut conn) => {
                let tx = conn.transaction()?;
                let ids: Vec<String> = sessions.iter().map(|s| s.id.clone()).collect();
                db::delete_sessions(&tx, &ids)?;
                tx.commit()?;
                freed += sessions.iter().map(|s| s.size_bytes).sum::<u64>();
                on_progress(ExecuteProgress::phase("deleting", sessions.len(), sessions.len()));

                // remove session diff files
                for s in &sessions {
                    let p = paths.session_diff_path(&s.id);
                    if let Ok(meta) = std::fs::symlink_metadata(&p) {
                        match std::fs::remove_file(&p) {
                            Ok(()) => freed += meta.len(),
                            Err(e) => errors.push(CleanError {
                                item: format!("session_diff/{}", s.id),
                                message: e.to_string(),
                            }),
                        }
                    }
                }

                // vacuum
                on_progress(ExecuteProgress::phase("vacuum", 0, 1));
                vacuum_reclaimed = db::vacuum(&mut conn, paths).unwrap_or(0);
                on_progress(ExecuteProgress::phase("vacuum", 1, 1));
            }
            Err(e) => {
                errors.push(CleanError {
                    item: "database".into(),
                    message: e.to_string(),
                });
            }
        }
    }

    // phase: remove directories
    let dir_count = sel.directories.len();
    for (i, key) in sel.directories.iter().enumerate() {
        on_progress(ExecuteProgress::phase("files", i, dir_count));
        let path = dir_for_key(paths, *key);
        if !path.exists() {
            continue;
        }
        let item = key.label().to_string();
        if *key == DirKeySel::CacheModels {
            let sz = dfs::file_size(&path);
            match std::fs::remove_file(&path) {
                Ok(()) => freed += sz,
                Err(e) => errors.push(CleanError {
                    item,
                    message: e.to_string(),
                }),
            }
        } else {
            let (removed, failed) = dfs::remove_dir_all_counted(&path);
            freed += removed;
            for f in failed {
                errors.push(CleanError {
                    item: item.clone(),
                    message: f,
                });
            }
            // Recreate the directories opencode expects to exist; for
            // session_diff/log/tool-output/repos an empty dir is valid, the
            // rest stay absent until opencode writes into them.
            if *key == DirKeySel::SessionDiff
                || *key == DirKeySel::Log
                || *key == DirKeySel::ToolOutput
                || *key == DirKeySel::Repos
            {
                let _ = std::fs::create_dir_all(&path);
            }
        }
    }
    on_progress(ExecuteProgress::phase("files", dir_count, dir_count));

    Ok(CleanResult {
        freed_bytes: freed,
        freed_mb: scan::bytes_to_mb(freed),
        backup_path,
        errors,
        vacuum_reclaimed_bytes: vacuum_reclaimed,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

pub struct ExecuteProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
}

impl ExecuteProgress {
    pub fn phase(phase: &str, current: usize, total: usize) -> Self {
        Self {
            phase: phase.to_string(),
            current,
            total,
        }
    }
}

fn estimate_backup_size(
    paths: &DataPaths,
    sel: &CleanSelection,
    sessions: &[SessionUsage],
) -> u64 {
    let mut b = 0u64;
    b += sessions.iter().map(|s| s.size_bytes).sum::<u64>();
    b += sessions
        .iter()
        .map(|s| dfs::file_size(&paths.session_diff_path(&s.id)))
        .sum::<u64>();
    for key in &sel.directories {
        b += dfs::dir_stat(&dir_for_key(paths, *key)).size_bytes;
    }
    // DB copy overhead
    if !sessions.is_empty() {
        b += db::db_file_bytes(paths);
    }
    b
}

// helper trait for PathBuf-ish; unused placeholder kept minimal
#[allow(dead_code)]
fn _ensure_path(p: &Path) {
    let _ = p;
}
