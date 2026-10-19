//! E38 collision kernels that are more than `Vec3` arithmetic: the separating-axis box tests and the ray/triangle test
//! (the SSE-ordered normalise is `Vec3::normalize_sse`). The plain scale, add, sub, dot and cross kernels are `math::vec` operations on the loads
//! and stores in `layout.rs` (see `collision/README.md` (git 9c28c0b8)).

use super::elem;
use crate::fpu::{round24, round_x87};
use crate::math::vec::{Vec3, Vec4};
use crate::math::{sign, sse};
use crate::mem::Mem;
use crate::Variant;

/// A separating-axis box: its centre and its half extents (`E38Ops` reads both from one record, centre at +0 and the
/// extents at +12).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SatBox {
    pub center: Vec3,
    pub half_extent: Vec3,
}

/// What `ray_triangle` leaves in its output record. The asm writes each field as it goes, so a miss keeps the fields
/// written before its exit: `u` after the front test, `v` after the barycentric test. Unset fields were not written.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RayTriangleResult {
    pub hit: bool,
    /// Output +0x20: the ray parameter, `dot(e2, q) * inv`.
    pub distance: Option<f32>,
    /// Output +0x24: `dot(s, p)`, scaled by `1/det` on a hit.
    pub u: Option<f32>,
    /// Output +0x28: `dot(q, direction)`, scaled by `1/det` on a hit.
    pub v: Option<f32>,
    /// Output +0x0c..+0x14: the triangle normal `cross(e2, e1)`.
    pub normal: Option<Vec3>,
}

/// Where the solver's fraction starts (the constant at 0x72ad94): above every event's.
pub const FRACTION_START: f32 = 2.0;

/// The solver's fraction after an event's (`E38_004FC500`): the asm's `fcomi`/`jbe` takes the event's `t` only when
/// the fraction so far is above it, so an unordered pair keeps the fraction. `t` is an f32 the x87 loaded exactly.
pub fn earlier_fraction(best: f32, t: f32) -> f32 {
    if best > t {
        t
    } else {
        best
    }
}

/// E38Ops::M_D230 (0x4fd230), the three world-axis separation tests. True when no world axis separates the boxes.
pub fn world_axes_overlap(this: SatBox, other_center: Vec3, other_extent: Vec3) -> bool {
    let delta = other_center - this.center;
    let overlaps = |d: f32, half: f32, extent: f32| f64::from(d.abs()) <= f64::from(half) + f64::from(extent);
    overlaps(delta.x, this.half_extent.x, other_extent.x)
        && overlaps(delta.y, this.half_extent.y, other_extent.y)
        && overlaps(delta.z, this.half_extent.z, other_extent.z)
}

/// E38Ops::M_D2C0 (0x4fd2c0), the world and cross-axis separating-axis tests for two boxes. True when no axis separates
/// them.
pub fn box_sat_overlap(this: SatBox, other_center: Vec3, other_extent: Vec3) -> bool {
    let a = [other_extent.x.abs(), other_extent.y.abs(), other_extent.z.abs()];
    let delta = (other_center + other_extent) - this.center;
    let half = this.half_extent;
    let world = [(delta.x, a[0], half.x), (delta.y, a[1], half.y), (delta.z, a[2], half.z)];
    for (d, extent, h) in world {
        if f64::from(d.abs()) > f64::from(extent) + f64::from(h) {
            return false;
        }
    }

    let cross = other_extent.cross3(delta);
    let limits = [
        f64::from(a[1]) * f64::from(half.z) + f64::from(a[2]) * f64::from(half.y),
        f64::from(a[0]) * f64::from(half.z) + f64::from(a[2]) * f64::from(half.x),
        f64::from(a[0]) * f64::from(half.y) + f64::from(a[1]) * f64::from(half.x),
    ];
    let cross = [cross.x, cross.y, cross.z];
    (0..3).all(|axis| !(f64::from(cross[axis].abs()) > limits[axis]))
}

