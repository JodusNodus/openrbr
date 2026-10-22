//! Oriented-box tests: a sphere or another oriented box against a box frame, with the contact each overlap gives.
//! Every sum keeps the order its instructions gave, and the comparisons keep their NaN behaviour.

use crate::math::vec::{Vec3, Vec4};

fn normalize_lane(value: f32, reciprocal: f32) -> f32 {
    if value == 0.0 && reciprocal.is_infinite() {
        // The reference SSE path emits its indefinite NaN with the sign bit set for 0 * infinity.
        f32::from_bits(0xffc0_0000)
    } else {
        value * reciprocal
    }
}

/// A box frame: the centre (its w lane is the centre's own), the half extents, and the three basis rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObbFrame {
    pub center: Vec4,
    pub extents: [f32; 3],
    pub basis: [[f32; 3]; 3],
}

/// The contact of a sphere with the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereContact {
    pub depth: f32,
    pub position: Vec4,
    pub normal: Vec4,
}

/// The contact of another oriented box with the box. The position's w lane is the slot's own, so it is not here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObbContact {
    pub depth: f32,
    pub position: Vec3,
    /// Named by use: the product of the extents, times 4 and the depth.
    pub metric: f32,
    pub normal: Vec4,
}

/// The sphere's offset from the box centre along the box axes, and the magnitude of each axis offset.
struct SphereOffset {
    delta: [f32; 3],
    axes: [f32; 3],
}

fn sphere_offset(frame: &ObbFrame, sphere: Vec4) -> SphereOffset {
    let dx = sphere.x - frame.center.x;
    let dy = sphere.y - frame.center.y;
    let dz = sphere.z - frame.center.z;
    let [x_axis, y_axis, z_axis] = frame.basis;
    let mut ax = dx * x_axis[0] + dy * x_axis[1] + dz * x_axis[2];
    let mut ay = dx * y_axis[0] + dy * y_axis[1] + dz * y_axis[2];
    let mut az = dx * z_axis[0] + dy * z_axis[1] + dz * z_axis[2];
    if ax < 0.0 {
        ax = -ax;
    }
    if ay < 0.0 {
        ay = -ay;
    }
    if az < 0.0 {
        az = -az;
    }
    SphereOffset { delta: [dx, dy, dz], axes: [ax, ay, az] }
}

/// The separation test: true when the sphere overlaps the box.
pub fn sphere_overlaps(frame: &ObbFrame, sphere: Vec4, radius: f32) -> bool {
    let [ax, ay, az] = sphere_offset(frame, sphere).axes;
    let extents = frame.extents;
    !(extents[0] + radius <= ax || extents[1] + radius <= ay || extents[2] + radius <= az)
}

/// The contact for an overlapping sphere: the depth, the position, and the normal. `normal_w` is the slot's
/// previous normal w, which the normalisation scales.
pub fn sphere_contact(frame: &ObbFrame, sphere: Vec4, radius: f32, normal_w: f32) -> SphereContact {
    let SphereOffset { delta: [dx, dy, dz], axes: [ax, ay, az] } = sphere_offset(frame, sphere);
    let center = frame.center;
    let extents = frame.extents;
    let (depth, position) = if extents[0] - ax <= radius || extents[1] - ay <= radius || extents[2] - az <= radius {
        let mut axis = usize::from(ax <= ay);
        let smallest = if axis == 0 { ax } else { ay };
        if smallest <= az && az != smallest {
            axis = 2;
        }
        let mut depth = 1.0 - ax / (extents[0] + radius);
        let y_depth = 1.0 - ay / (extents[1] + radius);
        if depth <= y_depth {
            depth = y_depth;
        }
        let mut z_depth = 1.0 - az / (extents[2] + radius);
        if z_depth <= depth {
            z_depth = depth;
        }

        // The normalisation uses the reference CPU's rsqrtss approximation here.
        let reciprocal = crate::math::rsqrt::rsqrt(dx * dx + dy * dy + dz * dz);
        let pick = extents[axis];
        let position = Vec4 {
            x: (dx * reciprocal) * pick + center.x,
            y: (dy * reciprocal) * pick + center.y,
            z: (dz * reciprocal) * pick + center.z,
            w: center.w,
        };
        (z_depth, position)
    } else {
        (1.0, sphere)
    };

    let offset = Vec3 { x: position.x - sphere.x, y: position.y - sphere.y, z: position.z - sphere.z };
    let length_sq = offset.x * offset.x + offset.y * offset.y + offset.z * offset.z;
    let normal = if 0.9 < length_sq {
        // This later normalization follows the source's explicit sqrt-and-divide path.
        let reciprocal = 1.0 / (f64::from(length_sq).sqrt() as f32);
        Vec4 { x: offset.x * reciprocal, y: offset.y * reciprocal, z: offset.z * reciprocal, w: normal_w * reciprocal }
    } else {
        Vec4 { x: 0.0, y: 0.0, z: 1.0, w: normal_w }
    };
    SphereContact { depth, position, normal }
}

