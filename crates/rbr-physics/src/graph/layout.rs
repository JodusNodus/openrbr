//! The graph's words: the period (+0x14), the ring (+0x18: base +0, sample count +4, cursor +8) and the push counters
//! (+0x40 for the 2D graph, +0x44 for the 1D one).

use crate::mem::Mem;

const PERIOD: u32 = 0x14;
const RING: u32 = 0x18;
const COUNTER_2D: u32 = 0x40;
const COUNTER_1D: u32 = 0x44;

/// `CGraph1D::Push(value)` (0x494cf0, thiscall, `ret 4`) on `graph`: the value's word goes to the ring on every
/// `period`-th push. Returns eax (the counter's quotient, or the ring after a sample); `None` where the original faults
/// (a period of 0).
pub fn push_1d(mem: &mut Mem, graph: u32, value: u32) -> Option<u32> {
    push(mem, graph, COUNTER_1D, &[value])
}

/// `CGraph2D::Push(x, y)` (0x495090, thiscall, `ret 8`) on `graph`: both words, as `push_1d`.
pub fn push_2d(mem: &mut Mem, graph: u32, x: u32, y: u32) -> Option<u32> {
    push(mem, graph, COUNTER_2D, &[x, y])
}

/// The counter is stored before the sample. The words go to the cursor; the cursor is read again after them, moved by
/// the sample's size and stored, then set back to the base when it is at the ring's end.
fn push(mem: &mut Mem, graph: u32, counter_at: u32, words: &[u32]) -> Option<u32> {
    let (counter, quotient) = super::tick(mem.u32(graph + counter_at), mem.u32(graph + PERIOD))?;
    mem.set_u32(graph + counter_at, counter);
    if counter != 0 {
        return Some(quotient);
    }
    let ring = mem.u32(graph + RING);
    let cursor = mem.u32(ring + 8);
    for (k, word) in words.iter().enumerate() {
        mem.set_u32(cursor.wrapping_add(4 * k as u32), *word);
    }
    let stride = 4 * words.len() as u32;
    let next = mem.u32(ring + 8).wrapping_add(stride);
    mem.set_u32(ring + 8, next);
    let base = mem.u32(ring);
    if super::wraps(next, base, mem.u32(ring + 4), stride) {
        mem.set_u32(ring + 8, base);
    }
    Some(ring)
}
