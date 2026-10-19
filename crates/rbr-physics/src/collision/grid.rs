//! E33 CColGrid sampling and status helpers. The host-sequenced ports (`CCellGrid` ctor and lookup, `E36SelectTable`,
//! `SampleA`) are edges in `layout.rs`; the arithmetic they use is here.

use crate::fpu;
use crate::math::vec::Vec4;

use super::layout::{CellGrid, ColGrid};

/// Start of the original's 0x6b0-byte collision-grid bank.
pub const GRID_TABLE_BANK: u32 = 0x008f_7030;

/// Calls outside CCellGrid::LookupTwin (0x4e03b0) and CCellGrid::CCellGrid (0x4e0550): `operator new` for the cell
/// block (`bytes` in, the block's address out, 0 on failure). The host owns the allocator.
pub trait AllocHost {
    fn allocate(&mut self, bytes: u32) -> u32;
}

/// The calls `E37GridOps::M6610` (0x4f6610) makes, each at its callee's entry stack pointer `sp`: `E38RaySeeds`
/// (0x4f6550) on a grid built from no source, and the copies of the grid's two blocks of `n` points in place through
/// the transform `a` (`E43PairWalk` 0x51b460, whose walk is `RoutePairOneTwo`, and its twin 0x51b500,
/// `RoutePairThreeFour`; thiscall on `a` with `(src, o2, dst, n)`).
pub trait GridOpsHost {
    fn ray_seeds(&mut self, sp: u32, grid: u32);
    fn copy_blocks_one_two(&mut self, sp: u32, a: u32, src: u32, o2: u32, dst: u32, n: u32);
    fn copy_blocks_three_four(&mut self, sp: u32, a: u32, src: u32, o2: u32, dst: u32, n: u32);
}

/// The lanewise bounds of a grid's points (`E37GridOps::M6610`): `minps` and `maxps` folded from the first point.
pub fn point_bounds(points: &[Vec4]) -> (Vec4, Vec4) {
    let first = points[0];
    points[1..].iter().fold((first, first), |(lo, hi), &p| (lo.min_lanes(p), hi.max_lanes(p)))
}

/// The grid's half extent from its bounds: `(hi - lo) * 0.5` on three lanes. The asm subtracts lane by lane into the
/// old half extent and scales all four lanes, so lane 3 is the old one's `w` halved.
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

/// The cell counts that CCellGrid::CCellGrid (0x4e0550) stores.
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

/// The cell that CCellGrid::LookupTwin (0x4e03b0) picks for a point, and the offset of its 10x10 link block within
/// the cell's 100 units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwinCell {
    pub cell: i32,
    pub local: i32,
}

/// The clamped cell and link index for the point `(x, y)` (CCellGrid::LookupTwin, 0x4e03b0).
pub fn twin_cell(grid: CellGrid, x: f32, y: f32) -> TwinCell {
    let mut ix = fpu::trunc_i32(f64::from(x - grid.origin_x));
    let mut iy = fpu::trunc_i32(f64::from(y - grid.origin_y));
    if ix < 0 {
        ix = 0;
    } else if grid.columns.wrapping_mul(100) <= ix {
        ix = grid.columns.wrapping_mul(100).wrapping_sub(1);
    }
    if iy < 0 {
        iy = 0;
    } else if grid.rows.wrapping_mul(100) <= iy {
        iy = grid.rows.wrapping_mul(100).wrapping_sub(1);
    }

    TwinCell {
        cell: grid.columns.wrapping_mul(iy / 100).wrapping_add(ix / 100),
        local: (ix % 100) / 10 + ((iy % 100) / 10) * 10,
    }
}

/// CColGrid::Flag68A (0x4e0820): one when bit 1 of the status byte is clear. The byte's meaning is not known yet.
pub fn status_flag_bit1_clear(status_flag: u8) -> u32 {
    u32::from(!(status_flag >> 1) & 1)
}

/// CColGrid::ReadClear (0x4e0a20): returns `(m_unk689, m_unk688)` from the flag bytes `(688, 689)` the caller took
/// (clearing them is `ColGrid::take_event_flags`).
pub fn read_clear((flag_688, flag_689): (u8, u8)) -> (u32, u32) {
    (u32::from(flag_689), u32::from(flag_688))
}

/// CColGrid::EdgeForA (0x4e0a90): the edge named by the first byte of the pair.
pub fn edge_for_a(grid: ColGrid, pair: [u8; 6]) -> u32 {
    grid.edge(u32::from(pair[0]))
}

/// CColGrid::EdgeForB (0x4e0ab0): the edge named by the second byte of the pair.
pub fn edge_for_b(grid: ColGrid, pair: [u8; 6]) -> u32 {
    grid.edge(u32::from(pair[1]))
}

/// The four corner samples that SampleA (0x4e0840) blends: `(x0, y0)` at `t` and `(x1, y1)` at `1 - t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleCorners {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// The blended point `(fx, fy)` of SampleA, in the unit square. The scalar SSE order blends each coordinate, then scales
/// it by 1/256.
pub fn blend_sample(corners: SampleCorners, t: f32) -> (f32, f32) {
    let one_minus_t = 1.0_f32 - t;
    let scale = f32::from_bits(0x3b80_0000); // 1/256
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
