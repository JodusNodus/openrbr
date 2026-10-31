//! Reader for the animated scenery models `SGS/*.sgs` (stage characters:
//! marshals, spectators, animals, the driver/codriver, the Red Bull cans).
//!
//! Container: the same 3DS Max export framing as the car models (see
//! [`crate::sgc`]): the 23-byte banner, then records of `[u32 header_len =
//! 12][u32 kind][u32 subtype][u32 1][u32 body_len][body]` tiling the file.
//! The HedgeHog3D character loader `CHH_3dsMaxCharacterLoader::
//! LoadCharacterFromBuf` (`HedgeHog3D.dll` 1.02, `0x100072a0`) reads kinds 0
//! (geometry, subtypes 1/2/3), 3 (helpers/subtype 13, bones/14) and 22
//! (points, `0x1000c550`); LocoMotion's `CLM_CharacterLoader::LoadHierarchy`
//! (`LocoMotion.dll` 1.02, `0x10015280`) reads kind 18 (the node table,
//! `LoadNodes`, `0x10014c20`). Every other kind is skipped by its body
//! length. See `notes/formats.md` "Characters" for the evidence.
//!
//! Parsed here:
//! - `(0, 1)` static mesh: the [`crate::sgc`] mesh layout (centre and
//!   half-extent, vertex/UV/aux streams, triangle list, faces) but with no
//!   trailing morph lists; the body ends after the faces.
//! - `(0, 2)` skinned mesh (`HH_H7LoadGeom`, `0x1000b490` with the skinned
//!   flag): min/max bounds, a vertex count and stream flags, per-vertex
//!   position/normal plus an 8-byte and a 4-byte stream, the triangle list,
//!   faces, then the skin block (head matrix, per-bone name and matrix, a
//!   count with that many 32-byte per-vertex records).
//! - `(1, 10)` omni light (`HH_H7LoadLight`, `0x1000c090`): node header,
//!   color, world matrix and an enable flag. Skipped by the character
//!   loader, decoded here by length.
//! - `(3, 13)` helper and `(3, 14)` bone: the [`crate::sgc`] node header.
//! - `(4, 0)` IK chain node: the node header and nothing else. Skipped by
//!   both loaders.
//! - `(18, 0)` hierarchy table: a count and that many node entries (chunk
//!   subtype, name, ids, world matrix, flags).
//! - `(2, 11)` camera and `(22, 0)` point: name and ids, the rest checked by
//!   length (same layouts as [`crate::sgc`]).
//!
//! Kinds 5, 14, 15 and 20 are kept as [`Element::Ignored`]: same bodies as
//! in the car files, unread by either loader.
//!
//! All reads are bounds-checked against the record body, [`parse`]
//! requires every body to be consumed exactly, and any non-record bytes
//! after the last record are kept in [`Sgs::trailing`] verbatim.

use crate::le::le_bytes;
use crate::sg_framing;

pub use crate::sg_framing::FileKind;
pub use crate::sg_framing::BANNER;

const KIND_GEOMETRY: u32 = 0;
const KIND_LIGHT: u32 = 1;
const KIND_CAMERA: u32 = 2;
const KIND_NODE: u32 = 3;
const KIND_IK: u32 = 4;
const KIND_HIERARCHY: u32 = 18;
const KIND_POINT: u32 = 22;

const SUBTYPE_STATIC: u32 = 1;
const SUBTYPE_SKINNED: u32 = 2;
const SUBTYPE_CAMERA: u32 = 11;
const SUBTYPE_HELPER: u32 = 13;
const SUBTYPE_BONE: u32 = 14;

/// Vertex stream flags (`streamFlags`, the word after the vertex count).
/// Same bits as [`crate::sgc`], except the loader also tests bit 4 (four
/// bytes per vertex) and ignores bit 1 (set in all 148 retail skinned
/// meshes, zero bytes read).
pub const STREAM_VERTICES: u32 = 0x1;
pub const STREAM_UVS: u32 = 0x4;
pub const STREAM_EXTRA: u32 = 0x10;

