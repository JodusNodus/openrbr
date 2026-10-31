//! Reader for the car model files `Cars/<CAR>/<car>.sgc` (one per car, about 2 MB each).
//!
//! Layout, little-endian, verified on all eight retail files (`notes/formats.md` "Car models"):
//!
//! - Banner: 22 ASCII characters and a NUL, 23 bytes. `CHH_CarLoader` compares all 23 bytes
//!   (`HedgeHog3D.dll` 1.02, `0x1001f2a0`, literal at `0x101af738`).
//! - Records to the end of the file, with nothing between them. Each record is a u32 header length
//!   (always 12), the header (`kind`, `subtype`, and a third word that is always 1 and unused by the
//!   loader), a u32 body length, and the body.
//!
//! The loader dispatches on `kind` and reads only kinds 0 (geometry), 2 (camera), 3 (helper),
//! 16 (collision) and 17 (shadow). Every other kind is skipped by its body length. `Point_*` records
//! (kind 22) are not read from this file: the points come from `<car>.ini`.
//!
//! Node bodies begin with a NUL-terminated name, then two i32 (`id`, `parent`, where `-1` is a
//! root), a 56-byte transform and a 3x4 world matrix. Mesh bodies continue with the bounds (centre and half-extent), vertices
//! (position and normal), optional UVs, an unused 4-byte stream, a `u16` triangle list, faces that
//! name a `FaceBlock<N>` shader slot, and, for damageable panels, a morph block.

use crate::le::le_bytes;
use crate::sg_framing;
use std::collections::HashMap;

pub use crate::sg_framing::FileKind;
pub use crate::sg_framing::BANNER;

pub const KIND_GEOMETRY: u32 = 0;
pub const KIND_CAMERA: u32 = 2;
pub const KIND_HELPER: u32 = 3;
pub const KIND_COLLISION: u32 = 16;
pub const KIND_SHADOW: u32 = 17;
pub const KIND_POINT: u32 = 22;

/// Geometry subtype of a static part (wheels, glass, lights, ...).
pub const SUBTYPE_STATIC: u32 = 1;
/// Geometry subtype of a damageable body panel. Its body carries a morph block.
pub const SUBTYPE_DAMAGEABLE: u32 = 3;
const SUBTYPE_CAMERA: u32 = 11;
const SUBTYPE_HELPER: u32 = 13;

/// Vertex stream flags (`streamFlags`, the word after the vertex count).
pub const STREAM_VERTICES: u32 = 0x1;
pub const STREAM_UVS: u32 = 0x4;
pub const STREAM_AUX: u32 = 0x10;

pub use crate::sg_framing::Record;

/// Failure to read a model. The variants live in [`sg_framing::Error`](crate::sg_framing::Error).
///
/// The car reader never constructs [`Trailing`](crate::sg_framing::Error::Trailing) or
/// [`UnknownBinding`](crate::sg_framing::Error::UnknownBinding); the banner
/// reports [`FileKind::Sgc`].
pub type Error = crate::sg_framing::Error;

/// A decoded record. Records the loader does not read keep only their kind and subtype.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// Kind 0, subtype 1 or 3.
    Mesh(Mesh),
    /// Kind 3, subtype 13 (helper) or kind 22 (point). Same layout.
    Node(Node),
    /// Kind 16, subtype 0.
    Collision(Collision),
    /// Kind 2 (camera) or 17 (shadow). The ids and name are read, the rest is
    /// checked by its length and not decoded.
    Unparsed { kind: u32, subtype: u32, name: String, id: i32, parent: i32 },
    /// Any other kind. The 1.02 car loader skips these.
    Ignored { kind: u32, subtype: u32 },
}

/// Common node header: name, ids, transform and world matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    /// First int of the node header. Unique within a car.
    pub id: i32,
    /// Second int: the `id` of the parent node, or `-1` for a root.
    pub parent: i32,
    pub transform: Transform,
    /// 3x4 matrix in file order: each row is `[R | t]`.
    pub world: [[f32; 4]; 3],
}

