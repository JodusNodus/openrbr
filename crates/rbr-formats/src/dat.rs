//! Reader for the game's `CHELALIC` archives: `Physics/physics.dat` (inside `physics.rbz`) and the
//! loose `Textures/{particles,splash,fence}.dat`.
//!
//! Layout, little-endian, verified on all four retail archives (`notes/formats.md` "Cars"):
//!
//! - Header, 16 bytes: the 8-byte magic `CHELALIC`, a u32 record count `N` at 8, and a u32
//!   `data_start` at 12. The game compares the magic with `_stricmp` and reads `data_start - 16`
//!   bytes of records after the header (`E15_FileInner::Check`, `src/rbr/RBR_E15_File.cpp`).
//! - Records, `N` in all, depth-first from offset 16. Each record is a 20-byte fixed part and a
//!   name of `name_len` bytes (NUL-padded, a multiple of 4): `u32 offset`, `u32 size`,
//!   `u32 sibling_count` (the group size on the first record of each group), `u32 child_count`
//!   (non-zero for a directory: its children follow), `u32 name_len`.
//! - File data: `data_start + offset`, `size` bytes. Directories have offset and size 0.
//!
//! Only the record table is read here; the data is sliced on demand by [`file_data`].

use std::fmt::{self, Display, Formatter};

pub const MAGIC: &[u8; 8] = b"CHELALIC";
const HEADER_LEN: usize = 16;
const FIXED_LEN: usize = 20;

/// One record, with its full `/`-separated path from the archive root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    /// Directory (`child_count != 0`) or file.
    pub is_dir: bool,
    /// Offset of the data relative to `data_start`.
    pub offset: u32,
    pub size: u32,
}

/// Failure to read the record table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Not at least a 16-byte header, or not the `CHELALIC` magic.
    BadHeader,
    /// `data_start` lies outside the file, or before the header.
    DataStartOutOfRange,
    /// A record runs past `data_start`.
    Truncated { offset: usize },
    /// The record count in the header does not match the walk.
    CountMismatch { header: u32, walked: u32 },
    /// The records do not end exactly at `data_start`.
    RecordsEndMismatch { end: usize, data_start: usize },
    /// A file's data range lies past the end of the file.
    DataOutOfRange { path: String },
    /// A group whose first record declares no siblings.
    EmptyRootGroup,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadHeader => write!(f, "not a CHELALIC archive"),
            Error::DataStartOutOfRange => write!(f, "data start outside the file"),
            Error::Truncated { offset } => write!(f, "record at {offset:#x} runs past the table"),
            Error::CountMismatch { header, walked } => {
                write!(f, "header lists {header} records, walk found {walked}")
            }
            Error::RecordsEndMismatch { end, data_start } => {
                write!(f, "records end at {end:#x}, data starts at {data_start:#x}")
            }
            Error::DataOutOfRange { path } => write!(f, "{path}: data past end of file"),
            Error::EmptyRootGroup => write!(f, "first record declares no siblings"),
        }
    }
}

