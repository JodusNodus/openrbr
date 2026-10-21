//! NGP's material sheet lookup (`0x1004df40`), which the exe's indexed lookup (`0x520240`, its call at `0x520265`)
//! calls in place of 1.02's slot lookup (`0x520010`, `entities::layout::sample_slot`).

use crate::mem::Mem;
use crate::ngp::material::{cell, SIDE};

/// The slot's data pointer.
const SLOT_DATA: u32 = 4;

/// `0x1004df40` (thiscall `(a, b)` on the slot): the byte at `(a, b)`, 0 when the slot has no data.
pub fn sample_slot(mem: &Mem, slot: u32, a: f32, b: f32) -> u32 {
    let data = mem.u32(slot + SLOT_DATA);
    if data == 0 {
        return 0;
    }
    mem.u8(data + cell(b) * SIDE + cell(a)) as u32
}
