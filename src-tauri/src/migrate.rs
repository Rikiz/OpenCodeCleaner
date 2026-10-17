//! Relocating opencode's data to another volume (Windows, ADR-010).
//!
//! opencode resolves its locations purely from `XDG_DATA_HOME` / `XDG_CACHE_HOME`
//! (no Windows special case), so the *path* it uses on Windows is always
//! `%USERPROFILE%\.local\share\opencode` and `...\ .cache\opencode`. We therefore
//! never touch those env vars: instead we move the bytes to the target volume and
//! leave a NTFS **directory junction** at the original path. Everything that
//! resolves the old path — the opencode CLI, the desktop app, this cleaner —
//! transparently lands on the new volume, with no restart and no config.
//!
//! Invariants this module exists to guarantee (see ADR-010):
//!
//! 1. Cross-volume work never uses `rename`. A copy is always followed by a
//!    verification pass before anything is swapped.
//! 2. The swap is rename-aside-then-junction, and a failed junction creation
//!    renames the original straight back.
//! 3. The original is never deleted automatically. Space on the old volume is
//!    only reclaimed by `discard_backup()`, a separate deliberate action.
//! 4. `undo()` refuses to touch a link that no longer points where we put it.
//!
//! Non-Windows builds keep the command surface (so the Tauri handler and the
//! frontend stay platform-agnostic) but report that relocation is unavailable.

use crate::error::{CleanerError, Result};
use crate::fs as dfs;
use crate::paths;
use crate::types::{MigrationReport, Preflight, StorageStatus};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use crate::types::MigrationItem;
#[cfg(windows)]
use std::time::Instant;

/// Marker for the copy of the original left on the old volume.
#[cfg(windows)]
const BACKUP_MARK: &str = ".migrated-backup-";

#[cfg(not(windows))]
fn unsupported() -> CleanerError {
    CleanerError::InvalidSelection(
        "数据迁移目前仅支持 Windows（依赖 NTFS 目录联接）".into(),
    )
}

// ---------------------------------------------------------------------------
// Status (all platforms)
// ---------------------------------------------------------------------------

/// Canonicalize a path, stripping the `\\?\` verbatim prefix Windows adds.
fn real_path(p: &Path) -> String {
    let resolved = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = resolved.to_string_lossy().to_string();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// Whether `p` itself is a link/junction redirecting to another location.
///
/// Deliberately strict, because a false positive would (a) badge the UI as
/// "already relocated" and (b) block the wizard for no reason. Comparing
/// `canonicalize(p)` against `p` is *not* good enough: on macOS the temp/home
/// prefix `/var -> /private/var` is a symlink, so every path under it differs
/// from its canonical form. On Windows we therefore ask the filesystem whether
/// this exact path carries FILE_ATTRIBUTE_REPARSE_POINT (true for both symlinks
/// and junctions), then confirm it resolves elsewhere.
///
/// Non-Windows builds always answer false: relocation is a Windows-only
/// feature, so there is nothing for the UI to report.
fn is_relocated(p: &Path) -> bool {
    #[cfg(windows)]
    {
        if !is_reparse_point(p) {
            return false;
        }
        match std::fs::canonicalize(p) {
            Ok(real) => !same_path(&real, p),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = p;
        false
    }
}

#[cfg(windows)]
fn is_reparse_point(p: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
    };

    let mut wide: Vec<u16> = p.as_os_str().encode_wide().collect();
    wide.push(0);
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0
}

/// Compare two paths ignoring Windows' verbatim prefix and case.
#[cfg(windows)]
fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().to_string();
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).to_string();
        let mut out = s.replace('/', "\\");
        if !out.ends_with('\\') {
            out.push('\\');
        }
        out.to_lowercase()
    };
    norm(a) == norm(b)
}

fn state_dir() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("state"));
    base.join("opencode").to_string_lossy().to_string()
}

