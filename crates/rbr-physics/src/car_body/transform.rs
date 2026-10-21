//! The car's body transform. The original's call is `compose(a, t, b)` on the car, where `t` is the body (its identity
//! getter) and `b` is the output; the reconstructed C++ passes `b` as the first argument, which the record disproves
//! (`harness/direct_calls.py`).
//!
//! The four per-step transforms (`to_world`, `world_compose`, `body_to_world_pair`, `body_compose`) follow it: the
//! identity getters make the car its own `this` and the body the entity at the car's body slot, and each walks or
//! composes against the world frame (the global at `FRAME`) or the car. The vector walk and the compose are externals
//! the caller answers (`FrameWalk`, `FrameCompose`).

use crate::car_body::store::CarStore;
pub use crate::entities::layout::ROOT_IDENTITY as FRAME;
use crate::math::vec::Vec3;

/// Walks `a` against `b` on `this` and returns the walked vector. The walk is the entities port's (`entities::walk`);
/// here it is an external the caller answers.
pub trait FrameWalk {
    fn walk_vector(&mut self, this: u32, a: Vec3, b: u32) -> Vec3;
}

/// Composes the frame `a` with `t` into `out`: copies `a` to `out`, then walks the car's tree against `t`. The walk is
/// the entities port's (`entities::walk`); here it is an external the caller answers.
pub trait FrameCompose {
    /// `compose(a, t, out)` on the car `this`. Returns `out`.
    fn compose_frames(&mut self, this: u32, a: u32, t: u32, out: u32) -> u32;
}

/// `b = compose(a, body, b)` on the car, and returns `b`. The null check on the body (an assert in the original) is
/// not ported: the port needs a body.
pub fn compose_with_body(compose: &mut impl FrameCompose, car: u32, body: u32, a: u32, b: u32) -> u32 {
    compose.compose_frames(car, a, body, b);
    b
}

/// The world frame global, which the original asserts is set (it logs and traps when it is zero).
fn frame<S: CarStore>(mem: &S) -> u32 {
    let frame = mem.u32(FRAME);
    assert_ne!(frame, 0, "to_world: the world frame is unset (the original halts here)");
    frame
}

/// The car-frame vector `a` walked against the world frame; returns the walked vector. `this` of the walk is the car.
pub fn to_world<S: CarStore>(walk: &mut impl FrameWalk, mem: &S, car: u32, a: Vec3) -> Vec3 {
    walk.walk_vector(car, a, frame(mem))
}

/// Walks `a` from the body against the car. Returns the walked vector.
pub fn body_to_world_pair<S: CarStore>(walk: &mut impl FrameWalk, mem: &S, car: u32, a: Vec3) -> Vec3 {
    let body = mem.body(car);
    assert_ne!(body, 0, "body_to_world_pair: the body is unset (the original halts here)");
    walk.walk_vector(body, a, car)
}

/// Composes the car's frame `a` with the world frame into `b`. Returns `b`.
pub fn world_compose<S: CarStore>(compose: &mut impl FrameCompose, mem: &S, car: u32, a: u32, b: u32) -> u32 {
    compose.compose_frames(car, a, frame(mem), b);
    b
}

/// Composes `a` from the body's frame against the car into `b`. Returns `b`.
pub fn body_compose<S: CarStore>(compose: &mut impl FrameCompose, mem: &S, car: u32, a: u32, b: u32) -> u32 {
    let body = mem.body(car);
    assert_ne!(body, 0, "body_compose: the body is unset (the original halts here)");
    compose.compose_frames(body, a, car, b);
    b
}
