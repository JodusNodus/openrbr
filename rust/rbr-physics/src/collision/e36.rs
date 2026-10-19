//! E36 collision primitives that produce an OBB contact from a sphere.

use crate::mem::Mem;

use crate::math::vec::Vec3;

use super::layout::{self, Vec4};

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

fn normalize_lane(value: f32, reciprocal: f32) -> f32 {
    if value == 0.0 && reciprocal.is_infinite() {
        // The reference SSE path emits its indefinite NaN with the sign bit set for 0 * infinity.
        f32::from_bits(0xffc0_0000)
    } else {
        value * reciprocal
    }
}

/// An E36 object's OBB frame, read from `E36Box`: the centre (its w lane is the centre's own), the half extents, and
/// the three basis rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObbFrame {
    pub center: Vec4,
    pub extents: [f32; 3],
    pub basis: [[f32; 3]; 3],
}

/// The fields that E36SphereBox writes into a contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SphereContact {
    pub depth: f32,
    pub position: Vec4,
    pub normal: Vec4,
}

/// The fields that E36ObbTest writes into a contact. The position's w lane is the slot's own, so it is not here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObbContact {
    pub depth: f32,
    pub position: Vec3,
    pub metric: f32,
    pub normal: Vec4,
}

/// The sphere's offset from the box centre along the box axes, and the magnitude of each axis offset.
struct SphereOffset {
    delta: [f32; 3],
    axes: [f32; 3],
}

fn sphere_offset(frame: &ObbFrame, sphere: Vec4) -> SphereOffset {
    let dx = sphere.x - frame.center.x;
    let dy = sphere.y - frame.center.y;
    let dz = sphere.z - frame.center.z;
    let [x_axis, y_axis, z_axis] = frame.basis;
    let mut ax = dx * x_axis[0] + dy * x_axis[1] + dz * x_axis[2];
    let mut ay = dx * y_axis[0] + dy * y_axis[1] + dz * y_axis[2];
    let mut az = dx * z_axis[0] + dy * z_axis[1] + dz * z_axis[2];
    if ax < 0.0 {
        ax = -ax;
    }
    if ay < 0.0 {
        ay = -ay;
    }
    if az < 0.0 {
        az = -az;
    }
    SphereOffset { delta: [dx, dy, dz], axes: [ax, ay, az] }
}

/// E36SphereBox's separation test: true when the sphere overlaps the box.
pub fn sphere_overlaps(frame: &ObbFrame, sphere: Vec4, radius: f32) -> bool {
    let [ax, ay, az] = sphere_offset(frame, sphere).axes;
    let extents = frame.extents;
    !(extents[0] + radius <= ax || extents[1] + radius <= ay || extents[2] + radius <= az)
}

/// E36SphereBox's contact for an overlapping sphere: the depth, the position, and the normal. `normal_w` is the slot's
/// previous normal w, which the normalisation scales.
pub fn sphere_contact(frame: &ObbFrame, sphere: Vec4, radius: f32, normal_w: f32) -> SphereContact {
    let SphereOffset { delta: [dx, dy, dz], axes: [ax, ay, az] } = sphere_offset(frame, sphere);
    let center = frame.center;
    let extents = frame.extents;
    let (depth, position) = if extents[0] - ax <= radius || extents[1] - ay <= radius || extents[2] - az <= radius {
        let mut axis = usize::from(ax <= ay);
        let smallest = if axis == 0 { ax } else { ay };
        if smallest <= az && az != smallest {
            axis = 2;
        }
        let mut depth = 1.0 - ax / (extents[0] + radius);
        let y_depth = 1.0 - ay / (extents[1] + radius);
        if depth <= y_depth {
            depth = y_depth;
        }
        let mut z_depth = 1.0 - az / (extents[2] + radius);
        if z_depth <= depth {
            z_depth = depth;
        }

        // E01Vec4::Normalize uses the reference CPU's rsqrtss approximation here.
        let reciprocal = crate::math::rsqrt::rsqrt(dx * dx + dy * dy + dz * dz);
        let pick = extents[axis];
        let position = Vec4 {
            x: (dx * reciprocal) * pick + center.x,
            y: (dy * reciprocal) * pick + center.y,
            z: (dz * reciprocal) * pick + center.z,
            w: center.w,
        };
        (z_depth, position)
    } else {
        (1.0, sphere)
    };

    let offset = Vec3 { x: position.x - sphere.x, y: position.y - sphere.y, z: position.z - sphere.z };
    let length_sq = offset.x * offset.x + offset.y * offset.y + offset.z * offset.z;
    let normal = if 0.9 < length_sq {
        // This later normalization follows the source's explicit sqrt-and-divide path.
        let reciprocal = 1.0 / (f64::from(length_sq).sqrt() as f32);
        Vec4 { x: offset.x * reciprocal, y: offset.y * reciprocal, z: offset.z * reciprocal, w: normal_w * reciprocal }
    } else {
        Vec4 { x: 0.0, y: 0.0, z: 1.0, w: normal_w }
    };
    SphereContact { depth, position, normal }
}

