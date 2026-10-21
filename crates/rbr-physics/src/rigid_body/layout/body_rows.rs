//! `CSolid`'s row and point calls: the row transforms and folds (`E43VfA0`-`E43VfC8`), the inverse mass, the point
//! velocity, `E43Vf138`, the response and the velocity clamp.

use super::scratch_state::load_quat;
use super::sleep_body::{Body, Def};
use super::{
    ANGULAR_VELOCITY, DEFINITION, DEF_INVERSE_MASS, DEF_VERY_SOFT, LINEAR_VELOCITY, POSITION, START_ORIENTATION,
    START_POSITION,
};
use crate::math::quat::Quat;
use crate::math::vec::Vec3;
use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::rigid_body;
use crate::rigid_body::dot::{self, Dot138};
use crate::rigid_body::response::ResponseHost;
use crate::rigid_body::rows::RowFrame;
use crate::rigid_body::{impulse, response};

/// `CSolid::E43VfAC` (0x52ad40): the body's matrix times the `count` rows at `rows` (16 bytes each), stored back in
/// place.
pub fn transform_body_rows(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_rows(mem, this, rows, count, rigid_body::rows::transform_rows);
}

/// `CSolid::E43VfB0` (0x52add0): the same round trip with the three-lane transform.
pub fn transform_body_rows3(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_rows(mem, this, rows, count, rigid_body::rows::transform_rows3);
}

/// `CSolid::E43VfA0` (0x52ad10): subtract the body's position from every row, then transform its first three lanes by
/// the body's matrix. The vector subtraction reads the origin once and changes all four lanes; `XformRows` reads the
/// matrix anew for each row and leaves lane 3 as the subtracted value.
pub fn solid_e43vfa0(mem: &mut Mem, this: u32, rows: u32, count: u32) -> u32 {
    let origin = Vec4::load(mem, this + POSITION);
    for k in 0..count {
        let at = rows + 16 * k;
        let mut row = Vec4::load(mem, at);
        rigid_body::rows::subtract_origin_row(&mut row, origin);
        row.store(mem, at);
    }
    for k in 0..count {
        let matrix = mem.f32s(this + 0x110);
        let at = rows + 16 * k;
        let mut row = Vec4::load(mem, at);
        rigid_body::rows::transform_matrix_row(&matrix, &mut row);
        mem.set_f32(at, row.x);
        mem.set_f32(at + 4, row.y);
        mem.set_f32(at + 8, row.z);
    }
    rows
}

/// `CSolid::E43Vf13C` (0x5297d0): the definition's inverse mass.
pub fn solid_inverse_mass(mem: &Mem, this: u32) -> f32 {
    let definition = mem.u32(this + DEFINITION);
    mem.f32(definition + DEF_INVERSE_MASS)
}

/// `CSolid::Step76` (0x529c80, thiscall, `ret 8`): `out` = `(point - pos) x w + v`, the velocity of the point at
/// `point` on the body. `pos` is at +0x140, `w` at +0x190 and `v` at +0x1c0; the first three lanes of each are read and
/// `out`'s are written (its fourth is not). The original's null checks on `this + 0x180` and `this + 0x1b0` never take
/// the null branch for a body.
pub fn point_velocity(mem: &mut Mem, this: u32, point: u32, out: u32) {
    let d = Vec3::load(mem, point) - Vec3::load(mem, this + POSITION);
    let w = Vec3::load(mem, this + ANGULAR_VELOCITY);
    let v = Vec3::load(mem, this + LINEAR_VELOCITY);
    (d.cross3(w) + v).store(mem, out);
}

/// `CSolid::Vf85` (0x52b870, slot 85, fastcall): whether the definition's word at `+0x1ec` is nonzero. The ground probe
/// asks it of its hit (the hit's `+0x154`).
pub fn def_flag(mem: &Mem, this: u32) -> bool {
    mem.u32(mem.u32(this + DEFINITION) + DEF_VERY_SOFT) != 0
}

/// `CSolid::E43Vf138` (0x52b990, slot 78): `args` are the stack words `a`, `b`, `c` (vector addresses), `dp` (the int),
/// `e` and `f` (the float addresses, written back). The def's `+0x130` and `+0x16c` are the scale and `k`.
pub fn e43vf138(mem: &mut Mem, this: u32, [a, b, c, dp, e, f]: [u32; 6]) {
    let def = Def::load(mem, mem.u32(this + DEFINITION));
    let input = Dot138 {
        a: Vec4::load(mem, a),
        b: Vec4::load(mem, b),
        c: Vec4::load(mem, c),
        scale: def.local_inverse_inertia,
        k: def.inverse_mass,
    };
    let (f_out, e_out) = dot::e43vf138(&input, dp, mem.f32(f), mem.f32(e));
    mem.set_f32(f, f_out);
    mem.set_f32(e, e_out);
}

/// `CSolid::E43Vf140` (0x52a570): compute the response at `point` and store the three returned lanes at `output`.
pub fn solid_response(mem: &mut Mem, this: u32, point: u32, output: u32, host: &mut impl ResponseHost) -> u32 {
    let body = Body::load(mem, this);
    let value = response::response(&body, Vec4::load(mem, point), host);
    mem.set_f32(output, value.x);
    mem.set_f32(output + 4, value.y);
    mem.set_f32(output + 8, value.z);
    output
}

