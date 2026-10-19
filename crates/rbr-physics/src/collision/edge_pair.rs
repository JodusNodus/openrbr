//! `E37GridOps::EdgePair` (exe 0x4f8560) with its walk `Walk5FF0` (0x4f5ff0), the edge frame `M74A0` (0x4f74a0) and the
//! signed gap `Store5570` (0x4f5570). EdgePair walks each row of the grid against the segment pair and keeps the
//! earliest hit; the frame and the sample bytes come from that row. The x87 ops on this path run at the variant's
//! precision (`fpu::round_x87`: 24 bits in 1.02, 53 under NGP); the SSE ops go through `math::sse`. Each sum is in the
//! asm's order.

use crate::fpu::round_x87;
use crate::math::sse;
use crate::math::vec::{Vec3, Vec4};
use crate::Variant;

use super::edge_edge::sample_index;

/// The x87 "real indefinite" (the default NaN, negative), which an invalid x87 operation gives.
const X87_INDEFINITE: f64 = f64::from_bits(0xfff8_0000_0000_0000);

/// The x87 works in f64 here: the operands are f32-valued or 53-bit results, so f64 holds each result before
/// `round_x87` rounds it to the variant's precision. Unlike f32 the x87 keeps an extended exponent, so a value past
/// f32's range stays finite until it is stored (`as f32` rounds it to infinity, as `fstps` does).
fn x87_add(a: f64, b: f64, v: Variant) -> f64 {
    if a.is_infinite() && b.is_infinite() && a.is_sign_negative() != b.is_sign_negative() {
        return X87_INDEFINITE;
    }
    round_x87(a + b, v)
}

fn x87_sub(a: f64, b: f64, v: Variant) -> f64 {
    if a.is_infinite() && b.is_infinite() && a.is_sign_negative() == b.is_sign_negative() {
        return X87_INDEFINITE;
    }
    round_x87(a - b, v)
}

fn x87_mul(a: f64, b: f64, v: Variant) -> f64 {
    if (a == 0.0 && b.is_infinite()) || (a.is_infinite() && b == 0.0) {
        return X87_INDEFINITE;
    }
    round_x87(a * b, v)
}

fn x87_div(a: f64, b: f64, v: Variant) -> f64 {
    if (a == 0.0 && b == 0.0) || (a.is_infinite() && b.is_infinite()) {
        return X87_INDEFINITE;
    }
    round_x87(a / b, v)
}

/// The grid reads EdgePair makes. `this` is the grid; a row is two vertex slots (`+0x484`, `+0x485`), a slot is a
/// vertex at `(slot + 4) * 0x10`, and the sample tables are at `+0x400` (pairs), `+0x404`/`+0x406` (size),
/// `+0x408`/`+0x40c` (bytes), `+0x690 + slot` (the sample index of a vertex).
pub trait PairGrid {
    fn row_count(&self) -> u32;
    fn row(&self, index: u32) -> [u8; 2];
    fn vertex(&self, slot: u8) -> Vec3;
    fn sample_slot(&self, slot: u8) -> u8;
    fn sample_pair(&self, sample: u8) -> [u8; 2];
    fn sample_dimensions(&self) -> Option<(u16, u16)>;
    fn sample_bytes(&self, first: i32, second: i32) -> [u8; 2];
}

/// What Walk5FF0 returns: `t` (2.0 when the pair misses). `lerps` are the two points it writes (`p8`, `p9`) once the
/// first gate passes; `point` is its output (`p7`) when every gate passes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    pub t: f32,
    pub lerps: Option<[Vec4; 2]>,
    pub point: Option<Vec4>,
}

/// One accepted pair: the distance left in EdgePair's `t`, the hit point, the normal, and the sample bytes when the
/// grid has a sample atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairHit {
    pub point: Vec4,
    pub normal: Vec4,
    pub samples: Option<[u8; 2]>,
}

/// EdgePair's result: `t` is what it leaves in `*p7` (1.01 when no row beat it), `hit` is set when a row won.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairResult {
    pub t: f32,
    pub hit: Option<PairHit>,
}

/// `(a.z * b.z + a.y * b.y) + a.x * b.x`: the order the asm sums a dot product in (the z and y terms first).
fn dot_zyx(a: Vec3, b: Vec3) -> f32 {
    sse::add(sse::add(sse::mul(a.z, b.z), sse::mul(a.y, b.y)), sse::mul(a.x, b.x))
}

/// The three first lanes.
fn xyz(v: Vec4) -> Vec3 {
    Vec3 { x: v.x, y: v.y, z: v.z }
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3 { x: sse::sub(a.x, b.x), y: sse::sub(a.y, b.y), z: sse::sub(a.z, b.z) }
}

/// `Store5570` (0x4f5570): the cross of `(p2 - this)` and `(p3 - this)` (SSE) dotted with `p4 - this`. The third
/// difference and the products and sums are x87; the difference is stored as f32 before them (`fstps`). The sum is
/// returned in st0, unstored.
pub fn gap(this: Vec3, p2: Vec3, p3: Vec3, p4: Vec3, v: Variant) -> f64 {
    let n = sub(p2, this).cross3(sub(p3, this));
    let d = Vec3 {
        x: x87_sub(f64::from(p4.x), f64::from(this.x), v) as f32,
        y: x87_sub(f64::from(p4.y), f64::from(this.y), v) as f32,
        z: x87_sub(f64::from(p4.z), f64::from(this.z), v) as f32,
    };
    let term = |a: f32, b: f32| x87_mul(f64::from(a), f64::from(b), v);
    x87_add(x87_add(term(n.z, d.z), term(n.y, d.y), v), term(n.x, d.x), v)
}