/// E38Ops::M_D480 (0x4fd480), ray/triangle intersection with the barycentric output. The asm reads every input before
/// it writes the output, so the inputs are values here.
pub fn ray_triangle(origin: Vec3, direction: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> RayTriangleResult {
    let mut result = RayTriangleResult::default();
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let p = e2.cross3(direction);
    let det = p.dot(e1);
    if det < 0.000001 {
        return result;
    }

    let s = origin - v0;
    let t = s.dot(p);
    result.u = Some(t);
    if t < 0.0 || t > det {
        return result;
    }

    let q = e1.cross3(s);
    let u = q.dot(direction);
    result.v = Some(u);
    if u < 0.0 || t + u > det {
        return result;
    }

    let inv = 1.0 / det;
    result.distance = Some(e2.dot(q) * inv);
    result.u = Some(t * inv);
    result.v = Some(u * inv);
    result.normal = Some(e2.cross3(e1));
    result.hit = true;
    result
}

/// What `point_triangle` leaves in its output record. The asm writes each field as it goes, so a miss keeps the fields
/// written before its exit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointTriangle {
    /// Output +0x0c..+0x14: the face normal `e2 x e1`, unit length once the depth test ran.
    pub normal: Vec3,
    /// Output +0x24: the foot's coordinate along `e1`, after the depth test.
    pub t: Option<f32>,
    /// Output +0x28: its coordinate along `e2`, after the `t` test.
    pub u: Option<f32>,
    /// Output +0x00..+0x08 on a hit: the foot of the point on the face's plane (and 1.0 at +0x20).
    pub foot: Option<Vec3>,
}

/// The depth window of `point_triangle` (0x795e8c and 0x795e18).
const MIN_DEPTH: f32 = 0.00001;
const MAX_DEPTH: f32 = 0.5;

/// Each lane times `s` (`mulss`): a degenerate face's infinite scale makes the indefinite NaN, as the instruction does.
fn scale_sse(v: Vec3, s: f32) -> Vec3 {
    Vec3 { x: sse::mul(v.x, s), y: sse::mul(v.y, s), z: sse::mul(v.z, s) }
}

/// An x87 product of two f32 values at the variant's precision, stored as f32 (`fmul`, `fstp`): an invalid product
/// (0 * inf) is the negative indefinite NaN, as on the x87.
fn x87_mul(a: f32, b: f32, variant: Variant) -> f32 {
    let product = f64::from(a) * f64::from(b);
    if product.is_nan() && !a.is_nan() && !b.is_nan() {
        return f32::from_bits(0xffc0_0000);
    }
    round_x87(product, variant) as f32
}

/// E38Ops::M_E2D0 (0x4fe2d0): the point against the face `v0, v1, v2`. A hit is a point behind the face (against its
/// normal `e2 x e1`) by 1e-5 to 0.5 whose foot on the plane lies inside the face. The normal's length and the two
/// reciprocals are x87 at the variant's precision (53 bits in NGP's race steps: the squares and sums stay on the stack,
/// only the reciprocals are stored); the rest is SSE in asm order. The window tests stay comparisons: a NaN passes them, as
/// the asm's `comiss`/`ja` and `fcomi`/`ja` let it through (a degenerate face is a hit with NaN fields).
#[allow(clippy::manual_range_contains)]
pub fn point_triangle(point: Vec3, v0: Vec3, v1: Vec3, v2: Vec3, variant: Variant) -> PointTriangle {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let s = point - v0;
    let normal = e2.cross3(e1);
    let mut result = PointTriangle { normal, ..PointTriangle::default() };
    let side = normal.dot(s);
    if side > 0.0 {
        return result;
    }
    let round = |x: f64| round_x87(x, variant);
    let square = |x: f32| round(f64::from(x) * f64::from(x));
    let length2 = round(round(square(normal.x) + square(normal.y)) + square(normal.z));
    let length = round(length2.sqrt());
    let inverse = round(1.0 / length) as f32;
    let depth = sse::sub(0.0, sse::mul(inverse, side));
    if MIN_DEPTH > depth || depth > MAX_DEPTH {
        return result;
    }
    result.normal = scale_sse(normal, inverse);
    let offset = scale_sse(result.normal, depth);
    let q = offset.cross3(e2);
    let inverse_det = round(1.0 / f64::from(e1.dot(q))) as f32;
    let t = x87_mul(s.dot(q), inverse_det, variant);
    result.t = Some(t);
    if 0.0 > t || t > 1.0 {
        return result;
    }
    let u = x87_mul(offset.dot(s.cross3(e1)), inverse_det, variant);
    result.u = Some(u);
    if 0.0 > u || u + t > 1.0 {
        return result;
    }
    result.foot = Some(point + offset);
    result
}

