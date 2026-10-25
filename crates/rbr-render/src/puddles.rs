//! Where water stands on the wet roads ([`crate::renderer::Enhance::wet`], ours, not the game's look): how far each
//! ground vertex lies below the ground around it, so the puddles (`puddle_shape` in `water.wgsl`) are larger in dips
//! and along the road's lower edges. The game's own puddle quads (`.col` list A) are not used: only 10 of the 43
//! retail maps have them (gravel stages, none on tarmac).
//!
//! The road is every world-anchored, opaque, unlit draw with the ground's mip bias and a specular map, over all
//! meshes, since the stage splits it into chunks; measured against the road only (with the terrain about it, the
//! whole road lay below its banks). Each vertex is compared with the mean height of the
//! ground vertices in the 3 x 3 cells about it at two scales ([`CELLS`]), measured from the vertex's tangent plane
//! (its smooth normal) at their mean position: a discrete Laplacian, zero on a plane however steep, positive in a
//! hollow. The renderer passes it to the shader in the length of the vertex normal
//! (1 + lowness), which every other use normalises away.
//!
//! The ground without a specular map (every RSF stage's roads) takes the film by the collision surface under each
//! vertex ([`mark_roads`]), kept in the vertex's specular strength, which these draws leave at zero.

use crate::particles::ground::Ground;
use crate::stage::{Anchor, Draw, Layer, Mesh, Vertex};
use rbr_formats::lsp_keys::{Class, KEYS};
use std::collections::HashMap;
use std::sync::LazyLock;

/// The cells' sizes, metres, and each scale's share: ruts and potholes, then dips along the road.
const CELLS: [(f32, f32); 2] = [(1.0, 0.6), (4.0, 0.4)];
/// The depth below the mean, metres, that counts as fully low.
const DEEP: f32 = 0.08;

/// Whether `draw` of `mesh` is the ground: world-anchored, opaque and unlit, with the ground's mip bias.
fn ground(mesh: &Mesh, draw: &Draw) -> bool {
    mesh.anchor == Anchor::World
        && mesh.layer == Layer::Solid
        && draw.state.shade.is_none()
        && draw.state.alpha_ref.is_none()
        && draw.state.mip_bias
}

/// The share of the wet film a ground of surface `name` takes without a specular map (ours): tarmac all, gravel and
/// dirt half, the rest (grass, rock, verges) none.
fn film_by_name(name: &str) -> f32 {
    let name = name.trim_start_matches("US2_").trim_start_matches("BR2_");
    if name.starts_with("Tarmac_") {
        1.0
    } else if ["Gravel_", "Dirt_"].iter().any(|p| name.starts_with(p)) {
        0.5
    } else {
        0.0
    }
}

/// [`film_by_name`] of each surface id in the exe's `MaterialManager` table; none for an id it lacks.
static FILM: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let mut by_id = [0.0; 256];
    for key in KEYS.iter().filter(|k| k.class == Class::MaterialManager) {
        if let Some(share) = by_id.get_mut(key.offset as usize) {
            *share = film_by_name(key.key);
        }
    }
    by_id
});

/// The ground vertices of `meshes`' draws without a specular map with their share of the wet film ([`film_by_name`])
/// by the collision surface under them in their specular strength.
pub(crate) fn mark_roads(meshes: &mut [Mesh], collision: &Ground) {
    for mesh in meshes {
        let unmapped: Vec<usize> = mesh
            .draws
            .iter()
            .filter(|draw| ground(mesh, draw) && draw.state.specular.is_none())
            .flat_map(|draw| mesh.indices[draw.indices.clone()].iter().map(|&i| i as usize))
            .collect();
        for i in unmapped {
            let Some(v) = mesh.vertices.get_mut(i) else { continue };
            let [x, y, z] = v.position;
            let surface = collision.below([x, z, y]).map_or(0, |hit| hit.surface);
            v.specular[2] = FILM.get(surface as usize).copied().unwrap_or(0.0);
        }
    }
}

/// The sum of a cell's road positions and their count.
type Cells = HashMap<(i32, i32), ([f32; 3], u32)>;

/// Sums of the road's positions and their counts per cell, at each of [`CELLS`]' scales.
pub(crate) struct Heights {
    cells: [Cells; 2],
}

