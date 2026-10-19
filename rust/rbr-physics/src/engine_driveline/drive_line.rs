//! `CDriveLine` (`RBR_E40_DriveLine.cpp`, `RBR_E41_DriveLine.cpp`): the entry table and its workers. Names are ours.
//!
//! The table holds up to 10000 entries of 0x30 bytes from `this`. Entry `k`'s fields start at `this + 0x30k + 0x10`:
//! the position `q` (vec4, +0), the tangent `v` (vec4, +0x10), the arc length `s` (float, +0x20) and a dword
//! tag (+0x24).
//! The count is at `this + 0x75310`. Entry `k`'s `s` is also the word at the next entry's base, so `Last` reads the
//! float past the end.

use crate::engine_driveline::store::EngineStore;
use crate::fpu;
use crate::math::vec::Vec4;

/// The count of entries at `this`.
pub const COUNT_AT: u32 = 0x0007_5310;
/// The table's capacity (`DRIVELINE_MAX_N_CTLPOINTS`).
pub const MAX_ENTRIES: u32 = 10000;
/// The stride of one entry in the table (`0x30` bytes).
pub const STRIDE: u32 = 0x30;
/// The E12 host word that `Thunk4F0` loads (`ecx` for `E12_SampleX`).
const E12_HOST_AT: u32 = 0x007e_aba8;

const ZERO: Vec4 = Vec4 { x: 0.0, y: 0.0, z: 0.0, w: 0.0 };

/// One entry: the position `q`, the tangent `v`, the arc length `s` and the tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub q: Vec4,
    pub v: Vec4,
    pub s: f32,
    pub tag: u32,
}

/// The address of entry `k`'s fields, `this + 0x30k + 0x10`. The index wraps as the original's 32-bit one does.
pub fn entry_at(this: u32, k: u32) -> u32 {
    this.wrapping_add(k.wrapping_mul(STRIDE)).wrapping_add(0x10)
}

/// `acc + v * s`, lane by lane: the original's `mulps` then `addps` (two f32 roundings, no fused multiply-add).
fn scale_add(s: f32, v: Vec4, acc: Vec4) -> Vec4 {
    Vec4 { x: v.x * s + acc.x, y: v.y * s + acc.y, z: v.z * s + acc.z, w: v.w * s + acc.w }
}

/// `CDriveLine::BlendB` (0x50a650): the cubic Hermite between entries `idx` and `idx + 1` at `t`. The tangents are
/// `v` of each entry, and the weights are the two accumulators `acc1` (`t^3`) and `acc2` (`t^2`).
pub fn blend_b<S: EngineStore>(mem: &S, this: u32, idx: i32, t: f32) -> Vec4 {
    let at = entry_at(this, idx as u32);
    let (p0, v0) = (Vec4::load(mem, at), Vec4::load(mem, at + 0x10));
    let next = at.wrapping_add(STRIDE);
    let (p1, v1) = (Vec4::load(mem, next), Vec4::load(mem, next + 0x10));
    let acc1 = [(2.0, p0), (-2.0, p1), (1.0, v0), (1.0, v1)].into_iter().fold(ZERO, |acc, (s, v)| scale_add(s, v, acc));
    let acc2 =
        [(-3.0, p0), (3.0, p1), (-2.0, v0), (-1.0, v1)].into_iter().fold(ZERO, |acc, (s, v)| scale_add(s, v, acc));
    let mut f4 = t * t;
    let out = scale_add(t, v0, p0);
    let out = scale_add(f4, acc2, out);
    f4 *= t;
    scale_add(f4, acc1, out)
}

/// `CDriveLine::ClampMix` (0x50b0f0): `BlendB` at the integer part of `x` and the fraction. `x` at or above 2^31
/// uses index 0x7fffffff (wrapping to `this - 0x30`). NaN and values below -2^31 log in the original and blend at
/// index 0 and fraction 0, as do negative integer parts.
pub fn clamp_mix<S: EngineStore>(mem: &S, this: u32, x: f32) -> Vec4 {
    const TWO_31: f32 = 2_147_483_648.0;
    let (idx, t) = if x >= TWO_31 {
        (0x7fff_ffff, x - TWO_31)
    } else if !(x >= -TWO_31) {
        (0, 0.0)
    } else {
        let i = fpu::trunc_i32(f64::from(x));
        if i < 0 {
            (0, 0.0)
        } else {
            (i, x - i as f32)
        }
    };
    blend_b(mem, this, idx, t)
}

/// `CDriveLine::Last` (0x50ad40): the arc length of the last entry, `s` of entry `count - 1`. The original traps on an
/// empty table.
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

/// `E12_SampleX` (0x475ea0) on a table of `n` entries whose arc lengths are `s(k)`: the entry at `idx = floor(x)` and
/// the next one, interpolated by `x - idx` (f32 at each step, as the SSE code does); the last entry gives its own `s`.
/// None when `idx` is outside `0..n` (or x is NaN): the original calls its assert there, so a port declines.
pub fn sample_x(n: u32, s: impl Fn(u32) -> f32, x: f32) -> Option<f32> {
    let floor = x.floor();
    if !(floor >= 0.0 && floor < n as f32) {
        return None;
    }
    let idx = floor as u32;
    let first = s(idx);
    if idx + 1 >= n {
        return Some(first);
    }
    let frac = (x as f64 - idx as f64) as f32;
    Some(first + (s(idx + 1) - first) * frac)
}

/// `CDriveLine::SetDriveLine` (0x50aec0): copies the signal's `n` entries (count at +0, base at +4, each entry
/// `q`, `v`, `s`, tag at its own 0x30 stride) and stores them (`layout::store_drive_line`).
pub fn set_drive_line<S: EngineStore>(mem: &mut S, this: u32, sig: u32) {
    let n = mem.u32(sig);
    assert!(n <= MAX_ENTRIES, "CDriveLine::SetDriveLine count {n} exceeds 10000 (the original traps)");
    let base = mem.u32(sig + 4);
    let entries: Vec<_> = (0..n).map(|k| mem.load_entry(base + k * STRIDE)).collect();
    mem.store_drive_line(this, &entries);
}
