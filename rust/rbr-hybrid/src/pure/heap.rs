//! The pure host's memory: the snapshot's bytes plus the CRT allocator stand-in's heap, in a range no snapshot maps.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rbr_physics::mem::{Backend, Mem, Pages};

/// Pure-run allocations live in a range no snapshot maps. The 3 step-0, 9 vcu-cov, snapH, and 2 impact snapshot
/// maps were checked before choosing this 8 MiB window. Its high end stays below the snapshots' 0x7ffe0000 shared
/// user-data region.
pub const ALLOCATOR_HEAP_BASE: u32 = 0x7f00_0000;
pub const ALLOCATOR_HEAP_END: u32 = 0x7f80_0000;

#[derive(Default)]
struct HeapBlock {
    bytes: Vec<u8>,
    initialized: Vec<bool>,
}

pub(super) struct PureHeap {
    next: u32,
    free: BTreeMap<u32, Vec<u32>>,
    pub(super) live: BTreeMap<u32, u32>,
    blocks: BTreeMap<u32, HeapBlock>,
    other: BTreeMap<u32, u8>,
    pub(super) unwritten_reads: BTreeSet<u32>,
    /// The pages that hold the blocks instead of `blocks` and `other` ([`Pure::untrack_allocator`]).
    pub(super) pages: Option<Pages>,
    /// The bindings running ([`PureRunGuard`]), shared with [`Pure`].
    run_depth: Arc<AtomicUsize>,
}

impl PureHeap {
    pub(super) fn new(run_depth: Arc<AtomicUsize>) -> Self {
        Self {
            next: ALLOCATOR_HEAP_BASE,
            free: BTreeMap::new(),
            live: BTreeMap::new(),
            blocks: BTreeMap::new(),
            other: BTreeMap::new(),
            unwritten_reads: BTreeSet::new(),
            pages: None,
            run_depth,
        }
    }

    pub(super) fn allocate(&mut self, size: u32) -> u32 {
        let span = ((u64::from(size).max(1) + 15) & !15) as usize;
        let ptr = match self.free.get_mut(&size).and_then(Vec::pop) {
            Some(ptr) => ptr,
            None => {
                let end = u64::from(self.next) + span as u64;
                assert!(end <= u64::from(ALLOCATOR_HEAP_END), "pure allocator stand-in exhausted its dedicated region");
                let ptr = self.next;
                self.next = end as u32;
                ptr
            }
        };
        self.live.insert(ptr, size);
        if let Some(pages) = &self.pages {
            pages.write(ptr, &vec![0; span]);
            return ptr;
        }
        let block_end = u64::from(ptr) + span as u64;
        self.other.retain(|&address, _| address < ptr || u64::from(address) >= block_end);
        let block = self.blocks.entry(ptr).or_default();
        if block.bytes.len() != span {
            block.bytes = vec![0; span];
            block.initialized = vec![false; span];
        } else {
            block.bytes.fill(0);
            block.initialized.fill(false);
        }
        ptr
    }

    pub(super) fn free(&mut self, ptr: u32) {
        if !(ALLOCATOR_HEAP_BASE..ALLOCATOR_HEAP_END).contains(&ptr) {
            return;
        }
        if let Some(size) = self.live.remove(&ptr) {
            self.free.entry(size).or_default().push(ptr);
        }
    }

    fn read(&mut self, at: u32, out: &mut [u8]) {
        let end = at as u64 + out.len() as u64;
        let mut cursor = at as u64;
        while cursor < end {
            let address = cursor as u32;
            let block_base = self
                .blocks
                .range(..=address)
                .next_back()
                .and_then(|(&base, block)| (((address - base) as usize) < block.bytes.len()).then_some(base));
            let next_block = self
                .blocks
                .range((std::ops::Bound::Excluded(address), std::ops::Bound::Unbounded))
                .next()
                .map_or(u64::from(ALLOCATOR_HEAP_END), |(&base, _)| u64::from(base));
            let chunk_end = if let Some(base) = block_base {
                (u64::from(base) + self.blocks[&base].bytes.len() as u64).min(end)
            } else {
                next_block.min(end)
            };
            let count = (chunk_end - cursor) as usize;
            let dest = (cursor - u64::from(at)) as usize;
            if let Some(base) = block_base {
                let offset = (cursor - u64::from(base)) as usize;
                let mut unread = Vec::new();
                {
                    let block = &self.blocks[&base];
                    out[dest..dest + count].copy_from_slice(&block.bytes[offset..offset + count]);
                    if self.run_depth.load(Ordering::Relaxed) != 0 {
                        unread.extend(
                            block.initialized[offset..offset + count]
                                .iter()
                                .enumerate()
                                .filter_map(|(i, &written)| (!written).then_some(address + i as u32)),
                        );
                    }
                }
                self.unwritten_reads.extend(unread);
            } else {
                for (i, byte) in out[dest..dest + count].iter_mut().enumerate() {
                    *byte = self.other.get(&(address + i as u32)).copied().unwrap_or(0);
                }
            }
            cursor = chunk_end;
        }
    }

