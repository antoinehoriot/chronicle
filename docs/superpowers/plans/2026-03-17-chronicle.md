# Chronicle Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI that hooks into Claude Code to record every agent action, with a ratatui TUI dashboard for session replay, undo/rollback, debugging, and live monitoring.

**Architecture:** A tokio-based daemon listens on a Unix socket for JSON events from Claude Code hooks, stores them in SQLite (WAL mode) with zstd-compressed file snapshots, and broadcasts to connected TUI clients via `tokio::sync::broadcast`. The CLI manages hooks, daemon lifecycle, and provides a ratatui timeline + detail panel interface.

**Tech Stack:** Rust, tokio, rusqlite, ratatui, crossterm, serde, similar, zstd, clap

---

## File Structure

```
Cargo.toml
src/
  lib.rs                   # Library crate re-exporting all modules (for integration tests)
  main.rs                  # CLI entry point (clap), dispatches to subcommands
  cli.rs                   # Clap command definitions
  db/
    mod.rs                 # Re-exports
    schema.rs              # Schema creation, migrations, schema_version table
    models.rs              # Session, Event, Snapshot structs
    queries.rs             # Insert/query functions for sessions, events, snapshots
  daemon/
    mod.rs                 # Re-exports
    server.rs              # Tokio Unix socket listener + event loop
    processor.rs           # Event processor (Pre/Post pairing, snapshot capture, broadcast)
  hooks/
    mod.rs                 # Re-exports
    relay.rs               # hook-relay subcommand (stdin → socket, always exit 0)
    installer.rs           # Init/show/remove hooks in settings.local.json
  restore/
    mod.rs                 # Snapshot-at restore logic, atomic file replacement, RestoreCheckpoint
  tui/
    mod.rs                 # Re-exports
    app.rs                 # App state, event handling, main loop
    timeline.rs            # Left panel: event timeline list widget
    detail.rs              # Right panel: diff/output viewer widget
    statusbar.rs           # Bottom bar: session info, keybindings
```

---

## Task 1: Project Scaffold + CLI Skeleton

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/cli.rs`

- [ ] **Step 1: Initialize the Cargo project**

```bash
cd /Users/antoinehoriot/Projects/chronicle-v2
cargo init --name chronicle
```

- [ ] **Step 2: Add dependencies to Cargo.toml**

Replace the generated `Cargo.toml` with:

```toml
[package]
name = "chronicle"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ratatui = "0.29"
crossterm = "0.28"
similar = "2"
zstd = "0.13"
anyhow = "1"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

- [ ] **Step 3: Create CLI definition in `src/cli.rs`**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "chronicle", about = "Track and replay Claude Code agent sessions")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize chronicle in the current project
    Init,
    /// Launch the TUI dashboard
    Tui,
    /// List recorded sessions
    Sessions,
    /// Restore files to their state at a specific event
    Restore {
        /// Event ID to restore to
        event_id: i64,
    },
    /// Manage hooks
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },
    /// Manage the chronicle daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },
    /// Relay hook data from stdin to the daemon (used by hook scripts)
    HookRelay,
}

#[derive(Subcommand)]
pub enum HooksCommands {
    /// Show installed hook configuration
    Show,
    /// Remove chronicle hooks
    Remove,
}

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Start the daemon
    Start,
    /// Stop the daemon
    Stop,
    /// Show daemon status
    Status,
}
```

- [ ] **Step 4: Create `src/main.rs` that dispatches commands**

```rust
mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands, DaemonCommands, HooksCommands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            // Default: launch TUI
            println!("TUI not yet implemented");
        }
        Some(Commands::Init) => println!("Init not yet implemented"),
        Some(Commands::Tui) => println!("TUI not yet implemented"),
        Some(Commands::Sessions) => println!("Sessions not yet implemented"),
        Some(Commands::Restore { event_id }) => {
            println!("Restore to event {event_id} not yet implemented");
        }
        Some(Commands::Hooks { command }) => match command {
            HooksCommands::Show => println!("Hooks show not yet implemented"),
            HooksCommands::Remove => println!("Hooks remove not yet implemented"),
        },
        Some(Commands::Daemon { command }) => match command {
            DaemonCommands::Start => println!("Daemon start not yet implemented"),
            DaemonCommands::Stop => println!("Daemon stop not yet implemented"),
            DaemonCommands::Status => println!("Daemon status not yet implemented"),
        },
        Some(Commands::HookRelay) => println!("Hook relay not yet implemented"),
    }

    Ok(())
}
```

- [ ] **Step 5: Verify it compiles and runs**

```bash
cargo build
cargo run -- --help
cargo run -- init
cargo run -- hooks show
```

Expected: Help text shows all commands. Subcommands print "not yet implemented".

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "feat: project scaffold with CLI skeleton (clap)"
```

---

## Task 2: Database Schema + Models

**Files:**
- Create: `src/db/mod.rs`
- Create: `src/db/schema.rs`
- Create: `src/db/models.rs`
- Create: `src/db/queries.rs`

- [ ] **Step 1: Create `src/db/mod.rs`**

```rust
pub mod models;
pub mod queries;
pub mod schema;
```

- [ ] **Step 2: Write the schema test first in `src/db/schema.rs`**

```rust
use anyhow::Result;
use rusqlite::Connection;

pub fn initialize(conn: &Connection) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();

        // Verify all tables exist
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
        initialize(&conn).unwrap(); // Should not error
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test db::schema
```

Expected: FAIL with "not yet implemented"

- [ ] **Step 4: Implement `initialize` in `src/db/schema.rs`**

```rust
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

    // Set schema version if not present
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
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test db::schema
```

Expected: all 3 tests PASS

- [ ] **Step 6: Create models in `src/db/models.rs`**

```rust
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
    // Tool-specific fields
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_use_id: Option<String>,
    pub tool_response: Option<String>,
    pub tool_error: Option<String>,
    // User prompt
    pub prompt: Option<String>,
    // Stop event
    pub last_assistant_message: Option<String>,
    // Session start
    pub source: Option<String>,
}
```

- [ ] **Step 7: Create query functions with tests in `src/db/queries.rs`**

