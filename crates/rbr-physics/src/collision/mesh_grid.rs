//! A collision grid built from a collision-mesh descriptor: the vertices, the face edges, a BSP of the faces' planes,
//! the vertex normals and the bounds. Its kernels are the edge table ([`add_edge`]), the face cross product and plane
//! ([`triangle_cross`], [`plane`]), the BSP insert and fill, and the tree test ([`tree_test`]). The instructions fix
//! the operations; the face rows and adjacency are one scratch row each.
//!
//! Left out, as they write nothing: the material lookups (whose flags the edge table ignores and whose other results
//! are dropped) and the debug walk the physics' debug flags gate.

use crate::car_body::helpers::cross_normalized;
use crate::math::vec::Vec4;

/// Vertex slots, and their normals.
pub const VERTICES: usize = 30;
/// BSP nodes: the front and back child of each, 0 for none.
pub const NODES: usize = 56;
/// Edge slots: the two vertices, the face that added it, the face on its other side, and two face bytes.
pub const EDGES: usize = 0x54;
/// The face rows the build keeps.
const ROWS: usize = 0x38;
/// The plane margin of the BSP insert.
const MARGIN: f32 = 0.1;
/// The edge filter's bound: the cosine of 10 degrees, stored as a float.
const CREASE: f32 = 0.984_807_75;
/// The descriptor flags of a sphere and of a box shape record.
const SPHERE_SHAPE: u32 = 0x10000;
const BOX_SHAPE: u32 = 0x20000;

/// A collision grid's fields.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshGrid {
    /// The vertex bounds, their centre and half extent.
    pub low: Vec4,
    pub high: Vec4,
    pub centre: Vec4,
    pub half: Vec4,
    pub vertices: [Vec4; VERTICES],
    pub normals: [Vec4; VERTICES],
    /// The sampled-surface data and its tables, all from the descriptor.
    pub sample_data: u32,
    pub sample_width: u16,
    pub sample_height: u16,
    pub sample_a: u32,
    pub sample_b: u32,
    /// The descriptor's faces: 7 bytes each, three vertex indices, the material, three bytes.
    pub faces: u32,
    pub nodes: [[u8; 2]; NODES],
    pub edges: [[u8; 6]; EDGES],
    /// Build errors: 1 a bad edge or a failed tree test, 2 a bad face pairing, 4 a face across a plane; all ones for
    /// a grid with no vertices.
    pub status: u32,
    /// The shape record, the name, the event bytes.
    pub shape: u32,
    pub name: u32,
    pub events: [u8; 2],
    /// 1 the descriptor's flag 4, 2 an edge with a face of material 0xff, 4 a sphere, 8 a box.
    pub flags: u8,
    pub node_count: u8,
    pub edge_count: u8,
    pub vertex_count: u8,
    pub sampled: u8,
    pub material: u8,
    /// Per vertex, a byte of the last face that names it.
    pub sample_index: [u8; 0x20],
}

/// The descriptor fields the build reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshDesc {
    /// 4 copies to the grid's flag 1, 0x10000 a sphere and 0x20000 a box shape record.
    pub flags: u32,
    pub name: u32,
    pub vertex_count: u16,
    pub face_count: u16,
    pub faces: u32,
    pub sampled: u8,
    pub sample_data: u32,
    pub sample_width: u16,
    pub sample_height: u16,
    pub sample_a: u32,
    pub sample_b: u32,
    pub shape: u32,
    pub material: u8,
}

/// One of the build's rows: the face, the face across each of its edges (none: all ones) and whether the BSP
/// holds it. Rows past the faces start held.
#[derive(Clone, Copy)]
struct Row {
    face: [u8; 7],
    across: [u32; 3],
    held: bool,
}

