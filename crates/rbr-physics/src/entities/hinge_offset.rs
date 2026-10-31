//! The hinged body's offset in the car's frame: `axes_rf` applied to the second row of the hinge rotation,
//! carried from the identity frame into the car's frame, plus the body's offset vector. The carry is the caller's
//! ([`OffsetCalls`]); this returns the vector the caller stores.
//!
//! The row's sums keep the original's order (single precision; the order changes the bits): column `k` is
//! `(m[1][k]*b[1] + m[2][k]*b[2]) + m[0][k]*b[0]`, the same for all three columns.

use std::array::from_fn;

/// The inputs read from the hinged body and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetInput {
    /// The identity frame (the root), as an object handle.
    pub identity: u32,
    /// The body's car, as an object handle: the receiver of the carry.
    pub car: u32,
    /// The second row of the hinge rotation: the vector the definition's matrix is applied to.
    pub rotation_row: [f32; 3],
    /// The axes built from the hinge axis and the zero axis in the reference frame, row-major.
    pub axes_rf: [[f32; 3]; 3],
    /// The body's offset vector (named by use: its meaning is not known), which the carried vector is added to.
    pub offset: [f32; 3],
}

/// What the offset needs from the game.
pub trait OffsetCalls {
    /// Carries `vector` from the identity frame into `receiver`'s frame; returns the result.
    fn carry(&mut self, receiver: u32, vector: [f32; 3], identity: u32) -> [f32; 3];
}

/// The body's offset: the carried matrix-times-row plus the old offset.
pub fn hinge_offset<C: OffsetCalls>(calls: &mut C, input: &OffsetInput) -> [f32; 3] {
    let [b0, b1, b2] = input.rotation_row;
    let m = input.axes_rf;
    let row = from_fn(|k| (m[1][k] * b1 + m[2][k] * b2) + m[0][k] * b0);
    let carried = calls.carry(input.car, row, input.identity);
    from_fn(|k| input.offset[k] + carried[k])
}
