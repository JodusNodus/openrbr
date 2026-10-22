//! The steering force-feedback torque (`notes/ngp.md` "Force feedback"): its filter. The order is the original's; the
//! constants are its own single-precision words (0.2 is one ulp below 0.2 as written, 1/6 is the nearest single).

use crate::car_body::store::CarStore;

/// The new filtered torque. While `override_timer` is above 0 the value is 0. Otherwise the target is
/// `rack_ratio * torque_sum` ramped by a speed factor (0.5 at rest, 0.7 above 13 m/s), blended with the current
/// `torque`. The caller dirties the cell in both cases (`accessors::set_cell`).
pub fn filtered_torque(override_timer: f32, speed: f32, rack_ratio: f32, torque_sum: f32, torque: f32) -> f32 {
    if override_timer > 0.0 {
        return 0.0;
    }
    // NaN-preserving clamp: a NaN speed stays NaN.
    let mut ramp = (speed - 7.0) * 0.16666667;
    if ramp < 0.0 {
        ramp = 0.0;
    } else if 1.0 < ramp {
        ramp = 1.0;
    }
    let blend = ramp * 0.19999998807907104 + 0.5;
    // Order: ((rack_ratio * torque_sum) * blend) + ((1 - blend) * torque).
    let target = rack_ratio * torque_sum;
    (target * blend) + ((1.0 - blend) * torque)
}

/// The force sent to the wheel for the car's state, in [-1, 1] before the player's strength and direction: NGP's curve
/// (`ngp::ffb`), in place of 1.02's. It is 0 while the car's force-feedback mute is set.
pub fn force<S: CarStore>(mem: &S, car: u32) -> f32 {
    crate::ngp::ffb::curve(mem.ffb_torque(car).value, mem.ffb_muted(car), mem.max_ff_torque(car))
}
