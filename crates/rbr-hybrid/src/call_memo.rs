//! Calls run in place on the pure host whose effect is kept and written again while what they read stays as it was
//! (the E36 collect's transforms of static scenery: the same walk of the same frames every step). A call is recorded
//! once: the bytes it read before writing them, and the bytes it left written. It is replayed (its writes written, in
//! any order: only their last values remain) while the pages it read have not been written since
//! ([`Pages::written_after`]), the stack words it read hold the same values, and so do the bytes it read and then
//! wrote. A call that enters the host is not kept; nor is a call that keeps changing what it reads.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::{Arc, Mutex};

use rbr_physics::mem::{Backend, Mem, Pages};

use crate::Host;

/// The stack below and above a call's entry whose words it reads by value (anything there changes all the time).
const STACK_BELOW: u32 = 0x1_0000;
const STACK_ABOVE: u32 = 0x2000;
/// The misses after which a call that never hit is no longer recorded.
const GIVE_UP: u32 = 3;
/// The calls recorded in one step, at most ([`new_step`]): a section of new scenery brings a few hundred at once, each
/// a few times a plain run's cost, so the rest run plain and are recorded on the next steps.
const RECORDS_PER_STEP: u32 = 16;
const PAGE: u32 = 0x1000;

/// What a call is: its entry stack pointer, its target, `this` and its three arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key(pub(crate) [u32; 6]);

impl std::hash::Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for w in self.0 {
            state.write_u32(w);
        }
    }
}

/// A word the replay writes or checks: its address, its value and the mask of the bytes that count.
#[derive(Clone, Copy)]
struct Word {
    at: u32,
    value: u32,
    mask: u32,
}

struct Kept {
    epoch: u64,
    pages: Vec<u32>,
    values: Vec<Word>,
    /// The writes: runs of whole words (their start and their words in `words`), then the other words.
    runs: Vec<(u32, u32, u32)>,
    words: Vec<u32>,
    partial: Vec<Word>,
}

#[derive(Default)]
struct Entry {
    kept: Option<Kept>,
    hits: u32,
    misses: u32,
}

#[derive(Default)]
pub(crate) struct Memo {
    pages: Option<Pages>,
    calls: HashMap<Key, Entry, BuildHasherDefault<KeyHasher>>,
    /// The pages a kept call missed by, written between its runs: the calls check the bytes they read there by value.
    hot: HashSet<u32, BuildHasherDefault<KeyHasher>>,
    /// Calls recorded since the step began.
    recorded: u32,
}

impl Memo {
    /// Runs the call `key` (entry `sp`) with `run` on `mem`, or replays it. `run` returns whether it ran the call in
    /// place (false: it wrote nothing, and nothing is kept).
    pub(crate) fn call(
        &mut self,
        host: &dyn Host,
        mem: &mut Mem,
        key: Key,
        run: impl FnOnce(&mut Mem) -> bool,
    ) -> bool {
        let Some(pages) = mem.pages() else {
            return run(mem);
        };
        if !self.pages.as_ref().is_some_and(|p| p.same(pages)) {
            self.calls.clear();
            self.pages = Some(pages.clone());
        }
        let entry = self.calls.entry(key).or_default();
        if let Some(kept) = &mut entry.kept {
            let unwritten = kept.pages.iter().all(|&at| !pages.written_after(at, kept.epoch));
            if unwritten && kept.values.iter().all(|w| mem.paged_u32(w.at).is_some_and(|v| v & w.mask == w.value)) {
                for &(at, from, to) in &kept.runs {
                    pages.set_run(at, &kept.words[from as usize..to as usize]);
                }
                for w in &kept.partial {
                    write(pages, *w);
                }
                kept.epoch = pages.end_epoch();
                entry.hits += 1;
                return true;
            }
            let before = self.hot.len();
            self.hot.extend(kept.pages.iter().copied().filter(|&at| pages.written_after(at, kept.epoch)));
            if self.hot.len() == before {
                entry.misses += 1;
            }
            entry.kept = None;
        }
        let pages = pages.clone();
        if entry.hits == 0 && entry.misses >= GIVE_UP || self.recorded >= RECORDS_PER_STEP {
            return run(mem);
        }
        self.recorded += 1;
        let recorder = Arc::new(Recorder { mem: mem.clone(), log: Mutex::default() });
        let entered = host.entered();
        let mut recording = Mem::with_backend(recorder.clone());
        if let Some(runtime) = mem.ngp_runtime() {
            recording.set_ngp_runtime(runtime.clone());
        }
        if !run(&mut recording) {
            return false;
        }
        drop(recording);
        let log = std::mem::take(&mut *recorder.log.lock().unwrap_or_else(|p| p.into_inner()));
        let entry = self.calls.entry(key).or_default();
        if host.entered() != entered || log.foreign {
            entry.misses += 1;
            return true;
        }
        entry.kept = Some(log.kept(key.0[0], pages.end_epoch(), &self.hot));
        true
    }
}

