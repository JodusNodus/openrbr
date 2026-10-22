//! E36 collision primitives that produce an OBB contact from a sphere.

use crate::mem::Mem;

use super as layout;

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

/// Calls owned by E43/E45 while E36Collect walks the collision grid.
/// The host calls of `collect`. `sp` is the callee's entry stack pointer at the original's call site (see
/// `collect`'s `frame`): the callee reads the arguments the original pushed there, and what its frame leaves below
/// `sp` is part of what the original's caller reads back.
pub trait CollectHost {
    fn virtual_call(&mut self, mem: &mut Mem, sp: u32, target: u32, args: &[u32]) -> u32;
    fn transform(&mut self, mem: &mut Mem, sp: u32, target: u32, this: u32, a: u32, b: u32, c: u32);
    fn query_cells(&mut self, mem: &mut Mem, sp: u32, lo: u32, hi: u32, cells: u32);
    fn length(&mut self, mem: &Mem, sp: u32, vector: u32) -> f32;
}

fn transform(
    mem: &mut Mem,
    owner: u32,
    a: u32,
    matrix: u32,
    c: u32,
    finish: u32,
    frame: u32,
    host: &mut impl CollectHost,
) {
    let vtable = mem.u32(owner);
    let target = mem.u32(vtable.wrapping_add(12 * 4));
    // Both calls are made from the same pushes: the original pushes a, matrix and c once, and the virtual call
    // leaves them on the stack for the finish call (0x4f399c, 0x4f39a1).
    let sp = frame.wrapping_sub(16);
    let receiver = host.virtual_call(mem, sp, target, &[owner, a, matrix, c]);
    host.transform(mem, sp, finish, receiver, a, matrix, c);
}

fn collision_pointer(mem: &mut Mem, owner: u32, frame: u32, host: &mut impl CollectHost) -> u32 {
    let vtable = mem.u32(owner);
    let target = mem.u32(vtable.wrapping_add(70 * 4));
    host.virtual_call(mem, frame.wrapping_sub(4), target, &[owner])
}

/// E36Collect (0x4f3950): transform the query box, traverse its four grid buckets, and collect OBB/sphere contacts.
/// E43 transforms, the body virtuals, and E45's grid selector remain explicit host calls.
pub fn collect(
    mem: &mut Mem,
    frame: u32,
    object: u32,
    body: u32,
    list_out: u32,
    count_out: u32,
    host: &mut impl CollectHost,
) -> u32 {
    // The query's arguments are the original's stack words: lo at +0x28, hi at +0x34 and the cells at +0x40, from the
    // frame (`call_at`'s pushes at frame-16 leave them). The query writes only some cells, so the rest read what the
    // original's frame held there.
    let cells = frame.wrapping_add(0x40);
    let lo = frame.wrapping_add(0x28);
    let hi = frame.wrapping_add(0x34);
    // The box and the sphere are the original's frame words too (+0x50, +0xa0): a callee's free stack is below the
    // frame, so they keep across the host calls.
    let box_at = frame.wrapping_add(0x50);
    let sphere_at = frame.wrapping_add(0xA0);
    // The original reuses the box's trailing stack words for its scaled vectors
    // and its first three transformed axes before passing the 0x50 SAT input.
    let axis_x = box_at + 0x20;
    let axis_y = box_at + 0x30;
    let axis_z = box_at + 0x40;
    let scaled_x = box_at + 0x80;
    let scaled_y = box_at + 0x70;
    let scaled_z = box_at + 0x90;
    let matrix = mem.u32(0x7CBB00);

    mem.set_u32(count_out, 0);
    if collision_pointer(mem, body, frame, host) == 0 {
        return 0;
    }

    transform(mem, body, object, matrix, object + 0x10, 0x51B4C0, frame, host);
    transform(mem, body, object + 0x60, matrix, object + 0x30, 0x51B560, frame, host);
    transform(mem, body, object + 0x70, matrix, object + 0x40, 0x51B560, frame, host);
    transform(mem, body, object + 0x80, matrix, object + 0x50, 0x51B560, frame, host);

    for axis in 0..3 {
        let center = mem.f32(object + 0x10 + axis * 4);
        mem.set_f32(lo + axis * 4, center - 0.1);
        mem.set_f32(hi + axis * 4, center + 0.1);
    }
    host.query_cells(mem, frame.wrapping_sub(16), lo, hi, cells);

    for index in 0..4 {
        if mem.u32(count_out) > 7 {
            break;
        }
        let cell = mem.u32(cells + index * 4);
        let mut node = if cell == 0 { 0 } else { mem.u32(cell) };
        while node != 0 && mem.u32(count_out) < 8 {
            let owner = mem.u32(node + 12);
            let collision = collision_pointer(mem, owner, frame, host);
            if collision != 0 {
                let hit = mem.u32(collision + 0x680);
                if hit != 0 {
                    let contact = if mem.u8(collision + 0x68A) & 4 == 0 {
                        transform(mem, owner, hit, matrix, box_at, 0x51B4C0, frame, host);
                        layout::shape_axis(mem, hit, 0).scale(layout::shape_extent(mem, hit, 0)).store(mem, scaled_x);
                        transform(mem, owner, scaled_x, matrix, axis_x, 0x51B560, frame, host);
                        layout::shape_axis(mem, hit, 1).scale(layout::shape_extent(mem, hit, 1)).store(mem, scaled_y);
                        transform(mem, owner, scaled_y, matrix, axis_y, 0x51B560, frame, host);
                        layout::shape_axis(mem, hit, 2).scale(layout::shape_extent(mem, hit, 2)).store(mem, scaled_z);
                        transform(mem, owner, scaled_z, matrix, axis_z, 0x51B560, frame, host);
                        let lengths = [
                            host.length(mem, frame.wrapping_sub(4), axis_x),
                            host.length(mem, frame.wrapping_sub(4), axis_y),
                            host.length(mem, frame.wrapping_sub(4), axis_z),
                        ];
                        layout::set_obb_extents(mem, box_at, lengths);
                        for (vector, length) in [(axis_x, lengths[0]), (axis_y, lengths[1]), (axis_z, lengths[2])] {
                            let reciprocal = 1.0 / length;
                            for lane in 0..4 {
                                let value = mem.f32(vector + lane * 4);
                                mem.set_f32(vector + lane * 4, reciprocal * value);
                            }
                        }
                        layout::obb_test(mem, object, box_at, count_out) != 0
                    } else {
                        transform(mem, owner, hit, matrix, sphere_at, 0x51B4C0, frame, host);
                        mem.set_f32(sphere_at + 0x10, mem.f32(hit + 0x10));
                        layout::sphere_box(mem, object, sphere_at, count_out) != 0
                    };
                    if contact {
                        let output = mem.u32(count_out);
                        let tag = mem.u8(collision + 0x68F);
                        mem.map(object.wrapping_add(output.wrapping_mul(0x40)).wrapping_add(0xC4), &[tag]);
                        mem.set_u32(count_out, output.wrapping_add(1));
                    }
                }
            }
            node = mem.u32(node);
        }
    }

    mem.set_u32(list_out, object.wrapping_add(0x90));
    u32::from(mem.u32(count_out) != 0)
}