pub use crate::sg_framing::Record;

/// Failure to read a scenery model. The variants live in
/// [`sg_framing::Error`](crate::sg_framing::Error).
///
/// The scenery reader never constructs
/// [`UnknownBinding`](crate::sg_framing::Error::UnknownBinding); the banner
/// reports [`FileKind::Sgs`].
pub type Error = crate::sg_framing::Error;

/// A decoded record. Records both loaders skip keep only their kind and subtype.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// Kind 0, subtype 1. Same mesh type as [`crate::sgc`], but the body
    /// ends after the faces: no morph block, no morph lists.
    StaticMesh(crate::sgc::Mesh),
    /// Kind 0, subtype 2: skinned character mesh with its skin block.
    SkinnedMesh(SkinnedMesh),
    /// Kind 3, subtype 13.
    Helper(crate::sgc::Node),
    /// Kind 3, subtype 14.
    Bone(crate::sgc::Node),
    /// Kind 4, subtype 0: node header only. Skipped by both loaders.
    IkNode(crate::sgc::Node),
    /// Kind 1, subtype 10. Skipped by the character loader.
    Light(Light),
    /// Kind 18, subtype 0: the LocoMotion node table.
    Hierarchy(Hierarchy),
    /// Kind 2 (camera) or 22 (point). The name and ids are read, the rest is
    /// checked by its length and not decoded.
    Unparsed { kind: u32, subtype: u32, name: String, id: i32, parent: i32 },
    /// Kind 5, 14, 15 or 20. Same bodies as in the car files; unread by
    /// both loaders.
    Ignored { kind: u32, subtype: u32 },
}

/// A skinned character mesh (kind 0, subtype 2).
#[derive(Debug, Clone, PartialEq)]
pub struct SkinnedMesh {
    pub node: crate::sgc::Node,
    /// Minimum corner of the vertex box (`HH_H7LoadGeom` stores it into the
    /// bounds first word; retail files hold min/max, not centre/extent).
    pub min: [f32; 3],
    /// Maximum corner of the vertex box.
    pub max: [f32; 3],
    pub stream_flags: u32,
    /// Position and normal per vertex. Empty unless [`STREAM_VERTICES`] is set.
    pub vertices: Vec<crate::sgc::Vertex>,
    /// Texture coordinates, two floats per vertex (the loader's stream 3; the bit `.sgc` calls its UV
    /// stream). Empty unless [`STREAM_UVS`] is set. **File**: every skinned mesh's faces sample their
    /// `SGS/textures` maps with them.
    pub uvs: Vec<[f32; 2]>,
    /// Four bytes per vertex. Empty unless [`STREAM_EXTRA`] is set. Meaning
    /// not verified: kept raw.
    pub extra: Vec<[u8; 4]>,
    /// Triangle list: three indices into `vertices` per triangle.
    pub triangles: Vec<[u16; 3]>,
    /// Draw ranges. They partition `triangles` in order.
    pub faces: Vec<crate::sgc::Face>,
    /// The skin block (`HH_H7LoadSkin`, `0x1000bb50`).
    pub skin: Skin,
}

/// The skin block of a skinned mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    /// Head matrix, 3x4 in file order.
    pub head: [[f32; 4]; 3],
    /// One entry per bone: its name and matrix.
    pub bones: Vec<SkinBone>,
    /// Count word before the per-vertex records. Equals the vertex count in
    /// all 148 retail skinned meshes.
    pub weight_count: u32,
    /// One record per vertex in all retail files. The loader reads
    /// `weight_count` of them into the skin holder and returns (its scratch
    /// matrices come from the in-memory bone count, not from the file), so
    /// the body ends here.
    pub weights: Vec<SkinWeight>,
}

/// One 32-byte skin record: the bones that move a vertex and how much. `HH_SkinUpload` (HH `0x1007f4c0`) reads
/// four weights, then four byte offsets into the skin's bone-matrix array (`bone * 0x40`, a 4x4 matrix each), and
/// stops at the first offset of `-0x40`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinWeight {
    pub weights: [f32; 4],
    /// Byte offsets into the bone-matrix array: [`Skin::bones`] index times 0x40, `-0x40` for none.
    pub offsets: [i32; 4],
}

