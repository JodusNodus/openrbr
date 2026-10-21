//! E37 event sorting and scalar getter leaves.

use super::event_pool::EVENT_COUNT;
use crate::mem::Mem;

/// CCollisionEvent::Get94 (0x4f3f80): return the stored penetration scalar.
pub fn event_get_94(mem: &Mem, event: u32) -> f32 {
    mem.f32(event + 0x94)
}

/// E37GetF7F0 (0x4f4070) (name ours): read the live event count.
pub fn live_event_count(mem: &Mem) -> u32 {
    mem.u32(EVENT_COUNT)
}

/// E37Zero4090 (0x4f4090).
pub const fn zero_4090() -> u32 {
    0
}

/// E37Nop40A0 (0x4f40a0).
pub const fn nop_40a0() {}

/// E37Zero40B0 (0x4f40b0).
pub const fn zero_40b0() -> u32 {
    0
}

/// E37Get795E14a (0x4f40c0).
pub const fn get_795e14a() -> f32 {
    1.0
}

/// E37Get795E14b (0x4f40d0).
pub const fn get_795e14b() -> f32 {
    1.0
}

/// E37Get72AFCC (0x4f40e0).
pub const fn get_72afcc() -> f32 {
    0.3
}

/// E37CmpInt (0x4f3f90): compare the unsigned first word of two sort records.
pub fn cmp_int(mem: &Mem, a: u32, b: u32) -> u32 {
    let x = mem.u32(a);
    let y = mem.u32(b);
    (i32::from(y < x) - i32::from(x < y)) as u32
}

/// E37CmpFloat (0x4f3fb0): compare the float at +4; retail orders less-than before greater-than.
pub fn cmp_float(mem: &Mem, a: u32, b: u32) -> u32 {
    let x = mem.f32(a + 4);
    let y = mem.f32(b + 4);
    (i32::from(x < y) - i32::from(x > y)) as u32
}

/// E37FindInt (0x4f4030): binary-search 8-byte records by their unsigned first word.
pub fn find_int(mem: &Mem, base: u32, count: u32, key: u32) -> u32 {
    let mut first = base;
    let mut left = count;
    while left > 1 {
        let half = left >> 1;
        let middle = first + half * 8;
        if mem.u32(middle) <= key {
            first = middle;
            left -= half;
        } else {
            left = half;
        }
    }
    if left == 0 || mem.u32(first) != key {
        0
    } else {
        first
    }
}

/// E37Init70Obj::Init70 (0x4f4080): load the four-float block at +0x70.
pub fn init_70(mem: &mut Mem, object: u32, out: u32) -> u32 {
    mem.copy(out, object + 0x70, 16);
    out
}