/// E36ObbTest's separating-axis tests for two boxes, with `a` to `e` the input vectors. True when the boxes overlap.
#[allow(clippy::manual_range_contains)] // Ordered comparisons preserve the source's NaN branch behavior.
pub fn obb_overlaps(frame: &ObbFrame, a: Vec4, b: Vec4, c: Vec4, d: Vec4, e: Vec4) -> bool {
    let center = frame.center;
    let extents = frame.extents;
    let basis = frame.basis;
    let bx = b.x;
    let by = b.y;
    let bz = b.z;
    let threshold = 0.999_998_986_721_038_8_f32;
    let l0 = center.x - a.x;
    let l1 = center.y - a.y;
    let l2 = center.z - a.z;
    let mut parallel = false;

    let c0 = c.x * basis[0][0] + c.y * basis[0][1] + c.z * basis[0][2];
    let c1 = c.x * basis[1][0] + basis[1][1] * c.y + basis[1][2] * c.z;
    let c2 = basis[2][0] * c.x + basis[2][1] * c.y + basis[2][2] * c.z;
    let ax = c0.abs();
    let ay = c1.abs();
    let az = c2.abs();
    if threshold < ax || threshold < ay || threshold < az {
        parallel = true;
    }
    let fc = l0 * c.x + l1 * c.y + l2 * c.z;
    let e1 = ax * extents[0];
    if fc.abs() > ay * extents[1] + az * extents[2] + e1 + bx {
        return false;
    }

    let dx0 = basis[0][0] * d.x + basis[0][1] * d.y + basis[0][2] * d.z;
    let dx1 = basis[1][0] * d.x + basis[1][1] * d.y + basis[1][2] * d.z;
    let dx2 = basis[2][0] * d.x + basis[2][1] * d.y + basis[2][2] * d.z;
    let ex = dx0.abs();
    let ey = dx1.abs();
    let ez = dx2.abs();
    if threshold < ex || threshold < ey || threshold < ez {
        parallel = true;
    }
    let fd = l0 * d.x + l1 * d.y + l2 * d.z;
    let e2 = ey * extents[1];
    if fd.abs() > ez * extents[2] + ex * extents[0] + e2 + by {
        return false;
    }

    let ex0 = basis[0][0] * e.x + basis[0][1] * e.y + basis[0][2] * e.z;
    let ex1 = basis[1][0] * e.x + basis[1][1] * e.y + basis[1][2] * e.z;
    let ex2 = e.x * basis[2][0] + basis[2][1] * e.y + basis[2][2] * e.z;
    let fx = ex0.abs();
    let fy = ex1.abs();
    let fz = ex2.abs();
    if threshold < fx || threshold < fy || threshold < fz {
        parallel = true;
    }
    let fe = l0 * e.x + l1 * e.y + l2 * e.z;
    let e3 = fz * extents[2];
    if fe.abs() > fy * extents[1] + fx * extents[0] + e3 + bz {
        return false;
    }

    let e4 = fx * extents[2];
    let e5 = ex * extents[1];
    let da = l0 * basis[0][0] + l1 * basis[0][1] + l2 * basis[0][2];
    if da.abs() > e5 + e4 + e1 + bx {
        return false;
    }
    let e6 = ay * extents[0];
    let e7 = fy * extents[2];
    let db = l0 * basis[1][0] + l1 * basis[1][1] + l2 * basis[1][2];
    if db.abs() > e6 + e7 + e2 + by {
        return false;
    }
    let e8 = ez * extents[1];
    let e9 = az * extents[0];
    let dc = l0 * basis[2][0] + l1 * basis[2][1] + l2 * basis[2][2];
    if dc.abs() > e9 + e8 + e3 + bz {
        return false;
    }

    if !parallel {
        let x1 = fe * dx0 - ex0 * fd;
        let x2 = fe * dx1 - ex1 * fd;
        let x3 = fe * dx2 - ex2 * fd;
        let x4 = ex0 * fc - fe * c0;
        let x5 = ex1 * fc - fe * c1;
        let x6 = ex2 * fc - fe * c2;
        let x7 = fd * c0 - dx0 * fc;
        let x8 = fd * c1 - dx1 * fc;
        let x9 = fd * c2 - dx2 * fc;
        if x1.abs() > ay * extents[2] + az * extents[1] + fx * by + ex * bz {
            return false;
        }
        if x2.abs() > ax * extents[2] + e9 + fy * by + ey * bz {
            return false;
        }
        if x3.abs() > ax * extents[1] + e6 + fz * by + ez * bz {
            return false;
        }
        if x4.abs() > ey * extents[2] + e8 + ax * bz + fx * bx {
            return false;
        }
        if x5.abs() > ex * extents[2] + ez * extents[0] + ay * bz + fy * bx {
            return false;
        }
        if x6.abs() > ey * extents[0] + e5 + fz * bx + az * bz {
            return false;
        }
        if x7.abs() > fz * extents[1] + e7 + ax * by + ex * bx {
            return false;
        }
        if x8.abs() > fz * extents[0] + e4 + ay * by + ey * bx {
            return false;
        }
        if x9.abs() > fx * extents[1] + fy * extents[0] + az * by + ez * bx {
            return false;
        }
    }
    true
}

