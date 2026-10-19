//! The harness's bookkeeping hooks (`harness/rust_step.py`): what its comparison rules need to know of a run, recorded
//! by native unicorn hooks instead of Python callbacks (about 0.6 us each, some 400,000 a step). The rules stay in
//! `rust_step.py`; each hook here is the native form of the Python function named in its doc, with the same state
//! and the same order of updates, and `rust_step.py` reads the results through the `rbr_book_*` functions.
//!
//! One [`Book`] per machine. The hooks are added in the order the harness asks for them (unicorn runs the hooks of
//! an event in the order they were added). Hooks never re-enter each other: none of them runs the emulator.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use crate::ffi::UcApi;

/// Bumped with any change to what the hooks record (`rust_step.py` keys run A's cache with it).
const VERSION: u32 = 1;

const ESP: i32 = 30;
const EIP: i32 = 26;
const HOOK_CODE: i32 = 4;
const HOOK_BLOCK: i32 = 8;
const HOOK_MEM_READ: i32 = 1 << 10;
const HOOK_MEM_WRITE: i32 = 1 << 11;

/// A set of byte addresses, as a bitmap per 4 KiB page.
#[derive(Default)]
struct ByteSet {
    pages: HashMap<u32, Box<[u64; 64]>>,
}

impl ByteSet {
    fn contains(&self, x: u32) -> bool {
        self.pages.get(&(x >> 12)).is_some_and(|p| p[(x as usize & 0xfff) >> 6] >> (x & 63) & 1 != 0)
    }

    fn insert(&mut self, x: u32) {
        let p = self.pages.entry(x >> 12).or_insert_with(|| Box::new([0; 64]));
        p[(x as usize & 0xfff) >> 6] |= 1 << (x & 63);
    }

    /// Removes x; whether it was there.
    fn remove(&mut self, x: u32) -> bool {
        match self.pages.get_mut(&(x >> 12)) {
            Some(p) => {
                let (w, bit) = ((x as usize & 0xfff) >> 6, 1u64 << (x & 63));
                let had = p[w] & bit != 0;
                p[w] &= !bit;
                had
            }
            None => false,
        }
    }

    fn all(&self, lo: u32, n: u32) -> bool {
        (0..n).all(|k| self.contains(lo.wrapping_add(k)))
    }

    fn any(&self, lo: u32, n: u32) -> bool {
        !self.pages.is_empty() && (0..n).any(|k| self.contains(lo.wrapping_add(k)))
    }

    fn insert_range(&mut self, lo: u32, n: u32) {
        for k in 0..n {
            self.insert(lo.wrapping_add(k));
        }
    }

    fn remove_range(&mut self, lo: u32, n: u32) {
        if !self.pages.is_empty() {
            for k in 0..n {
                self.remove(lo.wrapping_add(k));
            }
        }
    }

    fn clear(&mut self) {
        self.pages.clear();
    }
}

/// Byte runs (start, end), ascending.
type Runs = Vec<(u32, u32)>;

/// An open call of record_calls / track_own_writes: its entry esp, step, address, and the bytes written in
/// [entry - stack_fill, entry), as bits from entry - stack_fill.
struct Frame {
    entry: u32,
    tag: u32,
    addr: u32,
    bits: Vec<u64>,
}

impl Frame {
    /// Marks [addr, end) ∩ [entry - stack_fill, entry); `addr < entry`.
    fn mark(&mut self, addr: u32, end: u64, stack_fill: u32) {
        let lo = i64::from(self.entry) - i64::from(stack_fill);
        let from = i64::from(addr).max(lo);
        let to = (end as i64).min(i64::from(self.entry));
        for x in from..to {
            let k = (x - lo) as usize;
            self.bits[k >> 6] |= 1 << (k & 63);
        }
    }

    /// The marked bytes as (start, end) runs, ascending (rust_step.py's runs()).
    fn runs(&self, stack_fill: u32) -> Runs {
        let lo = i64::from(self.entry) - i64::from(stack_fill);
        let mut out = Runs::new();
        for (w, &word) in self.bits.iter().enumerate() {
            let mut word = word;
            while word != 0 {
                let b = word.trailing_zeros() as i64;
                let x = (lo + (w as i64) * 64 + b) as u32;
                match out.last_mut() {
                    Some(r) if r.1 == x => r.1 = x + 1,
                    _ => out.push((x, x + 1)),
                }
                word &= word - 1;
            }
        }
        out
    }
}

