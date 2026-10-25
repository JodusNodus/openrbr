//! The body's contact calls over the store: `BigA`, `Vf81x`, `E43Vf128`'s tail and the ring append.

use super::contact::{self, ContactHost};
use super::host::HostUnavailable;
use super::impulse;
use super::store::RigidStore;
use super::vote::{self, ContactVoteHost};
use crate::mem::Mem;
use crate::physics_core::layout as core_layout;

/// `BigA`'s outputs: the impulse pair's addresses (the force vector, 16 bytes, and the torque, 12), and the
/// force and depth floats' addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactOut {
    pub force_out: u32,
    pub torque_out: u32,
    pub force: u32,
    pub depth: u32,
}

/// `CSolid::BigA` (0x52b9f0) on the store: the contact at `e` with the range `f`. The body's inverse mass, the
/// world's `MaxTimeStep` and the sample test (the hook and flag 8) are read first; the writes are the depth, the force
/// when it is computed, and the pair on a hit. Returns eax (1 on a hit).
pub fn contact_worker<S: RigidStore>(
    store: &mut S,
    this: u32,
    out: ContactOut,
    e: u32,
    f: f32,
    host: &mut impl ContactHost,
) -> Result<u32, HostUnavailable> {
    let input = store.contact_input(this, e, f);
    let r = contact::contact_step(&input, host)?;
    store.set_f32(out.depth, r.depth);
    if let Some(force) = r.force {
        store.set_f32(out.force, force);
    }
    let Some(i) = r.impulse else {
        return Ok(0);
    };
    store.store_row(out.force_out, i.force);
    for (k, v) in [i.torque.x, i.torque.y, i.torque.z].into_iter().enumerate() {
        store.set_f32(out.torque_out + 4 * k as u32, v);
    }
    Ok(1)
}

/// `CSolid::Vf81x` (0x52c510) on the store: the vote into the force and torque outputs (16 bytes each, written
/// only when the body has a list). Returns eax.
pub fn contact_vote<S: RigidStore>(
    store: &mut S,
    this: u32,
    a: u32,
    b: u32,
    host: &mut impl ContactVoteHost,
) -> Result<u32, HostUnavailable> {
    let Some(v) = vote::vote(store.contact_count(this), host)? else {
        return Ok(0);
    };
    store.store_row(a, v.force);
    store.store_row(b, v.torque);
    Ok(u32::from(v.hit))
}

/// The bit of the CDef debug word (`core_layout::cdef_debug_flags`) that lets `E43Vf128` append to a ring tail.
const RING_BIT: u32 = 0x0100_0000;

/// The original callees of `E43Vf128`'s tail, at their entry stack pointers.
pub trait ImpulseTailCalls<S = Mem> {
    /// Slot 12 of the body (`S30`), with the word the original has pushed for the compose: returns the body's pose.
    fn pose(&mut self, store: &mut S, sp: u32, body: u32, pushed: u32) -> u32;
    /// `E33Mat34::ComposeA` (0x51b4c0) with the walker as ecx.
    fn compose_a(&mut self, store: &mut S, sp: u32, point: u32, matrix: u32, out: u32);
    /// Slot 1 of the hook.
    fn hook(&mut self, store: &mut S, sp: u32, hook: u32, args: [u32; 8]);
    /// `E46_RingAppend` (0x529100) on the ring tail.
    fn ring_append(&mut self, store: &mut S, sp: u32, ring: u32, args: [u32; 4]);
}

/// `CSolid::E43Vf128`'s tail (from 0x52a20b), after the velocities. With a hook, its slot 1 gets the point composed
/// with the body's pose and the call's other arguments. With the physics global's ring bit and a ring tail, the ring
/// gets the point composed with the body. `frame` is the original's esp after its prologue, `((entry - 4) & !0xf) -
/// 0x80`; both composes write `frame + 0x70`.
pub fn impulse_tail<S: RigidStore>(
    store: &mut S,
    this: u32,
    args: [u32; 8],
    frame: u32,
    calls: &mut impl ImpulseTailCalls<S>,
) {
    let point = args[0];
    let out = frame + 0x70;
    if store.hook(this) != 0 {
        let pose = calls.pose(store, frame - 8, this, out);
        calls.compose_a(store, frame - 0x10, point, pose, out);
        let pair = args[4];
        let pair = store.hook_pair(pair);
        let word = if impulse::hook_takes_first(pair.third, pair.half) { pair.first } else { pair.second };
        let hook = store.hook(this);
        let hook_args = [args[2], args[3], word, args[5], args[6], out, point, args[7]];
        calls.hook(store, frame - 0x24, hook, hook_args);
    }
    if core_layout::cdef_debug_flags(store.mem()) & RING_BIT != 0 && store.ring_tail(this) != 0 {
        calls.compose_a(store, frame - 0x10, point, this, out);
        let ring = store.ring_tail(this);
        calls.ring_append(store, frame - 0x14, ring, [out, 0, 1.0f32.to_bits(), 0]);
    }
}

/// `E46_RingAppend` (0x529100) on the ring at `ring`: the three words `a`, `b`, `c` and the vector at `src` go in at
/// the index, which then steps (`impulse::ring_step`). A full ring that does not wrap is left as it is. Returns the eax
/// the original leaves: the new index plus one (wrapping), the new count, or the index (full).
pub fn ring_append<S: RigidStore>(store: &mut S, ring: u32, src: u32, words: [u32; 3]) -> u32 {
    let header = store.ring_header(ring);
    let Some((next, count)) = impulse::ring_step(header.wraps, header.index, header.count) else {
        return header.index;
    };
    store.store_ring_entry(ring, header.index, words, src);
    store.store_ring_cursor(ring, next, count);
    if header.wraps {
        next + 1
    } else {
        count
    }
}
