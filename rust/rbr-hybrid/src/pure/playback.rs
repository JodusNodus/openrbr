//! A recording's playback on the pure host: each step's control emitted, then `StepForward`; and the read trace of a
//! snapshot run.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use rbr_physics::mem::{Backend, Mem};
use rbr_physics::physics_core::signal;
use rbr_physics::snapshot::Snapshot;
use rbr_recording::CONTROL_BYTES;

use super::heap::{Base, PureMemory, PureRunGuard};
use super::image::{app_state, CAR_PTR, LOAD_STATE};
use super::{Pure, EMIT, EMIT_OFFSET, SIGNALS, STEP_FORWARD};
use crate::car_body::UPDATE_SEED;
use crate::entities::MB560;
use crate::{Host, Regs, Return, SignalCallees};

/// One playback step from the call slot `slot` (`StepForward`'s return address slot, `regs` the other registers):
/// the control at `control` (`None` for the snapshot's own step, whose control was emitted before it) through `Emit`,
/// then `StepForward(1)`. `Emit` is the same template as `CMultiSignal1<const CSigLocalTime&>::Emit`, so it runs as
/// [`signal::emit`]; its connections are host calls from `slot`.
pub fn step(pure: &Pure, regs: Regs, slot: u32, control: Option<u32>, substeps: u32) -> Return {
    let _run = PureRunGuard::new(&pure.run_depth);
    let _ngp = crate::ngp::Scope::new(pure.ngp_base);
    // The load's settle steps run on the game's thread, at D3D's control word `0x7f`; under NGP the race's steps run on
    // NGP's thread, at `0x27f` (`notes/ngp.md`, "x87 precision").
    rbr_physics::fpu::set_pc53(app_state(&pure.mem) != LOAD_STATE);
    if let Some(control) = control {
        emit(pure, regs, slot, control);
    }
    pure.mem.clone().set_u32(slot + 4, substeps);
    crate::call_memo::new_step();
    pure.enter(STEP_FORWARD, Regs { esp: slot, ..regs })
}

/// [`step`]'s `Emit` of the payload at `control`.
pub(super) fn emit(pure: &Pure, regs: Regs, slot: u32, control: u32) {
    let mut mem = pure.mem.clone();
    let at = mem.u32(SIGNALS) + EMIT_OFFSET;
    let entry = Regs { ecx: at, esp: slot, ..regs };
    if crate::ngp::variant() == rbr_physics::Variant::Ngp778 {
        // NGP hands the payload to the player car's control input itself; the car has no connection on the emit,
        // and NGP's walk of the other listeners reaches no physics (`notes/ngp.md`, "Controls under NGP").
        let car = mem.u32(CAR_PTR);
        pure.with_frame(EMIT, entry, || pure.call(UPDATE_SEED, car, &[control]));
    } else {
        pure.with_frame(EMIT, entry, || signal::emit(&mut mem, &mut SignalCallees(pure), at, control));
    }
}

/// The return address of the driver's `StepForward` call (`replay.py`'s `SENTINEL`).
pub const SENTINEL: u32 = 0xfeed_0000;

const FRAME: u32 = rbr_physics::car_body::transform::FRAME;

/// A snapshot's playback, as `rust_step.py` drives it: every step enters `StepForward` from the snapshot's call slot
/// with its registers, and a step's control payload is written at the bottom of the snapshot's stack.
pub struct Playback {
    pub pure: Pure,
    regs: Regs,
    slot: u32,
    control_at: u32,
}

impl Playback {
    pub fn new(snap: &Snapshot, keep_going: bool) -> Self {
        let h = snap.metadata();
        let r = h.registers;
        let slot = r.esp - 4;
        let regs = Regs {
            ecx: r.ecx,
            edx: r.edx,
            esp: slot,
            eax: r.eax,
            ebx: r.ebx,
            esi: r.esi,
            edi: r.edi,
            ebp: r.ebp,
            stack: Default::default(),
        };
        Playback::from_image(snap.mem(), regs, slot, h.stack_lo, keep_going)
    }

    /// The playback of a memory image built without a snapshot (`rbr-standalone --boot`): `StepForward` entered from
    /// `slot` with `regs`, the control payload written at `control_at`.
    pub fn from_image(mem: Mem, regs: Regs, slot: u32, control_at: u32, keep_going: bool) -> Self {
        let ngp_base = crate::ngp::base_in(&mem);
        Playback { pure: Pure { ngp_base, ..Pure::new(mem, keep_going) }, regs, slot, control_at }
    }

    /// [`Playback::new`] on a [`ReadTrace`] of the snapshot's memory.
    pub fn traced(snap: &Snapshot, keep_going: bool) -> (Self, Arc<ReadTrace>) {
        let trace = Arc::new(ReadTrace::new(snap));
        let mut playback = Playback::new(snap, keep_going);
        playback.pure.mem = Mem::with_backend(Arc::new(PureMemory {
            base: Base::Backend(trace.clone()),
            heap: playback.pure.heap.clone(),
        }));
        playback.pure.trace = Some(trace.clone());
        (playback, trace)
    }

