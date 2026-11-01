//! The dust's colour from the stage's own road (ours): the drawn colour of the ground within [`ROAD`] of the driveline
//! whose collision surface is loose (gravel, dirt, rough), so every stage, RallySimFans' included, throws the dust of
//! the road it shows. Measured once at load, in place of a colour by the particle file's region.

use glam::{Vec2, Vec3, Vec3Swizzles};
use std::collections::{BTreeSet, HashMap};

use super::ground::{material, Kind};
use super::NEUTRAL_DUST;
use crate::grass::{tint, Line, Texels};
use crate::parallel::in_parallel;
use crate::particles::ground::Ground;
use crate::puddles;
use crate::stage::{Draw, Lod, Mesh, Stage, TextureRef, Vertex};

/// How far across from the driveline, metres, the ground counts as the road the wheels throw from: wider took in the
/// verges, which RSF stages often mark as loose under their grass.
const ROAD: f32 = 3.0;
/// The least share of the road's area that must be loose for its colour to count; below it the stage keeps
/// [`NEUTRAL_DUST`] (tarmac and snow stages).
const LOOSE_SHARE: f32 = 0.05;
/// The dust's brightness (luma, display) is kept within this: some stages' textures carry their light (RSF 417's and
/// 176's O variants measure a third darker than their M), and the wet ones are darker still; the hue holds.
const LUMA: [f32; 2] = [0.55, 0.75];
/// A triangle this much greener than red and blue is grass, whatever the ground under it is marked.
const GREEN: f32 = 1.03;
/// Where in a triangle its colour is taken (barycentric).
const POINTS: [[f32; 3]; 4] = [[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2], [0.2, 0.2, 0.6]];

/// The dust colour of `stage` (after its textures are loaded) on its collision `ground`: the area-weighted mean, in
/// linear colour, of the loose road's colour as drawn (texture 0, texture 1 blended in by its alpha and the vertex
/// alpha, the vertex tint), green triangles left out (grass), with its luma kept within [`LUMA`] and no more blue than
/// red (Gabiria's blue-grey verges). [`NEUTRAL_DUST`] where too little of the road is loose or there is no driveline.
pub fn of_stage(stage: &Stage, ground: &Ground) -> [f32; 3] {
    if stage.driveline.len() < 2 {
        return NEUTRAL_DUST;
    }
    let line = Line::new(&stage.driveline);
    // The road's triangles of each ground draw, over the load's threads (a large stage has millions of ground
    // triangles): the road's area, and the loose ones by their first index with their area. A mesh wholly off the road
    // is skipped by its bounds.
    let off_road =
        |m: &Mesh| m.bounds.is_some_and(|b| line.distance(b.center.xz()) > ROAD + b.half_extent.xz().length());
    let draws: Vec<(&Mesh, &Draw)> = stage
        .meshes
        .iter()
        .filter(|m| !off_road(m))
        .flat_map(|m| m.draws.iter().filter(|d| d.lod != Lod::Far && puddles::ground(m, d)).map(move |d| (m, d)))
        .collect();
    let found = in_parallel(&draws, |&(mesh, draw)| {
        let (mut road, mut loose) = (0.0, Vec::new());
        for (n, t) in mesh.indices[draw.indices.clone()].chunks_exact(3).enumerate() {
            let Some(p) = corners(mesh, t).map(|v| v.map(|v| Vec3::from(v.position))) else { continue };
            let centre = (p[0] + p[1] + p[2]) / 3.0;
            if line.distance(centre.xz()) > ROAD {
                continue;
            }
            let area = (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5;
            road += area;
            let surface = ground.below([centre.x, centre.z, centre.y]).map_or(0, |hit| hit.surface);
            if matches!(material(surface).kind, Kind::Gravel | Kind::Dirt) {
                loose.push((draw.indices.start + n * 3, area));
            }
        }
        (road, loose)
    });
    let road: f32 = found.iter().map(|(road, _)| road).sum();
    let textures = |d: &Draw| [d.state.textures[0], d.state.textures[1].filter(|_| d.state.second_texture)];
    let wanted: BTreeSet<TextureRef> = draws
        .iter()
        .zip(&found)
        .filter(|(_, (_, loose))| !loose.is_empty())
        .flat_map(|((_, d), _)| textures(d))
        .flatten()
        .collect();
    let wanted: Vec<TextureRef> = wanted.into_iter().collect();
    let made = in_parallel(&wanted, |&t| Texels::sharp(stage, t));
    let texels: HashMap<TextureRef, Texels> =
        wanted.into_iter().zip(made).filter_map(|(t, texels)| Some((t, texels?))).collect();
    let (mut loose, mut sum) = (0.0, Vec3::ZERO);
    for ((mesh, draw), (_, triangles)) in draws.iter().zip(&found) {
        let [first, second] = textures(draw).map(|t| t.and_then(|t| texels.get(&t)));
        let Some(first) = first else { continue };
        for &(start, area) in triangles {
            let Some(v) = corners(mesh, &mesh.indices[start..start + 3]) else { continue };
            let colour = colour(v, first, second);
            let shown = display(colour);
            // Grass, though the ground under it is marked loose.
            if shown.y > shown.x * GREEN && shown.y > shown.z * GREEN {
                continue;
            }
            loose += area;
            sum += colour * area;
        }
    }
    if loose <= 0.0 || loose < road * LOOSE_SHARE {
        return NEUTRAL_DUST;
    }
    shaped(display(sum / loose))
}

/// The dust of a ground of display colour `c`: no bluer than red, its luma within [`LUMA`].
fn shaped(c: Vec3) -> [f32; 3] {
    let c = Vec3::new(c.x, c.y, c.z.min(c.x));
    let luma = c.dot(Vec3::new(0.299, 0.587, 0.114)).max(1e-3);
    (c * luma.clamp(LUMA[0], LUMA[1]) / luma).min(Vec3::ONE).to_array()
}

/// The vertices of triangle `t` of `mesh`, or `None` with an index outside its vertices.
fn corners<'a>(mesh: &'a Mesh, t: &[u32]) -> Option<[&'a Vertex; 3]> {
    let v = |k: usize| mesh.vertices.get(t[k] as usize);
    Some([v(0)?, v(1)?, v(2)?])
}