/// The 56-byte transform block, kept as written; the loader composes it into a matrix. The field names are an older
/// reading: HedgeHog3D's reader (`HH_H7ReadTransform`, 1.02 0x1000c7a0) takes the 14 words ([`Transform::words`]) as a
/// translation (0-2), an angle and axis (3, 4-6), a scale (7-9) and a second angle and axis (10, 11-13).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub scale: [f32; 3],
    pub axis: [f32; 3],
    pub angle: f32,
    pub translation: [f32; 3],
    pub axis2: [f32; 3],
    pub angle2: f32,
}

/// A collision body (kind 16): one part of the car's collision model. The car loader (HedgeHog3D.dll) turns each into
/// a collision mesh (`CSigCollisionMesh`) whose grid id is given by the name.
#[derive(Debug, Clone, PartialEq)]
pub struct Collision {
    pub node: Node,
    pub vertices: Vec<[f32; 3]>,
    /// Two floats per entry; the loader stores each as a byte, `255 * x` truncated.
    pub samples: Vec<[f32; 2]>,
    /// Three vertex indices per face.
    pub faces: Vec<[i32; 3]>,
    /// Three sample indices per face, when the body has samples; else empty.
    pub face_samples: Vec<[i32; 3]>,
    /// The two optional maps, when present.
    pub maps: Vec<CollisionMap>,
}

/// One optional collision map: a name, a size `m` and, when its flag is set, `m * m` bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionMap {
    pub name: String,
    pub size: i32,
    pub data: Option<Vec<u8>>,
}

impl Transform {
    /// The block's 14 words in file order.
    pub fn words(&self) -> [f32; 14] {
        let mut w = [0.0; 14];
        w[0..3].copy_from_slice(&self.scale);
        w[3..6].copy_from_slice(&self.axis);
        w[6] = self.angle;
        w[7..10].copy_from_slice(&self.translation);
        w[10..13].copy_from_slice(&self.axis2);
        w[13] = self.angle2;
        w
    }
}

/// A geometry record (kind 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub node: Node,
    /// Subtype 3: a damageable panel with a morph block.
    pub damageable: bool,
    /// Centre of the vertex box. The vertices are stored relative to it: the world matrix places this
    /// point (rbr-recomp `notes/formats/sgc.md`, section 5.2).
    pub centre: [f32; 3],
    /// Half the size of the vertex box on each axis.
    pub half_extent: [f32; 3],
    pub stream_flags: u32,
    /// Position and normal per vertex. Empty unless [`STREAM_VERTICES`] is set.
    pub vertices: Vec<Vertex>,
    /// Texture coordinates. Empty unless [`STREAM_UVS`] is set.
    pub uvs: Vec<[f32; 2]>,
    /// Four bytes per vertex, read by the loader and dropped. Empty unless [`STREAM_AUX`] is set.
    pub aux: Vec<[u8; 4]>,
    /// Triangle list: three indices into `vertices` per triangle.
    pub triangles: Vec<[u16; 3]>,
    /// Draw ranges. They partition `triangles` in order.
    pub faces: Vec<Face>,
    /// Morph targets. Only present for subtype 3; zero or one in retail files.
    pub morph_targets: Vec<MorphTarget>,
    /// One list per morph entry: `(vertex index, weight)` pairs. The meaning is not verified.
    pub morph_lists: Vec<Vec<(u32, f32)>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

/// One draw range, in the field order of `CFaceBlock` (`DrawIndexedPrimitive`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    /// Material name. Equal to `FaceBlock<N>_Material` in the car's `<car>.ini`, where `N` is this
    /// face's index in the mesh. Empty for some meshes that have no shader slots.
    pub name: String,
    /// Triangle count.
    pub prim_count: u32,
    /// Lowest vertex index used.
    pub min_vertex: u32,
    /// Number of vertices in the range.
    pub num_vertices: u32,
    /// First index of the range. The index list is `triangles` flattened (three per triangle), so
    /// the first triangle is `start_index / 3`.
    pub start_index: u32,
}

