//! The C ABI the harness calls (`harness/rust_step.py`). The harness hands over its unicorn engine and the library's
//! functions; [`rbr_hybrid_install`] hooks the bound entries in it. From then on the emulator stops at a bound entry
//! and the binding runs from here: the arguments, the memory, the original callees (nested runs) and the return all go
//! through unicorn's C API, so no harness code runs per call or per access. The harness is called back only to log
//! the bindings' writes (when it logs writes), to say whether its own hooks stopped a run, and on a failure.
//!
//! A panic in a binding (a port reading unmapped memory, a failed host call) is caught and reported through the
//! harness's `fail`; by then the binding may have written, so the run stops.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt::Write as _;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use rbr_physics::fpu::Precision;
use rbr_physics::mem::{Backend, Mem};

use crate::pure::CALL_RET;
use crate::{bindings, extended, from_extended, run, Called, Ctx, Decline, Host, Regs, STACK_ARGS};

type Uc = *mut c_void;

/// The unicorn functions the bindings use, and the engine (`uc_engine*`).
#[repr(C)]
pub struct UcApi {
    pub uc: Uc,
    pub mem_read: unsafe extern "C" fn(Uc, u64, *mut u8, u64) -> i32,
    pub mem_write: unsafe extern "C" fn(Uc, u64, *const u8, u64) -> i32,
    pub reg_read: unsafe extern "C" fn(Uc, i32, *mut c_void) -> i32,
    pub reg_write: unsafe extern "C" fn(Uc, i32, *const c_void) -> i32,
    pub emu_start: unsafe extern "C" fn(Uc, u64, u64, u64, usize) -> i32,
    pub emu_stop: unsafe extern "C" fn(Uc) -> i32,
    pub context_alloc: unsafe extern "C" fn(Uc, *mut *mut c_void) -> i32,
    pub context_save: unsafe extern "C" fn(Uc, *mut c_void) -> i32,
    pub context_restore: unsafe extern "C" fn(Uc, *mut c_void) -> i32,
    pub context_free: unsafe extern "C" fn(*mut c_void) -> i32,
    pub hook_add: unsafe extern "C" fn(Uc, *mut usize, i32, *mut c_void, *mut c_void, u64, u64, ...) -> i32,
    pub strerror: unsafe extern "C" fn(i32) -> *const c_char,
}

/// The harness's side.
#[repr(C)]
pub struct Harness {
    pub ctx: *mut c_void,
    /// Told of each write a binding makes (`len` bytes from `src` at `at`, by the binding at `binding`); null when
    /// the harness doesn't log writes.
    pub log: Option<unsafe extern "C" fn(ctx: *mut c_void, at: u32, len: u32, src: *const u8, binding: u32)>,
    /// Nonzero when the harness's own hooks stopped the run (checked after each nested run).
    pub stopped: unsafe extern "C" fn(ctx: *mut c_void) -> i32,
    /// Stops the run with a message (the first one stays).
    pub fail: unsafe extern "C" fn(ctx: *mut c_void, message: *const c_char),
    /// Null, or called after each binding returns (its address, its entry esp, the esp after its return) in place of
    /// the fill of `Options`: the harness writes what it wants below the entry there (`rust_step.py`'s call fill).
    pub fill_call: Option<unsafe extern "C" fn(ctx: *mut c_void, addr: u32, entry: u32, after: u32)>,
}

/// What the harness does around each binding (`Hybrid.install` in `rust_step.py`).
#[repr(C)]
pub struct Options {
    /// With `has_fill`, the free stack a binding leaves (`stack_fill` bytes below its entry, down to `stack_floor`,
    /// and its popped arguments) is filled with the word `fill`.
    pub has_fill: u32,
    pub fill: u32,
    pub stack_fill: u32,
    pub stack_floor: u32,
    /// With `has_perturb`, `perturb` is added to each x87 result, rounded to f32.
    pub has_perturb: u32,
    pub perturb: f64,
    /// NGP.dll's base in the snapshot, 0 without NGP: its bindings are hooked there (`ngp`).
    pub ngp_base: u32,
}