```rust
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
            session.id,
            session.started_at,
            session.ended_at,
            session.cwd,
            session.model,
            session.permission_mode,
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
            event.session_id,
            event.timestamp,
            event.event_type,
            event.tool_name,
            event.tool_use_id,
            event.agent_id,
            event.agent_type,
            event.input_json,
            event.output_json,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn insert_snapshot(conn: &Connection, snapshot: &Snapshot) -> Result<i64> {
    conn.execute(
        "INSERT INTO snapshots (event_id, file_path, content_before, content_after, diff_unified)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            snapshot.event_id,
            snapshot.file_path,
            snapshot.content_before,
            snapshot.content_after,
            snapshot.diff_unified,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, started_at, ended_at, cwd, model, permission_mode
         FROM sessions ORDER BY started_at DESC",
    )?;
    let sessions = stmt
        .query_map([], |row| {
            Ok(Session {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                cwd: row.get(3)?,
                model: row.get(4)?,
                permission_mode: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(sessions)
}

pub fn list_events_for_session(conn: &Connection, session_id: &str) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, timestamp, event_type, tool_name, tool_use_id,
                agent_id, agent_type, input_json, output_json
         FROM events WHERE session_id = ?1 ORDER BY timestamp ASC",
    )?;
    let events = stmt
        .query_map([session_id], |row| {
            Ok(Event {
                id: row.get(0)?,
                session_id: row.get(1)?,
                timestamp: row.get(2)?,
                event_type: row.get(3)?,
                tool_name: row.get(4)?,
                tool_use_id: row.get(5)?,
                agent_id: row.get(6)?,
                agent_type: row.get(7)?,
                input_json: row.get(8)?,
                output_json: row.get(9)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(events)
}

pub fn get_snapshots_for_event(conn: &Connection, event_id: i64) -> Result<Vec<Snapshot>> {
    let mut stmt = conn.prepare(
        "SELECT id, event_id, file_path, content_before, content_after, diff_unified
         FROM snapshots WHERE event_id = ?1",
    )?;
    let snapshots = stmt
        .query_map([event_id], |row| {
            Ok(Snapshot {
                id: row.get(0)?,
                event_id: row.get(1)?,
                file_path: row.get(2)?,
                content_before: row.get(3)?,
                content_after: row.get(4)?,
                diff_unified: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(snapshots)
}

/// Get the most recent snapshot for each file at or before a given event ID.
/// Used by the snapshot-at restore model.
pub fn get_file_states_at_event(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
) -> Result<Vec<Snapshot>> {
    // Use a window function to get the most recent snapshot per file path
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
    let snapshots = stmt
        .query_map(params![session_id, event_id], |row| {
            Ok(Snapshot {
                id: row.get(0)?,
                event_id: row.get(1)?,
                file_path: row.get(2)?,
                content_before: row.get(3)?,
                content_after: row.get(4)?,
                diff_unified: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
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
            id: "test-session".into(),
            started_at: 1000,
            ended_at: None,
            cwd: "/tmp".into(),
            model: Some("claude-sonnet".into()),
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
        let session = Session {
            id: "s1".into(),
            started_at: 1000,
            ended_at: None,
            cwd: "/tmp".into(),
            model: None,
            permission_mode: None,
        };
        upsert_session(&conn, &session).unwrap();

        let event = Event {
            id: 0,
            session_id: "s1".into(),
            timestamp: 1001,
            event_type: "PostToolUse".into(),
            tool_name: Some("Edit".into()),
            tool_use_id: Some("tu1".into()),
            agent_id: None,
            agent_type: None,
            input_json: None,
            output_json: None,
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
        upsert_session(
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
        let event_id = insert_event(
            &conn,
            &Event {
                id: 0,
                session_id: "s1".into(),
                timestamp: 1001,
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

        let snapshot = Snapshot {
            id: 0,
            event_id,
            file_path: "/tmp/test.rs".into(),
            content_before: None, // new file
            content_after: Some(b"hello".to_vec()),
            diff_unified: "+hello".into(),
        };
        insert_snapshot(&conn, &snapshot).unwrap();

        let snaps = get_snapshots_for_event(&conn, event_id).unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].file_path, "/tmp/test.rs");
        assert!(snaps[0].content_before.is_none());
    }
}
```

- [ ] **Step 8: Add `mod db;` to `main.rs` and run all tests**

Add `mod db;` to `src/main.rs`.

```bash
cargo test
```

Expected: all tests PASS

- [ ] **Step 9: Commit**

```bash
git add src/db/ src/main.rs
git commit -m "feat: database schema, models, and query layer"
```

---

## Task 3: Hook Relay + Installer

**Files:**
- Create: `src/hooks/mod.rs`
- Create: `src/hooks/relay.rs`
- Create: `src/hooks/installer.rs`

- [ ] **Step 1: Create `src/hooks/mod.rs`**

```rust
pub mod installer;
pub mod relay;
```

- [ ] **Step 2: Write `src/hooks/relay.rs`**

The hook-relay command reads JSON from stdin and writes it to the Unix socket. Always exits 0.

```rust
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

pub fn run(chronicle_dir: &Path) -> i32 {
    let sock_path = chronicle_dir.join("chronicle.sock");

    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        log_error(chronicle_dir, &format!("Failed to read stdin: {e}"));
        return 0;
    }

    match UnixStream::connect(&sock_path) {
        Ok(mut stream) => {
            if let Err(e) = stream.write_all(input.as_bytes()) {
                log_error(chronicle_dir, &format!("Failed to write to socket: {e}"));
            }
        }
        Err(e) => {
            log_error(
                chronicle_dir,
                &format!("Failed to connect to {}: {e}", sock_path.display()),
            );
        }
    }

    0 // Always exit 0
}

fn log_error(chronicle_dir: &Path, msg: &str) {
    let log_path = chronicle_dir.join("relay.log");

    // Cap at 1 MB: if over, truncate to last 512 KB
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > 1_048_576 {
            if let Ok(contents) = std::fs::read(&log_path) {
                let keep_from = contents.len().saturating_sub(524_288);
                let _ = std::fs::write(&log_path, &contents[keep_from..]);
            }
        }
    }

    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let _ = writeln!(f, "[{timestamp}] {msg}");
    }
}
```

- [ ] **Step 3: Write `src/hooks/installer.rs`**

