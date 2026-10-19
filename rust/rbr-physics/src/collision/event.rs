//! E33 collision event creation around the E37 event builder.

use crate::math::sse;
use crate::math::vec::Vec3;
use crate::mem::Mem;

use super::layout::{BODY_GRID, GRID_TAG};

#[inline(always)]
fn mulss(lhs: f32, rhs: f32) -> f32 {
    let left = lhs.to_bits();
    let right = rhs.to_bits();
    if is_sse_nan(right) {
        return f32::from_bits(right | 0x0040_0000);
    }
    if is_sse_nan(left) {
        return f32::from_bits(left | 0x0040_0000);
    }
    if (left & 0x7fff_ffff == 0 && right & 0x7fff_ffff == 0x7f80_0000)
        || (right & 0x7fff_ffff == 0 && left & 0x7fff_ffff == 0x7f80_0000)
    {
        return f32::from_bits(0xffc0_0000);
    }
    lhs * rhs
}

#[inline(always)]
fn addss(lhs: f32, rhs: f32) -> f32 {
    let left = lhs.to_bits();
    let right = rhs.to_bits();
    if is_sse_nan(right) {
        return f32::from_bits(right | 0x0040_0000);
    }
    if is_sse_nan(left) {
        return f32::from_bits(left | 0x0040_0000);
    }
    if left & 0x7fff_ffff == 0x7f80_0000 && right & 0x7fff_ffff == 0x7f80_0000 && (left ^ right) & 0x8000_0000 != 0 {
        return f32::from_bits(0xffc0_0000);
    }
    lhs + rhs
}

#[inline(always)]
fn is_sse_nan(bits: u32) -> bool {
    bits & 0x7f80_0000 == 0x7f80_0000 && bits & 0x007f_ffff != 0
}

/// The partner slot E37EvOps::Emit4DC0 (0x4f4dc0) passes for the first body: the second slot's id and word (+0xa8,
/// +0xa4), or the first slot's (+0x8c, +0x90) when the second word is 0. Returns `(id, word)`.
pub fn partner_slot(second_word: u32, second_id: u32, first_id: u32, first_word: u32) -> (u32, u32) {
    if second_word == 0 {
        (first_id, first_word)
    } else {
        (second_id, second_word)
    }
}

pub(super) const EVENT_BASE_VTABLE: u32 = 0x0073_9558;
pub(crate) const EVENT_COUNT: u32 = 0x008e_f7f0;
pub(crate) const FREE_LIST_HEAD: u32 = 0x008e_f7f4;
pub(crate) const FREE_LIST_COUNT: u32 = 0x008e_f7fc;

/// The `CCollisionEvent` members the collision code reads and writes (`notes/physics/contacts.md` 2a).
pub(super) const EVENT_NEXT: u32 = 0x0c;
pub(super) const EVENT_MATERIAL_A: u32 = 0x80;
pub(super) const EVENT_PARTNER: u32 = 0x98;
pub(super) const EVENT_BODY: u32 = 0xa0;
pub(super) const EVENT_MATERIAL_B: u32 = 0xac;
pub(super) const EVENT_FLAGS: u32 = 0xe4;

/// Calls outside E36AllocEvent, currently the CRT allocator when the event pool is empty.
pub trait AllocEventHost {
    fn operator_new(&mut self, size: u32) -> u32;
}

/// E36AllocEvent (0x4f4f10): take the free-list head or allocate a new event when the pool is empty.
pub fn alloc_event(mem: &mut Mem, size: u32, host: &mut impl AllocEventHost) -> u32 {
    let event = mem.u32(FREE_LIST_HEAD);
    if event == 0 {
        return host.operator_new(size);
    }

    if mem.u32(event + 0x14) == FREE_LIST_HEAD {
        mem.set_u32(FREE_LIST_COUNT, mem.u32(FREE_LIST_COUNT).wrapping_sub(1));
        super::list::unlink(mem, event + 0x0c);
    }
    event
}

