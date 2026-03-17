use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    session_id: &str,
    event_count: usize,
    message: Option<&str>,
    new_events_count: usize,
) {
    let new_indicator = if new_events_count > 0 {
        format!(" | {} new events ↓", new_events_count)
    } else {
        String::new()
    };

    let status = if let Some(msg) = message {
        format!(
            " {} | Session: {} | Events: {}{}",
            msg,
            &session_id[..session_id.len().min(12)],
            event_count,
            new_indicator,
        )
    } else {
        format!(
            " Session: {} | Events: {} | q:quit  j/k:navigate  r:restore{}",
            &session_id[..session_id.len().min(12)],
            event_count,
            new_indicator,
        )
    };

    let bar = Paragraph::new(status)
        .style(Style::default().bg(Color::Blue).fg(Color::White));

    frame.render_widget(bar, area);
}
