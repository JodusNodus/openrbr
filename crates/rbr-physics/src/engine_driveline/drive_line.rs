//! The stage's drive line: the road's centre line as control points (position, tangent, distance along the road),
//! sampled as a cubic Hermite spline. The pure parts are here; the table in memory is `layout::drive_line`.

use crate::fpu;
use crate::math::vec::Vec4;

/// The table's capacity in control points.
pub const MAX_POINTS: u32 = 10000;

const ZERO: Vec4 = Vec4 { x: 0.0, y: 0.0, z: 0.0, w: 0.0 };

/// One control point: its position, the tangent to the next point, the distance along the road, and a tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveLinePoint {
    pub position: Vec4,
    pub tangent: Vec4,
    pub distance: f32,
    pub tag: u32,
}

/// `acc + v * s`, lane by lane: a multiply then an add (two f32 roundings, no fused multiply-add).
fn scale_add(s: f32, v: Vec4, acc: Vec4) -> Vec4 {
    Vec4 { x: v.x * s + acc.x, y: v.y * s + acc.y, z: v.z * s + acc.z, w: v.w * s + acc.w }
}

/// The cubic Hermite spline from `p0` to `p1` with the tangents `v0` and `v1`, at `t` in `0..1`:
/// `p0 + v0 t + c2 t^2 + c3 t^3`, with `c2 = -3 p0 + 3 p1 - 2 v0 - v1` and `c3 = 2 p0 - 2 p1 + v0 + v1`.
pub fn hermite(p0: Vec4, v0: Vec4, p1: Vec4, v1: Vec4, t: f32) -> Vec4 {
    let c3 = [(2.0, p0), (-2.0, p1), (1.0, v0), (1.0, v1)].into_iter().fold(ZERO, |acc, (s, v)| scale_add(s, v, acc));
    let c2 = [(-3.0, p0), (3.0, p1), (-2.0, v0), (-1.0, v1)].into_iter().fold(ZERO, |acc, (s, v)| scale_add(s, v, acc));
    let mut power = t * t;
    let out = scale_add(t, v0, p0);
    let out = scale_add(power, c2, out);
    power *= t;
    scale_add(power, c3, out)
}

/// The segment index and the fraction within it for the position `x` along the control points. `x` at or above 2^31
/// uses the index `0x7fffffff`. NaN and values below -2^31 (which the original logs) take segment 0 and fraction 0,
/// as do negative integer parts.
pub fn segment_of(x: f32) -> (i32, f32) {
    const TWO_31: f32 = 2_147_483_648.0;
    if x >= TWO_31 {
        (0x7fff_ffff, x - TWO_31)
    } else if !(x >= -TWO_31) {
        (0, 0.0)
    } else {
        let i = fpu::trunc_i32(f64::from(x));
        if i < 0 {
            (0, 0.0)
        } else {
            (i, x - i as f32)
        }
    }
}

/// The distance at the position `x` along `n` control points whose distances are `distance(k)`: the point at
/// `idx = floor(x)` and the next one, interpolated by `x - idx` (f32 at each step); the last point gives its own
/// distance. None when `idx` is outside `0..n` (or x is NaN): the original calls its assert there, so a port
/// declines.
pub fn interpolate_distance(n: u32, distance: impl Fn(u32) -> f32, x: f32) -> Option<f32> {
    let floor = x.floor();
    if !(floor >= 0.0 && floor < n as f32) {
        return None;
    }
    let idx = floor as u32;
    let first = distance(idx);
    if idx + 1 >= n {
        return Some(first);
    }
    let frac = (x as f64 - idx as f64) as f32;
    Some(first + (distance(idx + 1) - first) * frac)
}
