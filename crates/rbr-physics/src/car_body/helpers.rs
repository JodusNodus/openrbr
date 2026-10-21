//! Two small helpers the light system and the damage tables use: a normalised cross product and the pick of a surface
//! pair by blend weight.

use crate::math::rsqrt::rsqrt;
use crate::math::vec::Vec4;

/// `this x a` on the three first lanes, each lane times the reciprocal square root of the squared length of those
/// lanes (`(x*x + y*y) + z*z`). Lane 3 is `w*a.w - a.w*w` (zero for finite words), scaled the same way. The scale is the
/// reference CPU's reciprocal square root estimate (`math::rsqrt`), not an IEEE square root.
pub fn cross_normalized(this: Vec4, a: Vec4) -> Vec4 {
    let c = this.cross3(a);
    let r = rsqrt(((c.x * c.x) + c.y * c.y) + c.z * c.z);
    // Lane 3 is `w*w' - w'*w`: zero for finite words, NaN for an infinite one (`eq_op` is intended).
    #[allow(clippy::eq_op)]
    let w = this.w * a.w - a.w * this.w;
    Vec4 { x: c.x * r, y: c.y * r, z: c.z * r, w: w * r }
}

/// Two surface ids and the blend weight between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceBlend {
    pub first: i32,
    pub second: i32,
    pub weight: f32,
}

/// The first surface when the weight is below 0.5, else the second. An unordered weight (NaN) gives the second, as the
/// original's compare-and-branch does.
pub fn dominant_surface(blend: SurfaceBlend) -> i32 {
    if blend.weight < 0.5 {
        blend.first
    } else {
        blend.second
    }
}
