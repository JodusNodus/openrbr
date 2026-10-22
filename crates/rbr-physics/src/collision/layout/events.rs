//! The collision event calls: the event body query and impulse (`E37EvOps`), `PlaneDiff`, `InitFull`,
//! `E33MakeEvent` and the event pool.

use crate::math::vec::{clear_vec4, Vec3, Vec4};
use crate::mem::Mem;

use super::event_pool::{
    self as event, MakeEventArgs, MakeEventHost, PlaneDiffHost, EVENT_BODY, EVENT_FLAGS, EVENT_MATERIAL_A,
    EVENT_MATERIAL_B, EVENT_PARTNER, FREE_LIST_HEAD,
};
use super::vec_ops::scaled;
use crate::collision::contact_solver;
use crate::entities::layout::ROOT_IDENTITY;
use crate::physics_core::layout::vtable_target;

/// The calls E37EvOps::Query4390 (0x4f4390) and Emit4DC0 (0x4f4dc0) make on the event's bodies. Each is entered at
/// `sp`, the original's stack pointer for that call; `target` is the body's virtual slot (read from its vtable by the
/// caller). Every virtual call returns its eax.
pub trait EventBodyHost {
    /// Slot 0x30 (`Vf30`) with no arguments: the body's own pointer in the CEntity tree (eax).
    fn vf30(&mut self, sp: u32, target: u32, body: u32) -> u32;
    /// `E43PairWalk::MB560` (0x51b560), thiscall on `event` with `(a, b, c)`.
    fn mb560(&mut self, sp: u32, event: u32, a: u32, b: u32, c: u32);
    /// `ComposeA` (0x51b4c0), thiscall on `frame` with `(point, body, out)`.
    fn compose(&mut self, sp: u32, frame: u32, point: u32, body: u32, out: u32);
    /// Slot 0x138 (`Vf138x`) with six arguments.
    fn vf138x(&mut self, sp: u32, target: u32, body: u32, args: [u32; 6]) -> u32;
    /// Slot 0x128 (`Vf128x`) with eight arguments.
    fn vf128x(&mut self, sp: u32, target: u32, body: u32, args: [u32; 8]) -> u32;
}

const VF30: u32 = 0x30;
const VF128X: u32 = 0x128;
const VF138X: u32 = 0x138;

/// E37EvOps::Query4390 (0x4f4390) (name ours), thiscall on the event with `(a, b, c, d, e)`, ret 0x14. `d` and `e` are
/// zeroed, and each body (+0xa0, then +0x98) is walked by `a` and `b`, composed with the event's transform (+0x60) and
/// passed to its `Vf138x` with the three temporaries. The frame is the original's: `frame` is `(esp - 4) & !0xf`, the
/// temporaries sit below it, and every call is entered 0x48 to 0x5c bytes below it. Returns the second body's `Vf138x`
/// eax.
pub fn query_event_bodies(
    host: &mut impl EventBodyHost,
    mem: &mut Mem,
    event: u32,
    [a, b, c, d, e]: [u32; 5],
    esp: u32,
) -> u32 {
    let frame = (esp - 4) & !0xf;
    mem.set_u32(d, 0);
    mem.set_u32(e, 0);
    query_body(host, mem, event, EVENT_BODY, [a, b, c, d, e], frame);
    query_body(host, mem, event, EVENT_PARTNER, [a, b, c, d, e], frame)
}

/// One body of `query_event_bodies`: the walk of `a` into `tA` and of `b` into `tB`, the transform into `tC`, then
/// `Vf138x` on the body. Returns its eax.
fn query_body(
    host: &mut impl EventBodyHost,
    mem: &mut Mem,
    event: u32,
    body_at: u32,
    [a, b, c, d, e]: [u32; 5],
    frame: u32,
) -> u32 {
    let (ta, tb, tc) = (frame - 0x20, frame - 0x30, frame - 0x10);
    for (walk, out) in [(a, ta), (b, tb)] {
        let body = mem.u32(event + body_at);
        let r = host.vf30(frame - 0x48, vtable_target(mem, body, VF30), body);
        host.mb560(frame - 0x50, event, walk, r, out);
    }
    let body = mem.u32(event + body_at);
    let r = host.vf30(frame - 0x48, vtable_target(mem, body, VF30), body);
    host.compose(frame - 0x50, event, event + 0x60, r, tc);
    let body = mem.u32(event + body_at);
    host.vf138x(frame - 0x5c, vtable_target(mem, body, VF138X), body, [tc, ta, tb, c, d, e])
}