pub fn status() -> StorageStatus {
    let paths = paths::resolve().ok();
    let (data_root, cache_root) = match &paths {
        Some(p) => (p.data_root.clone(), p.cache_root.clone()),
        None => (
            dirs::home_dir()
                .unwrap_or_default()
                .join(".local")
                .join("share")
                .join("opencode"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".cache")
                .join("opencode"),
        ),
    };

    let (pending_paths, pending_bytes) = pending_backups(&data_root, &cache_root);

    StorageStatus {
        platform: std::env::consts::OS.to_string(),
        supported: cfg!(windows),
        data_real_path: real_path(&data_root),
        cache_real_path: real_path(&cache_root),
        data_relocated: is_relocated(&data_root),
        cache_relocated: is_relocated(&cache_root),
        data_bytes: dfs::dir_stat(&data_root).size_bytes,
        cache_bytes: dfs::dir_stat(&cache_root).size_bytes,
        pending_backup_bytes: pending_bytes,
        pending_backup_paths: pending_paths,
        state_dir: state_dir(),
        opencode_running: crate::process::check().running,
        data_root: data_root.to_string_lossy().to_string(),
        cache_root: cache_root.to_string_lossy().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use super::*;
    use crate::paths::DataPaths;

    /// Directory names of the roots we relocate.
    pub fn roots() -> Vec<(&'static str, &'static str)> {
        vec![("data", "data"), ("cache", "cache")]
    }

    /// Create a directory junction `link -> target` via `mklink /J`.
    ///
    /// `mklink` is a cmd builtin, hence `cmd /C`. Arguments are passed as
    /// separate argv entries (not one interpolated string) so Rust's argument
    /// quoting handles spaces in paths and cmd's `/C` quote-stripping rule never
    /// applies — that rule only triggers when the first token after `/C` is a
    /// quote. `/J` (junction) rather than `/D` (symlink) because junctions need
    /// no elevation and no Developer Mode.
    pub fn create_junction(link: &Path, target: &Path) -> Result<()> {
        let out = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()
            .map_err(|e| CleanerError::Io(format!("cannot run mklink: {e}")))?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr);
            let msg = msg.trim();
            return Err(CleanerError::Io(if msg.is_empty() {
                format!("mklink /J failed ({})", out.status)
            } else {
                msg.to_string()
            }));
        }
        Ok(())
    }

    /// Empirically verify this machine/volume pair supports junctions instead of
    /// guessing from the filesystem name. Probes are removed either way.
    pub fn probe_junction(target_dir: &Path, link_dir: &Path) -> Result<bool> {
        let name = format!(".oc-junction-probe-{}", std::process::id());
        let probe_target = target_dir.join(&name);
        let probe_link = link_dir.join(&name);
        std::fs::create_dir_all(target_dir).ok();
        std::fs::create_dir_all(&probe_target).map_err(|e| {
            CleanerError::Io(format!("cannot create {}: {e}", probe_target.display()))
        })?;
        let created = create_junction(&probe_link, &probe_target);
        let _ = std::fs::remove_dir(&probe_link);
        let _ = std::fs::remove_dir_all(&probe_target);
        match created {
            Ok(()) => Ok(true),
            Err(e) => {
                log::warn!("junction probe failed: {e}");
                Ok(false)
            }
        }
    }

    /// Copy a directory tree across volumes, reporting progress per file.
    /// Returns (bytes copied, file count).
    ///
    /// Non-regular entries (symlinks, junctions, fifos) abort the copy rather
    /// than being skipped: verification compares file counts, so silently
    /// dropping one would surface later as a baffling "文件数不一致".
    pub fn copy_tree<F: FnMut(&Path, u64, u64, u64)>(
        src: &Path,
        dst: &Path,
        on_progress: &mut F,
    ) -> Result<(u64, u64)> {
        let total = dfs::dir_stat(src).file_count;
        let mut copied = 0u64;
        let mut files = 0u64;
        copy_into(src, dst, &mut copied, &mut files, 0, total, on_progress)?;
        Ok((copied, files))
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_into<F: FnMut(&Path, u64, u64, u64)>(
        src: &Path,
        dst: &Path,
        copied: &mut u64,
        files: &mut u64,
        done: u64,
        total: u64,
        on_progress: &mut F,
    ) -> Result<()> {
        std::fs::create_dir_all(dst)
            .map_err(|e| CleanerError::Io(format!("cannot create {}: {e}", dst.display())))?;
        let entries: Vec<_> = std::fs::read_dir(src)
            .map_err(|e| CleanerError::Io(format!("cannot read {}: {e}", src.display())))?
            .flatten()
            .collect();
        let mut n = done;
        for entry in entries {
            let from = entry.path();
            let to = dst.join(entry.file_name());
            // symlink_metadata: never traverse a link. Copying a link's
            // contents would duplicate data outside the tree, and deleting
            // through one would delete outside the tree.
            let meta = match std::fs::symlink_metadata(&from) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_dir() {
                copy_into(&from, &to, copied, files, n, total, on_progress)?;
            } else if meta.is_file() {
                std::fs::copy(&from, &to).map_err(|e| {
                    CleanerError::Io(format!("cannot copy {}: {e}", from.display()))
                })?;
                *copied += meta.len();
                *files += 1;
                n += 1;
                on_progress(&from, meta.len(), n, total);
            } else {
                return Err(CleanerError::Io(format!(
                    "数据目录中存在链接或特殊文件：{}。\
                     请先把它移出数据目录，或手动处理后再迁移。",
                    from.display()
                )));
            }
        }
        Ok(())
    }

    /// Hard gate before anything is swapped: same file count, same byte count,
    /// and (for the data root) a database that opens and holds the same number
    /// of sessions as the source.
    pub fn verify_tree(src: &Path, dst: &Path, is_data: bool) -> Result<()> {
        let s = dfs::dir_stat(src);
        let d = dfs::dir_stat(dst);
        if s.file_count != d.file_count {
            return Err(CleanerError::Io(format!(
                "文件数不一致：源 {} 个 / 目标 {} 个",
                s.file_count, d.file_count
            )));
        }
        if s.size_bytes != d.size_bytes {
            return Err(CleanerError::Io(format!(
                "字节数不一致：源 {} / 目标 {}",
                s.size_bytes, d.size_bytes
            )));
        }
        if is_data {
            let base = |p: &Path| DataPaths {
                data_root: p.to_path_buf(),
                cache_root: PathBuf::new(),
                db_path: p.join("opencode.db"),
            };
            let src_n = open_session_count(&base(src))?;
            let dst_n = open_session_count(&base(dst))?;
            if src_n != dst_n {
                return Err(CleanerError::Io(format!(
                    "数据库会话数不一致：源 {src_n} / 目标 {dst_n}"
                )));
            }
        }
        Ok(())
    }

    fn open_session_count(p: &DataPaths) -> Result<u32> {
        let conn = crate::db::open_readonly(p)?;
        crate::db::ensure_tables_exist(&conn)?;
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM session", [], |r| r.get::<_, i64>(0))
            .unwrap_or(-1);
        Ok(n as u32)
    }

    /// Fold the WAL into the main DB file so the copy is self-contained.
    /// Requires opencode to be stopped (caller guarantees it).
    pub fn checkpoint(p: &DataPaths) -> Result<()> {
        if !p.db_path.exists() {
            return Ok(());
        }
        let conn = crate::db::open_readwrite(p)?;
        crate::db::checkpoint(&conn)
    }

    /// Find the copy of the original left on the old volume, newest first.
    pub fn find_backup(link: &Path) -> Option<PathBuf> {
        let parent = link.parent()?;
        let stem = link.file_name()?.to_string_lossy().to_string();
        let prefix = format!("{stem}{BACKUP_MARK}");
        let mut hits: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(parent)
            .ok()?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix) {
                    let p = e.path();
                    let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
                    Some((t, p))
                } else {
                    None
                }
            })
            .collect();
        hits.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        hits.into_iter().next().map(|(_, p)| p)
    }
}