/// E39_SegTest70 (0x4ff070)'s query box for the segment from `a` to `b`: the midpoint, and half the span as extents.
pub fn segment_box(a: Vec3, b: Vec3) -> SatBox {
    SatBox { center: (b + a).scale(0.5), half_extent: (b - a).scale(0.5) }
}

/// The hit of `mesh_ray_test`: the clamped distance and the four tags of the mesh record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshHit {
    pub distance: f32,
    pub tags: [u32; 4],
}

/// E38Ops::M_AC50 (0x4fac50), ray test against the mesh's two triangles: `(v0, v2, v1)`, then `(v0, v3, v2)` when the
/// first misses. `vertex(i)` and `tag(i)` read the mesh record where the asm does, so a hit on the first triangle never
/// reads `v3`.
pub fn mesh_ray_test(
    origin: Vec3,
    direction: Vec3,
    vertex: impl Fn(usize) -> Vec3,
    tag: impl Fn(usize) -> u32,
) -> Option<MeshHit> {
    let distance = elem::ray_triangle_distance(origin, direction, vertex(0), vertex(2), vertex(1))
        .or_else(|| elem::ray_triangle_distance(origin, direction, vertex(0), vertex(3), vertex(2)))?;
    Some(MeshHit { distance: if !(distance > 0.0) { 0.0 } else { distance }, tags: [tag(0), tag(1), tag(2), tag(3)] })
}

/// CCollisionMeshOpen::IsActive (0x4fb0b0): every open mesh is active, so eax is 1.
pub fn mesh_is_active() -> u32 {
    1
}

/// CCollisionSolver::CDef::Unk20 (0x4fb330): the address of the shared solver table.
pub fn shared_solver_table() -> u32 {
    0x0165_7890
}

/// The two material tags of a collision object and the weight that picks one (the record E38_004FB720 reads).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialPair {
    pub low: u32,
    pub high: u32,
    pub weight: f32,
}

/// E38_004FB720 (0x4fb720): the material tag of the heavier weight, `high` when the weight is above 0.5.
pub fn pick_material_tag(pair: MaterialPair) -> u32 {
    if 0.5_f32 < pair.weight {
        pair.high
    } else {
        pair.low
    }
}

/// Calls outside E38RaySeeds (0x4f6550): `E09Vec4::Clear` (0x465d80) on the vector at `at`, which returns `at`.
pub trait RayHost {
    fn vec4_clear(&mut self, at: u32) -> u32;
}

/// Calls outside E38 M_D760X (0x4fd760): `FUN_00520240` (0x520240) on the sample table, which returns the surface byte
/// for `byte` at the sample point (`low` and `high` are the nibble sums; three stack words).
pub trait SampleHost {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32;
}

/// Calls outside E38SegmentSweep (0x4fad90): the cell query E45 (0x5240c0) over the box `lo`..`hi`, which writes the
/// four list heads to `cells`.
pub trait CellQueryHost {
    fn query_cells(&mut self, mem: &mut Mem, lo: u32, hi: u32, cells: u32);
}

/// The material lookup made by E38_004FB720 (0x4fb720): `Tag520940` (name ours) on the material manager.
pub trait MaterialTagHost {
    fn material_tag(&mut self, manager: u32, tag: u32) -> u32;
}

