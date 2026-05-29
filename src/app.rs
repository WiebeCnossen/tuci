use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

use crate::fen::Position;
use crate::uci::UciEngine;

const MAX_ENGINE_LINES: usize = 5_000;

pub struct EngineState {
    pub name: String,
    pub lines: Vec<String>,
    pub info: BTreeMap<String, String>,
    pv_first_move: Option<String>,
    engine: Option<UciEngine>,
}

impl EngineState {
    fn new(name: String) -> Self {
        Self {
            name,
            lines: Vec::new(),
            info: BTreeMap::new(),
            pv_first_move: None,
            engine: None,
        }
    }

    fn clear_properties(&mut self) {
        self.info.clear();
        self.pv_first_move = None;
    }

    fn push_lines(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        for line in lines {
            if line.starts_with("info ") && !should_skip_info_properties(line) {
                parse_info_line(line, &mut self.info, &mut self.pv_first_move);
            }
        }
        self.lines.extend_from_slice(lines);
        if self.lines.len() > MAX_ENGINE_LINES {
            let excess = self.lines.len() - MAX_ENGINE_LINES;
            self.lines.drain(..excess);
        }
    }

    /// Most recent wrapped display rows that fit in `height` terminal rows.
    fn visible_display_lines(&self, height: usize, width: usize) -> Vec<String> {
        if self.lines.is_empty() || height == 0 || width == 0 {
            return Vec::new();
        }

        let mut visible = Vec::with_capacity(height);
        for line in self.lines.iter().rev() {
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
}

pub struct App {
    pub position: Position,
    /// Chronological positions; the last entry is always the current position.
    position_history: Vec<Position>,
    pub engines: Vec<EngineState>,
    pub input: String,
    pub status: String,
    pub should_quit: bool,
    pub engine_tile_visible: bool,
}

impl App {
    pub fn new(engine_names: Vec<String>) -> Self {
        let position = Position::default();
        Self {
            position: position.clone(),
            position_history: vec![position],
            engines: engine_names.into_iter().map(EngineState::new).collect(),
            input: String::new(),
            status: "Starting engines…".into(),
            should_quit: false,
            engine_tile_visible: false,
        }
    }

    pub fn attach_engine(&mut self, index: usize, engine: UciEngine) {
        if let Some(slot) = self.engines.get_mut(index) {
            engine.set_position_fen(&self.position.fen);
            slot.engine = Some(engine);
        }
        self.update_ready_status();
    }

    pub fn engines_ready(&self) -> bool {
        !self.engines.is_empty() && self.engines.iter().all(|e| e.engine.is_some())
    }

    pub fn any_engine_ready(&self) -> bool {
        self.engines.iter().any(|e| e.engine.is_some())
    }

    pub fn push_engine_lines(&mut self, index: usize, lines: &[String]) {
        if let Some(slot) = self.engines.get_mut(index) {
            slot.push_lines(lines);
        }
    }

    pub fn visible_engine_display_lines(
        &self,
        index: usize,
        height: usize,
        width: usize,
    ) -> Vec<String> {
        self.engines
            .get(index)
            .map(|slot| slot.visible_display_lines(height, width))
            .unwrap_or_default()
    }

    pub fn submit_input(&mut self) -> Result<()> {
        let raw = std::mem::take(&mut self.input);
        let line = expand_input_shorthand(raw.trim());

        if line.to_ascii_lowercase().starts_with("fen ") {
            let fen = line[4..].trim();
            let position = Position::from_fen(fen)?;
            return self.apply_new_position(position, "");
        }

        if line.eq_ignore_ascii_case("move") {
            let uci_move = self
                .engines
                .first()
                .and_then(|e| e.info.get("bestmove"))
                .ok_or_else(|| {
                    anyhow!(
                        "no bestmove available; specify a move (e.g. move e2e4) or wait for engine output"
                    )
                })?;
            let position = self.position.apply_uci_move(uci_move)?;
            return self.apply_new_position(position, &format!(" (move {uci_move})"));
        }

        if line.len() > 5 && line[..5].eq_ignore_ascii_case("move ") {
            let uci_move = line[5..].trim();
            if uci_move.is_empty() {
                return Err(anyhow!("move requires a UCI move (e.g. move e2e4)"));
            }
            let position = self.position.apply_uci_move(uci_move)?;
            return self.apply_new_position(position, &format!(" (move {uci_move})"));
        }

        if line.eq_ignore_ascii_case("back") || line.to_ascii_lowercase().starts_with("back ") {
            let steps = if line.eq_ignore_ascii_case("back") {
                1
            } else {
                let arg = line[5..].trim();
                arg.parse::<usize>()
                    .map_err(|_| anyhow!("back requires a positive step count (e.g. back 2)"))?
            };
            if steps == 0 {
                return Err(anyhow!("back requires a positive step count"));
            }
            return self.go_back(steps);
        }

        if line.eq_ignore_ascii_case("console show") {
            self.engine_tile_visible = true;
            self.status = "Console shown".into();
            return Ok(());
        }

        if line.eq_ignore_ascii_case("console hide") {
            self.engine_tile_visible = false;
            self.status = "Console hidden".into();
            return Ok(());
        }

        if line.eq_ignore_ascii_case("console") {
            self.engine_tile_visible = !self.engine_tile_visible;
            self.status = if self.engine_tile_visible {
                "Console shown".into()
            } else {
                "Console hidden".into()
            };
            return Ok(());
        }

        if !self.engines_ready() {
            return Err(anyhow!("Engines are still starting"));
        }

        if line.eq_ignore_ascii_case("quit") || line.eq_ignore_ascii_case("exit") {
            self.quit_all_engines();
            self.should_quit = true;
            self.status = "Quitting…".into();
            return Ok(());
        }

        if line.eq_ignore_ascii_case("stop") {
            for slot in &self.engines {
                if let Some(engine) = &slot.engine {
                    engine.stop();
                }
            }
            self.status = "Sent: stop".into();
            return Ok(());
        }

        if line.eq_ignore_ascii_case("go") || line.to_ascii_lowercase().starts_with("go ") {
            let args = line.strip_prefix("go").unwrap_or("").trim();
            for slot in &mut self.engines {
                slot.pv_first_move = None;
                if let Some(engine) = &slot.engine {
                    engine.go(args);
                }
            }
            self.status = if args.is_empty() {
                "Sent: go infinite".into()
            } else {
                format!("Sent: go {args}")
            };
            return Ok(());
        }

        Err(anyhow!(
            "Unknown command. Use FEN, UCI move, fen/move/back/-/console/go/stop/quit (empty = best move)"
        ))
    }

    pub fn quit_all_engines(&self) {
        for slot in &self.engines {
            if let Some(engine) = &slot.engine {
                engine.quit();
            }
        }
    }

    fn update_ready_status(&mut self) {
        let ready = self.engines.iter().filter(|e| e.engine.is_some()).count();
        let total = self.engines.len();
        if ready == total {
            self.status =
                "Ready. Enter FEN, UCI move, or: fen/move/back/-/go/stop/quit (empty = best move)"
                    .into();
        } else {
            self.status = format!("Starting engines… ({ready}/{total} ready)");
        }
    }

    fn clear_all_engine_properties(&mut self) {
        for slot in &mut self.engines {
            slot.clear_properties();
        }
    }

    /// Stop search, discard prior engine info, set position, and start analysis.
    fn apply_new_position(&mut self, position: Position, label: &str) -> Result<()> {
        append_position_history(&mut self.position_history, position.clone());
        self.set_position_on_engines(position, label)
    }

    fn go_back(&mut self, steps: usize) -> Result<()> {
        if !self.engines_ready() {
            return Err(anyhow!("Engines are still starting"));
        }
        let (position, steps) = rewind_position_history(&mut self.position_history, steps)?;
        self.set_position_on_engines(position, &format!(" (back {steps})"))
    }

    fn set_position_on_engines(&mut self, position: Position, label: &str) -> Result<()> {
        let fen = position.fen.clone();
        for slot in &self.engines {
            if let Some(engine) = &slot.engine {
                engine.stop();
            }
        }
        self.clear_all_engine_properties();
        for slot in &self.engines {
            if let Some(engine) = &slot.engine {
                engine.set_position_fen(&fen);
                engine.go("");
            }
        }
        self.position = position;
        self.status = format!("Position updated{label}; sent stop, position, go infinite");
        Ok(())
    }
}

fn append_position_history(history: &mut Vec<Position>, position: Position) {
    history.push(position);
}

fn rewind_position_history(history: &mut Vec<Position>, steps: usize) -> Result<(Position, usize)> {
    let len = history.len();
    if len <= 1 {
        return Err(anyhow!("already at the earliest position"));
    }
    let steps = steps.min(len - 1);
    history.truncate(len - steps);
    let position = history
        .last()
        .expect("history non-empty after truncate")
        .clone();
    Ok((position, steps))
}

/// Expand shorthand input: empty → `move`, UCI-like → `move …`, FEN-like → `fen …`.
fn expand_input_shorthand(line: &str) -> String {
    if line.is_empty() {
        return "move".into();
    }
    if line == "-" {
        return "back".into();
    }
    if is_explicit_command(line) {
        return line.into();
    }
    if looks_like_uci_move(line) {
        return format!("move {line}");
    }
    if looks_like_fen(line) {
        return format!("fen {line}");
    }
    line.into()
}

fn is_explicit_command(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower == "quit"
        || lower == "exit"
        || lower == "stop"
        || lower == "go"
        || lower.starts_with("go ")
        || lower == "move"
        || lower.starts_with("move ")
        || lower == "fen"
        || lower.starts_with("fen ")
        || lower == "back"
        || lower.starts_with("back ")
        || lower == "console"
        || lower == "console show"
        || lower == "console hide"
}

/// UCI coordinate move: `e2e4`, `e7e8q`, etc.
fn looks_like_uci_move(line: &str) -> bool {
    let bytes = line.to_ascii_lowercase().into_bytes();
    if bytes.len() < 4 || bytes.len() > 5 {
        return false;
    }
    let file = |b: u8| (b'a'..=b'h').contains(&b);
    let rank = |b: u8| (b'1'..=b'8').contains(&b);
    file(bytes[0])
        && rank(bytes[1])
        && file(bytes[2])
        && rank(bytes[3])
        && (bytes.len() == 4 || (bytes.len() == 5 && matches!(bytes[4], b'q' | b'r' | b'b' | b'n')))
}

/// At least one FEN field (board placement contains `/`).
fn looks_like_fen(line: &str) -> bool {
    line.split_whitespace()
        .next()
        .is_some_and(|part| part.contains('/'))
}

fn should_skip_info_properties(line: &str) -> bool {
    line.starts_with("info string")
}

fn has_score_bound(tokens: &[&str]) -> bool {
    tokens
        .iter()
        .any(|t| *t == "lowerbound" || *t == "upperbound")
}

/// Split `line` into rows of at most `width` columns (character-based).
pub(crate) fn wrap_line(line: &str, width: usize) -> Vec<String> {
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

/// Update `info` with key/value pairs from a UCI `info` line.
fn parse_info_line(
    line: &str,
    info: &mut BTreeMap<String, String>,
    pv_first_move: &mut Option<String>,
) {
    let Some(rest) = line.strip_prefix("info ") else {
        return;
    };
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let skip_pv_update = has_score_bound(&tokens);
    let mut line_time: Option<String> = None;
    let mut line_pv: Option<String> = None;
    let mut i = 0;
    while i < tokens.len() {
        let key = tokens[i];
        if key == "pv" {
            line_pv = Some(tokens[i + 1..].join(" "));
            break;
        }
        if key == "score" {
            if let Some(value) = parse_score_tokens(&tokens, &mut i) {
                info.insert("score".into(), value);
            }
            continue;
        }
        i += 1;
        if i < tokens.len() {
            let value = tokens[i].to_string();
            if key == "time" {
                line_time = Some(value.clone());
            }
            info.insert(key.into(), value);
            i += 1;
        }
    }

    if let Some(pv) = line_pv {
        if let Some(first_move) = pv.split_whitespace().next() {
            info.insert("bestmove".into(), first_move.into());
            if pv_first_move.as_deref() != Some(first_move) {
                *pv_first_move = Some(first_move.into());
                if let Some(time) = line_time {
                    info.insert("bestmovetime".into(), time);
                }
            }
        }
        if !skip_pv_update {
            info.insert("pv".into(), pv);
        }
    }
}

/// After `score`, parse `cp N`, `mate N`, or `cp N upperbound|lowerbound`.
fn parse_score_tokens(tokens: &[&str], i: &mut usize) -> Option<String> {
    *i += 1;
    if *i >= tokens.len() {
        return None;
    }
    let kind = tokens[*i];
    if kind != "cp" && kind != "mate" {
        return None;
    }
    *i += 1;
    let number = tokens.get(*i)?;
    *i += 1;
    let mut value = format!("{kind} {number}");
    if let Some(bound) = tokens.get(*i)
        && (*bound == "upperbound" || *bound == "lowerbound")
    {
        value.push(' ');
        value.push_str(bound);
        *i += 1;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::new(vec!["Engine".into()])
    }

    #[test]
    fn expand_input_shorthand_examples() {
        assert_eq!(expand_input_shorthand(""), "move");
        assert_eq!(expand_input_shorthand("e2e4"), "move e2e4");
        assert_eq!(expand_input_shorthand("e7e8q"), "move e7e8q");
        assert_eq!(
            expand_input_shorthand("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR"),
            "fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR"
        );
        assert_eq!(expand_input_shorthand("-"), "back");
        assert_eq!(expand_input_shorthand("go depth 10"), "go depth 10");
        assert_eq!(expand_input_shorthand("move d2d4"), "move d2d4");
        assert_eq!(expand_input_shorthand("back 2"), "back 2");
        assert_eq!(
            expand_input_shorthand("fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w"),
            "fen rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w"
        );
    }

    #[test]
    fn looks_like_uci_move_and_fen() {
        assert!(looks_like_uci_move("e2e4"));
        assert!(looks_like_uci_move("E7E8Q"));
        assert!(!looks_like_uci_move("go"));
        assert!(!looks_like_uci_move("e2e"));
        assert!(looks_like_fen("8/8/8/8/4k3/8/4K3/8"));
        assert!(!looks_like_fen("e2e4"));
    }

    #[test]
    fn clear_engine_properties_resets_info_and_pv() {
        let mut app = test_app();
        app.engines[0].info.insert("depth".into(), "10".into());
        app.engines[0].pv_first_move = Some("e2e4".into());
        app.engines[0].clear_properties();
        assert!(app.engines[0].info.is_empty());
        assert!(app.engines[0].pv_first_move.is_none());
    }

    #[test]
    fn wrap_line_splits_long_text() {
        let rows = wrap_line("abcdefghij", 4);
        assert_eq!(rows, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn parse_info_line_extracts_properties() {
        let mut info = BTreeMap::new();
        let mut pv_first_move = None;
        parse_info_line(
            "info depth 12 seldepth 20 multipv 1 score cp 25 nodes 1000 nps 500000 time 500 pv e2e4 e7e5",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("bestmovetime"), Some(&"500".into()));
        assert_eq!(info.get("depth"), Some(&"12".into()));
        assert_eq!(info.get("seldepth"), Some(&"20".into()));
        assert_eq!(info.get("score"), Some(&"cp 25".into()));
        assert_eq!(info.get("bestmove"), Some(&"e2e4".into()));
        assert_eq!(info.get("nodes"), Some(&"1000".into()));
        assert_eq!(info.get("pv"), Some(&"e2e4 e7e5".into()));
    }

    #[test]
    fn push_engine_lines_skips_info_properties_for_info_string_only() {
        let mut app = test_app();
        app.push_engine_lines(
            0,
            &[
                "info string debug msg".into(),
                "info depth 10 score cp 50 upperbound pv e2e4 e7e5".into(),
                "info depth 5 nodes 100".into(),
            ],
        );
        assert_eq!(app.engines[0].lines.len(), 3);
        assert_eq!(app.engines[0].info.get("depth"), Some(&"5".into()));
        assert_eq!(
            app.engines[0].info.get("score"),
            Some(&"cp 50 upperbound".into())
        );
        assert_eq!(app.engines[0].info.get("bestmove"), Some(&"e2e4".into()));
        assert_eq!(app.engines[0].info.get("pv"), None);
    }

    #[test]
    fn parse_info_line_upperbound_updates_score_and_bestmove_not_pv() {
        let mut info = BTreeMap::new();
        let mut pv_first_move = None;
        parse_info_line(
            "info depth 10 score cp 50 upperbound time 100 pv e2e4 e7e5",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("score"), Some(&"cp 50 upperbound".into()));
        assert_eq!(info.get("bestmove"), Some(&"e2e4".into()));
        assert_eq!(info.get("bestmovetime"), Some(&"100".into()));
        assert_eq!(info.get("pv"), None);

        info.clear();
        parse_info_line(
            "info depth 11 score cp 30 pv d2d4 d7d5",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("pv"), Some(&"d2d4 d7d5".into()));
        assert_eq!(info.get("bestmove"), Some(&"d2d4".into()));
    }

    #[test]
    fn parse_info_line_score_mate() {
        let mut info = BTreeMap::new();
        let mut pv_first_move = None;
        parse_info_line(
            "info depth 20 score mate 3 pv e2e4",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("score"), Some(&"mate 3".into()));
    }

    #[test]
    fn parse_info_line_pv_is_rest_of_line() {
        let mut info = BTreeMap::new();
        let mut pv_first_move = None;
        parse_info_line(
            "info depth 1 pv e2e4 e7e5 g1f3",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("depth"), Some(&"1".into()));
        assert_eq!(info.get("pv"), Some(&"e2e4 e7e5 g1f3".into()));
    }

    #[test]
    fn bestmovetime_updates_when_pv_head_changes() {
        let mut info = BTreeMap::new();
        let mut pv_first_move = None;

        parse_info_line(
            "info depth 10 time 100 pv e2e4",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("bestmovetime"), Some(&"100".into()));

        parse_info_line(
            "info depth 11 time 200 pv e2e4 e7e5",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("bestmovetime"), Some(&"100".into()));

        parse_info_line(
            "info depth 12 time 300 pv d2d4",
            &mut info,
            &mut pv_first_move,
        );
        assert_eq!(info.get("bestmovetime"), Some(&"300".into()));
    }

    #[test]
    fn position_history_and_back() {
        let mut history = vec![Position::default()];
        assert_eq!(history.len(), 1);

        let after_e4 = history[0].apply_uci_move("e2e4").unwrap();
        append_position_history(&mut history, after_e4);
        assert_eq!(history.len(), 2);

        let after_e5 = history.last().unwrap().apply_uci_move("e7e5").unwrap();
        append_position_history(&mut history, after_e5);
        assert_eq!(history.len(), 3);
        assert!(history.last().unwrap().fen.contains("4P3/8"));

        let (pos, _) = rewind_position_history(&mut history, 1).unwrap();
        assert_eq!(history.len(), 2);
        assert!(pos.fen.contains("4P3/8"));
        assert!(!pos.fen.contains("4P3/4p3"));

        let (pos, _) = rewind_position_history(&mut history, 1).unwrap();
        assert_eq!(history.len(), 1);
        assert!(
            pos.fen
                .starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
        );

        let after_d4 = pos.apply_uci_move("d2d4").unwrap();
        append_position_history(&mut history, after_d4.clone());
        assert_eq!(history.len(), 2);
        assert_eq!(history.last().unwrap().fen, after_d4.fen);
    }

    #[test]
    fn rewind_position_history_caps_steps() {
        let mut history = vec![Position::default()];
        let after_e4 = history[0].apply_uci_move("e2e4").unwrap();
        append_position_history(&mut history, after_e4);
        let (_, steps) = rewind_position_history(&mut history, 99).unwrap();
        assert_eq!(steps, 1);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn visible_engine_display_lines_shows_wrapped_tail() {
        let mut app = test_app();
        app.push_engine_lines(0, &["short".into(), "0123456789".into()]);
        let visible = app.visible_engine_display_lines(0, 3, 4);
        assert_eq!(visible, vec!["0123", "4567", "89"]);
    }
}
