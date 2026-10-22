//! The stage time card and the timekeeper that fills it: start, splits and finish of one stage.
//!
//! `NRegulation::CTimeKeeper` (`Load` 0x4a01f0, `Check` 0x4a0450, `Record` 0x4a0540, `Update` 0x4a0320) keeps a
//! `CTimeCard` (`src/rbr/RBR_E21.h`; constructor 0x49ff80, `Refresh` 0x4a0040, `Push` 0x4a0130, `Append` 0x4a00e0).
//! `CCar::Vf6cx` calls `Record` once per car step with the car's driveline distance (`driveline::sample_x` of +0x17c)
//! and the elapsed time (the double at `CState` +0x20, before the step advances it: [`advance_time`]). The keys come
//! from `rbr_formats::markers`.
//!
//! The behaviour is the original's assembly. Where the C++ reference or the header disagree, the README says which
//! one this follows. Field names are the original's where known (`start` is `m_f30`, `passed` is `m_count2`).

use rbr_formats::markers::{StageMarkers, MAX_SPLITS};

/// The card's time origin before the start, the double at 0x730b48 (12 hours). `Check` is then negative on every
/// step, so the start is found by the distance alone.
const UNSTARTED_ORIGIN: f64 = 43200.0;
/// `Check`'s speed start applies only while the elapsed time is above this, in seconds (0x730be0).
const SPEED_START_WINDOW: f32 = -1.5;
/// `Check`'s speed start: the car's speed, `(x - last) / MaxTimeStep`, must exceed this in m/s (0x795e18).
const SPEED_START: f32 = 0.5;
/// `Update`: the distance under which the car counts as stopped, in metres (0x72d150).
const STILL_DISTANCE: f32 = 4.0;
/// `Update` samples the card once per this many steps.
const SAMPLE_STEPS: u32 = 100;
/// The elapsed time before the first step, the double at 0x73f878 (`CState` +0x20 is seeded with it).
pub const CLOCK_START: f64 = 43185.0;

/// One substep of the clock (`StepForward`, 0x5273a0): `dt` is added to the elapsed time in f32 and the sum widened to
/// the double (`m_d20 = (double)((float)dt + (float)m_d20)`). The f32 sum is the bits that matter: near 43185 s its
/// ulp is 1/256 s, so `0.007` advances the clock by 2/256 s. [`CLOCK_START`] and `dt` make the peeked clock exact.
/// `pc53`: NGP's race steps run the x87 at 53 bits (`notes/ngp.md`, x87 precision), so the sum stays a double
/// (`rbr_physics::physics_core::pacing::advance_time`); at f32 NGP's 1/720 s would leave the clock at 43185.
pub fn advance_time(elapsed: f64, dt: f32, pc53: bool) -> f64 {
    if pc53 {
        f64::from(dt) + elapsed
    } else {
        f64::from(dt + elapsed as f32)
    }
}

/// One split key and, once the car has passed it, the elapsed time then (`CTimeCard::Entry`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Split {
    /// Driveline distance of the key.
    pub distance: f32,
    /// Elapsed time when `Append` marked the split passed; `None` until then.
    pub time: Option<f64>,
}

/// The stage's time card (`CTimeCard`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeCard {
    /// Start line distance (`m_f30`); 0 when the stage has no start key.
    pub start: f32,
    /// Finish line distance (`m_f48`); 0 when the stage has no finish key.
    pub finish: f32,
    splits: [Split; MAX_SPLITS],
    count: usize,
    /// Splits passed (`m_count2`): the next split is `splits[passed]`.
    pub passed: usize,
    /// The distance the car is checked against next (`m_cur`), as of the last `refresh`.
    pub cur: f32,
    /// The car's distance at the last `record` (`m_f1c`).
    pub last: f32,
    /// Timing has started (`m_flag40`).
    pub started: bool,
    /// Elapsed-time origin: the start time once started (`m_d38`).
    pub origin: f64,
    /// Finish time, once the finish is passed (`m_flag58`, `m_d50`).
    pub finish_time: Option<f64>,
    /// While nonzero, `Record` does not run `Check` (`m_i24`; `Set24` sets it). What sets it is not traced.
    pub hold: i32,
}

