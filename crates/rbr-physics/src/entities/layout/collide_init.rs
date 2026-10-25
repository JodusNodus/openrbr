//! `CGround::VfE8` (0x51c6f0) and `E33Mat34::CollideInit` (0x51bd30).

use crate::collision::layout::EVENT_SIZE;
use crate::entities::collide;
use crate::entities::ground::ProbeScratch;
pub(super) use crate::entities::layout::ROOT_IDENTITY as IDENTITY_GLOBAL;
use crate::entities::store::EntityStore;
use crate::mem::Mem;
use crate::stage;

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

/// The collision-system copy used by entity collisions and ground probes.
fn collide_host(mem: &Mem) -> u32 {
    stage::layout::CollisionQuery::EntityCollision.system(mem)
}

/// The frame of `CGround::VfE8` (`box_probe`) below its entry stack pointer `esp`: the scratch, and the entry stack
/// pointers of its calls (`FetchX` on both transforms, the host's query, and `CopyFrom`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbeFrame {
    pub scratch: ProbeScratch,
    pub fetch_sp: u32,
    pub query_sp: u32,
    pub copy_sp: u32,
}

pub fn box_probe_frame(esp: u32) -> ProbeFrame {
    let base = (esp - 4) & !0xf;
    ProbeFrame {
        scratch: ProbeScratch { first: base - 0x50, second: base - 0x60, out: base - 0x40 },
        fetch_sp: base - 0x7c,
        query_sp: base - 0x80,
        copy_sp: base - 0x78,
    }
}

/// bodyA's flags (+0x60), which `CollideInit` reads between its passes (bit 3: the second pass runs).
const BODY_FLAGS: u32 = 0x60;

/// The grid's counts and edge pairs (`CColGrid`, 0x6B0 bytes): the second pass's count at
/// +0x68C, the second pass's pairs of box indices as the bytes at +0x484 + 6i and +0x485 + 6i, the grid's point
/// (+0x20) and its first pass's boxes (+0x40, 16 bytes each).
const GRID_COUNT_SECOND: u32 = 0x68C;
const GRID_PAIRS: u32 = 0x484;
const GRID_PAIR_STRIDE: u32 = 6;
const GRID_POINT: u32 = 0x20;
const GRID_BOXES: u32 = 0x40;

/// The two samples `SampleA`/`SampleB` write into a collision event (`CCollisionEvent`, +0xa4 and +0xa8).
const EVENT_SAMPLES: [u32; 2] = [0xA4, 0xA8];

/// `CollideInit`'s frame: `s` is esp after the prologue (`push ebp`, `and esp, -0x10`, the exception record, 0x848
/// bytes of locals and three saved registers). The offsets are the asm's `esp`-relative ones.
struct CollideFrame {
    s: u32,
}

impl CollideFrame {
    /// The query point: the grid's point, moved by the fallback without the second pass.
    const POINT: u32 = 0x20;
    /// The second pass's first box of the current pair (the slot then takes a hit's first length).
    const PAIR_FIRST: u32 = 0x3C;
    /// A second-pass hit's summed normals, then the direction to the probe.
    const DIRECTION: u32 = 0x40;
    /// The second pass's pair index.
    const PAIR_INDEX: u32 = 0x5C;
    /// The contact `InitFull` reads: the composed point (+0), the query's first four floats (+0x10, `InitFull`'s point),
    /// the normal or plane (+0x20), the query's words at +0x28, +0x2c and +0x24 (+0x30, in that order), two zero words
    /// (+0x3c, +0x40), a zero float (+0x44, the fallback's +0x30 word) and bodyB (+0x48). `CE36SubBlock::Init` clears
    /// the block at +0x30 first.
    const CONTACT: u32 = 0x60;
    const HIT: u32 = 0x70;
    const PLANE: u32 = 0x80;
    const WORDS: u32 = 0x90;
    const ZEROS: u32 = 0x9C;
    const SCALE: u32 = 0xA4;
    const OWNER: u32 = 0xA8;
    /// The gate boxes bodyA's slot 0x120 fills (`a`, then `b`).
    const GATE_B: u32 = 0xB0;
    const GATE_A: u32 = 0xC0;
    /// The query's output and the reverse query's.
    const OUT: u32 = 0xD0;
    const REVERSE: u32 = 0x150;
    /// The two vectors a second-pass hit measures: hit to the second box, and the first box to the second.
    const TO_SECOND: u32 = 0x140;
    const SPAN: u32 = 0x130;
    /// The contact grid (`CColGrid`).
    const GRID: u32 = 0x190;

