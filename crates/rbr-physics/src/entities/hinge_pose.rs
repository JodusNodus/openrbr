//! The hinged body's pose from its hinge angle: the car's transforms of the definition's hinge point and hinge axis, the
//! rotation about the hinge axis (X) by the angle, then the products of that rotation with the definition's two matrices
//! (the body's orientation matrix and its frame matrix), the orientation quaternion of the frame, and the body's
//! position (the frame's columns weighted by `inverse_origin_bf`, plus the composed hinge point).
//!
//! The calls are the caller's ([`PoseCalls`]). The body is read back after the rotation is updated; the products are
//! stored before the quaternion is taken from the frame. The sums keep the original's term order, which changes the bits.

use crate::math::Mat3;

/// What the definition gives the pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseDefinition {
    /// The hinge point composed with the car (`vecHingePoint_RF` through the car): the position's offset, added last.
    pub composed_hinge_point: [f32; 3],
    /// The position takes the frame's columns against these weights: the inverse origin of the body-frame axes build
    /// (`-(F vecHingePoint_BF)`, see [`super::hinge_axes::inverse`]).
    pub inverse_origin_bf: [f32; 3],
    /// The axes built from `vecHingeAxis_RF` and `vecZeroAxis_RF` (row-major; [`super::hinge_axes::axes`]).
    pub axes_rf: Mat3,
    /// The inverse axes of the body-frame build, from `vecHingeAxis_BF` and `vecZeroAxis_BF` (row-major).
    pub inverse_axes_bf: Mat3,
}

/// The body's matrices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseBody {
    /// The orientation matrix, `hinge_rotation * axes_rf`: the matrix the orientation quaternion is taken from when
    /// the body is posed from its definition.
    pub orientation_matrix: Mat3,
    /// `(inverse_axes_bf * hinge_rotation) * axes_rf`: the orientation quaternion's matrix, and the position's frame.
    pub frame_matrix: Mat3,
    /// The rotation about X by the hinge angle ([`super::hinge_limits::rotation_block`]).
    pub hinge_rotation: Mat3,
}

/// The pose's inputs and outputs: the definition, the body's matrices and its position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseState {
    pub definition: PoseDefinition,
    pub body: PoseBody,
    pub position: [f32; 4],
}

/// The object handles (the game's addresses, opaque to the model) the pose passes to its calls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseHandles {
    /// The body's car: the receiver of the transform and the frame carry.
    pub car: u32,
    /// The definition's hinge point (`vecHingePoint_RF`), which the car transforms.
    pub hinge_point_rf: u32,
    /// The definition's hinge axis (`vecHingeAxis_RF`), which is carried from the identity frame into the car's.
    pub hinge_axis_rf: u32,
    /// The identity frame (the root).
    pub identity: u32,
    /// Where the car's transform of the hinge point goes.
    pub transformed_hinge_point: u32,
    /// Where the carried hinge axis goes.
    pub carried_hinge_axis: u32,
    /// The body.
    pub body: u32,
    /// The frame matrix, which the orientation quaternion is taken from.
    pub frame_matrix: u32,
    /// Where the orientation quaternion goes.
    pub quaternion_out: u32,
}

/// What the pose needs from the game: the body's words, and the calls it makes.
pub trait PoseCalls {
    /// The body's matrices and position and the definition's words, as they are now.
    fn load(&self) -> PoseState;
    /// Writes the body's products (`orientation_matrix`, `frame_matrix`) and its position.
    fn store(&mut self, state: &PoseState);
    /// The car transforms the definition's hinge point into `out`.
    fn car_transform(&mut self, car: u32, pose: u32, out: u32);
    /// Carries `vector` from the identity frame into the car's frame, into `out`.
    fn carry_into_car(&mut self, car: u32, vector: u32, identity: u32, out: u32);
    /// Updates the body's rotation about the hinge axis from the hinge angle.
    fn update_hinge_rotation(&mut self, body: u32);
    /// The quaternion of the 3x3 orientation matrix at `matrix`, into `out`.
    fn matrix_to_quaternion(&mut self, matrix: u32, out: u32);
}

/// Poses the hinged body at its hinge angle.
pub fn update_pose<C: PoseCalls>(calls: &mut C, handles: &PoseHandles) {
    calls.car_transform(handles.car, handles.hinge_point_rf, handles.transformed_hinge_point);
    calls.carry_into_car(handles.car, handles.hinge_axis_rf, handles.identity, handles.carried_hinge_axis);
    calls.update_hinge_rotation(handles.body);
    let mut state = calls.load();
    let (orientation_matrix, frame_matrix) = products(&state.definition, &state.body.hinge_rotation);
    state.body.orientation_matrix = orientation_matrix;
    state.body.frame_matrix = frame_matrix;
    calls.store(&state);
    calls.matrix_to_quaternion(handles.frame_matrix, handles.quaternion_out);
    // the position is a plain copy of the translation, after the quaternion
    let mut state = calls.load();
    state.position = translation(&state);
    calls.store(&state);
}

