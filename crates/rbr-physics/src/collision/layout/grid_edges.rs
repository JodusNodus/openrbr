//! The collision grid's edge tests: `E37GridOps::EdgeEdge` and `EdgePair` reads, `M6610`, `AddEdge` and the
//! edge-pair walks.

use crate::math::vec::Vec3;
use crate::mem::Mem;

use super::col_grids::ColGrid;
use super::grid_hosts::GridOpsHost;
use crate::collision::cells;
use crate::collision::edge_pair::{self as pair, PairGrid};
use crate::collision::face_sweep::{Bounds as EdgeBounds, Face as EdgeFace};
use crate::math::vec::Vec4;

/// The bounds used by `E37GridOps::M7640` from a collision grid.
pub fn edge_edge_bounds(mem: &Mem, grid: u32) -> EdgeBounds {
    EdgeBounds { centre: Vec3::load(mem, grid + 0x20), half: Vec3::load(mem, grid + 0x30) }
}

/// The face count byte consumed by `E37GridOps::EdgeEdge`.
pub fn edge_edge_face_count(mem: &Mem, grid: u32) -> u32 {
    u32::from(mem.u8(grid + 0x68b))
}

/// Whether the material byte on face `index` is not the skipped `0xff` value.
pub fn edge_edge_face_is_valid(mem: &Mem, grid: u32, index: u32) -> bool {
    let row = mem.u32(grid + 0x410).wrapping_add(index.wrapping_mul(7));
    mem.u8(row + 3) != 0xff
}

/// The vertex and sample indices of a valid seven-byte face row.
pub fn edge_edge_face(mem: &Mem, grid: u32, index: u32) -> EdgeFace {
    let row = mem.u32(grid + 0x410).wrapping_add(index.wrapping_mul(7));
    EdgeFace { vertices: [mem.u8(row), mem.u8(row + 1), mem.u8(row + 2)] }
}

/// The sample indices of a face row, read only if a sampled-surface hit uses them.
pub fn edge_edge_sample_indices(mem: &Mem, grid: u32, index: u32) -> [u8; 3] {
    let row = mem.u32(grid + 0x410).wrapping_add(index.wrapping_mul(7));
    [mem.u8(row + 4), mem.u8(row + 5), mem.u8(row + 6)]
}

/// The three triangle vertices at slots `face_index + 4` in the grid's vertex table.
pub fn edge_edge_vertices(mem: &Mem, grid: u32, indices: [u8; 3]) -> [Vec3; 3] {
    std::array::from_fn(|k| {
        let at = grid + (u32::from(indices[k]) + 4) * 0x10;
        Vec3::load(mem, at)
    })
}

/// The optional sample atlas dimensions read after a face hit.
pub fn edge_edge_sample_dimensions(mem: &Mem, grid: u32) -> Option<(u16, u16)> {
    (mem.u32(grid + 0x400) != 0).then(|| (mem.u16(grid + 0x404), mem.u16(grid + 0x406)))
}

/// The two sample bytes associated with each vertex in a valid face row.
pub fn edge_edge_vertex_samples(mem: &Mem, grid: u32, indices: [u8; 3]) -> [[u8; 2]; 3] {
    let data = mem.u32(grid + 0x400);
    std::array::from_fn(|k| {
        let at = data + 2 * u32::from(indices[k]);
        [mem.u8(at), mem.u8(at + 1)]
    })
}

/// The sampled-surface bytes at the row-major indices computed by `EdgeEdge`.
pub fn edge_edge_sample_bytes(mem: &Mem, grid: u32, first: i32, second: i32) -> [u8; 2] {
    let table_a = mem.u32(grid + 0x408);
    let table_b = mem.u32(grid + 0x40c);
    [mem.u8(table_a.wrapping_add(first as u32)), mem.u8(table_b.wrapping_add(second as u32))]
}

/// The grid's hit word copied to the output.
pub fn edge_edge_hit_word(mem: &Mem, grid: u32) -> u32 {
    u32::from(mem.u8(grid + 0x68f))
}

/// `EdgeEdge`'s sampled-surface result bytes (`grid+0x688` and `grid+0x689`).
pub fn store_edge_edge_sample_bytes(mem: &mut Mem, grid: u32, bytes: [u8; 2]) {
    mem.map(grid + 0x688, &bytes);
}