/// The separating-axis tests for two oriented boxes: the other box's centre, half extents and three unit axes. True
/// when the boxes overlap.
#[allow(clippy::manual_range_contains)] // Ordered comparisons preserve the source's NaN branch behavior.
pub fn obb_overlaps(
    frame: &ObbFrame,
    other_center: Vec4,
    other_half: Vec4,
    axis_x: Vec4,
    axis_y: Vec4,
    axis_z: Vec4,
) -> bool {
    let center = frame.center;
    let extents = frame.extents;
    let basis = frame.basis;
    let bx = other_half.x;
    let by = other_half.y;
    let bz = other_half.z;
    let threshold = 0.999_998_986_721_038_8_f32;
    let l0 = center.x - other_center.x;
    let l1 = center.y - other_center.y;
    let l2 = center.z - other_center.z;
    let mut parallel = false;

    let c0 = axis_x.x * basis[0][0] + axis_x.y * basis[0][1] + axis_x.z * basis[0][2];
    let c1 = axis_x.x * basis[1][0] + basis[1][1] * axis_x.y + basis[1][2] * axis_x.z;
    let c2 = basis[2][0] * axis_x.x + basis[2][1] * axis_x.y + basis[2][2] * axis_x.z;
    let ax = c0.abs();
    let ay = c1.abs();
    let az = c2.abs();
    if threshold < ax || threshold < ay || threshold < az {
        parallel = true;
    }
    let fc = l0 * axis_x.x + l1 * axis_x.y + l2 * axis_x.z;
    let e1 = ax * extents[0];
    if fc.abs() > ay * extents[1] + az * extents[2] + e1 + bx {
        return false;
    }

    let dx0 = basis[0][0] * axis_y.x + basis[0][1] * axis_y.y + basis[0][2] * axis_y.z;
    let dx1 = basis[1][0] * axis_y.x + basis[1][1] * axis_y.y + basis[1][2] * axis_y.z;
    let dx2 = basis[2][0] * axis_y.x + basis[2][1] * axis_y.y + basis[2][2] * axis_y.z;
    let ex = dx0.abs();
    let ey = dx1.abs();
    let ez = dx2.abs();
    if threshold < ex || threshold < ey || threshold < ez {
        parallel = true;
    }
    let fd = l0 * axis_y.x + l1 * axis_y.y + l2 * axis_y.z;
    let e2 = ey * extents[1];
    if fd.abs() > ez * extents[2] + ex * extents[0] + e2 + by {
        return false;
    }

    let ex0 = basis[0][0] * axis_z.x + basis[0][1] * axis_z.y + basis[0][2] * axis_z.z;
    let ex1 = basis[1][0] * axis_z.x + basis[1][1] * axis_z.y + basis[1][2] * axis_z.z;
    let ex2 = axis_z.x * basis[2][0] + basis[2][1] * axis_z.y + basis[2][2] * axis_z.z;
    let fx = ex0.abs();
    let fy = ex1.abs();
    let fz = ex2.abs();
    if threshold < fx || threshold < fy || threshold < fz {
        parallel = true;
    }
    let fe = l0 * axis_z.x + l1 * axis_z.y + l2 * axis_z.z;
    let e3 = fz * extents[2];
    if fe.abs() > fy * extents[1] + fx * extents[0] + e3 + bz {
        return false;
    }

    let e4 = fx * extents[2];
    let e5 = ex * extents[1];
    let da = l0 * basis[0][0] + l1 * basis[0][1] + l2 * basis[0][2];
    if da.abs() > e5 + e4 + e1 + bx {
        return false;
    }
    let e6 = ay * extents[0];
    let e7 = fy * extents[2];
    let db = l0 * basis[1][0] + l1 * basis[1][1] + l2 * basis[1][2];
    if db.abs() > e6 + e7 + e2 + by {
        return false;
    }
    let e8 = ez * extents[1];
    let e9 = az * extents[0];
    let dc = l0 * basis[2][0] + l1 * basis[2][1] + l2 * basis[2][2];
    if dc.abs() > e9 + e8 + e3 + bz {
        return false;
    }

    if !parallel {
        let x1 = fe * dx0 - ex0 * fd;
        let x2 = fe * dx1 - ex1 * fd;
        let x3 = fe * dx2 - ex2 * fd;
        let x4 = ex0 * fc - fe * c0;
        let x5 = ex1 * fc - fe * c1;
        let x6 = ex2 * fc - fe * c2;
        let x7 = fd * c0 - dx0 * fc;
        let x8 = fd * c1 - dx1 * fc;
        let x9 = fd * c2 - dx2 * fc;
        if x1.abs() > ay * extents[2] + az * extents[1] + fx * by + ex * bz {
            return false;
        }
        if x2.abs() > ax * extents[2] + e9 + fy * by + ey * bz {
            return false;
        }
        if x3.abs() > ax * extents[1] + e6 + fz * by + ez * bz {
            return false;
        }
        if x4.abs() > ey * extents[2] + e8 + ax * bz + fx * bx {
            return false;
        }
        if x5.abs() > ex * extents[2] + ez * extents[0] + ay * bz + fy * bx {
            return false;
        }
        if x6.abs() > ey * extents[0] + e5 + fz * bx + az * bz {
            return false;
        }
        if x7.abs() > fz * extents[1] + e7 + ax * by + ex * bx {
            return false;
        }
        if x8.abs() > fz * extents[0] + e4 + ay * by + ey * bx {
            return false;
        }
        if x9.abs() > fx * extents[1] + fy * extents[0] + az * by + ez * bx {
            return false;
        }
    }
    true
}

