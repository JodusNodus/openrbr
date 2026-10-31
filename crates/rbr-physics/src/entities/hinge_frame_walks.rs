//! The frames the hinged body is carried through for its signals: the car's carries of the rows of `axes_rf` and of the
//! products of the hinge rotation's rows with it, the car's transform of the frame matrix's product with
//! `inverse_origin_bf` (plus the hinge point), then the carry of the face normal and of the three basis vectors on the
//! hinged body. The carries' outputs and the transform's output are scratch, so what the model decides is
//! the calls it makes and their input vectors ([`FrameWalkCalls`], answered by the caller).
//!
//! The sums keep the original's order, which changes the bits: `row_times_matrix` and `row_times_matrix_alt` (the
//! rotation rows times the definition matrix) and `vector_times_matrix`.

use std::array::from_fn;

/// The basis the last three carries take: the x, y and z unit vectors.
const BASIS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// The inputs read from the hinged body and its definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameWalkInput {
    /// The identity frame (the root), as an object handle.
    pub identity: u32,
    /// The hinged body, as an object handle: the receiver of the axis and basis carries.
    pub body: u32,
    /// The body's car, as an object handle: the receiver of the matrix carries and the target of the transform.
    pub car: u32,
    /// The hinge rotation, one row per vector.
    pub hinge_rotation: [[f32; 3]; 3],
    /// The body's frame matrix ([`super::hinge_pose::PoseBody::frame_matrix`]): `vector_times_matrix`'s matrix.
    pub frame_matrix: [[f32; 3]; 3],
    /// The axes built from the hinge axis and the zero axis in the reference frame: its rows are carried, and the
    /// rotation rows are multiplied by it.
    pub axes_rf: [[f32; 3]; 3],
    /// The inverse origin of the body-frame axes build: `vector_times_matrix`'s vector.
    pub inverse_origin_bf: [f32; 3],
    /// The hinge point in the reference frame: added to the product before the transform.
    pub hinge_point_rf: [f32; 3],
    /// The face normal in the body frame: the vector carried on the body.
    pub face_normal_bf: [f32; 3],
}

/// What the frame walks need from the game: the carries and the car's transform, each with its input vector.
pub trait FrameWalkCalls {
    /// Carries `vector` from the identity frame into `receiver`'s frame; the output is not used.
    fn carry(&mut self, receiver: u32, vector: [f32; 3], identity: u32);
    /// The car's transform of `vector`; the output is not used.
    fn car_transform(&mut self, car: u32, vector: [f32; 3]);
}

/// The calls in order, with the vectors the original passes.
pub fn walk_hinge_frames<C: FrameWalkCalls>(calls: &mut C, input: &FrameWalkInput) {
    let matrix = input.axes_rf;
    for row in matrix {
        calls.carry(input.car, row, input.identity);
    }
    let [a, b, c] = input.hinge_rotation;
    for product in [row_times_matrix(a, matrix), row_times_matrix_alt(b, matrix), row_times_matrix(c, matrix)] {
        calls.carry(input.car, product, input.identity);
    }
    let product = vector_times_matrix(input.inverse_origin_bf, input.frame_matrix);
    let shifted = from_fn(|k| product[k] + input.hinge_point_rf[k]);
    calls.car_transform(input.car, shifted);
    calls.carry(input.body, input.face_normal_bf, input.identity);
    for basis in BASIS {
        calls.carry(input.body, basis, input.identity);
    }
}

/// `a * m` (row vector times matrix), summed in the original's order: columns 0 and 1 add `a[2]` first, then `a[1]`, then
/// `a[0]`; column 2 adds `a[2]`, then `a[0]`, then `a[1]`.
fn row_times_matrix(a: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        (a[2] * m[2][0] + a[1] * m[1][0]) + a[0] * m[0][0],
        (a[2] * m[2][1] + a[1] * m[1][1]) + a[0] * m[0][1],
        (a[2] * m[2][2] + a[0] * m[0][2]) + a[1] * m[1][2],
    ]
}

/// `b * m`, summed in the original's order (it differs from `row_times_matrix`): column 0 adds `b[1]`, `b[0]`, `b[2]`; column 1 adds
/// `b[1]`, `b[2]`, `b[0]`; column 2 adds `b[1]`, `b[0]`, `b[2]`.
fn row_times_matrix_alt(b: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        (b[1] * m[1][0] + b[0] * m[0][0]) + b[2] * m[2][0],
        (b[1] * m[1][1] + b[2] * m[2][1]) + b[0] * m[0][1],
        (b[1] * m[1][2] + b[0] * m[0][2]) + b[2] * m[2][2],
    ]
}

/// The vector times the matrix: lane `k` is `(m[0][k]*x + m[1][k]*y) + m[2][k]*z` for the vector `(x, y, z)`.
fn vector_times_matrix(v: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    from_fn(|k| (m[0][k] * v[0] + m[1][k] * v[1]) + m[2][k] * v[2])
}
