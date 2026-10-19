//! Reader for `.fnc` fence data.
//!
//! Layout (little-endian) from `CFenceHandler::Read` (`FUN_006038c0` in
//! the official 1.02 exe):
//!
//! ```text
//! u32 magic  (must be 2, checked against the global at 0x74f858)
//! u32 group_count
//! repeat group_count:
//!   u32 post_count
//!   u32 fence_type, post_type, material, rail_material
//!   f32 center[3], half_extent[3]
//!   repeat post_count:
//!     f32 position[3], center[3], half_extent[3]
//!     u32 color (packed BGRA, written to the vertex color slot)
//! if group_count != 0:
//!   u32 fence_texture_count
//!   if fence_texture_count != 0:
//!     u32 texture_name_bytes
//!     repeat fence_texture_count:
//!       u8 name[texture_name_bytes] (fixed-width, NUL-padded on retail)
//! ```
//!
//! Field meanings are loader-backed (`src/rbr/RBR_E90_Handler.cpp`,
//! `RBR_E90_Renderer.cpp`): the group words are the fence/post type and
//! material indices plus the group bounds, and each post is a position,
//! bounds to the next post, and a packed color word. File strides are 44
//! bytes per group header (4 + 16 + 24) and 40 bytes per post (36 + 4).
//! The larger in-memory stride (`0xb4`) is the linked post struct, not
//! the file.
//!
//! The group reader is `FUN_006038c0`; its `FUN_006058c0` tail reader loads
//! fence textures from `Textures\\<name>.dds`. The group loader only calls
//! that tail reader when the group count is nonzero. The parser preserves any
//! bytes the game leaves unread in `ignored_tail` (notably the 8-byte tail on
//! zero-group retail files). Counts and byte lengths are bounds-checked before
//! allocation.

use std::fmt::{self, Display, Formatter};

/// Magic at the start of every `.fnc` file.
pub const MAGIC: u32 = 2;
/// Group header file bytes after the post count: 4xu32 + 6xf32.
pub const GROUP_HEADER_LEN: usize = 40;
/// Post file bytes: 9xf32 + 1xu32.
pub const POST_LEN: usize = 40;

/// One fence post: position, per-post bounds, and a color word.
///
/// The loader reads three triples into `m_pos`, `m_center`, `m_half` and
/// the word into `m_unkAC` (`CDummyObject`, `RBR_E90.h`). The renderer
/// (`EmitPosts`/`EmitRails`) writes the position into emitted vertices and
/// the word into the vertex color slot, so the word is a packed BGRA color.
/// Posts are linked prev/next in load order; the bounds cover one post to
/// the next (used for box culling in `CFenceHandler`).
#[derive(Debug, Clone, PartialEq)]
pub struct FncPost {
    /// Post position (`m_pos`, file floats 0..3).
    pub position: [f32; 3],
    /// Bounds center (`m_center`, file floats 3..6).
    pub center: [f32; 3],
    /// Bounds half-extent (`m_half`, file floats 6..9).
    pub half_extent: [f32; 3],
    /// Packed color (`m_unkAC`), e.g. `0xffffffff`; retail is opaque greys
    /// and fence colors.
    pub color: u32,
}

/// One fence group with its posts.
#[derive(Debug, Clone, PartialEq)]
pub struct FncGroup {
    /// Fence type index into the fence-type table (`m_type`; selects layer
    /// count, height and UVs in `EmitPosts`). Retail: 0, 1, 2, 3, 12, 13.
    pub fence_type: u32,
    /// Post mesh type index (`m_postType`; selects the post mesh in
    /// `EmitRails`). Retail: 4, 5, 14.
    pub post_type: u32,
    /// Fence fabric material index (`m_material`; `EmitPosts` emits one
    /// draw per material). Retail: 0, 1, 2, 3, 5.
    pub material: u32,
    /// Rail/post material index (`m_unk38`; `EmitRails` matches on it).
    /// Retail: 0, 1, 2, 3, 4.
    pub rail_material: u32,
    /// Group bounds center (`m_center`, box culling).
    pub center: [f32; 3],
    /// Group bounds half-extent (`m_half`).
    pub half_extent: [f32; 3],
    pub posts: Vec<FncPost>,
}

