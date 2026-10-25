//! The top left clock and the stage's progress across the top.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};

use super::{digits, ALERT, DIM, MARGIN, PANEL, TEXT};
use crate::hud::{Part, Readout};

pub(super) const CLOCK_WIDTH: f32 = 250.0;
/// The clock's title chip.
const CHIP: Color32 = Color32::from_gray(205);
const CHIP_TEXT: Color32 = Color32::from_gray(25);

/// The clock ([`Readout::clock`]): a light chip naming it, the time in large digits, and a row for each note. The top
/// of the space under it.
pub fn clock(painter: &Painter, screen: Rect, r: &Readout) -> f32 {
    if !r.parts.shows(Part::Clock) {
        return screen.min.y + MARGIN;
    }
    let clock = r.clock();
    let left = screen.min + Vec2::splat(MARGIN);
    let chip = label_chip(painter, left, clock.title, CHIP, CHIP_TEXT);
    let time = Rect::from_min_size(Pos2::new(left.x, chip.max.y + 4.0), Vec2::new(CLOCK_WIDTH, 50.0));
    fade(painter, time);
    let at = Pos2::new(time.min.x + 10.0, time.center().y);
    digits(painter, at, Align2::LEFT_CENTER, &clock.time, FontId::proportional(40.0), TEXT);
    let mut top = time.max.y + 4.0;
    for (name, amount) in clock.notes {
        let row = Rect::from_min_size(Pos2::new(left.x, top), Vec2::new(CLOCK_WIDTH * 0.75, 20.0));
        fade(painter, row);
        let chip = label_chip(painter, row.min, name, Color32::from_white_alpha(60), TEXT);
        let at = Pos2::new(chip.max.x + 8.0, row.center().y);
        painter.text(at, Align2::LEFT_CENTER, amount, FontId::proportional(13.0), ALERT);
        top = row.max.y + 4.0;
    }
    top
}

/// The stage's progress across the top: the distance driven at the left, the stage's length at the right, the
/// line with the start, the splits and the car under it.
pub fn progress(painter: &Painter, screen: Rect, r: &Readout) {
    let s = &r.state;
    if !r.parts.shows(Part::Progress) || s.helicopter {
        return;
    }
    let Some((car, splits)) = &s.progress else { return };
    let line = Rect::from_center_size(
        Pos2::new(screen.center().x, screen.min.y + MARGIN + 10.0),
        Vec2::new(screen.width() * 0.4, 3.0),
    );
    painter.rect_filled(line.expand(1.0), 2.0, Color32::from_black_alpha(120));
    painter.rect_filled(line, 1.5, Color32::from_white_alpha(60));
    let x = line.min.x + line.width() * car.clamp(0.0, 1.0);
    painter.rect_filled(Rect::from_x_y_ranges(line.min.x..=x, line.y_range()), 1.5, Color32::from_white_alpha(150));
    painter.rect_filled(Rect::from_center_size(line.left_center(), Vec2::splat(6.0)), 0.0, TEXT);
    for split in splits {
        let x = line.min.x + line.width() * split;
        painter.line_segment([Pos2::new(x, line.min.y - 5.0), Pos2::new(x, line.max.y + 5.0)], (2.0, TEXT));
    }
    let tip = Pos2::new(x, line.max.y + 3.0);
    let diamond = [tip, tip + Vec2::new(4.0, 4.5), tip + Vec2::new(0.0, 9.0), tip + Vec2::new(-4.0, 4.5)];
    painter.add(egui::Shape::convex_polygon(diamond.to_vec(), TEXT, (1.0, Color32::from_black_alpha(120))));
    if let Some((driven, to_go)) = s.odometer {
        distance(painter, line.left_center() - Vec2::new(12.0, 0.0), Align2::RIGHT_CENTER, driven, r.mph);
        if let Some(to_go) = to_go {
            let length = driven + to_go.max(0.0);
            distance(painter, line.right_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, length, r.mph);
        }
    }
}

/// A distance in km (or miles with `mph`, the speed's unit; ours) to one decimal, its unit small after it.
fn distance(painter: &Painter, at: Pos2, align: Align2, metres: f32, mph: bool) {
    let (value, unit) = if mph { (metres / 1609.344, "mi") } else { (metres / 1000.0, "km") };
    let number = painter.layout_no_wrap(format!("{value:.1}"), FontId::proportional(17.0), TEXT);
    let unit = painter.layout_no_wrap(unit.to_owned(), FontId::proportional(10.0), DIM);
    let left = at.x - align.x().to_factor() * (number.size().x + 3.0 + unit.size().x);
    let number_at = Pos2::new(left, at.y - number.size().y / 2.0);
    let unit_at = Pos2::new(left + number.size().x + 3.0, at.y + number.size().y * 0.25 - unit.size().y);
    // On a dark pill, readable on a bright sky.
    let pill = Rect::from_min_size(number_at, number.size()).union(Rect::from_min_size(unit_at, unit.size()));
    painter.rect_filled(pill.expand2(Vec2::new(5.0, 1.0)), 3.0, Color32::from_black_alpha(110));
    painter.galley(number_at, number, TEXT);
    painter.galley(unit_at, unit, DIM);
}

/// A chip with `text` upper-cased, its top left at `at`; its rectangle.
fn label_chip(painter: &Painter, at: Pos2, text: &str, fill: Color32, ink: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_uppercase(), FontId::proportional(12.0), ink);
    let chip = Rect::from_min_size(at, Vec2::new(galley.size().x + 16.0, 20.0));
    painter.rect_filled(chip, 2.0, fill);
    painter.galley(chip.center() - galley.size() / 2.0, galley, ink);
    chip
}

/// The panel's fill across `rect`, fading out to the right.
fn fade(painter: &Painter, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    for (corner, colour) in [
        (rect.left_top(), PANEL),
        (rect.right_top(), Color32::TRANSPARENT),
        (rect.right_bottom(), Color32::TRANSPARENT),
        (rect.left_bottom(), PANEL),
    ] {
        mesh.colored_vertex(corner, colour);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}