/// record_accesses: run A's accesses that can decide a read window.
#[derive(Default)]
struct Accesses {
    log: Vec<u32>,
    closed: ByteSet,
    /// The store instruction last entered.
    store: Option<u32>,
    loads: HashSet<u32>,
}

/// record_calls: run A's calls of the bound entries and the stack bytes each writes below its entry.
#[derive(Default)]
struct Calls {
    frames: Vec<Frame>,
    /// Closed calls in the order they return: tag, addr, entry, number of runs, then the runs (start, end).
    log: Vec<u32>,
}

/// fill_stale_reads: a stale-stack run's dead stack bytes.
struct Stale {
    lo: u32,
    dead: Vec<u8>,
    floor: usize,
    tops: [u32; 2],
    last: i64,
    fill: u8,
}

/// track_b_window: run B's reads of the bytes that differ from run A.
#[derive(Default)]
struct BWindow {
    open: ByteSet,
    /// byte -> the first instruction that read it while open.
    reads: HashMap<u32, u32>,
    /// A copy's store instruction -> the offsets of the open bytes its load read.
    pending: HashMap<u32, Vec<u32>>,
    copies: HashMap<u32, u32>,
}

/// install's call fill (with track_own_writes).
#[derive(Default)]
struct CallFill {
    fill: [u8; 4],
    stack_floor: u32,
    record: HashMap<(u32, u32, u32), Vec<Runs>>,
    occurrence: HashMap<(u32, u32, u32), usize>,
    /// aligned, full (fallback), bytes, own
    stats: [u64; 4],
}

pub struct Book {
    api: UcApi,
    tag: u32,
    stack_fill: u32,
    acc: Accesses,
    calls: Calls,
    stale: Option<Stale>,
    /// The writes of the step so far (addr, size, eip or binding), when the machine logs writes (Machine.writes).
    writes: Vec<(u32, u32, u32)>,
    /// byte -> the instruction (or binding) that last wrote it, over the committed steps (last_writers).
    last: HashMap<u32, u32>,
    b: BWindow,
    /// track_own_writes' open calls.
    own: Vec<Frame>,
    fill: CallFill,
    /// count_entries: the function starts, and their entries.
    starts: HashSet<u32>,
    entries: HashMap<u32, u64>,
    /// The array the last `rbr_book_*` query returned.
    out: Vec<u32>,
}

impl Book {
    fn reg(&self, id: i32) -> u32 {
        let mut v = 0u64;
        // SAFETY: the engine is valid while the book is; v holds a 32-bit register.
        unsafe { (self.api.reg_read)(self.api.uc, id, (&mut v as *mut u64).cast()) };
        v as u32
    }

    fn hook(&mut self, kind: i32, cb: *mut c_void, begin: u64, end: u64) -> bool {
        let mut h = 0usize;
        let me = (self as *mut Book).cast();
        // SAFETY: the engine is valid; cb has the signature unicorn expects for kind; the book outlives the engine's
        // runs (rust_step.py frees it with the engine).
        unsafe { (self.api.hook_add)(self.api.uc, &mut h, kind, cb, me, begin, end) == 0 }
    }

    // record_accesses
    fn acc_read(&mut self, addr: u32, size: u32) {
        if !self.acc.closed.all(addr, size) {
            let at = self.reg(EIP);
            self.acc.log.extend([addr, size, at]);
            if !self.acc.loads.contains(&at) {
                self.acc.closed.insert_range(addr, size);
            }
        }
    }

    fn acc_write(&mut self, addr: u32, size: u32) {
        const WRITE: u32 = 0x100;
        if let Some(store) = self.acc.store {
            let at = self.reg(EIP);
            if at == store {
                self.acc.log.extend([addr, size | WRITE, at]);
                self.acc.closed.remove_range(addr, size);
                return;
            }
            self.acc.store = None;
        }
        if !self.acc.closed.all(addr, size) {
            let at = self.reg(EIP);
            self.acc.log.extend([addr, size | WRITE, at]);
            self.acc.closed.insert_range(addr, size);
        }
    }

    // record_calls
    fn close_calls(&mut self, esp: u64) {
        while let Some(f) = self.calls.frames.last() {
            if esp <= u64::from(f.entry) {
                break;
            }
            let f = self.calls.frames.pop().unwrap();
            let runs = f.runs(self.stack_fill);
            self.calls.log.extend([f.tag, f.addr, f.entry, runs.len() as u32]);
            self.calls.log.extend(runs.iter().flat_map(|&(a, b)| [a, b]));
        }
    }

