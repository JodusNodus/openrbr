//! Collision events: their creation and the calls they make on their bodies (`E37EvOps`, `InitFull`, `E33MakeEvent`),
//! generic over the store.

use super::contact_solver;
use super::layout::event_pool::partner_slot;
use super::store::{CollisionStore, Side};
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;

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
const SLOT_POINT_VELOCITY: u32 = 0x140;

fn side_body<S: CollisionStore>(store: &S, event: u32, side: Side) -> u32 {
    match side {
        Side::Body => store.event_body(event),
        Side::Partner => store.event_partner(event),
    }
}

/// E37EvOps::Query4390 (0x4f4390) (name ours), thiscall on the event with `(a, b, c, d, e)`, ret 0x14. `d` and `e` are
/// zeroed, and each body (+0xa0, then +0x98) is walked by `a` and `b`, composed with the event's transform (+0x60) and
/// passed to its `Vf138x` with the three temporaries. The frame is the original's: `frame` is `(esp - 4) & !0xf`, the
/// temporaries sit below it, and every call is entered 0x48 to 0x5c bytes below it. Returns the second body's `Vf138x`
/// eax.
pub fn query_event_bodies<S: CollisionStore>(
    host: &mut impl EventBodyHost,
    store: &mut S,
    event: u32,
    [a, b, c, d, e]: [u32; 5],
    esp: u32,
) -> u32 {
    let frame = (esp - 4) & !0xf;
    store.set_u32(d, 0);
    store.set_u32(e, 0);
    query_body(host, store, event, Side::Body, [a, b, c, d, e], frame);
    query_body(host, store, event, Side::Partner, [a, b, c, d, e], frame)
}

/// One body of `query_event_bodies`: the walk of `a` into `tA` and of `b` into `tB`, the transform into `tC`, then
/// `Vf138x` on the body. Returns its eax.
fn query_body<S: CollisionStore>(
    host: &mut impl EventBodyHost,
    store: &mut S,
    event: u32,
    side: Side,
    [a, b, c, d, e]: [u32; 5],
    frame: u32,
) -> u32 {
    let (ta, tb, tc) = (frame - 0x20, frame - 0x30, frame - 0x10);
    for (walk, out) in [(a, ta), (b, tb)] {
        let body = side_body(store, event, side);
        let r = host.vf30(frame - 0x48, store.vtable_slot(body, VF30), body);
        host.mb560(frame - 0x50, event, walk, r, out);
    }
    let body = side_body(store, event, side);
    let r = host.vf30(frame - 0x48, store.vtable_slot(body, VF30), body);
    host.compose(frame - 0x50, event, store.event_point(event), r, tc);
    let body = side_body(store, event, side);
    host.vf138x(frame - 0x5c, store.vtable_slot(body, VF138X), body, [tc, ta, tb, c, d, e])
}

/// E37EvOps::Emit4DC0 (0x4f4dc0), thiscall on the event with `(a, b, c, d)`, ret 0x10. Unless the event's flags (+0xe4)
/// carry 0x2000 or 0x4000, the first body (+0xa0) gets `Vf128x` with the partner slot, and the second (+0x98) gets it
/// with `c` and `d` negated into the frame. Returns the flags word when suppressed, else the second body's eax (0
/// without one).
pub fn apply_event_impulse<S: CollisionStore>(
    host: &mut impl EventBodyHost,
    store: &mut S,
    event: u32,
    [a, b, c, d]: [u32; 4],
    esp: u32,
) -> u32 {
    let flags = store.event_flags(event);
    if flags & 0x6000 != 0 {
        return flags;
    }
    let frame = (esp - 4) & !0xf;
    let body_a = store.event_body(event);
    if body_a != 0 {
        let ids = store.event_ids(event);
        let (id, word) = partner_slot(ids.second[0], ids.second[1], ids.first[0], ids.first[1]);
        let args = [store.event_point(event), c, id, word, store.event_materials_at(event, Side::Partner), a, b, d];
        host.vf128x(frame - 0x54, store.vtable_slot(body_a, VF128X), body_a, args);
    }
    let body_b = store.event_partner(event);
    if body_b == 0 {
        return 0;
    }
    let (ta, tb) = (frame - 0x20, frame - 0x10);
    scaled(store, d, ta, -1.0);
    let [id, word] = store.event_ids(event).first;
    scaled(store, c, tb, -1.0);
    let args = [store.event_point(event), tb, id, word, store.event_materials_at(event, Side::Body), a, b, ta];
    host.vf128x(frame - 0x54, store.vtable_slot(body_b, VF128X), body_b, args)
}

