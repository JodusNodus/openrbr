//! Bindings of NGP.dll's own functions (`rbr_physics::ngp`, `notes/ngp.md`). NGP.dll is relocated in a running game,
//! so its bindings are listed at file addresses (base `0x10000000`) and hooked at the snapshot's base, which the
//! harness passes at install ([`Scope`]; 0 for a 1.02 snapshot, where they aren't hooked).

use std::cell::Cell;

use rbr_physics::engine_driveline::layout::EngineTuning;
use rbr_physics::engine_driveline::layout::{with_worker, DriveWorker, HookBlock};
use rbr_physics::mem::Mem;
use rbr_physics::ngp::atmosphere::{atmosphere, Atmosphere};
use rbr_physics::ngp::axle;
use rbr_physics::ngp::curve_term::{BlockExtra, CurveTermIn};
use rbr_physics::ngp::damper;
use rbr_physics::ngp::diff;
use rbr_physics::ngp::downforce::{downforce, DownforceIn, Path};
use rbr_physics::ngp::drive;
use rbr_physics::ngp::drive_line;
use rbr_physics::ngp::engine::{self as ngp_engine, TorqueIn};
use rbr_physics::ngp::material;
use rbr_physics::ngp::outputs;
use rbr_physics::ngp::spring::{self, Latch};
use rbr_physics::ngp::starter;
use rbr_physics::ngp::step_wrapper;
use rbr_physics::ngp::tag_blend;

use rbr_physics::spring_damper::layout as damper_layout;
use rbr_physics::tyre::store::TyreStore;
use rbr_physics::wheel::layout::{ProbeTuning, WheelTuning};
use rbr_physics::Variant;

use crate::{Binding, Ctx, Decline, Regs, Return};

const FILE_BASE: u32 = 0x1000_0000;
/// NGP.dll's image size (`SizeOfImage` 0x1a2000), rounded up.
const SIZE: u32 = 0x0020_0000;

