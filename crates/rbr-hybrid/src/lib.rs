//! Runs `rbr-physics` ports in place of original functions while the original's `StepForward` runs in the
//! emulator (`harness/rust_step.py`). The emulator stops at the entry
//! of a bound function and hands its registers and stack arguments to [`run`]. The binding runs the port on the
//! emulator's memory (a [`Mem`] with the emulator as its backend), calls back into the emulator for the original
//! callees the port leaves to its caller, and says what the original's `ret` leaves: eax, the x87 result and the
//! bytes it pops.
//!
//! A binding declines (and the original runs) only before it changes anything. The C ABI is in [`ffi`].

pub mod book;
mod call_memo;
pub mod car_body;
pub mod collision;
pub mod engine_driveline;
pub mod entities;
pub mod ffi;
pub mod game_log;
pub mod graph;
pub mod ngp;
pub mod ntweak;
pub mod physics_core;
mod probe_cache;
pub mod pure;
pub mod rigid_body;
pub mod spring_damper;
pub mod timekeeper;
pub mod tyre;
pub mod vcu;
pub mod wheel;

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::SignalHost;

/// The registers and the stack arguments at a function's entry (the words after the return address).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Regs {
    pub ecx: u32,
    pub edx: u32,
    /// The return address's slot. The stack below it is free: the original's frame was there, and a binding
    /// may put what the original passes by address there (`engine_driveline`'s gear-change payload).
    pub esp: u32,
    /// The entry values of the other registers (a binding that does not read them ignores them).
    pub eax: u32,
    pub ebx: u32,
    pub esi: u32,
    pub edi: u32,
    pub ebp: u32,
    pub stack: [u32; STACK_ARGS],
}

/// Stack words passed to a binding. The most an original takes is 17 (`CScratch::ComputeOutputs`).
pub const STACK_ARGS: usize = 20;

/// What the original's `ret` leaves to the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Return {
    pub eax: u32,
    /// The x87 result pushed on the FPU stack, for a float-returning function.
    pub st0: Option<f64>,
    /// Argument bytes the callee pops (stdcall and thiscall); 0 for cdecl.
    pub pop: u32,
}

impl Return {
    /// A function with no result that pops `pop` bytes. `eax` is the residue the original leaves, when known.
    pub fn void(eax: u32, pop: u32) -> Self {
        Return { eax, st0: None, pop }
    }

    pub fn float(st0: f64, pop: u32) -> Self {
        Return { eax: 0, st0: Some(st0), pop }
    }

    /// A float-returning function that also leaves `eax` (the residue of an argument or `this`, as in the methods of
    /// `E38Vec` that return `this`).
    pub fn float_with_eax(eax: u32, st0: f64, pop: u32) -> Self {
        Return { eax, st0: Some(st0), pop }
    }
}

/// Why a binding did not run. The emulator then runs the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decline {
    /// No binding at this address.
    NotBound,
    /// The inputs take a path the port does not cover (or the binding doesn't map yet).
    Unsupported(&'static str),
}

/// What an original function returned: eax and the x87 top of stack (meaningful only for a float function).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Called {
    pub eax: u32,
    pub st0: f64,
}

