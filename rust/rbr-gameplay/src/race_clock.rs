//! The race clock the player sees and the outside-assistance penalty: the HUD's start countdown, the stage time it runs
//! from the go, and the timer the car's outside-assistance signal restarts, which adds the penalty to that time.
//!
//! `E62GameMenu` (the HUD at `*0x165fc68`) keeps them: the countdown +0x244, the clock +0x140 and its stop word +0x144,
//! the assistance timer +0x270, its fade +0x274 and the signal's code +0x278. `0x5772e0` resets them; `M573900`
//! (0x573900) is the `CSigOutsideAssistance` receiver; `CApplication::M004825b0` (0x4825b0) in state 1 ticks them each
//! frame after the physics substeps, with the frame's `facc`: the HUD tick `0x579df0`, whose first call is `M576710`
//! (0x576710, the assistance timer and the penalty), then `M573d90` (0x573d90, the countdown and the clock).
//!
//! A start before the go is a jump start (`E62GameMenu::M579430`, 0x579430): the clock takes the countdown's rest and
//! the jump-start penalty, and the countdown ends. Two callers: the start line's marker (key 0x15, `E12_AnimSet::Load3b0`
//! 0x4783b0) while the countdown runs, and the HUD's `CSigCamera` receiver (`M579590`, 0x579590) when the car moves
//! faster than 1 m/s in the countdown's last 4 s. The finish's marker stops the clock (+0x144).
//!
//! This is the time the race result is taken from (`E60Flow` reads +0x140 at the end of the race). The stage time card
//! (`timing::TimeCard`) runs on the physics clock and never sees the penalty.
//!
//! Pure: the host gives the frame's `dt`, the code of an outside-assistance emit and the penalty settings; the functions
//! return the next value and what the original emits. The arithmetic is the assembly's (SSE, f32).

// The comparisons follow the asm's, so a NaN takes the original's branch: `!(a > b)` is not `a <= b`.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

/// The countdown at the reset (`DAT_00738398`).
pub const COUNTDOWN: f32 = 7.0;
/// Below this the countdown fires the stage trigger `start_animation2` once (`DAT_00731b0c`).
pub const START_ANIMATION: f32 = 6.0;
/// The assistance timer at the reset (`DAT_00746a7c`): past the end, so nothing fires.
const TIMER_IDLE: f32 = f32::from_bits(0x461C_3F9A);
/// The timer at which the spectators start and the penalty is added (`DAT_00795e14`).
const PENALTY_AT: f32 = 1.0;
/// The fade starts at this time (`DAT_0073b8d8`) and rises at [`FADE_RATE`] per second to 1.
const FADE_FROM: f32 = 3.5;
const FADE_RATE: f32 = 2.0;
/// The spectators end at this time (`DAT_0073b9b0`).
const SPECTATORS_END: f32 = 4.5;
/// The menu's assistance window ends here (`E62_M573920`, 0x573920; the f32 at 0x746890).
const WINDOW_END: f32 = 8.5;
/// The menu offers "call for help" only from this race time on (`0x56a620`; the f32 at 0x72ad94).
const HELP_FROM: f32 = 2.0;
/// The penalties (`DAT_0072b154`, `DAT_0072f614`, `DAT_00746a78`).
const PENALTIES: [f32; 3] = [10.0, 20.0, 35.0];
/// The game-flow states whose penalty comes from the rally's setting (`E59Host90::M00565470` 3 and 5), and the rally
/// school's, which has no countdown (`0x566240`).
const RALLY_MODES: [u32; 2] = [3, 5];
const SCHOOL_MODE: u32 = 7;
/// The jump-start penalty outside a rally (`E60Flow::M565ea0`, 0x565ea0: `DAT_0072b154`).
const JUMP_START_PENALTY: f32 = 10.0;
/// A rally's jump-start penalties by the player's false starts so far in the rally (`E80Daemon::IdxFloat` 0x5d0810:
/// `DAT_0072b154`, `DAT_00730894`, `DAT_0072f42c`, `DAT_0072f00c`).
pub const RALLY_JUMP_START_PENALTIES: [f32; 4] = [10.0, 15.0, 30.0, 600.0];
/// `M579590`'s window: the countdown in `(0, 4)` (`DAT_0072d150`) and a speed above 1 m/s (`DAT_00795e14`).
const MOVING_START_FROM: f32 = 4.0;
const MOVING_START_SPEED: f32 = 1.0;