/// E36ObbTest's contact for overlapping boxes: the depth, the position, the metric, and the normal. `normal_w` is the
/// slot's previous normal w, which the normalisation scales.
#[allow(clippy::manual_range_contains)] // Ordered comparisons preserve the source's NaN branch behavior.
pub fn obb_contact(frame: &ObbFrame, a: Vec4, b: Vec4, normal_w: f32) -> ObbContact {
    let center = frame.center;
    let extents = frame.extents;
    let bx = b.x;
    let by = b.y;
    let bz = b.z;
    let l0 = center.x - a.x;
    let l1 = center.y - a.y;
    let l2 = center.z - a.z;
    let length_ax = extents[0] + bx;
    let length_ay = extents[1] + by;
    let length_az = extents[2] + bz;
    let length_a = (length_ax * length_ax + length_ay * length_ay + length_az * length_az).sqrt();
    let to_center = [a.x - center.x, a.y - center.y, a.z - center.z];
    let length_b = (to_center[0] * to_center[0] + to_center[1] * to_center[1] + to_center[2] * to_center[2]).sqrt();
    let depth = if length_a <= 0.0 {
        0.5
    } else {
        let ratio = length_b / length_a;
        let clamped = if ratio <= 1.0 { ratio } else { 1.0 };
        1.0 - clamped
    };

    let scaled = [l0 * depth, l1 * depth, l2 * depth];
    let position = Vec3 { x: center.x - scaled[0], y: center.y - scaled[1], z: center.z - scaled[2] };
    let metric = extents[0] * extents[2] * extents[1] * 4.0 * depth;

    let mut normal = Vec3 { x: position.x - a.x, y: position.y - a.y, z: position.z - a.z };
    let normal_length_sq = normal.x * normal.x + normal.y * normal.y + normal.z * normal.z;
    let mut reciprocal = crate::math::rsqrt::rsqrt(normal_length_sq);
    normal = Vec3 {
        x: normalize_lane(normal.x, reciprocal),
        y: normalize_lane(normal.y, reciprocal),
        z: normalize_lane(normal.z, reciprocal),
    };
    let mut w = normalize_lane(normal_w, reciprocal);
    let n2 = normal.z * normal.z + normal.y * normal.y + normal.x * normal.x;
    if n2 < 0.95 || 1.1 < n2 {
        normal = Vec3 { x: center.x - a.x, y: center.y - a.y, z: center.z - a.z };
        let fallback_length_sq = normal.x * normal.x + normal.y * normal.y + normal.z * normal.z;
        reciprocal = crate::math::rsqrt::rsqrt(fallback_length_sq);
        normal = Vec3 {
            x: normalize_lane(normal.x, reciprocal),
            y: normalize_lane(normal.y, reciprocal),
            z: normalize_lane(normal.z, reciprocal),
        };
        w = normalize_lane(w, reciprocal);
    }
    ObbContact { depth, position, metric, normal: Vec4 { x: normal.x, y: normal.y, z: normal.z, w } }
}
