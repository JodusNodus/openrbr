//! The pace-note symbols drawn as shapes, ours: not the game's `GEN_BOX_PACE_VERSION1_*` sprites.

use egui::{epaint::PathStroke, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use crate::hud::pace::{Kind, Symbol};

fn colour(rgb: [f32; 3], alpha: f32) -> Color32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgba_unmultiplied(c(rgb[0]), c(rgb[1]), c(rgb[2]), c(alpha))
}

/// Draws `symbol` in `rect` (square) at `alpha`.
pub fn paint(painter: &Painter, rect: Rect, symbol: &Symbol, alpha: f32) {
    let ink = colour(symbol.colour, alpha);
    let stroke = Stroke::new(rect.width() * 0.09, ink);
    let c = rect.center();
    let r = rect.width() * 0.5;
    // Mirror x for the left symbols.
    let side = if symbol.left { -1.0 } else { 1.0 };
    let at = |x: f32, y: f32| Pos2::new(c.x + side * x * r, c.y + y * r);
    let line = |points: Vec<Pos2>| painter.add(Shape::line(points, PathStroke::from(stroke)));
    match symbol.kind {
        Kind::Corner(grade) => {
            // An arrow up from the bottom that turns right by 30 degrees per grade, with the grade beside it.
            let turn = f32::from(grade) * 30f32.to_radians();
            let radius = 0.42;
            let x0 = -radius;
            let centre = (x0 + radius, 0.0);
            let mut units = vec![(x0, 0.85), (x0, 0.0)];
            for k in 1..=24 {
                let a = turn * k as f32 / 24.0;
                units.push((centre.0 - radius * a.cos(), centre.1 - radius * a.sin()));
            }
            let end = *units.last().unwrap();
            // The direction of travel at the arc's end (y down), its normal, and the head past the end.
            let (dx, dy) = (turn.sin(), -turn.cos());
            let tip = (end.0 + dx * 0.32, end.1 + dy * 0.32);
            let head = [-1.0, 1.0].map(|s| at(end.0 - dy * 0.22 * s, end.1 + dx * 0.22 * s));
            line(units.iter().map(|&(x, y)| at(x, y)).collect());
            painter.add(Shape::convex_polygon(vec![at(tip.0, tip.1), head[0], head[1]], ink, Stroke::NONE));
            painter.text(at(0.72, 0.62), Align2::CENTER_CENTER, grade.to_string(), FontId::proportional(r * 0.6), ink);
        }
        Kind::Twisty => {
            line((0..7).map(|k| at(if k % 2 == 0 { -0.35 } else { 0.35 }, 0.75 - 0.25 * k as f32)).collect());
        }
        Kind::Narrow => {
            line(vec![at(-0.6, 0.7), at(-0.2, 0.0), at(-0.2, -0.7)]);
            line(vec![at(0.6, 0.7), at(0.2, 0.0), at(0.2, -0.7)]);
        }
        Kind::Crest => {
            line(
                (0..=16)
                    .map(|k| {
                        let x = -0.75 + 1.5 * k as f32 / 16.0;
                        at(x, 0.35 - 0.6 * (1.0 - (x / 0.75).powi(2)))
                    })
                    .collect(),
            );
        }
        Kind::Water => {
            for row in [-0.3, 0.2] {
                line(
                    (0..=16)
                        .map(|k| {
                            let x = -0.7 + 1.4 * k as f32 / 16.0;
                            at(x, row + 0.12 * (x * 9.0).sin())
                        })
                        .collect(),
                );
            }
        }
        Kind::Caution => {
            let points = vec![at(0.0, -0.7), at(0.7, 0.6), at(-0.7, 0.6), at(0.0, -0.7)];
            line(points);
            painter.text(at(0.0, 0.1), Align2::CENTER_CENTER, "!", FontId::proportional(r * 0.8), ink);
        }
        Kind::Flat => {
            line(vec![at(-0.15, 0.75), at(0.15, -0.55)]);
            painter.add(Shape::convex_polygon(vec![at(0.2, -0.8), at(0.35, -0.4), at(-0.05, -0.5)], ink, Stroke::NONE));
        }
        Kind::Bridge => {
            line(vec![at(-0.7, -0.15), at(0.7, -0.15)]);
            line(vec![at(-0.7, 0.25), at(0.7, 0.25)]);
            for x in [-0.4, 0.0, 0.4] {
                line(vec![at(x, 0.25), at(x, 0.65)]);
            }
        }
        Kind::Straight => {
            line(vec![at(0.0, 0.75), at(0.0, -0.45)]);
            painter.add(Shape::convex_polygon(vec![at(0.0, -0.8), at(0.25, -0.4), at(-0.25, -0.4)], ink, Stroke::NONE));
        }
        Kind::Text(word) => {
            // The words wrapped to the box, centred.
            let mut job =
                egui::text::LayoutJob::simple(word.to_owned(), FontId::proportional(r * 0.4), ink, rect.width());
            job.halign = egui::Align::Center;
            let galley = painter.layout_job(job);
            painter.galley(c - Vec2::new(0.0, galley.size().y / 2.0), galley, ink);
        }
    }
}

/// The symbol's box size, points.
pub const SIZE: Vec2 = Vec2::splat(72.0);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::pace::Kind;

    /// Every symbol paints without panicking, on a headless context (no GPU: shapes only).
    #[test]
    fn every_symbol_paints() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
            ..Default::default()
        };
        let mut fullscreen = ctx.run_pass(raw, |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            let symbols = [
                Kind::Corner(1),
                Kind::Corner(6),
                Kind::Twisty,
                Kind::Narrow,
                Kind::Crest,
                Kind::Water,
                Kind::Caution,
                Kind::Flat,
                Kind::Bridge,
                Kind::Straight,
                Kind::Text("CUT"),
                Kind::Text("DON'T CUT"),
                Kind::Text("TIGHTENS BAD"),
                Kind::Text("A VERY LONG CUSTOM CALL NAME"),
            ];
            for (k, kind) in symbols.into_iter().enumerate() {
                let symbol = Symbol { kind, left: k % 2 == 0, colour: [1.0, 1.0, 1.0], tag: k as u32 };
                paint(&painter, Rect::from_center_size(Pos2::new(100.0, 100.0), SIZE), &symbol, 1.0);
            }
        });
        let _ = ctx.tessellate(fullscreen.shapes, fullscreen.pixels_per_point);
        fullscreen.textures_delta.clear();
    }
}