thread_local! {
    /// NGP.dll's base for the run on this thread, 0 without NGP ([`Scope`]).
    static BASE: Cell<u32> = const { Cell::new(0) };
    /// NGP's wall clock (us) for this thread's step, when a playback replays a recording's ([`set_wall_clock`]).
    static WALL_CLOCK: Cell<Option<u64>> = const { Cell::new(None) };
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

/// The running snapshot's physics: NGP's when NGP.dll is loaded.
pub fn variant() -> Variant {
    variant_at(BASE.get())
}

/// The physics of a snapshot with NGP.dll at `base` (0 without it).
pub fn variant_at(base: u32) -> Variant {
    if base == 0 {
        Variant::Stock102
    } else {
        Variant::Ngp778
    }
}

/// The target of the `call rel32` at `site`, which NGP may have retargeted to its own code.
pub(crate) fn call_target(mem: &Mem, site: u32) -> u32 {
    site.wrapping_add(5).wrapping_add(mem.u32(site + 1))
}

/// NGP's per-car load thresholds of the wheel step (`10172f58`, `10172f5c`, `10172f60`; `wheel::layout::STEP_SLIP`).
pub(crate) fn step_slip(ctx: &Ctx) -> Option<[f32; 3]> {
    at(FILE_BASE)?;
    Some([0x1017_2f58, 0x1017_2f5c, 0x1017_2f60].map(|a| data_f32(ctx, a)))
}

/// The drag factor `CCar::DragAndSpinDamp` reads: NGP's `1017147c` under NGP, else 1.02's 0.5.
pub(crate) fn drag_factor(ctx: &Ctx) -> f32 {
    match at(FILE_BASE) {
        Some(_) => data_f32(ctx, 0x1017_147c),
        None => rbr_physics::car_body::drag::DRAG_FACTOR,
    }
}

/// The scale of `Step1E0`'s clutch input: NGP's per-car `10171aa4` under NGP, else 1.02's 900.
pub(crate) fn clutch_scale(ctx: &Ctx) -> f32 {
    match at(FILE_BASE) {
        Some(_) => data_f32(ctx, 0x1017_1aa4),
        None => rbr_physics::engine_driveline::drive_step::CLUTCH_SCALE,
    }
}

/// `E36DamageOps::Slot1`'s constants: under NGP the patched push immediate (0x4f13a7) and the value its repointed
/// compare (operand at 0x4f13d3) reads, both scaled per car; else 1.02's.
pub(crate) fn damage_tuning(ctx: &Ctx) -> rbr_physics::car_body::damage::DamageTuning {
    if at(FILE_BASE).is_none() {
        return rbr_physics::car_body::damage::DamageTuning::STOCK;
    }
    rbr_physics::car_body::damage::DamageTuning {
        engine_gate_limit: ctx.mem.f32(0x004f_13a7),
        emit_impact: ctx.mem.f32(ctx.mem.u32(0x004f_13d3)),
    }
}

/// The VCU's shift cooldown, launch rpm and launch clutch band: NGP's per-car `10172db0`, `10172db4`, `10171ae4`,
/// `10172338` and `101767c4` under NGP, else 1.02's constants.
pub(crate) fn vcu_tuning(ctx: &Ctx) -> rbr_physics::vcu::layout::Tuning {
    if at(FILE_BASE).is_none() {
        return rbr_physics::vcu::layout::Tuning::STOCK;
    }
    let d = |a| data_f32(ctx, a);
    rbr_physics::vcu::layout::Tuning {
        cooldown: d(0x1017_2db0),
        launch_rpm: d(0x1017_2db4),
        clutch_from: d(0x1017_1ae4),
        clutch_to: d(0x1017_2338),
        clutch_slope: d(0x1017_67c4),
        cut_margin: d(0x1017_1ab4),
    }
}

/// The engine stage's air and tuning words: NGP's `.data` under NGP, else 1.02's constants.
pub(crate) fn engine_tuning(ctx: &Ctx) -> EngineTuning {
    if at(FILE_BASE).is_none() {
        return EngineTuning::STOCK;
    }
    let d = |a| data_f32(ctx, a);
    EngineTuning {
        variant: Variant::Ngp778,
        air: Atmosphere {
            temperature: d(0x1017_1adc),
            pressure: d(0x1017_1ad8),
            density: d(0x1017_1ae0),
            inv_pressure: d(0x1017_67c8),
            pressure_115: d(0x1017_67cc),
            pressure_0987: d(0x1017_67d0),
        },
        band: [d(0x1017_1ac4), d(0x1017_1ac8)],
        rev_gate: [d(0x1017_1aa8), d(0x1017_1aac), d(0x1017_1ab0)],
        misfire_speed: d(0x1017_1ab8),
        misfire_heat: d(0x1017_1ac0),
        stall_speed: d(0x1017_1abc),
    }
}

/// What the tyre thermal steps of `scratch` read under NGP (`None` without it).
pub(crate) fn thermal(ctx: &Ctx, scratch: u32) -> Option<damper::Thermal> {
    at(FILE_BASE)?;
    let m = &ctx.mem;
    let (inner, wheel) = (m.u32(scratch + 0x10), m.u32(scratch + 0x410));
    Some(damper::Thermal {
        temperature: data_f32(ctx, 0x1017_1adc),
        relax_rate: data_f32(ctx, 0x1017_1478),
        w9c: m.f32(inner + 0x9c),
        wac: m.f32(inner + 0xac),
        convection: damper::Convection {
            w7c: m.f32(inner + 0x7c),
            w8c: m.f32(inner + 0x8c),
            wec: m.f32(inner + 0xec),
            kind: m.u32(inner + 0x1c),
            spin: m.f32(wheel + 0xea0),
            car_speed: m.f32(m.u32(wheel + 0x7f4) + 0x15f8),
            boost: (m.u32(scratch + 0x480) != 0).then(|| m.f32(scratch + 0x1c8)),
            factors: [data_f32(ctx, 0x1017_1470), data_f32(ctx, 0x1017_1474)],
        },
    })
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

pub static BINDINGS: [Binding; 21] = [
    Binding { addr: 0x1001_5fc0, name: "NGP drive line sampler (SampleBig)", subsystem: "ngp", run: drive_line_sample },
    Binding { addr: STARTER, name: "NGP starter (M6C90)", subsystem: "ngp", run: starter_binding },
    Binding { addr: 0x1004_b5a0, name: "NGP atmosphere", subsystem: "ngp", run: atmosphere_update },
    Binding { addr: 0x1004_7040, name: "NGP downforce", subsystem: "ngp", run: downforce_update },
    Binding { addr: 0x1006_0230, name: "NGP DamperSolve (pressure limit)", subsystem: "ngp", run: damper_solve },
    Binding { addr: 0x1004_d2f0, name: "NGP DamperStep1", subsystem: "ngp", run: damper_step1 },
    Binding {
        addr: 0x1004_d3c0,
        name: "NGP DamperStep1 (scratch factor)",
        subsystem: "ngp",
        run: damper_step1_scratch,
    },
    Binding { addr: 0x1004_d380, name: "NGP DamperStep2", subsystem: "ngp", run: damper_step2 },
    Binding { addr: 0x1005_b4a0, name: "NGP ComputeOutputs wrapper", subsystem: "ngp", run: compute_outputs },
    Binding { addr: 0x1004_7300, name: "NGP CDrive::M93A0", subsystem: "ngp", run: clutch_torque },
    Binding { addr: 0x1005_e8a0, name: "NGP E51Wheel::M3D230", subsystem: "ngp", run: tag_blend },
    Binding { addr: 0x1004_58c0, name: "NGP CDrive::M5FE0", subsystem: "ngp", run: wheel_slip_value },
    Binding { addr: 0x1004_5b80, name: "NGP CDrive::M6260", subsystem: "ngp", run: wheel_latch },
    Binding { addr: 0x1004_df40, name: "NGP E44MatTable slot lookup", subsystem: "ngp", run: sample_slot },
    Binding { addr: 0x1004_8d50, name: "NGP CDrive::M9BC0 wrapper (diffs)", subsystem: "ngp", run: drive_wrapper },
    Binding { addr: 0x1004_87f0, name: "NGP axle worker (drive types 4 and 5)", subsystem: "ngp", run: axle_worker },
    Binding {
        addr: 0x1004_d410,
        name: "NGP tyre convection (RelaxStep's DamperStep1)",
        subsystem: "ngp",
        run: convection,
    },
    Binding {
        addr: CURVE_TERM,
        name: "NGP E39_Hook4160 rewrite (curve term and heat)",
        subsystem: "ngp",
        run: curve_term_binding,
    },
    Binding {
        addr: ENGINE_WRAPPER,
        name: "NGP engine wrapper (Fwd18E20's jmp)",
        subsystem: "ngp",
        run: engine_wrapper,
    },
    Binding { addr: 0x1005_b660, name: "NGP CScratch::Step wrapper", subsystem: "ngp", run: step_wrapper },
    Binding {
        addr: 0x1005_9f70,
        name: "NGP CSpringDamper::Step wrapper (bump stop)",
        subsystem: "ngp",
        run: spring_step,
    },
];

/// The words `0x1004b5a0` stores, in [`rbr_physics::ngp::atmosphere::Atmosphere`]'s order.
const ATMOSPHERE_OUT: [u32; 6] = [0x1017_1adc, 0x1017_1ad8, 0x1017_1ae0, 0x1017_67c8, 0x1017_67cc, 0x1017_67d0];

/// `0x1004b5a0` (cdecl, the car): the atmosphere at the car's height (`+0x148`), into NGP's `.data`. Leaves the car
/// in eax (its first load).
fn atmosphere_update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let car = r.stack[0];
    apply_atmosphere(&mut ctx.mem, car);
    Ok(Return::void(car, 0))
}

/// `0x1004b5a0`'s work for `car`.
fn apply_atmosphere(m: &mut Mem, car: u32) {
    let a = atmosphere(m.f32(car + 0x148));
    let values = [a.temperature, a.pressure, a.density, a.inv_pressure, a.pressure_115, a.pressure_0987];
    for (addr, v) in ATMOSPHERE_OUT.into_iter().zip(values) {
        m.set_f32(at(addr).expect("hooked only with NGP"), v);
    }
}

/// `0x10047040` (cdecl, `p`; the car is `*(*(p+8)+0x580)`): the downforce into the car's sums, and the drag factor.
/// `eax` is what the original's path leaves: 8 (no downforce), the speed's direction (0, 1 or -1, no speed) or the
/// wheel at `car+0x474` (applied).
fn downforce_update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(apply_downforce(&mut ctx.mem, r.stack[0]), 0))
}

