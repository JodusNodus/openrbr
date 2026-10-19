//! The row loops of `CSolid` (0x52ad40 to 0x52b380): each replaces 16-byte rows by the body's matrix (`+0x110`, ten words)
//! times the row's first three lanes, and some fold the body's origin (`+0x140`) into the fourth lane.

use super::layout::Vec4;
use crate::math::quat::{from_quat, Quat};
use crate::math::sse;

/// What the loops read from the body: the matrix (ten words from `+0x110`; `m[9]` is at `+0x134`) and the origin (the body's
/// position at `+0x140`; its `w` is not read).
pub struct RowFrame {
    pub m: [f32; 10],
    pub origin: Vec4,
}

/// `CSolid::E43VfAC` (0x52ad40): each row becomes the matrix times its `x, y, z` in all four lanes, summed as
/// `(c0 + c1) + c2` lane by lane (`c0` from `m[0..4]`, `c1` from `m[3..7]`, `c2` from `m[6..10]`).
pub fn transform_rows(f: &RowFrame, rows: &mut [Vec4]) {
    let m = &f.m;
    for row in rows {
        let (x, y, z) = (row.x, row.y, row.z);
        *row = Vec4 {
            x: (m[0] * x + m[3] * y) + m[6] * z,
            y: (m[1] * x + m[4] * y) + m[7] * z,
            z: (m[2] * x + m[5] * y) + m[8] * z,
            w: (m[3] * x + m[6] * y) + m[9] * z,
        };
    }
}

/// `CSolid::E43VfB0` (0x52add0): the three-lane variant. `x, y, z` are replaced by the matrix times them, with the
/// scalar chain's sums (`z` starts from `m[8]*z`); the fourth lane is left as it was.
pub fn transform_rows3(f: &RowFrame, rows: &mut [Vec4]) {
    let m = &f.m;
    for row in rows {
        let (x, y, z) = (row.x, row.y, row.z);
        row.x = (m[0] * x + m[1] * y) + m[2] * z;
        row.y = (m[3] * x + m[4] * y) + m[5] * z;
        row.z = (m[8] * z + m[6] * x) + m[7] * y;
    }
}

/// `CSolid::E43VfBC` (0x52aec0): with `f1 = (m0 x + m3 y) + m6 z`, `f2 = (m1 x + m4 y) + m7 z`, `f3 = (m2 x + m5 y) + m8 z`,
/// the row's `x, y, z` become `f1, f2, f3`, and the fourth lane becomes `0 - ((origin.y f2 + origin.z f3 + origin.x f1) - w)`.
pub fn fold_origin_rows(f: &RowFrame, rows: &mut [Vec4]) {
    let (m, o) = (&f.m, f.origin);
    for row in rows {
        let (x, y, z) = (row.x, row.y, row.z);
        let f1 = (m[0] * x + m[3] * y) + m[6] * z;
        let f2 = (m[1] * x + m[4] * y) + m[7] * z;
        let f3 = (m[2] * x + m[5] * y) + m[8] * z;
        row.x = f1;
        row.y = f2;
        row.z = f3;
        row.w = 0.0 - ((o.y * f2 + o.z * f3 + o.x * f1) - row.w);
    }
}

/// `CSolid::E43VfC0` (0x52afd0): the new `x, y, z` are summed from the old row, in the asm's order:
/// `x = (m2 z + m1 y) + m0 x`, `y = (m5 z + m4 y) + m3 x`, `z = (m7 y + m6 x) + m8 z`. The fourth lane folds the old
/// `x, y, z` against the origin, as `fold_origin_rows` does.
pub fn fold_origin_rows_reordered(f: &RowFrame, rows: &mut [Vec4]) {
    let (m, o) = (&f.m, f.origin);
    for row in rows {
        let (x, y, z) = (row.x, row.y, row.z);
        row.x = (m[2] * z + m[1] * y) + m[0] * x;
        row.y = (m[5] * z + m[4] * y) + m[3] * x;
        row.z = (m[7] * y + m[6] * x) + m[8] * z;
        row.w = 0.0 - ((0.0 - row.w) - ((o.y * y + o.z * z) + o.x * x));
    }
}

/// The word after the rotation matrix's nine (`lm[9]`, the fourth lane of the matrix's third row load). The original's
/// `SetFromQuat` leaves it unset, so it holds whatever stack word the caller had there; the records read it as 0.
const UNSET_MATRIX_WORD: f32 = 0.0;

/// The rotation matrix of `q` as the loops read it: `SetFromQuat`'s nine words, then `UNSET_MATRIX_WORD`.
pub fn quat_words(q: Quat) -> [f32; 10] {
    let m = from_quat(q);
    [m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2], UNSET_MATRIX_WORD]
}

/// `CSolid::E43VfB4` (0x52b2e0): each row becomes the rotation of `q` times its `x, y, z` (the `transform_rows` sums).
pub fn quat_transform_rows(q: Quat, rows: &mut [Vec4]) {
    let frame = RowFrame { m: quat_words(q), origin: Vec4 { x: 0.0, y: 0.0, z: 0.0, w: 0.0 } };
    transform_rows(&frame, rows);
}