/// `M74A0` (0x4f74a0): the unit normal of the edge frame. `e485` and `e484` are the row's two vertices, `p3` and `p4`
/// the two points of the pair. The normal is `(e485 - e484) x (p4 - p3)`, flipped when it points against `e485`, and
/// its w is `0 - (e484 . normal)`.
pub fn frame_normal(e485: Vec3, e484: Vec3, p3: Vec3, p4: Vec3) -> Vec4 {
    let mut n = sub(e485, e484).cross3(sub(p4, p3));
    if dot_zyx(e485, n) < 0.0 {
        n = n.scale(-1.0);
    }
    let unit = Vec4 { x: n.x, y: n.y, z: n.z, w: 0.0 }.normalize();
    let w = sse::sub(0.0, dot_zyx(e484, Vec3 { x: unit.x, y: unit.y, z: unit.z }));
    Vec4 { x: unit.x, y: unit.y, z: unit.z, w }
}

/// `Walk5FF0` (0x4f5ff0): one row of the grid (its vertices `e484`, `e485`) against the pair `p3`-`p4` and `p5`-`p6`.
/// Two gaps (`f2`, `f7`) must have opposite signs, and the four side normals must agree with the frame's
/// cross `g7`/`g8` (both signs) before the hit point is taken; each gate is an ordered compare, so a NaN passes it.
/// The gaps are stored as f32; `t` is computed on the stack and stored.
pub fn walk(e484: Vec3, e485: Vec3, p3: Vec4, p4: Vec4, p5: Vec4, p6: Vec4, v: Variant) -> Walk {
    let (ea, eb) = (e484, e485);
    let f2 = gap(ea, eb, xyz(p3), xyz(p4), v) as f32;
    let f7 = gap(ea, eb, xyz(p5), xyz(p6), v) as f32;
    let miss = |lerps| Walk { t: 2.0, lerps, point: None };
    if (f2 > 0.0 && f7 > 0.0) || (f2 < 0.0 && f7 < 0.0) {
        return miss(None);
    }
    let t = x87_div(f64::from(f2), x87_sub(f64::from(f2), f64::from(f7), v), v) as f32;
    let a = p3.lerp(p5, t);
    let b = p4.lerp(p6, t);
    let lerps = Some([a, b]);
    let (a3, b3) = (xyz(a), xyz(b));
    let n_a = sub(ea, a3).cross3(sub(b3, a3));
    let n_b = sub(b3, ea).cross3(sub(eb, ea));
    let n_c = sub(eb, b3).cross3(sub(a3, b3));
    let n_d = sub(a3, eb).cross3(sub(ea, eb));
    let frame = ea.cross3(eb);
    let g7 = dot_zyx(xyz(p5), frame);
    let g8 = dot_zyx(xyz(p6), frame);
    if (g7 > 0.0 && g8 > 0.0) || (g7 < 0.0 && g8 < 0.0) {
        return miss(lerps);
    }
    let sides = [dot_zyx(n_b, n_a), dot_zyx(n_c, n_b), dot_zyx(n_d, n_c), dot_zyx(n_d, n_a)];
    if sides.iter().any(|&d| d < 0.0) {
        return miss(lerps);
    }
    let s = sse::div(g7, sse::sub(g7, g8));
    Walk { t, lerps, point: Some(a.lerp(b, s)) }
}

/// `E37GridOps::EdgePair`: the earliest row (strictly smaller `t`, from 1.01) whose walk hits. The hit's frame and,
/// when the grid has a sample atlas, its two sample bytes come from that row.
pub fn edge_pair(grid: &impl PairGrid, a0: Vec4, a1: Vec4, b0: Vec4, b1: Vec4, v: Variant) -> PairResult {
    let mut t_min = 1.01_f32;
    let mut best = None;
    for index in 0..grid.row_count() {
        let rows = grid.row(index);
        let result = walk(grid.vertex(rows[0]), grid.vertex(rows[1]), a0, a1, b0, b1, v);
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
    let normal = frame_normal(grid.vertex(rows[1]), grid.vertex(rows[0]), xyz(a), xyz(b));
    let samples = grid.sample_dimensions().map(|dimensions| pair_samples(grid, rows, point, dimensions));
    PairResult { t: t_min, hit: Some(PairHit { point, normal, samples }) }
}

/// The two sample bytes of EdgePair's hit: the ratio `r` of the hit's distance along the row's edge (`|hit - e485|` over
/// `|e485 - e484|`) blends the two vertices' sample pairs, and each blended coordinate indexes its atlas.
fn pair_samples(grid: &impl PairGrid, rows: [u8; 2], hit: Vec4, (width, height): (u16, u16)) -> [u8; 2] {
    let e485 = grid.vertex(rows[1]);
    let e484 = grid.vertex(rows[0]);
    let r = sse::div(sub(xyz(hit), e485).length3(), sub(e485, e484).length3());
    let one_minus = sse::sub(1.0, r);
    let s485 = grid.sample_pair(grid.sample_slot(rows[1]));
    let s484 = grid.sample_pair(grid.sample_slot(rows[0]));
    let scale = 1.0 / 256.0;
    let g0 = sse::mul(sse::add(sse::mul(f32::from(s485[0]), one_minus), sse::mul(f32::from(s484[0]), r)), scale);
    let g1 = sse::mul(sse::add(sse::mul(f32::from(s484[1]), r), sse::mul(f32::from(s485[1]), one_minus)), scale);
    let (w, h) = (f32::from(width), f32::from(height));
    let first = sample_index(width, sse::mul(w, g0), sse::mul(w, g1));
    let second = sample_index(height, sse::mul(h, g0), sse::mul(h, g1));
    grid.sample_bytes(first, second)
}
