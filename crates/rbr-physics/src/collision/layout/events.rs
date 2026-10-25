//! The collision event pool and `MakeEvent`'s payload. The calls on the event and its bodies (`E37EvOps`, `PlaneDiff`,
//! `InitFull`, `E33MakeEvent`) are `collision::events`, generic over `CollisionStore`.

use crate::mem::Mem;

use super::event_pool::FREE_LIST_HEAD;

pub use crate::collision::events::{
    apply_event_impulse, init_full, make_event_at, make_event_frame, make_event_payload, query_event_bodies,
    relative_point_velocity, EventBodyHost,
};

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