    fn calls_enter(&mut self, addr: u32) {
        let esp = self.reg(ESP);
        self.close_calls(esp.into());
        let bits = vec![0; self.stack_fill.div_ceil(64) as usize];
        self.calls.frames.push(Frame { entry: esp, tag: self.tag, addr, bits });
    }

    fn calls_write(&mut self, addr: u32, size: u32) {
        if let Some(top) = self.calls.frames.last() {
            let top = top.entry;
            let esp = self.reg(ESP);
            if esp > top {
                self.close_calls(esp.into());
            }
            let end = u64::from(addr) + u64::from(size);
            for f in &mut self.calls.frames {
                if addr >= f.entry {
                    break;
                }
                f.mark(addr, end, self.stack_fill);
            }
        }
    }

    // fill_stale_reads
    fn stale_block(&mut self, addr: u32) {
        let esp = i64::from(self.reg(ESP));
        let s = self.stale.as_mut().unwrap();
        let esp = esp - i64::from(s.lo);
        let n = s.dead.len() as i64;
        let clamp = |x: i64| x.clamp(0, n) as usize;
        if s.tops.contains(&addr) {
            let (floor, e) = (s.floor.min(n as usize), clamp(esp));
            if e > floor {
                s.dead[floor..e].fill(1);
            }
            s.dead[e..].fill(0);
        } else {
            let start = s.last.max(s.floor as i64);
            if esp > start {
                s.dead[clamp(start)..clamp(esp)].fill(1);
            }
        }
        s.last = esp;
    }

    fn stale_write(&mut self, addr: u32, size: u32) {
        let s = self.stale.as_mut().unwrap();
        let at = (addr - s.lo) as usize;
        let end = (at + size as usize).min(s.dead.len());
        s.dead[at..end].fill(0);
    }

    fn stale_read(&mut self, addr: u32, size: u32) {
        let s = self.stale.as_ref().unwrap();
        let at = (addr - s.lo) as usize;
        let end = (at + size as usize).min(s.dead.len());
        for x in at..end {
            if s.dead[x] != 0 {
                // SAFETY: the engine is valid; the byte is mapped (the hook covers the dumped stack).
                unsafe { (self.api.mem_write)(self.api.uc, u64::from(s.lo) + x as u64, &s.fill, 1) };
            }
        }
    }

    // track_b_window
    fn b_read(&mut self, addr: u32, size: u32) {
        if !self.b.open.any(addr, size) {
            return;
        }
        let at = self.reg(EIP);
        if let Some(&store) = self.b.copies.get(&at) {
            let hit = (0..size).filter(|&k| self.b.open.contains(addr + k)).collect();
            self.b.pending.insert(store, hit);
            return;
        }
        self.note_b_read(addr, size, at);
    }

    fn note_b_read(&mut self, addr: u32, size: u32, eip: u32) {
        for x in addr..addr + size {
            if self.b.open.remove(x) {
                self.b.reads.entry(x).or_insert(eip);
            }
        }
    }

    fn b_write(&mut self, addr: u32, size: u32) {
        self.b.open.remove_range(addr, size);
        if !self.b.pending.is_empty() {
            if let Some(hit) = self.b.pending.remove(&self.reg(EIP)) {
                for k in hit {
                    self.b.open.insert(addr + k);
                }
            }
        }
    }

    // track_own_writes
    fn own_close(&mut self, esp: u32) {
        while self.own.last().is_some_and(|f| esp > f.entry) {
            self.own.pop();
        }
    }

    fn own_enter(&mut self, addr: u32) {
        let esp = self.reg(ESP);
        self.own_close(esp);
        let bits = vec![0; self.stack_fill.div_ceil(64) as usize];
        self.own.push(Frame { entry: esp, tag: 0, addr, bits });
    }

    fn own_write(&mut self, addr: u32, size: u32) {
        if !self.own.is_empty() {
            let esp = self.reg(ESP);
            self.own_close(esp);
            let end = u64::from(addr) + u64::from(size);
            for f in &mut self.own {
                if addr >= f.entry {
                    break;
                }
                f.mark(addr, end, self.stack_fill);
            }
        }
    }

