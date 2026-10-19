//! Reader for the game's `.rbz` archives.
//!
//! An `.rbz` file is a plain ZIP archive using only method 0 (stored) and 8 (deflate), with no
//! encryption or extra fields. The game reads them with a statically linked minizip `unzip.c`.
//! See `notes/formats/rbz.md` for the evidence.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

pub use zip::read::ZipFile;
pub use zip::result::ZipError as Error;
pub use zip::ZipArchive;

/// Opens an `.rbz` archive. Reading an entry to the end verifies its CRC-32.
pub fn open(path: &Path) -> Result<ZipArchive<BufReader<File>>, Error> {
    ZipArchive::new(BufReader::new(File::open(path)?))
}

/// The entry `name` (`Physics/physics.lsp`), its case and separators ignored: the game finds its files in archives
/// stored with `\` too (RallySimFans rewrites `physics.rbz` so).
pub fn by_name<'a, R: Read + Seek>(zip: &'a mut ZipArchive<R>, name: &str) -> Result<ZipFile<'a, R>, Error> {
    let same = |entry: &str| {
        entry.len() == name.len()
            && entry
                .bytes()
                .zip(name.bytes())
                .all(|(a, b)| a.eq_ignore_ascii_case(&b) || matches!((a, b), (b'/', b'\\') | (b'\\', b'/')))
    };
    match zip.index_for_name(name).or_else(|| zip.file_names().position(|entry| entry.is_ok_and(|entry| same(&entry))))
    {
        Some(index) => zip.by_index(index),
        None => Err(Error::FileNotFound),
    }
}
