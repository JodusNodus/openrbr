//! The bottom right dial: the engine speed round the rim, red past the rev limit, with the shift light over it; the
//! pedals inside it (brake and clutch up the left, throttle and handbrake up the right); the speed, gear and engine
//! speed in the middle with the water temperature and the turbo pressure beside the gear, and the steering under
//! them. The rim and the readouts show with [`Part::Dash`], the pedals and steering with [`Part::Inputs`].

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};

use super::{caption, digits, DIM, LAMP_ON, MARGIN, TEXT};
use crate::hud::{Part, Readout};

/// The rev limit the dial takes without the car's (SHF1). Ours.
const REV_LIMIT: f32 = 8500.0;
/// The digidash's shift light comes on at this engine speed (`E57DashData::UpdateFull`, 5500 rpm for every car); the
/// dial's takes the car's upshift point for the gear when SHF1 has one.
const SHIFT_RPM: f32 = 5500.0;
/// The rim's radius, points. Ours.
const DIAL: f32 = 80.0;
/// The pedal arcs: clutch, brake, throttle, handbrake.
const CLUTCH: Color32 = Color32::from_rgb(90, 150, 230);
const BRAKE: Color32 = Color32::from_rgb(225, 65, 55);
const THROTTLE: Color32 = Color32::from_rgb(70, 205, 100);
const HANDBRAKE: Color32 = Color32::from_rgb(235, 160, 50);
/// The sweep: from the bottom left (`START`, radians clockwise from +x on screen) round over the top to the bottom
/// right.
const START: f32 = 0.75 * std::f32::consts::PI;
const SWEEP: f32 = 1.5 * std::f32::consts::PI;

pub fn paint(painter: &Painter, screen: Rect, r: &Readout) {
    let (dash, inputs) = (r.parts.shows(Part::Dash), r.parts.shows(Part::Inputs));
    if !dash && !inputs {
        return;
    }
    let centre = screen.max - Vec2::splat(MARGIN + DIAL + 12.0);
    painter.circle_filled(centre, DIAL + 12.0, Color32::from_black_alpha(110));
    if dash {
        readouts(painter, centre, r);
    }
    if inputs {
        controls(painter, centre, r);
    }
}

/// The rev arc, the shift light, and the speed, gear, engine speed, water and turbo.
fn readouts(painter: &Painter, centre: Pos2, r: &Readout) {
    let (top, red) = rev_scale(r.state.rev_limit);
    let rpm = (r.rpm.unwrap_or(0.0) / top).clamp(0.0, 1.0);
    stroke_arc(painter, centre, DIAL, (0.0, red), 2.0, Color32::from_white_alpha(70));
    stroke_arc(painter, centre, DIAL, (red, 1.0), 6.0, Color32::from_rgb(150, 30, 30));
    stroke_arc(painter, centre, DIAL, (0.0, rpm.min(red)), 6.0, TEXT);
    stroke_arc(painter, centre, DIAL, (red, rpm.max(red)), 6.0, LAMP_ON);
    let shift = r.rpm.is_some_and(|rpm| rpm >= r.state.upshift.unwrap_or(SHIFT_RPM));
    let lamp = centre - Vec2::new(0.0, DIAL + 10.0);
    painter.circle_filled(lamp, 3.5, if shift { LAMP_ON } else { Color32::from_white_alpha(50) });

    let (speed, unit) = r.speed_in_unit();
    let at = centre - Vec2::new(0.0, 46.0);
    digits(painter, at, Align2::CENTER_CENTER, &format!("{speed:.0}"), FontId::proportional(20.0), TEXT);
    caption(painter, at + Vec2::new(0.0, 10.0), Align2::CENTER_TOP, unit);
    let (gear, font) = (r.gear.as_deref().unwrap_or("-"), FontId::proportional(52.0));
    digits(painter, centre + Vec2::new(0.0, 4.0), Align2::CENTER_CENTER, gear, font, TEXT);
    let revs = r.rpm.map_or("-".to_owned(), |rpm| format!("{:.0}", rpm.max(0.0)));
    let at = centre + Vec2::new(0.0, 40.0);
    digits(painter, at, Align2::CENTER_CENTER, &revs, FontId::proportional(15.0), TEXT);
    caption(painter, at + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, "rpm");
    let s = &r.state;
    let gauges = [
        (-42.0, s.water.map(|w| format!("{w:.0}°")), "water"),
        (42.0, s.boost.filter(|b| *b > 0.0).map(|b| format!("{:.2}", b / 100_000.0)), "bar"),
    ];
    for (x, value, name) in gauges {
        if let Some(value) = value {
            let at = centre + Vec2::new(x, 14.0);
            painter.text(at, Align2::CENTER_CENTER, value, FontId::proportional(12.0), TEXT);
            caption(painter, at + Vec2::new(0.0, 7.0), Align2::CENTER_TOP, name);
        }
    }
}

