//! The drive's accessors (`CDrive`, `RBR_E39_DriveA.cpp`, `RBR_E39_DriveB.cpp`): the setters of the input lanes
//! (throttle, brake, clutch), the commanded gear, and the getters of the engine speed, gear and drive speed key.
//! Each function is checked against the original's calls in `tests/engine_driveline_vectors.rs`.

use super::layout::{Drive, DriveDef, DriveState};

/// `CDrive::Set44A0` (0x5044a0): the throttle input.
pub fn set_throttle_in(drive: &mut Drive, v: f32) {
    drive.throttle_in.set(v);
}

/// `CDrive::Set4820` (0x504820): the clutch slip (1 = slipping).
pub fn set_clutch_slip(drive: &mut Drive, v: f32) {
    drive.clutch_slip.set(v);
}

/// `CDrive::Set30` (0x504450): the brake input, and the brake torque it scales into (`def * v`).
pub fn set_brake_in(drive: &mut Drive, def: &DriveDef, v: f32) {
    drive.brake_in.set(v);
    drive.brake_torque.set(def.max_footbrake_pressure * v);
}

/// `CDrive::Set47B0` (0x5047b0): the commanded gear moves by `delta` and is clamped to `0..=count - 1`. A negative
/// sum stores 0. The gear count is `count` (the definition's `NumberOfGears`).
pub fn request_gear(state: &mut DriveState, count: u32, delta: i32) {
    let sum = state.commanded_gear.value.wrapping_add(delta);
    let last = count as i32 - 1;
    state.commanded_gear.dirty = 1;
    state.commanded_gear.value = if sum < 0 {
        0
    } else if sum <= last {
        sum
    } else {
        last
    };
}

/// `CDrive::Get4520` (0x504520): the engine speed, rad/s.
pub fn engine_speed(state: &DriveState) -> f32 {
    state.engine_speed
}

/// `CDrive::Get4530` (0x504530): the gear.
pub fn gear(state: &DriveState) -> i32 {
    state.gear
}

/// `CDrive::Get4540` (0x504540): the drive speed key the diff locks read (`+0x384` of the state; the meaning is not
/// known, see the module README).
pub fn drive_speed_key(state: &DriveState) -> f32 {
    state.drive_speed_key
}

/// `E53SetupRows::Normalize` (0x546f70), one gear row: the ratio of the gear's teeth to the pinion's (`gear` over
/// `pinion`, each converted to float), or 0 for a row with no pinion.
pub fn gear_ratio(gear: i32, pinion: i32) -> f32 {
    if pinion == 0 {
        0.0
    } else {
        gear as f32 / pinion as f32
    }
}

/// The values `CDrive::M8770` (0x508770) derives from the drive setup for the drive definition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetupTerms {
    /// The final drive's ratio over the drop gear's.
    pub final_ratio: f32,
    /// The footbrake pressure: the rear and front pressures summed.
    pub footbrake: f32,
    /// The front and rear wheels' brake ratios: each axle's share of the footbrake pressure (0 when it is 0).
    pub front: f32,
    pub rear: f32,
}

/// `CDrive::M8770`'s derived values from the final drive and drop gear ratios and the front and rear brake pressures.
/// The shares are the pressures times the footbrake's reciprocal, which a NaN footbrake does not skip.
pub fn setup_terms(final_drive: f32, drop_gear: f32, front: f32, rear: f32) -> SetupTerms {
    let footbrake = rear + front;
    let (front, rear) = if footbrake != 0.0 {
        let inverse = 1.0 / footbrake;
        (inverse * front, inverse * rear)
    } else {
        (0.0, 0.0)
    };
    SetupTerms { final_ratio: final_drive / drop_gear, footbrake, front, rear }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_footbrake_leaves_no_brake_ratios() {
        let terms = setup_terms(3.5, 1.0, 0.0, 0.0);
        assert_eq!((terms.footbrake, terms.front, terms.rear), (0.0, 0.0, 0.0));
        assert_eq!(setup_terms(3.5, 1.0, 3.0, 1.0).front, 0.75);
        assert_eq!(gear_ratio(-32, 14), -32.0 / 14.0);
        assert_eq!(gear_ratio(38, 0), 0.0);
    }
}
