//! The engine's reflections (`notes/sound.md` 5.4): the `Reflection L` and `R` streams (`CCarSound +0x1090`,
//! `+0x1094`), fed in `CCarSound::Vf08` (0x4b0b50, 0x4b11cd-0x4b14fe) from the engine's block after `MixScale`
//! (`CDXEngine +0x20df8`, the same block the engine stream plays):
//!
//! - `E25Engine` (`CCarSound +0x111c`) low-passes it (`Filter80` 0x4b8780, a one-pole bilinear filter; `Mix60`
//!   0x4b8860, `RunD0` 0x4b87d0) into a 32768-sample ring, and `Blend90` (0x4b8e90) reads the ring back a delay behind
//!   for each side, times the side's gain and a volume, as two 16-bit blocks (`+0x1138`, `+0x11138`);
//! - in the car with the side's door off the car (`+0x41a9c` left, `+0x41aa0` right) a side's stream gets the delayed
//!   block plus the engine's block times `trunc(engine volume * 512) / 256`, clamped (0x4b13dd); otherwise the
//!   delayed block alone (0x4b144e).
//!
//! The delays and gains are the master's `+0x106cc..+0x106d8`. In the car `Vf08` sets them to 0.4 m and 1.4 m and gains
//! 1, and `Mcb80` (0x4acb80) then sets the left one to 1.6 m: delays of `trunc(m / 340 * 44100)` samples, 207 and 181.
//! Outside nothing sets them, so they keep the car's values once the camera has been in the car (before that 0: the
//! master's heap block, read as zero, **inferred**). The stage's `Real` animation channels of tags 0x15-0x18 could
//! set them (`E12_AnimSet::CbD80` 0x477d80 on `CSigAnimReal`, emitted by `Tic140` 0x478140), but no retail stage has
//! one: five `.dls` files have a single `Real` channel, of tag 11. The low-pass cutoff is 750 Hz in the car, else
//! `min(max((left + right) * 31.25, 100), min(16000 / distance, 2000))` Hz (0x4b1232-0x4b12e6). The volume is 256
//! from 4500 rpm (0x1194), below it `128 + trunc(rpm * 0.028444)` (0x4b1344). The read cursor starts 128 samples
//! behind the write cursor (`InitD0` 0x4b86d0: 0x7f80), which every delay adds.
//!
//! The voices are 3D, head-relative, at 1 m left and right (`CCarSound::Vf04`, the DX mixer's `+0x70` is 0), at the
//! `EnviormentalReflections` amp times the engine volume (`CCarSound +0x1d8`). Ours: at the output rate (delays and
//! filter for it, not 44.1 kHz), the left voice in the left channel and the right in the right, the delay change at
//! the first frame in the car taken at once (`StereoD0` 0x4b88d0 crossfades 256 samples), and the in-car volume
//! option taken as 1.

use crate::mixer::voices::OnePole;

/// The ring's length (`+0x205bc`, 0x8000 words).
const RING: usize = 0x8000;
/// The read cursor's start behind the write cursor (`InitD0`: 0x7f80 against 0).
const READ_LAG: usize = 0x80;
/// Samples per metre (0x7319d8 `1 / 340` times 0x7319d4 44100), at 44.1 kHz; the delay cap (0x60c0).
const PER_METRE: f32 = 0.002_941_176_5;
const MAX_DELAY: usize = 0x60c0;
/// The master's delays (m) and gains in the car.
const INSIDE_METRES: [f32; 2] = [1.6, 1.4];
const INSIDE_GAINS: [f32; 2] = [1.0, 1.0];
/// The in-car cutoff (0x443b8000), the outside floor and cap, the distance term (0x72bf14 320 times 0x7304a0 50) and
/// the factor on the distance sum (0x731d04 times 100).
const INSIDE_CUTOFF: f32 = 750.0;
const OUTSIDE_FLOOR: f32 = 100.0;
const OUTSIDE_CAP: f32 = 2000.0;
const PER_DISTANCE: f32 = 16000.0;
const PER_METRE_SUM: f32 = 31.25;
/// The volume's full-scale rpm and its slope below it (0x731cfc times 0x731cf8 -128).
const FULL_RPM: u32 = 0x1194;
const RPM_SLOPE: f32 = 0.000_222_222_22;
/// The direct block's clamp (0x5fffff and -0x600000, in the 16-bit scale).
const DIRECT_CLAMP: f32 = 24576.0;

/// The two reflections' state.
#[derive(Debug, Clone)]
pub struct Reflections {
    ring: Vec<f32>,
    at: usize,
    low_pass: OnePole,
    /// The camera has been in the car: the master holds the car's delays and gains.
    pub set: bool,
}

impl Default for Reflections {
    fn default() -> Self {
        Reflections { ring: vec![0.0; RING], at: 0, low_pass: OnePole::default(), set: false }
    }
}