/// The offset word of an unused [`SkinWeight`] slot.
pub const NO_BONE: i32 = -0x40;

impl SkinWeight {
    /// The used `(bone index, weight)` pairs, in order. The upload always applies the first slot and stops at the
    /// next unused one. A first slot of [`NO_BONE`] reads the matrix before bone 0, which `SetSkin` (HH `0x10005ab0`)
    /// sets to identity: `None` here (retail: a few `richard.sgs` vertices, with weight 0).
    pub fn bones(&self) -> impl Iterator<Item = (Option<usize>, f32)> + '_ {
        let used = 1 + self.offsets[1..].iter().take_while(|&&o| o != NO_BONE).count();
        (0..used).map(|i| (usize::try_from(self.offsets[i] / 0x40).ok(), self.weights[i]))
    }
}

/// One bone of a [`Skin`] block.
#[derive(Debug, Clone, PartialEq)]
pub struct SkinBone {
    pub name: String,
    /// 3x4 matrix in file order.
    pub matrix: [[f32; 4]; 3],
}

/// An omni light (kind 1, subtype 10).
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub node: crate::sgc::Node,
    /// Diffuse color (`HH_H7LoadLight`, `0x1000c090`).
    pub color: [f32; 3],
    /// Enable flag. Retail files all store 0 (a nonzero flag would be
    /// followed by two more floats).
    pub enabled: u32,
}

/// The LocoMotion node table (kind 18, subtype 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Hierarchy {
    pub nodes: Vec<HierarchyNode>,
}

/// One entry of a [`Hierarchy`] table.
#[derive(Debug, Clone, PartialEq)]
pub struct HierarchyNode {
    /// Chunk subtype from `LoadNodes` (`0x10014c20`): `47` (0x2f, PRS node)
    /// in all 153 retail files; `48` (0x30) would be a PS node.
    pub node_type: u32,
    pub name: String,
    /// First int after the name.
    pub id: i32,
    /// Second int: the `id` of the parent node, or `-1` for a root.
    pub parent: i32,
    /// 3x4 matrix in file order (`LM_ReadMatrix3x4`).
    pub world: [[f32; 4]; 3],
    /// Flag word after the matrix. Decides the node class with `node_type`
    /// (`2` with type 47 is a bone); the exact mapping is only partly traced.
    pub sub: u32,
}

/// A parsed scenery model: its records in file order, plus any trailing
/// bytes. Two retail files carry non-record bytes after their last record
/// (`REDBULL_prize_01.sgs`: 7,635; `moose_01.sgs`: 24); every other retail
/// file tiles exactly and has empty `trailing`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sgs<'a> {
    pub elements: Vec<Element>,
    pub trailing: &'a [u8],
}

/// Reads the banner and the record list. Records must tile the file exactly;
/// files with trailing bytes fail here but load through [`parse`].
pub fn records(bytes: &[u8]) -> Result<Vec<Record<'_>>, Error> {
    let (out, trailing) = sg_framing::split_records(bytes, true, FileKind::Sgs)?;
    if trailing.is_empty() {
        Ok(out)
    } else {
        Err(Error::Trailing { offset: bytes.len() - trailing.len(), len: trailing.len() })
    }
}

/// Decodes every record of a scenery model in file order, keeping any
/// trailing non-record bytes in [`Sgs::trailing`].
pub fn parse(bytes: &[u8]) -> Result<Sgs<'_>, Error> {
    let (recs, trailing) = sg_framing::split_records(bytes, true, FileKind::Sgs)?;
    Ok(Sgs { elements: recs.iter().map(element).collect::<Result<_, _>>()?, trailing })
}