/// The margin E38_004FC590 (0x4fc590) adds to a contact's depth (0x72fcb4), until its first two-sided contact.
pub const PENETRATION_MARGIN: f32 = 0.02;
/// The margin from a contact whose partner takes part too, for that contact and the rest of the pass (0x739970).
pub const TWO_SIDED_MARGIN: f32 = -0.001;

/// What E38_004FC590 (0x4fc590) reads of one contact event.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SolverContact {
    /// +0xe4: bit 11 moves the body (+0xa0), bit 12 the partner (+0x98); 0x6000 suppresses the move.
    pub flags: u32,
    /// The partner's byte +0x60 has bit 2.
    pub partner_active: bool,
    /// +0x70: the contact normal and the plane offset (`w`).
    pub plane: Vec4,
    /// +0xc0: the contact point.
    pub point: Vec3,
}

/// The move E38_004FC590 makes for one contact. The offset is `None` when the contact is not below its plane or is
/// suppressed; the rows (`M_BD30`, 0x4fbd30) of `Body` and `Partner` are built either way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Correction {
    None,
    /// Bit 11 only: the body moves by `-offset`, then the body's rows with side 1.
    Body(Option<Vec4>),
    /// Bit 12 only: the partner moves by `offset`, then the partner's rows with side 0.
    Partner(Option<Vec4>),
    /// Bits 11 and 12: the offset is split between the two by inverse mass (`split_offsets`); no rows.
    Split(Option<Vec4>),
}

/// E38_004FC590's test of one contact: the depth is `n . point + d + margin` (the asm's order), and the offset the
/// plane times the depth when that is below zero and the contact is not suppressed. Returns the move and the margin
/// for the next contact.
pub fn plan_correction(contact: SolverContact, margin: f32) -> (Correction, f32) {
    let body = contact.flags >> 11 & 1 != 0;
    let partner = contact.flags >> 12 & 1 != 0;
    let margin = if body && partner && contact.partner_active { TWO_SIDED_MARGIN } else { margin };
    if !body && !partner {
        return (Correction::None, margin);
    }
    let (n, p) = (contact.plane, contact.point);
    let depth = p.z * n.z + p.y * n.y + p.x * n.x + n.w + margin;
    let offset = (0.0 > depth && contact.flags & 0x6000 == 0).then(|| contact.plane.scale(depth));
    let correction = match (body, partner) {
        (true, true) => Correction::Split(offset),
        (true, false) => Correction::Body(offset),
        _ => Correction::Partner(offset),
    };
    (correction, margin)
}

/// The two-sided move of E38_004FC590: `f = 1 / (1 + inv_a / inv_b)` on the x87 (at the variant's precision control:
/// 53 bits in NGP's race steps), the body moves by `-(1 - f) * offset` and the partner by `f * offset`. The inverse
/// masses are the st0 results of slot 0x13c; the first is stored (`fstp dword`), the second divides unstored.
pub fn split_offsets(offset: Vec4, inv_a: f64, inv_b: f64, variant: Variant) -> (Vec4, Vec4) {
    let round = |x: f64| round_x87(x, variant);
    let inv_a = f64::from(inv_a as f32);
    let share = round(1.0 / round(round(inv_a / inv_b) + 1.0)) as f32;
    let body = -round(1.0 - f64::from(share)) as f32;
    (offset.scale(body), offset.scale(share))
}

/// E38_004FCA20 (0x4fca20): an event is solved when its time (`Get94`) less 0.1 is not above the smaller progress of
/// the body and the active partner (+0x290).
pub fn event_due(time: f32, progress: f32, partner_progress: Option<f32>) -> bool {
    let limit = match partner_progress {
        Some(p) if progress > p => p,
        _ => progress,
    };
    let v = round24(f64::from(time) - f64::from(0.1f32)) as f32;
    !(v > limit)
}

/// The solver passes' gate on a body: its inverse mass (slot 0x13c, `E47Solid::Vf79x`) is above zero.
pub fn movable(inverse_mass: f64) -> bool {
    inverse_mass > 0.0
}

