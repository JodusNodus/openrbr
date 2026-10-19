//! Reader for `.mat` surface-material grids.
//!
//! Layout (little-endian, from `notes/formats/stage-files.md` and the
//! loaders `FUN_0046a460` / `FUN_0046a290` in the official 1.02 exe):
//!
//! ```text
//! u32 entry_count
//! repeat entry_count:
//!   NUL-terminated name, e.g. "arctic standard dry new"
//!   u32 run_count
//!   repeat run_count:
//!     u32 width   (observed 16)
//!     u32 height  (observed 16)
//!     u8  cells[256]  (16x16 surface-class bytes, e.g. g/f/e/u/X)
//! ```
//!
//! Each run is a fixed 264-byte record. A run count of zero is valid: the
//! loader `FUN_0046a290` skips its run loop and can still select and emit that
//! named material entry. The loader keeps the first word,
//! the second word and a pointer to the 256 class bytes; this reader
//! preserves all three. Width/height are preserved as read (not validated)
//! so files with other dimensions still parse; every retail sample so far
//! uses 16x16.
//!
//! Bytes after the last entry are kept in [`MatFile::tail`]: the loader reads
//! the entries by their count and never looks past them. Community stages
//! append a `CMAP` block there (`notes/formats/rsf-stages.md`). All length
//! checks are bounds-checked; counts are never used for unchecked allocation.

use std::fmt::{self, Display, Formatter};

/// One 264-byte material run: grid dimensions plus the 16x16 class bytes.
///
/// Each class byte is a surface id: the loader (`FUN_0046a290`) files the
/// 256 bytes per run into the `CSigMaterialMaps` signal, `E44_TableBuild`
/// (`0x520190`) keeps the width byte and the cell pointer per run, and the
/// sampler (`0x520010`) reads `cells[clamp(floor(width * a))]` for the
/// barycentric-weighted `a` the collision queries pass (`0x4fe810` and
/// friends) — the first 16 bytes only, since the built height byte is 0
/// and the second float argument is never read. The id indexes the exe's
/// surface table at `0x7caf38` (`value = 88 * id + 3`; see
/// `notes/formats/col.md`). The height byte is written but never consumed
/// downstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatRun {
    pub width: u32,
    pub height: u32,
    pub cells: [u8; 256],
}

/// One named material entry with its runs. Empty `runs` means the on-disk
/// count was zero; the retail loader accepts this and emits an empty map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatEntry {
    pub name: String,
    pub runs: Vec<MatRun>,
}

/// A parsed `.mat` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatFile {
    pub entries: Vec<MatEntry>,
    /// The bytes after the last entry (none in retail), which the game does not read.
    pub tail: Vec<u8>,
}

/// Selects the first entry whose name contains both the wear word
/// (`"new"`/`"normal"`/`"worn"`) and the wetness word
/// (`"dry"`/`"damp"`/`"wet"`), mirroring `E09_LoadMaterials`
/// (`0x46a290`): it walks the entries in file order and emits the first
/// double `strstr` match (words from `E11_TyreCond`, `0x46f6c0`). A
/// zero-run entry still wins when it matches first, as in the six retail
/// USA files whose entries have no runs.
pub fn select(entries: &[MatEntry], wear: &str, wetness: &str) -> Option<usize> {
    entries.iter().position(|entry| entry.name.contains(wear) && entry.name.contains(wetness))
}

/// Observed grid dimensions; preserved, not enforced.
pub const GRID_WIDTH: u32 = 16;
pub const GRID_HEIGHT: u32 = 16;
/// Cells per run (16x16).
pub const GRID_CELLS: usize = 256;
/// Bytes per run on disk: two u32 dimensions plus the grid.
pub const RECORD_LEN: usize = 264;