/// The LocoMotion node table of a model in this framing, a car `.sgc` or a scenery `.sgs`: the nodes of every kind 18
/// record in file order, as `CLM_CharacterLoader::LoadHierarchy` (`0x10015280`) reads them for its character. The
/// other records are not decoded.
pub fn node_table(bytes: &[u8]) -> Result<Hierarchy, Error> {
    let mut nodes = Vec::new();
    for rec in sg_framing::split_records(bytes, true, FileKind::Sgs)?.0.iter().filter(|rec| rec.kind == KIND_HIERARCHY)
    {
        let mut r = Reader::new(rec.body, rec.offset);
        nodes.extend(hierarchy(&mut r)?.nodes);
        r.finish()?;
    }
    Ok(Hierarchy { nodes })
}

fn element(rec: &Record<'_>) -> Result<Element, Error> {
    let mut r = Reader::new(rec.body, rec.offset);
    let (kind, subtype) = (rec.kind, rec.subtype);
    let out = match (kind, subtype) {
        (KIND_GEOMETRY, SUBTYPE_STATIC) => Element::StaticMesh(static_mesh(&mut r)?),
        (KIND_GEOMETRY, SUBTYPE_SKINNED) => Element::SkinnedMesh(skinned_mesh(&mut r)?),
        (KIND_NODE, SUBTYPE_HELPER) => Element::Helper(node(&mut r)?),
        (KIND_NODE, SUBTYPE_BONE) => Element::Bone(node(&mut r)?),
        (KIND_IK, 0) => Element::IkNode(node(&mut r)?),
        (KIND_LIGHT, 10) => Element::Light(light(&mut r)?),
        (KIND_HIERARCHY, 0) => Element::Hierarchy(hierarchy(&mut r)?),
        (KIND_CAMERA, SUBTYPE_CAMERA) => {
            let (name, id, parent) = (r.cstr()?, r.i32()?, r.i32()?);
            // transform (56), near/far/fov (12), world (48), int (4), vec3 (12).
            r.skip(132)?;
            Element::Unparsed { kind, subtype, name, id, parent }
        }
        (KIND_POINT, 0) => {
            let (name, id, parent) = (r.cstr()?, r.i32()?, r.i32()?);
            // `FUN_1000c550`: transform (56) and world (48).
            r.skip(104)?;
            Element::Unparsed { kind, subtype, name, id, parent }
        }
        (KIND_GEOMETRY | KIND_LIGHT | KIND_CAMERA | KIND_NODE | KIND_IK | KIND_POINT, _) => {
            return Err(Error::UnknownSubtype { offset: rec.offset, kind, subtype })
        }
        _ => return Ok(Element::Ignored { kind, subtype }),
    };
    r.finish()?;
    Ok(out)
}

/// Common node header: name, ids, transform and world matrix. Same layout
/// as [`crate::sgc`], section 4 (the H7 readers are shared by both loaders).
fn node(r: &mut Reader<'_>) -> Result<crate::sgc::Node, Error> {
    let name = r.cstr()?;
    Ok(crate::sgc::Node {
        name,
        id: r.i32()?,
        parent: r.i32()?,
        transform: crate::sgc::Transform {
            scale: r.vec3()?,
            axis: r.vec3()?,
            angle: r.f32()?,
            translation: r.vec3()?,
            axis2: r.vec3()?,
            angle2: r.f32()?,
        },
        world: [r.vec4()?, r.vec4()?, r.vec4()?],
    })
}

