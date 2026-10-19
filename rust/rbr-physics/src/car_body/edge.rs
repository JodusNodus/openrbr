//! The car's grid edge: `CCar::PushGridEdge` (exe 0x4e7270) and `CCar::RefreshEdge` (0x4e72d0). PushGridEdge composes
//! the body frame of the zero vector (`BodyCompose`, the car's frame) and adds it to the grid object (`E37GridOps::AddEdge`,
//! at +0x64) once, then sets the flag at +0x16a8. RefreshEdge pushes the edge and calls `E43Vf20` (0x529200) on the car
//! with its argument, returning that call's eax. The asm (not the C++) fixes the order and the early returns.

use crate::math::vec::Vec4;

/// The calls the edge makes out of its own body. The test answers them from the original's records.
pub trait EdgeCalls {
    /// `CCar::BodyCompose(a, out)` (0x4e6a30) on the car: the body frame of `a`, as the four lanes of `out`.
    fn body_compose(&mut self, car: u32, a: Vec4) -> Vec4;
    /// `E37GridOps::AddEdge(edge)` (0x4f65b0) on the grid object.
    fn add_edge(&mut self, grid: u32, edge: Vec4);
    /// `CEntity::E43Vf20(a)` (0x529200) on the car: returns eax.
    fn vf20(&mut self, car: u32, a: u32) -> u32;
}

/// `CCar::PushGridEdge` (0x4e7270): returns at once when the flag is set or the grid is null. Otherwise the zero
/// vector goes through BodyCompose, the edge is added to the grid, and the flag becomes 1. Returns the flag.
pub fn push_grid_edge(calls: &mut impl EdgeCalls, car: u32, pushed: bool, grid: u32) -> bool {
    if pushed || grid == 0 {
        return pushed;
    }
    let edge = calls.body_compose(car, Vec4::default());
    calls.add_edge(grid, edge);
    true
}

/// `CCar::RefreshEdge(a)` (0x4e72d0): `PushGridEdge`, then `E43Vf20(a)` on the car. Returns the new flag and the eax of
/// the Vf20 call (the function's return).
pub fn refresh_edge(calls: &mut impl EdgeCalls, car: u32, pushed: bool, grid: u32, a: u32) -> (bool, u32) {
    let pushed = push_grid_edge(calls, car, pushed, grid);
    (pushed, calls.vf20(car, a))
}
