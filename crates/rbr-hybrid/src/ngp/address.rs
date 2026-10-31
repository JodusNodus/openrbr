//! NGP.dll's running addresses: the thread-local base ([`Scope`]), file and running conversion, and the trampoline and emit-wrapper tables.

use std::cell::Cell;

use rbr_physics::mem::Mem;

use crate::Decline;

pub(crate) const FILE_BASE: u32 = 0x1000_0000;

/// NGP.dll's image size (`SizeOfImage` 0x1a2000), rounded up.
const SIZE: u32 = 0x0020_0000;

thread_local! {
    /// NGP.dll's base for the run on this thread, 0 without NGP ([`Scope`]).
    static BASE: Cell<u32> = const { Cell::new(0) };
    /// NGP's wall clock (us) for this thread's step, when a playback replays a recording's ([`set_wall_clock`]).
    pub(crate) static WALL_CLOCK: Cell<Option<u64>> = const { Cell::new(None) };
}

/// The wall clock NGP's starter reads during the next steps on this thread (us, `QueryPerformanceCounter` as
/// `0x10111ce0` scales it), from a recording; `None` (the default) times the starter by simulation time.
pub fn set_wall_clock(us: Option<u64>) {
    WALL_CLOCK.set(us);
}

/// NGP.dll's base for the runs on this thread while it lives, the outer one restored on drop. Each host (a pure
/// playback, the emulator) holds its own base and opens a scope around its runs, so boots on several threads, or in
/// turn on one thread, don't see each other's.
pub struct Scope(u32);

impl Scope {
    pub fn new(base: u32) -> Self {
        Scope(BASE.replace(base))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        BASE.set(self.0);
    }
}

/// Whether the snapshot with NGP.dll at `base` runs NGP (`base` 0 is without it: a 1.02 snapshot).
pub fn runs_ngp(base: u32) -> bool {
    base != 0
}

/// Whether the running snapshot runs NGP.
pub(crate) fn is_ngp() -> bool {
    runs_ngp(BASE.get())
}

/// Declines on a 1.02 snapshot: the port is NGP's behaviour of a function NGP patches, so the original runs there.
pub(crate) fn only() -> Result<(), Decline> {
    if is_ngp() {
        Ok(())
    } else {
        Err(Decline::Unsupported("the port is NGP's; the snapshot runs 1.02"))
    }
}

/// NGP.dll's base in a memory image, 0 without NGP: under NGP the `CCar::CDef` key dict (`0x8ef73c`) points at NGP's
/// copy of its table (file address `0x10171318`) instead of the exe's (`0x7c6578`, `notes/ngp.md` "Load path").
pub fn base_in(mem: &Mem) -> u32 {
    let table = mem.u32(0x8e_f73c + 0xc);
    if table == 0x7c_6578 || table == 0 {
        0
    } else {
        table.wrapping_sub(0x17_1318)
    }
}

/// NGP's trampolines to the original exe functions behind its patches (rbr-recomp `notes/ngp-replacements.md`, "Trampoline to
/// the original"): each calls the exe function with its own arguments and returns its result.
const TRAMPOLINES: [(u32, u32); 16] = [
    (0x1006_0140, 0x52_e760),
    (0x1005_b370, 0x53_20c0),
    (0x1005_b540, 0x53_60c0),
    (0x1005_bb70, 0x53_4090),
    (0x1005_9e80, 0x52_e090),
    (0x1005_98a0, 0x52_dd70),
    (0x1004_b800, 0x51_8e20),
    (0x1004_5df0, 0x50_9bc0),
    (0x1004_5d20, 0x50_6b20),
    (0x1004_66b0, 0x50_4060),
    (0x1004_7650, 0x50_9210),
    (0x1004_7850, 0x50_9120),
    (0x1004_d060, 0x51_b4c0),
    (0x1004_dce0, 0x52_0940),
    (0x1003_3fc0, 0x61_e1b0),
    (0x1006_3f10, 0x60_1fb0),
];

