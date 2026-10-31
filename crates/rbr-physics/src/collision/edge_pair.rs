//! A pair of segments against a collision grid's edge rows: each row is walked against the pair and the earliest hit
//! is kept; the edge frame and the sample bytes come from that row. The x87 ops on this path run at the variant's
//! precision (`fpu::round_x87`: 24 bits in 1.02, 53 under NGP); the SSE ops go through `math::sse`. Each sum is in
//! the original's order.

use crate::fpu::{x87_add, x87_div, x87_mul, x87_sub};
use crate::math::sse;
use crate::math::vec::{Vec3, Vec4};

use super::face_sweep::table_index;

/// The grid reads the search makes: a row is two vertex slots, a slot names a vertex, and the sample data is a table
/// of byte pairs, the table sizes, the two byte tables and the sample index of each vertex.
pub trait PairGrid {
    fn row_count(&self) -> u32;
    fn row(&self, index: u32) -> [u8; 2];
    fn vertex(&self, slot: u8) -> Vec3;
    fn sample_slot(&self, slot: u8) -> u8;
    fn sample_pair(&self, sample: u8) -> [u8; 2];
    fn sample_dimensions(&self) -> Option<(u16, u16)>;
    fn sample_bytes(&self, first: i32, second: i32) -> [u8; 2];
}

/// What the walk of one row returns: `t` (2.0 when the pair misses). `lerps` are the two points interpolated once the
/// first gate passes; `point` is the hit point when every gate passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowWalk {
    pub t: f32,
    pub lerps: Option<[Vec4; 2]>,
    pub point: Option<Vec4>,
}

/// One accepted pair: the hit point, the normal, and the sample bytes when the grid has a sample atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairHit {
    pub point: Vec4,
    pub normal: Vec4,
    pub samples: Option<[u8; 2]>,
}

/// The search's result: `t` is the earliest row's (1.01 when no row beat it), `hit` is set when a row won.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairResult {
    pub t: f32,
    pub hit: Option<PairHit>,
}

/// `(a.z * b.z + a.y * b.y) + a.x * b.x`: the order the dot product is summed in (the z and y terms first).
fn dot_zyx(a: Vec3, b: Vec3) -> f32 {
    sse::add(sse::add(sse::mul(a.z, b.z), sse::mul(a.y, b.y)), sse::mul(a.x, b.x))
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 { x: sse::sub(a.x, b.x), y: sse::sub(a.y, b.y), z: sse::sub(a.z, b.z) }
}

/// Six times the signed volume of the tetrahedron `apex`, `b`, `c`, `d`: the cross of `b - apex` and `c - apex` (SSE)
/// dotted with `d - apex`. The third difference and the products and sums are x87; the difference is stored as f32
/// before them. The sum is returned unstored, in double precision.
pub fn signed_volume(apex: Vec3, b: Vec3, c: Vec3, d: Vec3) -> f64 {
    let n = sub(b, apex).cross3(sub(c, apex));
    let d = Vec3 {
        x: x87_sub(f64::from(d.x), f64::from(apex.x)) as f32,
        y: x87_sub(f64::from(d.y), f64::from(apex.y)) as f32,
        z: x87_sub(f64::from(d.z), f64::from(apex.z)) as f32,
    };
    let term = |a: f32, b: f32| x87_mul(f64::from(a), f64::from(b));
    x87_add(x87_add(term(n.z, d.z), term(n.y, d.y)), term(n.x, d.x))
}

/// The unit normal of the row's edge frame. `row_end` and `row_start` are the row's two vertices, `a` and `b` the two
/// points of the pair. The normal is `(row_end - row_start) x (b - a)`, flipped when it points against `row_end`, and
/// its w is `0 - (row_start . normal)`.
pub fn row_frame_normal(row_end: Vec3, row_start: Vec3, a: Vec3, b: Vec3) -> Vec4 {
    let mut n = sub(row_end, row_start).cross3(sub(b, a));
    if dot_zyx(row_end, n) < 0.0 {
        n = n.scale(-1.0);
    }
    let unit = n.extend(0.0).normalize();
    let w = sse::sub(0.0, dot_zyx(row_start, unit.xyz()));
    Vec4 { x: unit.x, y: unit.y, z: unit.z, w }
}

