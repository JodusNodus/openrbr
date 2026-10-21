//! The split panels under the pace note (`crate::hud::splits`): a passed split's time and difference; once the finish
//! is passed the stage result (the time with its penalties, the finish against its reference, each split, and with
//! `--saves` the stage record and the personal best); once the race is over the result page (`CQRResultUI::Vf40`
//! 0x5c3b10: the rows `STAGERECORD`, `PERSONALBEST` and the player's, each with split 1, split 2 and the time), and a
//! retired race's end page. The layout is ours.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};
use rbr_recording::splits::{Split, Splits, Standing, Verdict};

use super::ACCENT;
use crate::hud::splits::{self, difference, end_page_title, signed, verdict_text};
use crate::hud::time_label;

/// The panel's width and its top, points: below the pace-note symbol.
const WIDTH: f32 = 300.0;
const TOP: f32 = 192.0;
const PAD: f32 = 10.0;
const ROW: f32 = 16.0;
const TEXT: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(160);
/// Ahead of the reference, and behind it.
const AHEAD: Color32 = Color32::from_rgb(80, 200, 90);
const BEHIND: Color32 = Color32::from_rgb(220, 70, 60);

/// The difference's colour: green ahead, red behind.
fn colour(difference: f32) -> Color32 {
    if difference < 0.0 {
        AHEAD
    } else {
        BEHIND
    }
}

/// Draws the split panel for `record` over the top of `screen`, when there is one to show; `page` is the race's end
/// page once it is over (FLW1). `flash` shows a passed split's panel; the finish and end pages always show.
pub fn paint(painter: &Painter, screen: Rect, record: &Splits, page: Option<u32>, flash: bool) {
    if record.retired {
        retired_panel(painter, screen, record, page);
    } else if record.over && record.standing.is_some() {
        result_page(painter, screen, record);
    } else if record.finished {
        finish_panel(painter, screen, record);
    } else if let Some((number, split)) = splits::flash(record).filter(|_| flash) {
        flash_panel(painter, screen, number, split);
    }
}

/// The split as it is passed: its number, its time and the difference.
fn flash_panel(painter: &Painter, screen: Rect, number: usize, split: Split) {
    let panel = Rect::from_min_size(
        Pos2::new(screen.center().x - WIDTH / 2.0, screen.min.y + TOP),
        Vec2::new(WIDTH, 2.0 * PAD + 16.0 + 32.0 + if split.reference.is_some() { ROW } else { 0.0 }),
    );
    super::panel(painter, panel, difference(&split).map_or(DIM, colour), 1.0);
    let mut at = panel.min + Vec2::splat(PAD);
    painter.text(at, Align2::LEFT_TOP, format!("split {number}"), FontId::proportional(13.0), DIM);
    at.y += 16.0;
    let time = split.time.map_or("-".to_owned(), |t| time_label(f64::from(t)));
    painter.text(at, Align2::LEFT_TOP, time, FontId::monospace(26.0), TEXT);
    let right = Pos2::new(panel.max.x - PAD, at.y + 6.0);
    match difference(&split) {
        Some(diff) => {
            painter.text(right, Align2::RIGHT_TOP, format!("{} s", signed(diff)), FontId::monospace(22.0), colour(diff))
        }
        None => painter.text(right, Align2::RIGHT_TOP, "no reference", FontId::proportional(13.0), DIM),
    };
    if let Some(reference) = split.reference {
        at.y += 32.0;
        let line = format!("record {}", time_label(f64::from(reference)));
        painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), DIM);
    }
}

/// The stage result once the finish is passed.
fn finish_panel(painter: &Painter, screen: Rect, record: &Splits) {
    let rows = 4 + record.splits.len() + standing_rows(record.standing.as_ref());
    let panel = Rect::from_min_size(
        Pos2::new(screen.center().x - WIDTH / 2.0, screen.min.y + TOP),
        Vec2::new(WIDTH, 2.0 * PAD + 16.0 + 32.0 + ROW * rows as f32),
    );
    super::panel(painter, panel, ACCENT, 1.0);
    let mut at = panel.min + Vec2::splat(PAD);
    let title = if record.over { "stage over  (R: restart)" } else { "finish" };
    painter.text(at, Align2::LEFT_TOP, title, FontId::proportional(13.0), DIM);
    at.y += 16.0;
    painter.text(at, Align2::LEFT_TOP, time_label(f64::from(record.race_time)), FontId::monospace(26.0), TEXT);
    at.y += 32.0;
    let penalties = if record.penalties > 0.0 {
        format!("penalties +{:.0} s included", record.penalties)
    } else {
        "no penalties".to_owned()
    };
    painter.text(at, Align2::LEFT_TOP, penalties, FontId::proportional(13.0), Color32::from_rgb(240, 160, 60));
    at.y += ROW;
    if let Some(reference) = record.reference_finish {
        let diff = record.race_time - reference;
        let line = format!("{} s against {}", signed(diff), time_label(f64::from(reference)));
        painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), colour(diff));
    }
    at.y += ROW;
    if let Some(standing) = &record.standing {
        at = standing_lines(painter, at, standing);
    }
    for (i, split) in record.splits.iter().enumerate() {
        let time = split.time.map_or("not passed".to_owned(), |t| time_label(f64::from(t)));
        let line = format!("split {}  {time}", i + 1);
        painter.text(at, Align2::LEFT_TOP, line, FontId::monospace(13.0), TEXT);
        if let Some(diff) = difference(split) {
            let right = Pos2::new(panel.max.x - PAD, at.y);
            painter.text(right, Align2::RIGHT_TOP, signed(diff), FontId::monospace(13.0), colour(diff));
        }
        at.y += ROW;
    }
}