/// A parsed `.fnc` file.
#[derive(Debug, Clone, PartialEq)]
pub struct FncFile {
    pub groups: Vec<FncGroup>,
    /// Fixed-width texture-name bytes passed to the fence texture loader.
    pub textures: Vec<Vec<u8>>,
    /// Bytes after data consumed by the retail loader.
    pub ignored_tail: Vec<u8>,
}

/// Parse failure with the offending byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated { offset: usize, needed: usize, len: usize },
    BadMagic { found: u32 },
    Allocation { offset: usize, count: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed, len } => {
                write!(f, "truncated at offset {offset}: need {needed} bytes but file is {len} bytes")
            }
            Error::BadMagic { found } => {
                write!(f, "bad magic: found {found}, expected {}", MAGIC)
            }
            Error::Allocation { offset, count } => {
                write!(f, "could not allocate {count} records at offset {offset}")
            }
        }
    }
}

impl std::error::Error for Error {}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset.checked_add(4).ok_or(Error::Truncated { offset, needed: 4, len: bytes.len() })?;
    if end > bytes.len() {
        return Err(Error::Truncated { offset, needed: 4, len: bytes.len() });
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&bytes[offset..end]);
    Ok(u32::from_le_bytes(buf))
}

fn read_f32_le(bytes: &[u8], offset: usize) -> Result<f32, Error> {
    read_u32_le(bytes, offset).map(f32::from_bits)
}

fn need(bytes: &[u8], offset: usize, count: usize) -> Result<(), Error> {
    let end = offset.checked_add(count).ok_or(Error::Truncated { offset, needed: count, len: bytes.len() })?;
    if end > bytes.len() {
        return Err(Error::Truncated { offset, needed: count, len: bytes.len() });
    }
    Ok(())
}

