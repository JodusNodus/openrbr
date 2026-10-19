//! `E33Mat34`, the transform of a body or the ground probe (src/rbr/RBR_E43_MatrixC.cpp). The object holds the
//! pointer to its 16-byte matrix (+0) and the pair walker that walks it (+4).

use super::walk::{Walk, Walker};

/// A transform object: the matrix it points at and the walker at its `+4` (both read by `fetch_x`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transform {
    pub matrix: u32,
    pub worker: u32,
}

/// `E33Mat34::FetchX` (0x51b6c0) on `t`: copies the matrix to `out`, then the worker walks `arg` against `out` with
/// `RoutePairOneTwo` (`out` as its `a`, 1 as its `b`). Returns `out` (eax).
pub fn fetch_x<W: Walk>(w: &mut W, t: Transform, out: u32, arg: u32) -> u32 {
    w.copy16(out, t.matrix);
    w.walk(Walker::RoutePairOneTwo, t.worker, arg, out, 1);
    out
}

/// `E33Mat34::ComposeB` (0x51b600), thiscall on the worker with `(matrix, arg, out)`: `fetch_x` with the
/// `RoutePairFiveSix` walk. Returns `out` (eax).
pub fn compose_b<W: Walk>(w: &mut W, t: Transform, out: u32, arg: u32) -> u32 {
    w.copy16(out, t.matrix);
    w.walk(Walker::RoutePairFiveSix, t.worker, arg, out, 1);
    out
}

/// What `fetch_y` needs: the grid copy and the grid walk.
pub trait GridFetch {
    /// `E37GridOps::Copy6770` (0x4f6770) on `to`: the grid (0x1ac words) at `from` copied to `to`, nothing when `to` is
    /// 0.
    fn copy_grid(&mut self, to: u32, from: u32);
    /// `E43PairWalk::WB410` (0x51b410), the grid walk, on `receiver` with `(o2, a, b)`.
    fn grid_walk(&mut self, receiver: u32, o2: u32, a: u32, b: u32);
}

/// `E33Mat34::FetchY` (0x51b780) on `t`, whose matrix is a contact grid: copies the grid to `out`, then the worker
/// walks `arg` against `out` with the grid walk (`out` as its `a`, 1 as its `b`). Returns `out` (eax).
pub fn fetch_y<G: GridFetch>(g: &mut G, t: Transform, out: u32, arg: u32) -> u32 {
    g.copy_grid(out, t.matrix);
    g.grid_walk(t.worker, arg, out, 1);
    out
}

/// `E43PairWalk` (0x51b460): the `n` 16-byte blocks at `src` are copied to `dst`, block by block, then `RoutePairOneTwo`
/// walks `receiver` against `o2` with `dst` as its `a` and `n` as its `b`. Returns `dst`.
pub fn copy_blocks_then_walk<W: Walk>(w: &mut W, receiver: u32, o2: u32, src: u32, dst: u32, n: u32) -> u32 {
    copy_blocks_then(w, Walker::RoutePairOneTwo, receiver, o2, src, dst, n)
}

/// `E43PairWalk` (0x51b500): `copy_blocks_then_walk`'s twin, whose walk is `RoutePairThreeFour`. Returns `dst`.
pub fn copy_blocks_then_walk_three_four<W: Walk>(w: &mut W, receiver: u32, o2: u32, src: u32, dst: u32, n: u32) -> u32 {
    copy_blocks_then(w, Walker::RoutePairThreeFour, receiver, o2, src, dst, n)
}

fn copy_blocks_then<W: Walk>(w: &mut W, walker: Walker, receiver: u32, o2: u32, src: u32, dst: u32, n: u32) -> u32 {
    for k in 0..n {
        w.copy16(dst + 16 * k, src + 16 * k);
    }
    w.walk(walker, receiver, o2, dst, n);
    dst
}