/// The emulator: runs an original function (thiscall with `ecx`, or stdcall/cdecl with `ecx` ignored) on the
/// stack arguments `args` and returns what it left.
pub trait Host {
    /// `call` with the callee's entry stack pointer `sp` (its return address slot). `sp == 0` leaves the placement to
    /// the harness, which runs the callee below the binding's frame.
    fn call_at(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called;
    fn call(&self, addr: u32, ecx: u32, args: &[u32]) -> Called {
        self.call_at(0, addr, ecx, args)
    }
    /// `call_at` with `esi` set too, for a callee that reads its caller's `esi` (NGP's tyre particle emit wrapper).
    /// Only the emulator has registers to set; elsewhere it is `call_at`.
    fn call_at_esi(&self, sp: u32, addr: u32, ecx: u32, _esi: u32, args: &[u32]) -> Called {
        self.call_at(sp, addr, ecx, args)
    }
    /// The return address `call_at` writes in the callee's slot, for a host whose call writes only that slot and the
    /// arguments before the callee's binding runs (the pure host's [`pure::CALL_RET`]); `None` for a host that runs
    /// the original (the emulator). See [`skip_call`].
    fn call_slot_word(&self) -> Option<u32> {
        None
    }
    /// The bindings and stand-ins entered so far, for a host whose calls write only what [`Host::call_slot_word`]
    /// says before the callee runs: a caller that skips a call knows nothing ran in between while the count stays.
    fn entered(&self) -> u64 {
        0
    }
    /// Counts a binding a caller ran itself in place of a call ([`Host::entered`] goes up as for a run).
    fn ran_direct(&self) {}
    /// [`Host::call_at`] of a callee whose binding reads its stack words only from `args` (none through `esp`), and
    /// whose caller reads none of the call's words after it. Under the pure host's direct dispatch (B1b) the call
    /// runs as a plain call, without its words on the stack, when `args` are the words the callee pops; elsewhere it
    /// is `call_at`.
    fn call_direct(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
        self.call_at(sp, addr, ecx, args)
    }
    /// [`Host::call_slot_word`] and whether [`skip_call`] writes the skipped call's words (not under the pure host's
    /// direct dispatch, [`pure::Pure::direct`]), in one call for the skipped calls.
    fn skip_word(&self) -> Option<(u32, bool)> {
        self.call_slot_word().map(|word| (word, true))
    }
}

/// For a call at `sp` whose callee is known to do nothing more than the caller does itself (a getter, a walk of equal
/// nodes): writes the words `host.call_at(sp, ..)` writes before the callee runs, the return address and `args`, and
/// returns true, so the caller skips the call; false (nothing written) on a host that runs the original or without a
/// given `sp`. The pure host's `ran` does not count a skipped call. Under the direct dispatch nothing is written
/// ([`Host::skip_word`]).
pub(crate) fn skip_call(host: &dyn Host, mem: &mut Mem, sp: u32, args: &[u32]) -> bool {
    skip_call_with(host, mem, sp, args, false)
}

/// [`skip_call`] that writes the words under the direct dispatch too, for a call whose words are read after it: the
/// child walk a walker runs in place (`entities/walks.rs`: without them `--twin mem-direct` differs at the tanner
/// drive's first step).
pub(crate) fn skip_call_writing(host: &dyn Host, mem: &mut Mem, sp: u32, args: &[u32]) -> bool {
    skip_call_with(host, mem, sp, args, true)
}

fn skip_call_with(host: &dyn Host, mem: &mut Mem, sp: u32, args: &[u32], always_write: bool) -> bool {
    let Some((ret, write)) = host.skip_word().filter(|_| sp != 0) else {
        return false;
    };
    if write || always_write {
        for (k, w) in std::iter::once(ret).chain(args.iter().copied()).enumerate() {
            mem.set_u32(sp + 4 * k as u32, w);
        }
    }
    true
}

/// The frame base of a binding's original after `push ebp; mov ebp, esp; and esp, -16`, from its entry `esp` (the
/// return address's slot); the original's locals and its callees' pushes are at fixed offsets below it.
pub(crate) fn aligned_frame(esp: u32) -> u32 {
    esp.wrapping_sub(4) & !0xf
}

/// The `CMultiSignal1::Emit` instances, which differ only in their function name (the `%s` of their two log lines):
/// `(emit, name)`.
const EMIT_NAMES: [(u32, u32); 14] = [
    (0x48cd90, 0x72f948), // CSigCarControl
    (0x4a0390, 0x730b68), // CSigCheckPointTime
    (0x4e4490, 0x737f70), // CSigCarBrakeWallPenetration
    (0x4e8cb0, 0x738a40), // CSigMrRCarInit
    (0x4ef350, 0x739188), // CSigCarDamageHandlerInit
    (0x506f20, 0x73a238), // CSigMrRCarGearChange
    (0x50ef50, 0x73b778), // CSigEngineNoise
    (0x51f580, 0x73d3c0), // CSigDetachFromParent
    (0x51f700, 0x73d4b0), // CSigMrRBodyState
    (0x51f7c0, 0x73d528), // CSigMrRBodyEvent
    (0x523010, 0x73e8b0), // CIncarShake
    (0x52be70, 0x73fb80), // CSolid's damping collision
    (0x534f80, 0x740bf8), // CSigTyreNoise
    (0x53d0d0, 0x742a70), // CSigTireParticles
];

/// The function name the emit at `emit` logs with.
pub(crate) const fn emit_name(emit: u32) -> u32 {
    let mut k = 0;
    while EMIT_NAMES[k].0 != emit {
        k += 1;
    }
    EMIT_NAMES[k].1
}

/// The recursion's log line of a `CMultiSignal1::Emit` (`E01Log(6, 0, fmt, name)`). The count's (`E01Log(7, 0, ...)`)
/// is followed by `int3`: the bindings decline that path ([`decline_emit_trap`]).
const MSG_RECURSIVE: u32 = 0x730358;

/// Declines a `CMultiSignal1::Emit` on the signal at `signal` whose count is out of range: the original logs, then
/// traps (`physics_core::layout::emit_traps`).
pub(crate) fn decline_emit_trap(mem: &Mem, signal: u32) -> Result<(), Decline> {
    if rbr_physics::physics_core::layout::emit_traps(mem, signal) {
        return Err(Decline::Unsupported("the emit's count is out of range (the original logs, then traps at int3)"));
    }
    Ok(())
}

/// The connections of the `CMultiSignal1` instance named `.1` (see [`emit_name`]), invoked in the emulator: `Invoke`
/// is vtable slot 1, thiscall on the connection with the signal; the recursion's log is an `E01Log` host call.
pub(crate) struct SignalCallees<'a>(pub &'a dyn Host, pub u32);

