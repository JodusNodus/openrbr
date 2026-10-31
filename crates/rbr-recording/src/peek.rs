//! Reader for optional harness pointer-probe logs (`RBRRecompPeek-<tick>.bin`).
//!
//! The file starts with `PEK1`, version 1, and a probe count. Each probe has a
//! one-byte type, a little-endian `u16` name length, and UTF-8 name bytes. The
//! remainder is a sequence of records: a little-endian `u32` physics step,
//! followed by one value per probe in header order. `u8` values occupy one
//! byte; the other types occupy four bytes. Invalid pointer reads use reserved
//! markers: f32 bits `0x7fc00000`, `u32::MAX`, `u8::MAX`, or `i32::MIN`.

use std::fmt::{self, Display, Formatter};

use crate::read_u32;

pub const MAGIC: u32 = 0x314b_4550;
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PeekType {
    F32 = 1,
    U32 = 2,
    U8 = 3,
    I32 = 4,
}

impl PeekType {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::F32),
            2 => Some(Self::U32),
            3 => Some(Self::U8),
            4 => Some(Self::I32),
            _ => None,
        }
    }

    pub fn byte_len(self) -> usize {
        if self == Self::U8 {
            1
        } else {
            4
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::U32 => "u32",
            Self::U8 => "u8",
            Self::I32 => "i32",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub name: String,
    pub ty: PeekType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    F32(f32),
    U32(u32),
    U8(u8),
    I32(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadMagic { found: u32 },
    UnsupportedVersion { found: u32 },
    InvalidType { offset: usize, found: u8 },
    InvalidUtf8 { offset: usize },
    Truncated { offset: usize, needed: usize, len: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic { found } => {
                write!(f, "bad peek magic {found:#010x}, expected {MAGIC:#010x}")
            }
            Self::UnsupportedVersion { found } => {
                write!(f, "unsupported peek version {found}, expected {VERSION}")
            }
            Self::InvalidType { offset, found } => {
                write!(f, "invalid probe type {found} at offset {offset}")
            }
            Self::InvalidUtf8 { offset } => {
                write!(f, "invalid probe name UTF-8 at offset {offset}")
            }
            Self::Truncated { offset, needed, len } => write!(
                f,
                "truncated peek data at offset {offset}: need {needed} bytes but {} remain",
                len.saturating_sub(*offset)
            ),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
pub struct PeekFile<'a> {
    bytes: &'a [u8],
    records_offset: usize,
    pub probes: Vec<Probe>,
}

impl<'a> PeekFile<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let magic = read_u32(bytes, 0).ok_or(Error::Truncated { offset: 0, needed: 12, len: bytes.len() })?;
        if magic != MAGIC {
            return Err(Error::BadMagic { found: magic });
        }
        let version = read_u32(bytes, 4).ok_or(Error::Truncated { offset: 0, needed: 12, len: bytes.len() })?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion { found: version });
        }
        let count = read_u32(bytes, 8).ok_or(Error::Truncated { offset: 0, needed: 12, len: bytes.len() })? as usize;
        let mut offset = 12usize;
        if count > bytes.len().saturating_sub(offset) / 3 {
            return Err(Error::Truncated { offset, needed: count.saturating_mul(3), len: bytes.len() });
        }
        let mut probes = Vec::with_capacity(count);
        for _ in 0..count {
            let type_byte = *bytes.get(offset).ok_or(Error::Truncated {
                offset,
                needed: offset.saturating_add(3),
                len: bytes.len(),
            })?;
            let ty = PeekType::from_byte(type_byte).ok_or(Error::InvalidType { offset, found: type_byte })?;
            let name_len_bytes = bytes.get(offset + 1..offset + 3).ok_or(Error::Truncated {
                offset,
                needed: offset.saturating_add(3),
                len: bytes.len(),
            })?;
            let name_len = u16::from_le_bytes([name_len_bytes[0], name_len_bytes[1]]) as usize;
            let name_offset = offset + 3;
            let name_bytes = bytes.get(name_offset..name_offset.saturating_add(name_len)).ok_or(Error::Truncated {
                offset,
                needed: name_offset.saturating_add(name_len),
                len: bytes.len(),
            })?;
            let name =
                std::str::from_utf8(name_bytes).map_err(|_| Error::InvalidUtf8 { offset: name_offset })?.to_owned();
            probes.push(Probe { name, ty });
            offset = name_offset.saturating_add(name_len);
        }
        Ok(Self { bytes, records_offset: offset, probes })
    }

    pub fn records(&self) -> Records<'_> {
        Records { bytes: self.bytes, offset: self.records_offset, probes: &self.probes }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Record<'a> {
    pub offset: usize,
    pub step: u32,
    probes: &'a [Probe],
    values: &'a [u8],
}

impl Record<'_> {
    pub fn value(&self, index: usize) -> Option<Value> {
        let probe = self.probes.get(index)?;
        let offset: usize = self.probes[..index].iter().map(|item| item.ty.byte_len()).sum();
        let bytes = self.values.get(offset..offset + probe.ty.byte_len())?;
        match probe.ty {
            PeekType::F32 => Some(Value::F32(f32::from_bits(read_u32(bytes, 0)?))),
            PeekType::U32 => Some(Value::U32(read_u32(bytes, 0)?)),
            PeekType::U8 => Some(Value::U8(bytes.first().copied()?)),
            PeekType::I32 => Some(Value::I32(read_u32(bytes, 0)? as i32)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Records<'a> {
    bytes: &'a [u8],
    offset: usize,
    probes: &'a [Probe],
}

impl<'a> Iterator for Records<'a> {
    type Item = Result<Record<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.bytes.len() {
            return None;
        }
        let values_len: usize = self.probes.iter().map(|probe| probe.ty.byte_len()).sum();
        let needed = 4usize.saturating_add(values_len);
        let end = self.offset.saturating_add(needed);
        if end > self.bytes.len() {
            let error = Error::Truncated { offset: self.offset, needed, len: self.bytes.len() };
            self.offset = self.bytes.len();
            return Some(Err(error));
        }
        let offset = self.offset;
        let step = crate::word_at(self.bytes, offset);
        let values = &self.bytes[offset + 4..end];
        self.offset = end;
        Some(Ok(Record { offset, step, probes: self.probes, values }))
    }
}
