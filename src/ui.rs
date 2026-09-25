use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::app::{App, wrap_line};
use crate::fen::{PieceColor, piece_glyph};

const SI_PREFIXES: [&str; 8] = ["", "k", "M", "G", "T", "P", "E", "Z"];
const ENGINE_INNER_LINES: u16 = 8;
const ENGINE_TILE_HEIGHT: u16 = ENGINE_INNER_LINES + 2;

/// Format an integer with at most 3 significant digits and SI suffixes (k, M, G, …).
fn format_si_number(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let n = n.unsigned_abs();

    if n < 1000 {
        return format!("{sign}{n}");
    }

    let mut tier = 0usize;
    let mut divisor = 1u64;
    while n >= divisor.saturating_mul(1000) && tier + 1 < SI_PREFIXES.len() {
        divisor = divisor.saturating_mul(1000);
        tier += 1;
    }

    let mut mantissa = n as f64 / divisor as f64;
    let order = mantissa.log10().floor();
    let scale = 10f64.powf(2.0 - order);
    mantissa = (mantissa * scale).round() / scale;

    if mantissa >= 1000.0 && tier + 1 < SI_PREFIXES.len() {
        tier += 1;
        mantissa /= 1000.0;
    }

    let int_digits = if mantissa >= 100.0 {
        3
    } else if mantissa >= 10.0 {
        2
    } else {
        1
    };
    let frac_digits = 3 - int_digits;
    let multiplier = 10u64.pow(frac_digits);
    let scaled = (mantissa * multiplier as f64).round() as u64;
    let int_part = scaled / multiplier;
    let mut frac_part = scaled % multiplier;

    let prefix = SI_PREFIXES[tier];
    if frac_part == 0 {
        format!("{sign}{int_part}{prefix}")
    } else {
        while frac_part > 0 && frac_part.is_multiple_of(10) {
            frac_part /= 10;
        }
        format!("{sign}{int_part}{prefix}{frac_part}")
    }
}

fn format_property_value(key: &str, value: &str) -> String {
    match key {
        "score" => format_score_human(value),
        "wdl" => format_wdl_human(value),
        "time" | "bestmovetime" => format_time_human(value),
        _ => value
            .parse::<i64>()
            .map(format_si_number)
            .unwrap_or_else(|_| value.to_string()),
    }
}

fn format_score_human(value: &str) -> String {
    let mut parts = value.split_whitespace();
    let kind = parts.next().unwrap_or("");
    let number = parts.next().unwrap_or("");
    let bound = parts.next();
    let formatted = match kind {
        "cp" => number
            .parse::<i64>()
            .map(|cp| {
                let pawns = cp as f64 / 100.0;
                if pawns > 0.0 {
                    format!("+{pawns:.2}")
                } else if pawns == 0.0 {
                    "0.00".into()
                } else {
                    format!("{pawns:.2}")
                }
            })
            .unwrap_or_else(|_| value.to_string()),
        "mate" => number
            .parse::<i64>()
            .map(|n| {
                if n > 0 {
                    format!("mate in {n}")
                } else if n < 0 {
                    format!("mated in {}", n.unsigned_abs())
                } else {
                    "mate".into()
                }
            })
            .unwrap_or_else(|_| value.to_string()),
        _ => value.to_string(),
    };
    match bound {
        Some("upperbound") => format!("≤ {formatted}"),
        Some("lowerbound") => format!("≥ {formatted}"),
        _ => formatted,
    }
}

fn format_wdl_human(value: &str) -> String {
    let parts: Vec<_> = value.split_whitespace().collect();
    if parts.len() != 3 {
        return value.to_string();
    }
    let Ok(w) = parts[0].parse::<i64>() else {
        return value.to_string();
    };
    let Ok(d) = parts[1].parse::<i64>() else {
        return value.to_string();
    };
    let Ok(l) = parts[2].parse::<i64>() else {
        return value.to_string();
    };
    // UCI WDL values are typically permille.
    format!(
        "W {:.1}%  D {:.1}%  L {:.1}%",
        w as f64 / 10.0,
        d as f64 / 10.0,
        l as f64 / 10.0
    )
}

