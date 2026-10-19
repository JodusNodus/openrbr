//! Reader for the DirectDraw Surface (`.dds`) textures in the stage `*_textures.rbz` archives.
//!
//! The block-compressed forms the retail stages use are read: DXT1 (BC1, 8 bytes per 4x4
//! block) and DXT3 (BC2, 16 bytes per block); community stages add DXT5 and 32-bit RGBA. The header is the standard 124-byte block after the
//! `DDS ` magic, with no DX10 extension. Mip levels follow the header from the largest down, each
//! level stored row by row with no padding. Retail: 3,485 files, all DXT1 or DXT3, four mip levels,
//! at most 2048 pixels wide, dimensions multiples of four (`all_retail_dds_parse`). Layout:
//! `notes/formats/dds.md`. Community car models (RSF's) also use DXT5 (BC3, interpolated alpha, 16 bytes per block).

use std::fmt::{self, Display, Formatter};

const MAGIC: &[u8; 4] = b"DDS ";
/// Magic plus the 124-byte header.
const HEADER_LEN: usize = 128;
const HEADER_SIZE: u32 = 124;
const PIXEL_FORMAT_SIZE: u32 = 32;
/// `DDS_PIXELFORMAT` `dwFlags` bits: `dwFourCC` is valid; the RGB masks are (`DDPF_RGB`).
const PF_FOURCC: u32 = 0x4;
const PF_RGB: u32 = 0x40;

/// Byte offsets within the header (from the start of the file).
const OFFSET_HEIGHT: usize = 12;
const OFFSET_WIDTH: usize = 16;
const OFFSET_MIP_COUNT: usize = 28;
const OFFSET_PF_SIZE: usize = 76;
const OFFSET_PF_FLAGS: usize = 80;
const OFFSET_FOURCC: usize = 84;
const OFFSET_RGB_BITS: usize = 88;
const OFFSET_RGBA_MASKS: usize = 92;
/// The red, green, blue and alpha masks of 32-bit texels stored R, G, B, A in memory.
const RGBA_MASKS: [u32; 4] = [0xff, 0xff00, 0xff_0000, 0xff00_0000];

/// Texture format: block-compressed, or plain RGBA texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `DXT1`, BC1: colour only, 8 bytes per block.
    Dxt1,
    /// `DXT3`, BC2: explicit 4-bit alpha, 16 bytes per block.
    Dxt3,
    /// `DXT5`, BC3: interpolated alpha, 16 bytes per block. Not in retail; community stages use it.
    Dxt5,
    /// 32-bit texels stored R, G, B, A (`D3DFMT_A8B8G8R8`). Not in retail; one community stage texture uses it.
    Rgba8,
}

impl Format {
    /// Bytes per block ([`Format::block_edge`] texels square).
    pub fn block_len(self) -> usize {
        match self {
            Format::Dxt1 => 8,
            Format::Dxt3 | Format::Dxt5 => 16,
            Format::Rgba8 => 4,
        }
    }

    /// Edge length of a block in texels: 4 for the compressed formats, 1 for plain texels.
    pub fn block_edge(self) -> u32 {
        match self {
            Format::Rgba8 => 1,
            _ => 4,
        }
    }
}

/// A parsed DDS file: its size, format and the mip chain in one slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dds<'a> {
    pub width: u32,
    pub height: u32,
    pub format: Format,
    /// Mip levels stored, at least one.
    pub mip_count: u32,
    /// Every mip level back to back, from the largest.
    pub data: &'a [u8],
}

/// One mip level of a [`Dds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level<'a> {
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
}

impl<'a> Dds<'a> {
    /// The mip levels, largest first. Their sizes add up to `data` (checked by [`parse`]).
    pub fn levels(&self) -> Vec<Level<'a>> {
        let mut levels = Vec::with_capacity(self.mip_count as usize);
        let mut rest = self.data;
        let (mut width, mut height) = (self.width, self.height);
        for _ in 0..self.mip_count {
            let len = level_len(width, height, self.format);
            let Some((data, tail)) = rest.split_at_checked(len) else {
                break;
            };
            levels.push(Level { width, height, data });
            rest = tail;
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
        levels
    }
}

/// Bytes of one mip level: whole blocks per row and column, at least one block.
fn level_len(width: u32, height: u32, format: Format) -> usize {
    let blocks_x = width.div_ceil(format.block_edge()).max(1) as usize;
    let blocks_y = height.div_ceil(format.block_edge()).max(1) as usize;
    blocks_x * blocks_y * format.block_len()
}

/// Parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file is shorter than the 128-byte magic and header.
    Truncated { len: usize },
    /// The magic is not `DDS `.
    Magic,
    /// A header size field or the pixel format does not match the 124-byte block.
    Header { what: &'static str },
    /// A pixel format this reader does not decode (not DXT1, DXT3, DXT5 or 32-bit RGBA; zero for an RGB format).
    Format { fourcc: [u8; 4] },
    /// The mip chain does not fill the file exactly.
    Length { expected: usize, found: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { len } => write!(f, "truncated DDS header ({len} bytes)"),
            Error::Magic => write!(f, "not a DDS file (bad magic)"),
            Error::Header { what } => write!(f, "bad DDS header: {what}"),
            Error::Format { fourcc } => write!(f, "unsupported DDS format {:?}", String::from_utf8_lossy(fourcc)),
            Error::Length { expected, found } => {
                write!(f, "DDS mip chain is {found} bytes, expected {expected}")
            }
        }
    }
}

impl std::error::Error for Error {}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Parses a DDS file. The mip chain must follow the header; bytes after it are ignored, as D3DX does (some RSF car
/// textures hold one level more than their header counts).
pub fn parse(bytes: &[u8]) -> Result<Dds<'_>, Error> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::Truncated { len: bytes.len() });
    }
    if &bytes[..4] != MAGIC {
        return Err(Error::Magic);
    }
    if u32_at(bytes, 4) != HEADER_SIZE {
        return Err(Error::Header { what: "header size" });
    }
    if u32_at(bytes, OFFSET_PF_SIZE) != PIXEL_FORMAT_SIZE {
        return Err(Error::Header { what: "pixel format size" });
    }
    let flags = u32_at(bytes, OFFSET_PF_FLAGS);
    let fourcc: [u8; 4] = bytes[OFFSET_FOURCC..OFFSET_FOURCC + 4].try_into().expect("four bytes");
    let masks = std::array::from_fn(|i| u32_at(bytes, OFFSET_RGBA_MASKS + 4 * i));
    let format = if flags & PF_FOURCC != 0 {
        match &fourcc {
            b"DXT1" => Format::Dxt1,
            b"DXT3" => Format::Dxt3,
            b"DXT5" => Format::Dxt5,
            _ => return Err(Error::Format { fourcc }),
        }
    } else if flags & PF_RGB == 0 {
        return Err(Error::Header { what: "pixel format without a FourCC or RGB masks" });
    } else if u32_at(bytes, OFFSET_RGB_BITS) == 32 && masks == RGBA_MASKS {
        Format::Rgba8
    } else {
        return Err(Error::Format { fourcc: [0; 4] });
    };
    let (width, height) = (u32_at(bytes, OFFSET_WIDTH), u32_at(bytes, OFFSET_HEIGHT));
    if width == 0 || height == 0 {
        return Err(Error::Header { what: "zero dimension" });
    }
    // Retail files leave the `DDSD_MIPMAPCOUNT` flag clear but store the count (4), so the field
    // is read whenever it is non-zero.
    let mip_count = u32_at(bytes, OFFSET_MIP_COUNT).max(1);
    // A chain stops at 1x1, so more levels than that cannot be a real file.
    if mip_count > width.max(height).ilog2() + 1 {
        return Err(Error::Header { what: "mip count" });
    }

    let data = &bytes[HEADER_LEN..];
    let expected: usize = (0..mip_count).map(|level| level_len(width >> level, height >> level, format)).sum();
    let data = data.get(..expected).ok_or(Error::Length { expected, found: data.len() })?;
    Ok(Dds { width, height, format, mip_count, data })
}

/// `DDS_PIXELFORMAT` `dwFlags` bit: the alpha mask is valid (`DDPF_ALPHAPIXELS`).
const PF_ALPHA: u32 = 0x1;

