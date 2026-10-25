//! `E44MatTable`: the build, the stores and the slot lookups.

use crate::entities::material_table::{MaterialEntry, MaterialTable};
use crate::mem::Mem;

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
    // the words' meaning is not known
    for k in 0..3 {
        mem.set_u32(at + 4 * k, 1);
    }
}
