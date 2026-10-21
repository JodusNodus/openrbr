//! The car's value cells and their dirty flag, and the car's ground direction. `layout` reads and writes the words;
//! these are the values.

use super::layout::Real;
use crate::math::vec::Vec3;

/// An anti-roll bar's force as the wheel's strut takes it: negated for the bar's second side (exact).
pub fn signed_force(value: f32, negate: bool) -> f32 {
    if negate {
        -value
    } else {
        value
    }
}

/// Marks the cell changed: its dirty flag becomes 1.
pub fn set_dirty(cell: Real) -> Real {
    Real { dirty: 1, ..cell }
}

/// Adds `v` to the steering torque demand: the cell is dirtied, then `value += v` (single precision, the old value
/// first).
pub fn add_steer_torque(demand: Real, v: f32) -> Real {
    Real { value: demand.value + v, ..set_dirty(demand) }
}

/// Dirties the cell, then stores `v` in it.
pub fn set_cell(cell: Real, v: f32) -> Real {
    Real { value: v, ..set_dirty(cell) }
}

/// The car's direction in the ground plane. The sum of the wheel directions is `l` (the front wheel added, the rear
/// wheel subtracted, from zero); the direction is `(l.y, -l.x, 0)`, scaled to unit length. When its length is not above
/// 0.01 (NaN too), the result is `(0, -1, 0)`. The original also scales lane 3 of the caller's vector on the unit path
/// (and keeps it on the other); lane 3 is the caller's, not returned here.
pub fn ground_direction(front: Option<Vec3>, rear: Option<Vec3>) -> Vec3 {
    let mut l = Vec3::default();
    if let Some(f) = front {
        l = Vec3 { x: f.x + l.x, y: f.y + l.y, z: f.z + l.z };
    }
    if let Some(r) = rear {
        // The original multiplies the rear by -1 and adds, which is `l - r` bit for bit.
        l = Vec3 { x: l.x - r.x, y: l.y - r.y, z: l.z - r.z };
    }
    let dir = Vec3 { x: l.y, y: 0.0 - l.x, z: 0.0 };
    let len = dir.length3();
    if !(len > MIN_DIR_LENGTH) {
        return Vec3 { x: 0.0, y: -1.0, z: 0.0 };
    }
    dir.scale(1.0 / len)
}

/// The length, m, at or below which `ground_direction` returns the default direction.
const MIN_DIR_LENGTH: f32 = 0.01;
