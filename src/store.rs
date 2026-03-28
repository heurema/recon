/// Persistent SQLite store for run history.
///
/// Database location: ~/.local/share/recon/history.db
/// Tables: runs, source_results
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::error::AppError;
use crate::model::Briefing;

type Result<T> = std::result::Result<T, AppError>;

/// Return the path to history.db, creating the directory if necessary.
pub fn db_path() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| AppError::ConfigError("cannot determine data directory".to_string()))?
        .join("recon");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("history.db"))
}

/// Open (or create) the SQLite database and run schema migrations.
pub fn open() -> Result<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path)
        .map_err(|e| AppError::ConfigError(format!("cannot open history.db: {}", e)))?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            generated_at TEXT    NOT NULL,
            duration_ms  INTEGER NOT NULL,
            partial      INTEGER NOT NULL,
            config_path  TEXT    NOT NULL,
            scope        TEXT    NOT NULL
        );
        CREATE TABLE IF NOT EXISTS source_results (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id      INTEGER NOT NULL REFERENCES runs(id),
            source_id   TEXT    NOT NULL,
            section     TEXT    NOT NULL,
            status      TEXT    NOT NULL,
            duration_ms INTEGER NOT NULL,
            data_json   TEXT    NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_source_results_run_id
            ON source_results(run_id);
        CREATE INDEX IF NOT EXISTS idx_source_results_source_id
            ON source_results(source_id);",
    )
    .map_err(|e| AppError::ConfigError(format!("schema migration failed: {}", e)))
}

/// Persist a completed briefing to the database.
/// Returns the inserted run_id.
pub fn save_run(conn: &Connection, briefing: &Briefing) -> Result<i64> {
    conn.execute(
        "INSERT INTO runs (generated_at, duration_ms, partial, config_path, scope)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            briefing.generated_at.to_rfc3339(),
            briefing.duration_ms as i64,
            if briefing.partial { 1 } else { 0 },
            briefing.config.path,
            briefing.config.scope,
        ],
    )
    .map_err(|e| AppError::ConfigError(format!("insert run failed: {}", e)))?;

    let run_id = conn.last_insert_rowid();

    for section in &briefing.sections {
        for result in &section.sources {
            let data_json = serde_json::to_string(&result.data)
                .unwrap_or_else(|_| "null".to_string());
            conn.execute(
                "INSERT INTO source_results (run_id, source_id, section, status, duration_ms, data_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    run_id,
                    result.id,
                    section.id,
                    result.status,
                    result.duration_ms as i64,
                    data_json,
                ],
            )
            .map_err(|e| AppError::ConfigError(format!("insert source_result failed: {}", e)))?;
        }
    }

    Ok(run_id)
}

/// Load previous data for a given source_id, ordered oldest-first.
/// Returns a list of (status, data_json, generated_at) tuples.
pub fn load_source_history(
    conn: &Connection,
    source_id: &str,
) -> Result<Vec<(String, String, DateTime<Utc>)>> {
    let mut stmt = conn
        .prepare(
            "SELECT sr.status, sr.data_json, r.generated_at
             FROM source_results sr
             JOIN runs r ON r.id = sr.run_id
             WHERE sr.source_id = ?1
             ORDER BY r.generated_at ASC",
        )
        .map_err(|e| AppError::ConfigError(format!("prepare query failed: {}", e)))?;

    let rows = stmt
        .query_map(params![source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| AppError::ConfigError(format!("query failed: {}", e)))?;

    let mut out = Vec::new();
    for row in rows {
        let (status, data_json, ts_str) =
            row.map_err(|e| AppError::ConfigError(format!("row error: {}", e)))?;
        let ts = ts_str
            .parse::<DateTime<Utc>>()
            .unwrap_or(DateTime::UNIX_EPOCH);
        out.push((status, data_json, ts));
    }
    Ok(out)
}

/// Return the data_json of the most-recent entry for a source_id, if any.
pub fn latest_source_data(conn: &Connection, source_id: &str) -> Result<Option<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT sr.data_json
             FROM source_results sr
             JOIN runs r ON r.id = sr.run_id
             WHERE sr.source_id = ?1
             ORDER BY r.generated_at DESC
             LIMIT 1",
        )
        .map_err(|e| AppError::ConfigError(format!("prepare query failed: {}", e)))?;

    let mut rows = stmt
        .query_map(params![source_id], |row| row.get::<_, String>(0))
        .map_err(|e| AppError::ConfigError(format!("query failed: {}", e)))?;

    if let Some(row) = rows.next() {
        let data =
            row.map_err(|e| AppError::ConfigError(format!("row error: {}", e)))?;
        Ok(Some(data))
    } else {
        Ok(None)
    }
}