/// unicorn's ids of the registers used here (`x86_const.py`) and `UC_HOOK_BLOCK`.
const EAX: i32 = 19;
const EBP: i32 = 20;
const EBX: i32 = 21;
const ECX: i32 = 22;
const EDI: i32 = 23;
const EDX: i32 = 24;
const EIP: i32 = 26;
const ESI: i32 = 29;
const ESP: i32 = 30;
const FPSW: i32 = 31;
const FP0: i32 = 82;
const FPCW: i32 = 246;
const FPTAG: i32 = 247;
/// A block hook, not a code hook: at each instruction a code hook covers, unicorn walks every code hook (the harness
/// has one per x87 site, about 2,600), and the bound entries run some 700 times a step. The entries are call targets,
/// so a block starts at each.
const HOOK_BLOCK: i32 = 8;

/// unicorn's `uc_x86_float80`.
#[repr(C)]
#[derive(Default)]
struct Float80 {
    significand: u64,
    exponent: u16,
}

/// One install: the engine, the harness, and what the bindings did.
pub struct Native {
    api: UcApi,
    harness: Harness,
    options: Options,
    /// The binding that is running (the innermost), for the write log; 0 outside them.
    current: Cell<u32>,
    ran: RefCell<BTreeMap<u32, u64>>,
    declined: RefCell<BTreeMap<(u32, &'static str), u64>>,
    /// The host calls a pure step could not make (`pure::uncovered`), by the function it stops at and the binding.
    uncovered: RefCell<BTreeMap<(u32, u32), u64>>,
    /// Saved CPU contexts for the nested runs, one per depth, kept for reuse.
    contexts: RefCell<Vec<*mut c_void>>,
    counts: RefCell<CString>,
}

impl Native {
    fn reg(&self, id: i32) -> u32 {
        let mut v = 0u64;
        // SAFETY: the engine is valid while installed; v holds any of these registers.
        unsafe { (self.api.reg_read)(self.api.uc, id, (&mut v as *mut u64).cast()) };
        v as u32
    }

    fn set_reg(&self, id: i32, v: u32) {
        let v = u64::from(v);
        // SAFETY: as in reg.
        unsafe { (self.api.reg_write)(self.api.uc, id, (&v as *const u64).cast()) };
    }

    fn fp(&self, i: u32) -> Float80 {
        let mut v = Float80::default();
        // SAFETY: as in reg; v is a uc_x86_float80.
        unsafe { (self.api.reg_read)(self.api.uc, FP0 + i as i32, (&mut v as *mut Float80).cast()) };
        v
    }

    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        // SAFETY: as in reg; out holds its length.
        unsafe { (self.api.mem_read)(self.api.uc, at.into(), out.as_mut_ptr(), out.len() as u64) == 0 }
    }

    /// A write outside the bindings' log (the harness's own: arguments, the fill).
    fn put(&self, at: u32, bytes: &[u8]) {
        // SAFETY: as in read.
        let status = unsafe { (self.api.mem_write)(self.api.uc, at.into(), bytes.as_ptr(), bytes.len() as u64) };
        assert!(status == 0, "write of unmapped memory at {at:#010x}");
    }

    fn fail(&self, message: &str) {
        let message = CString::new(message.replace('\0', " ")).unwrap_or_default();
        // SAFETY: the harness's callbacks are valid while installed.
        unsafe {
            (self.harness.fail)(self.harness.ctx, message.as_ptr());
            (self.api.emu_stop)(self.api.uc);
        }
    }

    fn strerror(&self, err: i32) -> String {
        // SAFETY: uc_strerror returns a static string.
        unsafe { CStr::from_ptr((self.api.strerror)(err)) }.to_string_lossy().into_owned()
    }

    /// `fld` of an extended value: TOP down by one, the register valid (`push_st0` in `rust_step.py`).
    fn push_st0(&self, significand: u64, exponent: u16) {
        let sw = self.reg(FPSW);
        let top = (((sw >> 11) & 7).wrapping_sub(1)) & 7;
        self.set_reg(FPSW, (sw & !0x3800) | (top << 11));
        let v = Float80 { significand, exponent };
        // SAFETY: as in fp.
        unsafe { (self.api.reg_write)(self.api.uc, FP0 + top as i32, (&v as *const Float80).cast()) };
        self.set_reg(FPTAG, self.reg(FPTAG) & !(3 << (2 * top)));
    }

