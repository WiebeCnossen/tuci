use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::fen::Position;

const SESSION_FILE: &str = "Tuci.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionFile {
    #[serde(default)]
    fen: Option<String>,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// `Tuci.toml` in the user's home directory.
pub fn path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(SESSION_FILE))
}

/// Load the saved position from `~/Tuci.toml`, if present and valid.
pub fn load_position() -> Option<Position> {
    let path = path()?;
    let text = fs::read_to_string(&path).ok()?;
    let session: SessionFile = toml::from_str(&text).ok()?;
    let fen = session.fen.filter(|s| !s.trim().is_empty())?;
    Position::from_fen(&fen).ok()
}

/// Persist the current FEN to `~/Tuci.toml`.
pub fn save_position(position: &Position) -> Result<()> {
    let path = path().context("home directory not found")?;
    let session = SessionFile {
        fen: Some(position.fen.clone()),
    };
    let text = toml::to_string_pretty(&session).context("serializing session")?;
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_session_toml() {
        let fen = "8/8/8/4k3/8/4K3/8/8 w - - 0 1";
        let text = toml::to_string_pretty(&SessionFile {
            fen: Some(fen.into()),
        })
        .unwrap();
        let parsed: SessionFile = toml::from_str(&text).unwrap();
        assert_eq!(parsed.fen.as_deref(), Some(fen));
        assert!(Position::from_fen(fen).is_ok());
    }
}
