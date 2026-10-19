//! The [`Host`] without the emulator: a host call runs the binding at its
//! address on the same [`Mem`], so the bound functions call each other directly. A call to an address with no binding,
//! or a binding that declines, has no original to fall back to: it stops the run ([`Stop`]), or with `keep_going` is
//! logged and answers zero, so one run lists every address the pure step still lacks (after the first stop the
//! zeros change its path: `harness/pure_check.py` lists them exactly, from the hybrid).
//!
//! Code outside the bindings whose effect is known runs as a stand-in ([`StandIn`]): the connections' `Invoke`
//! trampolines, functions that only return, the pure-only CRT allocator, the CRT sine and cosine and vector iterators,
//! `QueryPerformanceCounter`, and the listeners the car does not depend on (in the exe by address, in the DLLs by
//! their code).
//!
//! The stack is the snapshot's, in `Mem`: a call with `sp == 0` runs its callee from `(esp - 0x1000) & !0xf` below the
//! running binding's entry, as the bridge does, with the return address `CALL_RET` and the arguments written there.

use leaves::LEAVES;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rbr_physics::math::trig;
use rbr_physics::mem::{Backend, Mem};

use crate::{binding_slot, bound, Called, Ctx, Decline, Host, Regs, Return, STACK_ARGS};

mod compare;
mod heap;
mod image;
mod leaves;
mod playback;
mod stand_in;

use crate::engine_driveline::SIGNALS;
use crate::rigid_body::OPERATOR_NEW as CRT_NEW;
pub use compare::{
    frame_bytes, frame_words, is_ngp_padding, recorded_words, stale_mask, static_mask, word_name, Compare, Diff,
    FRAME_WORDS, NGP_PADDING_WHEEL,
};
use heap::{Base, PureHeap, PureMemory, PureRunGuard};
pub use heap::{ALLOCATOR_HEAP_BASE, ALLOCATOR_HEAP_END};
pub use image::{app_state, clock, max_time_step, set_app_state, time_keeper, CAR_BYTES, CAR_PTR};
pub use playback::{step, Playback, ReadTrace, SENTINEL};
pub use stand_in::{stand_in, uncovered, StandIn};

/// The return address a host call leaves in its callee's slot; never mapped (`CALL_RET` in `rust_step.py`).
pub const CALL_RET: u32 = 0xfeed_2000;

/// `CPhysics::StepForward`.
pub const STEP_FORWARD: u32 = 0x5273a0;
/// `CSignalManager::CMultiSignal1<const CSigCarControl&>::Emit`, on the signal at `[SIGNALS] + EMIT_OFFSET`: the
/// plugin's playback emits each step's recorded control with it before `StepForward(1)` (`rust_step.py`'s `advance`).
pub const EMIT: u32 = 0x48cd90;
const EMIT_OFFSET: u32 = 0xc90;

const CRT_COS: u32 = 0x53f3a0;
const CRT_SIN: u32 = 0x53f3b0;
const VECTOR_CTOR: u32 = 0x70c408;
const VECTOR_DTOR: u32 = 0x70c4da;
/// The executable's static `operator new` (`msvcprt:newop_s`); `operator new[]` tail-jumps here.
const CRT_NEW_ARRAY: u32 = 0x70bf84;
/// The matching scalar and array delete entries in the executable's CRT block.
pub(crate) const CRT_DELETE: u32 = 0x70c260;
const CRT_DELETE_ARRAY: u32 = 0x70c27e;
/// The import table slot of `QueryPerformanceCounter`.
pub(crate) const QUERY_PERFORMANCE_COUNTER: u32 = 0x729190;

/// `CSigOutsideAssistance`'s emit, whose code [`Pure::take_outside_assistance`] gives the host.
pub(crate) const OUTSIDE_ASSISTANCE_EMIT: u32 = 0x4e89b0;
/// `CSigRetireCar`'s emit, whose code [`Pure::take_retire`] gives the host.
const RETIRE_EMIT: u32 = 0x4e8770;
/// `CMultiSignal1<CSigCarStandStill>::Emit` and `CMultiSignal1<CSigCarProblem>::Emit`, whose payloads
/// [`Pure::take_hud_signals`] gives the host (the HUD's messages, `rbr_recording::dash`).
const STAND_STILL_EMIT: u32 = 0x4e88f0;
const CAR_PROBLEM_EMIT: u32 = 0x4e8a70;

/// `CMultiSignal1<CSigTireParticles>::Emit` (0x53d0d0) on the tyre-particle signal, `[0x893600] + 0x4518` (the wheel
/// step's emit, `rbr_physics::wheel::layout`), whose payloads [`Pure::take_tyre_particles`] gives the host. The same
/// code emits `CSigSolidGeneralState`, on another signal.
pub(crate) const TYRE_PARTICLES_EMIT: u32 = 0x53d0d0;
const TYRE_PARTICLES_SIGNAL: u32 = 0x4518;
/// The payload bytes kept (`rbr_recording::particles::PAYLOAD_BYTES`).
pub const TYRE_PARTICLES_BYTES: usize = 0xb0;
/// The other emits the particles take (with [`Pure::keep_tyre_particles`]), each on its signal `[0x893600] + offset`:
/// `(emit, signal offset, kind, payload bytes)`. `CMultiSignal1<CSigCarDamage>::Emit` with the handler's 0x40-byte
/// record, the damping event of `CSolid::Emit` (0x52be70, 0x40 bytes) and `CSigEngineFire`'s float. The kinds are
/// `rbr_recording::particles::Event`'s.
const PARTICLE_EMITS: [(u32, u32, u32, usize); 3] =
    [(0x4ef1d0, 0x9f20, 1, 0x40), (0x52be70, 0xadc8, 2, 0x40), (0x4ef290, 0xb840, 3, 4)];

/// The subsystems whose calls, made or taken, [`Pure::direct`] runs in place: the entity walk, the wheel and its tyre,
/// strut and VCU, collision and the rigid body. A call between two others (the physics core, the car body, the
/// engine, NTweak, NGP) still writes its stack words.
const IN_PLACE: [&str; 7] = ["entities", "wheel", "tyre", "spring_damper", "vcu", "collision", "rigid-body"];

