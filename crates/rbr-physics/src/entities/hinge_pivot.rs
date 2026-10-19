//! `CHingedRigidBody::SimE1D0` (0x51e1d0): the hinge's pivot (+0x600..+0x608) and its settle value (+0x620, which
//! `SimEAD0` reads). The walks (`E43PairWalk::MB560`) and `CCar::BodyCompose` are calls the caller makes (`PivotCalls`);
//! the arithmetic is the asm's, in its order.
//!
//! Where src/rbr/RBR_E44_SimB.cpp disagrees with the asm (the asm is the reference):
//! - `l40.Cross(&l40.m0, m18 + 2)` crosses `m0` with itself: the asm passes `&l40.m0` as both operands (0x51e296), so
//!   the first cross is zero and the pivot is the second cross plus the first walk's hit. The port keeps that.

use super::hinge_solve::cross;

/// `SimE1D0` adds this to the car's height (`*(float*)(car+0x258)`), as a float (the constant at 0x72fc98).
const GRAVITY: f32 = 9.81;

/// The inputs read from the hinged body, its car and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PivotInput {
    /// The walks' receiver: the identity global's value.
    pub identity: u32,
    /// The car (`this+0x3d0`): the `b` of the walks and the receiver of `BodyCompose`.
    pub car: u32,
    /// The car's reference words at +0x250 and +0x254 (copied as bits into the first walk's vector).
    pub reference: [u32; 2],
    /// The car's height at +0x258; the first walk's vector gets this plus gravity.
    pub height: f32,
    /// The car's vector at +0x260: the second walk's `a`.
    pub car_offset: u32,
    /// The third walk's `a`: the car's +0x190 when its +0x180 is set, else 0 (the meaning is unknown).
    pub car_probe: u32,
    /// `BodyCompose`'s `a`: the definition's +0x300 when its +0x2f0 is set, else 0.
    pub compose_a: u32,
    /// The definition's settle weights (+0x2c0, `pf`).
    pub settle_weights: [f32; 3],
}

/// Scratch the port hands to the calls: the walks' outputs and the compose's output, as the original's frame holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PivotFrame {
    /// The first walk's vector (16 bytes): the reference words and the height.
    pub vector: u32,
    /// The first walk's output: the hit (16 bytes).
    pub hit: u32,
    /// The second walk's output: the hinged body's +0x610.
    pub settle_hit: u32,
    /// The third walk's output (`m0`, +0); `BodyCompose`'s output is 0x10 further on (`mC + 1`).
    pub scratch: u32,
}

/// What the caller stores in the hinged body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pivot {
    /// The pivot, written at +0x600..+0x608.
    pub pivot: [f32; 3],
    /// The settle value, written at +0x620.
    pub settle: f32,
}

/// What `SimE1D0` needs from the game: the vector's words, the walks and `BodyCompose`.
pub trait PivotCalls {
    /// Writes the three words of the walk's vector at `at` (the caller's frame).
    fn store_vector(&mut self, at: u32, words: [u32; 3]);
    /// `E43PairWalk::MB560` (0x51b560) on `receiver` with (`a`, `b`), output at `out`; returns the output's three floats.
    fn walk(&mut self, receiver: u32, a: u32, b: u32, out: u32) -> [f32; 3];
    /// `CCar::BodyCompose` (0x4e6a30) on `car` with (`a`, `out`); returns the output's three floats.
    fn body_compose(&mut self, car: u32, a: u32, out: u32) -> [f32; 3];
}

/// `CHingedRigidBody::SimE1D0`: the pivot and the settle value.
pub fn pivot_and_settle<C: PivotCalls>(calls: &mut C, input: &PivotInput, frame: &PivotFrame) -> Pivot {
    let height = input.height + GRAVITY;
    calls.store_vector(frame.vector, [input.reference[0], input.reference[1], height.to_bits()]);
    let hit = calls.walk(input.identity, frame.vector, input.car, frame.hit);
    let settle_hit = calls.walk(input.identity, input.car_offset, input.car, frame.settle_hit);
    let probe = calls.walk(input.identity, input.car_probe, input.car, frame.scratch);
    let composed = calls.body_compose(input.car, input.compose_a, frame.scratch + 0x10);

    let first = cross(probe, probe);
    let second = cross(first, composed);
    let pivot = [0, 1, 2].map(|i| second[i] + hit[i]);
    let w = input.settle_weights;
    let settle = (w[2] * settle_hit[2] + w[1] * settle_hit[1]) + w[0] * settle_hit[0];
    Pivot { pivot, settle }
}