fn write(pages: &Pages, w: Word) {
    if w.mask == !0 {
        pages.set_word(w.at, w.value);
        return;
    }
    for k in 0..4 {
        if w.mask >> (8 * k) & 0xff != 0 {
            pages.write(w.at + k, &[(w.value >> (8 * k)) as u8]);
        }
    }
}

/// A hash of the words of a [`Key`] (multiply and rotate; the keys are addresses).
#[derive(Default)]
pub(crate) struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_u8(*b);
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.write_u32(u32::from(i));
    }

    fn write_u32(&mut self, i: u32) {
        self.0 = (self.0.rotate_left(5) ^ u64::from(i)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_usize(&mut self, i: usize) {
        self.write_u32(i as u32);
    }
}

/// The bytes a recorded call read and wrote, in order (gathered into the bytes it read before it wrote them, their
/// first values, and the bytes it wrote, their last, when it is kept), and whether it reached bytes outside the pages
/// (then it is not kept).
#[derive(Default)]
struct Log {
    events: Vec<Event>,
    foreign: bool,
}

/// Bytes and their values, by address.
type Bytes = Vec<(u32, u8)>;

/// One byte read or written: its address, its place in the call's accesses (shifted left one, the low bit set for a
/// write) and its value.
struct Event {
    at: u32,
    seq: u32,
    byte: u8,
}

impl Log {
    fn log(&mut self, at: u32, bytes: &[u8], write: bool) {
        let seq = (self.events.len() as u32) << 1 | u32::from(write);
        self.events.extend(bytes.iter().enumerate().map(|(k, &byte)| Event {
            at: at.wrapping_add(k as u32),
            seq,
            byte,
        }));
    }

    /// The bytes read before any write to them (their first values) and the bytes written (their last), by address.
    fn read_written(mut self) -> (Bytes, Bytes) {
        self.events.sort_unstable_by_key(|e| u64::from(e.at) << 32 | u64::from(e.seq));
        let (mut read, mut written) = (Vec::new(), Vec::new());
        for group in self.events.chunk_by(|a, b| a.at == b.at) {
            if group[0].seq & 1 == 0 {
                read.push((group[0].at, group[0].byte));
            }
            if let Some(last) = group.iter().rev().find(|e| e.seq & 1 == 1) {
                written.push((last.at, last.byte));
            }
        }
        (read, written)
    }

    fn kept(self, sp: u32, epoch: u64, hot: &HashSet<u32, BuildHasherDefault<KeyHasher>>) -> Kept {
        let stack = sp.wrapping_sub(STACK_BELOW)..sp.wrapping_add(STACK_ABOVE);
        let (read, written) = self.read_written();
        let mut pages: Vec<u32> = Vec::new();
        let mut by_value = Vec::new();
        // Both by address: the writes walked along the reads, and the hot test made once a page.
        let mut writes = written.iter().map(|&(at, _)| at).peekable();
        let mut page = (1, false);
        for &(at, byte) in &read {
            while writes.next_if(|&w| w < at).is_some() {}
            if page.0 != at & !(PAGE - 1) {
                page = (at & !(PAGE - 1), hot.contains(&(at & !(PAGE - 1))));
            }
            if stack.contains(&at) || writes.peek() == Some(&at) || page.1 {
                by_value.push((at, byte));
            } else if pages.last() != Some(&page.0) {
                pages.push(page.0);
            }
        }
        let (mut runs, mut words_out, mut partial) = (Vec::new(), Vec::new(), Vec::new());
        for w in words(&written) {
            if w.mask != !0 {
                partial.push(w);
                continue;
            }
            let n = words_out.len() as u32;
            match runs.last_mut() {
                Some((at, from, to)) if *to == n && *at + 4 * (*to - *from) == w.at => *to += 1,
                _ => runs.push((w.at, n, n + 1)),
            }
            words_out.push(w.value);
        }
        Kept { epoch, pages, values: words(&by_value), runs, words: words_out, partial }
    }
}

/// `bytes` (by address) gathered into the aligned words that hold them.
fn words(bytes: &[(u32, u8)]) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for &(at, byte) in bytes {
        let word = at & !3;
        let shift = 8 * (at % 4);
        match out.last_mut() {
            Some(w) if w.at == word => {
                w.value |= u32::from(byte) << shift;
                w.mask |= 0xff << shift;
            }
            _ => out.push(Word { at: word, value: u32::from(byte) << shift, mask: 0xff << shift }),
        }
    }
    out
}

