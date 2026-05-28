use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use crate::process_priority;

pub struct UciEngine {
    _child: Child,
    command_tx: mpsc::UnboundedSender<String>,
}

impl UciEngine {
    pub async fn spawn(
        engine_path: &Path,
        options: &[(String, String)],
        engine_index: usize,
        output_tx: mpsc::UnboundedSender<(usize, String)>,
    ) -> Result<Self> {
        let mut child = Command::new(engine_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning engine {}", engine_path.display()))?;

        process_priority::set_lowest_priority_for_child(child.id())
            .context("setting engine process priority")?;

        let stdin = child.stdin.take().context("engine stdin not available")?;
        let stdout = child.stdout.take().context("engine stdout not available")?;
        let stderr = child.stderr.take();

        let out = output_tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = out.send((engine_index, line));
            }
        });

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = output_tx.send((engine_index, format!("[stderr] {line}")));
                }
            });
        }

        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(write_loop(stdin, command_rx));

        let engine = Self {
            _child: child,
            command_tx,
        };

        engine.handshake(options).await?;
        Ok(engine)
    }

    fn send(&self, line: impl Into<String>) {
        let _ = self.command_tx.send(line.into());
    }

    async fn handshake(&self, options: &[(String, String)]) -> Result<()> {
        self.send("uci");
        for (name, value) in options {
            self.send(format!("setoption name {name} value {value}"));
        }
        self.send("isready");
        Ok(())
    }

    pub fn set_position_fen(&self, fen: &str) {
        self.send(format!("position fen {fen}"));
    }

    pub fn go(&self, args: &str) {
        if args.trim().is_empty() {
            self.send("go infinite");
        } else {
            self.send(format!("go {args}"));
        }
    }

    pub fn stop(&self) {
        self.send("stop");
    }

    pub fn quit(&self) {
        self.send("quit");
    }
}

async fn write_loop(mut stdin: ChildStdin, mut commands: mpsc::UnboundedReceiver<String>) {
    while let Some(line) = commands.recv().await {
        if stdin.write_all(line.as_bytes()).await.is_err() {
            break;
        }
        if stdin.write_all(b"\n").await.is_err() {
            break;
        }
        if stdin.flush().await.is_err() {
            break;
        }
    }
}
