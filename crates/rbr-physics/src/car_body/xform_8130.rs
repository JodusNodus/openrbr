//! `CCar::Xform8130(a, b)` (exe 0x4e8130, `ret 8`): the car's body frame composed into the frame global. It calls
//! `BodyCompose(a, m1)` (0x4e6a30, into a 28-byte local `m1`), the identity getter S30 (slot 12) on the car, and
//! `ComposeA` on what S30 returns, with `m1`, the frame global (0x7cbb00) and `b` (the three words the asm pushes before
//! the S30 call are ComposeA's arguments). It returns `b`. The original halts when the frame global is zero (its assert
//! and `int 3`); the port panics the same way. The asm (not the C++) fixes the order; the calls are answered from the
//! original's records (`car_body/README.md` (git 9c28c0b8)).

use super::transform::{ComposeA, FRAME};
use crate::car_body::store::CarStore;
use crate::mem::Mem;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait Xform8130Calls<S = Mem>: ComposeA {
    /// `CCar::BodyCompose(a, out)` (0x4e6a30, `ret 8`) on the car: writes the body frame to `out`.
    fn body_compose(&mut self, mem: &mut S, car: u32, a: u32, out: u32);
    /// `S30` (slot 12, the identity getter 0x519c00) on the car: returns the car.
    fn s30(&mut self, car: u32) -> u32;
}

/// `CCar::Xform8130(car, a, b)`: `m1` is the 28-byte local the body frame goes to (on the original's stack).
pub fn xform_8130<S: CarStore>(
    calls: &mut impl Xform8130Calls<S>,
    mem: &mut S,
    car: u32,
    a: u32,
    b: u32,
    m1: u32,
) -> u32 {
    calls.body_compose(mem, car, a, m1);
    let frame = mem.u32(FRAME);
    assert_ne!(frame, 0, "Xform8130: the frame global is unset (the original halts here)");
    let u = calls.s30(car);
    calls.compose_a(u, m1, frame, b);
    b
}
