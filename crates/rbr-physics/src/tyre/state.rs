//! The two state setters the wheel calls on the scratch. `SetStateField` (0x532470) writes the flag `v` to the state's
//! `+0xc` and to `this+0x47c`. `SetState48` (0x533490) writes `m_state+0x48` as `total - v` when `0 < v < total`, else 0,
//! where `total` is the def's `+0x8c` plus its `+0x7c` (the tread depth and the carcass radius, `dff[0x23] + dff[0x1f]`).
//! `ClearBias` (0x534000) has no effect: its calls to `DamperBiasSub` (0x52e390, pure; its st0 is popped) are the only
//! work the asm does, so `clear_bias` is empty and the test checks that no window changes.

use super::layout::{sig, Frame};

/// 0x532470 `CScratch::SetStateField` (thiscall, the flag as its stack word).
pub fn set_state_field(f: &mut Frame, value: u32) {
    f.state.flag_0c = value;
    f.scratch.signal[sig::FLAG] = value;
}

/// 0x533490 `CScratch::SetState48` (thiscall, the float as its stack word). The compares are the asm's: `0 < v`, then
/// `total > v`, so a NaN `v` writes 0.
pub fn set_state48(f: &mut Frame, v: f32) {
    let total = f.setup.f8c + f.setup.f7c;
    f.state.tread_48 = if 0.0 < v && total > v { total - v } else { 0.0 };
}

/// 0x534000 `CScratch::ClearBias` (thiscall, no stack words). Its `DamperBiasSub` calls return a value the asm pops, so
/// nothing is written (see the module comment).
pub fn clear_bias() {}
