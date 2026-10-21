//! The car's constructors: the light system, the setup and options, the definition, the state, `CCar` and
//! `CCarBody`.

use super::car::{
    DAMAGE_HANDLER, GRID_PUSHED, PARTIAL_CONTACT, SNAPSHOT_ON, STATE, STEER_INPUT, STEP_COUNT, WALL_CONTACT,
};
use super::car_parts::SETUP_COPY;
use crate::car_body::links::{self, Link, LinkRef, Links, List, ListRef};
use crate::collision::e38::RayHost;
use crate::collision::layout::CtorHost;
use crate::entities::exclusion;
use crate::mem::Mem;
pub(super) use crate::physics_core::layout::SIGNAL_MANAGER as SIGNALS;
use crate::physics_core::passenger::PassengerHost;
use crate::physics_core::{construct, passenger};
use crate::rigid_body::host::HostUnavailable;
use crate::vcu::layout::SimParts;
use crate::{collision, engine_driveline, entities, ntweak, rigid_body, vcu};

/// `CCarLightSystem::CDef`'s vtable (0x739264) and the name its base gets (`"LightSystem"`, 0x7380b4).
const LIGHT_DEF_VTABLE: u32 = 0x0073_9264;
const LIGHT_DEF_NAME: u32 = 0x0073_80B4;

/// `CCarLightSystem::CDef::CDef` (0x4f1570) on the definition at `this`: the base, the vtable, a real at +0xc, then
/// unsigneds from +0x1c to +0x1fc (six single ones, then six arrays of four: the low beams, high beams, rear, brake and
/// reverse lights and one more).
pub fn construct_light_def(mem: &mut Mem, this: u32) {
    ntweak::construct_base(mem, this, LIGHT_DEF_NAME);
    mem.set_u32(this, LIGHT_DEF_VTABLE);
    ntweak::construct_real(mem, this + 0xC);
    for at in (0x1C..0x1FC).step_by(0x10) {
        ntweak::construct_unsigned(mem, this + at);
    }
}

/// `CCar::CSetup`'s vtable (0x72e494).
const SETUP_VTABLE: u32 = 0x0072_E494;

/// `CCar::CSetup::CSetup` (0x47b620) on the setup at `this`: an unnamed tweak, its vtable, and the reals at +0xc, +0x1c
/// and +0x2c (`MaxSteeringLock`, `FrontRollBarStiffness`, `RearRollBarStiffness`).
pub fn construct_setup(mem: &mut Mem, this: u32) {
    ntweak::construct_base(mem, this, 0);
    mem.set_u32(this, SETUP_VTABLE);
    for at in [0xC, 0x1C, 0x2C] {
        ntweak::construct_real(mem, this + at);
    }
}

/// `CCar::CSetupOptions`'s vtable (0x72e724).
const OPTIONS_VTABLE: u32 = 0x0072_E724;

/// `CCar::CSetupOptions::CSetupOptions` (0x47d260) on the options at `this`: an unnamed tweak, its vtable, three ranges
/// (`E13RealTriple`s: steering lock and the two roll bars) from +0xc, the vector at +0xa0 (`vecCenterOfGravity`), the
/// real at +0xc0 (`InverseMass`) and 20 reals from +0xd0 (`Tq00`..`Tq95`).
pub fn construct_options(mem: &mut Mem, this: u32) {
    ntweak::construct_base(mem, this, 0);
    mem.set_u32(this, OPTIONS_VTABLE);
    for at in (0xC..0x9C).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
    ntweak::construct_vector(mem, this + 0xA0);
    for at in (0xC0..0x210).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
}

/// The setup's frame words into the car's definition (`def`): the stores of `CCar::UpdateA720` (0x4ea720) after its
/// calls, with the car's `CCar::CSetup` (`setup`) as the seed's frame.
pub fn copy_setup(mem: &mut Mem, def: u32, setup: u32) {
    for (to, from) in SETUP_COPY {
        let word = mem.u32(setup + from);
        mem.set_u32(def + to, word);
    }
}

