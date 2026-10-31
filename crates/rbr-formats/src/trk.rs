//! Reader for `.trk` physics files.
//!
//! `CPC_TrackState::LoadPhysics` (`FUN_0046ed20`) walks a framed stream:
//!
//! ```text
//! repeat to EOF:
//!   u32 header_len
//!   u8  header[header_len]
//!   u32 payload_len
//!   u8  payload[payload_len]
//! ```
//!
//! The retail 1.0 stage files contain two chunk headers, both with type 3:
//! kind `0x14` is read by `E09_LoadDriveLine` (`FUN_0046a530`), and kind
//! `0x16` by `CPC_TrackLoaderDaemon::LoadCollisionMeshes` (`FUN_0046e300`).
//! Other chunk payloads remain opaque. The game seeks to the declared end of
//! each chunk after a handler returns, so any unread payload suffix is kept.

use crate::le::{self, Reader, Truncated};
use std::fmt::{self, Display, Formatter};

/// A 3D vector read from a loader stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// One record from kind `0x14`. The loader (`E09_LoadDriveLine`,
/// `FUN_0046a530`) reads two vectors, one float and two words, storing the
/// first word at element `+0xe` and the second at `+0xc`. The two words are
/// zero in every retail record; no consumer of them is traced.
#[derive(Debug, Clone, PartialEq)]
pub struct DriveLineRecord {
    pub vectors: [Vec3; 2],
    pub value: f32,
    pub unknown_u16: [u16; 2],
}

/// Parsed kind `0x14` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct DriveLineData {
    pub records: Vec<DriveLineRecord>,
    pub ignored_tail: Vec<u8>,
}

/// The three shape encodings read by the kind `0x16` loader.
#[derive(Debug, Clone, PartialEq)]
pub enum CollisionShape {
    None,
    Bounds {
        min: Vec3,
        max: Vec3,
    },
    Sphere {
        center: Vec3,
        radius: f32,
    },
    /// The loader consumes no additional bytes for other shape values.
    Unknown {
        kind: u32,
    },
}

/// A triangle-like record. The loader reads three words but stores only each
/// word's low byte in its runtime record; the words are kept here verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionFace {
    pub tag: u8,
    pub indices: [u32; 3],
}

/// One placement record after the per-mesh geometry. `interactive_id` and
/// `values` are preserved without assigning meanings not established by the
/// loader at `0x46e300`.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionObject {
    pub interactive_id: u32,
    pub position: Vec3,
    pub extent: Vec3,
    pub values: [f32; 4],
}

/// One named collision mesh or surface record from kind `0x16`.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionMesh {
    /// NUL-terminated string bytes, excluding the terminator.
    pub name: Vec<u8>,
    /// Raw loader `kind`; the game maps several values to `CSigStuff` types.
    pub kind: u32,
    /// Byte stored at mesh `+0x30` by `FUN_0046e300`. No consumer of it is
    /// traced in the loader; retail values match the `.col` surface-id
    /// space (bushes, trees, rocks, poles, spectator tags), but that use is
    /// unproven, so the byte stays raw here.
    pub unknown_byte: u8,
    /// Static/dynamic byte (`FUN_0046e300`): zero is static, nonzero is
    /// dynamic. A static mesh shares one collision id (`baseId`) across
    /// all its placements; a dynamic mesh's placements each point at the
    /// mesh itself (with `0x30` ORed into the placement flags at `+0x44`).
    /// Retail uses only 0 and 1, and 1 only on kind-0 meshes whose material
    /// byte above is 0.
    pub static_flag: u8,
    pub shape: CollisionShape,
    /// The source count is a u32, but the loader truncates it to u16 before
    /// iterating. Keep the original count word and the records actually read.
    pub vertex_count_word: u32,
    pub vertices: Vec<Vec3>,
    pub face_count_word: u32,
    pub faces: Vec<CollisionFace>,
    pub objects: Vec<CollisionObject>,
}

