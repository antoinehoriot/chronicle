use anyhow::Result;
use clap::Parser;
use chronicle::cli::{Cli, Commands, DaemonCommands, HooksCommands};
use chronicle::{daemon, db, hooks, restore, tui};

/// Check if a PID is alive AND belongs to a chronicle process.
fn is_chronicle_process(pid: i32) -> bool {
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    if !alive {
        return false;
    }
    // Verify via `ps` that the process is actually chronicle
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .map(|o| {
            let comm = String::from_utf8_lossy(&o.stdout);
            comm.trim().contains("chronicle")
        })
        .unwrap_or(false)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            let project_dir = std::env::current_dir()?;
            let chronicle_dir = project_dir.join(".chronicle");
            let db_path = chronicle_dir.join("chronicle.db");

            if !db_path.exists() {
                anyhow::bail!("No chronicle database found. Run `chronicle init` first.");
            }

            let pid_path = chronicle_dir.join("daemon.pid");
            let daemon_alive = pid_path.exists() && {
                std::fs::read_to_string(&pid_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .is_some_and(|pid| is_chronicle_process(pid))
            };
            if !daemon_alive {
                let _ = std::fs::remove_file(&pid_path); // clean stale PID
                let daemon_path = std::env::current_exe()?;
                std::process::Command::new(daemon_path)
                    .args(["daemon", "start"])
                    .current_dir(&project_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;
                // Wait for daemon to write sockets (up to 2s)
                let sock_path = chronicle_dir.join("chronicle.sock");
                let live_sock_path = chronicle_dir.join("chronicle-live.sock");
                for _ in 0..20 {
                    if sock_path.exists() && live_sock_path.exists() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }

            let conn = rusqlite::Connection::open(&db_path)?;
            let sessions = db::queries::list_sessions(&conn)?;
            let session_id = sessions
                .first()
                .map(|s| s.id.clone())
                .ok_or_else(|| anyhow::anyhow!("No sessions recorded yet."))?;

            let mut app = tui::app::App::new(conn, session_id, Some(&chronicle_dir))?;

            crossterm::terminal::enable_raw_mode()?;
            let mut stdout = std::io::stdout();
            crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
            let backend = ratatui::backend::CrosstermBackend::new(stdout);
            let mut terminal = ratatui::Terminal::new(backend)?;

            let result = app.run(&mut terminal);

            crossterm::terminal::disable_raw_mode()?;
            crossterm::execute!(
                terminal.backend_mut(),
                crossterm::terminal::LeaveAlternateScreen
            )?;

            result?;
        }
        Some(Commands::Init) => {
            let project_dir = std::env::current_dir()?;
            let chronicle_dir = project_dir.join(".chronicle");
            std::fs::create_dir_all(&chronicle_dir)?;

            let conn = rusqlite::Connection::open(chronicle_dir.join("chronicle.db"))?;
            db::schema::initialize(&conn)?;
            drop(conn);

            hooks::installer::install(&project_dir)?;

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
                println!(
                    "{:<40} {:<20} {:<8} {}",
                    "Session ID", "Started", "Events", "CWD"
                );
                for s in &sessions {
                    let started = chrono::DateTime::from_timestamp(s.started_at / 1000, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "unknown".into());
                    let event_count = db::queries::count_events_for_session(&conn, &s.id)
                        .unwrap_or(0);
                    println!("{:<40} {:<20} {:<8} {}", s.id, started, event_count, s.cwd);
                }
            }
        }
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
                    restore::RestoreAction::Overwrite { path } => {
                        println!("  OVERWRITE {path}")
                    }
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
        Some(Commands::Hooks { command }) => {
            let project_dir = std::env::current_dir()?;
            match command {
                HooksCommands::Show => hooks::installer::show(&project_dir)?,
                HooksCommands::Remove => hooks::installer::remove(&project_dir)?,
            }
        }
        Some(Commands::Daemon { command }) => {
            let project_dir = std::env::current_dir()?;
            let chronicle_dir = project_dir.join(".chronicle");
            match command {
                DaemonCommands::Start => {
                    let conn =
                        rusqlite::Connection::open(chronicle_dir.join("chronicle.db"))?;
                    db::schema::initialize(&conn)?;
                    let conn = std::sync::Arc::new(tokio::sync::Mutex::new(conn));
                    daemon::server::run(&chronicle_dir, conn).await?;
                }
                DaemonCommands::Stop => {
                    let pid_path = chronicle_dir.join("daemon.pid");
                    if pid_path.exists() {
                        let pid: i32 =
                            std::fs::read_to_string(&pid_path)?.trim().parse()?;
                        if is_chronicle_process(pid) {
                            let ret = unsafe { libc::kill(pid, libc::SIGTERM) };
                            if ret == 0 {
                                println!("Daemon stopped (PID {pid}).");
                            } else {
                                let err = std::io::Error::last_os_error();
                                eprintln!("Failed to send SIGTERM to PID {pid}: {err}");
                            }
                        } else {
                            println!(
                                "Daemon process {pid} not found (stale PID file)."
                            );
                        }
                        std::fs::remove_file(&pid_path)?;
                    } else {
                        println!("No daemon running.");
                    }
                }
                DaemonCommands::Status => {
                    let pid_path = chronicle_dir.join("daemon.pid");
                    if pid_path.exists() {
                        let pid =
                            std::fs::read_to_string(&pid_path)?.trim().to_string();
                        println!("Daemon running (PID {pid})");
                    } else {
                        println!("Daemon not running.");
                    }
                }
            }
        }
        Some(Commands::HookRelay) => {
            let project_dir = std::env::current_dir()?;
            let chronicle_dir = project_dir.join(".chronicle");
            std::process::exit(hooks::relay::run(&chronicle_dir));
        }
    }

    Ok(())
}
