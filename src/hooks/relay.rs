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
