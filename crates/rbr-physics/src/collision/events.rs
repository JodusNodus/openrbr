//! Collision events: their creation and the queries and impulses they run on their bodies, generic over the store.

use super::contact_solver;
use super::layout::event_pool::partner_slot;
use super::store::{CollisionStore, Side};
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;

/// The queries and impulses an event runs on its bodies. Each is entered at `sp`, the caller's frame pointer for
/// that call; `target` is the body's virtual slot. Every call returns its result.
pub trait EventBodyHost {
    /// The body's own pointer in the entity tree.
    fn vf30(&mut self, sp: u32, target: u32, body: u32) -> u32;
    /// The pair walk: combines `a` and `b` into the pair result at `out`.
    fn mb560(&mut self, sp: u32, event: u32, a: u32, b: u32, c: u32);
    /// Composes `point` with `body`'s frame into `out`.
    fn compose(&mut self, sp: u32, frame: u32, point: u32, body: u32, out: u32);
    /// The body's six-word query over the composed frames and arguments.
    fn vf138x(&mut self, sp: u32, target: u32, body: u32, args: [u32; 6]) -> u32;
    /// The body's eight-word impulse query.
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

/// Queries both bodies of the event: zeroes `d` and `e`, then walks each body into temporaries, composes it with
/// the event's transform, and queries it. Returns the second body's result.
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

/// One body of [`query_event_bodies`]: the walks of `a` and `b` into temporaries, the transform, then the body's
/// six-word query. Returns its result.
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

/// Applies the event's impulse to its bodies, unless the event's suppression flags are set (returning the flags word
/// then). The first body gets the impulse with the partner slot; the second with `c` and `d` negated. Returns the
/// second body's result (0 without one).
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

/// Stores the input vector scaled by `scale` at `output`.
fn scaled<S: CollisionStore>(store: &mut S, input: u32, output: u32, scale: f32) {
    let value = store.vec4(input).scale(scale);
    store.store_vec4(output, value);
}

/// A side's point-velocity query: the velocity of the body at `point`, written to `out`.
pub trait PlaneDiffHost {
    fn point_velocity(&mut self, sp: u32, target: u32, body: u32, point: u32, out: u32);
}

/// The velocity of the contact point on the body less that on the partner, x, y and z to `out` (its w is not
/// written). Each side's velocity is queried into a scratch frame, or zero for a missing side. Returns `out`.
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

/// One side of [`relative_point_velocity`]: the side's point velocity at the event's contact point into `at`, or zero.
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

/// Initializes the event's fields in order with the 16 payload words, then resolves the two contact points through
/// the body's tree pointer (compose and pair walk), and counts the event. Returns `event`.
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

/// Calls outside [`alloc_event`]: the fallback allocator when the pool is empty.
pub trait AllocEventHost {
    fn operator_new(&mut self, size: u32) -> u32;
}

/// Takes the free-list head, or allocates a new event when the pool is empty.
pub fn alloc_event<S: CollisionStore>(store: &mut S, size: u32, host: &mut impl AllocEventHost) -> u32 {
    let event = store.free_event();
    if event == 0 {
        return host.operator_new(size);
    }
    store.take_free_event(event);
    event
}

/// The nine argument words the event builder accepts.
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

/// Calls outside [`make_event`]: the body's grid lookup, the event allocator, and the full event initializer.
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

/// The event builder's stack-resident payload, before the full event initialization copies it.
pub fn payload<S: CollisionStore>(
    store: &mut S,
    args: MakeEventArgs,
    stack_tail: u32,
    host: &mut impl MakeEventHost<S>,
) -> [u32; 16] {
    let mut data = [0; 16];

    // The sub-block header words and the two event flags, cleared.
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

/// Builds a collision event, leaving the full event initialization to `MakeEventHost`.
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

/// The builder frame aligned from the entry frame pointer: the payload the initializer copies starts after the
/// header words, and its last word is never written.
pub fn make_event_frame(esp: u32) -> u32 {
    ((esp - 4) & !0xf) - 0x60
}

/// The payload's address within the builder frame `s`.
pub fn make_event_payload(s: u32) -> u32 {
    s + 0x10
}

/// Builds the event through [`make_event`], reading the payload's last word from the builder frame.
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