/// The linear colour of a triangle with vertices `v` as drawn, the mean of [`POINTS`]: `first` at their uv0, `second`
/// at their uv1 blended in by its alpha and the vertex alpha, times the vertex tint.
fn colour(v: [&Vertex; 3], first: &Texels, second: Option<&Texels>) -> Vec3 {
    POINTS.iter().fold(Vec3::ZERO, |sum, w| {
        let at = |uv: fn(&Vertex) -> [f32; 2]| (0..3).fold(Vec2::ZERO, |at, k| at + Vec2::from(uv(v[k])) * w[k]);
        let mut c = first.at(at(|v| v.uv0)).truncate();
        if let Some(second) = second {
            let s = second.at(at(|v| v.uv1));
            let alpha = (0..3).map(|k| f32::from(v[k].color[3]) / 255.0 * w[k]).sum::<f32>();
            c = c.lerp(s.truncate(), s.w * alpha);
        }
        let hue = (0..3).fold(Vec3::ZERO, |h, k| h + tint(v[k].color) * w[k]);
        sum + linear(c * hue) / POINTS.len() as f32
    })
}

fn linear(c: Vec3) -> Vec3 {
    c.max(Vec3::ZERO).powf(2.2)
}

fn display(c: Vec3) -> Vec3 {
    c.max(Vec3::ZERO).powf(1.0 / 2.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{self, Selection};

    #[test]
    fn dust_keeps_its_hue_within_the_brightness() {
        let dark = shaped(Vec3::new(0.21, 0.21, 0.19));
        let luma = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        assert!((luma(dark) - LUMA[0]).abs() < 1e-4 && dark[0] > dark[2], "{dark:?}");
        let tan = shaped(Vec3::new(0.7, 0.6, 0.5));
        assert!(Vec3::from(tan).abs_diff_eq(Vec3::new(0.7, 0.6, 0.5), 1e-6), "{tan:?}");
        let blue = shaped(Vec3::new(0.51, 0.52, 0.57));
        assert!(blue[2] <= blue[0], "{blue:?}");
    }

    /// Retail: Australia's pale gravel, the USA's red-brown, the French tarmac neutral. Needs `RBR_DATA`.
    #[test]
    fn retail_dust_by_the_road() {
        let Some(data) = rbr_formats::data::Data::from_env() else { return };
        let maps = data.install().join("Maps");
        let dust = |name: &str| {
            let mut stage = stage::load(&maps, name, Selection::default()).unwrap();
            stage.load_textures(&maps).unwrap();
            of_stage(&stage, &crate::particles::load_ground(&maps, name, stage.selection).unwrap())
        };
        let [australia, usa, france] = ["track-31_E", "track-61_M", "track-41_O"].map(dust);
        assert!(australia[0] > 0.7 && australia[0] - australia[2] < 0.1, "{australia:?}");
        assert!(usa[0] - usa[2] > 0.2, "{usa:?}");
        assert_eq!(france, NEUTRAL_DUST);
    }
}
