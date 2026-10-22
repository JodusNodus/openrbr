//! E33 CColGrid host calls and status helpers; the arithmetic of the cell grid and the samples is `cells`.

use super::ColGrid;

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
