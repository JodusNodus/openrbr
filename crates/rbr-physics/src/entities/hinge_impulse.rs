//! A hinged body's response to an impulse applied at a point: the torque it puts about the hinge axis changes the hinge
//! rate (divided by the definition's inertia term), and the impulse's component along the axis, clamped to the
//! definition's bounds, is what the car is given and what decides whether the hinge breaks off.

use crate::math::vec::Vec4;

/// The arguments sent to the car's impulse call. The zeros are part of the original call.
pub fn car_impulse_args(point: u32, axial_impulse: u32, arg4: u32, magnitude: f32, arg7: u32) -> [u32; 8] {
    [point, axial_impulse, 0, 0, arg4, magnitude.to_bits(), 0, arg7]
}

/// The torque's component about the hinge axis: `axis . torque`, summed `z*z + y*y` first, then `x*x`, as the original does.
pub fn torque_about_axis(axis: Vec4, torque: Vec4) -> f32 {
    let z_y = axis.z * torque.z + axis.y * torque.y;
    z_y + axis.x * torque.x
}

/// The impulse's component along the axis against its bounds: the upper bound is checked first, then the lower; a value
/// past either is replaced by it (`true`). NaNs pass through both tests.
pub fn clamp_axial_impulse(axial: f32, high: f32, low: f32) -> (f32, bool) {
    if high < axial {
        (high, true)
    } else if axial < low {
        (low, true)
    } else {
        (axial, false)
    }
}

/// The magnitude the car is given: the value when it is positive, else `0 - value`. Non-positive values, including NaN,
/// take the second form.
pub fn car_magnitude(axial: f32) -> f32 {
    if axial > 0.0 {
        axial
    } else {
        0.0 - axial
    }
}
