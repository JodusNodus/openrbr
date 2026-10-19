//! The E33/E37/E38 vector helpers on memory (`E38Vec`, `E09Vec4`, `E21Vec3`, `E01Vec4`, `E37Vec16`, `E33Vec4`).

use crate::math::vec::Vec3;
use crate::mem::Mem;

use crate::math::vec::Vec4;

/// E38Vec::M_CD10 (0x4fd10): `out = a - b`. Both inputs are read before the store. Returns `out`.
pub fn vec3_sub(mem: &mut Mem, a: u32, b: u32, out: u32) -> u32 {
    let difference = Vec3::load(mem, a) - Vec3::load(mem, b);
    difference.store(mem, out);
    out
}

/// E38Vec::M_CCB0 (0x4fccb0): `out = (a + b) - c`. All three inputs are read before the store, so exact aliasing
/// matches the asm (partial overlap does not occur in game code). Returns `out`.
pub fn vec3_add_sub(mem: &mut Mem, a: u32, b: u32, c: u32, out: u32) -> u32 {
    let sum = Vec3::load(mem, a) + Vec3::load(mem, b);
    (sum - Vec3::load(mem, c)).store(mem, out);
    out
}

/// E38Vec::M_CDD0 (0x4fdd0): `out = a + b`. Both inputs are read before the store. Returns `out`.
pub fn vec3_add(mem: &mut Mem, a: u32, b: u32, out: u32) -> u32 {
    let sum = Vec3::load(mem, a) + Vec3::load(mem, b);
    sum.store(mem, out);
    out
}

/// E38Vec::M_CF30 (0x4fcf30): `out = a x b` (`math::vec::cross3`). Both inputs are read before the store. Returns
/// `out`.
pub fn vec3_cross(mem: &mut Mem, a: u32, b: u32, out: u32) -> u32 {
    let cross = Vec3::load(mem, a).cross3(Vec3::load(mem, b));
    cross.store(mem, out);
    out
}

/// E38Vec::M_CD60 (0x4fcd60): stores `out = a - b`, then returns `out . d` (the st0 value). The difference is stored
/// before `d` is read, so `d` may be `out`.
pub fn vec3_sub_dot(mem: &mut Mem, a: u32, b: u32, out: u32, d: u32) -> f32 {
    let difference = Vec3::load(mem, a) - Vec3::load(mem, b);
    difference.store(mem, out);
    difference.dot(Vec3::load(mem, d))
}

/// E38Vec::M_CFA0 (0x4fcfa0): stores `out = a x b`, then returns `out . d` (the st0 value). As in `vec3_sub_dot`,
/// the product is stored before `d` is read, so `d` may be `out`.
pub fn vec3_cross_dot(mem: &mut Mem, a: u32, b: u32, out: u32, d: u32) -> f32 {
    let cross = Vec3::load(mem, a).cross3(Vec3::load(mem, b));
    cross.store(mem, out);
    cross.dot(Vec3::load(mem, d))
}

/// E38Vec::M_CEE0 (0x4fcee0): `a . b` (`math::vec::dot`), the value the original leaves in st0. Reads only.
pub fn vec3_dot(mem: &Mem, a: u32, b: u32) -> f32 {
    Vec3::load(mem, a).dot(Vec3::load(mem, b))
}

/// E38_004FD0F0 (0x4fd0f0): normalises the vector at `this` in place (`Vec3::normalize_sse`). Returns `this`.
pub fn vec3_normalize(mem: &mut Mem, this: u32) -> u32 {
    Vec3::load(mem, this).normalize_sse().store(mem, this);
    this
}

/// The fourth lane E09Vec4::Sub and E21Vec3::Add copy to `out`: the last word of their 16-byte scratch, never written
/// (`push ebp`, the alignment, `sub esp, 0x10`), so the stack word 4 below the aligned frame of the entry esp `esp`.
fn stale_lane(mem: &Mem, esp: u32) -> u32 {
    mem.u32(((esp - 4) & !0xf) - 4)
}

