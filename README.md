# Chronicle

A Rust CLI that hooks into [Claude Code](https://claude.ai/code) to record every agent action, with a ratatui TUI dashboard for session replay, time-travel debugging, and file restoration.

## What it does

Chronicle captures every tool call Claude Code makes — every file edit, bash command, read, grep, and more — storing them in a local SQLite database with compressed file snapshots. You can then browse the full session timeline, inspect diffs, and restore your codebase to any point in time.

```
Claude Code hooks → chronicle daemon → SQLite → TUI dashboard
```

### Use cases

- **Session replay** — review what an agent did after it's done
- **Undo/rollback** — restore files to any point in the session
- **Post-mortem debugging** — find exactly when something went wrong
- **Real-time monitoring** — watch agent actions as they happen

## Installation

### From source

```bash
git clone https://github.com/your-org/chronicle.git
cd chronicle
cargo build --release
```

The binary will be at `target/release/chronicle`. Add it to your PATH:

```bash
cp target/release/chronicle /usr/local/bin/
# or
export PATH="$PATH:$(pwd)/target/release"
```

### Requirements

- Rust 1.70+ (uses edition 2021)
- macOS or Linux (Unix sockets required)
- Claude Code installed

## Quick start

```bash
# In any project directory where you use Claude Code:
chronicle init
```

This will:
1. Create a `.chronicle/` directory with a SQLite database
2. Install Claude Code hooks in `.claude/settings.local.json`
3. Start the chronicle daemon in the background
4. Add `.chronicle/` to `.gitignore`

Now use Claude Code normally. Every action is recorded automatically.

```bash
# Open the TUI dashboard
chronicle tui
# or just:
chronicle
```

## CLI commands

| Command | Description |
|---------|-------------|
| `chronicle init` | Initialize chronicle in the current project |
| `chronicle` or `chronicle tui` | Launch the TUI dashboard |
| `chronicle sessions` | List all recorded sessions |
| `chronicle restore <event-id>` | Restore files to a specific point in time |
| `chronicle hooks show` | Show installed hook configuration |
| `chronicle hooks remove` | Remove hooks and stop daemon |
| `chronicle daemon start` | Start the daemon manually |
| `chronicle daemon stop` | Stop the daemon |
| `chronicle daemon status` | Check if daemon is running |

## TUI dashboard

The dashboard has three panels:

```
┌─ Timeline (40%) ──────────┬─ Detail (60%) ──────────────────┐
│ 14:23:01 [E] Edit main.rs │ Event: PostToolUse (id: 42)     │
│ 14:23:02 [B] Bash cargo.. │ Tool: Edit                      │
│ 14:23:05 [R] Read lib.rs  │                                 │
│ 14:23:06 [W] Write app.rs │ File: src/main.rs               │
│ 14:23:08 [G] Grep pattern │ --- a                           │
│ 14:23:10 [>] UserPrompt.. │ +++ b                           │
│>>14:23:12 [E] Edit cli.rs │ @@ -1,3 +1,5 @@                │
│ 14:23:15 [B] Bash cargo.. │ +use anyhow::Result;            │
│                            │  fn main() {                    │
├────────────────────────────┴─────────────────────────────────┤
│ Session: abc123def │ Events: 42 │ q:quit j/k:navigate r:rest│
└──────────────────────────────────────────────────────────────┘
```

### Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | Navigate timeline |
| `g` / `G` | Jump to first / last event |
| `r` | Restore files to selected event (with confirmation) |
| `q` / `Esc` | Quit |

### Event icons

| Icon | Tool/Event |
|------|-----------|
| `E` | Edit |
| `W` | Write |
| `B` | Bash |
| `R` | Read |
| `G` | Grep |
| `g` | Glob |
| `A` | Agent (subagent) |
| `>` | User prompt |
| `^` | Session start |
| `$` | Session end |

File-modifying events (Edit, Write) are highlighted in yellow.

## Restore

Chronicle uses the **snapshot-at** model: when you restore to an event, all tracked files are returned to the state they were in at that point in the session.

Restores are:
- **Atomic** — files are written to temp paths first, then renamed into place
- **Reversible** — a `RestoreCheckpoint` event is created before every restore, so you can undo a restore by restoring to the checkpoint
- **Confirmed** — the TUI shows a confirmation dialog listing all files that would change

### CLI restore

```bash
# Show what would happen
chronicle restore 42
# Output:
#   Restore plan:
#     OVERWRITE src/main.rs
#     CREATE src/new_file.rs
#     DELETE src/old_file.rs
#   Proceed? (y/N)
```

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
    └── Broadcast channel (for live TUI updates)
```

### How hooks work

Chronicle installs hooks in `.claude/settings.local.json` (project-local, not shared). Each hook runs a shell script that pipes Claude Code's JSON event data to the daemon via a Unix socket.

Hooks are **best-effort** — they always exit 0 and never block Claude Code. If the daemon is down, events are silently dropped (logged to `.chronicle/relay.log`).

### Captured events

| Event | What's recorded |
|-------|----------------|
| `SessionStart` / `SessionEnd` | Session lifecycle |
| `PreToolUse` / `PostToolUse` | Every tool call with input/output |
| `PostToolUseFailure` | Tool errors |
| `UserPromptSubmit` | User messages |
| `SubagentStart` / `SubagentStop` | Agent spawns |
| `Stop` | Assistant's final message |

### File snapshots

For file-modifying tools (Edit, Write), chronicle captures:
- File content **before** the change (at PreToolUse)
- File content **after** the change (flushed when the next event arrives, or at PostToolUse if delivered)
- A unified diff for display
- Both snapshots are zstd-compressed

### Storage

```
.chronicle/
  chronicle.db    # SQLite database (WAL mode)
  chronicle.sock  # Unix domain socket
  daemon.pid      # Daemon PID file
  relay.log       # Hook relay error log (1 MB max)
  hooks/          # Hook shell scripts
```

### Daemon lifecycle

- Starts automatically with `chronicle init` or `chronicle tui`
- Auto-exits after 30 minutes of inactivity
- Handles SIGTERM for clean shutdown
- PID tracked in `.chronicle/daemon.pid`

## Uninstall

```bash
# Remove hooks and stop daemon
chronicle hooks remove

# Delete chronicle data (optional)
rm -rf .chronicle/
```

## Development

```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- daemon start

# Build release
cargo build --release
```

## License

MIT