/// `CScratch::Step` (0x5360c0) and NGP's wrapper of it (`0x1005b660`): after each, with [`Pure::keep_tyre_particles`],
/// the term its `FillOutputs` (0x532f10) folded into the tyre-noise block's word 1 is kept for the host
/// ([`Pure::take_tyre_terms`]), since the frame finish that clears the word does not run here.
const TYRE_STEPS: [u32; 2] = [0x5360c0, 0x1005_b660];

/// [`Pure::stack_words`] of a binding not run through the host yet, and of one that never runs as a plain call (the
/// tyre steps and the watched emits, whose runs the host keeps something of).
const UNSEEN: u8 = u8::MAX - 1;
const NOT_DIRECT: u8 = u8::MAX;

/// The bytes of a particle event's payload.
pub const PARTICLE_EVENT_BYTES: usize = 0x40;
/// `CMultiSignal1<CSigEngineFire>::Emit` (0x4ef290) on the engine-fire signal (`[0x893600] + 0xb840`), whose emits
/// [`Pure::take_engine_fires`] counts for the host (the sound's fire).
const ENGINE_FIRE_EMIT: u32 = 0x4ef290;
const ENGINE_FIRE_SIGNAL: u32 = 0xb840;
/// The entries of [`Pure::slot_of`]'s cache.
const SLOT_CACHE: usize = 1024;

/// The emits [`Pure::enter`] watches ([`Pure::watch`]).
const WATCHED: [u32; 9] = [
    OUTSIDE_ASSISTANCE_EMIT,
    TYRE_PARTICLES_EMIT,
    PARTICLE_EMITS[0].0,
    PARTICLE_EMITS[1].0,
    PARTICLE_EMITS[2].0,
    ENGINE_FIRE_EMIT,
    RETIRE_EMIT,
    STAND_STILL_EMIT,
    CAR_PROBLEM_EMIT,
];

/// The term `CScratch::FillOutputs` (0x532f10, `rbr_physics::tyre::fill`) folds into word 1 of the tyre-noise block
/// of `scratch`, from the step it ended (`dt` its divisor, `Step`'s second word): `(tread depth + carcass radius - c -
/// state +0x48) / dt`, which nothing in `Step` changes after it. `None` when the step took the no-contact path, which
/// clears the block's flag (`+0x47c`).
fn tyre_term(mem: &Mem, scratch: u32, dt: f32) -> Option<f32> {
    use rbr_physics::tyre::layout::{DEF, DEF_CARCASS_RADIUS, DEF_TREAD_DEPTH, STATE};
    if mem.u32(scratch + 0x47c) == 0 {
        return None;
    }
    let (def, state) = (mem.u32(scratch + DEF), mem.u32(scratch + STATE));
    let c = mem.f32(scratch + 0x3a8);
    Some((mem.f32(def + DEF_TREAD_DEPTH) + mem.f32(def + DEF_CARCASS_RADIUS) - c - mem.f32(state + 0x48)) / dt)
}

/// Why the pure run could not go on: the callee at `addr`, called from the binding at `caller` (0: the driver).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stop {
    Unbound { addr: u32, caller: u32 },
    Declined { addr: u32, caller: u32, why: &'static str },
}

/// The panic payload of a strict stop, so the driver tells it from a port's own failure.
#[derive(Debug)]
pub struct Stopped(pub Stop);

/// Runs bindings on `mem`, which must share its bytes with its clones (a backend's, as `Snapshot::mem` gives).
pub struct Pure {
    pub mem: Mem,
    heap: Arc<Mutex<PureHeap>>,
    /// The bindings running, which the allocator's unwritten-read log watches.
    run_depth: Arc<AtomicUsize>,
    /// Clones of `mem` for the runs' contexts and the host calls' argument writes, so a run takes one instead of
    /// cloning `mem` (and dropping the clone) each time ([`Pure::lend`]).
    spare: RefCell<Vec<Mem>>,
    /// The binding slots of the addresses entered last, by a hash of the address: `(address, slot)`, the slot
    /// `u32::MAX` for an address with no binding ([`Pure::slot_of`]).
    slots: Box<[Cell<(u32, u32)>; SLOT_CACHE]>,
    /// Log a stop and answer zero instead of stopping.
    pub keep_going: bool,
    /// [`stand_in`] of the addresses entered with no binding.
    stand_ins: RefCell<HashMap<u32, Option<StandIn>>>,
    /// The runs entered ([`Host::entered`]).
    entered: Cell<u64>,
    /// The running bindings (address and entry registers but the stack's), innermost last.
    frames: RefCell<Vec<(u32, Registers)>>,
    /// Every stop met (with `keep_going`), with its count.
    pub stops: RefCell<BTreeMap<Stop, u64>>,
    /// Bindings run at their own address, by their slot in [`bound`], and those run at another (NGP.dll's running
    /// addresses), by that address ([`Pure::ran`]).
    ran: Box<[Cell<u64>]>,
    ran_moved: RefCell<BTreeMap<u32, u64>>,
    /// When each stop was first met: the stops met before it, and the bindings run (all addresses) by then.
    pub first_met: RefCell<BTreeMap<Stop, (usize, u64)>>,
    /// `QueryPerformanceCounter`'s count.
    ticks: Cell<u64>,
    /// The read trace `mem` is backed by, told the running binding ([`Playback::traced`]).
    trace: Option<Arc<ReadTrace>>,
    /// The code of the last `CSigOutsideAssistance` emit (0x4e89b0), the word its argument points at: what the HUD's
    /// receiver (0x573900) keeps for the race clock. The host takes it after each step.
    outside_assistance: Cell<Option<u32>>,
    /// B1's Mem-direct dispatch: a call to a leaf ([`leaves::LEAVES`]) from or of the [`IN_PLACE`] subsystems runs as
    /// a plain call ([`Pure::call_leaf`]), without the call's words on the stack.
    pub direct: bool,
    /// The calls run in place so far ([`Pure::direct_runs`]).
    direct_runs: Cell<u64>,
    /// Whether each binding of [`bound`] is of an [`IN_PLACE`] subsystem, and whether the running one is.
    in_place_slots: Box<[bool]>,
    in_place_now: Cell<bool>,
    /// The stack words each binding pops, the most seen through the host ([`UNSEEN`] before its first run there,
    /// [`NOT_DIRECT`] for the bindings that always run through it).
    stack_words: Box<[Cell<u8>]>,
    /// Whether each binding of [`bound`] is a leaf ([`leaves::LEAVES`]).
    leaf_slots: Box<[bool]>,
    /// [`bound`], kept for the plain calls.
    bound: &'static [&'static crate::Binding],
    /// Whether the tyre-particle payloads are kept (the live viewer's particles; off in every check).
    pub keep_tyre_particles: bool,
    /// The tyre-particle payloads emitted since the host last took them, copied at the emit's entry.
    tyre_particles: RefCell<Vec<[u8; TYRE_PARTICLES_BYTES]>>,
    /// The tyre steps since the host last took them ([`TYRE_STEPS`]): the scratch and its `FillOutputs` term.
    tyre_terms: RefCell<Vec<(u32, Option<f32>)>>,
    /// The other particle emits since the host last took them ([`PARTICLE_EMITS`]): kind and payload (zero padded).
    particle_events: RefCell<Vec<(u32, [u8; PARTICLE_EVENT_BYTES])>>,
    /// The code of the last `CSigRetireCar` emit (0x4e8770), what the race flow's receiver (0x5662c0) takes. The host
    /// takes it after each step.
    retire: Cell<Option<u32>>,
    /// The last `CSigCarStandStill` payload (count, kind) and `CSigCarProblem` kind, copied at the emits' entry for
    /// the HUD. The host takes them after each step.
    stand_still: Cell<Option<[u32; 2]>>,
    car_problem: Cell<Option<u32>>,
    /// The `CSigEngineFire` emits since the host last took them.
    engine_fires: Cell<u32>,
    /// NGP.dll's base in this image, 0 without NGP: the runs see it through [`crate::ngp::Scope`].
    pub ngp_base: u32,
}