/// Rewrites a plain 24- or 32-bit RGB file whose channels are whole bytes in another order (`D3DFMT_A8R8G8B8`,
/// `X8R8G8B8`, `R8G8B8`; community stage textures) as the [`Format::Rgba8`] file [`parse`] reads: `None` when it is
/// another format or already RGBA. Without an alpha mask (or `DDPF_ALPHAPIXELS`) the texels are opaque, as D3D's X
/// formats.
pub fn to_rgba8(bytes: &[u8]) -> Option<Vec<u8>> {
    let header = bytes.get(..HEADER_LEN)?;
    let flags = u32_at(header, OFFSET_PF_FLAGS);
    let bits = u32_at(header, OFFSET_RGB_BITS);
    let masks: [u32; 4] = std::array::from_fn(|i| u32_at(header, OFFSET_RGBA_MASKS + 4 * i));
    if &header[..4] != MAGIC || flags & (PF_FOURCC | PF_RGB) != PF_RGB || !matches!(bits, 24 | 32) {
        return None;
    }
    let alpha = flags & PF_ALPHA != 0 && masks[3] != 0;
    if bits == 32 && alpha && masks == RGBA_MASKS {
        return None;
    }
    // The byte of each channel in a texel.
    let lane = |mask: u32| (0..bits / 8).find(|&b| mask == 0xff << (8 * b)).map(|b| b as usize);
    let lanes = [lane(masks[0])?, lane(masks[1])?, lane(masks[2])?];
    let alpha_lane = if alpha { Some(lane(masks[3])?) } else { None };
    let (width, height) = (u32_at(header, OFFSET_WIDTH), u32_at(header, OFFSET_HEIGHT));
    let mips = u32_at(header, OFFSET_MIP_COUNT).max(1);
    if mips > width.max(height).max(1).ilog2() + 1 {
        return None;
    }
    let texels: usize = (0..mips).map(|l| ((width >> l).max(1) * (height >> l).max(1)) as usize).sum();
    let size = bits as usize / 8;
    let data = bytes.get(HEADER_LEN..HEADER_LEN + texels * size)?;
    let mut out = header.to_vec();
    out[OFFSET_PF_FLAGS..OFFSET_PF_FLAGS + 4].copy_from_slice(&(PF_RGB | PF_ALPHA).to_le_bytes());
    out[OFFSET_RGB_BITS..OFFSET_RGB_BITS + 4].copy_from_slice(&32u32.to_le_bytes());
    for (i, mask) in RGBA_MASKS.iter().enumerate() {
        out[OFFSET_RGBA_MASKS + 4 * i..][..4].copy_from_slice(&mask.to_le_bytes());
    }
    for texel in data.chunks_exact(size) {
        out.extend(lanes.map(|l| texel[l]));
        out.push(alpha_lane.map_or(0xff, |l| texel[l]));
    }
    Some(out)
}

