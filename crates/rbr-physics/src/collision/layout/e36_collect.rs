//! E36 collision primitives that produce an OBB contact from a sphere.

use crate::mem::Mem;

/// E36Get680 (0x4f2c30) (name ours): read the collision entry's active-shape pointer.
pub fn active_shape(mem: &Mem, object: u32) -> u32 {
    mem.u32(object.wrapping_add(0x680))
}

/// E36Get68a (0x4f2c40) (name ours): return collision flag bit 2 as a zero-or-one word.
pub fn flag_bit2(mem: &Mem, object: u32) -> u32 {
    u32::from((mem.u8(object.wrapping_add(0x68a)) & 4) >> 2)
}

/// E36CollideInit (0x4f2c50): initialize a collision entry's base vectors, matrix, and radii.
pub fn collide_init(mem: &mut Mem, object: u32) -> u32 {
    mem.set_f32(object.wrapping_add(0x60), 1.0);
    mem.set_f32(object.wrapping_add(0x64), 0.0);
    mem.set_f32(object.wrapping_add(0x68), 0.0);
    mem.set_f32(object.wrapping_add(0x70), 0.0);
    mem.set_f32(object.wrapping_add(0x74), 1.0);
    mem.set_f32(object.wrapping_add(0x78), 0.0);
    mem.set_f32(object.wrapping_add(0x80), 0.0);
    mem.set_f32(object.wrapping_add(0x84), 0.0);
    mem.set_f32(object.wrapping_add(0x88), 1.0);
    for offset in (0..0x10).step_by(4) {
        mem.set_u32(object.wrapping_add(offset), 0);
    }
    mem.set_f32(object.wrapping_add(0x20), 1.0);
    mem.set_f32(object.wrapping_add(0x24), 2.2);
    mem.set_f32(object.wrapping_add(0x28), 0.75);
    object
}

/// E36Nop (0x4f2cd0): empty collision callback barrier.
pub fn nop() {}

pub use crate::collision::e36::{collect, CollectHost};
