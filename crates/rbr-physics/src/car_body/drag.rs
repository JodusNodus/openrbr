//! Drag and its torque: the drag force on the car's velocity and the damping of its angular velocity. The order is the
//! original's: the gate's pre-step first, then the velocity is read. The force and the damping are added to the car's
//! force and torque accumulators. Vectors are three lanes: the fourth lane, which the accumulators write into the
//! car, is never read in the drive.

use crate::car_body::store::CarStore;
use crate::fpu::round_x87;
use crate::math::vec::Vec3;
use crate::mem::Mem;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait DragCalls<S = Mem> {
    /// The car's pre-step, called first when the pre-step gate is set. It may change the car, so the port reads the
    /// velocity after it.
    fn pre_step(&mut self, mem: &mut S, car: u32);
    /// Walks `a` through the world with the car as the identity entity and returns the result.
    fn walk_vector(&mut self, a: Vec3, car: u32) -> Vec3;
    /// Adds the drag force to the car's force accumulator.
    fn drag_force(&mut self, mem: &mut S, car: u32, force: Vec3);
    /// Adds the damping torque to the car's torque accumulator.
    fn spin_damping(&mut self, mem: &mut S, car: u32, damping: Vec3);
}

/// The drag's lever arm from the centre, m: the damping torque starts as `arm x drag`. Its meaning is open.
const LEVER_ARM: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.05 };

/// 1.02's drag factor, the `1/2` of `Cd * A * |v| / 2`. NGP replaces it with `0.5 rho / 1.20407` from the air density,
/// which its downforce update writes every step.
pub const DRAG_FACTOR: f32 = 0.5;

/// The angular speed, rad/s, above which the spin is damped harder.
const SPIN_LIMIT: f32 = 5.0;

/// The gain of the over-limit damping.
const SPIN_GAIN: f32 = -5.0;

/// Adds the drag and its torque. The drag opposes the car's velocity, scaled by `Cd * A * |v| / 2`; the damping is the
/// cross product of the lever arm with the drag, plus `k * spin` once the walked spin exceeds `SPIN_LIMIT`, where
/// `k = (|spin| - 5)^2 * -5 / |spin|`. `factor` is the drag factor ([`DRAG_FACTOR`] in 1.02).
pub fn apply_drag<S: CarStore>(calls: &mut impl DragCalls<S>, mem: &mut S, car: u32, factor: f32) {
    if mem.drag_pre_step_gate() {
        calls.pre_step(mem, car);
    }
    // Negated lane by lane (`0.0 - v`, not `-v`: they differ on a zero lane).
    let velocity = mem.velocity(car);
    let relative = Vec3 { x: 0.0 - velocity.x, y: 0.0 - velocity.y, z: 0.0 - velocity.z };
    let speed = relative.length3();
    // Extended precision: `Cd * A * |v| * factor`, stored.
    let x87 = round_x87;
    let cd_area = x87(f64::from(mem.drag_coefficient(car)) * f64::from(mem.frontal_area(car)));
    let force_scale = x87(x87(cd_area * f64::from(speed)) * f64::from(factor)) as f32;
    let drag = calls.walk_vector(relative.scale(force_scale), car);
    let lever_torque = LEVER_ARM.cross3(drag);
    let spin = calls.walk_vector(mem.angular_velocity(car), car);
    let spin_speed = spin.length3();
    let damping = if spin_speed > SPIN_LIMIT {
        let excess = spin_speed - SPIN_LIMIT;
        let k = excess * excess * SPIN_GAIN / spin_speed;
        Vec3 { x: spin.x * k + lever_torque.x, y: spin.y * k + lever_torque.y, z: spin.z * k + lever_torque.z }
    } else {
        lever_torque
    };
    calls.drag_force(mem, car, drag);
    calls.spin_damping(mem, car, damping);
}