/// Parse failure with the offending byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated { offset: usize, needed: usize, len: usize },
    MissingNul { offset: usize },
    InvalidName { offset: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed, len } => {
                write!(f, "truncated at offset {offset}: need {needed} bytes but file is {len} bytes")
            }
            Error::MissingNul { offset } => {
                write!(f, "missing NUL terminator for name at offset {offset}")
            }
            Error::InvalidName { offset } => {
                write!(f, "invalid UTF-8 in name at offset {offset}")
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

/// Parses a whole `.mat` buffer.
pub fn parse(bytes: &[u8]) -> Result<MatFile, Error> {
    if bytes.len() < 4 {
        return Err(Error::Truncated { offset: 0, needed: 4, len: bytes.len() });
    }
    let entry_count = read_u32_le(bytes, 0)?;
    let mut offset = 4usize;
    let mut entries = Vec::new();

    for _ in 0..entry_count {
        let name_start = offset;
        let nul = bytes[offset..].iter().position(|&b| b == 0).ok_or(Error::MissingNul { offset: name_start })?;
        let name_bytes = &bytes[offset..offset + nul];
        let name = std::str::from_utf8(name_bytes).map_err(|_| Error::InvalidName { offset: name_start })?.to_owned();
        offset += nul + 1;

        let run_count =
            read_u32_le(bytes, offset).map_err(|_| Error::Truncated { offset, needed: 4, len: bytes.len() })? as usize;
        offset += 4;

        let mut runs = Vec::new();
        for _ in 0..run_count {
            let end = offset.checked_add(RECORD_LEN).ok_or(Error::Truncated {
                offset,
                needed: RECORD_LEN,
                len: bytes.len(),
            })?;
            if end > bytes.len() {
                return Err(Error::Truncated { offset, needed: RECORD_LEN, len: bytes.len() });
            }
            let width = read_u32_le(bytes, offset)?;
            let height = read_u32_le(bytes, offset + 4)?;
            let mut cells = [0u8; GRID_CELLS];
            cells.copy_from_slice(&bytes[offset + 8..offset + RECORD_LEN]);
            runs.push(MatRun { width, height, cells });
            offset = end;
        }
        entries.push(MatEntry { name, runs });
    }

    Ok(MatFile { entries, tail: bytes[offset..].to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_u32(v: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn minimal_file() -> Vec<u8> {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"arctic standard dry new\0");
        encode_u32(1, &mut b);
        encode_u32(16, &mut b);
        encode_u32(16, &mut b);
        b.extend_from_slice(&[b'g'; 256]);
        b
    }

    #[test]
    fn minimal_round_trip() {
        let bytes = minimal_file();
        assert_eq!(bytes.len(), 4 + 24 + 4 + 264);
        let file = parse(&bytes).unwrap();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.entries[0].name, "arctic standard dry new");
        assert_eq!(file.entries[0].runs.len(), 1);
        assert_eq!(file.entries[0].runs[0].width, 16);
        assert_eq!(file.entries[0].runs[0].height, 16);
        assert_eq!(file.entries[0].runs[0].cells, [b'g'; 256]);
    }

    #[test]
    fn zero_entries() {
        let bytes = 0u32.to_le_bytes().to_vec();
        let file = parse(&bytes).unwrap();
        assert!(file.entries.is_empty());
    }

    #[test]
    fn zero_run_material_is_preserved() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"USA 2002 standard dry new\0");
        encode_u32(0, &mut b);
        let file = parse(&b).unwrap();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.entries[0].name, "USA 2002 standard dry new");
        assert!(file.entries[0].runs.is_empty());
    }

    #[test]
    fn empty_rejected() {
        assert!(matches!(parse(&[]), Err(Error::Truncated { offset: 0, .. })));
        assert!(matches!(parse(&[1, 0, 0]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn missing_nul() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"no-terminator");
        assert!(matches!(parse(&b), Err(Error::MissingNul { offset: 4 })));
    }

    #[test]
    fn invalid_utf8_name() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(&[0xff, 0xfe, 0x00]);
        assert!(matches!(parse(&b), Err(Error::InvalidName { offset: 4 })));
    }

    #[test]
    fn truncated_run_count() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"a\0");
        b.extend_from_slice(&[1, 2]);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn truncated_record() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"a\0");
        encode_u32(2, &mut b);
        encode_u32(16, &mut b);
        encode_u32(16, &mut b);
        b.extend_from_slice(&[0u8; 100]);
        assert!(matches!(parse(&b), Err(Error::Truncated { needed: 264, .. })));
    }

    #[test]
    fn huge_run_count_does_not_hang_or_oom() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"a\0");
        encode_u32(u32::MAX, &mut b);
        assert!(matches!(parse(&b), Err(Error::Truncated { .. })));
    }

    #[test]
    fn trailing_bytes_are_kept_unread() {
        let mut b = minimal_file();
        b.extend_from_slice(b"CMAP");
        let file = parse(&b).unwrap();
        assert_eq!(file.entries.len(), 1);
        assert_eq!(file.tail, b"CMAP");
    }

    #[test]
    fn select_first_double_match() {
        let entries = vec![
            MatEntry { name: "arctic standard dry new".to_owned(), runs: vec![] },
            MatEntry { name: "arctic standard damp new".to_owned(), runs: vec![] },
            MatEntry { name: "arctic standard dry worn".to_owned(), runs: vec![] },
        ];
        // First entry in file order wins, even with no runs.
        assert_eq!(select(&entries, "new", "dry"), Some(0));
        assert_eq!(select(&entries, "new", "damp"), Some(1));
        // Substring (`strstr`) semantics, case-sensitive.
        assert_eq!(select(&entries, "ew", "ry"), Some(0));
        assert_eq!(select(&entries, "New", "dry"), None);
        assert_eq!(select(&entries, "new", "snow"), None);
        assert_eq!(select(&[], "new", "dry"), None);
    }

    #[test]
    fn non_16_dimensions_preserved() {
        let mut b = Vec::new();
        encode_u32(1, &mut b);
        b.extend_from_slice(b"a\0");
        encode_u32(1, &mut b);
        encode_u32(8, &mut b);
        encode_u32(8, &mut b);
        b.extend_from_slice(&[0u8; 256]);
        let file = parse(&b).unwrap();
        assert_eq!(file.entries[0].runs[0].width, 8);
        assert_eq!(file.entries[0].runs[0].height, 8);
    }
}
