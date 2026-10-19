//! The car's collision meshes, from the collision bodies of its `.sgc`, as HedgeHog3D.dll's car loader emits them (1.02,
//! the `CSigCollisionMesh` emits at 0x1001ecc0, [`car_meshes`]; `E37SigFactory720` builds grids 1-8 from them,
//! `load::emit_car_meshes`). Each holds the vertices, the faces (three vertex indices, 0, and the body's three sample
//! indices) and, for the body, its samples and two maps. The vertices, per body: `A` = the body's transform block composed (`HH_H7ReadTransform` 0x1000c7a0, `NLB_ComposeTransformEH`
//! 0x1008d420), `B` = its world matrix (`HH_H7ReadMatrix` 0x1000c6c0), each vertex goes through `A * B` and is stored
//! as `(x, z, y)`.
//!
//! The NLB math goes through call slots (0x101c30c0) that the game fills with the SSE variants: the matrix product
//! 0x101552fa, the inverse 0x10155560 and the point transform 0x10156035 (which divides by `w` through `rcpps` and a
//! Newton step, so `w = 1` scales by `1 - 2^-24`). The quaternion matrix (0x100a65f5) and the transform reader are x87
//! code at the game's 24-bit precision control: each operation rounds to f32.
//!
//! The hinged bodies' grids (2-5) move by their hinge vectors later, in `CCar::UpdateA720` (`CHingedRigidBody::SimEFE0`
//! -> `E37GridOps::AddEdge` 0x4f65b0, `collision::layout::grid_add_edge`).

use crate::math::rcp::rcp;
use crate::math::sse::{add, mul, sub};
use crate::math::trig::{fcos, fsin, X87Ext80};
use rbr_formats::sgc::Collision;

/// A 4x4 matrix as the NLB code stores it: row-major, row vectors (`p' = p * M`), the translation in row 3.
pub type Mat4 = [[f32; 4]; 4];

type Lanes = [f32; 4];

/// The collision bodies the loader emits, in its order: the body's grid id is fixed (1), with the name
/// `"MrCarCollision"`; the others come from HedgeHog3D's table at 0x101d7eb0 (name, id), whose names are the bodies'.
/// Each with its name's offset in HedgeHog3D's image (base 0x10000000).
pub const PARTS: [(&str, u32, u32); 8] = [
    ("c_e_body", 1, 0x1a_ca74),
    ("c_e_door_l", 2, 0x1a_fe40),
    ("c_e_door_r", 3, 0x1a_fe34),
    ("c_e_bonnet", 4, 0x1a_fe28),
    ("c_e_exhaust", 5, 0x1a_fe1c),
    ("c_e_bumper_f", 6, 0x1a_fe0c),
    ("c_e_bumper_b", 7, 0x1a_fdfc),
    ("c_e_wing", 8, 0x1a_fdf0),
];

/// A sampled-surface map of the body: its size `m` (the file's int, as a `u16`) and its `m * m` bytes, if the file has
/// them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SampleMap {
    pub size: u16,
    pub data: Option<Vec<u8>>,
}

/// One `CSigCollisionMesh` the car loader emits (0x1001ecc0) and the blocks it points to.
#[derive(Debug, Clone, PartialEq)]
pub struct CarMesh {
    pub id: u32,
    /// The name's offset in HedgeHog3D's image.
    pub name: u32,
    pub vertices: Vec<[f32; 3]>,
    /// Three vertex indices, 0, then three sample indices. Only the body's are written: a part's bytes 4-6 are the
    /// heap's in the original, 0 here.
    pub faces: Vec<[u8; 7]>,
    /// The body's samples, each `255 * x` truncated to a byte; None for a part.
    pub samples: Option<Vec<[u8; 2]>>,
    /// The body's maps: `a` is the file's second (+0x20, +0x24), `b` its first (+0x22, +0x28).
    pub map_a: SampleMap,
    pub map_b: SampleMap,
}

/// `_ftol`'s low byte: truncation, `0x80000000` out of range.
fn ftol_byte(v: f32) -> u8 {
    if v.abs() < 2_147_483_648.0 {
        v as i32 as u8
    } else {
        0
    }
}

