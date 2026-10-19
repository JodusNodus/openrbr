//! `CHingedRigidBody::SimCFF0` (0x51cff0): the hinge's slip term, a force from the car's state on the hinged body. Its
//! two walks (`E43PairWalk::MB560`) and the copy of the car's vector are calls the caller makes (`SlipCalls`); the
//! arithmetic is the asm's, in its order.
//!
//! Where src/rbr/RBR_E44_SimA.cpp disagrees with the asm (the asm is the reference):
//! - The final sum is `(y*c1 + z*c2) + x*c0` (x87, each step at f32); the C++ sums `x*c0 + z*c2 + y*c1`.
//! - `f4 <= 0` in the C++ keeps a NaN; the asm's `ja` does not take the branch for a NaN, so it is negated.

use super::hinge_solve::cross;

/// The inputs read from the hinged body, its definition and its car.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlipInput {
    /// The hinged body: the receiver of the first walk.
    pub hinge: u32,
    /// The definition's axis object: the first walk's `a`.
    pub axis: u32,
    /// The car: the `b` of both walks.
    pub car: u32,
    /// The car's state object (`CCar::StateAtb`).
    pub state: u32,
    /// The identity object (the global at 0x7cbb00): the second walk's receiver.
    pub identity: u32,
    /// The car's state vector (16 bytes at the state's +0xc10), copied for the second walk.
    pub state_vector: u32,
    /// The hinged body's position.
    pub position: [f32; 3],
    /// The definition's anchor, subtracted from the position.
    pub anchor: [f32; 3],
    /// The definition's friction coefficient, scaling the walk's dot product.
    pub friction: f32,
    /// The definition's scale vector, applied to the cross product.
    pub scale: [f32; 3],
}

/// Scratch the port writes: the copy of the state vector (16 bytes), and the outputs of the two walks (16 bytes each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlipFrame {
    pub vector_copy: u32,
    pub hit: u32,
    pub velocity: u32,
}

/// What the slip term needs from the game: the copy, the two walks and the car's state word.
pub trait SlipCalls {
    /// `E01Vec4::CopyFrom` (0x43c5b0): the 16 bytes at `from` stored at `to`.
    fn copy_vector(&mut self, to: u32, from: u32);
    /// `E43PairWalk::MB560` (0x51b560) on `receiver` with (`a`, `b`), output at `out`; returns the output's three floats.
    fn walk(&mut self, receiver: u32, a: u32, b: u32, out: u32) -> [f32; 3];
    /// The float at +0xc00 of the car's state (its meaning is unknown; the asm's name is `f1`).
    fn car_state_c00(&self, state: u32) -> f32;
}

/// `CHingedRigidBody::SimCFF0`: the slip term (returned in st0).
pub fn slip_term<C: SlipCalls>(calls: &mut C, input: &SlipInput, frame: &SlipFrame) -> f32 {
    let hit = calls.walk(input.hinge, input.axis, input.car, frame.hit);
    calls.copy_vector(frame.vector_copy, input.state_vector);
    let speed = calls.car_state_c00(input.state);
    let velocity = calls.walk(input.identity, frame.vector_copy, input.car, frame.velocity);

    let lever =
        [input.position[0] - input.anchor[0], input.position[1] - input.anchor[1], input.position[2] - input.anchor[2]];
    // asm order: (v2*h2 + v1*h1) + v0*h0
    let mut f4 = (velocity[2] * hit[2] + velocity[1] * hit[1] + velocity[0] * hit[0]) * input.friction;
    // asm `ja`: only a value above zero is kept, a NaN is negated
    if !(f4 > 0.0) {
        f4 = 0.0 - f4;
    }
    let scale = f4 * speed * speed * 0.5;
    let slip = velocity.map(|v| scale * v);
    let torque = cross(lever, slip);
    // x87 order: (y*c1 + z*c2) + x*c0, each step at f32
    (torque[1] * input.scale[1] + torque[2] * input.scale[2]) + torque[0] * input.scale[0]
}
