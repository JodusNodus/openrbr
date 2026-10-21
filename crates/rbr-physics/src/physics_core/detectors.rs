//! The arithmetic of the detectors and the contact loop: the gates, the probe's box and the step fraction. The
//! detector calls themselves are in `layout`.

use super::math::Vec3;

/// Whether each of the three lanes is strictly below zero (NaN is not).
pub fn all_below_zero(v: Vec3) -> bool {
    0.0 > v.x && 0.0 > v.y && 0.0 > v.z
}

/// The box the probe looks up: the corners `center - half_size` and `center + half_size` (three lanes; the original
/// leaves lane 3 as the stack had it). `center` and `half_size` are named by use.
pub fn probe_box(center: Vec3, half_size: Vec3) -> (Vec3, Vec3) {
    let (a, b) = (center, half_size);
    let lo = Vec3 { x: a.x - b.x, y: a.y - b.y, z: a.z - b.z };
    let hi = Vec3 { x: a.x + b.x, y: a.y + b.y, z: b.z + a.z };
    (lo, hi)
}

/// The most entities the probe collects.
pub const PROBE_MAX_HITS: u32 = 8;

/// Whether the probe skips an entity before its tests: a flag word with a bit of `mask`, or the excluded entity.
pub fn probe_skips(flags: u32, mask: u32, entity: u32, excluded: u32) -> bool {
    flags & mask != 0 || entity == excluded
}

/// The contact loop's two bounds of a window (s): the fraction's offset `window * 0` and the floor `window * 0.05` the
/// window left has to stay above for the loop to go on.
pub fn window_bounds(window: f32) -> (f32, f32) {
    (window * 0.0, window * 0.05)
}

/// Whether the solver's fraction is above 1.1, where the loop takes no step. The original compares the x87 result
/// unrounded, and an unordered fraction counts as above.
pub fn fraction_overshoots(fraction: f64) -> bool {
    fraction.is_nan() || f64::from(1.1f32) < fraction
}

/// The step fraction `fraction - offset / window`, clamped to [0, 1] as the original's compares do (a NaN stays).
pub fn step_fraction(fraction: f32, offset: f32, window: f32) -> f32 {
    let f = fraction - offset / window;
    if 0.0 > f {
        0.0
    } else if f > 1.0 {
        1.0
    } else {
        f
    }
}

/// The window left after a step of `fraction`.
pub fn window_left(fraction: f32, window: f32) -> f32 {
    (1.0 - fraction) * window
}