fn refresh_side(mem: &mut Mem, event: u32, id0: u32, id1: u32, time: u32, body_offset: u32) {
    // Retail's branch is `time < 0.5 ? id0 : id1`; reconstructed C++ uses the inverse.
    let selected_id = if mem.f32(event + time) < 0.5 { mem.u32(event + id0) } else { mem.u32(event + id1) };
    if selected_id == 0 {
        let body = mem.u32(event + body_offset);
        if body != 0 && mem.u8(body + 0x60) & 2 != 0 {
            let grid = mem.u32(body + BODY_GRID);
            mem.set_u32(event + id0, u32::from(mem.u8(grid + GRID_TAG)));
            mem.set_u32(event + id1, 0);
            mem.set_u32(event + time, 0.0f32.to_bits());
        }
    }
}

/// E37Refresh4A20 (0x4f4a20) (name ours): refresh both material ids from their owning body's active grid tag.
pub fn refresh_materials(mem: &mut Mem, event: u32) {
    refresh_side(mem, event, EVENT_MATERIAL_A, EVENT_MATERIAL_A + 4, EVENT_MATERIAL_A + 8, EVENT_PARTNER);
    refresh_side(mem, event, EVENT_MATERIAL_B, EVENT_MATERIAL_B + 4, EVENT_MATERIAL_B + 8, EVENT_BODY);
}

/// E37Test4B20 (0x4f4b20): test for either of the event's two material flags.
pub fn is_suppressed(mem: &Mem, event: u32) -> bool {
    mem.u32(event + EVENT_FLAGS) & 0x6000 != 0
}

/// E37EvOps::Copy80 (0x4f4200): copy the three raw words at event +0x80 to the output.
pub fn copy_80(mem: &mut Mem, event: u32, out: u32) {
    let words = mem.read(event + EVENT_MATERIAL_A, 12);
    mem.map(out, &words);
}

/// E37EvOps::Cmp15 (0x4f4570): compare the nine selected float fields used for deduplication.
pub fn cmp_15(mem: &Mem, event: u32, other: u32) -> u32 {
    const FLOAT_LANES: [u32; 8] = [0, 1, 2, 4, 5, 6, 7, 13];
    u32::from(
        FLOAT_LANES.iter().all(|lane| mem.f32(event + lane * 4) == mem.f32(other + lane * 4))
            && mem.u32(event + 0x38) == mem.u32(other + 0x38),
    )
}

/// E37EvOps::PlaneScale (0x4f4190): scale all four output lanes by the event plane dot product.
pub fn plane_scale(mem: &mut Mem, event: u32, vector: u32) -> u32 {
    // Retail inlines CFloat3::LoadBinary with the output vector as its receiver.
    // The reconstructed C++ calls Init70(this+0x70), which would self-copy instead.
    let plane = mem.read(event + 0x70, 16);
    mem.map(vector, &plane);
    let first = mulss(mem.f32(event + 0xc8), mem.f32(event + 0x78));
    let second = mulss(mem.f32(event + 0xc4), mem.f32(event + 0x74));
    let sum = addss(first, second);
    let third = mulss(mem.f32(event + 0xc0), mem.f32(event + 0x70));
    let sum = addss(sum, third);
    let dot = addss(sum, mem.f32(event + 0x7c));
    for lane in 0..4 {
        let value = mulss(dot, mem.f32(vector + lane * 4));
        mem.set_f32(vector + lane * 4, value);
    }
    vector
}

/// CCollisionEvent::DtorBody (0x4f52c0): demote the vtable, release its count, and unlink its owner.
pub fn dtor_body(mem: &mut Mem, event: u32) {
    mem.set_u32(event, EVENT_BASE_VTABLE);
    mem.set_u32(EVENT_COUNT, mem.u32(EVENT_COUNT).wrapping_sub(1));

    let link = event + 0x0c;
    let owner = mem.u32(link + 8);
    if owner != 0 {
        mem.set_u32(owner + 8, mem.u32(owner + 8).wrapping_sub(1));
        super::list::unlink(mem, link);
    }
}