impl SignalHost for SignalCallees<'_> {
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32) {
        let target = mem.u32(mem.u32(conn) + 4);
        self.0.call(target, conn, &[sig]);
    }

    fn log_recursive(&mut self) {
        self.0.call(engine_driveline::E01LOG, 0, &[6, 0, MSG_RECURSIVE, self.1]);
    }

    fn log_range(&mut self, _count: u32) {
        unreachable!("the original traps after the count's log")
    }
}

/// [`SignalCallees`] with the calls at the original's stack: each `Invoke` and the recursion's log are entered at `sp`,
/// the return slot of the emit's own call to them (four pushes and the argument below the entry).
pub(crate) struct SignalCalleesAt<'a> {
    pub host: &'a dyn Host,
    pub sp: u32,
    pub name: u32,
}

impl SignalHost for SignalCalleesAt<'_> {
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32) {
        let target = mem.u32(mem.u32(conn) + 4);
        self.host.call_at(self.sp, target, conn, &[sig]);
    }

    fn log_recursive(&mut self) {
        self.log_recursive_eax();
    }

    fn log_range(&mut self, _count: u32) {
        unreachable!("the binding declines a count out of range (the original traps)")
    }
}

impl SignalCalleesAt<'_> {
    /// [`SignalHost::log_recursive`], eax after it.
    pub(crate) fn log_recursive_eax(&self) -> u32 {
        self.host.call_at(self.sp, engine_driveline::E01LOG, 0, &[6, 0, MSG_RECURSIVE, self.name]).eax
    }
}

/// A binding's view of the game: its memory and the original callees.
pub struct Ctx<'a> {
    pub mem: Mem,
    pub host: &'a dyn Host,
}

impl Ctx<'_> {
    /// An original float accessor (`fld` and `ret`), rounded to f32 as the callers store it.
    pub fn get_f32(&self, addr: u32, this: u32) -> f32 {
        self.host.call(addr, this, &[]).st0 as f32
    }
}

pub type BindingFn = fn(&mut Ctx, &Regs) -> Result<Return, Decline>;

/// An original function and the binding that runs its port.
pub struct Binding {
    pub addr: u32,
    pub name: &'static str,
    pub subsystem: &'static str,
    pub run: BindingFn,
}

/// Every bound function.
pub fn bindings() -> impl Iterator<Item = &'static Binding> {
    vcu::BINDINGS
        .iter()
        .chain(&car_body::BINDINGS)
        .chain(&collision::BINDINGS)
        .chain(&spring_damper::BINDINGS)
        .chain(&tyre::BINDINGS)
        .chain(&engine_driveline::BINDINGS)
        .chain(&wheel::BINDINGS)
        .chain(&rigid_body::BINDINGS)
        .chain(&entities::BINDINGS)
        .chain(&ntweak::BINDINGS)
        .chain(&physics_core::BINDINGS)
        .chain(&timekeeper::BINDINGS)
        .chain(&ngp::BINDINGS)
        .chain(&game_log::BINDINGS)
        .chain(&graph::BINDINGS)
}

/// The binding of the original function at `addr`.
pub fn binding(addr: u32) -> Option<&'static Binding> {
    binding_slot(addr).map(|slot| bound()[slot])
}

/// The bindings by address, the first of each address kept (as `bindings().find` finds it).
pub fn bound() -> &'static [&'static Binding] {
    static BY_ADDR: std::sync::OnceLock<Vec<&'static Binding>> = std::sync::OnceLock::new();
    BY_ADDR.get_or_init(|| {
        let mut all: Vec<&'static Binding> = bindings().collect();
        all.sort_by_key(|b| b.addr);
        all.dedup_by_key(|b| b.addr);
        all
    })
}

/// The index in [`bound`] of the binding at `addr`, searched in a table of the addresses alone (one cache line holds
/// 16 of them; [`bound`]'s entries point elsewhere).
pub fn binding_slot(addr: u32) -> Option<usize> {
    static ADDRS: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    ADDRS.get_or_init(|| bound().iter().map(|b| b.addr).collect()).binary_search(&addr).ok()
}

/// Runs the binding of the original function at `addr`.
pub fn run(addr: u32, ctx: &mut Ctx, regs: &Regs) -> Result<Return, Decline> {
    match binding(addr) {
        Some(b) => (b.run)(ctx, regs),
        None => Err(Decline::NotBound),
    }
}

