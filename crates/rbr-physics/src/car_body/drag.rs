//! `CCar::DragAndSpinDamp` (exe 0x4e7dc0): the drag force on the car's velocity and the spin damping of its angular
//! velocity. The asm fixes the order: the gate's pre-step (S00) first, then the velocity is read. The force goes to
//! the car's slot 28 (S70) and the damping to slot 29 (S74). Vectors are three lanes: the fourth lane, which S70 and S74
//! write into the car, is never read in the drive (`car_body/README.md` (git 9c28c0b8), open items).

use crate::car_body::store::CarStore;
use crate::fpu::round_x87;
use crate::math::vec::Vec3;
use crate::mem::Mem;
use crate::Variant;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait DragCalls<S = Mem> {
    /// `S00` (slot 0 of the car), called first when the time keeper's gate is set. It may change the car, so the port
    /// reads the velocity after it.
    fn pre_step(&mut self, mem: &mut S, car: u32);
    /// `E43PairWalk::MB560` on the world, with the car as the S30 identity: walks `a` and returns the result.
    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3;
    /// `S70` (slot 28) on the car: the drag force, added to the accumulator at +0x370.
    fn drag_force(&mut self, mem: &mut S, car: u32, force: Vec3);
    /// `S74` (slot 29) on the car: the spin damping, added to the accumulator at +0x310.
    fn spin_damping(&mut self, mem: &mut S, car: u32, damping: Vec3);
}

/// The fixed vector the drag's cross product uses, `(0, 0, 0.05)` (asm 0x72dd78 for the z lane). Its meaning is open.
const CROSS_AXIS: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.05 };

/// 1.02's drag factor, the `1/2` of `Cd * A * |v| / 2` (asm 0x795e18). NGP repoints it (at 0x4e7e62) at its
/// `1017147c`, `0.5 rho / 1.20407` from the air density, which its downforce update writes every step.
pub const DRAG_FACTOR: f32 = 0.5;

/// The angular speed above which the spin is damped harder (`5.0`, asm 0x72ae44).
const SPIN_LIMIT: f32 = 5.0;

/// The gain of the over-limit damping (`-5`, asm 0x73148c; the C++ `-5.0f`).
const SPIN_GAIN: f32 = -5.0;

/// `CCar::DragAndSpinDamp(this)`. The drag opposes the car's velocity, scaled by `Cd * A * |v| / 2`; the spin damping
/// is the cross product of the fixed axis with the drag, plus `k * spin` once the walked spin exceeds `SPIN_LIMIT`,
/// where `k = (|spin| - 5)^2 * -5 / |spin|`. The walks are the world's MB560 (`DragCalls::pair_walk`). `factor` is
/// the drag factor ([`DRAG_FACTOR`] in 1.02).
pub fn drag_and_spin_damp<S: CarStore>(
    calls: &mut impl DragCalls<S>,
    mem: &mut S,
    car: u32,
    factor: f32,
    variant: Variant,
) {
    if mem.time_keeper_gate() {
        calls.pre_step(mem, car);
    }
    // Negated lane by lane (`0.0 - v`, not `-v`: they differ on a zero lane).
    let velocity = mem.velocity(car);
    let relative = Vec3 { x: 0.0 - velocity.x, y: 0.0 - velocity.y, z: 0.0 - velocity.z };
    let speed = relative.length3();
    // x87: `Cd * A * |v| * factor`, stored.
    let x87 = |v: f64| round_x87(v, variant);
    let cd_area = x87(f64::from(mem.drag_coefficient(car)) * f64::from(mem.frontal_area(car)));
    let force_scale = x87(x87(cd_area * f64::from(speed)) * f64::from(factor)) as f32;
    let drag = calls.pair_walk(relative.scale(force_scale), car);
    let axis = CROSS_AXIS.cross3(drag);
    let spin = calls.pair_walk(mem.angular_velocity(car), car);
    let spin_speed = spin.length3();
    let damping = if spin_speed > SPIN_LIMIT {
        let excess = spin_speed - SPIN_LIMIT;
        let k = excess * excess * SPIN_GAIN / spin_speed;
        Vec3 { x: spin.x * k + axis.x, y: spin.y * k + axis.y, z: spin.z * k + axis.z }
    } else {
        axis
    };
    calls.drag_force(mem, car, drag);
    calls.spin_damping(mem, car, damping);
}