/// `CSolid::E43VfA4` (0x52b110): the rotation of `q` times each row, then `offset` (`+0x330`, xyz) added to its `x, y, z`.
pub fn quat_plus_rows(q: Quat, offset: Vec4, rows: &mut [Vec4]) {
    quat_transform_rows(q, rows);
    for row in rows {
        row.x += offset.x;
        row.y += offset.y;
        row.z += offset.z;
    }
}

/// `CSolid::E43VfA8` (0x52b1f0): each row's `x, y, z` less `offset` is rotated by the transpose of `q` (the three sums
/// `m2 d0 + m1 d1 + m0 d2` and so on, with `d2 = x - offset.x`, `d1 = y - offset.y`, `d0 = z - offset.z`); `w` is left.
pub fn quat_diff_rows(q: Quat, offset: Vec4, rows: &mut [Vec4]) {
    let m = quat_words(q);
    for row in rows {
        let d2 = row.x - offset.x;
        let d1 = row.y - offset.y;
        let d0 = row.z - offset.z;
        row.x = (m[2] * d0 + m[1] * d1) + m[0] * d2;
        row.y = (m[5] * d0 + m[4] * d1) + m[3] * d2;
        row.z = (m[8] * d0 + m[7] * d1) + m[6] * d2;
    }
}

/// The sums of `E43VfB8` (0x52b380) and `E43VfC8` (0x52b580): `x' = (m2 z + m1 y) + m0 x`, `y' = (m5 z + m4 y) + m3 x`,
/// `z' = (m8 z + m7 y) + m6 x`, the order the asm uses.
fn rotate_b8(m: &[f32; 10], x: f32, y: f32, z: f32) -> [f32; 3] {
    [(m[2] * z + m[1] * y) + m[0] * x, (m[5] * z + m[4] * y) + m[3] * x, (m[8] * z + m[7] * y) + m[6] * x]
}

/// `CSolid::E43VfB8` (0x52b380): the rotation of `q` rebuilt for each row (`rotate_b8`); the fourth lane is left as it was.
pub fn quat_rows_b8(q: Quat, rows: &mut [Vec4]) {
    let m = quat_words(q);
    for row in rows {
        let [x, y, z] = rotate_b8(&m, row.x, row.y, row.z);
        row.x = x;
        row.y = y;
        row.z = z;
    }
}

/// `E46Vec::Sub`'s per-row step in `CSolid::E43VfA0` (0x52ad10): subtract the position from all four lanes.
pub fn subtract_origin_row(row: &mut Vec4, origin: Vec4) {
    row.x = sse::sub(row.x, origin.x);
    row.y = sse::sub(row.y, origin.y);
    row.z = sse::sub(row.z, origin.z);
    row.w = sse::sub(row.w, origin.w);
}

/// `E46Mat::XformRows`'s per-row step in `CSolid::E43VfA0` (0x52ad10): the three matrix rows in the original's SSE order.
/// It leaves `w` alone; the preceding vector subtraction already changed that lane.
pub fn transform_matrix_row(matrix: &[f32; 9], row: &mut Vec4) {
    let (x, y, z) = (row.x, row.y, row.z);
    row.x = sse::add(sse::add(sse::mul(matrix[1], y), sse::mul(matrix[2], z)), sse::mul(matrix[0], x));
    row.y = sse::add(sse::add(sse::mul(matrix[4], y), sse::mul(matrix[5], z)), sse::mul(matrix[3], x));
    row.z = sse::add(sse::add(sse::mul(matrix[7], y), sse::mul(matrix[8], z)), sse::mul(matrix[6], x));
}

/// `CSolid::E43VfC4` (0x52b460): with the matrix built once, `x' = (m0 x + m3 y) + m6 z`, `y' = (m1 x + m4 y) + m7 z`,
/// `z' = (m2 x + m5 y) + m8 z` (the matrix's columns, packed in the asm), and the fourth lane is
/// `0 - ((offset.x x + offset.y y + offset.z z) - w)` with the old row.
pub fn quat_fold_rows_c4(m: &[f32; 10], offset: Vec4, rows: &mut [Vec4]) {
    for row in rows {
        let (x, y, z) = (row.x, row.y, row.z);
        row.x = (m[0] * x + m[3] * y) + m[6] * z;
        row.y = (m[1] * x + m[4] * y) + m[7] * z;
        row.z = (m[2] * x + m[5] * y) + m[8] * z;
        row.w = 0.0 - (((offset.x * x + offset.y * y) + offset.z * z) - row.w);
    }
}

/// `CSolid::E43VfC8` (0x52b580): as `fold_quat_rows_c4` with the `rotate_b8` sums, and the fourth lane is
/// `0 - ((0 - w) - ((offset.x x + offset.y y) + offset.z z))` with the old row.
pub fn quat_fold_rows_c8(m: &[f32; 10], offset: Vec4, rows: &mut [Vec4]) {
    for row in rows {
        let (x, y, z) = (row.x, row.y, row.z);
        let [nx, ny, nz] = rotate_b8(m, x, y, z);
        row.x = nx;
        row.y = ny;
        row.z = nz;
        row.w = 0.0 - ((0.0 - row.w) - ((offset.x * x + offset.y * y) + offset.z * z));
    }
}