/// E37EvOps::Emit4DC0 (0x4f4dc0), thiscall on the event with `(a, b, c, d)`, ret 0x10. Unless the event's flags (+0xe4)
/// carry 0x2000 or 0x4000, the first body (+0xa0) gets `Vf128x` with the partner slot, and the second (+0x98) gets it
/// with `c` and `d` negated into the frame. Returns the flags word when suppressed, else the second body's eax (0
/// without one).
pub fn apply_event_impulse(
    host: &mut impl EventBodyHost,
    mem: &mut Mem,
    event: u32,
    [a, b, c, d]: [u32; 4],
    esp: u32,
) -> u32 {
    let flags = mem.u32(event + EVENT_FLAGS);
    if flags & 0x6000 != 0 {
        return flags;
    }
    let frame = (esp - 4) & !0xf;
    let body_a = mem.u32(event + EVENT_BODY);
    if body_a != 0 {
        let (id, word) = event::partner_slot(
            mem.u32(event + 0xa4),
            mem.u32(event + 0xa8),
            mem.u32(event + 0x8c),
            mem.u32(event + 0x90),
        );
        let args = [event + 0x60, c, id, word, event + EVENT_MATERIAL_A, a, b, d];
        host.vf128x(frame - 0x54, vtable_target(mem, body_a, VF128X), body_a, args);
    }
    let body_b = mem.u32(event + EVENT_PARTNER);
    if body_b == 0 {
        return 0;
    }
    let (ta, tb) = (frame - 0x20, frame - 0x10);
    scaled(mem, d, ta, -1.0);
    let (id, word) = (mem.u32(event + 0x8c), mem.u32(event + 0x90));
    scaled(mem, c, tb, -1.0);
    let args = [event + 0x60, tb, id, word, event + EVENT_MATERIAL_B, a, b, ta];
    host.vf128x(frame - 0x54, vtable_target(mem, body_b, VF128X), body_b, args)
}

/// E37EvOps::PlaneDiff (0x4f40f0) on the event at `event`: the velocity of the contact point (+0x60) on the body
/// (+0xa0) less that on the partner (+0x98), x, y and z to `out` (its w is not written). Each velocity is the side's
/// slot 0x140 into the original's frame below the entry stack pointer `esp`, or zero for a missing side. Returns `out`.
pub fn relative_point_velocity(host: &mut impl PlaneDiffHost, mem: &mut Mem, event: u32, out: u32, esp: u32) -> u32 {
    let frame = ((esp - 4) & !0xf) - 0x30;
    let body = side_velocity(host, mem, event, EVENT_BODY, frame + 0x10, frame - 0xc);
    let partner = side_velocity(host, mem, event, EVENT_PARTNER, frame + 0x20, frame - 0xc);
    contact_solver::relative_velocity(body, partner).store(mem, out);
    out
}

/// One side of PlaneDiff: the side's slot 0x140 at the event's contact point into `at`, or `E09Vec4::Clear` of `at`.
fn side_velocity(host: &mut impl PlaneDiffHost, mem: &mut Mem, event: u32, side: u32, at: u32, sp: u32) -> Vec3 {
    let body = mem.u32(event + side);
    if body == 0 {
        Vec4::default().store(mem, at);
    } else {
        let target = mem.u32(mem.u32(body) + SLOT_POINT_VELOCITY);
        host.point_velocity(sp, target, body, event + 0x60, at);
    }
    Vec3::load(mem, at)
}

const SLOT_POINT_VELOCITY: u32 = 0x140;