    /// own_writes(addr, entry): ends that call and gives its bytes (empty when it is not the innermost open call).
    fn own_writes(&mut self, addr: u32, entry: u32) -> Option<Frame> {
        while self.own.last().is_some_and(|f| f.entry <= entry && (f.entry, f.addr) != (entry, addr)) {
            self.own.pop();
        }
        if self.own.last().is_some_and(|f| (f.entry, f.addr) == (entry, addr)) {
            self.own.pop()
        } else {
            None
        }
    }

    /// install's call_fill: fills the free stack below a returned binding's entry.
    fn call_fill(&mut self, addr: u32, entry: u32, after: u32) {
        let key = (self.tag, addr, entry);
        let n = {
            let o = self.fill.occurrence.entry(key).or_default();
            *o += 1;
            *o - 1
        };
        let mut spans = match self.fill.record.get(&key).and_then(|l| l.get(n)) {
            Some(spans) => {
                self.fill.stats[0] += 1;
                spans.clone()
            }
            None => {
                self.fill.stats[1] += 1;
                let lo = (i64::from(entry) - i64::from(self.stack_fill)).max(i64::from(self.fill.stack_floor));
                vec![(lo as u32, after)]
            }
        };
        if let Some(own) = self.own_writes(addr, entry) {
            let own = own.runs(self.stack_fill);
            if !own.is_empty() {
                let in_own = |x: u32| own.iter().any(|&(a, b)| a <= x && x < b);
                let mut filled: Vec<u32> = spans.iter().flat_map(|&(a, b)| a..b).collect();
                filled.sort_unstable();
                filled.dedup();
                let before = filled.len();
                filled.retain(|&x| !in_own(x));
                self.fill.stats[3] += (before - filled.len()) as u64;
                spans = Vec::new();
                for x in filled {
                    match spans.last_mut() {
                        Some(r) if r.1 == x => r.1 = x + 1,
                        _ => spans.push((x, x + 1)),
                    }
                }
            }
        }
        for &(a, b) in &spans {
            if b > a {
                self.fill.stats[2] += u64::from(b - a);
                let data: Vec<u8> = (0..b - a).map(|k| self.fill.fill[k as usize % 4]).collect();
                // SAFETY: the engine is valid; a fill only covers the free stack.
                unsafe { (self.api.mem_write)(self.api.uc, a.into(), data.as_ptr(), data.len() as u64) };
            }
        }
    }

