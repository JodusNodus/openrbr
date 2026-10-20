//! The game's stage visibility: which geometry blocks, object items and interactive props a view draws.
//!
//! The `.lbs` `(2,10)` chunk holds one box (centre, half extent) per geometry block, a visibility cell: block `i`,
//! object lane `i` and cell `i` go together, and the cell's box holds the block (the lane's objects may stick out)
//! (`notes/formats.md` "Stage geometry", Visibility cells). Every frame the game copies the camera's six frustum planes into the
//! cells' holder (`CHH_World` slot 0x28, HH 0x100671c0) and classifies each cell's box against them
//! (`HH_H24_VisSelect` 0x10062dd0, `FUN_100331c0`). It draws the blocks of the cells that pass (`HH_KObj_Unk2f600`
//! 0x1002f600) and tests their lanes'
//! objects one by one; an interactive prop shows while the cell its position lies in passes
//! (`CHH_KInteractiveObject::UpdateVisibility` 0x1002ab50, `CHH_World::Unk80` 0x10066a80).
//!
//! The planes include a far plane at the camera's `m_far` (`CHH_CameraFreeFly` 0x10013900 builds them from the near
//! and far distances), which is 300 m (the exe's camera, `CSigCamera+0x48`) times the renderer's far factor
//! (`CHH_World::OnCamera` 0x10066dc0; [`CELL_REACH`]). The projection itself reaches 10 km, so a cell that crosses
//! the far plane draws whole, and one wholly beyond it not at all. Ours draws farther, to [`DRAW_REACH`]. The side
//! planes are ours: the view's own frustum, which the renderer already culls each mesh's bounds against, since our
//! image is wider than the game's 4:3.
//!
//! The car's reflection cube selects its cells by distance instead (`HH_H24_VisFill5` 0x10062bc0 into lane 1, from the
//! high and medium renderers' cube update 0x10069cf0): every cell whose box lies within [`CUBE_REACH`] of the cube's
//! centre, with no plane test, on every face ([`near_cube`]).

use crate::stage::Bounds;
use glam::{Mat4, Vec3};

/// How far along the view the game draws a cell, metres: the exe's 300 m far plane times the high
/// renderer's far factor 1.05 (`CRendererHigh`, `HH_Renderer.cpp`; the low renderer's is 0.9 or 1.0). High, as the
/// rest of the renderer takes the highest quality (`stage::LOD_DISTANCE`).
pub const CELL_REACH: f32 = 300.0 * 1.05;

/// How far along the view a cell may begin and still be drawn, metres (ours): about ten times the game's
/// [`CELL_REACH`], so that the stage beyond it shows rather than the backdrop under it, as seen from a crest or from
/// above. The far cells draw their far versions (`stage::LOD_DISTANCE`); the whole of a stage in view is a few tens of
/// thousands of triangles more.
pub const DRAW_REACH: f32 = 3000.0;

/// Whether the cell `center` +- `half_extent` lies wholly beyond [`DRAW_REACH`] along the view of the perspective
/// `view_projection`, whose clip w is the view depth (`coords::perspective`).
pub fn beyond_reach(view_projection: Mat4, center: Vec3, half_extent: Vec3) -> bool {
    let depth = view_projection.row(3);
    let forward = depth.truncate();
    let length = forward.length();
    (forward.dot(center) + depth.w - forward.abs().dot(half_extent)) / length > DRAW_REACH
}

/// How near the car's reflection cube's centre a cell's box must come for the cube to draw it, metres
/// (`0x101b42e0`, the radius 0x10069cf0 passes `HH_H24_VisFill5`).
pub const CUBE_REACH: f32 = 10.0;

/// Whether the cell `center` +- `half_extent` comes within [`CUBE_REACH`] of `point`: the squared distance from the
/// point to the box against the squared reach, bounds included (`HH_H24_VisFill5` 0x10062bc0).
pub fn near_cube(point: Vec3, center: Vec3, half_extent: Vec3) -> bool {
    let outside = ((point - center).abs() - half_extent).max(Vec3::ZERO);
    outside.length_squared() <= CUBE_REACH * CUBE_REACH
}

/// The cell whose box holds `point` across (x and z, bounds included): the first in file order, as the game's search
/// (`CHH_WorldUnk3A0::Unk63270` 0x10063270, from `CInteractiveObjectManager::Init` 0x100706c0 for each prop's
/// position). `None` when no cell does; the game then reads past its cells, ours leaves the prop always drawn.
pub fn cell_at(cells: &[Bounds], point: Vec3) -> Option<&Bounds> {
    cells.iter().find(|cell| {
        let (low, high) = (cell.center - cell.half_extent, cell.center + cell.half_extent);
        (low.x..=high.x).contains(&point.x) && (low.z..=high.z).contains(&point.z)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::FreeFly;
    use crate::coords;

    fn cell(center: Vec3, half: f32) -> Bounds {
        Bounds { center, half_extent: Vec3::splat(half) }
    }

    #[test]
    fn cells_past_the_far_plane_are_beyond_reach() {
        let camera = FreeFly::looking_along(Vec3::new(5.0, 1.0, -2.0), Vec3::new(1.0, 0.0, 1.0));
        let view_projection = camera.view_projection(coords::FOV_DEGREES.to_radians(), 16.0 / 9.0);
        let along = |d: f32| Vec3::new(5.0, 1.0, -2.0) + Vec3::new(1.0, 0.0, 1.0).normalize() * d;
        assert!(!beyond_reach(view_projection, along(2990.0), Vec3::splat(1.0)));
        assert!(beyond_reach(view_projection, along(3010.0), Vec3::splat(1.0)));
        // A cell that crosses the reach is drawn whole: its nearest corner counts.
        assert!(!beyond_reach(view_projection, along(3030.0), Vec3::splat(40.0)));
        // Behind the eye and off to the side are the frustum's business.
        assert!(!beyond_reach(view_projection, along(-1000.0), Vec3::splat(1.0)));
    }

    #[test]
    fn the_cube_draws_the_cells_within_ten_metres() {
        let (center, half) = (Vec3::new(100.0, 0.0, 50.0), Vec3::new(20.0, 5.0, 30.0));
        assert!(near_cube(Vec3::new(110.0, 2.0, 60.0), center, half));
        assert!(near_cube(Vec3::new(130.0, 0.0, 50.0), center, half));
        assert!(near_cube(Vec3::new(126.0, 13.0, 50.0), center, half));
        assert!(!near_cube(Vec3::new(126.0, 13.0, 81.0), center, half));
        assert!(!near_cube(Vec3::new(130.01, 0.0, 50.0), center, half));
        assert!(!near_cube(Vec3::new(100.0, 0.0, 90.5), center, half));
    }

    #[test]
    fn a_point_finds_the_first_cell_across() {
        let cells = [cell(Vec3::new(0.0, 0.0, 0.0), 10.0), cell(Vec3::new(15.0, 0.0, 0.0), 10.0)];
        assert_eq!(cell_at(&cells, Vec3::new(8.0, 500.0, 0.0)), Some(&cells[0]));
        assert_eq!(cell_at(&cells, Vec3::new(10.0, 0.0, -10.0)), Some(&cells[0]));
        assert_eq!(cell_at(&cells, Vec3::new(20.0, 0.0, 0.0)), Some(&cells[1]));
        assert_eq!(cell_at(&cells, Vec3::new(0.0, 0.0, 30.0)), None);
    }
}
