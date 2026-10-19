//! InstallShield cabinets (`data1.hdr` with `data1.cab`, `data2.cab`, ...), as the retail Richard Burns Rally disc
//! and the official 1.02 patch ship their files: the file groups, the folders and file descriptors of the header, and
//! each file's bytes, stored or compressed (raw deflate in chunks), also when split over two volumes. The patch's
//! cabinets come inside an InstallShield 2002 self-extracting `.exe` ([`sfx_files`]).
//!
//! Ported from unshield (<https://github.com/twogood/unshield>, MIT licence, Copyright (c) 2003 David Eriksson), only
//! what these two installers need: header versions 6 and later (both are version 7), no components, and no
//! obfuscated or external files. Older cabinets (InstallShield 5 and before) are refused. A file's MD5 is checked
//! as unshield checks it.

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Range;

use flate2::{Decompress, FlushDecompress, Status};

use crate::pe;

const SIGNATURE: u32 = 0x2863_5349;
const SPLIT: u16 = 1;
const OBFUSCATED: u16 = 2;
const COMPRESSED: u16 = 4;
const INVALID: u16 = 8;
const LINK_PREVIOUS: u8 = 1;

fn invalid(what: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.into())
}

fn u16_at(b: &[u8], at: usize) -> io::Result<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| invalid("cabinet header cut short"))
}

fn u32_at(b: &[u8], at: usize) -> io::Result<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| invalid("cabinet header cut short"))
}

fn u64_at(b: &[u8], at: usize) -> io::Result<u64> {
    b.get(at..at + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| invalid("cabinet header cut short"))
}

/// The NUL-terminated string at `at`, one char per byte (the names are ANSI).
fn string_at(b: &[u8], at: usize) -> io::Result<String> {
    let s = b.get(at..).ok_or_else(|| invalid("cabinet header cut short"))?;
    Ok(s.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect())
}

/// A file group: a named range of files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    /// The indices of its files in [`Cabinet::files`]; empty for a group without files.
    pub files: Range<usize>,
}

/// A file descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    /// Its folder, as the header names it (`Maps\Textures`; empty for the group's top).
    pub dir: String,
    /// The extracted size.
    pub size: u64,
    flags: u16,
    compressed: u64,
    offset: u64,
    md5: [u8; 16],
    volume: u16,
    link_previous: u32,
    link_flags: u8,
}

impl File {
    /// Whether it holds a file: unshield skips the others.
    pub fn is_valid(&self) -> bool {
        self.flags & INVALID == 0 && !self.name.is_empty() && self.offset != 0
    }
}

/// A cabinet set's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cabinet {
    pub groups: Vec<Group>,
    pub files: Vec<File>,
}

