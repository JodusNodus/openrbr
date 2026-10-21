//! Typed views of the game memory the VCU reads and writes: the setup (`CVehicleControlUnit::CDef`,
//! 0xcac bytes), the unit's state (`CState`, 0x68 bytes) and the per-step scratch (`CScratchSim`).
//! Offsets are the original's (rbr-recomp `notes/physics/renames/vcu.csv`, `notes/physics.md` "VCU"). The `load`
//! and `store` functions below are the only code that knows them; the logic works on the fields.

use super::store::VcuStore;
use crate::mem::Mem;
use crate::store::Real;

/// Knots in a pedal map and in a speed map (the VCU always passes 11).
pub const KNOTS: usize = 11;
/// Forward gears with a shift threshold in the setup (`Gear0`..`Gear9`).
pub const GEARS: usize = 10;

/// `CVehicleControlUnit::CDef` members (rbr-recomp `notes/physics/renames/vcu.csv`). The per-gear rpm rows are 0x20
/// bytes apart.
const GEAR_UPSHIFT_RPM: u32 = 0x18;
const GEAR_DOWNSHIFT_RPM: u32 = 0x28;
const SHIFT_LOCKOUT_UP_S: u32 = 0x158;
const SHIFT_LOCKOUT_DOWN_S: u32 = 0x168;
const CLUTCH_RAMP_RATE: u32 = 0x178;
const RPM_LIMIT: u32 = 0x188;
const CENTRE_DIFF_HANDBRAKE_RELEASE: u32 = 0x198;
const LEFT_FOOT_BRAKE_THRESHOLD: u32 = 0x1a8;
/// CReal headers of the knot maps (the value is at +0xc; pedal knots are 0x10 bytes apart, speed records 0x20).
const CENTRE_THROTTLE_LOCK: u32 = 0x1ac;
const CENTRE_BRAKE_LOCK: u32 = 0x25c;
const CENTRE_SPEED_LOCK: u32 = 0x30c;
const LF_CENTRE_THROTTLE_LOCK: u32 = 0x46c;
const LF_CENTRE_BRAKE_LOCK: u32 = 0x51c;
const LF_CENTRE_SPEED_LOCK: u32 = 0x5cc;
const FRONT_THROTTLE_LOCK: u32 = 0x72c;
const FRONT_BRAKE_LOCK: u32 = 0x7dc;
const FRONT_SPEED_LOCK: u32 = 0x88c;
const REAR_THROTTLE_LOCK: u32 = 0x9ec;
const REAR_BRAKE_LOCK: u32 = 0xa9c;
const REAR_SPEED_LOCK: u32 = 0xb4c;

crate::object! {
    /// `CVehicleControlUnit::CState` (rbr-recomp `notes/physics/renames/vcu.csv`; the scratch's `m_state`). The words
    /// the constructor clears and the state's car; the size is the end of the last known field.
    pub struct StateObj, size 0x6c {
        vtable @ 0x00: u32,
        shift_lockout @ 0x2c: f32,
        /// The launch clutch's CReal: its dirty word (`+0x48`) and value (`+0x4c`).
        launch_clutch @ 0x40: Real,
        w50 @ 0x50: u32,
        w54 @ 0x54: u32,
        pedal_swap @ 0x58: u32,
        w5c @ 0x5c: u32,
        gear_timer @ 0x60: f32,
        shift_cooldown @ 0x64: f32,
        owner @ 0x68: u32,
    }
}

crate::object! {
    /// `CScratchSim` (rbr-recomp `notes/physics/renames/vcu.csv`; the car's `+0x850`). The size is the end of the last
    /// known field.
    pub struct ScratchObj, size 0x60 {
        def @ 0x00: u32,
        state @ 0x04: u32,
        vtable @ 0x08: u32,
        throttle @ 0x0c: f32,
        brake @ 0x10: f32,
        handbrake @ 0x14: f32,
        unknown_18 @ 0x18: u32,
        fixed_clutch @ 0x1c: f32,
        launch_enabled @ 0x40: u32,
        slip_gate_rpm @ 0x44: u32,
        slip_gate_speed @ 0x48: u32,
        launch_mode @ 0x4c: u32,
        vcu_enabled @ 0x50: u32,
        engine @ 0x54: u32,
        drive @ 0x58: u32,
        lights @ 0x5c: u32,
    }
}

const SHIFT_LOCKOUT: u32 = StateObj::shift_lockout.at;
const LAUNCH_CLUTCH_DIRTY: u32 = StateObj::launch_clutch.at + 8;
const LAUNCH_CLUTCH: u32 = StateObj::launch_clutch.at + 12;
const PEDAL_SWAP: u32 = StateObj::pedal_swap.at;
const GEAR_TIMER: u32 = StateObj::gear_timer.at;
const SHIFT_COOLDOWN: u32 = StateObj::shift_cooldown.at;

/// The drive state's gear fields the VCU reads (`CDrive::CState`, rbr-recomp
/// `notes/physics/renames/engine-driveline.csv`).
const DRIVE_STATE_GEAR: u32 = 0x54c;
const DRIVE_STATE_COMMANDED_GEAR: u32 = 0x570;

const SCRATCH_DEF: u32 = ScratchObj::def.at;
const SCRATCH_STATE: u32 = ScratchObj::state.at;
const SCRATCH_THROTTLE: u32 = ScratchObj::throttle.at;
const SCRATCH_BRAKE: u32 = ScratchObj::brake.at;
const SCRATCH_HANDBRAKE: u32 = ScratchObj::handbrake.at;
const SCRATCH_UNKNOWN_18: u32 = ScratchObj::unknown_18.at;
const SCRATCH_FIXED_CLUTCH: u32 = ScratchObj::fixed_clutch.at;
const SCRATCH_LAUNCH_ENABLED: u32 = ScratchObj::launch_enabled.at;
const SCRATCH_SLIP_GATE_RPM: u32 = ScratchObj::slip_gate_rpm.at;
const SCRATCH_SLIP_GATE_SPEED: u32 = ScratchObj::slip_gate_speed.at;
const SCRATCH_LAUNCH_MODE: u32 = ScratchObj::launch_mode.at;
const SCRATCH_VCU_ENABLED: u32 = ScratchObj::vcu_enabled.at;
const SCRATCH_ENGINE: u32 = ScratchObj::engine.at;
const SCRATCH_DRIVE: u32 = ScratchObj::drive.at;
const SCRATCH_LIGHTS: u32 = ScratchObj::lights.at;

/// One knot of a pedal map. Records are 0x10 bytes apart; the value is at +0x0c.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Knot {
    pub value: f32,
}

/// One record of a speed map. Records are 0x20 bytes apart; the speed key is at +0x0c and the
/// value at +0x1c.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeedKnot {
    pub key: f32,
    pub value: f32,
}

