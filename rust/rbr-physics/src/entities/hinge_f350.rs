//! Arithmetic and call shapes for `CHingedRigidBody::SimF350` (0x51f350).

use crate::math::vec::Vec4;

/// The arguments sent to the car's slot 0x128. The zeros are part of the original call.
pub fn car_impulse_args(point: u32, scaled: u32, e: u32, dot: f32, h: u32) -> [u32; 8] {
    [point, scaled, 0, 0, e, dot.to_bits(), 0, h]
}

/// The original forms `tz*cz + ty*cy`, then adds `tx*cx`.
pub fn torque_projection(t: Vec4, cross: Vec4) -> f32 {
    let z_y = t.z * cross.z + t.y * cross.y;
    z_y + t.x * cross.x
}

/// The original checks the upper bound first, then the lower bound. NaNs pass through both tests.
pub fn clamp_projection(dot: f32, high: f32, low: f32) -> (f32, bool) {
    if high < dot {
        (high, true)
    } else if dot < low {
        (low, true)
    } else {
        (dot, false)
    }
}

/// The COMISS/JBE branch sends non-positive values, including unordered NaNs, through `0.0 - dot`.
pub fn car_projection(dot: f32) -> f32 {
    if dot > 0.0 {
        dot
    } else {
        0.0 - dot
    }
}
