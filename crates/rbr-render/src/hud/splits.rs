//! The split comparison and the finish from the live split record (`rbr_recording::splits`): a passed split's
//! difference from its reference (`misc/times.ini`, or with `--saves` the stage record) for [`SHOW_SECONDS`] of stage
//! time, the result page's message (`CQRResultBaseUI::DrawGS`, 1.02's English texts) and the end page's title.

use rbr_recording::splits::{Split, Splits, Standing, Verdict};

/// How long a passed split's difference stays on screen, seconds of stage time. Ours.
pub const SHOW_SECONDS: f32 = 5.0;

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

/// The 1.02 English texts (`bstrings.eng`) of the result page.
pub const MSG_QRBEST: &str = "Congratulations %s! You have set a new record time for this stage.";
pub const BEAT_PERSONAL: &str = "Great drive! You've set a new personal record for the %s stage!";

/// What a retired race's end page is (`flow::EndPage`'s numbers): the quick rally's ENDRACE (0x12), the
/// championship's FAILED after a retire (0x17) or after a finish in a car that cannot go on (0x18). Ours: the wording.
pub fn end_page_title(page: Option<u32>) -> &'static str {
    match page {
        Some(0x17) => "stage failed: retired",
        Some(0x18) => "rally over: the car cannot go on",
        _ => "retired",
    }
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