impl Heights {
    /// The ground vertices of `meshes` (with `vertices` in place of each mesh's own, by index).
    pub(crate) fn of<'a>(meshes: impl Iterator<Item = (&'a Mesh, &'a [Vertex])>) -> Heights {
        let mut heights = Heights { cells: [HashMap::new(), HashMap::new()] };
        for (mesh, vertices) in meshes {
            for i in ground_vertices(mesh) {
                heights.add(vertices[i].position);
            }
        }
        heights
    }

    fn add(&mut self, p: [f32; 3]) {
        for (scale, &(size, _)) in self.cells.iter_mut().zip(&CELLS) {
            let cell = scale.entry(cell_of(p, size)).or_insert(([0.0; 3], 0));
            cell.0 = [cell.0[0] + p[0], cell.0[1] + p[1], cell.0[2] + p[2]];
            cell.1 += 1;
        }
    }

    /// How low a point at `p` with normal `n` lies: 0 level with or above the ground about it, 1 [`DEEP`] or more
    /// below it.
    pub(crate) fn lowness(&self, p: [f32; 3], n: [f32; 3]) -> f32 {
        if n[1] < 0.3 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() {
            return 0.0;
        }
        let slope = [-n[0] / n[1], -n[2] / n[1]];
        let mut low = 0.0;
        for (scale, &(size, share)) in self.cells.iter().zip(&CELLS) {
            let (x, z) = cell_of(p, size);
            let (mut sum, mut count) = ([0.0; 3], 0);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    if let Some(&(s, n)) = scale.get(&(x + dx, z + dz)) {
                        sum = [sum[0] + s[0], sum[1] + s[1], sum[2] + s[2]];
                        count += n;
                    }
                }
            }
            if count > 0 {
                let mean = sum.map(|c| c / count as f32);
                let plane = p[1] + slope[0] * (mean[0] - p[0]) + slope[1] * (mean[2] - p[2]);
                low += share * ((mean[1] - plane) / DEEP).clamp(0.0, 1.0);
            }
        }
        low
    }
}

fn cell_of(p: [f32; 3], size: f32) -> (i32, i32) {
    ((p[0] / size).floor() as i32, (p[2] / size).floor() as i32)
}

/// The indices of `mesh`'s road vertices (with repeats): the ground's with a specular map, or marked by
/// [`mark_roads`].
fn ground_vertices(mesh: &Mesh) -> impl Iterator<Item = usize> + '_ {
    mesh.draws.iter().filter(|draw| ground(mesh, draw)).flat_map(move |draw| {
        let mapped = draw.state.specular.is_some();
        mesh.indices[draw.indices.clone()]
            .iter()
            .map(|&i| i as usize)
            .filter(move |&i| mesh.vertices.get(i).is_some_and(|v| mapped || v.specular[2] > 0.0))
    })
}

/// `vertices` (of `mesh`, with normals) with each ground vertex's normal lengthened by its lowness.
pub(crate) fn with_lowness(mut vertices: Vec<Vertex>, mesh: &Mesh, heights: &Heights) -> Vec<Vertex> {
    let mut done = vec![false; vertices.len()];
    for i in ground_vertices(mesh) {
        if std::mem::replace(&mut done[i], true) {
            continue;
        }
        let v = &mut vertices[i];
        let scale = 1.0 + heights.lowness(v.position, v.normal);
        v.normal = v.normal.map(|c| c * scale);
    }
    vertices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heights(f: impl Fn(f32, f32) -> f32) -> Heights {
        let mut heights = Heights { cells: [HashMap::new(), HashMap::new()] };
        for x in -40..=40 {
            for z in -40..=40 {
                let (x, z) = (x as f32 * 0.5, z as f32 * 0.5);
                heights.add([x, f(x, z), z]);
            }
        }
        heights
    }

    #[test]
    fn a_slope_is_not_low_and_a_hollow_is() {
        let slope = heights(|x, _| 0.3 * x);
        assert!(slope.lowness([0.25, 0.075, 0.25], [-0.3, 1.0, 0.0]) < 0.01);
        let hollow = heights(|x, z| 0.02 * (x * x + z * z));
        assert!(hollow.lowness([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]) > 0.5);
        let ridge = heights(|x, _| -0.05 * x * x);
        assert_eq!(ridge.lowness([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]), 0.0);
    }
}
