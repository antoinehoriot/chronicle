use anyhow::Result;
use rusqlite::{params, Connection};

use super::models::{Event, Session, Snapshot};

pub fn upsert_session(conn: &Connection, session: &Session) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (id, started_at, ended_at, cwd, model, permission_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
            ended_at = COALESCE(excluded.ended_at, sessions.ended_at),
            model = COALESCE(excluded.model, sessions.model),
            permission_mode = COALESCE(excluded.permission_mode, sessions.permission_mode)",
        params![
            session.id, session.started_at, session.ended_at,
            session.cwd, session.model, session.permission_mode,
        ],
    )?;
    Ok(())
}

pub fn insert_event(conn: &Connection, event: &Event) -> Result<i64> {
    conn.execute(
        "INSERT INTO events (session_id, timestamp, event_type, tool_name, tool_use_id,
            agent_id, agent_type, input_json, output_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            event.session_id, event.timestamp, event.event_type,
            event.tool_name, event.tool_use_id, event.agent_id,
            event.agent_type, event.input_json, event.output_json,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_snapshot(conn: &Connection, snapshot: &Snapshot) -> Result<i64> {
    conn.execute(
        "INSERT INTO snapshots (event_id, file_path, content_before, content_after, diff_unified)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            snapshot.event_id, snapshot.file_path, snapshot.content_before,
            snapshot.content_after, snapshot.diff_unified,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, started_at, ended_at, cwd, model, permission_mode
         FROM sessions ORDER BY started_at DESC",
    )?;
    let sessions = stmt.query_map([], |row| {
        Ok(Session {
            id: row.get(0)?, started_at: row.get(1)?, ended_at: row.get(2)?,
            cwd: row.get(3)?, model: row.get(4)?, permission_mode: row.get(5)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(sessions)
}

pub fn list_events_for_session(conn: &Connection, session_id: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, timestamp, event_type, tool_name, tool_use_id,
                agent_id, agent_type, input_json, output_json
         FROM events WHERE session_id = ?1 ORDER BY timestamp ASC",
    )?;
    let events = stmt.query_map([session_id], |row| {
        Ok(Event {
            id: row.get(0)?, session_id: row.get(1)?, timestamp: row.get(2)?,
            event_type: row.get(3)?, tool_name: row.get(4)?, tool_use_id: row.get(5)?,
            agent_id: row.get(6)?, agent_type: row.get(7)?,
            input_json: row.get(8)?, output_json: row.get(9)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(events)
}

pub fn count_events_for_session(conn: &Connection, session_id: &str) -> Result<i64> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM events WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn get_snapshots_for_event(conn: &Connection, event_id: i64) -> Result<Vec<Snapshot>> {
    let mut stmt = conn.prepare(
        "SELECT id, event_id, file_path, content_before, content_after, diff_unified
         FROM snapshots WHERE event_id = ?1",
    )?;
    let snapshots = stmt.query_map([event_id], |row| {
        Ok(Snapshot {
            id: row.get(0)?, event_id: row.get(1)?, file_path: row.get(2)?,
            content_before: row.get(3)?, content_after: row.get(4)?, diff_unified: row.get(5)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(snapshots)
}

pub fn get_file_states_at_event(
    conn: &Connection, session_id: &str, event_id: i64,
) -> Result<Vec<Snapshot>> {
    let mut stmt = conn.prepare(
        "WITH ranked AS (
             SELECT s.id, s.event_id, s.file_path, s.content_before, s.content_after, s.diff_unified,
                    ROW_NUMBER() OVER (PARTITION BY s.file_path ORDER BY e.id DESC) AS rn
             FROM snapshots s
             JOIN events e ON s.event_id = e.id
             WHERE e.session_id = ?1 AND e.id <= ?2
         )
         SELECT id, event_id, file_path, content_before, content_after, diff_unified
         FROM ranked WHERE rn = 1",
    )?;
    let snapshots = stmt.query_map(params![session_id, event_id], |row| {
        Ok(Snapshot {
            id: row.get(0)?, event_id: row.get(1)?, file_path: row.get(2)?,
            content_before: row.get(3)?, content_after: row.get(4)?, diff_unified: row.get(5)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(snapshots)
}

/// For each file modified AFTER event_id in this session, return the desired
/// state at event_id. If the file had a snapshot at/before event_id, returns
/// that snapshot's content_after as the target state. If no snapshot existed
/// at/before event_id, content_after is NULL (file should not exist).
pub fn get_restore_targets(
    conn: &Connection, session_id: &str, event_id: i64,
) -> Result<Vec<Snapshot>> {
    let mut stmt = conn.prepare(
        "WITH files_changed_after AS (
             SELECT DISTINCT s.file_path
             FROM snapshots s
             JOIN events e ON s.event_id = e.id
             WHERE e.session_id = ?1 AND e.id > ?2
         ),
         state_at_target AS (
             SELECT s.id, s.event_id, s.file_path, s.content_before, s.content_after, s.diff_unified,
                    ROW_NUMBER() OVER (PARTITION BY s.file_path ORDER BY e.id DESC) AS rn
             FROM snapshots s
             JOIN events e ON s.event_id = e.id
             WHERE e.session_id = ?1 AND e.id <= ?2
         )
         SELECT COALESCE(st.id, 0), COALESCE(st.event_id, 0), f.file_path,
                st.content_before, st.content_after, COALESCE(st.diff_unified, '')
         FROM files_changed_after f
         LEFT JOIN state_at_target st ON st.file_path = f.file_path AND st.rn = 1",
    )?;
    let snapshots = stmt.query_map(params![session_id, event_id], |row| {
        Ok(Snapshot {
            id: row.get(0)?, event_id: row.get(1)?, file_path: row.get(2)?,
            content_before: row.get(3)?, content_after: row.get(4)?, diff_unified: row.get(5)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::initialize(&conn).unwrap();
        conn
    }

    #[test]
    fn test_upsert_session() {
        let conn = setup_db();
        let session = Session {
            id: "test-session".into(), started_at: 1000, ended_at: None,
            cwd: "/tmp".into(), model: Some("claude-sonnet".into()),
            permission_mode: Some("default".into()),
        };
        upsert_session(&conn, &session).unwrap();
        let sessions = list_sessions(&conn).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "test-session");
    }

    #[test]
    fn test_insert_and_query_events() {
        let conn = setup_db();
        upsert_session(&conn, &Session {
            id: "s1".into(), started_at: 1000, ended_at: None,
            cwd: "/tmp".into(), model: None, permission_mode: None,
        }).unwrap();

        let event = Event {
            id: 0, session_id: "s1".into(), timestamp: 1001,
            event_type: "PostToolUse".into(), tool_name: Some("Edit".into()),
            tool_use_id: Some("tu1".into()), agent_id: None, agent_type: None,
            input_json: None, output_json: None,
        };
        let event_id = insert_event(&conn, &event).unwrap();
        assert!(event_id > 0);

        let events = list_events_for_session(&conn, "s1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool_name.as_deref(), Some("Edit"));
    }

    #[test]
    fn test_snapshots() {
        let conn = setup_db();
        upsert_session(&conn, &Session {
            id: "s1".into(), started_at: 1000, ended_at: None,
            cwd: "/tmp".into(), model: None, permission_mode: None,
        }).unwrap();
        let event_id = insert_event(&conn, &Event {
            id: 0, session_id: "s1".into(), timestamp: 1001,
            event_type: "PostToolUse".into(), tool_name: Some("Write".into()),
            tool_use_id: Some("tu1".into()), agent_id: None, agent_type: None,
            input_json: None, output_json: None,
        }).unwrap();

        let snapshot = Snapshot {
            id: 0, event_id, file_path: "/tmp/test.rs".into(),
            content_before: None, content_after: Some(b"hello".to_vec()),
            diff_unified: "+hello".into(),
        };
        insert_snapshot(&conn, &snapshot).unwrap();

        let snaps = get_snapshots_for_event(&conn, event_id).unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].file_path, "/tmp/test.rs");
        assert!(snaps[0].content_before.is_none());
    }
}
