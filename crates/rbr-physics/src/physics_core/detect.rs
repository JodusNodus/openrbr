//! Collision-grid queries of a box: the cells under its corners and the nearest hit of an edge-pair sweep. The grid and
//! the edge test are the caller's; these functions take them as closures.

use crate::physics_core::math::Vec4;

/// The margin (m) the widened cell lookup extends the box by on each side of its midpoint.
const CELL_MARGIN: f32 = 4.7;
/// The distance a sweep's hit has to be below to count (named by use: the sweep's initial best distance).
pub const EDGE_START_DIST: f32 = 1.01;

/// A 2-D point (the box's corners, projected to the ground plane).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xy {
    pub x: f32,
    pub y: f32,
}

/// The cells of the box between the corners `a` and `b`: the cell at `(a.x, a.y)`, at `(a.x, b.y)`, at `(b.x, a.y)`
/// and at `(b.x, b.y)`, as `cell_at` answers them (0 for none), with the duplicate of an earlier cell cleared to 0.
/// Widened, the cells are those at the midpoint of the box, one margin either side on each axis.
pub fn box_cells(mut cell_at: impl FnMut(f32, f32) -> u32, a: Xy, b: Xy, widened: bool) -> [u32; 4] {
    let cells = if widened {
        let mid = Xy { x: (a.x + b.x) * 0.5, y: (a.y + b.y) * 0.5 };
        let lo = Xy { x: mid.x - CELL_MARGIN, y: mid.y - CELL_MARGIN };
        let hi = Xy { x: mid.x + CELL_MARGIN, y: mid.y + CELL_MARGIN };
        [cell_at(lo.x, lo.y), cell_at(lo.x, hi.y), cell_at(hi.x, lo.y), cell_at(hi.x, hi.y)]
    } else {
        [cell_at(a.x, a.y), cell_at(a.x, b.y), cell_at(b.x, a.y), cell_at(b.x, b.y)]
    };
    clear_duplicate_cells(cells)
}

/// The duplicate clearing, in the order the original does it: the fourth cell is cleared when it equals any of the
/// first three; then the third when it equals the first two; then the second when it equals the first.
fn clear_duplicate_cells([c0, mut c1, mut c2, c3]: [u32; 4]) -> [u32; 4] {
    let c3 = if c3 == c0 || c3 == c1 || c3 == c2 { 0 } else { c3 };
    if c2 == c0 || c2 == c1 {
        c2 = 0;
    }
    if c1 == c0 {
        c1 = 0;
    }
    [c0, c1, c2, c3]
}

/// One edge hit: the hit point, its distance, its normal and the hit word the edge test returns with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeHit {
    pub point: Vec4,
    pub dist: f32,
    pub normal: Vec4,
    pub hit_word: u32,
}

/// The best hit of a sweep: the edge hit and the id of its pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeBest {
    pub hit: EdgeHit,
    pub id: u32,
}

/// The nearest hit over `pairs` (`(pair, id)`), each tested in turn by `test`. A hit replaces the best so far when its
/// distance is below the best's, which starts at `EDGE_START_DIST` (the strict compare: a NaN distance never replaces).
pub fn edge_sweep(mut test: impl FnMut(u32) -> Option<EdgeHit>, pairs: &[(u32, u32)]) -> Option<EdgeBest> {
    let mut best: Option<EdgeBest> = None;
    let mut best_dist = EDGE_START_DIST;
    for &(pair, id) in pairs {
        if let Some(hit) = test(pair) {
            if best_dist > hit.dist {
                best_dist = hit.dist;
                best = Some(EdgeBest { hit, id });
            }
        }
    }
    best
}

/// The cell grid's bounds when the walls give none: the minimum corner, the width and the height (m).
const GRID_DEFAULT_MIN: Xy = Xy { x: -5200.0, y: -7200.0 };
const GRID_DEFAULT_WIDTH: f32 = 10000.0;
const GRID_DEFAULT_HEIGHT: f32 = 15000.0;

/// The cell grid's minimum corner, width and height for the walls' two corners (minimum, maximum), or the defaults
/// when there are none.
pub fn grid_bounds(walls: Option<[Xy; 2]>) -> (Xy, f32, f32) {
    match walls {
        Some([min, max]) => (min, max.x - min.x, max.y - min.y),
        None => (GRID_DEFAULT_MIN, GRID_DEFAULT_WIDTH, GRID_DEFAULT_HEIGHT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cells keyed by (x, y) bits: the cell is a fixed hash of them, 0 for the cells listed as empty.
    #[derive(Default)]
    struct Grid {
        empty: Vec<(u32, u32)>,
        calls: Vec<(u32, u32)>,
    }

    impl Grid {
        fn cell_at(&mut self, x: f32, y: f32) -> u32 {
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
        box_cells(|x, y| grid.cell_at(x, y), a, b, false);
        let want = [(1.0f32, 2.0f32), (1.0, 6.0), (3.0, 2.0), (3.0, 6.0)].map(|(x, y)| (x.to_bits(), y.to_bits()));
        assert_eq!(grid.calls, want);
    }

    #[test]
    fn widened_lookups_go_through_the_midpoint_margins() {
        let mut grid = Grid::default();
        let a = Xy { x: 1.0, y: 2.0 };
        let b = Xy { x: 3.0, y: 6.0 };
        box_cells(|x, y| grid.cell_at(x, y), a, b, true);
        // mid = (2, 4), each axis one margin either side of it
        let lo_x = 2.0f32 - CELL_MARGIN;
        let lo_y = 4.0f32 - CELL_MARGIN;
        let hi_x = 2.0f32 + CELL_MARGIN;
        let hi_y = 4.0f32 + CELL_MARGIN;
        let want = [(lo_x, lo_y), (lo_x, hi_y), (hi_x, lo_y), (hi_x, hi_y)].map(|(x, y)| (x.to_bits(), y.to_bits()));
        assert_eq!(grid.calls, want);
    }

    #[test]
    fn duplicates_are_cleared_in_the_originals_order() {
        assert_eq!(clear_duplicate_cells([5, 5, 5, 5]), [5, 0, 0, 0]);
        assert_eq!(clear_duplicate_cells([5, 6, 5, 7]), [5, 6, 0, 7]);
        assert_eq!(clear_duplicate_cells([5, 6, 7, 6]), [5, 6, 7, 0]);
        assert_eq!(clear_duplicate_cells([5, 6, 7, 8]), [5, 6, 7, 8]);
        assert_eq!(clear_duplicate_cells([0, 0, 1, 0]), [0, 0, 1, 0]);
    }

    /// Hits for the pairs listed, with the distances given.
    fn hits(list: &[(u32, f32)]) -> impl FnMut(u32) -> Option<EdgeHit> + '_ {
        move |pair| {
            let &(_, dist) = list.iter().find(|(p, _)| *p == pair)?;
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
        let best = edge_sweep(hits(&[(1, 0.5), (2, 0.25), (3, 0.75)]), &[(1, 10), (2, 20), (3, 30)]).expect("a hit");
        assert_eq!(best.id, 20);
        assert_eq!(best.hit.dist, 0.25);
    }

    #[test]
    fn sweep_ignores_hits_at_or_above_the_start() {
        assert_eq!(edge_sweep(hits(&[(1, EDGE_START_DIST), (2, 2.0)]), &[(1, 10), (2, 20)]), None);
    }
}
