//! The walk and the spin's vectors: `M3AEF0` (0x53aef0) and `M3AFB0` (0x53afb0). The sequences are `layout::walk_result`
//! and `layout::spin_cross`; this module holds the pure parts. `MB560` is `E43PairWalk::MB560` (0x51b560), and the gain
//! takes the car's `GetB1C` (0x4e65f0).

use crate::fpu::round_x87;
use crate::math::vec::{Vec3, Vec4};
use crate::Variant;

/// `E43PairWalk::MB560` (0x51b560, thiscall, `ret 0xc`): walks `a` against `b` on the walker and returns the result (the
/// `c` the original writes). The walk is the entities port's; the caller answers it (the record, in the tests).
pub trait WalkCalls {
    /// `MB560(walker, a, b, c)`: `a` is the walked vector (`None` when its guard is null, the asm's 0), `car` is `b`, and the
    /// result is what the original writes to `c`.
    fn pair_walk(&mut self, walker: u32, a: Option<Vec4>, car: u32) -> Vec4;
}

/// The gain of `M3AEF0`: `(ratio_num / ratio_den) * GetB1C` when `|ratio_den| > 0.001`, else 0. `rate_b1c` is called only
/// in the first case (x87 order: the quotient, then the product with `GetB1C`, at `variant`'s precision, stored as f32).
pub fn walk_gain(ratio_num: f32, ratio_den: f32, rate_b1c: impl FnOnce() -> f32, variant: Variant) -> f32 {
    // `if (f <= 0) f = -f` as the asm's `comiss`/`jbe` does: a NaN is negated, which is still NaN.
    let magnitude = if ratio_den > 0.0 { ratio_den } else { 0.0 - ratio_den };
    if magnitude > 0.001 {
        let b1c = f64::from(rate_b1c());
        let quotient = round_x87(f64::from(ratio_num) / f64::from(ratio_den), variant);
        round_x87(quotient * b1c, variant) as f32
    } else {
        0.0
    }
}

/// `offset * gain + walk`, the vector `M3AEF0` stores to the walk output.
pub fn offset_walk(offset: Vec4, gain: f32, walk: Vec4) -> Vec4 {
    offset.scale(gain) + walk
}

/// The cross product `M3AFB0` writes: `buf x s30` with `buf = scaled * spin + walk` and `s30 = scaled * (spin * 0.14)`.
pub fn spin_product(walk: Vec4, scaled: Vec4, spin: f32) -> Vec3 {
    let buf = scaled.scale(spin) + walk;
    let s30 = scaled.scale(spin * 0.14);
    Vec3 { x: buf.x, y: buf.y, z: buf.z }.cross3(Vec3 { x: s30.x, y: s30.y, z: s30.z })
}
