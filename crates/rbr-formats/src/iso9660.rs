//! ISO 9660 disc images, read from a `Read + Seek` without mounting: the volume descriptors, the directory tree, and
//! each file as a slice of the image ([`Extent`]). The names are the Joliet ones (UCS-2) when the disc has a Joliet
//! volume, else the plain ISO 9660 ones; both lose their `;1` version, and a plain name its bare trailing dot.
//!
//! Only what the retail Richard Burns Rally disc needs: the first session, files in one extent (no multi-extent files
//! of 4 GB and more, no interleaving), no Rock Ridge or UDF. The primary volume descriptor's reserved bytes are not
//! looked at: the retail disc has non-zero bytes there (1395-2047), which libarchive refuses.

use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};

/// The logical sector size: descriptors, directories and extents start on these.
pub const SECTOR: u64 = 2048;

/// A file or folder of the image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path from the root, `/`-separated (`SETUP/DATA1.HDR`).
    pub path: String,
    pub dir: bool,
    /// The byte offset of its extent in the image, and its length.
    pub start: u64,
    pub size: u64,
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("not an ISO 9660 image: {what}"))
}

fn read_at<R: Read + Seek>(image: &mut R, at: u64, len: usize) -> io::Result<Vec<u8>> {
    image.seek(SeekFrom::Start(at))?;
    let mut bytes = vec![0; len];
    image.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// A directory record's extent: its start and length in bytes, and whether it is a folder.
fn extent(record: &[u8]) -> (u64, u64, bool) {
    (u32_at(record, 2) as u64 * SECTOR, u32_at(record, 10) as u64, record[25] & 2 != 0)
}

/// Every file and folder of the image, parents before their children.
pub fn read_tree<R: Read + Seek>(image: &mut R) -> io::Result<Vec<Entry>> {
    let (mut primary, mut joliet) = (None, None);
    // The descriptor set starts at sector 16 and ends with a terminator (type 255).
    for sector in 16..80 {
        let d = read_at(image, sector * SECTOR, SECTOR as usize)?;
        if &d[1..6] != b"CD001" {
            return Err(invalid("no volume descriptor set"));
        }
        match d[0] {
            1 => primary = Some(extent(&d[156..190])),
            2 if matches!(&d[88..91], b"%/@" | b"%/C" | b"%/E") => joliet = Some(extent(&d[156..190])),
            255 => break,
            _ => {}
        }
    }
    let (start, size, _) = joliet.or(primary).ok_or_else(|| invalid("no primary volume descriptor"))?;
    let mut entries = Vec::new();
    // Extents already walked: directory records can point back at an ancestor, which re-walked every level
    // would expand exponentially.
    let mut visited = HashSet::from([start]);
    walk(image, start, size, "", joliet.is_some(), &mut entries, &mut visited, 0)?;
    Ok(entries)
}

fn walk<R: Read + Seek>(
    image: &mut R,
    start: u64,
    size: u64,
    prefix: &str,
    joliet: bool,
    entries: &mut Vec<Entry>,
    visited: &mut HashSet<u64>,
    depth: u32,
) -> io::Result<()> {
    if depth > 16 || size > 16 << 20 {
        return Err(invalid("a directory too deep or too large"));
    }
    let records = read_at(image, start, size as usize)?;
    let mut at = 0;
    while at < records.len() {
        let len = records[at] as usize;
        if len == 0 {
            // The rest of the sector is padding: records do not cross sectors.
            at = (at / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        let record = records.get(at..at + len).filter(|r| r.len() >= 33).ok_or_else(|| invalid("a short record"))?;
        at += len;
        let raw = record.get(33..33 + record[32] as usize).ok_or_else(|| invalid("a short record"))?;
        // The folder itself and its parent.
        if raw == [0] || raw == [1] {
            continue;
        }
        let name = if joliet {
            String::from_utf16_lossy(&raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>())
        } else {
            raw.iter().map(|&b| b as char).collect()
        };
        let name = name.split(';').next().unwrap_or_default();
        let name = if joliet { name } else { name.strip_suffix('.').unwrap_or(name) };
        let (start, size, dir) = extent(record);
        let path = if prefix.is_empty() { name.to_owned() } else { format!("{prefix}/{name}") };
        entries.push(Entry { path: path.clone(), dir, start, size });
        if dir && visited.insert(start) {
            walk(image, start, size, &path, joliet, entries, visited, depth + 1)?;
        }
    }
    Ok(())
}

/// One file of the image as a reader of its own: `Read + Seek` over its extent.
pub struct Extent<R> {
    image: R,
    start: u64,
    size: u64,
    at: u64,
}

impl<R: Read + Seek> Extent<R> {
    pub fn new(mut image: R, entry: &Entry) -> io::Result<Extent<R>> {
        image.seek(SeekFrom::Start(entry.start))?;
        Ok(Extent { image, start: entry.start, size: entry.size, at: 0 })
    }
}

impl<R: Read + Seek> Read for Extent<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min((self.size - self.at) as usize);
        let n = self.image.read(&mut buf[..len])?;
        self.at += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for Extent<R> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let at = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(n) => self.size.checked_add_signed(n),
            SeekFrom::Current(n) => self.at.checked_add_signed(n),
        };
        let at = at.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        self.image.seek(SeekFrom::Start(self.start + at))?;
        self.at = at.min(self.size);
        Ok(self.at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A directory record for `name` at `sector`, `size` bytes long.
    fn record(name: &[u8], sector: u32, size: u32, dir: bool) -> Vec<u8> {
        let mut r = vec![0u8; 33];
        r[2..6].copy_from_slice(&sector.to_le_bytes());
        r[10..14].copy_from_slice(&size.to_le_bytes());
        r[25] = if dir { 2 } else { 0 };
        r[32] = name.len() as u8;
        r.extend_from_slice(name);
        if r.len() % 2 == 1 {
            r.push(0);
        }
        r[0] = r.len() as u8;
        r
    }

    fn put(image: &mut [u8], sector: usize, bytes: &[u8]) {
        image[sector * 2048..sector * 2048 + bytes.len()].copy_from_slice(bytes);
    }

    fn descriptor(kind: u8, root: &[u8]) -> Vec<u8> {
        let mut d = vec![0u8; 2048];
        d[0] = kind;
        d[1..6].copy_from_slice(b"CD001");
        d[156..156 + root.len()].copy_from_slice(root);
        d
    }

    /// Root (sector 20): `SUB/` (21) and `DATA1.HDR;1` (22, "hello"); `SUB` holds `A.;1` (23, "abc"). With
    /// `joliet`, a Joliet root (24) names them `data1.hdr`, `Sub/` (25) and `a`.
    fn image(joliet: bool) -> Vec<u8> {
        let mut image = vec![0u8; 26 * 2048];
        let mut pvd = descriptor(1, &record(&[0], 20, 2048, true));
        // Non-zero reserved bytes, as on the retail disc.
        pvd[1395..].fill(0xaa);
        put(&mut image, 16, &pvd);
        let mut next = 17;
        if joliet {
            let mut svd = descriptor(2, &record(&[0], 24, 2048, true));
            svd[88..91].copy_from_slice(b"%/E");
            put(&mut image, next, &svd);
            next += 1;
        }
        put(&mut image, next, &descriptor(255, &[]));
        let dir = |records: &[Vec<u8>]| records.concat();
        let ucs2 = |s: &str| s.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>();
        put(
            &mut image,
            20,
            &dir(&[
                record(&[0], 20, 2048, true),
                record(&[1], 20, 2048, true),
                record(b"SUB", 21, 2048, true),
                record(b"DATA1.HDR;1", 22, 5, false),
            ]),
        );
        put(
            &mut image,
            21,
            &dir(&[record(&[0], 21, 2048, true), record(&[1], 20, 2048, true), record(b"A.;1", 23, 3, false)]),
        );
        put(&mut image, 22, b"hello");
        put(&mut image, 23, b"abc");
        put(
            &mut image,
            24,
            &dir(&[
                record(&[0], 24, 2048, true),
                record(&[1], 24, 2048, true),
                record(&ucs2("Sub"), 25, 2048, true),
                record(&ucs2("data1.hdr;1"), 22, 5, false),
            ]),
        );
        put(
            &mut image,
            25,
            &dir(&[record(&[0], 25, 2048, true), record(&[1], 24, 2048, true), record(&ucs2("a"), 23, 3, false)]),
        );
        image
    }

    fn paths(entries: &[Entry]) -> Vec<(&str, bool)> {
        entries.iter().map(|e| (e.path.as_str(), e.dir)).collect()
    }

    /// A minimal image: the root (sector 18) with one file (sector 19, `"abc"`).
    fn tiny_image() -> Vec<u8> {
        let mut image = vec![0u8; 20 * 2048];
        put(&mut image, 16, &descriptor(1, &record(&[0], 18, 2048, true)));
        put(&mut image, 17, &descriptor(255, &[]));
        put(
            &mut image,
            18,
            &[record(&[0], 18, 2048, true), record(&[1], 18, 2048, true), record(b"A.;1", 19, 3, false)]
                .concat(),
        );
        put(&mut image, 19, b"abc");
        image
    }

    #[test]
    fn reads_the_plain_tree_despite_the_reserved_bytes() {
        let entries = read_tree(&mut Cursor::new(image(false))).unwrap();
        assert_eq!(paths(&entries), [("SUB", true), ("SUB/A", false), ("DATA1.HDR", false)]);
        let mut text = String::new();
        Extent::new(Cursor::new(image(false)), &entries[2]).unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn prefers_the_joliet_names() {
        let entries = read_tree(&mut Cursor::new(image(true))).unwrap();
        assert_eq!(paths(&entries), [("Sub", true), ("Sub/a", false), ("data1.hdr", false)]);
    }

    #[test]
    fn an_extent_reads_and_seeks_inside_its_file_only() {
        let entry = Entry { path: "A".into(), dir: false, start: 23 * 2048, size: 3 };
        let mut file = Extent::new(Cursor::new(image(false)), &entry).unwrap();
        assert_eq!(file.seek(SeekFrom::End(-2)).unwrap(), 1);
        let mut rest = Vec::new();
        file.read_to_end(&mut rest).unwrap();
        assert_eq!(rest, b"bc");
    }

    #[test]
    fn damaged_images_never_panic() {
        let image = tiny_image();
        crate::damage::never_panics(&image, &[0, 0x7f, 0xff], |b| {
            let _ = read_tree(&mut Cursor::new(b));
        });
    }

    #[test]
    fn directories_pointing_back_do_not_expand() {
        // A folder pointing back at the root: the old walk re-read it at every level, blowing up.
        let mut image = tiny_image();
        image[18 * 2048..19 * 2048].fill(0);
        put(
            &mut image,
            18,
            &[record(&[0], 18, 2048, true), record(&[1], 18, 2048, true), record(b"SUB", 18, 2048, true)]
                .concat(),
        );
        assert_eq!(paths(&read_tree(&mut Cursor::new(image)).unwrap()), [("SUB", true)]);
        // A two-folder loop: the root holds A, A holds B, B points back at A.
        let mut image = vec![0u8; 22 * 2048];
        put(&mut image, 16, &descriptor(1, &record(&[0], 18, 2048, true)));
        put(&mut image, 17, &descriptor(255, &[]));
        put(
            &mut image,
            18,
            &[record(&[0], 18, 2048, true), record(&[1], 18, 2048, true), record(b"A", 20, 2048, true)]
                .concat(),
        );
        put(
            &mut image,
            20,
            &[record(&[0], 20, 2048, true), record(&[1], 18, 2048, true), record(b"B", 21, 2048, true)]
                .concat(),
        );
        put(
            &mut image,
            21,
            &[record(&[0], 21, 2048, true), record(&[1], 20, 2048, true), record(b"BACK", 20, 2048, true)]
                .concat(),
        );
        assert_eq!(
            paths(&read_tree(&mut Cursor::new(image)).unwrap()),
            [("A", true), ("A/B", true), ("A/B/BACK", true)]
        );
    }

    #[test]
    fn rejects_what_is_not_an_image() {
        assert!(read_tree(&mut Cursor::new(vec![0u8; 40 * 2048])).is_err());
        assert!(read_tree(&mut Cursor::new(b"PK".to_vec())).is_err());
    }
}
