//! The collision grid's edge tests: the segment sweep over the triangle faces, the edge-pair walks, the grid copy
//! and the edge move, generic over the store. The sweeps are `face_sweep` and `edge_pair`; the grid reads are
//! `CollisionStore`.

use super::cells;
use super::edge_pair::{self as pair, PairGrid};
use super::face_sweep::{self, Bounds, Face, FaceGrid, FaceHit};
use super::layout::grid_hosts::GridOpsHost;
use super::store::CollisionStore;
use crate::math::vec::{Vec3, Vec4};

/// The reads a sweep makes of a grid, each asked at the sweep's branch point.
struct GridReads<'a, S: CollisionStore> {
    store: &'a S,
    grid: u32,
}

impl<S: CollisionStore> FaceGrid for GridReads<'_, S> {
    fn bounds(&mut self) -> Bounds {
        self.store.grid_bounds(self.grid)
    }

    fn face_count(&mut self) -> u32 {
        self.store.grid_face_count(self.grid)
    }

    fn face_is_valid(&mut self, index: u32) -> bool {
        self.store.grid_face_is_valid(self.grid, index)
    }

    fn face(&mut self, index: u32) -> Face {
        self.store.grid_face(self.grid, index)
    }

    fn sample_indices(&mut self, index: u32) -> [u8; 3] {
        self.store.grid_face_samples(self.grid, index)
    }

    fn vertices(&mut self, indices: [u8; 3]) -> [Vec3; 3] {
        self.store.grid_face_vertices(self.grid, indices)
    }

    fn material(&mut self) -> u32 {
        self.store.grid_material(self.grid)
    }

    fn sample_dimensions(&mut self) -> Option<(u16, u16)> {
        self.store.grid_sample_dimensions(self.grid)
    }

    fn vertex_samples(&mut self, indices: [u8; 3]) -> [[u8; 2]; 3] {
        self.store.grid_vertex_samples(self.grid, indices)
    }

    fn sample_bytes(&mut self, first: i32, second: i32) -> [u8; 2] {
        self.store.grid_sample_bytes(self.grid, first, second)
    }
}

impl<S: CollisionStore> PairGrid for GridReads<'_, S> {
    fn row_count(&self) -> u32 {
        u32::from(self.store.grid_pair_count(self.grid))
    }

    fn row(&self, index: u32) -> [u8; 2] {
        self.store.grid_pair_row(self.grid, index)
    }

    fn vertex(&self, slot: u8) -> Vec3 {
        self.store.grid_vertex(self.grid, slot)
    }

    fn sample_slot(&self, slot: u8) -> u8 {
        self.store.grid_sample_slot(self.grid, slot)
    }

    fn sample_pair(&self, sample: u8) -> [u8; 2] {
        self.store.grid_sample_pair(self.grid, sample)
    }

    fn sample_dimensions(&self) -> Option<(u16, u16)> {
        self.store.grid_sample_dimensions(self.grid)
    }

    fn sample_bytes(&self, first: i32, second: i32) -> [u8; 2] {
        self.store.grid_sample_bytes(self.grid, first, second)
    }
}

/// Sweeps the segment through the grid's triangle faces and stores the first hit. True on a hit.
pub fn edge_edge<S: CollisionStore>(
    store: &mut S,
    grid: u32,
    [start_at, end_at, point_at, distance_at, normal_at, word_at]: [u32; 6],
) -> bool {
    let start = store.vec4(start_at);
    let end = store.vec3(end_at);
    let hit = face_sweep::sweep_faces(&mut GridReads { store: &*store, grid }, start, end);
    if let Some(hit) = hit {
        store_edge_edge_hit(store, grid, start_at, end_at, point_at, distance_at, normal_at, word_at, hit);
    }
    hit.is_some()
}

/// The output writes the sweep makes only after its first accepted face.
#[allow(clippy::too_many_arguments)]
pub fn store_edge_edge_hit<S: CollisionStore>(
    store: &mut S,
    grid: u32,
    start_at: u32,
    end_at: u32,
    point_at: u32,
    distance_at: u32,
    normal_at: u32,
    word_at: u32,
    hit: FaceHit,
) {
    let start = store.vec4(start_at);
    let end = store.vec3(end_at);
    let delta = [end.x - start.x, end.y - start.y, end.z - start.z, 0.0];
    store.set_f32(distance_at, hit.distance);
    store.store_vec4(normal_at, hit.normal);
    let t = store.f32(distance_at);
    for (lane, step) in delta.into_iter().enumerate() {
        let at = (lane as u32) * 4;
        let value = store.f32(start_at + at) + step * t;
        store.set_f32(point_at + at, value);
    }
    store.set_u32(word_at, hit.material);
    if let Some(samples) = hit.samples {
        store.store_grid_sample_bytes(grid, samples);
    }
}

