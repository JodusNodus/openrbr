//! Small E37 grid-element accessors used by collision collection.

use crate::collision::triangle::segment_triangle;
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