/// The output writes `EdgeEdge` makes only after its first accepted face.
pub fn store_edge_edge_hit(
    mem: &mut Mem,
    grid: u32,
    start_at: u32,
    end_at: u32,
    point_at: u32,
    distance_at: u32,
    normal_at: u32,
    word_at: u32,
    hit: crate::collision::face_sweep::FaceHit,
) {
    let start = Vec4::load(mem, start_at);
    let end = Vec3::load(mem, end_at);
    let delta = [end.x - start.x, end.y - start.y, end.z - start.z, 0.0];
    mem.set_f32(distance_at, hit.distance);
    hit.normal.store(mem, normal_at);
    let t = mem.f32(distance_at);
    for (lane, step) in delta.into_iter().enumerate() {
        let at = (lane as u32) * 4;
        let value = mem.f32(start_at + at) + step * t;
        mem.set_f32(point_at + at, value);
    }
    mem.set_u32(word_at, hit.material);
    if let Some(samples) = hit.samples {
        store_edge_edge_sample_bytes(mem, grid, samples);
    }
}

/// `E37GridOps::M6610(src, a, b)` (0x4f6610) on the grid at `this`; `entry` is its entry esp. With no source the grid's
/// rays are seeded. Else the grid becomes a copy of `src` (0x6b0 bytes); when it has points, its two blocks (+0x40 and
/// +0x220, the count byte re-read for each) are moved through the transform `a` against `b`, and the bounds of the
/// first block go to +0, +0x10, their centre (`E11_VecLerp` at 0.5) to +0x20 and the half extent to +0x30. Returns
/// `this` (eax). `None` (nothing changed) for no grid with a source, or one point: the asm's loop would not end.
pub fn grid_ops_m6610(
    host: &mut impl GridOpsHost,
    mem: &mut Mem,
    this: u32,
    src: u32,
    a: u32,
    b: u32,
    entry: u32,
) -> Option<u32> {
    const BLOCK_ONE_TWO: u32 = 0x40;
    const BLOCK_THREE_FOUR: u32 = 0x220;
    if src == 0 {
        if this != 0 {
            host.ray_seeds(entry - 0x14, this);
        }
        return Some(this);
    }
    if this == 0 || ColGrid::new(src).edge_count(mem) == 1 {
        return None;
    }
    mem.copy(this, src, ColGrid::SIZE);
    let grid = ColGrid::new(this);
    if grid.edge_count(mem) == 0 {
        return Some(this);
    }
    let points = this + BLOCK_ONE_TWO;
    let n = u32::from(grid.edge_count(mem));
    host.copy_blocks_one_two(entry - 0x28, a, points, b, points, n);
    let other = this + BLOCK_THREE_FOUR;
    let n = u32::from(grid.edge_count(mem));
    host.copy_blocks_three_four(entry - 0x28, a, other, b, other, n);
    let n = u32::from(grid.edge_count(mem));
    let points: Vec<Vec4> = (0..n).map(|k| Vec4::load(mem, points + 16 * k)).collect();
    let (lo, hi) = cells::point_bounds(&points);
    lo.store(mem, this);
    hi.store(mem, this + 0x10);
    lo.lerp(hi, 0.5).store(mem, this + 0x20);
    cells::half_extent(lo, hi, mem.f32(this + 0x3c)).store(mem, this + 0x30);
    Some(this)
}

/// `E37GridOps::AddEdge(vec)` (0x4f65b0) on the grid at `this`: each of the grid's points (+0x40, 16 bytes each; the
/// count byte at +0x68d, re-read after each point) moves by the vector at `vec` on three lanes (`addss` lane by lane);
/// lane 3 is kept. The vector is read again for each point, as the asm does. Returns the number of points moved.
pub fn grid_add_edge(mem: &mut Mem, this: u32, vec: u32) -> u32 {
    let grid = ColGrid::new(this);
    let mut moved = 0;
    while moved < u32::from(grid.edge_count(mem)) {
        let at = this + 0x40 + 16 * moved;
        (Vec3::load(mem, at) + Vec3::load(mem, vec)).store(mem, at);
        moved += 1;
    }
    moved
}

/// The grid reads of EdgePair and its helpers. `grid` is the `this` pointer.
struct PairGridView<'a> {
    mem: &'a Mem,
    grid: u32,
}

