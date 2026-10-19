//! `CHingedRigidBody::SimDB40` (0x51db40): the car's walks over the definition's matrix rows and over the three products
//! of the hinged body's rotation rows with that matrix, the car's transform of the matrix's product with the definition's
//! vector (plus its pose), then the walk over the definition's axis and the three basis walks on the hinged body. The
//! walks' outputs and the transform's output are scratch, so the port's observable behaviour is the calls it makes and
//! their input vectors. The calls are `E43PairWalk::MB560` (0x51b560) and `CCar::Xform8130` (0x4e8130), made by the caller.
//!
//! The arithmetic is the asm's (SSE single precision). The sums keep the asm's order, which changes the bits:
//! `row_times` and `row_times_b` (the rotation rows times the matrix) and `mul_m33` (`E44_MulM33`, RBR_E44_SimB.cpp:38).
//! Where RBR_E44_SimB.cpp:145-225 differs, it is in these orders only, and the asm is the reference.

use std::array::from_fn;

/// The basis the last three walks take: the x, y and z unit vectors.
const BASIS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// The inputs read from the hinged body and its definition (after the guard passes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DbInput {
    /// The walks' `b`: the identity global's value.
    pub identity: u32,
    /// The hinged body (`this`): the receiver of the axis walk and the basis walks.
    pub hinge: u32,
    /// The owner car (+0x3d0): the receiver of the matrix walks and the target of the transform.
    pub car: u32,
    /// The rotation block at +0x598, one row per vector (`A`, `B`, `C`). The meaning of the rows is unknown here.
    pub rotation: [[f32; 3]; 3],
    /// The hinged body's 3x3 at +0x574 (the meaning is unknown): `E44_MulM33`'s matrix.
    pub hinge_matrix: [[f32; 3]; 3],
    /// The definition's 3x3 at +0x3e0: its rows are walked, and the rotation rows are multiplied by it.
    pub definition_matrix: [[f32; 3]; 3],
    /// The definition's vector at +0x3d0 (the meaning is unknown): `E44_MulM33`'s vector.
    pub definition_vector: [f32; 3],
    /// The definition's pose at +0x300: added to the product before the transform.
    pub definition_pose: [f32; 3],
    /// The definition's axis at +0x360 (`DEF_AXIS`): the vector of the walk on the hinged body.
    pub definition_axis: [f32; 3],
}

/// What `SimDB40` needs from the game: the walks and the car's transform, each with its input vector.
pub trait DbCalls {
    /// `E43PairWalk::MB560` (0x51b560) on `receiver` with the vector `a` and the identity `b`; the output is not used.
    fn walk(&mut self, receiver: u32, a: [f32; 3], b: u32);
    /// `CCar::Xform8130` (0x4e8130) on `car` with the vector `a`; the output is not used.
    fn xform8130(&mut self, car: u32, a: [f32; 3]);
}

/// `CHingedRigidBody::SimDB40`: the calls in order, with the vectors the original passes.
pub fn sim_db40<C: DbCalls>(calls: &mut C, input: &DbInput) {
    let matrix = input.definition_matrix;
    for row in matrix {
        calls.walk(input.car, row, input.identity);
    }
    let [a, b, c] = input.rotation;
    for product in [row_times(a, matrix), row_times_b(b, matrix), row_times(c, matrix)] {
        calls.walk(input.car, product, input.identity);
    }
    let product = mul_m33(input.definition_vector, input.hinge_matrix);
    let shifted = from_fn(|k| product[k] + input.definition_pose[k]);
    calls.xform8130(input.car, shifted);
    calls.walk(input.hinge, input.definition_axis, input.identity);
    for basis in BASIS {
        calls.walk(input.hinge, basis, input.identity);
    }
}

/// `a * m` (row vector times matrix), summed in the asm's order: columns 0 and 1 add `a[2]` first, then `a[1]`, then
/// `a[0]`; column 2 adds `a[2]`, then `a[0]`, then `a[1]`.
fn row_times(a: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        (a[2] * m[2][0] + a[1] * m[1][0]) + a[0] * m[0][0],
        (a[2] * m[2][1] + a[1] * m[1][1]) + a[0] * m[0][1],
        (a[2] * m[2][2] + a[0] * m[0][2]) + a[1] * m[1][2],
    ]
}

/// `b * m`, summed in the asm's order (it differs from `row_times`): column 0 adds `b[1]`, `b[0]`, `b[2]`; column 1 adds
/// `b[1]`, `b[2]`, `b[0]`; column 2 adds `b[1]`, `b[0]`, `b[2]`.
fn row_times_b(b: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    [
        (b[1] * m[1][0] + b[0] * m[0][0]) + b[2] * m[2][0],
        (b[1] * m[1][1] + b[2] * m[2][1]) + b[0] * m[0][1],
        (b[1] * m[1][2] + b[0] * m[0][2]) + b[2] * m[2][2],
    ]
}

/// `E44_MulM33`: lane `k` is `(m[0][k]*x + m[1][k]*y) + m[2][k]*z` for the vector `(x, y, z)`.
fn mul_m33(v: [f32; 3], m: [[f32; 3]; 3]) -> [f32; 3] {
    from_fn(|k| (m[0][k] * v[0] + m[1][k] * v[1]) + m[2][k] * v[2])
}
