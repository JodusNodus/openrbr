//! The game memory a port reads and writes: the bytes a trace recorded (its regions),
//! addressed as the game does. Reads outside the mapped bytes panic, so a port that
//! reads more than the trace captured fails the test instead of reading garbage.
//!
//! The bytes live in a map by default (`Mem::new`). `Mem::with_backend` puts them somewhere else, such as the
//! emulator's memory in the hybrid step (`rbr-hybrid`), with the same API. `Mem::with_pages` keeps them in [`Pages`],
//! which a word access reads and writes directly, without a call through the backend.

use crate::math::vec::{Vec3, Vec4};
use crate::store::Raw;
use std::collections::BTreeMap;
use std::fmt;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::Arc;

mod pages;
pub use pages::Pages;

/// Storage for a [`Mem`] outside the byte map.
pub trait Backend: Send + Sync + UnwindSafe + RefUnwindSafe {
    /// Fills `out` with the bytes at `at..`; false when any of them is not mapped.
    fn read(&self, at: u32, out: &mut [u8]) -> bool;
    /// Writes `bytes` at `at`.
    fn write(&self, at: u32, bytes: &[u8]);
    /// The little-endian word at `at`, or `None` when any of its bytes is not mapped. A backend may override it (and
    /// `write_u32`) with a faster path for the step's word accesses.
    fn read_u32(&self, at: u32) -> Option<u32> {
        let mut out = [0; 4];
        self.read(at, &mut out).then(|| u32::from_le_bytes(out))
    }
    /// Writes the little-endian word `value` at `at`.
    fn write_u32(&self, at: u32, value: u32) {
        self.write(at, &value.to_le_bytes());
    }
}

#[derive(Clone, Default)]
pub struct Mem {
    storage: Storage,
    /// The byte ranges another storage owns (`Mem::poison`, check builds with the `poison` feature only).
    #[cfg(feature = "poison")]
    poison: Option<Arc<Poison>>,
}

/// Owned byte ranges, sorted by start, with their owners' names: an access to one of them panics.
#[cfg(feature = "poison")]
#[derive(Debug)]
struct Poison(Vec<(u32, u32, String)>);

#[cfg(feature = "poison")]
impl Poison {
    #[cold]
    #[inline(never)]
    fn hit(&self, at: u32, len: usize) -> ! {
        let end = u64::from(at) + len as u64;
        let (lo, _, name) =
            self.0.iter().find(|(lo, hi, _)| u64::from(*lo) < end && at < *hi).expect("a poisoned range");
        panic!("poisoned access {at:#010x}+{len:#x}: {name} (+{:#x}) belongs to the typed storage", at.max(*lo) - lo)
    }

    #[inline]
    fn check(&self, at: u32, len: usize) {
        let end = u64::from(at) + len as u64;
        // the first range ending after `at`
        let k = self.0.partition_point(|(_, hi, _)| *hi <= at);
        if self.0.get(k).is_some_and(|(lo, _, _)| u64::from(*lo) < end) {
            self.hit(at, len)
        }
    }
}

/// A clone of a byte map is a copy; a clone of a backend shares its memory.
#[derive(Clone)]
enum Storage {
    Bytes(BTreeMap<u32, u8>),
    Backend(Arc<dyn Backend>),
    /// The accesses outside `hole` go to `pages`, the others to `backend` (to `pages` without one), whose bytes outside
    /// `hole` are the pages'.
    Paged {
        pages: Pages,
        hole: (u32, u32),
        backend: Option<Arc<dyn Backend>>,
    },
}

/// Whether `at..at + len` lies outside `lo..hi` and inside the address space.
#[inline]
fn outside(at: u32, len: usize, (lo, hi): (u32, u32)) -> bool {
    let end = u64::from(at) + len as u64;
    end <= u64::from(lo) || (at >= hi && end <= 1 << 32)
}

impl Default for Storage {
    fn default() -> Self {
        Storage::Bytes(BTreeMap::new())
    }
}