/// `0x10047040`'s work for `p`; returns its eax.
fn apply_downforce(m: &mut Mem, p: u32) -> u32 {
    let car = m.u32(m.u32(p + 8) + 0x580);
    let def = m.u32(car + 0x34);
    let dmg = m.u32(car + 0x9f4);
    let i = DownforceIn {
        damage: (dmg != 0).then(|| [m.u32(dmg + 0x3db4), m.u32(dmg + 0x3dbc)]),
        front: m.f32(def + 0x1fc),
        rear: m.f32(def + 0x20c),
        density: m.f32(at(0x1017_1ae0).expect("hooked only with NGP")),
        speed: m.f32(car + 0x15f8),
        wheel_arms: [0x470, 0x474, 0x478, 0x47c].map(|o| m.f32(m.u32(car + o) + 0x8b4)),
        force_sum: m.f32(car + 0x378),
        moment_sum: m.f32(car + 0x310),
    };
    let o = downforce(&i);
    m.set_f32(car + 0x4a4, o.speed_ratio_sq);
    m.set_f32(car + 0x4a8, o.front_force);
    m.set_f32(car + 0x4ac, o.rear_force);
    m.set_f32(car + 0x378, o.force_sum);
    m.set_f32(car + 0x310, o.moment_sum);
    m.set_f32(at(0x1017_147c).expect("hooked only with NGP"), o.drag_factor);
    match o.path {
        Path::NoDownforce => 8,
        Path::NoSpeed(direction) => direction as u32,
        Path::Applied => m.u32(car + 0x474),
    }
}

/// `0x10048d50` (thiscall on the drive, `dt`, `ret 4`): NGP's wrapper of `M9BC0` (`rbr_physics::ngp::diff`). The
/// downforce and the gain's loss, the diffs' lock torques for NGP's drivetrain types, then the wheel worker: NGP's axle
/// worker `0x100487f0` for types 4 and 5 (ported), else `M9BC0` in the original through NGP's trampoline `0x10045df0`.
fn drive_wrapper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let drive = r.ecx;
    let m = &ctx.mem;
    let (def, state, obj) = (m.u32(drive + 4), m.u32(drive + 8), m.u32(drive + 0x170));
    let word = m.u32(def + 0x640);
    let kind = diff::drive_type(word);
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    if settings == 0 {
        return Err(Decline::Unsupported("NGP's settings object is not made yet"));
    }
    if (4..8).contains(&kind) && (0..4).any(|k| m.u32(def + 0x158 + 0xf0 * k) > 3) {
        return Err(Decline::Unsupported("a wheel's pack index is past the axles"));
    }
    if matches!(kind, 4 | 5) && !axle_fits(m, def, kind - 3) {
        return Err(Decline::Unsupported("the axle's blocks or packs are out of range"));
    }
    apply_downforce(&mut ctx.mem, drive);
    let loss = ctx.mem.u32(settings + 0xe88);
    let gain = diff::gain_loss(ctx.mem.f32(state + 0x37c), loss);
    ctx.mem.set_f32(state + 0x37c, gain);
    if (4..8).contains(&kind) {
        let m = &ctx.mem;
        let car = m.u32(state + 0x580);
        let locks = m.u32(car + 0x850);
        let tuning = diff::DiffTuning {
            locks: [[0x1b8, 0x268], [0x738, 0x7e8], [0x9f8, 0xaa8]].map(|p| p.map(|o| m.f32(locks + o))),
            speed: m.f32(car + 0x864),
            speed_limit: m.f32(locks + 0x198),
            preload: [0, 1, 2].map(|g| m.f32(def + 0x58 + 0x60 * g)),
            enabled: [4, 0xc, 0x14].map(|o| m.u32(obj + o)),
            split: data_f32(ctx, 0x1017_1a90),
            viscous: data_f32(ctx, 0x1017_1aa0),
        };
        let w = DriveWorker::load(m, drive);
        let scales = diff::lock_torques(&w, diff::diff_types(word), &tuning);
        for (g, v) in scales.into_iter().enumerate() {
            ctx.mem.set_f32(state + 0x18 + 0x14 * g as u32, v);
        }
    }
    let dt = r.stack[0];
    if matches!(kind, 4 | 5) {
        let g = (kind - 3) as usize;
        with_worker(&mut ctx.mem, drive, |w| axle::axle_worker(w, f(dt), g));
        return Ok(Return::void(0, 4));
    }
    let called = ctx.host.call(at(WHEEL_WORKER).unwrap(), drive, &[dt]);
    Ok(Return::void(called.eax, 4))
}

