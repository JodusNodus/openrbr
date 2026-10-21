//! The brake wall's friction: while the car penetrates the brake wall (`wall_penetration` above 0), the force
//! accumulator (moved to the world frame) less the weight gives the support force `support`. When its z is not below 0
//! and the horizontal speed is above `MIN_SPEED`, a force opposing the velocity, scaled by the support, the penetration
//! and the speed, and the torque of that force about the lever arm, are added to the car's accumulators. The order is
//! the original's (the reconstructed C++ differs, see its open items).

use crate::car_body::store::CarStore;
use crate::math::vec::Vec3;
use crate::mem::Mem;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait WallFrictionCalls<S = Mem> {
    /// The car-frame vector `v` in the world frame.
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3;
    /// Walks `a` through the world with the car as the identity entity (as `DragCalls::walk_vector`).
    fn walk_vector(&mut self, a: Vec3, car: u32) -> Vec3;
    /// Adds `force` to the car's force accumulator.
    fn wall_force(&mut self, mem: &mut S, car: u32, force: Vec3);
    /// Adds `torque` to the car's torque accumulator.
    fn wall_torque(&mut self, mem: &mut S, car: u32, torque: Vec3);
}

/// The weight of the wall penetration, per m of penetration.
const PENETRATION_GAIN: f32 = 3.0;

/// The weight of the speed in the low-speed term, per m/s (it saturates at 0.5 m/s).
const LOW_SPEED_GAIN: f32 = 2.0;

/// The weight of the speed in the high-speed term, per m/s (it saturates at 100 m/s).
const HIGH_SPEED_GAIN: f32 = 0.01;

/// The share of the high-speed term.
const HIGH_SPEED_SHARE: f32 = 0.5;

/// The share of the low-speed term.
const LOW_SPEED_SHARE: f32 = 0.3;

/// The gain of the yaw rate's weight in the torque's z lane, s.
const YAW_GAIN: f32 = 2.5;

/// The lever arm of the force from the centre, m: `(0, 0, -0.3)`.
const LEVER_ARM: Vec3 = Vec3 { x: 0.0, y: 0.0, z: -0.3 };

/// The horizontal speed, m/s, at or below which the wall does nothing.
const MIN_SPEED: f32 = 0.001;

/// Clamps `x` to `[lo, hi]` with compare-and-select: a NaN stays.
pub(super) fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if lo > x {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// Adds the wall's friction force and torque to the car's accumulators. The weight is `-1 / inverse_mass` times
/// gravity.
pub fn wall_friction<S: CarStore>(calls: &mut impl WallFrictionCalls<S>, mem: &mut S, car: u32) {
    let penetration = mem.wall_penetration(car);
    if !(penetration > 0.0) {
        return;
    }
    // The weight is added in place to the accumulated force, in the world frame (`s * gravity + force`).
    let weight_scale = -1.0 / mem.inverse_mass(car);
    let gravity = mem.gravity();
    let force = calls.to_world(car, mem.force_accumulator(car));
    let net = Vec3 {
        x: gravity.x * weight_scale + force.x,
        y: gravity.y * weight_scale + force.y,
        z: gravity.z * weight_scale + force.z,
    };
    let support = net.z;
    if 0.0 > support {
        return;
    }
    // The horizontal speed is the length of the velocity with its z lane forced to 0.
    let velocity = mem.velocity(car);
    let speed = Vec3 { x: velocity.x, y: velocity.y, z: 0.0 }.length3();
    if !(speed > MIN_SPEED) {
        return;
    }
    let unit = velocity.scale(1.0 / speed);
    let wall_weight = clamp(penetration * PENETRATION_GAIN, 0.0, 1.0);
    let low_speed_weight = clamp(speed * LOW_SPEED_GAIN, 0.0, 1.0);
    // The original scales the speed here (its register still holds it); the C++ scales the low-speed weight instead.
    let high_speed_weight = clamp(speed * HIGH_SPEED_GAIN, 0.0, 1.0);
    let dir = unit.scale(
        0.0 - (high_speed_weight * HIGH_SPEED_SHARE + low_speed_weight * LOW_SPEED_SHARE) * wall_weight * support,
    );
    let yaw_weight = clamp(mem.angular_velocity(car).z * YAW_GAIN, -1.0, 1.0);
    let mut torque = LEVER_ARM.cross3(dir);
    torque.z += yaw_weight * low_speed_weight * wall_weight * support * LEVER_ARM.z;
    let force = calls.walk_vector(dir, car);
    let torque = calls.walk_vector(torque, car);
    calls.wall_force(mem, car, force);
    calls.wall_torque(mem, car, torque);
}