/// The meshes the car loader emits from the car's collision bodies (the first of each name): the body, then the parts
/// in table order, up to the first one the car does not have (the loader's loop ends there). None without a body, or
/// when a body's transform has no inverse.
pub fn car_meshes(bodies: &[Collision]) -> Option<Vec<CarMesh>> {
    let mut meshes = Vec::new();
    for &(name, id, name_at) in &PARTS {
        let Some(body) = bodies.iter().find(|b| b.node.name == name) else {
            break;
        };
        let sampled = id == 1;
        let faces = body
            .faces
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let s = match body.face_samples.get(k) {
                    Some(s) if sampled => s.map(|i| i as u8),
                    _ => [0; 3],
                };
                [f[0] as u8, f[1] as u8, f[2] as u8, 0, s[0], s[1], s[2]]
            })
            .collect();
        let map = |k: usize| match body.maps.get(k) {
            Some(m) if sampled => SampleMap { size: m.size as u16, data: m.data.clone() },
            _ => SampleMap::default(),
        };
        meshes.push(CarMesh {
            id,
            name: name_at,
            vertices: grid_vertices(body)?,
            faces,
            samples: sampled.then(|| body.samples.iter().map(|s| s.map(|x| ftol_byte(mul(x, 255.0)))).collect()),
            map_a: map(1),
            map_b: map(0),
        });
    }
    Some(meshes)
}

/// `HH_H7ReadTransform`'s output: the translation, two rotations as quaternions `(axis * sin(angle / 2), cos(angle /
/// 2))`, and the scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HhTransform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub rotation2: [f32; 4],
}

/// `HH_H7ReadTransform` (0x1000c7a0) on the block's 14 words: translation (0-2), angle and axis (3, 4-6), scale (7-9),
/// second angle and axis (10, 11-13). The half angle is a `mulss`; `fsin` stays extended for the axis products.
pub fn read_transform(w: [f32; 14]) -> HhTransform {
    let quat = |angle: f32, axis: [f32; 3]| {
        let half = X87Ext80::from_f32(mul(angle, 0.5));
        let sin = fsin(half);
        let [x, y, z] = axis.map(|a| sin.mul_f32_pc24(a));
        [x, y, z, fcos(half).to_f32()]
    };
    HhTransform {
        translation: [w[0], w[1], w[2]],
        rotation: quat(w[3], [w[4], w[5], w[6]]),
        scale: [w[7], w[8], w[9]],
        rotation2: quat(w[10], [w[11], w[12], w[13]]),
    }
}

/// The quaternion's rotation matrix (0x100a65f5), in the x87 code's operation order.
pub fn quat_matrix(q: [f32; 4]) -> Mat4 {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x * 2.0, y * 2.0, z * 2.0);
    let (xw, yw, zw) = (x2 * w, y2 * w, z2 * w);
    let (xx, xy, xz) = (x2 * x, y2 * x, z2 * x);
    let (yy, yz, zz) = (y2 * y, z2 * y, z2 * z);
    [
        [(1.0 - yy) - zz, xy + zw, xz - yw, 0.0],
        [xy - zw, (1.0 - xx) - zz, yz + xw, 0.0],
        [xz + yw, yz - xw, (1.0 - xx) - yy, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn scale_matrix([x, y, z]: [f32; 3]) -> Mat4 {
    [[x, 0.0, 0.0, 0.0], [0.0, y, 0.0, 0.0], [0.0, 0.0, z, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

fn translation_matrix([x, y, z]: [f32; 3]) -> Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [x, y, z, 1.0]]
}

/// `HH_H7ReadMatrix` (0x1000c6c0) on a world matrix in file order (rows `[R | t]`): `M[r][i] = world[i][r]`, so row 3
/// is the translation; `M[3][3] = 1`.
pub fn world_matrix(world: [[f32; 4]; 3]) -> Mat4 {
    let mut m = scale_matrix([1.0; 3]);
    for (i, row) in world.iter().enumerate() {
        for (r, &v) in row.iter().enumerate() {
            m[r][i] = v;
        }
    }
    m
}

fn lanes2(f: impl Fn(f32, f32) -> f32, a: Lanes, b: Lanes) -> Lanes {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), f(a[3], b[3])]
}

fn mulps(a: Lanes, b: Lanes) -> Lanes {
    lanes2(mul, a, b)
}

fn addps(a: Lanes, b: Lanes) -> Lanes {
    lanes2(add, a, b)
}

fn subps(a: Lanes, b: Lanes) -> Lanes {
    lanes2(sub, a, b)
}

fn splat(v: f32) -> Lanes {
    [v; 4]
}

/// `shufps imm, src, dst`: two lanes of `dst`, then two of `src`.
fn shufps(dst: Lanes, src: Lanes, imm: u8) -> Lanes {
    let at = |k: u8| usize::from((imm >> (2 * k)) & 3);
    [dst[at(0)], dst[at(1)], src[at(2)], src[at(3)]]
}

/// The SSE matrix product (0x101552fa), `a * b`: each row of `a` broadcast against `b`'s rows, summed `(0 + 1) + (2 + 3)`
/// in rows 0 and 3 and `((0 + 1) + 2) + 3` in rows 1 and 2.
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let p = |i: usize, k: usize| mulps(splat(a[i][k]), b[k]);
    [
        addps(addps(p(0, 0), p(0, 1)), addps(p(0, 2), p(0, 3))),
        addps(addps(addps(p(1, 0), p(1, 1)), p(1, 2)), p(1, 3)),
        addps(addps(addps(p(2, 0), p(2, 1)), p(2, 2)), p(2, 3)),
        addps(addps(p(3, 0), p(3, 1)), addps(p(3, 2), p(3, 3))),
    ]
}