fn format_time_human(value: &str) -> String {
    let Ok(ms) = value.parse::<i64>() else {
        return value.to_string();
    };
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", ms as f64 / 1000.0)
    }
}

fn human_global_key(key: &str) -> &str {
    match key {
        "bestmove" => "best move",
        "bestmovetime" => "best move time",
        "hashfull" => "hash full",
        "tbhits" => "tablebase hits",
        "cpuload" => "CPU load",
        other => other,
    }
}

/// Global search stats first (preferred order), then each PV as its own block.
fn engine_property_lines(
    engine: &crate::app::EngineState,
    position: &crate::fen::Position,
) -> Vec<String> {
    let mut lines = Vec::new();

    const GLOBAL_ORDER: &[&str] = &[
        "bestmove",
        "bestmovetime",
        "nodes",
        "nps",
        "time",
        "hashfull",
        "tbhits",
        "cpuload",
    ];

    let mut remaining: BTreeMap<&str, &str> = engine
        .info
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    for key in GLOBAL_ORDER {
        if let Some(value) = remaining.remove(key) {
            lines.push(format!(
                "{}: {}",
                human_global_key(key),
                format_property_value(key, value)
            ));
        }
    }
    for (key, value) in remaining {
        lines.push(format!(
            "{}: {}",
            human_global_key(key),
            format_property_value(key, value)
        ));
    }

    let black_to_move = position.black_to_move();
    let fullmove = position.fullmove_number();
    for (index, pv) in &engine.pvs {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.extend(format_pv_block(*index, pv, black_to_move, fullmove));
    }

    lines
}

/// Number a UCI PV like `1.e2e4 e7e5 2.g1f3` or `1...e7e5 2.g1f3`.
fn format_pv_with_move_numbers(pv: &str, black_to_move: bool, mut fullmove: u32) -> String {
    let moves: Vec<&str> = pv.split_whitespace().collect();
    if moves.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut black = black_to_move;
    for (i, mv) in moves.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if !black {
            out.push_str(&format!("{fullmove}."));
        } else if i == 0 {
            out.push_str(&format!("{fullmove}..."));
        }
        out.push_str(mv);
        if black {
            fullmove = fullmove.saturating_add(1);
        }
        black = !black;
    }
    out
}

fn format_pv_block(
    index: u32,
    pv: &BTreeMap<String, String>,
    black_to_move: bool,
    fullmove: u32,
) -> Vec<String> {
    let mut lines = Vec::new();
    let move_text = pv
        .get("bestmove")
        .map(|m| format_pv_with_move_numbers(m, black_to_move, fullmove))
        .or_else(|| {
            pv.get("pv").map(|p| {
                let first = p.split_whitespace().next().unwrap_or("");
                format_pv_with_move_numbers(first, black_to_move, fullmove)
            })
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "—".into());
    let score = pv
        .get("score")
        .map(|s| format_score_human(s))
        .unwrap_or_else(|| "—".into());

    let depth = match (pv.get("depth"), pv.get("seldepth")) {
        (Some(d), Some(sd)) => format!("depth {d}/{sd}"),
        (Some(d), None) => format!("depth {d}"),
        (None, Some(sd)) => format!("seldepth {sd}"),
        (None, None) => String::new(),
    };

    let mut header = if depth.is_empty() {
        format!("PV {index}: {move_text}  {score}")
    } else {
        format!("PV {index}: {move_text}  {score}  {depth}")
    };
    if let Some(wdl) = pv.get("wdl") {
        let _ = write!(header, "  {}", format_wdl_human(wdl));
    }
    lines.push(header);

    if let Some(variation) = pv.get("pv") {
        lines.push(format!(
            "  {}",
            format_pv_with_move_numbers(variation, black_to_move, fullmove)
        ));
    }

    for (key, value) in pv {
        if matches!(
            key.as_str(),
            "bestmove" | "score" | "depth" | "seldepth" | "wdl" | "pv" | "multipv"
        ) {
            continue;
        }
        lines.push(format!("  {}: {}", key, format_property_value(key, value)));
    }

    lines
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let (position_width, position_height) = position_tile_size(app);

    let mut vertical_constraints = vec![Constraint::Length(position_height), Constraint::Min(1)];
    if app.engine_tile_visible {
        vertical_constraints.push(Constraint::Length(ENGINE_TILE_HEIGHT));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vertical_constraints)
        .split(area);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(position_width), Constraint::Min(1)])
        .split(chunks[0]);

    draw_position(frame, top_row[0], app);
    draw_command(frame, top_row[1], app);

    let engine_count = app.engines.len().max(1);
    let engine_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![
            Constraint::Ratio(1, engine_count as u32);
            engine_count
        ])
        .split(chunks[1]);

    for (index, column) in engine_columns.iter().enumerate() {
        if let Some(engine) = app.engines.get(index) {
            draw_properties(frame, *column, engine, &app.position);
        }
    }

    if app.engine_tile_visible {
        let engine_output_columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Ratio(1, engine_count as u32);
                engine_count
            ])
            .split(chunks[2]);

        for (index, column) in engine_output_columns.iter().enumerate() {
            if let Some(engine) = app.engines.get(index) {
                draw_engine_output(frame, *column, app, index, &engine.name);
            }
        }
    }
}

