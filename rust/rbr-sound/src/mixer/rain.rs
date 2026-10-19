//! The rain stream the mixer synthesises (`E25Rain::Render90` 0x4b7b90).

use super::voices::{Gain, OnePole};
use crate::engine::Rng;

/// The rain stream, `E25Rain::Render90` (0x4b7b90) on `E30Rain`'s three streams. Each drop is `span` samples of
/// a two-pole smoothing (`KernelA0`, `k` 0.25 from `Set50(0.25, 0.8)`) of a step of `amp` (up to 4095, its sign
/// alternating with the sample count's parity) that decays by 0.9 a sample, times `g` (`q / 8`) on the two main
/// streams and `g48` (0-0.99) on the third; `span` is `q % n + n` with `n = trunc((1 - min(amount / 100 + 0.5, 1)) *
/// 50 + 4)`, 4 for light and heavy rain alike. The streams go through 300 Hz `E32Flt` low-passes at the rain's gain
/// (`E30Rain::Vf08` 0x4d17e0: 2 outside, 1 in the car, in [`Command::Rain`](crate::voice::Command::Rain)'s gain); the third's cutoff is 0 outside
/// the car (silent). The original's 24-bit steps (`>> 8` in, `<< 8` out) are taken in floats. Ours: the kernel runs
/// at the output rate, a drop runs across blocks, the two main streams (which carry the same samples) play in both
/// channels, and the third is mixed in with them.
#[derive(Debug, Clone)]
pub(crate) struct Rain {
    pub gain: Gain,
    /// The precipitation amount (50 light, 100 heavy).
    pub amount: f32,
    inside: bool,
    /// The kernel's state (`+0x54..+0x64`).
    smooth: [f32; 2],
    step: f32,
    target: f32,
    fade: f32,
    /// The samples left of the drop, its gains, and the samples so far (the parity of the step's sign).
    left: u32,
    drop_gain: [f32; 2],
    count: u32,
    filters: [OnePole; 2],
    rng: Rng,
}

impl Rain {
    /// `Set50`'s smoothing, the decay a sample, the filters' cutoff.
    const K: f32 = 0.25;
    const DECAY: f32 = 0.9;
    const CUTOFF: f32 = 300.0;

    pub(crate) fn new(rate: u32) -> Rain {
        Rain {
            gain: Gain::default(),
            amount: 0.0,
            inside: false,
            smooth: [0.0; 2],
            step: 0.0,
            target: 0.0,
            fade: 0.0,
            left: 0,
            drop_gain: [0.0; 2],
            count: 0,
            filters: [OnePole::new(rate, Rain::CUTOFF); 2],
            rng: Rng::new(0x7a1e),
        }
    }

    pub(crate) fn set_inside(&mut self, inside: bool) {
        self.inside = inside;
    }

    /// `Render90`'s span base for the precipitation amount.
    fn span(amount: f32) -> u32 {
        (((1.0 - (amount * 0.01 + 0.5).min(1.0)) * 50.0 + 4.0) as i32).clamp(1, 255) as u32
    }

    /// One kernel sample (`KernelA0`), before the drop's gain.
    fn kernel(&mut self) -> f32 {
        let [a, b] = &mut self.smooth;
        *a += ((*a - *b) * Rain::K + (self.step - *a)) * Rain::K;
        *b += (*a - *b) * Rain::K;
        self.step = (1.0 - self.fade) * self.target;
        self.target *= Rain::DECAY;
        self.fade *= Rain::DECAY;
        *b
    }

    pub(crate) fn add(&mut self, out: &mut [[f32; 2]], k: f32) {
        if self.gain.target == 0.0 && self.gain.now.abs() < 1e-5 {
            self.gain.now = 0.0;
            return;
        }
        let n = Rain::span(self.amount);
        for frame in out.iter_mut() {
            if self.left == 0 {
                let q = self.rng.rand() & 0xffff;
                let amp = (q & 0xfff) as f32;
                let amp = if self.count & 1 != 0 { amp } else { -amp };
                self.left = q % n + n;
                let g48 = (self.rng.rand() % 100) as f32 * 0.01;
                self.drop_gain = [q as f32 * 0.125, g48];
                self.target = self.step - amp;
                self.fade = 1.0;
            }
            let f = self.kernel();
            let main = self.filters[0].run(f * self.drop_gain[0] / 256.0);
            let third = if self.inside { self.filters[1].run(f * self.drop_gain[1] / 256.0) } else { 0.0 };
            let s = (main + third) * self.gain.next(k);
            frame[0] += s;
            frame[1] += s;
            self.left -= 1;
            self.count = self.count.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rain_drops_every_few_samples_at_the_traced_level() {
        assert_eq!([0.0, 50.0, 100.0].map(Rain::span), [29, 4, 4]);
        let mut rain = Rain::new(44100);
        rain.gain = Gain { now: 2.0, target: 2.0 };
        rain.amount = 100.0;
        let mut out = vec![[0.0; 2]; 44100];
        rain.add(&mut out, 1.0);
        let rms = (out.iter().map(|f| f[0] * f[0]).sum::<f32>() / 44100.0).sqrt();
        // The Python run of Render90 on CRT-like draws: about 2170 outside.
        assert!((1500.0..3000.0).contains(&rms), "rms {rms}");
    }
}