/// The constants NGP repoints to its per-car `.data` (rbr-recomp `notes/ngp.md`, "VCU"): the shift cooldown (0.05 at 0x72dd78 in
/// GearboxUpdate and FlagDispatch), the launch rpm (5000, 0x731054), the launch clutch's band (200, 275 and
/// -1/75 at 0x730890, 0x741380, 0x74137c) and the rev cut's margin over the limit (300 at 0x72f5e0, RevLimitCut).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    pub cooldown: f32,
    pub launch_rpm: f32,
    pub clutch_from: f32,
    pub clutch_to: f32,
    pub clutch_slope: f32,
    pub cut_margin: f32,
}

impl Tuning {
    pub const STOCK: Tuning = Tuning {
        cooldown: 0.05,
        launch_rpm: 5000.0,
        clutch_from: 200.0,
        clutch_to: 275.0,
        clutch_slope: -0.013333334,
        cut_margin: 300.0,
    };
}

/// The setup object (`CDef`). Only the fields the ported functions read are here.
#[derive(Clone, Debug, PartialEq)]
pub struct Def {
    /// Upshift when the engine rpm is above this (0 = off), per gear.
    pub gear_upshift_rpm: [f32; GEARS],
    /// Downshift when the engine rpm is below this (0 = off), per gear.
    pub gear_downshift_rpm: [f32; GEARS],
    /// Shift lockout after an up request, seconds.
    pub shift_lockout_up_s: f32,
    /// Shift lockout after a down request, seconds.
    pub shift_lockout_down_s: f32,
    /// Launch clutch fall rate, per second.
    pub clutch_ramp_rate: f32,
    /// Rev limit, rpm.
    pub rpm_limit: f32,
    /// Centre lock is 0 while the handbrake is at or above this.
    pub centre_diff_handbrake_release: f32,
    /// Left-foot maps are used above this pedal value (0 = off).
    pub left_foot_brake_threshold: f32,
    /// Number of gears (`NumberOfGears`).
    pub gear_count: u32,
    pub centre_throttle_lock: Vec<Knot>,
    pub centre_brake_lock: Vec<Knot>,
    pub centre_speed_lock: Vec<SpeedKnot>,
    pub lf_centre_throttle_lock: Vec<Knot>,
    pub lf_centre_brake_lock: Vec<Knot>,
    pub lf_centre_speed_lock: Vec<SpeedKnot>,
    pub front_throttle_lock: Vec<Knot>,
    pub front_brake_lock: Vec<Knot>,
    pub front_speed_lock: Vec<SpeedKnot>,
    pub rear_throttle_lock: Vec<Knot>,
    pub rear_brake_lock: Vec<Knot>,
    pub rear_speed_lock: Vec<SpeedKnot>,
}

/// Three consecutive words of a knot record, the ones CopyDef moves. In a pedal map's record they sit at
/// +0x4, +0x8 and +0xc, and the knot value is `last`. A speed map's record holds two such triples: the key is
/// the last word of the first (+0xc) and the value the last of the second (+0x1c). `first` and `second` have
/// no known meaning.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KnotWords {
    pub first: u32,
    pub second: u32,
    pub last: u32,
}

impl KnotWords {
    fn load(mem: &Mem, at: u32) -> Self {
        KnotWords { first: mem.u32(at), second: mem.u32(at + 0x04), last: mem.u32(at + 0x08) }
    }

    fn store(&self, mem: &mut Mem, at: u32) {
        mem.set_u32(at, self.first);
        mem.set_u32(at + 0x04, self.second);
        mem.set_u32(at + 0x08, self.last);
    }
}

/// A pedal map's triples, one per knot.
pub type PedalWords = [KnotWords; KNOTS];

/// A speed map's triples: the keys (the first triple of each record) and the values (the second).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeedWords {
    pub keys: [KnotWords; KNOTS],
    pub values: [KnotWords; KNOTS],
}

fn load_triples(mem: &Mem, at: u32, stride: u32) -> [KnotWords; KNOTS] {
    std::array::from_fn(|k| KnotWords::load(mem, at + stride * k as u32))
}

fn store_triples(mem: &mut Mem, at: u32, stride: u32, triples: &[KnotWords; KNOTS]) {
    for (k, triple) in triples.iter().enumerate() {
        triple.store(mem, at + stride * k as u32);
    }
}

fn store_speed(mem: &mut Mem, keys: u32, values: u32, speed: &SpeedWords) {
    store_triples(mem, keys, 0x20, &speed.keys);
    store_triples(mem, values, 0x20, &speed.values);
}

/// The knot maps CopyDef copies from the setup into the CDef. In the setup each triple of a map is 0x10 bytes
/// from the next knot's; in the CDef a pedal triple is 0x10 and a speed triple 0x20 bytes from the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnotMaps {
    pub centre_throttle: PedalWords,
    pub centre_brake: PedalWords,
    pub lf_centre_throttle: PedalWords,
    pub lf_centre_brake: PedalWords,
    pub front_throttle: PedalWords,
    pub front_brake: PedalWords,
    pub rear_throttle: PedalWords,
    pub rear_brake: PedalWords,
    pub centre_speed: SpeedWords,
    pub lf_centre_speed: SpeedWords,
    pub front_speed: SpeedWords,
    pub rear_speed: SpeedWords,
}

/// The setup's words that CopyDef reads outside the knot maps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SetupFlags {
    /// `+0x18`: the launch flag. The scratch's launch and VCU flags take it.
    pub launch: u32,
    /// `+0x28`: the rpm slip gate.
    pub slip_gate_rpm: u32,
    /// `+0x38`: the speed slip gate.
    pub slip_gate_speed: u32,
    /// `+0x48`: the launch mode when the launch flag is off.
    pub launch_mode_when_off: u32,
}

impl SetupFlags {
    /// The scratch's launch mode (`+0x4c`): 1 when the launch flag is set, else `launch_mode_when_off`.
    pub fn launch_mode(&self) -> u32 {
        if self.launch != 0 {
            1
        } else {
            self.launch_mode_when_off
        }
    }
}

/// The unit's state (`CState`, the scratch's `m_state`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    /// Shift requests are blocked while this is nonzero (seconds).
    pub shift_lockout: f32,
    /// Launch clutch value sent to the drive (0..1).
    pub launch_clutch: f32,
    /// Dirty flag of the launch clutch's value object (`+0x48`), set on every clutch write.
    pub launch_clutch_dirty: u32,
    /// Pedal routing flag: 1 swaps the pedals sent to the engine and the drive (the asm stores an integer).
    pub pedal_swap: u32,
    /// Counts for the neutral and second-gear low-speed rules (seconds).
    pub gear_timer: f32,
    /// Set to 0.05 s on each request; read by the rev-limit cut (seconds).
    pub shift_cooldown: f32,
}

