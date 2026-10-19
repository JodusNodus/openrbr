//! The frame clock and the substep schedule of the original's frame. `CPC_Application::Vf0c` (0x44a3d0) splits the
//! wall time since the last frame into whole physics steps; `CApplication::Tic` passes the count on, and
//! `TicPhysicsRecording` (0x47f420) runs each substep as one `StepForward(1)`, through the single-step gate.
//!
//! Pure: the host gives the timer's reading and the physics step interval; the functions return the step count, the
//! time it covers, the clock for the next frame and the substeps the tick runs. The arithmetic is the assembly's; where
//! the C++ reference differs, `README.md` ("Tick") says how.

/// The longest wall time one frame may count, in seconds (0x795e30). The exe holds it as an f64 that is the f32 0.1
/// widened; a longer gap is cut to this and the rest is dropped.
pub const MAX_FRAME_SECONDS: f64 = 0.1f32 as f64;
/// The step interval when no physics definition is loaded (0x7c0bc8, 1/100 s).
pub const FALLBACK_STEP: f32 = 0.01;
/// `StepForward` counts of the four load-time settle calls, before the first frame. The caller is not traced
/// (`notes/gameplay-loop.md`); every recording starts with them.
pub const SETTLE_SUBSTEPS: [u32; 4] = [40; 4];

/// The physics step interval the frame loop uses: `E13Get9B70F` (0x47e7b0) returns the loaded `MaxTimeStep` (the f32
/// at physics definition +0x1608c), or 0 with no physics. A zero reading takes [`FALLBACK_STEP`].
pub fn step_interval(max_time_step: f32) -> f32 {
    if max_time_step == 0.0 {
        FALLBACK_STEP
    } else {
        max_time_step
    }
}

/// The frame clock. `last` is the timer reading the previous frame ended at, less the time it carried over (the f64
/// at `CPC_Application` +0xd30, `m_padCBC` +0x74).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameClock {
    last: f64,
}

/// One frame's split of its wall time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Whole physics steps (`nsteps`, the count `Tic` passes to `TicPhysicsRecording`).
    pub nsteps: u32,
    /// The time those steps cover (`facc`): their sum in f64, rounded to f32 once.
    pub facc: f32,
    /// The clock for the next frame.
    pub clock: FrameClock,
}

impl FrameClock {
    /// The clock at the timer reading `now`, taken before the frame loop starts.
    pub fn start(now: f64) -> Self {
        Self { last: now }
    }

    /// The frame that ends at the timer reading `now`, split into whole steps of `step` (positive; see
    /// [`step_interval`]).
    pub fn frame(self, now: f64, step: f32) -> Frame {
        let dt = now - self.last;
        let dt = if dt > MAX_FRAME_SECONDS { MAX_FRAME_SECONDS } else { dt };
        let (nsteps, sum, rest) = whole_steps(dt, step);
        Frame { nsteps, facc: sum as f32, clock: Self { last: now - rest } }
    }
}

/// The loop of `Vf0c` (0x44a3d0): how many whole `step`s fit in `dt`, their sum, and what is left. The x87 code
/// compares and subtracts in f64 (the step is widened first) and sums in f64.
fn whole_steps(dt: f64, step: f32) -> (u32, f64, f64) {
    let step = f64::from(step);
    let (mut rest, mut nsteps, mut sum) = (dt, 0, 0.0);
    while step <= rest {
        rest -= step;
        nsteps += 1;
        sum += step;
    }
    (nsteps, sum, rest)
}

/// `CApplication`'s single-step gate: `m_unkC38` (active) and `m_unkC3C` (credits). `M0047cd50(n)` turns it on and adds
/// `n` credits; `M0047cd70` clears both. While it is on, a tick runs one substep for each credit and none without one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SingleStep {
    active: bool,
    credits: i32,
}

impl SingleStep {
    /// `M0047cd50`: the gate on, with `credits` more.
    pub fn grant(self, credits: i32) -> Self {
        Self { active: true, credits: self.credits.wrapping_add(credits) }
    }

    /// The substeps a tick runs when the frame asked for `requested`, and the gate after it (the head of
    /// `TicPhysicsRecording`).
    pub fn take(self, requested: u32) -> (u32, Self) {
        if !self.active {
            (requested, self)
        } else if self.credits == 0 {
            (0, self)
        } else {
            let credits = self.credits.wrapping_sub(1);
            (1, Self { credits, ..self })
        }
    }
}

