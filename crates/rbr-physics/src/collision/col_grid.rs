//! `E37GridOps::Build9CD0` (0x4f9cd0): a collision grid (`CColGrid`, 0x6b0 bytes, the step's `layout::ColGrid`) built
//! from a collision-mesh descriptor (`CSigCollisionMesh`): the vertices, the face edges, a BSP of the faces' planes,
//! the vertex normals and the bounds. Its kernels are the edge table (`M7140`, 0x4f7140), the face cross product and
//! plane (`M7250`, 0x4f7250, and `M7320`, 0x4f7320), the BSP insert and fill (`M9780`, 0x4f9780; `M9B80`, 0x4f9b80;
//! `M9C00`, 0x4f9c00) and the tree test (`M40`, 0x4f8a20). The asm fixes the operations; the C++ is only partly
//! right (its face rows and adjacency are one 0x18-byte stack row, and it misses the sample-index table).
//!
//! Left out, as they write nothing: the material lookups (`CMaterialManager::Tag520940`, whose flags `M7140` ignores
//! and whose other results are dropped) and the debug walk `M96F0` (0x4f96f0) the physics' debug flags gate.

use crate::car_body::helpers::cross_normalized;
use crate::math::vec::Vec4;

/// Vertex slots (+0x40) and their normals (+0x220).
pub const VERTICES: usize = 30;
/// BSP nodes (+0x414): the front and back child of each, 0 for none.
pub const NODES: usize = 56;
/// Edge slots (+0x484): the two vertices, the face that added it, the face on its other side, and two face bytes.
pub const EDGES: usize = 0x54;
/// The face rows `Build9CD0` keeps on its stack.
const ROWS: usize = 0x38;
/// `M9780`'s plane margins (0x739594 and 0x72b068).
const MARGIN: f32 = 0.1;
/// The edge filter's bound: `fcos` of the double at 0x739598 (10 degrees), stored as a float.
const CREASE: u32 = 0x3f7c_1c5c;

/// A collision grid's fields, at their offsets in the 0x6b0-byte object.
#[derive(Clone, Debug, PartialEq)]
pub struct ColGridData {
    /// The vertex bounds (+0x00, +0x10), their centre (+0x20) and half extent (+0x30).
    pub low: Vec4,
    pub high: Vec4,
    pub centre: Vec4,
    pub half: Vec4,
    pub vertices: [Vec4; VERTICES],
    pub normals: [Vec4; VERTICES],
    /// The sampled-surface data (+0x400) and its tables (+0x404..+0x410), all from the descriptor.
    pub sample_data: u32,
    pub sample_width: u16,
    pub sample_height: u16,
    pub sample_a: u32,
    pub sample_b: u32,
    /// The descriptor's faces (+0x410): 7 bytes each, three vertex indices, the material, three bytes.
    pub faces: u32,
    pub nodes: [[u8; 2]; NODES],
    pub edges: [[u8; 6]; EDGES],
    /// Build errors (+0x67c): 1 a bad edge or a failed tree test, 2 a bad face pairing, 4 a face across a plane;
    /// all ones for a grid with no vertices.
    pub status: u32,
    /// The shape record (+0x680), the name (+0x684), the event bytes (+0x688).
    pub shape: u32,
    pub name: u32,
    pub events: [u8; 2],
    /// +0x68a: 1 the descriptor's flag 4, 2 an edge with a face of material 0xff, 4 a sphere, 8 a box.
    pub flags: u8,
    pub node_count: u8,
    pub edge_count: u8,
    pub vertex_count: u8,
    pub sampled: u8,
    pub material: u8,
    /// +0x690: per vertex, a byte of the last face that names it.
    pub sample_index: [u8; 0x20],
}

/// The descriptor fields `Build9CD0` reads (`CSigCollisionMesh`, 0x34 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshDesc {
    /// +4: 4 copies to the grid's flag 1, 0x10000 a sphere and 0x20000 a box shape record.
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

/// One of `Build9CD0`'s stack rows: the face, the face across each of its edges (none: all ones) and whether the BSP
/// holds it. Rows past the faces start held.
#[derive(Clone, Copy)]
struct Row {
    face: [u8; 7],
    across: [u32; 3],
    held: bool,
}

