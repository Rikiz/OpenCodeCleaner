use crate::error::{CleanerError, Result};
use crate::paths::DataPaths;
use crate::types::SessionUsage;
use rusqlite::{params_from_iter, Connection, OpenFlags, Transaction};
use std::collections::HashSet;

/// Columns we probe for existence; newer opencode may lack some.
pub struct SchemaInfo {
    pub has_agent: bool,
    pub has_model: bool,
    pub has_cost: bool,
    pub has_directory: bool,
    pub has_time_archived: bool,
}

pub fn schema_info(conn: &Connection) -> Result<SchemaInfo> {
    let cols = columns_of(conn, "session")?;
    let present = |c: &str| cols.contains(c);
    Ok(SchemaInfo {
        has_agent: present("agent"),
        has_model: present("model"),
        has_cost: present("cost"),
        has_directory: present("directory"),
        has_time_archived: present("time_archived"),
    })
}

fn columns_of(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut set = HashSet::new();
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |r| {
        let name: String = r.get(1)?;
        Ok(name)
    })?;
    for r in rows.flatten() {
        set.insert(r);
    }
    Ok(set)
}

pub fn ensure_tables_exist(conn: &Connection) -> Result<()> {
    let required = ["session", "message", "part", "project"];
    for t in required {
        let mut stmt = conn.prepare(&format!(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='{t}'"
        ))?;
        let exists: bool = stmt.exists([])?;
        if !exists {
            return Err(CleanerError::IncompatibleSchema(format!(
                "required table '{t}' missing"
            )));
        }
    }
    Ok(())
}

/// Open read-only for scanning/aggregation.
pub fn open_readonly(paths: &DataPaths) -> Result<Connection> {
    let db = &paths.db_path;
    if !db.exists() {
        return Err(CleanerError::DataDirNotFound(db.to_string_lossy().into()));
    }
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "query_only", 1)?;
    Ok(conn)
}

/// Open read-write for deletion/vacuum. Caller must ensure opencode is not running.
pub fn open_readwrite(paths: &DataPaths) -> Result<Connection> {
    let conn = Connection::open(&paths.db_path)?;
    // HARD REQUIREMENT: cascade deletes only fire with foreign_keys=ON (SQLite default OFF).
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

/// Aggregate per-session usage.
pub fn scan_sessions(conn: &Connection) -> Result<Vec<SessionUsage>> {
    let info = schema_info(conn)?;
    let agent_col = if info.has_agent { "s.agent" } else { "NULL" };
    let model_col = if info.has_model { "s.model" } else { "NULL" };
    let cost_col = if info.has_cost { "s.cost" } else { "0" };
    let dir_col = if info.has_directory { "s.directory" } else { "NULL" };
    let arch_col = if info.has_time_archived {
        "s.time_archived"
    } else {
        "NULL"
    };

    let sql = format!(
        "SELECT s.id, s.title, s.project_id, p.name,
                {dir_col} AS directory,
                {agent_col} AS agent,
                {model_col} AS model,
                s.time_created, s.time_updated,
                COALESCE({cost_col}, 0) AS cost,
                {arch_col} AS archived,
                (SELECT COALESCE(SUM(LENGTH(p2.data)),0) FROM part p2 WHERE p2.session_id = s.id) AS size_bytes,
                (SELECT COUNT(*) FROM part p2 WHERE p2.session_id = s.id) AS part_count,
                (SELECT COUNT(*) FROM message m2 WHERE m2.session_id = s.id) AS message_count
         FROM session s
         LEFT JOIN project p ON p.id = s.project_id
         ORDER BY size_bytes DESC"
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        let model_raw: Option<String> = r.get(6)?;
        let model_id = model_raw
            .as_ref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|v| {
                let id = v.get("id").and_then(|x| x.as_str())?;
                let prov = v.get("providerID").and_then(|x| x.as_str())?;
                Some(format!("{prov}/{id}"))
            });
        let archived_raw: Option<i64> = r.get(10)?;
        let dir_raw: Option<String> = r.get(4)?;
        Ok(SessionUsage {
            id: r.get(0)?,
            title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            project_id: r.get(2)?,
            project_name: r.get(3)?,
            directory: dir_raw,
            agent: r.get(5)?,
            model_id,
            created_at: r.get(7)?,
            last_active: r.get(8)?,
            cost: r.get(9)?,
            archived: archived_raw.is_some(),
            size_bytes: r.get(11)?,
            part_count: r.get(12)?,
            message_count: r.get(13)?,
            has_diff: false, // filled by scan layer
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Total DB file bytes (db + wal + shm).
pub fn db_file_bytes(paths: &DataPaths) -> u64 {
    let mut total = 0u64;
    for ext in ["", "-wal", "-shm"] {
        let p = format!("{}{ext}", paths.db_path.to_string_lossy());
        if let Ok(m) = std::fs::metadata(&p) {
            total += m.len();
        }
    }
    total
}

/// Delete sessions inside a transaction. Returns rows affected on the session table.
pub fn delete_sessions(tx: &Transaction, ids: &[String]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    // 1. session -> cascade message->part, todo, session_share, session_message,
    //    session_input, session_context_epoch
    let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
    let sql = format!(
        "DELETE FROM session WHERE id IN ({})",
        placeholders.join(",")
    );
    let affected = tx.execute(&sql, params_from_iter(ids.iter()))?;
    // 2. event_sequence has NO FK to session; clean explicitly. -> cascade event.
    let sql2 = format!(
        "DELETE FROM event_sequence WHERE aggregate_id IN ({})",
        placeholders.join(",")
    );
    let _ = tx.execute(&sql2, params_from_iter(ids.iter()))?;
    Ok(affected as u64)
}

/// Vacuum (outside transaction, exclusive). Returns estimated reclaimed bytes.
pub fn vacuum(conn: &mut Connection, paths: &DataPaths) -> Result<u64> {
    let before = db_file_bytes(paths);
    let before_free = freelist_pages(conn)? * page_size(conn)?;
    conn.execute_batch("VACUUM")?;
    let after = db_file_bytes(paths);
    let _ = before_free;
    Ok(before.saturating_sub(after))
}

pub fn freelist_pages(conn: &Connection) -> Result<u64> {
    Ok(conn
        .query_row("PRAGMA freelist_count", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64)
}

pub fn page_size(conn: &Connection) -> Result<u64> {
    Ok(conn
        .query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0))
        .unwrap_or(4096) as u64)
}

/// WAL checkpoint before backup so the main db file is current.
pub fn checkpoint(conn: &Connection) -> Result<()> {
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
        let _: i64 = r.get(0)?;
        Ok::<_, rusqlite::Error>(())
    })?;
    Ok(())
}

/// Count sessions (for backup metadata).
pub fn session_count(conn: &Connection) -> Result<u32> {
    ensure_tables_exist(conn)?;
    Ok(conn
        .query_row("SELECT COUNT(*) FROM session", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u32)
}

/// Best-effort opencode version detection from DB migrations or env.
pub fn opencode_version(_conn: &Connection) -> Option<String> {
    // opencode stores version on session.version rows; grab the latest nonzero.
    None
}

/// Detect opencode version by checking the installed CLI via PATH.
pub fn detect_opencode_version() -> Option<String> {
    let out = std::process::Command::new("opencode")
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    Some(s.trim().to_string())
}

/// Probe whether DB is safe-ish to operate: required tables exist.
pub fn check_compatible(paths: &DataPaths) -> Result<()> {
    let conn = open_readonly(paths)?;
    ensure_tables_exist(&conn)
}