/// Static mesh body (subtype 1): the [`crate::sgc`] mesh without the morph
/// block and morph lists. The generic loader (`0x1000b490`) returns after
/// the faces for a non-deformable object.
fn static_mesh(r: &mut Reader<'_>) -> Result<crate::sgc::Mesh, Error> {
    let node = node(r)?;
    let (centre, half_extent) = (r.vec3()?, r.vec3()?);
    let vertex_count = r.count()?;
    let stream_flags = r.u32()?;

    let vertices = if stream_flags & crate::sgc::STREAM_VERTICES != 0 {
        r.items(vertex_count, 24, |c| Ok(crate::sgc::Vertex { position: c.vec3()?, normal: c.vec3()? }))?
    } else {
        Vec::new()
    };
    let uvs =
        if stream_flags & crate::sgc::STREAM_UVS != 0 { r.items(vertex_count, 8, |c| c.vec2())? } else { Vec::new() };
    let aux = if stream_flags & crate::sgc::STREAM_AUX != 0 {
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
        faces.push(crate::sgc::Face {
            name: r.cstr()?,
            prim_count: r.count_u32()?,
            min_vertex: r.count_u32()?,
            num_vertices: r.count_u32()?,
            start_index: r.count_u32()?,
        });
    }

    Ok(crate::sgc::Mesh {
        node,
        damageable: false,
        centre,
        half_extent,
        stream_flags,
        vertices,
        uvs,
        aux,
        triangles,
        faces,
        morph_targets: Vec::new(),
        morph_lists: Vec::new(),
    })
}

/// Skinned mesh body (subtype 2): min/max bounds, H7 vertex streams, the
/// triangle list and faces, then the skin block. Follows `FUN_1000b490`
/// (`HH_H7LoadGeom` in `src/hedgehog3d/HH_3DSMaxLoaderGeom.cpp`).
fn skinned_mesh(r: &mut Reader<'_>) -> Result<SkinnedMesh, Error> {
    let node = node(r)?;
    let (min, max) = (r.vec3()?, r.vec3()?);
    let vertex_count = r.count()?;
    let stream_flags = r.u32()?;

    let vertices = if stream_flags & STREAM_VERTICES != 0 {
        r.items(vertex_count, 24, |c| Ok(crate::sgc::Vertex { position: c.vec3()?, normal: c.vec3()? }))?
    } else {
        Vec::new()
    };
    let uvs = if stream_flags & STREAM_UVS != 0 {
        r.items(vertex_count, 8, |c| Ok([c.f32()?, c.f32()?]))?
    } else {
        Vec::new()
    };
    let extra = if stream_flags & STREAM_EXTRA != 0 {
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
        faces.push(crate::sgc::Face {
            name: r.cstr()?,
            prim_count: r.count_u32()?,
            min_vertex: r.count_u32()?,
            num_vertices: r.count_u32()?,
            start_index: r.count_u32()?,
        });
    }

    let skin = skin_block(r)?;

    Ok(SkinnedMesh { node, min, max, stream_flags, vertices, uvs, extra, triangles, faces, skin })
}

/// Skin block (`HH_H7LoadSkin`, `0x1000bb50`): head matrix, bone count, per
/// bone a name and matrix, then a count with that many 32-byte per-vertex
/// records running to the body end.
fn skin_block(r: &mut Reader<'_>) -> Result<Skin, Error> {
    let head = [r.vec4()?, r.vec4()?, r.vec4()?];
    let bone_count = r.count()?;
    let mut bones = Vec::new();
    for _ in 0..bone_count {
        let name = r.cstr()?;
        bones.push(SkinBone { name, matrix: [r.vec4()?, r.vec4()?, r.vec4()?] });
    }
    let weight_count = r.count()?;
    let weights = r.items(weight_count, 32, |c| {
        Ok(SkinWeight {
            weights: [c.f32()?, c.f32()?, c.f32()?, c.f32()?],
            offsets: [c.i32()?, c.i32()?, c.i32()?, c.i32()?],
        })
    })?;
    Ok(Skin { head, bones, weight_count: weight_count as u32, weights })
}

/// Omni light body (subtype 10): node header, color, world matrix and the
/// enable flag (`HH_H7LoadLight`, `0x1000c090`). A nonzero flag would be
/// followed by two more floats; retail files all store 0.
fn light(r: &mut Reader<'_>) -> Result<Light, Error> {
    let name = r.cstr()?;
    let (id, parent) = (r.i32()?, r.i32()?);
    let transform = crate::sgc::Transform {
        scale: r.vec3()?,
        axis: r.vec3()?,
        angle: r.f32()?,
        translation: r.vec3()?,
        axis2: r.vec3()?,
        angle2: r.f32()?,
    };
    // Unlike the node header, the world matrix follows the color.
    let color = r.vec3()?;
    let world = [r.vec4()?, r.vec4()?, r.vec4()?];
    let enabled = r.u32()?;
    if enabled != 0 {
        r.skip(8)?;
    }
    Ok(Light { node: crate::sgc::Node { name, id, parent, transform, world }, color, enabled })
}