/// Builds `grid` from `desc`, its vertices (only x, y, z are read) and its faces.
pub fn build(grid: &mut MeshGrid, desc: &MeshDesc, vertices: &[Vec4], faces: &[[u8; 7]]) {
    let face_count = u32::from(desc.face_count);
    grid.faces = desc.faces;
    grid.material = desc.material;
    grid.name = desc.name;
    grid.vertex_count = desc.vertex_count as u8;
    grid.sample_width = desc.sample_width;
    grid.sample_height = desc.sample_height;
    grid.sample_a = 0;
    grid.sample_b = 0;
    grid.events = [0; 2];
    grid.node_count = 0;
    grid.edge_count = 0;
    grid.status = 0;
    grid.sample_data = 0;
    grid.sampled = desc.sampled;
    grid.shape = 0;
    grid.nodes = [[0; 2]; NODES];
    grid.flags = u8::from(desc.flags & 4 != 0);
    if desc.flags & (SPHERE_SHAPE | BOX_SHAPE) != 0 {
        grid.shape = desc.shape;
        grid.flags |= if desc.flags & BOX_SHAPE != 0 { 8 } else { 4 };
    }
    if desc.vertex_count == 0 {
        grid.status = u32::MAX;
        return;
    }
    if grid.sampled != 0 {
        grid.sample_data = desc.sample_data;
    }
    if grid.sample_width != 0 {
        grid.sample_a = desc.sample_a;
    }
    if grid.sample_height != 0 {
        grid.sample_b = desc.sample_b;
    }
    if grid.sample_b == 0 || grid.sample_a == 0 || grid.sample_data == 0 {
        (grid.sample_a, grid.sample_b, grid.sample_data) = (0, 0, 0);
    }
    for face in faces {
        for k in 0..3 {
            grid.sample_index[usize::from(face[k])] = face[4 + k];
        }
    }

    let mut max_x = 0.0f32;
    for (i, v) in vertices[..usize::from(grid.vertex_count)].iter().enumerate() {
        grid.normals[i] = Vec4::default();
        if v.x > max_x {
            max_x = v.x;
        }
        let slot = &mut grid.vertices[i];
        (slot.x, slot.y, slot.z) = (v.x, v.y, v.z);
    }
    for (i, f) in faces.iter().enumerate() {
        let i = i as u8;
        if add_edge(grid, i, f[0], f[1], f[4], f[5]) == 0
            || add_edge(grid, i, f[1], f[2], f[5], f[6]) == 0
            || add_edge(grid, i, f[2], f[0], f[6], f[4]) == 0
        {
            grid.status |= 1;
        }
    }

    assert!(faces.len() <= ROWS, "{} faces overrun the {} build rows", faces.len(), ROWS);
    let mut rows = [Row { face: [0; 7], across: [u32::MAX; 3], held: true }; ROWS];
    for (row, face) in rows.iter_mut().zip(faces) {
        (row.face, row.held) = (*face, false);
    }
    for e in 0..usize::from(grid.edge_count) {
        let edge = grid.edges[e];
        let (a, b) = (usize::from(edge[2]), usize::from(edge[3]));
        if a as u32 >= face_count || b as u32 >= face_count {
            grid.status |= 2;
            continue;
        }
        for u in 1..=3 {
            let (x, y) = (u % 3, (u + 1) % 3);
            for (row, other) in [(a, b), (b, a)] {
                let f = rows[row].face;
                if (f[x] == edge[0] && f[y] == edge[1]) || (f[y] == edge[0] && f[x] == edge[1]) {
                    let across = &mut rows[row].across[u - 1];
                    if *across > face_count {
                        *across = other as u32;
                    } else {
                        grid.status |= 2;
                    }
                }
            }
        }
    }

    if !rows[0].held {
        rows[0].held = true;
        insert_face(grid, faces, rows[0].face, 0);
        for k in 0..3 {
            let across = rows[0].across[k];
            if across != u32::MAX {
                fill_from(grid, faces, &mut rows, across as usize);
            }
        }
    }
    let probe = Vec4 { x: max_x * 2.0, ..Vec4::default() };
    if tree_test(grid, faces, probe, 0) {
        grid.status |= 1;
    }

    if grid.vertex_count != 0 {
        grid.low = grid.vertices[0];
        grid.high = grid.vertices[0];
        for v in &grid.vertices[1..usize::from(grid.vertex_count)] {
            grid.low = grid.low.min_lanes(*v);
            grid.high = grid.high.max_lanes(*v);
        }
        let (low, high) = (grid.low, grid.high);
        let (centre, half) = (&mut grid.centre, &mut grid.half);
        (centre.x, centre.y, centre.z) = (high.x + low.x, high.y + low.y, high.z + low.z);
        (half.x, half.y, half.z) = (high.x - low.x, high.y - low.y, high.z - low.z);
        for v in [centre, half] {
            *v = Vec4 { x: 0.5 * v.x, y: 0.5 * v.y, z: 0.5 * v.z, w: 0.5 * v.w };
        }
    }

    if grid.status == 0 {
        grid.edge_count = 0;
        for i in 0..grid.node_count {
            let f = faces[usize::from(i)];
            add_edge(grid, i, f[0], f[1], f[4], f[5]);
            add_edge(grid, i, f[1], f[2], f[5], f[6]);
            add_edge(grid, i, f[2], f[0], f[6], f[4]);
        }
        let mut kept = 0;
        for e in 0..usize::from(grid.edge_count) {
            let edge = grid.edges[e];
            let (a, b) = (edge[2], edge[3]);
            if a >= grid.node_count || b >= grid.node_count {
                continue;
            }
            if faces[usize::from(a)][3] == 0xff || faces[usize::from(b)][3] == 0xff {
                grid.flags |= 2;
                continue;
            }
            let (na, nb) = (plane(grid, faces, a), plane(grid, faces, b));
            let dot = (nb.z * na.z + nb.y * na.y) + nb.x * na.x;
            if CREASE >= dot {
                grid.edges[kept] = edge;
                kept += 1;
            }
        }
        grid.edge_count = kept as u8;
    }
}