/// The passengers' constructor calls, ported.
struct PassengerTweaks;

impl PassengerHost for PassengerTweaks {
    fn base(&mut self, mem: &mut Mem, at: u32, name: u32) {
        ntweak::construct_base(mem, at, name);
    }

    fn unsigned(&mut self, mem: &mut Mem, at: u32) {
        ntweak::construct_unsigned(mem, at);
    }

    fn real(&mut self, mem: &mut Mem, at: u32) {
        ntweak::construct_real(mem, at);
    }

    fn clear(&mut self, mem: &mut Mem, at: u32) {
        rigid_body::layout::clear_vec(mem, at);
    }

    /// `CPassenger::CScratch::Vf04` (0x5224e0) returns.
    fn scratch_vf04(&mut self, _: &mut Mem, _: u32) {}
}

/// `CCar::CDef`'s vtable (0x7382a4).
const DEF_VTABLE: u32 = 0x0073_82A4;
/// The size `CCar::CDef::Create` asks for.
pub const DEF_SIZE: u32 = 0x44E0;
/// `CCar::CDef` members the car-body ports read (rbr-recomp `notes/physics/car-body.md` 2b).
pub const DEF_FRONTAL_AREA: u32 = 0x4480;
pub const DEF_DRAG_COEFFICIENT: u32 = 0x4490;
pub const DEF_DANGEROUS_ACCELERATION: u32 = 0x44B0;
/// Where the definitions inside the car's are: the control unit's, the drive's, the engine's, the light system's, and
/// the driver's and co-driver's.
pub const DEF_VCU: u32 = 0x250;
pub const DEF_DRIVE: u32 = 0xEFC;
pub const DEF_ENGINE: u32 = 0x1990;
const DEF_LIGHTS: u32 = 0x4210;
const DEF_PASSENGERS: [u32; 2] = [0x440C, 0x4428];
/// The reals the constructor sets last (dirty, then the value): `MaxFFTorque` 10.0, `FrontArea` 2.0, `DragCoefficient`
/// 0.4 and `DangerousAcceleration` 490.50003 (the floats at 0x72b154, 0x72ad94, 0x72f664 and 0x738298).
const DEF_DEFAULTS: [(u32, u32); 4] =
    [(0x4494, 0x4120_0000), (0x4474, 0x4000_0000), (0x4484, 0x3ECC_CCCD), (0x44A4, 0x43F5_4001)];

/// `CCar::CDef::CDef(outer)` (0x4e61f0, thiscall, ret 4) on the definition at `this`: the solid definition's base, the
/// vtable, six reals (+0x1f0..+0x240), the definitions inside it (the engine's allocates from `host`), an unsigned
/// (+0x4444), two reals (+0x4454, the anti-roll bars), four reals and two unsigneds (+0x4474..+0x44c4), then the
/// defaults.
pub fn construct_def(mem: &mut Mem, host: &mut impl construct::Heap, this: u32, outer: u32) {
    rigid_body::layout::solid_def_ctor(mem, this, outer);
    mem.set_u32(this, DEF_VTABLE);
    for at in (0x1F0..0x250).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
    vcu::layout::construct_def(mem, this + DEF_VCU);
    engine_driveline::layout::construct_drive_def(mem, this + DEF_DRIVE);
    engine_driveline::layout::construct_engine_def(mem, host, this + DEF_ENGINE);
    construct_light_def(mem, this + DEF_LIGHTS);
    for at in DEF_PASSENGERS {
        passenger::construct_def(mem, &mut PassengerTweaks, this + at);
    }
    ntweak::construct_unsigned(mem, this + 0x4444);
    for at in [0x4454, 0x4464, 0x4474, 0x4484, 0x4494, 0x44A4] {
        ntweak::construct_real(mem, this + at);
    }
    for at in [0x44B4, 0x44C4] {
        ntweak::construct_unsigned(mem, this + at);
    }
    for (at, value) in DEF_DEFAULTS {
        ntweak::set_dirty(mem, this + at);
        mem.set_u32(this + at + 0xC, value);
    }
}

