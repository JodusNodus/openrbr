//! Pure math the physics ports share: the reference CPU's `rsqrtss` (`rsqrt`) and `rcpss` (`rcp`), its scalar SSE
//! operations (`sse`), the CRT float-to-int (`ftol`), the quaternion product and normalise (`quat`), and the
//! absolute value, compare and sign forms (`abs_asm`, `le_or_unordered`, `sign`). They follow the operation order;
//! `notes/physics.md` "x87 and float rules" has the x87 and SSE model.

pub mod ftol;
pub mod quat;
pub mod rcp;
pub mod rsqrt;
pub mod sse;
pub mod table;
pub mod trig;
pub mod vec;

/// A 3x3 matrix, `m[row][col]`. It reads as the nine words `v[3 * row + col]`.
pub type Mat3 = [[f32; 3]; 3];

/// `|v|` formed as: `v` when `v > 0`, else `0 - v` (so a NaN takes `0 - v`).
pub fn abs_asm(v: f32) -> f32 {
    if v > 0.0 {
        v
    } else {
        0.0 - v
    }
}

/// `a <= b` as the below-or-equal compare (`jbe`, or `ja` not taken) takes it: a NaN on either side counts as
/// less or equal.
pub fn le_or_unordered(a: f32, b: f32) -> bool {
    a.partial_cmp(&b) != Some(std::cmp::Ordering::Greater)
}

/// 1 above 0, -1 below 0, 0 otherwise: the sign the SSE compare gives (a NaN gives 0).
pub fn sign(v: f32) -> i32 {
    if v > 0.0 {
        1
    } else if 0.0 > v {
        -1
    } else {
        0
    }
}