impl PairGrid for PairGridView<'_> {
    fn row_count(&self) -> u32 {
        u32::from(self.mem.u8(self.grid + 0x68c))
    }

    fn row(&self, index: u32) -> [u8; 2] {
        let row = self.grid + index.wrapping_mul(6);
        [self.mem.u8(row + 0x484), self.mem.u8(row + 0x485)]
    }

    fn vertex(&self, slot: u8) -> Vec3 {
        Vec3::load(self.mem, self.grid + (u32::from(slot) + 4) * 0x10)
    }

    fn sample_slot(&self, slot: u8) -> u8 {
        self.mem.u8(self.grid + 0x690 + u32::from(slot))
    }

    fn sample_pair(&self, sample: u8) -> [u8; 2] {
        let at = self.mem.u32(self.grid + 0x400) + 2 * u32::from(sample);
        [self.mem.u8(at), self.mem.u8(at + 1)]
    }

    fn sample_dimensions(&self) -> Option<(u16, u16)> {
        edge_edge_sample_dimensions(self.mem, self.grid)
    }

    fn sample_bytes(&self, first: i32, second: i32) -> [u8; 2] {
        edge_edge_sample_bytes(self.mem, self.grid, first, second)
    }
}

/// `E37VecSet::Store5570` (0x4f5570) (name ours), thiscall on `this` with `(p2, p3, p4)`, ret 0xc: the gap, in st0.
pub fn vec_set_gap(mem: &Mem, this: u32, p2: u32, p3: u32, p4: u32) -> f64 {
    pair::signed_volume(Vec3::load(mem, this), Vec3::load(mem, p2), Vec3::load(mem, p3), Vec3::load(mem, p4))
}

/// `E37GridOps::M74A0` (0x4f74a0) (name ours), thiscall on the grid with `(idx, p3, p4, out)`, ret 0x10: the row's edge
/// normal to `out`. Returns 1.0 (st0).
pub fn edge_normal(mem: &mut Mem, grid: u32, index: u32, p3: u32, p4: u32, out: u32) -> f32 {
    let normal = {
        let view = PairGridView { mem, grid };
        let [e484, e485] = view.row(index);
        pair::row_frame_normal(view.vertex(e485), view.vertex(e484), Vec3::load(mem, p3), Vec3::load(mem, p4))
    };
    normal.store(mem, out);
    1.0
}

/// `E37GridOps::Walk5FF0` (0x4f5ff0), thiscall on the grid with `(idx, p3, p4, p5, p6, p7, p8, p9)`, ret 0x20: the
/// returned t, in st0 (2.0 for a miss). `p8` and `p9` are written once the first gate passes, `p7` on a hit.
pub fn walk_5ff0(mem: &mut Mem, grid: u32, index: u32, [p3, p4, p5, p6, p7, p8, p9]: [u32; 7]) -> f32 {
    let walk = {
        let view = PairGridView { mem, grid };
        let [e484, e485] = view.row(index);
        pair::row_walk(
            view.vertex(e484),
            view.vertex(e485),
            Vec4::load(mem, p3),
            Vec4::load(mem, p4),
            Vec4::load(mem, p5),
            Vec4::load(mem, p6),
        )
    };
    if let Some([a, b]) = walk.lerps {
        a.store(mem, p8);
        b.store(mem, p9);
    }
    if let Some(point) = walk.point {
        point.store(mem, p7);
    }
    walk.t
}

/// `E37GridOps::EdgePair` (0x4f8560), thiscall on the grid with `(p2, p3, p4, p5, hit, t, normal, scratch, grid, 0)`,
/// ret 0x28. `t` is always written (1.01 when no row beat it); the hit, normal and sample bytes only on a hit. Returns
/// 1 on a hit, else 0.
pub fn edge_pair(mem: &mut Mem, grid: u32, args: [u32; 10]) -> u32 {
    let [p2, p3, p4, p5, hit_at, t_at, normal_at, ..] = args;
    let result = {
        let view = PairGridView { mem, grid };
        let [a0, a1, b0, b1] = [p2, p3, p4, p5].map(|at| Vec4::load(mem, at));
        pair::edge_pair(&view, a0, a1, b0, b1)
    };
    mem.set_f32(t_at, result.t);
    let Some(hit) = result.hit else {
        return 0;
    };
    hit.point.store(mem, hit_at);
    hit.normal.store(mem, normal_at);
    if let Some(bytes) = hit.samples {
        store_edge_edge_sample_bytes(mem, grid, bytes);
    }
    1
}