/// `Blend90`'s volume at `rpm` (the engine's after the slew).
pub fn volume(rpm: u32) -> i32 {
    if rpm >= FULL_RPM {
        0x100
    } else {
        0x80 - (rpm as f32 * RPM_SLOPE * -128.0) as i32
    }
}

/// The low-pass cutoff, Hz: in the car, or outside at `distance` m from the car with the master's delays `metres`.
pub fn cutoff(inside: bool, metres: [f32; 2], distance: f32) -> f32 {
    if inside {
        return INSIDE_CUTOFF;
    }
    let far = (PER_DISTANCE / distance).min(OUTSIDE_CAP);
    ((metres[0] + metres[1]) * PER_METRE_SUM).max(OUTSIDE_FLOOR).min(far)
}

impl Reflections {
    /// The master's delays (m) and gains.
    pub fn params(&self) -> ([f32; 2], [f32; 2]) {
        if self.set {
            (INSIDE_METRES, INSIDE_GAINS)
        } else {
            ([0.0; 2], [0.0; 2])
        }
    }

    /// Adds both streams of the engine's block `raw` (after `MixScale`, 16-bit scale) into `out` at `gain` (each
    /// sample's, from the voices' gain move): in the car or not, `distance` m from the car, at `rpm`, with each
    /// side's door off the car or on it.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        raw: &[f32],
        rate: u32,
        inside: bool,
        distance: f32,
        rpm: u32,
        doors: [bool; 2],
        out: &mut [[f32; 2]],
        mut gain: impl FnMut() -> f32,
    ) {
        self.set |= inside;
        let (metres, gains) = self.params();
        let delay = metres.map(|m| ((m * PER_METRE * rate as f32) as usize).min(MAX_DELAY) + READ_LAG);
        self.low_pass.retune(rate, cutoff(inside, metres, distance));
        let vol = volume(rpm);
        // `Blend90`: `ring * ((trunc(gain * 128) * vol) >> 8) >> 16` on the ring's `y * 256`.
        let scale = gains.map(|g| (((g * 128.0) as i32 * vol) >> 8) as f32 / 256.0);
        for (frame, &x) in out.iter_mut().zip(raw) {
            self.ring[self.at] = self.low_pass.run(x);
            let direct = (x * 2.0).clamp(-DIRECT_CLAMP, DIRECT_CLAMP);
            let g = gain();
            for side in 0..2 {
                let delayed = self.ring[(self.at + RING - delay[side]) % RING] * scale[side];
                let s = if inside && doors[side] { delayed + direct } else { delayed };
                frame[side] += s * g;
            }
            self.at = (self.at + 1) % RING;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_rises_with_rpm_to_full_at_4500() {
        assert_eq!(volume(0), 0x80);
        assert_eq!(volume(1000), 0x80 + 28);
        assert_eq!(volume(4499), 0xff);
        assert_eq!(volume(4500), 0x100);
    }

    #[test]
    fn the_cutoff_is_750_in_the_car_and_100_outside_until_far() {
        assert_eq!(cutoff(true, [0.0; 2], 5.0), 750.0);
        assert_eq!(cutoff(false, INSIDE_METRES, 5.0), 100.0);
        assert_eq!(cutoff(false, INSIDE_METRES, 320.0), 50.0);
    }

    #[test]
    fn an_impulse_comes_back_a_delay_later_on_each_side() {
        let mut r = Reflections::default();
        let mut raw = vec![0.0; 600];
        raw[0] = 10000.0;
        let mut out = vec![[0.0; 2]; 600];
        r.add(&raw, 44100, true, 1.0, 5000, [false; 2], &mut out, || 1.0);
        let onset = |side: usize| out.iter().position(|f| f[side] != 0.0).unwrap();
        // 1.6 m: 207 samples, 1.4 m: 181, each 128 more.
        assert_eq!(onset(0), 207 + 128);
        assert_eq!(onset(1), 181 + 128);
    }

    #[test]
    fn a_missing_door_lets_the_engine_through_in_the_car_only() {
        let raw = vec![1000.0; 4];
        for (inside, doors, direct) in [(true, [true, false], [2000.0, 0.0]), (false, [true, true], [0.0, 0.0])] {
            let mut r = Reflections { set: true, ..Reflections::default() };
            let mut out = vec![[0.0; 2]; 4];
            r.add(&raw, 44100, inside, 1.0, 5000, doors, &mut out, || 1.0);
            assert_eq!(out[3], direct);
        }
    }

    #[test]
    fn outside_before_the_car_the_streams_are_silent() {
        let mut r = Reflections::default();
        let raw = vec![1000.0; 1000];
        let mut out = vec![[0.0; 2]; 1000];
        r.add(&raw, 44100, false, 3.0, 3000, [true; 2], &mut out, || 1.0);
        assert!(out.iter().all(|f| *f == [0.0; 2]));
    }
}
