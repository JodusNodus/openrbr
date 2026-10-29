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
//! The road is the ground whose collision surface takes the film ([`mark_roads`], [`Vertex::wet`]), the retail
//! stages' and the RSF stages' alike; the game's specular maps (only on retail roads) are left to its own shine.

use crate::particles::ground::Ground;
use crate::stage::{Anchor, Draw, Layer, Mesh, TextureRef, Vertex};
use rbr_formats::lsp_keys::{Class, KEYS};
use std::collections::HashMap;
use std::sync::LazyLock;

/// [`Vertex::edge`] of the off-road ground's vertices (ours, the ground scan, [`mark_ground`]): the road's edge distance
/// is 0 or more, so a negative one marks ground.
pub(crate) const GROUND_MARK: f32 = -1.0;

/// A stage whose ground is more than one in this many triangles on snow is a snow stage.
const SNOW_STAGE: usize = 20;

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

/// The share of the wet film a ground of surface `name` takes (ours): tarmac all, gravel and dirt half, the rest
/// (grass, rock, verges) none.
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

/// Whether a `MaterialManager` surface is snow (`SnowOnGravel_*`, `SnowOnIce_*`, `Snowwall*`): the snow scan's ground
/// (`Enhance::snow`; ours).
fn snow_by_name(name: &str) -> bool {
    name.trim_start_matches("US2_").trim_start_matches("BR2_").starts_with("Snow")
}

/// [`snow_by_name`] of each surface id in the exe's `MaterialManager` table.
static SNOW: LazyLock<[bool; 256]> = LazyLock::new(|| {
    let mut by_id = [false; 256];
    for key in KEYS.iter().filter(|k| k.class == Class::MaterialManager) {
        if let Some(snow) = by_id.get_mut(key.offset as usize) {
            *snow = snow_by_name(key.key);
        }
    }
    by_id
});

/// [`Vertex::wet`] of the snow's ground vertices that take no film: below the film's shares, so nothing of the wet road
/// takes them (ours, the snow scan).
pub(crate) const SNOW_MARK: f32 = -1.0;

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

/// The farthest from the road's edge, metres, that [`Vertex::edge`] tells apart.
const EDGE_RANGE: f32 = 8.0;
/// A position, in centimetres: the key that joins the vertices the stage splits across meshes and chunks.
type Spot = [i32; 3];
/// The two ends of a road edge.
type Segment = ([f32; 3], [f32; 3]);

fn spot(p: [f32; 3]) -> Spot {
    p.map(|c| (c * 100.0).round() as i32)
}

/// The distance from `p` to the segment `a` to `b`.
fn segment_distance(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = [0, 1, 2].map(|c| b[c] - a[c]);
    let ap = [0, 1, 2].map(|c| p[c] - a[c]);
    let len2 = ab.iter().map(|c| c * c).sum::<f32>();
    let t = if len2 > 0.0 { (ap.iter().zip(&ab).map(|(x, y)| x * y).sum::<f32>() / len2).clamp(0.0, 1.0) } else { 0.0 };
    (0..3).map(|c| (ap[c] - t * ab[c]).powi(2)).sum::<f32>().sqrt()
}