impl std::error::Error for Error {}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Reads the record table of a whole archive. Entries come out in file order.
pub fn parse(bytes: &[u8]) -> Result<Vec<Entry>, Error> {
    // The game compares the magic with _stricmp, so the match ignores ASCII case.
    if bytes.len() < HEADER_LEN || !bytes[..8].eq_ignore_ascii_case(MAGIC) {
        return Err(Error::BadHeader);
    }
    let header_count = u32_at(bytes, 8).ok_or(Error::BadHeader)?;
    let data_start = u32_at(bytes, 12).ok_or(Error::BadHeader)? as usize;
    if data_start < HEADER_LEN || data_start > bytes.len() {
        return Err(Error::DataStartOutOfRange);
    }
    // The root group holds the first record's sibling count (one top-level directory in every
    // retail archive).
    let root = u32_at(bytes, HEADER_LEN + 8).ok_or(Error::Truncated { offset: HEADER_LEN })?;
    if root == 0 {
        return Err(Error::EmptyRootGroup);
    }

    // Depth-first walk. `open` holds, per directory being read, the children still to come and
    // the directory's path prefix. Every record takes at least FIXED_LEN bytes, so the walk is
    // bounded by the table size, and the explicit stack avoids recursion.
    let mut entries: Vec<Entry> = Vec::new();
    let mut pos = HEADER_LEN;
    let mut open: Vec<(u32, String)> = vec![(root, String::new())];
    while let Some((remaining, prefix)) = open.last_mut() {
        if *remaining == 0 {
            open.pop();
            continue;
        }
        *remaining -= 1;
        let at = pos;
        let truncated = Error::Truncated { offset: at };
        let fixed_end = at.checked_add(FIXED_LEN).ok_or(truncated.clone())?;
        if fixed_end > data_start {
            return Err(truncated);
        }
        let offset = u32_at(bytes, at).ok_or(truncated.clone())?;
        let size = u32_at(bytes, at + 4).ok_or(truncated.clone())?;
        let children = u32_at(bytes, at + 12).ok_or(truncated.clone())?;
        let name_len = u32_at(bytes, at + 16).ok_or(truncated.clone())? as usize;
        let name_end = fixed_end.checked_add(name_len).ok_or(truncated.clone())?;
        if name_end > data_start {
            return Err(truncated);
        }
        let name_raw = bytes[fixed_end..name_end].split(|&b| b == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name_raw).into_owned();
        let path = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        pos = name_end;
        if entries.len() >= header_count as usize {
            return Err(Error::CountMismatch { header: header_count, walked: header_count + 1 });
        }
        let is_dir = children != 0;
        if is_dir {
            open.push((children, path.clone()));
        }
        entries.push(Entry { path, is_dir, offset, size });
    }
    let walked = entries.len() as u32;
    if walked != header_count {
        return Err(Error::CountMismatch { header: header_count, walked });
    }
    if pos != data_start {
        return Err(Error::RecordsEndMismatch { end: pos, data_start });
    }
    for e in entries.iter().filter(|e| !e.is_dir) {
        let end = data_start as u64 + e.offset as u64 + e.size as u64;
        if end > bytes.len() as u64 {
            return Err(Error::DataOutOfRange { path: e.path.clone() });
        }
    }
    Ok(entries)
}

