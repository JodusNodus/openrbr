//! Math of the physics core on typed values: batch vector and matrix operations, force accumulation and the mounted
//! body's motion. Float sums are not associative, so where the original adds the terms in an order other than the
//! written one, that order is kept and commented.

pub use crate::math::vec::{Vec3, Vec4};

/// A 3x3 matrix stored as 10 floats. `rows[r][c]` is row `r`, column `c`. `extra` is the tenth float; only
/// `scale_rows` reads it (lane 3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat {
    pub rows: [[f32; 3]; 3],
    pub extra: f32,
}

/// Which of the two orders a product sum takes in `Mat::mul_rows`: `true` is `(p0 + p2) + p1`, `false` is
/// `(p2 + p1) + p0`. The original uses both (the output's row and column pick one), so the table is copied as it is.
const SUM_FIRST_PAIR_IS_X_Z: [[bool; 3]; 3] = [[true, false, false], [false, true, false], [true, true, true]];

impl Mat {
    /// Each row `b_k` of `b` becomes `b_k * M` (a row vector times M).
    pub fn mul_rows(&self, b: [Vec3; 3]) -> [Vec3; 3] {
        std::array::from_fn(|k| {
            let c = [b[k].x, b[k].y, b[k].z];
            let out = |j: usize| {
                let p = [c[0] * self.rows[0][j], c[1] * self.rows[1][j], c[2] * self.rows[2][j]];
                if SUM_FIRST_PAIR_IS_X_Z[k][j] {
                    (p[0] + p[2]) + p[1]
                } else {
                    (p[2] + p[1]) + p[0]
                }
            };
            Vec3 { x: out(0), y: out(1), z: out(2) }
        })
    }

    /// The three 4-lane rows that `scale_rows` multiplies by. They are read at stride 3 from the flat matrix (floats
    /// 0..3, 3..6, 6..9), so the row's 4th lane is the next row's first float, and the last row's 4th lane is `extra`.
    fn stride3_rows(&self) -> [Vec4; 3] {
        let m = self.rows;
        [
            Vec4 { x: m[0][0], y: m[0][1], z: m[0][2], w: m[1][0] },
            Vec4 { x: m[1][0], y: m[1][1], z: m[1][2], w: m[2][0] },
            Vec4 { x: m[2][0], y: m[2][1], z: m[2][2], w: self.extra },
        ]
    }

    /// Each row `v` becomes `v.x * r0 + v.y * r1 + v.z * r2` lane by lane, where `r0..r2` are the stride-3 rows. The sum
    /// is `(x*r0 + y*r1) + z*r2`, the original's order.
    pub fn scale_rows(&self, rows: &mut [Vec4]) {
        let [r0, r1, r2] = self.stride3_rows();
        for v in rows {
            let lane = |a: f32, b: f32, c: f32| (v.x * a + v.y * b) + v.z * c;
            *v = Vec4 {
                x: lane(r0.x, r1.x, r2.x),
                y: lane(r0.y, r1.y, r2.y),
                z: lane(r0.z, r1.z, r2.z),
                w: lane(r0.w, r1.w, r2.w),
            };
        }
    }

    /// The xyz of each row becomes `M * (x, y, z)` (a column vector); `w` is kept. Each sum is
    /// `(M[r][1]*y + M[r][2]*z) + M[r][0]*x`, the original's order.
    pub fn transform_xyz(&self, rows: &mut [Vec4]) {
        let m = self.rows;
        for v in rows {
            let out = |r: usize| (m[r][1] * v.y + m[r][2] * v.z) + m[r][0] * v.x;
            let (x, y, z) = (out(0), out(1), out(2));
            v.x = x;
            v.y = y;
            v.z = z;
        }
    }
}

/// `force[i] += v[i] * force_scale`, the four lanes. The original adds the product to the force (`v * k + force`), which
/// is the same bits in either order.
pub fn accumulate_force(force: &mut Vec4, v: Vec4, force_scale: f32) {
    force.x += v.x * force_scale;
    force.y += v.y * force_scale;
    force.z += v.z * force_scale;
    force.w += v.w * force_scale;
}

