//! The steering walk and the spin's cross product. The sequences are `layout::walk_result` and `layout::spin_cross`; this
//! module holds the pure parts.

use crate::fpu::round_x87;
use crate::math::vec::{Vec3, Vec4};

/// The size below which a lever factor counts as zero.
const MIN_LEVER: f32 = 0.001;
/// The factor on the spin of the second vector of the spin cross (named by use).
const SPIN_ARM: f32 = 0.14;

/// The steering gain: `(lever_a / lever_b) * steer_rate` when `|lever_b| > 0.001`, else 0. The lever factors are the
/// refresh's inverses of the two lever terms. `steer_rate` is asked only in the first case (x87 order: the quotient, then
/// the product with the rate, at the step's precision, stored as f32).
pub fn steer_gain(lever_a: f32, lever_b: f32, steer_rate: impl FnOnce() -> f32) -> f32 {
    // `if (f <= 0) f = -f`: a NaN is negated, which is still NaN.
    let magnitude = if lever_b > 0.0 { lever_b } else { 0.0 - lever_b };
    if magnitude > MIN_LEVER {
        let rate = f64::from(steer_rate());
        let quotient = round_x87(f64::from(lever_a) / f64::from(lever_b));
        round_x87(quotient * rate) as f32
    } else {
        0.0
    }
}

/// `strut_direction * gain + walked`, the vector the steering walk stores.
pub fn steered_walk(strut_direction: Vec4, gain: f32, walked: Vec4) -> Vec4 {
    strut_direction.scale(gain) + walked
}

/// The cross product the spin stores: `v x w` with `v = axle * spin + walked` and `w = axle * (spin * 0.14)`.
pub fn spin_cross(walked: Vec4, axle: Vec4, spin: f32) -> Vec3 {
    let v = axle.scale(spin) + walked;
    let w = axle.scale(spin * SPIN_ARM);
    Vec3 { x: v.x, y: v.y, z: v.z }.cross3(Vec3 { x: w.x, y: w.y, z: w.z })
}
