//! The material table the stage loader fills: for each source entry, the slot at its tag takes the entry's low byte and
//! high word.

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