/// Calls out of the E38 solver passes (0x4fbc20, 0x4fc590, 0x4fca20). `target` is the body's virtual slot, which the
/// caller reads from its vtable.
pub trait SolverHost {
    /// Slot 0x13c (`E47Solid::Vf79x`), thiscall on the body: the inverse mass, as st0.
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64;
    /// Slots 0x80 and 0x8c, thiscall on the body: eax (name ours).
    fn body_test(&mut self, target: u32, body: u32) -> u32;
    /// Slot 0x90 (`E47Solid::Vf36x`), thiscall on the body, run when both tests are 0.
    fn body_wake(&mut self, target: u32, body: u32);
    /// `E38Ops::M_BD30` (0x4fbd30, `layout::contact_rows`), thiscall on the solver: the contact rows of `body` for
    /// `event`.
    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32);
    /// `E38_004FB890` (0x4fb890, `layout::apply_offsets`), thiscall on an offset row: the body's move from its
    /// collected offsets.
    fn apply_offsets(&mut self, row: u32);
    /// `E38Ops::M_C1A0` (0x4fc1a0, `layout::solve_event`), thiscall on the solver: the impulse of one event (its st0
    /// result is dropped).
    fn solve_event(&mut self, solver: u32, event: u32);
}

/// Calls out of E38Ops::M_C1A0 (0x4fc1a0) and M_BD30 (0x4fbd30), each entered at `sp` as the original's. The vectors
/// they take and the floats they write are in the caller's frame.
pub trait ImpulseHost {
    /// `E37EvOps::PlaneDiff` (0x4f40f0), thiscall on the event: the velocity of the contact relative to the partner
    /// (slot 0x140 of the body less the partner's); x, y and z to `out`.
    fn relative_velocity(&mut self, sp: u32, event: u32, out: u32);
    /// `E37EvOps::Query4390` (0x4f4390), thiscall on the event with `(direction, other, use_other, along, across)`: the
    /// bodies' response to an impulse along `direction` (names ours), two floats.
    #[allow(clippy::too_many_arguments)]
    fn response(&mut self, sp: u32, event: u32, direction: u32, other: u32, use_other: u32, along: u32, across: u32);
    /// `E37EvOps::Emit4DC0` (0x4f4dc0), thiscall on the event with `(impulse, slip_speed, vector, slip)`: applies the
    /// impulse to both bodies (their slot 0x128) unless the event is suppressed.
    #[allow(clippy::too_many_arguments)]
    fn apply_impulse(&mut self, sp: u32, event: u32, impulse: f32, slip_speed: f32, vector: u32, slip: u32);
    /// `ComposeA` (0x51b4c0), thiscall on `frame` with `(point, body, out)`.
    fn compose(&mut self, sp: u32, frame: u32, point: u32, body: u32, out: u32);
}

/// The four reals of a contact's material (`E38_004FB720`'s entry) that the impulse and the contact rows read (names
/// ours): friction and restitution, each blended from its slow value to its fast one by the approach speed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactMaterial {
    /// +0x108.
    pub friction_fast: f32,
    /// +0x118.
    pub friction_slow: f32,
    /// +0x128.
    pub restitution_fast: f32,
    /// +0x138.
    pub restitution_slow: f32,
}

/// What E38Ops::M_BD30 (0x4fbd30) reads of an event besides its material: the two contact points (+0x60, +0xc0), the
/// plane (+0x70), the id pairs of each side (+0x8c, +0x90 and +0xa4, +0xa8) and the flags (+0xe4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowEvent {
    pub point_a: Vec4,
    pub point_b: Vec4,
    pub plane: Vec4,
    pub ids_a: [u32; 2],
    pub ids_b: [u32; 2],
    pub flags: u32,
}

