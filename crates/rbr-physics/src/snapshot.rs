//! Reader for the runtime harness memory snapshots (`harness/replay.py::Snapshot`).
//!
//! The file is memory-mapped and region bytes are read on demand. Reads stay backed by the
//! mapping; writes are kept in private 4 KB pages that are populated only when first written.

use crate::mem::{Backend, Mem};
use memmap2::{Mmap, MmapOptions};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::path::Path;
use std::sync::{Arc, Mutex};

const MAGIC: u32 = 0x5352_4252; // 'RBRS'
const HEADER_WORDS: usize = 17;
const HEADER_BYTES: usize = HEADER_WORDS * 4;
const FXSAVE_BYTES: usize = 512;
const REGION_BYTES: usize = 24;
const PAGE_SHIFT: u32 = 12;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

#[derive(Debug)]
pub enum SnapshotError {
    Io(io::Error),
    Truncated(&'static str),
    BadMagic(u32),
    UnknownVersion(u32),
    InvalidLayout(&'static str),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "snapshot I/O error: {error}"),
            Self::Truncated(part) => write!(f, "truncated snapshot {part}"),
            Self::BadMagic(found) => write!(f, "bad snapshot magic {found:#010x}"),
            Self::UnknownVersion(version) => write!(f, "unknown snapshot version {version}"),
            Self::InvalidLayout(reason) => write!(f, "invalid snapshot layout: {reason}"),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for SnapshotError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registers {
    pub eax: u32,
    pub ecx: u32,
    pub edx: u32,
    pub ebx: u32,
    pub esp: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
}

impl Registers {
    pub fn words(self) -> [u32; 8] {
        [self.eax, self.ecx, self.edx, self.ebx, self.esp, self.ebp, self.esi, self.edi]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotHeader {
    pub version: u32,
    pub step: u32,
    pub substeps: u32,
    pub thread: u32,
    pub teb: u32,
    pub registers: Registers,
    pub stack_lo: u32,
    pub stack_len: u32,
    pub returns: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Region {
    pub base: u32,
    pub size: u32,
    pub file_offset: u32,
    pub protect: u32,
    pub kind: u32,
    pub allocation_base: u32,
}

struct SnapshotData {
    file: Mmap,
    header: SnapshotHeader,
    fxsave: [u8; FXSAVE_BYTES],
    stack: Vec<u8>,
    regions: Vec<Region>,
}

/// A parsed snapshot. Cloning shares the mapping and immutable metadata.
#[derive(Clone)]
pub struct Snapshot {
    data: Arc<SnapshotData>,
}

impl Snapshot {
    /// Opens and validates a version 1 step snapshot or version 2 load-time dump.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SnapshotError> {
        let file = File::open(path)?;
        // SAFETY: snapshots are immutable artifacts while they are being read. The mapping is
        // retained for every read and is never exposed mutably.
        let mapped = unsafe { MmapOptions::new().map(&file)? };
        let bytes = &mapped[..];
        if bytes.len() < 8 {
            return Err(SnapshotError::Truncated("trailer"));
        }
        let trailer = bytes.len() - 8;
        let header_offset = read_u32(bytes, trailer)? as usize;
        let trailer_magic = read_u32(bytes, trailer + 4)?;
        if trailer_magic != MAGIC {
            return Err(SnapshotError::BadMagic(trailer_magic));
        }
        let header_end =
            header_offset.checked_add(HEADER_BYTES).ok_or(SnapshotError::InvalidLayout("header offset overflow"))?;
        if header_end > trailer {
            return Err(SnapshotError::Truncated("header"));
        }
        if read_u32(bytes, header_offset)? != MAGIC {
            return Err(SnapshotError::BadMagic(read_u32(bytes, header_offset)?));
        }
        let version = read_u32(bytes, header_offset + 4)?;
        if !matches!(version, 1 | 2) {
            return Err(SnapshotError::UnknownVersion(version));
        }
        let words: [u32; HEADER_WORDS] = std::array::from_fn(|i| {
            read_u32(bytes, header_offset + i * 4).expect("validated fixed-size snapshot header")
        });
        let stack_lo = words[14];
        let stack_len = words[15];
        let region_count = words[16] as usize;
        if (stack_lo as u64) + (stack_len as u64) > (u32::MAX as u64) + 1 {
            return Err(SnapshotError::InvalidLayout("stack address overflow"));
        }

        let fx_offset = header_end;
        let stack_offset =
            fx_offset.checked_add(FXSAVE_BYTES).ok_or(SnapshotError::InvalidLayout("fxsave offset overflow"))?;
        let regions_offset = stack_offset
            .checked_add(stack_len as usize)
            .ok_or(SnapshotError::InvalidLayout("stack offset overflow"))?;
        let table_len =
            region_count.checked_mul(REGION_BYTES).ok_or(SnapshotError::InvalidLayout("region table size overflow"))?;
        let table_end = regions_offset
            .checked_add(table_len)
            .ok_or(SnapshotError::InvalidLayout("region table offset overflow"))?;
        if table_end > trailer {
            return Err(SnapshotError::Truncated("region table or stack"));
        }

        let fxsave = bytes[fx_offset..stack_offset].try_into().expect("fixed-size fxsave range");
        let stack = bytes[stack_offset..regions_offset].to_vec();
        let mut regions = Vec::with_capacity(region_count);
        for i in 0..region_count {
            let at = regions_offset + i * REGION_BYTES;
            let region = Region {
                base: read_u32(bytes, at)?,
                size: read_u32(bytes, at + 4)?,
                file_offset: read_u32(bytes, at + 8)?,
                protect: read_u32(bytes, at + 12)?,
                kind: read_u32(bytes, at + 16)?,
                allocation_base: read_u32(bytes, at + 20)?,
            };
            if (region.base as u64) + (region.size as u64) > (u32::MAX as u64) + 1 {
                return Err(SnapshotError::InvalidLayout("region address overflow"));
            }
            let data_end = (region.file_offset as usize)
                .checked_add(region.size as usize)
                .ok_or(SnapshotError::InvalidLayout("region file range overflow"))?;
            if data_end > header_offset {
                return Err(SnapshotError::InvalidLayout("region bytes overlap snapshot metadata"));
            }
            regions.push(region);
        }
        regions.sort_unstable();

        Ok(Self {
            data: Arc::new(SnapshotData {
                file: mapped,
                header: SnapshotHeader {
                    version,
                    step: words[2],
                    substeps: words[3],
                    thread: words[4],
                    teb: words[5],
                    registers: Registers {
                        eax: words[6],
                        ecx: words[7],
                        edx: words[8],
                        ebx: words[9],
                        esp: words[10],
                        ebp: words[11],
                        esi: words[12],
                        edi: words[13],
                    },
                    stack_lo,
                    stack_len,
                    returns: version == 2 && stack_len == 0,
                },
                fxsave,
                stack,
                regions,
            }),
        })
    }

    pub fn metadata(&self) -> SnapshotHeader {
        self.data.header
    }

    pub fn fxsave(&self) -> &[u8; FXSAVE_BYTES] {
        &self.data.fxsave
    }

    pub fn stack(&self) -> &[u8] {
        &self.data.stack
    }

    pub fn regions(&self) -> &[Region] {
        &self.data.regions
    }

    /// Returns a region-backed memory view. Snapshot bytes stay mmap-backed until written.
    pub fn mem(&self) -> Mem {
        Mem::with_backend(Arc::new(PagedMemory { data: self.data.clone(), pages: Mutex::new(BTreeMap::new()) }))
    }
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, SnapshotError> {
    let word = bytes.get(at..at + 4).ok_or(SnapshotError::Truncated("word"))?;
    Ok(u32::from_le_bytes(word.try_into().expect("four-byte slice")))
}

struct PagedMemory {
    data: Arc<SnapshotData>,
    pages: Mutex<BTreeMap<u32, Box<[u8; PAGE_SIZE]>>>,
}

impl PagedMemory {
    fn region_index(&self, at: u32) -> Option<usize> {
        let next = self.data.regions.partition_point(|region| region.base <= at);
        next.checked_sub(1).filter(|&index| {
            let region = self.data.regions[index];
            (at as u64) < region.base as u64 + region.size as u64
        })
    }

    fn is_stack_byte(&self, at: u32) -> bool {
        let start = self.data.header.stack_lo as u64;
        (at as u64) >= start && (at as u64) < start + self.data.header.stack_len as u64
    }

    fn load_page(&self, page_base: u32) -> Box<[u8; PAGE_SIZE]> {
        let mut page = Box::new([0; PAGE_SIZE]);
        let page_lo = page_base as u64;
        let page_hi = page_lo + PAGE_SIZE as u64;
        for region in &self.data.regions {
            let lo = page_lo.max(region.base as u64);
            let hi = page_hi.min(region.base as u64 + region.size as u64);
            if lo < hi {
                let file_at = region.file_offset as usize + (lo - region.base as u64) as usize;
                let dest_at = (lo - page_lo) as usize;
                let len = (hi - lo) as usize;
                page[dest_at..dest_at + len].copy_from_slice(&self.data.file[file_at..file_at + len]);
            }
        }
        let stack_lo = page_lo.max(self.data.header.stack_lo as u64);
        let stack_hi = page_hi.min(self.data.header.stack_lo as u64 + self.data.header.stack_len as u64);
        if stack_lo < stack_hi {
            let src_at = (stack_lo - self.data.header.stack_lo as u64) as usize;
            let dest_at = (stack_lo - page_lo) as usize;
            let len = (stack_hi - stack_lo) as usize;
            page[dest_at..dest_at + len].copy_from_slice(&self.data.stack[src_at..src_at + len]);
        }
        page
    }
}

impl Backend for PagedMemory {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        let end = at as u64 + out.len() as u64;
        if end > (u32::MAX as u64) + 1 {
            return false;
        }
        let pages = self.pages.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut cursor = at as u64;
        while cursor < end {
            let address = cursor as u32;
            let stack = self.is_stack_byte(address);
            let region_index = self.region_index(address);
            if !stack && region_index.is_none() {
                return false;
            }
            let page_base = address & !((PAGE_SIZE as u32) - 1);
            let page_end = page_base as u64 + PAGE_SIZE as u64;
            let source_end = if stack {
                self.data.header.stack_lo as u64 + self.data.header.stack_len as u64
            } else {
                let region_index = region_index.expect("checked above");
                let region = self.data.regions[region_index];
                let mut region_end = region.base as u64 + region.size as u64;
                if let Some(next_region) = self.data.regions.get(region_index + 1) {
                    let next_base = next_region.base as u64;
                    if next_base > cursor && next_base < region_end {
                        region_end = next_base;
                    }
                }
                let stack_start = self.data.header.stack_lo as u64;
                if stack_start > cursor && stack_start < region_end {
                    stack_start
                } else {
                    region_end
                }
            };
            let chunk_end = end.min(page_end).min(source_end);
            let count = (chunk_end - cursor) as usize;
            let dest_at = (cursor - at as u64) as usize;
            if let Some(page) = pages.get(&(address >> PAGE_SHIFT)) {
                let page_at = (address - page_base) as usize;
                out[dest_at..dest_at + count].copy_from_slice(&page[page_at..page_at + count]);
            } else if stack {
                let src_at = (cursor - self.data.header.stack_lo as u64) as usize;
                out[dest_at..dest_at + count].copy_from_slice(&self.data.stack[src_at..src_at + count]);
            } else {
                let region = self.data.regions[region_index.expect("checked above")];
                let src_at = region.file_offset as usize + (cursor - region.base as u64) as usize;
                out[dest_at..dest_at + count].copy_from_slice(&self.data.file[src_at..src_at + count]);
            }
            cursor = chunk_end;
        }
        true
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        let end = at as u64 + bytes.len() as u64;
        assert!(end <= (u32::MAX as u64) + 1, "snapshot memory write overflows address space");
        let mut pages = self.pages.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut cursor = at as u64;
        while cursor < end {
            let address = cursor as u32;
            let page_base = address & !((PAGE_SIZE as u32) - 1);
            let page_at = (address - page_base) as usize;
            let count = (end.min(page_base as u64 + PAGE_SIZE as u64) - cursor) as usize;
            let source_at = (cursor - at as u64) as usize;
            let page = pages.entry(address >> PAGE_SHIFT).or_insert_with(|| self.load_page(page_base));
            page[page_at..page_at + count].copy_from_slice(&bytes[source_at..source_at + count]);
            cursor += count as u64;
        }
    }
}

impl UnwindSafe for PagedMemory {}
impl RefUnwindSafe for PagedMemory {}