fn piece_style(color: PieceColor) -> Style {
    match color {
        PieceColor::White => Style::default().fg(Color::LightYellow),
        PieceColor::Black => Style::default().fg(Color::LightCyan),
    }
}

fn is_light_square(rank: usize, file: usize) -> bool {
    (file + (7 - rank)) % 2 == 1
}

fn position_board_lines(app: &App) -> Vec<Line<'_>> {
    app.position
        .board()
        .iter()
        .enumerate()
        .map(|(rank, row)| {
            let mut spans = Vec::with_capacity(row.len() * 2);
            for (file, &piece) in row.iter().enumerate() {
                if file > 0 {
                    spans.push(Span::raw(" "));
                }
                match piece_glyph(piece) {
                    Some((glyph, color)) => {
                        spans.push(Span::styled(glyph, piece_style(color)));
                    }
                    None if is_light_square(rank, file) => spans.push(Span::raw("·")),
                    None => spans.push(Span::styled("▪", Style::default().fg(Color::DarkGray))),
                }
            }
            Line::from(spans)
        })
        .collect()
}

/// Terminal size (columns × rows) for the Position tile including its border.
fn position_tile_size(app: &App) -> (u16, u16) {
    let lines = position_board_lines(app);
    let content_width = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = u16::try_from(content_width)
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    // Add 1 column spacing to the right
    (width + 1, height)
}

