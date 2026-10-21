//! The collision grid and detector objects in the game's memory (`RBR_E45_PhysB.cpp`): `E45_M40C0` (0x5240c0), which
//! looks up the cells of a box's corners on the collision grid, `E45_M46A0` (0x5246a0), which tests the box's edge
//! pairs against a work list, the penetration detector's constructor (0x5235b0) and the grid build `E45_MD60`
//! (0x523d60). The grid's `LookupTwin` (0x4e03b0) and the edge test `EdgeEdge` (0x4f8280) are the caller's (`GridHost`,
//! `EdgeHost`).

use crate::mem::Mem;
use crate::physics_core::detect::{self, EdgeHit, Xy};
use crate::physics_core::layout;

/// The CDef flag that widens the cell lookup (`0x2000` of the CDef's flags, +0x160cc).
const CELL_WIDEN_FLAG: u32 = 0x2000;

/// The caller's side of the cell lookup: `CCellGrid::LookupTwin(x, y)` (thiscall on the grid, two float stack
/// arguments), which returns the cell's handle, 0 for none.
pub trait GridHost {
    fn lookup_twin(&mut self, grid: u32, x: f32, y: f32) -> u32;
}

/// `E45_M40C0` (0x5240c0, cdecl): the cells of the box between the corners at `a` and `b`, written to `out`. The grid
/// is the cell grid global, and the box is widened when the CDef's flags have `0x2000` (the drive's flags do).
pub fn box_cells(mem: &mut Mem, host: &mut impl GridHost, a: u32, b: u32, out: u32) {
    let grid = layout::cell_grid(mem);
    let widened = layout::cdef_flags(mem) & CELL_WIDEN_FLAG != 0;
    let cells = detect::box_cells(|x, y| host.lookup_twin(grid, x, y), layout::xy(mem, a), layout::xy(mem, b), widened);
    layout::set_cells(mem, out, cells);
}

/// The caller's side of the edge test: `E37GridOps::EdgeEdge` on the pair object at `pair` with the corners `a` and `b`
/// (thiscall, six stack pointers). A hit returns its point, distance, normal and hit word; a miss returns `None`.
pub trait EdgeHost {
    fn edge_edge(&mut self, pair: u32, a: u32, b: u32) -> Option<EdgeHit>;
}

/// `E45_M46A0` (0x5246a0, fastcall with ecx = self, which the port does not read; four stack arguments): sweeps the
/// edge pairs of the list at `list` between the corners `a` and `b`, writes the result object `out`, and returns the
/// best hit's id (0 when nothing hit). A null list returns 0 before anything is written.
pub fn e45_m46a0(mem: &mut Mem, host: &mut impl EdgeHost, a: u32, b: u32, list: u32, out: u32) -> u32 {
    if list == 0 {
        return 0;
    }
    let pairs = layout::edge_pairs(mem, list);
    let best = detect::edge_sweep(|pair| host.edge_edge(pair, a, b), &pairs);
    layout::set_edge_result(mem, out, best);
    best.map_or(0, |best| best.id)
}

/// The vtable `CL2PenetrationDetector` stores (0x73f5c0).
pub(crate) const L2_PENETRATION_VTABLE: u32 = 0x0073_f5c0;

/// The caller's side of the penetration detector's constructor: `E38RaySeeds` (0x4f6550, thiscall on the sub-object at
/// +0x30, which it clears), and `E01Vec4::CopyFrom` (0x43c5b0, thiscall: the 16 bytes at `from` go to `to`).
pub trait PenetrationHost {
    fn ray_seeds(&mut self, mem: &mut Mem, seeds: u32);
    fn copy_vec4(&mut self, mem: &mut Mem, to: u32, from: u32);
}

/// `CL2PenetrationDetector::Ctor` (0x5235b0, thiscall, five stack arguments): the owner `a` at +4, the vectors `b` and `c`
/// at +0x10 and +0x20, the float `d` at +0x6e0, the word `e` at +0x6e4 and zero at +0x6e8. Returns the object.
pub fn l2_penetration_ctor(
    mem: &mut Mem,
    host: &mut impl PenetrationHost,
    this: u32,
    owner: u32,
    b: u32,
    c: u32,
    d: f32,
    e: u32,
) -> u32 {
    layout::set_word(mem, this, L2_PENETRATION_VTABLE);
    host.ray_seeds(mem, this + 0x30);
    layout::set_word(mem, this + 0x4, owner);
    host.copy_vec4(mem, this + 0x10, b);
    host.copy_vec4(mem, this + 0x20, c);
    layout::set_f32(mem, this + 0x6e0, d);
    layout::set_word(mem, this + 0x6e4, e);
    layout::set_word(mem, this + 0x6e8, 0);
    this
}

/// The caller's side of the grid build: `CBrakeWallManager::QueryWall` (0x4e4140, thiscall; the two corners of the walls,
/// returned when it answers nonzero), `operator new(0x14)` (0x70bec2), and `CCellGrid::CCellGrid` (0x4e0550, thiscall
/// on the new object, four float arguments; returns the object).
pub trait GridBuildHost {
    fn query_wall(&mut self, mem: &mut Mem, wall: u32) -> Option<[Xy; 2]>;
    fn alloc_grid(&mut self, mem: &mut Mem) -> u32;
    fn construct_grid(&mut self, mem: &mut Mem, object: u32, min: Xy, width: f32, height: f32) -> u32;
}

/// `E45_MD60` (0x523d60, cdecl, no arguments): builds the cell grid from the walls' bounds when the grid global is clear
/// (the default bounds when QueryWall answers 0), and stores the grid. A failed allocation stores 0. A set grid returns
/// at once.
pub fn build_cell_grid(mem: &mut Mem, host: &mut impl GridBuildHost) {
    if layout::cell_grid(mem) != 0 {
        return;
    }
    let walls = host.query_wall(mem, layout::wall_manager(mem));
    let (min, width, height) = detect::grid_bounds(walls);
    let object = host.alloc_grid(mem);
    let grid = if object == 0 { 0 } else { host.construct_grid(mem, object, min, width, height) };
    layout::set_cell_grid(mem, grid);
}