/// An f64 as the x87 holds it: the 64-bit significand with its explicit integer bit, and sign plus 15-bit exponent.
pub fn extended(x: f64) -> (u64, u16) {
    let bits = x.to_bits();
    let sign = ((bits >> 63) as u16) << 15;
    let exp = ((bits >> 52) & 0x7ff) as u16;
    let frac = bits & ((1 << 52) - 1);
    match exp {
        0 if frac == 0 => (0, sign),
        0 => {
            // Subnormal: frac * 2^-1074, normalised so the top bit is the integer bit.
            let shift = frac.leading_zeros();
            (frac << shift, sign | (15372 - shift) as u16)
        }
        0x7ff if frac == 0 => (1 << 63, sign | 0x7fff),
        // NaN keeps its payload and is quiet, as `fld` of a double gives.
        0x7ff => (1 << 63 | 1 << 62 | frac << 11, sign | 0x7fff),
        _ => (1 << 63 | frac << 11, sign | (exp + 16383 - 1023)),
    }
}

/// An x87 register's value as the nearest f64 (exact for every value an f32 or f64 load gives).
pub fn from_extended(significand: u64, exponent: u16) -> f64 {
    let sign = if exponent & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = (exponent & 0x7fff) as i32;
    if e == 0x7fff {
        return if significand << 1 == 0 { sign * f64::INFINITY } else { f64::NAN };
    }
    // Two exact scalings keep the exponent of each factor in range.
    let scale = e - 16383 - 63;
    sign * significand as f64 * 2f64.powi(scale / 2) * 2f64.powi(scale - scale / 2)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn extended_of_normals_zeros_and_specials() {
        assert_eq!(extended(1.0), (1 << 63, 0x3fff));
        assert_eq!(extended(-2.5), (0xa000_0000_0000_0000, 0xc000));
        assert_eq!(extended(0.0), (0, 0));
        assert_eq!(extended(-0.0), (0, 0x8000));
        assert_eq!(extended(f64::INFINITY), (1 << 63, 0x7fff));
        assert_eq!(extended(f64::NEG_INFINITY), (1 << 63, 0xffff));
        assert_eq!(extended(f64::NAN).1 & 0x7fff, 0x7fff);
        assert_eq!(extended(f64::NAN).0 >> 62, 0b11);
    }

    #[test]
    fn extended_of_subnormals_is_exact() {
        // 2^-1074: integer bit only, exponent -1074 + 16383.
        assert_eq!(extended(f64::from_bits(1)), (1 << 63, 16383 - 1074));
        // 3 * 2^-1060.
        let x = 3.0 * 2f64.powi(-1060);
        assert_eq!(extended(x), (0xc000_0000_0000_0000, 16383 - 1059));
    }

    #[test]
    fn from_extended_inverts_extended() {
        for x in [1.0, -2.5, 0.0, 1e-300, 3.0 * 2f64.powi(-1060), 0.1f32 as f64, -1e30, f64::INFINITY] {
            let (s, e) = extended(x);
            assert_eq!(from_extended(s, e).to_bits(), x.to_bits(), "{x}");
        }
        assert!(from_extended(1 << 63 | 1 << 62, 0x7fff).is_nan());
    }

    /// A host that answers float accessors from a table and logs every call.
    #[derive(Default)]
    pub struct Script {
        pub floats: Vec<(u32, u32, f32)>,
        pub log: RefCell<Vec<(u32, u32, Vec<u32>)>>,
    }

    impl Host for Script {
        fn call_at(&self, _sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
            self.log.borrow_mut().push((addr, ecx, args.to_vec()));
            let st0 = self.floats.iter().find(|f| f.0 == addr && f.1 == ecx).map_or(0.0, |f| f.2 as f64);
            Called { eax: 0, st0 }
        }
    }

    /// `bound.csv` (the hybrid gate's status per binding) lists every binding once.
    #[test]
    fn bound_csv_lists_every_binding() {
        let csv = include_str!("../bound.csv");
        let mut listed: Vec<(u32, &str)> = csv
            .lines()
            .skip(1)
            .map(|line| {
                let f: Vec<&str> = line.split(',').collect();
                (u32::from_str_radix(f[0].trim_start_matches("0x"), 16).unwrap(), f[1])
            })
            .collect();
        let mut bound: Vec<(u32, &str)> = bindings().map(|b| (b.addr, b.name)).collect();
        listed.sort();
        bound.sort();
        assert_eq!(listed, bound);
    }

    #[test]
    fn unbound_addresses_decline() {
        let host = Script::default();
        let mut ctx = Ctx { mem: Mem::new(), host: &host };
        assert_eq!(run(0x401000, &mut ctx, &Regs::default()), Err(Decline::NotBound));
    }
}
