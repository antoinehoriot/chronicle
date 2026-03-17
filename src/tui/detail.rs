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

    if let Some(ref input) = event.input_json {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(input) {
            if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                lines.push("Input:".to_string());
                lines.push(pretty);
                lines.push(String::new());
            }
        }
    }

    if !snapshots.is_empty() {
        for snap in snapshots {
            lines.push(format!("File: {}", snap.file_path));
            lines.push(snap.diff_unified.clone());
            lines.push(String::new());
        }
    } else if let Some(ref output) = event.output_json {
        let text = String::from_utf8_lossy(output);
        lines.push("Output:".to_string());
        lines.push(text.to_string());
    }

    lines.join("\n")
}
