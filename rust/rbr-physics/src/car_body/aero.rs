//! `CCar::Sim7AD0` (exe 0x4e7ad0): the wheel aero gust. While the brake-wall penetration is above 0, the car's seed
//! (the vector at +0x370, moved to the world by `ToWorld`) is blended with the wind. If the blend's z is not below 0 and
//! the speed of the horizontal velocity is above `MIN_SPEED`, two vectors are walked by MB560 and sent to S70 and S74
//! (slots 28 and 29). The asm fixes the order (`car_body/README.md` (git 9c28c0b8), open items for the C++ difference).

use crate::car_body::store::CarStore;
use crate::math::vec::Vec3;
use crate::mem::Mem;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait Sim7Calls<S = Mem> {
    /// `CCar::ToWorld` (0x4e68e0) on the car: the car-frame vector `v` in the world frame.
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3;
    /// `E43PairWalk::MB560` on the world, with the car as the S30 identity (as `DragCalls::pair_walk`).
    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3;
    /// `S70` (slot 28) on the car: the force along the speed, added to the accumulator at +0x370.
    fn aero_force(&mut self, mem: &mut S, car: u32, force: Vec3);
    /// `S74` (slot 29) on the car: the spin from the gust's cross product, added to the accumulator at +0x310.
    fn aero_spin(&mut self, mem: &mut S, car: u32, spin: Vec3);
}

/// The weight of the wall penetration, `3 * penetration` (asm 0x72b2a0).
const WALL_GAIN: f32 = 3.0;

/// The weight of the speed in the lift term, `2 * speed` (asm 0x72ad94).
const SPEED_GAIN: f32 = 2.0;

/// The speed's step in the drag term, `0.01 * speed` (asm 0x72f440).
const SPEED_STEP: f32 = 0.01;

/// The share of the drag term (asm 0x795e18).
const DRAG_SHARE: f32 = 0.5;

/// The share of the lift term (asm 0x72afcc).
const LIFT_SHARE: f32 = 0.3;

/// The spin's gain on the angular velocity's z lane (asm 0x72f57c).
const SPIN_GAIN: f32 = 2.5;

/// The wind's vector, `(0, 0, -0.3)` (asm 0x72f668 for the z lane).
const GUST: Vec3 = Vec3 { x: 0.0, y: 0.0, z: -0.3 };

/// The speed at or below which the gust does nothing (asm 0x795e94).
const MIN_SPEED: f32 = 0.001;

/// Clamps `x` to `[lo, hi]` with the asm's compare-and-select: a NaN stays.
pub(super) fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if lo > x {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// `CCar::Sim7AD0(this)`. The wind's term uses `-1 / inverse_mass` (asm 0x795e20 over `def + 0x16c`).
pub fn sim7ad0<S: CarStore>(calls: &mut impl Sim7Calls<S>, mem: &mut S, car: u32) {
    let wall = mem.wall_penetration(car);
    if !(wall > 0.0) {
        return;
    }
    // The wind is added in place to the seed, in the world frame (the asm's `s * wind + seed`).
    let wind_scale = -1.0 / mem.inverse_mass(car);
    let wind = mem.wind();
    let seed = calls.to_world(car, mem.accumulator(car));
    let blend =
        Vec3 { x: wind.x * wind_scale + seed.x, y: wind.y * wind_scale + seed.y, z: wind.z * wind_scale + seed.z };
    let bz = blend.z;
    if 0.0 > bz {
        return;
    }
    // The probe is the velocity with its z lane forced to 0; the gust needs its length above `MIN_SPEED`.
    let velocity = mem.velocity(car);
    let speed = Vec3 { x: velocity.x, y: velocity.y, z: 0.0 }.length3();
    if !(speed > MIN_SPEED) {
        return;
    }
    let unit = velocity.scale(1.0 / speed);
    let wall_w = clamp(wall * WALL_GAIN, 0.0, 1.0);
    let lift_w = clamp(speed * SPEED_GAIN, 0.0, 1.0);
    // The asm scales the speed here (its xmm0 still holds it); the C++ scales the lift weight `a0` instead.
    let drag_w = clamp(speed * SPEED_STEP, 0.0, 1.0);
    let dir = unit.scale(0.0 - (drag_w * DRAG_SHARE + lift_w * LIFT_SHARE) * wall_w * bz);
    let spin_w = clamp(mem.angular_velocity(car).z * SPIN_GAIN, -1.0, 1.0);
    let mut xout = GUST.cross3(dir);
    xout.z += spin_w * lift_w * wall_w * bz * GUST.z;
    let force = calls.pair_walk(dir, car);
    let spin = calls.pair_walk(xout, car);
    calls.aero_force(mem, car, force);
    calls.aero_spin(mem, car, spin);
}