/// `CCar::CDef::Create(outer)` (0x4e6f20): a definition block from `host`, built by `construct_def` on `outer`. Returns
/// the block, or 0 when the allocation fails.
pub fn new_def(mem: &mut Mem, outer: u32, host: &mut impl construct::Heap) -> u32 {
    let block = host.alloc(mem, DEF_SIZE);
    if block != 0 {
        construct_def(mem, host, block, outer);
    }
    block
}

/// `CCarLightSystem::CState`'s vtable (0x7392bc); its base is named like the definition's.
const LIGHT_STATE_VTABLE: u32 = 0x0073_92BC;
/// The light state's unsigneds' first values (dirty, then the value), in the asm's order.
const LIGHT_STATE_DEFAULTS: [(u32, u32); 5] = [(0x50, 0), (0x40, 1), (0x20, 1), (0x30, 0), (0x60, 0)];

/// `CCarLightSystem::CState::CState` (0x4f1840, thiscall, three stack arguments it does not read) on the state at
/// `this`: the base, the vtable, a real (+0x10), six unsigneds (+0x20..+0x70), the defaults, and `+0x5c` (the
/// unsigned at +0x50's padding word) cleared.
pub fn construct_light_state(mem: &mut Mem, this: u32) {
    ntweak::construct_base(mem, this, LIGHT_DEF_NAME);
    mem.set_u32(this, LIGHT_STATE_VTABLE);
    ntweak::construct_real(mem, this + 0x10);
    for at in (0x20..0x80).step_by(0x10) {
        ntweak::construct_unsigned(mem, this + at);
    }
    for (at, value) in LIGHT_STATE_DEFAULTS {
        ntweak::set_dirty(mem, this + at);
        mem.set_u32(this + at + 0xC, value);
        if at == 0x50 {
            mem.set_u32(this + 0x5C, 0);
        }
    }
}

/// `CCarLightSystem`'s vtable (0x73930c) and its scratch's (0x739258, at +0xc; slot 1 returns).
const LIGHTS_VTABLE: u32 = 0x0073_930C;
const LIGHTS_SCRATCH_VTABLE: u32 = 0x0073_9258;

/// `CCarLightSystem::CCarLightSystem(def, state, owner)` (0x4f1dc0, thiscall) on the light system at `this`.
pub fn construct_lights(mem: &mut Mem, this: u32, def: u32, state: u32, owner: u32) {
    mem.set_u32(this, LIGHTS_VTABLE);
    mem.set_u32(this + 0xC, LIGHTS_SCRATCH_VTABLE);
    mem.set_u32(this + 4, def);
    mem.set_u32(this + 8, state);
    mem.set_u32(this + 0x10, owner);
}

/// `CCar::CStatePair` (0x4e5610, 0x40 bytes, two in the car's state): four reals.
fn construct_state_pair(mem: &mut Mem, this: u32) {
    for at in (0..0x40).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
}

/// `CCar::CState`'s vtable (0x73823c), its pairs (+0xc, 0x40 bytes each) and the inner states: the control unit's,
/// the drive's, the engine's, the light system's and the driver's and co-driver's.
const STATE_VTABLE: u32 = 0x0073_823C;
const STATE_PAIRS: u32 = 0xC;
const STATE_PAIR: u32 = 0x40;
const STATE_VCU: u32 = 0x120;
const STATE_DRIVE: u32 = 0x190;
const STATE_ENGINE: u32 = 0x750;
const STATE_LIGHTS: u32 = 0x9D0;
const STATE_PASSENGERS: [u32; 2] = [0xA50, 0xB10];
/// `+0xc70`'s first value (4.0, the float at 0x72d150).
const STATE_C70: f32 = 4.0;

