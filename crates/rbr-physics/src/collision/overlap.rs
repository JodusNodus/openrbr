//! Overlap tests between axis-aligned boxes, segments and bounds: the cheap rejection tests before a contact search.

use crate::math::vec::{Vec3, Vec4};

/// True only when each of the first three lanes is strictly below zero. The fourth lane is not read.
pub fn all_lanes_negative(value: Vec4) -> bool {
    value.x < 0.0 && value.y < 0.0 && value.z < 0.0
}

/// Whether the box from `first_corner` to `second_corner` overlaps the bounds `lower`..`upper`: the first corner must
/// be strictly below the upper bound and the second strictly above the lower bound on every axis. The corners are
/// named by use: nothing orders them.
pub fn corners_overlap_bounds(lower: Vec4, upper: Vec4, first_corner: Vec4, second_corner: Vec4) -> bool {
    first_corner.x - upper.x < 0.0
        && first_corner.y - upper.y < 0.0
        && first_corner.z - upper.z < 0.0
        && lower.x - second_corner.x < 0.0
        && lower.y - second_corner.y < 0.0
        && lower.z - second_corner.z < 0.0
}

/// Whether the segment from `start` to `end` overlaps the box at `box_center` with half extents `box_half_extent`: the
/// three world axes, then the three cross axes of the segment. The fourth lanes are computed by the packed
/// operations but never used.
pub fn segment_overlaps_box(start: Vec4, end: Vec4, box_center: Vec4, box_half_extent: Vec4) -> bool {
    // Keep the packed subtract, half-scale, center and extent subtract as separate f32 steps.
    let half = Vec4 {
        x: (end.x - start.x) * 0.5,
        y: (end.y - start.y) * 0.5,
        z: (end.z - start.z) * 0.5,
        w: (end.w - start.w) * 0.5,
    };
    let separation = Vec4 {
        x: (start.x + half.x) - box_center.x,
        y: (start.y + half.y) - box_center.y,
        z: (start.z + half.z) - box_center.z,
        w: 0.0,
    };
    let (adx, ady, adz) = (half.x.abs(), half.y.abs(), half.z.abs());

    if abs_greater(separation.x, adx + box_half_extent.x)
        || abs_greater(separation.y, ady + box_half_extent.y)
        || abs_greater(separation.z, adz + box_half_extent.z)
    {
        return false;
    }

    let cross = [
        half.y * separation.z - half.z * separation.y,
        half.z * separation.x - half.x * separation.z,
        half.x * separation.y - half.y * separation.x,
    ];
    !(cross[0].abs() > ady * box_half_extent.z + adz * box_half_extent.y)
        && !(cross[1].abs() > adz * box_half_extent.x + adx * box_half_extent.z)
        && !(cross[2].abs() > ady * box_half_extent.x + adx * box_half_extent.y)
}

fn abs_greater(a: f32, b: f32) -> bool {
    a.abs() > b
}

/// A box for the separating-axis tests: its centre and its half extents.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SatBox {
    pub center: Vec3,
    pub half_extent: Vec3,
}

/// The box of the segment from `a` to `b`: the midpoint, and half the span as extents.
pub fn segment_box(a: Vec3, b: Vec3) -> SatBox {
    SatBox { center: (b + a).scale(0.5), half_extent: (b - a).scale(0.5) }
}

/// The three world-axis separation tests. True when no world axis separates the boxes.
pub fn world_axes_overlap(this: SatBox, other_center: Vec3, other_extent: Vec3) -> bool {
    let delta = other_center - this.center;
    let overlaps = |d: f32, half: f32, extent: f32| f64::from(d.abs()) <= f64::from(half) + f64::from(extent);
    overlaps(delta.x, this.half_extent.x, other_extent.x)
        && overlaps(delta.y, this.half_extent.y, other_extent.y)
        && overlaps(delta.z, this.half_extent.z, other_extent.z)
}

/// The world and cross-axis separating-axis tests for two boxes. True when no axis separates them.
pub fn box_sat_overlap(this: SatBox, other_center: Vec3, other_extent: Vec3) -> bool {
    let a = [other_extent.x.abs(), other_extent.y.abs(), other_extent.z.abs()];
    let delta = (other_center + other_extent) - this.center;
    let half = this.half_extent;
    let world = [(delta.x, a[0], half.x), (delta.y, a[1], half.y), (delta.z, a[2], half.z)];
    for (d, extent, h) in world {
        if f64::from(d.abs()) > f64::from(extent) + f64::from(h) {
            return false;
        }
    }

    let cross = other_extent.cross3(delta);
    let limits = [
        f64::from(a[1]) * f64::from(half.z) + f64::from(a[2]) * f64::from(half.y),
        f64::from(a[0]) * f64::from(half.z) + f64::from(a[2]) * f64::from(half.x),
        f64::from(a[0]) * f64::from(half.y) + f64::from(a[1]) * f64::from(half.x),
    ];
    let cross = [cross.x, cross.y, cross.z];
    (0..3).all(|axis| !(f64::from(cross[axis].abs()) > limits[axis]))
}
