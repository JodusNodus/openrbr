//! The collision grid's edge tests: `E37GridOps::EdgeEdge` and `EdgePair` reads, `M6610`, `AddEdge` and the
//! edge-pair walks.

use crate::math::vec::Vec3;
use crate::mem::Mem;

use crate::collision::face_sweep::{Bounds as EdgeBounds, Face as EdgeFace};

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

pub use crate::collision::grid_edge::{
    edge_normal, edge_pair, grid_add_edge, grid_ops_m6610, store_edge_edge_hit, vec_set_gap, walk_5ff0,
};