impl Cabinet {
    /// Reads the header (`data1.hdr`).
    pub fn parse(hdr: &[u8]) -> io::Result<Cabinet> {
        if u32_at(hdr, 0)? != SIGNATURE {
            return Err(invalid("not an InstallShield cabinet header"));
        }
        let version = u32_at(hdr, 4)?;
        let major = match version >> 24 {
            1 => (version >> 12) & 0xf,
            2 | 4 => (version & 0xffff) / 100,
            _ => 0,
        };
        if major < 6 {
            return Err(invalid(format!("InstallShield cabinet version {version:#010x} is not supported")));
        }
        let base = u32_at(hdr, 12)? as usize;
        let table = base + u32_at(hdr, base + 0xc)? as usize;
        let dirs = u32_at(hdr, base + 0x1c)? as usize;
        let count = u32_at(hdr, base + 0x28)? as usize;
        let descriptors = table + u32_at(hdr, base + 0x2c)? as usize;
        let dir_names = (0..dirs)
            .map(|k| string_at(hdr, table + u32_at(hdr, table + 4 * k)? as usize))
            .collect::<Result<Vec<_>, _>>()?;
        let files = (0..count)
            .map(|k| {
                let at = descriptors + 0x57 * k;
                let d = hdr.get(at..at + 0x57).ok_or_else(|| invalid("cabinet header cut short"))?;
                let dir = u16_at(d, 0x3e)? as usize;
                Ok(File {
                    name: string_at(hdr, table + u32_at(d, 0x3a)? as usize)?,
                    dir: dir_names.get(dir).cloned().ok_or_else(|| invalid("a file in no folder"))?,
                    flags: u16_at(d, 0)?,
                    size: u64_at(d, 2)?,
                    compressed: u64_at(d, 0xa)?,
                    offset: u64_at(d, 0x12)?,
                    md5: d[0x1a..0x2a].try_into().unwrap(),
                    link_previous: u32_at(d, 0x4c)?,
                    link_flags: d[0x54],
                    volume: u16_at(d, 0x55)?,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        // 71 chains of file groups, after the descriptor's fixed part.
        let mut groups = Vec::new();
        for k in 0..71 {
            let mut next = u32_at(hdr, base + 0x3e + 4 * k)? as usize;
            while next != 0 {
                let descriptor = base + u32_at(hdr, base + next + 4)? as usize;
                next = u32_at(hdr, base + next + 8)? as usize;
                let name = string_at(hdr, base + u32_at(hdr, descriptor)? as usize)?;
                let (first, last) = (u32_at(hdr, descriptor + 0x16)? as i32, u32_at(hdr, descriptor + 0x1a)? as i32);
                let files = if first < 0 || last < first { 0..0 } else { first as usize..last as usize + 1 };
                if files.end > count {
                    return Err(invalid(format!("the file group {name} lies past the files")));
                }
                groups.push(Group { name, files });
            }
        }
        Ok(Cabinet { groups, files })
    }

    /// The group named `name` (exactly, as unshield's `-g`).
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.name == name)
    }

    /// Writes file `index` to `out`, reading the volumes `open(n)` gives (`data<n>.cab`); checks its size and MD5.
    pub fn extract<R: Read + Seek, W: Write>(
        &self,
        index: usize,
        open: &mut impl FnMut(u16) -> io::Result<R>,
        out: &mut W,
    ) -> io::Result<u64> {
        let mut file = &self.files[index];
        if file.link_flags & LINK_PREVIOUS != 0 {
            file = self.files.get(file.link_previous as usize).ok_or_else(|| invalid("a link to no file"))?;
        }
        let what = |e: &str| invalid(format!("{}: {e}", file.name));
        if file.flags & OBFUSCATED != 0 {
            return Err(what("obfuscated files are not supported"));
        }
        let mut data = Volumes::open(file, index, open)?;
        let mut md5 = Md5::default();
        let mut buffer = vec![0u8; 1 << 16];
        let mut written = 0u64;
        if file.flags & COMPRESSED != 0 {
            // Chunks of raw deflate, each with its 16-bit length and inflated on its own.
            let mut inflate = Decompress::new(false);
            let mut left = file.compressed;
            while left > 0 {
                let mut len = [0u8; 2];
                data.read_exact(&mut len)?;
                let len = u16::from_le_bytes(len) as usize;
                if len == 0 || len as u64 + 2 > left {
                    return Err(what("a bad compressed chunk"));
                }
                let mut chunk = vec![0u8; len];
                data.read_exact(&mut chunk)?;
                left -= len as u64 + 2;
                inflate.reset(false);
                loop {
                    let (read, before) = (inflate.total_in() as usize, inflate.total_out());
                    let status = inflate
                        .decompress(&chunk[read..], &mut buffer, FlushDecompress::Sync)
                        .map_err(|e| what(&e.to_string()))?;
                    let n = (inflate.total_out() - before) as usize;
                    md5.update(&buffer[..n]);
                    out.write_all(&buffer[..n])?;
                    written += n as u64;
                    if inflate.total_in() as usize == chunk.len() || status == Status::StreamEnd {
                        break;
                    }
                    if n == 0 && inflate.total_in() as usize == read {
                        return Err(what("a compressed chunk that does not inflate"));
                    }
                }
            }
        } else {
            let mut left = file.size;
            while left > 0 {
                let n = left.min(buffer.len() as u64) as usize;
                data.read_exact(&mut buffer[..n])?;
                md5.update(&buffer[..n]);
                out.write_all(&buffer[..n])?;
                left -= n as u64;
                written += n as u64;
            }
        }
        if written != file.size {
            return Err(what(&format!("{written} bytes, the header says {}", file.size)));
        }
        if md5.finish() != file.md5 {
            return Err(what("its MD5 does not match"));
        }
        Ok(written)
    }
}

/// A file's stored bytes, read across the volumes it is split over.
struct Volumes<'a, R, F> {
    open: &'a mut F,
    file: &'a File,
    index: usize,
    volume: u16,
    reader: R,
    /// The file's bytes left in this volume.
    left: u64,
}

impl<'a, R: Read + Seek, F: FnMut(u16) -> io::Result<R>> Volumes<'a, R, F> {
    fn open(file: &'a File, index: usize, open: &'a mut F) -> io::Result<Self> {
        let (reader, left) = Self::volume(file, index, file.volume, open)?;
        Ok(Volumes { open, file, index, volume: file.volume, reader, left })
    }

