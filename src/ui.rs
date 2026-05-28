use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(4)])
        .split(area);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[0]);

    draw_board(frame, main_chunks[0], app);
    draw_engine_output(frame, main_chunks[1], app);
    draw_input(frame, chunks[1], app);
}

fn draw_board(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .position
        .board_lines()
        .into_iter()
        .map(|s| Line::from(Span::raw(s)))
        .collect();

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
