//! The torque about the hinge axis of the hinged face's slip term: a force on the face from the car's vector and speed,
//! `0.5 * face_area * |v . n| * speed^2 * v` along the carried vector `v` (n is the face normal in the car's frame),
//! applied at the body's position relative to the composed hinge point and projected on the hinge axis. The form is that of a
//! dynamic-pressure force, but the original does not say what the car's vector and speed are, so they are named by use.
//! The carries (frame transforms of a vector) and the vector copy are the caller's ([`SlipCalls`]); the arithmetic is the
//! original's, in its order.
//!
//! Where the original's source disagrees with its code (the code is the reference):
//! - The final sum is `(y*c1 + z*c2) + x*c0` (x87, each step at f32); the source sums `x*c0 + z*c2 + y*c1`.
//! - The source keeps a NaN where the code negates it: only a value above zero is kept.

use super::hinge_solve::cross;

/// The inputs read from the hinged body, its definition and its car.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlipInput {
    /// The hinged body, as an object handle: the receiver of the first carry.
    pub body: u32,
    /// `vecFaceNormal_BF`, as a handle: the first carry's vector.
    pub face_normal_bf: u32,
    /// The body's car, as an object handle: the frame the carries go into.
    pub car: u32,
    /// The car's state object, as a handle.
    pub state: u32,
    /// The identity frame (the root), as an object handle: the receiver of the second carry.
    pub identity: u32,
    /// The car's vector (16 bytes in its state, named by use), copied for the second carry.
    pub car_vector: u32,
    /// The hinged body's position.
    pub position: [f32; 3],
    /// The hinge point composed with the car, subtracted from the position.
    pub composed_hinge_point: [f32; 3],
    /// `FaceArea`, scaling the carried vector's dot product with the face normal.
    pub face_area: f32,
    /// `vecHingeAxis_RF`: the torque is projected on it.
    pub hinge_axis_rf: [f32; 3],
}

/// Scratch the model hands to the calls: the copy of the car's vector (16 bytes), and the outputs of the two carries
/// (16 bytes each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlipHandles {
    pub vector_copy: u32,
    pub face_normal: u32,
    pub velocity: u32,
}

/// What the slip term needs from the game: the copy, the two carries and the car's speed word.
pub trait SlipCalls {
    /// The 16 bytes at `from` stored at `to`.
    fn copy_vector(&mut self, to: u32, from: u32);
    /// Carries the vector at `vector` from `receiver`'s frame into `car`'s, output at `out`; returns the output's three
    /// floats.
    fn carry(&mut self, receiver: u32, vector: u32, car: u32, out: u32) -> [f32; 3];
    /// The car state's speed word (named by use: it is squared in the force).
    fn car_speed(&self, state: u32) -> f32;
}

/// The slip term: the slip force's torque about the hinge axis.
pub fn slip_torque<C: SlipCalls>(calls: &mut C, input: &SlipInput, handles: &SlipHandles) -> f32 {
    let face_normal = calls.carry(input.body, input.face_normal_bf, input.car, handles.face_normal);
    calls.copy_vector(handles.vector_copy, input.car_vector);
    let speed = calls.car_speed(input.state);
    let velocity = calls.carry(input.identity, handles.vector_copy, input.car, handles.velocity);

    let lever = [
        input.position[0] - input.composed_hinge_point[0],
        input.position[1] - input.composed_hinge_point[1],
        input.position[2] - input.composed_hinge_point[2],
    ];
    // the original's order: (v2*n2 + v1*n1) + v0*n0
    let mut projected =
        (velocity[2] * face_normal[2] + velocity[1] * face_normal[1] + velocity[0] * face_normal[0]) * input.face_area;
    // only a value above zero is kept, a NaN is negated
    if !(projected > 0.0) {
        projected = 0.0 - projected;
    }
    let scale = projected * speed * speed * 0.5;
    let slip = velocity.map(|v| scale * v);
    let torque = cross(lever, slip);
    // x87 order: (y*c1 + z*c2) + x*c0, each step at f32
    (torque[1] * input.hinge_axis_rf[1] + torque[2] * input.hinge_axis_rf[2]) + torque[0] * input.hinge_axis_rf[0]
}
