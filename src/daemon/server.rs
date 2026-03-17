use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, Mutex};

use crate::daemon::processor::EventProcessor;
use crate::db::models::Event;
use crate::db::models::HookPayload;

pub async fn run(chronicle_dir: &Path, conn: Arc<Mutex<rusqlite::Connection>>) -> Result<()> {
    let sock_path = chronicle_dir.join("chronicle.sock");
    if sock_path.exists() {
        std::fs::remove_file(&sock_path)?;
    }

    let live_sock_path = chronicle_dir.join("chronicle-live.sock");
    if live_sock_path.exists() {
        std::fs::remove_file(&live_sock_path)?;
    }

    let listener = UnixListener::bind(&sock_path)?;
    let live_listener = UnixListener::bind(&live_sock_path)?;
    let (broadcast_tx, _) = broadcast::channel::<Event>(1024);
    let mut processor = EventProcessor::new(conn, broadcast_tx.clone());

    let pid_path = chronicle_dir.join("daemon.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let idle_timeout = std::time::Duration::from_secs(30 * 60);
    let mut last_activity = std::time::Instant::now();
    let mut evict_interval = tokio::time::interval(std::time::Duration::from_secs(60));

    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    tracing::info!("Chronicle daemon listening on {}", sock_path.display());

    loop {
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received, shutting down");
                break;
            }
            _ = evict_interval.tick() => {
                processor.evict_stale_entries();
                if last_activity.elapsed() >= idle_timeout {
                    tracing::info!("Idle timeout reached, shutting down");
                    break;
                }
            }
            accept_result = live_listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let mut rx = broadcast_tx.subscribe();
                        tokio::spawn(async move {
                            let (_, mut writer) = tokio::io::split(stream);
                            loop {
                                match rx.recv().await {
                                    Ok(event) => {
                                        let mut data = match serde_json::to_vec(&event) {
                                            Ok(d) => d,
                                            Err(_) => continue,
                                        };
                                        data.push(b'\n');
                                        if writer.write_all(&data).await.is_err() {
                                            break; // client disconnected
                                        }
                                    }
                                    Err(broadcast::error::RecvError::Lagged(n)) => {
                                        tracing::warn!("Live subscriber lagged by {n} events");
                                    }
                                    Err(broadcast::error::RecvError::Closed) => break,
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Live accept error: {e}");
                    }
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

    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&live_sock_path);
    let _ = std::fs::remove_file(&chronicle_dir.join("daemon.pid"));
    processor.clear_pending();

    Ok(())
}
