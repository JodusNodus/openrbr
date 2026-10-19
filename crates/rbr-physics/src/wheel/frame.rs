//! 0x539290 `CWheel::Unk9290` (thiscall, `a`, `b`, `out`): two frame records are each dotted with the wheel's axis
//! (`this+0xa90`), the record with the larger magnitude picks `m`, and `out` is `m` times the axis plus the two contact
//! vectors `this+0xa70` and `this+0xa80`, scaled by the two signed terms of the normal of the frames' difference. The
//! sequence (the stores into `out`) is `layout::pick_contact`; this module holds the pure part.

use super::layout::Frame;
use crate::fpu::round_x87;
use crate::math::vec::Vec4;
use crate::Variant;

/// `round24(sqrt(x))` as the x87 gives it at 24-bit precision (see `fpu::round24`).
fn sqrt24(x: f32) -> f32 {
    crate::fpu::round24((x as f64).sqrt()) as f32
}

/// The dot of the axis with a record's vector, lanes 0..2 (`a94*t1 + a98*t2` first, then `a90*t0`, as the asm).
pub fn dot(axis: Vec4, dir: Vec4) -> f32 {
    (axis.y * dir.y + axis.z * dir.z) + axis.x * dir.x
}

/// `|d|` as the asm takes it: `0 - d` unless `d > 0` (so NaN is negated, as `comiss`+`jbe` does).
fn abs_asm(d: f32) -> f32 {
    if d > 0.0 {
        d
    } else {
        0.0 - d
    }
}

/// The scalars `Unk9290` uses: `m` (the picked dot, scales the axis), and the two terms `first` and `second` that scale
/// the contact vectors `+0xa80` and `+0xa70`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picks {
    pub m: f32,
    pub first: f32,
    pub second: f32,
}

/// The scalars of `Unk9290`. A record with no vector counts as a dot of 0.
/// The comparisons are the asm's (`!(x > 0)` takes the negating branch, and `e1 > e2` picks `d1`), so NaN follows
/// the same branches as the original.
pub fn picks(axis: Vec4, a: &Frame, b: &Frame, variant: Variant) -> Picks {
    let d1 = a.dir.map_or(0.0, |v| dot(axis, v));
    let d2 = b.dir.map_or(0.0, |v| dot(axis, v));
    let e1 = abs_asm(d1);
    let e2 = abs_asm(d2);
    let m = if e1 > e2 { d1 } else { d2 };
    // `sqrt(1 - m²)` when positive, else 0.
    let f1 = 1.0 - m * m;
    let s4 = if f1 > 0.0 { sqrt24(f1) } else { 0.0 };
    // The unit normal of the two records' lane difference (x87: each product, the sum, the root and the quotient are
    // rounded to the variant's precision, `fdivr` of 1.0, stored).
    let x87 = |v: f64| round_x87(v, variant);
    let x = b.lane_a - a.lane_a;
    let y = b.lane_b - a.lane_b;
    let sum = x87(x87(x as f64 * x as f64) + x87(y as f64 * y as f64));
    let r = x87(1.0 / x87(sum.sqrt())) as f32;
    // SSE, in the asm's order: `(r*x)*s4` and `(r*y)*s4`.
    let p = (r * x) * s4;
    let q = (r * y) * s4;
    let (first, second) = if p > 0.0 { (0.0 - p, 0.0 - (0.0 - q)) } else { (p, 0.0 - q) };
    Picks { m, first, second }
}
