use anyhow::{Result, bail};

const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

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
        let board_part = fen.split_whitespace().next().unwrap_or(fen);
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

        Ok(Self {
            fen: fen.to_string(),
            board,
        })
    }

    pub fn board(&self) -> &[[char; 8]; 8] {
        &self.board
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
