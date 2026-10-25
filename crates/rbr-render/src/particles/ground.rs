//! The stage surface under a point, for playback, whose recordings hold no tyre-particle payload: the `.col` ground
//! triangle below the point and its `.mat` surface id, as the wheel's ground probe finds them
//! (`notes/formats.md` "Collision", Materials and surfaces: the barycentric-weighted low nibbles of the triangle's material
//! point, scaled by 1/15, index the run's cells; the `+6` attribute is the blend weight between the two materials and
//! `E36Triple::MinPick` keeps the first below 0.5). The probe's own rays and the entity hits are not reproduced: the
//! triangle is the highest one under the point within reach. Physics frame, z up.

use std::collections::HashMap;

use rbr_formats::{col, mat};

/// Grid cell size, metres.
const CELL: f32 = 4.0;
/// How far above the point a triangle may be and still count as its ground, metres.
const ABOVE: f32 = 0.5;

#[derive(Debug, Clone, Copy)]
struct Face {
    corners: [[f32; 3]; 3],
    /// The two surface ids at each corner, sampled per corner (their weighted mean picks the cell).
    grid: [[f32; 3]; 2],
    materials: [u8; 2],
    blend: [f32; 3],
}

/// A ground hit: the height and the surface id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub z: f32,
    pub surface: u32,
}

/// The stage's ground triangles in a grid of [`CELL`] squares, and the material runs of the selected `.mat` entry.
pub struct Ground {
    faces: Vec<Face>,
    cells: HashMap<(i32, i32), Vec<u32>>,
    runs: Vec<mat::MatRun>,
}

impl Ground {
    /// The ground of `col` with the runs of `mat`'s entry for `wear` and `wetness` (`"new"`, `"dry"`, ...).
    pub fn new(col: &col::ColFile, mat: &mat::MatFile, wear: &str, wetness: &str) -> Ground {
        let runs = mat::select(&mat.entries, wear, wetness).map(|i| mat.entries[i].runs.clone()).unwrap_or_default();
        let mut faces = Vec::new();
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for cell in &col.cells {
            for t in &cell.tree.triangles {
                let Some(corners) =
                    t.vertices.map(|v| cell.vertices.get(v as usize).copied()).into_iter().collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let corners: [[f32; 3]; 3] = [corners[0], corners[1], corners[2]];
                let (blend, _) = t.attr_a();
                let face = Face {
                    corners,
                    grid: [
                        std::array::from_fn(|i| f32::from(t.grid[i] & 0xf) / 15.0),
                        std::array::from_fn(|i| f32::from(t.grid[3 + i] & 0xf) / 15.0),
                    ],
                    materials: t.materials,
                    blend: blend.map(|b| f32::from(b) / 31.0),
                };
                let index = faces.len() as u32;
                let lo = |k: usize| corners.iter().map(|c| c[k]).fold(f32::MAX, f32::min);
                let hi = |k: usize| corners.iter().map(|c| c[k]).fold(f32::MIN, f32::max);
                let (x0, x1) = ((lo(0) / CELL).floor() as i32, (hi(0) / CELL).floor() as i32);
                let (y0, y1) = ((lo(1) / CELL).floor() as i32, (hi(1) / CELL).floor() as i32);
                // Walls and slivers spanning many cells are not ground.
                if (x1 - x0) > 16 || (y1 - y0) > 16 {
                    continue;
                }
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        cells.entry((x, y)).or_default().push(index);
                    }
                }
                faces.push(face);
            }
        }
        Ground { faces, cells, runs }
    }

    /// Whether the `.mat` had an entry for the wear and wetness (some RSF stages have only dry ones).
    pub fn has_surfaces(&self) -> bool {
        !self.runs.is_empty()
    }

    /// The ground at or below `p` (within [`ABOVE`] over it), or `None` off the collision mesh.
    pub fn below(&self, p: [f32; 3]) -> Option<Hit> {
        let key = ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32);
        let mut best: Option<(f32, &Face, [f32; 3])> = None;
        for &i in self.cells.get(&key)? {
            let face = &self.faces[i as usize];
            let Some(w) = barycentric(face.corners, p[0], p[1]) else { continue };
            let z = (0..3).map(|k| w[k] * face.corners[k][2]).sum::<f32>();
            if z <= p[2] + ABOVE && best.is_none_or(|(bz, _, _)| z > bz) {
                best = Some((z, face, w));
            }
        }
        let (z, face, w) = best?;
        let dot = |v: [f32; 3]| (0..3).map(|k| w[k] * v[k]).sum::<f32>();
        let sample = |k: usize| {
            let run = self.runs.get(face.materials[k] as usize)?;
            let width = run.width.clamp(1, 256) as usize;
            let cell = ((width as f32 * dot(face.grid[k])).floor() as usize).min(width - 1);
            Some(u32::from(run.cells[cell]))
        };
        let k = usize::from(dot(face.blend) >= 0.5);
        Some(Hit { z, surface: sample(k).unwrap_or(0) })
    }
}

/// The barycentric weights of `(x, y)` in the triangle's projection on the ground plane, or `None` outside it.
fn barycentric(c: [[f32; 3]; 3], x: f32, y: f32) -> Option<[f32; 3]> {
    let d = (c[1][1] - c[2][1]) * (c[0][0] - c[2][0]) + (c[2][0] - c[1][0]) * (c[0][1] - c[2][1]);
    if d.abs() < 1e-9 {
        return None;
    }
    let a = ((c[1][1] - c[2][1]) * (x - c[2][0]) + (c[2][0] - c[1][0]) * (y - c[2][1])) / d;
    let b = ((c[2][1] - c[0][1]) * (x - c[2][0]) + (c[0][0] - c[2][0]) * (y - c[2][1])) / d;
    let e = 1.0 - a - b;
    const SLACK: f32 = -1e-4;
    (a >= SLACK && b >= SLACK && e >= SLACK).then_some([a, b, e])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barycentric_weights_sum_to_one_inside_only() {
        let c = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        let w = barycentric(c, 0.5, 0.5).unwrap();
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!((w[1] - 0.25).abs() < 1e-6 && (w[2] - 0.25).abs() < 1e-6);
        assert!(barycentric(c, 2.0, 2.0).is_none());
    }
}
