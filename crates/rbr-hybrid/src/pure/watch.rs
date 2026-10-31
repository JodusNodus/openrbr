//! The watched emits' capture: at each watched emit's entry ([`WATCHED`]) the payload or count the host takes after
//! the step is kept. [`Watch`] holds the kept payloads; the tyre-particle ones only with `keep` (the live viewer's
//! particles; off in every check). The damper-signal payloads arrive through the host's `emit_damper`, not an emit.

use std::cell::{Cell, RefCell};

use rbr_physics::mem::Mem;
use rbr_physics::spring_damper::DamperSignal;

use crate::engine_driveline::SIGNALS;
use crate::Regs;

/// `CSigOutsideAssistance`'s emit, whose code [`Watch::take_outside_assistance`] gives the host.
pub(crate) const OUTSIDE_ASSISTANCE_EMIT: u32 = 0x4e89b0;
/// `CSigRetireCar`'s emit, whose code [`Watch::take_retire`] gives the host.
const RETIRE_EMIT: u32 = 0x4e8770;
/// `CMultiSignal1<CSigCarStandStill>::Emit` and `CMultiSignal1<CSigCarProblem>::Emit`, whose payloads
/// [`Watch::take_hud_signals`] gives the host (the HUD's messages, `rbr_recording::dash`).
const STAND_STILL_EMIT: u32 = 0x4e88f0;
const CAR_PROBLEM_EMIT: u32 = 0x4e8a70;

/// `CMultiSignal1<CSigTireParticles>::Emit` (0x53d0d0) on the tyre-particle signal, `[0x893600] + 0x4518` (the wheel
/// step's emit, `rbr_physics::wheel::layout`), whose payloads [`Watch::take_tyre_particles`] gives the host. The same
/// code emits `CSigSolidGeneralState`, on another signal.
pub(crate) const TYRE_PARTICLES_EMIT: u32 = 0x53d0d0;
const TYRE_PARTICLES_SIGNAL: u32 = 0x4518;
/// The payload bytes kept (`rbr_recording::particles::PAYLOAD_BYTES`).
pub const TYRE_PARTICLES_BYTES: usize = 0xb0;
/// The other emits the particles take (with `keep`), each on its signal `[0x893600] + offset`:
/// `(emit, signal offset, kind, payload bytes)`. `CMultiSignal1<CSigCarDamage>::Emit` with the handler's 0x40-byte
/// record, the damping event of `CSolid::Emit` (0x52be70, 0x40 bytes) and `CSigEngineFire`'s float. The kinds are
/// `rbr_recording::particles::Event`'s.
const PARTICLE_EMITS: [(u32, u32, u32, usize); 3] =
    [(0x4ef1d0, 0x9f20, 1, 0x40), (0x52be70, 0xadc8, 2, 0x40), (0x4ef290, 0xb840, 3, 4)];

/// The bytes of a particle event's payload.
pub const PARTICLE_EVENT_BYTES: usize = 0x40;
/// `CMultiSignal1<CSigEngineFire>::Emit` (0x4ef290) on the engine-fire signal (`[0x893600] + 0xb840`), whose emits
/// [`Watch::take_engine_fires`] counts for the host (the sound's fire). It is [`PARTICLE_EMITS`]' last row.
const ENGINE_FIRE_EMIT: u32 = PARTICLE_EMITS[2].0;
const ENGINE_FIRE_SIGNAL: u32 = PARTICLE_EMITS[2].1;