    fn at(&self, offset: u32) -> u32 {
        self.s + offset
    }
}

fn load3(mem: &Mem, at: u32) -> [f32; 3] {
    mem.f32s(at)
}

fn store3(mem: &mut Mem, at: u32, v: [f32; 3]) {
    for (k, x) in v.into_iter().enumerate() {
        mem.set_f32(at + 4 * k as u32, x);
    }
}

/// The query output's words the contact takes (+0x28, +0x2c, +0x24 to the contact's +0x30, +0x34, +0x38), as the
/// asm's integer and SSE moves.
fn store_query_words(mem: &mut Mem, f: &CollideFrame) {
    let out = f.at(CollideFrame::OUT);
    let words = [mem.u32(out + 0x28), mem.u32(out + 0x2C), mem.u32(out + 0x24)];
    for (k, w) in words.into_iter().enumerate() {
        mem.set_u32(f.at(CollideFrame::WORDS) + 4 * k as u32, w);
    }
}

/// The query output's normal (+0x10, 16 bytes) to the contact's plane.
fn store_query_normal(mem: &mut Mem, f: &CollideFrame) {
    mem.copy(f.at(CollideFrame::PLANE), f.at(CollideFrame::OUT) + 0x10, 16);
}

/// `E33Mat34::CollideInit(a, b, c)` (0x51bd30, thiscall on bodyB, `ret 0xc`): bodyA is `body`, `sink` is `c`, `entry`
/// is the entry esp. The gate boxes from bodyA's slot 0x120 must pass the host's segment gate; then the grid is built
/// from bodyA's grid source and transform, and its point is queried against each first-pass box (a hit is a kind-4
/// contact at the box, sampled with `SampleB`). With bodyA's flag bit 3 each second-pass box pair is queried both ways;
/// both hitting is a kind-5 contact at the probe between them, sampled with `SampleA` at the hit's fraction. Without any
/// hit the host's fallback query on the point gives a kind-4 contact at the point (moved by half the gate height when
/// the second pass did not run). Every contact is reported to the sink. Returns 1 when one was, else 0 (eax).
pub fn collide_init(host: &mut impl CollideHost, mem: &mut Mem, this: u32, body: u32, sink: u32, entry: u32) -> u32 {
    let f = CollideFrame { s: ((entry - 4) & !0xF) - 0x860 };
    let s = f.s;
    let grid = f.at(CollideFrame::GRID);
    let (point, out) = (f.at(CollideFrame::POINT), f.at(CollideFrame::OUT));
    let (gate_a, gate_b) = (f.at(CollideFrame::GATE_A), f.at(CollideFrame::GATE_B));
    host.init_block(s - 4, f.at(CollideFrame::WORDS));
    host.gate_boxes(s - 0xC, body, gate_a, gate_b);
    if host.segment_gate(s - 0x10, collide_host(mem), gate_a, gate_b) == 0 {
        return 0;
    }
    let identity = mem.u32(IDENTITY_GLOBAL);
    let transform = host.transform(s - 8, body);
    let source = host.grid_source(s - 0xC, body);
    host.build_grid(s - 0x10, grid, source, transform, identity);
    host.copy_from(s - 8, point, grid + GRID_POINT);
    let count = mem.grid_count(grid);
    mem.set_f32(f.at(CollideFrame::SCALE), 0.0);
    mem.set_u32(f.at(CollideFrame::OWNER), this);
    mem.set_u32(f.at(CollideFrame::ZEROS), 0);
    mem.set_u32(f.at(CollideFrame::ZEROS) + 4, 0);
    let mut hit = false;
    for i in 0..count {
        let edge = grid + GRID_BOXES + 16 * i;
        if host.ray_query(s - 0x14, collide_host(mem), point, edge, out) == 0 {
            continue;
        }
        store_query_words(mem, &f);
        store_query_normal(mem, &f);
        let contact = f.at(CollideFrame::CONTACT);
        let event = report_contact(host, mem, &f, body, edge, contact, collide::HIT_KIND);
        let [a, b] = EVENT_SAMPLES.map(|k| event + k);
        host.sample_b(s - 0x10, grid, i, a, b);
        host.report(s - 8, sink, event);
        hit = true;
    }
    if mem.u32(body + BODY_FLAGS) & collide::SECOND_PASS == 0 {
        if !hit && host.fallback_query(s - 0x10, collide_host(mem), point, out) != 0 {
            fallback_hit(host, mem, &f, body, sink, true);
            hit = true;
        }
    } else {
        // The pair index and the pair's first box are frame words a fallback contact (at +0x20) spans.
        mem.set_u32(f.at(CollideFrame::PAIR_INDEX), 0);
        for i in 0..u32::from(mem.u8(grid + GRID_COUNT_SECOND)) {
            let pair = grid + GRID_PAIRS + GRID_PAIR_STRIDE * i;
            let first = grid + GRID_BOXES + 16 * u32::from(mem.u8(pair));
            let second = grid + GRID_BOXES + 16 * u32::from(mem.u8(pair + 1));
            mem.set_u32(f.at(CollideFrame::PAIR_FIRST), first);
            hit |= pair_hit(host, mem, &f, body, sink, i, first, second);
            mem.set_u32(f.at(CollideFrame::PAIR_INDEX), i + 1);
        }
        if !hit && host.fallback_query(s - 0x10, collide_host(mem), point, out) != 0 {
            fallback_hit(host, mem, &f, body, sink, false);
            hit = true;
        }
    }
    host.ray_clear(s - 4, grid);
    u32::from(hit)
}