/// Engine speed (rad/s) to rpm, the asm's `9.5492964f`.
pub const RPM_PER_RAD_S: f32 = 9.549296;

/// The drive's gear fields (the `CDrive + 8` sub-object): the gear in use, the gear request (its
/// `+0x564` block) and the slip flag that `CheckSlip` reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gearbox {
    /// Gear in use (`+0x54c`).
    pub gear: u32,
    /// Commanded gear (`+0x570`, the request's gear).
    pub commanded: u32,
    /// Set by every request (`+0x56c`).
    pub changed: u32,
    /// Slip flag (`+0x55c`).
    pub slip: u32,
}

/// The drive's CReal outputs the step sets. `CDrive::Set44A0` (+0x20) holds the throttle and `Set30` (+0x30)
/// the brake; `Set30` also writes the brake times `brake_scale` (the definition's `+0x620`) to +0x64, and
/// `Set4820` (+0x54) the clutch slip. A write sets the object's dirty flag (+8, to 1) and its value (+0xc).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reals {
    pub throttle: f32,
    pub brake: f32,
    pub brake_scaled: f32,
    pub clutch_slip: f32,
    /// Read by `Set30`, not stored here.
    pub brake_scale: f32,
}

/// What the step writes into the drive: the diff locks (`Set4570`) and the CReals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outputs {
    pub locks: Locks,
    pub reals: Reals,
}

/// The drive definition's per-gear ratios that `E50_QuerySlip` reads: `lim` at +0x644 and `div` at +0x694,
/// 0x50 bytes per gear.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GearRatios {
    pub lim: [f32; GEARS],
    pub div: [f32; GEARS],
}

/// The drive's diff-lock channels, as `CDrive::Set4570` reads and writes them: each channel's record in the drive's
/// D8 object (0x14 bytes: `ratio` at +0x0c, `torque` at +0x18), the gate at `drive + 0x170` (its `+0x90`), and each
/// channel's `max_torque` in the drive's definition (`drive + 4`, `+0x58` with 0x60 per channel).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Locks {
    pub ratio: [f32; 3],
    pub enabled: bool,
    pub max_torque: [f32; 3],
    pub torque: [f32; 3],
}

/// The driver's pedals as the scratch holds them (0..1; the handbrake too).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pedals {
    pub throttle: f32,
    pub brake: f32,
    pub handbrake: f32,
}

/// The request packet the car fills in each step, as `CScratchSim::ReadPacket` reads it. The pedals and the
/// fixed clutch go to the scratch; the flags and the gear value go to FlagDispatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Packet {
    pub pedals: Pedals,
    pub fixed_clutch: f32,
    /// The word at +0x10, copied to the scratch's `unknown_18`. Meaning unknown.
    pub unknown_10: u32,
    /// The request flags (+0x14): 1 upshift, 2 downshift, 4 set the gear to `value`.
    pub flags: u8,
    /// The gear for the set flag (+0x18).
    pub value: u32,
}

/// The per-step scratch (`CScratchSim`, `car + 0x850`). The pointers are the game's objects.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scratch {
    /// `CVehicleControlUnit::CDef`.
    pub def: u32,
    /// `CVehicleControlUnit::CState`.
    pub state: u32,
    pub pedals: Pedals,
    /// Fixed clutch sent when the launch mode is off (copied from the packet by ReadPacket).
    pub fixed_clutch: f32,
    /// Copied from the packet's `unknown_10` by ReadPacket. Meaning unknown.
    pub unknown_18: u32,
    /// The gear logic and the launch step run only when this is nonzero.
    pub launch_enabled: u32,
    /// Selects the launch clutch logic (`UpdateC`): set from the setup's `+0x18`, or else its `+0x48`.
    pub launch_mode: u32,
    /// The gear logic runs only when this is nonzero.
    pub vcu_enabled: u32,
    /// The rpm slip gate (`+0x44`, copied from the setup by CopyDef): the slip query checks it.
    pub slip_gate_rpm: u32,
    /// The speed slip gate (`+0x48`, copied from the setup by CopyDef): the slip query checks it.
    pub slip_gate_speed: u32,
    pub engine: u32,
    pub drive: u32,
    pub lights: u32,
}

/// `CVehicleControlUnit::CDef`'s vtable (0x7412dc) and the name its base gets (`"ControlUnit"`, 0x7380c8).
const DEF_VTABLE: u32 = 0x0074_12DC;
const DEF_NAME: u32 = 0x0073_80C8;
/// The definition's size: its tweaks are all reals, 0x10 apart from +0xc (`CReal2A`, `CReal2B` and the arrays are
/// pairs and runs of `NTweak::CReal`).
pub const DEF_SIZE: u32 = 0xCAC;
/// The reals the constructor sets (dirty, then the value): +0x14c..+0x17c, from the float words at 0x72f674, 0x72f428,
/// 0x72d150 and 0x731050.
const DEF_DEFAULTS: [(u32, u32); 4] =
    [(0x14C, 0x3F33_3333), (0x15C, 0x3E4C_CCCD), (0x16C, 0x4080_0000), (0x17C, 0x45F3_C000)];

/// `CVehicleControlUnit::CDef::CDef` (0x5377a0) on the definition at `this`: the base, the vtable, every real, then
/// the four defaults.
pub fn construct_def(mem: &mut Mem, this: u32) {
    crate::ntweak::construct_base(mem, this, DEF_NAME);
    mem.set_u32(this, DEF_VTABLE);
    for at in (0xC..DEF_SIZE).step_by(0x10) {
        crate::ntweak::construct_real(mem, this + at);
    }
    for (at, value) in DEF_DEFAULTS {
        crate::ntweak::set_dirty(mem, this + at);
        mem.set_u32(this + at + 0xC, value);
    }
}

/// `CVehicleControlUnit::CState`'s vtable (0x74132c) and `CScratchSim`'s (0x7412d0, at its +8; slot 1 returns).
const STATE_VTABLE: u32 = 0x0074_132C;
const SCRATCH_SIM_VTABLE: u32 = 0x0074_12D0;

/// `CVehicleControlUnit::CState::CState` (0x536d50, thiscall, three stack arguments it does not read) on the state at
/// `this`: the base named like the definition, the vtable, its tweaks (a real, an unsigned, two reals) and the cleared
/// words.
pub fn construct_state(mem: &mut Mem, this: u32) {
    crate::ntweak::construct_base(mem, this, DEF_NAME);
    mem.set_u32(this, STATE_VTABLE);
    crate::ntweak::construct_real(mem, this + 0xC);
    crate::ntweak::construct_unsigned(mem, this + 0x1C);
    crate::ntweak::construct_real(mem, this + 0x30);
    crate::ntweak::construct_real(mem, this + 0x40);
    for at in [0x50, 0x54, 0x5C, 0x60, 0x58, 0x2C, 0x64] {
        mem.set_u32(this + at, 0);
    }
}