impl Pure {
    pub fn new(mem: Mem, keep_going: bool) -> Self {
        let run_depth = Arc::new(AtomicUsize::new(0));
        let heap = Arc::new(Mutex::new(PureHeap::new(run_depth.clone())));
        let pages = mem.pages().cloned();
        let backend = Arc::new(PureMemory { base: Base::new(mem), heap: heap.clone() });
        // Paged memory outside the allocator's region is read and written directly.
        let mem = match pages {
            Some(pages) => Mem::with_pages_around(pages, ALLOCATOR_HEAP_BASE..ALLOCATOR_HEAP_END, backend),
            None => Mem::with_backend(backend),
        };
        Pure {
            mem,
            heap,
            run_depth,
            spare: RefCell::default(),
            slots: Box::new([const { Cell::new((u32::MAX, u32::MAX)) }; SLOT_CACHE]),
            keep_going,
            stand_ins: RefCell::default(),
            entered: Cell::new(0),
            frames: RefCell::default(),
            stops: RefCell::default(),
            ran: bound().iter().map(|_| Cell::new(0)).collect(),
            ran_moved: RefCell::default(),
            first_met: RefCell::default(),
            ticks: Cell::new(1 << 40),
            trace: None,
            outside_assistance: Cell::new(None),
            direct: false,
            direct_runs: Cell::new(0),
            in_place_slots: bound().iter().map(|b| IN_PLACE.contains(&b.subsystem)).collect(),
            in_place_now: Cell::new(false),
            stack_words: bound()
                .iter()
                .map(|b| {
                    let host_only = TYRE_STEPS.contains(&b.addr) || WATCHED.contains(&b.addr);
                    Cell::new(if host_only { NOT_DIRECT } else { UNSEEN })
                })
                .collect(),
            leaf_slots: bound().iter().map(|b| LEAVES.contains(&b.addr)).collect(),
            bound: bound(),
            keep_tyre_particles: false,
            tyre_particles: RefCell::default(),
            tyre_terms: RefCell::default(),
            particle_events: RefCell::default(),
            retire: Cell::new(None),
            stand_still: Cell::new(None),
            car_problem: Cell::new(None),
            engine_fires: Cell::new(0),
            ngp_base: 0,
        }
    }

    /// Puts the allocator's blocks into the pages `mem` keeps the rest in (a boot image's), before anything runs: every
    /// access is then direct, and the bytes read before a caller wrote them are not logged
    /// ([`Pure::allocator_unwritten_reads`] stays empty). The blocks read and write as before: the region reads zero
    /// where nothing wrote, and a block is zero when allocated. For the live run; the checks keep the log.
    pub fn untrack_allocator(&mut self) {
        let Some(pages) = self.mem.pages().cloned() else {
            return;
        };
        let mut heap = self.heap.lock().unwrap_or_else(|p| p.into_inner());
        assert!(heap.live.is_empty() && self.runs() == 0, "the allocator moves before anything runs");
        pages.write(ALLOCATOR_HEAP_BASE, &vec![0; (ALLOCATOR_HEAP_END - ALLOCATOR_HEAP_BASE) as usize]);
        heap.pages = Some(pages.clone());
        drop(heap);
        self.mem = Mem::with_pages(pages);
    }

    /// Whether the allocator's unwritten reads are logged ([`Pure::untrack_allocator`]).
    pub fn tracks_allocator(&self) -> bool {
        self.heap.lock().unwrap_or_else(|p| p.into_inner()).pages.is_none()
    }

    /// The calls [`Pure::direct`] ran in place.
    pub fn direct_runs(&self) -> u64 {
        self.direct_runs.get()
    }

    /// The tyre-particle payloads emitted since the last call (with [`Pure::keep_tyre_particles`]), in emit order.
    pub fn take_tyre_particles(&self) -> Vec<[u8; TYRE_PARTICLES_BYTES]> {
        self.tyre_particles.take()
    }

    /// The tyre steps since the last call (with [`Pure::keep_tyre_particles`]), in order: each one's scratch and the
    /// term its `FillOutputs` folded into the tyre-noise block's word 1, `None` for the no-contact path
    /// (`rbr_recording::sound::TyreMax`).
    pub fn take_tyre_terms(&self) -> Vec<(u32, Option<f32>)> {
        self.tyre_terms.take()
    }