    /// The bytes written in the step so far, ascending (the chain's `opened`).
    fn written(&self) -> Vec<u32> {
        let mut out: Vec<u32> = self.writes.iter().flat_map(|&(a, n, _)| a..a + n).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn book<'a>(ud: *mut c_void) -> &'a mut Book {
    // SAFETY: ud is the Book the hook was added with (Book::hook), alive while its engine runs; hooks don't nest.
    unsafe { &mut *ud.cast::<Book>() }
}

type MemHook = unsafe extern "C" fn(*mut c_void, i32, u64, i32, i64, *mut c_void);
type CodeHook = unsafe extern "C" fn(*mut c_void, u64, u32, *mut c_void);

macro_rules! mem_hook {
    ($name:ident, $method:ident) => {
        unsafe extern "C" fn $name(_: *mut c_void, _: i32, addr: u64, size: i32, _: i64, ud: *mut c_void) {
            book(ud).$method(addr as u32, size as u32);
        }
    };
}
mem_hook!(acc_read, acc_read);
mem_hook!(acc_write, acc_write);
mem_hook!(calls_write, calls_write);
mem_hook!(stale_write, stale_write);
mem_hook!(stale_read, stale_read);
mem_hook!(b_read, b_read);
mem_hook!(b_write, b_write);
mem_hook!(own_write, own_write);

unsafe extern "C" fn acc_store(_: *mut c_void, addr: u64, _: u32, ud: *mut c_void) {
    book(ud).acc.store = Some(addr as u32);
}

unsafe extern "C" fn calls_enter(_: *mut c_void, addr: u64, _: u32, ud: *mut c_void) {
    book(ud).calls_enter(addr as u32);
}

unsafe extern "C" fn stale_block(_: *mut c_void, addr: u64, _: u32, ud: *mut c_void) {
    book(ud).stale_block(addr as u32);
}

unsafe extern "C" fn own_enter(_: *mut c_void, addr: u64, _: u32, ud: *mut c_void) {
    book(ud).own_enter(addr as u32);
}

unsafe extern "C" fn log_write(_: *mut c_void, _: i32, addr: u64, size: i32, _: i64, ud: *mut c_void) {
    let b = book(ud);
    let eip = b.reg(EIP);
    b.writes.push((addr as u32, size as u32, eip));
}

unsafe extern "C" fn count_entry(_: *mut c_void, addr: u64, _: u32, ud: *mut c_void) {
    let b = book(ud);
    let addr = addr as u32;
    if b.starts.contains(&addr) {
        *b.entries.entry(addr).or_default() += 1;
    }
}

fn mem(cb: MemHook) -> *mut c_void {
    cb as *mut c_void
}

fn code(cb: CodeHook) -> *mut c_void {
    cb as *mut c_void
}

/// # Safety
/// `p` must hold `n` words (or be null with `n` 0).
unsafe fn words<'a>(p: *const u32, n: u32) -> &'a [u32] {
    if n == 0 {
        &[]
    } else {
        // SAFETY: as documented.
        unsafe { std::slice::from_raw_parts(p, n as usize) }
    }
}

/// The version of what the hooks record.
#[no_mangle]
pub extern "C" fn rbr_book_version() -> u32 {
    VERSION
}

/// A book for the engine in `api`, with STACK_FILL `stack_fill`.
///
/// # Safety
/// `api` must be valid; the engine must stay usable until [`rbr_book_free`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_new(api: *const UcApi, stack_fill: u32) -> *mut Book {
    // SAFETY: as documented.
    let api = unsafe { std::ptr::read(api) };
    Box::into_raw(Box::new(Book {
        api,
        tag: 0,
        stack_fill,
        acc: Accesses::default(),
        calls: Calls::default(),
        stale: None,
        writes: Vec::new(),
        last: HashMap::new(),
        b: BWindow::default(),
        own: Vec::new(),
        fill: CallFill::default(),
        starts: HashSet::new(),
        entries: HashMap::new(),
        out: Vec::new(),
    }))
}

/// # Safety
/// `b` must come from [`rbr_book_new`], its engine closed or never run again.
#[no_mangle]
pub unsafe extern "C" fn rbr_book_free(b: *mut Book) {
    // SAFETY: as documented.
    drop(unsafe { Box::from_raw(b) });
}

/// Every `rbr_book_*` below: `b` must come from [`rbr_book_new`] and not be freed; arrays hold their counts.
/// The ones that add hooks return 0 when unicorn refused one.
///
/// record_accesses: a read hook, a write hook, a code hook at each of `stores`; `loads` are the copies' loads.
///
/// # Safety
/// See above.
#[no_mangle]
pub unsafe extern "C" fn rbr_book_accesses(
    b: *mut Book,
    loads: *const u32,
    n_loads: u32,
    stores: *const u32,
    n_stores: u32,
) -> i32 {
    let b = book(b.cast());
    // SAFETY: see above.
    b.acc.loads = unsafe { words(loads, n_loads) }.iter().copied().collect();
    let mut ok = b.hook(HOOK_MEM_READ, mem(acc_read), 1, 0) && b.hook(HOOK_MEM_WRITE, mem(acc_write), 1, 0);
    // SAFETY: see above.
    for &at in unsafe { words(stores, n_stores) } {
        ok &= b.hook(HOOK_CODE, code(acc_store), at.into(), at.into());
    }
    ok as i32
}

/// record_accesses' log so far (address, size | WRITE, eip), in `*out`/`*n`; then the log and the closed bytes are
/// emptied (a step's start). The array stays valid until the next query.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_take_accesses(b: *mut Book, out: *mut *const u32, n: *mut u32) {
    let b = book(b.cast());
    b.out = std::mem::take(&mut b.acc.log);
    b.acc.closed.clear();
    // SAFETY: see above.
    unsafe {
        *out = b.out.as_ptr();
        *n = b.out.len() as u32;
    }
}

/// record_calls: a write hook, then a code hook at each of `addrs`.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_calls(b: *mut Book, addrs: *const u32, n: u32) -> i32 {
    let b = book(b.cast());
    let mut ok = b.hook(HOOK_MEM_WRITE, mem(calls_write), 1, 0);
    // SAFETY: see above.
    for &at in unsafe { words(addrs, n) } {
        ok &= b.hook(HOOK_CODE, code(calls_enter), at.into(), at.into());
    }
    ok as i32
}

