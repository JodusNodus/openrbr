//! Detector cell lookups and the edge-pair sweep of the physics core (`RBR_E45_PhysB.cpp`): `E45_M40C0` (0x5240c0),
//! which looks up the cells of a box's corners on the collision grid, and `E45_M46A0` (0x5246a0), which tests the box's
//! edge pairs against a work list. The grid's `LookupTwin` (0x4e03b0) and the edge test `EdgeEdge` (0x4f8280) are the
//! caller's (`GridHost`, `EdgeHost`).

use super::layout;
use crate::mem::Mem;
use crate::physics_core::math::Vec4;

/// The margin the cell lookup widens the box by (`4.7`, the constant at 0x73f7a8).
const CELL_MARGIN: f32 = 4.7;
/// The CDef flag that widens the cell lookup (`0x2000` of the CDef's flags, +0x160cc).
const CELL_WIDEN_FLAG: u32 = 0x2000;
/// The sweep's starting distance (`1.01`, the pool constant at 0x72fcac).
pub const EDGE_START_DIST: f32 = 1.01;

/// A 2-D point (the box's corners are the first two floats of their vectors).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xy {
    pub x: f32,
    pub y: f32,
}

/// The caller's side of the cell lookup: `CCellGrid::LookupTwin(x, y)` (thiscall on the grid, two float stack
/// arguments), which returns the cell's handle, 0 for none.
pub trait GridHost {
    fn lookup_twin(&mut self, grid: u32, x: f32, y: f32) -> u32;
}

/// The cells of a box, as `E45_M40C0` stores them (`out[0..4]`): the corner cells `(a.x, a.y)`, `(a.x, b.y)`,
/// `(b.x, a.y)` and the fourth cell, with duplicates of an earlier cell cleared to 0. Widened, the cells are the
/// midpoint's, one margin either side on each axis.
pub fn cell_lookup4(host: &mut impl GridHost, grid: u32, a: Xy, b: Xy, widened: bool) -> [u32; 4] {
    let cells = if widened {
        let mid = Xy { x: (a.x + b.x) * 0.5, y: (a.y + b.y) * 0.5 };
        let lo = Xy { x: mid.x - CELL_MARGIN, y: mid.y - CELL_MARGIN };
        let hi = Xy { x: mid.x + CELL_MARGIN, y: mid.y + CELL_MARGIN };
        [
            host.lookup_twin(grid, lo.x, lo.y),
            host.lookup_twin(grid, lo.x, hi.y),
            host.lookup_twin(grid, hi.x, lo.y),
            host.lookup_twin(grid, hi.x, hi.y),
        ]
    } else {
        [
            host.lookup_twin(grid, a.x, a.y),
            host.lookup_twin(grid, a.x, b.y),
            host.lookup_twin(grid, b.x, a.y),
            host.lookup_twin(grid, b.x, b.y),
        ]
    };
    dedup_cells(cells)
}

/// The duplicate clearing of `E45_M40C0`, in the asm's order: the fourth cell is cleared when it equals any of the
/// first three; then the third when it equals the first two; then the second when it equals the first.
fn dedup_cells([c0, mut c1, mut c2, c3]: [u32; 4]) -> [u32; 4] {
    let c3 = if c3 == c0 || c3 == c1 || c3 == c2 { 0 } else { c3 };
    if c2 == c0 || c2 == c1 {
        c2 = 0;
    }
    if c1 == c0 {
        c1 = 0;
    }
    [c0, c1, c2, c3]
}

/// `E45_M40C0` (0x5240c0, cdecl): the cells of the box between the corners at `a` and `b`, written to `out`. The grid
/// is the cell grid global, and the box is widened when the CDef's flags have `0x2000` (the drive's flags do).
pub fn box_cells(mem: &mut Mem, host: &mut impl GridHost, a: u32, b: u32, out: u32) {
    let grid = layout::cell_grid(mem);
    let widened = layout::cdef_flags(mem) & CELL_WIDEN_FLAG != 0;
    let cells = cell_lookup4(host, grid, layout::xy(mem, a), layout::xy(mem, b), widened);
    layout::set_cells(mem, out, cells);
}

/// The caller's side of the edge test: `E37GridOps::EdgeEdge` on the pair object at `pair` with the corners `a` and `b`
/// (thiscall, six stack pointers). A hit returns its point, distance, normal and hit word; a miss returns `None`.
pub trait EdgeHost {
    fn edge_edge(&mut self, pair: u32, a: u32, b: u32) -> Option<EdgeHit>;
}

/// One edge hit, as the test writes it: the hit point (`tmp`), its distance (`f48`), its normal (`f20`) and the hit
/// word (`f44`, stored twice in the result object).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeHit {
    pub point: Vec4,
    pub dist: f32,
    pub normal: Vec4,
    pub hit_word: u32,
}

/// The best hit of a sweep: the edge hit and the list id of its pair (what `E45_M46A0` returns).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeBest {
    pub hit: EdgeHit,
    pub id: u32,
}

/// The sweep of `E45_M46A0`: each pair in turn is tested, and a hit replaces the best so far when its distance is
/// below the best's (the strict compare: a NaN distance never replaces). The start distance is `EDGE_START_DIST`.
pub fn edge_sweep(host: &mut impl EdgeHost, a: u32, b: u32, pairs: &[(u32, u32)]) -> Option<EdgeBest> {
    let mut best: Option<EdgeBest> = None;
    let mut best_dist = EDGE_START_DIST;
    for &(pair, id) in pairs {
        if let Some(hit) = host.edge_edge(pair, a, b) {
            if best_dist > hit.dist {
                best_dist = hit.dist;
                best = Some(EdgeBest { hit, id });
            }
        }
    }
    best
}