/// `CCar::CState::CState(car, name, unused)` (0x4e6f80, thiscall) on the state at `this`: the base named `name`, the
/// vtable, the pairs, two vectors and five reals, the inner states (only the engine's reads an argument: `car`, its
/// owner), a real (+0xbe8), then with a car the first values in the asm's order. The engine state's `Err` (a crash in
/// the original) comes back.
pub fn construct_state(mem: &mut Mem, this: u32, car: u32, name: u32) -> Result<(), HostUnavailable> {
    ntweak::construct_base(mem, this, name);
    mem.set_u32(this, STATE_VTABLE);
    for k in 0..2 {
        construct_state_pair(mem, this + STATE_PAIRS + k * STATE_PAIR);
    }
    for at in [0x90, 0xB0] {
        ntweak::construct_vector(mem, this + at);
    }
    for at in (0xD0..0x120).step_by(0x10) {
        ntweak::construct_real(mem, this + at);
    }
    vcu::layout::construct_state(mem, this + STATE_VCU);
    engine_driveline::layout::construct_drive_state(mem, this + STATE_DRIVE);
    engine_driveline::layout::construct_engine_state(mem, this + STATE_ENGINE, car)?;
    construct_light_state(mem, this + STATE_LIGHTS);
    for at in STATE_PASSENGERS {
        passenger::construct_state(mem, &mut PassengerTweaks, this + at);
    }
    ntweak::construct_real(mem, this + 0xBE8);
    if car == 0 {
        return Ok(());
    }
    mem.set_u32(this + 0xC6C, 0);
    rigid_body::layout::clear_vec(mem, this + 0xBD0);
    mem.set_u32(this + 0xBE0, 0);
    for k in 0..2 {
        let pair = this + STATE_PAIRS + k * STATE_PAIR;
        for at in [0x30, 0x10, 0x20] {
            ntweak::set_dirty(mem, pair + at);
            mem.set_u32(pair + at + 0xC, 0);
        }
    }
    mem.set_u32(this + 0xC24, 0);
    mem.set_u32(this + 0xC20, 5);
    for at in [0xC28, 0xC2C, 0xC30, 0xC34, 0xC38, 0xC68, 0xBFC, 0xBF8] {
        mem.set_u32(this + at, 0);
    }
    rigid_body::layout::clear_vec(mem, this + 0xC10);
    mem.set_u32(this + 0xC00, 0);
    rigid_body::layout::clear_vec(mem, this + 0xC80);
    mem.set_f32(this + 0xC70, STATE_C70);
    mem.set_u32(this + 0xBE4, 0);
    Ok(())
}

/// `CCar`'s vtable (0x738b44) and the size `CCar::CDef::Vf50` asks for.
const CAR_VTABLE: u32 = 0x0073_8B44;
pub const CAR_SIZE: u32 = 0x1D60;
/// The car's signal connections (0x20 bytes: vtable, the link's next, prev and list at +4, the car at +0x10, the
/// callback at +0x18): offset, vtable, callback, and the list in the signal object (`[0x893600]`) they join.
const CONNECTIONS: [(u32, u32, u32, u32); 5] = [
    (0x3D0, 0x0073_8B1C, 0x004E_91F0, 0xCA8),
    (0x3F0, 0x0073_8B1C, 0x004E_91F0, 0xD18),
    (0x410, 0x0073_8B1C, 0x004E_91F0, 0xCD8),
    (0x430, 0x0073_8B28, 0x004E_A720, 0x8A20),
    (0x450, 0x0073_8B34, 0x004E_9590, 0x96B0),
];
/// The current car (`g_8ef660`), which the constructor sets.
pub const CURRENT_CAR: u32 = 0x008E_F660;
/// The car's parts, each with its state (in the car) and its definition (in the car's): the drive, the engine, the
/// light system, the control unit's scratch, and the driver and co-driver.
const CAR_DRIVE: u32 = 0x4B0;
const CAR_ENGINE: u32 = 0x690;
const CAR_LIGHTS: u32 = 0x838;
const CAR_SCRATCH_SIM: u32 = 0x850;
const CAR_PASSENGERS: [(u32, u32, u32); 2] = [(0x8B0, 0x1450, 0x440C), (0x900, 0x1510, 0x4428)];
/// The car's word after its state (`+0x1690`, a vtable 0x73828c, then five cleared words) and the ray object (+0x16b0).
const CAR_1690_VTABLE: u32 = 0x0073_828C;
const CAR_RAYS: u32 = 0x16B0;
/// The words of the exclusion set `CCar::CCar` asks for.
const CAR_EXCLUSIONS: u32 = 0x20;

