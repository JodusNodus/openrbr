//! [`Pages`]: memory in 4 KiB pages that a [`super::Mem`] reads and writes directly.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::Backend;

const PAGE: u32 = 0x1000;

/// Memory in 4 KiB pages: a page exists once something writes into it (the rest of it zero); a read of a page nothing
/// wrote fails, as an unmapped address does. The whole 32-bit address space is one anonymous mapping that the system
/// fills with zero pages as they are touched, with a word per page that says whether something wrote it, so an access
/// is a load or a store at the address with no table walk, and an aligned word access one. The words are
/// little-endian. The step runs on one thread: a byte write is a load and a store of its word, not an atomic update. A
/// clone shares the pages. The 4 GiB mapping is only reserved: on Unix without swap, on Windows with each page committed
/// on its first write ([`Map`]).
#[derive(Clone)]
pub struct Pages {
    flat: Arc<Flat>,
    /// The mapping's first word, kept here so that an access does not first load it from `flat`.
    words: *const AtomicU32,
}

const PAGES: usize = 1 << 20;
const WORDS: usize = 1 << 30;

/// The address space's words, then a mark per page: 0 while nothing wrote the page, else the epoch of its last write
/// (64 bits: they do not wrap).
struct Flat {
    _map: Map,
    epoch: AtomicU64,
}

/// The reserved mapping of `len` bytes that the system fills with zero pages as they are touched.
#[cfg(not(windows))]
struct Map(memmap2::MmapMut);

#[cfg(not(windows))]
impl Map {
    fn reserve(len: usize) -> Map {
        Map(memmap2::MmapOptions::new().len(len).no_reserve_swap().map_anon().expect("an address space for the pages"))
    }

    fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}

/// Readies the `len` bytes at `at`, inside the mapping, for writes: a Unix mapping does so as they are touched.
#[cfg(not(windows))]
#[inline]
fn commit(_at: *const u8, _len: usize) {}

/// Windows charges a committed mapping against the commit limit up front, 4 GiB a mapping (a few at once failed with
/// error 1450), so the address space is reserved and its pages committed as they are first written ([`commit`]).
#[cfg(windows)]
struct Map(*const u8);

#[cfg(windows)]
impl Map {
    fn reserve(len: usize) -> Map {
        use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_RESERVE, PAGE_READWRITE};
        // SAFETY: a new reservation, not an existing address.
        let at = unsafe { VirtualAlloc(std::ptr::null(), len, MEM_RESERVE, PAGE_READWRITE) };
        assert!(!at.is_null(), "an address space for the pages: {}", std::io::Error::last_os_error());
        Map(at.cast_const().cast())
    }

    fn as_ptr(&self) -> *const u8 {
        self.0
    }
}

#[cfg(windows)]
impl Drop for Map {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Memory::{VirtualFree, MEM_RELEASE};
        // SAFETY: the reservation's base, released once.
        unsafe { VirtualFree(self.0.cast_mut().cast(), 0, MEM_RELEASE) };
    }
}

/// Commits the `len` bytes at `at`, inside the reservation, as zeroes (again: a no-op).
#[cfg(windows)]
fn commit(at: *const u8, len: usize) {
    use windows_sys::Win32::System::Memory::{VirtualAlloc, MEM_COMMIT, PAGE_READWRITE};
    // SAFETY: inside the reservation.
    let done = unsafe { VirtualAlloc(at.cast(), len, MEM_COMMIT, PAGE_READWRITE) };
    assert!(!done.is_null(), "memory for a page: {}", std::io::Error::last_os_error());
}

// The mapping is only reached through its atomics.
unsafe impl Send for Pages {}
unsafe impl Sync for Pages {}
impl std::panic::RefUnwindSafe for Pages {}

impl Default for Pages {
    fn default() -> Self {
        let map = Map::reserve(4 * WORDS + 8 * PAGES);
        // The mapping holds `WORDS` zeroed, aligned words, then `PAGES` zeroed, aligned marks, which are all read.
        let words = map.as_ptr().cast::<AtomicU32>();
        // SAFETY: the marks are inside the mapping.
        commit(unsafe { map.as_ptr().add(4 * WORDS) }, 8 * PAGES);
        Pages { flat: Arc::new(Flat { _map: map, epoch: AtomicU64::new(1) }), words }
    }
}

impl Pages {
    /// The word holding byte `at`.
    #[inline]
    fn cell(&self, at: u32) -> &AtomicU32 {
        // SAFETY: `at / 4` is below `WORDS`.
        unsafe { &*self.words.add((at / 4) as usize) }
    }