/// `E45_M46A0` (0x5246a0, fastcall with ecx = self, which the port does not read; four stack arguments): sweeps the
/// edge pairs of the list at `list` between the corners `a` and `b`, writes the result object `out`, and returns the
/// best hit's id (0 when nothing hit). A null list returns 0 before anything is written.
pub fn e45_m46a0(mem: &mut Mem, host: &mut impl EdgeHost, a: u32, b: u32, list: u32, out: u32) -> u32 {
    if list == 0 {
        return 0;
    }
    let pairs = layout::edge_pairs(mem, list);
    let best = edge_sweep(host, a, b, &pairs);
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

/// The grid's default bounds when the walls give none (`-5200`, `-7200`, width `10000`, height `15000`, the constants at
/// 0x73f7a4, 0x73f7a0, 0x72f420 and 0x73f79c).
const GRID_DEFAULT_MIN: Xy = Xy { x: -5200.0, y: -7200.0 };
const GRID_DEFAULT_WIDTH: f32 = 10000.0;
const GRID_DEFAULT_HEIGHT: f32 = 15000.0;

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
    let (min, width, height) = match host.query_wall(mem, layout::wall_manager(mem)) {
        Some([min, max]) => (min, max.x - min.x, max.y - min.y),
        None => (GRID_DEFAULT_MIN, GRID_DEFAULT_WIDTH, GRID_DEFAULT_HEIGHT),
    };
    let object = host.alloc_grid(mem);
    let grid = if object == 0 { 0 } else { host.construct_grid(mem, object, min, width, height) };
    layout::set_cell_grid(mem, grid);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lookups keyed by (x, y) bits: the cell is a fixed hash of them, 0 for the cells listed as empty.
    #[derive(Default)]
    struct Grid {
        empty: Vec<(u32, u32)>,
        calls: Vec<(u32, u32)>,
    }

    impl GridHost for Grid {
        fn lookup_twin(&mut self, _grid: u32, x: f32, y: f32) -> u32 {
            let key = (x.to_bits(), y.to_bits());
            self.calls.push(key);
            if self.empty.contains(&key) {
                0
            } else {
                (key.0 ^ key.1.rotate_left(7)) | 1
            }
        }
    }

    #[test]
    fn plain_lookups_are_the_corners_then_the_far_corner() {
        let mut grid = Grid::default();
        let a = Xy { x: 1.0, y: 2.0 };
        let b = Xy { x: 3.0, y: 6.0 };
        cell_lookup4(&mut grid, 7, a, b, false);
        let want = [(1.0f32, 2.0f32), (1.0, 6.0), (3.0, 2.0), (3.0, 6.0)].map(|(x, y)| (x.to_bits(), y.to_bits()));
        assert_eq!(grid.calls, want);
    }

    #[test]
    fn widened_lookups_go_through_the_midpoint_margins() {
        let mut grid = Grid::default();
        let a = Xy { x: 1.0, y: 2.0 };
        let b = Xy { x: 3.0, y: 6.0 };
        cell_lookup4(&mut grid, 7, a, b, true);
        // mid = (2, 4), each axis one margin either side of it
        let lo_x = 2.0f32 - CELL_MARGIN;
        let lo_y = 4.0f32 - CELL_MARGIN;
        let hi_x = 2.0f32 + CELL_MARGIN;
        let hi_y = 4.0f32 + CELL_MARGIN;
        let want = [(lo_x, lo_y), (lo_x, hi_y), (hi_x, lo_y), (hi_x, hi_y)].map(|(x, y)| (x.to_bits(), y.to_bits()));
        assert_eq!(grid.calls, want);
    }

    #[test]
    fn duplicates_are_cleared_in_asm_order() {
        assert_eq!(dedup_cells([5, 5, 5, 5]), [5, 0, 0, 0]);
        assert_eq!(dedup_cells([5, 6, 5, 7]), [5, 6, 0, 7]);
        assert_eq!(dedup_cells([5, 6, 7, 6]), [5, 6, 7, 0]);
        assert_eq!(dedup_cells([5, 6, 7, 8]), [5, 6, 7, 8]);
        assert_eq!(dedup_cells([0, 0, 1, 0]), [0, 0, 1, 0]);
    }

    /// Hits for the pairs listed, with the distances given.
    struct Edges(Vec<(u32, f32)>);

    impl EdgeHost for Edges {
        fn edge_edge(&mut self, pair: u32, _a: u32, _b: u32) -> Option<EdgeHit> {
            let &(_, dist) = self.0.iter().find(|(p, _)| *p == pair)?;
            Some(EdgeHit {
                point: Vec4 { x: pair as f32, y: 0.0, z: 0.0, w: 0.0 },
                dist,
                normal: Vec4::default(),
                hit_word: pair,
            })
        }
    }

    #[test]
    fn sweep_keeps_the_nearest_hit_below_the_start() {
        let mut host = Edges(vec![(1, 0.5), (2, 0.25), (3, 0.75)]);
        let best = edge_sweep(&mut host, 0, 0, &[(1, 10), (2, 20), (3, 30)]).expect("a hit");
        assert_eq!(best.id, 20);
        assert_eq!(best.hit.dist, 0.25);
    }

    #[test]
    fn sweep_ignores_hits_at_or_above_the_start() {
        let mut host = Edges(vec![(1, EDGE_START_DIST), (2, 2.0)]);
        assert_eq!(edge_sweep(&mut host, 0, 0, &[(1, 10), (2, 20)]), None);
    }
}