/// A one-level [`Format::Rgba8`] file of `width` x `height` texels (`pixels`, R, G, B, A each), as [`parse`] reads it.
pub fn rgba8_file(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0u8; HEADER_LEN];
    bytes[..4].copy_from_slice(MAGIC);
    let mut put = |at: usize, value: u32| bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    put(4, HEADER_SIZE);
    put(OFFSET_HEIGHT, height);
    put(OFFSET_WIDTH, width);
    put(OFFSET_MIP_COUNT, 1);
    put(OFFSET_PF_SIZE, PIXEL_FORMAT_SIZE);
    put(OFFSET_PF_FLAGS, PF_RGB | PF_ALPHA);
    put(OFFSET_RGB_BITS, 32);
    for (i, mask) in RGBA_MASKS.iter().enumerate() {
        put(OFFSET_RGBA_MASKS + 4 * i, *mask);
    }
    bytes.extend_from_slice(pixels);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_rgba8_file_reads_back() {
        let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
        let bytes = rgba8_file(2, 1, &pixels);
        let dds = parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.format, dds.data), (2, 1, Format::Rgba8, &pixels[..]));
    }

    /// A header for a `width` x `height` DXT1 texture with `mips` levels, followed by zeroed data.
    fn dxt1(width: u32, height: u32, mips: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[..4].copy_from_slice(MAGIC);
        bytes[4..8].copy_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes[OFFSET_HEIGHT..OFFSET_HEIGHT + 4].copy_from_slice(&height.to_le_bytes());
        bytes[OFFSET_WIDTH..OFFSET_WIDTH + 4].copy_from_slice(&width.to_le_bytes());
        bytes[OFFSET_MIP_COUNT..OFFSET_MIP_COUNT + 4].copy_from_slice(&mips.to_le_bytes());
        bytes[OFFSET_PF_SIZE..OFFSET_PF_SIZE + 4].copy_from_slice(&PIXEL_FORMAT_SIZE.to_le_bytes());
        bytes[OFFSET_PF_FLAGS..OFFSET_PF_FLAGS + 4].copy_from_slice(&PF_FOURCC.to_le_bytes());
        bytes[OFFSET_FOURCC..OFFSET_FOURCC + 4].copy_from_slice(b"DXT1");
        let data: usize = (0..mips).map(|l| level_len(width >> l, height >> l, Format::Dxt1)).sum();
        bytes.resize(HEADER_LEN + data, 0);
        bytes
    }

    #[test]
    fn bgr_texels_become_rgba() {
        // 2x1, one level: X8R8G8B8 (stored B, G, R, X) and R8G8B8 (B, G, R).
        let header = |bits: u32, masks: [u32; 4]| {
            let mut bytes = dxt1(2, 1, 1)[..HEADER_LEN].to_vec();
            bytes[OFFSET_PF_FLAGS..OFFSET_PF_FLAGS + 4].copy_from_slice(&PF_RGB.to_le_bytes());
            bytes[OFFSET_RGB_BITS..OFFSET_RGB_BITS + 4].copy_from_slice(&bits.to_le_bytes());
            for (i, mask) in masks.iter().enumerate() {
                bytes[OFFSET_RGBA_MASKS + 4 * i..][..4].copy_from_slice(&mask.to_le_bytes());
            }
            bytes
        };
        let mut x8 = header(32, [0xff_0000, 0xff00, 0xff, 0]);
        x8.extend([1, 2, 3, 9, 4, 5, 6, 9]);
        let mut r8 = header(24, [0xff_0000, 0xff00, 0xff, 0]);
        r8.extend([1, 2, 3, 4, 5, 6]);
        for file in [x8, r8] {
            let rgba = to_rgba8(&file).unwrap();
            let dds = parse(&rgba).unwrap();
            assert_eq!((dds.format, dds.data), (Format::Rgba8, &[3, 2, 1, 255, 6, 5, 4, 255][..]));
            assert_eq!(to_rgba8(&rgba), None, "already RGBA");
        }
        assert_eq!(to_rgba8(&dxt1(4, 4, 1)), None);
    }

    #[test]
    fn levels_split_the_mip_chain() {
        let bytes = dxt1(16, 8, 3);
        let dds = parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.mip_count), (16, 8, 3));
        let levels = dds.levels();
        let sizes: Vec<(u32, u32, usize)> = levels.iter().map(|l| (l.width, l.height, l.data.len())).collect();
        // 16x8: 4x2 blocks = 64 bytes; 8x4: 2x1 = 16; 4x2: 1x1 = 8.
        assert_eq!(sizes, vec![(16, 8, 64), (8, 4, 16), (4, 2, 8)]);
    }

    #[test]
    fn small_levels_keep_one_block() {
        let bytes = dxt1(4, 4, 3);
        let levels = parse(&bytes).unwrap().levels();
        assert_eq!(levels.len(), 3);
        assert!(levels.iter().all(|l| l.data.len() == 8));
        assert_eq!((levels[2].width, levels[2].height), (1, 1));
    }

    #[test]
    fn rgba_texels_are_one_texel_blocks() {
        let mut bytes = dxt1(4, 2, 2);
        bytes[OFFSET_PF_FLAGS..OFFSET_PF_FLAGS + 4].copy_from_slice(&(PF_RGB | 1).to_le_bytes());
        bytes[OFFSET_FOURCC..OFFSET_FOURCC + 4].fill(0);
        bytes[OFFSET_RGB_BITS..OFFSET_RGB_BITS + 4].copy_from_slice(&32u32.to_le_bytes());
        for (i, mask) in RGBA_MASKS.iter().enumerate() {
            bytes[OFFSET_RGBA_MASKS + 4 * i..][..4].copy_from_slice(&mask.to_le_bytes());
        }
        bytes.resize(HEADER_LEN + (4 * 2 + 2) * 4, 0);
        let dds = parse(&bytes).unwrap();
        assert_eq!(dds.format, Format::Rgba8);
        let sizes: Vec<usize> = dds.levels().iter().map(|l| l.data.len()).collect();
        assert_eq!(sizes, vec![32, 8]);
        bytes[OFFSET_RGBA_MASKS..OFFSET_RGBA_MASKS + 4].copy_from_slice(&0xff_0000u32.to_le_bytes());
        assert_eq!(parse(&bytes), Err(Error::Format { fourcc: [0; 4] }));
    }

    #[test]
    fn rejects_bad_files() {
        let mut bytes = dxt1(8, 8, 1);
        assert_eq!(parse(&bytes[..100]), Err(Error::Truncated { len: 100 }));
        bytes[0] = b'X';
        assert_eq!(parse(&bytes), Err(Error::Magic));
        bytes[0] = b'D';
        bytes[OFFSET_FOURCC..OFFSET_FOURCC + 4].copy_from_slice(b"DX10");
        assert_eq!(parse(&bytes), Err(Error::Format { fourcc: *b"DX10" }));
        bytes[OFFSET_FOURCC..OFFSET_FOURCC + 4].copy_from_slice(b"DXT1");
        bytes.push(0);
        assert_eq!(parse(&bytes).map(|dds| dds.data.len()), Ok(32), "a trailing byte is left");
        bytes.truncate(bytes.len() - 2);
        assert_eq!(parse(&bytes), Err(Error::Length { expected: 32, found: 31 }));
    }
}
