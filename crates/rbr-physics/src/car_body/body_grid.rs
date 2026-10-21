//! Resetting a body's grid: the state's flag is set, the owner's grid is cleared over the bounds, then the two bound
//! vectors are copied into the state. The order is the original's. Lane 3 of the two vectors is not modelled: the
//! original copies it from its uninitialised frame.

use crate::math::vec::Vec3;

/// The grid's bound, m: lanes 0 to 2 of the two vectors are `+BOUND` (plus) and `-BOUND` (minus).
pub const BOUND: f32 = 999999.875;

/// The calls the reset makes. The owner returns its grid, which is cleared over the two vectors' addresses; a vector
/// copy writes 16 bytes from `src` to `dst` and returns `dst`.
pub trait GridCalls {
    fn grid(&mut self, owner: u32) -> u32;
    fn clear_grid(&mut self, owner: u32, grid: u32, plus: u32, minus: u32);
    fn copy_from(&mut self, dst: u32, src: u32) -> u32;
}

/// The two vectors the grid is cleared over: `(plus, minus)`.
pub fn grid_bounds() -> (Vec3, Vec3) {
    (Vec3 { x: BOUND, y: BOUND, z: BOUND }, Vec3 { x: -BOUND, y: -BOUND, z: -BOUND })
}
