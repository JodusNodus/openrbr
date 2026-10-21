//! The transform of a body or the ground probe: a 16-byte matrix and the node whose frames carry a vector through it.

use super::walk::{Walk, Walker};

/// A transform: the matrix it points at and the node the walks start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transform {
    pub matrix: u32,
    pub node: u32,
}

/// Copies the transform's matrix to `out`, then its node walks `arg` against `out` with `RoutePairOneTwo` (`out` as its `a`,
/// 1 as its `b`). Returns `out`.
pub fn transform_one_two<W: Walk>(w: &mut W, t: Transform, out: u32, arg: u32) -> u32 {
    w.copy16(out, t.matrix);
    w.walk(Walker::RoutePairOneTwo, t.node, arg, out, 1);
    out
}

/// `transform_one_two` with the `RoutePairFiveSix` walk. Returns `out`.
pub fn transform_five_six<W: Walk>(w: &mut W, t: Transform, out: u32, arg: u32) -> u32 {
    w.copy16(out, t.matrix);
    w.walk(Walker::RoutePairFiveSix, t.node, arg, out, 1);
    out
}

/// What `fetch_y` needs: the grid copy and the grid walk.
pub trait GridFetch {
    /// The contact grid at `from` copied to `to`; nothing when `to` is 0.
    fn copy_grid(&mut self, to: u32, from: u32);
    /// The grid walk on `receiver` with `(o2, a, b)`.
    fn grid_walk(&mut self, receiver: u32, o2: u32, a: u32, b: u32);
}

/// For a transform whose matrix is a contact grid: copies the grid to `out`, then the node walks `arg` against `out` with
/// the grid walk (`out` as its `a`, 1 as its `b`). Returns `out`.
pub fn transform_grid<G: GridFetch>(g: &mut G, t: Transform, out: u32, arg: u32) -> u32 {
    g.copy_grid(out, t.matrix);
    g.grid_walk(t.node, arg, out, 1);
    out
}

/// The `n` 16-byte blocks at `src` are copied to `dst`, block by block, then `RoutePairOneTwo` walks `receiver` against `o2`
/// with `dst` as its `a` and `n` as its `b`. Returns `dst`.
pub fn copy_blocks_and_walk_one_two<W: Walk>(w: &mut W, receiver: u32, o2: u32, src: u32, dst: u32, n: u32) -> u32 {
    copy_blocks_then(w, Walker::RoutePairOneTwo, receiver, o2, src, dst, n)
}

/// `copy_blocks_and_walk_one_two`'s twin, whose walk is `RoutePairThreeFour`. Returns `dst`.
pub fn copy_blocks_and_walk_three_four<W: Walk>(w: &mut W, receiver: u32, o2: u32, src: u32, dst: u32, n: u32) -> u32 {
    copy_blocks_then(w, Walker::RoutePairThreeFour, receiver, o2, src, dst, n)
}

fn copy_blocks_then<W: Walk>(w: &mut W, walker: Walker, receiver: u32, o2: u32, src: u32, dst: u32, n: u32) -> u32 {
    for k in 0..n {
        w.copy16(dst + 16 * k, src + 16 * k);
    }
    w.walk(walker, receiver, o2, dst, n);
    dst
}
