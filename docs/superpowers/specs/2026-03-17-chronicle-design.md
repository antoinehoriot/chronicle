# Chronicle — Design Spec

A Rust CLI that hooks into Claude Code to record every agent action, with a ratatui TUI dashboard for browsing session history and restoring file states.

## Use Cases (by priority)

1. **Session replay / audit** — review a full session after it's done
2. **Undo/rollback** — restore the codebase to any point in the session
3. **Post-mortem debugging** — find exactly when and what an agent changed
4. **Real-time supervision** — watch agent actions live

## Architecture

```
Claude Code hooks (shell scripts)
    │
    │  JSON via Unix socket
    ▼
chronicle daemon (tokio, background process)
    │
    ├── Event processor (parse, enrich, store)
    │       │
    │       ▼
    │   SQLite (WAL mode) ── .chronicle/chronicle.db
    │
    └── Broadcast channel (tokio::sync::broadcast)
            │
            ▼
        TUI subscriber (real-time updates)
```

### Data flow

1. Claude Code fires a hook event (PreToolUse, PostToolUse, etc.)
2. Hook shell script pipes JSON to Unix socket at `.chronicle/chronicle.sock`
3. Daemon deserializes, enriches (file snapshots for modifying tools), writes to SQLite
4. Daemon broadcasts event to any connected TUI clients

## CLI Commands

| Command | Description |
|---------|-------------|
| `chronicle init` | Create `.chronicle/`, install hooks, start daemon |
| `chronicle tui` (or just `chronicle`) | Launch ratatui dashboard |
| `chronicle sessions` | List recorded sessions |
| `chronicle restore <event-id>` | Restore all tracked files to their state at that event (same snapshot-at model as TUI `r` key) |
| `chronicle hooks show` | Print installed hook config |
| `chronicle hooks remove` | Remove hooks, stop daemon |
| `chronicle daemon start\|stop\|status` | Manage daemon lifecycle |

## Data Model (SQLite)

### sessions

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | session_id from Claude Code |
| started_at | INTEGER | Unix timestamp |
| ended_at | INTEGER | Unix timestamp, nullable |
| cwd | TEXT | Working directory |
| model | TEXT | Model used |
| permission_mode | TEXT | Permission mode |

### events

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| session_id | TEXT FK | References sessions.id |
| timestamp | INTEGER | Unix timestamp (ms) |
| event_type | TEXT | PreToolUse, PostToolUse, UserPromptSubmit, etc. |
| tool_name | TEXT | Bash, Edit, Write, Read, etc. (nullable) |
| tool_use_id | TEXT | Pairs Pre/Post events (nullable) |
| agent_id | TEXT | Subagent identifier (nullable) |
| agent_type | TEXT | Subagent type (nullable) |
| input_json | BLOB | Raw tool_input JSON |
| output_json | BLOB | Raw tool_response or tool_error JSON (nullable) |

### snapshots

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| event_id | INTEGER FK | References events.id |
| file_path | TEXT | Absolute file path |
| content_before | BLOB | zstd-compressed file content before change (NULL = file was newly created) |
| content_after | BLOB | zstd-compressed file content after change (NULL = file was deleted) |
| diff_unified | TEXT | Cached unified diff for TUI display (always populated — NULL content treated as empty bytes for diff computation) |

### Indexes

- `events(session_id, timestamp)` — timeline queries
- `events(tool_use_id)` — Pre/Post pairing
- `snapshots(event_id)` — snapshot lookup
- `snapshots(file_path, event_id)` — file history queries

### Schema versioning

A `schema_version` table tracks the current schema version. Chronicle checks this on startup and runs migrations forward as needed.

## Daemon Design

### Socket listener

Accepts connections on `.chronicle/chronicle.sock`. Each connection sends a single JSON payload (one hook event), then disconnects. The daemon deserializes, validates, and pushes to an internal `tokio::mpsc` channel.

### Event processor

Receives from the channel and processes events:

- **PreToolUse for file-modifying tools** (Edit, Write): reads current file content (or records NULL if file doesn't exist), holds in `HashMap<tool_use_id, Vec<u8>>` as "before" state
- **PostToolUse for same tool_use_id**: reads file again, computes unified diff via `similar`, writes event + snapshot to SQLite in one transaction. Removes the entry from the pending map.
- **PostToolUseFailure**: if `tool_use_id` has a pending "before" entry, discard it (failed tool did not modify the file)
- **Unmatched Pre entries**: evicted after 10 minutes or at session end to prevent memory leaks
- **All other events**: write directly to SQLite

Note: `tool_use_id` is expected to be non-null for all tool events from Claude Code. If null, file-modifying events are stored without snapshots and logged as warnings.

### Broadcast

Every written event is sent to a `tokio::sync::broadcast` channel. Connected TUI clients subscribe for live updates.

### Lifecycle

- `chronicle init` / `chronicle daemon start` spawns daemon, writes PID to `.chronicle/daemon.pid`
- Daemon auto-exits after 30 minutes of idle (no socket activity, including hook-relay connections) to avoid orphan processes. The idle timer resets on any socket connection, not just successfully processed events.
- `chronicle daemon stop` sends SIGTERM via PID file
- `chronicle tui` starts daemon if not running, then connects

## Hook Installation

### Events captured

| Hook Event | What it captures |
|------------|-----------------|
| SessionStart | Session lifecycle, model info |
| SessionEnd | Session termination |
| PreToolUse | Tool name + input (all tools) |
| PostToolUse | Tool output (all tools) |
| PostToolUseFailure | Tool errors |
| UserPromptSubmit | User messages |
| SubagentStart | Agent spawn events |
| SubagentStop | Agent completion |
| Stop | Last assistant message |

### Hook relay

Chronicle provides a built-in `chronicle hook-relay` subcommand that reads JSON from stdin and forwards to the Unix socket, avoiding a `socat` dependency.

`hook-relay` always exits 0 — recording is best-effort and must never block agent operation. Connection failures are logged to `.chronicle/relay.log` (capped at 1 MB, oldest entries truncated on rotation).

Hook scripts in `.chronicle/hooks/`:

```bash
#!/bin/bash
chronicle hook-relay
```

### Settings integration

`chronicle init` reads `.claude/settings.local.json`, merges chronicle hooks into any existing hook arrays, and writes back. Running init multiple times is idempotent (does not duplicate entries). Existing non-chronicle hooks are preserved.

```json
{
  "hooks": {
    "PreToolUse": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/pre_tool_use.sh" }] }],
    "PostToolUse": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/post_tool_use.sh" }] }],
    "PostToolUseFailure": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/post_tool_use_failure.sh" }] }],
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/user_prompt_submit.sh" }] }],
    "SessionStart": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/session_start.sh" }] }],
    "SessionEnd": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/session_end.sh" }] }],
    "SubagentStart": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/subagent_start.sh" }] }],
    "SubagentStop": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/subagent_stop.sh" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": ".chronicle/hooks/stop.sh" }] }]
  }
}
```

### Cleanup

`chronicle hooks remove` removes chronicle entries from settings, stops daemon, and optionally deletes `.chronicle/`.

`chronicle init` also adds `.chronicle/` to `.gitignore`.

## TUI Layout

### Left panel — Timeline (40% width)

- Vertical list of events, newest at bottom
- Each row: `[timestamp] [icon] [tool_name] [brief summary]`
- Icons by type: `E` Edit, `W` Write, `B` Bash, `R` Read, `G` Grep, `>` UserPrompt, `A` Agent
- File-modifying events highlighted (bold/color)
- Filter bar at top: filter by tool type, file path, session
- `j/k` or arrows to navigate, `Enter` to select

### Right panel — Detail view (60% width)

- **File-modifying events**: unified diff with syntax highlighting (green/red)
- **Bash events**: command + output
- **Read/Grep/Glob**: query + results summary
- **UserPrompt**: prompt text
- **Agent events**: agent type + task description

### Bottom bar — Status & commands

- Current session info, event count, daemon status
- Keybindings: `q` quit, `f` filter, `s` switch session, `r` restore, `/` search

### Restore flow

User presses `r` on an event. Chronicle uses the **snapshot-at** model: it reconstructs the state of every tracked file as it was at that point in time. The confirmation dialog lists all files that would be modified and whether each would be restored, created, or deleted.

Restore is atomic: all files are written to temporary paths first, then renamed into place. If interrupted, no partial state is left.

Before restoring, chronicle records the current state of all affected files as a synthetic `RestoreCheckpoint` event in the `events` + `snapshots` tables. This checkpoint appears in the TUI timeline and can itself be selected for a subsequent restore (undo the undo).

### Live mode

When the daemon broadcasts new events, the timeline auto-scrolls if user is at the bottom (tail -f behavior). Otherwise shows a "N new events" indicator.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime, Unix socket, channels |
| `rusqlite` | SQLite with WAL mode |
| `ratatui` + `crossterm` | TUI rendering |
| `serde` + `serde_json` | JSON serialization |
| `similar` | Diff computation |
| `zstd` | Snapshot compression |
| `clap` | CLI argument parsing |

## Granularity

Every tool call is a discrete, rewindable point in the timeline. This is the maximum granularity available from Claude Code hooks.

## Storage

All data stored in `.chronicle/` at the project root:

```
.chronicle/
  chronicle.db        # SQLite database
  chronicle.sock      # Unix domain socket
  daemon.pid          # Daemon PID file
  relay.log           # Hook relay error log (1 MB max)
  hooks/              # Hook shell scripts
```
