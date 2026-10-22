//! The car's forward to its drive: the original adds the drive member's offset to the car and tail-jumps to the drive's
//! own function, so the drive is the callee and the car passes no arguments.

use super::layout;

/// The drive's function the forward jumps to: its `this` is the drive member (not ported: the drive's own code).
pub trait DriveCalls {
    fn drive(&mut self, drive: u32);
}

/// Forwards to the drive member of `car`.
pub fn forward_to_drive(calls: &mut impl DriveCalls, car: u32) {
    calls.drive(layout::drive_at(car));
}