/// Hierarchy table (kind 18): a node count and that many entries
/// (`CLM_CharacterLoader::LoadNodes`, `0x10014c20`).
fn hierarchy(r: &mut Reader<'_>) -> Result<Hierarchy, Error> {
    let count = r.u32()? as usize;
    let mut nodes = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let header_len = r.u32()?;
        if header_len != 12 {
            return Err(Error::BadBody { offset: r.offset, what: "hierarchy entry header is not 12 bytes" });
        }
        let _kind = r.u32()?;
        let node_type = r.u32()?;
        let _third = r.u32()?;
        let _skip = r.u32()?;
        let name = r.cstr()?;
        let id = r.i32()?;
        let parent = r.i32()?;
        let world = [r.vec4()?, r.vec4()?, r.vec4()?];
        let sub = r.u32()?;
        nodes.push(HierarchyNode { node_type, name, id, parent, world, sub });
    }
    Ok(Hierarchy { nodes })
}

type Reader<'a> = sg_framing::Reader<'a>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sg_framing::RECORD_HEADER_LEN;

    /// A file from `(kind, subtype, body)` records.
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
        b.extend(f32s(&[0.0; 14]));
        b.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        b
    }

    #[test]
    fn skinned_mesh_reads_streams_faces_and_skin() {
        let mut body = node_body("Body", 96, -1);
        body.extend(f32s(&[-1.0, -1.0, -1.0, 1.0, 1.0, 1.0]));
        body.extend_from_slice(&2i32.to_le_bytes());
        body.extend_from_slice(&0x17u32.to_le_bytes());
        body.extend(f32s(&[0.0, 0.0, 0.0, 0.0, 0.0, 1.0]));
        body.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0]));
        body.extend(f32s(&[0.25, 0.75, 0.5, 1.0]));
        body.extend_from_slice(&[9u8; 4]);
        body.extend_from_slice(&[9u8; 4]);
        body.extend_from_slice(&1i32.to_le_bytes());
        body.extend_from_slice(&[0u16, 0, 0].map(|v: u16| v.to_le_bytes()).concat());
        body.extend_from_slice(&1i32.to_le_bytes());
        body.extend_from_slice(b"skin\0");
        for word in [1i32, 0, 1, 0] {
            body.extend_from_slice(&word.to_le_bytes());
        }
        body.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        body.extend_from_slice(&1i32.to_le_bytes());
        body.extend_from_slice(b"pelvis\0");
        body.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        body.extend_from_slice(&1i32.to_le_bytes());
        body.extend(f32s(&[0.75, 0.25, 0.0, 0.0]));
        for word in [0i32, 0x40, NO_BONE, NO_BONE] {
            body.extend_from_slice(&word.to_le_bytes());
        }
        let bytes = file(&[(KIND_GEOMETRY, SUBTYPE_SKINNED, body)]);
        let parsed = parse(&bytes).expect("parse");
        assert!(parsed.trailing.is_empty());
        let Element::SkinnedMesh(mesh) = &parsed.elements[0] else {
            panic!("expected skinned mesh");
        };
        assert_eq!(mesh.node.name, "Body");
        assert_eq!(mesh.min, [-1.0; 3]);
        assert_eq!(mesh.vertices.len(), 2);
        assert_eq!(mesh.uvs, vec![[0.25, 0.75], [0.5, 1.0]]);
        assert_eq!(mesh.faces[0].name, "skin");
        assert_eq!(mesh.skin.bones[0].name, "pelvis");
        assert_eq!(mesh.skin.weight_count, 1);
        assert_eq!(mesh.skin.weights[0].bones().collect::<Vec<_>>(), [(Some(0), 0.75), (Some(1), 0.25)]);
    }

    #[test]
    fn light_hierarchy_and_ik_nodes() {
        // Light order: node ids and transform, then color, world, flag.
        let mut light_body = b"Omni01\0".to_vec();
        light_body.extend_from_slice(&65i32.to_le_bytes());
        light_body.extend_from_slice(&(-1i32).to_le_bytes());
        light_body.extend(f32s(&[0.0; 14]));
        light_body.extend(f32s(&[1.0, 1.0, 1.0]));
        light_body.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        light_body.extend_from_slice(&0u32.to_le_bytes());
        let mut table = 1u32.to_le_bytes().to_vec();
        table.extend_from_slice(&12u32.to_le_bytes());
        table.extend_from_slice(&[19u32, 47, 1].map(|v| v.to_le_bytes()).concat());
        table.extend_from_slice(&74u32.to_le_bytes());
        table.extend_from_slice(b"D_mother\0");
        table.extend_from_slice(&1299i32.to_le_bytes());
        table.extend_from_slice(&(-1i32).to_le_bytes());
        table.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        table.extend_from_slice(&3u32.to_le_bytes());
        let bytes = file(&[
            (KIND_LIGHT, 10, light_body),
            (KIND_HIERARCHY, 0, table),
            (KIND_IK, 0, node_body("IK Chain03", 106, 75)),
            (5, 18, b"Default Material\0".to_vec()),
        ]);
        let parsed = parse(&bytes).expect("parse");
        assert!(parsed.trailing.is_empty());
        let elements = parsed.elements;
        let Element::Light(light) = &elements[0] else {
            panic!("expected light");
        };
        assert_eq!(light.color, [1.0; 3]);
        assert_eq!(light.enabled, 0);
        let Element::Hierarchy(h) = &elements[1] else {
            panic!("expected hierarchy");
        };
        assert_eq!(h.nodes[0].name, "D_mother");
        assert_eq!(h.nodes[0].node_type, 47);
        assert!(matches!(&elements[2], Element::IkNode(n) if n.name == "IK Chain03"));
        assert!(matches!(&elements[3], Element::Ignored { kind: 5, .. }));
        assert_eq!(node_table(&bytes).expect("node table"), *h);
    }

    #[test]
    fn trailing_bytes_are_kept_verbatim() {
        let mut bytes = file(&[(KIND_IK, 0, node_body("IK Chain03", 106, 75))]);
        // A full 20-byte prefix whose first word is not 12 starts trailing.
        let tail = [
            0x44, 0xb0, 0x42, 0x41, 0x11, 0x4c, 0x85, 0x42, 0x56, 0xce, 0x83, 0xc0, 0xe0, 0xb5, 0x3c, 0x41, 0x2a, 0xbb,
            0x50, 0x42, 0x49, 0x11, 0xd7, 0x40,
        ];
        bytes.extend_from_slice(&tail);
        let parsed = parse(&bytes).expect("parse");
        assert!(matches!(&parsed.elements[0], Element::IkNode(_)));
        assert_eq!(parsed.trailing, &tail);
        assert!(matches!(records(&bytes), Err(Error::Trailing { .. })));
    }

    #[test]
    fn damaged_files_never_panic() {
        let mut body = node_body("Body", 1, -1);
        body.extend(f32s(&[0.0; 6]));
        body.extend_from_slice(&1i32.to_le_bytes());
        body.extend_from_slice(&1u32.to_le_bytes());
        body.extend(f32s(&[0.0; 6]));
        body.extend_from_slice(&0i32.to_le_bytes());
        body.extend_from_slice(&0i32.to_le_bytes());
        body.extend_from_slice(&0i32.to_le_bytes());
        let bytes = file(&[(KIND_GEOMETRY, SUBTYPE_SKINNED, body)]);
        crate::damage::never_panics(&bytes, &[0, 0x7f, 0xff], |b| {
            let _ = parse(b);
        });
    }
}