/// The constructors' inline `push_back` of the signal connection at `conn` onto the list whose head is at `head`.
pub fn push_back_connection(mem: &mut Mem, head: u32, conn: u32) {
    links::push_back(&mut Connections(mem), ListRef::new(head), LinkRef::new(conn));
}

/// The signal connections' link words, 4 bytes into the connection.
struct Connections<'a>(&'a mut Mem);

impl Connections<'_> {
    fn words(at: LinkRef) -> LinkRef {
        LinkRef::new(at.addr() + 4)
    }
}

impl Links for Connections<'_> {
    fn link(&self, at: LinkRef) -> Link {
        self.0.link(Self::words(at))
    }

    fn set_link(&mut self, at: LinkRef, link: Link) {
        self.0.set_link(Self::words(at), link);
    }

    fn set_next(&mut self, at: LinkRef, next: Option<LinkRef>) {
        self.0.set_next(Self::words(at), next);
    }

    fn set_prev(&mut self, at: LinkRef, prev: Option<LinkRef>) {
        self.0.set_prev(Self::words(at), prev);
    }

    fn list(&self, at: ListRef) -> List {
        self.0.list(at)
    }

    fn set_list(&mut self, at: ListRef, list: List) {
        self.0.set_list(at, list);
    }
}

/// `E38RaySeeds`'s vector clears, made after it (it writes nothing they overlap).
#[derive(Default)]
struct RayClears(Vec<u32>);

impl RayHost for RayClears {
    fn vec4_clear(&mut self, at: u32) -> u32 {
        self.0.push(at);
        at
    }
}

