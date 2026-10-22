//! The intrusive lists and signals the core walks: `RegWall`, `E34Link10::UnlinkA`, the `CMultiSignal1` slots and
//! the plain word accessors.

use super::globals::entity_list;
use crate::mem::Mem;
use crate::physics_core::signal;

/// `CPhysics::CState::RegWall` (0x5263d0) after its choice of `list`, and `E34LinkList10::PushItem` (0x4e6af0), the
/// same code: `item` goes to the tail through its link at +0x10 (next, previous, list).
pub fn push_item(mem: &mut Mem, list: u32, item: u32) {
    let tail = mem.u32(list + 4);
    mem.set_u32(item + 0x14, tail);
    mem.set_u32(item + 0x10, 0);
    if tail != 0 {
        mem.set_u32(tail + 0x10, item);
    } else {
        mem.set_u32(list, item);
    }
    mem.set_u32(list + 4, item);
    mem.set_u32(item + 0x18, list);
    mem.set_u32(list + 8, mem.u32(list + 8).wrapping_add(1));
}

/// `E34Link10::UnlinkA` (0x4e41a0) on the link at `link` (an item's +0x10): out of its list, the neighbours (or the
/// list's head and tail) joined, the link cleared. The list's count is the caller's.
pub fn unlink_item(mem: &mut Mem, link: u32) {
    let (next, prev, list) = (mem.u32(link), mem.u32(link + 4), mem.u32(link + 8));
    if list != 0 {
        if next != 0 {
            mem.set_u32(next + 0x14, prev);
        } else {
            mem.set_u32(list + 4, prev);
        }
        if prev != 0 {
            mem.set_u32(prev + 0x10, next);
        } else {
            mem.set_u32(list, next);
        }
    }
    for i in 0..3 {
        mem.set_u32(link + 4 * i, 0);
    }
}

/// `E34Link10::UnlinkA` (0x4e41a0) as a call: `unlink_item`, and the eax the original leaves, the pointer it read last
/// (the previous item when both neighbours exist, else the list). An unlisted link leaves `caller_eax`.
pub fn unlink_a(mem: &mut Mem, link: u32, caller_eax: u32) -> u32 {
    let (next, prev, list) = (mem.u32(link), mem.u32(link + 4), mem.u32(link + 8));
    unlink_item(mem, link);
    match (list, next != 0 && prev != 0) {
        (0, _) => caller_eax,
        (_, true) => prev,
        (_, false) => list,
    }
}

/// `CPhysics::CState::RegWall` (0x5263d0) on the state at `state`: `object` goes to the list its slots 0x8c, 0x80 and
/// 0x88 choose (`entity_list`), asked in that order until one answers; `slot(mem, offset)` is the virtual call. Returns
/// the eax the original leaves: the list, or the object when it goes to the third list or none (`PushItem`'s).
pub fn reg_wall(mem: &mut Mem, state: u32, object: u32, mut slot: impl FnMut(&mut Mem, u32) -> bool) -> u32 {
    let filing = if slot(mem, 0x8c) {
        (true, false, false)
    } else if slot(mem, 0x80) {
        (false, true, false)
    } else {
        (false, false, slot(mem, 0x88))
    };
    let list = entity_list(state, filing.0, filing.1, filing.2);
    push_item(mem, list, object);
    if filing.0 || filing.1 {
        list
    } else {
        object
    }
}

/// The guard word of a `CMultiSignal1` (+0): set while an emission walks its connections.
pub fn signal_guard(mem: &Mem, signal: u32) -> u32 {
    mem.u32(signal)
}

pub fn set_signal_guard(mem: &mut Mem, signal: u32, value: u32) {
    mem.set_u32(signal, value);
}

/// The slot count's address (+4 points at it; zero when there is none).
pub fn signal_count_at(mem: &Mem, signal: u32) -> u32 {
    mem.u32(signal + 4)
}

/// Whether `emit` on the signal at `signal` traps (`signal::traps` on its words). The count is read only when the
/// count pointer is set.
pub fn emit_traps(mem: &Mem, signal: u32) -> bool {
    let count_at = signal_count_at(mem, signal);
    let count = if count_at == 0 { 0 } else { mem.u32(count_at) };
    signal::traps(count_at != 0, signal_guard(mem, signal), count)
}

/// The list of slot `index` of a `CMultiSignal1` (a `CSignal` at +8, 16 bytes each; the list is at its +0).
pub fn signal_slot(signal: u32, index: u32) -> u32 {
    signal + 8 + 16 * index
}

/// The overflow list of a `CMultiSignal1` (+0x208).
pub fn signal_overflow(signal: u32) -> u32 {
    signal + 0x208
}

/// The head of a connection list (`CLinkList` at +0; zero when empty).
pub fn list_head(mem: &Mem, list: u32) -> u32 {
    mem.u32(list)
}

/// The next link of a connection (+4).
pub fn link_next(mem: &Mem, link: u32) -> u32 {
    mem.u32(link + 4)
}

/// The word at `index` of a flag class's name or detail table (`table` is its address).
pub fn table_word(mem: &Mem, table: u32, index: u32) -> u32 {
    mem.u32(table + 4 * index)
}

/// The state word of a mounted body's enable flag: the pointer at +0x580 points at it.
pub fn mounted_state(mem: &Mem, body: u32) -> u32 {
    mem.u32(mem.u32(body + 0x580))
}

/// Points a mounted body's enable flag (+0x580) at the state word `state`.
pub fn set_mounted_state(mem: &mut Mem, body: u32, state: u32) {
    mem.set_u32(body + 0x580, state);
}

/// The function at `object`'s vtable byte offset `slot_offset`.
pub fn vtable_target(mem: &Mem, object: u32, slot_offset: u32) -> u32 {
    mem.u32(mem.u32(object) + slot_offset)
}

/// The word at `at`.
pub fn word(mem: &Mem, at: u32) -> u32 {
    mem.u32(at)
}

/// The float at `at`.
pub fn f32(mem: &Mem, at: u32) -> f32 {
    mem.f32(at)
}

/// Writes the float at `at`.
pub fn set_f32(mem: &mut Mem, at: u32, value: f32) {
    mem.set_f32(at, value);
}

/// Copies the 16 bytes of the vector at `from` to `to` (as four words, so the bits are kept).
pub fn copy_vec4(mem: &mut Mem, from: u32, to: u32) {
    let words = [0, 4, 8, 12].map(|k| mem.u32(from + k));
    for (k, value) in [0, 4, 8, 12].into_iter().zip(words) {
        mem.set_u32(to + k, value);
    }
}

/// Writes the word at `at`.
pub fn set_word(mem: &mut Mem, at: u32, value: u32) {
    mem.set_u32(at, value);
}

/// The owner of a connection (+0x10): the callback that its notify calls (`E46FuncConn`).
pub fn conn_owner(mem: &Mem, conn: u32) -> u32 {
    mem.u32(conn + 0x10)
}
