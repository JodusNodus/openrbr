//! The stage result: how the stage ended, its time with the penalties, and the split times. The time is the race
//! clock's (`race_clock`, +0x140), which `E60Flow` reads at the end of the race (`notes/gameplay-loop.md`). The splits
//! are the HUD's (+0x258...): a split key (0x17) in `Load3b0` hands the race clock, penalties included, to
//! `M572da0` (0x478731 is the only call of 0x572da0 in the exe), and the end race, the result page and the record take
//! them from there. The time card (`timing`, `CSigCheckPointTime`) gives only how many split keys the stage has.
//!
//! What the original shows (`CQRResultUI`: driver, split 1, split 2, total time, the stage record and the personal
//! best; `CHotSeatRallyStageResultsUI`: split, time and difference) is the layout this follows. The stage record and
//! personal best are not here: they need the save data. The outcome is the race flow's (`flow::StageEnd`) once the race
//! is over: `Retired` for a `CSigEndRace` with the state word 1 (the helicopter's bystander retire, off the stage, the
//! time limit), with its cause. Before that the race is `Running`, or `Finished` once the finish line is passed.

use crate::flow::{EndCause, StageEnd};
use crate::race_clock::RaceClock;
use crate::timing::TimeCard;

/// How the stage ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The race is on: neither over nor past the finish line.
    Running,
    /// The finish line was passed (the card's finish time is set), or the race ended finished.
    Finished,
    /// The race ended retired (DNF).
    Retired,
}

/// The stage result.
#[derive(Debug, Clone, PartialEq)]
pub struct StageResult {
    pub outcome: Outcome,
    /// What ended the race, once it is over.
    pub cause: Option<EndCause>,
    /// The race time at the finish, penalties included (the clock stops there). `None` unless finished.
    pub time: Option<f32>,
    /// The penalties the race clock took: the outside-assistance penalties and the jump start, in s.
    pub penalties: f32,
    /// Each split key's time, s from the start, in key order; `None` for a split the car has not passed.
    pub splits: Vec<Option<f32>>,
}

/// The stage result from the card, the HUD's split times so far (`RaceFlow::splits`), the race clock and the penalties
/// the game added (`Game::penalties`).
pub fn stage_result(card: &TimeCard, splits: &[f32], clock: RaceClock, penalties: f32) -> StageResult {
    let finished = card.finish_time.is_some();
    StageResult {
        outcome: if finished { Outcome::Finished } else { Outcome::Running },
        cause: None,
        time: finished.then_some(clock.time),
        penalties,
        splits: (0..card.splits().len()).map(|i| splits.get(i).copied()).collect(),
    }
}

/// The stage result once the race flow has ended the race (`flow::StageEnd`): `Finished` when the end was not a
/// retire, with the end's race time (the clock, stopped at the finish); `Retired` otherwise.
pub fn ended(end: &StageEnd, card: &TimeCard, splits: &[f32], penalties: f32) -> StageResult {
    let clock = RaceClock { time: end.end_race.time, ..RaceClock::default() };
    let outcome = if end.finished() { Outcome::Finished } else { Outcome::Retired };
    StageResult {
        outcome,
        cause: Some(end.cause),
        time: (outcome == Outcome::Finished).then_some(clock.time),
        ..stage_result(card, splits, clock, penalties)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::TimeCard;

    fn card() -> TimeCard {
        TimeCard::default().set_start(100.0).set_finish(400.0).push(200.0).push(300.0).refresh()
    }

    #[test]
    fn a_running_stage_has_no_time_and_its_passed_splits() {
        let card = card().set_started(10.0).append(0, 25.5).refresh();
        let clock = RaceClock { time: 40.0, running: true, ..RaceClock::default() };
        let result = stage_result(&card, &[27.25], clock, 10.0);
        assert_eq!((result.outcome, result.cause), (Outcome::Running, None));
        assert_eq!(result.time, None);
        assert_eq!(result.penalties, 10.0);
        // The HUD's split (the race clock at the key), not the card's 15.5 s.
        assert_eq!(result.splits, [Some(27.25), None]);
    }

    #[test]
    fn a_finished_stage_takes_the_race_clock_with_its_penalties() {
        let card = card().set_started(10.0).append(0, 25.5).append(1, 40.0).set_finish_time(60.0);
        let clock = RaceClock { time: 70.0, running: false, ..RaceClock::default() };
        let result = stage_result(&card, &[27.25, 42.5], clock, 20.0);
        assert_eq!(result.outcome, Outcome::Finished);
        assert_eq!(result.time, Some(70.0));
        assert_eq!(result.splits, [Some(27.25), Some(42.5)]);
    }

    #[test]
    fn the_race_flow_end_decides_the_outcome_and_the_time() {
        use crate::flow::EndRace;
        let card = card().set_started(10.0).append(0, 25.5).append(1, 40.0).set_finish_time(60.0);
        let end = |retired| StageEnd {
            cause: EndCause::Finish,
            end_race: EndRace { time: 70.0, splits: [15.5, 30.0], retired },
        };
        let finished = ended(&end(false), &card, &[15.5, 30.0], 10.0);
        assert_eq!((finished.outcome, finished.time, finished.penalties), (Outcome::Finished, Some(70.0), 10.0));
        assert_eq!(finished.splits, [Some(15.5), Some(30.0)]);
        let retired = ended(&end(true), &card, &[15.5, 30.0], 10.0);
        assert_eq!((retired.outcome, retired.time, retired.cause), (Outcome::Retired, None, Some(EndCause::Finish)));
    }

    #[test]
    fn a_stage_with_no_splits_has_none() {
        let card = TimeCard::default().set_finish(50.0).set_started(0.0).set_finish_time(5.0);
        let result = stage_result(&card, &[], RaceClock::default(), 0.0);
        assert!(result.splits.is_empty());
        assert_eq!(result.outcome, Outcome::Finished);
    }
}