/// E33Vec4::Scaled (0x4e0700): stores the input vector times `scale` at `output`.
fn scaled<S: CollisionStore>(store: &mut S, input: u32, output: u32, scale: f32) {
    let value = store.vec4(input).scale(scale);
    store.store_vec4(output, value);
}

/// The call out of E37EvOps::PlaneDiff (0x4f40f0), entered at `sp` as the original's: a side's slot 0x140 (`target`,
/// `CSolid::E43Vf140` for a solid), thiscall on the body with `(point, out)`, the velocity of the body at `point`.
pub trait PlaneDiffHost {
    fn point_velocity(&mut self, sp: u32, target: u32, body: u32, point: u32, out: u32);
}

/// E37EvOps::PlaneDiff (0x4f40f0) on the event at `event`: the velocity of the contact point (+0x60) on the body
/// (+0xa0) less that on the partner (+0x98), x, y and z to `out` (its w is not written). Each velocity is the side's
/// slot 0x140 into the original's frame below the entry stack pointer `esp`, or zero for a missing side. Returns `out`.
pub fn relative_point_velocity<S: CollisionStore>(
    host: &mut impl PlaneDiffHost,
    store: &mut S,
    event: u32,
    out: u32,
    esp: u32,
) -> u32 {
    let frame = ((esp - 4) & !0xf) - 0x30;
    let body = side_velocity(host, store, event, Side::Body, frame + 0x10, frame - 0xc);
    let partner = side_velocity(host, store, event, Side::Partner, frame + 0x20, frame - 0xc);
    store.store_vec3(out, contact_solver::relative_velocity(body, partner));
    out
}

/// One side of PlaneDiff: the side's slot 0x140 at the event's contact point into `at`, or `E09Vec4::Clear` of `at`.
fn side_velocity<S: CollisionStore>(
    host: &mut impl PlaneDiffHost,
    store: &mut S,
    event: u32,
    side: Side,
    at: u32,
    sp: u32,
) -> Vec3 {
    let body = side_body(store, event, side);
    if body == 0 {
        store.store_vec4(at, Vec4::default());
    } else {
        let target = store.vtable_slot(body, SLOT_POINT_VELOCITY);
        host.point_velocity(sp, target, body, store.event_point(event), at);
    }
    store.vec3(at)
}

/// CCollisionEvent::InitFull (0x4f50e0, thiscall on `event` with `(kind, body, point_a, point_b, payload, tag)`, ret
/// 0x18): the event's fields in the original's order, the 16 payload words to +0x60, then the two contact points
/// through the body's slot 0x30 (ComposeA to +0xc0, MB560 to +0xd0), and the global event count. Its calls enter 0x90
/// below the aligned frame of the entry esp `esp`. The rest of the original (an E19 stream and a frame built from
/// +0x70) writes only its own locals. Returns `event`.
pub fn init_full<S: CollisionStore>(
    host: &mut impl EventBodyHost,
    store: &mut S,
    event: u32,
    [kind, body, point_a, point_b, payload, tag]: [u32; 6],
    esp: u32,
) -> u32 {
    let sp = ((esp - 4) & !0xf) - 0x90;
    store.init_event(event, kind, body, tag, payload);

    let (compose_out, mb560_out) = store.event_contact_outputs(event);
    let body = store.event_body(event);
    let frame = host.vf30(sp, store.vtable_slot(body, VF30), body);
    host.compose(sp, frame, point_a, event, compose_out);
    let body = store.event_body(event);
    let frame = host.vf30(sp, store.vtable_slot(body, VF30), body);
    host.mb560(sp, frame, point_b, event, mb560_out);
    store.count_event();
    event
}

