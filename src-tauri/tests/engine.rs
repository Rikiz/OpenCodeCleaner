// Integration tests for the core engine (CONTRACTS §9).
// Run: cargo test --test '*' -- --test-threads=1
//
// These tests build a synthetic opencode data directory in a unique temp
// location and exercise scan / dry_run / delete / execute against it.
// A global mutex serializes tests because paths::resolve() & app_data_dir()
// consult the process-wide XDG_DATA_HOME env var.

use opencode_cleaner_lib as lib;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use lib::db;
use lib::clean;
use lib::fs as dfs;
use lib::paths::DataPaths;
use lib::scan;
use lib::types::{CleanSelection, DirKeySel};

/// Unique scratch dir for tests that don't need the opencode fixture.
fn scratch(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!(
        "oc-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

static SERIAL: Mutex<()> = Mutex::new(());

struct Fixture {
    _guard: std::sync::MutexGuard<'static, ()>,
    tmp: PathBuf,
    saved_data: Option<std::ffi::OsString>,
    saved_cache: Option<std::ffi::OsString>,
    saved_own: Option<std::ffi::OsString>,
    paths: DataPaths,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // restore env
        match &self.saved_data {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match &self.saved_cache {
            Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }
        match &self.saved_own {
            Some(v) => std::env::set_var("OPENCODE_CLEANER_HOME", v),
            None => std::env::remove_var("OPENCODE_CLEANER_HOME"),
        }
        let _ = fs::remove_dir_all(&self.tmp);
    }
}

fn setup() -> Fixture {
    let guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    // skip process detection (the test runner may itself look like opencode)
    std::env::set_var("OPENCODE_CLEANER_SKIP_PROC_CHECK", "1");
    let id = format!("oc-test-{}", std::process::id());
    // unique per call via atomic counter
    use std::sync::atomic::{AtomicU64, Ordering};
    static C: AtomicU64 = AtomicU64::new(0);
    let n = C.fetch_add(1, Ordering::SeqCst);
    let tmp = std::env::temp_dir().join(format!("{id}-{n}"));
    fs::create_dir_all(&tmp).unwrap();

    let data_root = tmp.join("data").join("opencode");
    let cache_root = tmp.join("cache").join("opencode");
    let db_path = data_root.join("opencode.db");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&cache_root).unwrap();

    // seed DB
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let seed = include_str!("fixtures/seed.sql");
    conn.execute_batch(seed).unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
    drop(conn);

    // create some directory contents
    let log_dir = data_root.join("log");
    fs::create_dir_all(&log_dir).unwrap();
    fs::write(log_dir.join("2026.log"), "x".repeat(5000)).unwrap();
    let diff_dir = data_root.join("storage").join("session_diff");
    fs::create_dir_all(&diff_dir).unwrap();
    fs::write(diff_dir.join("ses_a.json"), "{\"d\":1}").unwrap();
    fs::write(diff_dir.join("ses_b.json"), "{\"d\":2}").unwrap();
    let snap_dir = data_root.join("snapshot").join("prj_1").join("hash1");
    fs::create_dir_all(&snap_dir).unwrap();
    fs::write(snap_dir.join("HEAD"), "ref").unwrap();
    fs::create_dir_all(cache_root.join("packages")).unwrap();
    fs::write(cache_root.join("packages").join("model.bin"), "y".repeat(2000)).unwrap();

    // point env at temp so app_data_dir() (cleaner backups) stays in temp too
    let saved_data = std::env::var_os("XDG_DATA_HOME");
    let saved_cache = std::env::var_os("XDG_CACHE_HOME");
    let saved_own = std::env::var_os("OPENCODE_CLEANER_HOME");
    std::env::set_var("XDG_DATA_HOME", tmp.join("data"));
    std::env::set_var("XDG_CACHE_HOME", tmp.join("cache"));
    // On Windows the cleaner's own dir is %LOCALAPPDATA%\opencode-cleaner
    // (ADR-010), which XDG_DATA_HOME no longer covers — redirect it too so the
    // suite never writes into the real user profile.
    std::env::set_var("OPENCODE_CLEANER_HOME", tmp.join("own"));

    Fixture {
        _guard: guard,
        tmp,
        saved_data,
        saved_cache,
        saved_own,
        paths: DataPaths {
            data_root: data_root.clone(),
            cache_root: cache_root.clone(),
            db_path: db_path.clone(),
        },
    }
}

fn conn(paths: &DataPaths) -> rusqlite::Connection {
    let c = rusqlite::Connection::open(&paths.db_path).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c
}

#[test]
fn t1_scan_aggregates_and_sorts() {
    let f = setup();
    let report = scan::scan(&f.paths, |_| ()).unwrap();
    // sessions sorted desc by size: ses_a (largest) first
    assert_eq!(report.sessions.len(), 3);
    assert_eq!(report.sessions[0].id, "ses_a");
    assert!(report.sessions[0].size_bytes > report.sessions[1].size_bytes);
    // total = dir total + db bytes >= session total
    assert!(report.total_bytes > 0);
    // model_id parsed for ses_a and ses_b, None for ses_c
    let a = report.sessions.iter().find(|s| s.id == "ses_a").unwrap();
    assert_eq!(a.model_id.as_deref(), Some("anthropic/claude"));
    let c = report.sessions.iter().find(|s| s.id == "ses_c").unwrap();
    assert!(c.model_id.is_none());
    // has_diff flag
    assert!(a.has_diff);
}

#[test]
fn t2_dry_run_empty_returns_zero() {
    let f = setup();
    let sel = CleanSelection {
        session_ids: vec![],
        keep_recent: None,
        older_than_days: None,
        min_size_mb: None,
        project_ids: vec![],
        directories: vec![],
    };
    let pv = clean::dry_run(&f.paths, &sel).unwrap();
    assert_eq!(pv.total_freed_bytes, 0);
    assert_eq!(pv.affected_session_count, 0);
}

#[test]
fn t3_dry_run_session_precise_bytes() {
    let f = setup();
    let sel = CleanSelection {
        session_ids: vec!["ses_a".into()],
        keep_recent: None,
        older_than_days: None,
        min_size_mb: None,
        project_ids: vec![],
        directories: vec![],
    };
    let pv = clean::dry_run(&f.paths, &sel).unwrap();
    assert_eq!(pv.affected_session_count, 1);
    // the SessionData item size equals SUM(LENGTH(part.data)) for ses_a
    let c = conn(&f.paths);
    let db_size: i64 = c
        .query_row("SELECT COALESCE(SUM(LENGTH(data)),0) FROM part WHERE session_id='ses_a'", [], |r| r.get(0))
        .unwrap();
    let item = pv.items.iter().find(|i| matches!(i.category, lib::types::CleanCategory::SessionData)).unwrap();
    assert_eq!(item.size_bytes, db_size as u64);
}

#[test]
fn t13_keep_recent_selects_oldest() {
    let f = setup();
    // keep_recent=1 -> keep most recently active (ses_c, time_updated=13000), select ses_a+ses_b
    let sel = CleanSelection {
        session_ids: vec![],
        keep_recent: Some(1),
        older_than_days: None,
        min_size_mb: None,
        project_ids: vec![],
        directories: vec![],
    };
    let pv = clean::dry_run(&f.paths, &sel).unwrap();
    assert_eq!(pv.affected_session_count, 2);
    let ids: Vec<&str> = pv
        .items
        .iter()
        .filter(|i| matches!(i.category, lib::types::CleanCategory::SessionData))
        .flat_map(|i| i.label.split_whitespace())
        .collect();
    let _ = ids;
    // ensure ses_c is NOT in the affected sessions: re-resolve
    let chosen = clean::resolve_sessions(&f.paths, &sel).unwrap();
    let chosen_ids: Vec<String> = chosen.iter().map(|s| s.id.clone()).collect();
    assert!(chosen_ids.contains(&"ses_a".to_string()));
    assert!(chosen_ids.contains(&"ses_b".to_string()));
    assert!(!chosen_ids.contains(&"ses_c".to_string()));
}

#[test]
fn t5t6_delete_cascades_and_cleans_event_orphans() {
    let f = setup();
    {
        let mut c = conn(&f.paths);
        let tx = c.transaction().unwrap();
        let n = db::delete_sessions(&tx, &["ses_a".into(), "ses_b".into()]).unwrap();
        assert_eq!(n, 2);
        tx.commit().unwrap();
    }
    let c = conn(&f.paths);
    // sessions gone
    let cnt: i64 = c
        .query_row("SELECT COUNT(*) FROM session WHERE id IN ('ses_a','ses_b')", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 0);
    // messages + parts cascade-removed (ses_a/ ses_b)
    let mp: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM part WHERE session_id IN ('ses_a','ses_b')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(mp, 0);
    // event orphans explicitly cleaned
    let evo: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM event_sequence WHERE aggregate_id IN ('ses_a','ses_b')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(evo, 0);
    let ev: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM event WHERE aggregate_id IN ('ses_a','ses_b')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ev, 0);
    // ses_c untouched
    let alive: i64 = c.query_row("SELECT COUNT(*) FROM session WHERE id='ses_c'", [], |r| r.get(0)).unwrap();
    assert_eq!(alive, 1);
}