    /// The damage, damping and engine-fire emits since the last call (with [`Pure::keep_tyre_particles`]), in emit
    /// order: each one's kind ([`PARTICLE_EMITS`]) and payload.
    pub fn take_particle_events(&self) -> Vec<(u32, [u8; PARTICLE_EVENT_BYTES])> {
        self.particle_events.take()
    }

    /// The `CSigEngineFire` emits since the last call.
    pub fn take_engine_fires(&self) -> u32 {
        self.engine_fires.take()
    }

    /// The code of the `CSigOutsideAssistance` emitted since the last call, if any.
    pub fn take_outside_assistance(&self) -> Option<u32> {
        self.outside_assistance.take()
    }

    /// The last `CSigCarStandStill` payload (count, kind) and `CSigCarProblem` kind emitted since the last call.
    pub fn take_hud_signals(&self) -> (Option<[u32; 2]>, Option<u32>) {
        (self.stand_still.take(), self.car_problem.take())
    }

    /// The code of the `CSigRetireCar` emitted since the last call, if any.
    pub fn take_retire(&self) -> Option<u32> {
        self.retire.take()
    }

    /// Runs the binding at `addr` from the entry registers `regs` (`regs.esp` its return address slot; `regs.stack`
    /// is read from there). A stop panics with [`Stopped`] unless `keep_going`.
    pub fn enter(&self, addr: u32, regs: Regs) -> Return {
        self.enter_with(addr, regs, self.lend())
    }

    /// [`Pure::enter`] with a clone of `mem` ([`Pure::lend`]) for the run's context.
    #[inline(always)]
    fn enter_with(&self, addr: u32, regs: Regs, mem: Mem) -> Return {
        // The outermost run opens the NGP scope ([`step`] opens its own); the runs inside it see the same base.
        let _ngp = (self.run_depth.load(Ordering::Relaxed) == 0).then(|| crate::ngp::Scope::new(self.ngp_base));
        let _run = PureRunGuard::new(&self.run_depth);
        self.entered.set(self.entered.get() + 1);
        let mut regs = regs;
        regs.stack = self.mem.u32s(regs.esp + 4);
        if WATCHED.contains(&addr) {
            self.watch(addr, &regs);
        }
        match self.slot_of(crate::ngp::file_at(self.ngp_base, addr)) {
            Some(slot) => self.run_bound(slot, addr, regs, mem),
            None => {
                self.give_back(mem);
                self.enter_unbound(addr, regs)
            }
        }
    }

    /// [`binding_slot`] of `addr`, from the cache when the address was entered before.
    #[inline]
    fn slot_of(&self, addr: u32) -> Option<usize> {
        let cell = &self.slots[((addr >> 4 ^ addr >> 13) as usize) % SLOT_CACHE];
        let (at, slot) = cell.get();
        if at != addr {
            let slot = binding_slot(addr).map_or(u32::MAX, |s| s as u32);
            cell.set((addr, slot));
            return (slot != u32::MAX).then_some(slot as usize);
        }
        (slot != u32::MAX).then_some(slot as usize)
    }

    /// The emits the host keeps a payload or a count of ([`WATCHED`]), at their entry.
    #[inline(never)]
    fn watch(&self, addr: u32, regs: &Regs) {
        if addr == OUTSIDE_ASSISTANCE_EMIT {
            self.outside_assistance.set(Some(self.mem.u32(regs.stack[0])));
        }
        if self.keep_tyre_particles
            && addr == TYRE_PARTICLES_EMIT
            && regs.ecx == self.mem.u32(SIGNALS).wrapping_add(TYRE_PARTICLES_SIGNAL)
        {
            if let Some(bytes) = self.mem.try_read(regs.stack[0], TYRE_PARTICLES_BYTES) {
                self.tyre_particles.borrow_mut().push(bytes.try_into().expect("the length read"));
            }
        }
        if self.keep_tyre_particles {
            for &(emit, signal, kind, bytes) in &PARTICLE_EMITS {
                if addr == emit && regs.ecx == self.mem.u32(SIGNALS).wrapping_add(signal) {
                    if let Some(read) = self.mem.try_read(regs.stack[0], bytes) {
                        let mut payload = [0; PARTICLE_EVENT_BYTES];
                        payload[..bytes].copy_from_slice(&read);
                        self.particle_events.borrow_mut().push((kind, payload));
                    }
                }
            }
        }
        if addr == ENGINE_FIRE_EMIT && regs.ecx == self.mem.u32(SIGNALS).wrapping_add(ENGINE_FIRE_SIGNAL) {
            self.engine_fires.set(self.engine_fires.get() + 1);
        }
        if addr == RETIRE_EMIT {
            self.retire.set(Some(self.mem.u32(regs.stack[0])));
        }
        if addr == STAND_STILL_EMIT {
            self.stand_still.set(Some([self.mem.u32(regs.stack[0]), self.mem.u32(regs.stack[0] + 4)]));
        }
        if addr == CAR_PROBLEM_EMIT {
            self.car_problem.set(Some(self.mem.u32(regs.stack[0])));
        }
    }

    /// The running binding's address (0: the driver), the caller a stop names.
    fn caller(&self) -> u32 {
        self.frames.borrow().last().map_or(0, |f| f.0)
    }

    /// [`Pure::enter`] of the binding in `slot` of [`bound`].
    #[inline(always)]
    fn run_bound(&self, slot: usize, addr: u32, regs: Regs, mem: Mem) -> Return {
        let b = bound()[slot];
        let outer = self.direct.then(|| self.in_place_now.replace(self.in_place_slots[slot]));
        let r = self.with_frame(addr, regs, || {
            let mut ctx = Ctx { mem, host: self.host() };
            let r = (b.run)(&mut ctx, &regs);
            self.give_back(ctx.mem);
            r
        });
        if let Some(outer) = outer {
            self.in_place_now.set(outer);
        }
        match r {
            Ok(r) => {
                self.ran_bound(slot, addr, &regs);
                if self.direct {
                    self.learn_pop(slot, r.pop);
                }
                r
            }
            Err(Decline::Unsupported(why)) => self.stop(Stop::Declined { addr, caller: self.caller(), why }),
            Err(Decline::NotBound) => self.stop(Stop::Unbound { addr, caller: self.caller() }),
        }
    }