/// Whether `0x100487f0` on axle `g` stays in the drive's records: its two blocks are wheels and their packs index
/// the active flags (at most 2).
fn axle_fits(m: &Mem, def: u32, g: u32) -> bool {
    [0x28, 0x38].into_iter().all(|o| {
        let k = m.u32(def + 0x60 * g + o);
        k < 4 && m.u32(def + 0x158 + 0xf0 * k) <= 2
    })
}

/// `0x100487f0` (thiscall on the drive, `(dt, g)`, `ret 8`, void): NGP's axle worker (`rbr_physics::ngp::axle`).
fn axle_worker(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dt, g, ..] = r.stack;
    let def = ctx.mem.u32(r.ecx + 4);
    if !matches!(g, 1 | 2) || !axle_fits(&ctx.mem, def, g) {
        return Err(Decline::Unsupported("the axle's blocks or packs are out of range"));
    }
    with_worker(&mut ctx.mem, r.ecx, |w| axle::axle_worker(w, f(dt), g as usize));
    Ok(Return::void(0, 8))
}

/// `0x10015fc0` (thiscall on the drive line, `(pos, x)`, `ret 8`, st0): NGP's `SampleBig` in doubles
/// (`rbr_physics::ngp::drive_line`).
fn drive_line_sample(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let v = drive_line::sample(&ctx.mem, r.ecx, r.stack[0], f(r.stack[1]));
    Ok(Return::float(f64::from(v), 8))
}

/// NGP's starter, the rewrite of `CDrive::M6C90` that `Step1E0` calls at `0x50a25c`.
pub(crate) const STARTER: u32 = 0x1004_56c0;
/// NGP's wall-clock stamp of the starter (us, eight bytes); timed by simulation time, the port keeps its hold there.
const STARTER_HELD: u32 = 0x1017_6718;

/// `0x100456c0` (thiscall on the drive, `(dt)`, `ret 4`, st0): NGP's starter (`rbr_physics::ngp::starter`).
fn starter_binding(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let v = starter_step(&mut ctx.mem, r.ecx, f(r.stack[0]));
    Ok(Return::float(f64::from(v), 4))
}

/// `0x100456c0`'s work, timed by simulation time (the hold kept at NGP's stamp, `notes/ngp.md`) or by the wall clock
/// set for the step (the stamp taken while the gate is clear, as NGP does).
pub(crate) fn starter_step(m: &mut Mem, drive: u32, dt: f32) -> f32 {
    let held_at = at(STARTER_HELD).expect("hooked only with NGP");
    let kept = u64::from(m.u32(held_at)) | u64::from(m.u32(held_at + 4)) << 32;
    let band = m.f32(at(0x1017_1a98).unwrap());
    let (v, kept) = with_worker(m, drive, |w| {
        let gated = w.this.lane_gate != 0;
        let (held, kept) = match WALL_CLOCK.get() {
            Some(now) if gated => (now.wrapping_sub(kept), kept),
            Some(now) => (0, now),
            None => {
                let held = starter::sim_held(gated, dt, kept);
                (held, held)
            }
        };
        (starter::starter(w, dt, band, held), kept)
    });
    m.set_u32(held_at, kept as u32);
    m.set_u32(held_at + 4, (kept >> 32) as u32);
    v
}

/// NGP's rewrite of `E39_Hook4160`, which `Step1E0` calls at `0x50a2a8`.
pub(crate) const CURVE_TERM: u32 = 0x1004_6880;

/// `0x10046880` (thiscall on the drive, `(a1, c, n, x, dt)`, `ret 0x14`, st0): NGP's curve term
/// (`rbr_physics::ngp::curve_term`), with 1.02's `Worker4060` ported.
fn curve_term_binding(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a1, c, n, x, dt, ..] = r.stack;
    let v = curve_term(&mut ctx.mem, r.ecx, [a1, c, n, x, dt])?;
    Ok(Return::float(f64::from(v), 0x14))
}