impl CollisionMesh {
    /// Whether this mesh is static (`static_flag == 0`, sharing one
    /// collision id) or dynamic (per-placement mesh pointers).
    pub fn is_static(&self) -> bool {
        self.static_flag == 0
    }

    /// Runtime `CSigStuff` type selected for this mesh kind by `0x46e300`.
    pub fn game_object_type(&self) -> u32 {
        match self.kind {
            0 | 5 => 1,
            1 => 0x19,
            2 => 0x18,
            3 => 0x0d,
            6 => 0x15,
            7 => 0x1c,
            8 => 0x1e,
            9 => 0x1f,
            10 => 0x20,
            // Kind 15 falls in the default branch (type 1), like kind 0/5.
            // Retail has 12 kind-15 meshes, all the same static sphere bush
            // (`HO_BUSH01.KSH` on the Hokkaido stages), so this is live data.
            _ => 1,
        }
    }
}

/// Parsed kind `0x16` payload.
#[derive(Debug, Clone, PartialEq)]
pub struct CollisionMeshData {
    pub meshes: Vec<CollisionMesh>,
    pub ignored_tail: Vec<u8>,
}

/// Parsed contents for recognized chunk kinds; unrecognized chunks stay raw.
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkPayload {
    DriveLine(DriveLineData),
    CollisionMeshes(CollisionMeshData),
    Opaque,
}

/// One framed chunk: raw header and payload, plus a loader-backed parse for
/// the two known physics kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub header: Vec<u8>,
    pub data: Vec<u8>,
    pub payload: ChunkPayload,
}

impl Chunk {
    /// Decodes the header as little-endian u32 words, if its length is a
    /// multiple of 4.
    pub fn header_u32s(&self) -> Option<Vec<u32>> {
        if !self.header.len().is_multiple_of(4) {
            return None;
        }
        let mut out = Vec::with_capacity(self.header.len() / 4);
        for w in self.header.chunks_exact(4) {
            let mut buf = [0u8; 4];
            buf.copy_from_slice(w);
            out.push(u32::from_le_bytes(buf));
        }
        Some(out)
    }

    /// The observed 8-byte header as `(type, kind)`, e.g. `(3, 0x14)`.
    pub fn type_kind(&self) -> Option<(u32, u32)> {
        if self.header.len() != 8 {
            return None;
        }
        let words = self.header_u32s()?;
        Some((words[0], words[1]))
    }
}

/// A parsed `.trk` file: its chunks in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct TrkFile {
    pub chunks: Vec<Chunk>,
}