/// The SSE 4x4 inverse (0x10155560, Intel's Cramer's-rule code), instruction by instruction (registers `xN`, stack
/// temporaries `tNN` at `-0xNN(%ebp)`): the cofactors, the determinant, and `1/det` from `rcpss` with one Newton step.
/// `None` when the determinant is zero (the original then leaves its output unwritten).
pub fn inverse(m: &Mat4) -> Option<Mat4> {
    let f = m.as_flattened();
    let pair = |lo: usize, hi: usize| [f[lo], f[lo + 1], f[hi], f[hi + 1]];
    let (mut x1, mut x2, mut x3, mut x4) = (pair(0, 4), pair(2, 6), pair(8, 12), pair(10, 14));
    let (mut x5, mut x6, mut x7);
    x5 = x2;
    x5 = shufps(x5, x4, 0x88);
    x4 = shufps(x4, x2, 0xdd);
    x2 = x4;
    x2 = mulps(x2, x5);
    x2 = shufps(x2, x2, 0xb1);
    x6 = x2;
    x6 = shufps(x6, x6, 0x4e);
    x7 = x3;
    x3 = shufps(x3, x1, 0xdd);
    x1 = shufps(x1, x7, 0x88);
    x7 = x3;
    x3 = mulps(x3, x6);
    x6 = mulps(x6, x1);
    let td0 = x6;
    x6 = x7;
    x7 = mulps(x7, x2);
    x2 = mulps(x2, x1);
    x3 = subps(x3, x7);
    x7 = x6;
    x7 = mulps(x7, x5);
    x5 = shufps(x5, x5, 0x4e);
    x7 = shufps(x7, x7, 0xb1);
    let tc0 = x2;
    x2 = x4;
    x2 = mulps(x2, x7);
    x2 = addps(x2, x3);
    x3 = x7;
    x7 = shufps(x7, x7, 0x4e);
    x3 = mulps(x3, x1);
    let tb0 = x3;
    x3 = x4;
    x3 = mulps(x3, x7);
    x7 = mulps(x7, x1);
    x2 = subps(x2, x3);
    x3 = x6;
    x3 = shufps(x3, x3, 0x4e);
    x3 = mulps(x3, x4);
    x3 = shufps(x3, x3, 0xb1);
    let ta0 = x7;
    x7 = x5;
    x5 = mulps(x5, x3);
    x5 = addps(x5, x2);
    x2 = x3;
    x3 = shufps(x3, x3, 0x4e);
    x2 = mulps(x2, x1);
    let t90 = x4;
    x4 = x7;
    x7 = mulps(x7, x3);
    x3 = mulps(x3, x1);
    x5 = subps(x5, x7);
    x3 = subps(x3, x2);
    x2 = x1;
    x1 = mulps(x1, x5);
    x3 = shufps(x3, x3, 0x4e);
    x7 = x1;
    x1 = shufps(x1, x1, 0x4e);
    let te0 = x5;
    x1 = addps(x1, x7);
    x5 = x1;
    x1 = shufps(x1, x1, 0xb1);
    x1[0] = add(x1[0], x5[0]);
    x5 = x6;
    x5 = mulps(x5, x2);
    x5 = shufps(x5, x5, 0xb1);
    x7 = x5;
    x5 = shufps(x5, x5, 0x4e);
    let t80 = x4;
    x4 = t90;
    let t70 = x6;
    x6 = x4;
    x6 = mulps(x6, x7);
    x6 = addps(x6, x3);
    x3 = x4;
    x3 = mulps(x3, x5);
    x3 = subps(x3, x6);
    x6 = x4;
    x6 = mulps(x6, x2);
    x6 = shufps(x6, x6, 0xb1);
    let t60 = x5;
    x5 = t70;
    let t50 = x7;
    x7 = x6;
    x7 = mulps(x7, x5);
    x7 = addps(x7, x3);
    x3 = x6;
    x3 = shufps(x3, x3, 0x4e);
    let t40 = x4;
    x4 = x5;
    x5 = mulps(x5, x3);
    let t30 = x4;
    x4 = x6;
    x6 = x7;
    x6 = subps(x6, x5);
    x5 = td0;
    x7 = tc0;
    x5 = subps(x5, x7);
    x5 = shufps(x5, x5, 0x4e);
    x7 = t80;
    x4 = mulps(x4, x7);
    x3 = mulps(x3, x7);
    x5 = subps(x5, x4);
    x2 = mulps(x2, x7);
    x3 = addps(x3, x5);
    x2 = shufps(x2, x2, 0xb1);
    x4 = x2;
    x4 = shufps(x4, x4, 0x4e);
    x5 = t40;
    let t20 = x6;
    x6 = x5;
    x5 = mulps(x5, x2);
    x6 = mulps(x6, x4);
    x5 = addps(x5, x3);
    x3 = x4;
    x4 = x5;
    x4 = subps(x4, x6);
    x5 = ta0;
    x6 = tb0;
    x5 = subps(x5, x6);
    x5 = shufps(x5, x5, 0x4e);
    x6 = t50;
    x6 = mulps(x6, x7);
    x6 = subps(x6, x5);
    x5 = t60;
    x7 = mulps(x7, x5);
    x6 = subps(x6, x7);
    x5 = t30;
    x2 = mulps(x2, x5);
    x5 = mulps(x5, x3);
    x6 = subps(x6, x2);
    x2 = x5;
    x2 = addps(x2, x6);
    let det = x1[0];
    if det == 0.0 || det.is_nan() {
        return None;
    }
    let r = rcp(det);
    let inv = splat(sub(add(r, r), mul(det, mul(r, r))));
    Some([mulps(te0, inv), mulps(x4, inv), mulps(t20, inv), mulps(inv, x2)])
}