/// A contact's common tail: `InitFull`'s point is the query output's first four floats (`CopyFrom`), bodyA's transform
/// composes `at` into `contact`, and an allocated event is initialised with `kind`. Returns the event (0 when the
/// allocation failed).
fn report_contact(
    host: &mut impl CollideHost,
    mem: &Mem,
    f: &CollideFrame,
    body: u32,
    at: u32,
    contact: u32,
    kind: u32,
) -> u32 {
    let s = f.s;
    let hit = f.at(CollideFrame::HIT);
    host.copy_from(s - 8, hit, f.at(CollideFrame::OUT));
    compose_contact(host, mem, f, body, at, contact, kind)
}

fn compose_contact(
    host: &mut impl CollideHost,
    mem: &Mem,
    f: &CollideFrame,
    body: u32,
    at: u32,
    contact: u32,
    kind: u32,
) -> u32 {
    let s = f.s;
    let transform = host.transform(s - 8, body);
    host.compose(s - 0x10, mem.u32(IDENTITY_GLOBAL), at, transform, contact);
    let event = host.alloc_event(s - 8, EVENT_SIZE);
    if event == 0 {
        return 0;
    }
    host.init_event(s - 0x1C, event, kind, body, contact, f.at(CollideFrame::HIT))
}

/// The fallback hit (asm 0x51bf93 and 0x51c54d): the output's +0x30 float to the contact's scale, its normal to the
/// plane, `CopyFrom` of its point, its words; without the second pass (`moved`) the point first moves against the normal
/// by half the gate height. The contact is the point itself.
fn fallback_hit(host: &mut impl CollideHost, mem: &mut Mem, f: &CollideFrame, body: u32, sink: u32, moved: bool) {
    let (s, out, point) = (f.s, f.at(CollideFrame::OUT), f.at(CollideFrame::POINT));
    let scale = mem.u32(out + 0x30);
    mem.set_u32(f.at(CollideFrame::SCALE), scale);
    if !moved {
        // After the second pass the asm keeps the normal copy's source pointer in the pair word, which the contact spans.
        mem.set_u32(f.at(CollideFrame::PAIR_FIRST), out + 0x10);
    }
    store_query_normal(mem, f);
    host.copy_from(s - 8, f.at(CollideFrame::HIT), out);
    if moved {
        let normal = load3(mem, out + 0x10);
        let a_z = mem.f32(f.at(CollideFrame::GATE_A) + 8);
        let b_z = mem.f32(f.at(CollideFrame::GATE_B) + 8);
        let moved = collide::fallback_point(load3(mem, point), normal, a_z, b_z);
        store3(mem, point, moved);
    }
    store_query_words(mem, f);
    let event = compose_contact(host, mem, f, body, point, point, collide::HIT_KIND);
    host.report(s - 8, sink, event);
}

