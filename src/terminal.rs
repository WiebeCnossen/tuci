use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type TuiTerminal = Terminal<CrosstermBackend<std::io::Stdout>>;

pub async fn setup() -> Result<TuiTerminal> {
    tokio::task::spawn_blocking(|| {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(std::io::stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.hide_cursor()?;
        Ok(terminal)
    })
    .await?
}

pub async fn restore(mut terminal: TuiTerminal) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;
        Ok(())
    })
    .await?
}

pub fn draw(terminal: &mut TuiTerminal, app: &crate::app::App) -> Result<()> {
    tokio::task::block_in_place(|| {
        terminal.draw(|frame| crate::ui::draw(frame, app))?;
        Ok(())
    })
}

pub fn handle_key(key: crossterm::event::KeyEvent, app: &mut crate::app::App) -> Result<()> {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(engine) = app.engine() {
                engine.quit();
            }
            app.should_quit = true;
        }
        KeyCode::Esc => {
            if let Some(engine) = app.engine() {
                engine.quit();
            }
            app.should_quit = true;
        }
        KeyCode::Enter => {
            if let Err(err) = app.submit_input() {
                app.status = err.to_string();
            }
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(ch) => {
            app.input.push(ch);
        }
        _ => {}
    }
    Ok(())
}
