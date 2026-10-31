//! The 3DS Max export framing shared by [`crate::sgc`], [`crate::sgs`] and [`crate::sga`]: the 23-byte banner, then
//! records of `[u32 header_len = 12][u32 kind][u32 subtype][u32 1][u32 body_len][body]`, and a bounds-checked cursor
//! over one record body.
//!
//! The framing failures are one error type ([`Error`]), shared by the three readers: each keeps its variant paths
//! (`sgc::Error`, `sgs::Error` and `sga::Error` are aliases), and the banner text carries the file kind
//! ([`FileKind`]) because it differs per format. Variants a reader never constructs (the scenery trailing bytes,
//! the car unknown subtype, the animation unknown binding) exist for the readers that do.

use crate::le::{le_bytes, u32_at};
use std::fmt::{self, Display, Formatter};

/// Banner at the start of every file: 22 characters and a NUL.
pub const BANNER: &[u8; 23] = b"|-|3<|&3|-|()& 5<3|\\|3\0";

/// Length of the record header words that precede each body.
pub(crate) const RECORD_HEADER_LEN: u32 = 12;

/// Which file kind a framing error came from. Only the banner text differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Car models ([`crate::sgc`]).
    Sgc,
    /// Animated scenery models ([`crate::sgs`]).
    Sgs,
    /// Scenery animations ([`crate::sga`]).
    Sga,
}

/// One record: kind and subtype from its header, and its body bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    /// Offset of the record header in the file.
    pub offset: usize,
    pub kind: u32,
    pub subtype: u32,
    pub body: &'a [u8],
}

