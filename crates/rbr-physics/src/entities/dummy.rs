//! CDummyObject (`src/rbr/RBR_E90_Dummy.cpp`): the box its `Vf11C` (0x600b20) reports, the span test of its `VfCC`
//! (0x6009b0) and its `Vf110` (0x600c80). `CEntity::Vf120` reaches `Vf11C` through the entity's slot 0x11c.

use crate::fpu::round24;
use crate::math::vec::Vec3;

/// `CDummyObject::Vf11C` (0x600b20): the low and high corners of the box of `center` and `half`. The original stores y
/// and z swapped: its lanes are x, z, y.
pub fn box_corners(center: Vec3, half: Vec3) -> (Vec3, Vec3) {
    let lo = Vec3 { x: center.x - half.x, y: center.z - half.z, z: center.y - half.y };
    let hi = Vec3 { x: half.x + center.x, y: half.z + center.z, z: half.y + center.y };
    (lo, hi)
}

/// `CDummyObject::Vf110` (0x600c80): every dummy answers 1.
pub fn present() -> bool {
    true
}

/// The span `CDummyObject::VfCC` (0x6009b0) tests the box against: the centre `c` and the half extent `d` of the
/// segment from `a` to `b` (their `FetchX` origins). Per lane, `d = (b - a) * 0.5` and `c = a + d`, in SSE f32.
pub fn span(a: Vec3, b: Vec3) -> (Vec3, Vec3) {
    let d = Vec3 { x: (b.x - a.x) * 0.5, y: (b.y - a.y) * 0.5, z: (b.z - a.z) * 0.5 };
    let c = Vec3 { x: a.x + d.x, y: a.y + d.y, z: a.z + d.z };
    (c, d)
}

/// The overlap test of `CDummyObject::VfCC` (0x6009b0): the box of `center` and `half` (as the object stores them, see
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

/// The calls `CDummyObject::VfCC` makes, each with the callee's entry stack pointer `sp`: `E33Mat34::FetchX` (0x51b6c0)
/// on the transform `this` into `out` with `arg`, and the hit handler (0x601470, not named) on the group.
pub trait DummyCalls {
    fn fetch_x(&mut self, sp: u32, this: u32, out: u32, arg: u32);
    fn group_on_hit(&mut self, sp: u32, group: u32);
}
