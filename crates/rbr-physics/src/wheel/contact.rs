//! The ground probe's contact: how two ray blocks combine into the contact vector. The sequence (the stores into the
//! output) is `layout::pick_contact`; this module holds the pure part.

use crate::fpu::round_x87;
use crate::math::vec::Vec4;

/// The part of a ray block the contact reads: its point (only when the block's gate word is nonzero) and two floats of
/// unknown meaning (`coord_a`, and `difference`, the block's limited difference).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayBlock {
    pub point: Option<Vec4>,
    pub coord_a: f32,
    pub difference: f32,
}

/// `round24(sqrt(x))` as the x87 gives it at 24-bit precision (see `fpu::round24`).
fn sqrt24(x: f32) -> f32 {
    crate::fpu::round24((x as f64).sqrt()) as f32
}

/// The dot of the axis with a point, lanes 0..2, summed y, z, then x.
pub fn dot(axis: Vec4, point: Vec4) -> f32 {
    (axis.y * point.y + axis.z * point.z) + axis.x * point.x
}

/// `|d|` with the original's compare: `0 - d` unless `d > 0` (so NaN is negated).
fn abs_of(d: f32) -> f32 {
    if d > 0.0 {
        d
    } else {
        0.0 - d
    }
}

/// The scalars that build the contact vector: `axis_scale` scales the axis, `first` the wheel's second contact vector and
/// `second` its first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactMix {
    pub axis_scale: f32,
    pub first: f32,
    pub second: f32,
}

/// The mix of two ray blocks along `axis`. A block with no point counts as a dot of 0. `axis_scale` is the dot of larger
/// magnitude (`b`'s on a tie or a NaN). `s = sqrt(1 - axis_scale^2)` when positive, else 0. The unit normal of the blocks'
/// coordinate difference, `r * (b.coord_a - a.coord_a, b.difference - a.difference)`, scales by `s`; its sign makes `first`
/// and `second`. The comparisons are the original's (`!(x > 0)` takes the negating branch, and `e1 > e2` picks `d1`), so
/// NaN follows the same branches.
pub fn mix_contacts(axis: Vec4, a: &RayBlock, b: &RayBlock) -> ContactMix {
    let d1 = a.point.map_or(0.0, |v| dot(axis, v));
    let d2 = b.point.map_or(0.0, |v| dot(axis, v));
    let e1 = abs_of(d1);
    let e2 = abs_of(d2);
    let m = if e1 > e2 { d1 } else { d2 };
    let f1 = 1.0 - m * m;
    let s = if f1 > 0.0 { sqrt24(f1) } else { 0.0 };
    // The unit normal: each product, the sum, the root and the quotient are rounded to the thread's precision, the
    // reciprocal of the root stored as a float.
    let x87 = round_x87;
    let x = b.coord_a - a.coord_a;
    let y = b.difference - a.difference;
    let sum = x87(x87(x as f64 * x as f64) + x87(y as f64 * y as f64));
    let r = x87(1.0 / x87(sum.sqrt())) as f32;
    // SSE, in the original's order: `(r*x)*s` and `(r*y)*s`.
    let p = (r * x) * s;
    let q = (r * y) * s;
    let (first, second) = if p > 0.0 { (0.0 - p, 0.0 - (0.0 - q)) } else { (p, 0.0 - q) };
    ContactMix { axis_scale: m, first, second }
}