/// The emits [`Watch::watch`] keeps a payload or a count of.
pub(crate) const WATCHED: [u32; 9] = [
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

/// The watched emits' kept payloads and counts, taken by the host after each step.
#[derive(Default)]
pub(crate) struct Watch {
    /// The code of the last `CSigOutsideAssistance` emit (0x4e89b0), the word its argument points at: what the HUD's
    /// receiver (0x573900) keeps for the race clock. The host takes it after each step.
    outside_assistance: Cell<Option<u32>>,
    /// The tyre-particle payloads emitted since the host last took them, copied at the emit's entry.
    tyre_particles: RefCell<Vec<[u8; TYRE_PARTICLES_BYTES]>>,
    /// The tyre steps since the host last took them: the scratch and its `FillOutputs` term.
    tyre_terms: RefCell<Vec<(u32, Option<f32>)>>,
    /// The other particle emits since the host last took them: kind and payload (zero padded).
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
    /// The damper-signal payloads `CSpringDamper::ResetOutputs` emitted since the host last took them: the strut
    /// and the payload the reset handed to the signal manager.
    damper_signals: RefCell<Vec<(u32, DamperSignal)>>,
}

impl Watch {
    /// The emits the host keeps a payload or a count of ([`WATCHED`]), at their entry. `keep` keeps the
    /// tyre-particle payloads (the live viewer's particles; off in every check).
    #[inline(never)]
    pub(crate) fn watch(&self, mem: &Mem, addr: u32, regs: &Regs, keep: bool) {
        if addr == OUTSIDE_ASSISTANCE_EMIT {
            self.outside_assistance.set(Some(mem.u32(regs.stack[0])));
        }
        if keep && addr == TYRE_PARTICLES_EMIT && regs.ecx == mem.u32(SIGNALS).wrapping_add(TYRE_PARTICLES_SIGNAL) {
            if let Some(bytes) = mem.try_read(regs.stack[0], TYRE_PARTICLES_BYTES) {
                self.tyre_particles.borrow_mut().push(bytes.try_into().expect("the length read"));
            }
        }
        if keep {
            for &(emit, signal, kind, bytes) in &PARTICLE_EMITS {
                if addr == emit && regs.ecx == mem.u32(SIGNALS).wrapping_add(signal) {
                    if let Some(read) = mem.try_read(regs.stack[0], bytes) {
                        let mut payload = [0; PARTICLE_EVENT_BYTES];
                        payload[..bytes].copy_from_slice(&read);
                        self.particle_events.borrow_mut().push((kind, payload));
                    }
                }
            }
        }
        if addr == ENGINE_FIRE_EMIT && regs.ecx == mem.u32(SIGNALS).wrapping_add(ENGINE_FIRE_SIGNAL) {
            self.engine_fires.set(self.engine_fires.get() + 1);
        }
        if addr == RETIRE_EMIT {
            self.retire.set(Some(mem.u32(regs.stack[0])));
        }
        if addr == STAND_STILL_EMIT {
            self.stand_still.set(Some([mem.u32(regs.stack[0]), mem.u32(regs.stack[0] + 4)]));
        }
        if addr == CAR_PROBLEM_EMIT {
            self.car_problem.set(Some(mem.u32(regs.stack[0])));
        }
    }

    /// The tyre step's scratch and the term its `FillOutputs` folded into the tyre-noise block's word 1.
    pub(crate) fn push_tyre_term(&self, scratch: u32, term: Option<f32>) {
        self.tyre_terms.borrow_mut().push((scratch, term));
    }

    /// The damper-signal payload `ResetOutputs` on `strut` emitted, kept as a typed event for the host.
    pub(crate) fn push_damper(&self, strut: u32, signal: DamperSignal) {
        self.damper_signals.borrow_mut().push((strut, signal));
    }

    /// The tyre-particle payloads emitted since the last call (with `keep`), in emit order.
    pub(crate) fn take_tyre_particles(&self) -> Vec<[u8; TYRE_PARTICLES_BYTES]> {
        self.tyre_particles.take()
    }

    /// The tyre steps since the last call (with `keep`), in order: each one's scratch and the term its
    /// `FillOutputs` folded into the tyre-noise block's word 1, `None` for the no-contact path
    /// (`rbr_recording::sound::TyreMax`).
    pub(crate) fn take_tyre_terms(&self) -> Vec<(u32, Option<f32>)> {
        self.tyre_terms.take()
    }

    /// The damage, damping and engine-fire emits since the last call (with `keep`), in emit order: each one's kind
    /// ([`PARTICLE_EMITS`]) and payload.
    pub(crate) fn take_particle_events(&self) -> Vec<(u32, [u8; PARTICLE_EVENT_BYTES])> {
        self.particle_events.take()
    }

    /// The `CSigEngineFire` emits since the last call.
    pub(crate) fn take_engine_fires(&self) -> u32 {
        self.engine_fires.take()
    }

    /// The damper-signal payloads emitted since the last call, in emit order: the strut and its payload.
    pub(crate) fn take_damper_signals(&self) -> Vec<(u32, DamperSignal)> {
        self.damper_signals.take()
    }

    /// The code of the `CSigOutsideAssistance` emitted since the last call, if any.
    pub(crate) fn take_outside_assistance(&self) -> Option<u32> {
        self.outside_assistance.take()
    }

    /// The last `CSigCarStandStill` payload (count, kind) and `CSigCarProblem` kind emitted since the last call.
    pub(crate) fn take_hud_signals(&self) -> (Option<[u32; 2]>, Option<u32>) {
        (self.stand_still.take(), self.car_problem.take())
    }

    /// The code of the `CSigRetireCar` emitted since the last call, if any.
    pub(crate) fn take_retire(&self) -> Option<u32> {
        self.retire.take()
    }
}