    /// Volume `n`, at the file's bytes in it, and their length.
    fn volume(file: &File, index: usize, n: u16, open: &mut F) -> io::Result<(R, u64)> {
        let mut reader = open(n)?;
        let mut header = [0u8; 20 + 64];
        reader.read_exact(&mut header)?;
        if u32_at(&header, 0)? != SIGNATURE {
            return Err(invalid(format!("data{n}.cab is not an InstallShield cabinet")));
        }
        let v = &header[20..];
        let stored = |expanded: u64, compressed: u64| if file.flags & COMPRESSED != 0 { compressed } else { expanded };
        let (at, left) = if file.flags & SPLIT == 0 {
            (file.offset, stored(file.size, file.compressed))
        } else if index as u64 == u32_at(v, 12)? as u64 && u32_at(v, 40)? != 0x7fff_ffff {
            // The volume's last file, continued in the next one.
            (u64_at(v, 40)?, stored(u64_at(v, 48)?, u64_at(v, 56)?))
        } else if index as u64 == u32_at(v, 8)? as u64 {
            // The volume's first file, begun in the one before.
            (u64_at(v, 16)?, stored(u64_at(v, 24)?, u64_at(v, 32)?))
        } else {
            return Err(invalid(format!("{} is not in data{n}.cab", file.name)));
        };
        reader.seek(SeekFrom::Start(at))?;
        Ok((reader, left))
    }
}

impl<R: Read + Seek, F: FnMut(u16) -> io::Result<R>> Read for Volumes<'_, R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.left == 0 && !buf.is_empty() {
            self.volume += 1;
            (self.reader, self.left) = Self::volume(self.file, self.index, self.volume, self.open)?;
        }
        let len = buf.len().min(self.left.min(usize::MAX as u64) as usize);
        let n = self.reader.read(&mut buf[..len])?;
        if n == 0 && len > 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.left -= n as u64;
        Ok(n)
    }
}

/// The files of an InstallShield 2002 self-extracting `.exe`: its overlay (after the PE sections) holds, per file,
/// `name\0 path\0 version\0 size\0` and then the file's bytes. Returns each file's path (`Disk1\data1.hdr`) and its
/// bytes' range in `exe`.
pub fn sfx_files(exe: &[u8]) -> io::Result<Vec<(String, Range<usize>)>> {
    let mut at = pe::overlay_offset(exe).map_err(|e| invalid(e.to_string()))?;
    let mut files = Vec::new();
    while at < exe.len() {
        let mut fields: [String; 4] = Default::default();
        for field in &mut fields {
            let len = exe[at..].iter().position(|&c| c == 0).ok_or_else(|| invalid("a cut setup entry"))?;
            *field = exe[at..at + len].iter().map(|&c| c as char).collect();
            at += len + 1;
        }
        let size: usize = fields[3].parse().map_err(|_| invalid("a setup entry's size is not a number"))?;
        let end = at.checked_add(size).filter(|&end| end <= exe.len()).ok_or_else(|| invalid("a cut setup entry"))?;
        files.push((std::mem::take(&mut fields[1]), at..end));
        at = end;
    }
    Ok(files)
}