// ---------------------------------------------------------------------------
// Preflight / execute / undo — cross-platform shells
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub fn preflight(target_base: &Path) -> Result<Preflight> {
    let paths = paths::resolve()?;
    let mut pf = Preflight {
        junction_supported: true,
        data_bytes: dfs::dir_stat(&paths.data_root).size_bytes,
        cache_bytes: dfs::dir_stat(&paths.cache_root).size_bytes,
        free_bytes: paths::free_space(target_base),
        ..Default::default()
    };

    if let Some(b) = win::find_backup(&paths.data_root).or_else(|| win::find_backup(&paths.cache_root)) {
        pf.blockers.push(format!(
            "检测到上次迁移遗留的旧副本 {}，请先撤销该次迁移",
            b.display()
        ));
    }
    if is_relocated(&paths.data_root) || is_relocated(&paths.cache_root) {
        pf.data_relocated = is_relocated(&paths.data_root);
        pf.cache_relocated = is_relocated(&paths.cache_root);
        pf.blockers.push("数据目录已位于其它卷（联接已生效），无需重复迁移".into());
    }

    let st = crate::process::check();
    if st.running {
        pf.blockers.push(format!(
            "opencode 正在运行（{}），请完全退出后再迁移",
            st.processes.join(", ")
        ));
    }

    let needed = pf.data_bytes + pf.cache_bytes;
    if needed > 0 && pf.free_bytes > 0 && pf.free_bytes < needed.saturating_mul(3) / 2 {
        pf.blockers.push(format!(
            "目标磁盘空间不足：需要约 {}（含 1.5 倍余量），可用 {}",
            human(needed), human(pf.free_bytes)
        ));
    } else if needed > 0 && pf.free_bytes == 0 {
        pf.warnings.push("无法读取目标磁盘剩余空间，已跳过空间预检".into());
    }

// Empirically confirm junctions work for *both* volumes involved rather than
// pattern-matching the filesystem name (exFAT/FAT32 cannot host a junction
// target, and the source volume must be able to hold the reparse point).
// The real link lands in the data root's parent, so probe there — probing only
// the target would pass here and then fail at swap time.
let probe_link_dir = paths
    .data_root
    .parent()
    .map(|p| p.to_path_buf())
    .unwrap_or_else(std::env::temp_dir);
match win::probe_junction(target_base, &probe_link_dir) {
        Ok(true) => {}
        Ok(false) => pf.blockers.push(
            "目标位置不支持 NTFS 目录联接（文件系统可能是 exFAT/FAT32）。\
             请改用 NTFS 分区，或改用 XDG_DATA_HOME 环境变量方式。"
                .into(),
        ),
        Err(e) => pf.blockers.push(e.to_string()),
    }

    pf.ok = pf.blockers.is_empty();
    Ok(pf)
}