/// CCollisionEvent::Vf1Cx (0x4f52f0): demote and unlink, then optionally return to the free list.
pub fn vf1cx(mem: &mut Mem, event: u32, flags: u32) -> u32 {
    dtor_body(mem, event);
    if flags & 1 != 0 {
        // Retail stores the event pointer in the free-list node fields; the reconstructed C++'s
        // `this + 0xc` argument disagrees with the direct-call memory recordings.
        super::list::push_front(mem, event);
    }
    event
}

/// The nine cdecl words accepted by E33MakeEvent (0x4e0d00).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MakeEventArgs {
    pub kind: u32,
    pub vector_a: u32,
    pub p3: u32,
    pub body_a: u32,
    pub body_b: u32,
    pub aux_a: u32,
    pub aux_b: u32,
    pub vector_b: u32,
    pub z2: u32,
}

/// Calls outside E33MakeEvent, owned by the body vtable and E37 event pool.
pub trait MakeEventHost {
    fn body_grid(&mut self, mem: &Mem, body: u32) -> u32;
    fn alloc_event(&mut self, size: u32) -> u32;
    fn init_full(
        &mut self,
        mem: &mut Mem,
        event: u32,
        kind: u32,
        body_a: u32,
        aux_a: u32,
        aux_b: u32,
        data: &[u32; 16],
    ) -> u32;
}

/// E33MakeEvent's stack-resident payload, before E37::CCollisionEvent::InitFull copies it.
pub fn payload(mem: &mut Mem, args: MakeEventArgs, stack_tail: u32, host: &mut impl MakeEventHost) -> [u32; 16] {
    let mut data = [0; 16];

    // CE36SubBlock::Init, followed by the two event flags.
    data[8] = 0;
    data[9] = 0;
    data[10] = 0;
    data[11] = 0;
    data[12] = 0;

    let grid_b = host.body_grid(mem, args.body_b);
    if grid_b != 0 {
        data[12] = u32::from(mem.u8(grid_b + 0x689));
        data[11] = u32::from(mem.u8(grid_b + 0x688));
        mem.map(grid_b + 0x688, &[0, 0]);
        let tag = u32::from(mem.u8(grid_b + GRID_TAG));
        data[8] = tag;
        data[9] = tag;
    }

    for lane in 0..4 {
        data[lane] = mem.u32(args.vector_a + lane as u32 * 4);
        data[lane + 4] = mem.u32(args.vector_b + lane as u32 * 4);
    }
    data[13] = args.p3;
    data[14] = args.body_b;
    data[15] = stack_tail;
    data
}

/// Builds an E33 collision event, leaving E37's full event initialization to `MakeEventHost`.
pub fn make_event(mem: &mut Mem, args: MakeEventArgs, stack_tail: u32, host: &mut impl MakeEventHost) -> u32 {
    let data = payload(mem, args, stack_tail, host);
    let event = host.alloc_event(0xf0);
    if event == 0 {
        return 0;
    }

    let event = host.init_full(mem, event, args.kind, args.body_a, args.aux_a, args.aux_b, &data);
    if event != 0 {
        let grid_a = host.body_grid(mem, args.body_a);
        if grid_a != 0 {
            let tag = u32::from(mem.u8(grid_a + GRID_TAG));
            mem.set_u32(event + EVENT_MATERIAL_B + 4, tag);
            mem.set_u32(event + EVENT_MATERIAL_B, tag);
            mem.set_u32(event + EVENT_MATERIAL_B + 8, 0);
        }
    }
    event
}

/// The call out of E37EvOps::PlaneDiff (0x4f40f0), entered at `sp` as the original's: a side's slot 0x140 (`target`,
/// `CSolid::E43Vf140` for a solid), thiscall on the body with `(point, out)`, the velocity of the body at `point`.
pub trait PlaneDiffHost {
    fn point_velocity(&mut self, sp: u32, target: u32, body: u32, point: u32, out: u32);
}

/// E37EvOps::PlaneDiff (0x4f40f0): the event body's velocity at the contact less the partner's, `subss` per lane.
pub fn relative_velocity(body: Vec3, partner: Vec3) -> Vec3 {
    Vec3 { x: sse::sub(body.x, partner.x), y: sse::sub(body.y, partner.y), z: sse::sub(body.z, partner.z) }
}