/// Return the most-recent cached entry for (source_id) that is still within TTL.
/// Returns (status, data_json, cached_at) if valid cache hit, else None.
pub fn cached_result(
    conn: &Connection,
    source_id: &str,
    ttl_sec: u64,
) -> Result<Option<(String, String, DateTime<Utc>)>> {
    if ttl_sec == 0 {
        return Ok(None);
    }

    let mut stmt = conn
        .prepare(
            "SELECT sr.status, sr.data_json, r.generated_at
             FROM source_results sr
             JOIN runs r ON r.id = sr.run_id
             WHERE sr.source_id = ?1
               AND sr.status NOT IN ('error', 'timed_out')
             ORDER BY r.generated_at DESC
             LIMIT 1",
        )
        .map_err(|e| AppError::ConfigError(format!("prepare cached_result failed: {}", e)))?;

    let mut rows = stmt
        .query_map(params![source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| AppError::ConfigError(format!("query cached_result failed: {}", e)))?;

    if let Some(row) = rows.next() {
        let (status, data_json, ts_str) =
            row.map_err(|e| AppError::ConfigError(format!("row error: {}", e)))?;
        let ts = ts_str
            .parse::<DateTime<Utc>>()
            .unwrap_or(DateTime::UNIX_EPOCH);
        let age_sec = (Utc::now() - ts).num_seconds().max(0) as u64;
        if age_sec <= ttl_sec {
            return Ok(Some((status, data_json, ts)));
        }
    }
    Ok(None)
}

/// Count the number of consecutive runs for source_id that have identical data_json.
/// Used for stalled detection (5+ identical runs).
pub fn count_identical_tail(conn: &Connection, source_id: &str) -> Result<usize> {
    let history = load_source_history(conn, source_id)?;
    if history.is_empty() {
        return Ok(0);
    }

    // Walk from newest → oldest, count consecutive identical data
    let newest_data = &history.last().unwrap().1;
    let mut count = 0;
    for (_status, data, _ts) in history.iter().rev() {
        if data == newest_data {
            count += 1;
        } else {
            break;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Briefing, BriefingConfig, BriefingSummary, Section, SourceResult};
    use chrono::Utc;

    fn in_memory_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        migrate(&conn).expect("migrate");
        conn
    }

    fn make_briefing(source_id: &str, status: &str, data: serde_json::Value) -> Briefing {
        Briefing {
            schema_version: "0.1".to_string(),
            generated_at: Utc::now(),
            duration_ms: 10,
            partial: false,
            config: BriefingConfig {
                path: "/tmp/test.toml".to_string(),
                scope: "explicit".to_string(),
            },
            summary: BriefingSummary {
                sources_total: 1,
                sources_ok: 1,
                sources_failed: 0,
                sources_timed_out: 0,
            },
            sections: vec![Section {
                id: "code".to_string(),
                title: "Code".to_string(),
                sources: vec![SourceResult::new(
                    source_id.to_string(),
                    "shell",
                    "text/plain".to_string(),
                    status,
                    5,
                    data,
                    None,
                )],
            }],
            diff_mode: None,
            baseline_at: None,
        }
    }

    #[test]
    fn test_migrate_idempotent() {
        let conn = in_memory_conn();
        // second migration should not fail
        migrate(&conn).expect("second migrate");
    }

    #[test]
    fn test_save_and_query_run() {
        let conn = in_memory_conn();
        let b = make_briefing("s1", "ok", serde_json::json!("hello"));
        let run_id = save_run(&conn, &b).expect("save_run");
        assert!(run_id > 0);

        let history = load_source_history(&conn, "s1").expect("load_source_history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].0, "ok");
        assert_eq!(history[0].1, "\"hello\"");
    }

    #[test]
    fn test_latest_source_data() {
        let conn = in_memory_conn();
        let b1 = make_briefing("src", "ok", serde_json::json!("first"));
        save_run(&conn, &b1).expect("save 1");
        let b2 = make_briefing("src", "ok", serde_json::json!("second"));
        save_run(&conn, &b2).expect("save 2");

        let data = latest_source_data(&conn, "src").expect("latest").unwrap();
        assert_eq!(data, "\"second\"");
    }

    #[test]
    fn test_cached_result_no_ttl() {
        let conn = in_memory_conn();
        let b = make_briefing("src", "ok", serde_json::json!("x"));
        save_run(&conn, &b).expect("save");
        // ttl=0 → always miss
        let cached = cached_result(&conn, "src", 0).expect("cached_result");
        assert!(cached.is_none());
    }

    #[test]
    fn test_cached_result_within_ttl() {
        let conn = in_memory_conn();
        let b = make_briefing("src", "ok", serde_json::json!("data"));
        save_run(&conn, &b).expect("save");
        // ttl=3600 → should hit (just inserted)
        let cached = cached_result(&conn, "src", 3600).expect("cached_result");
        assert!(cached.is_some());
    }

    #[test]
    fn test_count_identical_tail() {
        let conn = in_memory_conn();
        // 3 identical runs
        for _ in 0..3 {
            let b = make_briefing("src", "ok", serde_json::json!({"x": 1}));
            save_run(&conn, &b).expect("save");
        }
        let count = count_identical_tail(&conn, "src").expect("count");
        assert_eq!(count, 3);
    }

    #[test]
    fn test_count_identical_tail_resets_on_change() {
        let conn = in_memory_conn();
        let b1 = make_briefing("src", "ok", serde_json::json!({"x": 1}));
        save_run(&conn, &b1).expect("save 1");
        let b2 = make_briefing("src", "ok", serde_json::json!({"x": 2}));
        save_run(&conn, &b2).expect("save 2");
        let b3 = make_briefing("src", "ok", serde_json::json!({"x": 2}));
        save_run(&conn, &b3).expect("save 3");

        let count = count_identical_tail(&conn, "src").expect("count");
        assert_eq!(count, 2); // only the last two are identical
    }
}
