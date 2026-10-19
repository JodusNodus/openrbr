//! Reader for stage collision files `Maps/track-<NN>_<X>.col`.
//!
//! Layout from the 1.02 loader `FUN_004fe570` and its fixup walkers
//! `FUN_004fdff0` (per object), `FUN_004fd6b0` (tree walk), `FUN_004fd630`
//! (vertex slot pool); see `notes/formats/col.md` for the evidence.
//!
//! ```text
//! file header (16 bytes):  u32 magic "OC7R", u32 root_off (0x10),
//!                          u32 cell_count, u32 table_off
//! root object at root_off (0x28 bytes of header, then data):
//!   u8 4, u8 3, u16 slot, u32 flags, u32 vertex_count (0),
//!   u32 tree_rel, u32 wall_rel, u32 mesh_a_count, u32 mesh_a_rel,
//!   u32 mesh_b_count, u32 mesh_b_rel
//!   brake-wall definition (not parsed), mesh_a and mesh_b (64 bytes each),
//!   root tree (32-byte nodes, offsets relative to the tree root)
//! table_off: cell_count u32 offsets (absolute)
//! cell at offset: u8 sub, u8 1, u16 slot, u32 graft flags, u32 vertex_count,
//!   u32 vertex_rel (0x14), u32 tree_rel; vertices (3 x f32 each);
//!   cell tree (32-byte nodes, offsets relative to the tree root);
//!   triangle arrays (18-byte records, each array padded to 4 bytes)
//! ```
//!
//! A node is 6 f32 bounds, u32 flags (low 21 bits triangle count, bit 21
//! graft placeholder), u32 pointer. Count 0 with a pointer is a pair of
//! children; count > 0 is a leaf whose triangles start at the pointer.
//! The loader grafts each cell tree onto a root placeholder chosen by the
//! cell's flags (top byte = depth, low bits = path).
//!
//! [`parse`] requires the whole buffer to be exactly tiled by the header,
//! table, root, mesh lists, cells and triangle arrays (the brake-wall
//! definition is the only unparsed region), so trailing or overlapping
//! data is an error. All reads are bounds-checked.

use std::collections::HashSet;
use std::fmt::{self, Display, Formatter};
use std::ops::Range;

mod relocate;

pub use relocate::relocate;

/// `"OC7R"` as a little-endian u32.
pub const MAGIC: u32 = 0x5237_434f;
/// File header length.
pub const FILE_HEADER_LEN: usize = 0x10;
/// Root object header length (includes the brake-wall pointers).
pub const ROOT_HEADER_LEN: usize = 0x28;
/// Cell object header length (the vertices start right after it).
pub const CELL_HEADER_LEN: usize = 0x14;
/// Bytes per tree node.
pub const NODE_LEN: usize = 0x20;
/// Bytes per mesh record (four 16-byte corners).
pub const MESH_LEN: usize = 0x40;
/// Bytes per vertex (three f32).
pub const VERTEX_LEN: usize = 12;
/// Bytes per triangle record.
pub const TRIANGLE_LEN: usize = 0x12;
/// Vertex slots available to the loader.
pub const SLOT_COUNT: usize = 0x400;

const COUNT_MASK: u32 = 0x001f_ffff;
/// Root tree: a node with any of these bits set is a graft placeholder.
const PLACEHOLDER_MASK: u32 = 0xffe0_0000;
/// Cell trees: no node is a placeholder.
const NO_MASK: u32 = 0;
/// Object type byte for cells (`mode` in the loader's switch).
const CELL_MODE: u8 = 1;
/// Root object sub-type and mode (the loader's `case 3` / `case 4` path).
const ROOT_SUB: u8 = 4;
const ROOT_MODE: u8 = 3;

/// Parsed `.col` file.
#[derive(Debug, Clone, PartialEq)]
pub struct ColFile {
    pub root: Root,
    pub cells: Vec<Cell>,
}

/// What [`relocate`] did to the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relocated {
    /// Objects that took a vertex slot (their `+2` and leaf slot bits).
    pub slotted: usize,
    /// Cells grafted onto the root tree.
    pub grafted: usize,
    /// Cells the loader skips: no vertices, so no slot and no graft.
    pub skipped: usize,
}

/// The root object: the skeleton tree whose placeholders the cells replace.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    pub tree: Tree,
    /// First mesh list (count and offset at root `+0x18`/`+0x1c`). The
    /// loader calls it only when a runtime check passes.
    pub mesh_a: Vec<Mesh>,
    /// Second mesh list (root `+0x20`/`+0x24`).
    pub mesh_b: Vec<Mesh>,
    /// Brake-wall definition at root `+0x14` (absent when that word is 0 or
    /// the point count is 0).
    pub brake_wall: Option<BrakeWall>,
}

