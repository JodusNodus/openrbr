//! The car's body frame composed into the world frame: the body frame of `a` is composed into a local, then the car's
//! identity entity composes that local with the world frame and `b`. It returns `b`. The original halts when the world
//! frame global is zero (its assert); the port panics the same way. The order is the original's; the calls are
//! answered from the original's records.

use super::transform::{FrameCompose, FRAME};
use crate::car_body::store::CarStore;
use crate::mem::Mem;

/// The calls the placement makes out of its own body. The test answers them from the original's records.
pub trait PlacementCalls<S = Mem>: FrameCompose {
    /// The body frame of `a` on the car, written to `out` (28 bytes).
    fn body_compose(&mut self, mem: &mut S, car: u32, a: u32, out: u32);
    /// The car's identity getter: returns the car.
    fn identity(&mut self, car: u32) -> u32;
}

/// Composes the body frame of `a` into the world frame with `b`: `local` is the 28-byte local the body frame goes to (on
/// the original's stack).
pub fn compose_body_into_world<S: CarStore>(
    calls: &mut impl PlacementCalls<S>,
    mem: &mut S,
    car: u32,
    a: u32,
    b: u32,
    local: u32,
) -> u32 {
    calls.body_compose(mem, car, a, local);
    let frame = mem.u32(FRAME);
    assert_ne!(frame, 0, "compose_body_into_world: the world frame is unset (the original halts here)");
    let entity = calls.identity(car);
    calls.compose_frames(entity, local, frame, b);
    b
}
