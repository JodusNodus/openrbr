//! NGP's material sheet lookup (`0x1004df40`), which the exe's indexed lookup (`0x520240`, its call at `0x520265`)
//! calls in place of 1.02's slot lookup (`0x520010`, `entities::layout::sample_slot`). Every sheet is read as 16 by
//! 16 bytes, both coordinates scaled by 16 and clamped to 0..15; 1.02 reads the slot's own size and flags, and only
//! the first coordinate.

use crate::fpu::trunc_i32;
use crate::mem::Mem;

const SCALE: f32 = f32::from_bits(0x4180_0000); // 10148bdc, 16
const SIDE: u32 = 16;
/// The slot's data pointer.
const SLOT_DATA: u32 = 4;

/// A coordinate's cell: `cvttss2si` of `16 v`, negative (NaN too) to 0, from 16 up to 15.
fn cell(v: f32) -> u32 {
    trunc_i32(f64::from(v * SCALE)).clamp(0, SIDE as i32 - 1) as u32
}

/// `0x1004df40` (thiscall `(a, b)` on the slot): the byte at `(a, b)`, 0 when the slot has no data.
pub fn sample_slot(mem: &Mem, slot: u32, a: f32, b: f32) -> u32 {
    let data = mem.u32(slot + SLOT_DATA);
    if data == 0 {
        return 0;
    }
    mem.u8(data + cell(b) * SIDE + cell(a)) as u32
}