/// The pedal arcs, each filling its quarter from the bottom up, and the steering bar.
fn controls(painter: &Painter, centre: Pos2, r: &Readout) {
    let pedals = [
        (DIAL - 11.0, (0.0, r.brake), BRAKE),
        (DIAL - 17.0, (0.0, r.clutch), CLUTCH),
        (DIAL - 11.0, (1.0, r.throttle), THROTTLE),
        (DIAL - 17.0, (1.0, r.handbrake), HANDBRAKE),
    ];
    for (radius, (end, value), colour) in pedals {
        let span = |v: f32| (end, end + (0.5 - end) * v.clamp(0.0, 1.0) * 0.9);
        stroke_arc(painter, centre, radius, span(1.0), 3.0, Color32::from_white_alpha(25));
        stroke_arc(painter, centre, radius, span(value), 3.0, colour);
    }
    let bar = Rect::from_center_size(centre + Vec2::new(0.0, DIAL - 4.0), Vec2::new(56.0, 3.0));
    painter.rect_filled(bar, 1.5, Color32::from_white_alpha(40));
    let end = bar.center().x + bar.width() * 0.5 * r.steering.clamp(-1.0, 1.0);
    let x = bar.center().x;
    painter.rect_filled(Rect::from_x_y_ranges(x.min(end)..=x.max(end), bar.y_range()), 1.5, TEXT);
    painter.line_segment([Pos2::new(x, bar.min.y - 3.0), Pos2::new(x, bar.max.y + 3.0)], (1.0, DIM));
}

/// The top engine speed and where its red starts, of it: the car's rev limit (SHF1, else [`REV_LIMIT`]) is the red's
/// start, the top a tenth over it, to the next 1000 rpm.
fn rev_scale(limit: Option<f32>) -> (f32, f32) {
    let limit = limit.unwrap_or(REV_LIMIT);
    let top = (limit * 1.1 / 1000.0).ceil() * 1000.0;
    (top, limit / top)
}

/// Points along the arc at `radius`, from `from` to `to` of its sweep (0 the bottom left, 1 the bottom right).
fn arc(centre: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let steps = ((to - from).abs() * 120.0).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|i| {
            let angle = START + SWEEP * (from + (to - from) * i as f32 / steps as f32);
            centre + radius * Vec2::new(angle.cos(), angle.sin())
        })
        .collect()
}

fn stroke_arc(painter: &Painter, centre: Pos2, radius: f32, span: (f32, f32), width: f32, colour: Color32) {
    if span.0 != span.1 {
        painter.add(egui::Shape::line(arc(centre, radius, span.0, span.1), (width, colour)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scale_tops_a_tenth_over_the_rev_limit() {
        assert_eq!(rev_scale(Some(7500.0)), (9000.0, 7500.0 / 9000.0));
        assert_eq!(rev_scale(None), (10_000.0, 0.85));
    }
}