/// The memory, with the bytes a call reads and writes logged.
struct Recorder {
    mem: Mem,
    log: Mutex<Log>,
}

impl Backend for Recorder {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        let mut log = self.log.lock().unwrap_or_else(|p| p.into_inner());
        log.foreign |= self.mem.pages_for(at, out.len()).is_none();
        if !self.mem.try_read_into(at, out) {
            return false;
        }
        log.log(at, out, false);
        true
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        let mut log = self.log.lock().unwrap_or_else(|p| p.into_inner());
        log.foreign |= self.mem.pages_for(at, bytes.len()).is_none();
        self.mem.clone().map(at, bytes);
        log.log(at, bytes, true);
    }
}

thread_local! {
    static MEMO: std::cell::Cell<Option<Box<Memo>>> = std::cell::Cell::new(Some(Box::default()));
}

/// [`Memo::call`] with the thread's memo; a call made while it runs (from inside a kept call) is run as it is.
pub(crate) fn call(host: &dyn Host, mem: &mut Mem, key: Key, run: impl FnOnce(&mut Mem) -> bool) -> bool {
    let Some(mut memo) = MEMO.take() else {
        return run(mem);
    };
    let out = memo.call(host, mem, key, run);
    MEMO.set(Some(memo));
    out
}

/// A step begins: [`RECORDS_PER_STEP`] calls may be recorded again.
pub(crate) fn new_step() {
    if let Some(mut memo) = MEMO.take() {
        memo.recorded = 0;
        MEMO.set(Some(memo));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_keeps_its_writes_in_runs_and_its_stack_reads_by_value() {
        let sp = 0x10_0000;
        let mut log = Log::default();
        for (at, byte) in [(0x2000, 1), (0x2001, 2), (sp - 8, 7), (0x3004, 9)] {
            log.log(at, &[byte], false);
        }
        for at in (0x4000..0x4008).chain(0x400c..0x4010).chain([0x3004, 0x5001]) {
            log.log(at, &[at as u8], true);
        }
        // Read after its write: not a read of the call's input.
        log.log(0x4000, &[0x55], false);
        let hot = HashSet::from_iter([0x6000]);
        let kept = log.kept(sp, 5, &hot);
        assert_eq!(kept.pages, [0x2000]);
        let values: Vec<_> = kept.values.iter().map(|w| (w.at, w.value, w.mask)).collect();
        assert_eq!(values, [(0x3004, 9, 0xff), (sp - 8, 7, 0xff)]);
        assert_eq!(kept.runs, [(0x4000, 0, 2), (0x400c, 2, 3)]);
        assert_eq!(kept.words, [0x0302_0100, 0x0706_0504, 0x0f0e_0d0c]);
        let partial: Vec<_> = kept.partial.iter().map(|w| (w.at, w.value, w.mask)).collect();
        assert_eq!(partial, [(0x3004, 4, 0xff), (0x5000, 0x100, 0xff00)]);
    }
}
