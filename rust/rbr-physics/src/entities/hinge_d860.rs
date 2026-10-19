//! `CHingedRigidBody::SimD860` (0x51d860): the car's walk over the definition's matrix applied to the rotation row at
//! +0x5a4, then the hinged body's vector at +0x5c0 plus the walk's output. The walk (`E43PairWalk::MB560`) is the caller's
//! call; the port returns the vector that `SimD860` writes to its `out`.
//!
//! The row's sums keep the asm's order (SSE single precision; the order changes the bits): column `k` is
//! `(m[1][k]*b[1] + m[2][k]*b[2]) + m[0][k]*b[0]`, the same for all three columns. RBR_E44_SimA.cpp:156 writes the same sums
//! in the same order.

use std::array::from_fn;

/// The inputs read from the hinged body and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D860Input {
    /// The walk's `b`: the identity global's value.
    pub identity: u32,
    /// The owner car (+0x3d0): the walk's receiver.
    pub car: u32,
    /// The rotation block's second row (+0x5a4): the vector the definition's matrix is applied to.
    pub rotation_row: [f32; 3],
    /// The definition's 3x3 at +0x3e0.
    pub definition_matrix: [[f32; 3]; 3],
    /// The hinged body's vector at +0x5c0 (the meaning is unknown): the offset of the walk's output.
    pub offset: [f32; 3],
}

/// What `SimD860` needs from the game: the walk, with its input vector, and its output.
pub trait D860Calls {
    /// `E43PairWalk::MB560` (0x51b560) on `receiver` with the vector `a` and the identity `b`; returns its output.
    fn walk(&mut self, receiver: u32, a: [f32; 3], b: u32) -> [f32; 3];
}

/// `CHingedRigidBody::SimD860`: the vector the caller stores in `out`.
pub fn sim_d860<C: D860Calls>(calls: &mut C, input: &D860Input) -> [f32; 3] {
    let [b0, b1, b2] = input.rotation_row;
    let m = input.definition_matrix;
    let row = from_fn(|k| (m[1][k] * b1 + m[2][k] * b2) + m[0][k] * b0);
    let walked = calls.walk(input.car, row, input.identity);
    from_fn(|k| input.offset[k] + walked[k])
}
