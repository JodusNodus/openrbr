//! The 3DS Max export framing shared by [`crate::sgc`], [`crate::sgs`] and [`crate::sga`]: the 23-byte banner, then
//! records of `[u32 header_len = 12][u32 kind][u32 subtype][u32 1][u32 body_len][body]`, and a bounds-checked cursor
//! over one record body. Each reader keeps its own error type ([`FrameError`]).

use std::marker::PhantomData;

/// Banner at the start of every file: 22 characters and a NUL.
pub const BANNER: &[u8; 23] = b"|-|3<|&3|-|()& 5<3|\\|3\0";

/// Length of the record header words that precede each body.
pub(crate) const RECORD_HEADER_LEN: u32 = 12;

/// One record: kind and subtype from its header, and its body bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    /// Offset of the record header in the file.
    pub offset: usize,
    pub kind: u32,
    pub subtype: u32,
    pub body: &'a [u8],
}

/// The framing failures, as each reader's error type spells them.
pub(crate) trait FrameError: Clone {
    fn bad_banner() -> Self;
    fn header_length(offset: usize, length: u32) -> Self;
    fn truncated(offset: usize) -> Self;
    fn bad_body(offset: usize, what: &'static str) -> Self;
    fn leftover(offset: usize, unread: usize) -> Self;
}

/// Walks the banner and the record list, returning the records and whatever bytes follow the last one. A short
/// header is corruption (`truncated`). A header length other than 12 is an error, except with `keep_trailing`,
/// where a full 20-byte prefix whose first word is not 12 starts the trailing run.
pub(crate) fn split_records<E: FrameError>(bytes: &[u8], keep_trailing: bool) -> Result<(Vec<Record<'_>>, &[u8]), E> {
    if bytes.get(..BANNER.len()) != Some(BANNER.as_slice()) {
        return Err(E::bad_banner());
    }
    let mut out = Vec::new();
    let mut pos = BANNER.len();
    while pos < bytes.len() {
        let header_len = u32_at(bytes, pos).ok_or(E::truncated(pos))?;
        if header_len != RECORD_HEADER_LEN {
            if keep_trailing && bytes.len() - pos >= 20 {
                return Ok((out, &bytes[pos..]));
            }
            return Err(E::header_length(pos, header_len));
        }
        let truncated = E::truncated(pos);
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
pub(crate) fn records<E: FrameError>(bytes: &[u8]) -> Result<Vec<Record<'_>>, E> {
    split_records(bytes, false).map(|(out, _)| out)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(le_bytes(b)))
}

/// Copies up to `N` bytes into an array. Missing bytes stay zero, so this never panics.
pub(crate) fn le_bytes<const N: usize>(b: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    for (d, s) in out.iter_mut().zip(b) {
        *d = *s;
    }
    out
}

/// Cursor over one record body. Every read is bounds-checked against the body.
pub(crate) struct Reader<'a, E> {
    pub(crate) body: &'a [u8],
    pub(crate) pos: usize,
    /// File offset of the record, for errors.
    pub(crate) offset: usize,
    error: PhantomData<E>,
}

impl<'a, E: FrameError> Reader<'a, E> {
    pub(crate) fn new(body: &'a [u8], offset: usize) -> Self {
        Reader { body, pos: 0, offset, error: PhantomData }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.body.len() - self.pos
    }

    pub(crate) fn short(&self, what: &'static str) -> E {
        E::bad_body(self.offset, what)
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], E> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.body.len())
            .ok_or_else(|| self.short("body ends early"))?;
        let bytes = self.body.get(self.pos..end).ok_or_else(|| self.short("body ends early"))?;
        self.pos = end;
        Ok(bytes)
    }

    pub(crate) fn skip(&mut self, n: usize) -> Result<(), E> {
        self.take(n).map(|_| ())
    }

    /// Skips `n` items of `size` bytes, checking the total against the body first.
    pub(crate) fn skip_items(&mut self, n: usize, size: usize) -> Result<(), E> {
        let total = n.checked_mul(size).ok_or_else(|| self.short("count overflows"))?;
        self.skip(total)
    }

    /// Reads `n` items of `size` bytes, each decoded by `f`. The total is checked against the body
    /// before anything is allocated.
    pub(crate) fn items<T>(
        &mut self,
        n: usize,
        size: usize,
        mut f: impl FnMut(&mut Self) -> Result<T, E>,
    ) -> Result<Vec<T>, E> {
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

    pub(crate) fn cstr(&mut self) -> Result<String, E> {
        let rest = self.body.get(self.pos..).unwrap_or(&[]);
        let nul = rest.iter().position(|&b| b == 0).ok_or_else(|| self.short("unterminated name"))?;
        let name = String::from_utf8_lossy(&rest[..nul]).into_owned();
        self.pos += nul + 1;
        Ok(name)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, E> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, E> {
        Ok(u16::from_le_bytes(le_bytes(self.take(2)?)))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, E> {
        Ok(u32::from_le_bytes(le_bytes(self.take(4)?)))
    }

    pub(crate) fn i32(&mut self) -> Result<i32, E> {
        Ok(i32::from_le_bytes(le_bytes(self.take(4)?)))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, E> {
        Ok(f32::from_le_bytes(le_bytes(self.take(4)?)))
    }

    /// An i32 count. Negative values are an error.
    pub(crate) fn count(&mut self) -> Result<usize, E> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| self.short("negative count"))
    }

    /// An i32 that must be non-negative and fit in u32 (face ranges).
    pub(crate) fn count_u32(&mut self) -> Result<u32, E> {
        let n = self.i32()?;
        u32::try_from(n).map_err(|_| self.short("negative range"))
    }

    pub(crate) fn vec2(&mut self) -> Result<[f32; 2], E> {
        Ok([self.f32()?, self.f32()?])
    }

    pub(crate) fn vec3(&mut self) -> Result<[f32; 3], E> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub(crate) fn vec4(&mut self) -> Result<[f32; 4], E> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    /// Fails if the body has bytes the layout did not read.
    pub(crate) fn finish(&self) -> Result<(), E> {
        match self.body.len() - self.pos {
            0 => Ok(()),
            unread => Err(E::leftover(self.offset, unread)),
        }
    }
}