/// Files edge `b`-`c` of face `a` (with the face's bytes `d`, `e`). An edge the table holds the
/// other way round takes `a` as its other face (-1); the same way round also does, and is an error (0). A new edge
/// goes in while there is room (1); its other-face byte keeps what was there.
pub fn add_edge(grid: &mut MeshGrid, a: u8, b: u8, c: u8, d: u8, e: u8) -> i32 {
    for edge in &mut grid.edges[..usize::from(grid.edge_count)] {
        if edge[0] == c && edge[1] == b {
            edge[3] = a;
            return -1;
        }
        if edge[0] == b && edge[1] == c {
            edge[3] = a;
            return 0;
        }
    }
    if usize::from(grid.edge_count) >= EDGES {
        return 0;
    }
    let edge = &mut grid.edges[usize::from(grid.edge_count)];
    (edge[0], edge[1], edge[2], edge[4], edge[5]) = (b, c, a, d, e);
    grid.edge_count += 1;
    1
}

fn vertex(grid: &MeshGrid, index: u8) -> Vec4 {
    *grid.vertices.get(usize::from(index)).unwrap_or_else(|| panic!("vertex {index} aliases the grid's other fields"))
}

/// The face's first and second edge from its middle vertex, `v0 - v1` and `v2 - v1` (x, y, z; `w` 0).
fn face_edges(grid: &MeshGrid, face: [u8; 7]) -> (Vec4, Vec4) {
    let (v0, v1, v2) = (vertex(grid, face[0]), vertex(grid, face[1]), vertex(grid, face[2]));
    let d = |a: Vec4, b: Vec4| Vec4 { x: a.x - b.x, y: a.y - b.y, z: a.z - b.z, w: 0.0 };
    (d(v0, v1), d(v2, v1))
}

/// `(v2 - v1) x (v0 - v1)` of face `index`.
pub fn triangle_cross(grid: &MeshGrid, faces: &[[u8; 7]], index: u8) -> Vec4 {
    let (t1, t2) = face_edges(grid, faces[usize::from(index)]);
    t2.cross3(t1)
}

/// Face `index`'s plane, the unit normal (the normalised same cross product) and `0 - n.v1` as `w`.
pub fn plane(grid: &MeshGrid, faces: &[[u8; 7]], index: u8) -> Vec4 {
    let face = faces[usize::from(index)];
    let (t1, t2) = face_edges(grid, face);
    let n = cross_normalized(t2, t1);
    let v1 = vertex(grid, face[1]);
    Vec4 { w: 0.0 - ((v1.z * n.z + v1.y * n.y) + v1.x * n.x), ..n }
}

/// Walks the BSP from `node` with the face's three vertices (each more than 0.1 behind a plane
/// sends it to the front child, else more than 0.1 in front to the back child, else to the front; behind and in
/// front both is error 4), naming the free child it reaches the next node. That node takes the face, adds the cross
/// product of face `node_count` (the original's index) to the face's vertex normals, and is returned.
pub fn insert_face(grid: &mut MeshGrid, faces: &[[u8; 7]], face: [u8; 7], mut node: u8) -> u8 {
    loop {
        if node >= grid.node_count {
            let n = triangle_cross(grid, faces, grid.node_count);
            for &v in &face[..3] {
                let normal = &mut grid.normals[usize::from(v)];
                (normal.x, normal.y, normal.z) = (normal.x + n.x, normal.y + n.y, normal.z + n.z);
            }
            grid.node_count += 1;
            return grid.node_count - 1;
        }
        let t = plane(grid, faces, node);
        let side = |v: u8| {
            let p = vertex(grid, v);
            ((p.y * t.y + p.z * t.z) + p.x * t.x) + t.w
        };
        let sides = [side(face[0]), side(face[1]), side(face[2])];
        let behind = sides.iter().any(|&s| -MARGIN > s);
        let ahead = sides.iter().any(|&s| s > MARGIN);
        if behind && ahead {
            grid.status |= 4;
        }
        let child = usize::from(!behind && ahead);
        let slot =
            &mut grid.nodes.get_mut(usize::from(node)).unwrap_or_else(|| panic!("node {node} aliases the edge table"))
                [child];
        if *slot == 0 {
            *slot = grid.node_count;
        }
        node = *slot;
    }
}

