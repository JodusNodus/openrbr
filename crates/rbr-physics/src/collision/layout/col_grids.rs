//! `CColGrid` views: the grid sync, samples, the grid record load and store, the collision ids and the mesh
//! factory.

use crate::mem::Mem;

use super::cbody::IDENTITY_WORKER_SLOT;
use super::grid_hosts::GRID_TABLE_BANK;
use super::vec_ops::{vec16_max, vec16_min};
use crate::collision::cells::{self, SampleCorners};
use crate::collision::mesh_grid::{self as col_grid, MeshDesc, MeshGrid as ColGridData};
use crate::math::vec::Vec4;
pub(super) use crate::physics_core::layout::PHYSICS_GLOBAL;

/// Calls outside CColGrid::SyncGrid (0x4f7090). `copy_transform` is `E37E43::M_B460` (0x51b460): thiscall with `a` in
/// ecx (SyncGrid's first argument; its walker, 0x51b320, compares `a` with the worker and reads both at +8), four stack
/// words, callee pops. It copies `count` vectors from `edges` to `out` and transforms them through `worker`.
/// `copy_vec4` is `E01Vec4::CopyFrom` (0x43c5b0, thiscall: the destination in ecx, the source on the stack). Both get
/// the memory SyncGrid works on. Checked by rust_step, and on the stage's placements (`stage::layout`'s walk).
pub trait GridSyncHost {
    fn copy_transform(&mut self, mem: &mut Mem, a: u32, edges: u32, worker: u32, out: u32, count: u32);
    fn copy_vec4(&mut self, mem: &mut Mem, dst: u32, src: u32);
}

/// CColGrid::SyncGrid (0x4f7090): when the grid has `count` edge vectors (at +0x40; the count byte at +0x68d), `b` and
/// `c` take the lanewise min and max of the host-transformed vectors. `a` is the transform's ecx (SyncGrid's first
/// argument). `scratch` holds the `count` vectors (16 bytes each); `copy_vec4` seeds `b` and `c` from the first.
/// Returns 1 when the grid has edges, else 0.
pub fn sync_grid(host: &mut impl GridSyncHost, mem: &mut Mem, grid: u32, a: u32, b: u32, c: u32, scratch: u32) -> u32 {
    let count = u32::from(mem.u8(grid + 0x68d));
    if count == 0 {
        return 0;
    }
    let worker = mem.u32(IDENTITY_WORKER_SLOT);
    host.copy_transform(mem, a, grid + 0x40, worker, scratch, count);
    host.copy_vec4(mem, b, scratch);
    host.copy_vec4(mem, c, scratch);
    for index in 1..count {
        let vector = scratch + 16 * index;
        vec16_min(mem, b, vector);
        vec16_max(mem, c, vector);
    }
    1
}

/// E36SelectTable (0x4f5ca0): select a grid using the object's type bits and table index.
pub fn select_table(mem: &Mem, object: u32) -> u32 {
    let mode = mem.u32(object + 0x44) & 0x30;
    if mode == 0x20 {
        return mem.u32(object + 0x34);
    }

    let index = match mode {
        0 => mem.u32(object + 0x34),
        0x10 | 0x30 => {
            let pointer = mem.u32(object + 0x34);
            mem.u32(pointer)
        }
        _ => return 0,
    };
    GRID_TABLE_BANK.wrapping_add(index.wrapping_mul(ColGrid::SIZE as u32))
}

/// CColGrid::SampleA (0x4e0840). Inputs are the edge-pair index and the interpolation parameter; the output tuple is
/// `(o1, o2)`, matching the original pointer write order.
pub fn sample_a(mem: &Mem, grid: u32, index: usize, t: f32) -> (u32, u32) {
    let grid = ColGrid::new(grid);
    let data = grid.data(mem);
    if data == 0 {
        return (0, 0);
    }

    let pair = grid.pair(mem, index);
    let pb0 = data.wrapping_add(u32::from(pair[4]) * 2);
    let pb1 = data.wrapping_add(u32::from(pair[5]) * 2);
    let corners = SampleCorners {
        x0: f32::from(mem.u8(pb0)),
        y0: f32::from(mem.u8(pb0 + 1)),
        x1: f32::from(mem.u8(pb1)),
        y1: f32::from(mem.u8(pb1 + 1)),
    };
    let (fx, fy) = cells::blend_sample(corners, t);

    let width = grid.width(mem);
    let table_a = grid.sample_table_a(mem);
    let o2 = u32::from(mem.u8(table_a.wrapping_add(cells::sample_index(width, fx, fy) as u32)));
    let height = grid.height(mem);
    let table_b = grid.sample_table_b(mem);
    let o1 = u32::from(mem.u8(table_b.wrapping_add(cells::sample_index(height, fx, fy) as u32)));
    (o1, o2)
}

