//! Reader for the call traces the harness writes in capture mode
//! (`RBRRecompTrace-<tick>.bin`, `CaptureC` in `harness/plugin/RBRRecompHarness.cpp`).
//!
//! The header is `RBTR`, version 1, and the number of calls logged per function. After
//! it come one record per logged call. All fields are little-endian:
//!
//! ```text
//! u32 'CAL1'     u32 addr   u32 index   u32 cw_in   u32 fpu_in
//! u32 x7         entry eax, ebx, ecx, edx, esi, edi, ebp
//! u32 x16        the first 16 stack argument words
//! u32 x7         return eax, ebx, ecx, edx, esi, edi, ebp
//! u32 popped     bytes popped from the stack by the return
//! u32 has_st0    st0 holds a value at return
//! f64 st0        that value (zero when has_st0 is 0)
//! u32 cw_out     x87 control word at return
//! u32 nregions
//! per region:    u32 base, u32 at, u32 size, u32 name_len, name bytes,
//!                size bytes before the call, size bytes after it
//! ```
//!
//! Control words are the 16-bit x87 value, zero-extended. `base` is the pointer a
//! region was found through, `at` the address it was copied from in the game.

use std::fmt::{self, Display, Formatter};

/// First word of the file: `'RBTR'` little-endian.
pub const MAGIC: u32 = 0x5254_4252;
pub const VERSION: u32 = 1;
/// First word of every call record: `'CAL1'` little-endian.
pub const CALL_MAGIC: u32 = 0x314c_4143;
/// Stack argument words stored per call.
pub const ARGS: usize = 16;

/// Reading failure, with the byte offset it occurred at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadMagic { found: u32 },
    UnsupportedVersion { found: u32 },
    BadCallMagic { offset: usize, found: u32 },
    InvalidUtf8 { offset: usize },
    Truncated { offset: usize, needed: usize, len: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic { found } => {
                write!(f, "bad trace magic {found:#010x}, expected {MAGIC:#010x}")
            }
            Self::UnsupportedVersion { found } => {
                write!(f, "unsupported trace version {found}, expected {VERSION}")
            }
            Self::BadCallMagic { offset, found } => {
                write!(f, "bad call magic {found:#010x} at offset {offset}")
            }
            Self::InvalidUtf8 { offset } => {
                write!(f, "invalid region name UTF-8 at offset {offset}")
            }
            Self::Truncated { offset, needed, len } => write!(
                f,
                "truncated trace at offset {offset}: need {needed} bytes but {} remain",
                len.saturating_sub(*offset)
            ),
        }
    }
}

impl std::error::Error for Error {}

/// The x87 precision-control field (bits 8-9 of the control word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// 24-bit significand (`0x??7f` style words with bits 8-9 clear).
    Single,
    Reserved,
    /// 53-bit significand, the Windows default `0x027f`.
    Double,
    /// 64-bit significand, `0x037f`.
    Extended,
}

/// The precision a control word selects.
pub fn precision(cw: u32) -> Precision {
    match (cw >> 8) & 3 {
        0 => Precision::Single,
        1 => Precision::Reserved,
        2 => Precision::Double,
        _ => Precision::Extended,
    }
}

/// General registers in the order the plugin writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Regs {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebp: u32,
}

/// One input region: its bytes before the call and after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region<'a> {
    pub base: u32,
    pub at: u32,
    /// Where it came from: a register or stack argument name.
    pub name: &'a str,
    pub pre: &'a [u8],
    pub post: &'a [u8],
}

/// One logged call of an original function.
#[derive(Debug, Clone, PartialEq)]
pub struct Call<'a> {
    /// Byte offset of this record's magic word.
    pub offset: usize,
    pub addr: u32,
    /// Zero-based index of the call among this function's calls.
    pub index: u32,
    pub cw_in: u32,
    /// The call arrived with a value on the x87 stack (not replayable from registers).
    pub fpu_in: bool,
    pub entry: Regs,
    pub args: [u32; ARGS],
    pub exit: Regs,
    pub popped: u32,
    pub has_st0: bool,
    pub st0: f64,
    pub cw_out: u32,
    pub regions: Vec<Region<'a>>,
}