/// Row `index` and, depth first, the rows across its edges go into the
/// BSP, each once.
fn fill_from(grid: &mut MeshGrid, faces: &[[u8; 7]], rows: &mut [Row; ROWS], index: usize) {
    let row = rows.get_mut(index).unwrap_or_else(|| panic!("row {index} overruns the build rows"));
    if row.held {
        return;
    }
    row.held = true;
    let (face, across) = (row.face, row.across);
    insert_face(grid, faces, face, 0);
    for next in across {
        if next != u32::MAX {
            fill_from(grid, faces, rows, next as usize);
        }
    }
}

/// Walks the BSP from `node` with the point `p` (`p.w` unread): on or in front of a plane to the
/// front child, else to the back child. True when it leaves through a missing front child.
pub fn tree_test(grid: &MeshGrid, faces: &[[u8; 7]], p: Vec4, mut node: u8) -> bool {
    loop {
        let t = plane(grid, faces, node);
        let s = ((p.x * t.x + t.y * p.y) + t.z * p.z) + t.w;
        let [front, back] = grid.nodes[usize::from(node)];
        if 0.0 - s >= 0.0 {
            if front == 0 {
                return true;
            }
            node = front;
        } else {
            if back == 0 {
                return false;
            }
            node = back;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> MeshGrid {
        MeshGrid {
            low: Vec4::default(),
            high: Vec4::default(),
            centre: Vec4::default(),
            half: Vec4::default(),
            vertices: [Vec4::default(); VERTICES],
            normals: [Vec4::default(); VERTICES],
            sample_data: 0,
            sample_width: 0,
            sample_height: 0,
            sample_a: 0,
            sample_b: 0,
            faces: 0,
            nodes: [[0; 2]; NODES],
            edges: [[0; 6]; EDGES],
            status: 0,
            shape: 0,
            name: 0,
            events: [0; 2],
            flags: 0,
            node_count: 0,
            edge_count: 0,
            vertex_count: 0,
            sampled: 0,
            material: 0,
            sample_index: [0; 0x20],
        }
    }

    #[test]
    fn crease_is_cos_ten_degrees() {
        assert_eq!(CREASE, 10f64.to_radians().cos() as f32);
        assert_eq!(CREASE.to_bits(), 0x3f7c_1c5c);
    }

    #[test]
    fn add_edge_pairs_opposite_edges() {
        let mut grid = empty();
        assert_eq!(add_edge(&mut grid, 0, 1, 2, 7, 8), 1);
        assert_eq!(add_edge(&mut grid, 1, 2, 1, 0, 0), -1);
        assert_eq!(grid.edges[0], [1, 2, 0, 1, 7, 8]);
        assert_eq!(add_edge(&mut grid, 2, 1, 2, 0, 0), 0);
        assert_eq!(grid.edge_count, 1);
    }

    #[test]
    fn tetrahedron_builds_without_errors() {
        let v = |x, y, z| Vec4 { x, y, z, w: 0.0 };
        let vertices = [v(0.0, 0.0, 0.0), v(1.0, 0.0, 0.0), v(0.0, 1.0, 0.0), v(0.0, 0.0, 1.0)];
        // Outward-facing faces, each edge used once each way.
        let faces = [[0, 2, 1, 0, 0, 0, 0], [0, 1, 3, 0, 0, 0, 0], [0, 3, 2, 0, 0, 0, 0], [1, 2, 3, 0, 0, 0, 0]];
        let desc = MeshDesc {
            flags: 0,
            name: 0,
            vertex_count: 4,
            face_count: 4,
            faces: 0x1000,
            sampled: 0,
            sample_data: 0,
            sample_width: 0,
            sample_height: 0,
            sample_a: 0,
            sample_b: 0,
            shape: 0,
            material: 0,
        };
        let mut grid = empty();
        build(&mut grid, &desc, &vertices, &faces);
        assert_eq!(grid.status, 0);
        assert_eq!(grid.node_count, 4);
        assert_eq!(grid.edge_count, 6);
        assert_eq!((grid.low, grid.high), (v(0.0, 0.0, 0.0), v(1.0, 1.0, 1.0)));
        assert_eq!(grid.centre, v(0.5, 0.5, 0.5));
    }
}