/// `NLB_ComposeTransformEH` (0x1008d420): `inverse(R2) * S * R2 * R1 * T`, the rotations from the quaternions, `S` the
/// scale and `T` the translation. `None` when `R2` has no inverse.
pub fn compose(t: &HhTransform) -> Option<Mat4> {
    let r2 = quat_matrix(t.rotation2);
    let r2_inv = inverse(&r2)?;
    let r1 = quat_matrix(t.rotation);
    let m = mat_mul(&r2_inv, &scale_matrix(t.scale));
    let m = mat_mul(&m, &r2);
    let m = mat_mul(&m, &r1);
    Some(mat_mul(&m, &translation_matrix(t.translation)))
}

/// The SSE point transform (0x10156035): `(v0 * M0 + v1 * M1) + (v2 * M2 + M3)`, then divided by its `w` through
/// `rcpps` and one Newton step. Returns x, y, z.
pub fn transform_point(v: [f32; 3], m: &Mat4) -> [f32; 3] {
    let hi = addps(mulps(m[2], splat(v[2])), m[3]);
    let mid = mulps(m[1], splat(v[1]));
    let out = addps(addps(mulps(m[0], splat(v[0])), mid), hi);
    let r = rcp(out[3]);
    let inv = sub(add(r, r), mul(mul(r, out[3]), r));
    [mul(out[0], inv), mul(out[1], inv), mul(out[2], inv)]
}

