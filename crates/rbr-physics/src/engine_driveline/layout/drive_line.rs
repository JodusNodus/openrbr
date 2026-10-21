//! The drive line table in memory: up to 10000 entries of 0x30 bytes from the table's address. Entry `k`'s fields
//! start at `this + 0x30k + 0x10`: the position (vec4, +0), the tangent (vec4, +0x10), the distance (float, +0x20)
//! and a dword tag (+0x24). The count is at `this + 0x75310`. Entry `k`'s distance is also the word at the next
//! entry's base, so `last` reads the float past the end.

use crate::engine_driveline::drive_line::{self, MAX_POINTS};
use crate::engine_driveline::store::EngineStore;
use crate::math::vec::Vec4;

/// The count of entries at `this`.
pub const COUNT_AT: u32 = 0x0007_5310;
/// The table's capacity (`DRIVELINE_MAX_N_CTLPOINTS`).
pub const MAX_ENTRIES: u32 = MAX_POINTS;
/// The stride of one entry in the table (`0x30` bytes).
pub const STRIDE: u32 = 0x30;
/// The E12 host word that `Thunk4F0` loads (`ecx` for `E12_SampleX`).
const E12_HOST_AT: u32 = 0x007e_aba8;

/// The address of entry `k`'s fields, `this + 0x30k + 0x10`. The index wraps as the original's 32-bit one does.
pub fn entry_at(this: u32, k: u32) -> u32 {
    this.wrapping_add(k.wrapping_mul(STRIDE)).wrapping_add(0x10)
}

/// `CDriveLine::BlendB` (0x50a650): the spline between entries `idx` and `idx + 1` at `t`.
pub fn blend_b<S: EngineStore>(mem: &S, this: u32, idx: i32, t: f32) -> Vec4 {
    let at = entry_at(this, idx as u32);
    let (p0, v0) = (Vec4::load(mem, at), Vec4::load(mem, at + 0x10));
    let next = at.wrapping_add(STRIDE);
    let (p1, v1) = (Vec4::load(mem, next), Vec4::load(mem, next + 0x10));
    drive_line::hermite(p0, v0, p1, v1, t)
}

/// `CDriveLine::ClampMix` (0x50b0f0): `BlendB` at the segment of `x` (see `drive_line::segment_of`). An index of
/// 0x7fffffff wraps to `this - 0x30`.
pub fn clamp_mix<S: EngineStore>(mem: &S, this: u32, x: f32) -> Vec4 {
    let (idx, t) = drive_line::segment_of(x);
    blend_b(mem, this, idx, t)
}

/// `CDriveLine::Last` (0x50ad40): the distance of the last entry, entry `count - 1`. The original traps on an empty
/// table.
pub fn last<S: EngineStore>(mem: &S, this: u32) -> f32 {
    let n = mem.u32(this + COUNT_AT);
    assert!(n > 0, "CDriveLine::Last on an empty table (the original traps)");
    mem.f32(entry_at(this, n - 1) + 0x20)
}

/// `CDriveLine::Thunk4F0` (0x50a4f0): loads the E12 host word and tail-jumps to `E12_SampleX` with `id` as its float
/// argument. The E12 side is `sample_x(host, x)`.
pub fn thunk_4f0<S: EngineStore>(mem: &S, id: u32, sample_x: impl FnOnce(u32, f32) -> f32) -> f32 {
    sample_x(mem.u32(E12_HOST_AT), f32::from_bits(id))
}

/// `CDriveLine::SetDriveLine` (0x50aec0): copies the signal's `n` entries (count at +0, base at +4, each entry
/// at its own 0x30 stride) and stores them (`store_drive_line`).
pub fn set_drive_line<S: EngineStore>(mem: &mut S, this: u32, sig: u32) {
    let n = mem.u32(sig);
    assert!(n <= MAX_ENTRIES, "CDriveLine::SetDriveLine count {n} exceeds 10000 (the original traps)");
    let base = mem.u32(sig + 4);
    let entries: Vec<_> = (0..n).map(|k| mem.load_entry(base + k * STRIDE)).collect();
    mem.store_drive_line(this, &entries);
}