/// One 0x60-byte contact row that E38Ops::M_BD30 appends (up to 8 per body).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactRow {
    /// +0x00: 1 when the side's suppress flag is set.
    pub suppressed: u32,
    /// +0x04: the material tag `E38_004FB720` picked.
    pub tag: u32,
    /// +0x08, +0x0c: the side's ids: +0x90 then +0x8c for side 0, +0xa4 then +0xa8 for side 1.
    pub ids: [u32; 2],
    /// +0x10, +0x14: the restitution (slow, fast) of a suppressed side, zero otherwise.
    pub restitution: [f32; 2],
    /// +0x18, +0x1c: the friction (fast, slow).
    pub friction: [f32; 2],
    /// +0x20: the side's own contact point.
    pub point: Vec4,
    /// +0x30: the plane, negated for side 0.
    pub plane: Vec4,
    /// +0x40: `ComposeA` (0x51b4c0) of the other point with the body, from the host.
    pub composed: Vec4,
    /// +0x50: the other contact point.
    pub other: Vec4,
}

/// E38Ops::M_BD30's row for `side` (1: the body, which sits at `point_a`; 0: the partner, at `point_b` with the plane
/// negated). Returns the row without its `composed` vector and the point `ComposeA` takes (the other one).
pub fn contact_row(event: RowEvent, material: ContactMaterial, tag: u32, side: u32) -> ContactRow {
    let (ids, point, plane, other, suppress) = if side == 0 {
        let [a, b] = event.ids_a;
        ([b, a], event.point_b, event.plane.scale(-1.0), event.point_a, 0x2000)
    } else {
        (event.ids_b, event.point_a, event.plane, event.point_b, 0x4000)
    };
    let suppressed = event.flags & suppress != 0;
    ContactRow {
        suppressed: u32::from(suppressed),
        tag,
        ids,
        restitution: if suppressed { [material.restitution_slow, material.restitution_fast] } else { [0.0; 2] },
        friction: [material.friction_fast, material.friction_slow],
        point,
        plane,
        composed: Vec4::default(),
        other,
    }
}

/// E38_004FB890's direction: the offsets summed in order, normalised on the x87 (at the variant's precision control:
/// NGP's race steps run it at 53 bits) when the squared length is above 1e-5 (a double constant). `None` below: the
/// body does not move.
fn offsets_direction(offsets: &[Vec4], variant: Variant) -> Option<Vec4> {
    let sum =
        offsets.iter().fold(Vec4::default(), |acc, o| Vec4 { x: o.x + acc.x, y: o.y + acc.y, z: o.z + acc.z, w: 0.0 });
    let round = |x: f64| round_x87(x, variant);
    let square = |v: f32| round(f64::from(v) * f64::from(v));
    let length2 = round(round(square(sum.z) + square(sum.y)) + square(sum.x));
    if !(length2 > 1e-5) {
        return None;
    }
    let inverse = round(1.0 / round(length2.sqrt())) as f32;
    Some(sum.scale(inverse))
}

/// The largest projection of the offsets on `direction` the move goes (0.03 at most, 0x739958).
const MAX_OFFSET_MOVE: f32 = 0.03;

/// E38_004FB890 (0x4fb890): a body's move from the offsets collected in its row: along their normalised sum, as far as
/// the largest projection of one of them (from 0, at most `MAX_OFFSET_MOVE`). The asm unrolls the projections by four;
/// the first of each four (and each of the rest) sums `z, x, y`, the other three `z, y, x`.
pub fn offsets_move(offsets: &[Vec4], variant: Variant) -> Option<Vec4> {
    let d = offsets_direction(offsets, variant)?;
    let unrolled = if offsets.len() >= 4 { offsets.len() & !3 } else { 0 };
    let mut best = 0.0f32;
    for (i, o) in offsets.iter().enumerate() {
        let v = if i < unrolled && i % 4 != 0 {
            o.z * d.z + o.y * d.y + o.x * d.x
        } else {
            o.z * d.z + o.x * d.x + o.y * d.y
        };
        if v > best {
            best = v;
        }
    }
    if best > MAX_OFFSET_MOVE {
        best = MAX_OFFSET_MOVE;
    }
    Some(d.scale(best))
}

