//! `CSolid`'s row and point calls over the store: the row transforms and folds (`E43VfA0`-`E43VfC8`, `Vf39x`), the
//! inverse mass, the point velocity, `E43Vf138`, the response and the velocity clamp.

use super::dot::{self, Dot138};
use super::response::ResponseHost;
use super::rows::RowFrame;
use super::store::RigidStore;
use super::{impulse, response, rows};
use crate::math::quat::Quat;
use crate::rigid_body::layout::Vec4;

/// `CSolid::E43VfAC` (0x52ad40): the body's matrix times the `count` rows at `rows`, stored back in place.
pub fn transform_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::transform_rows);
}

/// `CSolid::E43VfB0` (0x52add0): the same round trip with the three-lane transform.
pub fn transform_body_rows3<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::transform_rows3);
}

/// `CSolid::E43VfA0` (0x52ad10): subtract the body's position from every row, then transform its first three lanes by
/// the body's matrix. The vector subtraction reads the origin once and changes all four lanes; `XformRows` reads the
/// matrix anew for each row and leaves lane 3 as the subtracted value.
pub fn solid_e43vfa0<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) -> u32 {
    let origin = store.position(this);
    for k in 0..count {
        let at = rows + 16 * k;
        let mut row = store.row(at);
        super::rows::subtract_origin_row(&mut row, origin);
        store.store_row(at, row);
    }
    for k in 0..count {
        let matrix = store.matrix_words(this);
        let at = rows + 16 * k;
        let mut row = store.row(at);
        super::rows::transform_matrix_row(&matrix, &mut row);
        store.store_row3(at, row);
    }
    rows
}

/// `CSolid::E43Vf13C` (0x5297d0): the definition's inverse mass.
pub fn solid_inverse_mass(store: &impl RigidStore, this: u32) -> f32 {
    store.inverse_mass(this)
}

/// `CSolid::Step76` (0x529c80, thiscall, `ret 8`): `out` = `(point - pos) x w + v`, the velocity of the point at
/// `point` on the body. The first three lanes of each are read and `out`'s are written (its fourth is not). The
/// original's null checks on `this + 0x180` and `this + 0x1b0` never take the null branch for a body.
pub fn point_velocity<S: RigidStore>(store: &mut S, this: u32, point: u32, out: u32) {
    let motion = store.motion(this);
    let d = store.vec3(point) - motion.position;
    store.store_vec3(out, d.cross3(motion.angular_velocity) + motion.linear_velocity);
}

/// `CSolid::Vf85` (0x52b870, slot 85, fastcall): whether the definition's word at `+0x1ec` is nonzero. The ground probe
/// asks it of its hit (the hit's `+0x154`).
pub fn def_flag(store: &impl RigidStore, this: u32) -> bool {
    store.def_very_soft(this)
}

/// `CSolid::E43Vf138` (0x52b990, slot 78): `args` are the stack words `a`, `b`, `c` (vector addresses), `dp` (the int),
/// `e` and `f` (the float addresses, written back). The def's `+0x130` and `+0x16c` are the scale and `k`.
pub fn e43vf138<S: RigidStore>(store: &mut S, this: u32, [a, b, c, dp, e, f]: [u32; 6]) {
    let def = store.definition(this);
    let input = Dot138 {
        a: store.row(a),
        b: store.row(b),
        c: store.row(c),
        scale: def.local_inverse_inertia,
        k: def.inverse_mass,
    };
    let (f_out, e_out) = dot::e43vf138(&input, dp, store.f32(f), store.f32(e));
    store.set_f32(f, f_out);
    store.set_f32(e, e_out);
}

/// `CSolid::E43Vf140` (0x52a570): compute the response at `point` and store the three returned lanes at `output`.
pub fn solid_response<S: RigidStore>(
    store: &mut S,
    this: u32,
    point: u32,
    output: u32,
    host: &mut impl ResponseHost,
) -> u32 {
    let body = store.body(this);
    let value = response::response(&body, store.row(point), host);
    store.set_f32(output, value.x);
    store.set_f32(output + 4, value.y);
    store.set_f32(output + 8, value.z);
    output
}

/// `E46Impulse::Impulse640`'s memory sequence: the axis and accumulator share `this+0x1c0`; `dir`'s fourth lane is read
/// from the original call frame as supplied, including its stale stack value.
pub fn clamp_velocity_along<S: RigidStore>(store: &mut S, this: u32, dir: u32, penetration: f32, mu: f32) -> u32 {
    let (limit, scale) = store.impulse_limits();
    let accumulator = store.linear_velocity(this);
    let direction = store.row(dir);
    if let Some(updated) = impulse::contact_impulse(accumulator, direction, penetration, limit, scale, mu) {
        store.store_linear_velocity(this, updated);
    }
    store.linear_velocity_at(this)
}