impl Call<'_> {
    /// The region found through the named input, e.g. `"ecx"` or `"arg0"`.
    pub fn region(&self, name: &str) -> Option<&Region<'_>> {
        self.regions.iter().find(|region| region.name == name)
    }
}

/// Iterator over the calls in a trace buffer. Stops after the first error.
#[derive(Debug, Clone)]
pub struct Calls<'a> {
    bytes: &'a [u8],
    offset: usize,
    done: bool,
    /// Calls logged per function, from the header.
    pub calls_per_function: u32,
}

impl<'a> Calls<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut cursor = Cursor { bytes, offset: 0 };
        let magic = cursor.u32()?;
        if magic != MAGIC {
            return Err(Error::BadMagic { found: magic });
        }
        let version = cursor.u32()?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion { found: version });
        }
        let calls_per_function = cursor.u32()?;
        Ok(Self { bytes, offset: cursor.offset, done: false, calls_per_function })
    }
}

impl<'a> Iterator for Calls<'a> {
    type Item = Result<Call<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.offset == self.bytes.len() {
            return None;
        }
        let mut cursor = Cursor { bytes: self.bytes, offset: self.offset };
        match read_call(&mut cursor) {
            Ok(call) => {
                self.offset = cursor.offset;
                Some(Ok(call))
            }
            Err(error) => {
                self.done = true;
                Some(Err(error))
            }
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let bytes = self.bytes;
        let end = self.offset.checked_add(n).filter(|&end| end <= bytes.len()).ok_or(Error::Truncated {
            offset: self.offset,
            needed: n,
            len: bytes.len(),
        })?;
        let out = &bytes[self.offset..end];
        self.offset = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f64(&mut self) -> Result<f64, Error> {
        let b = self.take(8)?;
        let mut word = [0u8; 8];
        word.copy_from_slice(b);
        Ok(f64::from_le_bytes(word))
    }

    fn regs(&mut self) -> Result<Regs, Error> {
        Ok(Regs {
            eax: self.u32()?,
            ebx: self.u32()?,
            ecx: self.u32()?,
            edx: self.u32()?,
            esi: self.u32()?,
            edi: self.u32()?,
            ebp: self.u32()?,
        })
    }
}

fn read_call<'a>(c: &mut Cursor<'a>) -> Result<Call<'a>, Error> {
    let offset = c.offset;
    let magic = c.u32()?;
    if magic != CALL_MAGIC {
        return Err(Error::BadCallMagic { offset, found: magic });
    }
    let addr = c.u32()?;
    let index = c.u32()?;
    let cw_in = c.u32()?;
    let fpu_in = c.u32()? != 0;
    let entry = c.regs()?;
    let mut args = [0u32; ARGS];
    for arg in &mut args {
        *arg = c.u32()?;
    }
    let exit = c.regs()?;
    let popped = c.u32()?;
    let has_st0 = c.u32()? != 0;
    let st0 = c.f64()?;
    let cw_out = c.u32()?;
    let nregions = c.u32()?;
    let mut regions = Vec::new();
    for _ in 0..nregions {
        let base = c.u32()?;
        let at = c.u32()?;
        let size = c.u32()? as usize;
        let name_len = c.u32()? as usize;
        let name_offset = c.offset;
        let name = std::str::from_utf8(c.take(name_len)?).map_err(|_| Error::InvalidUtf8 { offset: name_offset })?;
        let pre = c.take(size)?;
        let post = c.take(size)?;
        regions.push(Region { base, at, name, pre, post });
    }
    Ok(Call { offset, addr, index, cw_in, fpu_in, entry, args, exit, popped, has_st0, st0, cw_out, regions })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(out: &mut Vec<u8>, word: u32) {
        out.extend_from_slice(&word.to_le_bytes());
    }

    fn sample() -> Vec<u8> {
        let mut out = Vec::new();
        push(&mut out, MAGIC);
        push(&mut out, VERSION);
        push(&mut out, 64);
        push(&mut out, CALL_MAGIC);
        push(&mut out, 0x1234_5678); // addr
        push(&mut out, 3); // index
        push(&mut out, 0x027f); // cw_in
        push(&mut out, 1); // fpu_in
        for word in 1..=7 {
            push(&mut out, word); // entry
        }
        for word in 11..=26 {
            push(&mut out, word); // args
        }
        for word in 21..=27 {
            push(&mut out, word); // exit
        }
        push(&mut out, 8); // popped
        push(&mut out, 1); // has_st0
        out.extend_from_slice(&1.5f64.to_le_bytes());
        push(&mut out, 0x027f); // cw_out
        push(&mut out, 1); // nregions
        push(&mut out, 0x0040_0000); // base
        push(&mut out, 0x0050_0000); // at
        push(&mut out, 2); // size
        push(&mut out, 3); // name length
        out.extend_from_slice(b"ecx");
        out.extend_from_slice(&[1, 2]); // pre
        out.extend_from_slice(&[3, 4]); // post
        out
    }

    #[test]
    fn reads_a_call_with_a_region() {
        let bytes = sample();
        let mut calls = Calls::new(&bytes).unwrap();
        assert_eq!(calls.calls_per_function, 64);
        let call = calls.next().unwrap().unwrap();
        assert!(calls.next().is_none());
        assert_eq!(call.offset, 12);
        assert_eq!((call.addr, call.index, call.cw_in), (0x1234_5678, 3, 0x027f));
        assert!(call.fpu_in);
        assert_eq!(call.entry.eax, 1);
        assert_eq!(call.entry.ebp, 7);
        assert_eq!(call.args[0], 11);
        assert_eq!(call.args[ARGS - 1], 26);
        assert_eq!(call.exit.ebp, 27);
        assert_eq!((call.popped, call.has_st0, call.st0), (8, true, 1.5));
        assert_eq!(call.cw_out, 0x027f);
        let region = call.region("ecx").unwrap();
        assert_eq!((region.base, region.at), (0x0040_0000, 0x0050_0000));
        assert_eq!((region.pre, region.post), (&[1u8, 2][..], &[3u8, 4][..]));
        assert!(call.region("edx").is_none());
    }

    #[test]
    fn rejects_bad_headers() {
        let mut bytes = sample();
        bytes[0] ^= 1;
        assert!(matches!(Calls::new(&bytes), Err(Error::BadMagic { .. })));
        let mut bytes = sample();
        bytes[4] = 2;
        assert_eq!(Calls::new(&bytes).err(), Some(Error::UnsupportedVersion { found: 2 }));
        assert!(matches!(Calls::new(&bytes[..6]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn stops_after_a_truncated_call() {
        let bytes = sample();
        let cut = &bytes[..bytes.len() - 1];
        let mut calls = Calls::new(cut).unwrap();
        assert!(matches!(calls.next(), Some(Err(Error::Truncated { .. }))));
        assert!(calls.next().is_none());
    }

    #[test]
    fn reports_a_bad_call_magic() {
        let mut bytes = sample();
        bytes[12] = 0;
        let mut calls = Calls::new(&bytes).unwrap();
        assert_eq!(calls.next(), Some(Err(Error::BadCallMagic { offset: 12, found: 0x314c_4100 })));
    }

    #[test]
    fn decodes_the_precision_field() {
        assert_eq!(precision(0x007f), Precision::Single);
        assert_eq!(precision(0x017f), Precision::Reserved);
        assert_eq!(precision(0x027f), Precision::Double);
        assert_eq!(precision(0x037f), Precision::Extended);
    }
}