/// One morph target of a damageable panel.
#[derive(Debug, Clone, PartialEq)]
pub struct MorphTarget {
    /// Always 1 in retail files. Meaning not verified.
    pub flag: u8,
    /// One vec3 per vertex.
    pub deltas: Vec<[f32; 3]>,
}

/// Reads the banner and the record list. Records must tile the file exactly.
pub fn records(bytes: &[u8]) -> Result<Vec<Record<'_>>, Error> {
    sg_framing::records(bytes, FileKind::Sgc)
}

/// Decodes every record of a model in file order.
pub fn parse(bytes: &[u8]) -> Result<Vec<Element>, Error> {
    records(bytes)?.iter().map(element).collect()
}

/// The 3x4 world matrix of every mesh and helper node, composed with its ancestors', by node id.
/// A node's stored matrix is relative to its parent's chained matrix, so a child of the car root is
/// placed by its own matrix, and a grandchild by its parent's and then its own. The root (a node
/// with parent `-1`, or whose parent is not in the file) is the car's origin and contributes nothing.
/// Checked against the interior and exterior door pairs (rbr-recomp `notes/formats/sgc.md`, section 5.2).
pub fn chained_worlds(elements: &[Element]) -> HashMap<i32, [[f32; 4]; 3]> {
    let nodes: HashMap<i32, &Node> = elements
        .iter()
        .filter_map(|e| match e {
            Element::Mesh(m) => Some((m.node.id, &m.node)),
            Element::Node(n) => Some((n.id, n)),
            _ => None,
        })
        .collect();
    nodes.keys().map(|&id| (id, chained_world(&nodes, id))).collect()
}

/// The chained world of node `id`: its ancestors' stored matrices, root first, multiplied in order.
fn chained_world(nodes: &HashMap<i32, &Node>, id: i32) -> [[f32; 4]; 3] {
    let mut stored = Vec::new();
    let mut node = nodes[&id];
    while let Some(parent) = nodes.get(&node.parent).filter(|p| p.id != node.id) {
        stored.push(node.world);
        node = parent;
    }
    stored.iter().rev().fold(IDENTITY_3X4, |acc, world| multiply_3x4(&acc, world))
}

/// The identity as a 3x4 matrix: rows `[R | t]`.
const IDENTITY_3X4: [[f32; 4]; 3] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

/// `a * b` for 3x4 matrices in the column-vector convention (`a` applied after `b`'s transform is
/// `a * [b; 1]`), so the result maps a child's points through the parent `a` after the child `b`.
fn multiply_3x4(a: &[[f32; 4]; 3], b: &[[f32; 4]; 3]) -> [[f32; 4]; 3] {
    let mut out = [[0.0; 4]; 3];
    for (row, out_row) in out.iter_mut().enumerate() {
        for (col, cell) in out_row.iter_mut().enumerate() {
            let rotation: f32 = (0..3).map(|k| a[row][k] * b[k][col]).sum();
            *cell = if col == 3 { rotation + a[row][3] } else { rotation };
        }
    }
    out
}