/// What a tick's substeps did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// Substeps run, each one `StepForward(1)`.
    pub substeps: u32,
    /// The replay ended the run on the last substep: the caller sends EndRace.
    pub ended: bool,
}

/// `TicPhysicsRecording`'s substep loop: runs `count` substeps, calling `substep(i)` for each. `substep` returns false
/// when the replay reports the run over; the loop stops after that substep, which still ran.
pub fn run_substeps(count: u32, mut substep: impl FnMut(u32) -> bool) -> Run {
    for i in 0..count {
        if !substep(i) {
            return Run { substeps: i + 1, ended: true };
        }
    }
    Run { substeps: count, ended: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f32 = 0.007;

    #[test]
    fn zero_interval_takes_the_fallback() {
        assert_eq!(step_interval(0.0), FALLBACK_STEP);
        assert_eq!(step_interval(STEP), STEP);
    }

    #[test]
    fn whole_steps_and_remainder() {
        let frame = FrameClock::start(0.0).frame(0.0255, STEP);
        assert_eq!(frame.nsteps, 3);
        assert_eq!(frame.facc, (3.0 * f64::from(STEP)) as f32);
        // The clock keeps the steps' time, so the rest (0.0255 - 3 steps) is carried, not lost.
        assert!((frame.clock.last - 3.0 * f64::from(STEP)).abs() < 1e-12);
    }

    #[test]
    fn long_gaps_are_cut_to_the_clamp() {
        let frame = FrameClock::start(0.0).frame(5.0, STEP);
        assert_eq!(frame.nsteps, 14);
        let rest = MAX_FRAME_SECONDS - 14.0 * f64::from(STEP);
        assert!((frame.clock.last - (5.0 - rest)).abs() < 1e-12);
        assert!(frame.clock.last > 4.99);
    }

    #[test]
    fn sum_is_rounded_once_not_per_step() {
        // Six steps: the f64 sum rounded to f32 is what the asm keeps; adding the f32 step six times is not.
        let frame = FrameClock::start(0.0).frame(0.0420001, STEP);
        let per_step = (0..6).fold(0.0f32, |sum, _| sum + STEP);
        assert_eq!(frame.nsteps, 6);
        assert_eq!(frame.facc, (6.0 * f64::from(STEP)) as f32);
        assert_ne!(frame.facc, per_step);
    }

    #[test]
    fn carry_keeps_the_count_over_a_regular_cadence() {
        let mut clock = FrameClock::start(0.0);
        let mut total = 0;
        for k in 1..=100 {
            let frame = clock.frame(f64::from(k) * 0.01, STEP);
            total += frame.nsteps;
            clock = frame.clock;
        }
        // 1.0 s of wall time at 0.007 s per step: 142 whole steps, the carry never lost.
        assert_eq!(total, 142);
    }

    #[test]
    fn exact_multiples_do_not_lose_a_step() {
        // The step test is `step <= dt` in f64 on the widened f32, so an exact fit counts.
        let dt = f64::from(STEP) * 2.0;
        let frame = FrameClock::start(0.0).frame(dt, STEP);
        assert_eq!(frame.nsteps, 2);
    }

    #[test]
    fn settle_is_four_calls_of_forty() {
        assert_eq!(SETTLE_SUBSTEPS.iter().sum::<u32>(), 160);
    }

    #[test]
    fn gate_off_runs_what_was_asked() {
        assert_eq!(SingleStep::default().take(7), (7, SingleStep::default()));
    }

    #[test]
    fn gate_on_runs_one_substep_per_credit() {
        let gate = SingleStep::default().grant(2);
        let (first, gate) = gate.take(40);
        let (second, gate) = gate.take(40);
        let (third, gate) = gate.take(40);
        assert_eq!((first, second, third), (1, 1, 0));
        assert_eq!(gate, SingleStep { active: true, credits: 0 });
    }

    #[test]
    fn clear_is_the_default_gate() {
        let gate = SingleStep::default().grant(5);
        assert_ne!(gate, SingleStep::default());
        assert_eq!(SingleStep::default(), SingleStep { active: false, credits: 0 });
    }

    #[test]
    fn run_stops_after_the_substep_that_ends_it() {
        let run = run_substeps(5, |i| i != 1);
        assert_eq!(run, Run { substeps: 2, ended: true });
        assert_eq!(run_substeps(3, |_| true), Run { substeps: 3, ended: false });
        assert_eq!(run_substeps(0, |_| false), Run { substeps: 0, ended: false });
    }
}