/// The objects `CScratchSim` keeps (its `+0x54..+0x5c`): the car's engine, drive and light system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimParts {
    pub engine: u32,
    pub drive: u32,
    pub lights: u32,
}

/// `CVehicleControlUnit::CScratchSim::CScratchSim(def, state, owner, engine, drive, lights)` (0x536e50, thiscall) on
/// the scratch at `this`: the vtable at +8, the cleared words, the parts, the definition and state, the owner into the
/// state (`+0x68`) when there is a definition, then the counters (+0x40 is 1).
pub fn construct_scratch_sim(mem: &mut Mem, this: u32, def: u32, state: u32, owner: u32, parts: SimParts) {
    mem.set_u32(this + 8, SCRATCH_SIM_VTABLE);
    for at in [0x10, 0x14, 0x18, 0xC] {
        mem.set_u32(this + at, 0);
    }
    mem.set_u32(this + 0x54, parts.engine);
    mem.set_u32(this + 0x58, parts.drive);
    mem.set_u32(this + 0x5C, parts.lights);
    mem.set_u32(this, def);
    mem.set_u32(this + 4, state);
    if def != 0 {
        mem.set_u32(state + 0x68, owner);
    }
    for at in [0x4C, 0x44, 0x48] {
        mem.set_u32(this + at, 0);
    }
    mem.set_u32(this + 0x40, 1);
}

/// `CVehicleControlUnit::CSetup`'s vtable (0x72e534) and its size (its last real ends at +0xb6c).
const SETUP_VTABLE: u32 = 0x0072_E534;
const SETUP_SIZE: u32 = 0xB6C;

/// `CVehicleControlUnit::CSetup::CSetup` (0x47b720) on the setup at `this`: an unnamed tweak, its vtable, unsigneds at
/// +0xc..+0x3c (`AutoGears`, `GearGuard`, `NeutralLock`, `ClutchHelp`), then reals from +0x4c on (the two thresholds
/// and the maps).
pub fn construct_setup(mem: &mut Mem, this: u32) {
    crate::ntweak::construct_base(mem, this, 0);
    mem.set_u32(this, SETUP_VTABLE);
    for at in (0xC..0x4C).step_by(0x10) {
        crate::ntweak::construct_unsigned(mem, this + at);
    }
    for at in (0x4C..SETUP_SIZE).step_by(0x10) {
        crate::ntweak::construct_real(mem, this + at);
    }
}

/// `CVehicleControlUnit::CSetupOptions`'s vtable (0x72e7c4).
const OPTIONS_VTABLE: u32 = 0x0072_E7C4;

/// `CVehicleControlUnit::CSetupOptions::CSetupOptions` (0x47d710) on the options at `this`: an unnamed tweak, its
/// vtable, and four ranges (`E13RealTriple`s: the diff map's lock and speed, the handbrake release and the left-foot
/// threshold) from +0xc, every real 0x10 apart.
pub fn construct_options(mem: &mut Mem, this: u32) {
    crate::ntweak::construct_base(mem, this, 0);
    mem.set_u32(this, OPTIONS_VTABLE);
    for at in (0xC..0xCC).step_by(0x10) {
        crate::ntweak::construct_real(mem, this + at);
    }
}

/// The setup's unsigneds from +0xc (`AutoGears`, `GearGuard`, `NeutralLock`, `ClutchHelp`) as the settings code stores
/// the player's driver aids there, each marked dirty first (`CCarSettingsManager::LoadGlobal` 0x547080 and `CommitGlobal`
/// 0x5471a0 the first three, the gears page's `vf48` 0x5486c0 the last).
pub fn set_driver_aids(mem: &mut Mem, this: u32, aids: [u32; 4]) {
    for (k, value) in aids.into_iter().enumerate() {
        let at = this + 0xC + 0x10 * k as u32;
        crate::ntweak::set_dirty(mem, at);
        mem.set_u32(at + 0xC, value);
    }
}

/// The definition's part of `CScratchSim::CopyDef` (0x537c90): the setup's knot maps into the definition at `def`
/// (`copy_def` does it through a scratch, with the scratch's own fields).
pub fn copy_maps<S: VcuStore>(mem: &mut S, def: u32, setup: u32) {
    let maps = mem.knot_maps(setup);
    mem.store_knot_maps(def, &maps);
}

/// `CScratchSim::CopyDef` (`CopySetup`, 0x537c90): copies the setup at `setup`'s knot maps into the CDef of the
/// scratch at `scratch`, then sets the launch and slip fields of the scratch. The copies are dword moves, so the bytes
/// are the original's whatever the values. The asm's order decides the writes (`notes/physics.md` "VCU"); the C++ in
/// `src/rbr` has them in a different order, which the asm decides.
pub fn copy_def<S: VcuStore>(mem: &mut S, scratch: u32, setup: u32) {
    let fields = mem.scratch(scratch);
    let maps = mem.knot_maps(setup);
    mem.store_knot_maps(fields.def, &maps);
    let flags = mem.setup_flags(setup);
    let value = Scratch {
        launch_enabled: flags.launch,
        vcu_enabled: flags.launch,
        launch_mode: flags.launch_mode(),
        slip_gate_rpm: flags.slip_gate_rpm,
        slip_gate_speed: flags.slip_gate_speed,
        ..fields
    };
    mem.store_scratch(scratch, &value);
}

/// `CScratchSim::ReadPacket` (0x537f20): copies the packet's pedals, fixed clutch and `unknown_10` into the
/// scratch, then returns the packet. The asm tail-jumps into FlagDispatch, so the caller applies the packet
/// with `super::dispatch::flag_dispatch`.
pub fn read_packet<S: VcuStore>(mem: &mut S, scratch: u32, packet_at: u32) -> Packet {
    let packet = mem.packet(packet_at);
    let value = Scratch {
        pedals: packet.pedals,
        fixed_clutch: packet.fixed_clutch,
        unknown_18: packet.unknown_10,
        ..mem.scratch(scratch)
    };
    mem.store_scratch(scratch, &value);
    packet
}

