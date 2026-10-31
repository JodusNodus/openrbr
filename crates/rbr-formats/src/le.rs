//! The little-endian reads the format parsers share: a bounds-checked cursor ([`Reader`]) whose failures are a
//! [`Truncated`] each module's error converts from, and offset reads that fail as [`Truncated`], `None` or a panic
//! (for slices the caller already holds).

use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;

/// A bounds failure: the offset it happened at, the bytes needed, and the
/// byte count reported as the length. Every module's
/// `Truncated { offset, needed, len }` variant converts from this, so cursor
/// code below just returns it and `?` maps it into the module's error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Truncated {
    pub offset: usize,
    pub needed: usize,
    pub len: usize,
}

impl Display for Truncated {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self { offset, needed, len } = *self;
        write!(f, "truncated at offset {offset}: need {needed} bytes but file is {len} bytes")
    }
}

impl std::error::Error for Truncated {}

/// The [`Truncated`] text shared by the `Truncated { offset, needed, len }`
/// `Display` arms. (The `.lbs` reader reports the bytes left in scope
/// instead and keeps its own line.)
pub(crate) fn fmt_truncated(f: &mut Formatter<'_>, offset: usize, needed: usize, len: usize) -> fmt::Result {
    write!(f, "{}", Truncated { offset, needed, len })
}

/// A read position in `bytes`, valid up to `end`. Offsets are absolute.
pub(crate) struct Reader<'a, E> {
    bytes: &'a [u8],
    pos: usize,
    end: usize,
    /// Whether errors report the bytes left in scope (`end - pos`) rather
    /// than the buffer length (the `.lbs` chunk convention).
    remaining: bool,
    error: PhantomData<E>,
}

impl<'a, E: From<Truncated>> Reader<'a, E> {
    /// A reader over the whole buffer, reporting its length in errors.
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0, end: bytes.len(), remaining: false, error: PhantomData }
    }

    /// A reader at `offset`, reporting the buffer length in errors.
    pub(crate) fn at(bytes: &'a [u8], offset: usize) -> Result<Self, E> {
        if offset > bytes.len() {
            return Err(E::from(Truncated { offset, needed: 0, len: bytes.len() }));
        }
        Ok(Reader { bytes, pos: offset, end: bytes.len(), remaining: false, error: PhantomData })
    }

    /// A reader over `bytes[pos..end]`, reporting the bytes left in scope in errors (the `.lbs` chunks). The
    /// bounds are not checked up front; the first read past them fails.
    pub(crate) fn within(bytes: &'a [u8], pos: usize, end: usize) -> Self {
        Reader { bytes, pos, end, remaining: true, error: PhantomData }
    }

    /// The truncation error at the current position, for the rare failure a
    /// checked read cannot name (a fixed-size record that cannot convert).
    pub(crate) fn truncated(&self, needed: usize) -> E {
        E::from(Truncated { offset: self.pos, needed, len: self.error_len() })
    }

    fn error_len(&self) -> usize {
        if self.remaining {
            self.end.saturating_sub(self.pos)
        } else {
            self.bytes.len()
        }
    }

    /// The current absolute offset.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// The bytes left in scope.
    pub(crate) fn remaining(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// Whether the scope is consumed exactly.
    pub(crate) fn is_empty(&self) -> bool {
        self.pos == self.end
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], E> {
        let stop = self.pos.checked_add(n).filter(|&s| s <= self.end).ok_or_else(|| self.truncated(n))?;
        let slice = self.bytes.get(self.pos..stop).ok_or_else(|| self.truncated(n))?;
        self.pos = stop;
        Ok(slice)
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

    pub(crate) fn f32(&mut self) -> Result<f32, E> {
        Ok(f32::from_bits(self.u32()?))
    }

    /// `n` words in file order. Nothing is reserved: `n` comes from the file, and the loop fails at the first
    /// missing word.
    pub(crate) fn u32s(&mut self, n: u32) -> Result<Vec<u32>, E> {
        let mut out = Vec::new();
        for _ in 0..n {
            out.push(self.u32()?);
        }
        Ok(out)
    }

    /// `N` words in file order.
    pub(crate) fn words<const N: usize>(&mut self) -> Result<[u32; N], E> {
        let mut words = [0u32; N];
        for word in &mut words {
            *word = self.u32()?;
        }
        Ok(words)
    }

    /// `N` floats in file order.
    pub(crate) fn f32s<const N: usize>(&mut self) -> Result<[f32; N], E> {
        let b = self.take(N * 4)?;
        Ok(std::array::from_fn(|i| f32_of(&b[i * 4..])))
    }

    /// Bytes up to the next NUL, advancing past it. A missing NUL fails like
    /// a one-byte read past the end of the scope.
    pub(crate) fn take_cstr(&mut self) -> Result<&'a [u8], E> {
        let rest = self.bytes.get(self.pos..self.end).ok_or_else(|| self.truncated(1))?;
        let len = rest.iter().position(|&b| b == 0).ok_or_else(|| self.truncated(1))?;
        let out = self.take(len)?;
        self.take(1)?;
        Ok(out)
    }

    /// Fails unless `count` records of `stride` bytes fit in what remains,
    /// before anything is allocated for them.
    pub(crate) fn require(&self, count: usize, stride: usize) -> Result<(), E> {
        let needed = count
            .checked_mul(stride)
            .ok_or_else(|| E::from(Truncated { offset: self.pos, needed: usize::MAX, len: self.error_len() }))?;
        if needed > self.remaining() {
            return Err(self.truncated(needed));
        }
        Ok(())
    }
}

