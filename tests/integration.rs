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
    stream.write_all(serde_json::to_string(&payload).unwrap().as_bytes()).unwrap();
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
    stream2.write_all(serde_json::to_string(&payload2).unwrap().as_bytes()).unwrap();
    drop(stream2);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Query the database
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let sessions = chronicle::db::queries::list_sessions(&conn).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "test-session");

    let events = chronicle::db::queries::list_events_for_session(&conn, "test-session").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tool_name.as_deref(), Some("Read"));

    // Cleanup
    daemon_handle.abort();
}
