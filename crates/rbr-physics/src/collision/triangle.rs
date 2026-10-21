//! Triangle tests: a segment or ray against a triangle's front face, a point behind a face, and the unnormalised
//! terms the grid builder derives from triangle vertices. Each sum keeps the order its instructions gave.

use crate::fpu::round_x87;
use crate::math::sse;
use crate::math::vec::Vec3;

/// The scalar triple product `(a x b) . c`. The y and z terms are summed first and the x term added last, as the x87
/// chain did: `(y*y' + z*z') + x*x'`.
pub fn triple_product(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let cross = a.cross3(b);
    let yz = cross.y * c.y + cross.z * c.z;
    yz + cross.x * c.x
}

/// The unnormalised normal of the triangle `origin`, `b`, `c`: the cross product of `b - origin` and `c - origin`. All
/// three inputs are read before the result is stored, so a caller may store it over an input.
pub fn normal_from_vertex(origin: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    (b - origin).cross3(c - origin)
}

/// The terms of [`triangle_plane_terms`]: the sum of the three cross products of the first three vertices, and four dot
/// products. Their physical meaning is not known; the names say how they are computed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TriangleTerms {
    /// `n_ab + (n_bc + n_ca)`, where `n_ab = a x b`, `n_bc = b x c`, `n_ca = c x a`.
    pub cross_sum: Vec3,
    /// `point . n_ab`.
    pub point_dot_ab: f32,
    /// `point . n_bc`.
    pub point_dot_bc: f32,
    /// `point . n_ca`.
    pub point_dot_ca: f32,
    /// `c . n_ab`, summed with the y and z terms first.
    pub c_dot_ab: f32,
}

/// The cross-product terms of the triangle `a`, `b`, `c` and the dot products of `point` with them.
pub fn triangle_plane_terms(a: Vec3, b: Vec3, c: Vec3, point: Vec3) -> TriangleTerms {
    let n_ab = a.cross3(b);
    let n_bc = b.cross3(c);
    let n_ca = c.cross3(a);
    TriangleTerms {
        cross_sum: n_ab + (n_bc + n_ca),
        point_dot_ab: point.dot(n_ab),
        point_dot_bc: point.dot(n_bc),
        point_dot_ca: point.dot(n_ca),
        c_dot_ab: c.dot_yz_first(n_ab),
    }
}

/// The result of [`segment_triangle`]. A field is written as the test goes, so a miss keeps the fields written before
/// its exit; unset fields were not written.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SegmentTriangle {
    pub hit: bool,
    /// The segment parameter, `(n2 . d2) * w`.
    pub t: Option<f32>,
    /// `f1` (raw on a miss after the first test), `w * f1` on a hit. Meaning unknown.
    pub u: Option<f32>,
    /// `f1'` (raw on a miss after the second test), `f1' * w` on a hit. Meaning unknown.
    pub v: Option<f32>,
    /// `d1 x d2`, the cross product of the triangle's two edges from `v0`.
    pub normal: Option<Vec3>,
}

/// The dot product in the order the segment and ray tests sum it: `(z + y) + x`.
/// `Vec3::dot` sums `(x + y) + z`, which rounds differently.
fn dot_zyx(a: Vec3, b: Vec3) -> f32 {
    (a.z * b.z + a.y * b.y) + a.x * b.x
}