/// `0x10046880`'s work; declines (before any writes) without NGP's settings or for a state that isn't a wheel's.
pub(crate) fn curve_term(m: &mut Mem, drive: u32, args: [u32; 5]) -> Result<f32, Decline> {
    let [a1, c, n, x, dt] = args;
    let state = m.u32(drive + 8);
    let car = m.u32(state + 0x580);
    let u = a1.wrapping_sub(state).wrapping_sub(0x48) / 200;
    if u > 3 {
        return Err(Decline::Unsupported("the curve state is not a wheel's"));
    }
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    if settings == 0 {
        return Err(Decline::Unsupported("NGP's settings object is not made yet"));
    }
    let front = u < 2;
    let rear_split =
        diff::diff_types(m.u32(n + 0x640))[0] == 3 && !front && m.f32(m.u32(car + 0x850) + 0x198) >= m.f32(car + 0x864);
    let i = CurveTermIn {
        block: HookBlock::load(m, c),
        extra: BlockExtra { convection: m.f32(c + 0x4c), share: m.f32(c + 0x5c), lane_convection: m.f32(c + 0xac) },
        slope: m.f32(a1 + 0x1c),
        term_gain: m.f32(a1 + 0xac),
        pos: m.u32(a1 + 0x7c),
        lane: m.u32(a1 + 0x8c),
        w60: m.f32(x + 0x60),
        w70: m.f32(x + 0x70),
        gain_factor: m.f32(at(0x1017_1a94).unwrap()),
        gated: front && m.u32(m.u32(drive + 0x170) + 0x80) == 0,
        slope_factor: m.f32(settings + if front { 0xfe8 } else { 0x1168 }),
        speed: m.f32(car + 0x15f8),
        air: m.f32(at(0x1017_1adc).unwrap()),
        dt: f(dt),
        split: rear_split,
    };
    let o = rbr_physics::ngp::curve_term::curve_term(&i, |w| {
        crate::engine_driveline::row_value_at(m, w, a1, c, n, f(dt)) as f32
    });
    m.set_u32(a1 + 0x7c, o.pos);
    m.set_u32(a1 + 0x8c, o.lane);
    let row = at(0x1017_662c).unwrap() + 0x3c * u;
    for (k, v) in o.telemetry.into_iter().enumerate() {
        m.set_f32(row + 4 * k as u32, v);
    }
    if let Some(t) = o.transfer {
        let slot = state + 200 * u - 0x98;
        m.set_f32(slot, m.f32(slot) + t);
    }
    Ok(o.value)
}

/// NGP's engine wrapper, the target of `CCar::Fwd18E20`'s `jmp`.
pub(crate) const ENGINE_WRAPPER: u32 = 0x1004_b8e0;
/// NGP's speed ring (an MSVC `std::deque<float>`: proxy, map, map size, offset, size) and idle table.
const SPEED_RING: u32 = 0x1017_67b0;
const IDLE_TABLE: u32 = 0x1017_67e8;
/// The idle table's lookup (slot 5 of its vtable).
const TABLE_LOOKUP: u32 = 0x1004_a700;
/// NGP's own CRT `operator new` (cdecl: size).
pub(crate) const OPERATOR_NEW: u32 = 0x1011_fe32;

/// `0x1004b8e0` (thiscall on the engine, `(dt, speed)`, `ret 8`, st0): NGP's engine wrapper
/// (`rbr_physics::ngp::engine`) around the ported `Sim8E20`. When the engine dies it emits the engine fire through
/// NGP's emit wrapper ([`engine_dies`]). Declines (before any writes) when the speed ring would grow its map, or for
/// an idle table of another class: neither occurs (`notes/ngp.md`, "Unreachable guards"). A block the ring's new entry
/// needs is allocated first, through NGP's `operator new`, as `push_back` does.
pub(crate) fn engine_wrapper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let engine = r.ecx;
    let [dt, speed, ..] = r.stack.map(f);
    let m = &ctx.mem;
    let car = m.u32(engine + 0x44);
    let health_at = m.u32(engine + 0x48);
    let limit = ngp_engine::limit(m.f32(m.u32(car + 0x850) + 0x188));
    let was_alive = m.u32(health_at + 0x74) != 0;
    let health = ngp_engine::health_step(m.f32(health_at + 0x58), speed, limit, dt);
    let dies = was_alive && ngp_engine::dies(health);
    let alive = was_alive && !dies;
    let flags = m.u32(at(0x1017_67d4).unwrap());
    let table = m.u32(at(IDLE_TABLE).unwrap());
    let ring = at(SPEED_RING).unwrap();
    if alive && flags & 8 != 0 && table != 0 {
        if m.u32(m.u32(table) + 0x14) != at(TABLE_LOOKUP).unwrap() {
            return Err(Decline::Unsupported("NGP's idle table is of another class"));
        }
        let Some((.., block_at)) = ring_slot(m, ring).filter(|_| m.u32(m.u32(ring)) == ring) else {
            return Err(Decline::Unsupported("NGP's speed ring would grow its map"));
        };
        if m.u32(block_at) == 0 {
            let block = ctx.host.call(at(OPERATOR_NEW).unwrap(), 0, &[16]).eax;
            ctx.mem.set_u32(block_at, block);
        }
    }
    if was_alive {
        ctx.mem.set_f32(health_at + 0x58, health);
    }
    if dies {
        engine_dies(ctx, r.esp, health_at);
    }
    apply_atmosphere(&mut ctx.mem, car);
    let sim = crate::engine_driveline::engine_step(ctx, r)?;
    let r_value = sim.st0.expect("Sim8E20 returns st0") as f32;
    if !alive || !crate::engine_driveline::engine_running(ctx, engine) {
        return Ok(Return::float(ngp_engine::result(r_value), 8));
    }
    let m = &ctx.mem;
    let (def, e8) = (m.u32(engine + 4), m.u32(engine + 8));
    let i = TorqueIn {
        speed,
        limit,
        blend: m.f32(engine + 0x20),
        w854_64: m.f32(m.u32(car + 0x854) + 0x64),
        idle_scale: m.f32(def + 0x283c),
        half_stall: m.u32(def + 0x26f8) != 0,
        flags,
        has_table: table != 0,
        braking: m.f32(at(0x1017_234c).unwrap()),
        gain: m.f32(at(0x1017_1aa0).unwrap()),
        stall_speed: m.f32(at(0x1017_1a9c).unwrap()),
        idle_band: m.f32(at(0x1017_1a98).unwrap()),
        density: m.f32(at(0x1017_1ae0).unwrap()),
        friction: m.f32(e8 + 0x270),
        damage: [8, 0x18, 0x28, 0x38, 0x54, 0x58, 0x5c, 0x60].map(|o| m.f32(health_at + o)),
    };
    // The table's two vectors (begin, end), read at knot slots as the lookup asks (`ngp_engine::table_lookup`).
    let [xs, end, ys] = if table != 0 { [8, 0xc, 0x14].map(|o| m.u32(table + o)) } else { [0; 3] };
    let n = (end.wrapping_sub(xs) / 8) as usize;
    let table_mem = m.clone();
    let double = move |at: u32| f64::from_bits(u64::from(table_mem.u32(at)) | u64::from(table_mem.u32(at + 4)) << 32);
    let slot = |base: u32, k: isize| base.wrapping_add((8 * k) as u32);
    let mem = &mut ctx.mem;
    let v = ngp_engine::torque(
        &i,
        r_value,
        || ring_shift(mem, ring, speed, i.stall_speed),
        |x| ngp_engine::table_lookup(n, |k| double(slot(xs, k)), |k| double(slot(ys, k)), x) as f32,
    );
    Ok(Return::float(ngp_engine::result(v), 8))
}

