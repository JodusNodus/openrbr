//! `E51Wheel::M3D030` (0x53d030): the per-frame update of one wheel (`CCar::Update9C70` calls it). The sequence is
//! `layout::update_strut`; this module holds the calls the strut answers and the pure blend.
//!
//! `M3B900` (0x53b900) reads the wheel's memory (its window reaches `this+0xec4`), so the sum is stored before that call.

use super::layout::{NAME_WEIGHT_28, NAME_WEIGHT_38};

/// The calls `M3D030` makes outside the wheel, one method each (addresses in the docs). The arguments and results are
/// typed values; the host does its own memory.
pub trait StrutCalls {
    /// `NTweak::CBase::GetName` (0x52db10) on the strut `strut`: the name object (`m_name`, +4).
    fn strut_name(&mut self, strut: u32) -> u32;
    /// `NTweak::CBase::SetDirty` (0x52f450) on the object `object` (the tweak at `this+0x5b0`).
    fn mark_dirty(&mut self, object: u32);
    /// `E51Wheel::M3B900` (0x53b900, thiscall, one float) on the wheel `wheel`: `sum` is the value stored at its
    /// `+0x5bc`. Working name `apply_dirty_sum`; its meaning is not known.
    fn apply_dirty_sum(&mut self, wheel: u32, sum: f32);
    /// `CSpringDamper::UpdateAndReset` (0x52e060) on the strut, the strut's last call.
    fn update_and_reset(&mut self, strut: u32);
}

/// The strut's blend `0.67 * name(+0x38) + 0.33 * name(+0x28)` (the asm's order: the `+0x38` product, then the `+0x28`
/// product, then the sum).
pub fn strut_blend(name_f28: f32, name_f38: f32) -> f32 {
    name_f38 * NAME_WEIGHT_38 + name_f28 * NAME_WEIGHT_28
}
