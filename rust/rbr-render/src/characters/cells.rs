//! The stage's visibility cells, as the characters use them (`notes/stage-characters.md`, sections 2 and 3).
//!
//! - The cells are the `.lbs` `(2, 10)` boxes (`HH21_Block2a`, HH `0x10056d90`): a centre and a half extent each,
//!   filed in order into the world's `+0x3a0` holder.
//! - A character's cell is the first whose box holds its spawn point in `x` and `z` (`IHH_World::Unk7C`, the holder's
//!   `0x10063270`), looked up once when it loads.
//! - Every frame `HH_H24_VisSelect` (HH `0x10062dd0`) tests each cell's box against the main camera's six clip planes
//!   and keeps a flag per cell (`+0xc8`); `E02AnimUpdate` shows a character, and runs its scripts, only while its
//!   cell's flag is set (`IHH_World::Unk80`).
//!
//! Ours: the clip planes are our camera's (its far plane is [`crate::coords::FAR`]); a spawn point outside every cell
//! (the game reads past the holder's end then) takes no cell and is tested by itself.

use glam::{Mat4, Vec3, Vec4};
use rbr_formats::lbs;

/// One `(2, 10)` box, stage axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub centre: Vec3,
    pub half: Vec3,
}

/// The cells of a parsed `.lbs`, in file order.
pub fn cells(file: &lbs::Lbs<'_>) -> Vec<Cell> {
    file.chunks
        .iter()
        .filter_map(|chunk| match chunk {
            lbs::Chunk::Visibility(v) => Some(&v.entries),
            _ => None,
        })
        .flatten()
        .map(|e| Cell { centre: Vec3::from(e.a), half: Vec3::from(e.b) })
        .collect()
}

/// `Unk63270`: the first cell whose box holds `point` in `x` and `z`.
pub fn cell_of(cells: &[Cell], point: Vec3) -> Option<usize> {
    cells.iter().position(|c| {
        c.centre.x - c.half.x <= point.x
            && point.x <= c.half.x + c.centre.x
            && c.centre.z - c.half.z <= point.z
            && point.z <= c.half.z + c.centre.z
    })
}

/// The six clip planes of `view_projection` (D3D depth `0..1`), each `(n, d)` with the inside at `n · p + d >= 0`.
pub fn planes(view_projection: Mat4) -> [Vec4; 6] {
    let row = |i: usize| view_projection.row(i);
    let (x, y, z, w) = (row(0), row(1), row(2), row(3));
    [w + x, w - x, w + y, w - y, z, w - z]
}

/// Whether the box at `centre` with half extent `half` is not wholly outside one of `planes` (`VisSelect` keeps a
/// cell unless the box test says it is outside).
pub fn in_view(planes: &[Vec4; 6], centre: Vec3, half: Vec3) -> bool {
    planes.iter().all(|p| {
        let n = p.truncate();
        n.dot(centre) + n.abs().dot(half) + p.w >= 0.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords;

    #[test]
    fn a_point_takes_the_first_cell_holding_it_in_x_and_z() {
        let cells = [
            Cell { centre: Vec3::new(0.0, 0.0, 0.0), half: Vec3::new(10.0, 1.0, 10.0) },
            Cell { centre: Vec3::new(15.0, 0.0, 0.0), half: Vec3::new(10.0, 1.0, 10.0) },
        ];
        // The height does not count.
        assert_eq!(cell_of(&cells, Vec3::new(5.0, 500.0, 0.0)), Some(0));
        assert_eq!(cell_of(&cells, Vec3::new(20.0, 0.0, 0.0)), Some(1));
        assert_eq!(cell_of(&cells, Vec3::new(10.0, 0.0, 10.0)), Some(0));
        assert_eq!(cell_of(&cells, Vec3::new(0.0, 0.0, 11.0)), None);
    }

    #[test]
    fn a_cell_is_shown_unless_wholly_outside_the_view() {
        let view = coords::view_matrix(Vec3::ZERO, Vec3::Z);
        let vp = coords::projection(60f32.to_radians(), 1.0) * view;
        let cells = [
            Cell { centre: Vec3::new(0.0, 0.0, 50.0), half: Vec3::splat(1.0) },
            // Behind the eye.
            Cell { centre: Vec3::new(0.0, 0.0, -50.0), half: Vec3::splat(1.0) },
            // Far off to the side, but large enough to reach into the view.
            Cell { centre: Vec3::new(200.0, 0.0, 50.0), half: Vec3::new(190.0, 1.0, 1.0) },
            Cell { centre: Vec3::new(200.0, 0.0, 50.0), half: Vec3::new(10.0, 1.0, 1.0) },
            // Past the far plane.
            Cell { centre: Vec3::new(0.0, 0.0, coords::FAR + 100.0), half: Vec3::splat(1.0) },
        ];
        let planes = planes(vp);
        let shown: Vec<bool> = cells.iter().map(|c| in_view(&planes, c.centre, c.half)).collect();
        assert_eq!(shown, [true, false, true, false, false]);
    }
}
