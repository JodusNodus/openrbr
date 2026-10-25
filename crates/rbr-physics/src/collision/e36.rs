//! E36's collision entry: the sphere and OBB tests and `E36Collect`'s walk of the cells, generic over the store.

use super::obb::{self, ObbFrame};
use super::store::CollisionStore;
use crate::mem::Mem;

/// E36SphereBox (0x4f2ce0): a separated sphere writes nothing. Otherwise the contact at `index` takes the sphere as its
/// input, then its depth, position, cleared kind, and normal.
pub fn sphere_box<S: CollisionStore>(store: &mut S, object: u32, sphere_at: u32, index_at: u32) -> u32 {
    let frame: ObbFrame = store.obb_frame(object);
    let (sphere, radius) = store.sphere_input(sphere_at);
    if !obb::sphere_overlaps(&frame, sphere, radius) {
        return 0;
    }

    let index = store.u32(index_at);
    let fields = obb::sphere_contact(&frame, sphere, radius, store.contact_normal_w(object, index));
    store.store_sphere_contact(object, index, sphere, &fields);
    1
}

/// The second vector of E36ObbTest's input at `vector_at`: the lengths of the other box's three axes, its half extents
/// (E36Collect stores them at its frame +0x60, 0x4f3bf4-0x4f3c14; the w lane stays as the frame held it).
pub fn set_obb_extents<S: CollisionStore>(store: &mut S, vector_at: u32, lengths: [f32; 3]) {
    for (at, length) in (vector_at + 0x10..).step_by(4).zip(lengths) {
        store.set_f32(at, length);
    }
}

/// E36ObbTest (0x4f3030): a separated pair writes nothing. Otherwise the contact at `index` takes its depth, the input
/// vector `a`, the position (the position's w lane is kept), the metric, and the normal.
pub fn obb_test<S: CollisionStore>(store: &mut S, object: u32, vector_at: u32, index_at: u32) -> u32 {
    let frame = store.obb_frame(object);
    let [a, b, c, d, e] = store.obb_input(vector_at);
    if !obb::obb_overlaps(&frame, a, b, c, d, e) {
        return 0;
    }

    let index = store.u32(index_at);
    let fields = obb::obb_contact(&frame, a, b, store.contact_normal_w(object, index));
    store.store_obb_contact(object, index, a, &fields);
    1
}

/// The host calls of `collect`. `sp` is the callee's entry stack pointer at the original's call site (see
/// `collect`'s `frame`): the callee reads the arguments the original pushed there, and what its frame leaves below
/// `sp` is part of what the original's caller reads back.
pub trait CollectHost<S = Mem> {
    fn virtual_call(&mut self, mem: &mut S, sp: u32, target: u32, args: &[u32]) -> u32;
    fn transform(&mut self, mem: &mut S, sp: u32, target: u32, this: u32, a: u32, b: u32, c: u32);
    fn query_cells(&mut self, mem: &mut S, sp: u32, lo: u32, hi: u32, cells: u32);
    fn length(&mut self, mem: &S, sp: u32, vector: u32) -> f32;
}

fn transform<S: CollisionStore>(
    store: &mut S,
    owner: u32,
    a: u32,
    matrix: u32,
    c: u32,
    finish: u32,
    frame: u32,
    host: &mut impl CollectHost<S>,
) {
    let target = store.vtable_slot(owner, 12 * 4);
    // Both calls are made from the same pushes: the original pushes a, matrix and c once, and the virtual call
    // leaves them on the stack for the finish call (0x4f399c, 0x4f39a1).
    let sp = frame.wrapping_sub(16);
    let receiver = host.virtual_call(store, sp, target, &[owner, a, matrix, c]);
    host.transform(store, sp, finish, receiver, a, matrix, c);
}

fn collision_pointer<S: CollisionStore>(store: &mut S, owner: u32, frame: u32, host: &mut impl CollectHost<S>) -> u32 {
    let target = store.vtable_slot(owner, 70 * 4);
    host.virtual_call(store, frame.wrapping_sub(4), target, &[owner])
}

