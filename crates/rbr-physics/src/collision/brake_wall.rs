//! The brake wall's segment tests: the box a pair of corners queries, a segment against the wall's quad edges, and the
//! penetration of a point inside a wall segment. The sums keep the order their instructions gave.

use crate::fpu;

/// The box queried from two corners' `x` and `y`: the centre and the half extent, `[cx, cy, hx, hy]`, in the original's
/// operand order.
pub fn corners_box(a: [f32; 2], b: [f32; 2]) -> [f32; 4] {
    [(b[0] + a[0]) * 0.5, (a[1] + b[1]) * 0.5, (b[0] - a[0]) * 0.5, (b[1] - a[1]) * 0.5]
}

/// The widest box queried: a half extent above it on either axis is no overlap (the compare is unordered: a NaN extent
/// is queried).
const WALL_BOX_LIMIT: f32 = 10.0;

pub fn box_too_wide(half_x: f32, half_y: f32) -> bool {
    half_x > WALL_BOX_LIMIT || half_y > WALL_BOX_LIMIT
}

/// One segment's hit: the unit normal, the point's signed distance along the normal, and the penetration fraction (0
/// when the along-distance is not positive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentHit {
    pub normal: [f32; 2],
    pub signed_distance: f32,
    pub penetration: f32,
}

/// The inside test and the hit of one segment record: `v` are the four vertices read at the 8-byte stride from the
/// record's first index (`v[0]` to `v[3]`), `p` the point. The point must be on the inner side of the four edge
/// tests. The products and sums keep the original's SSE order.
pub fn segment_hit(v: [[f32; 2]; 4], p: [f32; 2]) -> Option<SegmentHit> {
    let [v0, v1, v2, v3] = v;
    let ex1 = v1[0] - v0[0];
    let ey1 = v1[1] - v0[1];
    let ey2 = v3[1] - v1[1];
    let nx = 0.0 - (v3[0] - v1[0]);
    let px = p[0] - v1[0];
    let py = p[1] - v1[1];
    let qx = p[0] - v2[0];
    let qy = p[1] - v2[1];
    let inside = 0.0 < py * (0.0 - ex1) + px * ey1
        && 0.0 < py * nx + px * ey2
        && 0.0 < qy * (0.0 - (v2[0] - v3[0])) + qx * (v2[1] - v3[1])
        && 0.0 < qy * (0.0 - (v0[0] - v2[0])) + qx * (v0[1] - v2[1]);
    if !inside {
        return None;
    }

    // The length is an x87 chain at the thread's precision (24 bits for 1.02, 53 in NGP's race steps): each square,
    // the sum and the root round to it (the store rounds to f32). The squares are rounded too: an exact sum is one ulp
    // off on the 1.02 brake wall.
    let round = fpu::round_x87;
    let squares = round(f64::from(nx) * f64::from(nx)) + round(f64::from(ey2) * f64::from(ey2));
    let nl = round(round(squares).sqrt()) as f32;
    let (nnx, nny) = if 1e-5 < nl {
        let inv = 1.0 / nl;
        (inv * ey2, inv * nx)
    } else {
        (ey2, nx)
    };
    let signed_distance = nny * py + nnx * px;
    let along = 0.0 - (nny * ey1 + nnx * ex1);
    let penetration = if !(along > 0.0) { 0.0 } else { 1.0 - signed_distance / along };
    Some(SegmentHit { normal: [nnx, nny], signed_distance, penetration })
}

/// The squared length below which a swept segment is too short to test.
const SWEEP_MIN_SQ: f32 = 0.1;
/// The edge length above which the edge's normal is normalised, and the cross term above which the segment is not
/// parallel to the edge.
const EDGE_MIN_LEN: f32 = 1e-5;
const SWEEP_MIN_CROSS: f32 = 1e-6;

/// The segment from `p0` to `p1`'s `x` and `y` extent, when it is long enough (an unordered length is tested, as
/// `comiss`/`ja` does).
pub fn sweep(p0: [f32; 3], p1: [f32; 3]) -> Option<[f32; 2]> {
    let d = [p1[0] - p0[0], p1[1] - p0[1]];
    if SWEEP_MIN_SQ > d[1] * d[1] + d[0] * d[0] {
        return None;
    }
    Some(d)
}

/// Where the segment crosses one wall edge: the point (its `z` along the segment), the edge's normal, the line's offset
/// (`0 - n·a`) and the segment fraction `t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeCross {
    pub point: [f32; 3],
    pub normal: [f32; 2],
    pub offset: f32,
    pub t: f32,
}

/// The crossing of the segment (`p0` to `p1`, `d` from [`sweep`]) with the edge from `a` to `b`, when the ends are
/// strictly on opposite sides and the crossing lies within the edge. The edge length and the cross term are x87 at
/// 24-bit precision, each operation rounded, which f32 arithmetic gives; the rest is SSE.
pub fn edge_cross(p0: [f32; 3], p1: [f32; 3], d: [f32; 2], a: [f32; 2], b: [f32; 2]) -> Option<EdgeCross> {
    let ex = b[0] - a[0];
    let ey = b[1] - a[1];
    let len_sq = ex * ex + ey * ey;
    let len = len_sq.sqrt();
    let normal = if len > EDGE_MIN_LEN {
        let inv = 1.0 / len;
        [inv * ey, inv * -ex]
    } else {
        [ey, -ex]
    };
    let cross = d[1] * normal[1] + d[0] * normal[0];
    if !(cross.abs() > SWEEP_MIN_CROSS) {
        return None;
    }
    let offset = 0.0 - (a[0] * normal[0] + a[1] * normal[1]);
    let s0 = p0[0] * normal[0] + p0[1] * normal[1] + offset;
    let s1 = p1[0] * normal[0] + p1[1] * normal[1] + offset;
    if !((s0 > 0.0 && 0.0 > s1) || (0.0 > s0 && s1 > 0.0)) {
        return None;
    }
    let t = 0.0 - s0 / cross;
    let x = p0[0] + d[0] * t;
    let y = d[1] * t + p0[1];
    let along = (y - a[1]) * ey + (x - a[0]) * ex;
    if !(along >= 0.0 && len_sq >= along) {
        return None;
    }
    Some(EdgeCross { point: [x, y, (p1[2] - p0[2]) * t + p0[2]], normal, offset, t })
}
