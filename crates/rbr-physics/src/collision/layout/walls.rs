//! The brake walls (`E34WallVf`, `CBrakeWall`, `CBrakeWallManager`).

use crate::mem::Mem;

use super::cbody::IDENTITY_WORKER_SLOT;
use super::vec_ops::vec4_copy_from;
use crate::collision::wall::{self, segment_hit, SegmentHit, VfF0Host, WallVfCcHost};
use crate::math::vec::Vec4;
use crate::physics_core::layout::SIGNAL_MANAGER;
use crate::Variant;

/// `E34WallVf::VfCC(a, b)` (0x4e42f0) on the wall at `this`; `entry` is its entry esp. Each corner is fetched through
/// the wall's frame into the frame's +0x20 (the original's `and esp, -0x10` frame); the box of their `x` and `y` goes
/// to +0x10 and, when it is not too wide, is queried against the wall's leaves into the collector at +0x7c. Returns 1
/// when the query found any (eax).
pub fn wall_vf_cc(host: &mut impl WallVfCcHost, mem: &mut Mem, this: u32, a: u32, b: u32, entry: u32) -> u32 {
    let frame_at = ((entry - 4) & !0xf) - 0x30;
    let corner_at = frame_at + 0x20;
    let bounds_at = frame_at + 0x10;
    let mut corners = [[0.0; 2]; 2];
    for (corner, matrix) in corners.iter_mut().zip([a, b]) {
        let frame = host.frame(frame_at - 4, this);
        host.fetch_x(frame_at - 0xc, matrix, corner_at, frame);
        *corner = [mem.f32(corner_at), mem.f32(corner_at + 4)];
    }
    let bounds = wall::corner_box(corners[0], corners[1]);
    for (k, value) in bounds.into_iter().enumerate() {
        mem.set_f32(bounds_at + 4 * k as u32, value);
    }
    if wall::box_too_wide(bounds[2], bounds[3]) {
        return 0;
    }
    u32::from(host.query_box(frame_at - 0xc, this, bounds_at, this + 0x7c) != 0)
}

/// `CBrakeWall::VfF0` (0x4e3970) on the wall at `this` with its eight pointer arguments `a`..`h`; `entry` is its entry
/// esp. In the original's `and esp, -0x10` frame `CE36SubBlock::Init` clears the block at +0x2c, then `VfE8` fills the
/// four vectors at +0xc. On a hit: `CopyFrom` of the first vector to `c`, the frame word at +8 points at the second,
/// which goes to `e`; the fourth's `y` goes to `d`, and `f`, `g` and `h` get 0. Returns 1 on a hit, else 0 (eax).
pub fn brake_wall_vf_f0(host: &mut impl VfF0Host, mem: &mut Mem, this: u32, args: [u32; 8], entry: u32) -> u32 {
    let [a, b, c, d, e, f, g, h] = args;
    let frame = ((entry - 4) & !0xf) - 0x4c;
    let out = frame + 0xc;
    crate::car_body::light::sub_block_init(mem, frame + 0x2c);
    if host.vf_e8(frame - 0x1c, this, a, b, out) == 0 {
        return 0;
    }
    vec4_copy_from(mem, c, out);
    mem.set_u32(frame + 8, out + 0x10);
    Vec4::load(mem, out + 0x10).store(mem, e);
    let y = mem.u32(out + 0x34);
    mem.set_u32(d, y);
    for at in [f, g, h] {
        mem.set_u32(at, 0);
    }
    1
}

/// The wall's fields that `E34WallVf::Raycast` reads and writes: the latch word (+0x214) and the segment vertex table
/// (+0x218).
const WALL_LATCH: u32 = 0x214;
const WALL_MESH: u32 = 0x218;
/// The leaf's count (`seg[4]`) and index table (`seg[5]`); the query's list count is the word at 0x190 (`hits[100]`).
const SEG_COUNT: u32 = 0x10;
const SEG_INDEX: u32 = 0x14;
const HITS_COUNT: u32 = 0x190;
/// The signal manager (global 0x893600) and the penetration emitter's offset in it (0xa780).
const PENETRATION_EMITTER: u32 = 0xa780;

fn u16_at(mem: &Mem, at: u32) -> u32 {
    u32::from(mem.u8(at)) | u32::from(mem.u8(at + 1)) << 8
}

/// The four vertices of the record at index `s`, at the 8-byte stride (the original's overlapping reads).
fn segment_vertices(mem: &Mem, mesh: u32, s: u32) -> [[f32; 2]; 4] {
    let base = mesh + 8 * s;
    std::array::from_fn(|k| {
        let at = base + 8 * k as u32;
        [mem.f32(at), mem.f32(at + 4)]
    })
}

/// The first leaf segment (in the query's order) whose record the point is inside of: the index word and the hit.
fn first_hit(mem: &Mem, wall: u32, point: u32, hits: u32, variant: Variant) -> Option<(u32, SegmentHit)> {
    let mesh = mem.u32(wall + WALL_MESH);
    let p = [mem.f32(point), mem.f32(point + 4)];
    (0..mem.u32(hits + HITS_COUNT)).find_map(|i| {
        let seg = mem.u32(hits + 4 * i);
        let index = mem.u32(seg + SEG_INDEX);
        (0..mem.u32(seg + SEG_COUNT)).find_map(|j| {
            let word = u16_at(mem, index + 2 * j);
            let s = word & 0x3fff;
            segment_hit(segment_vertices(mem, mesh, s), p, variant).map(|hit| (word, hit))
        })
    })
}

