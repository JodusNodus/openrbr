//! The entity slot that dispatches to the entity's own slots, gated by the global flags. The memory it reads (the flags)
//! is in `layout`; the dispatch is here. The entity's small sequences over the store ([`EntityStore`]) are here too.

use super::hinge_detach;
use super::store::EntityStore;
use crate::math::vec::Vec4;

/// The calls the dispatch makes on the entity. The flags are read at each test, since the calls may change them. The callees
/// get the arguments `[a, c, e, f, g]` (`a` and `c` are the addresses of the caller's argument slots, which the callees may
/// write).
pub trait FlagDispatch {
    /// The flags object and its flag byte.
    fn flags(&mut self) -> (u32, u8);
    /// The first call (flag bit 1) on the entity; its result.
    fn first_call(&mut self, args: [u32; 5]) -> u32;
    /// The check (flag bit 2) on `e`; its result.
    fn check(&mut self, e: u32) -> u32;
    /// The second call (flag bit 2, when the check is nonzero) on the entity; its result.
    fn second_call(&mut self, args: [u32; 5]) -> u32;
}

/// The dispatch with the arguments `[a, c, e, f, g]`: flag bit 1 makes the first call, and its nonzero answer ends the call;
/// flag bit 2 makes the check on `e`, and a nonzero answer makes the second call. Returns what the original returns.
pub fn dispatch_by_flags(calls: &mut impl FlagDispatch, args: [u32; 5]) -> u32 {
    if calls.flags().1 & 1 != 0 {
        let eax = calls.first_call(args);
        if eax != 0 {
            return eax;
        }
    }
    let (flags, bits) = calls.flags();
    if bits & 2 == 0 {
        return flags;
    }
    if calls.check(args[2]) == 0 {
        return 0;
    }
    calls.second_call(args)
}

/// Initialises the entity's walker marks at `this`: both get the walker and the walker's
/// count plus one. Returns the second serial.
pub fn init_bd0<S: EntityStore>(store: &mut S, this: u32) -> u32 {
    let mut serial = 0;
    for mark in 0..2 {
        let walker = store.identity();
        store.store_walker(this, mark, walker);
        serial = hinge_detach::next_serial(store.walker_count(walker));
        store.store_serial(this, mark, serial);
    }
    serial
}

/// The calls that clear the car's bodies.
pub trait Clear480Calls {
    fn sim_fd00(&mut self, sp: u32, body: u32) -> u32;
}

/// Checks all four tags and clears a matching slot only when the body's settle check returns nonzero.
pub fn clear_car_bodies<S: EntityStore>(
    store: &mut S,
    car: u32,
    body: u32,
    frame: u32,
    calls: &mut impl Clear480Calls,
) {
    for i in 0..4 {
        if store.car_body_slot(car, i) == body && calls.sim_fd00(frame - 0x10, body) != 0 {
            store.store_car_body_slot(car, i, 0);
        }
    }
}

/// The componentwise product: `out[i] = this[i] * a[i]` over four floats (`Vec4::mul_lanes`).
/// Both inputs are read before `out` is written, so `out` may alias either one.
pub fn componentwise_mul<S: EntityStore>(store: &mut S, this: u32, a: u32, out: u32) {
    let [x, y, z, w] = store.vec4(this);
    let [ax, ay, az, aw] = store.vec4(a);
    let p = Vec4 { x, y, z, w }.mul_lanes(Vec4 { x: ax, y: ay, z: az, w: aw });
    store.store_vec4(out, [p.x, p.y, p.z, p.w]);
}