    fn write(&mut self, at: u32, bytes: &[u8]) {
        let end = at as u64 + bytes.len() as u64;
        let mut cursor = at as u64;
        while cursor < end {
            let address = cursor as u32;
            let block_base = self
                .blocks
                .range(..=address)
                .next_back()
                .and_then(|(&base, block)| (((address - base) as usize) < block.bytes.len()).then_some(base));
            let next_block = self
                .blocks
                .range((std::ops::Bound::Excluded(address), std::ops::Bound::Unbounded))
                .next()
                .map_or(u64::from(ALLOCATOR_HEAP_END), |(&base, _)| u64::from(base));
            let chunk_end = if let Some(base) = block_base {
                (u64::from(base) + self.blocks[&base].bytes.len() as u64).min(end)
            } else {
                next_block.min(end)
            };
            let count = (chunk_end - cursor) as usize;
            let source = (cursor - u64::from(at)) as usize;
            if let Some(base) = block_base {
                let offset = (cursor - u64::from(base)) as usize;
                let block = self.blocks.get_mut(&base).expect("found block");
                block.bytes[offset..offset + count].copy_from_slice(&bytes[source..source + count]);
                block.initialized[offset..offset + count].fill(true);
            } else {
                for (i, &byte) in bytes[source..source + count].iter().enumerate() {
                    self.other.insert(address + i as u32, byte);
                }
            }
            cursor = chunk_end;
        }
    }
}

pub(super) struct PureMemory {
    pub(super) base: Base,
    pub(super) heap: Arc<Mutex<PureHeap>>,
}

/// The memory outside the allocator's region: a backend is used as it is (it takes `&self` writes, no lock).
pub(super) enum Base {
    Backend(Arc<dyn Backend>),
    Bytes(Mutex<Mem>),
}

impl Base {
    pub(super) fn new(mem: Mem) -> Self {
        match mem.backend() {
            Some(backend) => Base::Backend(backend),
            None => Base::Bytes(Mutex::new(mem)),
        }
    }

    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        match self {
            Base::Backend(backend) => backend.read(at, out),
            Base::Bytes(mem) => mem.lock().unwrap_or_else(|p| p.into_inner()).try_read_into(at, out),
        }
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        match self {
            Base::Backend(backend) => backend.write(at, bytes),
            Base::Bytes(mem) => mem.lock().unwrap_or_else(|p| p.into_inner()).map(at, bytes),
        }
    }
}

impl PureMemory {
    fn base_backend(&self, at: u32) -> Option<&Arc<dyn Backend>> {
        match &self.base {
            Base::Backend(backend) if outside_heap(at, 4) => Some(backend),
            _ => None,
        }
    }
}

/// Whether `at..at + len` lies entirely outside the allocator's region (and inside the address space).
fn outside_heap(at: u32, len: usize) -> bool {
    let end = at as u64 + len as u64;
    end <= u64::from(ALLOCATOR_HEAP_BASE) || (at >= ALLOCATOR_HEAP_END && end <= u64::from(u32::MAX) + 1)
}

impl Backend for PureMemory {
    fn read_u32(&self, at: u32) -> Option<u32> {
        match self.base_backend(at) {
            Some(backend) => backend.read_u32(at),
            None => {
                let mut out = [0; 4];
                self.read(at, &mut out).then(|| u32::from_le_bytes(out))
            }
        }
    }

    fn write_u32(&self, at: u32, value: u32) {
        match self.base_backend(at) {
            Some(backend) => backend.write_u32(at, value),
            None => self.write(at, &value.to_le_bytes()),
        }
    }

    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        if outside_heap(at, out.len()) {
            return self.base.read(at, out);
        }
        let end = at as u64 + out.len() as u64;
        if end > u64::from(u32::MAX) + 1 {
            return false;
        }
        let mut cursor = u64::from(at);
        while cursor < end {
            let in_heap = (u64::from(ALLOCATOR_HEAP_BASE)..u64::from(ALLOCATOR_HEAP_END)).contains(&cursor);
            let boundary = if in_heap {
                u64::from(ALLOCATOR_HEAP_END)
            } else if cursor < u64::from(ALLOCATOR_HEAP_BASE) {
                u64::from(ALLOCATOR_HEAP_BASE)
            } else {
                end
            };
            let chunk_end = boundary.min(end);
            let count = (chunk_end - cursor) as usize;
            let offset = (cursor - u64::from(at)) as usize;
            if in_heap {
                self.heap
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .read(cursor as u32, &mut out[offset..offset + count]);
            } else {
                if !self.base.read(cursor as u32, &mut out[offset..offset + count]) {
                    return false;
                }
            }
            cursor = chunk_end;
        }
        true
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        if outside_heap(at, bytes.len()) {
            return self.base.write(at, bytes);
        }
        let end = at as u64 + bytes.len() as u64;
        assert!(end <= u64::from(u32::MAX) + 1, "pure memory write overflows address space");
        let mut cursor = u64::from(at);
        while cursor < end {
            let in_heap = (u64::from(ALLOCATOR_HEAP_BASE)..u64::from(ALLOCATOR_HEAP_END)).contains(&cursor);
            let boundary = if in_heap {
                u64::from(ALLOCATOR_HEAP_END)
            } else if cursor < u64::from(ALLOCATOR_HEAP_BASE) {
                u64::from(ALLOCATOR_HEAP_BASE)
            } else {
                end
            };
            let chunk_end = boundary.min(end);
            let count = (chunk_end - cursor) as usize;
            let offset = (cursor - u64::from(at)) as usize;
            if in_heap {
                self.heap
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .write(cursor as u32, &bytes[offset..offset + count]);
            } else {
                self.base.write(cursor as u32, &bytes[offset..offset + count]);
            }
            cursor = chunk_end;
        }
    }
}

pub(super) struct PureRunGuard<'a>(pub(super) &'a AtomicUsize);

/// The count changes on the one thread that runs the [`Pure`] (it is not `Sync`), so it is a load and a store, not a
/// locked update.
impl<'a> PureRunGuard<'a> {
    pub(super) fn new(run_depth: &'a AtomicUsize) -> Self {
        run_depth.store(run_depth.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
        Self(run_depth)
    }
}

impl Drop for PureRunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(self.0.load(Ordering::Relaxed) - 1, Ordering::Relaxed);
    }
}