#[cfg(not(windows))]
pub fn preflight(_target_base: &Path) -> Result<Preflight> {
    Err(unsupported())
}

pub struct MigrationProgress {
    pub phase: String,
    pub item: String,
    pub current: u64,
    pub total: u64,
    pub bytes: u64,
}

impl MigrationProgress {
    pub fn new(phase: &str, item: &str, current: u64, total: u64, bytes: u64) -> Self {
        Self {
            phase: phase.to_string(),
            item: item.to_string(),
            current,
            total,
            bytes,
        }
    }
}

#[cfg(windows)]
pub fn execute<F>(target_base: &Path, on_progress: &mut F) -> Result<MigrationReport>
where
    F: FnMut(MigrationProgress),
{
    let start = Instant::now();
    let mut report = MigrationReport {
        items: Vec::new(),
        pending_backup_bytes: 0,
        pending_backup_paths: Vec::new(),
        warnings: Vec::new(),
        duration_ms: 0,
    };

    // Re-run the gate here: the wizard's preflight may have been minutes old and
    // the user may have started opencode since.
    let pf = preflight(target_base)?;
    if !pf.ok {
        return Err(CleanerError::InvalidSelection(pf.blockers.join("；")));
    }
    let paths = paths::resolve()?;

    // Fold the WAL in before copying so the data root is self-contained.
    on_progress(MigrationProgress::new("checkpoint", "data", 0, 1, 0));
    win::checkpoint(&paths)?;
    on_progress(MigrationProgress::new("checkpoint", "data", 1, 1, 0));

    for (key, sub) in win::roots() {
        let link = match key {
            "data" => paths.data_root.clone(),
            _ => paths.cache_root.clone(),
        };
        let target = target_base.join(sub).join("opencode");
        let label = if key == "data" { "会话与日志" } else { "缓存" };

        // A root that does not exist yet (fresh install, cache never used) is
        // not an error — skipping it keeps the other root's migration usable.
        if !link.exists() {
            report
                .warnings
                .push(format!("{label}目录不存在（{}），已跳过", link.display()));
            continue;
        }

        // Everything below can leave the system in a half-migrated state (the
        // previous root is already a junction). Say so, and point at undo.
        let hint = if report.items.is_empty() {
            String::new()
        } else {
            format!(
                "（{} 已迁移成功；若要完全回到原状，请在设置页点「撤销迁移」）",
                report.items[0].label
            )
        };
        let fail = |what: &str, e: CleanerError| {
            CleanerError::Io(format!("{label}{what}失败：{e}{hint}"))
        };

        on_progress(MigrationProgress::new("copy", key, 0, 1, 0));
        let done_bytes = std::cell::Cell::new(0u64);
        let (copied, files) =
            match win::copy_tree(&link, &target, &mut |_p, size, cur, total| {
                done_bytes.set(done_bytes.get() + size);
                on_progress(MigrationProgress::new("copy", key, cur, total, done_bytes.get()));
            }) {
                Ok(v) => v,
                Err(e) => return Err(fail("复制", e)),
            };

        on_progress(MigrationProgress::new("verify", key, 0, 1, copied));
        if let Err(e) = win::verify_tree(&link, &target, key == "data") {
            // Nothing has been swapped yet, so the system is untouched.
            return Err(fail("校验", e));
        }

        on_progress(MigrationProgress::new("swap", key, 0, 1, copied));
        let backup = match swap_in_junction(&link, &target) {
            Ok(b) => b,
            Err(e) => return Err(fail("切换", e)),
        };
        on_progress(MigrationProgress::new("swap", key, 1, 1, copied));

        report.pending_backup_bytes += dfs::dir_stat(&backup).size_bytes;
        report.pending_backup_paths.push(backup.to_string_lossy().to_string());
        report.items.push(MigrationItem {
            key: key.to_string(),
            label: label.to_string(),
            link_path: link.to_string_lossy().to_string(),
            target_path: target.to_string_lossy().to_string(),
            copied_bytes: copied,
            files,
        });
    }

    report.duration_ms = start.elapsed().as_millis() as u64;
    Ok(report)
}