/// The segment from `start` along `delta` against the triangle `v0`, `v1`, `v2`.
pub fn segment_triangle(start: Vec3, delta: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> SegmentTriangle {
    let mut out = SegmentTriangle::default();
    let d1 = v1 - v0;
    let d2 = v2 - v0;
    let n1 = delta.cross3(d2);
    let f2 = dot_zyx(n1, d1);
    if !(1.0e-6f32 <= f2) {
        return out;
    }

    let t1 = start - v0;
    let f1 = dot_zyx(t1, n1);
    out.u = Some(f1);
    if !(0.0 <= f1 && f1 <= f2) {
        return out;
    }

    let n2 = t1.cross3(d1);
    let f1_next = dot_zyx(delta, n2);
    out.v = Some(f1_next);
    if !(0.0 <= f1_next && f1_next + f1 <= f2) {
        return out;
    }

    let w = 1.0 / f2;
    out.t = Some(dot_zyx(n2, d2) * w);
    out.u = Some(w * f1);
    out.v = Some(f1_next * w);
    out.normal = Some(d1.cross3(d2));
    out.hit = true;
    out
}

/// A directed ray against a triangle: the scalar hit distance, or `None` when the ray misses. The tests take the
/// unordered-NaN branch as the instructions did (`x87_le_unordered`).
pub fn ray_triangle_distance(origin: Vec3, direction: Vec3, base: Vec3, end1: Vec3, end2: Vec3) -> Option<f32> {
    let d1 = end1 - base;
    let d2 = end2 - base;
    let n1 = direction.cross3(d2);
    let f3 = dot_zyx(n1, d1);
    if !x87_le_unordered(1.0e-6f32, f3) {
        return None;
    }

    let t1 = origin - base;
    let f1 = dot_zyx(t1, n1);
    if !x87_le_unordered(0.0, f1) || !x87_le_unordered(f1, f3) {
        return None;
    }

    let n2 = t1.cross3(d1);
    let f2 = dot_zyx(direction, n2);
    if !x87_le_unordered(0.0, f2) || !x87_le_unordered(f2 + f1, f3) {
        return None;
    }

    Some(dot_zyx(n2, d2) / f3)
}

fn x87_le_unordered(left: f32, right: f32) -> bool {
    // The retail FCOMI/JBE sequence also takes the branch for unordered NaNs.
    !(left > right)
}

/// What [`ray_triangle`] leaves in its result. A field is written as the test goes, so a miss keeps the fields written
/// before its exit: `u` after the front test, `v` after the barycentric test. Unset fields were not written.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RayTriangleResult {
    pub hit: bool,
    /// The ray parameter, `dot(e2, q) * inv`.
    pub distance: Option<f32>,
    /// `dot(s, p)`, scaled by `1/det` on a hit.
    pub u: Option<f32>,
    /// `dot(q, direction)`, scaled by `1/det` on a hit.
    pub v: Option<f32>,
    /// The triangle normal `cross(e2, e1)`.
    pub normal: Option<Vec3>,
}

/// Ray/triangle intersection with the barycentric output.
pub fn ray_triangle(origin: Vec3, direction: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> RayTriangleResult {
    let mut result = RayTriangleResult::default();
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let p = e2.cross3(direction);
    let det = p.dot(e1);
    if det < 0.000001 {
        return result;
    }

    let s = origin - v0;
    let t = s.dot(p);
    result.u = Some(t);
    if t < 0.0 || t > det {
        return result;
    }

    let q = e1.cross3(s);
    let u = q.dot(direction);
    result.v = Some(u);
    if u < 0.0 || t + u > det {
        return result;
    }

    let inv = 1.0 / det;
    result.distance = Some(e2.dot(q) * inv);
    result.u = Some(t * inv);
    result.v = Some(u * inv);
    result.normal = Some(e2.cross3(e1));
    result.hit = true;
    result
}

/// What [`point_triangle`] leaves in its result. A field is written as the test goes, so a miss keeps the fields
/// written before its exit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointTriangle {
    /// The face normal `e2 x e1`, unit length once the depth test ran.
    pub normal: Vec3,
    /// The foot's coordinate along `e1`, after the depth test.
    pub t: Option<f32>,
    /// Its coordinate along `e2`, after the `t` test.
    pub u: Option<f32>,
    /// On a hit: the foot of the point on the face's plane.
    pub foot: Option<Vec3>,
}

/// The depth window of [`point_triangle`], behind the face.
const MIN_DEPTH: f32 = 0.00001;
const MAX_DEPTH: f32 = 0.5;

