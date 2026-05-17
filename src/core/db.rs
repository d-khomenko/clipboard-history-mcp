use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

const SCHEMA_V1: &str = include_str!("schema_v1.sql");
const SCHEMA_V2: &str = include_str!("schema_v2.sql");
const SCHEMA_V3: &str = include_str!("schema_v3.sql");

pub fn open_db<P: AsRef<Path>>(path: P) -> Result<Connection> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent).context("create data dir")?;
    }
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    let current: i64 = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| {
            r.get::<_, String>(0).map(|s| s.parse().unwrap_or(0))
        })
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(SCHEMA_V1)?;
    }
    if current < 2 {
        conn.execute_batch(SCHEMA_V2)?;
    }
    if current < 3 {
        conn.execute_batch(SCHEMA_V3)?;
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_creates_tables() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_db(tmp.path().join("t.db")).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('clips','secrets','kinds','tags','meta')",
            [], |r| r.get(0)).unwrap();
        assert_eq!(count, 5);
        let mode: String = conn.pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn migrates_v1_to_v2_preserves_existing_rows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("t.db");

        // 1. Open at v1 only (bypass migration ladder), insert a text row.
        // We construct v1 directly so that the second open_db genuinely
        // exercises the v1 -> v2 migration step.
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            // schema_version is set to '1' by SCHEMA_V1 itself.
            conn.execute(
                "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                    first_copied_at, last_copied_at)
                 VALUES ('u1', 'hello', 'hello', 5, 5, 'abc', 'text', 1, 1)",
                [],
            )
            .unwrap();
        }

        // 2. Re-open: migration ladder should run v2 and v3 (current max).
        let conn = open_db(&db_path).unwrap();
        let version: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "3");

        // 3. Existing row survives, with default payload_kind = 'text'.
        let (text, payload_kind, blob_path): (String, String, Option<String>) = conn
            .query_row(
                "SELECT text, payload_kind, blob_path FROM clips WHERE uuid = 'u1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(text, "hello");
        assert_eq!(payload_kind, "text");
        assert!(blob_path.is_none());
    }

    #[test]
    fn migrates_v2_to_v3_preserves_existing_rows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("t.db");

        // 1. Build a v2 DB directly (v1 + v2), insert a row.
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(SCHEMA_V1).unwrap();
            conn.execute_batch(SCHEMA_V2).unwrap();
            conn.execute(
                "INSERT INTO clips (uuid, text, preview, length, byte_length, hash, primary_kind,
                                    first_copied_at, last_copied_at)
                 VALUES ('u2', 'world', 'world', 5, 5, 'def', 'text', 2, 2)",
                [],
            )
            .unwrap();
        }

        // 2. Re-open: migration ladder should run v3.
        let conn = open_db(&db_path).unwrap();
        let version: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, "3");

        // 3. Old row survives.
        let text: String = conn
            .query_row("SELECT text FROM clips WHERE uuid = 'u2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(text, "world");

        // 4. audit_log table exists and is empty.
        let audit_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(audit_count, 0);

        // 5. secrets.last_revealed_at column exists — verify via pragma.
        let col_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('secrets') WHERE name = 'last_revealed_at'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(col_exists, 1, "secrets.last_revealed_at column should exist after v3 migration");
    }

    #[test]
    fn v2_index_on_payload_kind_exists() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open_db(tmp.path().join("t.db")).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_clips_payload_kind'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