fn draw_position(frame: &mut Frame, area: Rect, app: &App) {
    let lines = position_board_lines(app);

    let block = Block::default()
        .title(" Position ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_properties(
    frame: &mut Frame,
    area: Rect,
    engine: &crate::app::EngineState,
    position: &crate::fen::Position,
) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let mut lines = Vec::new();

    if engine.info.is_empty() && engine.pvs.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no engine properties yet)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for line in engine_property_lines(engine, position) {
            if line.is_empty() {
                lines.push(Line::default());
                continue;
            }
            for row in wrap_line(&line, inner_width.max(1)) {
                lines.push(Line::from(Span::raw(row)));
            }
        }
    }

    if let Some(last) = engine.lines.last() {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        for row in wrap_line(last, inner_width.max(1)) {
            lines.push(Line::from(Span::styled(
                row,
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let block = Block::default()
        .title(format!(" {} ", engine.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_engine_output(frame: &mut Frame, area: Rect, app: &App, index: usize, _name: &str) {
    let inner_height = area.height.saturating_sub(2).min(ENGINE_INNER_LINES) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let visible = app.visible_engine_display_lines(index, inner_height, inner_width);

    let lines: Vec<Line> = visible
        .into_iter()
        .map(|line| Line::from(Span::raw(line)))
        .collect();

    let block = Block::default()
        .title(" Console ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_command(frame: &mut Frame, area: Rect, app: &App) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "Status: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.status.clone()),
    ])];

    lines.push(Line::default());
    for row in wrap_line(&format!("FEN: {}", app.position.fen), inner_width.max(1)) {
        lines.push(Line::from(Span::raw(row)));
    }

    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::raw(app.input.clone()),
        Span::styled(
            "█",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]));

    let block = Block::default()
        .title(" Command ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_si_number_examples() {
        assert_eq!(format_si_number(999), "999");
        assert_eq!(format_si_number(19184), "19k2");
        assert_eq!(format_si_number(2321102), "2M32");
        assert_eq!(format_si_number(123_456_789_012), "123G");
    }

    #[test]
    fn format_si_number_small_and_signed() {
        assert_eq!(format_si_number(0), "0");
        assert_eq!(format_si_number(500), "500");
        assert_eq!(format_si_number(-19184), "-19k2");
    }

    #[test]
    fn format_property_value_formats_known_keys() {
        assert_eq!(format_property_value("score", "cp 25"), "+0.25");
        assert_eq!(format_property_value("score", "cp -30"), "-0.30");
        assert_eq!(format_property_value("score", "mate 3"), "mate in 3");
        assert_eq!(
            format_property_value("score", "cp 50 upperbound"),
            "≤ +0.50"
        );
        assert_eq!(
            format_property_value("wdl", "100 200 700"),
            "W 10.0%  D 20.0%  L 70.0%"
        );
        assert_eq!(format_property_value("time", "500"), "500 ms");
        assert_eq!(format_property_value("time", "1500"), "1.50 s");
        assert_eq!(format_property_value("nodes", "1000"), "1k");
        assert_eq!(format_property_value("bestmove", "e2e4"), "e2e4");
    }

    #[test]
    fn is_light_square_a1_is_dark() {
        assert!(!is_light_square(7, 0));
        assert!(is_light_square(7, 1));
    }

    #[test]
    fn position_tile_size_matches_board() {
        let app = crate::app::App::new(vec!["Engine".into()], 1);
        assert_eq!(position_tile_size(&app), (18, 10));
    }

    #[test]
    fn format_pv_with_move_numbers_white_to_move() {
        assert_eq!(
            format_pv_with_move_numbers("e2e4 e7e5 g1f3 b8c6", false, 1),
            "1.e2e4 e7e5 2.g1f3 b8c6"
        );
    }

    #[test]
    fn format_pv_with_move_numbers_black_to_move() {
        assert_eq!(
            format_pv_with_move_numbers("e7e5 g1f3 b8c6", true, 1),
            "1...e7e5 2.g1f3 b8c6"
        );
    }

    #[test]
    fn engine_property_lines_are_per_pv_and_human_readable() {
        let mut app = crate::app::App::new(vec!["Engine".into()], 1);
        app.push_engine_lines(
            0,
            &[
                "info depth 12 seldepth 20 multipv 1 score cp 25 wdl 100 200 700 nodes 1000 nps 500000 time 500 pv e2e4 e7e5".into(),
                "info depth 12 seldepth 18 multipv 2 score cp 10 nodes 1000 time 500 pv d2d4 d7d5".into(),
            ],
        );
        let lines = engine_property_lines(&app.engines[0], &app.position);
        assert!(lines.iter().any(|l| l.starts_with("best move: e2e4")));
        assert!(lines.iter().any(|l| l.starts_with("nodes: 1k")));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("PV 1: 1.e2e4  +0.25  depth 12/20"))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("W 10.0%  D 20.0%  L 70.0%"))
        );
        assert!(lines.iter().any(|l| l.contains("1.e2e4 e7e5")));
        assert!(
            lines
                .iter()
                .any(|l| l.contains("PV 2: 1.d2d4  +0.10  depth 12/18"))
        );
        assert!(lines.iter().any(|l| l.contains("1.d2d4 d7d5")));
    }
}
