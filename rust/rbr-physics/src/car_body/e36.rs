//! Two small E36 helpers the light system and the damage ops use: `E36VecU::CrossNorm` (exe 0x4f3d40, `ret 8`) and
//! `E36Triple::MinPick` (0x4f3ec0, `ret 0`). The asm fixes the operations; `CrossNorm` scales by the reference CPU's
//! `rsqrtss` (`math::rsqrt`), not by an IEEE square root.

use crate::math::rsqrt::rsqrt;
use crate::math::vec::Vec4;

/// `E36VecU::CrossNorm(a, out)` (0x4f3d40): `this x a` on the three first lanes, each lane times `rsqrtss` of the squared
/// length of those lanes (`(x*x + y*y) + z*z`). Lane 3 is `w*a.w - a.w*w` (zero for finite words), scaled the same way.
pub fn cross_norm(this: Vec4, a: Vec4) -> Vec4 {
    let c = this.cross3(a);
    let r = rsqrt(((c.x * c.x) + c.y * c.y) + c.z * c.z);
    // The asm's lane 3 is `w*w' - w'*w`: zero for finite words, NaN for an infinite one (`eq_op` is intended).
    #[allow(clippy::eq_op)]
    let w = this.w * a.w - a.w * this.w;
    Vec4 { x: c.x * r, y: c.y * r, z: c.z * r, w: w * r }
}

/// `E36Triple`: an int pair and a float (`m_a` at +0, `m_b` at +4, `m_f` at +8).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triple {
    pub a: i32,
    pub b: i32,
    pub f: f32,
}

/// `E36Triple::MinPick()` (0x4f3ec0): `m_a` when `m_f < 0.5`, else `m_b`. An unordered `m_f` (NaN) gives `m_b`, as the
/// asm's `jbe` does.
pub fn min_pick(t: Triple) -> i32 {
    if t.f < 0.5 {
        t.a
    } else {
        t.b
    }
}