    /// The bound entry at `addr`, as the emulator reaches it.
    fn entry(&'static self, addr: u32) {
        let esp = self.reg(ESP);
        let mut words = [0u8; 4 * (STACK_ARGS + 1)];
        if !self.read(esp, &mut words) {
            self.fail(&format!("binding {addr:#x}: its stack is not mapped"));
            return;
        }
        let word = |k: usize| u32::from_le_bytes(words[4 * k..4 * k + 4].try_into().unwrap());
        let regs = Regs {
            ecx: self.reg(ECX),
            edx: self.reg(EDX),
            esp,
            eax: self.reg(EAX),
            ebx: self.reg(EBX),
            esi: self.reg(ESI),
            edi: self.reg(EDI),
            ebp: self.reg(EBP),
            stack: std::array::from_fn(|k| word(k + 1)),
        };
        // The ports' x87 precision is the emulator's control word's (PC, bits 8-9: 2 or 3 is 53 bits or more); a 1.02
        // snapshot runs at PC=24.
        let pc53 = crate::ngp::runs_ngp(self.options.ngp_base) && self.reg(FPCW) & 0x200 != 0;
        rbr_physics::fpu::set_precision(if pc53 { Precision::Pc53 } else { Precision::Pc24 });
        let emulator = Arc::new(Emulator(AssertUnwindSafe(self)));
        let _ngp = crate::ngp::Scope::new(self.options.ngp_base);
        let file = crate::ngp::file(addr);
        let file = crate::ngp::trampolines().find_map(|(t, exe)| (t == file).then_some(exe)).unwrap_or(file);
        let outer = self.current.replace(file);
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut ctx = Ctx { mem: Mem::with_backend(emulator.clone()), host: &*emulator };
            run(file, &mut ctx, &regs)
        }));
        self.current.set(outer);
        let r = match result {
            Ok(Ok(r)) => r,
            Ok(Err(decline)) => {
                let why = match decline {
                    Decline::NotBound => "not bound",
                    Decline::Unsupported(why) => why,
                };
                *self.declined.borrow_mut().entry((file, why)).or_default() += 1;
                return; // the original runs
            }
            Err(panic) => {
                let why = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                self.fail(&format!("binding {file:#x} failed: {why}"));
                return;
            }
        };
        *self.ran.borrow_mut().entry(file).or_default() += 1;
        if let Some(st0) = r.st0 {
            let st0 = if self.options.has_perturb != 0 { f64::from((st0 + self.options.perturb) as f32) } else { st0 };
            let (significand, exponent) = extended(st0);
            self.push_st0(significand, exponent);
        }
        let o = &self.options;
        if let Some(fill_call) = self.harness.fill_call {
            // SAFETY: the harness's callback is valid while installed; it may write the engine's memory.
            unsafe { fill_call(self.harness.ctx, addr, esp, esp + 4 + r.pop) };
        } else if o.has_fill != 0 {
            let lo = esp.saturating_sub(o.stack_fill).max(o.stack_floor);
            let n = (esp + 4 + r.pop - lo) / 4;
            self.put(lo, &o.fill.to_le_bytes().repeat(n as usize));
        }
        self.set_reg(EAX, r.eax);
        self.set_reg(ESP, esp + 4 + r.pop);
        self.set_reg(EIP, word(0));
    }

    /// The counts as text: `R addr n` per binding that ran, `D addr n reason` per decline, `U addr n caller` per
    /// function a pure step would stop at.
    fn counts(&self) -> *const c_char {
        let mut out = String::new();
        for (addr, n) in self.ran.borrow().iter() {
            let _ = writeln!(out, "R {addr} {n}");
        }
        for ((addr, why), n) in self.declined.borrow().iter() {
            let _ = writeln!(out, "D {addr} {n} {}", why.replace('\n', " "));
        }
        for ((addr, caller), n) in self.uncovered.borrow().iter() {
            let _ = writeln!(out, "U {addr} {n} {caller}");
        }
        *self.counts.borrow_mut() = CString::new(out.replace('\0', " ")).unwrap_or_default();
        self.counts.borrow().as_ptr()
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        for c in self.contexts.get_mut().drain(..) {
            // SAFETY: c came from uc_context_alloc; it may outlive its engine (uc_close detaches it).
            unsafe { (self.api.context_free)(c) };
        }
    }
}