/// The original functions the VCU's entries call (thiscall each): the drive's, the engine's, the car's and the lights'
/// accessors and setters, and the slip query. The bindings make them through the host.
pub trait VcuCalls {
    /// `CDrive::Get4520`: the engine speed (rad/s).
    fn engine_speed(&mut self, drive: u32) -> f32;
    /// `CDrive::Get4540`: the diff-lock speed key.
    fn lock_speed(&mut self, drive: u32) -> f32;
    /// `CCar::Get15F8`: the car speed.
    fn car_speed(&mut self, car: u32) -> f32;
    /// `CDrive::Set4570 (channel, value)`, eax.
    fn set_diff_lock(&mut self, drive: u32, channel: u32, value: f32) -> u32;
    /// `CDrive::Set4820 (value)`: the clutch slip, eax.
    fn set_clutch_slip(&mut self, drive: u32, value: f32) -> u32;
    /// `CEngine::SetState4`: the four cut flags, eax.
    fn set_cut_flags(&mut self, engine: u32, flags: [u32; 4]) -> u32;
    /// `CEngine::SetScratch08`: the pedal.
    fn set_engine_pedal(&mut self, engine: u32, value: f32);
    /// `Lights::Set1e40`: the brake light.
    fn set_brake_light(&mut self, lights: u32, on: bool);
    /// `Lights::Set1ed0`: the neutral light.
    fn set_neutral_light(&mut self, lights: u32, on: bool);
    /// `E50_QuerySlip` (fastcall on the scratch), as FlagDispatch calls it.
    fn query_slip(&mut self, scratch: u32) -> bool;
    /// `NTweak::CBase::SetDirty` on `object`, as `CDrive::M6840` calls it.
    fn set_dirty(&mut self, object: u32);
}

// The VCU functions' sequences, one per original entry, generic over the store (moved from the bindings in
// `rbr-hybrid/src/vcu.rs`, which decode the registers and call these).

/// The scratch's drive (`+0x58`): the object the drive accessors and setters take.
fn drive_of<S: VcuStore>(mem: &S, this: u32) -> u32 {
    mem.u32(this + ScratchObj::drive.at)
}

/// The drive's gear block (`CDrive::Get08`, `[drive + 8]`).
fn gears_at<S: VcuStore>(mem: &S, drive: u32) -> u32 {
    mem.u32(drive + 8)
}

/// The state's car (`m_state + 0x68`).
fn state_car<S: VcuStore>(mem: &S, state: u32) -> u32 {
    mem.u32(state + 0x68)
}

/// `E49Proxy::VcuTableBlend (table, x, lo, hi, n)`, st0, for `n` from 2 (the VCU always passes 11).
pub fn table_blend<S: VcuStore>(mem: &S, table: u32, x: f32, lo: f32, hi: f32, n: u32) -> f64 {
    super::tables::table_blend(&mem.pedal_map(table, n as usize), x, lo, hi)
}

/// `NPhysics::LookupPairTable (table, x, n)`, st0, for `n` from 1.
pub fn lookup_pair_table<S: VcuStore>(mem: &S, table: u32, x: f32, n: u32) -> f64 {
    super::tables::lookup_pair_table(&mem.speed_map(table, n as usize), x)
}

/// `CScratchSim::LockMax (t1, t2, t3, f)`, st0: the largest of the throttle, brake and speed maps.
pub fn lock_max<S: VcuStore>(mem: &S, this: u32, t: [u32; 3], f: f32) -> f32 {
    super::locks::lock_max(
        mem.pedals(this),
        &mem.pedal_map(t[0], KNOTS),
        &mem.pedal_map(t[1], KNOTS),
        &mem.speed_map(t[2], KNOTS),
        f,
    )
}

/// `CScratchSim::DiffLocks`: the three diff locks, sent to the drive's `Set4570`; eax is the last send's.
pub fn diff_locks<S: VcuStore>(mem: &S, calls: &mut impl VcuCalls, this: u32) -> u32 {
    let drive = drive_of(mem, this);
    let speed = calls.lock_speed(drive);
    let def = mem.def(mem.u32(this + ScratchObj::def.at));
    let values = super::locks::diff_locks(mem.pedals(this), &def, speed);
    let mut eax = 0;
    for (channel, value) in values.into_iter().enumerate() {
        eax = calls.set_diff_lock(drive, channel as u32, value);
    }
    eax
}

/// `E50_QuerySlip`: the slip query before a downshift. The engine speed and the car speed come from their getters,
/// each called only where its gate reads it, as the original does.
pub fn query_slip<S: VcuStore>(mem: &S, calls: &mut impl VcuCalls, this: u32) -> bool {
    let scratch = mem.scratch(this);
    let gear = mem.gearbox(gears_at(mem, scratch.drive)).gear;
    let gears = mem.gear_ratios(scratch.drive);
    let rpm_limit = mem.def(scratch.def).rpm_limit;
    let engine = if scratch.slip_gate_rpm != 0 && gear as i32 > 2 { calls.engine_speed(scratch.drive) } else { 0.0 };
    let car_speed =
        if scratch.slip_gate_speed != 0 && gear == 2 { calls.car_speed(state_car(mem, scratch.state)) } else { 0.0 };
    super::slip::query_slip(scratch.slip_gate_rpm, scratch.slip_gate_speed, gear, &gears, rpm_limit, engine, car_speed)
}

/// `CScratchSim::RevLimitCut`: the rev-limit cut flags, sent to the engine's `SetState4`; eax is its.
pub fn rev_limit_cut<S: VcuStore>(mem: &S, calls: &mut impl VcuCalls, this: u32, margin: f32) -> u32 {
    let engine_speed = calls.engine_speed(drive_of(mem, this));
    let def = mem.def(mem.u32(this + ScratchObj::def.at));
    let state = mem.state(mem.u32(this + ScratchObj::state.at));
    let flags = super::cut::rev_limit_cut(engine_speed, &def, &state, margin);
    calls.set_cut_flags(mem.u32(this + ScratchObj::engine.at), flags)
}

/// `CScratchSim::CheckSlip (x)`: the slip gate.
pub fn check_slip<S: VcuStore>(mem: &S, this: u32, x: f32) -> bool {
    let slip = mem.gearbox(gears_at(mem, drive_of(mem, this))).slip;
    super::cut::check_slip(mem.scratch(this).vcu_enabled, slip, x)
}

/// The engine speed and the car speed the gear logic reads, in its order (`CDrive::Get4520`, then `CCar::Get15F8` on
/// the state's car), or None where it returns before reading them: the launch flag off, or a shift under way.
fn gear_speeds<S: VcuStore>(
    mem: &S,
    calls: &mut impl VcuCalls,
    scratch: &Scratch,
    gears: &Gearbox,
) -> Option<(f32, f32)> {
    if scratch.launch_enabled == 0 || gears.gear != gears.commanded {
        return None;
    }
    let engine = calls.engine_speed(scratch.drive);
    Some((engine, calls.car_speed(state_car(mem, scratch.state))))
}

