//! `CCar::Vf24x` (exe 0x4e58e0): the slot-9 forward to the drive member at +0x4b0. The asm adds 0x4b0 to `this` and
//! tail-jumps to 0x503860 (E39), so the drive's function is the callee and the car passes no arguments.

use super::layout;

/// The drive's function the forward jumps to (0x503860, E39): its `this` is the drive member.
pub trait Vf24xCalls {
    /// `CDrive` function at 0x503860 with `this` = the drive member (not ported: the drive's own code).
    fn drive(&mut self, drive: u32);
}

/// `CCar::Vf24x(car)`: forwards to the drive member at `car + 0x4b0`.
pub fn vf24x(calls: &mut impl Vf24xCalls, car: u32) {
    calls.drive(layout::drive_at(car));
}
