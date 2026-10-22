//! The force on a gated wheel sub-object along its axis, with friction against the part of a velocity across the axis. The
//! edge is `layout::update_sub_objects` and `layout::split_on_sub`.

use crate::math::sse;
use crate::math::vec::Vec4;

/// The force per unit of scaled spin along the axis (800000; named by use).
const STIFFNESS: f32 = 800_000.0;
/// The size of the across-axis velocity below which no friction is applied.
const MIN_SLIDING: f32 = 0.001;
/// The friction against the across-axis velocity, as a fraction of the axis force (0.3).
const FRICTION: f32 = 0.3;

/// What [`axis_force`] reads: the sub-object's axis (four lanes) and scale, the wheel's spin vector, the velocity `v`
/// (three lanes), the force so far (four lanes) and the word the original takes as the fourth lane of the across-axis
/// part (a leftover of its stack, which the model keeps as an input).
#[derive(Clone, Copy, Debug)]
pub struct AxisForceIn {
    pub axis: Vec4,
    pub scale: f32,
    pub spin: Vec4,
    pub velocity: Vec4,
    pub force: Vec4,
    pub residue: f32,
}

/// What [`axis_force`] writes: the force and the across-axis part of the velocity (four lanes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisForceOut {
    pub force: Vec4,
    pub across: Vec4,
}

/// `force + d * s` in four lanes: the vector's product with `s` (the vector first), then the sum (the product first).
fn add_scaled(force: Vec4, d: Vec4, s: f32) -> Vec4 {
    Vec4 {
        x: sse::add(sse::mul(d.x, s), force.x),
        y: sse::add(sse::mul(d.y, s), force.y),
        z: sse::add(sse::mul(d.z, s), force.z),
        w: sse::add(sse::mul(d.w, s), force.w),
    }
}

/// The velocity `v` split along the sub-object's axis `n`: `p = (v . n) n`, the across part `r = v - p` in three lanes
/// (lane 3 the residue). The force gets `n * s` with `s = -(spin . n) * scale * 800000` (the spin's dot summed y, z, then
/// x), then, when `|r| > 0.001`, `r * ((-1 / |r|) * s * 0.3)`. Every lane in the original's SSE order.
pub fn axis_force(i: &AxisForceIn) -> AxisForceOut {
    let (n, v) = (i.axis, i.velocity);
    let dot = sse::add(sse::add(sse::mul(v.x, n.x), sse::mul(v.y, n.y)), sse::mul(v.z, n.z));
    let p = Vec4 { x: sse::mul(dot, n.x), y: sse::mul(dot, n.y), z: sse::mul(dot, n.z), w: sse::mul(dot, n.w) };
    let across = Vec4 { x: sse::sub(v.x, p.x), y: sse::sub(v.y, p.y), z: sse::sub(v.z, p.z), w: i.residue };
    // The length is the square root of `(x*x + y*y) + z*z`, taken through the x87 as a float.
    let len =
        sse::add(sse::add(sse::mul(across.x, across.x), sse::mul(across.y, across.y)), sse::mul(across.z, across.z))
            .sqrt();
    let k = sse::add(sse::add(sse::mul(i.spin.y, n.y), sse::mul(i.spin.z, n.z)), sse::mul(i.spin.x, n.x));
    let s = sse::mul(sse::sub(0.0, sse::mul(k, i.scale)), STIFFNESS);
    let mut force = add_scaled(i.force, n, s);
    if len > MIN_SLIDING {
        force = add_scaled(force, across, sse::mul(sse::mul(sse::div(-1.0, len), s), FRICTION));
    }
    AxisForceOut { force, across }
}
