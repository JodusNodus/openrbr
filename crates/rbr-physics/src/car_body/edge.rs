//! The car's grid edge. Pushing composes the body frame of the zero vector (the car's frame) and adds it to the grid
//! object once, then sets the pushed flag. Refreshing pushes the edge and then runs the entity's refresh on the car with
//! its argument, returning that call's result. The original fixes the order and the early returns.

use crate::math::vec::Vec4;

/// The calls the edge makes out of its own body. The test answers them from the original's records.
pub trait EdgeCalls {
    /// The body frame of `a` on the car, as the four lanes of the result.
    fn body_compose(&mut self, car: u32, a: Vec4) -> Vec4;
    /// Adds the edge to the grid object.
    fn add_edge(&mut self, grid: u32, edge: Vec4);
    /// The entity's refresh with `a` on the car: returns its result word.
    fn entity_refresh(&mut self, car: u32, a: u32) -> u32;
}

/// Returns at once when the edge is pushed or the grid is null. Otherwise the zero vector goes through the body
/// compose, the edge is added to the grid, and the flag becomes 1. Returns the flag.
pub fn push_grid_edge(calls: &mut impl EdgeCalls, car: u32, pushed: bool, grid: u32) -> bool {
    if pushed || grid == 0 {
        return pushed;
    }
    let edge = calls.body_compose(car, Vec4::default());
    calls.add_edge(grid, edge);
    true
}

/// Pushes the edge, then refreshes the entity with `a`. Returns the new flag and the refresh's result word.
pub fn refresh_edge(calls: &mut impl EdgeCalls, car: u32, pushed: bool, grid: u32, a: u32) -> (bool, u32) {
    let pushed = push_grid_edge(calls, car, pushed, grid);
    (pushed, calls.entity_refresh(car, a))
}