/// The data of `entry` within the whole archive `bytes`. Panic-free: `None` if out of range.
pub fn file_data<'a>(bytes: &'a [u8], entry: &Entry) -> Option<&'a [u8]> {
    if entry.is_dir {
        return None;
    }
    let data_start = u32_at(bytes, 12)? as usize;
    let start = data_start.checked_add(entry.offset as usize)?;
    let end = start.checked_add(entry.size as usize)?;
    bytes.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One record: 20-byte fixed part, then the NUL-padded name.
    fn record(name: &str, offset: u32, size: u32, siblings: u32, children: u32) -> Vec<u8> {
        let name_len = (name.len() + 1).next_multiple_of(4);
        let mut out = Vec::new();
        for v in [offset, size, siblings, children, name_len as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(name.as_bytes());
        out.resize(out.len() + name_len - name.len(), 0);
        out
    }

    /// Header (count, data_start computed from the table) + records + data.
    fn archive(count: u32, records: &[Vec<u8>], data: &[u8]) -> Vec<u8> {
        let table: Vec<u8> = records.concat();
        let data_start = (HEADER_LEN + table.len()) as u32;
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&data_start.to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(data);
        out
    }

    /// root/ { a.txt, sub/ { b.txt } }
    fn tree() -> (Vec<Vec<u8>>, Vec<u8>) {
        let records = vec![
            record("root", 0, 0, 1, 2),
            record("a.txt", 0, 3, 2, 0),
            record("sub", 0, 0, 0, 1),
            record("b.txt", 3, 2, 1, 0),
        ];
        (records, b"xyzhi".to_vec())
    }

    #[test]
    fn walks_nested_directories_in_file_order() {
        let (records, data) = tree();
        let bytes = archive(4, &records, &data);
        let entries = parse(&bytes).unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["root", "root/a.txt", "root/sub", "root/sub/b.txt"]);
        assert_eq!(entries.iter().map(|e| e.is_dir).collect::<Vec<_>>(), [true, false, true, false]);
        assert_eq!(file_data(&bytes, &entries[1]), Some(&b"xyz"[..]));
        assert_eq!(file_data(&bytes, &entries[3]), Some(&b"hi"[..]));
        assert_eq!(file_data(&bytes, &entries[0]), None);
    }

    #[test]
    fn name_is_cut_at_first_nul_and_padding_is_skipped() {
        let records = vec![record("r", 0, 0, 1, 1), record("abcdefgh", 0, 1, 1, 0)];
        let bytes = archive(2, &records, b"x");
        let entries = parse(&bytes).unwrap();
        assert_eq!(entries[1].path, "r/abcdefgh");
    }

    #[test]
    fn rejects_bad_magic_and_short_header() {
        let (records, data) = tree();
        let mut bytes = archive(4, &records, &data);
        bytes[0] = b'X';
        assert_eq!(parse(&bytes).unwrap_err(), Error::BadHeader);
        assert_eq!(parse(&bytes[..10]).unwrap_err(), Error::BadHeader);
        assert_eq!(parse(&[]).unwrap_err(), Error::BadHeader);
    }

    #[test]
    fn magic_is_case_insensitive_like_stricmp() {
        let (records, data) = tree();
        let mut bytes = archive(4, &records, &data);
        bytes[..8].copy_from_slice(b"chelalic");
        assert_eq!(parse(&bytes).unwrap().len(), 4);
    }

    #[test]
    fn data_start_out_of_range() {
        let (records, data) = tree();
        let mut bytes = archive(4, &records, &data);
        bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&bytes).unwrap_err(), Error::DataStartOutOfRange);
        bytes[12..16].copy_from_slice(&4u32.to_le_bytes());
        assert_eq!(parse(&bytes).unwrap_err(), Error::DataStartOutOfRange);
    }

    #[test]
    fn count_mismatch_both_ways() {
        let (records, data) = tree();
        assert_eq!(parse(&archive(3, &records, &data)).unwrap_err(), Error::CountMismatch { header: 3, walked: 4 });
        // A huge header count must not allocate or loop: the walk stops at the table end.
        assert!(matches!(parse(&archive(u32::MAX, &records, &data)).unwrap_err(), Error::CountMismatch { .. }));
    }

    #[test]
    fn record_past_table_is_truncated() {
        let records = vec![record("root", 0, 0, 1, 1), record("file", 0, 0, 1, 0)];
        let mut bytes = archive(2, &records, b"");
        // Pull data_start in so the second record's name runs past it.
        let ds = (HEADER_LEN + records[0].len() + 10) as u32;
        bytes[12..16].copy_from_slice(&ds.to_le_bytes());
        assert!(matches!(parse(&bytes).unwrap_err(), Error::Truncated { .. }));
    }

    #[test]
    fn records_must_end_at_data_start() {
        let (records, data) = tree();
        let mut bytes = archive(4, &records, &data);
        let ds = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        bytes[12..16].copy_from_slice(&(ds + 4).to_le_bytes());
        assert!(matches!(parse(&bytes).unwrap_err(), Error::RecordsEndMismatch { .. }));
    }

    #[test]
    fn file_data_past_end_of_file_is_rejected() {
        let records = vec![record("root", 0, 0, 1, 1), record("f", 0, 100, 1, 0)];
        let bytes = archive(2, &records, b"short");
        assert_eq!(parse(&bytes).unwrap_err(), Error::DataOutOfRange { path: "root/f".to_owned() });
    }

    #[test]
    fn empty_root_group_is_rejected() {
        let records = vec![record("root", 0, 0, 0, 0)];
        let bytes = archive(1, &records, b"");
        assert_eq!(parse(&bytes).unwrap_err(), Error::EmptyRootGroup);
    }

    #[test]
    fn file_data_never_panics() {
        let entry = Entry { path: "x".to_owned(), is_dir: false, offset: u32::MAX, size: u32::MAX };
        assert_eq!(file_data(&[0u8; 32], &entry), None);
        assert_eq!(file_data(&[], &entry), None);
    }
}