/// Brake-wall definition (E34 `SetWall` and `E34WallVf` ctor).
#[derive(Debug, Clone, PartialEq)]
pub struct BrakeWall {
    /// 2-D points (`def+0x10`, count at `def+4`, 8 bytes each). Read as
    /// vertices by `CBrakeWall` (inferred from the ctor's field name).
    pub points: Vec<[f32; 2]>,
    /// Root node of the wall tree: centre x, centre y, half x, half y
    /// (from `CBrakeWall::QueryPoint`). Its extent equals the point
    /// extremes in every retail file, to float rounding.
    pub root_box: [f32; 4],
}

/// One water mesh record: four corners of 4 f32 each (inferred from
/// `CCollisionMeshWater::CtorWater`, which takes min/max over the corners).
pub type Mesh = [[f32; 4]; 4];

/// A stage cell: vertices and a tree of triangles, grafted onto the root.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Raw graft word. Top byte = depth of the root placeholder, low bits
    /// = the path taken (bit 0 first).
    pub flags: u32,
    /// Index into `root.tree.nodes` of the placeholder this cell replaces.
    pub graft: usize,
    pub vertices: Vec<[f32; 3]>,
    pub tree: Tree,
}

/// A node arena. Node 0 is the root; an inner node's two children are
/// consecutive entries at `first_child` and `first_child + 1`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub triangles: Vec<Triangle>,
}

/// One 32-byte tree node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Min x, y, z followed by the extent x, y, z (inferred: children lie
    /// inside their parent).
    pub bounds: [f32; 6],
    /// Raw node flags word.
    pub flags: u32,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// Count 0 with a pointer: children at `first_child` and `first_child + 1`.
    Inner { first_child: usize },
    /// Count > 0: triangles `tree.triangles[range]`.
    Leaf { triangles: Range<usize> },
    /// Root only: flags carry the placeholder bit. A cell's tree is grafted
    /// here at load time, so the file's own contents are never walked.
    Placeholder,
    /// Count 0 and pointer 0: the loader skips it. Not seen in retail data.
    Empty,
}

/// One 18-byte triangle record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    /// Indices into the owning cell's vertex array (checked).
    pub vertices: [u16; 3],
    /// Two 16-bit words (`+6`, `+8`): three 5-bit values (bits 0-4, 5-9,
    /// 10-14, first vertex first) and a flag in bit 15
    /// (see [`Triangle::attr_parts`]). The loader
    /// interpolates `+6` into the ground-hit `hit[9]`, which the wheel probe
    /// passes as the blend weight between the two surface entries
    /// (`notes/formats/col.md`); `+8` interpolates into `hit[8]`, whose
    /// game reader is unknown.
    pub attrs: [u16; 2],
    /// `+0xa` and `+0xb`: index into the stage's E44 material table. Raw
    /// values; out-of-range values are not rejected (see notes).
    pub materials: [u8; 2],
    /// `+0xc..+0x11`: per-vertex low/high nibble pairs for the two
    /// materials (`+0xc..+0xe` for `materials[0]`, `+0xf..+0x11` for
    /// `materials[1]`). Read as (u, v) grid coordinates (inferred).
    pub grid: [u8; 6],
}

/// Parse failure with the offending byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated {
        offset: usize,
        needed: usize,
        len: usize,
    },
    BadMagic {
        found: u32,
    },
    BadRoot {
        offset: usize,
    },
    BadCellMode {
        offset: usize,
        mode: u8,
    },
    SharedNode {
        offset: usize,
    },
    BadGraft {
        cell: usize,
    },
    VertexIndex {
        cell: usize,
        index: u16,
        vertex_count: usize,
    },
    TooManyCells {
        with_vertices: usize,
    },
    Overlap {
        offset: usize,
    },
    Gap {
        offset: usize,
    },
    /// Cell mode 2 (the loader's re-entry path: base 0, slot bits
    /// cleared, no retail coverage). [`relocate`] refuses it.
    AlreadyRelocated {
        offset: usize,
    },
    BrakeWall {
        offset: usize,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed, len } => {
                write!(f, "truncated at offset {offset}: need {needed} bytes but file is {len} bytes")
            }
            Error::BadMagic { found } => write!(f, "bad magic {found:#010x}"),
            Error::BadRoot { offset } => write!(f, "root object at {offset} is not type 4/3"),
            Error::BadCellMode { offset, mode } => {
                write!(f, "cell at {offset} has mode {mode}, expected 1")
            }
            Error::SharedNode { offset } => write!(f, "tree pair at {offset} referenced twice"),
            Error::BadGraft { cell } => write!(f, "cell {cell} path does not reach a placeholder"),
            Error::VertexIndex { cell, index, vertex_count } => {
                write!(f, "cell {cell}: vertex index {index} outside {vertex_count} vertices")
            }
            Error::TooManyCells { with_vertices } => {
                write!(f, "{with_vertices} cells with vertices exceed {SLOT_COUNT} slots")
            }
            Error::Overlap { offset } => write!(f, "overlapping data at offset {offset}"),
            Error::Gap { offset } => write!(f, "unaccounted bytes at offset {offset}"),
            Error::AlreadyRelocated { offset } => {
                write!(f, "cell at {offset} is already relocated (mode 2); no retail coverage")
            }
            Error::BrakeWall { offset } => {
                write!(f, "brake-wall definition at {offset} is inconsistent")
            }
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

