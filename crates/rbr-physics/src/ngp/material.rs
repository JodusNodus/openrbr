//! NGP's material sheet lookup: every sheet is read as 16 by 16 bytes, both coordinates scaled by 16 and clamped to
//! 0..15 ([`layout::material::sample_slot`](super::layout::material::sample_slot)); 1.02 reads the slot's own size
//! and flags, and only the first coordinate.

use crate::fpu::trunc_i32;

/// The sheet's side, in bytes.
pub const SIDE: u32 = 16;

/// A coordinate's cell: the truncation of `16 v`, negative (NaN too) to 0, from 16 up to 15.
pub fn cell(v: f32) -> u32 {
    trunc_i32(f64::from(v * SIDE as f32)).clamp(0, SIDE as i32 - 1) as u32
}
