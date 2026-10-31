//! The `E46Vec`/`E46Mat` rows and the table-index output, the body's force words, and the math calls over them
//! (`E46Vec::Add`, `E46Mat::Mul`/`ScaleRows`, `CSolid::E43Vf60`, `E46_Offset540`).

use crate::math::table::TableIndex;
use crate::mem::Mem;
use crate::physics_core::math::{Mat, Vec3, Vec4};

/// The calls `E46_Accum250` (`CSolid::E43Vf60`) makes: the hook `S00` (vtable 0, when the log flag 0x4000 is set), the
/// base `CEntity::E43Vf60`, the point `S30` (vtable 0x30, given the out pointer the original pushes), the walk
/// `E43PairWalk::MB560` on the walker (copy, point, out) and the accumulate `S70` (vtable 0x70, the scaled force).
pub trait AccumHost {
    fn hook(&mut self, body: u32);
    fn base(&mut self, body: u32);
    fn point(&mut self, body: u32, out: u32) -> u32;
    fn walk(&mut self, copy: u32, point: u32, out: u32);
    fn accumulate(&mut self, body: u32, force: u32);
}

/// `n` consecutive 4-float rows at `at`.
pub fn vec4s(mem: &Mem, at: u32, n: usize) -> Vec<Vec4> {
    (0..n as u32).map(|i| Vec4::load(mem, at + 16 * i)).collect()
}

/// Writes 4-float rows at `at`.
pub fn set_vec4s(mem: &mut Mem, at: u32, rows: &[Vec4]) {
    for (i, v) in rows.iter().enumerate() {
        v.store(mem, at + 16 * i as u32);
    }
}

/// `N` consecutive 3-float vectors at `at`.
pub fn vec3s<const N: usize>(mem: &Mem, at: u32) -> [Vec3; N] {
    std::array::from_fn(|i| Vec3::load(mem, at + 12 * i as u32))
}

/// Writes 3-float vectors at `at`.
pub fn set_vec3s(mem: &mut Mem, at: u32, vs: &[Vec3]) {
    for (i, v) in vs.iter().enumerate() {
        v.store(mem, at + 12 * i as u32);
    }
}

/// The `E46Mat` at `at`: nine floats row-major, then the tenth.
pub fn mat(mem: &Mem, at: u32) -> Mat {
    Mat {
        rows: std::array::from_fn(|r| std::array::from_fn(|c| mem.f32(at + 4 * (3 * r + c) as u32))),
        extra: mem.f32(at + 36),
    }
}

/// `n` floats at `at`.
pub fn floats(mem: &Mem, at: u32, n: usize) -> Vec<f32> {
    (0..n as u32).map(|i| mem.f32(at + 4 * i)).collect()
}

/// The `E46_IdxWrite` output at `at`: `lo`, `hi` (int words) and `frac` (float).
pub fn set_table_index(mem: &mut Mem, at: u32, out: TableIndex) {
    mem.set_u32(at, out.lo as u32);
    mem.set_u32(at + 4, out.hi as u32);
    mem.set_f32(at + 8, out.frac);
}

/// The `lo` hint of the `E46_IdxWrite` output at `at`.
pub fn table_index_hint(mem: &Mem, at: u32) -> i32 {
    mem.u32(at) as i32
}

/// The body's force accumulator (`CSolid` +0x370, four floats).
pub fn force_accum(mem: &Mem, body: u32) -> Vec4 {
    Vec4::load(mem, body + 0x370)
}

/// Writes the body's force accumulator.
pub fn set_force_accum(mem: &mut Mem, body: u32, force: Vec4) {
    set_vec4s(mem, body + 0x370, &[force]);
}

/// The body's force scale (`+0xd0`, the `k` of `accumulate_force`).
pub fn force_scale(mem: &Mem, body: u32) -> f32 {
    mem.f32(body + 0xd0)
}

/// The inverse mass of the body's object at `+0x34` (`+0x16c` of that object), the value `gravity_force` scales by.
pub fn inverse_mass(mem: &Mem, body: u32) -> f32 {
    mem.f32(mem.u32(body + 0x34) + 0x16c)
}

/// The word at `+0x1dc` (`CSolid::E43Vf80`). Cleared with the body's dirty flags; its meaning is not known.
pub fn cache_1dc(mem: &Mem, body: u32) -> u32 {
    mem.u32(body + 0x1dc)
}

/// The word at `+0x1ec` (`CSolid::E43Vf8C`). Cleared with the body's dirty flags; its meaning is not known.
pub fn cache_1ec(mem: &Mem, body: u32) -> u32 {
    mem.u32(body + 0x1ec)
}

/// `E46Vec::Add` (0x528b00) on the vector at `this`: adds it to each of the `n` rows at `dst`.
pub fn vec_add(mem: &mut Mem, this: u32, dst: u32, n: u32) {
    let v = Vec4::load(mem, this);
    let mut rows = vec4s(mem, dst, n as usize);
    for row in &mut rows {
        *row = *row + v;
    }
    set_vec4s(mem, dst, &rows);
}

/// `E46Mat::Mul` (0x528c20) on the matrix at `this` (its nine words; the tenth is not read): `out` takes the three rows
/// at `b` times it (`Mat::mul_rows`), written after all are read, so `out` may be `b`.
pub fn mat_mul(mem: &mut Mem, this: u32, out: u32, b: u32) {
    let m = Mat { rows: vec3s::<3>(mem, this).map(|r| [r.x, r.y, r.z]), extra: 0.0 };
    let rows = m.mul_rows(vec3s::<3>(mem, b));
    set_vec3s(mem, out, &rows);
}

/// `E46Mat::ScaleRows` (0x528dd0) on the matrix at `this`: scales each of the `n` rows at `dst` (none for 0).
pub fn mat_scale_rows(mem: &mut Mem, this: u32, dst: u32, n: u32) {
    if n == 0 {
        return;
    }
    let m = mat(mem, this);
    let mut rows = vec4s(mem, dst, n as usize);
    m.scale_rows(&mut rows);
    set_vec4s(mem, dst, &rows);
}

pub use crate::physics_core::accumulate::{accumulate_forces, offset_540};
