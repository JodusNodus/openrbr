//! `CCar::UpdateMrr` (exe 0x4e9c70, `ret 0`): the car's wiring to its damage object, which it keeps at +0x9f4. The
//! damage object's vtable slot 4 (S10) runs first, then setters on the drive and the light system (their argument is
//! an address inside the damage object), then the three lists of rigid bodies at +0x470, +0x480 and +0x490 (each
//! entry: a non-null object gets its two calls; the first list's entries get a walk and a setter, see the asm). Then
//! UpdateSeed runs on a local block (words 0 to 5 and 7 zeroed; word 6 is left as the stack has it), and the
//! emit 0x4e8cb0 takes the pair (the car, the word at +0x30). The asm (not the C++) fixes the order; the calls are
//! answered from the original's records (`README.md`).

use super::layout::DAMAGE_HANDLER;
use crate::car_body::store::CarStore;
use crate::physics_core::layout::SIGNAL_MANAGER;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait UpdateMrrCalls {
    /// A call out of the body: `callee` on `this`, with the words `args` (the callee's arguments, first one first). The
    /// callee's return is not used.
    fn call(&mut self, callee: u32, this: u32, args: &[u32]);
    /// `0x4e8cb0` on the signal object `obj` with the pair (the car, the word at car+0x30), passed by pointer.
    fn emit(&mut self, obj: u32, pair: [u32; 2]);
}

/// The emit's offset in the signal manager (`SIGNAL_MANAGER`, 0x893600).
const EMIT_OFFSET: u32 = 0xE868;

/// UpdateSeed (0x4e91f0), the car's seed step (see `update_seed.rs`).
const UPDATE_SEED: u32 = 0x4E91F0;
/// `0x503ab0`: stores its argument at the drive's +0x170 (meaning unknown).
const UNKNOWN_DRIVE_STORE_170: u32 = 0x503AB0;
/// `0x50d310`: stores its argument at the +0x48 of the object at car+0x690 (meaning unknown).
const UNKNOWN_STORE_48: u32 = 0x50D310;
/// `0x4f1e00` (CCarLightSystem::Attach, on car+0x838): stores its argument at +0x14.
const LIGHT_SYSTEM_ATTACH: u32 = 0x4F1E00;
/// `0x53a860`: the first list's walk, on an entry of the car+0x470 list, with three vectors of the damage object.
const UNKNOWN_RIGID_WALK: u32 = 0x53A860;
/// `0x53d030`: the call after the walk on the same entry (meaning unknown).
const UNKNOWN_RIGID_AFTER_WALK: u32 = 0x53D030;
/// `0x51da50`: stores its argument at the +0x650 of an entry of the car+0x480 list (meaning unknown).
const UNKNOWN_STORE_650: u32 = 0x51DA50;
/// `0x51efe0`: `CHingedRigidBody::SimEFE0`, the hinged body's frame setup (`entities/hinge_efe0.rs`).
const HINGE_FRAME_SETUP: u32 = 0x51EFE0;
/// `0x521910` (E45Smalls::M1910): stores its argument at +0x580 of an entry of the car+0x490 list (meaning unknown).
const UNKNOWN_MOUNTED_STORE_580: u32 = 0x521910;
/// `0x521980` (E45_M1980): the call after `UNKNOWN_MOUNTED_STORE_580` on the same entry (meaning unknown).
const UNKNOWN_MOUNTED_AFTER_580: u32 = 0x521980;
/// `0x503aa0`: a drive hook that returns at once (meaning unknown).
const UNKNOWN_DRIVE_HOOK: u32 = 0x503AA0;

/// `CCar::UpdateMrr(car)`. `block` is the local block UpdateSeed takes as its `p` (0x20 bytes on the original's stack).
pub fn update_mrr<S: CarStore>(calls: &mut impl UpdateMrrCalls, mem: &mut S, car: u32, block: u32) {
    let damage = mem.u32(car + 0x3CC);
    mem.set_u32(car + DAMAGE_HANDLER, damage);
    // S10 is slot 4 of the damage object's vtable.
    calls.call(mem.u32(mem.u32(damage) + 0x10), damage, &[]);
    calls.call(UNKNOWN_DRIVE_STORE_170, car + 0x4B0, &[damage + 0x3A18]);
    calls.call(UNKNOWN_STORE_48, car + 0x690, &[damage + 0x3AAC]);
    calls.call(LIGHT_SYSTEM_ATTACH, car + 0x838, &[damage + 0x3B24]);
    for i in 0..4 {
        let slot = car + 0x470 + 4 * i;
        let walk = [damage + 0x3C04 + 0x38 * i, damage + 0x3CE4 + 8 * i, damage + 0x3D04 + 0x20 * i];
        calls.call(UNKNOWN_RIGID_WALK, mem.u32(slot), &walk);
        calls.call(UNKNOWN_RIGID_AFTER_WALK, mem.u32(slot), &[]);
    }
    for i in 0..4 {
        let slot = car + 0x480 + 4 * i;
        if mem.u32(slot) != 0 {
            calls.call(UNKNOWN_STORE_650, mem.u32(slot), &[damage + 0x3D84 + 0xC * i]);
            calls.call(HINGE_FRAME_SETUP, mem.u32(slot), &[]);
        }
    }
    for i in 0..4 {
        let slot = car + 0x490 + 4 * i;
        if mem.u32(slot) != 0 {
            calls.call(UNKNOWN_MOUNTED_STORE_580, mem.u32(slot), &[damage + 0x3DB4 + 4 * i]);
            calls.call(UNKNOWN_MOUNTED_AFTER_580, mem.u32(slot), &[]);
        }
    }
    calls.call(UNKNOWN_DRIVE_HOOK, car + 0x4B0, &[]);
    for word in [0, 1, 2, 3, 4, 5, 7] {
        mem.set_u32(block + 4 * word, 0);
    }
    calls.call(UPDATE_SEED, car, &[block]);
    calls.emit(mem.u32(SIGNAL_MANAGER) + EMIT_OFFSET, [car, mem.u32(car + 0x30)]);
}
