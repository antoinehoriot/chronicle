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