/// Parse failure with the offending byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated { offset: usize, needed: usize, len: usize },
    MissingNul { offset: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed, len } => le::fmt_truncated(f, *offset, *needed, *len),
            Error::MissingNul { offset } => {
                write!(f, "missing NUL terminator for string at offset {offset}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<Truncated> for Error {
    fn from(t: Truncated) -> Self {
        Error::Truncated { offset: t.offset, needed: t.needed, len: t.len }
    }
}

fn vec3(rd: &mut Reader<'_, Error>) -> Result<Vec3, Error> {
    Ok(Vec3 { x: rd.f32()?, y: rd.f32()?, z: rd.f32()? })
}

fn c_string(rd: &mut Reader<'_, Error>) -> Result<Vec<u8>, Error> {
    let start = rd.pos();
    rd.take_cstr().map(|b| b.to_vec()).map_err(|_| Error::MissingNul { offset: start })
}

fn parse_driveline(bytes: &[u8]) -> Result<DriveLineData, Error> {
    let mut rd = Reader::new(bytes);
    let count = rd.u32()? as usize;
    rd.require(count, 32)?;
    let mut records = Vec::new();
    records.try_reserve_exact(count).map_err(|_| Error::Truncated {
        offset: rd.pos(),
        needed: count.saturating_mul(32),
        len: bytes.len(),
    })?;
    for _ in 0..count {
        records.push(DriveLineRecord {
            vectors: [vec3(&mut rd)?, vec3(&mut rd)?],
            value: rd.f32()?,
            unknown_u16: [rd.u16()?, rd.u16()?],
        });
    }
    let ignored_tail = bytes[rd.pos()..].to_vec();
    Ok(DriveLineData { records, ignored_tail })
}

fn parse_collision_meshes(bytes: &[u8]) -> Result<CollisionMeshData, Error> {
    let mut rd = Reader::new(bytes);
    let count = rd.u32()? as usize;
    let mut meshes = Vec::new();
    for _ in 0..count {
        let name = c_string(&mut rd)?;
        let kind = rd.u32()?;
        let unknown_byte = rd.u8()?;
        let static_flag = rd.u8()?;
        let shape_kind = rd.u32()?;
        let shape = match shape_kind {
            0 => CollisionShape::None,
            1 => CollisionShape::Bounds { min: vec3(&mut rd)?, max: vec3(&mut rd)? },
            2 => CollisionShape::Sphere { center: vec3(&mut rd)?, radius: rd.f32()? },
            kind => CollisionShape::Unknown { kind },
        };

        let vertex_count_word = rd.u32()?;
        let vertex_count = (vertex_count_word as u16) as usize;
        rd.require(vertex_count, 12)?;
        let mut vertices = Vec::new();
        vertices.try_reserve_exact(vertex_count).map_err(|_| Error::Truncated {
            offset: rd.pos(),
            needed: vertex_count.saturating_mul(12),
            len: bytes.len(),
        })?;
        for _ in 0..vertex_count {
            vertices.push(vec3(&mut rd)?);
        }

        let face_count_word = rd.u32()?;
        let face_count = (face_count_word as u16) as usize;
        rd.require(face_count, 13)?;
        let mut faces = Vec::new();
        faces.try_reserve_exact(face_count).map_err(|_| Error::Truncated {
            offset: rd.pos(),
            needed: face_count.saturating_mul(13),
            len: bytes.len(),
        })?;
        for _ in 0..face_count {
            faces.push(CollisionFace { tag: rd.u8()?, indices: [rd.u32()?, rd.u32()?, rd.u32()?] });
        }

        let object_count = rd.u32()? as usize;
        rd.require(object_count, 44)?;
        let mut objects = Vec::new();
        objects.try_reserve_exact(object_count).map_err(|_| Error::Truncated {
            offset: rd.pos(),
            needed: object_count.saturating_mul(44),
            len: bytes.len(),
        })?;
        for _ in 0..object_count {
            let interactive_id = rd.u32()?;
            let position = vec3(&mut rd)?;
            let extent = vec3(&mut rd)?;
            let values = [rd.f32()?, rd.f32()?, rd.f32()?, rd.f32()?];
            objects.push(CollisionObject { interactive_id, position, extent, values });
        }

        meshes.push(CollisionMesh {
            name,
            kind,
            unknown_byte,
            static_flag,
            shape,
            vertex_count_word,
            vertices,
            face_count_word,
            faces,
            objects,
        });
    }
    let ignored_tail = bytes[rd.pos()..].to_vec();
    Ok(CollisionMeshData { meshes, ignored_tail })
}

/// Parses the outer chunks and all recognized 1.02 `.trk` payloads.
pub fn parse(bytes: &[u8]) -> Result<TrkFile, Error> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    let len = bytes.len();

    while offset < len {
        let header_len = le::read_u32_le(bytes, offset)? as usize;
        offset += 4;

        let header_end = offset.checked_add(header_len).ok_or(Error::Truncated { offset, needed: header_len, len })?;
        if header_end > len {
            return Err(Error::Truncated { offset, needed: header_len, len });
        }
        let header = bytes[offset..header_end].to_vec();
        offset = header_end;

        let payload_len = le::read_u32_le(bytes, offset)? as usize;
        offset += 4;

        let data_end = offset.checked_add(payload_len).ok_or(Error::Truncated { offset, needed: payload_len, len })?;
        if data_end > len {
            return Err(Error::Truncated { offset, needed: payload_len, len });
        }
        let data = bytes[offset..data_end].to_vec();
        offset = data_end;

        let payload = match (header.len(), le::u32_at(&header, 0), le::u32_at(&header, 4)) {
            (8, Some(3), Some(0x14)) => ChunkPayload::DriveLine(parse_driveline(&data)?),
            (8, Some(3), Some(0x16)) => ChunkPayload::CollisionMeshes(parse_collision_meshes(&data)?),
            _ => ChunkPayload::Opaque,
        };
        chunks.push(Chunk { header, data, payload });
    }

    Ok(TrkFile { chunks })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_chunk(header: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&(header.len() as u32).to_le_bytes());
        b.extend_from_slice(header);
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b.extend_from_slice(payload);
        b
    }

    fn header_worded(t: u32, k: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&t.to_le_bytes());
        b.extend_from_slice(&k.to_le_bytes());
        b
    }

    fn put_u32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn put_f32(out: &mut Vec<u8>, v: f32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn collision_payload() -> Vec<u8> {
        let mut b = Vec::new();
        put_u32(&mut b, 1); // mesh count
        b.extend_from_slice(b"road\0");
        put_u32(&mut b, 1); // raw kind
        b.push(0x55); // unknown byte between kind and static flag
        b.push(0); // static
        put_u32(&mut b, 1); // bounding box
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            put_f32(&mut b, v);
        }
        put_u32(&mut b, 1); // vertex count
        for v in [7.0, 8.0, 9.0] {
            put_f32(&mut b, v);
        }
        put_u32(&mut b, 1); // face count
        b.push(0xaa);
        for v in [2, 3, 4] {
            put_u32(&mut b, v);
        }
        put_u32(&mut b, 1); // placement count
        put_u32(&mut b, 99); // interactive id
        for v in 10..=15 {
            put_f32(&mut b, v as f32);
        }
        for v in 16..=19 {
            put_f32(&mut b, v as f32);
        }
        b
    }

    #[test]
    fn empty_is_zero_chunks() {
        let file = parse(&[]).unwrap();
        assert!(file.chunks.is_empty());
    }

    #[test]
    fn drive_line_payload_is_decoded() {
        let mut p = Vec::new();
        put_u32(&mut p, 1);
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0] {
            put_f32(&mut p, v);
        }
        p.extend_from_slice(&[0x34, 0x12, 0x78, 0x56]);
        p.push(0xee); // the game seeks to the declared chunk end
        let file = parse(&encode_chunk(&header_worded(3, 0x14), &p)).unwrap();
        let ChunkPayload::DriveLine(decoded) = &file.chunks[0].payload else { panic!("expected driveline payload") };
        assert_eq!(decoded.records.len(), 1);
        assert_eq!(decoded.records[0].vectors[0].x, 1.0);
        assert_eq!(decoded.records[0].vectors[1].z, 6.0);
        assert_eq!(decoded.records[0].value, 7.0);
        assert_eq!(decoded.records[0].unknown_u16, [0x1234, 0x5678]);
        assert_eq!(decoded.ignored_tail, [0xee]);
    }

    #[test]
    fn collision_mesh_payload_is_decoded() {
        let file = parse(&encode_chunk(&header_worded(3, 0x16), &collision_payload())).unwrap();
        let ChunkPayload::CollisionMeshes(decoded) = &file.chunks[0].payload else {
            panic!("expected collision mesh payload")
        };
        assert_eq!(decoded.meshes.len(), 1);
        let mesh = &decoded.meshes[0];
        assert_eq!(mesh.name, b"road");
        assert_eq!(mesh.kind, 1);
        assert_eq!(mesh.unknown_byte, 0x55);
        assert_eq!(mesh.game_object_type(), 0x19);
        assert!(matches!(mesh.shape, CollisionShape::Bounds { .. }));
        assert_eq!(mesh.vertices.len(), 1);
        assert_eq!(mesh.faces[0].indices, [2, 3, 4]);
        assert_eq!(mesh.objects[0].interactive_id, 99);
    }

    /// The `CSigStuff` type switch in `FUN_0046e300`, all branches.
    #[test]
    fn game_object_type_matches_loader_switch() {
        fn typed(kind: u32) -> u32 {
            CollisionMesh {
                name: Vec::new(),
                kind,
                unknown_byte: 0,
                static_flag: 0,
                shape: CollisionShape::None,
                vertex_count_word: 0,
                vertices: Vec::new(),
                face_count_word: 0,
                faces: Vec::new(),
                objects: Vec::new(),
            }
            .game_object_type()
        }
        let cases = [
            (0, 1),
            (1, 0x19),
            (2, 0x18),
            (3, 0x0d),
            (4, 1),
            (5, 1),
            (6, 0x15),
            (7, 0x1c),
            (8, 0x1e),
            (9, 0x1f),
            (10, 0x20),
            (11, 1),
            (15, 1),
            (99, 1),
        ];
        for (kind, want) in cases {
            assert_eq!(typed(kind), want, "mesh kind {kind}");
        }
    }

    #[test]
    fn two_known_chunks_round_trip() {
        let mut p = Vec::new();
        put_u32(&mut p, 0);
        let mut b = encode_chunk(&header_worded(3, 0x14), &p);
        b.extend(encode_chunk(&header_worded(3, 0x16), &0u32.to_le_bytes()));
        let file = parse(&b).unwrap();
        assert_eq!(file.chunks.len(), 2);
        assert_eq!(file.chunks[0].type_kind(), Some((3, 0x14)));
        assert_eq!(file.chunks[1].type_kind(), Some((3, 0x16)));
        assert!(matches!(file.chunks[0].payload, ChunkPayload::DriveLine(_)));
        assert!(matches!(file.chunks[1].payload, ChunkPayload::CollisionMeshes(_)));
    }

    #[test]
    fn unknown_chunk_payload_is_preserved() {
        let file = parse(&encode_chunk(&header_worded(9, 99), &[1, 2, 3])).unwrap();
        assert_eq!(file.chunks[0].payload, ChunkPayload::Opaque);
        assert_eq!(file.chunks[0].data, vec![1, 2, 3]);
    }

    #[test]
    fn zero_length_header_and_payload() {
        let b = encode_chunk(&[], &[]);
        let file = parse(&b).unwrap();
        assert_eq!(file.chunks.len(), 1);
        assert!(file.chunks[0].header.is_empty());
        assert!(file.chunks[0].data.is_empty());
        assert_eq!(file.chunks[0].type_kind(), None);
    }

    #[test]
    fn truncated_header_len() {
        assert!(matches!(parse(&[1, 2]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn truncated_header_bytes() {
        let mut b = Vec::new();
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&[3, 0, 0]);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn truncated_payload_len() {
        let mut b = encode_chunk(&header_worded(9, 0), &[1]);
        b.truncate(b.len() - 2);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn truncated_payload_bytes() {
        let mut b = Vec::new();
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend(header_worded(9, 0));
        b.extend_from_slice(&100u32.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3]);
        assert!(matches!(parse(&b), Err(Error::Truncated { needed: 100, .. })));
    }

    #[test]
    fn truncated_known_payload_is_rejected() {
        assert!(matches!(parse(&encode_chunk(&header_worded(3, 0x14), &[1, 2, 3, 4])), Err(Error::Truncated { .. })));
    }

    #[test]
    fn huge_lengths_do_not_oom() {
        let mut b = Vec::new();
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));

        let mut c = Vec::new();
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(parse(&c), Err(Error::Truncated { .. })));
    }
}