    /// The bookkeeping after the binding in `slot` ran at `addr`: its count, and the tyre step's term.
    #[inline(always)]
    fn ran_bound(&self, slot: usize, addr: u32, regs: &Regs) {
        let b = bound()[slot];
        if self.keep_tyre_particles && TYRE_STEPS.contains(&b.addr) {
            let term = tyre_term(&self.mem, regs.ecx, f32::from_bits(regs.stack[1]));
            self.tyre_terms.borrow_mut().push((regs.ecx, term));
        }
        if b.addr == addr {
            let ran = &self.ran[slot];
            ran.set(ran.get() + 1);
        } else {
            *self.ran_moved.borrow_mut().entry(addr).or_default() += 1;
        }
    }

    /// A binding's stack words, learned from what it pops at its runs through the host ([`Pure::stack_words`]).
    #[cold]
    fn learn_pop(&self, slot: usize, pop: u32) {
        let words = &self.stack_words[slot];
        if words.get() != NOT_DIRECT {
            // A cdecl binding pops nothing, so the words it reads are learned as none: only a call that passes none
            // runs it as a plain call.
            let popped = (pop / 4).min(STACK_ARGS as u32) as u8;
            if words.get() == UNSEEN || popped > words.get() {
                words.set(popped);
            }
        }
    }

    /// The registers of a call at `sp` (0: below the running binding's frame) on `ecx`: the other registers are the
    /// caller's entry values, as the emulator's nested run keeps them.
    #[inline(always)]
    fn call_regs(&self, sp: u32, ecx: u32) -> Regs {
        let outer = self.frames.borrow().last().map(|f| f.1).unwrap_or_default().regs();
        let esp = if sp == 0 { (outer.esp - 0x1000) & !0xf } else { sp };
        Regs { ecx, esp, ..outer }
    }

    /// The host the bindings get: [`Direct`] under the direct dispatch, else `self`.
    #[inline(always)]
    fn host(&self) -> &dyn Host {
        match self.direct {
            true => Direct::of(self),
            false => self,
        }
    }

    /// [`Host::call_at`] through the host under the direct dispatch, out of line (the fact-check's `call_at` has its
    /// own copy inline): the call's words written, and its callee entered as [`Pure::enter`] does.
    #[inline(never)]
    fn call_host(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
        let regs = self.call_regs(sp, ecx);
        let mut mem = self.lend();
        write_call(&mut mem, regs.esp, args);
        let r = self.enter_with(addr, regs, mem);
        Called { eax: r.eax, st0: r.st0.unwrap_or(f64::NAN) }
    }

    /// The direct dispatch's plain call of `addr` at `sp` (None, nothing changed: the call goes through the host). The
    /// binding gets `args` as its stack words, and nothing is read back from the stack or counted but
    /// [`Host::entered`]. A leaf ([`leaves::LEAVES`]) of or from the [`IN_PLACE`] subsystems runs so when `args` are the
    /// words it pops ([`Pure::stack_words`]), with nothing written: a caller that passes more may read them after it, or
    /// call on with them as its next callee's (the pose step's parent-frame call leaves the arguments of 0x51b4c0,
    /// `rigid_body/pose.rs`). Another binding (B1b) runs in its own frame (the callees it leaves the placement of run
    /// below it, as through the host) as a run of its own subsystem ([`Pure::in_place_now`]): with `write`, after the
    /// call's words are written as the host writes them, when `args` hold at least the words it pops (a cdecl callee
    /// pops none); without, when they are exactly those words. The watched emits and the tyre steps never run so
    /// ([`NOT_DIRECT`]).
    #[inline(always)]
    fn call_plain(&self, sp: u32, addr: u32, ecx: u32, args: &[u32], write: bool) -> Option<Called> {
        match self.slot_of(crate::ngp::file_at(self.ngp_base, addr)) {
            Some(slot) => self.run_plain(slot, addr, self.call_regs(sp, ecx), args, write),
            None => self.invoke_plain(addr, self.call_regs(sp, ecx), args, write),
        }
    }

    /// [`Pure::call_plain`] of the binding in `slot` at `addr`, entered with `regs` but the stack words.
    #[inline(always)]
    fn run_plain(&self, slot: usize, addr: u32, mut regs: Regs, args: &[u32], write: bool) -> Option<Called> {
        let words = usize::from(self.stack_words[slot].get());
        if words != args.len() && !(write && words <= args.len()) {
            return None;
        }
        regs.stack[..args.len()].copy_from_slice(args);
        let run = self.bound[slot].run;
        let mut ctx = Ctx { mem: self.lend(), host: Direct::of(self) };
        let leaf = self.leaf_slots[slot] && (self.in_place_now.get() || self.in_place_slots[slot]);
        let r = if leaf && words == args.len() {
            run(&mut ctx, &regs)
        } else {
            if write {
                write_call(&mut ctx.mem, regs.esp, args);
            }
            self.entered.set(self.entered.get() + 1);
            let outer = self.in_place_now.replace(self.in_place_slots[slot]);
            let r = self.with_frame(addr, regs, || run(&mut ctx, &regs));
            self.in_place_now.set(outer);
            r
        };
        self.give_back(ctx.mem);
        let r = r.ok()?;
        if leaf && words == args.len() {
            self.entered.set(self.entered.get() + 1);
        }
        self.direct_runs.set(self.direct_runs.get() + 1);
        Some(Called { eax: r.eax, st0: r.st0.unwrap_or(f64::NAN) })
    }

    /// [`Pure::call_plain`] of an address with no binding: an `Invoke` trampoline's method runs as a plain call, entered
    /// as the stand-in enters it ([`Pure::stand_in`]); the other stand-ins go through the host.
    #[inline(never)]
    fn invoke_plain(&self, addr: u32, mut regs: Regs, args: &[u32], write: bool) -> Option<Called> {
        let known = *self.stand_ins.borrow_mut().entry(addr).or_insert_with(|| stand_in(&self.mem, addr));
        let Some(StandIn::Invoke { method, owner_offset }) = known else {
            return None;
        };
        let conn = regs.ecx;
        let owner_offset = owner_offset.map_or(0, |at| self.mem.u32(conn + at));
        let target = self.mem.u32(conn + method);
        let slot = self.slot_of(crate::ngp::file_at(self.ngp_base, target))?;
        regs.ecx = self.mem.u32(conn + 0x10).wrapping_add(owner_offset);
        regs.eax = conn;
        self.entered.set(self.entered.get() + 1);
        self.run_plain(slot, target, regs, args, write)
    }