/// The step (m.tag) of the calls that start from now on; the call fill's occurrences start again.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_step(b: *mut Book, tag: u32) {
    let b = book(b.cast());
    b.tag = tag;
    b.fill.occurrence.clear();
}

/// record_calls' log of the calls closed so far (with `close`, all open calls are closed first), flat: tag, addr,
/// entry, number of runs, the runs (start, end); then the log is emptied. Valid until the next query.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_take_calls(b: *mut Book, close: i32, out: *mut *const u32, n: *mut u32) {
    let b = book(b.cast());
    if close != 0 {
        b.close_calls(1 << 32);
    }
    b.out = std::mem::take(&mut b.calls.log);
    // SAFETY: see above.
    unsafe {
        *out = b.out.as_ptr();
        *n = b.out.len() as u32;
    }
}

/// fill_stale_reads over the dumped stack [lo, lo + n): a block hook, then a write and a read hook on the stack.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_stale(
    b: *mut Book,
    lo: u32,
    n: u32,
    floor: u32,
    emit: u32,
    step_forward: u32,
    fill: u8,
) -> i32 {
    let b = book(b.cast());
    b.stale = Some(Stale {
        lo,
        dead: vec![0; n as usize],
        floor: (floor - lo) as usize,
        tops: [emit, step_forward],
        last: n.into(),
        fill,
    });
    let (start, end) = (u64::from(lo), u64::from(lo) + u64::from(n) - 1);
    (b.hook(HOOK_BLOCK, code(stale_block), 1, 0)
        && b.hook(HOOK_MEM_WRITE, mem(stale_write), start, end)
        && b.hook(HOOK_MEM_READ, mem(stale_read), start, end)) as i32
}

/// Machine.on_write: a write hook logging each write of the step (address, size, eip).
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_log_writes(b: *mut Book) -> i32 {
    let b = book(b.cast());
    b.hook(HOOK_MEM_WRITE, mem(log_write), 1, 0) as i32
}

/// A write the harness made (a stub's Machine.poke), logged in order with the hooks'.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_note_write(b: *mut Book, addr: u32, size: u32, eip: u32) {
    book(b.cast()).writes.push((addr, size, eip));
}

/// rbr-hybrid's write log (ffi.rs `Harness::log`, ctx the book): a binding's write closes run B's window of the
/// bytes and is logged in 4-byte chunks, attributed to the binding.
///
/// # Safety
/// `ctx` must be a book; `src` is not read.
#[no_mangle]
pub unsafe extern "C" fn rbr_book_binding_write(ctx: *mut c_void, at: u32, n: u32, _: *const u8, binding: u32) {
    let b = book(ctx);
    b.b.open.remove_range(at, n);
    for k in (0..n).step_by(4) {
        b.writes.push((at + k, (n - k).min(4), binding));
    }
}

/// The bytes written in the step so far, ascending, in `*out`/`*n`. Valid until the next query.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_written(b: *mut Book, out: *mut *const u32, n: *mut u32) {
    let b = book(b.cast());
    b.out = b.written();
    // SAFETY: see above.
    unsafe {
        *out = b.out.as_ptr();
        *n = b.out.len() as u32;
    }
}

/// last_writers: the step's writes go into the last-writer map, in order, and the log is emptied.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_commit(b: *mut Book) {
    let b = book(b.cast());
    for (a, n, eip) in std::mem::take(&mut b.writes) {
        for x in a..a + n {
            b.last.insert(x, eip);
        }
    }
}

/// Whether a committed write covered byte x; its last writer in `*eip`.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_last(b: *mut Book, x: u32, eip: *mut u32) -> i32 {
    match book(b.cast()).last.get(&x) {
        Some(&e) => {
            // SAFETY: see above.
            unsafe { *eip = e };
            1
        }
        None => 0,
    }
}

/// The number of bytes in the last-writer map.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_last_len(b: *mut Book) -> u32 {
    book(b.cast()).last.len() as u32
}

/// track_b_window: a read and a write hook; `loads[i]` is the load of the copy whose store is `stores[i]`.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_b_window(b: *mut Book, loads: *const u32, stores: *const u32, n: u32) -> i32 {
    let b = book(b.cast());
    // SAFETY: see above.
    let (loads, stores) = unsafe { (words(loads, n), words(stores, n)) };
    b.b.copies = loads.iter().copied().zip(stores.iter().copied()).collect();
    (b.hook(HOOK_MEM_READ, mem(b_read), 1, 0) && b.hook(HOOK_MEM_WRITE, mem(b_write), 1, 0)) as i32
}