/// The gravity force on a body: `walked` is the gravity carried through the body's frame, scaled by
/// `1 / inverse_mass`. None when the inverse mass is 0, the case where the original makes no walk and no accumulate. The
/// caller adds the result to the force with `accumulate_force`.
pub fn gravity_force(walked: Vec4, inverse_mass: f32) -> Option<Vec4> {
    if inverse_mass == 0.0 {
        return None;
    }
    let scale = 1.0 / inverse_mass;
    Some(Vec4 { x: scale * walked.x, y: scale * walked.y, z: scale * walked.z, w: scale * walked.w })
}

/// The body terms the velocity of a point on the body reads: the position, the angular velocity, the lever term and the
/// base velocity (the last two named by use). Only the first three lanes of each are read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BodyMotion {
    pub position: Vec4,
    pub angular: Vec4,
    pub lever: Vec4,
    pub base: Vec4,
}

/// The velocity term of the point `p` on the body, `((w x w) x d + lever x d) + base` with `d = p - position`, three
/// lanes (the caller's fourth lane is not written). The sums keep the original's operand order.
pub fn point_velocity(body: &BodyMotion, p: Vec4) -> [f32; 3] {
    let d = Vec4 { x: p.x - body.position.x, y: p.y - body.position.y, z: p.z - body.position.z, w: 0.0 };
    let spin = body.angular.cross3(body.angular).cross3(d);
    let lever = body.lever.cross3(d);
    let sum = [spin.x + lever.x, lever.y + spin.y, lever.z + spin.z];
    [sum[0] + body.base.x, body.base.y + sum[1], body.base.z + sum[2]]
}

/// The pull velocity of a mounted body: the owner's pull negated, plus `-1000` times the gap between the body and its
/// mount point, plus `-10` times the body's linear velocity, lanewise over four lanes (the gap's fourth lane is the
/// original's stack word).
pub fn mounted_velocity(pull: Vec4, gap: Vec4, linear: Vec4) -> Vec4 {
    pull.scale(-1.0).add_scaled(gap, -1000.0).add_scaled(linear, -10.0)
}

/// The bounds the gap between a mounted body and its mount point is kept in (m), per lane (x, y, z).
pub const MOUNT_GAP_LOW: [f32; 3] = [-0.02, -0.02, -0.03];
pub const MOUNT_GAP_HIGH: [f32; 3] = [0.02, 0.005, 0.01];

/// The linear velocity and the position a mounted body has after its step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MountedStep {
    pub linear: Vec4,
    pub position: Vec4,
}

/// The original's clamp: the low bound when it is above `gap`, else the high bound when `gap` is above it, else `gap` (NaN
/// stays).
fn clamp_gap(gap: f32, low: f32, high: f32) -> f32 {
    if low > gap {
        low
    } else if gap > high {
        high
    } else {
        gap
    }
}

/// The step of a mounted body with an owner: the linear velocity takes `pull * dt` (`pull` is the pull velocity), the
/// position the new velocity times `dt`, four lanes each; then the gap between the position and the mount point is
/// clamped (`MOUNT_GAP_LOW`, `MOUNT_GAP_HIGH`) and added back to the mount point, three lanes (the fourth keeps the
/// integrated value). The sums keep the original's operand order.
pub fn mounted_step(linear: Vec4, pull: Vec4, position: Vec4, mount: Vec4, dt: f32) -> MountedStep {
    let linear = linear.add_scaled(pull, dt);
    let position = position.add_scaled(linear, dt);
    let gap = [position.x - mount.x, position.y - mount.y, position.z - mount.z];
    let clamped: [f32; 3] = std::array::from_fn(|k| clamp_gap(gap[k], MOUNT_GAP_LOW[k], MOUNT_GAP_HIGH[k]));
    let position = Vec4 { x: clamped[0] + mount.x, y: mount.y + clamped[1], z: mount.z + clamped[2], w: position.w };
    MountedStep { linear, position }
}
