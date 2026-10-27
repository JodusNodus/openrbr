//! How much of the sky each stage vertex sees, computed from the stage's geometry at load (ours, behind
//! [`Enhance::lighting`](crate::renderer::Enhance::lighting)): the stage is drawn as depth along [`DIRECTIONS`]
//! directions over the sky, tile by tile ([`TILE`] metres square, as the sun's cascades are drawn), and each vertex
//! weighs the directions it sees by how squarely it faces them (`sky_occlusion.wgsl`). The result is
//! [`Vertex::occlusion`](crate::stage::Vertex::occlusion), 0 in the open and 1 under full cover; it takes the place of
//! the bake's vertex light as the stage's ambient occlusion, so it holds for stages baked under another light or not
//! baked at all.

use glam::camera::lh::{proj::directx, view};
use glam::{Mat4, Vec2, Vec3};

pub(crate) const SHADER: &str = include_str!("sky_occlusion.wgsl");
/// Directions over the sky, spread evenly over the hemisphere (equal solid angles).
pub(crate) const DIRECTIONS: usize = 32;
/// A tile's side, metres, and texels across its depth map.
pub(crate) const TILE: f32 = 1000.0;
pub(crate) const SIZE: u32 = 2048;
/// How far toward the sky a tile's map reaches past the tile for casters (ridges, trees beside it), metres.
const BACK: f32 = 600.0;
/// The tiles' maps drawn at once.
pub(crate) const BATCH: usize = 8;

/// Directions toward the sky ([`spiral`]).
pub(crate) fn directions() -> Vec<Vec3> {
    spiral(DIRECTIONS)
}

/// `count` directions on a Fibonacci spiral over the upper hemisphere, uniform in height (so in solid angle).
pub(crate) fn spiral(count: usize) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    (0..count)
        .map(|i| {
            let y = 1.0 - (i as f32 + 0.5) / count as f32;
            let r = (1.0 - y * y).sqrt();
            let a = golden * i as f32;
            Vec3::new(r * a.cos(), y, r * a.sin())
        })
        .collect()
}

/// The square tiles that boxes (centre, half extent) touch, in a grid from `origin`: each tile's column and row, and the
/// lowest and highest of the boxes over it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tiles {
    pub(crate) origin: Vec2,
    pub(crate) columns: u32,
    pub(crate) rows: u32,
    /// Each touched tile: its index (`row * columns + column`) and its height range.
    pub(crate) touched: Vec<(u32, f32, f32)>,
}

