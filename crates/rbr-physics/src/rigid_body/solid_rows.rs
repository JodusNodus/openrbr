//! The body's row and point calls over the store: the row transforms and folds, the inverse mass, the point
//! velocity, the response and the velocity clamp.

use super::dot::{self, Dot138};
use super::response::ResponseHost;
use super::rows::RowFrame;
use super::store::RigidStore;
use super::{impulse, response, rows};
use crate::math::quat::Quat;
use crate::rigid_body::layout::Vec4;

/// The body's matrix times the `count` rows at `rows`, stored back in place.
pub fn transform_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::transform_rows);
}

/// The same round trip with the three-lane transform.
pub fn transform_body_rows3<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::transform_rows3);
}

/// Subtracts the body's position from every row, then transforms its first three lanes by the body's matrix. The
/// origin is read once and all four lanes change; the matrix is read anew for each row and lane 3 keeps the
/// subtracted value.
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

/// The definition's inverse mass.
pub fn solid_inverse_mass(store: &impl RigidStore, this: u32) -> f32 {
    store.inverse_mass(this)
}

/// `out` = `(point - pos) x w + v`, the velocity of the point at `point` on the body. The first three lanes of each
/// are read and `out`'s are written (its fourth is not).
pub fn point_velocity<S: RigidStore>(store: &mut S, this: u32, point: u32, out: u32) {
    let motion = store.motion(this);
    let d = store.vec3(point) - motion.position;
    store.store_vec3(out, d.cross3(motion.angular_velocity) + motion.linear_velocity);
}

/// Whether the body's definition is very soft.
pub fn def_flag(store: &impl RigidStore, this: u32) -> bool {
    store.def_very_soft(this)
}

/// `args` are the addresses `a`, `b`, `c` (vectors), `dp` (the int), `e` and `f` (the floats, written back). The
/// definition's scale and `k`.
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

/// The response at `point`; the three returned lanes go to `output`.
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

/// Clamps the velocity along `dir` with the impulse limits: the axis and accumulator share one word, and `dir`'s
/// fourth lane is read as supplied, including any stale stack value.
pub fn clamp_velocity_along<S: RigidStore>(store: &mut S, this: u32, dir: u32, penetration: f32, mu: f32) -> u32 {
    let (limit, scale) = store.impulse_limits();
    let accumulator = store.linear_velocity(this);
    let direction = store.row(dir);
    if let Some(updated) = impulse::contact_impulse(accumulator, direction, penetration, limit, scale, mu) {
        store.store_linear_velocity(this, updated);
    }
    store.linear_velocity_at(this)
}

/// The same round trip with the origin folded into the fourth lane.
pub fn fold_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::fold_origin_rows);
}

/// The same round trip with the reordered sums.
pub fn fold_body_rows_reordered<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows(store, this, rows, count, rows::fold_origin_rows_reordered);
}

/// The quaternion loops: the body's start orientation and the vector at the start position are read again for each
/// row, and `transform` runs on that row.
pub fn quat_plus_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, rows::quat_plus_rows);
}

/// The same round trip with the difference-rotation.
pub fn quat_diff_body_rows<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, rows::quat_diff_rows);
}

/// The same round trip without the offset.
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

/// The quaternion's matrix rebuilt for each row.
pub fn quat_body_rows_b8<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(store, this, rows, count, |q, _, rows| rows::quat_rows_b8(q, rows));
}

/// The matrix built once before the loop, the offset read for each row.
pub fn fold_quat_body_rows_c4<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows_once(store, this, rows, count, rows::quat_fold_rows_c4);
}

/// As `fold_quat_body_rows_c4`, with the other sums.
pub fn fold_quat_body_rows_c8<S: RigidStore>(store: &mut S, this: u32, rows: u32, count: u32) {
    update_body_rows_once(store, this, rows, count, rows::quat_fold_rows_c8);
}

/// The matrix of the start orientation is built once; the offset and each row are read per row.
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

/// Scales the `n` rows at `dst` by the matrix, then adds the origin to them. Returns `dst`. The matrix and origin are
/// read before the rows are written, so `dst` must not overlap them (the recorded calls never do).
pub fn scale_add_rows<S: RigidStore>(store: &mut S, this: u32, dst: u32, n: u32) -> u32 {
    let matrix = store.matrix(this);
    let origin = store.position(this);
    // Row by row, with no buffer: each row's sums are its own.
    for at in (0..n).map(|i| dst + 16 * i) {
        let mut row = [store.row(at)];
        matrix.scale_rows(&mut row);
        for lane in &mut row {
            *lane = *lane + origin;
        }
        store.store_row(at, row[0]);
    }
    dst
}