#[test]
fn t7t8_execute_removes_session_diff_and_vacuum() {
    let f = setup();
    let settings = lib::types::AppSettings::default();
    let sel = CleanSelection {
        session_ids: vec!["ses_a".into()],
        keep_recent: None,
        older_than_days: None,
        min_size_mb: None,
        project_ids: vec![],
        directories: vec![DirKeySel::Log],
    };
    let result = clean::execute(&f.paths, &sel, &settings, |_| ()).unwrap();
    assert!(result.freed_bytes > 0);
    assert!(result.backup_path.is_some());
    // ses_a gone
    let c = conn(&f.paths);
    let alive: i64 = c.query_row("SELECT COUNT(*) FROM session WHERE id='ses_a'", [], |r| r.get(0)).unwrap();
    assert_eq!(alive, 0);
    // session diff file for ses_a removed
    assert!(!f.paths.session_diff_path("ses_a").exists());
    // ses_b diff still present
    assert!(f.paths.session_diff_path("ses_b").exists());
    // log directory recreated empty
    let log_files = fs::read_dir(f.paths.log_dir()).unwrap().count();
    assert_eq!(log_files, 0);
}

#[test]
fn t9_backup_restore_roundtrip() {
    let f = setup();
    let settings = lib::types::AppSettings::default();
    // delete ses_a via execute (creates a backup)
    let sel = CleanSelection {
        session_ids: vec!["ses_a".into()],
        keep_recent: None,
        older_than_days: None,
        min_size_mb: None,
        project_ids: vec![],
        directories: vec![],
    };
    let result = clean::execute(&f.paths, &sel, &settings, |_| ()).unwrap();
    assert!(result.backup_path.is_some());

    let backups = lib::backup::list().unwrap();
    assert!(!backups.is_empty());
    let id = backups[0].id.clone();

    // restore
    lib::backup::restore(&f.paths, &id).unwrap();
    // ses_a back
    let c = conn(&f.paths);
    let alive: i64 = c.query_row("SELECT COUNT(*) FROM session WHERE id='ses_a'", [], |r| r.get(0)).unwrap();
    assert_eq!(alive, 1);
}

