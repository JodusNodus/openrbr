//! `CCarBody::CState::Vf44x` (exe 0x4eb6a0, slot 17, `ret 0`): sets the state's flag at +0x1c, clears the owner's grid
//! over the bounds, then copies the two bound vectors to +0x20 (plus) and +0x30 (minus). The asm (not the C++) decides
//! the order; the C++ in `RBR_E35_BodyA.cpp` agrees. Lane 3 of the two vectors is not modelled: the original copies it
//! from its uninitialised frame.

use crate::math::vec::Vec3;

/// The grid's bound: lanes 0 to 2 of the two vectors are `+BOUND` (plus) and `-BOUND` (minus).
pub const BOUND: f32 = 999999.875;

/// The calls the state makes. The owner's slot 12 is thiscall with no arguments and returns the grid. `CBody::ClearGrid`
/// is thiscall on the owner with the grid and the two vectors' addresses. `E01Vec4::CopyFrom` copies 16 bytes from `src`
/// to `dst` and returns `dst`.
pub trait StateCalls {
    fn grid(&mut self, owner: u32) -> u32;
    fn clear_grid(&mut self, owner: u32, grid: u32, plus: u32, minus: u32);
    fn copy_from(&mut self, dst: u32, src: u32) -> u32;
}

/// The two vectors the grid is cleared over: `(plus, minus)`.
pub fn bounds() -> (Vec3, Vec3) {
    (Vec3 { x: BOUND, y: BOUND, z: BOUND }, Vec3 { x: -BOUND, y: -BOUND, z: -BOUND })
}