/// The game's signal hub as NGP's getter `0x1002cc00` gives it (`*(*101760d4)`, `*0x893600`), its engine-fire signal
/// (`CMultiSignal1<CSigEngineFire>`, the one `E36DamageOps::EngineGate` emits), and NGP's emit wrapper of that signal.
const SIGNALS: u32 = 0x1017_60d4;
const ENGINE_FIRE_SIGNAL: u32 = 0xb840;
const ENGINE_FIRE_EMIT: u32 = 0x100e_df40;

/// `0x1004b9b3..0x1004ba5c`, the engine's death in the wrapper whose entry `esp` is `esp`: the health object's alive flag
/// (`+0x74`) clears, and NGP emits the engine fire with payload 1.0 (the particles' and the car sound's fire, through
/// their `CSigEngineFire` connections). NGP's code is the emit wrapper's inline (the post on its strand when its
/// singleton's `+0xd` is set, else the fan-out), so the host runs that wrapper: the original's own code in the
/// emulator, and in the pure host the 1.02 emit it stands for (`original_for`), which the host shows as an engine fire.
/// The payload sits where the original keeps it (its `ebp - 0x10`).
fn engine_dies(ctx: &mut Ctx, esp: u32, health_at: u32) {
    ctx.mem.set_u32(health_at + 0x74, 0);
    let payload = esp - 0x14;
    ctx.mem.set_f32(payload, 1.0);
    let signal = ctx.mem.u32(ctx.mem.u32(at(SIGNALS).unwrap())) + ENGINE_FIRE_SIGNAL;
    ctx.host.call(at(ENGINE_FIRE_EMIT).unwrap(), signal, &[payload]);
}

/// The ring's offset and size after dropping its oldest entry, the new entry's index in its block and the map slot of
/// that block (`None` when `push_back` would grow the map).
fn ring_slot(m: &Mem, ring: u32) -> Option<(u32, u32, u32, u32)> {
    let (map, map_size) = (m.u32(ring + 4), m.u32(ring + 8));
    let size = m.u32(ring + 0x10).wrapping_sub(1);
    let off = if size == 0 { 0 } else { m.u32(ring + 0xc).wrapping_add(1) };
    if off.wrapping_add(size) & 3 == 0 && map_size <= size.wrapping_add(4) >> 2 {
        return None;
    }
    let off = off & map_size.wrapping_mul(4).wrapping_sub(1);
    let pos = off.wrapping_add(size);
    Some((off, size, pos & 3, map + 4 * ((pos >> 2) & map_size.wrapping_sub(1))))
}

/// NGP's ring step: `pop_front`, `push_back(speed)`, then whether no entry is above `stall_speed`.
fn ring_shift(m: &mut Mem, ring: u32, speed: f32, stall_speed: f32) -> bool {
    let (off, size, index, block_at) = ring_slot(m, ring).expect("checked before any writes");
    m.set_f32(m.u32(block_at) + 4 * index, speed);
    m.set_u32(ring + 0xc, off);
    m.set_u32(ring + 0x10, size + 1);
    let (map, map_size) = (m.u32(ring + 4), m.u32(ring + 8));
    !(off..off.wrapping_add(size + 1)).any(|k| {
        let block = m.u32(map + 4 * ((k >> 2) & map_size.wrapping_sub(1)));
        m.f32(block + 4 * (k & 3)) > stall_speed
    })
}

/// `0x1005b660` (thiscall on the scratch, 16 words, `ret 0x40`): NGP's wrapper of `CScratch::Step`
/// (`rbr_physics::ngp::step_wrapper`) around the ported `Step`.
fn step_wrapper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let m = &ctx.mem;
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    if settings == 0 {
        return Err(Decline::Unsupported("NGP's settings object is not made yet"));
    }
    let def = m.u32(r.ecx + 0x10);
    let saved: Vec<u32> = (0x204..0x284).step_by(4).map(|o| m.u32(def + o)).collect();
    let p = r.stack[4];
    let k = step_wrapper::speed_factor(m.f32(r.ecx + 0x360));
    let scale = step_wrapper::surface_scale(m.u32(p + 0x148), m.u32(def + 0xc), |o| m.u32(settings + o));
    let words = [0x48, 0x58, 0x78].map(|o| m.u32(p + o));
    for o in [0x48, 0x58, 0x78] {
        ctx.mem.set_f32(p + o, k * ctx.mem.f32(p + o));
    }
    if let Some(s) = scale {
        for o in (0x204..0x244).step_by(4) {
            ctx.mem.set_f32(def + o, ctx.mem.f32(def + o) * s);
        }
    }
    let ret = crate::tyre::step(ctx, r);
    for (o, w) in (0x204..0x284).step_by(4).zip(saved) {
        ctx.mem.set_u32(def + o, w);
    }
    if ret.is_err() {
        // `Step` declined before writing: undo the speed factor too, so the original runs on the entry state.
        for (o, w) in [0x48, 0x58, 0x78].into_iter().zip(words) {
            ctx.mem.set_u32(p + o, w);
        }
    }
    ret
}