/// E38Ops::M_C1A0's approach speed: the relative velocity (`E37EvOps::PlaneDiff`) on the event's normal, summed
/// `z, y, x`.
pub fn approach_speed(velocity: Vec4, normal: Vec4) -> f32 {
    velocity.z * normal.z + velocity.y * normal.y + velocity.x * normal.x
}

/// E38Ops::M_C1A0's gate: the event is solved when it is not suppressed (`E37Test4B20`), both sides are movable (flags
/// 0x800 and 0x1000) or it approaches faster than 1, and it approaches at all.
pub fn impulse_due(speed: f32, flags: u32, suppressed: bool) -> bool {
    let both = flags & 0x800 != 0 && flags & 0x1000 != 0;
    !suppressed && (both || -1.0 > speed) && 0.0 > speed
}

/// The slip of an approaching contact (M_C1A0): the relative velocity less its normal part (`velocity - n * speed`, all
/// four lanes), its length, and its direction when the length is above 0.001.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slip {
    pub velocity: Vec4,
    pub speed: f32,
    pub direction: Option<Vec4>,
}

/// The slip speed below which M_C1A0 takes no friction (0x795e94).
const SLIP_EPSILON: f32 = 0.001;

/// M_C1A0's slip of the relative velocity `velocity` off `normal`, at approach speed `speed`.
pub fn slip(velocity: Vec4, normal: Vec4, speed: f32) -> Slip {
    let velocity = velocity.add_scaled(normal, 0.0 - speed);
    let length = velocity.length3();
    Slip { velocity, speed: length, direction: (length > SLIP_EPSILON).then(|| velocity.scale(1.0 / length)) }
}

/// M_C1A0's blend of the material from slow to fast: the approach speed times -0.05 (0x73996c), clamped to 0..1 (an
/// unordered value stays).
pub fn speed_blend(speed: f32) -> f32 {
    let k = speed * -0.05;
    if 0.0 > k {
        0.0
    } else if k > 1.0 {
        1.0
    } else {
        k
    }
}

/// The friction coefficient at blend `k` (SSE).
pub fn friction(material: ContactMaterial, k: f32) -> f32 {
    (material.friction_fast - material.friction_slow) * k + material.friction_slow
}

/// The normal impulse of M_C1A0 on the x87 (each operation at the precision control, 53 bits under NGP):
/// `(1 + e) * (-1 / response) * speed`, `e` the restitution at blend `k` and `response` the normal's
/// (`E37EvOps::Query4390`'s first output).
pub fn normal_impulse(material: ContactMaterial, k: f32, response: f32, speed: f32, variant: Variant) -> f32 {
    let round = |x: f64| round_x87(x, variant);
    let slow = f64::from(material.restitution_slow);
    let rise = round(round(f64::from(material.restitution_fast) - slow) * f64::from(k));
    let scale = round(round(rise + slow) + 1.0);
    let r = round(-1.0 / f64::from(response));
    round(round(scale * r) * f64::from(speed)) as f32
}

/// M_C1A0's friction on the impulse vector: the impulse that stops the slip (`slip_speed + across * impulse` over
/// `along`, `Query4390` on the slip direction), capped by the coefficient times the normal impulse; it opposes the
/// slip unless the stopping impulse's sign differs from the slip speed's. Returns `impulse - direction * friction`.
pub fn friction_impulse(
    impulse: Vec4,
    direction: Vec4,
    normal_impulse: f32,
    slip_speed: f32,
    coefficient: f32,
    along: f32,
    across: f32,
) -> Vec4 {
    let stop = across * normal_impulse + slip_speed;
    let q = stop / along;
    let limit = if q > 0.0 { q } else { 0.0 - q };
    let magnitude = if normal_impulse > 0.0 { normal_impulse } else { 0.0 - normal_impulse };
    let mut f = magnitude * coefficient;
    if f > limit {
        f = limit;
    }
    if sign(stop) != sign(slip_speed) {
        f = 0.0 - f;
    }
    impulse.add_scaled(direction, 0.0 - f)
}

