//! The car's body transform (`CCar::Xform6870`, exe 0x4e6870). The asm (not the C++) fixes the call: `ComposeA(a, t, b)`
//! on the car, where `t` is the body (the identity S30 of the body, slot 12) and `b` is the output. The C++ passes `b`
//! as the first argument, which the record disproves (`harness/direct_calls.py`, the inner call of 0x51b4c0).
//!
//! The four per-step transforms (`ToWorld`, `WorldCompose`, `BodyToWorldPair`, `BodyCompose`) follow it: the S30 calls
//! are the identity getters (the car is its own `this`, the body is at +0x4a0), and each walks or composes against the
//! world frame (the global at `FRAME`) or the car. The walk (`E43PairWalk::MB560`) and `ComposeA` are externals the
//! caller answers (`PairWalk`, `ComposeA`).

use crate::car_body::store::CarStore;
pub use crate::entities::layout::ROOT_IDENTITY as FRAME;
use crate::math::vec::Vec3;

/// `E43PairWalk::MB560` (0x51b560, `ret 0xc`): walks `a` against `b` on `this` and returns the walked vector (the `c` the
/// original writes). Its walk is the entities port's (`entities::walk`); here it is an external the caller answers.
pub trait PairWalk {
    /// `MB560(this, a, b, c)`: returns `c`.
    fn pair_walk(&mut self, this: u32, a: Vec3, b: u32) -> Vec3;
}

/// `E33Mat34::ComposeA` (0x51b4c0, exe; `ret 0xc`): copies `a` to `out`, then walks the car's tree against `t`. Its
/// walk is the entities port's (`entities::walk`); here it is an external the caller answers.
pub trait ComposeA {
    /// `ComposeA(a, t, out)` on the car `this`. Returns `out`.
    fn compose_a(&mut self, this: u32, a: u32, t: u32, out: u32) -> u32;
}

/// `CCar::Xform6870(a, b)`: `b = ComposeA(a, body, b)` on the car, and returns `b`. The two S30 calls are the identity
/// getters: the body's, then the car's. The asm's null check on the body (its assert) is not ported: the port
/// needs a body.
pub fn xform_6870(compose: &mut impl ComposeA, car: u32, body: u32, a: u32, b: u32) -> u32 {
    compose.compose_a(car, a, body, b);
    b
}

/// The frame global, which the original asserts is set (`E01Log` and `int 3` when it is zero).
fn frame<S: CarStore>(mem: &S) -> u32 {
    let frame = mem.u32(FRAME);
    assert_ne!(frame, 0, "ToWorld: the frame global is unset (the original halts here)");
    frame
}

/// `CCar::ToWorld(a, b)` (0x4e68e0): the car-frame vector `a` walked against the world frame; returns the walked vector.
/// `this` of the walk is the S30 identity of the car, the car itself.
pub fn to_world<S: CarStore>(walk: &mut impl PairWalk, mem: &S, car: u32, a: Vec3) -> Vec3 {
    walk.pair_walk(car, a, frame(mem))
}

/// `CCar::BodyToWorldPair(a, b)` (0x4e69c0): `MB560(S30(body), a, S30(car), b)`; both S30 are identities. Returns the walk.
pub fn body_to_world_pair<S: CarStore>(walk: &mut impl PairWalk, mem: &S, car: u32, a: Vec3) -> Vec3 {
    let body = mem.body(car);
    assert_ne!(body, 0, "BodyToWorldPair: the body is unset (the original halts here)");
    walk.pair_walk(body, a, car)
}

/// `CCar::WorldCompose(a, b)` (0x4e6950, `ret 8`): `ComposeA(S30(car), a, frame, b)`. Returns `b`.
pub fn world_compose<S: CarStore>(compose: &mut impl ComposeA, mem: &S, car: u32, a: u32, b: u32) -> u32 {
    compose.compose_a(car, a, frame(mem), b);
    b
}

/// `CCar::BodyCompose(a, b)` (0x4e6a30, `ret 8`): `ComposeA(S30(body), a, S30(car), b)`. Returns `b`. The body is at +0x4a0.
pub fn body_compose<S: CarStore>(compose: &mut impl ComposeA, mem: &S, car: u32, a: u32, b: u32) -> u32 {
    let body = mem.body(car);
    assert_ne!(body, 0, "BodyCompose: the body is unset (the original halts here)");
    compose.compose_a(body, a, car, b);
    b
}
