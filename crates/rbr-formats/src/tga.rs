//! Uncompressed Targa images (`.tga`): true colour (type 2, 24 or 32 bits a texel, stored B, G, R and A) and grey
//! (type 3, 8 bits). Retail 1.0 ships five, all of these kinds (`Textures/car_dpshad2.tga`, the car's shadow decal,
//! among them). Run-length and colour-mapped files are not read.

use std::fmt;

/// An image as RGBA texels, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `width * height` texels of R, G, B, A. A 24-bit or grey file has alpha 255; a grey file has R = G = B.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Shorter than its header and texels.
    Truncated,
    /// An image type or texel depth this reader does not handle: `(type, bits)`.
    Unsupported(u8, u8),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Truncated => write!(f, "truncated tga"),
            ParseError::Unsupported(kind, bits) => write!(f, "unsupported tga: type {kind}, {bits} bits"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Reads `bytes` as an uncompressed true-colour or grey Targa image.
pub fn parse(bytes: &[u8]) -> Result<Image, ParseError> {
    let header = bytes.get(..18).ok_or(ParseError::Truncated)?;
    let (kind, bits, descriptor) = (header[2], header[16], header[17]);
    let texel = match (kind, bits) {
        (2, 24) => 3,
        (2, 32) => 4,
        (3, 8) => 1,
        _ => return Err(ParseError::Unsupported(kind, bits)),
    };
    let width = u16::from_le_bytes([header[12], header[13]]) as usize;
    let height = u16::from_le_bytes([header[14], header[15]]) as usize;
    // The image id follows the header; a colour map would too, but these types have none.
    let start = 18 + usize::from(header[0]);
    let texels = bytes.get(start..start + width * height * texel).ok_or(ParseError::Truncated)?;
    let mut rows: Vec<&[u8]> = texels.chunks_exact((width * texel).max(1)).take(height).collect();
    // Descriptor bit 5 set: the first row is the top one; clear (the usual): the bottom one.
    if descriptor & 0x20 == 0 {
        rows.reverse();
    }
    let rgba = rows
        .into_iter()
        .flat_map(|row| row.chunks_exact(texel))
        .flat_map(|t| match t {
            [grey] => [*grey, *grey, *grey, 255],
            [b, g, r] => [*r, *g, *b, 255],
            [b, g, r, a] => [*r, *g, *b, *a],
            _ => unreachable!("texels are 1, 3 or 4 bytes"),
        })
        .collect();
    Ok(Image { width: width as u32, height: height as u32, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: u8, bits: u8, width: u16, height: u16, descriptor: u8) -> Vec<u8> {
        let mut h = vec![0u8; 18];
        h[2] = kind;
        h[12..14].copy_from_slice(&width.to_le_bytes());
        h[14..16].copy_from_slice(&height.to_le_bytes());
        h[16] = bits;
        h[17] = descriptor;
        h
    }

    #[test]
    fn a_bottom_up_32_bit_image_reads_top_row_first_as_rgba() {
        let mut bytes = header(2, 32, 1, 2, 8);
        bytes.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        let image = parse(&bytes).unwrap();
        assert_eq!((image.width, image.height), (1, 2));
        assert_eq!(image.rgba, [7, 6, 5, 8, 3, 2, 1, 4]);
    }

    #[test]
    fn grey_and_24_bit_images_are_opaque_and_top_down_stays() {
        let mut grey = header(3, 8, 2, 1, 0x20);
        grey.extend([10, 20]);
        assert_eq!(parse(&grey).unwrap().rgba, [10, 10, 10, 255, 20, 20, 20, 255]);
        let mut colour = header(2, 24, 1, 1, 0);
        colour.extend([1, 2, 3]);
        assert_eq!(parse(&colour).unwrap().rgba, [3, 2, 1, 255]);
    }

    #[test]
    fn short_and_compressed_files_are_refused() {
        assert_eq!(parse(&header(2, 32, 2, 2, 0)), Err(ParseError::Truncated));
        assert_eq!(parse(&header(10, 32, 1, 1, 0)), Err(ParseError::Unsupported(10, 32)));
    }
}
