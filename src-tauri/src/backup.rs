use crate::db;
use crate::error::{CleanerError, Result};
use crate::fs as dfs;
use crate::paths::{backups_dir, DataPaths};
use crate::types::{BackupInfo, DirKeySel};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Manifest {
    id: String,
    created_at: i64,
    session_count: u32,
    sessions: Vec<String>,
    directories: Vec<String>,
    size_bytes: u64,
}

pub fn create(
    paths: &DataPaths,
    sessions: &[crate::types::SessionUsage],
    directories: &[DirKeySel],
) -> Result<PathBuf> {
    let id = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let dir = backups_dir().join(&id);
    fs::create_dir_all(&dir)?;

    let mut size_bytes = 0u64;

    // DB: checkpoint + copy (only when touching sessions)
    if !sessions.is_empty() {
        let conn = db::open_readwrite(paths)?;
        db::checkpoint(&conn)?;
        drop(conn);
        let dst = dir.join("opencode.db");
        fs::copy(&paths.db_path, &dst)?;
        size_bytes += dfs::file_size(&dst);
        // also salvage the affected session diffs
        let diff_dst = dir.join("session_diff");
        fs::create_dir_all(&diff_dst)?;
        for s in sessions {
            let src = paths.session_diff_path(&s.id);
            if src.exists() {
                let dst = diff_dst.join(format!("{}.json", s.id));
                let _ = fs::copy(&src, &dst);
                size_bytes += dfs::file_size(&dst);
            }
        }
    }

    // directories -> tar.gz archives
    for key in directories {
        let src = dir_for_key(paths, *key);
        if !src.exists() {
            continue;
        }
        let stat = dfs::dir_stat(&src);
        if stat.size_bytes == 0 {
            continue;
        }
        let dst = dir.join(format!("{}.tar.gz", dir_key_name(*key)));
        match dfs::archive_dir(&src, &dst) {
            Ok(sz) => size_bytes += sz,
            Err(e) => return Err(CleanerError::BackupFailed(e.to_string())),
        }
    }

    let manifest = Manifest {
        id: id.clone(),
        created_at: chrono::Utc::now().timestamp_millis(),
        session_count: sessions.len() as u32,
        sessions: sessions.iter().map(|s| s.id.clone()).collect(),
        directories: directories.iter().map(|k| dir_key_name(*k).to_string()).collect(),
        size_bytes,
    };
    let manifest_path = dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| CleanerError::BackupFailed(e.to_string()))?,
    )?;

    Ok(dir)
}

pub fn list() -> Result<Vec<BackupInfo>> {
    let root = backups_dir();
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(&root)?.flatten() {
        if !entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            continue;
        }
        let p = entry.path();
        let manifest_path = p.join("manifest.json");
        let info = match fs::read_to_string(&manifest_path) {
            Ok(content) => match serde_json::from_str::<Manifest>(&content) {
                Ok(m) => BackupInfo {
                    id: m.id,
                    created_at: m.created_at,
                    size_bytes: m.size_bytes,
                    session_count: m.session_count,
                    label: format!(
                        "{} sessions, {} dirs",
                        m.session_count,
                        m.directories.len()
                    ),
                },
                Err(_) => fallback_info(&p),
            },
            Err(_) => fallback_info(&p),
        };
        out.push(info);
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

fn fallback_info(p: &Path) -> BackupInfo {
    let id = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let size = dfs::dir_stat(p).size_bytes;
    BackupInfo {
        id,
        created_at: 0,
        size_bytes: size,
        session_count: 0,
        label: "(unmanifested backup)".into(),
    }
}

pub fn delete(id: &str) -> Result<()> {
    let p = backups_dir().join(id);
    if p.exists() {
        fs::remove_dir_all(&p)?;
    }
    Ok(())
}

/// Prune old backups keeping the most recent `retention` count.
pub fn prune_old(retention: u32) -> Result<()> {
    let mut infos = list()?;
    let keep = retention as usize;
    if infos.len() <= keep {
        return Ok(());
    }
    infos.truncate(keep.min(infos.len()));
    // infos already sorted desc by created_at; the dropped ones are older
    // re-list and remove those not in keep set
    let all = list()?;
    let keep_ids: std::collections::HashSet<String> =
        all.iter().take(keep).map(|b| b.id.clone()).collect();
    for b in all.iter().skip(keep) {
        let _ = delete(&b.id);
    }
    let _ = keep_ids;
    Ok(())
}

pub fn restore(paths: &DataPaths, id: &str) -> Result<()> {
    let src_dir = backups_dir().join(id);
    if !src_dir.exists() {
        return Err(CleanerError::RestoreFailed(format!(
            "backup {id} not found"
        )));
    }

    // DB restore requires opencode not running
    let db_backup = src_dir.join("opencode.db");
    if db_backup.exists() {
        let st = crate::process::check();
        if st.running {
            return Err(CleanerError::ProcessRunning(st.processes));
        }
        fs::copy(&db_backup, &paths.db_path)?;
        // remove stale wal/shm
        for ext in ["-wal", "-shm"] {
            let p = format!("{}{ext}", paths.db_path.to_string_lossy());
            let _ = fs::remove_file(&p);
        }
    }

    // restore session diffs
    let diff_dir = src_dir.join("session_diff");
    if diff_dir.exists() {
        fs::create_dir_all(paths.session_diff_dir())?;
        for entry in fs::read_dir(&diff_dir)?.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                let dst = paths.session_diff_dir().join(name);
                let _ = fs::copy(entry.path(), dst);
            }
        }
    }

    // restore directory archives
    for entry in fs::read_dir(&src_dir)?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(stripped) = name.strip_suffix(".tar.gz") {
            if let Some(key) = parse_dir_key_name(stripped) {
                let target = dir_for_key(paths, key);
                extract_tar(&entry.path(), &target)?;
            }
        }
    }

    Ok(())
}

fn extract_tar(archive: &Path, dst: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(dec);
    let _ = std::fs::create_dir_all(dst);
    tar.unpack(dst)
        .map_err(|e| CleanerError::RestoreFailed(e.to_string()))?;
    Ok(())
}

fn dir_for_key(paths: &DataPaths, key: DirKeySel) -> PathBuf {
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

fn dir_key_name(k: DirKeySel) -> &'static str {
    match k {
        DirKeySel::Log => "log",
        DirKeySel::SessionDiff => "session_diff",
        DirKeySel::Snapshot => "snapshot",
        DirKeySel::ToolOutput => "tool_output",
        DirKeySel::Repos => "repos",
        DirKeySel::CachePackages => "cache_packages",
        DirKeySel::CacheBin => "cache_bin",
        DirKeySel::CacheModels => "cache_models",
    }
}

fn parse_dir_key_name(s: &str) -> Option<DirKeySel> {
    Some(match s {
        "log" => DirKeySel::Log,
        "session_diff" => DirKeySel::SessionDiff,
        "snapshot" => DirKeySel::Snapshot,
        "tool_output" => DirKeySel::ToolOutput,
        "repos" => DirKeySel::Repos,
        "cache_packages" => DirKeySel::CachePackages,
        "cache_bin" => DirKeySel::CacheBin,
        "cache_models" => DirKeySel::CacheModels,
        _ => return None,
    })
}