fn element(rec: &Record<'_>) -> Result<Element, Error> {
    let mut r = Reader::new(rec.body, rec.offset);
    let (kind, subtype) = (rec.kind, rec.subtype);
    let out = match (kind, subtype) {
        (KIND_GEOMETRY, SUBTYPE_STATIC | SUBTYPE_DAMAGEABLE) => {
            let mesh = mesh(&mut r, subtype == SUBTYPE_DAMAGEABLE)?;
            r.skip_padding();
            Element::Mesh(mesh)
        }
        (KIND_HELPER, SUBTYPE_HELPER) | (KIND_POINT, 0) => {
            let name = r.cstr()?;
            Element::Node(node(&mut r, name)?)
        }
        (KIND_CAMERA, SUBTYPE_CAMERA) => {
            let (name, id, parent) = (r.cstr()?, r.i32()?, r.i32()?);
            // transform (56), near/far/fov (12), world (48), int (4), vec3 (12).
            r.skip(132)?;
            Element::Unparsed { kind, subtype, name, id, parent }
        }
        (KIND_COLLISION, 0) => {
            // The car loader (0x1001f2a0, kind 0x10) moves on to the body's end after `0x1000a3f0`, so bytes after the
            // maps are not read: RSF's Escort Mk II has 64513 of them in `c_e_body`.
            let name = r.cstr()?;
            return Ok(Element::Collision(collision(&mut r, name)?));
        }
        (KIND_SHADOW, 0) => {
            let (name, id, parent) = (r.cstr()?, r.i32()?, r.i32()?);
            shadow(&mut r)?;
            Element::Unparsed { kind, subtype, name, id, parent }
        }
        (KIND_GEOMETRY | KIND_CAMERA | KIND_HELPER | KIND_COLLISION | KIND_SHADOW, _) => {
            return Err(Error::UnknownSubtype { offset: rec.offset, kind, subtype })
        }
        _ => return Ok(Element::Ignored { kind, subtype }),
    };
    r.finish()?;
    Ok(out)
}

fn node(r: &mut Reader<'_>, name: String) -> Result<Node, Error> {
    let id = r.i32()?;
    let parent = r.i32()?;
    let transform = Transform {
        scale: r.vec3()?,
        axis: r.vec3()?,
        angle: r.f32()?,
        translation: r.vec3()?,
        axis2: r.vec3()?,
        angle2: r.f32()?,
    };
    let world = [r.vec4()?, r.vec4()?, r.vec4()?];
    Ok(Node { name, id, parent, transform, world })
}

fn mesh(r: &mut Reader<'_>, damageable: bool) -> Result<Mesh, Error> {
    let name = r.cstr()?;
    let node = node(r, name)?;
    let (centre, half_extent) = (r.vec3()?, r.vec3()?);
    let vertex_count = r.count()?;
    let stream_flags = r.u32()?;

    let vertices = if stream_flags & STREAM_VERTICES != 0 {
        r.items(vertex_count, 24, |c| Ok(Vertex { position: c.vec3()?, normal: c.vec3()? }))?
    } else {
        Vec::new()
    };
    let uvs = if stream_flags & STREAM_UVS != 0 { r.items(vertex_count, 8, |c| c.vec2())? } else { Vec::new() };
    let aux = if stream_flags & STREAM_AUX != 0 {
        r.items(vertex_count, 4, |c| {
            let b = c.take(4)?;
            Ok(le_bytes(b))
        })?
    } else {
        Vec::new()
    };

    let triangle_count = r.count()?;
    let triangles = r.items(triangle_count, 6, |c| Ok([c.u16()?, c.u16()?, c.u16()?]))?;

    let face_count = r.count()?;
    let mut faces = Vec::new();
    for _ in 0..face_count {
        faces.push(Face {
            name: r.cstr()?,
            prim_count: r.count_u32()?,
            min_vertex: r.count_u32()?,
            num_vertices: r.count_u32()?,
            start_index: r.count_u32()?,
        });
    }

    let mut morph_targets = Vec::new();
    if damageable {
        let target_count = r.count()?;
        let per_target = r.count()?;
        for _ in 0..target_count {
            let flag = r.u8()?;
            let deltas = r.items(per_target, 12, |c| c.vec3())?;
            morph_targets.push(MorphTarget { flag, deltas });
        }
    }

    let list_count = r.u32()?;
    let mut morph_lists = Vec::new();
    for _ in 0..list_count {
        let pairs = r.i32()?;
        let list = if pairs > 0 { r.items(pairs as usize, 8, |c| Ok((c.u32()?, c.f32()?)))? } else { Vec::new() };
        morph_lists.push(list);
    }

    Ok(Mesh {
        node,
        damageable,
        centre,
        half_extent,
        stream_flags,
        vertices,
        uvs,
        aux,
        triangles,
        faces,
        morph_targets,
        morph_lists,
    })
}