    /// [`Pure::enter`] of an address with no binding: its original's binding, or a stand-in.
    #[inline(never)]
    fn enter_unbound(&self, addr: u32, regs: Regs) -> Return {
        if let Some(exe) = crate::ngp::original_for(addr) {
            return self.enter(exe, regs);
        }
        match self.stand_in(addr, &regs) {
            Some(r) => r,
            None => self.stop(Stop::Unbound { addr, caller: self.caller() }),
        }
    }

    /// A clone of `mem`, from the spare ones when one still shares its bytes.
    #[inline(always)]
    fn lend(&self) -> Mem {
        match self.spare.borrow_mut().pop() {
            Some(mem) if mem.shares(&self.mem) => mem,
            _ => self.mem.clone(),
        }
    }

    /// Keeps a clone of [`Pure::lend`] for the next run.
    #[inline(always)]
    fn give_back(&self, mem: Mem) {
        self.spare.borrow_mut().push(mem);
    }

    /// Runs the stand-in at `addr` (see [`stand_in`]), or `None` without one.
    fn stand_in(&self, addr: u32, regs: &Regs) -> Option<Return> {
        let angle = f32::from_bits(regs.stack[0]);
        // The code and the import slot a stand-in is known by do not change while the image runs.
        let known = *self.stand_ins.borrow_mut().entry(addr).or_insert_with(|| stand_in(&self.mem, addr));
        let r = match known? {
            StandIn::Invoke { method, owner_offset } => {
                let conn = regs.ecx;
                let owner_offset = owner_offset.map_or(0, |at| self.mem.u32(conn + at));
                let owner = self.mem.u32(conn + 0x10).wrapping_add(owner_offset);
                let r = Regs { ecx: owner, eax: conn, ..*regs };
                self.enter(self.mem.u32(conn + method), r)
            }
            StandIn::Ret(pop) | StandIn::Listener(pop) => Return::void(regs.eax, pop),
            StandIn::Sin => Return::float_with_eax(regs.eax, trig::x87_sin(angle), 0),
            StandIn::Cos => Return::float_with_eax(regs.eax, trig::x87_cos(angle), 0),
            StandIn::VectorCtor => self.with_frame(addr, *regs, || self.vector_ctor(regs.stack)),
            StandIn::VectorDtor => self.with_frame(addr, *regs, || self.vector_dtor(regs.stack)),
            StandIn::Counter => self.counter(regs.stack[0]),
            StandIn::Allocate => {
                let ptr = self.heap.lock().unwrap_or_else(|p| p.into_inner()).allocate(regs.stack[0]);
                Return::void(ptr, 0)
            }
            StandIn::Free => {
                self.heap.lock().unwrap_or_else(|p| p.into_inner()).free(regs.stack[0]);
                Return::void(regs.eax, 0)
            }
        };
        Some(r)
    }

    /// `eh vector constructor iterator(ptr, size, count, ctor, dtor)`, stdcall: `ctor` (thiscall) on each element in
    /// order; eax is the count (0 for a count below 1).
    fn vector_ctor(&self, [ptr, size, count, ctor, ..]: [u32; STACK_ARGS]) -> Return {
        let n = (count as i32).max(0) as u32;
        for i in 0..n {
            self.host().call_direct(0, ctor, ptr.wrapping_add(i.wrapping_mul(size)), &[]);
        }
        Return::void(n, 0x14)
    }

    /// `eh vector destructor iterator(ptr, size, count, dtor)`, stdcall: `dtor` (thiscall) on each element from the
    /// last; eax is the last call's (`size * count` without one).
    fn vector_dtor(&self, [ptr, size, count, dtor, ..]: [u32; STACK_ARGS]) -> Return {
        let mut eax = size.wrapping_mul(count);
        let mut at = ptr.wrapping_add(eax);
        for _ in 0..(count as i32).max(0) {
            at = at.wrapping_sub(size);
            eax = self.host().call_direct(0, dtor, at, &[]).eax;
        }
        Return::void(eax, 0x10)
    }

    /// `QueryPerformanceCounter(out)`, stdcall: as `replay.py`'s stub, a counter that starts at 2^40 and goes up by
    /// 1000 a call (only the profiler reads it).
    fn counter(&self, out: u32) -> Return {
        let ticks = self.ticks.get() + 1000;
        self.ticks.set(ticks);
        let mut mem = self.mem.clone();
        mem.map(out, &ticks.to_le_bytes());
        Return::void(1, 4)
    }

    /// The bindings run so far (all addresses).
    pub fn runs(&self) -> u64 {
        self.ran.iter().map(Cell::get).sum::<u64>() + self.ran_moved.borrow().values().sum::<u64>()
    }

    /// Bindings run, by address.
    pub fn ran(&self) -> BTreeMap<u32, u64> {
        let mut out: BTreeMap<u32, u64> =
            self.ran.iter().zip(bound()).filter(|(n, _)| n.get() != 0).map(|(n, b)| (b.addr, n.get())).collect();
        for (&addr, &n) in self.ran_moved.borrow().iter() {
            *out.entry(addr).or_default() += n;
        }
        out
    }

    /// Addresses of stand-in block bytes read by the pure code before a caller wrote them.
    pub fn allocator_unwritten_reads(&self) -> Vec<u32> {
        self.heap.lock().unwrap_or_else(|p| p.into_inner()).unwritten_reads.iter().copied().collect()
    }

    fn stop(&self, stop: Stop) -> Return {
        let runs = self.runs();
        let mut first_met = self.first_met.borrow_mut();
        let order = first_met.len();
        first_met.entry(stop.clone()).or_insert((order, runs));
        drop(first_met);
        if !self.keep_going {
            std::panic::panic_any(Stopped(stop));
        }
        *self.stops.borrow_mut().entry(stop).or_default() += 1;
        Return::void(0, 0)
    }

    /// The running bindings and stand-ins, outermost first (after a port's panic, where it failed).
    pub fn chain(&self) -> Vec<u32> {
        self.frames.borrow().iter().map(|f| f.0).collect()
    }