/// `CScratchSim::GearboxUpdate (dt)`: the gear logic. With the launch flag off, or a shift under way, it returns before
/// any call. The requests' `CDrive::Set47B0` writes (`SetDirty` and the commanded gear) are the gear block it stores.
pub fn gearbox_update<S: VcuStore>(mem: &mut S, calls: &mut impl VcuCalls, this: u32, dt: f32, tuning: Tuning) {
    let scratch = mem.scratch(this);
    let gears_at = gears_at(mem, scratch.drive);
    let mut gears = mem.gearbox(gears_at);
    let (engine, speed) = gear_speeds(mem, calls, &scratch, &gears).unwrap_or((0.0, 0.0));
    let input = super::gear::Input {
        launch_enabled: scratch.launch_enabled,
        vcu_enabled: scratch.vcu_enabled,
        pedals: scratch.pedals,
        engine,
        speed,
        dt,
        gear_count: mem.gear_count(scratch.drive),
        tuning,
    };
    let def = mem.def(scratch.def);
    let mut state = mem.state(scratch.state);
    super::gear::update(input, &def, &mut state, &mut gears);
    mem.store_state(scratch.state, &state);
    mem.store_gearbox(gears_at, &gears);
}

/// `CScratchSim::LaunchClutch (dt)`: the launch clutch, sent to the drive's `Set4820` (nothing with the launch mode
/// off); eax is the send's, else 0.
pub fn launch_clutch<S: VcuStore>(mem: &mut S, calls: &mut impl VcuCalls, this: u32, dt: f32, tuning: Tuning) -> u32 {
    let scratch = mem.scratch(this);
    if scratch.launch_mode == 0 {
        return 0;
    }
    let drive = scratch.drive;
    let input =
        super::clutch::Input { launch_mode: scratch.launch_mode, engine: calls.engine_speed(drive), dt, tuning };
    let gears = mem.gearbox(gears_at(mem, drive));
    let def = mem.def(scratch.def);
    let mut state = mem.state(scratch.state);
    let sent = super::clutch::launch_clutch(input, &gears, &def, &mut state);
    mem.store_state(scratch.state, &state);
    match sent {
        Some(value) => calls.set_clutch_slip(drive, value),
        None => 0,
    }
}

/// FlagDispatch's body: the packet's flags as gear requests; whether anything was requested. The drive setters it
/// calls (`Set47B0`, `M6840`) write the gear block it stores; the slip query reads the gear in use, the tables and the
/// speeds, none of which an upshift request changes, so the stores can follow it.
pub fn apply_packet<S: VcuStore>(
    mem: &mut S,
    calls: &mut impl VcuCalls,
    this: u32,
    packet: Packet,
    tuning: &Tuning,
) -> bool {
    let scratch = mem.scratch(this);
    let drive = scratch.drive;
    let gears_at = gears_at(mem, drive);
    let mut gears = mem.gearbox(gears_at);
    let def = mem.def(scratch.def);
    let mut state = mem.state(scratch.state);
    let gear_count = mem.gear_count(drive);
    let on = super::dispatch::flag_dispatch(
        packet,
        || calls.query_slip(this),
        scratch.vcu_enabled,
        gear_count,
        &def,
        tuning,
        &mut state,
        &mut gears,
    );
    if on {
        mem.store_state(scratch.state, &state);
        mem.store_gearbox(gears_at, &gears);
    }
    on
}

/// `CDrive::M6840 (value)`: `SetDirty` on the gear block's `+0x564`, then the commanded gear at `+0x570`; the request
/// flag is not touched.
pub fn set_commanded_gear<S: VcuStore>(mem: &mut S, calls: &mut impl VcuCalls, drive: u32, value: u32) {
    let count = mem.gear_count(drive);
    let gears = gears_at(mem, drive);
    calls.set_dirty(gears + 0x564);
    mem.set_u32(gears + DRIVE_STATE_COMMANDED_GEAR, super::gear::commanded_gear(count, value));
}

/// `CScratchSim::Step (dt)` (`gear`, with the gear logic) or `StepBasic`. It writes the state, the drive's gear block
/// and its outputs (the drive setters' writes); the engine and light setters are called in the port's order, after
/// the stores. Returns the state pointer, the original's last load (eax at its ret).
pub fn step<S: VcuStore>(
    mem: &mut S,
    calls: &mut impl VcuCalls,
    this: u32,
    dt: f32,
    gear: bool,
    tuning: Tuning,
) -> u32 {
    use super::step::Send;
    let scratch = mem.scratch(this);
    let drive = scratch.drive;
    let gears_at = gears_at(mem, drive);
    let mut gears = mem.gearbox(gears_at);
    // Read only by the gear logic, which skips it without the launch flag or during a shift.
    let car_speed = if gear { gear_speeds(mem, calls, &scratch, &gears).map_or(0.0, |(_, speed)| speed) } else { 0.0 };
    let input = super::step::Input {
        dt,
        engine: calls.engine_speed(drive),
        car_speed,
        drive_speed: calls.lock_speed(drive),
        gear_count: mem.gear_count(drive),
        tuning,
    };
    let def = mem.def(scratch.def);
    let mut state = mem.state(scratch.state);
    let mut out = mem.outputs(drive);
    let mut sends = Vec::new();
    let port = if gear { super::step::step } else { super::step::step_basic };
    port(input, &scratch, &def, &mut state, &mut gears, &mut out, &mut |s| sends.push(s));
    mem.store_state(scratch.state, &state);
    mem.store_gearbox(gears_at, &gears);
    mem.store_outputs(drive, &out);
    for send in sends {
        match send {
            Send::EnginePedal(v) => calls.set_engine_pedal(scratch.engine, v),
            Send::CutFlags(flags) => {
                calls.set_cut_flags(scratch.engine, flags);
            }
            Send::BrakeLight(on) => calls.set_brake_light(scratch.lights, on),
            Send::NeutralLight(on) => calls.set_neutral_light(scratch.lights, on),
            // the drive setters' writes are the outputs stored above
            Send::DriveThrottle(_) | Send::DriveBrake(_) | Send::DriveClutch(_) | Send::DiffLock { .. } => {}
        }
    }
    scratch.state
}

/// The Mem store: the bodies are the layout's reads and writes.
impl VcuStore for Mem {
    /// Reads a speed map of `n` records at `at`.
    fn speed_map(&self, at: u32, n: usize) -> Vec<SpeedKnot> {
        let mem = self;
        (0..n as u32)
            .map(|k| SpeedKnot { key: mem.f32(at + 0x20 * k + 0x0c), value: mem.f32(at + 0x20 * k + 0x1c) })
            .collect()
    }

    /// Reads a pedal map of `n` knots at `at`.
    fn pedal_map(&self, at: u32, n: usize) -> Vec<Knot> {
        let mem = self;
        (0..n as u32).map(|k| Knot { value: mem.f32(at + 0x10 * k + 0x0c) }).collect()
    }

