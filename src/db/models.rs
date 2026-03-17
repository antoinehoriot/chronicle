use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub cwd: String,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    pub session_id: String,
    pub timestamp: i64,
    pub event_type: String,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub input_json: Option<Vec<u8>>,
    pub output_json: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub id: i64,
    pub event_id: i64,
    pub file_path: String,
    pub content_before: Option<Vec<u8>>,
    pub content_after: Option<Vec<u8>>,
    pub diff_unified: String,
}

/// The raw JSON payload from a Claude Code hook (common fields).
#[derive(Debug, Clone, Deserialize)]
pub struct HookPayload {
    pub session_id: Option<String>,
    pub hook_event_name: Option<String>,
    pub cwd: Option<String>,
    pub permission_mode: Option<String>,
    pub model: Option<String>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_use_id: Option<String>,
    pub tool_response: Option<String>,
    pub tool_error: Option<String>,
    pub prompt: Option<String>,
    pub last_assistant_message: Option<String>,
    pub source: Option<String>,
}