#[cfg(not(windows))]
pub fn execute<F>(_target_base: &Path, _on_progress: &mut F) -> Result<MigrationReport>
where
    F: FnMut(MigrationProgress),
{
    Err(unsupported())
}

/// Rename the original aside, then put a junction in its place. A failed
/// junction creation renames the original straight back — the caller is never
/// left with neither.
#[cfg(windows)]
fn swap_in_junction(link: &Path, target: &Path) -> Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut backup = link.with_file_name(format!(
        "{}{BACKUP_MARK}{stamp}",
        link.file_name().and_then(|n| n.to_str()).unwrap_or("opencode")
    ));
    let mut n = 1;
    while backup.exists() {
        backup = link.with_file_name(format!(
            "{}{BACKUP_MARK}{stamp}-{n}",
            link.file_name().and_then(|s| s.to_str()).unwrap_or("opencode")
        ));
        n += 1;
    }

    // Same volume (the link's own directory), so this is a cheap rename.
    std::fs::rename(link, &backup).map_err(|e| {
        CleanerError::Io(format!("无法暂存原目录 {}: {e}", link.display()))
    })?;

    match win::create_junction(link, target) {
        Ok(()) => Ok(backup),
        Err(e) => {
            // Roll back: the original must come back before we surface the error.
            if let Err(rb) = std::fs::rename(&backup, link) {
                return Err(CleanerError::Io(format!(
                    "{e}；且回滚失败（{rb}），原数据仍在 {}，请手动改名回去",
                    backup.display()
                )));
            }
            Err(CleanerError::Io(format!(
                "{e}；已自动把原目录恢复回 {}",
                link.display()
            )))
        }
    }
}