    /// Reads the setup at `at`. The memory must cover the whole object (0xcac bytes).
    fn def(&self, at: u32) -> Def {
        let mem = self;
        let f = |off: u32| mem.f32(at + off);
        Def {
            gear_upshift_rpm: std::array::from_fn(|k| f(GEAR_UPSHIFT_RPM + 0x20 * k as u32)),
            gear_downshift_rpm: std::array::from_fn(|k| f(GEAR_DOWNSHIFT_RPM + 0x20 * k as u32)),
            shift_lockout_up_s: f(SHIFT_LOCKOUT_UP_S),
            shift_lockout_down_s: f(SHIFT_LOCKOUT_DOWN_S),
            clutch_ramp_rate: f(CLUTCH_RAMP_RATE),
            rpm_limit: f(RPM_LIMIT),
            centre_diff_handbrake_release: f(CENTRE_DIFF_HANDBRAKE_RELEASE),
            left_foot_brake_threshold: f(LEFT_FOOT_BRAKE_THRESHOLD),
            gear_count: mem.u32(at + 0x654),
            centre_throttle_lock: mem.pedal_map(at + CENTRE_THROTTLE_LOCK, KNOTS),
            centre_brake_lock: mem.pedal_map(at + CENTRE_BRAKE_LOCK, KNOTS),
            centre_speed_lock: mem.speed_map(at + CENTRE_SPEED_LOCK, KNOTS),
            lf_centre_throttle_lock: mem.pedal_map(at + LF_CENTRE_THROTTLE_LOCK, KNOTS),
            lf_centre_brake_lock: mem.pedal_map(at + LF_CENTRE_BRAKE_LOCK, KNOTS),
            lf_centre_speed_lock: mem.speed_map(at + LF_CENTRE_SPEED_LOCK, KNOTS),
            front_throttle_lock: mem.pedal_map(at + FRONT_THROTTLE_LOCK, KNOTS),
            front_brake_lock: mem.pedal_map(at + FRONT_BRAKE_LOCK, KNOTS),
            front_speed_lock: mem.speed_map(at + FRONT_SPEED_LOCK, KNOTS),
            rear_throttle_lock: mem.pedal_map(at + REAR_THROTTLE_LOCK, KNOTS),
            rear_brake_lock: mem.pedal_map(at + REAR_BRAKE_LOCK, KNOTS),
            rear_speed_lock: mem.speed_map(at + REAR_SPEED_LOCK, KNOTS),
        }
    }

    /// Reads the maps of the setup at `setup`.
    fn knot_maps(&self, setup: u32) -> KnotMaps {
        let mem = self;
        let pedal = |at: u32| load_triples(mem, setup + at, 0x10);
        let speed = |keys: u32, values: u32| SpeedWords {
            keys: load_triples(mem, setup + keys, 0x10),
            values: load_triples(mem, setup + values, 0x10),
        };
        KnotMaps {
            centre_throttle: pedal(0x070),
            centre_brake: pedal(0x120),
            lf_centre_throttle: pedal(0x330),
            lf_centre_brake: pedal(0x3e0),
            front_throttle: pedal(0x5f0),
            front_brake: pedal(0x6a0),
            rear_throttle: pedal(0x8b0),
            rear_brake: pedal(0x960),
            centre_speed: speed(0x1d0, 0x280),
            lf_centre_speed: speed(0x490, 0x540),
            front_speed: speed(0x750, 0x800),
            rear_speed: speed(0xa10, 0xac0),
        }
    }

    /// Writes the maps into the CDef at `def`.
    fn store_knot_maps(&mut self, def: u32, value: &KnotMaps) {
        let mem = self;
        store_triples(mem, def + 0x1b0, 0x10, &value.centre_throttle);
        store_triples(mem, def + 0x260, 0x10, &value.centre_brake);
        store_triples(mem, def + 0x470, 0x10, &value.lf_centre_throttle);
        store_triples(mem, def + 0x520, 0x10, &value.lf_centre_brake);
        store_triples(mem, def + 0x730, 0x10, &value.front_throttle);
        store_triples(mem, def + 0x7e0, 0x10, &value.front_brake);
        store_triples(mem, def + 0x9f0, 0x10, &value.rear_throttle);
        store_triples(mem, def + 0xaa0, 0x10, &value.rear_brake);
        store_speed(mem, def + 0x310, def + 0x320, &value.centre_speed);
        store_speed(mem, def + 0x5d0, def + 0x5e0, &value.lf_centre_speed);
        store_speed(mem, def + 0x890, def + 0x8a0, &value.front_speed);
        store_speed(mem, def + 0xb50, def + 0xb60, &value.rear_speed);
    }

    fn setup_flags(&self, setup: u32) -> SetupFlags {
        let mem = self;
        SetupFlags {
            launch: mem.u32(setup + 0x18),
            slip_gate_rpm: mem.u32(setup + 0x28),
            slip_gate_speed: mem.u32(setup + 0x38),
            launch_mode_when_off: mem.u32(setup + 0x48),
        }
    }

    fn state(&self, at: u32) -> State {
        let mem = self;
        State {
            shift_lockout: mem.f32(at + SHIFT_LOCKOUT),
            launch_clutch: mem.f32(at + LAUNCH_CLUTCH),
            launch_clutch_dirty: mem.u32(at + LAUNCH_CLUTCH_DIRTY),
            pedal_swap: mem.u32(at + PEDAL_SWAP),
            gear_timer: mem.f32(at + GEAR_TIMER),
            shift_cooldown: mem.f32(at + SHIFT_COOLDOWN),
        }
    }

    fn store_state(&mut self, at: u32, value: &State) {
        let mem = self;
        mem.set_f32(at + SHIFT_LOCKOUT, value.shift_lockout);
        mem.set_f32(at + LAUNCH_CLUTCH, value.launch_clutch);
        mem.set_u32(at + LAUNCH_CLUTCH_DIRTY, value.launch_clutch_dirty);
        mem.set_u32(at + PEDAL_SWAP, value.pedal_swap);
        mem.set_f32(at + GEAR_TIMER, value.gear_timer);
        mem.set_f32(at + SHIFT_COOLDOWN, value.shift_cooldown);
    }

    fn gearbox(&self, at: u32) -> Gearbox {
        let mem = self;
        Gearbox {
            gear: mem.u32(at + DRIVE_STATE_GEAR),
            commanded: mem.u32(at + DRIVE_STATE_COMMANDED_GEAR),
            changed: mem.u32(at + 0x56c),
            slip: mem.u32(at + 0x55c),
        }
    }