/// `E46Impulse::Impulse640`'s memory sequence: the axis and accumulator share `this+0x1c0`; `dir`'s fourth lane is read
/// from the original call frame as supplied, including its stale stack value.
pub fn clamp_velocity_along(mem: &mut Mem, this: u32, dir: u32, penetration: f32, mu: f32) -> u32 {
    let app = mem.u32(0x1659b70);
    let settings = mem.u32(app + 0x24);
    let accumulator = Vec4::load(mem, this + LINEAR_VELOCITY);
    let direction = Vec4::load(mem, dir);
    if let Some(updated) = impulse::contact_impulse(
        accumulator,
        direction,
        penetration,
        mem.f32(settings + 0x1610c),
        mem.f32(settings + 0x1611c),
        mu,
    ) {
        updated.store(mem, this + LINEAR_VELOCITY);
    }
    this + LINEAR_VELOCITY
}

/// `CSolid::E43VfBC` (0x52aec0): the same round trip with the origin folded into the fourth lane.
pub fn fold_body_rows(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_rows(mem, this, rows, count, rigid_body::rows::fold_origin_rows);
}

/// `CSolid::E43VfC0` (0x52afd0): the same round trip with the reordered sums.
pub fn fold_body_rows_reordered(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_rows(mem, this, rows, count, rigid_body::rows::fold_origin_rows_reordered);
}

/// The quaternion loops (`E43VfA4`, `E43VfA8`, `E43VfB4`): the body's start orientation (`+0x2a0`) and the vector at
/// `+0x330` are read again for each row, and `transform` runs on that row.
pub fn quat_plus_body_rows(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(mem, this, rows, count, rigid_body::rows::quat_plus_rows);
}

/// `CSolid::E43VfA8` (0x52b1f0): the same round trip with the difference-rotation.
pub fn quat_diff_body_rows(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(mem, this, rows, count, rigid_body::rows::quat_diff_rows);
}

/// `CSolid::E43VfB4` (0x52b2e0): the same round trip without the offset.
pub fn quat_transform_body_rows(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(mem, this, rows, count, |q, _, rows| rigid_body::rows::quat_transform_rows(q, rows));
}

fn update_body_quat_rows(mem: &mut Mem, this: u32, rows: u32, count: u32, transform: fn(Quat, Vec4, &mut [Vec4])) {
    for k in 0..count {
        let q = load_quat(mem, this + START_ORIENTATION);
        let offset = Vec3::load(mem, this + START_POSITION).extend(0.0);
        let at = rows + 16 * k;
        let mut row = [Vec4::load(mem, at)];
        transform(q, offset, &mut row);
        row[0].store(mem, at);
    }
}

/// `CSolid::E43VfB8` (0x52b380): the quaternion's matrix rebuilt for each row.
pub fn quat_body_rows_b8(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_quat_rows(mem, this, rows, count, |q, _, rows| rigid_body::rows::quat_rows_b8(q, rows));
}

/// `CSolid::E43VfC4` (0x52b460): the matrix built once before the loop, the offset read for each row.
pub fn fold_quat_body_rows_c4(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_rows_once(mem, this, rows, count, rigid_body::rows::quat_fold_rows_c4);
}

/// `CSolid::E43VfC8` (0x52b580): as `fold_quat_body_rows_c4`, with the other sums.
pub fn fold_quat_body_rows_c8(mem: &mut Mem, this: u32, rows: u32, count: u32) {
    update_body_rows_once(mem, this, rows, count, rigid_body::rows::quat_fold_rows_c8);
}

/// The matrix of the start orientation is built once (`SetFromQuat` before the loop); the offset and each row are read
/// per row.
fn update_body_rows_once(
    mem: &mut Mem,
    this: u32,
    rows: u32,
    count: u32,
    transform: fn(&[f32; 10], Vec4, &mut [Vec4]),
) {
    let m = rigid_body::rows::quat_words(load_quat(mem, this + START_ORIENTATION));
    for k in 0..count {
        let offset = Vec3::load(mem, this + START_POSITION).extend(0.0);
        let at = rows + 16 * k;
        let mut row = [Vec4::load(mem, at)];
        transform(&m, offset, &mut row);
        row[0].store(mem, at);
    }
}

/// Runs `transform` on each of the `count` rows at `rows`, one row at a time: the body's matrix and origin are read
/// again for each row, as the original's loops read them (so a row that overlaps the matrix sees the stores made before
/// it).
fn update_body_rows(mem: &mut Mem, this: u32, rows: u32, count: u32, transform: fn(&RowFrame, &mut [Vec4])) {
    for k in 0..count {
        let frame = RowFrame { m: mem.f32s(this + 0x110), origin: Vec3::load(mem, this + POSITION).extend(0.0) };
        let at = rows + 16 * k;
        let mut row = [Vec4::load(mem, at)];
        transform(&frame, &mut row);
        row[0].store(mem, at);
    }
}