/// Collision body after the name: ids, transform and world, then a vertex list, a sample list (two floats each), the
/// faces (12 bytes, or 24 with sample indices when there are samples), and an optional pair of maps.
fn collision(r: &mut Reader<'_>, name: String) -> Result<Collision, Error> {
    let node = node(r, name)?;
    let vertex_count = r.count()?;
    let vertices = r.items(vertex_count, 12, |c| c.vec3())?;
    let sample_count = r.u32()? as usize;
    let samples = r.items(sample_count, 8, |c| c.vec2())?;
    let face_count = r.count()?;
    let triple = |c: &mut Reader<'_>| Ok([c.i32()?, c.i32()?, c.i32()?]);
    let (mut faces, mut face_samples) = (Vec::new(), Vec::new());
    if sample_count != 0 {
        for (f, s) in r.items(face_count, 24, |c| Ok((triple(c)?, triple(c)?)))? {
            faces.push(f);
            face_samples.push(s);
        }
    } else {
        faces = r.items(face_count, 12, triple)?;
    }
    let maps = if r.u8()? == 1 { vec![collision_map(r)?, collision_map(r)?] } else { Vec::new() };
    Ok(Collision { node, vertices, samples, faces, face_samples, maps })
}

/// One optional collision map: a name, an int `m` and a flag. When the flag is set, `m * m` bytes
/// follow (unsigned product, as in the loader).
fn collision_map(r: &mut Reader<'_>) -> Result<CollisionMap, Error> {
    let name = r.cstr()?;
    let size = r.i32()?;
    let m = size as u32;
    let data = if r.u8()? != 0 { Some(r.take(m.wrapping_mul(m) as usize)?.to_vec()) } else { None };
    Ok(CollisionMap { name, size, data })
}

/// Shadow body after the name and ids: transform and world, a vec3 list, a list of 24-byte faces
/// (three ints and a vec3), and a list of 16-byte elements.
fn shadow(r: &mut Reader<'_>) -> Result<(), Error> {
    r.skip(56 + 48)?;
    let vertex_count = r.count()?;
    r.skip_items(vertex_count, 12)?;
    let face_count = r.count()?;
    r.skip_items(face_count, 24)?;
    let element_count = r.count()?;
    r.skip_items(element_count, 16)
}

type Reader<'a> = sg_framing::Reader<'a>;