/// The rows [`standing_lines`] draws.
fn standing_rows(standing: Option<&Standing>) -> usize {
    standing.map_or(0, |s| 1 + usize::from(s.personal.is_some()) + usize::from(s.verdict != Verdict::None))
}

/// The result page's message and the stage record and the personal best; the position after them.
fn standing_lines(painter: &Painter, mut at: Pos2, standing: &Standing) -> Pos2 {
    let message = match standing.verdict {
        Verdict::None => None,
        Verdict::PersonalBest => Some("new personal best"),
        Verdict::StageRecord => Some("new stage record"),
    };
    if let Some(message) = message {
        painter.text(at, Align2::LEFT_TOP, message, FontId::proportional(14.0), AHEAD);
        at.y += ROW;
    }
    let record = &standing.record;
    let line = format!("stage record  {}  {}", time_label(f64::from(record.time)), record.holder);
    painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), DIM);
    at.y += ROW;
    if let Some(best) = &standing.personal {
        let line = format!("personal best  {}", time_label(f64::from(best.time)));
        painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), DIM);
        at.y += ROW;
    }
    at
}

/// A retired race (DNF): the race time it was given up at, and the end page.
fn retired_panel(painter: &Painter, screen: Rect, record: &Splits, page: Option<u32>) {
    let panel = Rect::from_min_size(
        Pos2::new(screen.center().x - WIDTH / 2.0, screen.min.y + TOP),
        Vec2::new(WIDTH, 2.0 * PAD + 16.0 + 32.0 + ROW),
    );
    super::panel(painter, panel, BEHIND, 1.0);
    let mut at = panel.min + Vec2::splat(PAD);
    let title = format!("{}  (R: restart)", end_page_title(page));
    painter.text(at, Align2::LEFT_TOP, title, FontId::proportional(13.0), DIM);
    at.y += 16.0;
    painter.text(at, Align2::LEFT_TOP, "DNF", FontId::monospace(26.0), BEHIND);
    at.y += 32.0;
    let line = format!("given up at {}", time_label(f64::from(record.race_time)));
    painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), TEXT);
}

/// The quick rally's result page (end page 0x10) once the race is over: the stage time with its penalties, the
/// message, and the three rows of `CQRResultUI::Vf40` (the stage record, the personal best or else this race, and the
/// player's race) with split 1, split 2 and the time. The layout is ours.
fn result_page(painter: &Painter, screen: Rect, record: &Splits) {
    let Some(standing) = &record.standing else { return };
    let message = verdict_text(standing);
    let width = 520.0;
    let height = 2.0 * PAD + 16.0 + 32.0 + ROW * (5.0 + if message.is_some() { 1.5 } else { 0.0 });
    let panel =
        Rect::from_min_size(Pos2::new(screen.center().x - width / 2.0, screen.min.y + TOP), Vec2::new(width, height));
    super::panel(painter, panel, ACCENT, 1.0);
    let mut at = panel.min + Vec2::splat(PAD);
    painter.text(at, Align2::LEFT_TOP, "stage result  (R: restart)", FontId::proportional(13.0), DIM);
    at.y += 16.0;
    painter.text(at, Align2::LEFT_TOP, time_label(f64::from(record.race_time)), FontId::monospace(26.0), TEXT);
    let penalties = if record.penalties > 0.0 {
        format!("penalties +{:.0} s included", record.penalties)
    } else {
        "no penalties".to_owned()
    };
    let right = Pos2::new(panel.max.x - PAD, at.y + 8.0);
    painter.text(right, Align2::RIGHT_TOP, penalties, FontId::proportional(13.0), Color32::from_rgb(240, 160, 60));
    at.y += 32.0;
    if let Some(message) = message {
        painter.text(at, Align2::LEFT_TOP, message, FontId::proportional(14.0), AHEAD);
        at.y += ROW * 1.5;
    }
    let columns = [190.0, 290.0, 390.0];
    let row = |at: Pos2, cells: [String; 4], colour: Color32| {
        painter.text(at, Align2::LEFT_TOP, &cells[0], FontId::proportional(13.0), colour);
        for (x, cell) in columns.iter().zip(&cells[1..]) {
            painter.text(Pos2::new(panel.min.x + x, at.y), Align2::LEFT_TOP, cell, FontId::monospace(13.0), colour);
        }
    };
    let t = |x: f32| if x > 0.0 && x.is_finite() { time_label(f64::from(x)) } else { "-".to_owned() };
    row(at, ["".into(), "Split 1".into(), "Split 2".into(), "Time".into()], DIM);
    at.y += ROW;
    let held = &standing.record;
    let name = format!("Stage Record  {}", held.holder);
    row(at, [name, t(held.splits[0]), t(held.splits[1]), t(held.time)], TEXT);
    at.y += ROW;
    let split = |i: usize| record.splits.get(i).and_then(|s| s.time).unwrap_or(0.0);
    let player = [t(split(0)), t(split(1)), t(record.race_time)];
    let best = match &standing.personal {
        Some(best) => [t(best.splits[0]), t(best.splits[1]), t(best.time)],
        None => player.clone(),
    };
    let [a, b, c] = best;
    row(at, ["Personal Best".into(), a, b, c], TEXT);
    at.y += ROW;
    let [a, b, c] = player;
    row(at, ["Player".into(), a, b, c], AHEAD);
    at.y += ROW;
    if let Some(reference) = record.reference_finish {
        let diff = record.race_time - reference;
        let line = format!("{} s against {}", signed(diff), time_label(f64::from(reference)));
        painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), colour(diff));
    }
}