/// CColGrid::SampleA and SampleB (0x4e0840, 0x4e0950) store their pair: `o2` first, then `o1`.
pub fn store_samples(mem: &mut Mem, out_1: u32, out_2: u32, (o1, o2): (u32, u32)) {
    mem.set_u32(out_2, o2);
    mem.set_u32(out_1, o1);
}

/// CColGrid::SampleB (0x4e0950): as SampleA, but at the point the grid's byte `+0x690 + index` names, unblended
/// (`(x, y) / 256`). `(o1, o2)` as SampleA.
pub fn sample_b(mem: &Mem, grid: u32, index: usize) -> (u32, u32) {
    let grid = ColGrid::new(grid);
    let data = grid.data(mem);
    if data == 0 {
        return (0, 0);
    }

    let point = data.wrapping_add(u32::from(grid.sample_index(mem, index)) * 2);
    let scale = f32::from_bits(0x3b80_0000); // 1/256
    let fx = f32::from(mem.u8(point)) * scale;
    let fy = f32::from(mem.u8(point + 1)) * scale;

    let width = grid.width(mem);
    let table_a = grid.sample_table_a(mem);
    let o2 = u32::from(mem.u8(table_a.wrapping_add(cells::sample_index(width, fx, fy) as u32)));
    let height = grid.height(mem);
    let table_b = grid.sample_table_b(mem);
    let o1 = u32::from(mem.u8(table_b.wrapping_add(cells::sample_index(height, fx, fy) as u32)));
    (o1, o2)
}

/// The `CColGrid` bytes `ColGrid` names (`collision/README.md` (git 9c28c0b8), the `CColGrid::Flag68x` rows).
const GRID_STATUS_FLAG: u32 = 0x68a;
const GRID_ACTIVE: u32 = 0x68b;
const GRID_PAIR_COUNT: u32 = 0x68c;
const GRID_EDGE_COUNT: u32 = 0x68d;
pub(in crate::collision) const GRID_TAG: u32 = 0x68f;

/// A view of an E33 CColGrid entry (0x6b0 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColGrid {
    base: u32,
}

impl ColGrid {
    pub const SIZE: usize = 0x6b0;

    pub const fn new(base: u32) -> Self {
        Self { base }
    }

    pub fn vector(self, mem: &Mem) -> Vec4 {
        Vec4::load(mem, self.base + 0x20)
    }

    pub fn data(self, mem: &Mem) -> u32 {
        mem.u32(self.base + 0x400)
    }

    pub fn width(self, mem: &Mem) -> u16 {
        mem.u16(self.base + 0x404)
    }

    pub fn height(self, mem: &Mem) -> u16 {
        mem.u16(self.base + 0x406)
    }

    pub fn sample_table_a(self, mem: &Mem) -> u32 {
        mem.u32(self.base + 0x408)
    }

    pub fn sample_table_b(self, mem: &Mem) -> u32 {
        mem.u32(self.base + 0x40c)
    }

    pub fn pair(self, mem: &Mem, index: usize) -> [u8; 6] {
        let at = self.base + 0x484 + index as u32 * 6;
        [mem.u8(at), mem.u8(at + 1), mem.u8(at + 2), mem.u8(at + 3), mem.u8(at + 4), mem.u8(at + 5)]
    }

    pub fn sample_index(self, mem: &Mem, index: usize) -> u8 {
        mem.u8(self.base + 0x690 + index as u32)
    }

    pub fn event_flags(self, mem: &Mem) -> (u8, u8) {
        (mem.u8(self.base + 0x688), mem.u8(self.base + 0x689))
    }

    pub fn take_event_flags(self, mem: &mut Mem) -> (u8, u8) {
        let flags = self.event_flags(mem);
        mem.map(self.base + 0x688, &[0, 0]);
        flags
    }

    /// The status byte at +0x68a. Its meaning is not known yet (`collision/README.md` (git 9c28c0b8)).
    pub fn status_flag(self, mem: &Mem) -> u8 {
        mem.u8(self.base + GRID_STATUS_FLAG)
    }

