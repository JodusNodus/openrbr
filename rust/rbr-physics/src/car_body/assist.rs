//! The car's outside-assistance announcers: `CCar::Test7910` (exe 0x4e7910, the lane test), `CCar::Assist8DF0` (0x4e8df0,
//! the outside-assistance worker) and `E34_RetireA` (0x4e8fa0, the retire announcer that calls it). The asm fixes the
//! order of the calls and the constants; the C++ in `src/rbr` (`RBR_E34_CarA.cpp`, `RBR_E34_CarC.cpp`, `RBR_E34_CarD.cpp`)
//! is approximate in places (its `Test7910` hands `ToWorld` the car's xy, the asm hands it `(0, -1, 0)`). The sequences
//! are in `layout.rs`; the calls are answered by the caller (host calls to the original, or ports).

use crate::math::vec::Vec3;

/// The calls `Assist8DF0` makes out of its own body. Each is answered by the original's code or by its port.
pub trait AssistCalls {
    /// `CCar::Test7910` (0x4e7910) on the car: nonzero when the lane verdict fails.
    fn lane_test(&mut self, car: u32) -> u32;
    /// `CMultiSignal1<CSigOutsideAssistance>::Emit` (0x4e89b0) on the outside-assistance signal, with the address of the code.
    fn emit_outside(&mut self, code_at: u32);
    /// `S158` (vtable slot 86) on the car.
    fn s158(&mut self, car: u32);
    /// `CCar::UpdateSeed` (0x4e91f0) on the car, with the seed block at `block`.
    fn update_seed(&mut self, car: u32, block: u32);
    /// `CBase::SetDirty` (0x52f450) on the filtered steering torque at `state` (the car's +0xae0).
    fn set_dirty(&mut self, state: u32);
}

/// The calls `E34_RetireA` makes: the retire emit (0x4e8770) with the address of its code, and `Assist8DF0` on the car.
pub trait RetireCalls {
    /// `CMultiSignal1<CSigRetireCar>::Emit` (0x4e8770) on the retire signal, with the address of the code.
    fn emit_retire(&mut self, code_at: u32);
    /// `CCar::Assist8DF0(a, b, c)` (0x4e8df0) on the car.
    fn assist(&mut self, car: u32, a: u32, b: u32, c: u32);
}

/// The lane verdict of `Test7910` (`d` is the car's position less the drive line's point at its lane). It is 1 when the
/// car is within 2 m of that point. Otherwise it is 1 when the dot of the world-frame `(0, -1, 0)` (`to_world`, the car's
/// `ToWorld`) and the unit `d` is at most 0.35, else 0. The comparisons are the asm's `jbe` after `comiss`/`fcompi`, so a NaN
/// takes the 1 side.
pub fn lane_verdict(d: Vec3, to_world: impl FnOnce(Vec3) -> Vec3) -> u32 {
    let len = d.length3();
    if !(len > 2.0) {
        return 1;
    }
    let inv = 1.0 / len;
    let n = Vec3 { x: d.x * inv, y: d.y * inv, z: d.z * inv };
    let out = to_world(Vec3 { x: 0.0, y: -1.0, z: 0.0 });
    let dot = out.z * n.z + out.y * n.y + out.x * n.x;
    u32::from(!(dot > 0.35))
}
