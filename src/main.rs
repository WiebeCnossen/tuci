mod app;
mod config;
mod fen;
mod process_priority;
mod terminal;
mod uci;
mod ui;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
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
    let engine_names = config.engine_display_names();

    let (output_tx, mut output_rx) = mpsc::unbounded_channel::<(usize, String)>();
    let (ready_tx, mut ready_rx) = mpsc::unbounded_channel::<(usize, UciEngine)>();

    for (index, engine_config) in config.engines.iter().enumerate() {
        let options: Vec<(String, String)> = engine_config
            .options
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let path = engine_config.path.clone();
        let output_tx = output_tx.clone();
        let ready_tx = ready_tx.clone();

        tokio::spawn(async move {
            match UciEngine::spawn(&path, &options, index, output_tx.clone()).await {
                Ok(engine) => {
                    let _ = ready_tx.send((index, engine));
                }
                Err(err) => {
                    let _ = output_tx
                        .send((index, format!("[stderr] failed to start engine: {err:#}")));
                }
            }
        });
    }
    drop(output_tx);
    drop(ready_tx);

    let mut terminal = terminal::setup().await?;
    let mut app = App::new(engine_names);

    let result = run_loop(&mut terminal, &mut app, &mut output_rx, &mut ready_rx).await;

    app.quit_all_engines();
    terminal::restore(terminal).await?;
    result
}

const ENGINE_OUTPUT_DRAIN_LIMIT: usize = 256;

fn drain_engine_output(
    output_rx: &mut mpsc::UnboundedReceiver<(usize, String)>,
    app: &mut App,
) -> bool {
    let mut changed = false;
    for _ in 0..ENGINE_OUTPUT_DRAIN_LIMIT {
        match output_rx.try_recv() {
            Ok((index, line)) => {
                app.push_engine_lines(index, &[line]);
                changed = true;
            }
            Err(mpsc::error::TryRecvError::Empty) => break,
            Err(mpsc::error::TryRecvError::Disconnected) => {
                app.status = if app.any_engine_ready() {
                    "All engines disconnected".into()
                } else {
                    "All engines disconnected during startup".into()
                };
                return true;
            }
        }
    }
    changed
}

fn drain_engine_ready(ready_rx: &mut mpsc::UnboundedReceiver<(usize, UciEngine)>, app: &mut App) {
    while let Ok((index, engine)) = ready_rx.try_recv() {
        app.attach_engine(index, engine);
    }
}

async fn run_loop(
    terminal: &mut TuiTerminal,
    app: &mut App,
    output_rx: &mut mpsc::UnboundedReceiver<(usize, String)>,
    ready_rx: &mut mpsc::UnboundedReceiver<(usize, UciEngine)>,
) -> Result<()> {
    let mut events = EventStream::new();
    let mut redraw = tokio::time::interval(Duration::from_millis(16));
    redraw.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut needs_redraw = true;

    loop {
        drain_engine_ready(ready_rx, app);
        if drain_engine_output(output_rx, app) {
            needs_redraw = true;
        }

        if needs_redraw {
            terminal::draw(terminal, app)?;
            needs_redraw = false;
        }

        tokio::select! {
            biased;

            event = events.next() => {
                match event {
                    Some(Ok(crossterm::event::Event::Key(key))) if key.kind == crossterm::event::KeyEventKind::Press => {
                        terminal::handle_key(key, app)?;
                        needs_redraw = true;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => return Err(err.into()),
                    None => return Ok(()),
                }
            }
            _ = redraw.tick() => {
                needs_redraw = true;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
