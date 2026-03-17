use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyModifiers};
use ratatui::prelude::*;
use rusqlite::Connection;
use std::io::BufRead;
use std::path::Path;
use std::time::Duration;

use crate::db::models::Event;
use crate::db::queries;
use crate::tui::{detail, statusbar, timeline};

pub struct App {
    pub events: Vec<Event>,
    pub selected_index: usize,
    pub session_id: String,
    pub should_quit: bool,
    pub conn: Connection,
    pub confirm_restore: Option<(i64, Vec<crate::restore::RestoreAction>)>,
    pub status_message: Option<String>,
    live_rx: Option<std::sync::mpsc::Receiver<Event>>,
    new_events_count: usize,
}

impl App {
    pub fn new(conn: Connection, session_id: String, chronicle_dir: Option<&Path>) -> Result<Self> {
        let events = queries::list_events_for_session(&conn, &session_id)?;
        let selected_index = events.len().saturating_sub(1);
        let live_rx = chronicle_dir.and_then(|dir| Self::start_live_reader(dir, &session_id));
        Ok(Self {
            events,
            selected_index,
            session_id,
            should_quit: false,
            conn,
            confirm_restore: None,
            status_message: None,
            live_rx,
            new_events_count: 0,
        })
    }

    fn start_live_reader(
        chronicle_dir: &Path,
        session_id: &str,
    ) -> Option<std::sync::mpsc::Receiver<Event>> {
        let sock_path = chronicle_dir.join("chronicle-live.sock");
        let stream = std::os::unix::net::UnixStream::connect(&sock_path).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .ok()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let filter_session = session_id.to_string();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stream);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                };
                if let Ok(event) = serde_json::from_str::<Event>(&line) {
                    if event.session_id == filter_session {
                        if tx.send(event).is_err() {
                            break; // receiver dropped
                        }
                    }
                }
            }
        });
        Some(rx)
    }

    fn drain_live_events(&mut self) {
        let rx = match &self.live_rx {
            Some(rx) => rx,
            None => return,
        };
        let at_bottom = self.selected_index + 1 >= self.events.len() || self.events.is_empty();
        let mut received = false;
        while let Ok(event) = rx.try_recv() {
            self.events.push(event);
            received = true;
        }
        if received {
            if at_bottom {
                self.selected_index = self.events.len().saturating_sub(1);
            } else {
                self.new_events_count = self.events.len().saturating_sub(self.selected_index + 1);
            }
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;
            if event::poll(Duration::from_millis(100))? {
                if let CrosstermEvent::Key(key) = event::read()? {
                    self.handle_key(key.code, key.modifiers);
                }
            }
            self.drain_live_events();
        }
        Ok(())
    }

    fn render(&self, frame: &mut Frame) {
        let status_height = if self.status_message.is_some() { 2 } else { 1 };
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(status_height)])
            .split(frame.area());

        // If confirmation dialog is active, show it in the detail panel
        if let Some((_event_id, ref actions)) = self.confirm_restore {
            let main_area = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(layout[0]);

            timeline::render(frame, main_area[0], &self.events, self.selected_index);

            let mut lines = vec!["Restore to this point? (y/n)".to_string(), String::new()];
            for action in actions {
                match action {
                    crate::restore::RestoreAction::Overwrite { path } => {
                        lines.push(format!("  OVERWRITE {path}"));
                    }
                    crate::restore::RestoreAction::Create { path } => {
                        lines.push(format!("  CREATE    {path}"));
                    }
                    crate::restore::RestoreAction::Delete { path } => {
                        lines.push(format!("  DELETE    {path}"));
                    }
                }
            }
            let dialog = ratatui::widgets::Paragraph::new(lines.join("\n"))
                .block(
                    ratatui::widgets::Block::default()
                        .title(" Confirm Restore ")
                        .borders(ratatui::widgets::Borders::ALL),
                )
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(dialog, main_area[1]);
        } else {
            let main_area = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(layout[0]);

            timeline::render(frame, main_area[0], &self.events, self.selected_index);

            let selected_event = self.events.get(self.selected_index);
            let snapshots = selected_event
                .map(|e| {
                    queries::get_snapshots_for_event(&self.conn, e.id).unwrap_or_default()
                })
                .unwrap_or_default();
            detail::render(frame, main_area[1], selected_event, &snapshots);
        }

        statusbar::render(
            frame,
            layout[1],
            &self.session_id,
            self.events.len(),
            self.status_message.as_deref(),
            self.new_events_count,
        );
    }

    fn handle_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) {
        self.status_message = None;

        // Handle confirmation dialog first
        if let Some((event_id, _)) = self.confirm_restore.take() {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    match crate::restore::execute_restore(
                        &self.conn,
                        &self.session_id,
                        event_id,
                    ) {
                        Ok(()) => {
                            self.status_message =
                                Some("Restored. RestoreCheckpoint created.".into());
                            // Refresh events to show the checkpoint
                            if let Ok(events) =
                                queries::list_events_for_session(&self.conn, &self.session_id)
                            {
                                self.events = events;
                                self.selected_index =
                                    self.events.len().saturating_sub(1);
                            }
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Restore failed: {e}"));
                        }
                    }
                }
                _ => {
                    self.status_message = Some("Restore cancelled.".into());
                }
            }
            return;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index + 1 < self.events.len() {
                    self.selected_index += 1;
                    if self.selected_index + 1 >= self.events.len() {
                        self.new_events_count = 0;
                    }
                }
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected_index = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.selected_index = self.events.len().saturating_sub(1);
                self.new_events_count = 0;
            }
            KeyCode::Char('r') => {
                if let Some(event) = self.events.get(self.selected_index) {
                    let event_id = event.id;
                    match crate::restore::restore_to_event(
                        &self.conn,
                        &self.session_id,
                        event_id,
                    ) {
                        Ok(actions) if !actions.is_empty() => {
                            self.confirm_restore = Some((event_id, actions));
                        }
                        Ok(_) => {
                            self.status_message = Some("Nothing to restore.".into());
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Restore plan failed: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