/// Each lane times `s` (`mulss`): a degenerate face's infinite scale makes the indefinite NaN, as the instruction does.
fn scale_sse(v: Vec3, s: f32) -> Vec3 {
    Vec3 { x: sse::mul(v.x, s), y: sse::mul(v.y, s), z: sse::mul(v.z, s) }
}

/// An x87 product of two f32 values at the thread's precision, stored as f32 (`fmul`, `fstp`): an invalid product
/// (0 * inf) is the negative indefinite NaN, as on the x87.
fn x87_mul(a: f32, b: f32) -> f32 {
    let product = f64::from(a) * f64::from(b);
    if product.is_nan() && !a.is_nan() && !b.is_nan() {
        return f32::from_bits(0xffc0_0000);
    }
    round_x87(product) as f32
}

/// The point against the face `v0, v1, v2`. A hit is a point behind the face (against its normal `e2 x e1`) by 1e-5
/// to 0.5 whose foot on the plane lies inside the face. The normal's length and the two reciprocals are x87 at the
/// thread's precision (53 bits in NGP's race steps: the squares and sums stay on the stack, only the reciprocals are
/// stored); the rest is SSE in instruction order. The window tests stay comparisons: a NaN passes them, as
/// `comiss`/`ja` and `fcomi`/`ja` let it through (a degenerate face is a hit with NaN fields).
#[allow(clippy::manual_range_contains)]
pub fn point_triangle(point: Vec3, v0: Vec3, v1: Vec3, v2: Vec3) -> PointTriangle {
    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let s = point - v0;
    let normal = e2.cross3(e1);
    let mut result = PointTriangle { normal, ..PointTriangle::default() };
    let side = normal.dot(s);
    if side > 0.0 {
        return result;
    }
    let round = round_x87;
    let square = |x: f32| round(f64::from(x) * f64::from(x));
    let length2 = round(round(square(normal.x) + square(normal.y)) + square(normal.z));
    let length = round(length2.sqrt());
    let inverse = round(1.0 / length) as f32;
    let depth = sse::sub(0.0, sse::mul(inverse, side));
    if MIN_DEPTH > depth || depth > MAX_DEPTH {
        return result;
    }
    result.normal = scale_sse(normal, inverse);
    let offset = scale_sse(result.normal, depth);
    let q = offset.cross3(e2);
    let inverse_det = round(1.0 / f64::from(e1.dot(q))) as f32;
    let t = x87_mul(s.dot(q), inverse_det);
    result.t = Some(t);
    if 0.0 > t || t > 1.0 {
        return result;
    }
    let u = x87_mul(offset.dot(s.cross3(e1)), inverse_det);
    result.u = Some(u);
    if 0.0 > u || u + t > 1.0 {
        return result;
    }
    result.foot = Some(point + offset);
    result
}

/// The hit of [`mesh_ray_test`]: the clamped distance and the four tags of the mesh record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshHit {
    pub distance: f32,
    pub tags: [u32; 4],
}

/// A ray test against the mesh's two triangles: `(v0, v2, v1)`, then `(v0, v3, v2)` when the first misses. `vertex(i)`
/// and `tag(i)` read the mesh record where the original does, so a hit on the first triangle never reads `v3`.
pub fn mesh_ray_test(
    origin: Vec3,
    direction: Vec3,
    vertex: impl Fn(usize) -> Vec3,
    tag: impl Fn(usize) -> u32,
) -> Option<MeshHit> {
    let distance = ray_triangle_distance(origin, direction, vertex(0), vertex(2), vertex(1))
        .or_else(|| ray_triangle_distance(origin, direction, vertex(0), vertex(3), vertex(2)))?;
    Some(MeshHit { distance: if !(distance > 0.0) { 0.0 } else { distance }, tags: [tag(0), tag(1), tag(2), tag(3)] })
}
