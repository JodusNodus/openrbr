//! The tyre's friction curve: the friction coefficient against the slip, and the table interpolation of its falling
//! part.
//!
//! The branches follow the original: each comparison is `a > b` or `!(a > b)`, so a NaN takes the same path.

use crate::fpu::{round_x87, trunc_i32, INT_INDEFINITE};

/// `2^31`, the index's bound (f32, as the original compares it), and `2^32`, the offset of a negative count.
const TWO_31: f32 = 2147483648.0;
const TWO_32: f64 = 4294967296.0;

/// What the curve reads: all in the units of the slip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveInput {
    /// The slip, at least 0.
    pub slip: f32,
    /// The slip at the friction peak (a bound on where the linear part ends) and the slip where the curve reaches the
    /// sliding friction.
    pub peak_slip: f32,
    pub end_slip: f32,
    /// The friction coefficient at the peak and while sliding.
    pub peak_friction: f32,
    pub slide_friction: f32,
    /// The slip stiffness (force per unit of slip) and the load it acts on.
    pub stiffness: f32,
    pub load: f32,
    /// The pneumatic trail fraction at the start of the curve and while sliding.
    pub trail_linear: f32,
    pub trail_sliding: f32,
    /// The number of segments of the falling part's table.
    pub count: i32,
}

/// What the curve gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveOutput {
    /// The friction coefficient.
    pub friction: f64,
    /// The pneumatic trail fraction.
    pub trail_fraction: f32,
    /// The slip phase: 0 to 1 up to the peak, 1 to 2 on the falling part, 2 while sliding.
    pub phase: f32,
}

/// The index `table_lerp` takes from `p`: truncation, clamped as the original does. From `2^31` up it is `i32::MAX`.
/// Below `-2^31` and for NaN it is the integer indefinite (the original also logs, which has no effect on the
/// result).
fn index_of(p: f32) -> i32 {
    if p >= TWO_31 {
        i32::MAX
    } else if p >= -TWO_31 {
        trunc_i32(f64::from(p))
    } else {
        INT_INDEFINITE
    }
}

/// An int the original reads as unsigned: a negative value gets `2^32` added (rounded to 24 bits by the x87).
fn unsigned_f64(v: i32) -> f64 {
    let round24 = round_x87;
    if v < 0 {
        round24(f64::from(v) + TWO_32)
    } else {
        f64::from(v)
    }
}

/// The table `table(i)` interpolated at `p = x · n`, where the segment count `n` is read as unsigned: `x` in `[0, 1]`
/// spans the `n` segments. Returns the x87 value.
pub fn table_lerp(x: f32, n: i32, table: impl Fn(u32) -> f32) -> f64 {
    let round24 = round_x87;
    let p = round24(unsigned_f64(n) * f64::from(x)) as f32;
    let k = (index_of(p) as u32).min(n as u32);
    let frac = round24(f64::from(p) - unsigned_f64(k as i32));
    let low = round24(round24(1.0 - frac) * f64::from(table(k)));
    let high = round24(frac * f64::from(table(k + 1)));
    round24(low + high)
}

/// The friction curve. Up to the peak it is a Hermite cubic in `slip / m`, where `m` is the slip at which the linear
/// stiffness reaches the peak (`1.2 · peak_friction · load / stiffness`, tripled when it is under `peak_slip`, and
/// at most `peak_slip` then); beyond the peak it falls to the sliding friction over the tabulated falloff up to
/// `end_slip`; beyond that it stays at the sliding friction.
pub fn friction_curve(x: &CurveInput, falloff: impl Fn(u32) -> f32) -> CurveOutput {
    let round24 = round_x87;
    let CurveInput { slip, peak_slip, end_slip, peak_friction, slide_friction, stiffness, load, .. } = *x;
    let linear_slip = peak_friction * load / stiffness * 1.2;
    // The larger of `linear_slip` and `peak_slip`, where `linear_slip` is first tripled when it is below `peak_slip`.
    let knee = if linear_slip > peak_slip {
        linear_slip
    } else {
        let t = linear_slip * 3.0;
        if peak_slip > t {
            t
        } else {
            peak_slip
        }
    };
    if knee > slip {
        let q = slip / knee;
        let r = (knee / (peak_friction * load)) * stiffness;
        let p = (3.0 - r * 2.0) + (r - 2.0) * q;
        let v = (p * (q * q) + q * r) * peak_friction;
        CurveOutput {
            friction: f64::from(v),
            trail_fraction: (1.0 - q) * x.trail_linear + q * x.trail_sliding,
            phase: q,
        }
    } else if end_slip > slip {
        let s = (slip - knee) / (end_slip - knee);
        let f = table_lerp(s, x.count, falloff);
        let g = round24(round24(f64::from(peak_friction) - f64::from(slide_friction)) * f);
        CurveOutput {
            friction: round24(g + f64::from(slide_friction)),
            trail_fraction: round24(f * f64::from(x.trail_sliding)) as f32,
            phase: s + 1.0,
        }
    } else {
        CurveOutput { friction: f64::from(slide_friction), trail_fraction: 0.0, phase: 2.0 }
    }
}