/// NGP's game object (`*(*10174ba0)`, `0x10011cb0`), whose `+0x69c` bit 1 enables the progressive bump stop with the
/// settings' bit `0x20000`.
const GAME: u32 = 0x1017_4ba0;

/// `0x10059f70` (thiscall on the strut, 1.02's six words, `ret 0x18`): NGP's wrapper of `CSpringDamper::Step`
/// (`rbr_physics::ngp::spring`) around the ported `Step` and `UpdateForces`. eax is the strut's `+8`.
fn spring_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let strut = r.ecx;
    let [_, x, v, ..] = r.stack.map(f);
    let m = &ctx.mem;
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    let game = m.u32(m.u32(at(GAME).unwrap()));
    if settings == 0 || game == 0 {
        return Err(Decline::Unsupported("NGP's settings or game object is not made yet"));
    }
    let (def, obj) = (m.u32(strut + 4), m.u32(strut + 8));
    let car = m.u32(obj + 0x18);
    let car_def = m.u32(car + 0x34);
    let progressive = m.u32(settings + 0xe38) & 0x20000 != 0 && m.u32(game + 0x69c) & 1 != 0;
    let bump = spring::bump_stop(
        progressive,
        m.f32(def + 0x28),
        x,
        v,
        m.f32(car_def + 0x41c),
        m.f32(car_def + 0x1fc),
        m.f32(def + 0x98),
        m.f32(def + 0xd8),
    );
    let saved = [0x98, 0xd8, 0xe8, 0xf8, 0x118].map(|o| m.u32(def + o));
    let rebound = spring::rebound_scale(m.f32(def + 0x118), m.f32(m.u32(strut + 0x10) + 4));
    let latch = spring::latch(m.f32(def + 0xe8), m.f32(def + 0xf8), v, m.f32(obj + 0x14), rebound);
    let mem = &mut ctx.mem;
    mem.set_f32(strut + 0x30, 0.0);
    match bump {
        Some(b) => {
            mem.set_f32(def + 0x98, b.rate);
            mem.set_f32(def + 0xd8, b.damping);
            mem.set_f32(strut + 0x30, b.ratio);
            mem.set_f32(strut + 0x34, b.rate);
            mem.set_f32(strut + 0x38, b.damping_out);
        }
        None => {
            mem.set_u32(strut + 0x34, saved[0]);
            mem.set_u32(strut + 0x38, saved[1]);
        }
    }
    if mem.f32(def + 0x118) > 0.0 {
        mem.set_f32(def + 0x118, rebound);
    }
    match latch {
        Latch::Compression(limit) => {
            mem.set_f32(obj + 0xc, limit);
            mem.set_u32(def + 0xe8, 0);
        }
        Latch::Rebound(value) => {
            mem.set_f32(obj + 0x10, value);
            mem.set_u32(def + 0xf8, 0);
        }
        Latch::None => {}
    }
    for (k, o) in [0xc, 0x10].into_iter().enumerate() {
        mem.set_u32(car + 0xe80 + 4 * k as u32, mem.u32(obj + o));
    }
    mem.set_u32(car + 0xe88, mem.u32(def + 0xe8));
    mem.set_u32(car + 0xe8c, mem.u32(def + 0xf8));
    let [a0, a1, a2, a3, a4, a5, ..] = r.stack;
    let ngp = damper_step(mem);
    damper_layout::step(mem, &[strut, a0, a1, a2, a3, a4, a5], ngp);
    let restore = if bump.is_some() { 0..5 } else { 2..5 };
    for k in restore {
        mem.set_u32(def + [0x98, 0xd8, 0xe8, 0xf8, 0x118][k], saved[k]);
    }
    damper_layout::update_forces(mem, strut);
    Ok(Return::void(mem.u32(strut + 8), 0x18))
}

/// NGP's edits to 1.02's `CSpringDamper::Step` (0x52e090) with its per-car fade words, None without NGP.
pub(crate) fn damper_step(mem: &Mem) -> Option<rbr_physics::spring_damper::NgpStep> {
    Some(rbr_physics::spring_damper::NgpStep {
        fade_threshold: mem.f32(at(0x1017_2ca8)?),
        fade_scale: mem.f32(at(0x1017_2cac)?),
    })
}

/// The pointer to NGP's settings object (`0x10013cd0` returns `*(*10174bb0)`) and NGP's trampoline to the original
/// `M9BC0` (`0x10045df0`).
const SETTINGS: u32 = 0x1017_4bb0;
const WHEEL_WORKER: u32 = 0x1004_5df0;

fn f(word: u32) -> f32 {
    f32::from_bits(word)
}

/// The float at NGP.dll file address `addr` in its `.data` (as `10171ad8`, the air pressure).
fn data_f32(ctx: &Ctx, addr: u32) -> f32 {
    ctx.mem.f32(at(addr).expect("hooked only with NGP"))
}