/// Copies the source grid into `this` and moves its points through the transform `a` against `b`, rebinding them.
/// With no source the grid's rays are seeded. Returns `this`. `None` (nothing changed) for no grid with a source,
/// or a single point.
pub fn grid_ops_m6610<S: CollisionStore>(
    host: &mut impl GridOpsHost,
    store: &mut S,
    this: u32,
    src: u32,
    a: u32,
    b: u32,
    entry: u32,
) -> Option<u32> {
    if src == 0 {
        if this != 0 {
            host.ray_seeds(entry - 0x14, this);
        }
        return Some(this);
    }
    if this == 0 || store.grid_edge_count(src) == 1 {
        return None;
    }
    store.copy_grid(this, src);
    if store.grid_edge_count(this) == 0 {
        return Some(this);
    }
    let (points, other) = store.grid_blocks(this);
    let n = u32::from(store.grid_edge_count(this));
    host.copy_blocks_one_two(entry - 0x28, a, points, b, points, n);
    let n = u32::from(store.grid_edge_count(this));
    host.copy_blocks_three_four(entry - 0x28, a, other, b, other, n);
    let n = u32::from(store.grid_edge_count(this));
    let points: Vec<Vec4> = (0..n).map(|k| store.vec4(points + 16 * k)).collect();
    let (lo, hi) = cells::point_bounds(&points);
    let half = cells::half_extent(lo, hi, store.grid_half_scale(this));
    store.store_grid_bounds(this, lo, hi, lo.lerp(hi, 0.5), half);
    Some(this)
}

/// Moves each of the grid's points by the vector at `vec` on three lanes; lane 3 is kept. The vector is read again
/// for each point. Returns the number of points moved.
pub fn grid_add_edge<S: CollisionStore>(store: &mut S, this: u32, vec: u32) -> u32 {
    let mut moved = 0;
    while moved < u32::from(store.grid_edge_count(this)) {
        let at = store.grid_edge(this, moved);
        let sum = store.vec3(at) + store.vec3(vec);
        store.store_vec3(at, sum);
        moved += 1;
    }
    moved
}

/// The signed volume of the tetrahedron at `this`, `p2`, `p3`, `p4`: the gap.
pub fn vec_set_gap<S: CollisionStore>(store: &S, this: u32, p2: u32, p3: u32, p4: u32) -> f64 {
    pair::signed_volume(store.vec3(this), store.vec3(p2), store.vec3(p3), store.vec3(p4))
}

/// The row's edge normal to `out`. Returns 1.0.
pub fn edge_normal<S: CollisionStore>(store: &mut S, grid: u32, index: u32, p3: u32, p4: u32, out: u32) -> f32 {
    let normal = {
        let view = GridReads { store: &*store, grid };
        let [e484, e485] = view.row(index);
        pair::row_frame_normal(view.vertex(e485), view.vertex(e484), store.vec3(p3), store.vec3(p4))
    };
    store.store_vec4(out, normal);
    1.0
}

/// The walk's `t` (2.0 for a miss). `p8` and `p9` are written once the first gate passes, `p7` on a hit.
pub fn walk_5ff0<S: CollisionStore>(
    store: &mut S,
    grid: u32,
    index: u32,
    [p3, p4, p5, p6, p7, p8, p9]: [u32; 7],
) -> f32 {
    let walk = {
        let view = GridReads { store: &*store, grid };
        let [e484, e485] = view.row(index);
        pair::row_walk(
            view.vertex(e484),
            view.vertex(e485),
            store.vec4(p3),
            store.vec4(p4),
            store.vec4(p5),
            store.vec4(p6),
        )
    };
    if let Some([a, b]) = walk.lerps {
        store.store_vec4(p8, a);
        store.store_vec4(p9, b);
    }
    if let Some(point) = walk.point {
        store.store_vec4(p7, point);
    }
    walk.t
}

/// The edge-pair test of the two edges through the grid's rows. `t` is always written (1.01 when no row beat it);
/// the hit, normal and sample bytes only on a hit. Returns 1 on a hit, else 0.
pub fn edge_pair<S: CollisionStore>(store: &mut S, grid: u32, args: [u32; 10]) -> u32 {
    let [p2, p3, p4, p5, hit_at, t_at, normal_at, ..] = args;
    let result = {
        let view = GridReads { store: &*store, grid };
        let [a0, a1, b0, b1] = [p2, p3, p4, p5].map(|at| store.vec4(at));
        pair::edge_pair(&view, a0, a1, b0, b1)
    };
    store.set_f32(t_at, result.t);
    let Some(hit) = result.hit else {
        return 0;
    };
    store.store_vec4(hit_at, hit.point);
    store.store_vec4(normal_at, hit.normal);
    if let Some(bytes) = hit.samples {
        store.store_grid_sample_bytes(grid, bytes);
    }
    1
}
