//! 0x539180 `CWheel::Blend4` (thiscall, `a`, `b`, `c`, `d`): with `k = tail+0xcc * 0.5`, `c = a*k + b` and
//! `d = a*(0 - k) + b`. The sequence (the stores, and the copies into four wheel vectors) is `layout::blend_pair`; this
//! module holds the pure part.

use crate::math::vec::Vec4;
use crate::ntweak;
use crate::wheel::store::WheelStore;

/// One blend: each lane `a * k`, rounded, then `+ b` (`mulps` then `addps`, not fused).
pub fn blend(a: Vec4, b: Vec4, k: f32) -> Vec4 {
    a.scale(k) + b
}

/// `E50BlendOps::Blend(a, b, c, d, e)` (0x5381f0, thiscall, `ret 0x14`) on the ops at `this`, from the entries `a` and `b`.
/// The words at +0x48, +0x58, +0x68 and +0x78 take `b * (d * e) + a * ((1 - e) * c)`; the others take `a * (1 - e) + b * e`.
/// Each word's `CBase` (at its -0xc) is marked dirty before the word is written, as the original does.
pub fn blend_ops<S: WheelStore>(mem: &mut S, this: u32, a: u32, b: u32, c: f32, d: f32, e: f32) {
    let rest = 1.0 - e;
    let de = d * e;
    let rest_c = rest * c;
    for at in [0x48, 0x58, 0x68, 0x78] {
        blend_word(mem, this, a, b, at, rest_c, de);
    }
    for at in [0x18, 0x28, 0x38, 0x88, 0x98, 0xA8, 0xB8, 0xE8, 0xF8] {
        blend_word(mem, this, a, b, at, rest, e);
    }
}

/// One word of `blend_ops`: `a`'s word times `wa`, plus `b`'s times `wb`, each product rounded (the x87 at 24 bits).
fn blend_word<S: WheelStore>(mem: &mut S, this: u32, a: u32, b: u32, at: u32, wa: f32, wb: f32) {
    let (va, vb) = (mem.f32(a + at), mem.f32(b + at));
    ntweak::set_dirty(mem.mem_mut(), this + at - 0xC);
    mem.set_f32(this + at, va * wa + vb * wb);
}

/// The calls `CWheel::Unk9900` makes outside the wheel (addresses in the docs). The tyre scratch and the blend ops answer
/// them with their own memory.
pub trait BlendCalls {
    /// `CTyre::CScratch::BlendTable` (0x534e70) on the tyre scratch: the two coefficients `(blendA, blendB)`.
    fn blend_coefficients(&mut self, scratch: u32, i: u32, j: u32, a: f32, b: f32) -> (f32, f32);
    /// `E50BlendOps::Blend` (0x5381f0) on the blend ops `ops`: blends the material entries `s1` and `s2` with `a`, `b`, `z`.
    fn blend_materials(&mut self, ops: u32, s1: u32, s2: u32, a: f32, b: f32, z: f32);
}
