//! `CHingedRigidBody::SimD160` (0x51d160): the hinged body's frame from its rotation block. It runs `CCar::Xform8130` and
//! `E43PairWalk::MB560` (the car's transforms, into +0x5c0 and +0x5f0), `SimC960` (the rotation block, from the hinge angle),
//! then the products of the rotation with the definition's matrices (the body's `quat_matrix` and `frame`), `E34Quat::ToQuat` on
//! `frame` (into the quaternion, +0x100), and the translation into +0x140 (the original's `E01Vec4::CopyFrom`; the port stores it).
//!
//! The calls are the caller's (`D160Calls`). The body is read back after `SimC960`, which writes the rotation block; the
//! products are stored before `ToQuat`, which reads `frame`. The sums keep the asm's term order, which changes the bits.

use crate::math::Mat3;

/// The definition's frame words `SimD160` reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D160Def {
    /// The anchor (+0x320): the translation's offset, added last.
    pub anchor: [f32; 3],
    /// The column weights (+0x3d0): the translation takes the frame's columns against them. The same vector as `SimEAD0`'s
    /// `c` (`hinge_solve`'s `column_weights`).
    pub column_weights: [f32; 3],
    /// The hinge matrix (+0x3e0, row-major; `SimEAD0`'s `d`).
    pub hinge_mat: Mat3,
    /// The second matrix (+0x428, row-major). Meaning unknown (`README.md`).
    pub second_mat: Mat3,
}

/// The body words `SimD160` reads or writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D160Body {
    /// The E34Quat matrix at +0x550 (the object `ToQuat` reads in SimFB40): `SimD160` writes `rot_x * hinge_mat` there.
    pub quat_matrix: Mat3,
    /// `(second_mat * rot_x) * hinge_mat` (+0x574): `ToQuat`'s matrix, and the translation's frame.
    pub frame: Mat3,
    /// The rotation block about X by the hinge angle (+0x598), written by `SimC960`.
    pub rot_x: Mat3,
}

/// The words `SimD160` reads or writes: the definition's frame words, the body's block and its position (+0x140).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D160Words {
    pub def: D160Def,
    pub body: D160Body,
    pub position: [f32; 4],
}

/// The addresses `SimD160` passes to its calls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct D160Frame {
    /// the body's car (the receiver of `Xform8130` and `MB560`)
    pub car: u32,
    /// `Xform8130`'s input: the definition's pose, +0x300
    pub pose: u32,
    /// `MB560`'s first input: the definition's settle vector, +0x2c0
    pub walk_a: u32,
    /// `MB560`'s second input: the identity global's value
    pub identity: u32,
    /// `Xform8130`'s output: the body's +0x5c0
    pub xform_out: u32,
    /// `MB560`'s output: the body's +0x5f0
    pub walk_out: u32,
    /// the body (`SimC960`'s receiver)
    pub this: u32,
    /// the matrix `ToQuat` reads: the body's +0x574
    pub matrix: u32,
    /// the quaternion `ToQuat` writes: the body's +0x100
    pub quat_out: u32,
}

/// The calls `SimD160` makes: the caller answers them, and reads and writes the body and the definition through `load`
/// and `store`.
pub trait D160Calls {
    /// The body's block and the definition's frame words, as they are now.
    fn load(&self) -> D160Words;
    /// Writes the body's products (`quat_matrix`, `frame`) and its position (the translation).
    fn store(&mut self, words: &D160Words);
    /// `CCar::Xform8130(pose, out)` on the car.
    fn xform_8130(&mut self, car: u32, pose: u32, out: u32);
    /// `E43PairWalk::MB560(a, identity, out)` on the car.
    fn mb560(&mut self, car: u32, a: u32, identity: u32, out: u32);
    /// `CHingedRigidBody::SimC960` on the body: the rotation block from the hinge angle.
    fn hinge_rot_block(&mut self, this: u32);
    /// `E34Quat::ToQuat` on the 3x3 matrix at `matrix`, into `out`.
    fn to_quat(&mut self, matrix: u32, out: u32);
}

/// `CHingedRigidBody::SimD160` (0x51d160).
pub fn sim_d160<C: D160Calls>(calls: &mut C, frame: &D160Frame) {
    calls.xform_8130(frame.car, frame.pose, frame.xform_out);
    calls.mb560(frame.car, frame.walk_a, frame.identity, frame.walk_out);
    calls.hinge_rot_block(frame.this);
    let mut words = calls.load();
    let (quat_matrix, body_frame) = products(&words.def, &words.body.rot_x);
    words.body.quat_matrix = quat_matrix;
    words.body.frame = body_frame;
    calls.store(&words);
    calls.to_quat(frame.matrix, frame.quat_out);
    // E01Vec4::CopyFrom of the translation into +0x140, as the store (the original's copy is a call; the port's is inline)
    let mut words = calls.load();
    words.position = translation(&words);
    calls.store(&words);
}

/// A sum of three products, added left to right in the given order (the asm's order, which changes the bits).
fn sum3(terms: [(f32, f32); 3]) -> f32 {
    terms[0].0 * terms[0].1 + terms[1].0 * terms[1].1 + terms[2].0 * terms[2].1
}

/// The products of the rotation `rot_x` with the definition's matrices, each entry in the asm's term order:
/// `quat_matrix = rot_x * hinge_mat`, and `frame = (second_mat * rot_x) * hinge_mat`.
fn products(def: &D160Def, rot_x: &Mat3) -> (Mat3, Mat3) {
    let a = &def.hinge_mat;
    let b = &def.second_mat;
    let r = rot_x;
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
    let quat_matrix: Mat3 = [
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
    let frame: Mat3 = [
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
    (quat_matrix, frame)
}

/// The translation: the column weights against the frame's columns, plus the anchor. The fourth lane is the asm's own sum:
/// its last term is `rot_x`'s (0, 0) entry, not a frame entry (kept as the asm reads it).
fn translation(words: &D160Words) -> [f32; 4] {
    let [w0, w1, w2] = words.def.column_weights;
    let f = &words.body.frame;
    let column = |c: usize| f[0][c] * w0 + f[1][c] * w1 + f[2][c] * w2;
    let anchor = words.def.anchor;
    let w3 = f[1][0] * w0 + f[2][0] * w1 + words.body.rot_x[0][0] * w2;
    [column(0) + anchor[0], column(1) + anchor[1], column(2) + anchor[2], w3]
}
