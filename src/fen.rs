use anyhow::{Result, anyhow, bail};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position as ShakPosition};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// Defaults for FEN fields 2–6 when omitted (active color, castling, en passant, halfmove, fullmove).
const FEN_FIELD_DEFAULTS: [&str; 5] = ["w", "-", "-", "0", "1"];

/// Pad a partial FEN (1–5 fields) with standard defaults for missing trailing fields.
fn pad_fen(fen: &str) -> String {
    let parts: Vec<&str> = fen.split_whitespace().collect();
    if parts.is_empty() || parts.len() >= 6 {
        return fen.trim().to_string();
    }
    let mut fields = parts;
    fields.extend_from_slice(&FEN_FIELD_DEFAULTS[fields.len() - 1..]);
    fields.join(" ")
}

#[derive(Clone, Debug)]
pub struct Position {
    pub fen: String,
    board: [[char; 8]; 8],
}

impl Default for Position {
    fn default() -> Self {
        Self::from_fen(START_FEN).expect("start position is valid")
    }
}

impl Position {
    pub fn from_fen(fen: &str) -> Result<Self> {
        let fen = pad_fen(fen);
        let board_part = fen.split_whitespace().next().unwrap_or(fen.as_str());
        let mut board = [[' '; 8]; 8];

        let mut rank = 0usize;
        for segment in board_part.split('/') {
            if rank >= 8 {
                bail!("FEN has more than 8 ranks");
            }
            let mut file = 0usize;
            for ch in segment.chars() {
                if ch.is_ascii_digit() {
                    let empty = ch.to_digit(10).unwrap() as usize;
                    file += empty;
                } else if ch.is_ascii_alphabetic() {
                    if file >= 8 {
                        bail!("rank {} has more than 8 files", rank + 1);
                    }
                    board[rank][file] = ch;
                    file += 1;
                } else {
                    bail!("invalid character in FEN board: {ch}");
                }
            }
            if file != 8 {
                bail!("rank {} does not have 8 files", rank + 1);
            }
            rank += 1;
        }
        if rank != 8 {
            bail!("FEN must have 8 ranks, found {rank}");
        }

        Ok(Self { fen, board })
    }

    pub fn board(&self) -> &[[char; 8]; 8] {
        &self.board
    }

    /// Apply a move in UCI notation (e.g. `e2e4`, `e7e8q`) and return the resulting position.
    pub fn apply_uci_move(&self, uci_move: &str) -> Result<Self> {
        let fen: Fen = self.fen.parse().map_err(|e| anyhow!("invalid FEN: {e}"))?;
        let pos: Chess = fen
            .into_position(CastlingMode::Standard)
            .map_err(|e| anyhow!("invalid position: {e}"))?;
        let uci: UciMove = uci_move
            .parse()
            .map_err(|e| anyhow!("invalid UCI move: {e}"))?;
        let chess_move = uci
            .to_move(&pos)
            .map_err(|e| anyhow!("illegal move {uci_move}: {e}"))?;
        let new_pos = pos
            .play(&chess_move)
            .map_err(|e| anyhow!("illegal move {uci_move}: {e}"))?;
        let new_fen = Fen::from_position(new_pos, EnPassantMode::Legal);
        Self::from_fen(&new_fen.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceColor {
    White,
    Black,
}

pub fn piece_glyph(piece: char) -> Option<(&'static str, PieceColor)> {
    match piece {
        'K' => Some(("♔", PieceColor::White)),
        'Q' => Some(("♕", PieceColor::White)),
        'R' => Some(("♖", PieceColor::White)),
        'B' => Some(("♗", PieceColor::White)),
        'N' => Some(("♘", PieceColor::White)),
        'P' => Some(("♙", PieceColor::White)),
        'k' => Some(("♔", PieceColor::Black)),
        'q' => Some(("♕", PieceColor::Black)),
        'r' => Some(("♖", PieceColor::Black)),
        'b' => Some(("♗", PieceColor::Black)),
        'n' => Some(("♘", PieceColor::Black)),
        'p' => Some(("♙", PieceColor::Black)),
        ' ' => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR";

    #[test]
    fn pad_fen_board_only() {
        assert_eq!(pad_fen(BOARD), format!("{BOARD} w - - 0 1"));
    }

    #[test]
    fn pad_fen_partial_fields() {
        assert_eq!(
            pad_fen(&format!("{BOARD} b KQ")),
            format!("{BOARD} b KQ - 0 1")
        );
    }

    #[test]
    fn pad_fen_full_unchanged() {
        let full = START_FEN;
        assert_eq!(pad_fen(full), full);
    }

    #[test]
    fn from_fen_pads_before_parse() {
        let pos = Position::from_fen(BOARD).unwrap();
        assert_eq!(pos.fen, format!("{BOARD} w - - 0 1"));
    }

    #[test]
    fn apply_uci_move_e2e4() {
        let start = Position::default();
        let after = start.apply_uci_move("e2e4").unwrap();
        assert!(
            after
                .fen
                .starts_with("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR")
        );
        assert!(after.fen.contains(" b "));
    }
}
