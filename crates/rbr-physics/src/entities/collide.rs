//! `E33Mat34::CollideInit` (0x51bd30): the contact grid between a body (`a`, bodyA) and the transform it is
//! called on. The port follows the asm (rbr-recomp `notes/physics/collide-init.asm.txt`); `src/rbr/RBR_E43_GroundC.cpp`
//! is a best-effort transcription with one pass where the asm has two. The sequence, in the original's frame, is
//! `layout::collide_init`; the contact arithmetic of the hit tails is here.

/// Bit 3 of bodyA's flags (+0x60): the second pass runs.
pub const SECOND_PASS: u32 = 0x8;

/// The kinds a hit's event is given (`InitFull`'s first argument): 4 for a first-pass or fallback hit, 5 for a
/// second-pass hit. Its fourth argument is the zero vector (`physics_core::layout::ZERO_VEC4`).
pub const HIT_KIND: u32 = 4;
pub const PAIR_HIT_KIND: u32 = 5;

/// The midpoint fraction a second-pass hit gives `E11_VecLerp` (0.5).
pub const MIDPOINT: f32 = 0.5;

/// The calls `CollideInit` makes, each at its callee's entry stack pointer `sp` (the original's pushes), with `this`
/// first. The vectors are in the original's frame.
pub trait CollideHost {
    /// `NTweak::CE36SubBlock::Init` (0x4f3e80) on the block.
    fn init_block(&mut self, sp: u32, block: u32);
    /// bodyA's slot 0x120 (`CEntity::Vf120`): fills the two gate boxes `a` and `b`.
    fn gate_boxes(&mut self, sp: u32, body: u32, a: u32, b: u32);
    /// The host's segment gate (0x4ff070) on `(a, b, 0)`. Non-zero when it passes.
    fn segment_gate(&mut self, sp: u32, host: u32, a: u32, b: u32) -> u32;
    /// bodyA's slot 0x30: its transform.
    fn transform(&mut self, sp: u32, body: u32) -> u32;
    /// bodyA's slot 0x118: the grid's source.
    fn grid_source(&mut self, sp: u32, body: u32) -> u32;
    /// `E37GridOps::M6610` (0x4f6610) on the grid with `(source, transform, identity)`.
    fn build_grid(&mut self, sp: u32, grid: u32, source: u32, transform: u32, identity: u32);
    /// `E01Vec4::CopyFrom` (0x43c5b0): the four floats at `from` to `to`.
    fn copy_from(&mut self, sp: u32, to: u32, from: u32);
    /// `E38Ops::M_D760X` (0x4fd760) on the host with `(a, b, out, 0)`. Non-zero on a hit.
    fn ray_query(&mut self, sp: u32, host: u32, a: u32, b: u32, out: u32) -> u32;
    /// `E38Ops::M_EBC0` (0x4febc0) on the host with `(a, out, 0)`. Non-zero on a hit.
    fn fallback_query(&mut self, sp: u32, host: u32, a: u32, out: u32) -> u32;
    /// `E38RayClear` (0x4f6740) on the grid.
    fn ray_clear(&mut self, sp: u32, grid: u32);
    /// `E33Mat34::ComposeA` (0x51b4c0) on the identity object with `(point, transform, out)`.
    fn compose(&mut self, sp: u32, identity: u32, point: u32, transform: u32, out: u32);
    /// `E36AllocEvent` (0x4f4f10, cdecl): a `size`-byte event, or 0.
    fn alloc_event(&mut self, sp: u32, size: u32) -> u32;
    /// `CCollisionEvent::InitFull` (0x4f50e0) on `event` with `(kind, body, contact, table, point, 0)`. Returns the
    /// event.
    fn init_event(&mut self, sp: u32, event: u32, kind: u32, body: u32, contact: u32, point: u32) -> u32;
    /// `E37GridOps::SampleB` (0x4e0950) on the grid with `(index, out_a, out_b)`.
    fn sample_b(&mut self, sp: u32, grid: u32, index: u32, out_a: u32, out_b: u32);
    /// `CColGrid::SampleA` (0x4e0840) on the grid with `(index, t, out_a, out_b)`.
    fn sample_a(&mut self, sp: u32, grid: u32, index: u32, t: f32, out_a: u32, out_b: u32);
    /// The event sink's slot 1 (0x4fc9a0) with `event`.
    fn report(&mut self, sp: u32, sink: u32, event: u32);
    /// `E11_VecLerp` (0x473c90) on `a` with `(b, t, out)`.
    fn lerp(&mut self, sp: u32, a: u32, b: u32, t: f32, out: u32);
    /// `E01Vec4::Normalize` (0x43c6a0) on the vector.
    fn normalize(&mut self, sp: u32, vector: u32);
    /// `CFloat3::Length` (0x473b30) of the vector (its x87 result).
    fn length(&mut self, sp: u32, vector: u32) -> f32;
}

/// The fallback hit's contact point without the second pass (asm 0x51bfda-0x51c082): the point moved against the
/// normal by half the gate boxes' height (`b.z - a.z`).
pub fn fallback_point(point: [f32; 3], normal: [f32; 3], a_z: f32, b_z: f32) -> [f32; 3] {
    let height = b_z - a_z;
    std::array::from_fn(|k| point[k] - height * normal[k] * MIDPOINT)
}

/// A second-pass hit's summed normals (x87 adds, which f32 gives), `reverse + forward` per lane.
pub fn normal_sum(forward: [f32; 3], reverse: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| reverse[k] + forward[k])
}

/// Whether the summed normals nearly cancel: their absolute lanes sum to below 1.0 (the x87 order `|y| + (|x| + |z|)`;
/// unordered is not).
pub fn normals_cancel(sum: [f32; 3]) -> bool {
    1.0 > sum[1].abs() + (sum[0].abs() + sum[2].abs())
}

/// The plane a second-pass hit reports when the normals do not cancel: the normal and `0 - hit·normal`, in the asm's
/// order (`x`, then `z`, then `y`).
pub fn contact_plane(hit: [f32; 3], normal: [f32; 3]) -> [f32; 4] {
    let dot = hit[0] * normal[0] + hit[2] * normal[2] + hit[1] * normal[1];
    [normal[0], normal[1], normal[2], 0.0 - dot]
}

/// The probe point of a second-pass hit: the midpoint moved along the normalised direction, `n + mid` per lane in the
/// asm's operand order.
pub fn probe_point(mid: [f32; 3], n: [f32; 3]) -> [f32; 3] {
    [n[0] + mid[0], mid[1] + n[1], mid[2] + n[2]]
}
