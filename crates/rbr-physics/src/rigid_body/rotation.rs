//! The body's rotation matrix from its orientation, and, when the gate is positive, the product of that matrix
//! with the diagonal scale (a physics-core call, made through `MulHost`).

use super::host::HostUnavailable;
use crate::math::quat::{from_quat, Quat};
use crate::physics_core::math::{Mat, Vec3};

/// The host's matrix product: `out` of the matrix `m` (its tenth word is unused) with `b`, nine row-major floats
/// each.
pub trait MulHost {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], HostUnavailable>;
}

/// The host's product without the host: each row of `b` times `m` ([`Mat::mul_rows`]), so a diagonal `b` scales the
/// rows of `m`. For hosts that defer the product's callees but not the product.
pub fn mul_rows(m: [f32; 9], b: [f32; 9]) -> [f32; 9] {
    let m = Mat { rows: std::array::from_fn(|i| [m[3 * i], m[3 * i + 1], m[3 * i + 2]]), extra: 0.0 };
    let rows = m.mul_rows(std::array::from_fn(|k| Vec3 { x: b[3 * k], y: b[3 * k + 1], z: b[3 * k + 2] }));
    std::array::from_fn(|i| {
        let row = rows[i / 3];
        [row.x, row.y, row.z][i % 3]
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diagonal_product_scales_the_rows() {
        let m = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let d = [2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0];
        assert_eq!(mul_rows(m, d), [2.0, 4.0, 6.0, 12.0, 15.0, 18.0, 28.0, 32.0, 36.0]);
    }
}
