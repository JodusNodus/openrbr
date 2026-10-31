//! The slip query that the flag dispatch makes before a downshift. Two gates, each on
//! when its slip gate (the setup's, copied from the definition) is nonzero. The rpm gate, in gears above 2, asks whether
//! the gear's rpm is above the rev limit. The speed gate, in second gear, asks whether the car speed's magnitude
//! is above 10 km/h. The operation order is fixed: the rpm is rounded to f32
//! before it multiplies the gear ratio.

use super::gear::gear_index;
use super::layout::{GearRatios, RPM_PER_RAD_S};

/// 10 km/h in m/s.
const SLIP_SPEED: f32 = 2.777_777_9;

/// The slip answer. `engine` is the engine speed (rad/s), `car_speed` the car's speed (m/s); both are passed in by
/// the caller.
pub fn query_slip(
    gate_rpm: u32,
    gate_speed: u32,
    gear: u32,
    gears: &GearRatios,
    rpm_limit: f32,
    engine: f32,
    car_speed: f32,
) -> bool {
    // The gear compares as a signed word (`jle`).
    let gear_signed = gear as i32;
    if gate_rpm != 0 && gear_signed > 2 {
        let k = gear_index(gear);
        let rpm = engine * RPM_PER_RAD_S;
        let v = gears.lim[k] / gears.div[k] * rpm;
        if v > rpm_limit {
            return true;
        }
    }
    if gate_speed != 0 && gear == 2 {
        let speed = if car_speed > 0.0 { car_speed } else { 0.0 - car_speed };
        return speed > SLIP_SPEED;
    }
    false
}
