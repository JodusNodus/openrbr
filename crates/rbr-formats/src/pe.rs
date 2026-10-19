//! The sections of a 32-bit PE image, by virtual address: how the load path reads the retail exe's `.data` and
//! `.rdata` (vtables, constant tables, the initial globals) as the process starts with them. A section's bytes past
//! its raw data, up to its virtual size, read as zero, as the loader maps them.

use std::fmt::{self, Display, Formatter};

/// One section, mapped at `base + virtual_address`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    /// Absolute virtual address (the image base added).
    pub address: u32,
    /// The mapped bytes: the raw data, zero-filled to the virtual size.
    pub bytes: Vec<u8>,
}

/// The image base and its sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub base: u32,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// No `MZ`/`PE\0\0` headers, or a header past the end of the file.
    BadHeader,
    /// Not a 32-bit (PE32) optional header.
    NotPe32,
    /// A section's raw data lies outside the file.
    SectionOutOfRange,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::BadHeader => "not a PE image",
            Error::NotPe32 => "not a 32-bit PE image",
            Error::SectionOutOfRange => "section data outside the file",
        })
    }
}

impl std::error::Error for Error {}

fn u16_at(b: &[u8], at: usize) -> Result<u16, Error> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(Error::BadHeader)
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, Error> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or(Error::BadHeader)
}

pub fn parse(bytes: &[u8]) -> Result<Image, Error> {
    if bytes.get(..2) != Some(b"MZ") {
        return Err(Error::BadHeader);
    }
    let pe = u32_at(bytes, 0x3c)? as usize;
    if bytes.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(Error::BadHeader);
    }
    let count = u16_at(bytes, pe + 6)? as usize;
    let optional = pe + 24;
    let optional_size = u16_at(bytes, pe + 20)? as usize;
    if u16_at(bytes, optional)? != 0x10b {
        return Err(Error::NotPe32);
    }
    let base = u32_at(bytes, optional + 28)?;
    let sections = (0..count)
        .map(|k| {
            let at = optional + optional_size + 40 * k;
            let header = bytes.get(at..at + 40).ok_or(Error::BadHeader)?;
            let name = String::from_utf8_lossy(&header[..8]).trim_end_matches('\0').to_owned();
            let (virtual_size, address) = (u32_at(header, 8)?, u32_at(header, 12)?);
            let (raw_size, raw_at) = (u32_at(header, 16)? as usize, u32_at(header, 20)? as usize);
            let size = virtual_size.max(raw_size as u32) as usize;
            let raw = bytes.get(raw_at..raw_at + raw_size.min(size)).ok_or(Error::SectionOutOfRange)?;
            let mut mapped = raw.to_vec();
            mapped.resize(size, 0);
            Ok(Section { name, address: base + address, bytes: mapped })
        })
        .collect::<Result<_, _>>()?;
    Ok(Image { base, sections })
}

/// Where the file's overlay starts: the end of the furthest section's raw data. Installers append their payload
/// there (`installshield::sfx_files`).
pub fn overlay_offset(bytes: &[u8]) -> Result<usize, Error> {
    if bytes.get(..2) != Some(b"MZ") {
        return Err(Error::BadHeader);
    }
    let pe = u32_at(bytes, 0x3c)? as usize;
    let table = pe + 24 + u16_at(bytes, pe + 20)? as usize;
    (0..u16_at(bytes, pe + 6)? as usize).try_fold(0, |end, k| {
        let at = table + 40 * k;
        Ok(end.max(u32_at(bytes, at + 20)? as usize + u32_at(bytes, at + 16)? as usize))
    })
}

impl Image {
    /// The section named `name` (`.data`, `.rdata`, ...).
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// `len` bytes at the virtual address `at`, when one section holds them all.
    pub fn read(&self, at: u32, len: usize) -> Option<&[u8]> {
        self.sections.iter().find_map(|s| {
            let off = at.checked_sub(s.address)? as usize;
            s.bytes.get(off..off + len)
        })
    }

    /// The little-endian word at `at`.
    pub fn u32(&self, at: u32) -> Option<u32> {
        self.read(at, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE32 image: one `.data` section of 8 raw bytes, 0x10 virtual, at 0x1000.
    fn image() -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        b[0x40..0x44].copy_from_slice(b"PE\0\0");
        b[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
        b[0x54..0x56].copy_from_slice(&0xe0u16.to_le_bytes());
        b[0x58..0x5a].copy_from_slice(&0x10bu16.to_le_bytes());
        b[0x58 + 28..0x58 + 32].copy_from_slice(&0x400000u32.to_le_bytes());
        let s = 0x58 + 0xe0;
        b[s..s + 5].copy_from_slice(b".data");
        for (at, v) in [(8, 0x10u32), (12, 0x1000), (16, 8), (20, 0x180)] {
            b[s + at..s + at + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[0x180..0x188].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        b
    }

    #[test]
    fn sections_map_at_their_virtual_address_zero_filled() {
        let img = parse(&image()).unwrap();
        assert_eq!(img.base, 0x400000);
        assert_eq!(img.u32(0x401004), Some(0x0807_0605));
        assert_eq!(img.read(0x401008, 8), Some(&[0u8; 8][..]));
        assert_eq!(img.read(0x40100c, 8), None);
        assert_eq!(img.section(".data").unwrap().bytes.len(), 0x10);
        assert_eq!(overlay_offset(&image()), Ok(0x188));
    }

    #[test]
    fn rejects_what_is_not_pe32() {
        assert_eq!(parse(b"ZM"), Err(Error::BadHeader));
        let mut b = image();
        b[0x58] = 0x0b;
        b[0x59] = 0x02;
        assert_eq!(parse(&b), Err(Error::NotPe32));
    }
}
