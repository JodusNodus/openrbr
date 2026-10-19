//! `CCar::Vf64x` (exe 0x4e9020, no stack arguments): the per-step composite. It runs the drag (`DragAndSpinDamp`)
//! and the wheel aero gust (`Sim7AD0`), walks the angular velocity, crosses it with the walk scaled by the inverse of
//! the def's three words, and sends the cross product to S74. Then the vector at +0x15d0 is blended with the walk of
//! +0x380 (`0.7 * v`, then `+ 0.3 * walk`, in the asm's order), two externals run on the car, and when the keeper's
//! bit 0x10 is set and the word at +0x16ac is nonzero, 0x495090 gets it as `this` and two floats. Last, the four
//! objects at +0x490 each pass a test (0x521920) and then an action (0x521740). The asm fixes the order; the blend runs
//! over all four lanes (the walk's lane 3 is what the emulator's MB560 copies from its input).

use super::aero::{self, Sim7Calls};
use super::drag::{self, DragCalls};
use crate::car_body::store::CarStore;
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use crate::Variant;

/// The externals this function calls itself (the drag and the aero gust are `DragCalls` and `Sim7Calls`).
pub trait Vf64xCalls<S = Mem>: DragCalls<S> + Sim7Calls<S> {
    /// `0x5224f0` (a plain `ret`) on the car's object at +0x8b0.
    fn e5224f0(&mut self, obj: u32);
    /// `0x52cb20` on the car. It writes the vector at +0x380 (the walk's input), so it gets the memory.
    fn e52cb20(&mut self, mem: &mut S, car: u32);
    /// `E43PairWalk::MB560` on the car's vector at +0x380 (the asm passes its address, so the walk reads all four
    /// lanes of it), with the car as `b`. All four lanes of the output: lane 3 is the blend's fourth lane.
    fn walk_380(&mut self, car: u32) -> Vec4;
    /// `0x495090` (`ret 8`): the flag word at +0x16ac is its `this` (the asm leaves it in ecx), then the two floats:
    /// the first `v[0] * k`, the second `aux_scalar * k`.
    fn e495090(&mut self, obj: u32, first: f32, second: f32);
    /// `0x521920` on an object of the car's list: nonzero when its test holds.
    fn e521920(&mut self, obj: u32) -> u32;
    /// `0x521740` on an object of the car's list.
    fn e521740(&mut self, obj: u32);
}

/// The blend's decay of the vector at +0x15d0 (`0.7`, asm 0x72f674).
const DECAY: f32 = f32::from_bits(0x3F33_3333);

/// The walk's share in the blend (`0.3`, asm 0x72afcc).
const BLEND_SHARE: f32 = f32::from_bits(0x3E99_999A);

/// The scale of the two floats sent to 0x495090 (asm 0x738ab0, `-0.10193679`).
const SEND_SCALE: f32 = f32::from_bits(0xBDD0_C43D);

/// The keeper's bit that gates the 0x495090 call (tested in the keeper's word at +0x160e0).
const KEEPER_BIT: u32 = 0x10;

/// `CCar::Vf64x(this)` (name ours). `drag_factor` and `variant` are the drag's (`drag::drag_and_spin_damp`).
pub fn step_aero<S: CarStore>(
    calls: &mut impl Vf64xCalls<S>,
    mem: &mut S,
    car: u32,
    drag_factor: f32,
    variant: Variant,
) {
    drag::drag_and_spin_damp(calls, mem, car, drag_factor, variant);
    aero::sim7ad0(calls, mem, car);
    let walked = DragCalls::pair_walk(calls, mem.angular_velocity(car), car);
    let inertia = mem.def_inertia(car);
    let weighted =
        Vec3 { x: (-1.0 / inertia.x) * walked.x, y: (-1.0 / inertia.y) * walked.y, z: (-1.0 / inertia.z) * walked.z };
    calls.spin_damping(mem, car, walked.cross3(weighted));
    calls.e5224f0(car + 0x8b0);
    calls.e52cb20(mem, car);
    let walk = calls.walk_380(car);
    let decayed = mem.aux_vector(car).scale(DECAY);
    let blend = Vec4 {
        x: walk.x * BLEND_SHARE + decayed.x,
        y: walk.y * BLEND_SHARE + decayed.y,
        z: walk.z * BLEND_SHARE + decayed.z,
        w: walk.w * BLEND_SHARE + decayed.w,
    };
    mem.set_aux_vector(car, blend);
    let flag = mem.aux_flag(car);
    if mem.keeper_flags() & KEEPER_BIT != 0 && flag != 0 {
        calls.e495090(flag, blend.x * SEND_SCALE, mem.aux_scalar(car) * SEND_SCALE);
    }
    for i in 0..4 {
        let obj = mem.car_object(car, i);
        if obj != 0 && calls.e521920(obj) != 0 {
            calls.e521740(obj);
        }
    }
}