impl Default for TimeCard {
    /// The constructor (0x49ff80) with no splits, as `CTimeKeeper::Init` makes it.
    fn default() -> Self {
        Self {
            start: 0.0,
            finish: 0.0,
            splits: [Split::default(); MAX_SPLITS],
            count: 0,
            passed: 0,
            cur: 0.0,
            last: 0.0,
            started: false,
            origin: UNSTARTED_ORIGIN,
            finish_time: None,
            hold: 0,
        }
    }
}

/// A checkpoint the timekeeper emits (`CSigCheckPointTime`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    pub kind: CheckpointKind,
    /// Elapsed time since the start, as the signal carries it (`cur` in the original, not the distance).
    pub time: f32,
    /// True when `Check` started timing (the original's fourth payload word). False for the start `Record` makes
    /// (the car did not pass the line in `Check`), and for splits and the finish.
    pub fired: bool,
}

/// What the checkpoint marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointKind {
    Start,
    /// Split `n`, counting from 0.
    Split(usize),
    Finish,
}

impl TimeCard {
    /// `Push` (0x4a0130): adds a split key, unless the card already holds [`MAX_SPLITS`].
    pub fn push(mut self, distance: f32) -> Self {
        if self.count < MAX_SPLITS {
            self.splits[self.count] = Split { distance, time: None };
            self.count += 1;
        }
        self
    }

    /// `Refresh` (0x4a0040): the distance the card is checked against next.
    pub fn refresh(mut self) -> Self {
        self.cur = if !self.started {
            self.start
        } else if self.passed < self.count {
            self.splits[self.passed].distance
        } else {
            self.finish
        };
        self
    }

    /// `Append` (0x4a00e0): marks split `i` passed at elapsed time `t`. Ignored unless `i` is a split.
    pub fn append(mut self, i: usize, t: f64) -> Self {
        if i < self.count {
            self.splits[i].time = Some(t);
            self.passed = i + 1;
        }
        self
    }

    /// `Set30` (0x4a0110): the start distance. Does not refresh.
    pub fn set_start(mut self, distance: f32) -> Self {
        self.start = distance;
        self
    }

    /// `Set48` (0x4a0120): the finish distance. Does not refresh.
    pub fn set_finish(mut self, distance: f32) -> Self {
        self.finish = distance;
        self
    }

    /// `Set24` (0x4a0070): the hold that stops `Record` from running `Check`.
    pub fn set_hold(mut self, hold: i32) -> Self {
        self.hold = hold;
        self
    }

    /// `SetA` (0x4a00a0): timing starts at elapsed time `t`.
    pub fn set_started(mut self, t: f64) -> Self {
        self.started = true;
        self.origin = t;
        self
    }

    /// `SetB` (0x4a00c0): the finish is passed at elapsed time `t`.
    pub fn set_finish_time(mut self, t: f64) -> Self {
        self.finish_time = Some(t);
        self
    }

    /// The split keys in ascending distance order.
    pub fn splits(&self) -> &[Split] {
        &self.splits[..self.count]
    }

    /// The elapsed time since the origin at `t`: to the finish once it is passed (`cur` in `Check` and `Record`).
    fn elapsed(&self, t: f64) -> f32 {
        (self.finish_time.unwrap_or(t) - self.origin) as f32
    }

    /// `CTimeKeeper::Check` (0x4a0450), before the start: starts timing when the car is past the start line, or
    /// just short of it and moving fast enough (`dt` is `PhysicsEngine.MaxTimeStep`). Returns the checkpoint it
    /// emits; `None` leaves the card unchanged.
    fn check(self, x: f32, t: f64, dt: f32) -> (Self, Option<Checkpoint>) {
        let cur = self.elapsed(t);
        if cur >= 0.0 {
            let card = self.set_started(t);
            let checkpoint = Checkpoint { kind: CheckpointKind::Start, time: cur, fired: false };
            return (card, Some(checkpoint));
        }
        let speed = (x - self.last) / dt;
        let starts = x > self.start || (cur > SPEED_START_WINDOW && speed > SPEED_START);
        if !starts {
            return (self, None);
        }
        let checkpoint = Checkpoint { kind: CheckpointKind::Start, time: cur, fired: true };
        (self.set_started(t), Some(checkpoint))
    }

    /// `CTimeKeeper::Record` (0x4a0540): one car step at driveline distance `x` and elapsed time `t`. Before the
    /// start, `check` runs unless held; the rest passes the next key the car has reached. Returns the card and the
    /// checkpoint emitted, at most one per step.
    pub fn record(self, x: f32, t: f64, dt: f32) -> (Self, Option<Checkpoint>) {
        let (card, checked) = if !self.started && self.hold == 0 { self.check(x, t, dt) } else { (self, None) };
        if checked.is_some() {
            return (card.set_last(x).refresh(), checked);
        }
        card.pass(x, t)
    }