    /// The written mark of the page holding `at`.
    #[inline]
    fn mark(&self, at: u32) -> &AtomicU64 {
        // SAFETY: the marks follow the words, and `at / PAGE` is below `PAGES`.
        unsafe { &*self.words.add(WORDS).cast::<AtomicU64>().add((at / PAGE) as usize) }
    }

    #[inline]
    fn mapped(&self, at: u32) -> bool {
        self.mark(at).load(Ordering::Relaxed) != 0
    }

    /// Whether every page of `at..at + len` (`len` above 0, inside the address space) was written.
    #[inline]
    fn mapped_range(&self, at: u32, len: u32) -> bool {
        let last = at + (len - 1);
        (at / PAGE..=last / PAGE).all(|p| self.mapped(p * PAGE))
    }

    /// Marks the page holding `at` written; its words are read only once it is (on Windows committed then).
    #[inline]
    fn touch(&self, at: u32) {
        if cfg!(windows) && !self.mapped(at) {
            // SAFETY: the page's words are inside the mapping.
            commit(unsafe { self.words.add((at / PAGE * PAGE / 4) as usize).cast() }, PAGE as usize);
        }
        self.mark(at).store(self.flat.epoch.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    #[inline]
    fn touch_range(&self, at: u32, len: u32) {
        let last = at + (len - 1);
        for p in at / PAGE..=last / PAGE {
            self.touch(p * PAGE);
        }
    }

    /// Ends the current write epoch and returns it: the writes until now are of it or earlier, the later ones of a
    /// later epoch ([`Pages::written_after`]). For a cache of values read from the pages.
    pub fn end_epoch(&self) -> u64 {
        self.flat.epoch.fetch_add(1, Ordering::Relaxed)
    }

    /// Whether something wrote the page holding `at` after `epoch` ended ([`Pages::end_epoch`]).
    #[inline]
    pub fn written_after(&self, at: u32, epoch: u64) -> bool {
        self.mark(at).load(Ordering::Relaxed) > epoch
    }

    /// The little-endian word at `at`, or `None` when any of its bytes is not mapped.
    #[inline]
    pub fn word(&self, at: u32) -> Option<u32> {
        if at.is_multiple_of(4) {
            return self.mapped(at).then(|| self.cell(at).load(Ordering::Relaxed));
        }
        let mut out = [0; 4];
        self.read(at, &mut out).then(|| u32::from_le_bytes(out))
    }

    /// The `N` little-endian words from `at`, or `None` when any of their bytes is not mapped.
    #[inline]
    pub fn words<const N: usize>(&self, at: u32) -> Option<[u32; N]> {
        let o = (at % PAGE) as usize;
        if at.is_multiple_of(4) && o + 4 * N <= PAGE as usize {
            if !self.mapped(at) {
                return None;
            }
            return Some(std::array::from_fn(|k| self.cell(at + 4 * k as u32).load(Ordering::Relaxed)));
        }
        let mut bytes = [[0; 4]; N];
        self.read(at, bytes.as_flattened_mut()).then(|| bytes.map(u32::from_le_bytes))
    }

    /// Writes the `N` little-endian words from `at` when they are aligned in one page; false (nothing written)
    /// otherwise.
    #[inline]
    pub fn set_words<const N: usize>(&self, at: u32, words: [u32; N]) -> bool {
        let o = (at % PAGE) as usize;
        if !at.is_multiple_of(4) || o + 4 * N > PAGE as usize {
            return false;
        }
        self.touch(at);
        for (k, word) in words.into_iter().enumerate() {
            self.cell(at + 4 * k as u32).store(word, Ordering::Relaxed);
        }
        true
    }

    /// Writes the byte `value` at `at`.
    #[inline]
    pub fn set_u8(&self, at: u32, value: u8) {
        self.touch(at);
        self.set_byte(at, value);
    }

    /// Writes the little-endian words `words` from the aligned `at` (inside the address space).
    #[inline]
    pub fn set_run(&self, at: u32, words: &[u32]) {
        debug_assert!(at.is_multiple_of(4) && !words.is_empty());
        self.touch_range(at, 4 * words.len() as u32);
        for (k, &word) in words.iter().enumerate() {
            self.cell(at + 4 * k as u32).store(word, Ordering::Relaxed);
        }
    }

    /// Writes the little-endian word `value` at `at`.
    #[inline]
    pub fn set_word(&self, at: u32, value: u32) {
        if at.is_multiple_of(4) {
            self.touch(at);
            self.cell(at).store(value, Ordering::Relaxed);
        } else {
            self.write(at, &value.to_le_bytes());
        }
    }

    /// Copies `n` words from `from` to `to` one at a time in ascending order, as `rep movsd` does. False, with nothing
    /// copied, when an address is not aligned, a range wraps the address space or a source page is unmapped.
    #[inline]
    pub fn copy_words(&self, to: u32, from: u32, n: u32) -> bool {
        self.copy_run(to, from, n, false)
    }

    /// Copies `n` words from `from` to `to` as a read of them all and then a write gives it (`memmove`); false as
    /// [`Pages::copy_words`].
    #[inline]
    pub fn move_words(&self, to: u32, from: u32, n: u32) -> bool {
        // Only a destination inside the source after its start would read words the copy already wrote.
        self.copy_run(to, from, n, to > from && to - from < n.saturating_mul(4))
    }

    fn copy_run(&self, to: u32, from: u32, n: u32, descending: bool) -> bool {
        let Some(len) = n.checked_mul(4) else {
            return false;
        };
        if len == 0 {
            return true;
        }
        let fits = |at: u32| at.is_multiple_of(4) && at.checked_add(len - 1).is_some();
        if !fits(to) || !fits(from) || !self.mapped_range(from, len) {
            return false;
        }
        self.touch_range(to, len);
        let copy =
            |k: u32| self.cell(to + 4 * k).store(self.cell(from + 4 * k).load(Ordering::Relaxed), Ordering::Relaxed);
        if descending {
            (0..n).rev().for_each(copy);
        } else {
            (0..n).for_each(copy);
        }
        true
    }

    /// Reads `out.len()` bytes from `at` (inside the address space, the pages mapped) word by word.
    fn read_in(&self, at: u32, out: &mut [u8]) {
        let o = at as usize;
        // The bytes up to the first whole word, the whole words, then the bytes after them.
        let head = ((4 - o % 4) % 4).min(out.len());
        let (head_out, rest) = out.split_at_mut(head);
        for (k, byte) in head_out.iter_mut().enumerate() {
            *byte = self.byte(at + k as u32);
        }
        let first = at.wrapping_add(head as u32);
        let (words, tail) = rest.as_chunks_mut::<4>();
        for (k, bytes) in words.iter_mut().enumerate() {
            *bytes = self.cell(first + 4 * k as u32).load(Ordering::Relaxed).to_le_bytes();
        }
        let tail_at = first.wrapping_add(4 * words.len() as u32);
        for (k, byte) in tail.iter_mut().enumerate() {
            *byte = self.byte(tail_at + k as u32);
        }
    }

    fn byte(&self, at: u32) -> u8 {
        (self.cell(at).load(Ordering::Relaxed) >> (8 * (at % 4))) as u8
    }

    fn set_byte(&self, at: u32, byte: u8) {
        let cell = self.cell(at);
        let shift = 8 * (at % 4);
        let word = cell.load(Ordering::Relaxed);
        cell.store(word & !(0xff << shift) | u32::from(byte) << shift, Ordering::Relaxed);
    }

    /// Writes `bytes` at `at` (inside the address space) word by word.
    fn write_in(&self, at: u32, bytes: &[u8]) {
        let o = at as usize;
        let head = ((4 - o % 4) % 4).min(bytes.len());
        for (k, &byte) in bytes[..head].iter().enumerate() {
            self.set_byte(at + k as u32, byte);
        }
        let first = at.wrapping_add(head as u32);
        let (words, tail) = bytes[head..].as_chunks::<4>();
        for (k, &src) in words.iter().enumerate() {
            self.cell(first + 4 * k as u32).store(u32::from_le_bytes(src), Ordering::Relaxed);
        }
        let tail_at = first.wrapping_add(4 * words.len() as u32);
        for (k, &byte) in tail.iter().enumerate() {
            self.set_byte(tail_at + k as u32, byte);
        }
    }
}

/// `at..at + len` split at the end of the address space: the part from `at` and the length of the part from 0.
fn split(at: u32, len: usize) -> (usize, usize) {
    let room = (1u64 << 32) - u64::from(at);
    let first = (len as u64).min(room) as usize;
    (first, len - first)
}

impl Backend for Pages {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        let (first, rest) = split(at, out.len());
        let (a, b) = out.split_at_mut(first);
        if (!a.is_empty() && !self.mapped_range(at, first as u32))
            || (!b.is_empty() && !self.mapped_range(0, rest as u32))
        {
            return false;
        }
        self.read_in(at, a);
        self.read_in(0, b);
        true
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        let (first, rest) = split(at, bytes.len());
        let (a, b) = bytes.split_at(first);
        if !a.is_empty() {
            self.touch_range(at, first as u32);
            self.write_in(at, a);
        }
        if !b.is_empty() {
            self.touch_range(0, rest as u32);
            self.write_in(0, b);
        }
    }

    fn read_u32(&self, at: u32) -> Option<u32> {
        self.word(at)
    }

    fn write_u32(&self, at: u32, value: u32) {
        self.set_word(at, value);
    }
}

impl Pages {
    /// Whether `other` is a clone of these pages.
    pub fn same(&self, other: &Pages) -> bool {
        Arc::ptr_eq(&self.flat, &other.flat)
    }

