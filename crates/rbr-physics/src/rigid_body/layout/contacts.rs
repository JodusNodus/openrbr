//! The body's contact list: the records and events, `BigA`, `Vf81x`, `E43Vf128`'s tail and the ring append.

use super::{BODY_FLAG_8, DEFINITION, DEF_INVERSE_MASS, ENTITY_FLAGS};
use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::physics_core::layout as core_layout;
use crate::rigid_body;
use crate::rigid_body::contact::{Contact, ContactHost, Input};
use crate::rigid_body::contact_list::{Event, Record};
use crate::rigid_body::host::HostUnavailable;
use crate::rigid_body::vote::{self, ContactVoteHost};

/// The contact records `E36Collect` left: `count` records of 0x40 bytes from `list` (`BigB` reads them in order).
pub fn contact_records(mem: &Mem, list: u32, count: u32) -> Vec<Record> {
    (0..count)
        .map(|i| {
            let at = list + 0x40 * i;
            Record {
                point: Vec4::load(mem, at),
                direction: Vec4::load(mem, at + 0x10),
                aux: Vec4::load(mem, at + 0x20),
                weight: mem.f32(at + 0x30),
                kind: mem.u32(at + 0x34),
                factor: mem.f32(at + 0x38),
            }
        })
        .collect()
}

/// The event `BigB` emits, in the original's layout (the weight and kind words, then the three vectors).
pub fn store_event(mem: &mut Mem, at: u32, event: &Event) {
    mem.set_f32(at, event.weight);
    mem.set_u32(at + 0x4, event.kind);
    event.aux.store(mem, at + 0x10);
    event.contact.store(mem, at + 0x20);
    event.point.store(mem, at + 0x30);
}

/// The body's contact list word (`CSolid+0x274`, also the sleep-armed word): the list's count is at +4, its 0x60-byte
/// records from +0x10.
pub const CONTACT_LIST: u32 = 0x274;

/// The address of record `k` of the body's contact list (the vote reads the list word again for each record).
pub fn contact_record(mem: &Mem, this: u32, k: u32) -> u32 {
    mem.u32(this + CONTACT_LIST) + 0x10 + 0x60 * k
}

/// The contact record at `e` (`BigA`'s fifth argument).
pub fn load_contact(mem: &Mem, e: u32) -> Contact {
    Contact {
        id: mem.u32(e + 0x4),
        f10: mem.f32(e + 0x10),
        f14: mem.f32(e + 0x14),
        f18: mem.f32(e + 0x18),
        f1c: mem.f32(e + 0x1C),
        point: Vec4::load(mem, e + 0x20),
        normal: Vec4::load(mem, e + 0x30),
        vector: Vec4::load(mem, e + 0x40),
    }
}

/// `BigA`'s outputs: the impulse pair `a` (16 bytes) and `b` (12), the force and the depth (floats).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactOut {
    pub a: u32,
    pub b: u32,
    pub force: u32,
    pub depth: u32,
}

/// `CSolid::BigA` (0x52b9f0) on memory: the contact at `e` with the range `f`. The body's def gain (`def+0x16c`), the
/// tuning float (`g_1659b70 -> +0x24 -> +0x1608c`) and the sample test (the hook at `+0x3cc` and flag 8) are read
/// first; the writes are the depth, the force when it is computed, and the pair on a hit. Returns eax (1 on a hit).
pub fn contact_worker(
    mem: &mut Mem,
    this: u32,
    out: ContactOut,
    e: u32,
    f: f32,
    host: &mut impl ContactHost,
) -> Result<u32, HostUnavailable> {
    let tuning = mem.f32(mem.u32(mem.u32(0x1659b70) + 0x24) + 0x1608C);
    let input = Input {
        contact: load_contact(mem, e),
        def_gain: mem.f32(mem.u32(this + DEFINITION) + DEF_INVERSE_MASS),
        tuning,
        f,
        sample: mem.u32(this + HOOK) != 0 && mem.u8(this + ENTITY_FLAGS) & BODY_FLAG_8 != 0,
    };
    let r = rigid_body::contact::contact_step(&input, host)?;
    mem.set_f32(out.depth, r.depth);
    if let Some(force) = r.force {
        mem.set_f32(out.force, force);
    }
    let Some(i) = r.impulse else {
        return Ok(0);
    };
    i.a.store(mem, out.a);
    for (k, v) in [i.b.x, i.b.y, i.b.z].into_iter().enumerate() {
        mem.set_f32(out.b + 4 * k as u32, v);
    }
    Ok(1)
}

/// `CSolid::Vf81x` (0x52c510) on memory: the vote into `a` and `b` (16 bytes each, written only when the body has a
/// list). Returns eax.
pub fn contact_vote(
    mem: &mut Mem,
    this: u32,
    a: u32,
    b: u32,
    host: &mut impl ContactVoteHost,
) -> Result<u32, HostUnavailable> {
    let list = mem.u32(this + CONTACT_LIST);
    let count = (list != 0).then(|| mem.u32(list + 4));
    let Some(v) = vote::vote(count, host)? else {
        return Ok(0);
    };
    v.a.store(mem, a);
    v.b.store(mem, b);
    Ok(u32::from(v.hit))
}