impl Reader<'_> {
    /// Skips a zero word ending the body. Community models (the RSF cars' `evo_IX.sgc`) end every damageable mesh
    /// with one after the morph lists; GeomCar (`0x1000acf0`) reads the record through a cursor and never checks its
    /// end, so the game ignores it.
    fn skip_padding(&mut self) {
        if self.body[self.pos..] == [0; 4] {
            self.pos += 4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sg_framing::RECORD_HEADER_LEN;

    /// A file from `(kind, subtype, body)` records, with the 12-byte header and the trailing flag word.
    fn file(records: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = BANNER.to_vec();
        for (kind, subtype, body) in records {
            for word in [RECORD_HEADER_LEN, *kind, *subtype, 1, body.len() as u32] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            out.extend_from_slice(body);
        }
        out
    }

    fn f32s(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// Name, ids, transform (14 floats) and world (12 floats).
    fn node_body(name: &str, id: i32, parent: i32) -> Vec<u8> {
        let mut b = name.as_bytes().to_vec();
        b.push(0);
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&parent.to_le_bytes());
        b.extend(f32s(&[1.0, 2.0, 3.0, 0.0, 1.0, 0.0, 0.0, 4.0, 5.0, 6.0, 0.0, 1.0, 0.0, 0.0]));
        b.extend(f32s(&[1.0, 0.0, 0.0, 7.0, 0.0, 1.0, 0.0, 8.0, 0.0, 0.0, 1.0, 9.0]));
        b
    }

    /// Mesh body with three vertices, streams 0x17, one triangle and one face. `damageable` adds a
    /// morph target and one morph list.
    fn mesh_body(damageable: bool) -> Vec<u8> {
        let mut b = node_body("e_test", 10, -1);
        b.extend(f32s(&[-1.0, -1.0, -1.0, 1.0, 1.0, 1.0]));
        b.extend_from_slice(&3i32.to_le_bytes());
        b.extend_from_slice(&0x17u32.to_le_bytes());
        for i in 0..3 {
            let x = i as f32;
            b.extend(f32s(&[x, 0.0, 0.0, 0.0, 0.0, 1.0]));
        }
        for i in 0..3 {
            b.extend(f32s(&[i as f32, 0.5]));
        }
        b.extend_from_slice(&[255; 12]);
        b.extend_from_slice(&1i32.to_le_bytes());
        for index in [0u16, 1, 2] {
            b.extend_from_slice(&index.to_le_bytes());
        }
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(b"external::carpaint\0");
        for word in [1i32, 0, 3, 0] {
            b.extend_from_slice(&word.to_le_bytes());
        }
        if damageable {
            b.extend_from_slice(&1i32.to_le_bytes());
            b.extend_from_slice(&3i32.to_le_bytes());
            b.push(1);
            b.extend(f32s(&[0.1, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1]));
        }
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend(f32s(&[0.5]));
        b
    }

    #[test]
    fn rejects_bad_banner_and_header_length() {
        let mut bytes = file(&[]);
        bytes[0] = b'x';
        assert_eq!(parse(&bytes), Err(Error::BadBanner { kind: FileKind::Sgc }));

        let mut bytes = file(&[(KIND_HELPER, SUBTYPE_HELPER, node_body("parent", 1, -1))]);
        bytes[BANNER.len()] = 16;
        assert_eq!(records(&bytes), Err(Error::HeaderLength { offset: BANNER.len(), length: 16 }));
    }

    #[test]
    fn truncated_record_is_an_error() {
        let bytes = file(&[(KIND_HELPER, SUBTYPE_HELPER, node_body("parent", 1, -1))]);
        let cut = &bytes[..bytes.len() - 1];
        assert_eq!(records(cut), Err(Error::Truncated { offset: BANNER.len() }));
    }

    #[test]
    fn helper_and_point_read_as_nodes() {
        let bytes = file(&[
            (KIND_HELPER, SUBTYPE_HELPER, node_body("parent", 2966, -1)),
            (KIND_POINT, 0, node_body("Point_exhaust1", 4060, 2966)),
        ]);
        let elements = parse(&bytes).expect("parse");
        let Element::Node(root) = &elements[0] else {
            panic!("expected node, got {:?}", elements[0]);
        };
        assert_eq!(root.name, "parent");
        assert_eq!((root.id, root.parent), (2966, -1));
        assert_eq!(root.transform.scale, [1.0, 2.0, 3.0]);
        assert_eq!(root.transform.translation, [4.0, 5.0, 6.0]);
        assert_eq!(root.world[0], [1.0, 0.0, 0.0, 7.0]);
        assert_eq!(root.world[2], [0.0, 0.0, 1.0, 9.0]);
        let Element::Node(point) = &elements[1] else {
            panic!("expected node");
        };
        assert_eq!((point.name.as_str(), point.parent), ("Point_exhaust1", 2966));
    }

    #[test]
    fn static_mesh_reads_every_stream() {
        let bytes = file(&[(KIND_GEOMETRY, SUBTYPE_STATIC, mesh_body(false))]);
        let Element::Mesh(mesh) = &parse(&bytes).expect("parse")[0] else {
            panic!("expected mesh");
        };
        assert!(!mesh.damageable);
        assert_eq!(mesh.node.name, "e_test");
        assert_eq!(mesh.centre, [-1.0; 3]);
        assert_eq!(mesh.half_extent, [1.0; 3]);
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.vertices[2].position, [2.0, 0.0, 0.0]);
        assert_eq!(mesh.vertices[2].normal, [0.0, 0.0, 1.0]);
        assert_eq!(mesh.uvs[1], [1.0, 0.5]);
        assert_eq!(mesh.aux, vec![[255; 4]; 3]);
        assert_eq!(mesh.triangles, vec![[0, 1, 2]]);
        assert_eq!(mesh.faces[0].name, "external::carpaint");
        assert_eq!((mesh.faces[0].prim_count, mesh.faces[0].min_vertex, mesh.faces[0].num_vertices), (1, 0, 3));
        assert!(mesh.morph_targets.is_empty());
        assert_eq!(mesh.morph_lists, vec![vec![(2, 0.5)]]);
    }

    #[test]
    fn damageable_mesh_has_a_morph_target() {
        let bytes = file(&[(KIND_GEOMETRY, SUBTYPE_DAMAGEABLE, mesh_body(true))]);
        let Element::Mesh(mesh) = &parse(&bytes).expect("parse")[0] else {
            panic!("expected mesh");
        };
        assert!(mesh.damageable);
        assert_eq!(mesh.morph_targets.len(), 1);
        assert_eq!(mesh.morph_targets[0].flag, 1);
        assert_eq!(mesh.morph_targets[0].deltas, vec![[0.1, 0.0, 0.0], [0.0, 0.1, 0.0], [0.0, 0.0, 0.1]]);
        assert_eq!(mesh.morph_lists, vec![vec![(2, 0.5)]]);
    }

    #[test]
    fn unread_kinds_and_checked_layouts() {
        // Camera tail after the name: 140 bytes.
        let mut cam = b"Cam_external\0".to_vec();
        cam.extend(vec![0u8; 8 + 56 + 12 + 48 + 4 + 12]);
        let mut shadow = b"s_e_test\0".to_vec();
        shadow.extend(vec![0u8; 8 + 56 + 48]);
        shadow.extend_from_slice(&0i32.to_le_bytes());
        shadow.extend_from_slice(&0i32.to_le_bytes());
        shadow.extend_from_slice(&0i32.to_le_bytes());
        let bytes =
            file(&[(KIND_CAMERA, SUBTYPE_CAMERA, cam), (KIND_SHADOW, 0, shadow), (6, 21, b"parent::PRS\0".to_vec())]);
        let elements = parse(&bytes).expect("parse");
        assert!(matches!(&elements[0], Element::Unparsed { kind: 2, name, .. } if name == "Cam_external"));
        assert!(matches!(&elements[1], Element::Unparsed { kind: 17, name, id: 0, .. } if name == "s_e_test"));
        assert_eq!(elements[2], Element::Ignored { kind: 6, subtype: 21 });
    }

    #[test]
    fn body_length_mismatch_is_an_error() {
        let mut body = node_body("parent", 1, -1);
        body.push(0);
        let bytes = file(&[(KIND_HELPER, SUBTYPE_HELPER, body)]);
        assert_eq!(parse(&bytes), Err(Error::Leftover { offset: BANNER.len(), unread: 1 }));
    }

    #[test]
    fn unknown_subtype_and_negative_count_are_errors() {
        let bytes = file(&[(KIND_GEOMETRY, 2, node_body("e_x", 1, -1))]);
        assert_eq!(parse(&bytes), Err(Error::UnknownSubtype { offset: BANNER.len(), kind: KIND_GEOMETRY, subtype: 2 }));

        let mut body = node_body("e_x", 1, -1);
        body.extend(f32s(&[0.0; 6]));
        body.extend_from_slice(&(-1i32).to_le_bytes());
        let bytes = file(&[(KIND_GEOMETRY, SUBTYPE_STATIC, body)]);
        assert!(matches!(parse(&bytes), Err(Error::BadBody { .. })));
    }

    #[test]
    fn damaged_files_never_panic() {
        let bytes = file(&[
            (KIND_GEOMETRY, SUBTYPE_DAMAGEABLE, mesh_body(true)),
            (KIND_HELPER, SUBTYPE_HELPER, node_body("parent", 1, -1)),
        ]);
        crate::damage::never_panics(&bytes, &[0, 0x7f, 0xff], |b| {
            let _ = parse(b);
        });
    }
}