/// Failure to read a framed model or animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file does not start with [`BANNER`].
    BadBanner { kind: FileKind },
    /// A record header length other than 12.
    HeaderLength { offset: usize, length: u32 },
    /// A record header or body runs past the end of the file.
    Truncated { offset: usize },
    /// A body is shorter than its layout, or a count is negative.
    BadBody { offset: usize, what: &'static str },
    /// Non-record bytes after the last record: [`crate::sgs::parse`] keeps them, [`crate::sgs::records`] rejects
    /// them. Only two retail files have any.
    Trailing { offset: usize, len: usize },
    /// A body ends before its layout does.
    Leftover { offset: usize, unread: usize },
    /// A geometry, camera, helper, collision or shadow record with a subtype this reader does not know.
    UnknownSubtype { offset: usize, kind: u32, subtype: u32 },
    /// A 6/21 inner type, or a track type, the animation reader does not decode. Rotation, look-at, Euler,
    /// link-constraints and plain-PRS bindings never occur in retail files.
    UnknownBinding { offset: usize, controller: u32, subtype: u32 },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadBanner { kind } => match kind {
                FileKind::Sgc => write!(f, "not an sgc model (banner mismatch)"),
                FileKind::Sgs => write!(f, "not a scenery model (banner mismatch)"),
                FileKind::Sga => write!(f, "not a scenery animation (banner mismatch)"),
            },
            Error::HeaderLength { offset, length } => {
                write!(f, "record at {offset:#x}: header length {length}, expected 12")
            }
            Error::Truncated { offset } => write!(f, "record at {offset:#x} runs past the file"),
            Error::BadBody { offset, what } => write!(f, "record at {offset:#x}: {what}"),
            Error::Trailing { offset, len } => {
                write!(f, "{len} trailing bytes at {offset:#x} past the last record")
            }
            Error::Leftover { offset, unread } => {
                write!(f, "record at {offset:#x}: {unread} bytes unread")
            }
            Error::UnknownSubtype { offset, kind, subtype } => {
                write!(f, "record at {offset:#x}: kind {kind}, unknown subtype {subtype}")
            }
            Error::UnknownBinding { offset, controller, subtype } => {
                write!(f, "record at {offset:#x}: unknown animation binding ({controller}, {subtype})")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Walks the banner and the record list, returning the records and whatever bytes follow the last one. A short
/// header is corruption (`truncated`). A header length other than 12 is an error, except with `keep_trailing`,
/// where a full 20-byte prefix whose first word is not 12 starts the trailing run.
pub(crate) fn split_records(
    bytes: &[u8],
    keep_trailing: bool,
    file: FileKind,
) -> Result<(Vec<Record<'_>>, &[u8]), Error> {
    if bytes.get(..BANNER.len()) != Some(BANNER.as_slice()) {
        return Err(Error::BadBanner { kind: file });
    }
    let mut out = Vec::new();
    let mut pos = BANNER.len();
    while pos < bytes.len() {
        let header_len = u32_at(bytes, pos).ok_or(Error::Truncated { offset: pos })?;
        if header_len != RECORD_HEADER_LEN {
            if keep_trailing && bytes.len() - pos >= 20 {
                return Ok((out, &bytes[pos..]));
            }
            return Err(Error::HeaderLength { offset: pos, length: header_len });
        }
        let truncated = Error::Truncated { offset: pos };
        let kind = u32_at(bytes, pos + 4).ok_or_else(|| truncated.clone())?;
        let subtype = u32_at(bytes, pos + 8).ok_or_else(|| truncated.clone())?;
        let body_len = u32_at(bytes, pos + 16).ok_or_else(|| truncated.clone())?;
        let start = pos + 20;
        let end =
            start.checked_add(body_len as usize).filter(|&end| end <= bytes.len()).ok_or_else(|| truncated.clone())?;
        let body = bytes.get(start..end).ok_or(truncated)?;
        out.push(Record { offset: pos, kind, subtype, body });
        pos = end;
    }
    Ok((out, &bytes[bytes.len()..]))
}

/// Reads the banner and the record list. Records must tile the file exactly.
pub(crate) fn records(bytes: &[u8], file: FileKind) -> Result<Vec<Record<'_>>, Error> {
    split_records(bytes, false, file).map(|(out, _)| out)
}

/// Cursor over one record body. Every read is bounds-checked against the body.
pub(crate) struct Reader<'a> {
    pub(crate) body: &'a [u8],
    pub(crate) pos: usize,
    /// File offset of the record, for errors.
    pub(crate) offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(body: &'a [u8], offset: usize) -> Self {
        Reader { body, pos: 0, offset }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.body.len() - self.pos
    }

    fn short(&self, what: &'static str) -> Error {
        Error::BadBody { offset: self.offset, what }
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.body.len())
            .ok_or_else(|| self.short("body ends early"))?;
        let bytes = self.body.get(self.pos..end).ok_or_else(|| self.short("body ends early"))?;
        self.pos = end;
        Ok(bytes)
    }

    pub(crate) fn skip(&mut self, n: usize) -> Result<(), Error> {
        self.take(n).map(|_| ())
    }

    /// Skips `n` items of `size` bytes, checking the total against the body first.
    pub(crate) fn skip_items(&mut self, n: usize, size: usize) -> Result<(), Error> {
        let total = n.checked_mul(size).ok_or_else(|| self.short("count overflows"))?;
        self.skip(total)
    }

    /// Reads `n` items of `size` bytes, each decoded by `f`. The total is checked against the body
    /// before anything is allocated.
    pub(crate) fn items<T>(
        &mut self,
        n: usize,
        size: usize,
        mut f: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        let total = n.checked_mul(size).ok_or_else(|| self.short("count overflows"))?;
        if total > self.body.len() - self.pos {
            return Err(self.short("body ends early"));
        }
        let mut out = Vec::with_capacity(n.min(1 << 20));
        for _ in 0..n {
            out.push(f(self)?);
        }
        Ok(out)
    }

    pub(crate) fn cstr(&mut self) -> Result<String, Error> {
        let rest = self.body.get(self.pos..).unwrap_or(&[]);
        let nul = rest.iter().position(|&b| b == 0).ok_or_else(|| self.short("unterminated name"))?;
        let name = String::from_utf8_lossy(&rest[..nul]).into_owned();
        self.pos += nul + 1;
        Ok(name)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, Error> {
        Ok(u16::from_le_bytes(le_bytes(self.take(2)?)))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(le_bytes(self.take(4)?)))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(le_bytes(self.take(4)?)))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(le_bytes(self.take(4)?)))
    }

    /// An i32 count. Negative values are an error.
    pub(crate) fn count(&mut self) -> Result<usize, Error> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| self.short("negative count"))
    }

    /// An i32 that must be non-negative and fit in u32 (face ranges).
    pub(crate) fn count_u32(&mut self) -> Result<u32, Error> {
        let n = self.i32()?;
        u32::try_from(n).map_err(|_| self.short("negative range"))
    }

    pub(crate) fn vec2(&mut self) -> Result<[f32; 2], Error> {
        Ok([self.f32()?, self.f32()?])
    }

    pub(crate) fn vec3(&mut self) -> Result<[f32; 3], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub(crate) fn vec4(&mut self) -> Result<[f32; 4], Error> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    /// Fails if the body has bytes the layout did not read.
    pub(crate) fn finish(&self) -> Result<(), Error> {
        match self.body.len() - self.pos {
            0 => Ok(()),
            unread => Err(Error::Leftover { offset: self.offset, unread }),
        }
    }
}
