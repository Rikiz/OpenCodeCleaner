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

pub fn remove_dir_all(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
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
        {
            if fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
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
