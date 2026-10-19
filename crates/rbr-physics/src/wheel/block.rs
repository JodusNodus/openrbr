//! The tyre block's pose blend and its clamp flag: `M3D230` (0x53d230) blends the two material tags of the block into the
//! blend and the total, and `M3D460` (0x53d460) sets the flag from the difference of the target and the total. The
//! sequences (the reads, the host calls and the stores, in the asm's order) are `layout::tag_blend` and `layout::pose_flag`;
//! this module holds the pure parts. Operation order follows the asm (`RBR_E51_WheelB.cpp:151-236`): the first sine is
//! stored as f32, the second stays in x87 st0 for the sum, and each x87 arithmetic result rounds at PC=24.

use super::axes::CrtTrig;
use crate::fpu::round24;

/// `2*pi` as f32: the asm's `0x7427a8` (`6.28318548f`).
const TAU: f32 = std::f32::consts::TAU;

/// The host's answer to `E41Plain::InitRealLerp` (0x50f010, cdecl): the lerp `(b - a) * t + a` the caller reads at the
/// buffer's +0xc. The callee's `NTweak::CBase` constructor (0x52f090) writes the buffer's header and is the host's to
/// model; the caller never reads it.
pub trait RealLerp {
    fn init_real_lerp(&mut self, a: f32, b: f32, t: f32) -> f32;
}

/// The ratio `2*pi / period` of a material's phase (the asm's `6.28318548f / +0xd8`).
pub fn phase_ratio(period: f32) -> f32 {
    TAU / period
}

/// The two sines of a material's phase: `sin(phase_b * r)` rounded to f32 when it is stored, and `sin(phase_a * r)`
/// unrounded (the asm adds it in the x87). `phase_a` is the block's `+0x10`, `phase_b` its `+0x14`.
pub fn phase_sines(trig: &mut impl CrtTrig, phase_a: f32, phase_b: f32, r: f32) -> (f32, f64) {
    let s1 = trig.sin(phase_b * r) as f32;
    let s2 = trig.sin(phase_a * r);
    (s1, s2)
}

/// The first material's term of the blend. The x87 adds the two sines, multiplies by gain, then by weight; every
/// operation rounds to the game's 24-bit x87 precision before the final f32 store.
pub fn first_term(s1: f32, s2: f64, gain: f32, weight: f32) -> f32 {
    let sum = round24(s2 + f64::from(s1));
    let scaled = round24(sum * f64::from(gain));
    round24(scaled * f64::from(weight)) as f32
}

/// The second material's term, added to the blend so far. In x87 order it subtracts weight from 1, multiplies that by
/// the sine sum, scales by gain, and adds the previous f32 accumulator; each operation rounds to PC=24.
pub fn second_term(acc: f32, s1: f32, s2: f64, gain: f32, weight: f32) -> f32 {
    let inverse_weight = round24(1.0 - f64::from(weight));
    let sum = round24(s2 + f64::from(s1));
    let weighted = round24(inverse_weight * sum);
    let scaled = round24(weighted * f64::from(gain));
    round24(scaled + f64::from(acc)) as f32
}

/// What `M3D460` stores after the blend: the difference, the flag at `+0x4`, and the clamps `+0x5c` and `+0x60`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlagOut {
    pub diff: f32,
    pub flag: u32,
    pub clamp: Option<[f32; 2]>,
}

/// The flag outcome of `M3D460` for the difference `d` (`target - total`) and the floats `b` and `c`. `d <= b` sets the
/// flag, and the diff is then `c` when `d < c`; the clamps are zeroed. Otherwise the flag is cleared and nothing else is
/// written. A NaN `d` takes the second branch, as the asm's unordered compares do.
pub fn flag_outcome(d: f32, b: f32, c: f32) -> FlagOut {
    if d <= b {
        let diff = if d < c { c } else { d };
        FlagOut { diff, flag: 1, clamp: Some([0.0, 0.0]) }
    } else {
        FlagOut { diff: d, flag: 0, clamp: None }
    }
}
