//! The body's contact list: the records and events, the contact record and the ring's words.

use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::rigid_body::contact::Contact;
use crate::rigid_body::contact_list::{Event, Record};
use crate::rigid_body::store::RingHeader;

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

/// The contact record at `e` (`BigA`'s fifth argument). Offsets are the original's; names are `contact.rs`'s
/// (by use in the force formulas).
pub fn load_contact(mem: &Mem, e: u32) -> Contact {
    Contact {
        id: mem.u32(e + 0x4),
        stiffness: mem.f32(e + 0x10),
        force_cap: mem.f32(e + 0x14),
        tangent_cap_0: mem.f32(e + 0x18),
        tangent_cap_1: mem.f32(e + 0x1C),
        point: Vec4::load(mem, e + 0x20),
        normal: Vec4::load(mem, e + 0x30),
        vector: Vec4::load(mem, e + 0x40),
    }
}

/// The 0.5 that `E43Vf128` compares the pair's third lane with.
pub(crate) const HALF: u32 = 0x0079_5E18;
/// The body's hook and ring tail.
pub(super) const HOOK: u32 = 0x3CC;
pub(super) const RING_TAIL: u32 = 0x3C8;

/// A ring's wrap flag, index and count; its float triples (`+0x410`, 12 bytes each) and its vectors (from `+0x10`).
const RING_WRAPS: u32 = 0x0;
const RING_INDEX: u32 = 0x4;
const RING_COUNT: u32 = 0x8;
const RING_FLOATS: u32 = 0x410;

/// The ring's wrap flag, index and count.
pub(super) fn ring_header(mem: &Mem, ring: u32) -> RingHeader {
    RingHeader {
        wraps: mem.u32(ring + RING_WRAPS) != 0,
        index: mem.u32(ring + RING_INDEX),
        count: mem.u32(ring + RING_COUNT),
    }
}

/// The entry at `index`: its three float words, and the vector at `src` copied into the vector slot after the header.
pub(super) fn store_ring_entry(mem: &mut Mem, ring: u32, index: u32, words: [u32; 3], src: u32) {
    let floats = ring + RING_FLOATS + index.wrapping_mul(12);
    for (k, word) in words.into_iter().enumerate() {
        mem.set_u32(floats + 4 * k as u32, word);
    }
    mem.copy(ring + index.wrapping_add(1).wrapping_mul(16), src, 16);
}

pub(super) fn store_ring_cursor(mem: &mut Mem, ring: u32, index: u32, count: u32) {
    mem.set_u32(ring + RING_INDEX, index);
    mem.set_u32(ring + RING_COUNT, count);
}