/// MD5's per-step constants, `floor(abs(sin(i + 1)) * 2^32)`.
const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8,
    0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340,
    0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87,
    0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039,
    0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92,
    0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
    0xeb86d391,
];

/// MD5 (RFC 1321), for the files' checksums.
#[derive(Clone)]
struct Md5 {
    state: [u32; 4],
    block: Vec<u8>,
    len: u64,
}

impl Default for Md5 {
    fn default() -> Md5 {
        Md5 { state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476], block: Vec::with_capacity(64), len: 0 }
    }
}

impl Md5 {
    fn update(&mut self, mut bytes: &[u8]) {
        self.len += bytes.len() as u64;
        if !self.block.is_empty() {
            let n = (64 - self.block.len()).min(bytes.len());
            self.block.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            if self.block.len() < 64 {
                return;
            }
            let block: [u8; 64] = self.block[..].try_into().unwrap();
            self.compress(&block);
            self.block.clear();
        }
        let mut blocks = bytes.chunks_exact(64);
        for block in &mut blocks {
            self.compress(block.try_into().unwrap());
        }
        self.block.extend_from_slice(blocks.remainder());
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const S: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
        let m: [u32; 16] = std::array::from_fn(|k| u32::from_le_bytes(block[4 * k..4 * k + 4].try_into().unwrap()));
        let [mut a, mut b, mut c, mut d] = self.state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]).rotate_left(S[i / 16 * 4 + i % 4]);
            (a, d, c) = (d, c, b);
            b = b.wrapping_add(rotated);
        }
        for (s, v) in self.state.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }

    fn finish(mut self) -> [u8; 16] {
        let bits = self.len.wrapping_mul(8);
        self.update(&[0x80]);
        while self.block.len() != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_le_bytes());
        let mut out = [0u8; 16];
        for (k, s) in self.state.iter().enumerate() {
            out[4 * k..4 * k + 4].copy_from_slice(&s.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compress, Compression, FlushCompress};
    use std::io::Cursor;

    fn md5(bytes: &[u8]) -> [u8; 16] {
        let mut m = Md5::default();
        m.update(bytes);
        m.finish()
    }

    fn hex(b: [u8; 16]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn md5_matches_rfc_1321() {
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        let long = b"12345678901234567890123456789012345678901234567890123456789012345678901234567890";
        assert_eq!(hex(md5(long)), "57edf4a22be3c955ac49da2e2107b67a");
    }

    /// Raw deflate chunks of up to 64 KiB of `bytes`, each with its length, as the cabinets store them.
    fn chunks(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for part in bytes.chunks(1 << 16) {
            let mut z = Compress::new(Compression::default(), false);
            let mut chunk = Vec::with_capacity(part.len() + 64);
            z.compress_vec(part, &mut chunk, FlushCompress::Sync).unwrap();
            out.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            out.extend_from_slice(&chunk);
        }
        out
    }

    /// A file for [`header`]: name, folder index, flags, size, stored size, offset, volume, its bytes' MD5.
    type Spec = (&'static str, u16, u16, u64, u64, u64, u16, [u8; 16]);

    /// A version 7 header with folders `dirs`, the files `files` and the groups (name, first, last file).
    fn header(dirs: &[&str], files: &[Spec], groups: &[(&str, i32, i32)]) -> Vec<u8> {
        let base = 0x200usize;
        let mut h = vec![0u8; 0x4000];
        h[0..4].copy_from_slice(&SIGNATURE.to_le_bytes());
        h[4..8].copy_from_slice(&0x0100_7000u32.to_le_bytes());
        h[12..16].copy_from_slice(&(base as u32).to_le_bytes());
        let put32 = |h: &mut Vec<u8>, at: usize, v: u32| h[at..at + 4].copy_from_slice(&v.to_le_bytes());
        // The file table at base + 0x400: the folders' name offsets, names from 0x100, descriptors from 0x800.
        let table = 0x400;
        put32(&mut h, base + 0xc, table as u32);
        put32(&mut h, base + 0x1c, dirs.len() as u32);
        put32(&mut h, base + 0x28, files.len() as u32);
        put32(&mut h, base + 0x2c, 0x800);
        let mut names = 0x100;
        let mut name = |h: &mut Vec<u8>, s: &str| {
            let at = names;
            h[base + table + at..base + table + at + s.len()].copy_from_slice(s.as_bytes());
            names += s.len() + 1;
            at as u32
        };
        for (k, d) in dirs.iter().enumerate() {
            let at = name(&mut h, d);
            put32(&mut h, base + table + 4 * k, at);
        }
        for (k, &(file, dir, flags, size, stored, offset, volume, md5)) in files.iter().enumerate() {
            let at = name(&mut h, file);
            let d = base + table + 0x800 + 0x57 * k;
            h[d..d + 2].copy_from_slice(&flags.to_le_bytes());
            h[d + 2..d + 10].copy_from_slice(&size.to_le_bytes());
            h[d + 0xa..d + 0x12].copy_from_slice(&stored.to_le_bytes());
            h[d + 0x12..d + 0x1a].copy_from_slice(&offset.to_le_bytes());
            h[d + 0x1a..d + 0x2a].copy_from_slice(&md5);
            put32(&mut h, d + 0x3a, at);
            h[d + 0x3e..d + 0x40].copy_from_slice(&dir.to_le_bytes());
            h[d + 0x55..d + 0x57].copy_from_slice(&volume.to_le_bytes());
        }
        // One chain of groups: list nodes at 0x2000 + 12 k, descriptors at 0x2400 + 0x40 k, names at 0x2800.
        put32(&mut h, base + 0x3e, if groups.is_empty() { 0 } else { 0x2000 });
        for (k, &(group, first, last)) in groups.iter().enumerate() {
            let (node, descriptor, at) = (0x2000 + 12 * k, 0x2400 + 0x40 * k, 0x2800 + 0x20 * k);
            h[base + at..base + at + group.len()].copy_from_slice(group.as_bytes());
            put32(&mut h, base + node + 4, descriptor as u32);
            put32(&mut h, base + node + 8, if k + 1 < groups.len() { (node + 12) as u32 } else { 0 });
            put32(&mut h, base + descriptor, at as u32);
            put32(&mut h, base + descriptor + 0x16, first as u32);
            put32(&mut h, base + descriptor + 0x1a, last as u32);
        }
        h
    }

    /// A volume: the common header, the volume header (first file: index, offset, sizes; last file: the same) and the
    /// data from 0x200.
    fn volume(first: (u32, u64, u64, u64), last: (u32, u64, u64, u64), data: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; 0x200];
        v[0..4].copy_from_slice(&SIGNATURE.to_le_bytes());
        let h = 20;
        v[h + 8..h + 12].copy_from_slice(&first.0.to_le_bytes());
        v[h + 12..h + 16].copy_from_slice(&last.0.to_le_bytes());
        for (at, value) in [(16, first.1), (24, first.2), (32, first.3), (40, last.1), (48, last.2), (56, last.3)] {
            v[h + at..h + at + 8].copy_from_slice(&value.to_le_bytes());
        }
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn extracts_stored_compressed_and_split_files() {
        let stored = b"stored bytes".to_vec();
        let big: Vec<u8> = (0..200_000u32).map(|k| (k * 7 % 251) as u8).collect();
        let packed = chunks(&big);
        let (head, tail) = packed.split_at(1000);
        let data1 = [&stored[..], head].concat();
        let split_at = 0x200 + stored.len() as u64;
        let files: [Spec; 3] = [
            ("a.txt", 0, 0, stored.len() as u64, stored.len() as u64, 0x200, 1, md5(&stored)),
            ("big.dat", 1, COMPRESSED | SPLIT, big.len() as u64, packed.len() as u64, split_at, 1, md5(&big)),
            ("copy.txt", 1, 0, 0, 0, 0x200, 1, [0; 16]),
        ];
        let mut hdr = header(&["", "Sub\\Dir"], &files, &[("Main group", 0, 2), ("_Empty", -1, -1)]);
        // copy.txt links to a.txt, as the installers store duplicates.
        let d = 0x200 + 0x400 + 0x800 + 2 * 0x57;
        hdr[d + 0x4c..d + 0x50].copy_from_slice(&0u32.to_le_bytes());
        hdr[d + 0x54] = LINK_PREVIOUS;
        let cab = Cabinet::parse(&hdr).unwrap();
        assert_eq!(
            cab.groups,
            [Group { name: "Main group".into(), files: 0..3 }, Group { name: "_Empty".into(), files: 0..0 }]
        );
        assert_eq!((cab.files[1].name.as_str(), cab.files[1].dir.as_str()), ("big.dat", "Sub\\Dir"));
        let volumes = [
            volume((0, 0, 0, 0), (1, split_at, 0, head.len() as u64), &data1),
            volume((1, 0x200, 0, tail.len() as u64), (1, 0x7fff_ffff, 0, 0), tail),
        ];
        let mut open = |n: u16| Ok(Cursor::new(volumes[n as usize - 1].clone()));
        let extract = |k: usize, open: &mut dyn FnMut(u16) -> io::Result<Cursor<Vec<u8>>>| {
            let mut out = Vec::new();
            cab.extract(k, &mut |n| open(n), &mut out).map(|_| out)
        };
        assert_eq!(extract(0, &mut open).unwrap(), stored);
        assert_eq!(extract(1, &mut open).unwrap(), big);
        assert_eq!(extract(2, &mut open).unwrap(), stored);
    }

    #[test]
    fn a_wrong_checksum_and_old_versions_are_refused() {
        let bytes = b"abc".to_vec();
        let hdr = header(&[""], &[("a", 0, 0, 3, 3, 0x200, 1, [0; 16])], &[]);
        let cab = Cabinet::parse(&hdr).unwrap();
        let data = volume((0, 0, 0, 0), (0, 0, 0, 0), &bytes);
        let err = cab.extract(0, &mut |_| Ok(Cursor::new(data.clone())), &mut Vec::new()).unwrap_err();
        assert!(err.to_string().contains("MD5"), "{err}");
        let mut old = hdr.clone();
        old[4..8].copy_from_slice(&0x0100_5000u32.to_le_bytes());
        assert!(Cabinet::parse(&old).unwrap_err().to_string().contains("not supported"));
        assert!(Cabinet::parse(b"MSCF").is_err());
    }

    #[test]
    fn reads_the_setup_overlay() {
        let mut exe = vec![0u8; 0x200];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        exe[0x40..0x44].copy_from_slice(b"PE\0\0");
        exe[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
        // No optional header; one section with its raw data at 0x100, 0x100 bytes.
        let s = 0x40 + 24;
        exe[s + 16..s + 20].copy_from_slice(&0x100u32.to_le_bytes());
        exe[s + 20..s + 24].copy_from_slice(&0x100u32.to_le_bytes());
        exe.extend_from_slice(b"data1.hdr\0Disk1\\data1.hdr\x001.0\x003\x00abc");
        exe.extend_from_slice(b"setup.ini\0Disk1\\setup.ini\0\x002\x00hi");
        let files = sfx_files(&exe).unwrap();
        assert_eq!(
            files.iter().map(|(p, r)| (p.as_str(), &exe[r.clone()])).collect::<Vec<_>>(),
            [("Disk1\\data1.hdr", &b"abc"[..]), ("Disk1\\setup.ini", &b"hi"[..])]
        );
        exe.truncate(exe.len() - 1);
        assert!(sfx_files(&exe).is_err());
    }
}
