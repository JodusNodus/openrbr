//! Brake-wall query helpers from E33.

use crate::fpu;
use crate::mem::Mem;

/// CBrakeWall::Vf88 (0x4e38e0) (name ours): the wall reports itself as available.
pub const fn is_available() -> u32 {
    1
}

/// CBrakeWall::Vf110 (0x4e3860) (name ours): the wall reports itself as active.
pub const fn is_active() -> u32 {
    1
}

/// CBrakeWall::Vf11C (0x4e3870) (name ours): copy the inline root node's horizontal bounds
/// into two four-lane vectors, preserving the retail store order and W lanes.
pub fn copy_root_bounds(mem: &mut Mem, wall: u32, lower: u32, upper: u32) {
    let min_x = mem.f32(wall + 0x64);
    let min_y = mem.f32(wall + 0x68);
    let max_x = mem.f32(wall + 0x6c);
    let max_y = mem.f32(wall + 0x70);

    mem.set_f32(lower + 4, min_y - max_y);
    mem.set_f32(lower + 8, -999_999.0);
    mem.set_f32(lower, min_x - max_x);
    mem.set_f32(upper + 4, max_y + min_y);
    mem.set_f32(upper + 8, 999_999.0);
    mem.set_f32(upper, min_x + max_x);
}

fn query(mem: &mut Mem, wall: u32, shape: u32, collector: u32, box_query: bool) -> u32 {
    let mut stack = [0u32; 257];
    stack[1] = wall + 0x64;
    mem.set_u32(collector + 0x190, 0);
    let mut top = 1usize;

    loop {
        let node = stack[top];
        let mut next = top - 1;
        if mem.u32(node + 0x10) == 0 {
            let children = mem.u32(node + 0x14);
            if children != 0 {
                if child_hit(mem, shape, children, box_query) {
                    stack[top] = children;
                    next = top;
                }
                let second = children + 0x18;
                if child_hit(mem, shape, second, box_query) {
                    stack[next + 1] = second;
                    next += 1;
                }
            }
        } else {
            let count = mem.u32(collector + 0x190);
            mem.set_u32(collector + count * 4, node);
            mem.set_u32(collector + 0x190, count + 1);
        }

        if next == 0 {
            break;
        }
        top = next;
    }
    u32::from(mem.u32(collector + 0x190) != 0)
}

fn child_hit(mem: &Mem, shape: u32, child: u32, box_query: bool) -> bool {
    let dx = (mem.f32(shape) - mem.f32(child)).abs();
    let dy = (mem.f32(shape + 4) - mem.f32(child + 4)).abs();
    if box_query {
        dx < mem.f32(child + 8) + mem.f32(shape + 8) && dy < mem.f32(child + 12) + mem.f32(shape + 12)
    } else {
        dx < mem.f32(child + 8) && dy < mem.f32(child + 12)
    }
}

fn query_any(mem: &Mem, wall: u32, shape: [f32; 4]) -> bool {
    let mut stack = [0u32; 257];
    stack[1] = wall + 0x64;
    let mut top = 1usize;
    let mut found = false;

    loop {
        let node = stack[top];
        let mut next = top - 1;
        if mem.u32(node + 0x10) == 0 {
            let children = mem.u32(node + 0x14);
            if children != 0 {
                if child_hit_values(mem, shape, children) {
                    stack[top] = children;
                    next = top;
                }
                let second = children + 0x18;
                if child_hit_values(mem, shape, second) {
                    stack[next + 1] = second;
                    next += 1;
                }
            }
        } else {
            found = true;
        }

        if next == 0 {
            break;
        }
        top = next;
    }
    found
}

fn child_hit_values(mem: &Mem, shape: [f32; 4], child: u32) -> bool {
    (shape[0] - mem.f32(child)).abs() < mem.f32(child + 8) + shape[2]
        && (shape[1] - mem.f32(child + 4)).abs() < mem.f32(child + 12) + shape[3]
}

/// CBrakeWall::QueryPoint (0x4e3dc0): collect leaf nodes containing a point.
pub fn query_point(mem: &mut Mem, wall: u32, point: u32, collector: u32) -> u32 {
    query(mem, wall, point, collector, false)
}

/// CBrakeWall::QueryBox (0x4e3f10): collect leaf nodes overlapping a box.
pub fn query_box(mem: &mut Mem, wall: u32, bounds: u32, collector: u32) -> u32 {
    query(mem, wall, bounds, collector, true)
}

/// CBrakeWall::Vf15C (0x4e4070) (name ours): query the wall with a ten-unit box centered
/// at the point's X/Y coordinates.
pub fn query_xy_box(mem: &Mem, wall: u32, point: u32) -> u32 {
    let bounds = [mem.f32(point), mem.f32(point + 4), 10.0, 10.0];
    u32::from(query_any(mem, wall, bounds))
}

/// CBrakeWall::AddCount (0x4e3810): relocate the relative pointers in a
/// serialized wall quadtree using the data block's base address as the delta.
pub fn add_count(mem: &mut Mem, wall: u32, node: u32, delta: u32) {
    let _ = wall;
    let mut current = node;
    let mut items = mem.u32(current + 0x10);
    loop {
        if items != 0 {
            let relative = mem.u32(current + 0x14);
            mem.set_u32(current + 0x14, relative.wrapping_add(delta));
            return;
        }
        let relative_child = mem.u32(current + 0x14);
        if relative_child == 0 {
            break;
        }
        let child = relative_child.wrapping_add(delta);
        mem.set_u32(current + 0x14, child);
        add_count(mem, wall, child, delta);
        items = mem.u32(child + 0x28);
        current = child + 0x18;
    }
}

