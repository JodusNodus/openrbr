//! The car's status at the bottom left, drawn only when there is something to show: the lit warning lamps, then a
//! card with the damaged systems ([`CarStatus`]) and a top view of the car whose body and wheels take their damage's
//! colour. Ours.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};

use super::{LAMP_ON, MARGIN, PAD, PANEL, RADIUS, TEXT};
use crate::hud::model::{CarStatus, HudView, Lamp, BODYWORK, LAMPS, LEVELS};

/// A level's colour: yellow, orange, red, then dark red once lost.
fn colour(level: u8) -> Color32 {
    match level {
        0 => Color32::from_white_alpha(45),
        1 => Color32::from_rgb(245, 205, 60),
        2 => Color32::from_rgb(245, 145, 40),
        3 => LAMP_ON,
        _ => Color32::from_rgb(150, 25, 25),
    }
}

/// Draws the status over the bottom left of `screen`: the card, and the lit lamps over it.
pub fn paint(painter: &Painter, screen: Rect, s: &HudView) {
    let mut bottom = screen.max.y - MARGIN;
    if let Some(status) = s.car {
        let rows = status.rows();
        let car = status.shows_car();
        if car || !rows.is_empty() {
            let column = if car { 60.0 } else { 0.0 };
            let height = (rows.len() as f32 * 18.0).max(if car { 78.0 } else { 0.0 });
            let size = Vec2::new(column + 170.0, height + 2.0 * PAD);
            let card = Rect::from_min_size(Pos2::new(screen.min.x + MARGIN, bottom - size.y), size);
            painter.rect_filled(card, RADIUS, PANEL);
            let inner = card.shrink(PAD);
            if car {
                top_view(painter, Rect::from_min_size(inner.min, Vec2::new(48.0, height)), &status);
            }
            for (i, (name, level)) in rows.into_iter().enumerate() {
                let row = Rect::from_min_size(
                    inner.min + Vec2::new(column, 18.0 * i as f32),
                    Vec2::new(inner.width() - column, 18.0),
                );
                let mark = Rect::from_center_size(row.left_center() + Vec2::new(2.0, 0.0), Vec2::new(4.0, 12.0));
                painter.rect_filled(mark, 1.0, colour(level));
                let at = row.left_center() + Vec2::new(12.0, 0.0);
                painter.text(at, Align2::LEFT_CENTER, name, FontId::proportional(13.0), TEXT);
                let word = LEVELS[usize::from(level.min(4))].to_uppercase();
                painter.text(
                    row.right_center(),
                    Align2::RIGHT_CENTER,
                    word,
                    FontId::proportional(11.0),
                    colour(level.min(3)),
                );
            }
            bottom = card.min.y - 6.0;
        }
    }
    lamps(painter, Pos2::new(screen.min.x + MARGIN, bottom), s);
}

/// The car from above in `rect`: its body outlined in the bodywork's colour, each wheel filled with the worse of its
/// suspension's and its tyre's.
fn top_view(painter: &Painter, rect: Rect, status: &CarStatus) {
    let body = Rect::from_center_size(rect.center(), Vec2::new(26.0, 62.0));
    painter.rect_filled(body, 8.0, Color32::from_white_alpha(10));
    let outline = match status.systems[BODYWORK] {
        0 => Color32::from_white_alpha(90),
        level => colour(level),
    };
    painter.rect_stroke(body, 8.0, (1.5, outline), egui::StrokeKind::Inside);
    let screen = Rect::from_center_size(body.center() - Vec2::new(0.0, 9.0), Vec2::new(18.0, 9.0));
    painter.rect_filled(screen, 2.0, Color32::from_white_alpha(40));
    for w in 0..4 {
        let x = if w % 2 == 0 { body.min.x - 3.0 } else { body.max.x + 3.0 };
        let y = if w < 2 { body.min.y + 14.0 } else { body.max.y - 14.0 };
        let wheel = Rect::from_center_size(Pos2::new(x, y), Vec2::new(7.0, 15.0));
        let level = status.suspension[w].max(status.tyres[w]);
        painter.rect_filled(wheel, 2.0, colour(level));
    }
}

/// The lit warning lamps as red chips from `corner` (bottom left), left to right; a flashing one turns to an outline
/// between its flashes.
fn lamps(painter: &Painter, corner: Pos2, s: &HudView) {
    let mut left = corner.x;
    for (spec, lamp) in LAMPS.iter().zip(s.lamps.unwrap_or([Lamp::Off; 5])) {
        let (fill, ink) = match lamp {
            Lamp::Off => continue,
            Lamp::Flashing if !s.flash_on => (PANEL, LAMP_ON),
            _ => (LAMP_ON, TEXT),
        };
        let galley = painter.layout_no_wrap(spec.name.to_uppercase(), FontId::proportional(12.0), ink);
        let chip =
            Rect::from_min_max(Pos2::new(left, corner.y - 22.0), Pos2::new(left + galley.size().x + 16.0, corner.y));
        painter.rect_filled(chip, RADIUS, fill);
        if fill == PANEL {
            painter.rect_stroke(chip, RADIUS, (1.0, LAMP_ON), egui::StrokeKind::Inside);
        }
        painter.galley(chip.center() - galley.size() / 2.0, galley, ink);
        left = chip.max.x + 6.0;
    }
}