    /// [`step`] with the payload `control` (`None` for the snapshot's own step). A stop panics as [`Pure::enter`].
    pub fn step(&self, control: Option<&[u8; CONTROL_BYTES]>, substeps: u32) -> Return {
        let mut mem = self.pure.mem.clone();
        let control = control.map(|c| {
            mem.map(self.control_at, c);
            self.control_at
        });
        mem.set_u32(self.slot, SENTINEL);
        step(&self.pure, self.regs, self.slot, control, substeps)
    }

    /// `NotifyA`'s vector (`p[4..8]` of `CSigCarToCarInputFilter`): `MB560` of the car's vector at +0x220, as `CCar::Vf28x`
    /// (0x4ea020) walks it before the emit at +0x860. Called between steps, from the step's entry frame, with the walk's
    /// output in the scratch below it. Read-only on the car; the walk's node hooks store what a step stores.
    pub fn notify_a_vector(&self) -> [f32; 4] {
        let _run = PureRunGuard::new(&self.pure.run_depth);
        let mem = self.pure.mem.clone();
        let entry = Regs { esp: self.slot, ..self.regs };
        let scratch = (self.slot - 0x100) & !0xf;
        let (car, frame) = (mem.u32(CAR_PTR), mem.u32(FRAME));
        self.pure.with_frame(STEP_FORWARD, entry, || {
            self.pure.call(MB560, frame, &[car + 0x220, car, scratch]);
        });
        mem.f32s(scratch)
    }

    /// Only the `Emit` of [`Playback::step`]: the state a snapshot of the next step holds (`rbr-standalone
    /// --write-snapshot`).
    pub fn emit(&self, control: &[u8; CONTROL_BYTES]) {
        let _run = PureRunGuard::new(&self.pure.run_depth);
        self.pure.mem.clone().map(self.control_at, control);
        emit(&self.pure, self.regs, self.slot, self.control_at);
    }
}

/// A snapshot's memory that logs, while `on`, every byte read before anything wrote it (`rbr-standalone
/// --read-set`): the state the step takes from the image. Writes count whether `on`
/// or not. Each byte keeps its first reader, the innermost running binding (0: the driver).
pub struct ReadTrace {
    inner: Mem,
    /// The snapshot's regions and stack, `(lo, hi)`: a read outside them fails as the snapshot's own memory does.
    mapped: Vec<(u64, u64)>,
    pub on: AtomicBool,
    pub(super) reader: AtomicU32,
    log: Mutex<TraceLog>,
}

#[derive(Default)]
struct TraceLog {
    /// Per 4 KiB page, a bit per byte.
    written: HashMap<u32, Box<[u64; 64]>>,
    /// The first reads: address to (the image's byte, the reader).
    read: BTreeMap<u32, (u8, u32)>,
}

impl ReadTrace {
    fn new(snap: &Snapshot) -> Self {
        let h = snap.metadata();
        let mut mapped: Vec<(u64, u64)> =
            snap.regions().iter().map(|r| (r.base as u64, r.base as u64 + r.size as u64)).collect();
        mapped.push((h.stack_lo as u64, h.stack_lo as u64 + h.stack_len as u64));
        ReadTrace {
            inner: snap.mem(),
            mapped,
            on: AtomicBool::new(false),
            reader: AtomicU32::new(0),
            log: Mutex::default(),
        }
    }

    fn is_mapped(&self, at: u32, len: usize) -> bool {
        let (mut at, end) = (at as u64, at as u64 + len as u64);
        while at < end {
            match self.mapped.iter().find(|&&(lo, hi)| lo <= at && at < hi) {
                Some(&(_, hi)) => at = hi,
                None => return false,
            }
        }
        true
    }

    /// The first reads, as runs of consecutive addresses with one reader: (address, the image's bytes, reader).
    pub fn first_reads(&self) -> Vec<(u32, Vec<u8>, u32)> {
        let log = self.log.lock().unwrap();
        let mut runs: Vec<(u32, Vec<u8>, u32)> = vec![];
        for (&at, &(byte, reader)) in &log.read {
            match runs.last_mut() {
                Some((lo, bytes, r)) if *r == reader && *lo as u64 + bytes.len() as u64 == at as u64 => {
                    bytes.push(byte)
                }
                _ => runs.push((at, vec![byte], reader)),
            }
        }
        runs
    }
}

impl Backend for ReadTrace {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        if !self.is_mapped(at, out.len()) {
            return false;
        }
        out.copy_from_slice(&self.inner.read(at, out.len()));
        if self.on.load(Ordering::Relaxed) {
            let reader = self.reader.load(Ordering::Relaxed);
            let mut log = self.log.lock().unwrap();
            for (i, &byte) in out.iter().enumerate() {
                let a = at + i as u32;
                let written = log
                    .written
                    .get(&(a >> 12))
                    .is_some_and(|bits| bits[(a as usize & 0xfff) >> 6] >> (a & 63) & 1 != 0);
                if !written {
                    log.read.entry(a).or_insert((byte, reader));
                }
            }
        }
        true
    }

    fn write(&self, at: u32, bytes: &[u8]) {
        let mut inner = self.inner.clone();
        inner.map(at, bytes);
        let mut log = self.log.lock().unwrap();
        for i in 0..bytes.len() as u32 {
            let a = at + i;
            log.written.entry(a >> 12).or_insert_with(|| Box::new([0; 64]))[(a as usize & 0xfff) >> 6] |= 1 << (a & 63);
        }
    }
}