/// The engine as a [`Mem`] backend and a [`Host`], for one binding call.
struct Emulator(AssertUnwindSafe<&'static Native>);

// SAFETY: the emulator is single-threaded; an Emulator lives only inside one binding call on that thread.
unsafe impl Send for Emulator {}
// SAFETY: as above.
unsafe impl Sync for Emulator {}

impl Backend for Emulator {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        self.0.read(at, out)
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        let n: &Native = *self.0;
        n.put(at, bytes);
        if let Some(log) = n.harness.log {
            // SAFETY: the harness's callbacks are valid while installed.
            unsafe { log(n.harness.ctx, at, bytes.len() as u32, bytes.as_ptr(), n.current.get()) }
        }
    }
}

impl Host for Emulator {
    /// A nested run of the original at `addr`, from `sp` (the callee's entry) or else from 0x1000 below the current
    /// stack; the registers come back as they were.
    fn call_at(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
        self.call_with(sp, addr, ecx, None, args)
    }

    fn call_at_esi(&self, sp: u32, addr: u32, ecx: u32, esi: u32, args: &[u32]) -> Called {
        self.call_with(sp, addr, ecx, Some(esi), args)
    }
}

impl Emulator {
    fn call_with(&self, sp: u32, addr: u32, ecx: u32, esi: Option<u32>, args: &[u32]) -> Called {
        let n: &Native = *self.0;
        let mem = Mem::with_backend(Arc::new(Emulator(AssertUnwindSafe(n))));
        if let Some(at) = crate::pure::uncovered(&mem, addr, ecx, args) {
            *n.uncovered.borrow_mut().entry((at, n.current.get())).or_default() += 1;
        }
        let saved = n.contexts.borrow_mut().pop().unwrap_or_else(|| {
            let mut c = std::ptr::null_mut();
            // SAFETY: the engine is valid while installed.
            assert!(unsafe { (n.api.context_alloc)(n.api.uc, &mut c) } == 0, "uc_context_alloc failed");
            c
        });
        // SAFETY: saved is a context of this engine.
        unsafe { (n.api.context_save)(n.api.uc, saved) };
        let sp = if sp == 0 { (n.reg(ESP) - 0x1000) & !0xf } else { sp };
        let frame: Vec<u8> = std::iter::once(CALL_RET).chain(args.iter().copied()).flat_map(u32::to_le_bytes).collect();
        n.put(sp, &frame);
        n.set_reg(ESP, sp);
        n.set_reg(ECX, ecx);
        if let Some(esi) = esi {
            n.set_reg(ESI, esi);
        }
        // SAFETY: as above.
        let err = unsafe { (n.api.emu_start)(n.api.uc, addr.into(), CALL_RET.into(), 0, 0) };
        let eip = n.reg(EIP);
        // SAFETY: the harness's callbacks are valid while installed.
        let stopped = unsafe { (n.harness.stopped)(n.harness.ctx) } != 0;
        let called = (err == 0 && eip == CALL_RET && !stopped).then(|| {
            let top = (n.reg(FPSW) >> 11) & 7;
            let st0 = n.fp(top);
            Called { eax: n.reg(EAX), st0: from_extended(st0.significand, st0.exponent) }
        });
        // SAFETY: as above.
        unsafe { (n.api.context_restore)(n.api.uc, saved) };
        n.contexts.borrow_mut().push(saved);
        match called {
            Some(called) => called,
            None => {
                if err != 0 {
                    n.fail(&format!("host call {addr:#x}: {} at eip {eip:#x}", n.strerror(err)));
                } else {
                    n.fail(&format!("host call {addr:#x} did not return"));
                }
                panic!("the emulator's run of {addr:#x} stopped");
            }
        }
    }
}

