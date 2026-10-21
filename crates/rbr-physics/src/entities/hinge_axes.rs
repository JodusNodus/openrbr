//! An orthonormal frame from a direction and a reference vector, and its inverse placement. The frame `axes` has three
//! rows: `q` (the direction normalised), `m = n x q`, and `n` (the normalised `direction x reference`). The inverse axes
//! are its transpose, and the inverse origin is `-(F origin)`, where `F` is the inverse axes followed by one more word
//! (`tail`): the lane-3 sum reads that word. The layout stores the axes first and then reads the tail, so for the
//! definition's second frame the tail is the first axes' first entry.
//!
//! A normalise is the reciprocal square root estimate of the squared length ([`rsqrt`]), with the lanes summed as
//! `(x*x + y*y) + z*z`. The inverse origin's negation is a sign flip: the original multiplies by -1.0, which differs only
//! in the sign of a NaN (platform NaN, not compared).

use crate::math::rsqrt::rsqrt;
use crate::math::vec::Vec3;

/// The memory a frame build works on, as object handles: the origin, the direction (a quaternion: its xyz is read) and the
/// reference vector it reads, and the inverse origin, axes and inverse axes it writes (the last is also read one word past
/// its end).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxesHandles {
    pub origin: u32,
    pub direction: u32,
    pub reference: u32,
    pub inverse_origin: u32,
    pub axes: u32,
    pub inverse_axes: u32,
}

/// The unit vector of `v`: each lane times the reciprocal square root of `(x*x + y*y) + z*z`.
fn normalize(v: Vec3) -> Vec3 {
    v.scale(rsqrt(v.x * v.x + v.y * v.y + v.z * v.z))
}

fn to_array(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

/// The frame's rows (`q`, `m`, `n`) from the direction's xyz and the reference. The cross products keep the original's
/// operands: `n` is the cross of the raw direction and reference, and `m` is `n x q`.
pub fn axes(direction: [f32; 4], reference: [f32; 3]) -> [[f32; 3]; 3] {
    let direction = Vec3 { x: direction[0], y: direction[1], z: direction[2] };
    let reference = Vec3 { x: reference[0], y: reference[1], z: reference[2] };
    let q = normalize(direction);
    let n = normalize(direction.cross3(reference));
    let m = n.cross3(q);
    [to_array(q), to_array(m), to_array(n)]
}

/// The inverse placement of `axes` at `origin`: the inverse origin (`-(F origin)`, four lanes; `F` is the nine words of the
/// inverse axes and `tail`, so lane 3 reads `tail`) and the inverse axes (the transpose). Lane `k` is
/// `F[k] origin0 + F[k+3] origin1 + F[k+6] origin2` in the original's order.
pub fn inverse(origin: [f32; 3], axes: &[[f32; 3]; 3], tail: f32) -> ([f32; 4], [f32; 9]) {
    let inverse_axes: [f32; 9] = std::array::from_fn(|i| axes[i % 3][i / 3]);
    let words: [f32; 10] = std::array::from_fn(|i| if i < 9 { inverse_axes[i] } else { tail });
    let dot = |k: usize| words[k] * origin[0] + words[k + 3] * origin[1] + words[k + 6] * origin[2];
    ([-dot(0), -dot(1), -dot(2), -dot(3)], inverse_axes)
}