    pub fn active(self, mem: &Mem) -> u8 {
        mem.u8(self.base + GRID_ACTIVE)
    }

    pub fn pair_count(self, mem: &Mem) -> u8 {
        mem.u8(self.base + GRID_PAIR_COUNT)
    }

    pub fn edge_count(self, mem: &Mem) -> u8 {
        mem.u8(self.base + GRID_EDGE_COUNT)
    }

    pub fn tag(self, mem: &Mem) -> u8 {
        mem.u8(self.base + GRID_TAG)
    }

    pub fn edge(self, index: u32) -> u32 {
        self.base.wrapping_add(index.wrapping_add(4).wrapping_mul(0x10))
    }
}

/// A collision grid's fields at `at` (`col_grid::ColGridData`, 0x6b0 bytes).
pub fn load_col_grid(mem: &Mem, at: u32) -> ColGridData {
    let vecs = |base: u32| std::array::from_fn(|i| Vec4::load(mem, at + base + 0x10 * i as u32));
    let bytes = |base: u32, n: usize| -> Vec<u8> { mem.read(at + base, n) };
    let nodes = bytes(0x414, 2 * col_grid::NODES);
    let edges = bytes(0x484, 6 * col_grid::EDGES);
    let tail = bytes(0x688, 8);
    ColGridData {
        low: Vec4::load(mem, at),
        high: Vec4::load(mem, at + 0x10),
        centre: Vec4::load(mem, at + 0x20),
        half: Vec4::load(mem, at + 0x30),
        vertices: vecs(0x40),
        normals: vecs(0x220),
        sample_data: mem.u32(at + 0x400),
        sample_width: mem.u16(at + 0x404),
        sample_height: mem.u16(at + 0x406),
        sample_a: mem.u32(at + 0x408),
        sample_b: mem.u32(at + 0x40c),
        faces: mem.u32(at + 0x410),
        nodes: std::array::from_fn(|i| [nodes[2 * i], nodes[2 * i + 1]]),
        edges: std::array::from_fn(|i| edges[6 * i..6 * i + 6].try_into().unwrap()),
        status: mem.u32(at + 0x67c),
        shape: mem.u32(at + 0x680),
        name: mem.u32(at + 0x684),
        events: [tail[0], tail[1]],
        flags: tail[2],
        node_count: tail[3],
        edge_count: tail[4],
        vertex_count: tail[5],
        sampled: tail[6],
        material: tail[7],
        sample_index: bytes(0x690, 0x20).try_into().unwrap(),
    }
}

/// Stores a collision grid's fields at `at`.
pub fn store_col_grid(mem: &mut Mem, at: u32, grid: &ColGridData) {
    for (base, v) in [(0, grid.low), (0x10, grid.high), (0x20, grid.centre), (0x30, grid.half)] {
        v.store(mem, at + base);
    }
    for (i, (v, n)) in grid.vertices.iter().zip(&grid.normals).enumerate() {
        v.store(mem, at + 0x40 + 0x10 * i as u32);
        n.store(mem, at + 0x220 + 0x10 * i as u32);
    }
    mem.set_u32(at + 0x400, grid.sample_data);
    mem.map(at + 0x404, &grid.sample_width.to_le_bytes());
    mem.map(at + 0x406, &grid.sample_height.to_le_bytes());
    mem.set_u32(at + 0x408, grid.sample_a);
    mem.set_u32(at + 0x40c, grid.sample_b);
    mem.set_u32(at + 0x410, grid.faces);
    mem.map(at + 0x414, grid.nodes.as_flattened());
    mem.map(at + 0x484, grid.edges.as_flattened());
    mem.set_u32(at + 0x67c, grid.status);
    mem.set_u32(at + 0x680, grid.shape);
    mem.set_u32(at + 0x684, grid.name);
    let tail = [
        grid.events[0],
        grid.events[1],
        grid.flags,
        grid.node_count,
        grid.edge_count,
        grid.vertex_count,
        grid.sampled,
        grid.material,
    ];
    mem.map(at + 0x688, &tail);
    mem.map(at + 0x690, &grid.sample_index);
}

/// The collision grid of id `id` in the bank.
pub fn col_grid_at(id: u32) -> u32 {
    GRID_TABLE_BANK.wrapping_add(id.wrapping_mul(ColGrid::SIZE as u32))
}

