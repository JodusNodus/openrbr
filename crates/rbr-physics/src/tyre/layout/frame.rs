//! `load_frame` and `store_frame`: the whole tyre step footprint read once and written back, and the table and
//! segment words they use.

use crate::mem::Mem;

use super::frame_types::Frame;
use crate::tyre::material::Material;

/// Stores `values` as floats starting at `at`.
pub(super) fn store_floats(mem: &mut Mem, at: u32, values: &[f32]) {
    for (k, value) in values.iter().enumerate() {
        mem.set_f32(at + 4 * k as u32, *value);
    }
}

/// The material block's copied words, by their word index in the block (the last is word 0x3e, byte +0xf8), in the
/// order the original copies them.
pub const WORDS: [u32; 39] = [
    0x14, 0x15, 0x16, 0x10, 0x11, 0x12, 0x0c, 0x0d, 0x0e, 0x04, 0x05, 0x06, 0x08, 0x09, 0x0a, 0x24, 0x25, 0x26, 0x20,
    0x21, 0x22, 0x2c, 0x2d, 0x2e, 0x28, 0x29, 0x2a, 0x3c, 0x3d, 0x3e, 0x18, 0x19, 0x1a, 0x1c, 0x1d, 0x1e, 0x38, 0x39,
    0x3a,
];

/// Copies the `CTyreMatData` words into the frame, as `Step` does at `+0x1a0`. Ten of the block's words are scalars of
/// the scratch (`+0x1e4`, `+0x1e8`, ...): the copy sets those too, so the scalars agree with the block.
pub fn copy_material(f: &mut Frame, m: &Material) {
    let s = &mut f.scratch;
    s.material = *m;
    for (i, word) in m.iter().enumerate() {
        let v = f32::from_bits(*word);
        match 0x1A0 + 4 * WORDS[i] {
            0x1E4 => s.static_friction_dirty = *word,
            0x1E8 => s.static_friction = v,
            0x1F4 => s.kinetic_friction_dirty = *word,
            0x1F8 => s.kinetic_friction = v,
            0x208 => s.static_friction_soil = v,
            0x218 => s.kinetic_friction_soil = v,
            0x228 => s.sink_factor = v,
            0x258 => s.soil_thickness = v,
            0x288 => s.water_factor = v,
            0x298 => s.rolling_resistance = v,
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
