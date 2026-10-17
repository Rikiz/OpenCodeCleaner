use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub struct DirStat {
    pub size_bytes: u64,
    pub file_count: u64,
}

pub fn dir_stat(path: &Path) -> DirStat {
    if !path.exists() {
        return DirStat {
            size_bytes: 0,
            file_count: 0,
        };
    }
    let mut size = 0u64;
    let mut count = 0u64;
    walk(path, &mut |entry| {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                size += meta.len();
                count += 1;
            }
        }
    });
    DirStat {
        size_bytes: size,
        file_count: count,
    }
}

fn walk<F: FnMut(&PathBuf)>(path: &Path, f: &mut F) {
    let Ok(rd) = fs::read_dir(path) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                walk(&p, f);
            } else {
                f(&p);
            }
        } else {
            f(&p);
        }
    }
}

pub fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Recursive delete that tolerates individual locked entries and reports what
/// it actually removed.
///
/// `fs::remove_dir_all` aborts the whole subtree on the first failure, which on
/// Windows means one file held open by a running process leaves *everything*
/// behind (on Unix the rest still goes). We therefore remove files first
/// (one by one) and then directories deepest-first, collecting failures rather
/// than propagating them.
///
/// Returns `(bytes actually freed, paths that could not be removed)`.
pub fn remove_dir_all_counted(path: &Path) -> (u64, Vec<String>) {
    let mut freed = 0u64;
    let mut failed = Vec::new();
    if !path.exists() {
        return (freed, failed);
    }
    let mut dirs = Vec::new();
    collect_removal_plan(path, &mut dirs);
    // dirs is pre-order (parents before children); iterate reversed so children
    // are removed before their parents.
    for p in dirs.iter().rev() {
        let Ok(meta) = fs::symlink_metadata(p) else {
            continue;
        };
        if meta.is_dir() {
            if let Err(e) = fs::remove_dir(p) {
                // A non-empty directory here means some child was locked.
                failed.push(format!("{} ({e})", p.display()));
            }
        } else {
            // Everything that is not a real directory — regular files, symlinks
            // (cache/bin entries often are), fifos — is unlinked individually.
            match fs::remove_file(p) {
                Ok(()) => freed += meta.len(),
                Err(e) => failed.push(format!("{} ({e})", p.display())),
            }
        }
    }
    (freed, failed)
}

fn collect_removal_plan(dir: &Path, out: &mut Vec<PathBuf>) {
    out.push(dir.to_path_buf());
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    children.sort();
    for child in children {
        // symlink_metadata: never follow links. A symlinked directory must be
        // unlinked itself, not descended into, or we would delete files outside
        // the tree the user selected.
        let Ok(meta) = fs::symlink_metadata(&child) else {
            continue;
        };
        if meta.is_dir() {
            collect_removal_plan(&child, out);
        } else {
            out.push(child);
        }
    }
}

/// Remove files matching a glob-style prefix+"suffix" within `dir`.
/// For session_diff: pattern = "ses_*.json". We split on '*'.
pub fn remove_glob(dir: &Path, pattern: &str) -> Result<u64> {
    if !dir.exists() {
        return Ok(0);
    }
    let (prefix, suffix) = match pattern.split_once('*') {
        Some((p, s)) => (p, s),
        None => (pattern, ""),
    };
    let mut removed = 0u64;
    let Ok(rd) = fs::read_dir(dir) else {
        return Ok(0);
    };
    for entry in rd.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) && name.ends_with(suffix) && name.len() >= prefix.len() + suffix.len()
            && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
    }
    Ok(removed)
}

pub fn archive_dir(src: &Path, dst: &Path) -> Result<u64> {
    let file = fs::File::create(dst)?;
    let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(".", src)?;
    tar.finish()?;
    drop(tar);
    Ok(file_size(dst))
}

pub fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    fs::copy(src, dst)?;
    Ok(())
}

pub fn list_subdirs(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(path) {
        for entry in rd.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    out.push(entry.path());
                }
            }
        }
    }
    out
}