// ---------------------------------------------------------------------------
// Counted removal (fs::remove_dir_all_counted) — the primitive that makes
// clean accounting honest on Windows, where one locked file aborts a plain
// recursive delete.
// ---------------------------------------------------------------------------

#[test]
fn t14_counted_removal_reports_exact_bytes() {
    let root = scratch("counted");
    fs::create_dir_all(root.join("log").join("2026")).unwrap();
    fs::create_dir_all(root.join("tool-output")).unwrap();
    fs::write(root.join("log").join("2026").join("a.log"), "a".repeat(1200)).unwrap();
    fs::write(root.join("log").join("2026").join("b.log"), "b".repeat(800)).unwrap();
    fs::write(root.join("log").join("top.log"), "c".repeat(200)).unwrap();
    fs::write(root.join("tool-output").join("t.out"), "d".repeat(300)).unwrap();
    let expected = 1200 + 800 + 200 + 300;

    let (freed, failed) = dfs::remove_dir_all_counted(&root);

    assert!(failed.is_empty(), "unexpected failures: {failed:?}");
    assert_eq!(freed, expected, "must account for exactly the bytes removed");
    assert!(!root.exists(), "tree must be gone");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn t14b_counted_removal_missing_path_is_noop() {
    let root = scratch("counted-missing");
    let gone = root.join("nope");
    let (freed, failed) = dfs::remove_dir_all_counted(&gone);
    assert_eq!(freed, 0);
    assert!(failed.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn t15_counted_removal_reports_partial_failure() {
    use std::os::unix::fs::PermissionsExt;

    // Reproduces the Windows "file held open by a running process" situation:
    // an entry that cannot be unlinked must not abort the rest of the tree,
    // must be reported, and must not be counted as freed.
    let root = scratch("counted-partial");
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("held.bin"), "h".repeat(4096)).unwrap();
    fs::write(root.join("free.bin"), "f".repeat(512)).unwrap();

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();

    // Self-calibrate: root ignores directory permissions, so if the probe
    // deletion succeeds this test proves nothing.
    let probe = root.join("probe");
    fs::create_dir_all(&probe).unwrap();
    fs::write(probe.join("p.bin"), "x".repeat(4096)).unwrap();
    fs::set_permissions(&probe, fs::Permissions::from_mode(0o500)).unwrap();
    let probe_result = dfs::remove_dir_all_counted(&probe);
    if probe_result.1.is_empty() {
        // Running with permission bypass (root): restore and skip assertions.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&probe, fs::Permissions::from_mode(0o700)).unwrap();
        let _ = fs::remove_dir_all(&root);
        return;
    }

    let (freed, failed) = dfs::remove_dir_all_counted(&root);

    assert_eq!(
        freed, 512,
        "only the removable file counts toward freed_bytes"
    );
    assert!(
        !failed.is_empty(),
        "the undeletable entry must be reported, not silently swallowed"
    );
    assert!(
        failed.iter().any(|f| f.contains("held.bin")),
        "failure list must name the blocked file: {failed:?}"
    );
    assert!(
        !root.join("free.bin").exists(),
        "removable siblings must still be deleted"
    );

    // restore permissions so cleanup succeeds
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn t16_execute_reports_freed_bytes_from_actual_deletion() {
    let f = setup();
    let settings = lib::types::AppSettings::default();
    // 5000-byte log file seeded by the fixture
    let log_size = dfs::dir_stat(&f.paths.log_dir()).size_bytes;
    assert!(log_size >= 5000);

    let sel = CleanSelection {
        session_ids: vec![],
        keep_recent: None,
        older_than_days: None,
        min_size_mb: None,
        project_ids: vec![],
        directories: vec![DirKeySel::Log],
    };
    let result = clean::execute(&f.paths, &sel, &settings, |_| ()).unwrap();

    assert!(
        result.errors.is_empty(),
        "no failures expected, got: {:?}",
        result.errors
    );
    assert_eq!(
        result.freed_bytes, log_size,
        "freed_bytes must equal bytes actually deleted, not the pre-scan estimate"
    );
    assert_eq!(dfs::dir_stat(&f.paths.log_dir()).size_bytes, 0);
    // log dir is recreated empty for opencode
    assert!(f.paths.log_dir().exists());
}

#[cfg(unix)]
#[test]
fn t17_counted_removal_unlinks_symlinks_without_following() {
    use std::os::unix::fs::symlink;

    let base = scratch("counted-symlink");
    // Links point at `base/target`, which is OUTSIDE the tree we delete.
    let outside = base.join("target");
    let root = base.join("tree");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keep.txt"), "precious".repeat(100)).unwrap();

    let bin = root.join("cache").join("bin");
    fs::create_dir_all(&bin).unwrap();
    symlink(&outside, bin.join("opencode")).unwrap();
    // a symlinked *directory* must not be descended into
    symlink(&outside, root.join("cache").join("pkgs")).unwrap();
    fs::write(bin.join("real.bin"), "b".repeat(64)).unwrap();

    let (_, failed) = dfs::remove_dir_all_counted(&root);

    assert!(failed.is_empty(), "unexpected failures: {failed:?}");
    assert!(
        outside.join("keep.txt").exists(),
        "must never delete through a symlink"
    );
    assert!(!root.exists());
    let _ = fs::remove_dir_all(&base);
}

// ---------------------------------------------------------------------------
// Relocation (ADR-010). The fixture redirects the roots into a temp dir, so the
// whole relocate → verify → undo cycle runs without touching a real profile.
// Only Windows has junctions; elsewhere the API must degrade gracefully.
// ---------------------------------------------------------------------------

#[test]
fn t18_storage_status_is_reported_on_every_platform() {
    let f = setup();
    let st = lib::migrate::status();
    assert_eq!(st.platform, std::env::consts::OS);
    assert_eq!(st.supported, cfg!(windows));
    assert_eq!(st.data_root, f.paths.data_root.to_string_lossy());
    // Fresh fixture: nothing is relocated and nothing is pending.
    assert!(!st.data_relocated);
    assert!(!st.cache_relocated);
    assert_eq!(st.pending_backup_bytes, 0);
    assert!(st.pending_backup_paths.is_empty());
    assert!(st.data_bytes > 0);
}

#[cfg(not(windows))]
#[test]
fn t19_relocation_api_reports_unsupported_off_windows() {
    let base = scratch("migrate-unsupported");
    let err = lib::migrate::preflight(&base).unwrap_err();
    assert!(err.to_string().contains("Windows"), "got: {err}");
    assert!(lib::migrate::execute(&base, &mut |_| {}).is_err());
    assert!(lib::migrate::undo().is_err());
    let _ = fs::remove_dir_all(&base);
}

#[cfg(windows)]
#[test]
fn t19_relocate_verify_and_undo_roundtrip() {
    let f = setup();
    let target_base = f.tmp.join("other-volume").join("opencode-relocated");

    // preflight on a clean fixture
    let pf = lib::migrate::preflight(&target_base).unwrap();
    assert!(pf.ok, "preflight blocked: {:?}", pf.blockers);
    assert!(pf.junction_supported);

    let report = lib::migrate::execute(&target_base, &mut |_| {}).unwrap();
    assert_eq!(report.items.len(), 2, "data + cache");
    assert!(report.pending_backup_bytes > 0, "old copy must be kept");
    assert_eq!(report.pending_backup_paths.len(), 2);

    // the link path opencode uses must now resolve into the target
    let st = lib::migrate::status();
    assert!(st.data_relocated);
    assert!(st.cache_relocated);
    assert!(
        st.data_real_path
            .starts_with(target_base.to_string_lossy().as_ref()),
        "unexpected real path: {}",
        st.data_real_path
    );
    // no \\?\ verbatim prefix leaking into the UI-facing string
    assert!(!st.data_real_path.starts_with(r"\\?\"));

    // readable *through* the junction, DB intact and openable
    let through = lib::paths::resolve().unwrap();
    let c = conn(&through);
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3, "all seeded sessions must survive the move");

    // undo puts the original back
    let undone = lib::migrate::undo().unwrap();
    assert_eq!(undone.len(), 2);
    let st = lib::migrate::status();
    assert!(!st.data_relocated);
    assert!(!st.cache_relocated);
    assert_eq!(st.pending_backup_bytes, 0);
    let after = lib::paths::resolve().unwrap();
    assert!(after.data_root.join("log").exists());
    let c = conn(&after);
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[cfg(windows)]
#[test]
fn t20_second_migration_is_refused_while_one_is_pending() {
    let f = setup();
    let target_base = f.tmp.join("vol2");
    lib::migrate::execute(&target_base, &mut |_| {}).unwrap();

    let pf = lib::migrate::preflight(&f.tmp.join("vol3")).unwrap();
    assert!(!pf.ok, "must refuse to stack relocations");
    assert!(
        pf.blockers.iter().any(|b| b.contains("撤销")),
        "blockers should point at undo: {:?}",
        pf.blockers
    );

    // and executing anyway is refused too
    assert!(lib::migrate::execute(&f.tmp.join("vol3"), &mut |_| {}).is_err());

    lib::migrate::undo().unwrap();
}

#[cfg(windows)]
#[test]
fn t21_discard_backup_is_separate_from_migration() {
    let f = setup();
    let target_base = f.tmp.join("vol2");
    let report = lib::migrate::execute(&target_base, &mut |_| {}).unwrap();
    assert!(report.pending_backup_bytes > 0);

    // before discarding, the data is reachable through the junction
    assert!(lib::migrate::status().data_relocated);

    let removed = lib::migrate::discard_backup().unwrap();
    assert_eq!(removed.len(), 2);
    let st = lib::migrate::status();
    assert_eq!(st.pending_backup_bytes, 0);
    // the junction keeps working with the old copy gone
    assert!(st.data_relocated);
    assert!(st.data_bytes > 0);

    // nothing left to discard
    assert!(lib::migrate::discard_backup().is_err());
}

#[cfg(windows)]
#[test]
fn t22_missing_cache_root_is_skipped_not_fatal() {
    let f = setup();
    // Fresh install: cache root never created.
    fs::remove_dir_all(&f.paths.cache_root).unwrap();

    let report = lib::migrate::execute(&f.tmp.join("vol2"), &mut |_| {}).unwrap();
    assert_eq!(report.items.len(), 1, "only the data root should migrate");
    assert_eq!(report.items[0].key, "data");
    assert!(
        report.warnings.iter().any(|w| w.contains("缓存")),
        "should warn about the skipped root: {:?}",
        report.warnings
    );
    // data really moved, and the session DB is intact through the junction
    let st = lib::migrate::status();
    assert!(st.data_relocated);
    assert!(!st.cache_relocated);
    let c = conn(&lib::paths::resolve().unwrap());
    let n: i64 = c
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);

    lib::migrate::undo().unwrap();
    assert!(!lib::migrate::status().data_relocated);
}

#[cfg(windows)]
#[test]
fn t23_link_in_data_dir_aborts_copy_before_any_swap() {
    let f = setup();
    // A junction inside the data root. symlink_metadata reports it as neither
    // file nor dir, so copy_tree must refuse rather than silently skip —
    // otherwise verification would later fail with a bare count mismatch.
    let outside = f.tmp.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("big.bin"), "z".repeat(4096)).unwrap();
    let link = f.paths.data_root.join("linked");
    let out = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(out.status.success(), "mklink failed: {out:?}");

    let err = lib::migrate::execute(&f.tmp.join("vol2"), &mut |_| {}).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("链接") || msg.contains("特殊文件"),
        "got: {msg}"
    );
    // nothing was swapped, and the link target was never touched
    let st = lib::migrate::status();
    assert!(!st.data_relocated);
    assert_eq!(st.pending_backup_bytes, 0);
    assert!(outside.join("big.bin").exists());

    let _ = std::fs::remove_dir(&link);
}
