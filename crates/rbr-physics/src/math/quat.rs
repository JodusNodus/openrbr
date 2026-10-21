//! Quaternions as the physics stores them: four floats, `w` last (`x, y, z, w`).

use super::rsqrt::rsqrt;
use super::Mat3;
use crate::fpu::{precision, Precision};

/// A quaternion (or any four-float vector the asm treats the same way).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

/// The quaternion product `p * q` (`NPhysics::E15Shell::M83DD0`, 0x483dd0). The asm reads both operands before it
/// writes the result, so `q` may be the output's own storage. Order (asm): the cross part is summed first, then
/// `p*q.w` is added to it, then `q*p.w`. The w component is `q.w*p.w - ((p.z*q.z + p.y*q.y) + q.x*p.x)`.
pub fn mul(p: Quat, q: Quat) -> Quat {
    let cross = [p.y * q.z - p.z * q.y, p.z * q.x - p.x * q.z, p.x * q.y - p.y * q.x];
    let lane = |p_i: f32, q_i: f32, cross_i: f32| (cross_i + p_i * q.w) + q_i * p.w;
    Quat {
        x: lane(p.x, q.x, cross[0]),
        y: lane(p.y, q.y, cross[1]),
        z: lane(p.z, q.z, cross[2]),
        w: q.w * p.w - ((p.z * q.z + p.y * q.y) + q.x * p.x),
    }
}

/// Scales the quaternion to unit length (`E18_QuatNormalize`, 0x4921a0, and `E34Quat::Norm`, 0x4e4c70, which have the
/// same math: the first writes in place, the second to an output). The squared length is summed as
/// `((x*x + y*y) + z*z) + w*w`, and each component is multiplied by the reference CPU's `rsqrtss` of it.
pub fn normalize(q: Quat) -> Quat {
    let sum = ((q.x * q.x + q.y * q.y) + q.z * q.z) + q.w * q.w;
    let r = rsqrt(sum);
    Quat { x: q.x * r, y: q.y * r, z: q.z * r, w: q.w * r }
}

/// `E34Quat::ToQuat`: the unit quaternion of `m`. The branch on the trace `t` (above 0) takes the three off-diagonal
/// differences; otherwise the largest diagonal `i` is used, with `j = i + 1` and `k = j + 1` (mod 3). NaN takes the
/// diagonal branch, as the asm's `jbe` does. The root's argument is an x87 chain: under NGP (53 bits) it and the root
/// round once, at the store of `s`.
pub fn to_quat(m: &Mat3) -> Quat {
    let t = (m[0][0] + m[2][2]) + m[1][1];
    if t > 0.0 {
        let s = match precision() {
            Precision::Pc24 => (t + 1.0).sqrt(),
            Precision::Pc53 => (f64::from(t) + 1.0).sqrt() as f32,
        };
        let inv = 0.5 / s;
        return Quat {
            x: (m[1][2] - m[2][1]) * inv,
            y: (m[2][0] - m[0][2]) * inv,
            z: (m[0][1] - m[1][0]) * inv,
            w: s * 0.5,
        };
    }
    let mut i = usize::from(m[0][0] < m[1][1]);
    if m[2][2] > m[i][i] {
        i = 2;
    }
    let j = (i + 1) % 3;
    let k = (j + 1) % 3;
    // x87 chain: (m[i][i] - (m[k][k] + m[j][j])) + 1, then sqrt; s is kept as an f32 word.
    let s = match precision() {
        Precision::Pc24 => ((m[i][i] - (m[k][k] + m[j][j])) + 1.0).sqrt(),
        Precision::Pc53 => {
            let d = |a: usize| f64::from(m[a][a]);
            ((d(i) - (d(k) + d(j))) + 1.0).sqrt() as f32
        }
    };
    // The asm divides only when s is not zero (`fucomip` with the zero, a NaN divides): else it keeps s.
    let inv = if s != 0.0 { 0.5 / s } else { s };
    let mut out = [0.0f32; 4];
    out[i] = s * 0.5;
    out[3] = (m[j][k] - m[k][j]) * inv;
    out[j] = (m[i][j] + m[j][i]) * inv;
    out[k] = (m[i][k] + m[k][i]) * inv;
    Quat { x: out[0], y: out[1], z: out[2], w: out[3] }
}

/// `E01Mat3::SetFromQuat` (0x43c6e0): the rotation matrix of `q` (not normalised). The products are formed as the asm
/// forms them (`xx[i] = x*q[i]`, `yy[i] = y*q[i]`, `zz[i] = z*q[i]`), then each entry is `1 - 2*(a + b)` or `2*(a - b)`.
/// The reference stores it row-major (`out[3*row + col]`).
pub fn from_quat(q: Quat) -> Mat3 {
    let (x, y, z) = (q.x, q.y, q.z);
    let xx = [x * q.x, x * q.y, x * q.z, x * q.w];
    let yy = [y * q.x, y * q.y, y * q.z, y * q.w];
    let zz = [z * q.x, z * q.y, z * q.z, z * q.w];
    [
        [1.0 - (zz[2] + yy[1]) * 2.0, (zz[3] + xx[1]) * 2.0, (xx[2] - yy[3]) * 2.0],
        [(xx[1] - zz[3]) * 2.0, 1.0 - (xx[0] + zz[2]) * 2.0, (xx[3] + yy[2]) * 2.0],
        [(yy[3] + xx[2]) * 2.0, (yy[2] - xx[3]) * 2.0, 1.0 - (xx[0] + yy[1]) * 2.0],
    ]
}