/// What decides the penalty: the game flow's state (the first word of `*0x165fa90`), the rally's assistance level
/// (`*0x7d6128` +0x8e84) and the other modes' (the global 0x1660850).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssistSettings {
    pub mode: u32,
    pub rally_level: u32,
    pub level: u32,
    /// In a rally, the player's false starts so far in it (the season's +0x174, `GetPlayerFalseStarts` 0x5d07a0).
    pub false_starts: u8,
}

impl AssistSettings {
    /// A quick race on the 1.02 install, as the peeks read it (`tests/race_clock_peek.rs`): flow state 1 and 1 in the
    /// global, so 10 s.
    pub const QUICK_RACE: AssistSettings = AssistSettings { mode: 1, rally_level: 0, level: 1, false_starts: 0 };

    /// The penalty in seconds: in a rally (modes 3 and 5) level 0, 1 or 2 of the rally's setting gives 10, 20 or 35;
    /// otherwise the global's 0 and 1 give 10, 2 gives 20 and 3 gives 35. Any other value adds 0.
    pub fn penalty(self) -> f32 {
        let index = if RALLY_MODES.contains(&self.mode) { self.rally_level } else { self.level.saturating_sub(1) };
        PENALTIES.get(index as usize).copied().unwrap_or(0.0)
    }

    /// The jump-start penalty (`M565ea0`): 10 s; in a rally (modes 3 and 5) the season's by the player's false starts
    /// so far (`E80Daemon::IdxFloat` 0x5d0810, [`RALLY_JUMP_START_PENALTIES`]; then `IncT174` counts this one). Ours:
    /// a fifth and later take 600 s (the original reads past its four-float table).
    pub fn jump_start_penalty(self) -> f32 {
        if RALLY_MODES.contains(&self.mode) {
            RALLY_JUMP_START_PENALTIES[usize::from(self.false_starts).min(3)]
        } else {
            JUMP_START_PENALTY
        }
    }

    /// The rally school (`0x566240`): no countdown, and the start line starts the clock.
    pub fn school(self) -> bool {
        self.mode == SCHOOL_MODE
    }

    /// The false starts `M579430` (0x579430) puts in HUD +0x148 for the warning banner (`FSWAR_1` to `FSWAR_3`, drawn
    /// by 0x573100 for 10 s): after a jump start in a rally (modes 3 and 5, `FUN_00565f20`), the season's count with
    /// this one (`M565ea0` counts it first, `IncT174`); 0 outside a rally or before a jump start. Above 3 the game ends
    /// the race ([`Self::excluded`]).
    pub fn false_start_warning(self, jumped: bool) -> u8 {
        if jumped && RALLY_MODES.contains(&self.mode) {
            self.false_starts.saturating_add(1)
        } else {
            0
        }
    }

    /// The exclusion from the rally: a jump start that brings the season's false starts above 3 makes `M579430`
    /// clear the jump and the banner (+0x248, +0x14c) and emit `CSigEndRace` with the state word 1 (0x5794f5).
    pub fn excluded(self, jumped: bool) -> bool {
        self.false_start_warning(jumped) > 3
    }
}

/// What a frame of the clock emitted.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClockEvents {
    /// `CSigInitSpectatorFlipAnimation {code, 0}` when the timer passed [`PENALTY_AT`], `{code, 1}` when it passed
    /// [`SPECTATORS_END`]: the car-flip spectators (presentation, rbr-recomp `notes/scripted-events.md` section 2).
    pub spectators: Option<(u32, bool)>,
    /// The penalty added to the clock this frame (also `CGhostCarManager::Accumulate`'s, the ghost, out of scope).
    pub penalty: Option<f32>,
    /// The countdown fell below [`START_ANIMATION`]: `E12_AnimSet::FireByIdx("start_animation2")`.
    pub start_animation: bool,
    /// The countdown passed 0: the go, from which the clock runs.
    pub go: bool,
    /// The jump-start penalty, when the frame took it ([`RaceClock::jump_start`]; the host's event, not the tick's).
    pub jump_start: Option<f32>,
}