/// The slice `bytes[offset..offset+len]`, or a truncation error naming `offset`.
/// Returns [`Truncated`] itself so `?` maps it into the caller's error.
pub(crate) fn bytes_at(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8], Truncated> {
    let end = offset.checked_add(len).ok_or(Truncated { offset, needed: len, len: bytes.len() })?;
    if end > bytes.len() {
        return Err(Truncated { offset, needed: len, len: bytes.len() });
    }
    Ok(&bytes[offset..end])
}

/// Fails unless `count` bytes at `offset` are in the buffer.
pub(crate) fn need(bytes: &[u8], offset: usize, count: usize) -> Result<(), Truncated> {
    bytes_at(bytes, offset, count).map(|_| ())
}

/// The little-endian word at `offset`.
pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, Truncated> {
    Ok(u32::from_le_bytes(bytes_at(bytes, offset, 4)?.try_into().unwrap()))
}

/// The little-endian float at `offset`.
pub(crate) fn read_f32_le(bytes: &[u8], offset: usize) -> Result<f32, Truncated> {
    read_u32_le(bytes, offset).map(f32::from_bits)
}

/// The little-endian half word at `at`, or `None` when it runs past the end.
pub(crate) fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

/// The little-endian word at `at`, or `None` when it runs past the end.
pub(crate) fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// The little-endian float at `at`, or `None` when it runs past the end.
pub(crate) fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    u32_at(b, at).map(f32::from_bits)
}