    /// Runs `f` as the code at `addr` entered with `regs` would: host calls in it are placed below `regs.esp` and
    /// name `addr` as their caller (the driver's own emits).
    #[inline(always)]
    pub fn with_frame<T>(&self, addr: u32, regs: Regs, f: impl FnOnce() -> T) -> T {
        self.frames.borrow_mut().push((addr, Registers::of(&regs)));
        self.tell_trace();
        let out = f();
        self.frames.borrow_mut().pop();
        self.tell_trace();
        out
    }

    #[inline(always)]
    fn tell_trace(&self) {
        if let Some(trace) = &self.trace {
            let reader = self.frames.borrow().last().map_or(0, |f| f.0);
            trace.reader.store(reader, Ordering::Relaxed);
        }
    }
}

/// A frame's entry registers but the stack's: what a call from it keeps ([`Host::call_at`]).
#[derive(Clone, Copy, Default)]
struct Registers {
    ecx: u32,
    edx: u32,
    esp: u32,
    eax: u32,
    ebx: u32,
    esi: u32,
    edi: u32,
    ebp: u32,
}

impl Registers {
    fn of(r: &Regs) -> Self {
        let Regs { ecx, edx, esp, eax, ebx, esi, edi, ebp, .. } = *r;
        Registers { ecx, edx, esp, eax, ebx, esi, edi, ebp }
    }

    fn regs(self) -> Regs {
        let Registers { ecx, edx, esp, eax, ebx, esi, edi, ebp } = self;
        Regs { ecx, edx, esp, eax, ebx, esi, edi, ebp, stack: [0; STACK_ARGS] }
    }
}

/// The words a call at `esp` writes before its callee runs: the return address and `args`.
#[inline(always)]
fn write_call(mem: &mut Mem, esp: u32, args: &[u32]) {
    for (k, w) in std::iter::once(CALL_RET).chain(args.iter().copied()).enumerate() {
        mem.set_u32(esp + 4 * k as u32, w);
    }
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Stop::Unbound { addr, caller } => {
                write!(f, "unbound {addr:#x} (called from {caller:#x})")
            }
            Stop::Declined { addr, caller, why } => {
                write!(f, "{addr:#x} declined (called from {caller:#x}): {why}")
            }
        }
    }
}

impl Host for Pure {
    fn call_slot_word(&self) -> Option<u32> {
        Some(CALL_RET)
    }

    fn entered(&self) -> u64 {
        self.entered.get()
    }

    fn ran_direct(&self) {
        self.entered.set(self.entered.get() + 1);
    }

    fn call_at(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
        assert!(args.len() <= STACK_ARGS, "host call {addr:#x}: too many arguments");
        let regs = self.call_regs(sp, ecx);
        let mut mem = self.lend();
        write_call(&mut mem, regs.esp, args);
        let r = self.enter_with(addr, regs, mem);
        Called { eax: r.eax, st0: r.st0.unwrap_or(f64::NAN) }
    }
}

/// [`Pure`] as its bindings' host under the direct dispatch ([`Pure::direct`], [`Pure::host`]): a call runs as a plain
/// call when it can ([`Pure::call_plain`]), and a skipped call writes nothing. `Pure`'s own [`Host`] is the
/// fact-check's, with no branch on the dispatch on its path.
#[repr(transparent)]
struct Direct(Pure);

impl Direct {
    #[inline(always)]
    fn of(pure: &Pure) -> &Direct {
        // SAFETY: `Direct` is a `repr(transparent)` wrapper of `Pure`, so the two references have the same layout.
        unsafe { &*(pure as *const Pure as *const Direct) }
    }
}

impl Host for Direct {
    fn call_slot_word(&self) -> Option<u32> {
        Some(CALL_RET)
    }

    fn skip_word(&self) -> Option<(u32, bool)> {
        Some((CALL_RET, false))
    }

    fn entered(&self) -> u64 {
        self.0.entered.get()
    }

    fn ran_direct(&self) {
        self.0.entered.set(self.0.entered.get() + 1);
    }

    fn call_at(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
        assert!(args.len() <= STACK_ARGS, "host call {addr:#x}: too many arguments");
        match self.0.call_plain(sp, addr, ecx, args, true) {
            Some(called) => called,
            None => self.0.call_host(sp, addr, ecx, args),
        }
    }