/// The HUD's race clock and assistance timer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaceClock {
    /// The start countdown (+0x244).
    pub countdown: f32,
    /// `start_animation2` was fired (+0x8).
    pub start_fired: bool,
    /// The clock runs (+0x144 is 0).
    pub running: bool,
    /// The race time (+0x140), with the penalties.
    pub time: f32,
    /// The time since the last outside-assistance signal (+0x270).
    pub assist_timer: f32,
    /// The fade over the assistance (+0x274, presentation).
    pub fade: f32,
    /// The last outside-assistance code (+0x278): 1 with the car not on its wheels, else 2 or 3.
    pub code: u32,
    /// The jump start was taken (+0x248): it is taken once.
    pub jumped: bool,
}

impl Default for RaceClock {
    /// The HUD reset (0x5772e0).
    fn default() -> Self {
        Self {
            countdown: COUNTDOWN,
            start_fired: false,
            running: false,
            time: 0.0,
            assist_timer: TIMER_IDLE,
            fade: 0.0,
            code: 0,
            jumped: false,
        }
    }
}

impl RaceClock {
    /// `M573900` (0x573900), the `CSigOutsideAssistance` receiver: keeps the code and restarts the timer.
    pub fn outside_assistance(self, code: u32) -> Self {
        Self { code, assist_timer: 0.0, ..self }
    }

    /// `E62_M572fc0` (0x572fc0): the countdown runs, which the start line's jump start needs.
    pub fn counting_down(self) -> bool {
        self.countdown > 0.0
    }

    /// `M579430` (0x579430), the jump start: the clock takes the countdown's rest and the penalty (two f32 adds, the
    /// second on the x87 and rounded once), the countdown is 0 and `start_animation2` counts as fired. Once per race;
    /// returns the penalty when it is taken. In a rally the penalty grows with the player's false starts
    /// ([`AssistSettings::jump_start_penalty`]); the season counts the jump from the stage's result.
    pub fn jump_start(self, settings: AssistSettings) -> (Self, Option<f32>) {
        if self.jumped {
            return (self, None);
        }
        let penalty = settings.jump_start_penalty();
        let clock = Self {
            time: (self.time + self.countdown) + penalty,
            countdown: 0.0,
            start_fired: true,
            jumped: true,
            ..self
        };
        (clock, Some(penalty))
    }

    /// `M579590`'s test (0x579590): outside the school, the countdown in its last 4 s and the car faster than 1 m/s.
    pub fn moving_start(self, settings: AssistSettings, speed: f32) -> bool {
        !settings.school() && 0.0 < self.countdown && self.countdown < MOVING_START_FROM && MOVING_START_SPEED < speed
    }

    /// `E62_M573920` (0x573920): the assistance timer is in its window, `[0, 8.5)`.
    pub fn assistance_window(self) -> bool {
        0.0 <= self.assist_timer && self.assist_timer < WINDOW_END
    }

    /// The menu's "call for help" predicate (`0x56a620`): outside the assistance window and from 2 s of race time. The
    /// original also needs the rescue helicopter idle and a timer at `*0x166e378` +0x844 at or below 0; neither is
    /// modelled (neither runs before the menu's rescue).
    pub fn help_offered(self) -> bool {
        !self.assistance_window() && self.time >= HELP_FROM
    }

    /// One frame of the HUD in the driving state: `M576710` (0x576710) then `M573d90` (0x573d90), both with the frame's
    /// `dt` (`facc`).
    pub fn tick(self, dt: f32, settings: AssistSettings) -> (Self, ClockEvents) {
        let mut events = ClockEvents::default();
        let clock = self.assistance(dt, settings, &mut events);
        (clock.countdown_tick(dt, settings, &mut events), events)
    }

    /// `M576710`: the timer, the spectators' start and end, the penalty at 1 s while the clock runs, and the fade.
    fn assistance(self, dt: f32, settings: AssistSettings, events: &mut ClockEvents) -> Self {
        let timer = dt + self.assist_timer;
        let mut next = Self { assist_timer: timer, fade: 0.0, ..self };
        // `comiss 4.5, timer; jbe`: at or past the end (or NaN) only the end's frame emits.
        if !(SPECTATORS_END > timer) {
            if !(SPECTATORS_END < timer - dt) {
                events.spectators = Some((self.code, true));
            }
            return next;
        }
        if PENALTY_AT > timer {
            return next.with_fade(timer);
        }
        if PENALTY_AT > timer - dt {
            events.spectators = Some((self.code, false));
            if self.running {
                let penalty = settings.penalty();
                next.time = self.time + penalty;
                events.penalty = Some(penalty);
            }
        }
        if !(timer > FADE_FROM) {
            return next;
        }
        next.with_fade(timer - FADE_FROM)
    }

