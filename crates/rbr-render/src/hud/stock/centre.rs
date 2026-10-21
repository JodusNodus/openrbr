//! The middle of the screen: the countdown's plaque, FALSE START, the pace note, the messages, WRONG WAY, the rally's
//! false-start warning, the pace stack and the 3D pace notes.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};
use glam::{Mat4, Vec3};

use super::{countdown, pace, ACCENT, ALERT, DIM, MARGIN, PAD, PANEL, RADIUS, TEXT};
use crate::hud::model::StackNote;
use crate::hud::pace::{label, modifier};
use crate::hud::{countdown_call, Part, Readout, FALSE_START_WARNINGS, REFERENCE_HEIGHT, WRONG_WAY};

/// The pace note's top, under the progress line.
const PACE_TOP: f32 = 56.0;

/// Big centred text over a soft shadow, readable on a bright sky.
fn shadowed(painter: &Painter, at: Pos2, text: String, font: FontId, colour: Color32) {
    let shadow = Color32::from_black_alpha(colour.a() / 2);
    painter.text(at + Vec2::splat(2.0), Align2::CENTER_CENTER, &text, font.clone(), shadow);
    painter.text(at, Align2::CENTER_CENTER, text, font, colour);
}

/// The live HUD's centre parts: the countdown, the false start, the pace note, the messages and the pace stack.
pub fn paint(painter: &Painter, screen: Rect, r: &Readout) {
    let s = &r.state;
    let font = FontId::proportional;
    let centre = screen.center().x;
    if let Some(call) = r.race.as_ref().and_then(countdown_call) {
        countdown::paint(painter, Pos2::new(centre, screen.height() * 0.3 + 20.0), &call);
    }
    if let Some(alpha) = s.false_start {
        let colour = Color32::from_rgba_unmultiplied(235, 60, 50, (alpha * 255.0) as u8);
        shadowed(painter, Pos2::new(centre, screen.height() * 0.42), "FALSE START".into(), font(48.0), colour);
    }
    if let Some((note, alpha)) = s.note.as_ref().filter(|_| r.parts.shows(Part::Pace)) {
        let panel = Rect::from_min_size(Pos2::new(centre - 60.0, screen.min.y + PACE_TOP), Vec2::new(120.0, 124.0));
        painter.rect_filled(panel, RADIUS, PANEL.gamma_multiply(*alpha));
        let symbol = Rect::from_center_size(Pos2::new(centre, panel.min.y + 8.0 + pace::SIZE.y / 2.0), pace::SIZE);
        pace::paint(painter, symbol, &note.symbol, *alpha);
        let ink = |c: Color32| c.gamma_multiply(*alpha);
        let mut line = label(&note.symbol).to_owned();
        if let Some(word) = modifier(note.flags) {
            line = format!("{line}, {word}");
        }
        painter.text(Pos2::new(centre, symbol.max.y + 4.0), Align2::CENTER_TOP, line, font(13.0), ink(TEXT));
        // The called distance (the game's DISTANCE option) and the countdown to the note (DIST_CNTDWN), both snapped
        // as the game does.
        if note.called > 0.0 {
            let called = format!("{:.0}", rbr_sound::codriver::snap(note.called));
            painter.text(panel.right_top() + Vec2::new(-6.0, 4.0), Align2::RIGHT_TOP, called, font(13.0), ink(DIM));
        }
        if let Some(d) = s.note_distance.filter(|d| *d > 0.0) {
            let left = format!("{:.0} m", rbr_sound::codriver::snap(d));
            painter.text(panel.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, left, font(13.0), ink(ACCENT));
        }
    }
    if let Some((message, alpha)) = s.message.filter(|_| r.parts.shows(Part::Messages)) {
        let lines = message.text().lines().count() as f32;
        let panel = Rect::from_center_size(
            Pos2::new(centre, screen.height() * 0.62),
            Vec2::new(380.0, 2.0 * PAD + 20.0 * lines),
        );
        super::panel(painter, panel, ALERT, alpha);
        painter.text(panel.center(), Align2::CENTER_CENTER, message.text(), font(17.0), TEXT.gamma_multiply(alpha));
    }
    if let Some(alpha) = s.wrong_way {
        let colour = Color32::from_rgba_unmultiplied(235, 60, 50, (alpha * 255.0) as u8);
        let panel = Rect::from_center_size(Pos2::new(centre, screen.height() * 0.5), Vec2::new(300.0, 64.0));
        painter.rect_filled(panel, RADIUS, PANEL.gamma_multiply(alpha));
        painter.rect_stroke(panel, RADIUS, (2.0, colour), egui::StrokeKind::Inside);
        painter.text(panel.center(), Align2::CENTER_CENTER, WRONG_WAY, font(40.0), colour);
    }
    if let Some((count, alpha)) = s.warning {
        let text = FALSE_START_WARNINGS[usize::from(count.clamp(1, 3)) - 1];
        let panel = Rect::from_center_size(Pos2::new(centre, screen.height() * 0.78), Vec2::new(560.0, 52.0));
        painter.rect_filled(panel, RADIUS, Color32::from_black_alpha((255.0 * alpha) as u8));
        let ink = TEXT.gamma_multiply((alpha * 2.0).min(1.0));
        painter.text(panel.center(), Align2::CENTER_CENTER, text, font(17.0), ink);
    }
    if !s.stack.is_empty() {
        paint_stack(painter, screen, &s.stack);
    }
    if let Some(view_projection) = r.pace_3d {
        paint_3d(painter, screen, view_projection, &s.stack);
    }
}