/// Calls outside E36AllocEvent, currently the CRT allocator when the event pool is empty.
pub trait AllocEventHost {
    fn operator_new(&mut self, size: u32) -> u32;
}

/// E36AllocEvent (0x4f4f10): take the free-list head or allocate a new event when the pool is empty.
pub fn alloc_event<S: CollisionStore>(store: &mut S, size: u32, host: &mut impl AllocEventHost) -> u32 {
    let event = store.free_event();
    if event == 0 {
        return host.operator_new(size);
    }
    store.take_free_event(event);
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
pub trait MakeEventHost<S = Mem> {
    fn body_grid(&mut self, mem: &S, body: u32) -> u32;
    fn alloc_event(&mut self, size: u32) -> u32;
    fn init_full(
        &mut self,
        mem: &mut S,
        event: u32,
        kind: u32,
        body_a: u32,
        aux_a: u32,
        aux_b: u32,
        data: &[u32; 16],
    ) -> u32;
}

/// E33MakeEvent's stack-resident payload, before E37::CCollisionEvent::InitFull copies it.
pub fn payload<S: CollisionStore>(
    store: &mut S,
    args: MakeEventArgs,
    stack_tail: u32,
    host: &mut impl MakeEventHost<S>,
) -> [u32; 16] {
    let mut data = [0; 16];

    // CE36SubBlock::Init, followed by the two event flags.
    data[8] = 0;
    data[9] = 0;
    data[10] = 0;
    data[11] = 0;
    data[12] = 0;

    let grid_b = host.body_grid(store, args.body_b);
    if grid_b != 0 {
        let [first, second] = store.take_grid_hit_flags(grid_b);
        data[12] = second;
        data[11] = first;
        let tag = store.grid_tag(grid_b);
        data[8] = tag;
        data[9] = tag;
    }

    for lane in 0..4 {
        data[lane] = store.u32(args.vector_a + lane as u32 * 4);
        data[lane + 4] = store.u32(args.vector_b + lane as u32 * 4);
    }
    data[13] = args.p3;
    data[14] = args.body_b;
    data[15] = stack_tail;
    data
}

/// Builds an E33 collision event, leaving E37's full event initialization to `MakeEventHost`.
pub fn make_event<S: CollisionStore>(
    store: &mut S,
    args: MakeEventArgs,
    stack_tail: u32,
    host: &mut impl MakeEventHost<S>,
) -> u32 {
    let data = payload(store, args, stack_tail, host);
    let event = host.alloc_event(0xf0);
    if event == 0 {
        return 0;
    }

    let event = host.init_full(store, event, args.kind, args.body_a, args.aux_a, args.aux_b, &data);
    if event != 0 {
        let grid_a = host.body_grid(store, args.body_a);
        if grid_a != 0 {
            let tag = store.grid_tag(grid_a);
            store.store_event_tag(event, tag);
        }
    }
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

/// E33MakeEvent (0x4e0d00, cdecl with nine words) through `make_event`, the payload's last word from the frame.
pub fn make_event_at<S: CollisionStore>(
    host: &mut impl MakeEventHost<S>,
    store: &mut S,
    words: [u32; 9],
    esp: u32,
) -> u32 {
    let [kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2] = words;
    let args = MakeEventArgs { kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2 };
    let stack_tail = store.u32(make_event_payload(make_event_frame(esp)) + 0x3c);
    make_event(store, args, stack_tail, host)
}
