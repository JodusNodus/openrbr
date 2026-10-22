//! The acceleration of the hinge point in the car's frame and the car's angular acceleration along the hinge axis, which
//! the hinge solve ([`super::hinge_solve`]) uses. The names are by use: the terms have the form of a rotating frame's
//! `a + w x (w x r)` and `alpha . axis` (the car's words are read as an acceleration with gravity added to z, an angular
//! velocity and an angular acceleration), but the original does not name them. The carries
//! (frame transforms of a vector) and the pose composition are the caller's ([`PivotCalls`]); the arithmetic is the
//! original's, in its order.
//!
//! The original's source differs from its code here, and the code is the reference: the first cross product takes the
//! angular velocity with itself, so it is zero, and the result is the second cross plus the carried acceleration. The
//! model keeps that.

use super::hinge_solve::cross;

/// Gravity added to the car's z word before it is carried, m/s^2 (f32).
const GRAVITY: f32 = 9.81;

/// The inputs read from the hinged body, its car and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PivotInput {
    /// The identity frame (the root), as an object handle: the receiver of the carries.
    pub identity: u32,
    /// The body's car, as an object handle: the frame the carries go into, and the receiver of the pose composition.
    pub car: u32,
    /// The car's x and y words of the acceleration vector (copied as bits into the first carry's vector).
    pub car_acceleration_xy: [u32; 2],
    /// The car's z word; the first carry's vector gets this plus gravity.
    pub car_acceleration_z: f32,
    /// The car's angular acceleration vector, as a handle: the second carry's input.
    pub car_angular_acceleration: u32,
    /// The car's angular velocity vector, as a handle (0 when its gate address is 0): the third carry's input.
    pub car_angular_velocity: u32,
    /// `vecHingePoint_RF`, as a handle (0 when its gate address is 0): the pose composition's input.
    pub hinge_point_rf: u32,
    /// `vecHingeAxis_RF`: the weights of the angular acceleration.
    pub hinge_axis_rf: [f32; 3],
}

/// The scratch handles the model gives the calls: the carries' outputs and the composition's output, as the original's
/// frame holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PivotHandles {
    /// The first carry's vector (16 bytes): the acceleration words and gravity.
    pub vector: u32,
    /// The first carry's output.
    pub acceleration: u32,
    /// The second carry's output: the body's carried angular acceleration.
    pub angular_acceleration: u32,
    /// The third carry's output: the car's carried angular velocity.
    pub angular_velocity: u32,
    /// The pose composition's output.
    pub lever: u32,
}

/// What the caller stores in the hinged body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pivot {
    /// The acceleration of the hinge point in the car's frame.
    pub hinge_point_acceleration: [f32; 3],
    /// The car's angular acceleration along the hinge axis.
    pub frame_angular_acceleration: f32,
}

/// What the pivot needs from the game: the vector's words, the carries and the pose composition.
pub trait PivotCalls {
    /// Writes the three words of the first carry's vector at `at` (the caller's frame).
    fn store_vector(&mut self, at: u32, words: [u32; 3]);
    /// Carries the vector `vector` from the `identity` frame into `car`'s frame, output at `out`; returns the output's
    /// three floats.
    fn carry(&mut self, identity: u32, vector: u32, car: u32, out: u32) -> [f32; 3];
    /// Composes `input` with `car`'s pose, output at `out`; returns the output's three floats.
    fn compose_with_car(&mut self, car: u32, input: u32, out: u32) -> [f32; 3];
}

/// The hinge point's acceleration and the frame's angular acceleration along the hinge axis.
pub fn pivot_terms<C: PivotCalls>(calls: &mut C, input: &PivotInput, handles: &PivotHandles) -> Pivot {
    let z = input.car_acceleration_z + GRAVITY;
    calls.store_vector(handles.vector, [input.car_acceleration_xy[0], input.car_acceleration_xy[1], z.to_bits()]);
    let acceleration = calls.carry(input.identity, handles.vector, input.car, handles.acceleration);
    let angular_acceleration =
        calls.carry(input.identity, input.car_angular_acceleration, input.car, handles.angular_acceleration);
    let angular_velocity = calls.carry(input.identity, input.car_angular_velocity, input.car, handles.angular_velocity);
    let lever = calls.compose_with_car(input.car, input.hinge_point_rf, handles.lever);

    let first = cross(angular_velocity, angular_velocity);
    let second = cross(first, lever);
    let hinge_point_acceleration = [0, 1, 2].map(|i| second[i] + acceleration[i]);
    let w = input.hinge_axis_rf;
    let frame_angular_acceleration =
        (w[2] * angular_acceleration[2] + w[1] * angular_acceleration[1]) + w[0] * angular_acceleration[0];
    Pivot { hinge_point_acceleration, frame_angular_acceleration }
}
