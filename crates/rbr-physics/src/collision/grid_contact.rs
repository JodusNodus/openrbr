//! The contact arithmetic of a body's grid against another entity's, for point contacts (a grid point on a face) and
//! edge contacts (an edge pair): the midpoint of two edge hits, the probe direction, the contact plane, and the
//! overlap box of an edge-pair search. Names ours.

use crate::math::sse;
use crate::math::vec::{Vec3, Vec4};

/// Which contact a pair of edge hits gives: kind 4 for a grid point, kind 5 for a grid edge (the event kind).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactKind {
    Point = 4,
    Edge = 5,
}

/// The midpoint of the two edge hits, `(second + first) * 0.5` per lane (`addss`, then `mulss`).
pub fn midpoint(first: Vec3, second: Vec3) -> Vec3 {
    let half = |a: f32, b: f32| sse::mul(sse::add(a, b), 0.5);
    Vec3 { x: half(second.x, first.x), y: half(second.y, first.y), z: half(second.z, first.z) }
}

/// The direction the probe steps from the midpoint, before it is normalised: away from the other grid's centre `back`
/// when `away`, else from the midpoint toward this grid's centre `from`. A point contact's direction is flat (z = 0);
/// an edge contact's loses z only when x or y is greater than it.
pub fn probe_direction(mid: Vec3, back: Vec3, from: Vec3, away: bool, kind: ContactKind) -> Vec3 {
    let (a, b) = if away { (mid, back) } else { (from, mid) };
    let mut dir = Vec3 { x: sse::sub(a.x, b.x), y: sse::sub(a.y, b.y), z: sse::sub(a.z, b.z) };
    let drop_z = match kind {
        ContactKind::Point => true,
        ContactKind::Edge => dir.x > dir.z || dir.y > dir.z,
    };
    if drop_z {
        dir.z = 0.0;
    }
    dir
}

/// The plane offset of the contact normal `dir` through `hit`: `0 - ((d.x h.x + d.z h.z) + d.y h.y)`.
pub fn plane_offset(dir: Vec3, hit: Vec3) -> f32 {
    let sum = sse::add(sse::add(sse::mul(dir.x, hit.x), sse::mul(dir.z, hit.z)), sse::mul(dir.y, hit.y));
    sse::sub(0.0, sum)
}

/// Where the hit lies along the edge: `hit_length / edge_length`, at most 1, and 0 for an edge that is not longer than
/// 0 (or NaN; the x87 compare is unordered then).
pub fn edge_ratio(edge_length: f32, hit_length: f32) -> f32 {
    if edge_length > 0.0 {
        let ratio = sse::div(hit_length, edge_length);
        if ratio > 1.0 {
            1.0
        } else {
            ratio
        }
    } else {
        0.0
    }
}

/// The overlap box of an edge-pair search `[lo, hi]` as a centre and a half extent: `(hi + lo) * 0.5` and
/// `(hi - lo) * 0.5`. The original sums (`addss`, `subss`) the three first lanes into locals whose fourth lane it never writes, then scales all four by
/// 0.5 (`mulps`), so each `w` is that local's stale lane (`centre_w`, `half_w`) halved.
pub fn overlap_centre_half(lo: Vec4, hi: Vec4, centre_w: f32, half_w: f32) -> (Vec4, Vec4) {
    let halved = |x: f32, y: f32, z: f32, w: f32| Vec4 {
        x: sse::mul(0.5, x),
        y: sse::mul(0.5, y),
        z: sse::mul(0.5, z),
        w: sse::mul(0.5, w),
    };
    let centre = halved(sse::add(hi.x, lo.x), sse::add(hi.y, lo.y), sse::add(hi.z, lo.z), centre_w);
    let half = halved(sse::sub(hi.x, lo.x), sse::sub(hi.y, lo.y), sse::sub(hi.z, lo.z), half_w);
    (centre, half)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    #[test]
    fn point_direction_is_flat_and_edge_keeps_a_rising_z() {
        let (mid, back, from) = (v(2.0, 3.0, 4.0), v(1.0, 1.0, 1.0), v(0.0, 0.0, 9.0));
        assert_eq!(probe_direction(mid, back, from, true, ContactKind::Point), v(1.0, 2.0, 0.0));
        assert_eq!(probe_direction(mid, back, from, false, ContactKind::Edge), v(-2.0, -3.0, 5.0));
        assert_eq!(probe_direction(mid, back, from, true, ContactKind::Edge), v(1.0, 2.0, 3.0));
        assert_eq!(probe_direction(v(5.0, 3.0, 4.0), back, from, true, ContactKind::Edge), v(4.0, 2.0, 0.0));
    }

    #[test]
    fn plane_offset_and_ratio() {
        assert_eq!(plane_offset(v(0.0, 0.0, 1.0), v(5.0, 6.0, 2.0)), -2.0);
        assert_eq!(plane_offset(v(1.0, 0.0, 0.0), v(0.0, 6.0, 2.0)).to_bits(), 0);
        assert_eq!(edge_ratio(2.0, 1.0), 0.5);
        assert_eq!(edge_ratio(2.0, 3.0), 1.0);
        assert_eq!(edge_ratio(0.0, 3.0), 0.0);
        assert_eq!(edge_ratio(f32::NAN, 3.0), 0.0);
        assert_eq!(midpoint(v(1.0, 2.0, 3.0), v(3.0, 2.0, 1.0)), v(2.0, 2.0, 2.0));
    }

    #[test]
    fn overlap_box_halves_all_four_lanes() {
        let lo = Vec4 { x: -1.0, y: 2.0, z: 0.0, w: 9.0 };
        let hi = Vec4 { x: 3.0, y: 4.0, z: 1.0, w: 7.0 };
        let (centre, half) = overlap_centre_half(lo, hi, 6.0, -2.0);
        assert_eq!(centre, Vec4 { x: 1.0, y: 3.0, z: 0.5, w: 3.0 });
        assert_eq!(half, Vec4 { x: 2.0, y: 1.0, z: 0.5, w: -1.0 });
    }
}