    fn call_direct(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> Called {
        match self.0.call_plain(sp, addr, ecx, args, false) {
            Some(called) => called,
            None => self.call_at(sp, addr, ecx, args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::stand_in::{code_stand_in, dll_listener, CODE_STAND_INS, DLL_LISTENERS};
    use super::*;
    use rbr_physics::mem::Backend;
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::{Arc, Mutex};

    #[test]
    fn leaves_are_bound() {
        for addr in LEAVES {
            assert!(binding_slot(addr).is_some(), "{addr:#x}");
        }
    }

    #[test]
    fn code_stand_ins_match_the_exe() {
        let Some(data) = rbr_formats::data::Data::from_env().filter(|d| d.patch().is_some()) else {
            eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
            return;
        };
        let path = data.resolve("RichardBurnsRally_SSE.exe").expect("the exe");
        let exe = rbr_formats::pe::parse(&std::fs::read(path).unwrap()).unwrap();
        for (at, stand_in) in CODE_STAND_INS {
            let code = exe.read(at, 19).expect("code in the exe");
            assert_eq!(code_stand_in(code), Some(stand_in), "{at:#x}");
        }
    }

    /// Memory shared by its clones, as a snapshot's is.
    #[derive(Default)]
    struct Shared(Mutex<BTreeMap<u32, u8>>);

    impl Backend for Shared {
        fn read(&self, at: u32, out: &mut [u8]) -> bool {
            let bytes = self.0.lock().unwrap();
            for (i, b) in out.iter_mut().enumerate() {
                match bytes.get(&(at + i as u32)) {
                    Some(&v) => *b = v,
                    None => return false,
                }
            }
            true
        }

        fn write(&self, at: u32, src: &[u8]) {
            let mut bytes = self.0.lock().unwrap();
            for (i, &b) in src.iter().enumerate() {
                bytes.insert(at + i as u32, b);
            }
        }
    }

    const TRAMPOLINE: u32 = 0x401000;
    const UNBOUND: u32 = 0x402000;
    const RET_4: u32 = 0x403000;
    const CONN: u32 = 0x20000;
    const OWNER: u32 = 0x30000;

    /// A stack at 0x10000-0x12000, the import slot, a connection, and code: an `Invoke` trampoline, a function with
    /// no binding and one that is `ret 4`.
    fn host(keep_going: bool) -> Pure {
        let mut mem = Mem::with_backend(Arc::new(Shared::default()));
        mem.map(0x10000, &[0; 0x2000]);
        mem.map(QUERY_PERFORMANCE_COUNTER, &[0; 4]);
        for at in [UNBOUND, CRT_SIN] {
            mem.map(at, &[0x90; 64]);
        }
        let code = |bytes: &[u8]| [bytes, &[0xcc; 64][bytes.len()..]].concat();
        mem.map(TRAMPOLINE, &code(&[0x8b, 0xc1, 0x8b, 0x48, 0x10, 0xff, 0x60, 0x14]));
        mem.map(RET_4, &code(&[0xc2, 0x04, 0x00]));
        mem.map(CONN, &[0; 0x18]);
        mem.set_u32(CONN + 0x10, OWNER);
        Pure::new(mem, keep_going)
    }

    /// Calls `addr` from a frame of `StepForward` with its entry at 0x11ff0.
    fn call(pure: &Pure, addr: u32, ecx: u32, args: &[u32]) -> Called {
        let regs = Regs { eax: 0x77, esp: 0x11ff0, ..Regs::default() };
        pure.with_frame(STEP_FORWARD, regs, || pure.call(addr, ecx, args))
    }

    fn stop_of(pure: &Pure, addr: u32, ecx: u32) -> Stop {
        let e = panic::catch_unwind(AssertUnwindSafe(|| call(pure, addr, ecx, &[1]))).unwrap_err();
        e.downcast_ref::<Stopped>().expect("a stop").0.clone()
    }

    #[test]
    fn a_call_with_no_binding_stops_with_its_address_and_caller() {
        let pure = host(false);
        let unbound = Stop::Unbound { addr: UNBOUND, caller: STEP_FORWARD };
        assert_eq!(stop_of(&pure, UNBOUND, 0), unbound);
        // Through a trampoline, the stop is the method it leads to.
        pure.mem.clone().set_u32(CONN + 0x14, UNBOUND);
        assert_eq!(stop_of(&pure, TRAMPOLINE, CONN), unbound);
        assert_eq!(uncovered(&pure.mem, TRAMPOLINE, CONN, &[1]), Some(UNBOUND));
    }

    #[test]
    fn keep_going_counts_the_stop_and_answers_zero() {
        let pure = host(true);
        assert_eq!(call(&pure, UNBOUND, 0, &[]).eax, 0);
        assert_eq!(call(&pure, UNBOUND, 0, &[]).eax, 0);
        let unbound = Stop::Unbound { addr: UNBOUND, caller: STEP_FORWARD };
        assert_eq!(pure.stops.take(), BTreeMap::from([(unbound, 2)]));
    }

    #[test]
    fn a_call_runs_below_its_caller_with_the_return_slot_and_arguments() {
        let pure = host(false);
        let mut mem = pure.mem.clone();
        mem.set_u32(CONN + 0x14, RET_4);
        // The trampoline leaves eax = the connection; `ret 4` keeps it.
        assert_eq!(call(&pure, TRAMPOLINE, CONN, &[0x1234]).eax, CONN);
        let sp = (0x11ff0 - 0x1000) & !0xf;
        assert_eq!((mem.u32(sp), mem.u32(sp + 4)), (CALL_RET, 0x1234));
        assert_eq!(uncovered(&pure.mem, TRAMPOLINE, CONN, &[0x1234]), None);
    }

    #[test]
    fn a_dll_listener_is_known_at_any_load_address() {
        let (bytes, Some(at), _) = DLL_LISTENERS[1] else { panic!("the CSigCarControl listener relocates") };
        for entry in [0x6e70ac0u32, 0x7470ac0] {
            let mut code = bytes.to_vec();
            let word = u32::from_le_bytes(code[at..at + 4].try_into().unwrap());
            code[at..at + 4].copy_from_slice(&word.wrapping_add(entry).to_le_bytes());
            assert_eq!(dll_listener(&code, entry), Some(4));
            assert_eq!(dll_listener(&code, entry + 0x10000), None);
        }
    }

    #[test]
    fn the_crt_sine_is_the_x87_model_and_keeps_eax() {
        let pure = host(false);
        let angle = 0.7f32;
        let r = call(&pure, CRT_SIN, 0, &[angle.to_bits()]);
        assert_eq!((r.eax, r.st0.to_bits()), (0x77, trig::x87_sin(angle).to_bits()));
    }

    #[test]
    fn the_pure_crt_allocator_aligns_reuses_by_size_and_zeroes_new_blocks() {
        let mut mem = Mem::new();
        mem.map(0x10000, &[0; 0x2000]);
        for at in [CRT_NEW, CRT_NEW_ARRAY, CRT_DELETE, CRT_DELETE_ARRAY] {
            mem.map(at, &[0x90; 64]);
        }
        let pure = Pure::new(mem, false);
        let a = call(&pure, CRT_NEW_ARRAY, 0, &[0x21]).eax;
        let b = call(&pure, CRT_NEW, 0, &[0x30]).eax;
        assert_eq!(a % 16, 0);
        assert_eq!(b % 16, 0);
        assert_ne!(a, b);
        pure.mem.clone().map(a, &[0xaa; 0x21]);
        call(&pure, CRT_DELETE_ARRAY, 0, &[a]);
        call(&pure, CRT_DELETE, 0, &[b]);
        assert_eq!(call(&pure, CRT_NEW, 0, &[0x30]).eax, b);
        assert_eq!(call(&pure, CRT_NEW_ARRAY, 0, &[0x21]).eax, a);
        assert_eq!(pure.mem.read(a, 0x21), vec![0; 0x21]);

        {
            let _run = PureRunGuard::new(&pure.run_depth);
            assert_eq!(pure.mem.read(a, 4), [0, 0, 0, 0]);
        }
        assert_eq!(pure.allocator_unwritten_reads(), (a..a + 4).collect::<Vec<_>>());
    }
}