/// E36Collect (0x4f3950): transform the query box, traverse its four grid buckets, and collect OBB/sphere contacts.
/// E43 transforms, the body virtuals, and E45's grid selector remain explicit host calls.
pub fn collect<S: CollisionStore>(
    store: &mut S,
    frame: u32,
    object: u32,
    body: u32,
    list_out: u32,
    count_out: u32,
    host: &mut impl CollectHost<S>,
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
    let matrix = store.collide_matrix();
    let entry = store.entry_frame(object);

    store.set_u32(count_out, 0);
    if collision_pointer(store, body, frame, host) == 0 {
        return 0;
    }

    transform(store, body, entry.centre_in, matrix, entry.centre_out, 0x51B4C0, frame, host);
    for axis in 0..3 {
        transform(store, body, entry.axes_in[axis], matrix, entry.axes_out[axis], 0x51B560, frame, host);
    }

    let centre = store.entry_centre(object);
    for (axis, center) in centre.into_iter().enumerate() {
        store.set_f32(lo + axis as u32 * 4, center - 0.1);
        store.set_f32(hi + axis as u32 * 4, center + 0.1);
    }
    host.query_cells(store, frame.wrapping_sub(16), lo, hi, cells);

    for index in 0..4 {
        if store.u32(count_out) > 7 {
            break;
        }
        let cell = store.u32(cells + index * 4);
        let mut node = if cell == 0 { 0 } else { store.cell_first(cell) };
        while node != 0 && store.u32(count_out) < 8 {
            let owner = store.node_owner(node);
            let collision = collision_pointer(store, owner, frame, host);
            if collision != 0 {
                let hit = store.collision_shape(collision);
                if hit != 0 {
                    let contact = if !store.collision_is_sphere(collision) {
                        transform(store, owner, hit, matrix, box_at, 0x51B4C0, frame, host);
                        let scaled = store.shape_axis(hit, 0).scale(store.shape_extent(hit, 0));
                        store.store_vec4(scaled_x, scaled);
                        transform(store, owner, scaled_x, matrix, axis_x, 0x51B560, frame, host);
                        let scaled = store.shape_axis(hit, 1).scale(store.shape_extent(hit, 1));
                        store.store_vec4(scaled_y, scaled);
                        transform(store, owner, scaled_y, matrix, axis_y, 0x51B560, frame, host);
                        let scaled = store.shape_axis(hit, 2).scale(store.shape_extent(hit, 2));
                        store.store_vec4(scaled_z, scaled);
                        transform(store, owner, scaled_z, matrix, axis_z, 0x51B560, frame, host);
                        let lengths = [
                            host.length(store, frame.wrapping_sub(4), axis_x),
                            host.length(store, frame.wrapping_sub(4), axis_y),
                            host.length(store, frame.wrapping_sub(4), axis_z),
                        ];
                        set_obb_extents(store, box_at, lengths);
                        for (vector, length) in [(axis_x, lengths[0]), (axis_y, lengths[1]), (axis_z, lengths[2])] {
                            let reciprocal = 1.0 / length;
                            for lane in 0..4 {
                                let value = store.f32(vector + lane * 4);
                                store.set_f32(vector + lane * 4, reciprocal * value);
                            }
                        }
                        obb_test(store, object, box_at, count_out) != 0
                    } else {
                        transform(store, owner, hit, matrix, sphere_at, 0x51B4C0, frame, host);
                        let radius = store.shape_extent(hit, 0);
                        store.set_f32(sphere_at + 0x10, radius);
                        sphere_box(store, object, sphere_at, count_out) != 0
                    };
                    if contact {
                        let output = store.u32(count_out);
                        let tag = store.collision_tag(collision);
                        store.store_contact_tag(object, output, tag);
                        store.set_u32(count_out, output.wrapping_add(1));
                    }
                }
            }
            node = store.node_next(node);
        }
    }

    let list = store.contact_list(object);
    store.set_u32(list_out, list);
    u32::from(store.u32(count_out) != 0)
}
