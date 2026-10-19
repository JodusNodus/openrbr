//! Small E37 grid-element accessors used by collision collection.

use crate::math::vec::Vec3;
use crate::mem::Mem;

/// E37GridOps::T68A (0x4f5890).
pub fn t68a(mem: &Mem, elem: u32) -> u32 {
    u32::from(mem.u8(elem + 0x68a) & 1)
}

/// E37GridOps::AtE8 (0x4f58a0).
pub fn at_e8(elem: u32, index: i32) -> u32 {
    let offset = index.wrapping_add(0x22).wrapping_mul(0x10) as u32;
    elem.wrapping_add(offset)
}

/// E37GridOps::AtE10 (0x4f58b0).
pub fn at_e10(elem: u32, index: i32) -> u32 {
    let offset = (u32::from(index as u8) + 4) * 0x10;
    elem.wrapping_add(offset)
}

/// E37GridOps::Idx7 (0x4f58c0).
pub fn idx7(mem: &Mem, elem: u32, index: i32) -> u32 {
    let base = mem.u32(elem + 0x410) as i32;
    base.wrapping_add(index.wrapping_mul(7)) as u32
}

/// E37GridOps::Get684 (0x4f58e0).
pub fn get_684(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x684)
}

/// E37GridOps::Set68A (0x4f58f0).
pub fn set_68a(mem: &mut Mem, elem: u32, value: u32) -> u32 {
    let flag = mem.u8(elem + 0x68a);
    let flag = if value != 0 { flag | 1 } else { flag & !1 };
    mem.map(elem + 0x68a, &[flag]);
    (value & 0xffff_ff00) | u32::from(flag)
}

/// E37GridOps::T158_2 (0x4f5920).
pub fn t158_2(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x158) & 2
}

/// E37GridOps::T158_4 (0x4f5930).
pub fn t158_4(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x158) & 4
}

/// E37GridOps::Get30 (0x4f5940).
pub fn get_30(mem: &Mem, elem: u32) -> u32 {
    u32::from(mem.u8(elem + 0x30))
}

/// E37GridOps::Get04 (0x4f5950).
pub fn get_04(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 4)
}

/// E37GridOps::Set04 (0x4f5960).
pub fn set_04(mem: &mut Mem, elem: u32, value: u32) {
    mem.set_u32(elem + 4, value);
}

/// E37GridOps::Get00 (0x4f5970).
pub fn get_00(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem)
}

/// E37GridOps::T04_2 (0x4f5980).
pub fn t04_2(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 4) & 2
}

/// E37GridOps::Valid5990 (0x4f5990).
pub fn valid_5990(mem: &Mem, elem: u32) -> u32 {
    if mem.u16(elem + 0xc) != 0 || mem.u32(elem + 0x2c) != 0 {
        1
    } else {
        0
    }
}

/// E37GridOps::Get08 (0x4f59b0).
pub fn get_08(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 8)
}

/// E37GridOps::Get0Cw (0x4f59c0).
pub fn get_0c_w(mem: &Mem, elem: u32) -> u32 {
    u32::from(mem.u16(elem + 0xc))
}

/// E37GridOps::Get10 (0x4f59d0).
pub fn get_10(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x10)
}

/// E37GridOps::Get0Ew (0x4f59e0).
pub fn get_0e_w(mem: &Mem, elem: u32) -> u32 {
    u32::from(mem.u16(elem + 0xe))
}

/// E37GridOps::Get14 (0x4f59f0).
pub fn get_14(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x14)
}

/// E37GridOps::Get18 (0x4f5a00).
pub fn get_18(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x18)
}

/// E37GridOps::Get1C (0x4f5a10).
pub fn get_1c(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x1c)
}

/// E37GridOps::Get20w (0x4f5a20).
pub fn get_20_w(mem: &Mem, elem: u32) -> u32 {
    u32::from(mem.u16(elem + 0x20))
}

/// E37GridOps::Get22w (0x4f5a30).
pub fn get_22_w(mem: &Mem, elem: u32) -> u32 {
    u32::from(mem.u16(elem + 0x22))
}

/// E37GridOps::Get24 (0x4f5a40).
pub fn get_24(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x24)
}

/// E37GridOps::Get28 (0x4f5a50).
pub fn get_28(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x28)
}

/// E37GridOps::Get2C (0x4f5a60).
pub fn get_2c(mem: &Mem, elem: u32) -> u32 {
    mem.u32(elem + 0x2c)
}

/// E37Zero5A70 (0x4f5a70).
pub fn zero_5a70(mem: &mut Mem) {
    mem.set_u32(0x8f7010, 0);
}

/// E37Get5A80 (0x4f5a80).
pub fn get_5a80(mem: &Mem) -> u32 {
    mem.u32(0x8f7010)
}

/// E37Nop5CF0 (0x4f5cf0).
pub fn nop_5cf0() {}

/// The terms `E37Compose9` (0x4f5d00) writes: the sum of the three cross products of the first three vectors, and four
/// dot products. Their meanings are not known yet (`collision/README.md` (git 9c28c0b8), unknown meaning).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TriangleTerms {
    /// Output p5: `n1 + (n2 + n3)`, where `n1 = p1 x p2`, `n2 = p2 x p3`, `n3 = p3 x p1`.
    pub cross_sum: Vec3,
    /// Output p6: `p4 . n1`.
    pub dot_p4_n1: f32,
    /// Output p7: `p4 . n2`.
    pub dot_p4_n2: f32,
    /// Output p8: `p4 . n3`.
    pub dot_p4_n3: f32,
    /// Output p9: `p3 . n1`, summed with the y and z terms first.
    pub dot_p3_n1: f32,
}

