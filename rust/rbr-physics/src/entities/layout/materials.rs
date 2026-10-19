//! `E44MatTable`: the build, the stores and the slot lookups.

use crate::entities::hinged;
use crate::entities::mat::{self, MaterialEntry, MaterialTable};
use crate::mem::Mem;
use crate::ngp;
use crate::Variant;

/// Source record of `E44MatTable::Build`: the count at +0x700, the tags at +0x200, the low bytes at +0
/// and the high words at +0x300 (one per entry, in order).
const SRC_COUNT: u32 = 0x700;
const SRC_TAGS: u32 = 0x200;
const SRC_HIGH: u32 = 0x300;
/// The table's own address and count, at +0x800 and +0x804; the slots are at `8 * tag`.
const TABLE_SELF: u32 = 0x800;
const TABLE_COUNT: u32 = 0x804;

/// `E44MatTable::Build` (0x520190), thiscall `(src)`: fills the table at `at` from the source record `src`.
pub fn build_material_table(mem: &mut Mem, at: u32, src: u32) {
    let count = mem.u32(src + SRC_COUNT);
    let entries: Vec<MaterialEntry> = (0..count)
        .map(|i| MaterialEntry {
            tag: mem.u8(src + SRC_TAGS + i),
            low: mem.u8(src + i),
            high: mem.u32(src + SRC_HIGH + 4 * i),
        })
        .collect();
    store_material_table(mem, at, &MaterialTable::build(at, &entries));
}

/// The stores of `E44MatTable::Build` (0x520190): the table at `at`.
pub fn store_material_table(mem: &mut Mem, at: u32, table: &MaterialTable) {
    mem.set_u32(at + TABLE_COUNT, table.count);
    mem.set_u32(at + TABLE_SELF, table.self_ref);
    for (tag, slot) in &table.slots {
        mem.set_u32(at + 8 * *tag as u32, slot.low);
        mem.set_u32(at + 8 * *tag as u32 + 4, slot.high);
    }
}

/// `E44_FillOnes` (0x51cd60): the three words at `at` become 1.
pub fn fill_ones(mem: &mut Mem, at: u32) {
    for (k, word) in hinged::unit_triple().into_iter().enumerate() {
        mem.set_u32(at + 4 * k as u32, word);
    }
}

/// A slot of the material table (0x520190's `8 * tag` entries): the sheet header bytes at +0 and the data pointer at
/// +4. The table's own address is at +0x800 and its count at +0x804.
const SLOT_WIDTH: u32 = 0;
const SLOT_HEIGHT: u32 = 1;
const SLOT_FLAGS: u32 = 2;
const SLOT_DATA: u32 = 4;
const SLOT_SIZE: u32 = 8;

/// `E44MatTable` slot lookup (0x520010) on the slot at `slot`: the data byte the sheet selects for `f`; 0 for no slot or
/// no data.
pub fn sample_slot(mem: &Mem, slot: u32, f: f32) -> Result<u32, mat::DivideByZero> {
    if slot == 0 || mem.u32(slot + SLOT_DATA) == 0 {
        return Ok(0);
    }
    let sheet = mat::Sheet {
        width: mem.u8(slot + SLOT_WIDTH),
        height: mem.u8(slot + SLOT_HEIGHT),
        wrap_flags: mem.u8(slot + SLOT_FLAGS),
    };
    let offset = mat::texel_offset(sheet, f)?;
    Ok(mem.u8(mem.u32(slot + SLOT_DATA).wrapping_add(offset)) as u32)
}

/// What the indexed lookup (0x520240) found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indexed {
    Texel(u32),
    /// The table's own address (+0x800) is 0: the original clears only al of eax.
    NoTable,
    OutOfRange,
}

/// `E44MatTable` lookup by index (0x520240): slot `index` of the table at `this`, if it has one. NGP's slot lookup
/// (`ngp::material`) also reads `b`, which 1.02 passes on and ignores.
pub fn sample_indexed(
    mem: &Mem,
    this: u32,
    index: u8,
    f: f32,
    b: f32,
    variant: Variant,
) -> Result<Indexed, mat::DivideByZero> {
    let table = mem.u32(this + TABLE_SELF);
    if table == 0 {
        return Ok(Indexed::NoTable);
    }
    if index as u32 >= mem.u32(this + TABLE_COUNT) {
        return Ok(Indexed::OutOfRange);
    }
    let slot = table + SLOT_SIZE * index as u32;
    match variant {
        Variant::Stock102 => sample_slot(mem, slot, f).map(Indexed::Texel),
        Variant::Ngp778 => Ok(Indexed::Texel(ngp::material::sample_slot(mem, slot, f, b))),
    }
}
