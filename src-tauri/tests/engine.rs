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
use lib::paths::DataPaths;
use lib::scan;
use lib::types::{CleanSelection, DirKeySel};

static SERIAL: Mutex<()> = Mutex::new(());

struct Fixture {
    _guard: std::sync::MutexGuard<'static, ()>,
    tmp: PathBuf,
    saved_data: Option<std::ffi::OsString>,
    saved_cache: Option<std::ffi::OsString>,
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
    std::env::set_var("XDG_DATA_HOME", tmp.join("data"));
    std::env::set_var("XDG_CACHE_HOME", tmp.join("cache"));

    Fixture {
        _guard: guard,
        tmp,
        saved_data,
        saved_cache,
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