    /// The part of `Record` after `Check`: short of the next key only `last` moves; at or past it, the key is
    /// passed. The first key is the start, then each split, then the finish.
    fn pass(self, x: f32, t: f64) -> (Self, Option<Checkpoint>) {
        if x < self.cur {
            return (self.set_last(x), None);
        }
        let time = self.elapsed(t);
        let (card, kind) = if !self.started {
            (self.set_started(t), CheckpointKind::Start)
        } else if self.passed < self.count {
            (self.append(self.passed, t), CheckpointKind::Split(self.passed))
        } else if self.finish_time.is_none() {
            (self.set_finish_time(t), CheckpointKind::Finish)
        } else {
            return (self, None);
        };
        let checkpoint = Checkpoint { kind, time, fired: false };
        (card.set_last(x).refresh(), Some(checkpoint))
    }

    /// `m_f1c` = `x`.
    fn set_last(mut self, x: f32) -> Self {
        self.last = x;
        self
    }
}

/// The timekeeper (`NRegulation::CTimeKeeper`): the card and the `Update` state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TimeKeeper {
    pub card: TimeCard,
    /// The card's distance at the last `Update` sample (`m_f8`).
    pub sampled: f32,
    /// Consecutive samples in which the car moved less than [`STILL_DISTANCE`] (`m_i0c`).
    pub still: u32,
}

impl TimeKeeper {
    /// `Load` (0x4a01f0) on a new card: the start and finish keys, the splits in order, then `refresh`. A stage
    /// without a start or finish key keeps the constructor's 0.
    pub fn load(markers: &StageMarkers) -> Self {
        let card = markers.splits().fold(
            TimeCard {
                start: markers.start().unwrap_or(0.0),
                finish: markers.finish().unwrap_or(0.0),
                ..TimeCard::default()
            },
            TimeCard::push,
        );
        Self { card: card.refresh(), sampled: 0.0, still: 0 }
    }

    /// `Record` (0x4a0540) on the card; see [`TimeCard::record`].
    pub fn record(self, x: f32, t: f64, dt: f32) -> (Self, Option<Checkpoint>) {
        let (card, checkpoint) = self.card.record(x, t, dt);
        (Self { card, ..self }, checkpoint)
    }

