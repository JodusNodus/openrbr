//! E33 collision event creation around the E37 event builder.

use crate::math::sse;
use crate::mem::Mem;

use super::{BODY_GRID, GRID_TAG};

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

/// The `CCollisionEvent` members the collision code reads and writes (rbr-recomp `notes/physics/contacts.md` 2a).
pub(super) const EVENT_NEXT: u32 = 0x0c;
pub(super) const EVENT_MATERIAL_A: u32 = 0x80;
pub(super) const EVENT_PARTNER: u32 = 0x98;
pub(super) const EVENT_BODY: u32 = 0xa0;
pub(super) const EVENT_MATERIAL_B: u32 = 0xac;
pub(super) const EVENT_FLAGS: u32 = 0xe4;

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
    mem.copy(out, event + EVENT_MATERIAL_A, 12);
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
    mem.copy(vector, event + 0x70, 16);
    let first = sse::mul(mem.f32(event + 0xc8), mem.f32(event + 0x78));
    let second = sse::mul(mem.f32(event + 0xc4), mem.f32(event + 0x74));
    let sum = sse::add(first, second);
    let third = sse::mul(mem.f32(event + 0xc0), mem.f32(event + 0x70));
    let sum = sse::add(sum, third);
    let dot = sse::add(sum, mem.f32(event + 0x7c));
    for lane in 0..4 {
        let value = sse::mul(dot, mem.f32(vector + lane * 4));
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

pub use crate::collision::events::{
    alloc_event, make_event, payload, AllocEventHost, MakeEventArgs, MakeEventHost, PlaneDiffHost,
};
