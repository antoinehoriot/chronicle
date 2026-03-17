use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::db::models::{Event, HookPayload, Snapshot};
use crate::db::queries;
#[cfg(test)]
use crate::db::schema;

const FILE_MODIFYING_TOOLS: &[&str] = &["Edit", "Write"];

struct PendingSnapshot {
    file_path: String,
    content_before: Option<Vec<u8>>,
    event_id: i64,
    inserted_at: std::time::Instant,
}

pub struct EventProcessor {
    conn: Arc<Mutex<Connection>>,
    pending_pre: HashMap<String, PendingSnapshot>,
    broadcast_tx: broadcast::Sender<Event>,
}

impl EventProcessor {
    pub fn new(conn: Arc<Mutex<Connection>>, broadcast_tx: broadcast::Sender<Event>) -> Self {
        Self {
            conn,
            pending_pre: HashMap::new(),
            broadcast_tx,
        }
    }

    pub async fn process(&mut self, payload: HookPayload) -> Result<()> {
        self.flush_pending_snapshots().await?;

        let event_type = payload.hook_event_name.as_deref().unwrap_or("Unknown");
        let session_id = payload.session_id.as_deref().unwrap_or("unknown");
        let now = chrono::Utc::now().timestamp_millis();

        match event_type {
            "SessionStart" => {
                let session = crate::db::models::Session {
                    id: session_id.to_string(),
                    started_at: now,
                    ended_at: None,
                    cwd: payload.cwd.unwrap_or_default(),
                    model: payload.model,
                    permission_mode: payload.permission_mode,
                };
                let db = self.conn.lock().await;
                queries::upsert_session(&db, &session)?;
            }
            "SessionEnd" => {
                let db = self.conn.lock().await;
                db.execute(
                    "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                    rusqlite::params![now, session_id],
                )?;
            }
            "PreToolUse" => {
                let tool_name = payload.tool_name.as_deref().unwrap_or("");
                let tool_use_id = payload.tool_use_id.clone();

                // File I/O outside the lock
                let file_path = if FILE_MODIFYING_TOOLS.contains(&tool_name) {
                    self.extract_file_path(&payload)
                } else {
                    None
                };
                let content_before = file_path.as_ref().and_then(|p| std::fs::read(p).ok());

                let event = self.payload_to_event(&payload, session_id, now);
                let db = self.conn.lock().await;
                let event_id = queries::insert_event(&db, &event)?;
                drop(db);

                // Store pending snapshot keyed by tool_use_id
                if let (Some(tuid), Some(path)) = (tool_use_id, file_path) {
                    self.pending_pre.insert(
                        tuid,
                        PendingSnapshot {
                            file_path: path,
                            content_before,
                            event_id,
                            inserted_at: std::time::Instant::now(),
                        },
                    );
                }

                let mut stored = event;
                stored.id = event_id;
                let _ = self.broadcast_tx.send(stored);
            }
            "PostToolUse" => {
                let tool_use_id = payload.tool_use_id.clone();

                // File I/O and compression outside the lock
                let snapshot = if let Some(ref tuid) = tool_use_id {
                    if let Some(pending) = self.pending_pre.remove(tuid) {
                        let content_after = std::fs::read(&pending.file_path).ok();
                        let diff = self.compute_diff(
                            pending.content_before.as_deref(),
                            content_after.as_deref(),
                        );
                        let compressed_before = pending
                            .content_before
                            .map(|c| zstd::encode_all(c.as_slice(), 3))
                            .transpose()?;
                        let compressed_after = content_after
                            .map(|c| zstd::encode_all(c.as_slice(), 3))
                            .transpose()?;
                        Some((pending.file_path, compressed_before, compressed_after, diff))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let event = self.payload_to_event(&payload, session_id, now);
                let db = self.conn.lock().await;
                let event_id = queries::insert_event(&db, &event)?;

                if let Some((path, compressed_before, compressed_after, diff)) = snapshot {
                    let snap = Snapshot {
                        id: 0,
                        event_id,
                        file_path: path,
                        content_before: compressed_before,
                        content_after: compressed_after,
                        diff_unified: diff,
                    };
                    queries::insert_snapshot(&db, &snap)?;
                }
                drop(db);

                let mut stored = event;
                stored.id = event_id;
                let _ = self.broadcast_tx.send(stored);
            }
            "PostToolUseFailure" => {
                if let Some(ref tuid) = payload.tool_use_id {
                    self.pending_pre.remove(tuid);
                }
                let event = self.payload_to_event(&payload, session_id, now);
                let db = self.conn.lock().await;
                let event_id = queries::insert_event(&db, &event)?;
                drop(db);
                let mut stored = event;
                stored.id = event_id;
                let _ = self.broadcast_tx.send(stored);
            }
            _ => {
                let session = crate::db::models::Session {
                    id: session_id.to_string(),
                    started_at: now,
                    ended_at: None,
                    cwd: payload.cwd.clone().unwrap_or_default(),
                    model: None,
                    permission_mode: None,
                };
                let event = self.payload_to_event(&payload, session_id, now);
                let db = self.conn.lock().await;
                queries::upsert_session(&db, &session)?;
                let event_id = queries::insert_event(&db, &event)?;
                drop(db);
                let mut stored = event;
                stored.id = event_id;
                let _ = self.broadcast_tx.send(stored);
            }
        }

        Ok(())
    }

    fn payload_to_event(&self, payload: &HookPayload, session_id: &str, timestamp: i64) -> Event {
        Event {
            id: 0,
            session_id: session_id.to_string(),
            timestamp,
            event_type: payload.hook_event_name.clone().unwrap_or_default(),
            tool_name: payload.tool_name.clone(),
            tool_use_id: payload.tool_use_id.clone(),
            agent_id: payload.agent_id.clone(),
            agent_type: payload.agent_type.clone(),
            input_json: payload
                .tool_input
                .as_ref()
                .map(|v| serde_json::to_vec(v))
                .transpose()
                .ok()
                .flatten(),
            output_json: payload
                .tool_response
                .as_ref()
                .map(|s| s.as_bytes().to_vec())
                .or_else(|| payload.tool_error.as_ref().map(|s| s.as_bytes().to_vec()))
                .or_else(|| payload.prompt.as_ref().map(|s| s.as_bytes().to_vec()))
                .or_else(|| {
                    payload
                        .last_assistant_message
                        .as_ref()
                        .map(|s| s.as_bytes().to_vec())
                }),
        }
    }

    fn extract_file_path(&self, payload: &HookPayload) -> Option<String> {
        payload
            .tool_input
            .as_ref()
            .and_then(|v| v.get("file_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    fn compute_diff(&self, before: Option<&[u8]>, after: Option<&[u8]>) -> String {
        let before_str = before
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let after_str = after
            .map(|b| String::from_utf8_lossy(b).to_string())
            .unwrap_or_default();
        let diff = similar::TextDiff::from_lines(&before_str, &after_str);
        diff.unified_diff().header("a", "b").to_string()
    }

    async fn flush_pending_snapshots(&mut self) -> Result<()> {
        let flushed: Vec<String> = self
            .pending_pre
            .iter()
            .filter_map(|(tuid, pending)| {
                let content_after = std::fs::read(&pending.file_path).ok();
                if content_after != pending.content_before {
                    Some(tuid.clone())
                } else {
                    None
                }
            })
            .collect();

        for tuid in flushed {
            let pending = self.pending_pre.remove(&tuid).unwrap();
            let content_after = std::fs::read(&pending.file_path).ok();
            let diff = self.compute_diff(
                pending.content_before.as_deref(),
                content_after.as_deref(),
            );
            let compressed_before = pending
                .content_before
                .map(|c| zstd::encode_all(c.as_slice(), 3))
                .transpose()?;
            let compressed_after = content_after
                .map(|c| zstd::encode_all(c.as_slice(), 3))
                .transpose()?;

            let db = self.conn.lock().await;
            let snap = Snapshot {
                id: 0,
                event_id: pending.event_id,
                file_path: pending.file_path,
                content_before: compressed_before,
                content_after: compressed_after,
                diff_unified: diff,
            };
            queries::insert_snapshot(&db, &snap)?;
        }

        Ok(())
    }

    pub fn evict_stale_entries(&mut self) {
        let ttl = std::time::Duration::from_secs(10 * 60);
        let now = std::time::Instant::now();
        self.pending_pre
            .retain(|_id, pending| now.duration_since(pending.inserted_at) < ttl);
    }

    pub fn clear_pending(&mut self) {
        self.pending_pre.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<Mutex<Connection>>, broadcast::Sender<Event>) {
        let conn = Connection::open_in_memory().unwrap();
        schema::initialize(&conn).unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let (tx, _rx) = broadcast::channel(100);
        (conn, tx)
    }

    #[tokio::test]
    async fn test_session_start_creates_session() {
        let (conn, tx) = setup();
        let mut proc = EventProcessor::new(conn.clone(), tx);
        let payload = HookPayload {
            session_id: Some("sess1".into()),
            hook_event_name: Some("SessionStart".into()),
            cwd: Some("/tmp/project".into()),
            model: Some("claude-sonnet".into()),
            permission_mode: Some("default".into()),
            ..default_payload()
        };
        proc.process(payload).await.unwrap();
        let db = conn.lock().await;
        let sessions = queries::list_sessions(&db).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "sess1");
    }

    #[tokio::test]
    async fn test_post_tool_use_creates_event() {
        let (conn, tx) = setup();
        let mut proc = EventProcessor::new(conn.clone(), tx);
        let start = HookPayload {
            session_id: Some("sess1".into()),
            hook_event_name: Some("SessionStart".into()),
            cwd: Some("/tmp".into()),
            ..default_payload()
        };
        proc.process(start).await.unwrap();
        let payload = HookPayload {
            session_id: Some("sess1".into()),
            hook_event_name: Some("PostToolUse".into()),
            tool_name: Some("Read".into()),
            tool_use_id: Some("tu1".into()),
            ..default_payload()
        };
        proc.process(payload).await.unwrap();
        let db = conn.lock().await;
        let events = queries::list_events_for_session(&db, "sess1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool_name.as_deref(), Some("Read"));
    }

    fn default_payload() -> HookPayload {
        HookPayload {
            session_id: None,
            hook_event_name: None,
            cwd: None,
            permission_mode: None,
            model: None,
            agent_id: None,
            agent_type: None,
            tool_name: None,
            tool_input: None,
            tool_use_id: None,
            tool_response: None,
            tool_error: None,
            prompt: None,
            last_assistant_message: None,
            source: None,
        }
    }
}