```rust
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

const HOOK_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "UserPromptSubmit",
    "SessionStart",
    "SessionEnd",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

const CHRONICLE_MARKER: &str = ".chronicle/hooks/";

pub fn install(project_dir: &Path) -> Result<()> {
    let chronicle_dir = project_dir.join(".chronicle");
    let hooks_dir = chronicle_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Write hook scripts — use explicit name mapping to avoid mangling
    let script_names: std::collections::HashMap<&str, &str> = [
        ("PreToolUse", "pre_tool_use"),
        ("PostToolUse", "post_tool_use"),
        ("PostToolUseFailure", "post_tool_use_failure"),
        ("UserPromptSubmit", "user_prompt_submit"),
        ("SessionStart", "session_start"),
        ("SessionEnd", "session_end"),
        ("SubagentStart", "subagent_start"),
        ("SubagentStop", "subagent_stop"),
        ("Stop", "stop"),
    ].into_iter().collect();

    for event in HOOK_EVENTS {
        let base = script_names.get(event).expect("missing script name mapping");
        let script_name = format!("{base}.sh");
        let script_path = hooks_dir.join(&script_name);
        let script = "#!/bin/bash\nchronicle hook-relay\n";
        fs::write(&script_path, script)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
        }
    }

    // Merge into .claude/settings.local.json
    let claude_dir = project_dir.join(".claude");
    fs::create_dir_all(&claude_dir)?;
    let settings_path = claude_dir.join("settings.local.json");

    let mut settings: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let hooks = settings
        .as_object_mut()
        .context("settings is not an object")?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks is not an object")?;

    for event in HOOK_EVENTS {
        let base = script_names.get(event).expect("missing script name mapping");
        let command = format!(".chronicle/hooks/{base}.sh");

        let hook_entry = json!({
            "hooks": [{ "type": "command", "command": command }]
        });

        let arr = hooks
            .entry(*event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("hook event is not an array")?;

        // Remove existing chronicle entries (idempotent)
        arr.retain(|entry| {
            !entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|hooks| {
                    hooks.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| c.contains(CHRONICLE_MARKER))
                    })
                })
                .unwrap_or(false)
        });

        arr.push(hook_entry);
    }

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    // Add .chronicle/ to .gitignore
    let gitignore_path = project_dir.join(".gitignore");
    let gitignore_entry = ".chronicle/";
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)?;
        if !content.lines().any(|l| l.trim() == gitignore_entry) {
            fs::write(&gitignore_path, format!("{content}\n{gitignore_entry}\n"))?;
        }
    } else {
        fs::write(&gitignore_path, format!("{gitignore_entry}\n"))?;
    }

    println!("Chronicle initialized in {}", chronicle_dir.display());
    Ok(())
}

pub fn show(project_dir: &Path) -> Result<()> {
    let settings_path = project_dir.join(".claude/settings.local.json");
    if !settings_path.exists() {
        println!("No chronicle hooks installed.");
        return Ok(());
    }

    let content = fs::read_to_string(&settings_path)?;
    let settings: Value = serde_json::from_str(&content)?;

    if let Some(hooks) = settings.get("hooks").and_then(|h| h.as_object()) {
        println!("Chronicle hooks in {}:", settings_path.display());
        for (event, config) in hooks {
            if let Some(arr) = config.as_array() {
                for entry in arr {
                    if let Some(hooks_arr) = entry.get("hooks").and_then(|h| h.as_array()) {
                        for h in hooks_arr {
                            if let Some(cmd) = h.get("command").and_then(|c| c.as_str()) {
                                if cmd.contains(CHRONICLE_MARKER) {
                                    println!("  {event}: {cmd}");
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        println!("No hooks configured.");
    }

    Ok(())
}

pub fn remove(project_dir: &Path) -> Result<()> {
    let settings_path = project_dir.join(".claude/settings.local.json");
    if !settings_path.exists() {
        println!("No settings file found.");
        return Ok(());
    }

    let content = fs::read_to_string(&settings_path)?;
    let mut settings: Value = serde_json::from_str(&content)?;

    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for (_event, config) in hooks.iter_mut() {
            if let Some(arr) = config.as_array_mut() {
                arr.retain(|entry| {
                    !entry
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|hooks| {
                            hooks.iter().any(|h| {
                                h.get("command")
                                    .and_then(|c| c.as_str())
                                    .is_some_and(|c| c.contains(CHRONICLE_MARKER))
                            })
                        })
                        .unwrap_or(false)
                });
            }
        }

        // Clean up empty event arrays
        let empty_events: Vec<String> = hooks
            .iter()
            .filter(|(_, v)| v.as_array().is_some_and(|a| a.is_empty()))
            .map(|(k, _)| k.clone())
            .collect();
        for key in empty_events {
            hooks.remove(&key);
        }
    }

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    println!("Chronicle hooks removed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_install_creates_hooks_and_settings() {
        let dir = tempdir().unwrap();
        install(dir.path()).unwrap();

        // Hook scripts exist
        assert!(dir.path().join(".chronicle/hooks/pretooluse.sh").exists()
            || dir.path().join(".chronicle/hooks/pre_tool_use.sh").exists());

        // Settings file exists with hooks
        let settings_path = dir.path().join(".claude/settings.local.json");
        assert!(settings_path.exists());
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        assert!(settings.get("hooks").is_some());

        // .gitignore has .chronicle/
        let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".chronicle/"));
    }

    #[test]
    fn test_install_is_idempotent() {
        let dir = tempdir().unwrap();
        install(dir.path()).unwrap();
        install(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(".claude/settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
        // Should have exactly one chronicle entry, not two
        let chronicle_entries: Vec<_> = pre
            .iter()
            .filter(|e| {
                e.get("hooks")
                    .and_then(|h| h.as_array())
                    .is_some_and(|arr| {
                        arr.iter().any(|h| {
                            h.get("command")
                                .and_then(|c| c.as_str())
                                .is_some_and(|c| c.contains(".chronicle/"))
                        })
                    })
            })
            .collect();
        assert_eq!(chronicle_entries.len(), 1);
    }

    #[test]
    fn test_remove_cleans_up() {
        let dir = tempdir().unwrap();
        install(dir.path()).unwrap();
        remove(dir.path()).unwrap();

        let content = fs::read_to_string(dir.path().join(".claude/settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        // All event arrays should be empty or removed
        for (_event, config) in hooks {
            if let Some(arr) = config.as_array() {
                assert!(arr.is_empty());
            }
        }
    }
}
```

- [ ] **Step 4: Add `tempfile` dev dependency to `Cargo.toml`**

Add under `[dev-dependencies]`:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 5: Add `mod hooks;` to `main.rs` and run tests**

```bash
cargo test hooks
```

Expected: all tests PASS

- [ ] **Step 6: Wire up CLI commands in `main.rs`**