impl fmt::Debug for Mem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.storage {
            Storage::Bytes(bytes) => f.debug_struct("Mem").field("bytes", bytes).finish(),
            Storage::Backend(_) | Storage::Paged { .. } => f.debug_struct("Mem").field("backend", &"..").finish(),
        }
    }
}

impl Mem {
    pub fn new() -> Self {
        Self::default()
    }

    fn of(storage: Storage) -> Self {
        Self {
            storage,
            #[cfg(feature = "poison")]
            poison: None,
        }
    }

    /// Memory kept by `backend` instead of the byte map.
    pub fn with_backend(backend: Arc<dyn Backend>) -> Self {
        Self::of(Storage::Backend(backend))
    }

    /// Memory kept in `pages`.
    pub fn with_pages(pages: Pages) -> Self {
        Self::of(Storage::Paged { pages, hole: (0, 0), backend: None })
    }

    /// Memory kept by `backend`, whose bytes outside `hole` are those of `pages`: the accesses outside `hole` go to
    /// `pages` directly.
    pub fn with_pages_around(pages: Pages, hole: std::ops::Range<u32>, backend: Arc<dyn Backend>) -> Self {
        Self::of(Storage::Paged { pages, hole: (hole.start, hole.end), backend: Some(backend) })
    }

    /// The backend that holds this memory, if it is not the byte map.
    pub fn backend(&self) -> Option<Arc<dyn Backend>> {
        match &self.storage {
            Storage::Backend(backend) | Storage::Paged { backend: Some(backend), .. } => Some(backend.clone()),
            Storage::Paged { pages, backend: None, .. } => Some(Arc::new(pages.clone())),
            Storage::Bytes(_) => None,
        }
    }

