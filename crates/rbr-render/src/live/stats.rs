//! The live drive's figures the viewer prints at exit ([`LiveStats`]): the drawn frames and the answers.

use std::time::Instant;

/// A drawn frame longer than this is listed in the summary (ms).
const LONG_GAP_MS: f32 = 50.0;

/// The live drive measured over the pumps: the wall time between pumps (the drawn frames), the answers and their
/// physics steps, and the pauses.
#[derive(Debug, Default, Clone)]
pub struct LiveStats {
    first: Option<Instant>,
    last: Option<Instant>,
    /// Milliseconds between pumps.
    gaps: Vec<f32>,
    /// The gaps over [`LONG_GAP_MS`]: when they ended (s from the first pump) and their length (ms).
    long: Vec<(f32, f32)>,
    /// Answers and physics steps from the first pump on.
    pub answers: u64,
    pub steps: u64,
    /// Wall time paused (the pause menu), s: not in the rates, and the gap across a pause is not a drawn frame's.
    pub paused: f64,
    paused_at: Option<Instant>,
}

impl LiveStats {
    pub(super) fn pump(&mut self, now: Instant) {
        let first = *self.first.get_or_insert(now);
        if let Some(last) = self.last.replace(now) {
            let gap = (now - last).as_secs_f32() * 1000.0;
            self.gaps.push(gap);
            if gap > LONG_GAP_MS {
                self.long.push(((now - first).as_secs_f32(), gap));
            }
        }
    }

    pub(super) fn pause(&mut self, now: Instant) {
        self.paused_at.get_or_insert(now);
    }

    pub(super) fn resume(&mut self, now: Instant) {
        if let Some(at) = self.paused_at.take() {
            self.paused += (now - at).as_secs_f64();
            self.last = None;
        }
    }

    pub(super) fn answer(&mut self, substeps: u32) {
        if self.first.is_some() {
            self.answers += 1;
            self.steps += u64::from(substeps);
        }
    }

    /// One line: the drawn frames' rate and gaps, the answers' rate, and the game time against the wall time (`step`,
    /// s per physics step).
    pub fn summary(&self, step: f32) -> Option<String> {
        let wall = (self.last? - self.first?).as_secs_f64() - self.paused;
        let mut gaps = self.gaps.clone();
        gaps.sort_by(f32::total_cmp);
        let at = |q: f64| gaps.get((gaps.len().saturating_sub(1) as f64 * q) as usize).copied().unwrap_or(0.0);
        Some(
            format!(
            "live: {} drawn frames in {wall:.1} s ({:.1}/s; gap p50 {:.1} ms, p99 {:.1} ms, max {:.1} ms), {} answers \
             ({:.1}/s), {:.1} s paused, {} steps = {:.2} s of game time",
            gaps.len() + 1,
            gaps.len() as f64 / wall.max(1e-9),
            at(0.5),
            at(0.99),
            at(1.0),
            self.answers,
            self.answers as f64 / wall.max(1e-9),
            self.paused,
            self.steps,
            self.steps as f64 * f64::from(step),
        ) + &self.long.iter().map(|(at, ms)| format!(" {ms:.0} ms at {at:.1} s,")).collect::<String>(),
        )
    }
}
