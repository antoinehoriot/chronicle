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
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_default();
    dt.format("%H:%M:%S").to_string()
}

fn is_file_modifying(event: &Event) -> bool {
    matches!(event.tool_name.as_deref(), Some("Edit") | Some("Write"))
}

fn extract_summary(event: &Event) -> String {
    if let Some(ref input) = event.input_json {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(input) {
            if let Some(path) = val.get("file_path").and_then(|v| v.as_str()) {
                return path.rsplit('/').next().unwrap_or(path).to_string();
            }
            if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
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