/// The first four bytes as a word. Callers pass a slice they already hold;
/// like indexing, this panics when it is short.
pub(crate) fn u32_of(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// The first four bytes as a float. Callers pass a slice they already hold;
/// like indexing, this panics when it is short.
pub(crate) fn f32_of(b: &[u8]) -> f32 {
    f32::from_bits(u32_of(b))
}

/// The first `4 * N` bytes as floats. Callers pass a slice they already
/// hold; like indexing, this panics when it is short.
pub(crate) fn floats_of<const N: usize>(b: &[u8]) -> [f32; N] {
    std::array::from_fn(|i| f32_of(&b[i * 4..]))
}

/// Copies up to `N` bytes into an array. Missing bytes stay zero, so this never panics.
pub(crate) fn le_bytes<const N: usize>(b: &[u8]) -> [u8; N] {
    let mut out = [0; N];
    for (d, s) in out.iter_mut().zip(b) {
        *d = *s;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An error with the standard truncated shape, for the reader tests.
    #[derive(Debug, PartialEq)]
    struct Shaped {
        truncated: Truncated,
    }

    impl From<Truncated> for Shaped {
        fn from(truncated: Truncated) -> Self {
            Shaped { truncated }
        }
    }

    /// An error that drops the lengths, like the engine bank reader's.
    #[derive(Debug, PartialEq)]
    struct OffsetOnly {
        offset: usize,
    }

    impl From<Truncated> for OffsetOnly {
        fn from(t: Truncated) -> Self {
            OffsetOnly { offset: t.offset }
        }
    }

    #[test]
    fn truncated_text_is_the_shared_line() {
        assert_eq!(
            Truncated { offset: 8, needed: 4, len: 10 }.to_string(),
            "truncated at offset 8: need 4 bytes but file is 10 bytes"
        );
    }

    #[test]
    fn whole_reader_reports_file_values() {
        let bytes = [1, 0, 0, 0, 2, 0, 7, 0, 0, 0x80, 0x3f, 5];
        let mut r = Reader::<Shaped>::new(&bytes);
        assert_eq!(r.pos(), 0);
        assert_eq!(r.remaining(), 12);
        assert_eq!(r.u32().unwrap(), 1);
        assert_eq!(r.u16().unwrap(), 2);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.f32().unwrap(), 1.0);
        assert_eq!(r.u8().unwrap(), 5);
        assert!(r.is_empty());
        assert_eq!(r.take(1).unwrap_err(), Shaped { truncated: Truncated { offset: 12, needed: 1, len: 12 } });
        let mut r = Reader::<Shaped>::new(&bytes);
        assert_eq!(
            r.take(usize::MAX).unwrap_err(),
            Shaped { truncated: Truncated { offset: 0, needed: usize::MAX, len: 12 } }
        );
        assert_eq!(r.require(4, 4).unwrap_err(), Shaped { truncated: Truncated { offset: 0, needed: 16, len: 12 } });
        assert_eq!(
            r.require(usize::MAX, 40).unwrap_err(),
            Shaped { truncated: Truncated { offset: 0, needed: usize::MAX, len: 12 } }
        );
        r.u32().unwrap();
        assert_eq!(r.words::<2>().unwrap(), [0x0007_0002, 0x053f_8000]);
        assert_eq!(
            Reader::<Shaped>::at(&bytes, 13).err().unwrap(),
            Shaped { truncated: Truncated { offset: 13, needed: 0, len: 12 } }
        );
    }

    #[test]
    fn scoped_reader_reports_bytes_left() {
        // The `.lbs` convention: `len` is what remains in scope, not the file length.
        let bytes = [1u8; 64];
        let mut r = Reader::<Shaped>::within(&bytes, 48, 56);
        assert_eq!(r.remaining(), 8);
        r.take(8).unwrap();
        assert!(r.is_empty());
        assert_eq!(r.take(4).unwrap_err(), Shaped { truncated: Truncated { offset: 56, needed: 4, len: 0 } });
        let mut r = Reader::<Shaped>::within(&bytes, 48, 56);
        assert_eq!(r.take_cstr().unwrap_err(), Shaped { truncated: Truncated { offset: 48, needed: 1, len: 8 } });
    }

    #[test]
    fn offset_only_errors_drop_the_lengths() {
        let bytes = [0u8; 4];
        let mut r = Reader::<OffsetOnly>::new(&bytes);
        r.take(4).unwrap();
        assert_eq!(r.take(1).unwrap_err(), OffsetOnly { offset: 4 });
    }

    #[test]
    fn cstr_takes_the_terminator() {
        let bytes = b"road\0next";
        let mut r = Reader::<Shaped>::new(bytes);
        assert_eq!(r.take_cstr().unwrap(), b"road");
        assert_eq!(r.pos(), 5);
        // No terminator in what is left: the one-byte-past-the-end failure.
        assert_eq!(r.take_cstr().unwrap_err(), Shaped { truncated: Truncated { offset: 5, needed: 1, len: 9 } });
    }

    #[test]
    fn offset_helpers_match() {
        let bytes = [1u8, 0, 0, 0, 0, 0, 0x80, 0x3f];
        assert_eq!(read_u32_le(&bytes, 0).unwrap(), 1);
        assert_eq!(read_f32_le(&bytes, 4).unwrap(), 1.0);
        assert_eq!(read_u32_le(&bytes, 6).unwrap_err(), Truncated { offset: 6, needed: 4, len: 8 });
        assert!(need(&bytes, 0, 8).is_ok());
        assert_eq!(need(&bytes, 7, 4).unwrap_err(), Truncated { offset: 7, needed: 4, len: 8 });
        assert_eq!(u16_at(&bytes, 0), Some(1));
        assert_eq!(u32_at(&bytes, 0), Some(1));
        assert_eq!(f32_at(&bytes, 4), Some(1.0));
        assert_eq!(u16_at(&bytes, 8), None);
        assert_eq!(u32_at(&bytes, 6), None);
        assert_eq!(u32_of(&bytes), 1);
        assert_eq!(f32_of(&bytes[4..]), 1.0);
        assert_eq!(floats_of::<2>(&[0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f]), [1.0, 1.0]);
    }
}
