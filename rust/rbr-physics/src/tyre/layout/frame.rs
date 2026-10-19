//! `load_frame` and `store_frame`: the whole tyre step footprint read once and written back, and the table and
//! segment words they use.

use crate::mem::Mem;

use super::frame_types::Frame;
use crate::tyre::material::{self, Material};

/// Stores `values` as floats starting at `at`.
pub(super) fn store_floats(mem: &mut Mem, at: u32, values: &[f32]) {
    for (k, value) in values.iter().enumerate() {
        mem.set_f32(at + 4 * k as u32, *value);
    }
}

/// Copies the `CTyreMatData` words into the frame, as `Step` does at `+0x1a0`. Ten of the block's words are scalars of
/// the scratch (`+0x1e4`, `+0x1e8`, ...): the copy sets those too, so the scalars agree with the block.
pub fn copy_material(f: &mut Frame, m: &Material) {
    let s = &mut f.scratch;
    s.material = *m;
    for (i, word) in m.iter().enumerate() {
        let v = f32::from_bits(*word);
        match 0x1A0 + 4 * material::WORDS[i] {
            0x1E4 => s.dirty_1e4 = *word,
            0x1E8 => s.f_1e8 = v,
            0x1F4 => s.flag_1f4 = *word,
            0x1F8 => s.factor_1f8 = v,
            0x208 => s.f_208 = v,
            0x218 => s.f_218 = v,
            0x228 => s.g_228 = v,
            0x258 => s.f96 = v,
            0x288 => s.heat_source = v,
            0x298 => s.factor_298 = v,
            _ => {}
        }
    }
}

/// A table index the frame does not hold: `index` is outside the table's window (`-1`, the word below it, and its `len`
/// entries). The original would read memory the port does not hold, so the caller declines and the original runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outside {
    pub index: i32,
    pub len: usize,
}

/// A search or blend table as the original reads it. `at(-1)` is the word below the table: a NaN search walks to
/// index -1 and reads it. `at(i)` for `0 <= i < len` is entry `i`. The largest table has 8 entries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Table {
    pub(super) below: f32,
    pub(super) len: usize,
    pub(super) entries: [f32; 8],
}

impl Table {
    pub fn count(&self) -> usize {
        self.len
    }

    /// Scales the entries by `k` (`k * entry`, as NGP's `mulss`), and `next`'s word below, which is the last of them.
    pub fn scale(&mut self, k: f32, next: &mut Table) {
        for e in &mut self.entries[..self.len] {
            *e *= k;
        }
        next.below = self.entries[self.len - 1];
    }

    pub fn at(&self, i: i32) -> Option<f32> {
        match i {
            -1 => Some(self.below),
            i if i >= 0 && (i as usize) < self.len => Some(self.entries[i as usize]),
            _ => None,
        }
    }
}
