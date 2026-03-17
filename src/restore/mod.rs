use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

use crate::db::{models::Snapshot, queries};

#[derive(Debug, PartialEq)]
pub enum RestoreAction {
    Overwrite { path: String },
    Create { path: String },
    Delete { path: String },
}

pub fn restore_to_event(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
) -> Result<Vec<RestoreAction>> {
    // Get each file modified AFTER the target event, along with its desired
    // state at the target event. content_after = desired state (None = should not exist).
    let snapshots = queries::get_restore_targets(conn, session_id, event_id)?;
    let mut actions = Vec::new();

    for snap in &snapshots {
        let path = Path::new(&snap.file_path);
        let file_exists = path.exists();

        match &snap.content_after {
            Some(_) => {
                // File should exist with this content at the target event
                if file_exists {
                    actions.push(RestoreAction::Overwrite {
                        path: snap.file_path.clone(),
                    });
                } else {
                    actions.push(RestoreAction::Create {
                        path: snap.file_path.clone(),
                    });
                }
            }
            None => {
                // File should not exist at the target event (created after it)
                if file_exists {
                    actions.push(RestoreAction::Delete {
                        path: snap.file_path.clone(),
                    });
                }
            }
        }
    }

    Ok(actions)
}

pub fn execute_restore(conn: &Connection, session_id: &str, event_id: i64) -> Result<()> {
    let snapshots = queries::get_restore_targets(conn, session_id, event_id)?;

    let now = chrono::Utc::now().timestamp_millis();
    let checkpoint_event = crate::db::models::Event {
        id: 0,
        session_id: session_id.to_string(),
        timestamp: now,
        event_type: "RestoreCheckpoint".to_string(),
        tool_name: None,
        tool_use_id: None,
        agent_id: None,
        agent_type: None,
        input_json: Some(format!(r#"{{"restored_to_event_id":{event_id}}}"#).into_bytes()),
        output_json: None,
    };
    let checkpoint_id = queries::insert_event(conn, &checkpoint_event)?;

    for snap in &snapshots {
        let current_content = std::fs::read(&snap.file_path).ok();
        let checkpoint_snap = Snapshot {
            id: 0,
            event_id: checkpoint_id,
            file_path: snap.file_path.clone(),
            content_before: current_content
                .as_ref()
                .map(|c| zstd::encode_all(c.as_slice(), 3))
                .transpose()?,
            content_after: current_content
                .map(|c| zstd::encode_all(c.as_slice(), 3))
                .transpose()?,
            diff_unified: "(checkpoint)".to_string(),
        };
        queries::insert_snapshot(conn, &checkpoint_snap)?;
    }

    // Phase 1: Write all restored files to temp paths
    let mut temp_files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut files_to_delete: Vec<String> = Vec::new();

    for snap in &snapshots {
        if let Some(ref compressed) = snap.content_after {
            let content = zstd::decode_all(compressed.as_slice())?;
            let path = Path::new(&snap.file_path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temp_path = path.with_extension("chronicle_tmp");
            std::fs::write(&temp_path, &content)?;
            temp_files.push((snap.file_path.clone(), temp_path));
        } else if Path::new(&snap.file_path).exists() {
            files_to_delete.push(snap.file_path.clone());
        }
    }

    // Phase 2: Rename all temp files into place
    for (target, temp) in &temp_files {
        std::fs::rename(temp, target)
            .with_context(|| format!("Failed to rename {} to {}", temp.display(), target))?;
    }

    // Phase 3: Delete files that should no longer exist
    for path in &files_to_delete {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to delete {path}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Event, Session};
    use crate::db::{queries, schema};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::initialize(&conn).unwrap();
        queries::upsert_session(
            &conn,
            &Session {
                id: "s1".into(),
                started_at: 1000,
                ended_at: None,
                cwd: "/tmp".into(),
                model: None,
                permission_mode: None,
            },
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_restore_plan_deletes_file_created_after_target() {
        let conn = setup_db();

        // Event 1: some baseline event (no snapshots)
        let eid1 = queries::insert_event(
            &conn,
            &Event {
                id: 0,
                session_id: "s1".into(),
                timestamp: 1001,
                event_type: "PostToolUse".into(),
                tool_name: Some("Read".into()),
                tool_use_id: Some("tu0".into()),
                agent_id: None,
                agent_type: None,
                input_json: None,
                output_json: None,
            },
        )
        .unwrap();

        // Event 2: creates a new file
        let eid2 = queries::insert_event(
            &conn,
            &Event {
                id: 0,
                session_id: "s1".into(),
                timestamp: 1002,
                event_type: "PostToolUse".into(),
                tool_name: Some("Write".into()),
                tool_use_id: Some("tu1".into()),
                agent_id: None,
                agent_type: None,
                input_json: None,
                output_json: None,
            },
        )
        .unwrap();

        // Create a temp file to simulate it existing on disk
        let tmp_path = std::env::temp_dir().join("chronicle_test_restore_delete.rs");
        std::fs::write(&tmp_path, "fn main() {}").unwrap();
        let tmp_path_str = tmp_path.to_str().unwrap().to_string();

        queries::insert_snapshot(
            &conn,
            &Snapshot {
                id: 0,
                event_id: eid2,
                file_path: tmp_path_str.clone(),
                content_before: None,
                content_after: Some(zstd::encode_all(b"fn main() {}".as_slice(), 3).unwrap()),
                diff_unified: "+fn main() {}".into(),
            },
        )
        .unwrap();

        // Restoring to event 1 (before the file was created) should delete it
        let actions = restore_to_event(&conn, "s1", eid1).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            RestoreAction::Delete { path } if path == &tmp_path_str
        ));

        // Restoring to event 2 (when the file was created) should have no actions
        let actions2 = restore_to_event(&conn, "s1", eid2).unwrap();
        assert!(actions2.is_empty());

        let _ = std::fs::remove_file(&tmp_path);
    }
}
