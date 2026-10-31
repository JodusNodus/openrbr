//! The body's rotation matrix from its orientation, and, when the gate is positive, the product of that matrix
//! with the diagonal scale (a physics-core call, made through `MulHost`).

use super::host::HostUnavailable;
use crate::math::quat::{from_quat, Quat};

/// The host's matrix product: `out` of the matrix `m` (its tenth word is unused) with `b`, nine row-major floats
/// each.
pub trait MulHost {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], HostUnavailable>;
}

/// The rotation matrix of `q`, times `diag(diag[0], diag[1], diag[2])` through `host` when `gate > 0`. Row-major, as
/// the state stores it.
pub fn refresh_rotation(
    q: Quat,
    gate: f32,
    diag: [f32; 3],
    host: &mut impl MulHost,
) -> Result<[f32; 9], HostUnavailable> {
    let m = from_quat(q);
    let flat = [m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2]];
    if gate > 0.0 {
        return host.mul(flat, [diag[0], 0.0, 0.0, 0.0, diag[1], 0.0, 0.0, 0.0, diag[2]]);
    }
    Ok(flat)
}
