use anyhow::{Result, anyhow};

use crate::fen::Position;
use crate::uci::UciEngine;

const MAX_ENGINE_LINES: usize = 5_000;

pub struct App {
    pub position: Position,
    pub engine_lines: Vec<String>,
    pub input: String,
    pub status: String,
    pub should_quit: bool,
    engine: Option<UciEngine>,
}

impl App {
    pub fn new() -> Self {
        Self {
            position: Position::default(),
            engine_lines: Vec::new(),
            input: String::new(),
            status: "Starting engine…".into(),
            should_quit: false,
            engine: None,
        }
    }

    pub fn attach_engine(&mut self, engine: UciEngine) {
        engine.set_position_fen(&self.position.fen);
        self.engine = Some(engine);
        self.status = "Ready. Commands: fen <FEN>, go [args], stop, quit".into();
    }

    pub fn engine_ready(&self) -> bool {
        self.engine.is_some()
    }

    pub fn push_engine_lines(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        self.engine_lines.extend_from_slice(lines);
        if self.engine_lines.len() > MAX_ENGINE_LINES {
            let excess = self.engine_lines.len() - MAX_ENGINE_LINES;
            self.engine_lines.drain(..excess);
        }
    }

    /// Most recent wrapped display rows that fit in `height` terminal rows.
    pub fn visible_engine_display_lines(&self, height: usize, width: usize) -> Vec<String> {
        if self.engine_lines.is_empty() || height == 0 || width == 0 {
            return Vec::new();
        }

        let mut visible = Vec::with_capacity(height);
        for line in self.engine_lines.iter().rev() {
            for row in wrap_line(line, width).into_iter().rev() {
                visible.push(row);
                if visible.len() == height {
                    visible.reverse();
                    return visible;
                }
            }
        }
        visible.reverse();
        visible
    }

    pub fn submit_input(&mut self) -> Result<()> {
        let raw = std::mem::take(&mut self.input);
        let line = raw.trim();
        if line.is_empty() {
            return Ok(());
        }

        let Some(engine) = self.engine.as_ref() else {
            return Err(anyhow!("Engine is still starting"));
        };

        if line.eq_ignore_ascii_case("quit") || line.eq_ignore_ascii_case("exit") {
            engine.quit();
            self.should_quit = true;
            self.status = "Quitting…".into();
            return Ok(());
        }

        if line.eq_ignore_ascii_case("stop") {
            engine.stop();
            self.status = "Sent: stop".into();
            return Ok(());
        }

        if line.eq_ignore_ascii_case("go") || line.to_ascii_lowercase().starts_with("go ") {
            let args = line.strip_prefix("go").unwrap_or("").trim();
            engine.go(args);
            self.status = if args.is_empty() {
                "Sent: go infinite".into()
            } else {
                format!("Sent: go {args}")
            };
            return Ok(());
        }

        if line.to_ascii_lowercase().starts_with("fen ") {
            let fen = line[4..].trim();
            let position = Position::from_fen(fen)?;
            engine.set_position_fen(&position.fen);
            self.position = position;
            self.status = "Position updated".into();
            return Ok(());
        }

        if !line.contains(' ') && line.contains('/') {
            let position = Position::from_fen(line)?;
            engine.set_position_fen(&position.fen);
            self.position = position;
            self.status = "Position updated (bare FEN)".into();
            return Ok(());
        }

        Err(anyhow!(
            "Unknown command. Use: fen <FEN>, go [args], stop, quit"
        ))
    }

    pub fn engine(&self) -> Option<&UciEngine> {
        self.engine.as_ref()
    }
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut rows = Vec::new();
    let mut current = String::new();
    let mut cols = 0usize;
    for ch in line.chars() {
        if cols == width {
            rows.push(std::mem::take(&mut current));
            cols = 0;
        }
        current.push(ch);
        cols += 1;
    }
    rows.push(current);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_line_splits_long_text() {
        let rows = wrap_line("abcdefghij", 4);
        assert_eq!(rows, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn visible_engine_display_lines_shows_wrapped_tail() {
        let mut app = App::new();
        app.push_engine_lines(&["short".into(), "0123456789".into()]);
        let visible = app.visible_engine_display_lines(3, 4);
        assert_eq!(visible, vec!["0123", "4567", "89"]);
    }
}
