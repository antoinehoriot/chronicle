use anyhow::{Context, Result};
use serde_json::{json, Value};
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
    ]
    .into_iter()
    .collect();

    for event in HOOK_EVENTS {
        let base = script_names
            .get(event)
            .expect("missing script name mapping");
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
        let base = script_names
            .get(event)
            .expect("missing script name mapping");
        let command = format!(".chronicle/hooks/{base}.sh");
        let hook_entry = json!({ "hooks": [{ "type": "command", "command": command }] });

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

/// Check if a PID is alive AND belongs to a chronicle process.
fn is_chronicle_process(pid: i32) -> bool {
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    if !alive {
        return false;
    }
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .map(|o| {
            let comm = String::from_utf8_lossy(&o.stdout);
            comm.trim().contains("chronicle")
        })
        .unwrap_or(false)
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

    // Stop daemon if running
    let pid_path = project_dir.join(".chronicle/daemon.pid");
    if pid_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                if is_chronicle_process(pid) {
                    let ret = unsafe { libc::kill(pid, libc::SIGTERM) };
                    if ret == 0 {
                        println!("Daemon stopped (PID {pid}).");
                    } else {
                        let err = std::io::Error::last_os_error();
                        eprintln!("Failed to stop daemon (PID {pid}): {err}");
                    }
                }
            }
        }
        let _ = fs::remove_file(&pid_path);
    }

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
        assert!(dir.path().join(".chronicle/hooks/pre_tool_use.sh").exists());
        let settings_path = dir.path().join(".claude/settings.local.json");
        assert!(settings_path.exists());
        let content = fs::read_to_string(&settings_path).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        assert!(settings.get("hooks").is_some());
        let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains(".chronicle/"));
    }

    #[test]
    fn test_install_is_idempotent() {
        let dir = tempdir().unwrap();
        install(dir.path()).unwrap();
        install(dir.path()).unwrap();
        let content =
            fs::read_to_string(dir.path().join(".claude/settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        let pre = settings["hooks"]["PreToolUse"].as_array().unwrap();
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
        let content =
            fs::read_to_string(dir.path().join(".claude/settings.local.json")).unwrap();
        let settings: Value = serde_json::from_str(&content).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        for (_event, config) in hooks {
            if let Some(arr) = config.as_array() {
                assert!(arr.is_empty());
            }
        }
    }
}
