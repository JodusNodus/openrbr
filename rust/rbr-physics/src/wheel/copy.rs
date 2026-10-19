//! 0x5390d0 `CWheel::Copy100` (thiscall, `x` and `out`): `tmp = scaled * k + offset` with `k` the tail's scale
//! (`tail+0x6c`); the car's `CCar::Xform6870(&tmp, x)` gives `x`; the block at `this+0x100` goes to `out`. The sequence is
//! `layout::transform_and_copy`; this module holds the pure part.

use crate::math::vec::Vec4;

/// The vector `Copy100` hands to `Xform6870`: each lane `scaled * k`, rounded, then `+ offset` (the asm's `mulps` then
/// `addps`; the product is not fused).
pub fn scaled_sum(scaled: Vec4, k: f32, offset: Vec4) -> Vec4 {
    scaled.scale(k) + offset
}
