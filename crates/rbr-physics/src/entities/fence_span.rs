//! Typed operations for `E90Seg::Init` (0x605b60). The two normalisations are the D3DX calls the layout makes.

use crate::math::vec::Vec3;

/// The span's plane `m_plane` for the normal `n` through the dummy's position `p`: `n` and `-(p . n)`, with `p.y`
/// lifted by 0.05. The sum is `((p.z n.z + p.x n.x) + n.y p.y')`, as the original adds it.
pub fn plane(n: Vec3, p: Vec3) -> [f32; 4] {
    let lifted = p.y + 0.05;
    let sum = (p.z * n.z + p.x * n.x) + n.y * lifted;
    [n.x, n.y, n.z, 0.0 - sum]
}