    /// The pages as address ranges `(base, size)`.
    pub fn regions(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for k in 0..PAGES as u32 {
            if !self.mapped(k * PAGE) {
                continue;
            }
            match out.last_mut() {
                Some((base, size)) if *base + *size == k * PAGE => *size += PAGE,
                _ => out.push((k * PAGE, PAGE)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_read_back_bytes_and_words_across_a_page_boundary() {
        let pages = Pages::default();
        let bytes: Vec<u8> = (0..23).collect();
        pages.write(0x2ff5, &bytes);
        let mut out = [0; 23];
        assert!(pages.read(0x2ff5, &mut out));
        assert_eq!(out, *bytes);
        assert_eq!(pages.read_u32(0x2ff8), Some(u32::from_le_bytes([3, 4, 5, 6])));
        assert_eq!(pages.read_u32(0x2ffe), Some(u32::from_le_bytes([9, 10, 11, 12])));
        assert_eq!(pages.words::<2>(0x2ff8), Some([pages.word(0x2ff8).unwrap(), pages.word(0x2ffc).unwrap()]));
        assert_eq!(pages.words::<3>(0x2ffa), Some([0x0807_0605, 0x0c0b_0a09, 0x100f_0e0d]));
        assert_eq!(pages.words::<4>(0x3ff8), None);
        pages.write_u32(0x3001, 0xa1b2_c3d4);
        pages.write_u32(0x3004, 0x0102_0304);
        let mut out = [0; 8];
        assert!(pages.read(0x3000, &mut out));
        assert_eq!(out, [11, 0xd4, 0xc3, 0xb2, 4, 3, 2, 1]);
        // The rest of a written page is zero; a page nothing wrote is unmapped.
        assert_eq!(pages.read_u32(0x2000), Some(0));
        assert_eq!(pages.read_u32(0x4000), None);
        assert!(!pages.read(0x3ffe, &mut [0; 4]));
        assert_eq!(pages.regions(), [(0x2000, 0x2000)]);
    }

    #[test]
    fn pages_tell_what_was_written_after_an_epoch() {
        let pages = Pages::default();
        pages.set_run(0x5ffc, &[1, 2]);
        let epoch = pages.end_epoch();
        assert!(!pages.written_after(0x5ffc, epoch) && !pages.written_after(0x6000, epoch));
        assert_eq!(pages.words::<2>(0x5ffc), Some([1, 2]));
        pages.set_word(0x6004, 3);
        assert!(!pages.written_after(0x5000, epoch) && pages.written_after(0x6000, epoch));
        let later = pages.end_epoch();
        assert!(later > epoch && !pages.written_after(0x6000, later));
    }

    #[test]
    fn epochs_past_32_bits_still_tell_a_later_write() {
        let pages = Pages::default();
        pages.set_word(0x7000, 1);
        pages.flat.epoch.store(u64::from(u32::MAX) - 1, Ordering::Relaxed);
        let before = pages.end_epoch();
        pages.set_word(0x8000, 2);
        let epoch = pages.end_epoch();
        let last = pages.end_epoch();
        assert!(epoch > u64::from(u32::MAX) - 1 && last > u64::from(u32::MAX));
        assert!(!pages.written_after(0x7000, before) && pages.written_after(0x8000, before));
        pages.set_word(0x7004, 3);
        assert!(pages.written_after(0x7000, epoch) && pages.written_after(0x7000, last));
        assert!(!pages.written_after(0x8000, epoch));
    }
}