/// `E34WallVf::Raycast(point, out)` (0x4e4570, thiscall on the wall, `ret 8`): the query's leaves under the point are
/// in the frame's `hits` list. The first segment the point is inside of answers: `out` gets its normal, perpendicular
/// distance and penetration, and the flags word (the index's bit 15, bit 1 for the latch, bit 2 when the index's bit 14
/// emits). Bit 14 emits the penetration signal and answers 0; otherwise the answer is 1.
pub fn wall_raycast(
    calls: &mut impl wall::RaycastCalls,
    mem: &mut Mem,
    wall: u32,
    point: u32,
    out: u32,
    hits: u32,
    variant: Variant,
) -> u32 {
    if wall::query_point(mem, wall, point, hits) == 0 {
        mem.set_u32(wall + WALL_LATCH, 0);
        return 0;
    }
    let Some((word, hit)) = first_hit(mem, wall, point, hits, variant) else {
        mem.set_u32(wall + WALL_LATCH, 0);
        return 0;
    };
    let latch = if mem.u32(wall + WALL_LATCH) != 0 { 2 } else { 0 };
    let gate = word >> 15;
    let flags = latch | gate;
    let (nx, ny) = (hit.normal[0], hit.normal[1]);
    mem.set_f32(out, nx);
    mem.set_f32(out + 4, ny);
    mem.set_f32(out + 8, 0.0);
    mem.set_f32(out + 0x10, hit.penetration);
    mem.set_f32(out + 0x14, hit.perp);
    mem.set_u32(out + 0x18, flags);
    mem.set_u32(wall + WALL_LATCH, 0);
    if word & 0x4000 == 0 {
        return 1;
    }
    mem.set_u32(out + 0x18, flags | 4);
    let emitter = mem.u32(SIGNAL_MANAGER) + PENETRATION_EMITTER;
    calls.emit_penetration(emitter, out);
    0
}

/// The wall's leaf list (+0x7c) that `CBrakeWall::QueryBox` fills (its count is the list's word at +0x190).
const WALL_LEAVES: u32 = 0x7c;

/// `CBrakeWall::VfE8(a, b, c, out, flag)` (0x4e3a10, thiscall on the wall, `ret 0x14`); `entry` is its entry esp. With
/// `out` and `flag` both set, the two end matrices `a` and `b` are fetched (`FetchX`, host) into the original's
/// `and esp, -0x10` frame (+0x60 and +0x70), and the segment between them is tested against each edge of the wall's
/// leaf segments, in order, until one hits. A crossing within an edge sets the latch (+0x214); unless the index word's
/// bit 14 is set it is the hit: `out` gets the point, the normal (`z` = 0), the offset, `t` (+0x34) and the wall
/// (+0x38). Returns 1 on a hit, else 0 (eax).
pub fn brake_wall_vf_e8(
    host: &mut impl wall::VfE8Host,
    mem: &mut Mem,
    this: u32,
    [a, b, _c, out, flag]: [u32; 5],
    entry: u32,
) -> u32 {
    if out == 0 || flag == 0 {
        return 0;
    }
    let frame = ((entry - 4) & !0xf) - 0x80;
    let (first, second) = (frame + 0x60, frame + 0x70);
    host.fetch_x(frame - 0xc, a, first, mem.u32(IDENTITY_WORKER_SLOT));
    host.fetch_x(frame - 0xc, b, second, mem.u32(IDENTITY_WORKER_SLOT));
    let point = |at: u32| [mem.f32(at), mem.f32(at + 4), mem.f32(at + 8)];
    let (p0, p1) = (point(first), point(second));
    let Some(d) = wall::sweep(p0, p1) else {
        return 0;
    };
    let leaves = this + WALL_LEAVES;
    for i in 0..mem.u32(leaves + HITS_COUNT) {
        let seg = mem.u32(leaves + 4 * i);
        for j in 0..mem.u32(seg + SEG_COUNT) {
            let word = u16_at(mem, mem.u32(seg + SEG_INDEX) + 2 * j);
            let [_, from, _, to] = segment_vertices(mem, mem.u32(this + WALL_MESH), word & 0x3fff);
            let Some(hit) = wall::edge_cross(p0, p1, d, from, to) else {
                continue;
            };
            mem.set_u32(this + WALL_LATCH, 1);
            if word & 0x4000 != 0 {
                continue;
            }
            for (k, v) in hit.point.into_iter().enumerate() {
                mem.set_f32(out + 4 * k as u32, v);
            }
            mem.set_f32(out + 0x10, hit.normal[0]);
            mem.set_f32(out + 0x14, hit.normal[1]);
            mem.set_f32(out + 0x18, 0.0);
            mem.set_f32(out + 0x1c, hit.offset);
            mem.set_f32(out + 0x34, hit.t);
            mem.set_u32(out + 0x38, this);
            return 1;
        }
    }
    0
}

/// `CBrakeWallManager::RaycastEntry(point, out)` (0x4e4960, thiscall, `ret 8`): copies the point's x and y to the
/// 8-byte buffer at `tmp` (the original's frame), then runs the manager's wall (+4) through `wall_raycast`; 0 without a
/// wall. The raycast's frame is the one the original calls it from: its leaf list is 0x1a0 bytes below `tmp`.
pub fn wall_probe(
    calls: &mut impl wall::RaycastCalls,
    mem: &mut Mem,
    manager: u32,
    point: u32,
    out: u32,
    tmp: u32,
    variant: Variant,
) -> u32 {
    let (x, y) = (mem.u32(point), mem.u32(point + 4));
    let wall = mem.u32(manager + 4);
    mem.set_u32(tmp, x);
    mem.set_u32(tmp + 4, y);
    if wall == 0 {
        return 0;
    }
    wall_raycast(calls, mem, wall, tmp, out, tmp - 0x1a0, variant)
}
