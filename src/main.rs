mod app;
mod config;
mod fen;
mod process_priority;
mod terminal;
mod uci;
mod ui;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::EventStream;
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::app::App;
use crate::config::Config;
use crate::terminal::TuiTerminal;
use crate::uci::UciEngine;

#[derive(Parser, Debug)]
#[command(name = "tuci", about = "Terminal UCI chess client")]
struct Cli {
    /// Path to the TOML config file
    #[arg(short, long, default_value = "tuci.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config).await?;
    let options: Vec<(String, String)> = config.options.into_iter().collect();

    let (engine_tx, mut engine_rx) = mpsc::unbounded_channel();
    let engine_path = config.engine.path.clone();
    let mut spawn_engine = Box::pin(UciEngine::spawn(&engine_path, &options, engine_tx));

    let mut terminal = terminal::setup().await?;
    let mut app = App::new();

    let result = run_loop(&mut terminal, &mut app, &mut engine_rx, &mut spawn_engine).await;

    if let Some(engine) = app.engine() {
        engine.quit();
    }
    terminal::restore(terminal).await?;
    result
}

async fn run_loop(
    terminal: &mut TuiTerminal,
    app: &mut App,
    engine_rx: &mut mpsc::UnboundedReceiver<String>,
    spawn_engine: &mut Pin<Box<impl Future<Output = Result<UciEngine>>>>,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut redraw = tokio::time::interval(Duration::from_millis(16));
    redraw.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut engine_batch = Vec::with_capacity(512);

    loop {
        tokio::select! {
            biased;

            count = engine_rx.recv_many(&mut engine_batch, 512) => {
                match count {
                    0 if !app.engine_ready() => {
                        app.status = "Engine disconnected during startup".into();
                    }
                    0 => app.status = "Engine disconnected".into(),
                    _ => app.push_engine_lines(&engine_batch[..count]),
                }
                engine_batch.clear();
                terminal::draw(terminal, app)?;
            }
            result = spawn_engine.as_mut(), if !app.engine_ready() => {
                let engine = result.context("starting UCI engine")?;
                app.attach_engine(engine);
                terminal::draw(terminal, app)?;
            }
            event = events.next() => {
                match event {
                    Some(Ok(crossterm::event::Event::Key(key))) if key.kind == crossterm::event::KeyEventKind::Press => {
                        terminal::handle_key(key, app)?;
                        terminal::draw(terminal, app)?;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => return Err(err.into()),
                    None => return Ok(()),
                }
            }
            _ = redraw.tick() => {
                terminal::draw(terminal, app)?;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