    /// The fade `2 x` capped at 1 (`mulss`, then `comiss` against 1.0).
    fn with_fade(self, x: f32) -> Self {
        let fade = x * FADE_RATE;
        Self { fade: if fade > 1.0 { 1.0 } else { fade }, ..self }
    }

    /// `M573d90`: the countdown (not in the rally school), the go when it passes 0, and the clock while it runs.
    fn countdown_tick(self, dt: f32, settings: AssistSettings, events: &mut ClockEvents) -> Self {
        let mut next = self;
        if settings.mode != SCHOOL_MODE {
            let old = self.countdown;
            next.countdown = old - dt;
            if next.countdown < START_ANIMATION && !self.start_fired {
                next.start_fired = true;
                events.start_animation = true;
            }
            if 0.0 < next.countdown || next.countdown == 0.0 {
                return next;
            }
            if 0.0 <= old {
                next.running = true;
                events.go = true;
            }
        }
        if next.running {
            next.time += dt;
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUICK: AssistSettings = AssistSettings { mode: 1, rally_level: 0, level: 0, false_starts: 0 };
    const DT: f32 = 0.007;

    #[test]
    fn penalties_follow_the_mode_and_its_setting() {
        let quick = |level| AssistSettings { level, ..QUICK };
        let rally = |rally_level| AssistSettings { mode: 3, rally_level, level: 3, false_starts: 0 };
        assert_eq!([0, 1, 2, 3, 4].map(|l| quick(l).penalty()), [10.0, 10.0, 20.0, 35.0, 0.0]);
        assert_eq!([0, 1, 2, 3].map(|l| rally(l).penalty()), [10.0, 20.0, 35.0, 0.0]);
        assert_eq!(AssistSettings { mode: 5, ..rally(1) }.penalty(), 20.0);
    }

    #[test]
    fn the_clock_runs_from_the_go_with_the_whole_frame() {
        let mut clock = RaceClock::default();
        let mut go_at = None;
        for frame in 0..1100 {
            let (next, events) = clock.tick(DT, QUICK);
            if events.go {
                go_at = Some(frame);
                assert_eq!(next.time, DT, "the go's frame adds its whole dt");
            }
            clock = next;
        }
        let go_at = go_at.expect("the go");
        assert!(clock.countdown < 0.0 && clock.running && clock.start_fired);
        assert!((999..=1001).contains(&go_at), "7 s of 7 ms frames: {go_at}");
    }

    #[test]
    fn assistance_adds_the_penalty_once_at_one_second() {
        let running =
            RaceClock { running: true, countdown: -1.0, start_fired: true, time: 30.0, ..RaceClock::default() };
        let mut clock = running.outside_assistance(1);
        assert!(clock.assistance_window());
        let mut seen = Vec::new();
        for frame in 0..1300 {
            let (next, events) = clock.tick(DT, QUICK);
            if events.spectators.is_some() || events.penalty.is_some() {
                seen.push((frame, events.spectators, events.penalty));
            }
            clock = next;
        }
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].1, Some((1, false)));
        assert_eq!(seen[0].2, Some(10.0));
        assert_eq!(seen[1].1, Some((1, true)));
        assert_eq!(seen[1].2, None);
        assert!(!clock.assistance_window());
        assert_eq!(clock.fade, 0.0);
    }

    #[test]
    fn no_penalty_before_the_go() {
        let clock = RaceClock::default().outside_assistance(2);
        let (clock, events) = clock.tick(0.5, QUICK);
        assert_eq!(events.penalty, None);
        let (clock, events) = clock.tick(0.6, QUICK);
        assert_eq!(events.spectators, Some((2, false)));
        assert_eq!(events.penalty, None);
        assert_eq!(clock.time, 0.0);
    }