/// A collision body's grid vertices as the loader emits them: through `compose(read_transform(block)) * world`,
/// stored `(x, z, y)`. `None` when the transform has no inverse.
pub fn grid_vertices(body: &Collision) -> Option<Vec<[f32; 3]>> {
    let a = compose(&read_transform(body.node.transform.words()))?;
    let m = mat_mul(&a, &world_matrix(body.node.world));
    Some(
        body.vertices
            .iter()
            .map(|&v| {
                let [x, y, z] = transform_point(v, &m);
                [x, z, y]
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{inverse, mat_mul, quat_matrix, read_transform, transform_point, Mat4};

    // Random cases run on HedgeHog3D.dll's own code in unicorn (the SSE slots, the reference CPU's rcp table, x87 at
    // 24-bit precision control), as bits.
    const INVERSE: &[([u32; 16], [u32; 16])] = &[
        (
            [
                0xbfed4c53, 0x3f308186, 0x3e6f6d7b, 0xbbaeb0e1, 0x3da558bc, 0x3f2d04f5, 0x3fe0f6db, 0x3e311d18,
                0xbf2009be, 0x3ff529ea, 0xbef1150b, 0x3fa07a19, 0x3fd8f75a, 0x3f94584f, 0x3f1d1b7f, 0xbd8c7155,
            ],
            [
                0xbeb73483, 0xbca6b50d, 0x3c4c07be, 0x3e510fbb, 0x3f0229c7, 0xbe833f60, 0x3d8f68e4, 0x3f18190a,
                0xbda80a91, 0x3f1bc402, 0xbdbdc171, 0xbe1c9009, 0xbf7c6af4, 0x3f1c287e, 0x3f297c82, 0xbf5d006e,
            ],
        ),
        (
            [
                0xbf9924a5, 0xbe783f44, 0xbee356af, 0x3f941047, 0x3fcefa0d, 0x3eebd4e3, 0x3fe09835, 0xbfe801c7,
                0xbe316e6c, 0xbfe856d4, 0x3fee8ea0, 0xbf89b458, 0xbe0d5aa2, 0xbefe6a83, 0x3ffcc85d, 0x3fb4de91,
            ],
            [
                0xc00f8acf, 0xbf8141fc, 0xbdb17a6c, 0x3ef30fa8, 0x3fc4e531, 0x3f8992e7, 0xbedac31b, 0xbe52ce9b,
                0x3f510d27, 0x3f1ddc8a, 0x3c3c2cae, 0x3e0664e6, 0xbf51c80b, 0xbf151fb0, 0xbe32f6e9, 0x3eff1a73,
            ],
        ),
        (
            [
                0x3dfe141d, 0xbfe0ac27, 0x3f88a38d, 0x3fd72c04, 0xbb5d7695, 0x3fcf23ad, 0xbfc4cebf, 0xbe96db99,
                0xbf86d082, 0xbfd78e86, 0x3fae21eb, 0xbf8c0860, 0x3ff959af, 0x3fad3c24, 0x3e882f99, 0xbed933b4,
            ],
            [
                0x40a77ff0, 0x4104d95e, 0x40979e5a, 0x403063c8, 0xc0e17e3a, 0xc1341f2f, 0xc0d2a1eb, 0xc0484a83,
                0xc0dcb7b1, 0xc138d9ae, 0xc0ca5ba3, 0xc03ffc13, 0xc0313aa7, 0xc0a0fd88, 0xc04d3db2, 0xbfc879ca,
            ],
        ),
    ];
    const MAT_MUL: &[([u32; 16], [u32; 16], [u32; 16])] = &[
        (
            [
                0xbf6997e6, 0xbfea6763, 0xbf405a5f, 0xbfd9930f, 0x3d5e4e8b, 0x3fe9e691, 0xbf2fdfef, 0x3f954619,
                0x3e801e24, 0x3faca193, 0x3ffe515c, 0xbfd57185, 0x3fc7a148, 0x3f3515ad, 0xbf6b0024, 0x3efcdc4b,
            ],
            [
                0xbfe0dda1, 0xbfc15053, 0xbe45c594, 0xbf7cc740, 0xbff72b3d, 0x3ed9d3cb, 0x3dbed087, 0xbf5a8da5,
                0xbfdff024, 0xbfed38f7, 0x3fe0cdfd, 0xbf2f9f59, 0x3f9f653a, 0xbfbf4904, 0x3fe9d2a6, 0xbf8bde4f,
            ],
            [
                0x408ac910, 0x409103c3, 0xc08d6972, 0x409acaef, 0xbf784428, 0x3e6755e8, 0x3f8ab146, 0xc01aaab8,
                0xc1098b2e, 0xbf7e907c, 0x3f054b5c, 0xbf707aca, 0xbff14070, 0xbf8badfa, 0xbf720342, 0xc0037130,
            ],
        ),
        (
            [
                0xbfd4e3a7, 0x3fe62cc9, 0xbfd149c8, 0x3f0e0dd8, 0x3da35b60, 0x3ec07764, 0x3e1103c9, 0x3e8af201,
                0x3fae2f81, 0x3fd54ed5, 0x3fa31729, 0xbcc59d32, 0x3fdf8717, 0xbeb624c4, 0x3fcd9150, 0x3f949b93,
            ],
            [
                0x3f9628ad, 0x3f190c6c, 0x3f965220, 0xbf8fb0d9, 0x3f7a2a72, 0xbf9147c4, 0x3f6b60e2, 0x3faa8ba7,
                0x3f9e0f60, 0x3fdf7521, 0xbf22868d, 0xbf7c481d, 0x3fdd215a, 0xbf34b2fa, 0xbef85912, 0xbff0d886,
            ],
            [
                0xbfa08c4a, 0xc0c901b0, 0x3ef02752, 0x409a9162, 0x3f8d6416, 0xbea5876d, 0x3e5efd6b, 0xbe7499f4,
                0x409835ff, 0x3f94ed7b, 0x40155247, 0xbf047b70, 0x40b6134d, 0x405ba5c2, 0x3e1062c0, 0xc0c6736b,
            ],
        ),
    ];
    const POINT: &[([u32; 3], [u32; 16], [u32; 3])] = &[
        (
            [0xba072b19, 0xbf0c1b1b, 0x3ffc803a],
            [
                0x3f84dc04, 0x3f8398e0, 0x3eef2ee1, 0x3f21507e, 0xbd859bb8, 0x3fc0f2ef, 0xbf5ea68d, 0xbfc00967,
                0xbeb3de02, 0x3fe02c5d, 0x3f5c1ddc, 0xbf43edfc, 0xbfba937f, 0x3f2f948a, 0xbea68d97, 0x3dd6220b,
            ],
            [0x4067a684, 0xc0b582f6, 0xc04a35d6],
        ),
        (
            [0x3fcdf2fd, 0xbf2116e6, 0x3faafb20],
            [
                0xbf233543, 0xbf9ef039, 0x3fdbedf3, 0xbf4624c0, 0xbf2b4ebb, 0x3fd8f7b6, 0x3fab2ad9, 0xbf3a36fe,
                0x3d56473d, 0xbeb93efd, 0xbf2de9ea, 0xbf827fa0, 0xbf14f149, 0xbfbe4141, 0x3f23eb77, 0x3f4017f1,
            ],
            [0x3f4c4f6e, 0x40664721, 0xbf977e45],
        ),
        (
            [0xbb51e4ed, 0x3dd64edf, 0xbf8a5cd5],
            [
                0xbdd13301, 0xbf9cdc16, 0xbfc9f953, 0xbeca0002, 0x3fd699b1, 0xbf6f502f, 0xbfb14c2f, 0xbfc2f072,
                0x3f4af3f4, 0xbec61588, 0x3f6262a4, 0xbf27e9b5, 0x3f882273, 0x3ea77d80, 0x3ff33153, 0xbfb336de,
            ],
            [0xbee686c1, 0xbf44619e, 0xbf726a62],
        ),
    ];
    const QUAT: &[([u32; 4], [u32; 16])] = &[
        (
            [0xbed0cd8c, 0x3f08063a, 0xbea9d793, 0xbf041c2e],
            [
                0x3e5c6e65, 0xbdba6364, 0x3f51a821, 0x00000000, 0xbf469820, 0x3ee50302, 0x3d8c1284, 0x00000000,
                0xbe8e4136, 0xbf45ffa0, 0x3dd25b80, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x3f800000,
            ],
        ),
        (
            [0xbede842f, 0x3f580b9a, 0xbf31291b, 0x3f4dfc45],
            [
                0xbfb0ed72, 0xbfec714e, 0xbf41af41, 0x00000000, 0x3ec29ec2, 0xbeabd0fe, 0xbfef0846, 0x00000000,
                0x3ffad4c0, 0xbeeff498, 0xbf4d5beb, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x3f800000,
            ],
        ),
        (
            [0xbf18aeae, 0xbcb557ae, 0xbe7e2c53, 0xbd1d73e7],
            [
                0x3f603446, 0x3d3a51ed, 0x3e96b8ae, 0x00000000, 0x3befed58, 0x3e2952d2, 0x3d68d39b, 0x00000000,
                0x3e9876d2, 0xbd0ecd69, 0x3e933ff3, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x3f800000,
            ],
        ),
    ];

    fn mat(w: &[u32; 16]) -> Mat4 {
        std::array::from_fn(|r| std::array::from_fn(|c| f32::from_bits(w[4 * r + c])))
    }

    fn words(m: &Mat4) -> Vec<u32> {
        m.as_flattened().iter().map(|v| v.to_bits()).collect()
    }

    #[test]
    fn matrix_ops_match_the_dll() {
        for (m, want) in INVERSE {
            assert_eq!(words(&inverse(&mat(m)).unwrap()), want);
        }
        for (a, b, want) in MAT_MUL {
            assert_eq!(words(&mat_mul(&mat(a), &mat(b))), want);
        }
        for (v, m, want) in POINT {
            assert_eq!(transform_point(v.map(f32::from_bits), &mat(m)).map(f32::to_bits), *want);
        }
        for (q, want) in QUAT {
            assert_eq!(words(&quat_matrix(q.map(f32::from_bits))), want);
        }
    }

    // ReadTransform's quaternion on the reference CPU (the test machine's i9-10980HK: x87 at
    // 24-bit precision control): angle and axis, then the quaternion, as bits. Six angles each up to 3.2, 7, 100 and
    // 30000 (24 of rt.c's 64 cases; all 64 are equal).
    const READ_TRANSFORM: &[(u32, [u32; 3], [u32; 4])] = &[
        (0x3e426f80, [0xbf5549b7, 0xbedf7c2c, 0x3c59d680], [0xbda1c07d, 0xbd297c30, 0x3aa533d4, 0x3f7ed8de]),
        (0xbec9e608, [0xbf2cae3c, 0x3d9ca4f0, 0x3f0a30dc], [0x3e074e84, 0xbc757b7d, 0xbdd8903b, 0x3f7b0a47]),
        (0x40058d6b, [0xbf0a9d6a, 0xbe320610, 0xbec7c368], [0xbeef8e90, 0xbe19d4fa, 0xbeac9e0b, 0x3f00d887]),
        (0x40479709, [0xbf15342c, 0x3dcc6cf0, 0x3f6db55a], [0xbf1531a5, 0x3dcc697a, 0x3f6db154, 0x3c3c67c1]),
        (0x3ff8274a, [0x3f5d3548, 0x3f4f8508, 0xbefcec16], [0x3f3663da, 0x3f2b1a89, 0xbed089f3, 0x3f10daa1]),
        (0xc0044536, [0x3f55c6f8, 0xbf2c09a2, 0x3f1e9752], [0xbf37a3d6, 0x3f13c8de, 0xbf083bd5, 0x3f030e14]),
        (0x3f6def50, [0xbe2b143c, 0xbdb328a0, 0x3e210c00], [0xbd995842, 0xbd20963c, 0x3d905a40, 0x3f64d9d2]),
        (0xc0a55a57, [0x3d45d3a0, 0x3f4b3444, 0x3e2d1188], [0xbcd17aad, 0xbed72c67, 0xbdb74340, 0xbf592cd4]),
        (0x3ff00030, [0xbc47f440, 0x3f591aa4, 0x3e08db48], [0xbc212df8, 0x3f2f00fd, 0x3ddca2ad, 0x3f178063]),
        (0xbea43a00, [0x3de86400, 0x3e337950, 0xbf5237c0], [0xbc947152, 0xbce54861, 0x3e064789, 0x3f7cb6fb]),
        (0x40898f0a, [0x3f05cc30, 0xbf04d6ba, 0x3e308c20], [0x3ee00b63, 0xbede705d, 0x3e13d085, 0xbf0bfc45]),
        (0xbf906240, [0xbecb0b56, 0x3f4c7b6c, 0xbec6e69a], [0x3e591537, 0xbeda9ec0, 0x3e54a724, 0x3f585a0c]),
        (0xc281a841, [0x3f0c7a3c, 0x3f3714ba, 0x3e22cbf8], [0xbeec2b79, 0xbf19e5b3, 0xbe08d8af, 0x3f0aaa54]),
        (0x42684ed8, [0x3f49dba2, 0x3d641600, 0x3ed5c624], [0xbf0bac1d, 0xbd1dd202, 0xbe93eadd, 0xbf38d270]),
        (0xc2b96534, [0xbe3f12c8, 0xbeed8012, 0x3f0cf0d8], [0x3e05b482, 0x3ea63155, 0xbec53fb7, 0xbf36e19c]),
        (0x420572a4, [0xbeb834e4, 0xbf271aee, 0x3f21ddbc], [0x3e984334, 0x3f0a2072, 0xbf05cbd5, 0xbf101463]),
        (0xc15adb78, [0xbf248095, 0x3ee45c98, 0x3f4ed1ee], [0x3eadacfb, 0xbe7118a3, 0xbeda5a72, 0x3f596cdf]),
        (0x4266efd4, [0xbe86842e, 0x3e4d2f70, 0x3e980db0], [0x3e164c41, 0xbde541fe, 0xbe29e47a, 0xbf545355]),
        (0x46c85280, [0x3f61bea6, 0xbed0abf4, 0x3e543498], [0x3e404ff7, 0xbdb1c48f, 0x3d34c73a, 0xbf7a2071]),
        (0xc625dac0, [0xbf7f43fa, 0x3f00523c, 0x3e0ede70], [0xbf6d55e1, 0x3eee9d9c, 0x3e04d56b, 0xbebc8090]),
        (0x46c44244, [0x3edec448, 0xbf362f0c, 0xbebbe3c2], [0x3e4d5572, 0xbea7ed3c, 0xbe2d2fa3, 0x3f633106]),
        (0x462a0874, [0xbf6b5dea, 0xbed765a2, 0x3ca55080], [0x3e2a4012, 0x3d9bce27, 0xbb6f2839, 0x3f7bc78a]),
        (0xc5665780, [0xbe9e4752, 0xbf04a402, 0x3f1a425e], [0x3e9b6bf9, 0x3f023f1f, 0xbf177997, 0xbe41ad40]),
        (0x46d0b456, [0xbe19a0d4, 0xbf47893b, 0xbc007700], [0x3dfd1991, 0x3f245db9, 0x3bd3a4c4, 0x3f1124aa]),
    ];

    #[test]
    fn read_transform_matches_the_cpu() {
        for &(angle, axis, want) in READ_TRANSFORM {
            let mut w = [0.0; 14];
            w[3] = f32::from_bits(angle);
            for (k, a) in axis.into_iter().enumerate() {
                w[4 + k] = f32::from_bits(a);
            }
            let got = read_transform(w).rotation.map(f32::to_bits);
            assert_eq!(got, want, "angle {angle:#x}");
        }
    }
}