/// The calls `E34WallVf::VfCC` (0x4e42f0) makes, each at its callee's entry stack pointer `sp`: the wall's slot 12
/// (`Vf30`, its frame), `E33Mat34::FetchX` (0x51b6c0, thiscall on a corner with `(out, frame)`) and
/// `CBrakeWall::QueryBox` (0x4e3f10, thiscall on the wall with `(bounds, collector)`).
pub trait WallVfCcHost {
    fn frame(&mut self, sp: u32, wall: u32) -> u32;
    fn fetch_x(&mut self, sp: u32, corner: u32, out: u32, frame: u32);
    fn query_box(&mut self, sp: u32, wall: u32, bounds: u32, collector: u32) -> u32;
}

/// The call `CBrakeWall::VfF0` (0x4e3970) makes at its callee's entry stack pointer `sp`: the wall's slot 0xe8 (`VfE8`)
/// with `(a, b, 0, out, 1)`, which fills the four vectors at `out` on a hit (eax non-zero).
pub trait VfF0Host {
    fn vf_e8(&mut self, sp: u32, wall: u32, a: u32, b: u32, out: u32) -> u32;
}

/// The box `E34WallVf::VfCC` queries from the corners' `x` and `y`: the centre and the half extent, `[cx, cy, hx, hy]`,
/// in the asm's operand order.
pub fn corner_box(a: [f32; 2], b: [f32; 2]) -> [f32; 4] {
    [(b[0] + a[0]) * 0.5, (a[1] + b[1]) * 0.5, (b[0] - a[0]) * 0.5, (b[1] - a[1]) * 0.5]
}

/// The widest box `E34WallVf::VfCC` queries (the constant at 0x72b154): a half extent above it on either axis is no
/// overlap (`comiss`, `ja`: an unordered one is queried).
const WALL_BOX_LIMIT: f32 = 10.0;

pub fn box_too_wide(half_x: f32, half_y: f32) -> bool {
    half_x > WALL_BOX_LIMIT || half_y > WALL_BOX_LIMIT
}

/// The original's call out of `E34WallVf::Raycast` (0x4e4570): the penetration signal's emit (0x4e4490, thiscall on
/// the signal manager's emit object, one stack word: the buffer).
pub trait RaycastCalls {
    /// `CMultiSignal1<const CSigCarBrakeWallPenetration&>::Emit` (0x4e4490) on `this`, with the 28-byte buffer at
    /// `out`.
    fn emit_penetration(&mut self, this: u32, out: u32);
}

/// One segment's hit in `E34WallVf::Raycast`: the unit normal, the point's signed distance along the normal, and the
/// penetration fraction (0 when the along-distance is not positive).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentHit {
    pub normal: [f32; 2],
    pub perp: f32,
    pub penetration: f32,
}

/// The inside test and the hit of one segment record: `v` are the four vertices read at the 8-byte stride from the
/// record's first index (`v[0]` to `v[3]`), `p` the point. The point must be on the inner side of the four edge
/// tests. The products and sums keep the original's SSE order (`0x4e4570`, `0x4e46f7-0x4e47da`).
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
    let perp = nny * py + nnx * px;
    let along = 0.0 - (nny * ey1 + nnx * ex1);
    let penetration = if !(along > 0.0) { 0.0 } else { 1.0 - perp / along };
    Some(SegmentHit { normal: [nnx, nny], perp, penetration })
}

/// The calls `CBrakeWall::VfE8` (0x4e3a10) makes at its callee's entry stack pointer `sp`: `E33Mat34::FetchX`
/// (0x51b6c0) on each end point's matrix, into `out`, with the identity worker.
pub trait VfE8Host {
    fn fetch_x(&mut self, sp: u32, matrix: u32, out: u32, worker: u32);
}

/// The squared length below which `VfE8`'s segment is too short to test (0.1, the constant at 0x72b068).
const SWEEP_MIN_SQ: f32 = 0.1;
/// The edge length above which `VfE8` normalises the edge's normal (1e-5, 0x795e8c), and the cross term above which
/// the segment is not parallel to the edge (1e-6, 0x795e70).
const EDGE_MIN_LEN: f32 = 1e-5;
const SWEEP_MIN_CROSS: f32 = 1e-6;

/// `VfE8`'s segment from `p0` to `p1` (the two fetched points), when it is long enough in `x` and `y` (an unordered
/// length is tested, the asm's `comiss`, `ja`).
pub fn sweep(p0: [f32; 3], p1: [f32; 3]) -> Option<[f32; 2]> {
    let d = [p1[0] - p0[0], p1[1] - p0[1]];
    if SWEEP_MIN_SQ > d[1] * d[1] + d[0] * d[0] {
        return None;
    }
    Some(d)
}

/// Where `VfE8`'s segment crosses one wall edge: the point (its `z` along the segment), the edge's normal, the line's
/// offset (`0 - n·a`) and the segment fraction `t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeCross {
    pub point: [f32; 3],
    pub normal: [f32; 2],
    pub offset: f32,
    pub t: f32,
}

/// The crossing of `VfE8`'s segment (`p0` to `p1`, `d` from `sweep`) with the edge from `a` to `b`, when the ends are
/// strictly on opposite sides and the crossing lies within the edge (asm 0x4e3b08-0x4e3cf1). The edge length and the
/// cross term are x87 at 24-bit precision, each operation rounded, which f32 arithmetic gives; the rest is SSE.
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