/// E09Vec4::Sub (0x465da0, thiscall on `a` with `(out, b)`, ret 8): `out = a - b` on x, y and z (`subss`, all read
/// before `out` is written), w the stale scratch word (`stale_lane`). Returns `out`.
pub fn vec4_sub_stale(mem: &mut Mem, a: u32, out: u32, b: u32, esp: u32) -> u32 {
    let w = stale_lane(mem, esp);
    (Vec3::load(mem, a) - Vec3::load(mem, b)).store(mem, out);
    mem.set_u32(out + 0xc, w);
    out
}

/// E21Vec3::Add (0x4a19e0, thiscall on `a` with `(out, b)`, ret 8): `out = b + a` on x, y and z (`addss` into `b`'s
/// lane), w the stale scratch word (`stale_lane`). Returns `out`.
pub fn vec3_add_stale(mem: &mut Mem, a: u32, out: u32, b: u32, esp: u32) -> u32 {
    let w = stale_lane(mem, esp);
    (Vec3::load(mem, b) + Vec3::load(mem, a)).store(mem, out);
    mem.set_u32(out + 0xc, w);
    out
}

/// E01Vec4::Normalize (0x43c6a0): the vector at `this` scaled in place by `Vec4::normalize`, all four lanes. Returns
/// `this`.
pub fn vec4_normalize(mem: &mut Mem, this: u32) -> u32 {
    Vec4::load(mem, this).normalize().store(mem, this);
    this
}

/// E38Vec::M_D030 (0x4fd030): `out = a * scale`. Returns `out`.
pub fn vec3_scale(mem: &mut Mem, a: u32, scale: f32, out: u32) -> u32 {
    Vec3::load(mem, a).scale(scale).store(mem, out);
    out
}

/// E01Vec4::CopyFrom (0x43c5b0, thiscall on `dst`, `(src*)`, ret 4): the sixteen bytes at `src` go to `dst`, read whole
/// before the store (`movups`), so the two may overlap. Returns `dst`.
pub fn vec4_copy_from(mem: &mut Mem, dst: u32, src: u32) -> u32 {
    Vec4::load(mem, src).store(mem, dst);
    dst
}

/// E37Vec16::Min (0x4f5480): the lanewise `minps` of the vectors at `object` and `other`, stored at `object`. Returns
/// `object`.
pub fn vec16_min(mem: &mut Mem, object: u32, other: u32) -> u32 {
    Vec4::load(mem, object).min_lanes(Vec4::load(mem, other)).store(mem, object);
    object
}

/// E37Vec16::Max (0x4f54b0): the lanewise `maxps` of the vectors at `object` and `other`, stored at `object`. Returns
/// `object`.
pub fn vec16_max(mem: &mut Mem, object: u32, other: u32) -> u32 {
    Vec4::load(mem, object).max_lanes(Vec4::load(mem, other)).store(mem, object);
    object
}

/// Two adjacent vectors used as a four-dimensional lower and upper bound.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec4Pair {
    pub lower: Vec4,
    pub upper: Vec4,
}

/// `E33Vec4::MinMaxA(src, lo, hi)` (0x4e0ae0) on the pair at `this`: `E33Vec4::Min` of the lower vectors to `lo`, then
/// `E33Vec4::Max` of the upper ones (+0x10) to `hi`, read after the first store as the two calls do. Returns `hi`, the
/// second call's eax.
pub fn min_max_a(mem: &mut Mem, this: u32, src: u32, lo: u32, hi: u32) -> u32 {
    Vec4::load(mem, this).min_lanes(Vec4::load(mem, src)).store(mem, lo);
    Vec4::load(mem, this + 0x10).max_lanes(Vec4::load(mem, src + 0x10)).store(mem, hi);
    hi
}

/// E33Vec4::Scaled (0x4e0700): stores the input vector times `scale` at `output`. `this` is the input and the scale is
/// the second stack word, after the output pointer.
pub fn scaled(mem: &mut Mem, input: u32, output: u32, scale: f32) {
    Vec4::load(mem, input).scale(scale).store(mem, output);
}

impl Vec4Pair {
    /// Loads a bound pair from game memory.
    pub fn load(mem: &Mem, at: u32) -> Self {
        Self { lower: Vec4::load(mem, at), upper: Vec4::load(mem, at + 16) }
    }

    /// Stores a bound pair to game memory.
    pub fn store(self, mem: &mut Mem, at: u32) {
        self.lower.store(mem, at);
        self.upper.store(mem, at + 16);
    }
}