/// CCollisionEvent::InitFull (0x4f50e0, thiscall on `event` with `(kind, body, point_a, point_b, payload, tag)`, ret
/// 0x18): the event's fields in the original's order, the 16 payload words to +0x60, then the two contact points
/// through the body's slot 0x30 (ComposeA to +0xc0, MB560 to +0xd0), and the global event count. Its calls enter 0x90
/// below the aligned frame of the entry esp `esp`. The rest of the original (an E19 stream and a frame built from
/// +0x70) writes only its own locals. Returns `event`.
pub fn init_full(
    host: &mut impl EventBodyHost,
    mem: &mut Mem,
    event: u32,
    [kind, body, point_a, point_b, payload, tag]: [u32; 6],
    esp: u32,
) -> u32 {
    let sp = ((esp - 4) & !0xf) - 0x90;
    for offset in [0x10, 0xc, 0x14] {
        mem.set_u32(event + offset, 0);
    }
    // M_AFA0: the handle on the root identity, the count one more than the root's (two vtable stores, both replaced).
    let root = mem.u32(ROOT_IDENTITY);
    mem.set_u32(event + 4, root);
    let count = if root == 0 { 0 } else { mem.u32(root + 8).wrapping_add(1) };
    mem.set_u32(event + 8, count);
    mem.set_u32(event, event::EVENT_BASE_VTABLE);
    clear_vec4().store(mem, event + 0x40);
    clear_vec4().store(mem, event + 0x50);
    for offset in [0x80, 0x84, 0x88, 0xac, 0xb0, 0xb4] {
        mem.set_u32(event + offset, 0);
    }
    mem.set_u32(event + 0xe0, tag);
    mem.set_u32(event + 0xa8, 0);
    mem.set_u32(event + 0xa4, 0);
    for k in 0..16 {
        mem.set_u32(event + 0x60 + 4 * k, mem.u32(payload + 4 * k));
    }
    mem.set_u32(event + EVENT_BODY, body);
    mem.set_u32(event + EVENT_FLAGS, kind);

    let body = mem.u32(event + EVENT_BODY);
    let frame = host.vf30(sp, vtable_target(mem, body, VF30), body);
    host.compose(sp, frame, point_a, event, event + 0xc0);
    let body = mem.u32(event + EVENT_BODY);
    let frame = host.vf30(sp, vtable_target(mem, body, VF30), body);
    host.mb560(sp, frame, point_b, event, event + 0xd0);
    mem.set_u32(event::EVENT_COUNT, mem.u32(event::EVENT_COUNT).wrapping_add(1));
    event
}

/// E33MakeEvent's frame (0x4e0d00) from the entry esp: `push ebp`, the alignment, the exception record, 0x48 bytes of
/// locals and three saved registers. The payload InitFull copies is at +0x10; its last word (+0x4c) is never written.
pub fn make_event_frame(esp: u32) -> u32 {
    ((esp - 4) & !0xf) - 0x60
}

/// The payload's place in E33MakeEvent's frame `s` (`make_event_frame`).
pub fn make_event_payload(s: u32) -> u32 {
    s + 0x10
}

/// E33MakeEvent (0x4e0d00, cdecl with nine words) through `event::make_event`, the payload's last word from the frame.
pub fn make_event_at(host: &mut impl MakeEventHost, mem: &mut Mem, words: [u32; 9], esp: u32) -> u32 {
    let [kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2] = words;
    let args = MakeEventArgs { kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2 };
    let stack_tail = mem.u32(make_event_payload(make_event_frame(esp)) + 0x3c);
    event::make_event(mem, args, stack_tail, host)
}

/// Writes MakeEvent's payload words at `at` before InitFull copies them.
pub fn store_payload(mem: &mut Mem, at: u32, data: &[u32; 16]) {
    for (k, word) in (0..).zip(data) {
        mem.set_u32(at + 4 * k, *word);
    }
}

/// The pool's events: 0x80 `CCollisionEvent`s (0xf0 bytes each) in `.data` from 0x8ef810, which `E37Flush5010`
/// (0x4f5010) keeps (it deletes only events outside them).
const POOL_EVENTS: u32 = 0x008e_f810;
const POOL_EVENT_COUNT: u32 = 0x80;
/// The size of a `CCollisionEvent` (`E36AllocEvent`'s argument).
pub const EVENT_SIZE: u32 = 0xf0;

/// E37Prefill5340 (0x4f5340): `E37Flush5010`, then the pool's events pushed at the free list's head in address order
/// (`list::insert`'s stores; the asm keeps the head in a register and stores it last). The load (`CPhysics::CPhysics`)
/// finds the free list empty, where the flush does nothing; flushing a non-empty list is not ported.
pub fn prefill_pool(mem: &mut Mem) {
    assert_eq!(mem.u32(FREE_LIST_HEAD), 0, "E37Flush5010 on a non-empty event pool is not ported");
    for k in 0..POOL_EVENT_COUNT {
        super::list::insert(mem, FREE_LIST_HEAD, POOL_EVENTS + EVENT_SIZE * k);
    }
}