/// Run B's open bytes (m.b_open) are now `xs`.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_set_open(b: *mut Book, xs: *const u32, n: u32) {
    let b = book(b.cast());
    b.b.open.clear();
    // SAFETY: see above.
    for &x in unsafe { words(xs, n) } {
        b.b.open.insert(x);
    }
}

/// Run B's reads of open bytes (m.b_reads), flat (byte, eip) in `*out`/`*n`. Valid until the next query.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_b_reads(b: *mut Book, out: *mut *const u32, n: *mut u32) {
    let b = book(b.cast());
    b.out = b.b.reads.iter().flat_map(|(&x, &e)| [x, e]).collect();
    // SAFETY: see above.
    unsafe {
        *out = b.out.as_ptr();
        *n = b.out.len() as u32;
    }
}

/// track_own_writes: a block hook at each of `addrs`, then a write hook on [lo, hi].
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_own_writes(b: *mut Book, addrs: *const u32, n: u32, lo: u32, hi: u32) -> i32 {
    let b = book(b.cast());
    let mut ok = true;
    // SAFETY: see above.
    for &at in unsafe { words(addrs, n) } {
        ok &= b.hook(HOOK_BLOCK, code(own_enter), at.into(), at.into());
    }
    (ok && b.hook(HOOK_MEM_WRITE, mem(own_write), lo.into(), hi.into())) as i32
}

/// The call fill's word and floor (install's `fill`, m.stack_floor).
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_fill(b: *mut Book, fill: u32, stack_floor: u32) {
    let b = book(b.cast());
    b.fill.fill = fill.to_le_bytes();
    b.fill.stack_floor = stack_floor;
}

/// Run A's call log for the step (rbr_book_take_calls' format), replacing the previous one.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_set_record(b: *mut Book, log: *const u32, n: u32) {
    let b = book(b.cast());
    // SAFETY: see above.
    let log = unsafe { words(log, n) };
    let mut record: HashMap<_, Vec<Runs>> = HashMap::new();
    let mut i = 0;
    while i < log.len() {
        let (tag, addr, entry, k) = (log[i], log[i + 1], log[i + 2], log[i + 3] as usize);
        let runs = (0..k).map(|j| (log[i + 4 + 2 * j], log[i + 5 + 2 * j])).collect();
        record.entry((tag, addr, entry)).or_default().push(runs);
        i += 4 + 2 * k;
    }
    b.fill.record = record;
}

/// rbr-hybrid's fill_call (ffi.rs `Harness::fill_call`, ctx the book): install's call_fill.
///
/// # Safety
/// `ctx` must be a book.
#[no_mangle]
pub unsafe extern "C" fn rbr_book_fill_call(ctx: *mut c_void, addr: u32, entry: u32, after: u32) {
    book(ctx).call_fill(addr, entry, after);
}

/// The call fill's counts: aligned, fallback, bytes filled, bytes left as run B's original code wrote them.
///
/// # Safety
/// `out` must hold 4 words.
#[no_mangle]
pub unsafe extern "C" fn rbr_book_fill_stats(b: *mut Book, out: *mut u64) {
    let b = book(b.cast());
    // SAFETY: as documented.
    unsafe { std::ptr::copy_nonoverlapping(b.fill.stats.as_ptr(), out, 4) };
}

/// count_entries: a block hook on [lo, hi] counting the entries of `starts`.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_count_entries(b: *mut Book, starts: *const u32, n: u32, lo: u32, hi: u32) -> i32 {
    let b = book(b.cast());
    // SAFETY: see above.
    b.starts = unsafe { words(starts, n) }.iter().copied().collect();
    b.hook(HOOK_BLOCK, code(count_entry), lo.into(), hi.into()) as i32
}

/// The entries counted, flat (addr, count low word, count high word), in `*out`/`*n`. Valid until the next query.
///
/// # Safety
/// See [`rbr_book_accesses`].
#[no_mangle]
pub unsafe extern "C" fn rbr_book_entries(b: *mut Book, out: *mut *const u32, n: *mut u32) {
    let b = book(b.cast());
    b.out = b.entries.iter().flat_map(|(&a, &c)| [a, c as u32, (c >> 32) as u32]).collect();
    // SAFETY: see above.
    unsafe {
        *out = b.out.as_ptr();
        *n = b.out.len() as u32;
    }
}