/// The contact for overlapping boxes: the depth, the position, the metric, and the normal. `normal_w` is the
/// slot's previous normal w, which the normalisation scales.
#[allow(clippy::manual_range_contains)] // Ordered comparisons preserve the source's NaN branch behavior.
pub fn obb_contact(frame: &ObbFrame, other_center: Vec4, other_half: Vec4, normal_w: f32) -> ObbContact {
    let center = frame.center;
    let extents = frame.extents;
    let bx = other_half.x;
    let by = other_half.y;
    let bz = other_half.z;
    let l0 = center.x - other_center.x;
    let l1 = center.y - other_center.y;
    let l2 = center.z - other_center.z;
    let length_ax = extents[0] + bx;
    let length_ay = extents[1] + by;
    let length_az = extents[2] + bz;
    let length_a = (length_ax * length_ax + length_ay * length_ay + length_az * length_az).sqrt();
    let to_center = [other_center.x - center.x, other_center.y - center.y, other_center.z - center.z];
    let length_b = (to_center[0] * to_center[0] + to_center[1] * to_center[1] + to_center[2] * to_center[2]).sqrt();
    let depth = if length_a <= 0.0 {
        0.5
    } else {
        let ratio = length_b / length_a;
        let clamped = if ratio <= 1.0 { ratio } else { 1.0 };
        1.0 - clamped
    };

    let scaled = [l0 * depth, l1 * depth, l2 * depth];
    let position = Vec3 { x: center.x - scaled[0], y: center.y - scaled[1], z: center.z - scaled[2] };
    let metric = extents[0] * extents[2] * extents[1] * 4.0 * depth;

    let mut normal =
        Vec3 { x: position.x - other_center.x, y: position.y - other_center.y, z: position.z - other_center.z };
    let normal_length_sq = normal.x * normal.x + normal.y * normal.y + normal.z * normal.z;
    let mut reciprocal = crate::math::rsqrt::rsqrt(normal_length_sq);
    normal = Vec3 {
        x: normalize_lane(normal.x, reciprocal),
        y: normalize_lane(normal.y, reciprocal),
        z: normalize_lane(normal.z, reciprocal),
    };
    let mut w = normalize_lane(normal_w, reciprocal);
    let n2 = normal.z * normal.z + normal.y * normal.y + normal.x * normal.x;
    if n2 < 0.95 || 1.1 < n2 {
        normal = Vec3 { x: center.x - other_center.x, y: center.y - other_center.y, z: center.z - other_center.z };
        let fallback_length_sq = normal.x * normal.x + normal.y * normal.y + normal.z * normal.z;
        reciprocal = crate::math::rsqrt::rsqrt(fallback_length_sq);
        normal = Vec3 {
            x: normalize_lane(normal.x, reciprocal),
            y: normalize_lane(normal.y, reciprocal),
            z: normalize_lane(normal.z, reciprocal),
        };
        w = normalize_lane(w, reciprocal);
    }
    ObbContact { depth, position, metric, normal: Vec4 { x: normal.x, y: normal.y, z: normal.z, w } }
}