Replace the placeholder matches for `Init`, `HooksCommands::Show`, `HooksCommands::Remove`, and `HookRelay`:

```rust
Some(Commands::Init) => {
    let project_dir = std::env::current_dir()?;
    hooks::installer::install(&project_dir)?;
    // TODO: also create DB and start daemon
}
Some(Commands::Hooks { command }) => {
    let project_dir = std::env::current_dir()?;
    match command {
        HooksCommands::Show => hooks::installer::show(&project_dir)?,
        HooksCommands::Remove => hooks::installer::remove(&project_dir)?,
    }
}
Some(Commands::HookRelay) => {
    let project_dir = std::env::current_dir()?;
    let chronicle_dir = project_dir.join(".chronicle");
    std::process::exit(hooks::relay::run(&chronicle_dir));
}
```

- [ ] **Step 7: Run all tests and verify**

```bash
cargo test
```

Expected: all tests PASS

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/hooks/ src/main.rs
git commit -m "feat: hook relay and installer (init/show/remove)"
```

---

## Task 4: Daemon — Socket Listener + Event Processor

**Files:**
- Create: `src/daemon/mod.rs`
- Create: `src/daemon/server.rs`
- Create: `src/daemon/processor.rs`

- [ ] **Step 1: Create `src/daemon/mod.rs`**

```rust
pub mod processor;
pub mod server;
```

- [ ] **Step 2: Write tests for the event processor in `src/daemon/processor.rs`**

```rust
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

use crate::db::models::{Event, HookPayload, Snapshot};
use crate::db::{queries, schema};

/// File-modifying tool names
const FILE_MODIFYING_TOOLS: &[&str] = &["Edit", "Write"];

pub struct EventProcessor {
    conn: Arc<Mutex<Connection>>,
    pending_pre: HashMap<String, (Option<Vec<u8>>, std::time::Instant)>, // tool_use_id -> (content_before, inserted_at)
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
        todo!()
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

