//! `E37GridOps::EdgeEdge` (exe 0x4f8280): sweep a segment against the grid's triangle faces.

use crate::math::vec::{Vec3, Vec4};

use super::elem;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub centre: Vec3,
    pub half: Vec3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Face {
    pub vertices: [u8; 3],
}

/// Memory-side access for the original grid's bounds, rows, vertices, and optional sampled surface.
pub trait GridData {
    fn bounds(&mut self) -> Bounds;
    fn face_count(&mut self) -> u32;
    fn face_is_valid(&mut self, index: u32) -> bool;
    fn face(&mut self, index: u32) -> Face;
    fn sample_indices(&mut self, index: u32) -> [u8; 3];
    fn vertices(&mut self, indices: [u8; 3]) -> [Vec3; 3];
    fn hit_word(&mut self) -> u32;
    fn sample_dimensions(&mut self) -> Option<(u16, u16)>;
    fn vertex_samples(&mut self, indices: [u8; 3]) -> [[u8; 2]; 3];
    fn sample_bytes(&mut self, first: i32, second: i32) -> [u8; 2];
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hit {
    pub point: Vec4,
    pub distance: f32,
    pub normal: Vec4,
    pub word: u32,
    pub samples: Option<[u8; 2]>,
}

/// `E37GridOps::EdgeEdge`. `start` supplies all four lanes used for the hit point; only xyz of `end` is read.
pub fn edge_edge(grid: &mut impl GridData, start: Vec4, end: Vec3) -> Option<Hit> {
    let delta = Vec3 { x: end.x - start.x, y: end.y - start.y, z: end.z - start.z };
    let half = delta.scale(0.5);
    if !overlaps(grid.bounds(), start, half) {
        return None;
    }

    let count = grid.face_count();
    for index in 0..count {
        if !grid.face_is_valid(index) {
            continue;
        }
        let face = grid.face(index);
        let vertices = grid.vertices(face.vertices);
        let result = elem::segment_triangle(
            Vec3 { x: start.x, y: start.y, z: start.z },
            delta,
            vertices[0],
            vertices[1],
            vertices[2],
        );
        let Some(distance) = result.t else {
            continue;
        };
        if !(distance > 1.0e-5 && distance < 1.01) {
            continue;
        }

        let n = result.normal.expect("a segment-triangle hit has a normal");
        let normalized = Vec4 { x: n.x, y: n.y, z: n.z, w: distance }.normalize();
        let normal = Vec4 {
            x: normalized.x,
            y: normalized.y,
            z: normalized.z,
            w: -((vertices[0].z * normalized.z + vertices[0].y * normalized.y) + vertices[0].x * normalized.x),
        };
        let delta4 = Vec4 {
            x: delta.x,
            y: delta.y,
            z: delta.z,
            // EdgeEdge's fourth point lane is the start lane unchanged in the original call.
            // The native routine's stack temporary does not carry half.x into this lane.
            w: 0.0,
        };
        let point = start.add_scaled(delta4, distance);

        let word = grid.hit_word();
        let samples = if let Some((width, height)) = grid.sample_dimensions() {
            let sample_indices = grid.sample_indices(index);
            let [r0, r1, r2] = grid.vertex_samples(sample_indices);
            let u = result.u.expect("a segment-triangle hit has u");
            let v = result.v.expect("a segment-triangle hit has v");
            let w = 1.0 - (v + u);
            let g0 = (((r1[0] as f32 * u) + (r2[0] as f32 * v)) + (r0[0] as f32 * w)) * (1.0 / 256.0);
            let g1 = (((r0[1] as f32 * w) + (r1[1] as f32 * u)) + (r2[1] as f32 * v)) * (1.0 / 256.0);
            let width_f = f32::from(width);
            let height_f = f32::from(height);
            let first = sample_index(width, width_f * g0, width_f * g1);
            let second = sample_index(height, height_f * g0, height_f * g1);
            Some(grid.sample_bytes(first, second))
        } else {
            None
        };

        return Some(Hit { point, distance, normal, word, samples });
    }
    None
}

/// The original's 0x4f7640 segment-vs-box test, including its three cross axes.
fn overlaps(bounds: Bounds, start: Vec4, half: Vec3) -> bool {
    let t = Vec3 {
        x: (start.x + half.x) - bounds.centre.x,
        y: (start.y + half.y) - bounds.centre.y,
        z: (start.z + half.z) - bounds.centre.z,
    };
    if t.x.abs() > half.x.abs() + bounds.half.x
        || t.y.abs() > half.y.abs() + bounds.half.y
        || t.z.abs() > half.z.abs() + bounds.half.z
    {
        return false;
    }
    let cross = half.cross3(t);
    if cross.x.abs() > half.z.abs() * bounds.half.y + half.y.abs() * bounds.half.z
        || cross.y.abs() > half.z.abs() * bounds.half.x + half.x.abs() * bounds.half.z
        || cross.z.abs() > half.y.abs() * bounds.half.x + half.x.abs() * bounds.half.y
    {
        return false;
    }
    true
}

/// The sample atlas' row-major index, with the original `cvttss2si` truncation and 32-bit wrapping.
pub(super) fn sample_index(size: u16, x: f32, y: f32) -> i32 {
    let row = crate::fpu::trunc_i32(f64::from(y));
    let column = crate::fpu::trunc_i32(f64::from(x));
    row.wrapping_mul(i32::from(size)).wrapping_add(column)
}