/// A sum of three products, added left to right in the given order (the original's order, which changes the bits).
fn sum3(terms: [(f32, f32); 3]) -> f32 {
    terms[0].0 * terms[0].1 + terms[1].0 * terms[1].1 + terms[2].0 * terms[2].1
}

/// The products of the rotation `hinge_rotation` with the definition's matrices, each entry in the original's term order:
/// `orientation_matrix = hinge_rotation * axes_rf`, and `frame_matrix = (inverse_axes_bf * hinge_rotation) * axes_rf`.
fn products(def: &PoseDefinition, hinge_rotation: &Mat3) -> (Mat3, Mat3) {
    let a = &def.axes_rf;
    let b = &def.inverse_axes_bf;
    let r = hinge_rotation;
    let v: Mat3 = [
        [
            sum3([(b[0][2], r[2][0]), (b[0][0], r[0][0]), (b[0][1], r[1][0])]),
            sum3([(b[0][2], r[2][1]), (r[1][1], b[0][1]), (r[0][1], b[0][0])]),
            sum3([(b[0][2], r[2][2]), (b[0][1], r[1][2]), (r[0][2], b[0][0])]),
        ],
        [
            sum3([(r[2][0], b[1][2]), (b[1][1], r[1][0]), (b[1][0], r[0][0])]),
            sum3([(r[0][1], b[1][0]), (b[1][2], r[2][1]), (b[1][1], r[1][1])]),
            sum3([(r[2][2], b[1][2]), (b[1][1], r[1][2]), (b[1][0], r[0][2])]),
        ],
        [
            sum3([(b[2][2], r[2][0]), (b[2][0], r[0][0]), (b[2][1], r[1][0])]),
            sum3([(b[2][2], r[2][1]), (r[0][1], b[2][0]), (r[1][1], b[2][1])]),
            sum3([(b[2][2], r[2][2]), (r[0][2], b[2][0]), (b[2][1], r[1][2])]),
        ],
    ];
    let orientation_matrix: Mat3 = [
        [
            sum3([(a[0][0], r[0][0]), (a[2][0], r[0][2]), (a[1][0], r[0][1])]),
            sum3([(a[2][1], r[0][2]), (a[0][1], r[0][0]), (r[0][1], a[1][1])]),
            sum3([(a[2][2], r[0][2]), (a[1][2], r[0][1]), (a[0][2], r[0][0])]),
        ],
        [
            sum3([(a[0][0], r[1][0]), (a[2][0], r[1][2]), (a[1][0], r[1][1])]),
            sum3([(a[2][1], r[1][2]), (a[0][1], r[1][0]), (r[1][1], a[1][1])]),
            sum3([(a[2][2], r[1][2]), (a[1][2], r[1][1]), (a[0][2], r[1][0])]),
        ],
        [
            sum3([(r[2][2], a[2][0]), (a[1][0], r[2][1]), (a[0][0], r[2][0])]),
            sum3([(r[2][2], a[2][1]), (a[0][1], r[2][0]), (r[2][1], a[1][1])]),
            sum3([(r[2][0], a[0][2]), (r[2][2], a[2][2]), (a[1][2], r[2][1])]),
        ],
    ];
    let frame_matrix: Mat3 = [
        [
            sum3([(a[0][0], v[0][0]), (a[1][0], v[0][1]), (a[2][0], v[0][2])]),
            sum3([(v[0][1], a[1][1]), (a[0][1], v[0][0]), (v[0][2], a[2][1])]),
            sum3([(v[0][1], a[1][2]), (a[2][2], v[0][2]), (a[0][2], v[0][0])]),
        ],
        [
            sum3([(a[0][0], v[1][0]), (a[1][0], v[1][1]), (a[2][0], v[1][2])]),
            sum3([(v[1][1], a[1][1]), (v[1][2], a[2][1]), (a[0][1], v[1][0])]),
            sum3([(a[2][2], v[1][2]), (v[1][1], a[1][2]), (a[0][2], v[1][0])]),
        ],
        [
            sum3([(a[0][0], v[2][0]), (a[1][0], v[2][1]), (a[2][0], v[2][2])]),
            sum3([(v[2][1], a[1][1]), (a[0][1], v[2][0]), (v[2][2], a[2][1])]),
            sum3([(a[2][2], v[2][2]), (v[2][1], a[1][2]), (a[0][2], v[2][0])]),
        ],
    ];
    (orientation_matrix, frame_matrix)
}

/// The translation: the inverse origin against the frame's columns, plus the composed hinge point. The fourth lane is the original's own sum:
/// its last term is `hinge_rotation`'s (0, 0) entry, not a frame entry (kept as the original reads it).
fn translation(state: &PoseState) -> [f32; 4] {
    let [w0, w1, w2] = state.definition.inverse_origin_bf;
    let f = &state.body.frame_matrix;
    let column = |c: usize| f[0][c] * w0 + f[1][c] * w1 + f[2][c] * w2;
    let hinge_point = state.definition.composed_hinge_point;
    let w3 = f[1][0] * w0 + f[2][0] * w1 + state.body.hinge_rotation[0][0] * w2;
    [column(0) + hinge_point[0], column(1) + hinge_point[1], column(2) + hinge_point[2], w3]
}
