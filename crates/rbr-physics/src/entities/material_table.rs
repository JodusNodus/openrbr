//! The material table the stage loader fills: for each source entry, the slot at its tag takes the entry's low byte and
//! high word.

use super::store::{EntityStore, SlotLookup};
use crate::ngp;

/// One source record: the material's tag (its table slot), and the low byte and high word of the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialEntry {
    pub tag: u8,
    pub low: u8,
    pub high: u32,
}

/// The two words stored in the table's slot of a tag. The low byte is zero-extended to a word: the original stores
/// the zero bytes itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialSlot {
    pub low: u32,
    pub high: u32,
}

/// The table the build produces: `self_ref` is the table's own address, `count` the number of source
/// entries, and `slots` the stored `(tag, slot)` pairs in order (a later entry with the same tag wins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialTable {
    pub self_ref: u32,
    pub count: u32,
    pub slots: Vec<(u8, MaterialSlot)>,
}

impl MaterialTable {
    /// Builds the table at address `table` from the source entries, in source order.
    pub fn build(table: u32, entries: &[MaterialEntry]) -> Self {
        let slots = entries.iter().map(|e| (e.tag, MaterialSlot { low: e.low as u32, high: e.high })).collect();
        MaterialTable { self_ref: table, count: entries.len() as u32, slots }
    }
}

/// What the indexed lookup (0x520240) found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indexed {
    Texel(u32),
    /// The table's own address (+0x800) is 0: the original clears only al of eax.
    NoTable,
    OutOfRange,
}

/// `E44MatTable` lookup by index (0x520240): slot `index` of the table at `this`, if it has one. The slot lookup is
/// NGP's (`ngp::layout::material`, in place of 1.02's `0x520010`), which also reads `b`.
pub fn sample_indexed<S: EntityStore>(store: &S, this: u32, index: u8, f: f32, b: f32) -> Indexed {
    match store.material_slot(this, index) {
        SlotLookup::NoTable => Indexed::NoTable,
        SlotLookup::OutOfRange => Indexed::OutOfRange,
        SlotLookup::At(slot) => Indexed::Texel(ngp::layout::material::sample_slot(store.mem(), slot, f, b)),
    }
}
