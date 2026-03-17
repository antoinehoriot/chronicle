use anyhow::Result;
use rusqlite::Connection;

const CURRENT_SCHEMA_VERSION: i32 = 1;

pub fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            started_at INTEGER NOT NULL,
            ended_at INTEGER,
            cwd TEXT NOT NULL,
            model TEXT,
            permission_mode TEXT
        );

        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id),
            timestamp INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            tool_name TEXT,
            tool_use_id TEXT,
            agent_id TEXT,
            agent_type TEXT,
            input_json BLOB,
            output_json BLOB
        );

        CREATE TABLE IF NOT EXISTS snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id INTEGER NOT NULL REFERENCES events(id),
            file_path TEXT NOT NULL,
            content_before BLOB,
            content_after BLOB,
            diff_unified TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_events_session_ts ON events(session_id, timestamp);
        CREATE INDEX IF NOT EXISTS idx_events_tool_use_id ON events(tool_use_id);
        CREATE INDEX IF NOT EXISTS idx_snapshots_event ON snapshots(event_id);
        CREATE INDEX IF NOT EXISTS idx_snapshots_file_event ON snapshots(file_path, event_id);",
    )?;

    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM schema_version",
        [],
        |row| row.get(0),
    )?;
    if count == 0 {
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [CURRENT_SCHEMA_VERSION],
        )?;
    }

    Ok(())
}

pub fn get_version(conn: &Connection) -> Result<i32> {
    let version = conn.query_row(
        "SELECT version FROM schema_version",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"events".to_string()));
        assert!(tables.contains(&"snapshots".to_string()));
        assert!(tables.contains(&"schema_version".to_string()));
    }

    #[test]
    fn test_initialize_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        initialize(&conn).unwrap();
    }

    #[test]
    fn test_schema_version() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        assert_eq!(get_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
    }
}