/// One second-pass box pair (asm 0x51c110-0x51c4f8): `first` to `second`, then on a hit `second` to `first` into the
/// reverse output. Both hitting: the midpoint of the two outputs (`E11_VecLerp`), the direction from the summed normals
/// (or, when they cancel, from the midpoint to the point), normalised, and the probe from the midpoint along it queried
/// against the midpoint. A probe hit is the contact: its point, its words, and the query's normal or the plane through
/// the hit; the event is sampled at the hit's fraction from `first` to `second`. Returns whether it reported one.
#[allow(clippy::too_many_arguments)]
fn pair_hit(
    host: &mut impl CollideHost,
    mem: &mut Mem,
    f: &CollideFrame,
    body: u32,
    sink: u32,
    index: u32,
    first: u32,
    second: u32,
) -> bool {
    let s = f.s;
    let (out, reverse) = (f.at(CollideFrame::OUT), f.at(CollideFrame::REVERSE));
    if host.ray_query(s - 0x14, collide_host(mem), first, second, out) == 0
        || host.ray_query(s - 0x14, collide_host(mem), second, first, reverse) == 0
    {
        return false;
    }
    let (contact, direction) = (f.at(CollideFrame::CONTACT), f.at(CollideFrame::DIRECTION));
    let sum = collide::normal_sum(load3(mem, out + 0x10), load3(mem, reverse + 0x10));
    store3(mem, direction, sum);
    host.lerp(s - 0x10, out, reverse, collide::MIDPOINT, contact);
    let cancel = collide::normals_cancel(sum);
    if cancel {
        let (p, mid) = (load3(mem, f.at(CollideFrame::POINT)), load3(mem, contact));
        store3(mem, direction, std::array::from_fn(|k| p[k] - mid[k]));
    }
    host.normalize(s - 4, direction);
    let (n, mid) = (load3(mem, direction), load3(mem, contact));
    let hit = f.at(CollideFrame::HIT);
    store3(mem, hit, collide::probe_point(mid, n));
    if host.ray_query(s - 0x14, collide_host(mem), hit, contact, out) == 0 {
        return false;
    }
    host.copy_from(s - 8, hit, out);
    store_query_words(mem, f);
    if cancel {
        store_query_normal(mem, f);
    } else {
        let plane = collide::contact_plane(load3(mem, hit), n);
        for (k, v) in plane.into_iter().enumerate() {
            mem.set_f32(f.at(CollideFrame::PLANE) + 4 * k as u32, v);
        }
    }
    let event = compose_contact(host, mem, f, body, contact, contact, collide::PAIR_HIT_KIND);
    let (b, h, a) = (load3(mem, second), load3(mem, hit), load3(mem, first));
    store3(mem, f.at(CollideFrame::TO_SECOND), std::array::from_fn(|k| b[k] - h[k]));
    store3(mem, f.at(CollideFrame::SPAN), std::array::from_fn(|k| b[k] - a[k]));
    let to_second = host.length(s - 0xC, f.at(CollideFrame::TO_SECOND));
    mem.set_f32(f.at(CollideFrame::PAIR_FIRST), to_second);
    let span = host.length(s - 0xC, f.at(CollideFrame::SPAN));
    let [out_a, out_b] = EVENT_SAMPLES.map(|k| event + k);
    host.sample_a(s - 0x14, f.at(CollideFrame::GRID), index, to_second / span, out_a, out_b);
    host.report(s - 8, sink, event);
    true
}
