//! `CCellGrid` and its intrusive body links.

use crate::mem::Mem;

use super::grid_hosts::AllocHost;
use super::links;
use crate::collision::cells::{self, CellMap};

/// CBodyLink::Unlink (0x4e0020), preserving the original's neighbor-write order. Returns the eax the original leaves:
/// the pointer it read last, `previous` when both neighbours exist and `owner` otherwise. An unattached link (no owner)
/// leaves the caller's eax, which this does not know, so it returns 0.
pub fn unlink(mem: &mut Mem, link: u32) -> u32 {
    let current = Link::load(mem, link);
    let mut residue = 0;
    if current.owner != 0 {
        residue = if current.next != 0 && current.previous != 0 { current.previous } else { current.owner };
        if current.next != 0 {
            mem.set_u32(current.next + 4, current.previous);
            if current.previous == 0 {
                mem.set_u32(current.owner, current.next);
            } else {
                mem.set_u32(current.previous, current.next);
            }
        } else if current.previous != 0 {
            mem.set_u32(current.previous, 0);
            mem.set_u32(current.owner + 4, current.previous);
        } else {
            mem.set_u32(current.owner, 0);
            mem.set_u32(current.owner + 4, 0);
        }
    }
    mem.set_u32(link, 0);
    mem.set_u32(link + 4, 0);
    mem.set_u32(link + 8, 0);
    residue
}

/// CCellGrid::CCellGrid (0x4e0550): store the bounds and cell counts, allocate the zeroed cell-reference table (host),
/// then point the grid at its entries.
pub fn cell_grid_ctor(
    host: &mut impl AllocHost,
    mem: &mut Mem,
    grid: u32,
    origin_x: f32,
    origin_y: f32,
    width: f32,
    height: f32,
) -> u32 {
    let counts = cells::cell_counts(width, height);
    mem.set_f32(grid, origin_x);
    mem.set_f32(grid + 4, origin_y);
    mem.set_u32(grid + 0x08, counts.rows as u32);
    mem.set_u32(grid + 0x0c, counts.columns as u32);

    let cells = counts.cells();
    let allocation = host.allocate(cells::table_bytes(cells));
    if allocation == 0 {
        mem.set_u32(grid + 0x10, 0);
    } else {
        mem.set_u32(allocation, cells as u32);
        for index in 0..cells as u32 {
            mem.set_u32(allocation + 4 + index * 4, 0);
        }
        mem.set_u32(grid + 0x10, allocation + 4);
    }
    grid
}

/// A body's cell link (next, previous, cell) at `link` moved to the cell link `cell` (head, tail, count) that
/// `lookup_twin` gave: out of its old cell (count down, `unlink`), then in at the head of `cell` unless it is 0. The
/// inline sequence `CCollisionMeshWater::Vf15CW` (0x4faf00) and `CSolid::Vf44` (0x52c700) share.
pub fn move_to_cell(mem: &mut Mem, link: u32, cell: u32) {
    let owner = mem.u32(link + 8);
    if owner != 0 {
        mem.set_u32(owner + 8, mem.u32(owner + 8).wrapping_sub(1));
        unlink(mem, link);
    }
    if cell != 0 {
        let head = mem.u32(cell);
        mem.set_u32(link, head);
        mem.set_u32(link + 4, 0);
        if head != 0 {
            mem.set_u32(head + 4, link);
        } else {
            mem.set_u32(cell + 4, link);
        }
        mem.set_u32(cell, link);
        mem.set_u32(link + 8, cell);
        mem.set_u32(cell + 8, mem.u32(cell + 8).wrapping_add(1));
    }
}

/// CCellGrid::LookupTwin (0x4e03b0): the point's link, after lazily building its cell's 10x10 link block (host
/// allocation, then the link constructors).
pub fn lookup_twin(host: &mut impl AllocHost, mem: &mut Mem, grid: u32, x: f32, y: f32) -> u32 {
    let cell_grid = CellGrid::load(mem, grid);
    let map = CellMap {
        origin_x: cell_grid.origin_x,
        origin_y: cell_grid.origin_y,
        columns: cell_grid.columns,
        rows: cell_grid.rows,
    };
    let twin = cells::point_cell(map, x, y);
    let slot = cell_grid.cells.wrapping_add(twin.cell.wrapping_mul(4) as u32);
    let mut block = mem.u32(slot);
    if block == 0 {
        let allocation = host.allocate(0x644);
        mem.set_u32(allocation, 100);
        for index in 0..100 {
            links::cell_link_ctor(mem, allocation + 4 + index * 0x10);
        }
        block = allocation + 4;
        mem.set_u32(slot, block);
    }
    block.wrapping_add(twin.sub_cell.wrapping_mul(0x10) as u32)
}

/// The intrusive four-word link used by both E33 body and cell lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub next: u32,
    pub previous: u32,
    pub owner: u32,
    pub item: u32,
}

impl Link {
    pub fn load(mem: &Mem, at: u32) -> Self {
        Self { next: mem.u32(at), previous: mem.u32(at + 4), owner: mem.u32(at + 8), item: mem.u32(at + 12) }
    }

    pub fn store(self, mem: &mut Mem, at: u32) {
        mem.set_u32(at, self.next);
        mem.set_u32(at + 4, self.previous);
        mem.set_u32(at + 8, self.owner);
        mem.set_u32(at + 12, self.item);
    }
}

/// A CCellGrid header; cell storage is allocated lazily by the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellGrid {
    pub origin_x: f32,
    pub origin_y: f32,
    pub rows: i32,
    pub columns: i32,
    pub cells: u32,
}

impl CellGrid {
    pub fn load(mem: &Mem, at: u32) -> Self {
        Self {
            origin_x: mem.f32(at),
            origin_y: mem.f32(at + 4),
            rows: mem.u32(at + 8) as i32,
            columns: mem.u32(at + 12) as i32,
            cells: mem.u32(at + 16),
        }
    }

    pub fn store(self, mem: &mut Mem, at: u32) {
        mem.set_f32(at, self.origin_x);
        mem.set_f32(at + 4, self.origin_y);
        mem.set_u32(at + 8, self.rows as u32);
        mem.set_u32(at + 12, self.columns as u32);
        mem.set_u32(at + 16, self.cells);
    }
}
