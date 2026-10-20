//! The mix's output limiter (ours): a look-ahead peak limiter that turns the whole mix down smoothly before a peak
//! instead of bending each sample past a knee, so a loud moment gets quieter without the distortion of a clipper.

use std::collections::VecDeque;

/// The most the output reaches, the look-ahead and the release, seconds.
const CEILING: f32 = 0.8;
const LOOK_AHEAD: f32 = 0.002;
const RELEASE: f32 = 0.12;

#[derive(Debug, Clone)]
pub(crate) struct Limiter {
    /// The frames not yet out, and the gain each one needs, as a rising run of (frame index, gain) whose front is
    /// the least within the look-ahead.
    delay: VecDeque<[f32; 2]>,
    least: VecDeque<(u64, f32)>,
    at: u64,
    gain: f32,
    attack: f32,
    release: f32,
}

impl Limiter {
    pub(crate) fn new(rate: u32) -> Limiter {
        let ahead = ((LOOK_AHEAD * rate as f32) as usize).max(1);
        Limiter {
            delay: VecDeque::from(vec![[0.0; 2]; ahead]),
            least: VecDeque::new(),
            at: 0,
            gain: 1.0,
            // Most of the way down within the look-ahead.
            attack: 1.0 - (-4.0 / ahead as f32).exp(),
            release: 1.0 - (-1.0 / (RELEASE * rate as f32)).exp(),
        }
    }

    /// The gain it is at (1 untouched).
    #[cfg(test)]
    pub(crate) fn gain(&self) -> f32 {
        self.gain
    }

    /// Limits `out` in place, the look-ahead behind.
    pub(crate) fn run(&mut self, out: &mut [[f32; 2]]) {
        let ahead = self.delay.len() as u64;
        for frame in out {
            let peak = frame[0].abs().max(frame[1].abs());
            let need = if peak > CEILING { CEILING / peak } else { 1.0 };
            while self.least.back().is_some_and(|&(_, g)| g >= need) {
                self.least.pop_back();
            }
            self.least.push_back((self.at, need));
            while self.least.front().is_some_and(|&(i, _)| i + ahead < self.at) {
                self.least.pop_front();
            }
            self.at += 1;
            let target = self.least.front().map_or(1.0, |&(_, g)| g);
            let k = if target < self.gain { self.attack } else { self.release };
            self.gain += (target - self.gain) * k;
            self.delay.push_back(*frame);
            let late = self.delay.pop_front().unwrap_or_default();
            *frame = late.map(|s| s * self.gain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_input_passes_late_and_untouched() {
        let mut l = Limiter::new(1000);
        let mut out = [[0.5, -0.25], [0.0; 2], [0.0; 2], [0.0; 2]];
        l.run(&mut out);
        assert_eq!(out[2], [0.5, -0.25]);
        assert_eq!(l.gain(), 1.0);
    }

    #[test]
    fn a_loud_burst_is_turned_down_smoothly_and_recovers() {
        let rate = 44100;
        let mut l = Limiter::new(rate);
        let mut out: Vec<[f32; 2]> =
            (0..rate as usize).map(|i| if (4410..8820).contains(&i) { [2.0, -2.0] } else { [0.5; 2] }).collect();
        l.run(&mut out);
        let peak = out.iter().map(|f| f[0].abs()).fold(0.0, f32::max);
        assert!(peak < CEILING * 1.05, "{peak}");
        // No step bigger than the burst's own edge would give at the ceiling.
        let jump = out.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).fold(0.0, f32::max);
        assert!(jump < 1.3, "{jump}");
        assert!((out[rate as usize - 1][0] - 0.5).abs() < 0.01);
    }
}