unsafe extern "C" fn on_entry(_: Uc, address: u64, _: u32, native: *mut c_void) {
    // SAFETY: native is the installed Native (rbr_hybrid_install), alive until rbr_hybrid_free.
    let native: &'static Native = unsafe { &*(native as *const Native) };
    native.entry(address as u32);
}

/// Writes the bound entries to `out` (up to `cap`) and returns how many there are.
///
/// # Safety
/// `out` must hold `cap` words.
#[no_mangle]
pub unsafe extern "C" fn rbr_hybrid_bound(out: *mut u32, cap: u32) -> u32 {
    let mut n = 0;
    for b in bindings() {
        if n < cap {
            // SAFETY: n < cap, and out holds cap words.
            unsafe { *out.add(n as usize) = b.addr };
        }
        n += 1;
    }
    n
}

/// Hooks the bound entries (the `n_only` in `only` when it is not null) in the engine. Returns the install, for
/// [`rbr_hybrid_counts`] and [`rbr_hybrid_free`], or null when a hook could not be added.
///
/// # Safety
/// The three structs must be valid, and the engine and callbacks usable until [`rbr_hybrid_free`] (and the engine
/// closed or no longer run by then). `only` must hold `n_only` words.
#[no_mangle]
pub unsafe extern "C" fn rbr_hybrid_install(
    api: *const UcApi,
    harness: *const Harness,
    options: *const Options,
    only: *const u32,
    n_only: u32,
) -> *mut Native {
    // SAFETY: the caller guarantees the pointers.
    let (api, harness, options) = unsafe { (std::ptr::read(api), std::ptr::read(harness), std::ptr::read(options)) };
    let only = (!only.is_null()).then(|| unsafe { std::slice::from_raw_parts(only, n_only as usize) });
    let native = Box::into_raw(Box::new(Native {
        api,
        harness,
        options,
        current: Cell::new(0),
        ran: RefCell::default(),
        declined: RefCell::default(),
        uncovered: RefCell::default(),
        contexts: RefCell::default(),
        counts: RefCell::default(),
    }));
    // SAFETY: native was just made.
    let n = unsafe { &*native };
    let _ngp = crate::ngp::Scope::new(n.options.ngp_base);
    let bound = |addr: u32| only.is_none_or(|o| o.contains(&addr));
    // A trampoline jumps into its exe function past the entry, so it is hooked for the exe function's binding.
    let trampolines = crate::ngp::trampolines().filter(|&(_, exe)| bound(exe) && crate::binding(exe).is_some());
    let entries = bindings().filter(|b| bound(b.addr)).map(|b| b.addr).chain(trampolines.map(|(t, _)| t));
    for entry in entries {
        let addr = if crate::ngp::is_file(entry) {
            match crate::ngp::at(entry) {
                Some(at) => at,
                None => continue, // a 1.02 snapshot
            }
        } else {
            entry
        };
        let mut hook = 0usize;
        let at = u64::from(addr);
        // SAFETY: the engine is valid; on_entry has uc_cb_hookcode_t's signature (block hooks take it too).
        let err = unsafe {
            (n.api.hook_add)(n.api.uc, &mut hook, HOOK_BLOCK, on_entry as *mut c_void, native.cast(), at, at)
        };
        if err != 0 {
            return std::ptr::null_mut(); // the hooks added so far refer to it: left allocated
        }
    }
    native
}

/// The bindings' counts so far (see [`Native::counts`]), valid until the next call.
///
/// # Safety
/// `native` must come from [`rbr_hybrid_install`] and not be freed.
#[no_mangle]
pub unsafe extern "C" fn rbr_hybrid_counts(native: *const Native) -> *const c_char {
    // SAFETY: as documented.
    unsafe { &*native }.counts()
}

/// Frees an install, once its engine is closed (its hooks refer to it).
///
/// # Safety
/// `native` must come from [`rbr_hybrid_install`], and its engine never run again.
#[no_mangle]
pub unsafe extern "C" fn rbr_hybrid_free(native: *mut Native) {
    // SAFETY: as documented.
    drop(unsafe { Box::from_raw(native) });
}