/// The constants the E38 face sums scale by, read from the image: 1/15 (0x7399bc, the nibble scale) and 1/31 (0x7399c0,
/// the 5-bit channel scale).
pub const NIBBLE_SCALE: f32 = f32::from_bits(0x3d88_8889);
pub const CHANNEL_SCALE: f32 = f32::from_bits(0x3d04_2108);

/// The 5-bit channel sum a face word gives (bits 10, 5 and 0), weighted by `v`, `u` and `w` (asm order).
pub fn channel_sum(word: u16, v: f32, u: f32, w: f32) -> f32 {
    let channel = |shift: u32| f32::from((word >> shift) & 0x1f);
    (channel(10) * v + channel(5) * u) + channel(0) * w
}

/// The low (or high, with `shift` 4) nibble sum of three face bytes, weighted by `w`, `u` and `v` (asm order).
pub fn nibble_sum(bytes: [u8; 3], shift: u32, w: f32, u: f32, v: f32) -> f32 {
    let nibble = |byte: u8| f32::from((byte >> shift) & 0xf);
    (nibble(bytes[0]) * w + nibble(bytes[1]) * u) + nibble(bytes[2]) * v
}

/// The mesh ray's hit as `CCollisionSystem::Vf00` (0x4fe810) reads it from M_DE40X's output record: the ray parameter
/// `t`, the normal (normalised in place), the barycentric sums `n0` and `n1` (+0x24, +0x28), the dot of the face's
/// first vertex with the normal (`len`), and the face's 0x12 bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub t: f32,
    pub normal: Vec3,
    pub n0: f32,
    pub n1: f32,
    pub len: f32,
    pub face: [u8; 0x12],
}

/// A surface lookup `CCollisionSystem::Vf00` makes (`SampleHost::sample`): the byte for `byte` at the sample `low`,
/// `high`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleQuery {
    pub byte: u32,
    pub low: f32,
    pub high: f32,
}

/// The record `CCollisionSystem::Vf00` writes, by the word it lands in: the point (words 0..2), the normal (4..6), the
/// negated length (7), the two channel sums (8, 9), the two lookups (10, 11: each stores the byte it returns), `t`
/// (12), and the two face flags (13, 14). Word 3 is not written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentRecord {
    pub point: Vec3,
    pub normal: Vec3,
    pub neg_len: f32,
    pub channels: [f32; 2],
    pub lookups: [SampleQuery; 2],
    pub t: f32,
    pub flags: [u32; 2],
}

/// `CCollisionSystem::Vf00` (0x4fe810) after the mesh ray hit: the point `direction * t + origin`, the normal and the
/// negated length, the channel sums and the two lookups. The float order is the asm's.
pub fn segment_record(origin: Vec3, direction: Vec3, hit: SurfaceHit) -> SegmentRecord {
    let f = hit.face;
    let w8 = u16::from_le_bytes([f[8], f[9]]);
    let w6 = u16::from_le_bytes([f[6], f[7]]);
    let (n0, n1) = (hit.n0, hit.n1);
    let w = 1.0_f32 - (n1 + n0);
    let t = hit.t;
    let lookup = |byte: u8, bytes: [u8; 3]| SampleQuery {
        byte: u32::from(byte),
        low: nibble_sum(bytes, 0, n1, w, n0) * NIBBLE_SCALE,
        high: nibble_sum(bytes, 4, n1, w, n0) * NIBBLE_SCALE,
    };
    SegmentRecord {
        point: Vec3 { x: direction.x * t + origin.x, y: direction.y * t + origin.y, z: direction.z * t + origin.z },
        normal: hit.normal,
        neg_len: -hit.len,
        channels: [channel_sum(w8, n1, n0, w) * CHANNEL_SCALE, channel_sum(w6, n1, n0, w) * CHANNEL_SCALE],
        lookups: [lookup(f[10], [f[0xe], f[0xc], f[0xd]]), lookup(f[11], [f[0x11], f[0xf], f[0x10]])],
        t,
        flags: [u32::from(w6 >> 15), u32::from(w8 >> 15)],
    }
}