/// `CCar::CCar(def, a, b, flags)` (0x4ea9d0, thiscall, ret 0x10) on the car at `this`: the solid base (`host`, with the
/// flags' 0x8), the vtable, the signal connections, the parts on the definition's parts and the car's states (their
/// owner is the car when there is a definition), the state, the words after it and the ray object, the current car,
/// the definition (+0x34), the exclusion set (`host`), then with a definition the parts' resets (the light system's
/// and scratch's slot 1 return) and the connections joining the signal object's lists, and the last words. Returns
/// `this`; the engine state's `Err` comes back.
pub fn construct_car(
    mem: &mut Mem,
    host: &mut (impl CtorHost + exclusion::Heap),
    this: u32,
    def: u32,
    a: u32,
    b: u32,
    flags: u32,
) -> Result<u32, HostUnavailable> {
    rigid_body::layout::construct_solid(mem, host, this, def, a, b, flags | 8);
    mem.set_u32(this, CAR_VTABLE);
    for (at, vtable, callback, _) in CONNECTIONS {
        let connection = this + at;
        for word in [8, 4, 0xC] {
            mem.set_u32(connection + word, 0);
        }
        mem.set_u32(connection, vtable);
        mem.set_u32(connection + 0x10, this);
        mem.set_u32(connection + 0x18, callback);
        mem.set_u32(connection + 0x1C, 0);
    }
    let part = |at: u32| if def != 0 { def + at } else { 0 };
    let owner = if def != 0 { this } else { 0 };
    let state = this + STATE;
    engine_driveline::layout::construct_drive(mem, this + CAR_DRIVE, part(DEF_DRIVE), state + STATE_DRIVE, owner);
    engine_driveline::layout::construct_engine(mem, this + CAR_ENGINE, part(DEF_ENGINE), state + STATE_ENGINE, owner);
    construct_lights(mem, this + CAR_LIGHTS, part(DEF_LIGHTS), state + STATE_LIGHTS, owner);
    vcu::layout::construct_scratch_sim(
        mem,
        this + CAR_SCRATCH_SIM,
        part(DEF_VCU),
        state + STATE_VCU,
        owner,
        SimParts { engine: this + CAR_ENGINE, drive: this + CAR_DRIVE, lights: this + CAR_LIGHTS },
    );
    for (at, state_at, def_at) in CAR_PASSENGERS {
        passenger::construct_body(mem, &mut PassengerTweaks, this + at, part(def_at), this + state_at, owner);
    }
    let name = if def != 0 { ntweak::name_or_null(mem, def) } else { 0 };
    construct_state(mem, state, this, name)?;
    mem.set_u32(this + 0x1690, CAR_1690_VTABLE);
    for at in [0x1698, 0x16A0, STEER_INPUT, 0x1694, 0x169C] {
        mem.set_u32(this + at, 0);
    }
    let mut clears = RayClears::default();
    collision::layout::ray_seeds(&mut clears, mem, this + CAR_RAYS);
    for at in clears.0 {
        rigid_body::layout::clear_vec(mem, at);
    }
    mem.set_u32(CURRENT_CAR, this);
    mem.set_u32(this + 0x34, def);
    for at in [PARTIAL_CONTACT, WALL_CONTACT, 0x16AC, DAMAGE_HANDLER, 0x3C0] {
        mem.set_u32(this + at, 0);
    }
    entities::layout::alloc_exclusion(mem, this, CAR_EXCLUSIONS, host);
    mem.set_u32(this + GRID_PUSHED, 0);
    if def != 0 {
        engine_driveline::layout::drive_reset_part(mem, this + CAR_DRIVE);
    }
    for at in (0x470..0x4A4).step_by(4) {
        mem.set_u32(this + at, 0);
    }
    if def != 0 {
        for (at, _, _, list) in CONNECTIONS {
            let head = mem.u32(SIGNALS) + list;
            push_back_connection(mem, head, this + at);
        }
    }
    for at in [SNAPSHOT_ON, 0x9F0, STEP_COUNT] {
        mem.set_u32(this + at, 0);
    }
    for at in [0x954, 0x958, 0x95C, 0x960] {
        mem.set_u32(this + at, u32::MAX);
    }
    Ok(this)
}

/// `CCar::CDef::Vf50(a, b)` (0x4eb3e0, thiscall, ret 8) on the definition `def`: a car block from `host`, built by
/// [`construct_car`] with no flags. Returns the car, or 0 when the allocation fails.
pub fn new_car(
    mem: &mut Mem,
    host: &mut (impl construct::Heap + CtorHost + exclusion::Heap),
    def: u32,
    a: u32,
    b: u32,
) -> Result<u32, HostUnavailable> {
    let block = host.alloc(mem, CAR_SIZE);
    if block == 0 {
        return Ok(0);
    }
    construct_car(mem, host, block, def, a, b, 0)
}

/// `CCarBody::CDef`'s size (the block `NewDefD0` allocates) and vtable (0x738ddc).
pub const BODY_DEF_SIZE: u32 = 0x150;
const BODY_DEF_VTABLE: u32 = 0x0073_8DDC;
/// The body definition's `vecCenterOfGravity`, and the word its slot 17 sets.
const BODY_DEF_CENTER: u32 = 0x120;
const BODY_DEF_PARSED: u32 = 0x140;
/// `CCarBody::CDef::Vf44` (0x4eb5e0), the body definition's slot 17.
pub const BODY_DEF_AFTER_PARSE: u32 = 0x004E_B5E0;