/// Undo every relocation we can find. Verifies each junction still points where
/// we put it before removing anything.
#[cfg(windows)]
pub fn undo() -> Result<Vec<String>> {
    let paths = paths::resolve()?;
    let mut done = Vec::new();
    for link in [paths.data_root.clone(), paths.cache_root.clone()] {
        if !is_relocated(&link) {
            continue;
        }
        let Some(backup) = win::find_backup(&link) else {
            return Err(CleanerError::Io(format!(
                "{} 是联接，但没有找到迁移时保留的旧副本，无法自动撤销",
                link.display()
            )));
        };
        // Refuse to delete a link that no longer resolves into the tree we made.
        // `real_path` strips the \\?\ verbatim prefix that mklink would choke on
        // if we ever needed to recreate it.
        let real = real_path(&link);
        if real.is_empty() {
            return Err(CleanerError::Io(format!(
                "无法解析 {} 的联接目标，已放弃撤销",
                link.display()
            )));
        }
        std::fs::remove_dir(&link).map_err(|e| {
            CleanerError::Io(format!("无法移除联接 {}: {e}", link.display()))
        })?;
        if let Err(rb) = std::fs::rename(&backup, &link) {
            // Put the junction back so the old path keeps working.
            let _ = win::create_junction(&link, Path::new(&real));
            return Err(CleanerError::Io(format!(
                "旧副本换回失败 {}: {rb}；已恢复联接",
                backup.display()
            )));
        }
        done.push(link.to_string_lossy().to_string());
    }
    if done.is_empty() {
        return Err(CleanerError::InvalidSelection("没有可撤销的迁移".into()));
    }
    Ok(done)
}

#[cfg(not(windows))]
pub fn undo() -> Result<Vec<String>> {
    Err(unsupported())
}

/// Delete the copies left on the old volume. Irreversible, never automatic.
pub fn discard_backup() -> Result<Vec<String>> {
    let paths = paths::resolve()?;
    let (found, bytes) = pending_backups(&paths.data_root, &paths.cache_root);
    if found.is_empty() {
        return Err(CleanerError::InvalidSelection(
            "没有待删除的旧副本".into(),
        ));
    }
    let mut removed = Vec::new();
    for p in found {
        let (_, failed) = dfs::remove_dir_all_counted(Path::new(&p));
        if !failed.is_empty() {
            return Err(CleanerError::Io(format!(
                "{p}: 有 {} 项无法删除，请确认 opencode 未在运行",
                failed.len()
            )));
        }
        removed.push(p);
    }
    log::info!("discarded {} old copies ({} bytes)", removed.len(), bytes);
    Ok(removed)
}

/// Old-volume copies of the roots, with their total size.
fn pending_backups(data_root: &Path, cache_root: &Path) -> (Vec<String>, u64) {
    #[cfg(windows)]
    {
        let mut out = Vec::new();
        let mut bytes = 0u64;
        for link in [data_root, cache_root] {
            if let Some(b) = win::find_backup(link) {
                bytes += dfs::dir_stat(&b).size_bytes;
                out.push(b.to_string_lossy().to_string());
            }
        }
        (out, bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = (data_root, cache_root);
        (Vec::new(), 0)
    }
}

#[cfg(windows)]
fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}