/// The ground vertices of `meshes` with their share of the wet film ([`film_by_name`], [`Vertex::wet`]): the most of
/// their triangles', each by the collision surface under its centre. A road's edge vertices lie on the verge's
/// surface as often as on the road's; by the vertex, a road two triangles wide was half wet across. Then the road's
/// edge: an edge of a film triangle that a ground triangle without film shares (by position, over all meshes), and
/// each film vertex's distance to the nearest ([`Vertex::edge`]).
pub(crate) fn mark_roads(meshes: &mut [Mesh], collision: &Ground) {
    // Each ground triangle with its texture and the share of the surface under it, and per texture how many film
    // triangles take each share: a stage may tag stretches of its tarmac as gravel (RSF Bois de Bie, 46000 of its 185000
    // road triangles), so a film triangle takes the share most of its texture's film triangles have.
    let mut triangles = Vec::new();
    let mut snowy = Vec::new();
    let mut votes: HashMap<Option<TextureRef>, HashMap<u32, u32>> = HashMap::new();
    for (m, mesh) in meshes.iter().enumerate() {
        for draw in mesh.draws.iter().filter(|draw| ground(mesh, draw)) {
            for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                let t = [0, 1, 2].map(|k| t[k] as usize);
                if t.iter().any(|&i| i >= mesh.vertices.len()) {
                    continue;
                }
                let [x, y, z] = [0, 1, 2].map(|c| t.iter().map(|&i| mesh.vertices[i].position[c]).sum::<f32>() / 3.0);
                let surface = collision.below([x, z, y]).map_or(0, |hit| hit.surface);
                let share = FILM.get(surface as usize).copied().unwrap_or(0.0);
                let texture = draw.state.textures[0];
                if share > 0.0 {
                    *votes.entry(texture).or_default().entry(share.to_bits()).or_default() += 1;
                }
                if SNOW.get(surface as usize).copied().unwrap_or(false) {
                    snowy.push((m, t));
                }
                triangles.push((m, t, texture, share));
            }
        }
    }
    let most: HashMap<_, f32> = votes
        .into_iter()
        .filter_map(|(texture, shares)| {
            let (bits, _) = shares.into_iter().max_by_key(|&(bits, count)| (count, bits))?;
            Some((texture, f32::from_bits(bits)))
        })
        .collect();
    // Per edge, the count of film triangles and of the others.
    let mut edges: HashMap<(Spot, Spot), [u32; 2]> = HashMap::new();
    for (m, t, texture, share) in triangles {
        let share = if share > 0.0 { most.get(&texture).copied().unwrap_or(share) } else { share };
        let mesh = &mut meshes[m];
        for k in 0..3 {
            let [a, b] = [t[k], t[(k + 1) % 3]].map(|i| spot(mesh.vertices[i].position));
            edges.entry((a.min(b), a.max(b))).or_default()[usize::from(share == 0.0)] += 1;
        }
        for i in t {
            mesh.vertices[i].wet = mesh.vertices[i].wet.max(share);
        }
    }
    // The snow's vertices that took no film.
    for (m, t) in snowy {
        for i in t {
            let wet = &mut meshes[m].vertices[i].wet;
            if *wet <= 0.0 {
                *wet = SNOW_MARK;
            }
        }
    }
    let mut grid: HashMap<(i32, i32), Vec<Segment>> = HashMap::new();
    for (&(a, b), &[film, other]) in &edges {
        if film == 0 || other == 0 {
            continue;
        }
        let [a, b] = [a, b].map(|s| s.map(|c| c as f32 / 100.0));
        let (x, z) = (cell_of(a, EDGE_RANGE), cell_of(b, EDGE_RANGE));
        for cx in x.0.min(z.0)..=x.0.max(z.0) {
            for cz in x.1.min(z.1)..=x.1.max(z.1) {
                grid.entry((cx, cz)).or_default().push((a, b));
            }
        }
    }
    for mesh in meshes.iter_mut() {
        for i in ground_vertices(mesh).collect::<std::collections::BTreeSet<_>>() {
            let p = mesh.vertices[i].position;
            let (x, z) = cell_of(p, EDGE_RANGE);
            let near = (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dz| (x + dx, z + dz)))
                .filter_map(|c| grid.get(&c))
                .flatten()
                .map(|&(a, b)| segment_distance(p, a, b))
                .fold(EDGE_RANGE, f32::min);
            mesh.vertices[i].edge = near;
        }
    }
}

/// How far above the collision surface, metres, a ground triangle's centre may lie and still be the ground itself
/// (not a wall, a roof or a bridge deck over it).
const GROUND_REACH: f32 = 0.6;

/// Marks the vertices of the off-road ground of `meshes` ([`Vertex::edge`] [`GROUND_MARK`]): the ground draws' triangles
/// lying on a collision surface that is neither the road's (taking the film) nor snow, and whose vertices [`mark_roads`]
/// left unmarked. Run after it (the ground scan, `Enhance::ground`). Returns whether the stage is a snow stage.
pub(crate) fn mark_ground(meshes: &mut [Mesh], collision: &Ground) -> bool {
    let (mut snow_triangles, mut triangles) = (0, 0);
    for mesh in meshes.iter_mut() {
        let mut marked = Vec::new();
        for draw in mesh.draws.iter().filter(|draw| ground(mesh, draw)) {
            for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                let t = [0, 1, 2].map(|k| t[k] as usize);
                if t.iter().any(|&i| i >= mesh.vertices.len()) {
                    continue;
                }
                let [x, y, z] = [0, 1, 2].map(|c| t.iter().map(|&i| mesh.vertices[i].position[c]).sum::<f32>() / 3.0);
                let Some(hit) = collision.below([x, z, y]) else { continue };
                let surface = hit.surface as usize;
                let snow = SNOW.get(surface).copied().unwrap_or(false);
                snow_triangles += usize::from(snow);
                triangles += 1;
                let road = FILM.get(surface).copied().unwrap_or(0.0) > 0.0 || snow;
                if !road && y - hit.z < GROUND_REACH {
                    marked.extend(t);
                }
            }
        }
        for i in marked {
            let v = &mut mesh.vertices[i];
            if v.wet == 0.0 {
                v.edge = GROUND_MARK;
            }
        }
    }
    snow_triangles * SNOW_STAGE > triangles
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

/// The indices of `mesh`'s road vertices (with repeats): the ground's marked by [`mark_roads`].
fn ground_vertices(mesh: &Mesh) -> impl Iterator<Item = usize> + '_ {
    mesh.draws
        .iter()
        .filter(|draw| ground(mesh, draw))
        .flat_map(|draw| mesh.indices[draw.indices.clone()].iter().map(|&i| i as usize))
        .filter(|&i| mesh.vertices.get(i).is_some_and(|v| v.wet > 0.0))
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
    fn snow_is_its_own_ground_apart_from_the_film() {
        assert!(["SnowOnGravel_Deep", "SnowOnIce_Shallow", "Snowwall_Bottom"].iter().all(|n| snow_by_name(n)));
        assert!(!["Gravel_Fine_Deep_Dry", "Tarmac_Fine_Clean_Wet", "Grass_Soft_Dry"].iter().any(|n| snow_by_name(n)));
        assert!(SNOW.iter().any(|&snow| snow));
        assert!(SNOW.iter().zip(FILM.iter()).all(|(&snow, &film)| !(snow && film > 0.0)));
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
