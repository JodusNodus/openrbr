//! E33 collision vector leaves.

use crate::math::vec::Vec4;

/// Returns true only when each of the first three lanes is strictly below zero. The fourth lane is not read. (0x4e07b0)
pub fn all_below_zero(value: Vec4) -> bool {
    value.x < 0.0 && value.y < 0.0 && value.z < 0.0
}

/// The original's axis-aligned and cross-axis box test. `w` is computed by the packed operation but not used. (0x4e0e40)
pub fn boxes_overlap(a: Vec4, b: Vec4, extents: Vec4, tolerance: Vec4) -> bool {
    // Keep the packed subtract, half-scale, center and extent subtract as separate f32 steps.
    let half = Vec4 { x: (b.x - a.x) * 0.5, y: (b.y - a.y) * 0.5, z: (b.z - a.z) * 0.5, w: (b.w - a.w) * 0.5 };
    let separation =
        Vec4 { x: (a.x + half.x) - extents.x, y: (a.y + half.y) - extents.y, z: (a.z + half.z) - extents.z, w: 0.0 };
    let (adx, ady, adz) = (half.x.abs(), half.y.abs(), half.z.abs());

    if abs_greater(separation.x, adx + tolerance.x)
        || abs_greater(separation.y, ady + tolerance.y)
        || abs_greater(separation.z, adz + tolerance.z)
    {
        return false;
    }

    let cross = [
        half.y * separation.z - half.z * separation.y,
        half.z * separation.x - half.x * separation.z,
        half.x * separation.y - half.y * separation.x,
    ];
    !(cross[0].abs() > ady * tolerance.z + adz * tolerance.y)
        && !(cross[1].abs() > adz * tolerance.x + adx * tolerance.z)
        && !(cross[2].abs() > ady * tolerance.x + adx * tolerance.y)
}

fn abs_greater(a: f32, b: f32) -> bool {
    a.abs() > b
}