    #[test]
    fn the_fade_rises_in_the_first_half_second_and_after_three_and_a_half() {
        let clock = RaceClock::default().outside_assistance(3);
        let (c, _) = clock.tick(0.25, QUICK);
        assert_eq!(c.fade, 0.5);
        let (c, _) = c.tick(0.5, QUICK);
        assert_eq!(c.fade, 1.0);
        let (c, _) = c.tick(0.25, QUICK);
        assert_eq!(c.fade, 0.0);
        let (c, _) = c.tick(2.75, QUICK);
        assert_eq!(c.assist_timer, 3.75);
        assert_eq!(c.fade, 0.5);
        let (c, _) = c.tick(0.5, QUICK);
        assert_eq!(c.fade, 1.0);
    }

    #[test]
    fn help_is_offered_outside_the_window_from_two_seconds() {
        let clock = RaceClock { running: true, time: 2.0, ..RaceClock::default() };
        assert!(clock.help_offered());
        assert!(!RaceClock { time: 1.9, ..clock }.help_offered());
        assert!(!clock.outside_assistance(1).help_offered());
        let late = RaceClock { assist_timer: 8.5, ..clock };
        assert!(late.help_offered());
    }

    /// In a rally the jump start costs 10, 15, 30, then 600 s by the false starts before it (`IdxFloat`); outside, 10.
    #[test]
    fn the_false_start_warning_counts_this_one_in_a_rally_only() {
        let rally = AssistSettings { mode: 3, rally_level: 0, level: 1, false_starts: 1 };
        assert_eq!(rally.false_start_warning(true), 2);
        assert_eq!(rally.false_start_warning(false), 0);
        assert_eq!(AssistSettings { false_starts: 1, ..QUICK }.false_start_warning(true), 0);
    }

    /// The fourth false start of a rally excludes the player; outside a rally none does.
    #[test]
    fn the_fourth_false_start_of_a_rally_excludes() {
        let rally = |false_starts| AssistSettings { mode: 5, rally_level: 0, level: 1, false_starts };
        assert_eq!([0, 1, 2, 3].map(|n| rally(n).excluded(true)), [false, false, false, true]);
        assert!(!rally(3).excluded(false));
        assert!(!AssistSettings { false_starts: 9, ..QUICK }.excluded(true));
    }

    #[test]
    fn a_rallys_jump_start_grows_with_the_false_starts() {
        let rally = |false_starts| AssistSettings { mode: 3, rally_level: 0, level: 1, false_starts };
        assert_eq!([0, 1, 2, 3, 7].map(|n| rally(n).jump_start_penalty()), [10.0, 15.0, 30.0, 600.0, 600.0]);
        assert_eq!(AssistSettings { false_starts: 2, ..QUICK }.jump_start_penalty(), 10.0);
    }

    #[test]
    fn a_jump_start_takes_the_countdowns_rest_and_ten_seconds_once() {
        let clock = RaceClock { countdown: 5.943, start_fired: true, ..RaceClock::default() };
        assert!(clock.counting_down());
        let (clock, penalty) = clock.jump_start(QUICK);
        assert_eq!(penalty, Some(10.0));
        assert_eq!((clock.time, clock.countdown), (5.943 + 10.0, 0.0));
        assert_eq!(clock.jump_start(QUICK), (clock, None));
        let (clock, events) = clock.tick(DT, QUICK);
        assert!(events.go && clock.running);
        assert_eq!(clock.countdown, -DT);
        assert_eq!(clock.time, 5.943 + 10.0 + DT);
    }

    #[test]
    fn moving_in_the_last_four_seconds_is_a_jump_start() {
        let at = |countdown| RaceClock { countdown, ..RaceClock::default() };
        assert!(at(3.9).moving_start(QUICK, 1.1));
        assert!(!at(3.9).moving_start(QUICK, 1.0));
        assert!(!at(4.0).moving_start(QUICK, 5.0));
        assert!(!at(0.0).moving_start(QUICK, 5.0));
        assert!(!at(3.9).moving_start(AssistSettings { mode: 7, ..QUICK }, 5.0));
    }

    #[test]
    fn the_school_has_no_countdown() {
        let school = AssistSettings { mode: 7, ..QUICK };
        let (clock, events) = RaceClock::default().tick(DT, school);
        assert_eq!(clock.countdown, COUNTDOWN);
        assert!(!events.go);
        assert_eq!(clock.time, 0.0);
    }
}
