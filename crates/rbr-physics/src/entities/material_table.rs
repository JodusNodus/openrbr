//! The material table (`E44MatTable`, filled by `E44MatTable::Build`, 0x520190) and the CMaterial
//! tweak-key dictionary slot (`CMaterial::Unk20`, 0x520820).

/// One source record: the material's tag (its table slot), and the low byte and high word of the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialEntry {
    pub tag: u8,
    pub low: u8,
    pub high: u32,
}

/// The two words stored at `8 * tag` of the table. The low byte is zero-extended to a word: the
/// original's bytes 1-3 of it are zeros it stores itself (see `entities/README.md` (git 9c28c0b8)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialSlot {
    pub low: u32,
    pub high: u32,
}

/// The table `Build` produces: `self_ref` is the table's own address, `count` the number of source
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

/// `CMaterial::Unk20` (0x520820): the CMaterial tweak-key dictionary.
pub const TWEAK_DICT: u32 = 0x01659aec; // dict of NPhysics::CMaterial (notes/physics.md "Renames and tweak keys")

pub fn tweak_dict() -> u32 {
    TWEAK_DICT
}