/// Points of the 480-line screen the game lays the HUD out on.
const GAME_POINT: f32 = REFERENCE_HEIGHT / 480.0;

/// The pace stack (`HUD::Update`, `PN_STACK`): a column of small symbols at the right (x 605 of 640), the next note at
/// y 190 + 46 times the way covered to it, the passed one 44 below it, the later ones above, 3 apart. The positions
/// are the game's; the symbols are ours.
fn paint_stack(painter: &Painter, screen: Rect, stack: &[StackNote]) {
    let size = pace::SIZE * 0.6;
    let x = screen.max.x - MARGIN - size.x / 2.0;
    let fraction = stack.first().map_or(1.0, |n| n.fraction);
    let mut y = screen.min.y + (190.0 + 46.0 * fraction) * GAME_POINT;
    for note in stack {
        let at = if note.passed { y + 44.0 * GAME_POINT } else { y };
        let rect = Rect::from_center_size(Pos2::new(x, at), size);
        painter.rect_filled(rect.expand(4.0), RADIUS, PANEL.gamma_multiply(note.alpha));
        pace::paint(painter, rect, &note.symbol, (note.alpha * 2.0).min(1.0));
        if !note.passed {
            y -= size.y + 3.0 * GAME_POINT + 8.0;
        }
    }
}

/// The 3D pace notes (`AID_MARKER`, 0x576680: the stack's next four keys, 0x574440 at each): a symbol over each key's
/// driveline point, 1.5 m up, sized by its distance. Ours: the game's 3D sprite is not ported, so the symbol is
/// drawn on the HUD at the projected point.
fn paint_3d(painter: &Painter, screen: Rect, view_projection: Mat4, stack: &[StackNote]) {
    for note in stack.iter().filter(|n| !n.passed).take(4) {
        let Some([x, y, z]) = note.position else { continue };
        let clip = view_projection * Vec3::new(x, z + 1.5, y).extend(1.0);
        if clip.w <= 0.5 {
            continue;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.x.abs() > 1.1 || ndc.y.abs() > 1.1 {
            continue;
        }
        let at = Pos2::new(
            screen.min.x + (ndc.x + 1.0) * 0.5 * screen.width(),
            screen.min.y + (1.0 - ndc.y) * 0.5 * screen.height(),
        );
        let size = pace::SIZE * (30.0 / clip.w).clamp(0.25, 1.0);
        pace::paint(painter, Rect::from_center_size(at, size), &note.symbol, 0.85);
    }
}