    /// Whether `other` is a clone of this memory that shares its bytes.
    pub fn shares(&self, other: &Mem) -> bool {
        let same = |a: &Arc<dyn Backend>, b: &Arc<dyn Backend>| std::ptr::addr_eq(Arc::as_ptr(a), Arc::as_ptr(b));
        match (&self.storage, &other.storage) {
            (Storage::Backend(a), Storage::Backend(b)) => same(a, b),
            (
                Storage::Paged { pages: a, hole: ha, backend: ba },
                Storage::Paged { pages: b, hole: hb, backend: bb },
            ) => {
                a.same(b)
                    && ha == hb
                    && match (ba, bb) {
                        (Some(a), Some(b)) => same(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    /// The pages this memory is kept in, outside the hole of [`Mem::with_pages_around`].
    pub fn pages(&self) -> Option<&Pages> {
        match &self.storage {
            Storage::Paged { pages, .. } => Some(pages),
            _ => None,
        }
    }

    /// Poisons `ranges` (check builds with the `poison` feature): every later access to one of their bytes through
    /// this memory, or a clone made after this call, panics with the owner's name. An empty list clears the poison.
    #[cfg(feature = "poison")]
    pub fn poison(&mut self, ranges: impl IntoIterator<Item = (std::ops::Range<u32>, String)>) {
        let mut list: Vec<_> =
            ranges.into_iter().filter(|(r, _)| r.start < r.end).map(|(r, n)| (r.start, r.end, n)).collect();
        list.sort_by_key(|&(lo, _, _)| lo);
        assert!(list.windows(2).all(|w| w[0].1 <= w[1].0), "poisoned ranges overlap");
        self.poison = (!list.is_empty()).then(|| Arc::new(Poison(list)));
    }

    /// This memory without its poison, sharing the storage: what `lift` and `lower` of the owner read and write.
    #[cfg(feature = "poison")]
    pub fn unpoisoned(&self) -> Mem {
        Self::of(self.storage.clone())
    }

    /// Panics when `at..at + len` touches a poisoned byte; nothing without the `poison` feature.
    #[inline(always)]
    fn guard(&self, _at: u32, _len: usize) {
        #[cfg(feature = "poison")]
        if let Some(poison) = &self.poison {
            poison.check(_at, _len)
        }
    }

    /// The backend that holds `at..at + len`, or `None` for the byte map.
    #[inline]
    fn holder(&self, at: u32, len: usize) -> Option<&dyn Backend> {
        self.guard(at, len);
        match &self.storage {
            Storage::Bytes(_) => None,
            Storage::Backend(backend) => Some(&**backend),
            Storage::Paged { backend: Some(backend), hole, .. } if !outside(at, len, *hole) => Some(&**backend),
            Storage::Paged { pages, .. } => Some(pages),
        }
    }

    /// The pages that hold `at..at + len`, if pages do: the accesses there read and write them directly.
    #[inline]
    pub fn pages_for(&self, at: u32, len: usize) -> Option<&Pages> {
        self.guard(at, len);
        match &self.storage {
            Storage::Paged { pages, hole, .. } if outside(at, len, *hole) => Some(pages),
            _ => None,
        }
    }

    /// The word at `at` when the pages hold it and it is mapped; `None` otherwise (never a panic).
    #[inline]
    pub fn paged_u32(&self, at: u32) -> Option<u32> {
        self.pages_for(at, 4)?.word(at)
    }

    /// Maps `bytes` at address `at`. Overlapping mappings must hold the same bytes.
    pub fn map(&mut self, at: u32, bytes: &[u8]) {
        match self.holder(at, bytes.len()) {
            Some(backend) => backend.write(at, bytes),
            None => {
                if let Storage::Bytes(map) = &mut self.storage {
                    for (i, &byte) in bytes.iter().enumerate() {
                        map.insert(at + i as u32, byte);
                    }
                }
            }
        }
    }

    /// Copies `len` bytes from `from` to `to` as a read of them all and then a write gives it (`memmove`).
    pub fn copy(&mut self, to: u32, from: u32, len: usize) {
        let words = u32::try_from(len / 4).ok().filter(|_| len.is_multiple_of(4));
        if let (Some(n), Some(pages)) = (words, self.pages_for(from, len).and(self.pages_for(to, len))) {
            if pages.move_words(to, from, n) {
                return;
            }
        }
        let bytes = self.read(from, len);
        self.map(to, &bytes);
    }

    /// Copies `n` words from `from` to `to` one at a time in ascending order, as `rep movsd` does.
    pub fn copy_words(&mut self, to: u32, from: u32, n: u32) {
        let len = 4 * n as usize;
        if let Some(pages) = self.pages_for(from, len).and(self.pages_for(to, len)) {
            if pages.copy_words(to, from, n) {
                return;
            }
        }
        for k in 0..n {
            let word = self.u32(from + 4 * k);
            self.set_u32(to + 4 * k, word);
        }
    }

    pub fn u8(&self, at: u32) -> u8 {
        if let Some(word) = self.pages_for(at, 1).and_then(|pages| pages.word(at & !3)) {
            return (word >> (8 * (at % 4))) as u8;
        }
        let byte = match &self.storage {
            Storage::Bytes(map) => map.get(&at).copied(),
            _ => {
                let mut out = [0];
                self.holder(at, 1).expect("a backend").read(at, &mut out).then_some(out[0])
            }
        };
        match byte {
            Some(byte) => byte,
            None => panic!("read of unmapped address {at:#010x}"),
        }
    }

    /// The little-endian halfword at `at`.
    #[inline]
    pub fn u16(&self, at: u32) -> u16 {
        if at % 4 != 3 {
            if let Some(word) = self.pages_for(at, 2).and_then(|pages| pages.word(at & !3)) {
                return (word >> (8 * (at % 4))) as u16;
            }
        }
        u16::from_le_bytes([self.u8(at), self.u8(at + 1)])
    }

    #[inline]
    pub fn u32(&self, at: u32) -> u32 {
        if let Some(pages) = self.pages_for(at, 4) {
            if let Some(word) = pages.word(at) {
                return word;
            }
        } else if let Some(backend) = self.holder(at, 4) {
            if let Some(word) = backend.read_u32(at) {
                return word;
            }
        }
        self.u32_bytes(at)
    }

    /// [`Mem::u32`] byte by byte: the byte map's read, and the panic that names a backend's first unmapped byte.
    #[cold]
    #[inline(never)]
    fn u32_bytes(&self, at: u32) -> u32 {
        u32::from_le_bytes([self.u8(at), self.u8(at + 1), self.u8(at + 2), self.u8(at + 3)])
    }

    /// `N` consecutive words from `at`, as `N` [`Mem::u32`] reads in order give them (and fail).
    pub fn u32s<const N: usize>(&self, at: u32) -> [u32; N] {
        if let Some(words) = self.paged_u32s(at) {
            return words;
        }
        let mut bytes = [[0; 4]; N];
        if self.try_read_into(at, bytes.as_flattened_mut()) {
            return bytes.map(u32::from_le_bytes);
        }
        std::array::from_fn(|k| self.u32(at + 4 * k as u32))
    }

    /// `N` consecutive words from `at` when the pages hold them all, read at once.
    #[inline]
    fn paged_u32s<const N: usize>(&self, at: u32) -> Option<[u32; N]> {
        self.pages_for(at, 4 * N)?.words(at)
    }

    /// Writes `N` consecutive words from `at` at once when the pages hold them all; false (nothing written) otherwise.
    #[inline]
    fn set_paged_u32s<const N: usize>(&self, at: u32, words: [u32; N]) -> bool {
        self.pages_for(at, 4 * N).is_some_and(|pages| pages.set_words(at, words))
    }

    #[inline]
    pub fn f32(&self, at: u32) -> f32 {
        f32::from_bits(self.u32(at))
    }

    /// `N` consecutive floats from `at`, as `N` [`Mem::f32`] reads in order.
    pub fn f32s<const N: usize>(&self, at: u32) -> [f32; N] {
        self.u32s(at).map(f32::from_bits)
    }

    pub fn set_u8(&mut self, at: u32, value: u8) {
        if let Some(pages) = self.pages_for(at, 1) {
            return pages.set_u8(at, value);
        }
        self.map(at, &[value]);
    }

    #[inline]
    pub fn set_u32(&mut self, at: u32, value: u32) {
        if let Some(pages) = self.pages_for(at, 4) {
            return pages.set_word(at, value);
        }
        match self.holder(at, 4) {
            Some(backend) => backend.write_u32(at, value),
            None => self.map(at, &value.to_le_bytes()),
        }
    }

    #[inline]
    pub fn set_f32(&mut self, at: u32, value: f32) {
        self.set_u32(at, value.to_bits());
    }

    /// Fills `out` with the bytes at `at..`; false when any of them is not mapped.
    pub fn try_read_into(&self, at: u32, out: &mut [u8]) -> bool {
        self.guard(at, out.len());
        match &self.storage {
            Storage::Backend(_) | Storage::Paged { .. } => self.holder(at, out.len()).expect("a backend").read(at, out),
            Storage::Bytes(map) => out.iter_mut().zip(at..).all(|(byte, a)| match map.get(&a) {
                Some(&b) => {
                    *byte = b;
                    true
                }
                None => false,
            }),
        }
    }

    /// The bytes at `at..at + len`, or `None` when any of them is not mapped.
    pub fn try_read(&self, at: u32, len: usize) -> Option<Vec<u8>> {
        self.guard(at, len);
        match &self.storage {
            Storage::Backend(_) | Storage::Paged { .. } => {
                let mut out = vec![0; len];
                self.holder(at, len).expect("a backend").read(at, &mut out).then_some(out)
            }
            Storage::Bytes(map) => (0..len as u32).map(|i| map.get(&(at + i)).copied()).collect(),
        }
    }

    /// The bytes at `at..at + len`, for comparing against a trace.
    pub fn read(&self, at: u32, len: usize) -> Vec<u8> {
        if let Some(backend) = self.holder(at, len) {
            let mut out = vec![0; len];
            if backend.read(at, &mut out) {
                return out;
            }
        }
        (0..len as u32).map(|i| self.u8(at + i)).collect()
    }
}

/// A vector as the game stores it: consecutive f32 lanes from `at`.
impl Vec3 {
    #[inline]
    pub fn load<R: Raw + ?Sized>(mem: &R, at: u32) -> Self {
        let mem = mem.mem();
        if let Some([x, y, z]) = mem.paged_u32s(at) {
            return Self { x: f32::from_bits(x), y: f32::from_bits(y), z: f32::from_bits(z) };
        }
        Self { x: mem.f32(at), y: mem.f32(at + 4), z: mem.f32(at + 8) }
    }

    /// Stores x, y and z; the word after them is not touched.
    #[inline]
    pub fn store<R: Raw + ?Sized>(self, mem: &mut R, at: u32) {
        let mem = mem.mem_mut();
        if mem.set_paged_u32s(at, [self.x, self.y, self.z].map(f32::to_bits)) {
            return;
        }
        mem.set_f32(at, self.x);
        mem.set_f32(at + 4, self.y);
        mem.set_f32(at + 8, self.z);
    }
}

impl Vec4 {
    #[inline]
    pub fn load<R: Raw + ?Sized>(mem: &R, at: u32) -> Self {
        let mem = mem.mem();
        if let Some([x, y, z, w]) = mem.paged_u32s(at) {
            return Self { x: f32::from_bits(x), y: f32::from_bits(y), z: f32::from_bits(z), w: f32::from_bits(w) };
        }
        Self { x: mem.f32(at), y: mem.f32(at + 4), z: mem.f32(at + 8), w: mem.f32(at + 12) }
    }

    #[inline]
    pub fn store<R: Raw + ?Sized>(self, mem: &mut R, at: u32) {
        let mem = mem.mem_mut();
        if mem.set_paged_u32s(at, [self.x, self.y, self.z, self.w].map(f32::to_bits)) {
            return;
        }
        mem.set_f32(at, self.x);
        mem.set_f32(at + 4, self.y);
        mem.set_f32(at + 8, self.z);
        mem.set_f32(at + 12, self.w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// 0x100 bytes at 0x1000, starting as their low address byte.
    struct Flat(Mutex<Vec<u8>>);

    impl Backend for Flat {
        fn read(&self, at: u32, out: &mut [u8]) -> bool {
            let bytes = self.0.lock().unwrap();
            let Some(start) = at.checked_sub(0x1000).map(|s| s as usize) else {
                return false;
            };
            match bytes.get(start..start + out.len()) {
                Some(src) => {
                    out.copy_from_slice(src);
                    true
                }
                None => false,
            }
        }

        fn write(&self, at: u32, src: &[u8]) {
            let start = (at - 0x1000) as usize;
            self.0.lock().unwrap()[start..start + src.len()].copy_from_slice(src);
        }
    }

    fn flat() -> Mem {
        Mem::with_backend(Arc::new(Flat(Mutex::new((0..=255).collect()))))
    }

    #[test]
    fn mem_keeps_its_auto_traits() {
        fn check<T: Send + Sync + UnwindSafe + RefUnwindSafe>() {}
        check::<Mem>();
    }

    #[test]
    fn backend_reads_and_writes_through() {
        let mut mem = flat();
        assert_eq!(mem.u8(0x1001), 1);
        assert_eq!(mem.u32(0x1004), 0x0706_0504);
        mem.set_f32(0x1010, 1.5);
        assert_eq!(mem.f32(0x1010), 1.5);
        mem.map(0x10fe, &[9, 8]);
        assert_eq!(mem.read(0x10fd, 3), [0xfd, 9, 8]);
        // A clone shares the backend's memory.
        let copy = mem.clone();
        mem.set_u32(0x1020, 7);
        assert_eq!(copy.u32(0x1020), 7);
    }

    #[test]
    fn words_read_as_single_words_do() {
        let mem = flat();
        assert_eq!(mem.u32s::<2>(0x1001), [mem.u32(0x1001), mem.u32(0x1005)]);
        let mut bytes = Mem::new();
        bytes.map(0x10, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(bytes.u32s::<2>(0x10), [0x0403_0201, 0x0807_0605]);
    }

    /// `copy` is a memmove and `copy_words` a forward `rep movsd`, on pages as on the byte map; `u16` reads across a
    /// word.
    #[test]
    fn copies_and_halfwords_match_the_byte_accesses() {
        let words: Vec<u8> = (0..32).collect();
        for paged in [false, true] {
            let fresh = || {
                let mut mem = if paged { Mem::with_pages(Pages::default()) } else { Mem::new() };
                mem.map(0x1000, &words);
                mem
            };
            let mut mem = fresh();
            mem.copy(0x1004, 0x1000, 16);
            assert_eq!(mem.read(0x1000, 20), [&words[..4], &words[..16]].concat(), "memmove up, paged {paged}");
            let mut mem = fresh();
            mem.copy(0x1000, 0x1004, 16);
            assert_eq!(mem.read(0x1000, 16), words[4..20], "memmove down, paged {paged}");
            let mut mem = fresh();
            mem.copy_words(0x1004, 0x1000, 4);
            assert_eq!(mem.read(0x1000, 20), [&words[..4]; 5].concat(), "rep movsd up, paged {paged}");
            let mut mem = fresh();
            mem.copy(0x1001, 0x1010, 3);
            assert_eq!(mem.read(0x1000, 4), [0, 16, 17, 18], "unaligned copy, paged {paged}");
            assert_eq!(mem.u16(0x1002), u16::from_le_bytes([17, 18]));
            assert_eq!(mem.u16(0x1003), u16::from_le_bytes([18, 4]));
        }
    }

    #[test]
    #[should_panic(expected = "read of unmapped address 0x00001100")]
    fn words_past_the_end_name_the_first_unmapped_byte() {
        flat().u32s::<3>(0x10f8);
    }

    #[test]
    #[should_panic(expected = "read of unmapped address 0x00001100")]
    fn backend_read_past_the_end_names_the_first_unmapped_byte() {
        flat().u32(0x10fe);
    }

    #[cfg(feature = "poison")]
    #[test]
    fn poisoned_bytes_panic_and_others_do_not() {
        let mut mem = flat();
        mem.poison([(0x1010..0x1018, "Sample".to_string()), (0x1040..0x1044, "Other".to_string())]);
        assert_eq!(mem.u32(0x100c), 0x0f0e_0d0c);
        assert_eq!(mem.u32(0x1018), 0x1b1a_1918);
        assert_eq!(mem.unpoisoned().u32(0x1010), 0x1312_1110);
        let hit = |f: fn(&mut Mem)| {
            let mut m = mem.clone();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || f(&mut m))).unwrap_err()
        };
        let message = |e: Box<dyn std::any::Any + Send>| e.downcast::<String>().map(|s| *s).unwrap_or_default();
        assert!(message(hit(|m| {
            m.u8(0x1017);
        }))
        .contains("Sample (+0x7)"));
        assert!(message(hit(|m| m.set_u32(0x1042, 0))).contains("Other (+0x2)"));
        assert!(message(hit(|m| {
            m.read(0x1000, 0x11);
        }))
        .contains(": Sample (+0x0)"));
        mem.poison([]);
        assert_eq!(mem.u32(0x1010), 0x1312_1110);
    }

    #[test]
    #[should_panic(expected = "read of unmapped address 0x00000ffc")]
    fn byte_map_read_outside_panics() {
        let mut mem = Mem::new();
        mem.map(0x1000, &[1, 2, 3, 4]);
        assert_eq!(mem.u32(0x1000), 0x0403_0201);
        mem.u32(0x0ffc);
    }
}