        // Create session first
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
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test daemon::processor
```

Expected: FAIL with "not yet implemented"

- [ ] **Step 4: Implement `EventProcessor::process`**

```rust
pub async fn process(&mut self, payload: HookPayload) -> Result<()> {
    let event_type = payload
        .hook_event_name
        .as_deref()
        .unwrap_or("Unknown");
    let session_id = payload.session_id.as_deref().unwrap_or("unknown");
    let now = chrono::Utc::now().timestamp_millis();

    let db = self.conn.lock().await;

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
            queries::upsert_session(&db, &session)?;
        }
        "SessionEnd" => {
            // Update session end time
            db.execute(
                "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
                rusqlite::params![now, session_id],
            )?;
        }
        "PreToolUse" => {
            let tool_name = payload.tool_name.as_deref().unwrap_or("");
            let tool_use_id = payload.tool_use_id.clone();

            // For file-modifying tools, capture "before" state
            if FILE_MODIFYING_TOOLS.contains(&tool_name) {
                if let Some(ref tuid) = tool_use_id {
                    let file_path = self.extract_file_path(&payload);
                    let content_before = file_path
                        .as_ref()
                        .and_then(|p| std::fs::read(p).ok());
                    self.pending_pre.insert(tuid.clone(), (content_before, std::time::Instant::now()));
                }
            }

            // Store the event
            let event = self.payload_to_event(&payload, session_id, now);
            let event_id = queries::insert_event(&db, &event)?;
            let mut stored = event;
            stored.id = event_id;
            let _ = self.broadcast_tx.send(stored);
        }
        "PostToolUse" => {
            let tool_name = payload.tool_name.as_deref().unwrap_or("");
            let tool_use_id = payload.tool_use_id.clone();

            let event = self.payload_to_event(&payload, session_id, now);
            let event_id = queries::insert_event(&db, &event)?;

            // For file-modifying tools, capture snapshot
            if FILE_MODIFYING_TOOLS.contains(&tool_name) {
                if let Some(ref tuid) = tool_use_id {
                    if let Some((content_before, _inserted_at)) = self.pending_pre.remove(tuid) {
                        let file_path = self.extract_file_path(&payload);
                        if let Some(path) = file_path {
                            let content_after = std::fs::read(&path).ok();
                            let diff = self.compute_diff(
                                content_before.as_deref(),
                                content_after.as_deref(),
                            );
                            let snapshot = Snapshot {
                                id: 0,
                                event_id,
                                file_path: path,
                                content_before: content_before
                                    .map(|c| zstd::encode_all(c.as_slice(), 3))
                                    .transpose()?,
                                content_after: content_after
                                    .map(|c| zstd::encode_all(c.as_slice(), 3))
                                    .transpose()?,
                                diff_unified: diff,
                            };
                            queries::insert_snapshot(&db, &snapshot)?;
                        }
                    }
                }
            }

            let mut stored = self.payload_to_event(&payload, session_id, now);
            stored.id = event_id;
            let _ = self.broadcast_tx.send(stored);
        }
        "PostToolUseFailure" => {
            // Discard pending pre entry if it exists
            if let Some(ref tuid) = payload.tool_use_id {
                self.pending_pre.remove(tuid);
            }

            let event = self.payload_to_event(&payload, session_id, now);
            let event_id = queries::insert_event(&db, &event)?;
            let mut stored = event;
            stored.id = event_id;
            let _ = self.broadcast_tx.send(stored);
        }
        _ => {
            // Ensure session exists for any event
            let session = crate::db::models::Session {
                id: session_id.to_string(),
                started_at: now,
                ended_at: None,
                cwd: payload.cwd.unwrap_or_default(),
                model: None,
                permission_mode: None,
            };
            queries::upsert_session(&db, &session)?;

            let event = self.payload_to_event(&payload, session_id, now);
            let event_id = queries::insert_event(&db, &event)?;
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
            .or_else(|| {
                payload.tool_error.as_ref().map(|s| s.as_bytes().to_vec())
            })
            .or_else(|| {
                payload.prompt.as_ref().map(|s| s.as_bytes().to_vec())
            })
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
    diff.unified_diff()
        .header("a", "b")
        .to_string()
}

/// Remove entries older than 10 minutes. Called periodically by the server loop and at session end.
pub fn evict_stale_entries(&mut self) {
    let ttl = std::time::Duration::from_secs(10 * 60);
    let now = std::time::Instant::now();
    self.pending_pre.retain(|_id, (_content, inserted_at)| {
        now.duration_since(*inserted_at) < ttl
    });
}

/// Clear all pending entries (called at session end)
pub fn clear_pending(&mut self) {
    self.pending_pre.clear();
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test daemon::processor
```

Expected: all tests PASS

- [ ] **Step 6: Write `src/daemon/server.rs`**

```rust
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::{broadcast, Mutex};

use crate::db::models::{Event, HookPayload};
use crate::daemon::processor::EventProcessor;

pub async fn run(chronicle_dir: &Path, conn: Arc<Mutex<rusqlite::Connection>>) -> Result<()> {
    let sock_path = chronicle_dir.join("chronicle.sock");

    // Remove stale socket
    if sock_path.exists() {
        std::fs::remove_file(&sock_path)?;
    }

    let listener = UnixListener::bind(&sock_path)?;
    let (broadcast_tx, _) = broadcast::channel::<Event>(1024);
    let mut processor = EventProcessor::new(conn, broadcast_tx.clone());

    // Write PID file
    let pid_path = chronicle_dir.join("daemon.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    // Idle timeout: 30 minutes
    let idle_timeout = std::time::Duration::from_secs(30 * 60);
    let mut last_activity = std::time::Instant::now();
    let mut evict_interval = tokio::time::interval(std::time::Duration::from_secs(60));

    tracing::info!("Chronicle daemon listening on {}", sock_path.display());

    loop {
        tokio::select! {
            _ = evict_interval.tick() => {
                processor.evict_stale_entries();
                if last_activity.elapsed() >= idle_timeout {
                    tracing::info!("Idle timeout reached, shutting down");
                    break;
                }
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((mut stream, _addr)) => {
                        last_activity = std::time::Instant::now();
                        let mut buf = Vec::new();
                        if let Err(e) = stream.read_to_end(&mut buf).await {
                            tracing::warn!("Failed to read from socket: {e}");
                            continue;
                        }
                        match serde_json::from_slice::<HookPayload>(&buf) {
                            Ok(payload) => {
                                if let Err(e) = processor.process(payload).await {
                                    tracing::error!("Failed to process event: {e}");
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Invalid JSON from hook: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Accept error: {e}");
                    }
                }
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&chronicle_dir.join("daemon.pid"));
    processor.clear_pending();

    Ok(())
}
```

- [ ] **Step 7: Add `mod daemon;` to `main.rs`, wire up daemon commands**

```rust
Some(Commands::Daemon { command }) => {
    let project_dir = std::env::current_dir()?;
    let chronicle_dir = project_dir.join(".chronicle");
    match command {
        DaemonCommands::Start => {
            let conn = rusqlite::Connection::open(chronicle_dir.join("chronicle.db"))?;
            db::schema::initialize(&conn)?;
            let conn = std::sync::Arc::new(tokio::sync::Mutex::new(conn));
            daemon::server::run(&chronicle_dir, conn).await?;
        }
        DaemonCommands::Stop => {
            let pid_path = chronicle_dir.join("daemon.pid");
            if pid_path.exists() {
                let pid: i32 = std::fs::read_to_string(&pid_path)?.trim().parse()?;
                // Verify the process exists before signaling
                let exists = unsafe { libc::kill(pid, 0) } == 0;
                if exists {
                    unsafe { libc::kill(pid, libc::SIGTERM); }
                    println!("Daemon stopped (PID {pid}).");
                } else {
                    println!("Daemon process {pid} not found (stale PID file).");
                }
                std::fs::remove_file(&pid_path)?;
            } else {
                println!("No daemon running.");
            }
        }
        DaemonCommands::Status => {
            let pid_path = chronicle_dir.join("daemon.pid");
            if pid_path.exists() {
                let pid = std::fs::read_to_string(&pid_path)?.trim().to_string();
                println!("Daemon running (PID {pid})");
            } else {
                println!("Daemon not running.");
            }
        }
    }
}
```

Add `libc = "0.2"` to `[dependencies]` in `Cargo.toml`.

- [ ] **Step 8: Run all tests**

```bash
cargo test
```

Expected: all tests PASS

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml src/daemon/ src/main.rs
git commit -m "feat: daemon with Unix socket listener and event processor"
```

---

## Task 5: Restore Logic

**Files:**
- Create: `src/restore/mod.rs`

- [ ] **Step 1: Write restore tests**

```rust
use anyhow::Result;
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::db::{models::Snapshot, queries, schema};

pub fn restore_to_event(conn: &Connection, session_id: &str, event_id: i64) -> Result<Vec<RestoreAction>> {
    todo!()
}

#[derive(Debug, PartialEq)]
pub enum RestoreAction {
    Overwrite { path: String },
    Create { path: String },
    Delete { path: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Event, Session};
    use tempfile::tempdir;

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
        ).unwrap();

        conn
    }

    #[test]
    fn test_restore_plan_includes_created_file() {
        let conn = setup_db();

        let eid = queries::insert_event(&conn, &Event {
            id: 0,
            session_id: "s1".into(),
            timestamp: 1001,
            event_type: "PostToolUse".into(),
            tool_name: Some("Write".into()),
            tool_use_id: Some("tu1".into()),
            agent_id: None,
            agent_type: None,
            input_json: None,
            output_json: None,
        }).unwrap();

        queries::insert_snapshot(&conn, &Snapshot {
            id: 0,
            event_id: eid,
            file_path: "/tmp/new_file.rs".into(),
            content_before: None, // newly created
            content_after: Some(zstd::encode_all(b"fn main() {}".as_slice(), 3).unwrap()),
            diff_unified: "+fn main() {}".into(),
        }).unwrap();

        let actions = restore_to_event(&conn, "s1", eid).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], RestoreAction::Create { path } if path == "/tmp/new_file.rs"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test restore
```

Expected: FAIL

- [ ] **Step 3: Implement restore logic**

```rust
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

/// Plan what a restore would do without executing it.
pub fn restore_to_event(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
) -> Result<Vec<RestoreAction>> {
    let snapshots = queries::get_file_states_at_event(conn, session_id, event_id)?;
    let mut actions = Vec::new();

    for snap in &snapshots {
        let path = Path::new(&snap.file_path);
        let file_exists = path.exists();

        match (&snap.content_before, &snap.content_after) {
            (_, Some(_)) => {
                // File should exist at this point (created or modified)
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
            (Some(_), None) => {
                // File was deleted at this point — restore means delete
                if file_exists {
                    actions.push(RestoreAction::Delete {
                        path: snap.file_path.clone(),
                    });
                }
            }
            (None, None) => {
                // No content on either side — nothing to do
            }
        }
    }

    Ok(actions)
}

/// Execute a restore: write all files atomically, creating a RestoreCheckpoint first.
pub fn execute_restore(
    conn: &Connection,
    session_id: &str,
    event_id: i64,
) -> Result<()> {
    let snapshots = queries::get_file_states_at_event(conn, session_id, event_id)?;

    // Create RestoreCheckpoint: capture current state of affected files
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

    // Snapshot current state of each affected file
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

    // Atomic restore: write to temp files, then rename
    let mut temp_files: Vec<(String, std::path::PathBuf)> = Vec::new();

    for snap in &snapshots {
        if let Some(ref compressed) = snap.content_after {
            let content = zstd::decode_all(compressed.as_slice())?;
            let path = Path::new(&snap.file_path);

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let temp_path = path.with_extension("chronicle_tmp");
            std::fs::write(&temp_path, &content)?;
            temp_files.push((snap.file_path.clone(), temp_path));
        } else {
            // content_after is NULL — file should be deleted
            if Path::new(&snap.file_path).exists() {
                std::fs::remove_file(&snap.file_path)
                    .with_context(|| format!("Failed to delete {}", snap.file_path))?;
            }
        }
    }

    // Rename all temp files into place
    for (target, temp) in temp_files {
        std::fs::rename(&temp, &target)
            .with_context(|| format!("Failed to rename {} to {}", temp.display(), target))?;
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
    fn test_restore_plan_includes_created_file() {
        let conn = setup_db();

        let eid = queries::insert_event(
            &conn,
            &Event {
                id: 0,
                session_id: "s1".into(),
                timestamp: 1001,
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

        queries::insert_snapshot(
            &conn,
            &Snapshot {
                id: 0,
                event_id: eid,
                file_path: "/tmp/chronicle_test_new_file.rs".into(),
                content_before: None,
                content_after: Some(
                    zstd::encode_all(b"fn main() {}".as_slice(), 3).unwrap(),
                ),
                diff_unified: "+fn main() {}".into(),
            },
        )
        .unwrap();

        let actions = restore_to_event(&conn, "s1", eid).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            RestoreAction::Create { path } if path == "/tmp/chronicle_test_new_file.rs"
        ));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test restore
```

Expected: PASS

- [ ] **Step 5: Add `mod restore;` to `main.rs` and wire up the CLI**

```rust
Some(Commands::Restore { event_id }) => {
    let project_dir = std::env::current_dir()?;
    let db_path = project_dir.join(".chronicle/chronicle.db");
    let conn = rusqlite::Connection::open(&db_path)?;

    let sessions = db::queries::list_sessions(&conn)?;
    let session_id = sessions
        .first()
        .map(|s| s.id.clone())
        .ok_or_else(|| anyhow::anyhow!("No sessions found"))?;

    let actions = restore::restore_to_event(&conn, &session_id, event_id)?;
    if actions.is_empty() {
        println!("Nothing to restore.");
        return Ok(());
    }

    println!("Restore plan:");
    for action in &actions {
        match action {
            restore::RestoreAction::Overwrite { path } => println!("  OVERWRITE {path}"),
            restore::RestoreAction::Create { path } => println!("  CREATE {path}"),
            restore::RestoreAction::Delete { path } => println!("  DELETE {path}"),
        }
    }
    println!("\nProceed? (y/N)");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim().eq_ignore_ascii_case("y") {
        restore::execute_restore(&conn, &session_id, event_id)?;
        println!("Restore complete. A RestoreCheckpoint was created for undo.");
    } else {
        println!("Aborted.");
    }
}
```

- [ ] **Step 6: Run all tests**

```bash
cargo test
```

Expected: all tests PASS

- [ ] **Step 7: Commit**

```bash
git add src/restore/ src/main.rs
git commit -m "feat: snapshot-at restore with atomic file replacement and safety checkpoints"
```

---

## Task 6: TUI — App Shell + Timeline Panel

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/app.rs`
- Create: `src/tui/timeline.rs`
- Create: `src/tui/detail.rs`
- Create: `src/tui/statusbar.rs`

- [ ] **Step 1: Create `src/tui/mod.rs`**

```rust
pub mod app;
pub mod detail;
pub mod statusbar;
pub mod timeline;
```

- [ ] **Step 2: Write `src/tui/app.rs` — main TUI state and event loop**

```rust
use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyModifiers};
use ratatui::prelude::*;
use rusqlite::Connection;
use std::time::Duration;

use crate::db::models::Event;
use crate::db::queries;
use crate::tui::{detail, statusbar, timeline};

pub struct App {
    pub events: Vec<Event>,
    pub selected_index: usize,
    pub session_id: String,
    pub should_quit: bool,
    pub conn: Connection,
}

impl App {
    pub fn new(conn: Connection, session_id: String) -> Result<Self> {
        let events = queries::list_events_for_session(&conn, &session_id)?;
        let selected_index = events.len().saturating_sub(1);
        Ok(Self {
            events,
            selected_index,
            session_id,
            should_quit: false,
            conn,
        })
    }

    pub fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(100))? {
                if let CrosstermEvent::Key(key) = event::read()? {
                    self.handle_key(key.code, key.modifiers);
                }
            }
        }
        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(frame.area());

        let main_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(layout[0]);

        timeline::render(frame, main_area[0], &self.events, self.selected_index);

        let selected_event = self.events.get(self.selected_index);
        let snapshots = selected_event
            .map(|e| queries::get_snapshots_for_event(&self.conn, e.id).unwrap_or_default())
            .unwrap_or_default();
        detail::render(frame, main_area[1], selected_event, &snapshots);

        statusbar::render(
            frame,
            layout[1],
            &self.session_id,
            self.events.len(),
        );
    }

    fn handle_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index + 1 < self.events.len() {
                    self.selected_index += 1;
                }
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected_index = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.selected_index = self.events.len().saturating_sub(1);
            }
            KeyCode::Char('r') => {
                // Restore to selected event
                if let Some(event) = self.events.get(self.selected_index) {
                    let event_id = event.id;
                    match crate::restore::restore_to_event(
                        &self.conn,
                        &self.session_id,
                        event_id,
                    ) {
                        Ok(actions) if !actions.is_empty() => {
                            // TODO: show confirmation dialog in TUI
                            // For now, execute immediately
                            if let Err(e) = crate::restore::execute_restore(
                                &self.conn,
                                &self.session_id,
                                event_id,
                            ) {
                                tracing::error!("Restore failed: {e}");
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 3: Write `src/tui/timeline.rs`**

```rust
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::db::models::Event;

pub fn render(frame: &mut Frame, area: Rect, events: &[Event], selected: usize) {
    let items: Vec<ListItem> = events
        .iter()
        .map(|e| {
            let icon = match e.tool_name.as_deref() {
                Some("Edit") => "E",
                Some("Write") => "W",
                Some("Bash") => "B",
                Some("Read") => "R",
                Some("Grep") => "G",
                Some("Glob") => "g",
                Some("Agent") => "A",
                _ => match e.event_type.as_str() {
                    "UserPromptSubmit" => ">",
                    "SessionStart" => "^",
                    "SessionEnd" => "$",
                    "SubagentStart" => "+",
                    "SubagentStop" => "-",
                    "Stop" => ".",
                    "RestoreCheckpoint" => "R",
                    _ => "?",
                },
            };

            let tool = e.tool_name.as_deref().unwrap_or(&e.event_type);
            let ts = format_timestamp(e.timestamp);
            let summary = extract_summary(e);

            let style = if is_file_modifying(e) {
                Style::default().fg(Color::Yellow).bold()
            } else {
                Style::default()
            };

            ListItem::new(format!("{ts} [{icon}] {tool} {summary}")).style(style)
        })
        .collect();

    let mut state = ListState::default().with_selected(Some(selected));

    let list = List::new(items)
        .block(Block::default().title(" Timeline ").borders(Borders::ALL))
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol(">> ");

    frame.render_stateful_widget(list, area, &mut state);
}

fn format_timestamp(ts: i64) -> String {
    let secs = ts / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0)
        .unwrap_or_default();
    dt.format("%H:%M:%S").to_string()
}

fn is_file_modifying(event: &Event) -> bool {
    matches!(
        event.tool_name.as_deref(),
        Some("Edit") | Some("Write")
    )
}

fn extract_summary(event: &Event) -> String {
    if let Some(ref input) = event.input_json {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(input) {
            if let Some(path) = val.get("file_path").and_then(|v| v.as_str()) {
                // Show just the filename
                return path.rsplit('/').next().unwrap_or(path).to_string();
            }
            if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
                // Truncate long commands
                let short: String = cmd.chars().take(40).collect();
                return if cmd.len() > 40 {
                    format!("{short}...")
                } else {
                    short
                };
            }
            if let Some(pattern) = val.get("pattern").and_then(|v| v.as_str()) {
                return format!("/{pattern}/");
            }
        }
    }

    if let Some(ref output) = event.output_json {
        let text = String::from_utf8_lossy(output);
        let short: String = text.chars().take(40).collect();
        return if text.len() > 40 {
            format!("{short}...")
        } else {
            short.to_string()
        };
    }

    String::new()
}
```

- [ ] **Step 4: Write `src/tui/detail.rs`**

```rust
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::db::models::{Event, Snapshot};

pub fn render(frame: &mut Frame, area: Rect, event: Option<&Event>, snapshots: &[Snapshot]) {
    let content = match event {
        None => "No event selected".to_string(),
        Some(e) => format_event(e, snapshots),
    };

    let paragraph = Paragraph::new(content)
        .block(Block::default().title(" Detail ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn format_event(event: &Event, snapshots: &[Snapshot]) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Event: {} (id: {})", event.event_type, event.id));
    if let Some(ref tool) = event.tool_name {
        lines.push(format!("Tool: {tool}"));
    }
    if let Some(ref agent) = event.agent_type {
        lines.push(format!("Agent: {agent}"));
    }
    lines.push(String::new());

    // Show input
    if let Some(ref input) = event.input_json {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(input) {
            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                lines.push("Input:".to_string());
                lines.push(pretty);
                lines.push(String::new());
            }
        }
    }

    // Show diff for file-modifying events
    if !snapshots.is_empty() {
        for snap in snapshots {
            lines.push(format!("File: {}", snap.file_path));
            lines.push(snap.diff_unified.clone());
            lines.push(String::new());
        }
    } else if let Some(ref output) = event.output_json {
        // Show output for non-snapshot events
        let text = String::from_utf8_lossy(output);
        lines.push("Output:".to_string());
        lines.push(text.to_string());
    }

    lines.join("\n")
}
```

- [ ] **Step 5: Write `src/tui/statusbar.rs`**

```rust
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub fn render(frame: &mut Frame, area: Rect, session_id: &str, event_count: usize) {
    let status = format!(
        " Session: {} | Events: {} | q:quit  j/k:navigate  r:restore",
        &session_id[..session_id.len().min(12)],
        event_count,
    );

    let bar = Paragraph::new(status)
        .style(Style::default().bg(Color::Blue).fg(Color::White));

    frame.render_widget(bar, area);
}
```

- [ ] **Step 6: Add `mod tui;` to `main.rs`, wire up TUI command**

```rust
None | Some(Commands::Tui) => {
    let project_dir = std::env::current_dir()?;
    let chronicle_dir = project_dir.join(".chronicle");
    let db_path = chronicle_dir.join("chronicle.db");

    if !db_path.exists() {
        anyhow::bail!("No chronicle database found. Run `chronicle init` first.");
    }

    // Start daemon if not running
    let pid_path = chronicle_dir.join("daemon.pid");
    if !pid_path.exists() {
        let daemon_path = std::env::current_exe()?;
        std::process::Command::new(daemon_path)
            .args(["daemon", "start"])
            .current_dir(&project_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }

    let conn = rusqlite::Connection::open(&db_path)?;
    let sessions = db::queries::list_sessions(&conn)?;
    let session_id = sessions
        .first()
        .map(|s| s.id.clone())
        .ok_or_else(|| anyhow::anyhow!("No sessions recorded yet."))?;

    let mut app = tui::app::App::new(conn, session_id)?;

    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = app.run(&mut terminal);

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    result?;
}
```

- [ ] **Step 7: Wire up `Sessions` command**

```rust
Some(Commands::Sessions) => {
    let project_dir = std::env::current_dir()?;
    let db_path = project_dir.join(".chronicle/chronicle.db");

    if !db_path.exists() {
        anyhow::bail!("No chronicle database found. Run `chronicle init` first.");
    }

    let conn = rusqlite::Connection::open(&db_path)?;
    let sessions = db::queries::list_sessions(&conn)?;

    if sessions.is_empty() {
        println!("No sessions recorded yet.");
    } else {
        println!("{:<40} {:<20} {:<8} {}", "Session ID", "Started", "Events", "CWD");
        for s in &sessions {
            let started = chrono::DateTime::from_timestamp(s.started_at / 1000, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "unknown".into());
            let event_count = db::queries::list_events_for_session(&conn, &s.id)
                .map(|e| e.len())
                .unwrap_or(0);
            println!("{:<40} {:<20} {:<8} {}", s.id, started, event_count, s.cwd);
        }
    }
}
```

- [ ] **Step 8: Build and verify**

```bash
cargo build
```

Expected: compiles without errors

- [ ] **Step 9: Commit**

```bash
git add src/tui/ src/main.rs
git commit -m "feat: ratatui TUI with timeline, detail panel, and status bar"
```

---

## Task 7: Init Command (Full Integration)

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Update `Init` to create DB, install hooks, and start daemon**

```rust
Some(Commands::Init) => {
    let project_dir = std::env::current_dir()?;
    let chronicle_dir = project_dir.join(".chronicle");
    std::fs::create_dir_all(&chronicle_dir)?;

    // Create database
    let conn = rusqlite::Connection::open(chronicle_dir.join("chronicle.db"))?;
    db::schema::initialize(&conn)?;
    drop(conn);

    // Install hooks
    hooks::installer::install(&project_dir)?;

    // Start daemon in background
    let daemon_path = std::env::current_exe()?;
    std::process::Command::new(daemon_path)
        .args(["daemon", "start"])
        .current_dir(&project_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    println!("Chronicle initialized. Daemon started.");
    println!("Run `chronicle tui` to open the dashboard.");
}
```

- [ ] **Step 2: Test the full flow manually**

```bash
cargo build
# In a test project:
cd /tmp && mkdir chronicle-test && cd chronicle-test && git init
/path/to/chronicle init
/path/to/chronicle daemon status
/path/to/chronicle hooks show
/path/to/chronicle daemon stop
```

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: full init command (database + hooks + daemon)"
```

---

## Task 8: End-to-End Integration Test

**Files:**
- Create: `tests/integration.rs`

- [ ] **Step 1: Write an integration test**

```rust
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use tempfile::tempdir;

fn chronicle_dir(base: &std::path::Path) -> PathBuf {
    base.join(".chronicle")
}

#[tokio::test]
async fn test_hook_relay_to_daemon_to_db() {
    let dir = tempdir().unwrap();
    let chron_dir = chronicle_dir(dir.path());
    std::fs::create_dir_all(&chron_dir).unwrap();

    // Initialize DB
    let db_path = chron_dir.join("chronicle.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    chronicle::db::schema::initialize(&conn).unwrap();
    drop(conn);

    // Start daemon in a task
    let chron_dir_clone = chron_dir.clone();
    let db_path_clone = db_path.clone();
    let daemon_handle = tokio::spawn(async move {
        let conn = rusqlite::Connection::open(&db_path_clone).unwrap();
        chronicle::db::schema::initialize(&conn).unwrap();
        let conn = std::sync::Arc::new(tokio::sync::Mutex::new(conn));
        chronicle::daemon::server::run(&chron_dir_clone, conn).await
    });

    // Wait for socket to be ready
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Send a SessionStart event
    let payload = serde_json::json!({
        "session_id": "test-session",
        "hook_event_name": "SessionStart",
        "cwd": dir.path().to_str().unwrap(),
        "model": "claude-sonnet",
        "permission_mode": "default"
    });

    let sock_path = chron_dir.join("chronicle.sock");
    let mut stream = UnixStream::connect(&sock_path).unwrap();
    stream
        .write_all(serde_json::to_string(&payload).unwrap().as_bytes())
        .unwrap();
    drop(stream);

    // Send a PostToolUse event
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let payload2 = serde_json::json!({
        "session_id": "test-session",
        "hook_event_name": "PostToolUse",
        "tool_name": "Read",
        "tool_use_id": "tu1",
        "tool_input": { "file_path": "/tmp/test.rs" }
    });
    let mut stream2 = UnixStream::connect(&sock_path).unwrap();
    stream2
        .write_all(serde_json::to_string(&payload2).unwrap().as_bytes())
        .unwrap();
    drop(stream2);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Query the database
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let sessions = chronicle::db::queries::list_sessions(&conn).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "test-session");

    let events = chronicle::db::queries::list_events_for_session(&conn, "test-session").unwrap();
    assert_eq!(events.len(), 1); // PostToolUse (SessionStart doesn't create an event row, it creates a session)
    assert_eq!(events[0].tool_name.as_deref(), Some("Read"));

    // Cleanup: the daemon will timeout, but we can abort it
    daemon_handle.abort();
}
```

- [ ] **Step 2: Create `src/lib.rs` for integration test access**

```rust
pub mod cli;
pub mod db;
pub mod daemon;
pub mod hooks;
pub mod restore;
pub mod tui;
```

Keep `src/main.rs` using `chronicle::` imports:
```rust
use chronicle::cli::{Cli, Commands, DaemonCommands, HooksCommands};
use chronicle::{db, daemon, hooks, restore, tui};
```

- [ ] **Step 3: Run the integration test**

```bash
cargo test --test integration
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add tests/ src/main.rs Cargo.toml
git commit -m "test: end-to-end integration test (hook → daemon → SQLite)"
```

---

## Build Order & Dependencies

```
Task 1 (scaffold)
    ↓
Task 2 (database)
    ├──→ Task 3 (hooks)     ← can run in parallel
    ├──→ Task 4 (daemon)    ← can run in parallel
    └──→ Task 5 (restore)   ← can run in parallel
              ↓
         Task 6 (TUI) ← needs db + restore
              ↓
         Task 7 (init integration) ← needs hooks + db + daemon
              ↓
         Task 8 (e2e test) ← needs everything
```

Tasks 3, 4, 5 can all be worked in parallel after Task 2 completes. Task 6 needs Task 5 (restore module) but not Tasks 3-4. Task 8 validates the full pipeline.

### Deferred to follow-up tasks

- `s` session-switching in TUI
- `f` filter and `/` search in TUI
- Live broadcast mode (TUI subscribes to daemon broadcast channel, auto-scroll, "N new events" indicator)
