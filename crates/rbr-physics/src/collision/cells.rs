//! The cell grid and the sample blend of a collision grid: the cell a point falls in, the bounds and half extent of a
//! grid's points, and the index of a blended sample in its table.

use crate::fpu;
use crate::math::vec::Vec4;

/// The lanewise bounds of a grid's points: `minps` and `maxps` folded from the first point.
pub fn point_bounds(points: &[Vec4]) -> (Vec4, Vec4) {
    let first = points[0];
    points[1..].iter().fold((first, first), |(lo, hi), &p| (lo.min_lanes(p), hi.max_lanes(p)))
}

/// The grid's half extent from its bounds: `(hi - lo) * 0.5` on three lanes. The original subtracts lane by lane into
/// the old half extent and scales all four lanes, so lane 3 is the old one's `w` halved.
pub fn half_extent(lo: Vec4, hi: Vec4, old_w: f32) -> Vec4 {
    Vec4 { x: hi.x - lo.x, y: hi.y - lo.y, z: hi.z - lo.z, w: old_w }.scale(0.5)
}

/// The cell counts of a grid: one cell per 100 units of width and height, rounded up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellCounts {
    pub columns: i32,
    pub rows: i32,
}

impl CellCounts {
    pub fn cells(self) -> i32 {
        self.rows.wrapping_mul(self.columns)
    }
}

/// The cell counts of a grid `width` by `height` units.
pub fn cell_counts(width: f32, height: f32) -> CellCounts {
    CellCounts {
        columns: fpu::trunc_i32(f64::from(width)).wrapping_add(99) / 100,
        rows: fpu::trunc_i32(f64::from(height)).wrapping_add(99) / 100,
    }
}

/// The bytes of the cell-reference table: a count word and one pointer per cell.
pub fn table_bytes(cells: i32) -> u32 {
    cells.wrapping_mul(4).wrapping_add(4) as u32
}

/// The placement of a cell grid: its origin and its cell counts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellMap {
    pub origin_x: f32,
    pub origin_y: f32,
    pub columns: i32,
    pub rows: i32,
}

/// The cell that a point falls in, and the 10x10 sub-cell within the cell's 100 units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointCell {
    pub cell: i32,
    pub sub_cell: i32,
}

/// The cell and sub-cell of the point `(x, y)`, clamped to the grid.
pub fn point_cell(map: CellMap, x: f32, y: f32) -> PointCell {
    let mut ix = fpu::trunc_i32(f64::from(x - map.origin_x));
    let mut iy = fpu::trunc_i32(f64::from(y - map.origin_y));
    if ix < 0 {
        ix = 0;
    } else if map.columns.wrapping_mul(100) <= ix {
        ix = map.columns.wrapping_mul(100).wrapping_sub(1);
    }
    if iy < 0 {
        iy = 0;
    } else if map.rows.wrapping_mul(100) <= iy {
        iy = map.rows.wrapping_mul(100).wrapping_sub(1);
    }

    PointCell {
        cell: map.columns.wrapping_mul(iy / 100).wrapping_add(ix / 100),
        sub_cell: (ix % 100) / 10 + ((iy % 100) / 10) * 10,
    }
}

/// The four corner samples that [`blend_sample`] blends: `(x0, y0)` at `t` and `(x1, y1)` at `1 - t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleCorners {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// The blended point `(fx, fy)` in the unit square. The scalar SSE order blends each coordinate, then scales it by
/// 1/256.
pub fn blend_sample(corners: SampleCorners, t: f32) -> (f32, f32) {
    let one_minus_t = 1.0_f32 - t;
    let scale = 1.0_f32 / 256.0;
    let fx = (corners.x1 * one_minus_t + corners.x0 * t) * scale;
    let fy = (corners.y0 * t + corners.y1 * one_minus_t) * scale;
    (fx, fy)
}

/// The index of the sample-table entry for `(fx, fy)` on a table `size` cells across (`y * size + x`).
pub fn sample_index(size: u16, fx: f32, fy: f32) -> i32 {
    let size_f = f32::from(size);
    let x = fpu::trunc_i32(f64::from(size_f * fx));
    let y = fpu::trunc_i32(f64::from(size_f * fy));
    y.wrapping_mul(i32::from(size)).wrapping_add(x)
}
