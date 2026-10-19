//! `CCar::Vf28x` (exe 0x4ea020, `ret 0`): the car's sensor and signal composite. The asm fixes the order: the drive,
//! passenger, engine and light calls, the body loop, the car's vectors copied into the frame (the steering step, the
//! frame's Xform8130, the wheel loop and the four signal emits), the sensor sampler and its emit, the clamped value's
//! emit, the walk and the direction into the frame, the emit at +0x860, the keeper branch, the damage object's slot 5
//! and the engine's `E9A0`. `frame` is the base of the function's esp-relative locals: each pointer the original
//! passes is `frame + offset`. The externals are answered by `Vf28xCalls`.

use super::layout;
use crate::car_body::store::CarStore;
use crate::entities::layout::ROOT_IDENTITY;
use crate::mem::Mem;
use crate::physics_core::layout::SIGNAL_MANAGER;

/// The calls `Vf28x` makes out of its own body. The test answers them from the original's records.
pub trait Vf28xCalls<S = Mem> {
    /// A call on `this` (ecx) with its stack arguments, returning eax. The virtual targets go through here too.
    fn call(&mut self, mem: &mut S, callee: u32, this: u32, args: &[u32]) -> u32;
    /// One of the four signal emits, on the signal object `signal` with the payload pointer `payload`.
    fn emit(&mut self, mem: &mut S, callee: u32, signal: u32, payload: u32);
}

/// The constant passed as `CSolid::Step77`'s second argument (`5.0`, asm 0x72ae44; meaning unknown).
const UNKNOWN_STEP77_ARG: f32 = 5.0;
/// The damage object's virtual slot that `Vf28x` calls (`+0x14`).
const DAMAGE_SLOT: u32 = 5;

/// `0x508480`: called on the drive at car+0x4b0 (meaning unknown).
const UNKNOWN_DRIVE: u32 = 0x508480;
/// `0x523210`: called on the passenger at car+0x8b0 (meaning unknown).
const UNKNOWN_PASSENGER: u32 = 0x523210;
/// `0x518cf0`: called on the engine at car+0x690 (meaning unknown).
const UNKNOWN_ENGINE: u32 = 0x518CF0;
/// `0x4f2150`: called on car+0x838, the light system's block (meaning unknown).
const UNKNOWN_LIGHTS: u32 = 0x4F2150;
/// `0x51a3e0`: called on the car itself (meaning unknown).
const UNKNOWN_CAR_VF28: u32 = 0x51A3E0;
/// `0x521920`: the body test on each rigid body of car+0x490; true when the body's word at +0x580 is 1.
const UNKNOWN_BODY_TEST: u32 = 0x521920;
/// `0x522000`: the body's step, when its test holds (meaning unknown).
const UNKNOWN_BODY_STEP: u32 = 0x522000;
/// `0x43c5b0` (E01Vec4::CopyFrom): copies 16 bytes from the source into `this`.
pub(crate) const VEC4_COPY_FROM: u32 = 0x43C5B0;
/// `0x465d80` (E09Vec4::Clear): zeroes the 16-byte vector at `this`.
pub(crate) const VEC4_CLEAR: u32 = 0x465D80;
/// `0x529d30` (CSolid::Step77), on the car with the frame's two vectors.
const SOLID_STEP77: u32 = 0x529D30;
/// `0x4e8130` (CCar::Xform8130, `xform_8130.rs`): the car's transform of the frame's two vectors.
const XFORM_8130: u32 = 0x4E8130;
/// `0x5390d0`: called on each wheel at car+0x470 with two vectors of the frame (meaning unknown).
const UNKNOWN_WHEEL_STEP: u32 = 0x5390D0;
/// `0x4e83f0`: emit on the signal at +0x2e10, with the frame's payload at +0xa0 (meaning unknown).
const UNKNOWN_SIGNAL_2E10: u32 = 0x4E83F0;
/// `0x4e8330`: emit on the signal at +0x29e0, with the same payload (meaning unknown).
const UNKNOWN_SIGNAL_29E0: u32 = 0x4E8330;
/// `0x4e84b0`: emit on the signal at +0x51a8, with the car's float at +0x17c as its payload (meaning unknown).
const UNKNOWN_SIGNAL_51A8: u32 = 0x4E84B0;
/// `0x4e81b0`: emit on the signal at +0x860, with the clamped value as its payload (meaning unknown).
const UNKNOWN_SIGNAL_860: u32 = 0x4E81B0;
/// `0x4e9800` (CCar::Sample9800, `sample9800.rs`).
const SAMPLE_9800: u32 = 0x4E9800;
/// `0x51b560` (E43PairWalk::MB560, as in `vf6cx.rs`): walks the car's vector into the frame.
const PAIR_WALK: u32 = 0x51B560;
/// `0x4e5cf0` (CCar::GetDir, meaning as recorded): writes a vector into the frame.
const GET_DIR: u32 = 0x4E5CF0;
/// `0x50e9a0`: called on the engine at car+0x690, last (meaning unknown).
const UNKNOWN_ENGINE_STEP: u32 = 0x50E9A0;
/// `0x51b4c0` (E33Mat34::ComposeA, `transform.rs`).
const COMPOSE_A: u32 = 0x51B4C0;