fn bytes_at(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], Error> {
    offset.checked_add(len).and_then(|end| bytes.get(offset..end)).ok_or(Error::Truncated {
        offset,
        needed: len,
        len: bytes.len(),
    })
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(le_u32(bytes_at(bytes, offset, 4)?))
}

/// `b` must hold at least 4 bytes (callers take them from `bytes_at`).
fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn le_f32(b: &[u8]) -> f32 {
    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn read_floats<const N: usize>(b: &[u8]) -> [f32; N] {
    std::array::from_fn(|i| le_f32(&b[i * 4..]))
}

fn read_triangle(b: &[u8]) -> Triangle {
    Triangle {
        vertices: [
            u16::from_le_bytes([b[0], b[1]]),
            u16::from_le_bytes([b[2], b[3]]),
            u16::from_le_bytes([b[4], b[5]]),
        ],
        attrs: [u16::from_le_bytes([b[6], b[7]]), u16::from_le_bytes([b[8], b[9]])],
        materials: [b[10], b[11]],
        grid: [b[12], b[13], b[14], b[15], b[16], b[17]],
    }
}

fn padded4(len: usize) -> usize {
    (len + 3) & !3
}

impl Triangle {
    /// Splits one attr word (`attrs[0]` = `+6`, `attrs[1]` = `+8`) into its
    /// three 5-bit per-vertex values (bits 0-4, 5-9, 10-14, first vertex
    /// first) and the bit-15 flag (`hit[13]` for `+6`, `hit[14]` for `+8`
    /// in the ground-hit record). The ground query (`FUN_004fd760`)
    /// interpolates the values with the hit barycentric weights and divides
    /// by 31; the flags pass through as 0.0/1.0 floats.
    pub fn attr_parts(word: u16) -> ([u8; 3], bool) {
        ([(word & 0x1f) as u8, ((word >> 5) & 0x1f) as u8, ((word >> 10) & 0x1f) as u8], word & 0x8000 != 0)
    }

    /// `+6` attr: per-vertex values and flag (see [`Triangle::attr_parts`]).
    pub fn attr_a(self) -> ([u8; 3], bool) {
        Self::attr_parts(self.attrs[0])
    }

    /// `+8` attr: per-vertex values and flag (see [`Triangle::attr_parts`]).
    pub fn attr_b(self) -> ([u8; 3], bool) {
        Self::attr_parts(self.attrs[1])
    }
}

/// Parses a whole `.col` buffer.
pub fn parse(bytes: &[u8]) -> Result<ColFile, Error> {
    let header = bytes_at(bytes, 0, FILE_HEADER_LEN)?;
    let magic = le_u32(header);
    if magic != MAGIC {
        return Err(Error::BadMagic { found: magic });
    }
    let root_off = le_u32(&header[4..]) as usize;
    let cell_count = le_u32(&header[8..]) as usize;
    let table_off = le_u32(&header[12..]) as usize;
    let table_len = cell_count.checked_mul(4).ok_or(Error::Truncated {
        offset: table_off,
        needed: usize::MAX,
        len: bytes.len(),
    })?;
    bytes_at(bytes, table_off, table_len)?;

    let mut spans =
        vec![Span { start: 0, end: FILE_HEADER_LEN }, Span { start: table_off, end: table_off + table_len }];

    let root_raw = bytes_at(bytes, root_off, ROOT_HEADER_LEN)?;
    if root_raw[0] != ROOT_SUB || root_raw[1] != ROOT_MODE {
        return Err(Error::BadRoot { offset: root_off });
    }
    let tree_off = root_off + le_u32(&root_raw[0x10..]) as usize;
    let mesh_a_off = root_off + le_u32(&root_raw[0x1c..]) as usize;
    let mesh_a_len = le_u32(&root_raw[0x18..]) as usize;
    let mesh_b_off = root_off + le_u32(&root_raw[0x24..]) as usize;
    let mesh_b_len = le_u32(&root_raw[0x20..]) as usize;
    let wall_word = le_u32(&root_raw[0x14..]) as usize;
    let wall_off = (wall_word != 0).then_some(root_off + wall_word);

    // The brake-wall definition runs from the header to the first following
    // region. Only its header, points and root node are read (`parse_brake_wall`).
    let following = [Some(tree_off), (mesh_a_len > 0).then_some(mesh_a_off), (mesh_b_len > 0).then_some(mesh_b_off)]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(tree_off);
    if following < root_off + ROOT_HEADER_LEN {
        return Err(Error::Overlap { offset: following });
    }
    spans.push(Span { start: root_off, end: root_off + ROOT_HEADER_LEN });
    spans.push(Span { start: root_off + ROOT_HEADER_LEN, end: following });

    let root_tree = parse_tree(bytes, tree_off, PLACEHOLDER_MASK, &mut spans)?;
    let mesh_a = parse_meshes(bytes, mesh_a_off, mesh_a_len, &mut spans)?;
    let mesh_b = parse_meshes(bytes, mesh_b_off, mesh_b_len, &mut spans)?;
    let brake_wall = match wall_off {
        Some(off) => parse_brake_wall(bytes, off)?,
        None => None,
    };

    let mut cells = Vec::with_capacity(cell_count);
    let mut with_vertices = 0usize;
    for i in 0..cell_count {
        let cell_off = u32_at(bytes, table_off + 4 * i)? as usize;
        let cell = parse_cell(bytes, i, cell_off, &mut spans)?;
        if !cell.vertices.is_empty() {
            with_vertices += 1;
        }
        cells.push(cell);
    }
    if with_vertices > SLOT_COUNT {
        return Err(Error::TooManyCells { with_vertices });
    }

    // Two cells may reach the same placeholder (community stages, not retail): the loader copies each over it in
    // turn, so the later cell is the one the root reaches ([`relocate`]).
    for (i, cell) in cells.iter_mut().enumerate() {
        cell.graft = resolve_graft(&root_tree, cell.flags).ok_or(Error::BadGraft { cell: i })?;
    }

    check_tiling(spans, bytes.len())?;
    Ok(ColFile { root: Root { tree: root_tree, mesh_a, mesh_b, brake_wall }, cells })
}

fn parse_cell(bytes: &[u8], index: usize, off: usize, spans: &mut Vec<Span>) -> Result<Cell, Error> {
    let raw = bytes_at(bytes, off, CELL_HEADER_LEN)?;
    if raw[1] != CELL_MODE {
        return Err(Error::BadCellMode { offset: off, mode: raw[1] });
    }
    let flags = le_u32(&raw[4..]);
    let vertex_count = le_u32(&raw[8..]) as usize;
    let vertex_rel = le_u32(&raw[12..]) as usize;
    let tree_rel = le_u32(&raw[16..]) as usize;
    spans.push(Span { start: off, end: off + CELL_HEADER_LEN });

    let vertex_start = off + vertex_rel;
    let vertex_bytes = bytes_at(bytes, vertex_start, vertex_count * VERTEX_LEN)?;
    if vertex_count > 0 {
        spans.push(Span { start: vertex_start, end: vertex_start + vertex_count * VERTEX_LEN });
    }
    let vertices = vertex_bytes.chunks_exact(VERTEX_LEN).map(read_floats::<3>).collect();

    let tree = parse_tree(bytes, off + tree_rel, NO_MASK, spans)?;
    for t in &tree.triangles {
        if let Some(&bad) = t.vertices.iter().find(|&&v| usize::from(v) >= vertex_count) {
            return Err(Error::VertexIndex { cell: index, index: bad, vertex_count });
        }
    }
    Ok(Cell { flags, graft: 0, vertices, tree })
}

/// Walks one tree from `base` the way `FUN_004fd6b0` does: nodes whose flags
/// hit `mask` are placeholders and are not descended; count 0 with a pointer
/// is a pair of children at `base + pointer`; count > 0 is a leaf whose
/// triangles start at `base + pointer`.
fn parse_tree(bytes: &[u8], base: usize, mask: u32, spans: &mut Vec<Span>) -> Result<Tree, Error> {
    let mut tree = Tree::default();
    let mut pairs = HashSet::new();
    spans.push(Span { start: base, end: base + NODE_LEN });
    tree.nodes.push(EMPTY_NODE);
    let mut pending = vec![(base, 0usize)];
    while let Some((off, index)) = pending.pop() {
        let raw = bytes_at(bytes, off, NODE_LEN)?;
        let bounds = read_floats::<6>(raw);
        let flags = le_u32(&raw[0x18..]);
        let pointer = le_u32(&raw[0x1c..]) as usize;
        let kind = if flags & mask != 0 {
            NodeKind::Placeholder
        } else if flags & COUNT_MASK == 0 {
            if pointer == 0 {
                NodeKind::Empty
            } else {
                let pair = base + pointer;
                bytes_at(bytes, pair, 2 * NODE_LEN)?;
                if !pairs.insert(pair) {
                    return Err(Error::SharedNode { offset: pair });
                }
                spans.push(Span { start: pair, end: pair + 2 * NODE_LEN });
                let first_child = tree.nodes.len();
                tree.nodes.push(EMPTY_NODE);
                tree.nodes.push(EMPTY_NODE);
                pending.push((pair, first_child));
                pending.push((pair + NODE_LEN, first_child + 1));
                NodeKind::Inner { first_child }
            }
        } else {
            let count = (flags & COUNT_MASK) as usize;
            let tris = base + pointer;
            let raw_tris = bytes_at(bytes, tris, padded4(count * TRIANGLE_LEN))?;
            spans.push(Span { start: tris, end: tris + padded4(count * TRIANGLE_LEN) });
            let first = tree.triangles.len();
            tree.triangles.extend(raw_tris.chunks_exact(TRIANGLE_LEN).take(count).map(read_triangle));
            NodeKind::Leaf { triangles: first..tree.triangles.len() }
        };
        tree.nodes[index] = Node { bounds, flags, kind };
    }
    Ok(tree)
}

const EMPTY_NODE: Node = Node { bounds: [0.0; 6], flags: 0, kind: NodeKind::Empty };

/// Reads the brake-wall header at `off` (E34 `SetWall`): `+0` size (unused
/// by the ctor), `+4` point count (0 means no wall), `+8` point block offset,
/// `+0xc` wall root node offset. The point block must end where the root
/// node starts (76 of 76 retail definitions).
fn parse_brake_wall(bytes: &[u8], off: usize) -> Result<Option<BrakeWall>, Error> {
    let head = bytes_at(bytes, off, 0x10)?;
    let count = le_u32(&head[4..]) as usize;
    if count == 0 {
        return Ok(None);
    }
    let points_off = off + le_u32(&head[8..]) as usize;
    let root_off = off + le_u32(&head[12..]) as usize;
    let points = bytes_at(bytes, points_off, count * 8)?;
    if points_off + count * 8 != root_off {
        return Err(Error::BrakeWall { offset: off });
    }
    let root = bytes_at(bytes, root_off, 0x18)?;
    Ok(Some(BrakeWall {
        points: points.chunks_exact(8).map(read_floats::<2>).collect(),
        root_box: read_floats::<4>(root),
    }))
}

fn parse_meshes(bytes: &[u8], off: usize, count: usize, spans: &mut Vec<Span>) -> Result<Vec<Mesh>, Error> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let raw = bytes_at(bytes, off, count * MESH_LEN)?;
    spans.push(Span { start: off, end: off + count * MESH_LEN });
    Ok(raw.chunks_exact(MESH_LEN).map(|m| std::array::from_fn(|c| read_floats::<4>(&m[c * 16..]))).collect())
}

/// Follows a cell's graft path from the root node: `depth` steps, each
/// taking child `bit` of an inner node, least significant bit first. The
/// node reached must be a placeholder.
fn resolve_graft(root: &Tree, flags: u32) -> Option<usize> {
    let depth = (flags >> 24) as usize;
    if depth == 0 {
        return None;
    }
    let mut index = 0usize;
    let mut bits = flags;
    for _ in 0..depth {
        let NodeKind::Inner { first_child } = root.nodes.get(index)?.kind else {
            return None;
        };
        index = first_child + (bits & 1) as usize;
        bits >>= 1;
    }
    matches!(root.nodes.get(index)?.kind, NodeKind::Placeholder).then_some(index)
}

/// Requires the spans to cover `0..len` exactly, with no overlap.
fn check_tiling(mut spans: Vec<Span>, len: usize) -> Result<(), Error> {
    spans.sort_unstable_by_key(|s| s.start);
    let mut pos = 0usize;
    for s in &spans {
        if s.start < pos {
            return Err(Error::Overlap { offset: s.start });
        }
        if s.start > pos {
            return Err(Error::Gap { offset: pos });
        }
        pos = s.end;
    }
    if pos != len {
        return Err(Error::Gap { offset: pos });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
