//! Pacing the payloads to real time ([`Pace`]) and the pacing statistics the viewer prints at exit ([`PaceStats`]).

use super::{MAX_BEHIND, STEPS_PER_SECOND};
use std::time::Instant;

/// The wall time the payloads are paced to: real time since the second pump, less the time not played
/// ([`MAX_BEHIND`]) and the pauses.
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    start: Instant,
    /// Wall time not played since `start` ([`MAX_BEHIND`]), s.
    pub skipped: f64,
    /// Wall time paused before the current pause, s.
    pub(super) paused: f64,
    /// When the current pause began.
    pub(super) paused_at: Option<Instant>,
}

impl Pace {
    pub fn new(start: Instant) -> Pace {
        Pace { start, skipped: 0.0, paused: 0.0, paused_at: None }
    }

    /// The payload periods due at `now` with `sent` sent, and how many were due and not sent before [`MAX_BEHIND`]
    /// cut them (the time past it is not played). While paused, the periods due when the pause began.
    pub fn due(&mut self, now: Instant, sent: u64) -> (u64, u64) {
        let now = self.paused_at.unwrap_or(now);
        let played = (now - self.start).as_secs_f64() - self.skipped - self.paused;
        let due = played.mul_add(STEPS_PER_SECOND, 1.0) as u64;
        let most = sent + (MAX_BEHIND * STEPS_PER_SECOND) as u64;
        if due > most {
            self.skipped += (due - most) as f64 / STEPS_PER_SECOND;
        }
        (due.min(most), due.saturating_sub(sent))
    }

    pub fn pause(&mut self, now: Instant) {
        self.paused_at.get_or_insert(now);
    }

    /// Ends the pause; returns its length, s.
    pub fn resume(&mut self, now: Instant) -> f64 {
        let paused = self.paused_at.take().map_or(0.0, |at| (now - at).as_secs_f64());
        self.paused += paused;
        paused
    }
}

/// A drawn frame longer than this is listed in the pacing's summary (ms).
const LONG_GAP_MS: f32 = 50.0;

/// The live pacing measured over the pumps: how far behind real time each pump found the payloads, and the wall time
/// between pumps (the drawn frames).
#[derive(Debug, Default, Clone)]
pub struct PaceStats {
    first: Option<Instant>,
    last: Option<Instant>,
    /// Milliseconds between pumps.
    gaps: Vec<f32>,
    /// The gaps over [`LONG_GAP_MS`]: when they ended (s from the first pump) and their length (ms).
    long: Vec<(f32, f32)>,
    /// Payloads due and not sent at each pump, at most (before [`MAX_BEHIND`] drops the rest).
    pub most_behind: u64,
    /// Answers, the payload periods sent (more than the payloads once a late viewer sends frames of several) and
    /// physics steps from the first pump on.
    pub answers: u64,
    pub periods: u64,
    pub steps: u64,
    /// Wall time paused (the pause menu), s: not in the rates, and the gap across a pause is not a drawn frame's.
    pub paused: f64,
}

impl PaceStats {
    pub(super) fn pump(&mut self, now: Instant, behind: u64) {
        let first = *self.first.get_or_insert(now);
        if let Some(last) = self.last.replace(now) {
            let gap = (now - last).as_secs_f32() * 1000.0;
            self.gaps.push(gap);
            if gap > LONG_GAP_MS {
                self.long.push(((now - first).as_secs_f32(), gap));
            }
        }
        self.most_behind = self.most_behind.max(behind);
    }

    pub(super) fn resume(&mut self, paused: f64) {
        self.paused += paused;
        self.last = None;
    }

    pub(super) fn sent(&mut self, periods: u64) {
        if self.first.is_some() {
            self.periods += periods;
        }
    }

    pub(super) fn answer(&mut self, substeps: u32) {
        if self.first.is_some() {
            self.answers += 1;
            self.steps += u64::from(substeps);
        }
    }

    /// One line: the drawn frames' rate and gaps, the payload periods' rate against real time, the steps per payload and
    /// the game time against the wall time (`step`, s per physics step), and the wall time not played (`skipped`, s).
    pub fn summary(&self, step: f32, skipped: f64) -> Option<String> {
        let wall = (self.last? - self.first?).as_secs_f64() - self.paused;
        let mut gaps = self.gaps.clone();
        gaps.sort_by(f32::total_cmp);
        let at = |q: f64| gaps.get((gaps.len().saturating_sub(1) as f64 * q) as usize).copied().unwrap_or(0.0);
        Some(format!(
            "live pacing: {} drawn frames in {wall:.1} s ({:.1}/s; gap p50 {:.1} ms, p99 {:.1} ms, max {:.1} ms), \
             {} answers, {} periods sent ({:.1}/s, real time {:.1}/s), most behind {} payloads, {skipped:.2} s not played, {:.1} s paused, {} steps = {:.2} s of game time",
            gaps.len() + 1,
            gaps.len() as f64 / wall.max(1e-9),
            at(0.5),
            at(0.99),
            at(1.0),
            self.answers,
            self.periods,
            self.periods as f64 / wall.max(1e-9),
            STEPS_PER_SECOND,
            self.most_behind,
            self.paused,
            self.steps,
            self.steps as f64 * f64::from(step),
        ) + &self.long.iter().map(|(at, ms)| format!(" {ms:.0} ms at {at:.1} s,")).collect::<String>())
    }
}