/// `Build9CD0` on `grid` from `desc`, its vertices (only x, y, z are read) and its faces.
pub fn build(grid: &mut ColGridData, desc: &MeshDesc, vertices: &[Vec4], faces: &[[u8; 7]]) {
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
    if desc.flags & 0x30000 != 0 {
        grid.shape = desc.shape;
        grid.flags |= if desc.flags & 0x20000 != 0 { 8 } else { 4 };
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

    assert!(faces.len() <= ROWS, "{} faces overrun Build9CD0's 0x38 stack rows", faces.len());
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
        insert(grid, faces, rows[0].face, 0);
        for k in 0..3 {
            let across = rows[0].across[k];
            if across != u32::MAX {
                fill(grid, faces, &mut rows, across as usize);
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
        let crease = f32::from_bits(CREASE);
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
            if crease >= dot {
                grid.edges[kept] = edge;
                kept += 1;
            }
        }
        grid.edge_count = kept as u8;
    }
}

/// `M7140` (0x4f7140): files edge `b`-`c` of face `a` (with the face's bytes `d`, `e`). An edge the table holds the
/// other way round takes `a` as its other face (-1); the same way round also does, and is an error (0). A new edge
/// goes in while there is room (1); its other-face byte keeps what was there.
pub fn add_edge(grid: &mut ColGridData, a: u8, b: u8, c: u8, d: u8, e: u8) -> i32 {
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

fn vertex(grid: &ColGridData, index: u8) -> Vec4 {
    *grid.vertices.get(usize::from(index)).unwrap_or_else(|| panic!("vertex {index} aliases the grid's other fields"))
}

/// The face's first and second edge from its middle vertex, `v0 - v1` and `v2 - v1` (x, y, z; `w` 0).
fn face_edges(grid: &ColGridData, face: [u8; 7]) -> (Vec4, Vec4) {
    let (v0, v1, v2) = (vertex(grid, face[0]), vertex(grid, face[1]), vertex(grid, face[2]));
    let d = |a: Vec4, b: Vec4| Vec4 { x: a.x - b.x, y: a.y - b.y, z: a.z - b.z, w: 0.0 };
    (d(v0, v1), d(v2, v1))
}

/// `M7250` (0x4f7250): `(v2 - v1) x (v0 - v1)` of face `index`.
pub fn triangle_cross(grid: &ColGridData, faces: &[[u8; 7]], index: u8) -> Vec4 {
    let (t1, t2) = face_edges(grid, faces[usize::from(index)]);
    t2.cross3(t1)
}

/// `M7320` (0x4f7320): face `index`'s plane, the unit normal (`CrossNorm` of the same cross product) and
/// `0 - n.v1` as `w`.
pub fn plane(grid: &ColGridData, faces: &[[u8; 7]], index: u8) -> Vec4 {
    let face = faces[usize::from(index)];
    let (t1, t2) = face_edges(grid, face);
    let n = cross_normalized(t2, t1);
    let v1 = vertex(grid, face[1]);
    Vec4 { w: 0.0 - ((v1.z * n.z + v1.y * n.y) + v1.x * n.x), ..n }
}

/// `M9780` (0x4f9780): walks the BSP from `node` with the face's three vertices (each more than 0.1 behind a plane
/// sends it to the front child, else more than 0.1 in front to the back child, else to the front; behind and in
/// front both is error 4), naming the free child it reaches the next node. That node takes the face, adds the cross
/// product of face `node_count` (the original's index) to the face's vertex normals, and is returned.
pub fn insert(grid: &mut ColGridData, faces: &[[u8; 7]], face: [u8; 7], mut node: u8) -> u8 {
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

/// `M9C00` (0x4f9c00) and `M9B80` (0x4f9b80): row `index` and, depth first, the rows across its edges go into the
/// BSP, each once.
fn fill(grid: &mut ColGridData, faces: &[[u8; 7]], rows: &mut [Row; ROWS], index: usize) {
    let row = rows.get_mut(index).unwrap_or_else(|| panic!("row {index} overruns Build9CD0's stack rows"));
    if row.held {
        return;
    }
    row.held = true;
    let (face, across) = (row.face, row.across);
    insert(grid, faces, face, 0);
    for next in across {
        if next != u32::MAX {
            fill(grid, faces, rows, next as usize);
        }
    }
}

/// `M40` (0x4f8a20): walks the BSP from `node` with the point `p` (`p.w` unread): on or in front of a plane to the
/// front child, else to the back child. True when it leaves through a missing front child.
pub fn tree_test(grid: &ColGridData, faces: &[[u8; 7]], p: Vec4, mut node: u8) -> bool {
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

    fn empty() -> ColGridData {
        ColGridData {
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
        assert_eq!(f32::from_bits(CREASE), 10f64.to_radians().cos() as f32);
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
