//! `CHingedRigidBody::VfE070` (0x51e070): an orthonormal frame from the quaternion `b` and the vector `c`. The frame `e` has
//! three rows: `q` (the normalised xyz of `b`), `m = n x q`, and `n` (the normalised `b x c`). `f` is the transpose of `e`, and
//! `d = -(F a)`, where `F` is `f` and the word after it (`f[9]`); the asm reads that word after it stores `e` (the second call's
//! `f[9]` is `e[0]`).
//!
//! The asm's `E29Quat::Normalize`, `E01Vec4::Normalize` and `E01VecSetA::Cross` are inline here. A normalise is `rsqrtss` of the
//! squared length (`math::rsqrt`), with the lanes summed as `(x*x + y*y) + z*z`. The layout layer stores `e`, reads `f[9]`, then
//! stores `f` and `d` (`layout::vf_e070`). `d`'s negation is a sign flip: the asm multiplies by -1.0, which differs only in
//! the sign of a NaN (platform NaN, not compared).

use crate::math::rsqrt::rsqrt;
use crate::math::vec::Vec3;

/// The addresses `VfE070` takes: `a` (3 floats), `b` (a quaternion: its xyz is read), `c` (3 floats), and the outputs `d`
/// (4 floats), `e` (3x3) and `f` (3x3, which the asm also reads one word past its end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct E070Frame {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
    pub e: u32,
    pub f: u32,
}

/// The unit vector of `v`: each lane times `rsqrtss` of `(x*x + y*y) + z*z`.
fn normalize(v: Vec3) -> Vec3 {
    v.scale(rsqrt(v.x * v.x + v.y * v.y + v.z * v.z))
}

fn to_array(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// The frame `e` (rows `q`, `m`, `n`) from the quaternion `b` (its xyz) and the vector `c`. The cross products keep the asm's
/// operands: `n` is the cross of the raw `b` and `c`, and `m` is `n x q`.
pub fn frame(b: [f32; 4], c: [f32; 3]) -> [[f32; 3]; 3] {
    let b = Vec3 { x: b[0], y: b[1], z: b[2] };
    let c = Vec3 { x: c[0], y: c[1], z: c[2] };
    let q = normalize(b);
    let n = normalize(b.cross3(c));
    let m = n.cross3(q);
    [to_array(q), to_array(m), to_array(n)]
}

/// `VfE070`'s outputs from `e`: `f` is its transpose, and `d` is the negated `F a` with `F` the nine words of `f` and `tail`
/// (`f[9]`). `d[k] = F[k] a0 + F[k+3] a1 + F[k+6] a2` in the asm's order, so `d[3]` reads `tail`.
pub fn project(a: [f32; 3], e: &[[f32; 3]; 3], tail: f32) -> ([f32; 4], [f32; 9]) {
    let f: [f32; 9] = std::array::from_fn(|i| e[i % 3][i / 3]);
    let big: [f32; 10] = std::array::from_fn(|i| if i < 9 { f[i] } else { tail });
    let dot = |k: usize| big[k] * a[0] + big[k + 3] * a[1] + big[k + 6] * a[2];
    ([-dot(0), -dot(1), -dot(2), -dot(3)], f)
}