/// NGP's emit wrappers at retargeted emit calls, with the 1.02 emit each replaces (`patches-runtime.tsv`): they emit
/// the signal to its 1.02 connections (on NGP's strand when the singleton's `+0xd` is set) and feed NGP's own
/// listeners. `0x1005f8a0` also emits a copy of the tyre particle payload with its vectors moved, `0x100ee000` calls
/// NGP's exhaust sound state and `0x100ed5e0` the replay guard first (`notes/ngp.md` "Replacements"). The general-state
/// pair (`0x10041f30`, `0x1005f6f0`) take the car or wheel from the caller's esi, write a vector per wheel from NGP's
/// own getters into their copy of the signal, and `0x10041f30` hands the copy to the fence handler (cloth only) before
/// the fan-out. None writes car state, so the pure host runs the 1.02 emit in their place (rbr-recomp `notes/ngp.md`, "Pure step
/// under NGP").
const EMIT_WRAPPERS: [(u32, u32); 22] = [
    // `Sim8E20` (0x5191a9) and `E88Car::Vf18`: `E41EngineNoiseSignal::Emit`.
    (0x100e_e000, 0x50_ef50),
    // `E51Wheel::M3DD30` (0x53e317) and `M3EB80`: the tyre particle emit.
    (0x1005_f8a0, 0x53_d0d0),
    // `E34WallVf::Raycast` and `CCar::Vf6cx` (0x4ea3b1): BrakeWallPenetration.
    (0x100e_d470, 0x4e_4490),
    // `E34_RetireA` and `CCar::Update9AF0`: RetireCar.
    (0x100e_e6e0, 0x4e_8770),
    // `CCar::UpdateSeed`: CarProblem.
    (0x100e_d8a0, 0x4e_8a70),
    // `CCar::Vf180`: Respawn.
    (0x100e_e4e0, 0x4e_8830),
    // `CCar::Update9AF0`: StandStill.
    (0x100e_da50, 0x4e_88f0),
    // `CCar::Vf28x`, `CCarBody::Vf28x`, `SimFB40`, the E45 mount and `CSolid::E43Vf28`: SolidGeneralState.
    (0x100e_e7a0, 0x4e_8330),
    // `CCar::Vf28x` (0x4ea1f7): the float emit.
    (0x100e_d3a0, 0x4e_84b0),
    // `E36DamageOps::EngineGate`: EngineFire.
    (0x100e_df40, 0x4e_f290),
    // `E36DamageOps::Slot1`: CarDamage.
    (0x100e_d5e0, 0x4e_f1d0),
    // `CCarLightSystem::EmitLights`: CarLight.
    (0x100e_d7d0, 0x4f_2000),
    // `CDrive::M8480`: DrivetrainState.
    (0x100e_dd90, 0x50_6e60),
    // `Sim8CF0`: the exhaust emit.
    (0x100e_e0f0, 0x50_ee90),
    // `HingedRigidBody::SimFB40`: HingedBody.
    (0x100e_e260, 0x51_f640),
    // `SimFD00` and the E45 mount: DetachFromParent.
    (0x100e_dcd0, 0x51_f580),
    // The E45 passenger: IncarShake.
    (0x100e_e330, 0x52_3010),
    // `CSolid::BigB`: `CSolid::Emit`.
    (0x100e_dbf0, 0x52_be70),
    // `CSpringDamper::ResetOutputs`: `DamperSignal::Emit`.
    (0x100e_db20, 0x52_de60),
    // `CCar::Vf28x` (0x4ea1b3): CarGeneralState.
    (0x1004_1f30, 0x4e_83f0),
    // `CWheel::E43Vf28` (0x53f386): SolidGeneralState.
    (0x1005_f6f0, 0x4e_8330),
    // `CScratch::EmitNoise` (0x5358a6): `CMultiSignal1<CSigTyreNoise>::Emit`. On the strand it posts every fifth call
    // (by the count at `+0x18b86c` of the object at its singleton's `+0x1c`), with the 0x60-byte payload copied.
    (0x100e_e970, 0x53_4f80),
];

/// The exe function the pure host runs for the NGP code at `addr` (a running address): the target of a trampoline,
/// or the 1.02 emit an emit wrapper stands for.
pub fn original_for(addr: u32) -> Option<u32> {
    let at = file(addr);
    TRAMPOLINES.iter().chain(&EMIT_WRAPPERS).find_map(|&(t, exe)| (t == at).then_some(exe))
}

/// The trampolines (file addresses) with the exe functions they reach: the emulator binds each where its target is
/// bound, since it enters the target past the hooked entry.
pub(crate) fn trampolines() -> impl Iterator<Item = (u32, u32)> {
    TRAMPOLINES.into_iter()
}

/// Whether `addr` is an NGP.dll file address.
pub(crate) fn is_file(addr: u32) -> bool {
    (FILE_BASE..FILE_BASE + SIZE).contains(&addr)
}

/// The file address `addr` at the snapshot's base (`None` without NGP).
pub(crate) fn at(addr: u32) -> Option<u32> {
    let base = BASE.get();
    (base != 0).then(|| addr - FILE_BASE + base)
}

/// A hooked address back at the file base (unchanged outside NGP.dll).
pub fn file(addr: u32) -> u32 {
    file_at(BASE.get(), addr)
}

/// [`file`] with NGP.dll at `base` (0 without it).
pub fn file_at(base: u32, addr: u32) -> u32 {
    if base != 0 && (base..base + SIZE).contains(&addr) {
        addr - base + FILE_BASE
    } else {
        addr
    }
}