/// One row of the grid (its vertices `row_start`, `row_end`) against the pair `a0`-`a1` and `b0`-`b1`.
/// Two volumes (`f2`, `f7`) must have opposite signs, and the four side normals must agree with the frame's
/// cross `g7`/`g8` (both signs) before the hit point is taken; each gate is an ordered compare, so a NaN passes it.
/// The volumes are stored as f32; `t` is stored.
pub fn row_walk(row_start: Vec3, row_end: Vec3, a0: Vec4, a1: Vec4, b0: Vec4, b1: Vec4) -> RowWalk {
    let f2 = signed_volume(row_start, row_end, a0.xyz(), a1.xyz()) as f32;
    let f7 = signed_volume(row_start, row_end, b0.xyz(), b1.xyz()) as f32;
    let miss = |lerps| RowWalk { t: 2.0, lerps, point: None };
    if (f2 > 0.0 && f7 > 0.0) || (f2 < 0.0 && f7 < 0.0) {
        return miss(None);
    }
    let t = x87_div(f64::from(f2), x87_sub(f64::from(f2), f64::from(f7))) as f32;
    let a = a0.lerp(b0, t);
    let b = a1.lerp(b1, t);
    let lerps = Some([a, b]);
    let (a3, b3) = (a.xyz(), b.xyz());
    let n_a = sub(row_start, a3).cross3(sub(b3, a3));
    let n_b = sub(b3, row_start).cross3(sub(row_end, row_start));
    let n_c = sub(row_end, b3).cross3(sub(a3, b3));
    let n_d = sub(a3, row_end).cross3(sub(row_start, row_end));
    let frame = row_start.cross3(row_end);
    let g7 = dot_zyx(b0.xyz(), frame);
    let g8 = dot_zyx(b1.xyz(), frame);
    if (g7 > 0.0 && g8 > 0.0) || (g7 < 0.0 && g8 < 0.0) {
        return miss(lerps);
    }
    let sides = [dot_zyx(n_b, n_a), dot_zyx(n_c, n_b), dot_zyx(n_d, n_c), dot_zyx(n_d, n_a)];
    if sides.iter().any(|&d| d < 0.0) {
        return miss(lerps);
    }
    let s = sse::div(g7, sse::sub(g7, g8));
    RowWalk { t, lerps, point: Some(a.lerp(b, s)) }
}

/// The earliest row (strictly smaller `t`, from 1.01) whose walk hits. The hit's frame and, when the grid has a
/// sample atlas, its two sample bytes come from that row.
pub fn edge_pair(grid: &impl PairGrid, a0: Vec4, a1: Vec4, b0: Vec4, b1: Vec4) -> PairResult {
    let mut t_min = 1.01_f32;
    let mut best = None;
    for index in 0..grid.row_count() {
        let rows = grid.row(index);
        let result = row_walk(grid.vertex(rows[0]), grid.vertex(rows[1]), a0, a1, b0, b1);
        match (result.point, result.lerps) {
            (Some(point), Some([a, b])) if result.t < t_min => {
                t_min = result.t;
                best = Some((rows, point, a, b));
            }
            _ => {}
        }
    }
    let Some((rows, point, a, b)) = best else {
        return PairResult { t: t_min, hit: None };
    };
    let normal = row_frame_normal(grid.vertex(rows[1]), grid.vertex(rows[0]), a.xyz(), b.xyz());
    let samples = grid.sample_dimensions().map(|dimensions| pair_samples(grid, rows, point, dimensions));
    PairResult { t: t_min, hit: Some(PairHit { point, normal, samples }) }
}

/// The two sample bytes of the hit: the ratio `r` of the hit's distance along the row's edge (`|hit - row_end|` over
/// `|row_end - row_start|`) blends the two vertices' sample pairs, and each blended coordinate indexes its atlas.
fn pair_samples(grid: &impl PairGrid, rows: [u8; 2], hit: Vec4, (width, height): (u16, u16)) -> [u8; 2] {
    let row_end = grid.vertex(rows[1]);
    let row_start = grid.vertex(rows[0]);
    let r = sse::div(sub(hit.xyz(), row_end).length3(), sub(row_end, row_start).length3());
    let one_minus = sse::sub(1.0, r);
    let end_sample = grid.sample_pair(grid.sample_slot(rows[1]));
    let start_sample = grid.sample_pair(grid.sample_slot(rows[0]));
    let scale = 1.0 / 256.0;
    let g0 = sse::mul(
        sse::add(sse::mul(f32::from(end_sample[0]), one_minus), sse::mul(f32::from(start_sample[0]), r)),
        scale,
    );
    let g1 = sse::mul(
        sse::add(sse::mul(f32::from(start_sample[1]), r), sse::mul(f32::from(end_sample[1]), one_minus)),
        scale,
    );
    let (w, h) = (f32::from(width), f32::from(height));
    let first = table_index(width, sse::mul(w, g0), sse::mul(w, g1));
    let second = table_index(height, sse::mul(h, g0), sse::mul(h, g1));
    grid.sample_bytes(first, second)
}