    /// `Update` (0x4a0320): called every step. Every 100th step while timing, compares the card's distance with
    /// the last sample and counts the samples in which the car moved less than 4 m. Returns the count, or 0 on
    /// the other steps and while not timing.
    pub fn update(self, step: u32) -> (Self, u32) {
        if !self.card.started || !step.is_multiple_of(SAMPLE_STEPS) {
            return (self, 0);
        }
        let moved = (self.card.last - self.sampled).abs();
        let sampled = self.card.last;
        if moved < STILL_DISTANCE {
            let still = self.still + 1;
            (Self { sampled, still, ..self }, still)
        } else {
            (Self { sampled, still: 0, ..self }, 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::markers::keys_from_words;

    const DT: f32 = 0.007;
    /// A time at which `Check` is not in its speed branch (the origin is 43200, `cur` below -1.5).
    const EARLY: f64 = 10.0;

    fn key(tag: u32, distance: f32) -> [u32; 3] {
        [tag, 0, distance.to_bits()]
    }

    /// A card with a start at 100 m, splits at 200 and 300 m and a finish at 400 m.
    fn stage() -> TimeCard {
        let card = TimeCard::default().set_start(100.0).set_finish(400.0).push(200.0).push(300.0);
        card.refresh()
    }

    #[test]
    fn constructor_values() {
        let card = TimeCard::default();
        assert_eq!(card.start, 0.0);
        assert_eq!(card.finish, 0.0);
        assert!(card.splits().is_empty());
        assert_eq!(card.passed, 0);
        assert_eq!(card.cur, 0.0);
        assert!(!card.started);
        assert_eq!(card.origin, 43200.0);
        assert_eq!(card.finish_time, None);
        assert_eq!(card.hold, 0);
    }

    #[test]
    fn push_keeps_sixteen_splits() {
        let card = (0..20).fold(TimeCard::default(), |c, i| c.push(i as f32));
        assert_eq!(card.splits().len(), MAX_SPLITS);
        assert_eq!(card.splits()[15].distance, 15.0);
    }

    #[test]
    fn refresh_picks_the_next_key() {
        let card = stage();
        assert_eq!(card.cur, 100.0, "before the start: the start line");
        let card = card.set_started(EARLY).refresh();
        assert_eq!(card.cur, 200.0, "timing: the first split");
        let card = card.append(0, 11.0).refresh();
        assert_eq!(card.cur, 300.0);
        let card = card.append(1, 12.0).refresh();
        assert_eq!(card.cur, 400.0, "all splits passed: the finish");
    }

    #[test]
    fn set_without_refresh_leaves_cur() {
        let card = stage().set_start(50.0);
        assert_eq!(card.cur, 100.0);
        assert_eq!(card.refresh().cur, 50.0);
    }

    #[test]
    fn append_ignores_indices_past_the_splits() {
        let card = stage().set_started(EARLY).append(2, 5.0);
        assert_eq!(card.passed, 0);
        assert!(card.splits().iter().all(|s| s.time.is_none()));
        let card = card.append(1, 5.0);
        assert_eq!(card.passed, 2);
        assert_eq!(card.splits()[1].time, Some(5.0));
    }

    #[test]
    fn check_starts_when_past_the_start_line() {
        let card = stage();
        let (card, cp) = card.check(100.5, EARLY, DT);
        assert!(card.started);
        assert_eq!(card.origin, EARLY);
        let cp = cp.expect("start");
        assert_eq!(cp.kind, CheckpointKind::Start);
        assert!(cp.fired);
    }

    #[test]
    fn check_waits_at_the_line() {
        let card = stage();
        let (card, cp) = card.check(99.0, EARLY, DT);
        assert_eq!(card, stage());
        assert_eq!(cp, None);
    }

    #[test]
    fn check_starts_just_short_when_moving() {
        // cur = EARLY - 43200 = -43190, below the -1.5 floor: no speed start.
        let card = stage().set_last(99.9);
        let (_, cp) = card.check(99.95, EARLY, DT);
        assert_eq!(cp, None);
        // cur is -1.0, inside the window, and 0.05 m in one step is about 7 m/s: the speed start fires.
        let t = 43199.0;
        let card = stage().set_last(99.9);
        let (card, cp) = card.check(99.95, t, DT);
        assert!(card.started);
        let cp = cp.expect("speed start");
        assert!(cp.fired);
        assert_eq!(cp.time, -1.0);
    }

    #[test]
    fn check_starts_at_the_start_time_when_cur_is_not_negative() {
        let t = 43201.0;
        let (card, cp) = stage().check(0.0, t, DT);
        assert_eq!(card.origin, t);
        let cp = cp.expect("start");
        assert!(!cp.fired);
        assert_eq!(cp.time, 1.0);
    }

    #[test]
    fn record_before_the_start_is_checked() {
        let (card, cp) = stage().record(150.0, EARLY, DT);
        assert!(card.started);
        assert_eq!(card.last, 150.0);
        assert!(cp.is_some_and(|c| c.kind == CheckpointKind::Start && c.fired));
    }

    #[test]
    fn record_at_the_start_line_starts_without_the_check() {
        // Check does not fire at the line (x is not past it), so the start is the record's own: fired is false.
        let (card, cp) = stage().record(100.0, EARLY, DT);
        assert!(card.started);
        let cp = cp.expect("start");
        assert!(!cp.fired);
        assert_eq!(cp.kind, CheckpointKind::Start);
    }

    #[test]
    fn hold_skips_check_and_the_record_starts_instead() {
        let card = stage().set_hold(1);
        let (card, cp) = card.record(150.0, EARLY, DT);
        assert!(card.started, "started by the record, not by Check");
        let cp = cp.expect("start");
        assert!(!cp.fired);
    }

    #[test]
    fn short_of_the_next_key_only_moves_last() {
        let card = stage().set_started(EARLY).refresh();
        let (card, cp) = card.record(150.0, 20.0, DT);
        assert_eq!(card.last, 150.0);
        assert_eq!(cp, None);
        assert_eq!(card.passed, 0);
    }

    #[test]
    fn keys_pass_in_order_then_the_finish_once() {
        let mut card = stage().set_started(EARLY).refresh();
        let mut events = Vec::new();
        for (step, x) in [(1.0, 199.0), (2.0, 200.0), (3.0, 350.0), (4.0, 400.0), (5.0, 401.0)] {
            let (next, cp) = card.record(x, step, DT);
            card = next;
            events.push(cp.map(|c| (c.kind, c.time)));
        }
        assert_eq!(
            events,
            [
                None,
                Some((CheckpointKind::Split(0), 2.0 - EARLY as f32)),
                Some((CheckpointKind::Split(1), 3.0 - EARLY as f32)),
                Some((CheckpointKind::Finish, 4.0 - EARLY as f32)),
                None,
            ]
        );
        assert_eq!(card.finish_time, Some(4.0));
        assert_eq!(card.splits()[0].time, Some(2.0));
        assert_eq!(card.splits()[1].time, Some(3.0));
    }

    #[test]
    fn a_big_step_passes_one_key_per_record() {
        let card = stage().set_started(EARLY).refresh();
        let (card, cp) = card.record(450.0, 2.0, DT);
        assert_eq!(cp.map(|c| c.kind), Some(CheckpointKind::Split(0)));
        assert_eq!(card.passed, 1);
        assert_eq!(card.cur, 300.0);
    }

    #[test]
    fn finish_time_is_the_elapsed_time_after_it() {
        let card = stage().set_started(EARLY).set_finish_time(30.0);
        assert_eq!(card.elapsed(40.0), (30.0 - EARLY) as f32);
    }

    #[test]
    fn load_reads_the_markers() {
        let words: Vec<u32> =
            [key(0x17, 2000.0), key(0x15, 90.0), key(0x16, 6000.0), key(0x17, 1000.0), key(0x04, 500.0)].concat();
        let keeper = TimeKeeper::load(&keys_from_words(&words));
        let card = keeper.card;
        assert_eq!(card.start, 90.0);
        assert_eq!(card.finish, 6000.0);
        assert_eq!(card.splits().len(), 2);
        assert_eq!(card.splits()[0].distance, 1000.0);
        assert_eq!(card.splits()[1].distance, 2000.0);
        assert_eq!(card.cur, 90.0, "refreshed after the load");
    }

    #[test]
    fn load_without_a_start_key_keeps_zero() {
        let keeper = TimeKeeper::load(&keys_from_words(&[key(0x16, 60.0)].concat()));
        assert_eq!(keeper.card.start, 0.0);
        assert_eq!(keeper.card.finish, 60.0);
    }

    #[test]
    fn load_keeps_the_last_start_key_and_sixteen_splits() {
        let mut words = [key(0x15, 10.0), key(0x15, 20.0)].concat();
        for i in 0..20 {
            words.extend(key(0x17, 100.0 + i as f32));
        }
        let keeper = TimeKeeper::load(&keys_from_words(&words));
        assert_eq!(keeper.card.start, 20.0);
        assert_eq!(keeper.card.splits().len(), MAX_SPLITS);
    }

    #[test]
    fn update_counts_still_samples_every_hundred_steps() {
        let mut keeper =
            TimeKeeper { card: stage().set_started(EARLY).set_last(100.0), sampled: 100.0, ..TimeKeeper::default() };
        let mut seen = Vec::new();
        for step in [100u32, 150, 200, 300] {
            let (next, still) = keeper.update(step);
            keeper = next;
            seen.push(still);
        }
        assert_eq!(seen, [1, 0, 2, 3]);
    }

    #[test]
    fn update_resets_when_the_car_moves() {
        let mut keeper = TimeKeeper { card: stage().set_started(EARLY), sampled: 0.0, still: 5 };
        keeper.card = keeper.card.set_last(10.0);
        let (keeper, still) = keeper.update(200);
        assert_eq!(still, 0);
        assert_eq!(keeper.still, 0);
        assert_eq!(keeper.sampled, 10.0);
    }

    #[test]
    fn clock_step_rounds_to_two_256ths_at_the_start() {
        // The f32 ulp at 43185 s is 1/256 s, so 0.007 s comes out as 2/256 s on every substep (the peek shows it).
        let t = advance_time(CLOCK_START, DT, false);
        assert_eq!(t, CLOCK_START + 2.0 / 256.0);
        assert_eq!(advance_time(t, DT, false) - t, 2.0 / 256.0);
    }

    #[test]
    fn update_does_nothing_before_the_start() {
        let keeper = TimeKeeper { card: stage().set_last(100.0), ..TimeKeeper::default() };
        let (next, still) = keeper.update(100);
        assert_eq!(still, 0);
        assert_eq!(next, keeper);
    }
}