/// `0x10060230` (cdecl: state, input, limit, divisor): 1.02's `DamperSolve` with the air pressure as the limit.
fn damper_solve(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [state, input, _, divisor, ..] = r.stack;
    let pressure = data_f32(ctx, 0x1017_1ad8);
    let st0 = damper_layout::solve(&mut ctx.mem, state, f(input), pressure, f(divisor), Variant::Ngp778);
    Ok(Return::float_with_eax(state, f64::from(st0), 0))
}

/// `0x1004d2f0` (cdecl: a, b, c, d).
fn damper_step1(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, ..] = r.stack;
    let t = data_f32(ctx, 0x1017_1adc);
    let st0 = damper::step1(f(a), f(b), f(c), f(d), t);
    Ok(Return::float_with_eax(r.eax, f64::from(st0), 0))
}

/// The scratch's factor (`0x1004d340`) for the scratch in `esi`, and the word it leaves in eax (`*(scratch+0x10)`).
fn scratch_factor(ctx: &Ctx, r: &Regs) -> (f32, u32) {
    let inner = ctx.mem.u32(r.esi + 0x10);
    let w = |o| ctx.mem.f32(inner + o);
    (damper::scratch_factor(w(0x7c), w(0x8c), w(0xac)), inner)
}

/// `0x1004d3c0` (cdecl: a, b, c; the scratch in esi): `DamperStep1` with the scratch's factor as d.
fn damper_step1_scratch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let (factor, eax) = scratch_factor(ctx, r);
    let t = data_f32(ctx, 0x1017_1adc);
    let st0 = damper::step1(f(a), f(b), f(c), factor, t);
    Ok(Return::float_with_eax(eax, f64::from(st0), 0))
}

/// `0x1004d380` (cdecl: a, b, unused c; the scratch in esi).
fn damper_step2(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let (factor, eax) = scratch_factor(ctx, r);
    Ok(Return::float_with_eax(eax, damper::step2(f(a), f(b), factor), 0))
}

/// `0x1005b4a0` (thiscall, `CScratch::ComputeOutputs`'s 17 stack words): the 1.02 function through NGP's call of the
/// original, then its edits of outputs 14 and 16.
fn compute_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let s = &r.stack;
    let input = ctx.mem.load_compute_outputs(r.ecx, s[6], f(s[1]), f(s[2]), f(s[4]), f(s[5]));
    let out = outputs::compute_outputs(&input);
    ctx.mem.store_compute_outputs(std::array::from_fn(|i| s[7 + i]), &out);
    Ok(Return::void(0, 0x44))
}

/// `0x10047300` (thiscall, `M93A0`'s arguments): NGP's rewrite of the clutch-torque worker.
fn clutch_torque(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::engine_driveline::clutch_torque_for(ctx, r, Variant::Ngp778)
}

/// `0x1005e8a0` (stdcall `(blk)`): NGP's rewrite of `M3D230`. Its eax (the CRT's leftovers) isn't modelled.
fn tag_blend(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    tag_blend::update(&mut ctx.mem, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// NGP's per-car axle split (`rbr_physics::ngp::drive`).
const SPLIT: u32 = 0x1017_1a90;

/// NGP's axle split for `M9BC0`'s leaves (`None` without NGP): under NGP `M9BC0` runs only from NGP's trampoline, and its
/// calls of `M5FE0` and `M6260` go to NGP's rewrites.
pub(crate) fn drive_split(ctx: &Ctx) -> Option<f32> {
    at(FILE_BASE).map(|_| data_f32(ctx, SPLIT))
}

/// `0x100458c0` (thiscall `(x)`): NGP's `M5FE0`.
fn wheel_slip_value(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let split = data_f32(ctx, SPLIT);
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| drive::wheel_slip_value(w, x, split));
    Ok(Return::void(0, 4))
}

/// `0x10045b80` (thiscall `(x)`): NGP's `M6260`.
fn wheel_latch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let split = data_f32(ctx, SPLIT);
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| drive::wheel_latch(w, x, split));
    Ok(Return::void(0, 4))
}

/// `0x1004df40` (thiscall `(a, b)` on the slot): NGP's 16 by 16 sheet lookup.
fn sample_slot(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    Ok(Return::void(material::sample_slot(&ctx.mem, r.ecx, f(a), f(b)), 8))
}

/// NGP's per-car wheel constants (`10171300`, `10171304`, `10171308`; the last NGP also writes into the ground
/// probe's push at `0x53daa6`).
pub(crate) fn wheel_tuning(ctx: &Ctx) -> WheelTuning {
    WheelTuning {
        fill: data_f32(ctx, 0x1017_1300),
        weight: data_f32(ctx, 0x1017_1304),
        pose: data_f32(ctx, 0x1017_1308),
    }
}

/// The ground probe's tuning under NGP (`E51Wheel::M3D520`): NGP's collision system, through the variable its
/// patches point the probe's `g_7c755c` operands at (read from the patched operand at `0x53d5e0`; the variable is
/// on the heap), and the wheel constants.
pub(crate) fn probe_tuning(ctx: &Ctx) -> ProbeTuning {
    ProbeTuning { collision: ctx.mem.u32(ctx.mem.u32(0x0053_d5e0)), wheel: wheel_tuning(ctx) }
}

/// `0x1004d410` (cdecl `(a, b, c, _)`, the scratch in esi): `DamperStep1` with NGP's convection term. Its eax (the
/// CRT `pow`'s leftovers) isn't modelled.
fn convection(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let n = thermal(ctx, r.esi).expect("hooked only with NGP");
    let st0 = damper::convection(f(a), f(b), f(c), &n.convection, n.temperature);
    Ok(Return::float_with_eax(0, f64::from(st0), 0))
}