    fn store_gearbox(&mut self, at: u32, value: &Gearbox) {
        let mem = self;
        mem.set_u32(at + DRIVE_STATE_GEAR, value.gear);
        mem.set_u32(at + DRIVE_STATE_COMMANDED_GEAR, value.commanded);
        mem.set_u32(at + 0x56c, value.changed);
        mem.set_u32(at + 0x55c, value.slip);
    }

    /// Gear count of the drive's own definition (`[drive + 4] + 0x654`), which `CDrive::Set47B0` clamps to.
    fn gear_count(&self, drive: u32) -> u32 {
        let mem = self;
        mem.u32(mem.u32(drive + 0x4) + 0x654)
    }

    fn reals(&self, drive: u32) -> Reals {
        let mem = self;
        Reals {
            throttle: mem.f32(drive + 0x2c),
            brake: mem.f32(drive + 0x3c),
            brake_scaled: mem.f32(drive + 0x70),
            clutch_slip: mem.f32(drive + 0x60),
            brake_scale: mem.f32(mem.u32(drive + 0x04) + 0x620),
        }
    }

    fn store_reals(&mut self, drive: u32, value: &Reals) {
        let mem = self;
        for (dirty, value, v) in [
            (0x28, 0x2c, value.throttle),
            (0x38, 0x3c, value.brake),
            (0x6c, 0x70, value.brake_scaled),
            (0x5c, 0x60, value.clutch_slip),
        ] {
            mem.set_u32(drive + dirty, 1);
            mem.set_f32(drive + value, v);
        }
    }

    fn outputs(&self, drive: u32) -> Outputs {
        let mem = self;
        Outputs { locks: mem.locks(drive), reals: mem.reals(drive) }
    }

    fn store_outputs(&mut self, drive: u32, value: &Outputs) {
        let mem = self;
        mem.store_locks(drive, &value.locks);
        mem.store_reals(drive, &value.reals);
    }

    fn gear_ratios(&self, drive: u32) -> GearRatios {
        let mem = self;
        let def = mem.u32(drive + 0x04);
        GearRatios {
            lim: std::array::from_fn(|g| mem.f32(def + 0x644 + 0x50 * g as u32)),
            div: std::array::from_fn(|g| mem.f32(def + 0x694 + 0x50 * g as u32)),
        }
    }

    fn locks(&self, drive: u32) -> Locks {
        let mem = self;
        let d8 = mem.u32(drive + 0x08);
        let def = mem.u32(drive + 0x04);
        Locks {
            ratio: std::array::from_fn(|ch| mem.f32(d8 + 0x14 * ch as u32 + 0x0c)),
            enabled: mem.u32(mem.u32(drive + 0x170) + 0x90) != 0,
            max_torque: std::array::from_fn(|ch| mem.f32(def + 0x58 + 0x60 * ch as u32)),
            torque: std::array::from_fn(|ch| mem.f32(d8 + 0x14 * ch as u32 + 0x18)),
        }
    }

    /// Writes the locks back. Only the torques change; the ratios and the maximum torques are inputs here.
    fn store_locks(&mut self, drive: u32, value: &Locks) {
        let mem = self;
        let d8 = mem.u32(drive + 0x08);
        for (ch, &torque) in value.torque.iter().enumerate() {
            mem.set_f32(d8 + 0x14 * ch as u32 + 0x18, torque);
        }
    }

    fn pedals(&self, scratch: u32) -> Pedals {
        let mem = self;
        Pedals {
            throttle: mem.f32(scratch + SCRATCH_THROTTLE),
            brake: mem.f32(scratch + SCRATCH_BRAKE),
            handbrake: mem.f32(scratch + SCRATCH_HANDBRAKE),
        }
    }

    fn store_pedals(&mut self, scratch: u32, value: &Pedals) {
        let mem = self;
        mem.set_f32(scratch + SCRATCH_THROTTLE, value.throttle);
        mem.set_f32(scratch + SCRATCH_BRAKE, value.brake);
        mem.set_f32(scratch + SCRATCH_HANDBRAKE, value.handbrake);
    }

    fn packet(&self, at: u32) -> Packet {
        let mem = self;
        Packet {
            pedals: Pedals { throttle: mem.f32(at), brake: mem.f32(at + 0x04), handbrake: mem.f32(at + 0x08) },
            fixed_clutch: mem.f32(at + 0x0c),
            unknown_10: mem.u32(at + 0x10),
            flags: mem.u8(at + 0x14),
            value: mem.u32(at + 0x18),
        }
    }

    fn scratch(&self, at: u32) -> Scratch {
        let mem = self;
        Scratch {
            def: mem.u32(at + SCRATCH_DEF),
            state: mem.u32(at + SCRATCH_STATE),
            pedals: mem.pedals(at),
            fixed_clutch: mem.f32(at + SCRATCH_FIXED_CLUTCH),
            unknown_18: mem.u32(at + SCRATCH_UNKNOWN_18),
            launch_enabled: mem.u32(at + SCRATCH_LAUNCH_ENABLED),
            launch_mode: mem.u32(at + SCRATCH_LAUNCH_MODE),
            vcu_enabled: mem.u32(at + SCRATCH_VCU_ENABLED),
            slip_gate_rpm: mem.u32(at + SCRATCH_SLIP_GATE_RPM),
            slip_gate_speed: mem.u32(at + SCRATCH_SLIP_GATE_SPEED),
            engine: mem.u32(at + SCRATCH_ENGINE),
            drive: mem.u32(at + SCRATCH_DRIVE),
            lights: mem.u32(at + SCRATCH_LIGHTS),
        }
    }

    fn store_scratch(&mut self, at: u32, value: &Scratch) {
        let mem = self;
        mem.set_u32(at + SCRATCH_DEF, value.def);
        mem.set_u32(at + SCRATCH_STATE, value.state);
        mem.store_pedals(at, &value.pedals);
        mem.set_f32(at + SCRATCH_FIXED_CLUTCH, value.fixed_clutch);
        mem.set_u32(at + SCRATCH_UNKNOWN_18, value.unknown_18);
        mem.set_u32(at + SCRATCH_LAUNCH_ENABLED, value.launch_enabled);
        mem.set_u32(at + SCRATCH_LAUNCH_MODE, value.launch_mode);
        mem.set_u32(at + SCRATCH_VCU_ENABLED, value.vcu_enabled);
        mem.set_u32(at + SCRATCH_SLIP_GATE_RPM, value.slip_gate_rpm);
        mem.set_u32(at + SCRATCH_SLIP_GATE_SPEED, value.slip_gate_speed);
        mem.set_u32(at + SCRATCH_ENGINE, value.engine);
        mem.set_u32(at + SCRATCH_DRIVE, value.drive);
        mem.set_u32(at + SCRATCH_LIGHTS, value.lights);
    }
}
