//! The split comparison and the finish panel of the viewer's HUD, from the live split record (`rbr_recording::splits`).
//! After a split is passed its time and the difference from the reference time (`misc/times.ini`) show for
//! [`SHOW_SECONDS`] of stage time; once the finish is passed the panel shows the stage result: the time with its
//! penalties, the finish against its reference, and each split. Once the race is over (the game's end page, the
//! car no longer stepped) the panel says so, and a retired race shows a retire panel. The layout is ours; the game's
//! split display and end pages are not drawn as the original's. With the player's save files (`--saves`) the
//! comparisons are against the stage record, the finish panel names the record and the personal best, and a new
//! best shows the result page's message (`MSG_QRBEST`, `BEAT_PERSONAL`, with the 1.02 English text). Once the race is
//! over the finish panel is the result page (`CQRResultUI::Vf40` 0x5c3b10): the rows `STAGERECORD`, `PERSONALBEST` and
//! the player's (split 1, split 2, time), and a retired race names its end page (`flow::EndPage`).

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Vec2};
use rbr_recording::splits::{Split, Splits, Standing, Verdict};

use crate::hud::time_label;

/// How long a passed split's difference stays on screen, seconds of stage time. Ours.
pub const SHOW_SECONDS: f32 = 5.0;
/// The panel's width and its top, points: below the pace-note symbol.
const WIDTH: f32 = 300.0;
const TOP: f32 = 148.0;
const PAD: f32 = 10.0;
const ROW: f32 = 16.0;
const PANEL: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 150);
const TEXT: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(160);
/// Ahead of the reference, and behind it.
const AHEAD: Color32 = Color32::from_rgb(80, 200, 90);
const BEHIND: Color32 = Color32::from_rgb(220, 70, 60);

/// A split's difference from its reference, seconds, negative when ahead; `None` without a time or a reference.
pub fn difference(split: &Split) -> Option<f32> {
    Some(split.time? - split.reference?)
}

/// `+1.25` or `-0.40`.
pub fn signed(seconds: f32) -> String {
    format!("{seconds:+.2}")
}

/// The split to show now: the last one passed, when it was passed less than [`SHOW_SECONDS`] of stage time ago. Its
/// number, from 1, and the split.
pub fn flash(record: &Splits) -> Option<(usize, Split)> {
    let (index, split) = record.splits.iter().enumerate().rev().find(|(_, split)| split.time.is_some())?;
    let passed = split.time?;
    (record.elapsed - passed < SHOW_SECONDS).then_some((index + 1, *split))
}

/// The difference's colour: green ahead, red behind.
fn colour(difference: f32) -> Color32 {
    if difference < 0.0 {
        AHEAD
    } else {
        BEHIND
    }
}

/// The 1.02 English texts (`bstrings.eng`) of the result page.
pub const MSG_QRBEST: &str = "Congratulations %s! You have set a new record time for this stage.";
pub const BEAT_PERSONAL: &str = "Great drive! You've set a new personal record for the %s stage!";

/// Draws the split panel for `record` over the top of `screen`, when there is one to show; `page` is the race's end
/// page once it is over (FLW1).
pub fn paint(painter: &Painter, screen: Rect, record: &Splits, page: Option<u32>) {
    if record.retired {
        retired_panel(painter, screen, record, page);
    } else if record.over && record.standing.is_some() {
        result_page(painter, screen, record);
    } else if record.finished {
        finish_panel(painter, screen, record);
    } else if let Some((number, split)) = flash(record) {
        flash_panel(painter, screen, number, split);
    }
}

/// The split as it is passed: its number, its time and the difference.
fn flash_panel(painter: &Painter, screen: Rect, number: usize, split: Split) {
    let panel = Rect::from_min_size(
        Pos2::new(screen.center().x - WIDTH / 2.0, screen.min.y + TOP),
        Vec2::new(WIDTH, 2.0 * PAD + 16.0 + 32.0 + if split.reference.is_some() { ROW } else { 0.0 }),
    );
    painter.rect_filled(panel, 6.0, PANEL);
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
    painter.rect_filled(panel, 6.0, PANEL);
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

/// What a retired race's end page is (`flow::EndPage`'s numbers): the quick rally's ENDRACE (0x12), the
/// championship's FAILED after a retire (0x17) or after a finish in a car that cannot go on (0x18). Ours: the wording.
pub fn end_page_title(page: Option<u32>) -> &'static str {
    match page {
        Some(0x17) => "stage failed: retired",
        Some(0x18) => "rally over: the car cannot go on",
        _ => "retired",
    }
}

