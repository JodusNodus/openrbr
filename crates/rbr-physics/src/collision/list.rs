//! E37 intrusive-list pointer accessors and fixed-size zero helpers.

use crate::collision::event::{FREE_LIST_COUNT, FREE_LIST_HEAD};
use crate::mem::Mem;

const FREE_LIST_TAIL: u32 = 0x008e_f7f8;

/// E37Zero3 (0x4f4b40): zero three dwords, preserving the retail store order.
pub fn zero_3(mem: &mut Mem, object: u32) {
    for offset in [4, 0, 8] {
        mem.set_u32(object + offset, 0);
    }
}

/// E37First (0x4f4b50): return the first word at the list head.
pub fn first(mem: &Mem, object: u32) -> u32 {
    mem.u32(object)
}

/// E37FirstTwin (0x4f4b60): identical first-word accessor.
pub fn first_twin(mem: &Mem, object: u32) -> u32 {
    mem.u32(object)
}

/// E37At8 (0x4f4b70): return the word at list-head offset eight.
pub fn at_8(mem: &Mem, object: u32) -> u32 {
    mem.u32(object + 8)
}

/// E37Zero4 (0x4f4b80): zero four dwords, preserving the retail store order.
pub fn zero_4(mem: &mut Mem, object: u32) {
    for offset in [4, 0, 8, 0x0c] {
        mem.set_u32(object + offset, 0);
    }
}

/// E37ListOps::Insert (0x4f4b90): push a node at the list head and update its links/count.
pub fn insert(mem: &mut Mem, head: u32, node: u32) {
    let first = mem.u32(head);
    mem.set_u32(node + 0x0c, first);
    mem.set_u32(node + 0x10, 0);
    if first != 0 {
        mem.set_u32(first + 0x10, node);
        mem.set_u32(head, node);
    } else {
        mem.set_u32(head + 4, node);
        mem.set_u32(head, node);
    }
    mem.set_u32(node + 0x14, head);
    mem.set_u32(head + 8, mem.u32(head + 8).wrapping_add(1));
}

/// E37Unlink (0x4f4bc0): unlink the three-word node and clear its local links.
pub fn unlink(mem: &mut Mem, node: u32) {
    let next = mem.u32(node);
    let prev = mem.u32(node + 4);
    let head = mem.u32(node + 8);
    if head != 0 {
        if next != 0 {
            mem.set_u32(next + 0x10, prev);
            if prev != 0 {
                mem.set_u32(prev + 0x0c, next);
            } else {
                mem.set_u32(head, next);
                mem.set_u32(node, 0);
                mem.set_u32(node + 4, 0);
                mem.set_u32(node + 8, 0);
                return;
            }
        } else if prev != 0 {
            mem.set_u32(prev + 0x0c, 0);
            mem.set_u32(head + 4, prev);
        } else {
            mem.set_u32(head, 0);
            mem.set_u32(head + 4, 0);
        }
    }
    mem.set_u32(node, 0);
    mem.set_u32(node + 4, 0);
    mem.set_u32(node + 8, 0);
}

/// E37Rate4C70 (0x4f4c70) (name ours): return 2 when no link is set, otherwise the linked rate at +0x94.
pub fn link_rate(mem: &Mem, object: u32) -> f32 {
    if mem.u32(object + 0x0c) == 0 {
        2.0
    } else {
        f32::from_bits(mem.u32(mem.u32(object + 4) + 0x94))
    }
}

/// E37PushFront (0x4f4c30): prepend a node to the global collision-event free list.
pub fn push_front(mem: &mut Mem, node: u32) {
    let old_head = mem.u32(FREE_LIST_HEAD);
    mem.set_u32(node + 0x0c, old_head);
    mem.set_u32(node + 0x10, 0);
    if old_head == 0 {
        mem.set_u32(FREE_LIST_TAIL, node);
    } else {
        mem.set_u32(old_head + 0x10, node);
    }
    mem.set_u32(FREE_LIST_HEAD, node);
    mem.set_u32(node + 0x14, FREE_LIST_HEAD);
    mem.set_u32(FREE_LIST_COUNT, mem.u32(FREE_LIST_COUNT).wrapping_add(1));
}

/// E37ListOps::Release (0x4f4ea0): decrement and unlink only when the link belongs to this head.
pub fn release(mem: &mut Mem, head: u32, node: u32) {
    if mem.u32(node + 8) == head {
        mem.set_u32(head + 8, mem.u32(head + 8).wrapping_sub(1));
        unlink(mem, node);
    }
}

/// E37ListOps::Drain (0x4f4ec0): unlink owned events, optionally demote and pool each one.
pub fn drain(mem: &mut Mem, head: u32, flag: u32) {
    let mut left = mem.u32(head + 8);
    while left != 0 {
        let node = mem.u32(head);
        let link = if node == 0 { 0 } else { node + 0x0c };
        if mem.u32(link + 8) == head {
            mem.set_u32(head + 8, mem.u32(head + 8).wrapping_sub(1));
            unlink(mem, link);
        }
        if flag != 0 && node != 0 {
            super::event::vf1cx(mem, node, 1);
        }
        left = mem.u32(head + 8);
    }
}

/// CCollisionList::Vf08x (0x4f4f40): drain the embedded head at +4 with the pool flag set.
pub fn collision_list_vf08x(mem: &mut Mem, object: u32) {
    drain(mem, object + 4, 1);
}

/// E37Drain4F70 (0x4f4f70) (name ours): drain this list using its flag word at +0xc.
pub fn drain_by_flag(mem: &mut Mem, object: u32) {
    drain(mem, object, mem.u32(object + 0x0c));
}

fn release_owner(mem: &mut Mem, node: u32) {
    let owner = mem.u32(node + 8);
    if owner != 0 {
        mem.set_u32(owner + 8, mem.u32(owner + 8).wrapping_sub(1));
        unlink(mem, node);
    }
}

/// E37Release4F50 (0x4f4f50): decrement the non-null owner and unlink the node.
pub fn release_4f50(mem: &mut Mem, node: u32) {
    release_owner(mem, node);
}

/// E37Release4F60 (0x4f4f60): tail-jump twin of E37Release4F50.
pub fn release_4f60(mem: &mut Mem, node: u32) {
    release_owner(mem, node);
}

/// E37ListOps::Set1C (0x4f5470): store the supplied pointer at list-head offset 0x1c.
pub fn set_1c(mem: &mut Mem, object: u32, value: u32) {
    mem.set_u32(object + 0x1c, value);
}

/// E37AllNonNeg (0x4f54e0): true only when each xyz lane is ordered >= zero.
pub fn all_non_neg(mem: &Mem, object: u32) -> u32 {
    (0..3).all(|index| f32::from_bits(mem.u32(object + index * 4)) >= 0.0) as u32
}

/// E37AllNonPos (0x4f5510): true only when each xyz lane is ordered <= zero.
pub fn all_non_pos(mem: &Mem, object: u32) -> u32 {
    (0..3).all(|index| f32::from_bits(mem.u32(object + index * 4)) <= 0.0) as u32
}
