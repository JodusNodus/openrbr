//! `CBody`'s grid calls and the grid bounds sync, generic over the store.

use super::layout::body_hosts::{self as body_host, ClearGridHost, VfCcHost};
use super::overlap;
use super::store::CollisionStore;

/// Calls outside CColGrid::SyncGrid (0x4f7090). `copy_transform` is `E37E43::M_B460` (0x51b460): thiscall with `a` in
/// ecx (SyncGrid's first argument; its walker, 0x51b320, compares `a` with the worker and reads both at +8), four stack
/// words, callee pops. It copies `count` vectors from `edges` to `out` and transforms them through `worker`.
/// `copy_vec4` is `E01Vec4::CopyFrom` (0x43c5b0, thiscall: the destination in ecx, the source on the stack). Both get
/// the memory SyncGrid works on. Checked by rust_step, and on the stage's placements (`stage::layout`'s walk).
pub trait GridSyncHost<S = crate::mem::Mem> {
    fn copy_transform(&mut self, mem: &mut S, a: u32, edges: u32, worker: u32, out: u32, count: u32);
    fn copy_vec4(&mut self, mem: &mut S, dst: u32, src: u32);
}

/// CColGrid::SyncGrid (0x4f7090): when the grid has `count` edge vectors (at +0x40; the count byte at +0x68d), `b` and
/// `c` take the lanewise min and max of the host-transformed vectors. `a` is the transform's ecx (SyncGrid's first
/// argument). `scratch` holds the `count` vectors (16 bytes each); `copy_vec4` seeds `b` and `c` from the first.
/// Returns 1 when the grid has edges, else 0.
pub fn sync_grid<S: CollisionStore>(
    host: &mut impl GridSyncHost<S>,
    store: &mut S,
    grid: u32,
    a: u32,
    b: u32,
    c: u32,
    scratch: u32,
) -> u32 {
    let count = u32::from(store.grid_edge_count(grid));
    if count == 0 {
        return 0;
    }
    let worker = store.frame_root();
    host.copy_transform(store, a, store.grid_edge(grid, 0), worker, scratch, count);
    host.copy_vec4(store, b, scratch);
    host.copy_vec4(store, c, scratch);
    for index in 1..count {
        let vector = scratch + 16 * index;
        let lower = store.vec4(b).min_lanes(store.vec4(vector));
        store.store_vec4(b, lower);
        let upper = store.vec4(c).max_lanes(store.vec4(vector));
        store.store_vec4(c, upper);
    }
    1
}

/// CBody::Vf10C (0x4e11b0): whether the body has a grid.
pub fn body_has_grid<S: CollisionStore>(store: &S, body: u32) -> bool {
    body_host::has_grid(store.body_grid(body))
}

/// CBody::Vf110 (0x4e11c0): whether the body's grid is active (its byte +0x68b, read only when there is a grid).
pub fn body_grid_active<S: CollisionStore>(store: &S, body: u32) -> bool {
    let grid = store.body_grid(body);
    body_host::grid_active((grid != 0).then(|| store.grid_active(grid)))
}

/// CBody::ClearGrid (0x4e11a0): synchronize bounds only when a grid is attached (`u32::MAX` otherwise).
pub fn clear_grid<S: CollisionStore>(
    host: &mut impl ClearGridHost,
    store: &S,
    body: u32,
    source: u32,
    lower: u32,
    upper: u32,
) -> u32 {
    let grid = store.body_grid(body);
    if grid == 0 {
        u32::MAX
    } else {
        host.sync_grid(grid, source, lower, upper)
    }
}

/// CBody::VfCC (0x4e1230): fetches both corners (host, into `scratch` and `scratch + 16`), then tests them against the
/// body's bounds. Returns the flag in eax.
pub fn aabb_pretest<S: CollisionStore>(
    host: &mut impl VfCcHost,
    store: &mut S,
    body: u32,
    first: u32,
    second: u32,
    scratch: u32,
) -> u32 {
    let worker = store.frame_root();
    host.fetch_x(first, scratch, worker);
    host.fetch_x(second, scratch + 16, worker);
    let (lower, upper) = store.body_bounds(body);
    let first = store.vec4(scratch);
    let second = store.vec4(scratch + 16);
    u32::from(overlap::corners_overlap_bounds(lower, upper, first, second))
}