/// The bit of the CDef debug word (`core_layout::cdef_debug_flags`) that lets `E43Vf128` append to a ring tail.
const RING_BIT: u32 = 0x0100_0000;
/// The 0.5 that `E43Vf128` compares the pair's third lane with.
pub(crate) const HALF: u32 = 0x0079_5E18;
/// The body's hook and ring tail.
pub(super) const HOOK: u32 = 0x3CC;
pub(super) const RING_TAIL: u32 = 0x3C8;

/// The original callees of `E43Vf128`'s tail, at their entry stack pointers.
pub trait ImpulseTailCalls {
    /// Slot 12 of the body (`S30`), with the word the original has pushed for the compose: returns the body's pose.
    fn pose(&mut self, mem: &mut Mem, sp: u32, body: u32, pushed: u32) -> u32;
    /// `E33Mat34::ComposeA` (0x51b4c0) with the walker as ecx.
    fn compose_a(&mut self, mem: &mut Mem, sp: u32, point: u32, matrix: u32, out: u32);
    /// Slot 1 of the hook.
    fn hook(&mut self, mem: &mut Mem, sp: u32, hook: u32, args: [u32; 8]);
    /// `E46_RingAppend` (0x529100) on the ring tail.
    fn ring_append(&mut self, mem: &mut Mem, sp: u32, ring: u32, args: [u32; 4]);
}

/// `CSolid::E43Vf128`'s tail (from 0x52a20b), after the velocities. With a hook, its slot 1 gets the point composed
/// with the body's pose and the call's other arguments. With the physics global's ring bit and a ring tail, the ring
/// gets the point composed with the body. `frame` is the original's esp after its prologue, `((entry - 4) & !0xf) -
/// 0x80`; both composes write `frame + 0x70`.
pub fn impulse_tail(mem: &mut Mem, this: u32, args: [u32; 8], frame: u32, calls: &mut impl ImpulseTailCalls) {
    let point = args[0];
    let out = frame + 0x70;
    if mem.u32(this + HOOK) != 0 {
        let pose = calls.pose(mem, frame - 8, this, out);
        calls.compose_a(mem, frame - 0x10, point, pose, out);
        let pair = args[4];
        let word = if rigid_body::impulse::hook_takes_first(mem.f32(pair + 8), mem.f32(HALF)) {
            mem.u32(pair)
        } else {
            mem.u32(pair + 4)
        };
        let hook = mem.u32(this + HOOK);
        let hook_args = [args[2], args[3], word, args[5], args[6], out, point, args[7]];
        calls.hook(mem, frame - 0x24, hook, hook_args);
    }
    if core_layout::cdef_debug_flags(mem) & RING_BIT != 0 && mem.u32(this + RING_TAIL) != 0 {
        calls.compose_a(mem, frame - 0x10, point, this, out);
        let ring = mem.u32(this + RING_TAIL);
        calls.ring_append(mem, frame - 0x14, ring, [out, 0, 1.0f32.to_bits(), 0]);
    }
}

/// A ring's wrap flag, index and count; its float triples (`+0x410`, 12 bytes each) and its vectors (from `+0x10`).
const RING_WRAPS: u32 = 0x0;
const RING_INDEX: u32 = 0x4;
const RING_COUNT: u32 = 0x8;
const RING_FLOATS: u32 = 0x410;

/// `E46_RingAppend` (0x529100) on the ring at `ring`: the three words `a`, `b`, `c` and the vector at `src` go in at
/// the index, which then steps (`impulse::ring_step`). A full ring that does not wrap is left as it is. Returns the eax
/// the original leaves: the new index plus one (wrapping), the new count, or the index (full).
pub fn ring_append(mem: &mut Mem, ring: u32, src: u32, [a, b, c]: [u32; 3]) -> u32 {
    let wraps = mem.u32(ring + RING_WRAPS) != 0;
    let index = mem.u32(ring + RING_INDEX);
    let Some((next, count)) = rigid_body::impulse::ring_step(wraps, index, mem.u32(ring + RING_COUNT)) else {
        return index;
    };
    let floats = ring + RING_FLOATS + index.wrapping_mul(12);
    for (k, word) in [a, b, c].into_iter().enumerate() {
        mem.set_u32(floats + 4 * k as u32, word);
    }
    let vector = mem.read(src, 16);
    mem.map(ring + index.wrapping_add(1).wrapping_mul(16), &vector);
    mem.set_u32(ring + RING_INDEX, next);
    mem.set_u32(ring + RING_COUNT, count);
    if wraps {
        next + 1
    } else {
        count
    }
}