impl Tiles {
    pub(crate) fn of(boxes: impl Iterator<Item = (Vec3, Vec3)> + Clone) -> Option<Tiles> {
        let (lo, hi) = boxes
            .clone()
            .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), (c, h)| (lo.min(c - h), hi.max(c + h)));
        if lo.x > hi.x {
            return None;
        }
        let origin = Vec2::new((lo.x / TILE).floor(), (lo.z / TILE).floor()) * TILE;
        let columns = ((hi.x - origin.x) / TILE).floor() as u32 + 1;
        let rows = ((hi.z - origin.y) / TILE).floor() as u32 + 1;
        let mut heights = vec![None::<(f32, f32)>; (columns * rows) as usize];
        for (c, h) in boxes {
            let (a, b) = (c - h, c + h);
            let column = |x: f32| (((x - origin.x) / TILE).floor() as u32).min(columns - 1);
            let row = |z: f32| (((z - origin.y) / TILE).floor() as u32).min(rows - 1);
            for r in row(a.z)..=row(b.z) {
                for k in column(a.x)..=column(b.x) {
                    let slot = &mut heights[(r * columns + k) as usize];
                    *slot = Some(slot.map_or((a.y, b.y), |(low, high)| (low.min(a.y), high.max(b.y))));
                }
            }
        }
        let touched =
            heights.iter().enumerate().filter_map(|(i, h)| h.map(|(low, high)| (i as u32, low, high))).collect();
        Some(Tiles { origin, columns, rows, touched })
    }

    /// The clip matrix of tile `index`'s map along `toward` (toward the sky), with height range `low` to `high`: an
    /// orthographic view down the direction over the tile's box, reaching [`BACK`] metres past it toward the sky; and
    /// its largest texel, metres.
    pub(crate) fn matrix(&self, index: u32, low: f32, high: f32, toward: Vec3) -> (Mat4, f32) {
        let travel = -toward;
        let up = if travel.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let light = view::look_to_mat4(Vec3::ZERO, travel, up);
        let corner = Vec2::new((index % self.columns) as f32, (index / self.columns) as f32) * TILE + self.origin;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for i in 0..8 {
            let p = Vec3::new(
                corner.x + if i & 1 == 0 { 0.0 } else { TILE },
                if i & 2 == 0 { low } else { high },
                corner.y + if i & 4 == 0 { 0.0 } else { TILE },
            );
            let q = light.transform_point3(p);
            (lo, hi) = (lo.min(q), hi.max(q));
        }
        let centre = (lo + hi) * 0.5;
        let half = (hi - lo) * 0.5;
        let eye = light.inverse().transform_point3(Vec3::new(centre.x, centre.y, lo.z - BACK));
        let matrix = directx::orthographic(-half.x, half.x, -half.y, half.y, 0.0, hi.z - lo.z + BACK)
            * view::look_to_mat4(eye, travel, up);
        (matrix, 2.0 * half.x.max(half.y) / SIZE as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directions_cover_the_sky_evenly() {
        let dirs = directions();
        assert_eq!(dirs.len(), DIRECTIONS);
        assert!(dirs.iter().all(|d| (d.length() - 1.0).abs() < 1e-5 && d.y > 0.0));
        // Uniform in solid angle: the mean height of a hemisphere's directions is a half.
        let mean = dirs.iter().map(|d| d.y).sum::<f32>() / DIRECTIONS as f32;
        assert!((mean - 0.5).abs() < 1e-4);
        // Balanced round the compass.
        let sum: Vec3 = dirs.iter().copied().sum();
        assert!(Vec2::new(sum.x, sum.z).length() / (DIRECTIONS as f32) < 0.05);
    }

    #[test]
    fn tiles_are_the_ones_the_boxes_touch_with_their_heights() {
        let boxes = [
            (Vec3::new(500.0, 10.0, 500.0), Vec3::splat(100.0)),
            (Vec3::new(2600.0, 50.0, 400.0), Vec3::new(500.0, 20.0, 50.0)),
        ];
        let tiles = Tiles::of(boxes.iter().copied()).unwrap();
        assert_eq!((tiles.origin, tiles.columns, tiles.rows), (Vec2::ZERO, 4, 1));
        assert_eq!(tiles.touched, vec![(0, -90.0, 110.0), (2, 30.0, 70.0), (3, 30.0, 70.0)]);
    }

    #[test]
    fn a_tile_map_holds_its_box_and_the_casters_toward_the_sky() {
        let tiles = Tiles::of([(Vec3::new(500.0, 50.0, 500.0), Vec3::new(500.0, 50.0, 500.0))].into_iter()).unwrap();
        let toward = Vec3::new(0.3, 0.8, 0.2).normalize();
        let (matrix, texel) = tiles.matrix(0, 0.0, 100.0, toward);
        let clip = |p: Vec3| matrix.project_point3(p);
        for p in [Vec3::ZERO, Vec3::new(1000.0, 100.0, 1000.0), Vec3::new(1000.0, 0.0, 0.0)] {
            let c = clip(p);
            assert!(
                c.x.abs() <= 1.0 + 1e-4 && c.y.abs() <= 1.0 + 1e-4 && (-1e-4..=1.0 + 1e-4).contains(&c.z),
                "{p} {c}"
            );
        }
        // A caster 300 m toward the sky from the tile's middle lies nearer the sky in the map than the ground below.
        let ground = Vec3::new(500.0, 0.0, 500.0);
        assert!(clip(ground + toward * 300.0).z < clip(ground).z);
        assert!(clip(ground + toward * 300.0).z >= 0.0);
        assert!(texel > 0.4 && texel < 1.0, "{texel}");
    }
}
