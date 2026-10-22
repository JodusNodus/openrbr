//! The car's outside-assistance announcers: the lane test, the outside-assistance worker, and the retire announcer
//! that calls it. The order of the calls and the constants are the original's; the reconstructed C++ is approximate in
//! places (its lane test hands `ToWorld` the car's xy, the original hands it `(0, -1, 0)`). The call sequences are in
//! `layout`; the calls are answered by the caller (host calls to the original, or ports).

use crate::math::vec::Vec3;

/// The calls the outside-assistance worker makes out of its own body. Each is answered by the original's code or by
/// its port.
pub trait AssistCalls {
    /// The lane test on the car: nonzero when the lane verdict fails.
    fn lane_test(&mut self, car: u32) -> u32;
    /// The outside-assistance signal's emit, with the address of the code word.
    fn emit_outside(&mut self, code_at: u32);
    /// The car's reset (the solid's reset method).
    fn reset_car(&mut self, car: u32);
    /// The control update on the car, with the control block at `block`.
    fn apply_control(&mut self, car: u32, block: u32);
    /// Marks the filtered steering torque cell at `torque` changed.
    fn set_dirty(&mut self, torque: u32);
}

/// The calls the retire announcer makes: the retire emit with the address of its code, and the outside-assistance
/// worker on the car.
pub trait RetireCalls {
    /// The retire signal's emit, with the address of the code word.
    fn emit_retire(&mut self, code_at: u32);
    /// The outside-assistance worker on the car, with its three argument words.
    fn assist(&mut self, car: u32, a: u32, b: u32, c: u32);
}

/// The lane verdict (`d` is the car's position less the drive line's point at its lane). It is 1 when the car is
/// within 2 m of that point. Otherwise it is 1 when the dot of the world-frame `(0, -1, 0)` (`to_world`, the car's
/// transform to the world) and the unit `d` is at most 0.35, else 0. The comparisons take the 1 side for a NaN.
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
