//! Car accessors and the dirty-flag setters (batch A2). `layout` reads and writes the words; these are the values.

use super::layout::Real;
use crate::math::vec::Vec3;

/// `CCar::GetA48` (0x4e6730): the lane value, negated when `negate` (x87 `fchs`, exact).
pub fn lane_value(value: f32, negate: bool) -> f32 {
    if negate {
        -value
    } else {
        value
    }
}

/// `CBase::SetDirty` (0x52f450, the CReal's dirty flag at +8): sets it to 1.
pub fn set_dirty(cell: Real) -> Real {
    Real { dirty: 1, ..cell }
}

/// `CCar::AddADC` (0x4e6660): dirties the demand, then `value += v` (SSE `addss`, the old value first).
pub fn add_steer_torque(demand: Real, v: f32) -> Real {
    Real { value: demand.value + v, ..set_dirty(demand) }
}

/// `CCar::SetA1C` (0x4e66a0): dirties the cell, then stores `v` in it.
pub fn set_cell(cell: Real, v: f32) -> Real {
    Real { value: v, ..set_dirty(cell) }
}

/// `CCar::GetDir(out)` (0x4e5cf0, `ret 4`): the car's direction in the ground plane. The sum of the wheel directions is
/// `l` (wheel 0 added, wheel 1 subtracted, from zero); the direction is `(l.y, -l.x, 0)`, scaled to unit length. When its
/// length is not above 0.01 (NaN too, the asm's `jbe`), the result is `(0, -1, 0)`. The asm also scales lane 3 of the
/// caller's vector on the unit path (and keeps it on the other); lane 3 is the caller's, not returned here.
pub fn get_dir(front: Option<Vec3>, rear: Option<Vec3>) -> Vec3 {
    let mut l = Vec3::default();
    if let Some(f) = front {
        l = Vec3 { x: f.x + l.x, y: f.y + l.y, z: f.z + l.z };
    }
    if let Some(r) = rear {
        // x87-free: the asm multiplies the rear by -1 and adds, which is `l - r` bit for bit.
        l = Vec3 { x: l.x - r.x, y: l.y - r.y, z: l.z - r.z };
    }
    let dir = Vec3 { x: l.y, y: 0.0 - l.x, z: 0.0 };
    let len = dir.length3();
    if !(len > MIN_DIR_LENGTH) {
        return Vec3 { x: 0.0, y: -1.0, z: 0.0 };
    }
    dir.scale(1.0 / len)
}

/// The length below which `GetDir` returns the default direction (asm 0x72f440, 0.01).
const MIN_DIR_LENGTH: f32 = 0.01;