/// A retired race (DNF): the race time it was given up at, and the end page.
fn retired_panel(painter: &Painter, screen: Rect, record: &Splits, page: Option<u32>) {
    let panel = Rect::from_min_size(
        Pos2::new(screen.center().x - WIDTH / 2.0, screen.min.y + TOP),
        Vec2::new(WIDTH, 2.0 * PAD + 16.0 + 32.0 + ROW),
    );
    painter.rect_filled(panel, 6.0, PANEL);
    let mut at = panel.min + Vec2::splat(PAD);
    let title = format!("{}  (R: restart)", end_page_title(page));
    painter.text(at, Align2::LEFT_TOP, title, FontId::proportional(13.0), DIM);
    at.y += 16.0;
    painter.text(at, Align2::LEFT_TOP, "DNF", FontId::monospace(26.0), BEHIND);
    at.y += 32.0;
    let line = format!("given up at {}", time_label(f64::from(record.race_time)));
    painter.text(at, Align2::LEFT_TOP, line, FontId::proportional(13.0), TEXT);
}

/// The result page's message (`CQRResultBaseUI::DrawGS` 0x5c3880): `MSG_QRBEST` with the holder's name, or
/// `BEAT_PERSONAL` (the stage's name is not in the record: "this").
pub fn verdict_text(standing: &Standing) -> Option<String> {
    match standing.verdict {
        Verdict::None => None,
        Verdict::StageRecord => Some(MSG_QRBEST.replace("%s", &standing.record.holder)),
        Verdict::PersonalBest => Some(BEAT_PERSONAL.replace("the %s stage", "this stage")),
    }
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
    painter.rect_filled(panel, 6.0, PANEL);
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

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_recording::splits::Mark;

    fn record(elapsed: f32, splits: Vec<Split>) -> Splits {
        Splits { elapsed, splits, ..Splits::default() }
    }

    #[test]
    fn the_difference_is_negative_when_ahead_and_needs_both_values() {
        let ahead = Split { time: Some(57.0), reference: Some(57.5) };
        assert_eq!(difference(&ahead), Some(-0.5));
        assert_eq!(difference(&Split { time: Some(1.0), reference: None }), None);
        assert_eq!(difference(&Split { time: None, reference: Some(1.0) }), None);
        assert_eq!(signed(1.25), "+1.25");
        assert_eq!(signed(-0.4), "-0.40");
    }

    #[test]
    fn the_last_split_shows_for_five_seconds_after_it_is_passed() {
        let split = Split { time: Some(57.0), reference: Some(57.5) };
        let passed = record(57.0, vec![split, Split::default()]);
        assert_eq!(flash(&passed), Some((1, split)));
        assert_eq!(flash(&record(61.9, vec![split])), Some((1, split)));
        assert_eq!(flash(&record(62.0, vec![split])), None);
        assert_eq!(flash(&record(10.0, vec![Split::default()])), None, "nothing passed");
    }

    #[test]
    fn the_result_messages_are_the_games() {
        let record = Mark { holder: "thomas".into(), ..Mark::default() };
        let standing = Standing { record, personal: None, verdict: Verdict::StageRecord };
        assert_eq!(
            verdict_text(&standing).unwrap(),
            "Congratulations thomas! You have set a new record time for this stage."
        );
        let personal = Standing { verdict: Verdict::PersonalBest, ..standing.clone() };
        assert_eq!(verdict_text(&personal).unwrap(), "Great drive! You've set a new personal record for this stage!");
        assert_eq!(verdict_text(&Standing { verdict: Verdict::None, ..standing }), None);
        assert_eq!(end_page_title(Some(0x12)), "retired");
    }

    #[test]
    fn the_newest_passed_split_is_the_one_shown() {
        let first = Split { time: Some(57.0), reference: Some(57.5) };
        let second = Split { time: Some(119.0), reference: Some(118.0) };
        assert_eq!(flash(&record(120.0, vec![first, second])), Some((2, second)));
    }
}
