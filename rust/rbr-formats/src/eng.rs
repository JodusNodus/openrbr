//! Reader for the engine sound banks (`Audio/Cars/*.eng` inside `Root/audio.dat`), the granular engine's samples:
//! one recording per rpm and load, with the sample position of every firing in it. Layout, little-endian, as
//! `CEngine::Vf04` (0x4bc370), `CEnginePart::Vf04` (0x4bc0b0) and `SOUND::CWave::Vf04` (0x4dd6b0) read it
//! (`notes/formats.md` "Audio"):
//!
//! ```text
//! "ENGI" u32 version (0x100, 0x101) u32 nbanks u32 nparts[nbanks]
//! part[sum nparts]:  "PART" u32 version u32 rpm u32 load u32 unused
//!                    "WAVE" u32 version u32 rate u32 nsamples i16 samples[nsamples] u32 ncycles (u32 whole, u32 frac)[ncycles]
//! version != 0x100:  u32 n u32 a[n] u32 b[n]
//! ```
//!
//! The readers skip the tags without checking them. Four retail files (`Saxo`, `bubbla`, `fiat`, `vaux`) hold two
//! `WAVE` blocks per part, an older layout 1.02 cannot load; [`parse`] refuses them ([`Error::TwoWaveParts`]).

use std::fmt::{self, Display, Formatter};

/// One recording: a bank's rpm at one load.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub rpm: u32,
    /// 0 or 100 in retail.
    pub load: u32,
    /// Sample rate, Hz.
    pub rate: u32,
    /// Mono 16-bit samples.
    pub samples: Vec<i16>,
    /// The start of every firing, in samples (16.16 in the file).
    pub cycles: Vec<f64>,
}

/// The parts of one rpm, in file order (retail: load 0, then load 100).
#[derive(Debug, Clone, PartialEq)]
pub struct Bank {
    pub parts: Vec<Part>,
}

impl Bank {
    /// The bank's rpm: its first part's.
    pub fn rpm(&self) -> u32 {
        self.parts.first().map_or(0, |p| p.rpm)
    }
}

/// A whole `.eng` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Engine {
    pub version: u32,
    pub banks: Vec<Bank>,
    /// The trailing tables of version 0x101 (`CEngine +0x1050`/`+0x1054`; meaning not traced).
    pub table_a: Vec<u32>,
    pub table_b: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file ends inside the field at this offset.
    Truncated { offset: usize },
    /// A part's wave is followed by another `WAVE` block: the older two-wave layout.
    TwoWaveParts,
    /// Bytes are left after the last field.
    TrailingBytes { offset: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset } => write!(f, "truncated at {offset:#x}"),
            Error::TwoWaveParts => write!(f, "two WAVE blocks per part (the pre-1.02 layout)"),
            Error::TrailingBytes { offset } => write!(f, "trailing bytes from {offset:#x}"),
        }
    }
}

impl std::error::Error for Error {}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], Error> {
        let end =
            self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or(Error::Truncated { offset: self.at })?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u32s(&mut self, n: u32) -> Result<Vec<u32>, Error> {
        (0..n).map(|_| self.u32()).collect()
    }
}

/// Reads a whole `.eng` file.
pub fn parse(bytes: &[u8]) -> Result<Engine, Error> {
    let mut c = Cursor { bytes, at: 0 };
    c.take(4)?;
    let version = c.u32()?;
    let nbanks = c.u32()?;
    let nparts = c.u32s(nbanks)?;
    let mut banks = Vec::with_capacity(nbanks as usize);
    for &n in &nparts {
        let mut parts = Vec::with_capacity(n as usize);
        for _ in 0..n {
            c.take(8)?;
            let rpm = c.u32()?;
            let load = c.u32()?;
            c.u32()?;
            c.take(8)?;
            let rate = c.u32()?;
            let nsamples = c.u32()? as usize;
            let raw = c.take(nsamples.checked_mul(2).ok_or(Error::Truncated { offset: c.at })?)?;
            let samples = raw.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]])).collect();
            let ncycles = c.u32()?;
            let rows = c.u32s(ncycles.checked_mul(2).ok_or(Error::Truncated { offset: c.at })?)?;
            let cycles = rows.chunks_exact(2).map(|r| f64::from(r[0]) + f64::from(r[1]) / 65536.0).collect();
            if bytes.get(c.at..c.at + 4) == Some(b"WAVE") {
                return Err(Error::TwoWaveParts);
            }
            parts.push(Part { rpm, load, rate, samples, cycles });
        }
        banks.push(Bank { parts });
    }
    let (table_a, table_b) = if version == 0x100 {
        (Vec::new(), Vec::new())
    } else {
        let n = c.u32()?;
        (c.u32s(n)?, c.u32s(n)?)
    };
    if c.at != bytes.len() {
        return Err(Error::TrailingBytes { offset: c.at });
    }
    Ok(Engine { version, banks, table_a, table_b })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(out: &mut Vec<u8>, rpm: u32, load: u32, waves: usize) {
        out.extend_from_slice(b"PART");
        for w in [0x100, rpm, load, 0xcdcd_cdcd] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        for _ in 0..waves {
            out.extend_from_slice(b"WAVE");
            for w in [0x100u32, 44100, 3] {
                out.extend_from_slice(&w.to_le_bytes());
            }
            for s in [1i16, -2, 3] {
                out.extend_from_slice(&s.to_le_bytes());
            }
            for w in [2u32, 1, 0, 2, 0x8000] {
                out.extend_from_slice(&w.to_le_bytes());
            }
        }
    }

    fn file(waves: usize) -> Vec<u8> {
        let mut out = b"ENGI".to_vec();
        for w in [0x101u32, 1, 2] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        part(&mut out, 1700, 0, waves);
        part(&mut out, 1700, 100, waves);
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    #[test]
    fn reads_banks_parts_and_cycles() {
        let engine = parse(&file(1)).unwrap();
        assert_eq!(engine.banks.len(), 1);
        let bank = &engine.banks[0];
        assert_eq!(bank.rpm(), 1700);
        assert_eq!(bank.parts[1].load, 100);
        assert_eq!(bank.parts[0].samples, [1, -2, 3]);
        assert_eq!(bank.parts[0].cycles, [1.0, 2.5]);
    }

    #[test]
    fn refuses_two_wave_parts_and_trailing_bytes() {
        assert_eq!(parse(&file(2)), Err(Error::TwoWaveParts));
        let mut extra = file(1);
        extra.push(0);
        assert_eq!(parse(&extra), Err(Error::TrailingBytes { offset: extra.len() - 1 }));
        assert!(matches!(parse(&file(1)[..30]), Err(Error::Truncated { .. })));
    }
}