/// The ids `E10_AllocCollisionId` hands out: 1 to 9 are left to the car.
const FIRST_FREE_ID: u32 = 10;
const ID_LIMIT: u32 = 0x2000;

/// `E10_AllocCollisionId` (0x4f7fa0): the first id from 10 whose grid has no vertices (+0x68d) and no shape record
/// (+0x680). The original traps (`int3`) when all are taken.
pub fn alloc_collision_id(mem: &Mem) -> u32 {
    (FIRST_FREE_ID..ID_LIMIT)
        .find(|&id| {
            let grid = col_grid_at(id);
            mem.u8(grid + 0x68d) == 0 && mem.u32(grid + 0x680) == 0
        })
        .expect("E10_AllocCollisionId: every collision id is taken (the original traps)")
}

/// `CSigCollisionMesh` (0x34 bytes): the id, flags, name, counts, vertex and face arrays, sampling fields, shape
/// record and material.
pub fn load_mesh_desc(mem: &Mem, at: u32) -> MeshDesc {
    MeshDesc {
        flags: mem.u32(at + 4),
        name: mem.u32(at + 8),
        vertex_count: mem.u16(at + 0xc),
        face_count: mem.u16(at + 0xe),
        faces: mem.u32(at + 0x14),
        sampled: mem.u8(at + 0x18),
        sample_data: mem.u32(at + 0x1c),
        sample_width: mem.u16(at + 0x20),
        sample_height: mem.u16(at + 0x22),
        sample_a: mem.u32(at + 0x24),
        sample_b: mem.u32(at + 0x28),
        shape: mem.u32(at + 0x2c),
        material: mem.u8(at + 0x30),
    }
}

/// `E37GridOps::Build9CD0` (0x4f9cd0) on the grid at `grid` from the descriptor at `desc`: its vertices (16 bytes
/// apart, as many as the grid's byte count) and its 7-byte faces.
pub fn build_col_grid(mem: &mut Mem, grid: u32, desc: u32) {
    let d = load_mesh_desc(mem, desc);
    let vertices: Vec<_> =
        (0..u32::from(d.vertex_count as u8)).map(|i| Vec4::load(mem, mem.u32(desc + 0x10) + 0x10 * i)).collect();
    let faces: Vec<[u8; 7]> =
        (0..u32::from(d.face_count)).map(|i| mem.read(d.faces + 7 * i, 7).try_into().unwrap()).collect();
    let mut data = load_col_grid(mem, grid);
    col_grid::build(&mut data, &d, &vertices, &faces);
    store_col_grid(mem, grid, &data);
}

/// The name a nameless id-1 mesh takes (0x7395b8), and the name part that skips a mesh (0x7395ac).
const CAR_NAME: u32 = 0x0073_95b8;
const WATER_SOFT: &[u8] = b"WATER_SOFT";

/// `E37SigFactory720` (0x4fa720), the `CSigCollisionMesh` receiver, on the descriptor at `desc`: unless the
/// descriptor's flag 2 is set, the grid of its id is cleared if used and built if the mesh has vertices or a shape.
/// A nameless id 1 is named `"Looks like a car"`; a `WATER_SOFT` mesh and id 0 are skipped.
pub fn collision_mesh_factory(mem: &mut Mem, desc: u32) {
    if mem.u32(PHYSICS_GLOBAL) == 0 || mem.u8(desc + 4) & 2 != 0 {
        return;
    }
    let id = mem.u32(desc);
    if id == 1 && mem.u32(desc + 8) == 0 {
        mem.set_u32(desc + 8, CAR_NAME);
    } else {
        let name = c_string(mem, mem.u32(desc + 8));
        if name.windows(WATER_SOFT.len()).any(|w| w == WATER_SOFT) || id == 0 {
            return;
        }
    }
    let grid = col_grid_at(id);
    if mem.u8(grid + 0x68d) != 0 || mem.u32(grid + 0x680) != 0 {
        for field in [0x400, 0x408, 0x40c, 0x680] {
            mem.set_u32(grid + field, 0);
        }
        mem.set_u8(grid + 0x68b, 0);
        mem.set_u8(grid + 0x68d, 0);
    }
    if mem.u16(desc + 0xc) != 0 || mem.u32(desc + 0x2c) != 0 {
        build_col_grid(mem, grid, desc);
    }
}

fn c_string(mem: &Mem, at: u32) -> Vec<u8> {
    (at..).map(|a| mem.u8(a)).take_while(|&b| b != 0).collect()
}
