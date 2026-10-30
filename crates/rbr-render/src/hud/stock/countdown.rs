//! The start countdown as an LED plaque: a dark board of dots with the call ([`crate::hud::countdown_call`]) in a 5x7 dot
//! font, its top and bottom rows lit red beside it for 3, 2, 1 and green for GO, dimmed before the 3. Ours: the look
//! follows RallySimFans' countdown board, drawn as shapes; 1.02 draws its "3 2 1 GO!" sprites instead.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};

/// The board's dots across and down, their pitch and radius, and its padding, points.
const COLUMNS: usize = 25;
const ROWS: usize = 7;
const PITCH: f32 = 7.0;
const DOT: f32 = 2.6;
const PAD: f32 = 9.0;
const BOARD: Color32 = Color32::from_rgba_premultiplied(6, 6, 8, 245);
const UNLIT: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 22);
const DIGIT: Color32 = Color32::from_gray(245);
const RED: Color32 = Color32::from_rgb(235, 40, 35);
const GREEN: Color32 = Color32::from_rgb(70, 220, 90);

/// A glyph's rows, top first, its five columns in the low bits (bit 4 the left).
fn glyph(c: char) -> Option<[u8; ROWS]> {
    Some(match c {
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        _ => return None,
    })
}

/// The board's lit dots for `call`: each dot's colour, `None` unlit. The glyphs sit in the middle, one column apart;
/// the top and bottom rows beside them take `rows`, one unlit column away.
pub fn dots(call: &str, rows: Option<Color32>, ink: Color32) -> [[Option<Color32>; COLUMNS]; ROWS] {
    let glyphs: Vec<[u8; ROWS]> = call.chars().filter_map(glyph).collect();
    let width = (glyphs.len() * 6).saturating_sub(1);
    let left = (COLUMNS - width) / 2;
    let mut board = [[None; COLUMNS]; ROWS];
    for (y, row) in board.iter_mut().enumerate() {
        for (x, dot) in row.iter_mut().enumerate() {
            let inside = (left..left + width).contains(&x);
            *dot = if inside {
                let (k, column) = ((x - left) / 6, (x - left) % 6);
                (column < 5 && (glyphs[k][y] >> (4 - column)) & 1 == 1).then_some(ink)
            } else if (y == 0 || y == ROWS - 1) && (x + 1 < left || x > left + width) {
                rows
            } else {
                None
            };
        }
    }
    board
}

/// Draws the board for `call` centred at `at`: the digits, "GO", or with any other call (the wait before the 3) an
/// empty board with dim red rows and the call over it.
pub fn paint(painter: &Painter, at: Pos2, call: &str) {
    let size = Vec2::new(COLUMNS as f32 * PITCH, ROWS as f32 * PITCH) + Vec2::splat(2.0 * PAD);
    let board = Rect::from_center_size(at, size);
    painter.rect_filled(board.translate(Vec2::new(0.0, 3.0)), 6.0, Color32::from_black_alpha(90));
    painter.rect_filled(board, 6.0, BOARD);
    painter.rect_stroke(board, 6.0, (1.0, Color32::from_white_alpha(18)), egui::StrokeKind::Inside);
    let lit = match call {
        "GO" => dots(call, Some(GREEN), GREEN),
        _ if call.chars().all(|c| glyph(c).is_some()) => dots(call, Some(RED), DIGIT),
        _ => {
            let caption = call.to_uppercase();
            let above = board.center_top() - Vec2::new(0.0, 6.0);
            painter.text(above, Align2::CENTER_BOTTOM, caption, FontId::proportional(13.0), DIGIT);
            dots("", Some(RED.gamma_multiply(0.35)), DIGIT)
        }
    };
    let origin = board.min + Vec2::splat(PAD + PITCH / 2.0);
    for (y, row) in lit.iter().enumerate() {
        for (x, dot) in row.iter().enumerate() {
            let centre = origin + Vec2::new(x as f32, y as f32) * PITCH;
            match dot {
                Some(colour) => {
                    painter.circle_filled(centre, DOT + 1.6, colour.gamma_multiply(0.25));
                    painter.circle_filled(centre, DOT, *colour);
                }
                None => {
                    painter.circle_filled(centre, DOT, UNLIT);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digit_sits_in_the_middle_between_the_lit_rows() {
        let board = dots("3", Some(RED), DIGIT);
        // The 3 takes columns 10 to 14; its top row is full.
        assert!(board[0][10..15].iter().all(|d| *d == Some(DIGIT)));
        assert!(board[0][..9].iter().chain(&board[0][16..]).all(|d| *d == Some(RED)));
        assert_eq!((board[0][9], board[0][15]), (None, None));
        assert!(board[3][..10].iter().all(Option::is_none));
        let go = dots("GO", Some(GREEN), GREEN);
        // G and O take columns 7 to 11 and 13 to 17, with the gap between them unlit.
        assert_eq!(go[3][12], None);
        assert_eq!((go[0][5], go[0][6]), (Some(GREEN), None));
    }
}