/// E37Compose9 (0x4f5d00).
pub fn triangle_plane_terms(p1: Vec3, p2: Vec3, p3: Vec3, p4: Vec3) -> TriangleTerms {
    let n1 = p1.cross3(p2);
    let n2 = p2.cross3(p3);
    let n3 = p3.cross3(p1);
    TriangleTerms {
        cross_sum: n1 + (n2 + n3),
        dot_p4_n1: p4.dot(n1),
        dot_p4_n2: p4.dot(n2),
        dot_p4_n3: p4.dot(n3),
        dot_p3_n1: p3.dot_yz_first(n1),
    }
}

/// The hit record `E37Tri5A90` (0x4f5a90) writes to its output. The asm writes a field as it goes, so a miss keeps the
/// fields written before its exit. Unset fields were not written.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SegmentTriangle {
    pub hit: bool,
    /// Output +0x14: the segment parameter, `(n2 . d2) * w`.
    pub t: Option<f32>,
    /// Output +0x18: `f1` (raw on a miss after the first test), `w * f1` on a hit. Meaning unknown.
    pub u: Option<f32>,
    /// Output +0x1c: `f1'` (raw on a miss after the second test), `f1' * w` on a hit. Meaning unknown.
    pub v: Option<f32>,
    /// Output +0x00..+0x08: `d1 x d2`.
    pub normal: Option<Vec3>,
}

/// The dot product in the order `E37Tri5A90` and `E37Run56C0` sum it (asm 0x4f5b29, 0x4f5b95, 0x4f5bf0, 0x4f5c27; 0x4f5776,
/// 0x4f57d2, 0x4f5834, 0x4f586e): `(z + y) + x`.
/// `Vec3::dot` sums `(x + y) + z`, which rounds differently.
fn dot_zyx(a: Vec3, b: Vec3) -> f32 {
    (a.z * b.z + a.y * b.y) + a.x * b.x
}

/// E37Tri5A90 (0x4f5a90): the segment from `p1` along `p2` against the triangle `p3`, `p4`, `p5`.
pub fn segment_triangle(p1: Vec3, p2: Vec3, p3: Vec3, p4: Vec3, p5: Vec3) -> SegmentTriangle {
    let mut out = SegmentTriangle::default();
    let d1 = p4 - p3;
    let d2 = p5 - p3;
    let n1 = p2.cross3(d2);
    let f2 = dot_zyx(n1, d1);
    if !(1.0e-6f32 <= f2) {
        return out;
    }

    let t1 = p1 - p3;
    let f1 = dot_zyx(t1, n1);
    out.u = Some(f1);
    if !(0.0 <= f1 && f1 <= f2) {
        return out;
    }

    let n2 = t1.cross3(d1);
    let f1_next = dot_zyx(p2, n2);
    out.v = Some(f1_next);
    if !(0.0 <= f1_next && f1_next + f1 <= f2) {
        return out;
    }

    let w = 1.0 / f2;
    out.t = Some(dot_zyx(n2, d2) * w);
    out.u = Some(w * f1);
    out.v = Some(f1_next * w);
    out.normal = Some(d1.cross3(d2));
    out.hit = true;
    out
}

/// E37Run56C0 (0x4f56c0), directed ray/triangle test: the scalar hit distance, or `None` when the ray misses. The
/// tests take the unordered-NaN branch as the asm does (`x87_le_unordered`).
pub fn ray_triangle_distance(origin: Vec3, direction: Vec3, base: Vec3, end1: Vec3, end2: Vec3) -> Option<f32> {
    let d1 = end1 - base;
    let d2 = end2 - base;
    let n1 = direction.cross3(d2);
    let f3 = dot_zyx(n1, d1);
    if !x87_le_unordered(1.0e-6f32, f3) {
        return None;
    }

    let t1 = origin - base;
    let f1 = dot_zyx(t1, n1);
    if !x87_le_unordered(0.0, f1) || !x87_le_unordered(f1, f3) {
        return None;
    }

    let n2 = t1.cross3(d1);
    let f2 = dot_zyx(direction, n2);
    if !x87_le_unordered(0.0, f2) || !x87_le_unordered(f2 + f1, f3) {
        return None;
    }

    Some(dot_zyx(n2, d2) / f3)
}

fn x87_le_unordered(left: f32, right: f32) -> bool {
    // The retail FCOMI/JBE sequence also takes the branch for unordered NaNs.
    !(left > right)
}

/// E37GridOps::Elem5F20 (0x4f5f20).
pub fn elem_5f20(mem: &Mem, this: u32, index: u32, p3: u32, p4: u32) -> f32 {
    let table = mem.u32(this.wrapping_add(0x410));
    let row = table.wrapping_add(index.wrapping_mul(7));
    if mem.u8(row.wrapping_add(3)) == 0xff {
        return 2.0;
    }

    let vertex = |slot: u32| this.wrapping_add((u32::from(mem.u8(row.wrapping_add(slot))) + 4).wrapping_mul(0x10));
    let start = Vec3::load(mem, p3);
    let end = Vec3::load(mem, p4);
    let hit = segment_triangle(
        start,
        end - start,
        Vec3::load(mem, vertex(0)),
        Vec3::load(mem, vertex(1)),
        Vec3::load(mem, vertex(2)),
    );
    match hit.t {
        Some(t) if hit.hit && 0.0 < t && t < 1.0 => t,
        _ => 2.0,
    }
}