/// `CSolid::E43VfBC` (0x52aec0): the same round trip with the origin folded into the fourth lane.
pub fn fold_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::fold_origin_rows);
}

/// `CSolid::E43VfC0` (0x52afd0): the same round trip with the reordered sums.
pub fn fold_body_rows_reordered<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::fold_origin_rows_reordered);
}

/// The quaternion loops (`E43VfA4`, `E43VfA8`, `E43VfB4`): the body's start orientation and the vector at the start
/// position are read again for each row, and `transform` runs on that row.
pub fn quat_plus_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, rows::quat_plus_rows);
}

/// `CSolid::E43VfA8` (0x52b1f0): the same round trip with the difference-rotation.
pub fn quat_diff_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, rows::quat_diff_rows);
}

/// `CSolid::E43VfB4` (0x52b2e0): the same round trip without the offset.
pub fn quat_transform_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, |q, _, rows| rows::quat_transform_rows(q, rows));
}

fn update_body_quat_rows<S: RigidStore>(
    store: &mut S,
    this: u32,
    rows: u32,
    count: u32,
    transform: fn(Quat, Vec4, &mut [Vec4]),
) {
    for k in 0..count {
        let q = store.start_orientation(this);
        let offset = store.start_offset(this);
        let at = rows + 16 * k;
        let mut row = [store.row(at)];
        transform(q, offset, &mut row);
        store.store_row(at, row[0]);
    }
}

/// `CSolid::E43VfB8` (0x52b380): the quaternion's matrix rebuilt for each row.
pub fn quat_body_rows_b8<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, |q, _, rows| rows::quat_rows_b8(q, rows));
}

/// `CSolid::E43VfC4` (0x52b460): the matrix built once before the loop, the offset read for each row.
pub fn fold_quat_body_rows_c4<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows_once(store, this, rows, count, rows::quat_fold_rows_c4);
}

/// `CSolid::E43VfC8` (0x52b580): as `fold_quat_body_rows_c4`, with the other sums.
pub fn fold_quat_body_rows_c8<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows_once(store, this, rows, count, rows::quat_fold_rows_c8);
}

/// The matrix of the start orientation is built once (`SetFromQuat` before the loop); the offset and each row are read
/// per row.
fn update_body_rows_once<S: RigidStore>(
    store: &mut S,
    this: u32,
    rows: u32,
    count: u32,
    transform: fn(&[f32; 10], Vec4, &mut [Vec4]),
) {
    let m = rows::quat_words(store.start_orientation(this));
    for k in 0..count {
        let offset = store.start_offset(this);
        let at = rows + 16 * k;
        let mut row = [store.row(at)];
        transform(&m, offset, &mut row);
        store.store_row(at, row[0]);
    }
}

/// Runs `transform` on each of the `count` rows at `rows`, one row at a time: the body's matrix and origin are read
/// again for each row, as the original's loops read them (so a row that overlaps the matrix sees the stores made before
/// it).
fn update_body_rows<S: RigidStore>(
    store: &mut S,
    this: u32,
    rows: u32,
    count: u32,
    transform: fn(&RowFrame, &mut [Vec4]),
) {
    for k in 0..count {
        let frame = store.row_frame(this);
        let at = rows + 16 * k;
        let mut row = [store.row(at)];
        transform(&frame, &mut row);
        store.store_row(at, row[0]);
    }
}

/// `E47Solid::Vf39x` (0x52ac80), `E43Vf9C` in the C++: scales the `n` rows at `dst` by the matrix, then adds the origin
/// to them. Returns `dst`. The original's two row calls (`E46Mat::ScaleRows`, `E46Vec::Add`) are `Mat::scale_rows` and
/// `Vec4Rows::add_each`. The matrix and origin are read before the rows are written, so `dst` must not overlap them
/// (the recorded calls never do).
pub fn scale_add_rows<S: RigidStore>(store: &mut S, this: u32, dst: u32, n: u32) -> u32 {
    use crate::physics_core::math::Vec4Rows;
    let matrix = store.matrix(this);
    let origin = store.position(this);
    // Row by row, with no buffer: each row's sums are its own.
    for at in (0..n).map(|i| dst + 16 * i) {
        let mut row = [store.row(at)];
        matrix.scale_rows(&mut row);
        origin.add_each(&mut row);
        store.store_row(at, row[0]);
    }
    dst
}
