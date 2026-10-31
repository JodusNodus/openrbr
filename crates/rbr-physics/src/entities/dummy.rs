//! The dummy object (a fence post): the box it reports, the span test against a segment, and its presence answer.

use super::store::EntityStore;
use crate::fpu::round24;
use crate::math::vec::Vec3;

/// The low and high corners of the box of `center` and `half`. The original stores y and z swapped: its lanes are x, z, y.
pub fn box_corners(center: Vec3, half: Vec3) -> (Vec3, Vec3) {
    let lo = Vec3 { x: center.x - half.x, y: center.z - half.z, z: center.y - half.y };
    let hi = Vec3 { x: half.x + center.x, y: half.z + center.z, z: half.y + center.y };
    (lo, hi)
}

/// Every dummy answers present.
pub fn present() -> bool {
    true
}

/// The span the box is tested against: the centre `c` and the half extent `d` of the segment from `a` to `b` (the origins of
/// the two transforms). Per lane, `d = (b - a) * 0.5` and `c = a + d`, in SSE f32.
pub fn span(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    let d = Vec3 { x: (b.x - a.x) * 0.5, y: (b.y - a.y) * 0.5, z: (b.z - a.z) * 0.5 };
    let c = Vec3 { x: a.x + d.x, y: a.y + d.y, z: a.z + d.z };
    (c, d)
}

/// The overlap test: the box of `center` and `half` (as the object stores them, see
/// [`box_corners`]) against the span `c`, `d`. On each lane the distance `|center - c|` must be below `d + half`; the
/// original sums the two with x87 at 24 bits, and a NaN fails the test.
pub fn overlaps(center: Vec3, half: Vec3, c: Vec3, d: Vec3) -> bool {
    let center = swap_yz(center);
    let half = swap_yz(half);
    lane_overlaps(center.x, c.x, d.x, half.x)
        && lane_overlaps(center.y, c.y, d.y, half.y)
        && lane_overlaps(center.z, c.z, d.z, half.z)
}

/// The stored order (x, z, y) as lanes (x, y, z), the order `box_corners` reads it in.
fn swap_yz(v: Vec3) -> Vec3 {
    Vec3 { x: v.x, y: v.z, z: v.y }
}

fn lane_overlaps(center: f32, c: f32, d: f32, half: f32) -> bool {
    let gap = (center - c).abs();
    round24(d as f64 + half as f64) > gap as f64
}

/// The calls the span test makes, each with the callee's frame pointer `sp`: the transform fetch
/// on `this` into `out` with `arg`, and the hit handler on the group.
pub trait DummyCalls {
    fn fetch_x(&mut self, sp: u32, this: u32, out: u32, arg: u32);
    fn group_on_hit(&mut self, sp: u32, group: u32);
}

/// The dummy's box at `this`: the box's low and high corners, three floats each, to `lo` and `hi`.
pub fn dummy_box<S: EntityStore>(store: &mut S, this: u32, lo: u32, hi: u32) {
    let (center, half) = store.dummy_extents(this);
    let (low, high) = box_corners(center, half);
    store.store_vec3(lo, low);
    store.store_vec3(hi, high);
}

/// The dummy's span test at `this`, with `frame` the frame's base: the two origins, the span and the overlap
/// test. On a hit the group's handler runs and both segments get the hit flag (it stores to the post without a
/// null check). Always returns 0.
pub fn dummy_span_test<S: EntityStore>(
    store: &mut S,
    this: u32,
    a: u32,
    b: u32,
    frame: u32,
    calls: &mut impl DummyCalls,
) {
    let identity = store.identity();
    let (origin_a, origin_b) = (frame + 0x1c, frame + 0xc);
    calls.fetch_x(frame - 0x10, a, origin_a, identity);
    calls.fetch_x(frame - 0x10, b, origin_b, identity);
    let (c, d) = span(Vec3::load(store, origin_a), Vec3::load(store, origin_b));
    let (center, half) = store.dummy_extents(this);
    if !overlaps(center, half, c, d) {
        return;
    }
    calls.group_on_hit(frame - 0x8, store.dummy_group(this));
    let span_segment = store.dummy_span(this);
    if span_segment != 0 {
        store.mark_segment_hit(span_segment);
    }
    let post = store.dummy_post(this);
    store.mark_segment_hit(post);
}