/// `CCarBody::CDef::CDef(name)` (0x4eb510) on the definition at `this`: the body definition's base, the vtable, then the
/// centre of gravity vector.
pub fn construct_body_def(mem: &mut Mem, this: u32, name: u32) {
    collision::state::body_def_ctor(mem, this, name);
    mem.set_u32(this, BODY_DEF_VTABLE);
    ntweak::construct_vector(mem, this + BODY_DEF_CENTER);
}

/// `CCarBody::NewDefD0(name)` (0x4eba80): a body definition block from `host`, built by [`construct_body_def`]. Returns
/// the block, or 0 when the allocation fails.
pub fn new_body_def(mem: &mut Mem, name: u32, host: &mut impl construct::Heap) -> u32 {
    let block = host.alloc(mem, BODY_DEF_SIZE);
    if block != 0 {
        construct_body_def(mem, block, name);
    }
    block
}

/// `CCarBody`'s size (the block its definition's `Vf50x` allocates), vtable (0x738e9c), and its state (`CCarBody::CState`,
/// vtable 0x738e4c): the state's unsigned and the owner it keeps.
pub const BODY_SIZE: u32 = 0x130;
const BODY_VTABLE: u32 = 0x0073_8E9C;
const BODY_STATE: u32 = 0xD0;
const BODY_STATE_VTABLE: u32 = 0x0073_8E4C;
const BODY_STATE_UNSIGNED: u32 = 0xC;
const BODY_STATE_OWNER: u32 = 0x40;
/// The body's flags byte (the entity's options): the state keeps the body only when bit 4 is set.
const BODY_FLAGS: u32 = 0x60;
const BODY_FLAG: u8 = 0x10;

/// `CCarBody::CCarBody(def, a, b, flags)` (0x4eb730) on the body at `this`: the `CBody` base (with flag 0x10), the
/// vtable, then the state (0x4eb610) on the definition's name: a tweak base, its vtable, an unsigned, and the body (when
/// its flag is set; the constructor's flag 0x10 sets it).
pub fn construct_body(mem: &mut Mem, host: &mut impl CtorHost, this: u32, def: u32, a: u32, b: u32, flags: u32) -> u32 {
    collision::layout::ctor(host, mem, this, def, a, b, flags | 0x10);
    mem.set_u32(this, BODY_VTABLE);
    let name = if def != 0 { ntweak::name_or_null(mem, def) } else { 0 };
    let state = this + BODY_STATE;
    ntweak::construct_base(mem, state, name);
    mem.set_u32(state, BODY_STATE_VTABLE);
    ntweak::construct_unsigned(mem, state + BODY_STATE_UNSIGNED);
    let owner = if mem.u8(this + BODY_FLAGS) & BODY_FLAG != 0 { this } else { 0 };
    mem.set_u32(state + BODY_STATE_OWNER, owner);
    this
}

/// `CCarBody::CDef::Vf50x(a, b)` (0x4ebb00) on the definition `def`: a body block from `host`, built by
/// [`construct_body`] with no flags. Returns the body, or 0 when the allocation fails.
pub fn new_body(mem: &mut Mem, host: &mut (impl construct::Heap + CtorHost), def: u32, a: u32, b: u32) -> u32 {
    let block = host.alloc(mem, BODY_SIZE);
    if block == 0 {
        return 0;
    }
    construct_body(mem, host, block, def, a, b, 0)
}

/// `CCarBody::CDef::Vf44` (0x4eb5e0), run after each group applied to the body definition at `this`: sets its word
/// `+0x140` to 1.
pub fn body_def_after_parse(mem: &mut Mem, this: u32) {
    mem.set_u32(this + BODY_DEF_PARSED, 1);
}