/// Parses a whole `.fnc` buffer.
pub fn parse(bytes: &[u8]) -> Result<FncFile, Error> {
    if bytes.len() < 8 {
        return Err(Error::Truncated { offset: 0, needed: 8, len: bytes.len() });
    }
    let magic = read_u32_le(bytes, 0)?;
    if magic != MAGIC {
        return Err(Error::BadMagic { found: magic });
    }
    let group_count = read_u32_le(bytes, 4)? as usize;
    let min_group_bytes = group_count.checked_mul(4 + GROUP_HEADER_LEN).ok_or(Error::Truncated {
        offset: 8,
        needed: usize::MAX,
        len: bytes.len(),
    })?;
    need(bytes, 8, min_group_bytes)?;
    let mut offset = 8usize;
    let mut groups = Vec::new();
    groups.try_reserve_exact(group_count).map_err(|_| Error::Allocation { offset, count: group_count })?;

    for _ in 0..group_count {
        let post_count =
            read_u32_le(bytes, offset).map_err(|_| Error::Truncated { offset, needed: 4, len: bytes.len() })? as usize;
        offset += 4;

        need(bytes, offset, GROUP_HEADER_LEN)?;
        let fence_type = read_u32_le(bytes, offset)?;
        let post_type = read_u32_le(bytes, offset + 4)?;
        let material = read_u32_le(bytes, offset + 8)?;
        let rail_material = read_u32_le(bytes, offset + 12)?;
        offset += 16;
        let mut center = [0.0f32; 3];
        for v in &mut center {
            *v = read_f32_le(bytes, offset)?;
            offset += 4;
        }
        let mut half_extent = [0.0f32; 3];
        for v in &mut half_extent {
            *v = read_f32_le(bytes, offset)?;
            offset += 4;
        }

        let post_bytes = post_count.checked_mul(POST_LEN).ok_or(Error::Truncated {
            offset,
            needed: usize::MAX,
            len: bytes.len(),
        })?;
        need(bytes, offset, post_bytes)?;
        let mut posts = Vec::new();
        posts.try_reserve_exact(post_count).map_err(|_| Error::Allocation { offset, count: post_count })?;
        for _ in 0..post_count {
            need(bytes, offset, POST_LEN)?;
            let mut position = [0.0f32; 3];
            let mut center = [0.0f32; 3];
            let mut half_extent = [0.0f32; 3];
            for v in position.iter_mut().chain(center.iter_mut()).chain(half_extent.iter_mut()) {
                *v = read_f32_le(bytes, offset)?;
                offset += 4;
            }
            let color = read_u32_le(bytes, offset)?;
            offset += 4;
            posts.push(FncPost { position, center, half_extent, color });
        }
        groups.push(FncGroup { fence_type, post_type, material, rail_material, center, half_extent, posts });
    }

    let mut textures = Vec::new();
    let mut ignored_tail = Vec::new();

    // FUN_006038c0 calls FUN_006058c0 only inside its non-empty group path.
    // With zero groups, the retail loader does not read even the texture
    // count word, so retain the remainder verbatim.
    if group_count == 0 {
        ignored_tail.extend_from_slice(&bytes[offset..]);
    } else {
        let texture_count =
            read_u32_le(bytes, offset).map_err(|_| Error::Truncated { offset, needed: 4, len: bytes.len() })? as usize;
        offset += 4;

        if texture_count != 0 {
            let name_len =
                read_u32_le(bytes, offset).map_err(|_| Error::Truncated { offset, needed: 4, len: bytes.len() })?
                    as usize;
            offset += 4;
            if texture_count > bytes.len() {
                return Err(Error::Truncated { offset, needed: texture_count, len: bytes.len() });
            }
            let names_len = texture_count.checked_mul(name_len).ok_or(Error::Truncated {
                offset,
                needed: usize::MAX,
                len: bytes.len(),
            })?;
            need(bytes, offset, names_len)?;
            textures
                .try_reserve_exact(texture_count)
                .map_err(|_| Error::Allocation { offset, count: texture_count })?;
            for _ in 0..texture_count {
                let end = offset + name_len;
                textures.push(bytes[offset..end].to_vec());
                offset = end;
            }
        }
        ignored_tail.extend_from_slice(&bytes[offset..]);
    }

    Ok(FncFile { groups, textures, ignored_tail })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_u32(v: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn encode_f32(v: f32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn minimal_file() -> Vec<u8> {
        let mut b = Vec::new();
        encode_u32(2, &mut b);
        encode_u32(1, &mut b);
        encode_u32(1, &mut b); // post_count
        for v in [1, 2, 3, 4] {
            encode_u32(v, &mut b);
        }
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            encode_f32(v, &mut b);
        }
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0] {
            encode_f32(v, &mut b);
        }
        encode_u32(7, &mut b);
        encode_u32(1, &mut b); // fence texture count
        encode_u32(8, &mut b); // fixed-width name length
        b.extend_from_slice(b"fence\0\0\0");
        b
    }

    #[test]
    fn minimal_round_trip() {
        let bytes = minimal_file();
        assert_eq!(bytes.len(), 8 + 4 + 40 + 40 + 8 + 8);
        let file = parse(&bytes).unwrap();
        assert_eq!(file.groups.len(), 1);
        assert_eq!(
            (
                file.groups[0].fence_type,
                file.groups[0].post_type,
                file.groups[0].material,
                file.groups[0].rail_material
            ),
            (1, 2, 3, 4)
        );
        assert_eq!(file.groups[0].center, [1.0, 2.0, 3.0]);
        assert_eq!(file.groups[0].half_extent, [4.0, 5.0, 6.0]);
        assert_eq!(file.groups[0].posts.len(), 1);
        assert_eq!(file.groups[0].posts[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(file.groups[0].posts[0].center, [4.0, 5.0, 6.0]);
        assert_eq!(file.groups[0].posts[0].half_extent, [7.0, 8.0, 9.0]);
        assert_eq!(file.groups[0].posts[0].color, 7);
        assert_eq!(file.textures, vec![b"fence\0\0\0".to_vec()]);
        assert!(file.ignored_tail.is_empty());
    }

    #[test]
    fn zero_groups_accepted() {
        let mut b = Vec::new();
        encode_u32(2, &mut b);
        encode_u32(0, &mut b);
        b.extend_from_slice(&[0, 0, 0, 0, 32, 0, 0, 0]);
        let file = parse(&b).unwrap();
        assert!(file.groups.is_empty());
        assert!(file.textures.is_empty());
        assert_eq!(file.ignored_tail, [0, 0, 0, 0, 32, 0, 0, 0]);
    }

    #[test]
    fn zero_posts_accepted() {
        let mut b = Vec::new();
        encode_u32(2, &mut b);
        encode_u32(1, &mut b);
        encode_u32(0, &mut b);
        for _ in 0..4 {
            encode_u32(0, &mut b);
        }
        for _ in 0..6 {
            encode_f32(0.0, &mut b);
        }
        encode_u32(0, &mut b); // texture count
        let file = parse(&b).unwrap();
        assert_eq!(file.groups.len(), 1);
        assert!(file.groups[0].posts.is_empty());
    }

    #[test]
    fn bad_magic() {
        let mut b = Vec::new();
        encode_u32(3, &mut b);
        encode_u32(1, &mut b);
        assert!(matches!(parse(&b), Err(Error::BadMagic { found: 3 })));
    }

    #[test]
    fn truncated_header() {
        assert!(matches!(parse(&[]), Err(Error::Truncated { .. })));
        assert!(matches!(parse(&[2, 0, 0]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn truncated_group() {
        let mut b = Vec::new();
        encode_u32(2, &mut b);
        encode_u32(1, &mut b);
        encode_u32(0, &mut b);
        b.extend_from_slice(&[0u8; 10]);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn truncated_post() {
        let mut b = Vec::new();
        encode_u32(2, &mut b);
        encode_u32(1, &mut b);
        encode_u32(2, &mut b);
        for _ in 0..4 {
            encode_u32(0, &mut b);
        }
        for _ in 0..6 {
            encode_f32(0.0, &mut b);
        }
        b.extend_from_slice(&[0u8; 20]);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn huge_post_count_does_not_oom() {
        let mut b = Vec::new();
        encode_u32(2, &mut b);
        encode_u32(1, &mut b);
        encode_u32(u32::MAX, &mut b);
        for _ in 0..4 {
            encode_u32(0, &mut b);
        }
        for _ in 0..6 {
            encode_f32(0.0, &mut b);
        }
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn trailing_bytes_are_preserved() {
        let mut b = minimal_file();
        b.push(0);
        assert_eq!(parse(&b).unwrap().ignored_tail, vec![0]);
    }

    #[test]
    fn texture_table_bounds_are_checked() {
        let mut b = minimal_file();
        b.truncate(b.len() - 16);
        encode_u32(1, &mut b);
        encode_u32(32, &mut b);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn nonempty_group_requires_texture_count_word() {
        let mut b = minimal_file();
        b.truncate(b.len() - 16);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn zero_texture_count_does_not_read_width() {
        let mut b = minimal_file();
        b.truncate(b.len() - 16);
        encode_u32(0, &mut b);
        b.extend_from_slice(&[0xde, 0xad]);
        let file = parse(&b).unwrap();
        assert!(file.textures.is_empty());
        assert_eq!(file.ignored_tail, [0xde, 0xad]);
    }

    #[test]
    fn zero_width_texture_names_are_bounded() {
        let mut b = minimal_file();
        b.truncate(b.len() - 16);
        encode_u32(u32::MAX, &mut b);
        encode_u32(0, &mut b);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }
}
