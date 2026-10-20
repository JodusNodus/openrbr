//! `CWheel::Unk97a0` (0x5397a0): the two sub-objects at `+0xd00` and `+0xc90`, each called with the vector the wheel
//! builds. The edge is `layout::update_sub_objects`; its one host call is `SubCalls::sub_unk94d0`.

use crate::math::sse;
use crate::math::vec::Vec4;

/// The call `CWheel::Unk97a0` makes on a gated-on sub-object.
pub trait SubCalls {
    /// `CWheel::Unk94d0` (0x5394d0) as the asm calls it: `wheel` is the `this` register, the first stack word is the
    /// sub-object `sub`, then `v` (a stack vector), `a`, `b` and `c`. Working name `sub_unk94d0`.
    fn sub_unk94d0(&mut self, wheel: u32, sub: u32, v: Vec4, a: u32, b: u32, c: u32);
}

/// What `CWheel::Unk94d0` reads: the sub-object's axis (its `+0x20`, four lanes) and scale (`+0x5c`), the wheel's
/// `+0xa80` lanes, the vector `v` (three lanes), the accumulator (four lanes) and the stack word the asm takes as the
/// remainder's lane 3 (its frame's `-0x14` from the aligned `ebp`, never written there: the stack's residue).
#[derive(Clone, Copy, Debug)]
pub struct SubIn {
    pub axis: Vec4,
    pub scale: f32,
    pub spin: Vec4,
    pub v: Vec4,
    pub acc: Vec4,
    pub residue: f32,
}

/// What `Unk94d0` writes: the accumulator and the remainder (copied to the wheel's `+0xe40`, four lanes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubOut {
    pub acc: Vec4,
    pub rest: Vec4,
}

/// The floats at 0x742a50 (800000), 0x795e94 (0.001), 0x795e20 (-1) and 0x72afcc (0.3).
const GAIN: f32 = f32::from_bits(0x4943_5000);
const MIN_REST: f32 = f32::from_bits(0x3A83_126F);
const MINUS_ONE: f32 = f32::from_bits(0xBF80_0000);
const REST_GAIN: f32 = f32::from_bits(0x3E99_999A);

/// `acc + d * s` in four lanes, as the asm's `mulps` (the vector the first operand) then `addps` (the product first).
fn add_scaled(acc: Vec4, d: Vec4, s: f32) -> Vec4 {
    Vec4 {
        x: sse::add(sse::mul(d.x, s), acc.x),
        y: sse::add(sse::mul(d.y, s), acc.y),
        z: sse::add(sse::mul(d.z, s), acc.z),
        w: sse::add(sse::mul(d.w, s), acc.w),
    }
}

/// `CWheel::Unk94d0` (0x5394d0) (name ours): `v` split along the sub-object's axis `n` (`p = (v . n) n`, the rest
/// `r = v - p` in three lanes, lane 3 the residue). The accumulator gets `n * s` with `s = -(spin . n) * scale *
/// 800000` (the dot summed y, z, then x), then, when `|r| > 0.001`, `r * ((-1 / |r|) * s * 0.3)`. Every lane in the
/// SSE order of the asm.
pub fn split(i: &SubIn) -> SubOut {
    let (n, v) = (i.axis, i.v);
    let dot = sse::add(sse::add(sse::mul(v.x, n.x), sse::mul(v.y, n.y)), sse::mul(v.z, n.z));
    let p = Vec4 { x: sse::mul(dot, n.x), y: sse::mul(dot, n.y), z: sse::mul(dot, n.z), w: sse::mul(dot, n.w) };
    let rest = Vec4 { x: sse::sub(v.x, p.x), y: sse::sub(v.y, p.y), z: sse::sub(v.z, p.z), w: i.residue };
    // `E01Vec4::Length` (0x473b30): `sqrtss` of `(x*x + y*y) + z*z`, through st0 as a float.
    let len = sse::add(sse::add(sse::mul(rest.x, rest.x), sse::mul(rest.y, rest.y)), sse::mul(rest.z, rest.z)).sqrt();
    let k = sse::add(sse::add(sse::mul(i.spin.y, n.y), sse::mul(i.spin.z, n.z)), sse::mul(i.spin.x, n.x));
    let s = sse::mul(sse::sub(0.0, sse::mul(k, i.scale)), GAIN);
    let mut acc = add_scaled(i.acc, n, s);
    if len > MIN_REST {
        acc = add_scaled(acc, rest, sse::mul(sse::mul(sse::div(MINUS_ONE, len), s), REST_GAIN));
    }
    SubOut { acc, rest }
}