/// The virtual target of `obj`'s `slot`: the vtable's word at `4 * slot`.
fn virtual_target<S: CarStore>(mem: &S, obj: u32, slot: u32) -> u32 {
    mem.u32(mem.u32(obj) + 4 * slot)
}

/// `CCar::Vf28x(this)` (name ours).
pub fn update_sensors<S: CarStore>(calls: &mut impl Vf28xCalls<S>, mem: &mut S, car: u32, frame: u32) {
    let at = |offset: u32| frame + offset;
    calls.call(mem, UNKNOWN_DRIVE, car + 0x4B0, &[]);
    calls.call(mem, UNKNOWN_PASSENGER, car + 0x8B0, &[]);
    calls.call(mem, UNKNOWN_ENGINE, car + 0x690, &[]);
    calls.call(mem, UNKNOWN_LIGHTS, car + 0x838, &[]);
    calls.call(mem, UNKNOWN_CAR_VF28, car, &[]);
    for i in 0..4 {
        let body = mem.u32(layout::body_slot(car, i));
        if body != 0 && calls.call(mem, UNKNOWN_BODY_TEST, body, &[]) != 0 {
            calls.call(mem, UNKNOWN_BODY_STEP, body, &[]);
        }
    }

    if mem.u32(car + 0x30) != 0 {
        calls.call(mem, VEC4_COPY_FROM, at(0xB0), &[car + 0x140]);
        // The 16 bytes at car+0x100 go to frame+0xc0 in the original; no call reads them, so they are not copied.
        calls.call(mem, VEC4_COPY_FROM, at(0xD0), &[car + layout::VELOCITY]);
        calls.call(mem, VEC4_COPY_FROM, at(0xE0), &[car + layout::ANGULAR_VELOCITY]);
        mem.set_u32(at(0xA0), mem.u32(car + 0x30));
        mem.set_f32(at(0xF4), UNKNOWN_STEP77_ARG);
        calls.call(mem, SOLID_STEP77, car, &[at(0xF0), at(0xF4)]);
        calls.call(mem, VEC4_CLEAR, at(0x20), &[]);
        calls.call(mem, XFORM_8130, car, &[at(0x20), at(0xB0)]);
        for i in 0..4 {
            let wheel = mem.u32(car + 0x470 + 4 * i);
            let (a, b) = (at(0x100 + 0x10 * i), at(0x140 + 0x10 * i));
            calls.call(mem, UNKNOWN_WHEEL_STEP, wheel, &[a, b]);
        }
        let signal = mem.u32(SIGNAL_MANAGER);
        calls.emit(mem, UNKNOWN_SIGNAL_2E10, signal + 0x2E10, at(0xA0));
        calls.emit(mem, UNKNOWN_SIGNAL_29E0, signal + 0x29E0, at(0xA0));
    }

    calls.call(mem, SAMPLE_9800, car, &[]);
    mem.set_u32(at(0x18), mem.u32(car + 0x17C));
    let signal = mem.u32(SIGNAL_MANAGER);
    calls.emit(mem, UNKNOWN_SIGNAL_51A8, signal + 0x51A8, at(0x18));
    // `max(x, 0)` as the asm's `comiss` takes it: a NaN keeps x.
    let clamped = mem.f32(car + 0x15F8);
    mem.set_f32(at(0x50), if 0.0 > clamped { 0.0 } else { clamped });
    let frame_global = mem.u32(ROOT_IDENTITY);
    calls.call(mem, PAIR_WALK, frame_global, &[car + 0x220, car, at(0x30)]);
    calls.call(mem, VEC4_COPY_FROM, at(0x60), &[at(0x30)]);
    calls.call(mem, GET_DIR, car, &[at(0x40)]);
    calls.call(mem, VEC4_COPY_FROM, at(0x70), &[at(0x40)]);
    let signal = mem.u32(SIGNAL_MANAGER);
    calls.emit(mem, UNKNOWN_SIGNAL_860, signal + 0x860, at(0x50));

    // The keeper branch. The drive's and the car's slot-12 getters are the identity (they return `this`), and they leave
    // their pushed arguments on the stack for the ComposeA calls that pop them.
    if mem.keeper_flags() & 8 != 0 {
        calls.call(mem, VEC4_CLEAR, at(0x20), &[]);
        let drive = mem.u32(car + 0x4A0);
        let drive_getter = virtual_target(mem, drive, 12);
        let r1 = calls.call(mem, drive_getter, drive, &[at(0x90)]);
        let car_getter = virtual_target(mem, car, 12);
        let r2 = calls.call(mem, car_getter, car, &[at(0x20), r1]);
        calls.call(mem, COMPOSE_A, r2, &[at(0x20), r1, at(0x90)]);
        let frame_global = mem.u32(ROOT_IDENTITY);
        let r3 = calls.call(mem, car_getter, car, &[at(0x20), frame_global, at(0x80)]);
        calls.call(mem, COMPOSE_A, r3, &[at(0x20), frame_global, at(0x80)]);
    }

    let damage = mem.u32(car + 0x3CC);
    let damage_target = virtual_target(mem, damage, DAMAGE_SLOT);
    calls.call(mem, damage_target, damage, &[]);
    calls.call(mem, UNKNOWN_ENGINE_STEP, car + 0x690, &[]);
}
