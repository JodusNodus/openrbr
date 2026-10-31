//! Applies the impulse `b` at the point `a`. The lever arm `d = a - position` gives the torque `d x b`; the body's
//! transform of the torque is scaled lanewise by the local inverse inertia, transformed again, and `0.6` of it is
//! added to the angular velocity; the impulse times the inverse mass is added to the linear velocity. The hook and
//! the ring tail run after that.

use super::layout::{Body, Def, Vec4};
use crate::math::sse;

/// The factor the angular term takes.
const ANGULAR_FACTOR: f32 = 0.6;

/// The two transforms the impulse makes outside the body, in order.
pub trait ImpulseHost {
    /// The transform of the torque, with the body as the matrix.
    fn transform_by_body(&mut self, src: Vec4) -> Vec4;
    /// The transform of the scaled torque, with the walker as the matrix.
    fn transform_by_walker(&mut self, src: Vec4) -> Vec4;
}

/// Applies `impulse` at `point` to the body (`def` has its inverse mass and inverse inertia diagonal).
pub fn impulse(body: &mut Body, def: &Def, point: Vec4, impulse: Vec4, host: &mut impl ImpulseHost) {
    let arm = Vec4 { x: point.x - body.position.x, y: point.y - body.position.y, z: point.z - body.position.z, w: 0.0 };
    let torque = host.transform_by_body(arm.cross3(impulse));
    let torque = host.transform_by_walker(torque.mul_lanes(def.local_inverse_inertia));
    body.angular_velocity = body.angular_velocity.add_scaled(torque, ANGULAR_FACTOR);
    body.linear_velocity = body.linear_velocity.add_scaled(impulse, def.inverse_mass);
}

/// Which of the two words the hook gets: the first when `half` is above the pair's third lane `z`. An unordered `z`
/// takes the second.
pub fn hook_takes_first(z: f32, half: f32) -> bool {
    half > z
}

/// Where the ring append writes in a ring with `index` and `count`, and the index and count after: `None` when a
/// ring that does not wrap is full (64 entries). A wrapping ring's index goes round, and its count is at least the new
/// index plus one.
pub fn ring_step(wraps: bool, index: u32, count: u32) -> Option<(u32, u32)> {
    if wraps {
        let next = index.wrapping_add(1) & 0x3f;
        Some((next, count.max(next + 1)))
    } else if index < 0x40 {
        Some((index + 1, count.wrapping_add(1)))
    } else {
        None
    }
}

/// The scalar impulse adjustment. The first three lanes of `accumulator` are also the axis used by the dot product.
/// The vector update uses all four direction lanes, including the caller's original lane-3 stack residue.
pub fn contact_impulse(
    accumulator: Vec4,
    direction: Vec4,
    penetration: f32,
    minimum_penetration: f32,
    response: f32,
    mu: f32,
) -> Option<Vec4> {
    let n = sse::sub(penetration, minimum_penetration);
    let dot = sse::add(
        sse::add(sse::mul(accumulator.z, direction.z), sse::mul(accumulator.y, direction.y)),
        sse::mul(accumulator.x, direction.x),
    );
    if !(dot < 0.0) {
        return None;
    }

    let mut magnitude = sse::sub(0.0, dot);
    if n > 0.009_999_999_8 {
        let dot2 = sse::div(sse::mul(dot, dot), sse::mul(n, 2.0));
        if sse::div(magnitude, dot2) > mu {
            magnitude = sse::mul(sse::mul(dot2, response), mu);
        }
    }
    Some(Vec4 {
        x: sse::add(sse::mul(direction.x, magnitude), accumulator.x),
        y: sse::add(sse::mul(direction.y, magnitude), accumulator.y),
        z: sse::add(sse::mul(direction.z, magnitude), accumulator.z),
        w: sse::add(sse::mul(direction.w, magnitude), accumulator.w),
    })
}
