use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use std::collections::BTreeMap;

use crate::app::{App, wrap_line};
use crate::fen::{PieceColor, piece_glyph};

const SI_PREFIXES: [&str; 8] = ["", "k", "M", "G", "T", "P", "E", "Z"];

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

fn format_property_value(value: &str) -> String {
    value
        .parse::<i64>()
        .map(format_si_number)
        .unwrap_or_else(|_| value.to_string())
}

/// Display order: alphabetical keys except `pv`, then `pv` last.
fn engine_info_display_keys(info: &BTreeMap<String, String>) -> Vec<(&str, &str)> {
    let mut pairs: Vec<_> = info
        .iter()
        .filter(|(k, _)| k.as_str() != "pv")
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    if let Some(pv) = info.get("pv") {
        pairs.push(("pv", pv.as_str()));
    }
    pairs
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(4)])
        .split(area);

    if app.engine_tile_visible {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(chunks[0]);

        draw_board(frame, main_chunks[0], app);
        draw_engine_output(frame, main_chunks[1], app);
    } else {
        draw_board(frame, chunks[0], app);
    }
    draw_input(frame, chunks[1], app);
}

fn piece_style(color: PieceColor) -> Style {
    match color {
        PieceColor::White => Style::default().fg(Color::LightYellow),
        PieceColor::Black => Style::default().fg(Color::LightCyan),
    }
}

fn draw_board(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = vec![Line::from(Span::raw("  a b c d e f g h"))];

    for (row_idx, row) in app.position.board().iter().enumerate() {
        let rank = 8 - row_idx;
        let mut spans = vec![Span::raw(format!("{rank} "))];
        for (file, &piece) in row.iter().enumerate() {
            if file > 0 {
                spans.push(Span::raw(" "));
            }
            match piece_glyph(piece) {
                Some((glyph, color)) => {
                    spans.push(Span::styled(glyph, piece_style(color)));
                }
                None => spans.push(Span::raw("·")),
            }
        }
        spans.push(Span::raw(format!(" {rank}")));
        lines.push(Line::from(spans));
    }

    lines.push(Line::default());

    let inner_width = area.width.saturating_sub(2) as usize;
    for row in wrap_line(&format!("FEN: {}", app.position.fen), inner_width.max(1)) {
        lines.push(Line::from(Span::raw(row)));
    }

    if !app.engine_info.is_empty() {
        lines.push(Line::default());
        for (key, value) in engine_info_display_keys(&app.engine_info) {
            let display = format_property_value(value);
            for row in wrap_line(&format!("{key}: {display}"), inner_width.max(1)) {
                lines.push(Line::from(Span::raw(row)));
            }
        }
    }

    let block = Block::default()
        .title(" Position ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_engine_output(frame: &mut Frame, area: Rect, app: &App) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let visible = app.visible_engine_display_lines(inner_height, inner_width);

    let lines: Vec<Line> = visible
        .into_iter()
        .map(|line| Line::from(Span::raw(line)))
        .collect();

    let block = Block::default()
        .title(" Engine ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, area: Rect, app: &App) {
    let status = Line::from(vec![
        Span::styled(
            "Status: ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.status.clone()),
    ]);

    let input_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Yellow)),
        Span::raw(app.input.clone()),
        Span::styled(
            "█",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);

    let block = Block::default().borders(Borders::ALL);
    let paragraph = Paragraph::new(vec![status, input_line]).block(block);
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
    fn format_property_value_non_numeric_unchanged() {
        assert_eq!(format_property_value("cp 25"), "cp 25");
        assert_eq!(format_property_value("e2e4"), "e2e4");
        assert_eq!(format_property_value("1000"), "1k");
    }

    #[test]
    fn engine_info_display_keys_pv_last() {
        let mut info = BTreeMap::new();
        info.insert("depth".into(), "10".into());
        info.insert("pv".into(), "e2e4 e7e5".into());
        info.insert("nodes".into(), "1000".into());
        info.insert("score".into(), "cp 25".into());

        let keys: Vec<_> = engine_info_display_keys(&info)
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(keys, ["depth", "nodes", "score", "pv"]);
    }
}
