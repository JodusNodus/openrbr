//! `--boot`: the memory image the pure step starts from, built from the retail files and the exe's `.data`/`.rdata`,
//! with no snapshot. The load's objects are made by the load ports
//! (`rbr_physics::load`, `stage`, `physics_core`, `car_body`, `entities`) in the game's order, their blocks from the
//! boot's own allocator, zero-filled (decisions O2 and O4). Only the physics' and the gameplay's signal connections are
//! made (O1): the constructors' own links.
//!
//! The order: the exe's sections; the application's state word and the signal manager on it; the game flow's words;
//! the physics definition and the definition registry from the physics files; the car's setups; the physics object
//! (its state, scratch, time keeper and event pool) and the `.data` words the step reads; the ground; the stage world
//! (`.col`, `.trk`, `.mat`); the damage host; the car's collision grids; the car's placements (the car, its wheels and
//! bodies, by their factories); the fence (`.fnc`); the stack. Then `CCar::UpdateA720` runs through its binding on the pure host
//! ([`update_a720`]), as the setup's emit runs it in the game.
//!
//! The race is a [`Race`] its caller fills, its weather from `rbr_gameplay::weather` (the `.mat` table's words and the
//! wet meshes). The car placements' handles are LocoMotion's numbering of the car's and the flip spectators' nodes
//! ([`crate::handles`]).

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Read;
use std::rc::Rc;

use rbr_formats::data::Data;
use rbr_formats::lsp::{self, Node};
use rbr_formats::{cars, fnc, lbs, markers, pe, rbz, rsf, sgc, stage_files, trk};
use rbr_gameplay::car_damage::CarriedDamage;
use rbr_gameplay::flow::Mode;
use rbr_gameplay::race_clock::AssistSettings;
use rbr_gameplay::race_options::{self, ChampionshipOptions, RaceOptions};
use rbr_gameplay::timing::TimeKeeper;
use rbr_gameplay::weather::Weather;
use rbr_hybrid::pure::Pure;
use rbr_hybrid::{Host, Regs};
use rbr_physics::car_body::{self, damage};
use rbr_physics::collision::layout as collision;
use rbr_physics::entities;
use rbr_physics::load::{self, car_grids, layout as load_layout, Registry};
use rbr_physics::math::trig;
use rbr_physics::mem::{Mem, Pages};
use rbr_physics::ngp::exe_patches;
use rbr_physics::ngp::layout::{car_init, stage as ngp_stage};
use rbr_physics::physics_core::layout as physics;
use rbr_physics::physics_core::layout::physics_object::cdef_new_physics;
use rbr_physics::stage::layout as stage;

use crate::handles;

mod hosts;

use hosts::{BootHeap, Files, NoCalls, PhysicsCalls, Profiler, StageCalls, HEAP_BASE};

const PAGE: u32 = 0x1000;
/// NGP.dll's base in the reference runs; its `.data` (`0x10171000..0x10178000` at the file base) starts zeroed.
const NGP: car_init::Data = car_init::Data { base: exe_patches::BASE };
const NGP_DATA: (u32, usize) = (0x1017_1000, 0x7000);

/// A car placement's `CSigStuff` as the car load emits it (0x46b160, the same for every car): the part's registry name,
/// the section map name its handle is looked up by (the car body has none: handle 0), its collision grid, its flags
/// (1 wake, 2 parent of the next ones, 4 the last child) and its last word.
pub(crate) struct CarPart {
    name: &'static str,
    pub(crate) node: Option<&'static str>,
    grid: u32,
    flags: u32,
    tail: u32,
}

const fn part(name: &'static str, node: Option<&'static str>, grid: u32, flags: u32, tail: u32) -> CarPart {
    CarPart { name, node, grid, flags, tail }
}

/// The car load's placements in its order: the car (`"parent"`), the wheels, the bodies, the car body.
pub(crate) const CAR_PARTS: [CarPart; 13] = [
    part("CAR_ROOT", Some("parent"), 1, 3, 1),
    part("CAR_WHEEL_LF", Some("e_wheel_lf"), 0, 1, 0x101),
    part("CAR_WHEEL_RF", Some("e_wheel_rf"), 0, 1, 0x101),
    part("CAR_WHEEL_LB", Some("e_wheel_lb"), 0, 1, 0x101),
    part("CAR_WHEEL_RB", Some("e_wheel_rb"), 0, 1, 0x101),
    part("CAR_DOOR_L", Some("e_door_l"), 2, 0, 0x101),
    part("CAR_DOOR_R", Some("e_door_r"), 3, 0, 0x101),
    part("CAR_BONNET", Some("e_bonnet"), 4, 0, 0x101),
    part("CAR_EXHAUSTPIPE", Some("e_exhaust"), 5, 0, 0x101),
    part("CAR_BUMPER_F", Some("e_bumper_f"), 6, 0, 0x101),
    part("CAR_BUMPER_B", Some("e_bumper_b"), 7, 0, 0x101),
    part("CAR_WING", Some("e_wing"), 8, 0, 0x101),
    part("CAR_BODY", None, 0, 5, 0x101),
];

/// The race the boot loads (lead decision 3a: a typed race configuration its caller fills, from a drive's `.auto` or
/// the launcher): `IRBRGame::StartGame`'s map, car and tyre, and the settings the game takes from its menus.
#[derive(Clone, Debug)]
pub struct Race {
    pub map: u32,
    /// The `[CarNN]` of `Cars/Cars.ini`.
    pub car: u32,
    /// `StartGame`'s tyre (`TyreType04`, 0x566250's index): 0-2 tarmac, 3-5 gravel, 6 snow.
    pub tyre: u32,
    /// The menu's race options: the damage level, the driver aids and the weather preset (`rbr_gameplay::race_options`).
    pub options: RaceOptions,
    /// The surface of the map's car files (`E14_4a1330`, [`surface`]); a community stage's own (`rsf::Surface`).
    pub surface: &'static str,
    /// `CWeatherDaemon`'s state after `StartGame`: the `.mat` entry's words (`E11_TyreCond`, 0x46f6c0) and the wet
    /// meshes.
    pub weather: Weather,
    /// The game flow's mode word (`[0x165fa90]`, `rbr_gameplay::flow::Mode`).
    pub flow_mode: u32,
    /// A championship stage's options (flow mode 3): the season's skill and damage level. None outside one.
    pub championship: Option<ChampionshipOptions>,
    /// `QueryPerformanceFrequency` (Wine's in the reference runs).
    pub frequency: u64,
    /// NGP's `physics.rbz` (with its cars in their slots): the physics files come from it, parsed with NGP's keys.
    pub ngp_physics: std::path::PathBuf,
    /// An NGP car pack read in place of the slot's physics folder (an RSF car, `rbr_formats::rsf`): what RSF copies to
    /// `Physics/<dir>/` before the game loads the slot. Its model comes through `Data::with_car`.
    pub car_pack: Option<rsf::Pack>,
    /// The player's setup file's root group (`%sSetups/<name>.lsp`, `rbr_gameplay::setup`), walked on the setup after
    /// the default file as the game's loader layers it; none for the default setup.
    pub setup: Option<Vec<Node>>,
    /// The damage a championship carries into this stage (`rbr_gameplay::car_damage`); none for a fresh car.
    pub damage: Option<CarriedDamage>,
    /// A championship stage: the player's false starts so far in the rally (the jump-start penalty's index).
    pub false_starts: u8,
    /// NGP's `ForceFeedbackSensitivity<Surface>` percentages for gravel, tarmac and snow (100 leaves the car's).
    pub ffb_sensitivity: [i32; 3],
    /// NGP's `headGasketDurability` (1, its default, leaves the stall roll as 1.02's; `car_init::head_gasket`).
    pub head_gasket_durability: i32,
}

impl Race {
    /// A quick race of `map` and `car` on `tyre` in `weather` under the physics archive `ngp_physics`, with the default
    /// options (aids 0/1/0/1, damage level 1, damage selection 1) and flow mode 1 (the weather's state 2).
    pub fn quick_race(map: u32, car: u32, tyre: u32, weather: Weather, ngp_physics: std::path::PathBuf) -> Race {
        Race {
            map,
            car,
            tyre,
            surface: surface(map),
            options: RaceOptions::default(),
            weather,
            flow_mode: Mode::QUICK_RALLY.value(),
            championship: None,
            frequency: 10_000_000,
            setup: None,
            ngp_physics,
            car_pack: None,
            damage: None,
            false_starts: 0,
            ffb_sensitivity: [100; 3],
            head_gasket_durability: 1,
        }
    }

    /// The race as a championship stage: flow mode 3 with the season's options.
    pub fn in_championship(self, options: ChampionshipOptions) -> Race {
        Race { flow_mode: Mode::CHAMPIONSHIP.value(), championship: Some(options), ..self }
    }

    /// The penalty settings the race flow takes: the flow mode, the season's skill (+0x8e84) and the quick race's
    /// damage level (`0x1660850`).
    pub fn assist_settings(&self) -> AssistSettings {
        let skill = self.championship.map_or(0, |c| c.skill.word());
        AssistSettings {
            mode: self.flow_mode,
            rally_level: skill,
            level: self.options.damage.word(),
            false_starts: self.false_starts,
        }
    }

    /// The level `InitQuickRaceDamageHandler` (0x567640) gives the car in this flow mode.
    pub fn damage_level(&self) -> u32 {
        let championship = self.championship.unwrap_or_default().damage;
        race_options::damage_level(Mode(self.flow_mode), self.options.damage, championship).word()
    }

    /// Where `archive` (a data path) is read from: NGP's `physics.rbz` in its place.
    fn archive(&self, data: &Data, archive: &str) -> Result<std::path::PathBuf, String> {
        if archive == "Root/physics.rbz" {
            return Ok(self.ngp_physics.clone());
        }
        data.resolve(archive).ok_or(format!("{archive}: not found"))
    }
}

/// The surface of `map` (`E14_4a1330`, 0x4a1330): the suffix of the car's surface, setup and options files.
pub fn surface(map: u32) -> &'static str {
    match map {
        10..=18 => "snow",
        0x29..=0x31 | 0x49 | 0x55 | 0x56 | 0x5e => "tarmac",
        _ => "gravel",
    }
}

/// The tyre the reference drives start `map`'s surface on (their `.auto` tyre: 0 tarmac, 4 gravel, 6 snow).
pub fn default_tyre(surface: &str) -> u32 {
    match surface {
        "tarmac" => 0,
        "snow" => 6,
        _ => 4,
    }
}

/// `g_carNames` (0x7c18d4): each car's physics folder (`"Physics/t_coroll/"`).
const CAR_PHYSICS_DIRS: u32 = 0x007C_18D4;

/// The interactive objects' handles start here (`HedgeHog3D`'s numbering of the `(1, 6)` objects' instances).
pub const INTERACTIVE_HANDLE_BASE: u32 = 40000;

/// The stage's interactive objects' handles by `.trk` interactive id: the instances of the `.lbs`'s `(1, 6)` objects
/// numbered from 40000 in file order, each instance's first word its id (checked against tanner-corolla's 35 keys).
fn interactive_handles(lbs: &lbs::Lbs) -> HashMap<u32, u32> {
    lbs.chunks
        .iter()
        .filter_map(|c| match c {
            lbs::Chunk::Interactive(i) => Some(i),
            _ => None,
        })
        .flat_map(|i| i.objects.iter().flat_map(|o| o.visibility.iter().map(|v| v.unknown)))
        .zip(INTERACTIVE_HANDLE_BASE..)
        .collect()
}

/// The car's start pose (0x46b160): the `.lbs` start location's `(x, z, y + 0.5)` (z up), turned about z by
/// `pi - yaw`, the quaternion `(0, 0, sin, cos)` of the half angle from the x87's `fsin` and `fcos`.
fn start_pose(start: &lbs::StartLocation) -> ([f32; 3], [f32; 4]) {
    let [x, y, z] = start.position;
    let half = (std::f32::consts::PI - start.rotation[1]) * 0.5;
    let at = trig::X87Ext80::from_f32(half);
    ([x, z, y + 0.5], [0.0, 0.0, trig::fsin(at).to_f32(), trig::fcos(at).to_f32()])
}

/// The car's files: its physics folder (the exe's table), the map's surface, the tyre brand (`LoadTyreBrand`, 0x48df00:
/// Pirelli for cars 5 and 7) and the model (`Cars/Cars.ini`).
struct CarFiles {
    /// The car's physics folder (`Physics/t_coroll/`).
    dir: String,
    physics: [(&'static str, String); 4],
    setup: String,
    options: [String; 2],
    model: String,
}

fn car_files(data: &Data, mem: &Mem, race: &Race) -> Result<CarFiles, String> {
    if race.car > 7 {
        return Err(format!("car {}: not a retail car", race.car));
    }
    let name = mem.u32(CAR_PHYSICS_DIRS + 4 * race.car);
    let dir: Vec<u8> = (name..).map(|a| mem.u8(a)).take_while(|&b| b != 0).collect();
    let dir = String::from_utf8(dir).map_err(|_| "the exe's car folder is not text")?;
    let surface = race.surface;
    let brand = if matches!(race.car, 5 | 7) { "pirelli" } else { "michelin" };
    let ini = String::from_utf8_lossy(&read(data, "Cars/Cars.ini")?).into_owned();
    let model = data
        .car_slots(cars::parse(&ini).map_err(|e| e.to_string())?)
        .into_iter()
        .find(|c| c.index == race.car as usize)
        .ok_or(format!("Cars.ini: no [Car{:02}]", race.car))?
        .file_name;
    Ok(CarFiles {
        physics: [
            ("Root/misc.rbz", "Misc/prime.lsp".into()),
            ("Root/physics.rbz", "Physics/physics.lsp".into()),
            ("Root/physics.rbz", format!("{dir}common.lsp")),
            ("Root/physics.rbz", format!("{dir}{surface}.lsp")),
        ],
        setup: format!("{dir}setups/d_{surface}.lsp"),
        options: [format!("Physics/{brand}/tyres.lsp"), format!("{dir}r_{surface}.lsp")],
        model,
        dir,
    })
}

/// `CPC_Application` (`[0x7eac48]`, 0x1bd0 bytes) and its state word, which every signal selects its lists by.
const APPLICATION: u32 = 0x007E_AC48;
const APPLICATION_SIZE: u32 = 0x1BD0;
const APP_STATE: u32 = 0x728;
/// The signal manager's pointer (`g_signalManager`).
const SIGNALS: u32 = 0x0089_3600;
/// The game's flow object (`[0x165fa90]`): its mode word (+0) and the host `damage_select` reads (+0xc98, whose first
/// half-word is 0 in the reference run, so it reads the damage selection global).
const FLOW: u32 = 0x0165_FA90;
const FLOW_SIZE: u32 = 0xCB0;
const FLOW_HOST: u32 = 0xC98;
/// The game's `CCompleteCarSetup` and `CCompleteCarSetupOptions` (NGP's `0x10011cb0` and `0x10013cd0` read them).
const COMPLETE_SETUP: u32 = 0x007E_CD00;
const SETUP_OPTIONS: u32 = 0x007E_CD04;
/// The time card's block (`CTimeKeeper::Init`'s).
const CARD_SIZE: u32 = 0x1E0;
/// `QueryPerformanceCounter`'s import slot and what the boot puts there: an address nothing maps, which the pure host
/// answers by the slot's value.
const QPC_SLOT: u32 = 0x0072_9190;
const QPC_STAND_IN: u32 = 0xFEED_3000;
/// The track state (its `.mat` image word at +0x148).
const TRACK_STATE_SIZE: u32 = 0x150;
/// The drive line's block (`CreateDriveLine`'s).
const DRIVE_LINE_SIZE: u32 = 0x75330;
/// Free memory for the original's temporaries (the placements, `SyncGrid`'s vertices, the fire mesh's descriptor).
const SCRATCH: u32 = 0x0030_0000;
const SCRATCH_SIZE: u32 = 0x1_0000;
const STUFF_SIZE: u32 = 0x64;
/// A car placement's fields: the orientation, the extent, the grid, the registry index, the sort key, the flags, the
/// hook (the car's damage handler), the word 0xf, the position again (`x`, `z - 0.5`, `y`) and the last word.
const STUFF_ORIENTATION: u32 = 0x10;
const STUFF_EXTENT: u32 = 0x20;
const STUFF_GRID: u32 = 0x34;
const STUFF_INDEX: u32 = 0x38;
const STUFF_OBJECT: u32 = 0x3C;
const STUFF_HANDLE: u32 = 0x40;
const STUFF_FLAGS: u32 = 0x44;
const STUFF_HOOK: u32 = 0x48;
const STUFF_F: u32 = 0x50;
const STUFF_POSITION: u32 = 0x54;
const STUFF_TAIL: u32 = 0x60;
/// The damage host's slots (0x10 into it, `DAMAGE_OPS_SIZE` apart).
const DAMAGE_SLOTS: u32 = 0x10;
/// The stack: the control payload at its bottom, `StepForward`'s return slot near its top.
const STACK_LO: u32 = 0x0010_0000;
const STACK_HI: u32 = 0x0020_0000;
const STEP_SLOT: u32 = STACK_HI - 0x1000;
/// `CCar::UpdateA720` (0x4ea720).
const UPDATE_A720: u32 = 0x004E_A720;

/// The season daemon's pointer (`g_79host28`, `[0x7d6128]`) and the word of it a stage reads: the skill (+0x8e84),
/// which `E60Flow::M566640` (`car_body::layout::flow_holds`) reads in mode 3.
const SEASON_DAEMON: u32 = 0x007D_6128;
const SEASON_SKILL: u32 = 0x8E84;

/// A championship stage's season daemon (ours): the boot does not build `E80Daemon` (0xa0c8 bytes, the season's
/// drivers, rallies and weather); a zeroed block up to the skill word stands in, as `DaemonInit` leaves +0x8e84.
fn season_stand_in(mem: &mut Mem, heap: &BootHeap, options: ChampionshipOptions) {
    let daemon = heap.take(SEASON_SKILL + 4);
    mem.set_u32(daemon + SEASON_SKILL, options.skill.word());
    mem.set_u32(SEASON_DAEMON, daemon);
}

/// The built image and where the step starts.
pub struct Image {
    pub mem: Mem,
    pub regions: Vec<(u32, u32)>,
    /// `StepForward`'s entry registers (the physics object in ecx), its return slot, and where the control goes.
    pub regs: Regs,
    pub slot: u32,
    pub control_at: u32,
    /// The car and the block `UpdateA720` takes.
    pub car: u32,
    pub seed: u32,
    /// The car's setup and setup options.
    pub setup: u32,
    pub options: u32,
    /// NGP.dll's `.data`.
    pub ngp: car_init::Data,
    /// The block for NGP's idle table (`car_init::IDLE_TABLE_SIZE` bytes), which its init builds.
    pub idle_table: u32,
    /// The race's [`Race::ffb_sensitivity`], for NGP's init.
    pub ffb_sensitivity: [i32; 3],
    /// What the boot built, for the report.
    pub built: Vec<(&'static str, u64)>,
    /// The boot heap's blocks `(address, size)`, in allocation order (`--dump-image`).
    pub blocks: Vec<(u32, u32)>,
    /// The handles of the detachable car parts (`rbr_recording::detach::PARTS`): each part's animation object, with its index.
    pub detach: Vec<(u32, u32)>,
}

fn read(data: &Data, path: &str) -> Result<Vec<u8>, String> {
    let at = data.resolve(path).ok_or(format!("{path}: not found"))?;
    std::fs::read(&at).map_err(|e| format!("{}: {e}", at.display()))
}

fn lsp_root(data: &Data, race: &Race, files: &CarFiles, archive: &str, name: &str) -> Result<Vec<Node>, String> {
    let text = match pack_file(race, &files.dir, name) {
        Some(bytes) => String::from_utf8_lossy(&bytes?).into_owned(),
        None => {
            let path = race.archive(data, archive)?;
            let mut zip = rbz::open(&path).map_err(|e| format!("{archive}: {e}"))?;
            let mut text = String::new();
            rbz::by_name(&mut zip, name)
                .map_err(|e| format!("{archive}/{name}: {e}"))?
                .read_to_string(&mut text)
                .map_err(|e| format!("{archive}/{name}: {e}"))?;
            text
        }
    };
    let file = lsp::parse(&text).map_err(|e| format!("{name}: {e:?}"))?;
    Ok(load::root(&file).ok_or(format!("{name}: no root group"))?.to_vec())
}

/// The physics file `name` (`Physics/s_i2003/common.lsp`) when it lies in the car's folder `dir` and the race reads
/// that folder from a car pack.
fn pack_file(race: &Race, dir: &str, name: &str) -> Option<Result<Vec<u8>, String>> {
    let rest = name.strip_prefix(dir)?;
    Some(race.car_pack.as_ref()?.read(rest))
}

/// The car placement `part` at `at` (its handle, the registry index `index`, the damage handler `hook`), as the car
/// load emits it: the car's at the start pose, the parts' at the origin with no rotation.
fn store_car_stuff(
    mem: &mut Mem,
    at: u32,
    pose: &([f32; 3], [f32; 4]),
    part: &CarPart,
    handle: u32,
    index: u32,
    hook: u32,
) {
    mem.map(at, &[0; STUFF_SIZE as usize]);
    let root = std::ptr::eq(part, &CAR_PARTS[0]);
    let (position, orientation) = pose;
    if root {
        for (k, &x) in position.iter().enumerate() {
            mem.set_f32(at + 4 * k as u32, x);
        }
    }
    let orientation = if root { *orientation } else { [0.0, 0.0, 0.0, 1.0] };
    for (k, &x) in orientation.iter().enumerate() {
        mem.set_f32(at + STUFF_ORIENTATION + 4 * k as u32, x);
    }
    for k in 0..3 {
        mem.set_f32(at + STUFF_EXTENT + 4 * k, 1.0);
    }
    mem.set_u32(at + STUFF_GRID, part.grid);
    mem.set_u32(at + STUFF_INDEX, index);
    mem.set_u32(at + STUFF_OBJECT, 0);
    mem.set_u32(at + STUFF_HANDLE, handle);
    mem.set_u32(at + STUFF_FLAGS, part.flags);
    mem.set_u32(at + STUFF_HOOK, hook);
    mem.set_u32(at + STUFF_F, 0xf);
    let [x, y, z] = *position;
    for (k, v) in [x, z - 0.5, y].into_iter().enumerate() {
        mem.set_f32(at + STUFF_POSITION + 4 * k as u32, v);
    }
    mem.set_u32(at + STUFF_TAIL, part.tail);
}

/// The image for `race` on `stage_name`, the application in state `app_state`.
pub fn build(data: &Data, stage_name: &str, app_state: u32, race: &Race) -> Result<Image, String> {
    if !stage_files::name(stage_name).eq_ignore_ascii_case(&race.weather.stage()) {
        return Err(format!("the race's weather loads {}, not {stage_name}", race.weather.stage()));
    }
    // The game thread loads at PC=24 (`notes/ngp.md`, "x87 precision"), NGP's race steps on its own thread aside.
    rbr_physics::fpu::set_precision(rbr_physics::fpu::Precision::Pc24);
    let pages = Pages::default();
    let mut mem = Mem::with_pages(pages.clone());
    let mut built = Vec::new();

    // The exe's sections as the process starts with them (the code for the call targets the step reads), with NGP's
    // edits under NGP.
    let exe = pe::parse(&read(data, "RichardBurnsRally_SSE.exe")?).map_err(|e| format!("the exe: {e:?}"))?;
    for name in [".text", ".rdata", ".data"] {
        let section = exe.section(name).ok_or(format!("the exe has no {name}"))?;
        mem.map(section.address, &section.bytes);
    }
    mem.set_u32(QPC_SLOT, QPC_STAND_IN);
    mem.map(NGP.at(NGP_DATA.0), &vec![0; NGP_DATA.1]);
    let heap = BootHeap { mem: mem.clone(), next: Rc::new(Cell::new(HEAP_BASE)), blocks: Rc::default() };

    // The application's state word and the signal manager on it (`CPC_Application`'s constructor).
    let app = heap.take(APPLICATION_SIZE);
    mem.set_u32(APPLICATION, app);
    mem.set_u32(app + APP_STATE, app_state);
    let signals = heap.take(load_layout::SIGNAL_COUNT * load_layout::SIGNAL_SIZE);
    load_layout::construct_signal_manager(&mut mem, signals, app + APP_STATE);
    mem.set_u32(SIGNALS, signals);

    // The game flow's words the step reads.
    let flow = heap.take(FLOW_SIZE);
    mem.set_u32(flow, race.flow_mode);
    mem.set_u32(flow + FLOW_HOST, heap.take(0x10));
    mem.set_u32(FLOW, flow);
    // The menu settings the loads read (`E66MenuSettings`): the damage selection the step reads, and the others.
    for (at, word) in race.options.globals() {
        mem.set_u32(at, word);
    }
    if let Some(championship) = &race.championship {
        for (at, word) in championship.globals() {
            mem.set_u32(at, word);
        }
    }

    // The physics definition and the registry's definitions, from the physics files.
    let car_files = car_files(data, &mem, race)?;
    let mut files = Files(heap.clone());
    let mut registry = Registry::default();
    let def = load::new_physics_def(&mut mem, &mut files);
    for (archive, file) in &car_files.physics {
        let root = lsp_root(data, race, &car_files, archive, file)?;
        load::physics_file(&mut mem, &mut files, &mut registry, def, &root).map_err(|e| format!("{file}: {e:?}"))?;
    }
    load_layout::store_registry(&mut mem, &registry);
    built.push(("registry entries", registry.entries().len() as u64));

    // The car's setups: the owners, their files, the race's choices.
    let complete = heap.take(load_layout::COMPLETE_SETUP_SIZE);
    let options = heap.take(load_layout::SETUP_OPTIONS_SIZE);
    load::construct_complete_setup(&mut mem, complete);
    load::construct_setup_options(&mut mem, options);
    mem.set_u32(COMPLETE_SETUP, complete);
    mem.set_u32(SETUP_OPTIONS, options);
    let root = lsp_root(data, race, &car_files, "Root/physics.rbz", &car_files.setup)?;
    load::setup_file(&mut mem, &mut files, complete, &root).map_err(|e| format!("{}: {e:?}", car_files.setup))?;
    if let Some(root) = &race.setup {
        load::setup_file(&mut mem, &mut files, complete, root).map_err(|e| format!("the player's setup: {e:?}"))?;
    }
    for file in &car_files.options {
        let root = lsp_root(data, race, &car_files, "Root/physics.rbz", file)?;
        load::options_file(&mut mem, &mut files, options, &root).map_err(|e| format!("{file}: {e:?}"))?;
    }
    let setups = load::setups(complete);
    load::select_tyre_type(&mut mem, &setups, load::tyre_type(options, race.tyre));
    load::select_drive_options(&mut mem, setups.drive, load::drive_options(options));
    load::select_top_mounts(&mut mem, &setups, options);
    let [auto_gears, gear_guard, neutral_lock, clutch_help] = race.options.aids.words();
    load::set_driver_aids(&mut mem, &setups, &load::DriverAids { auto_gears, gear_guard, neutral_lock, clutch_help });

    // The physics object (`E46_InitPhysics`'s `CDef::Vf4c`), its event pool, and the `.data` words the step reads.
    let dls =
        markers::parse(&stage_files::read(data, stage_name, "dls")?).map_err(|e| format!("{stage_name}.dls: {e:?}"))?;
    let mut calls = PhysicsCalls { heap: heap.clone(), keeper: TimeKeeper::load(&dls), prefilled: false };
    let physics_at = cdef_new_physics(&mut mem, &mut calls, def);
    assert!(calls.prefilled, "CPhysics::CPhysics prefills the event pool");
    collision::prefill_pool(&mut mem);
    physics::set_cdef_global(&mut mem, def);
    physics::set_physics_global(&mut mem, physics_at);
    physics::store_frequency(&mut mem, race.frequency);
    load::enter_tic_sample(&mut mem, &mut Profiler);
    load::store_statics(&mut mem);
    let system = mem.u32(stage::COLLISION_SYSTEM);
    stage::collision_system_ctor(&mut mem, system);
    load::new_replay_stand_in(&mut mem, &mut heap.clone());

    // The ground, filed in the physics state's wall list (`CState::RegWall`).
    let ground_def = registry.def("Ground", "GROUND").ok_or("the files make no ground definition")?;
    let ground = entities::layout::ground_make(&mut mem, ground_def, 0, 0, &mut heap.clone(), &mut NoCalls);
    let state = physics::state_of(&mem, physics_at);
    physics::push_item(&mut mem, physics::entity_list(state, false, false, true), ground);

    // The stage world: the cell grid an earlier `E45_MD60` built (no wall yet), the `.col`, the `.trk` (the drive line,
    // the meshes and their placements) and the `.mat`.
    let drive_line = heap.take(DRIVE_LINE_SIZE);
    mem.set_u32(stage::DRIVE_LINE, drive_line);
    let track =
        trk::parse(&stage_files::read(data, stage_name, "trk")?).map_err(|e| format!("{stage_name}.trk: {e:?}"))?;
    let mesh_capacity: usize = track
        .chunks
        .iter()
        .map(|c| match &c.payload {
            trk::ChunkPayload::CollisionMeshes(m) => m.meshes.len(),
            _ => 0,
        })
        .sum();
    let mesh_capacity = mesh_capacity as u32;
    let track_state = heap.take(TRACK_STATE_SIZE);
    let mesh_list = heap.take(4 * mesh_capacity);
    let col = stage_files::read(data, stage_name, "col")?;
    let mat = stage_files::read(data, stage_name, "mat")?;
    let lbs_bytes = stage_files::read(data, stage_name, "lbs")?;
    let lbs = lbs::parse(&lbs_bytes).map_err(|e| format!("{stage_name}.lbs: {e:?}"))?;
    let start = lbs
        .chunks
        .iter()
        .find_map(|c| match c {
            lbs::Chunk::StartLocation(s) => Some(*s),
            _ => None,
        })
        .ok_or(format!("{stage_name}.lbs: no start location"))?;
    let mut calls = StageCalls {
        heap: heap.clone(),
        handles: interactive_handles(&lbs),
        keyed: 0,
        stuffs: 0,
        scratch: 0,
        mesh_list,
        mesh_capacity,
        meshes: 0,
        failed: None,
        wet: race.weather.wet_meshes(),
    };
    stage::build_cell_grid(&mut mem, &mut calls);
    let col_image = heap.take(col.len() as u32);
    stage::load_col(&mut mem, &mut calls, col_image, &col).map_err(|e| format!("{stage_name}.col: {e:?}"))?;
    stage::load_trk(&mut mem, &mut calls, &track);
    if let Some(k) = calls.failed {
        return Err(format!("{stage_name}: stage placement {k} is not ported"));
    }
    built.push(("stage placements", calls.stuffs as u64));
    built.push(("interactive handles", calls.keyed as u64));
    built.push(("stage meshes", u64::from(calls.meshes)));
    let mat_image = heap.take(mat.len() as u32);
    let (wear, wetness) = race.weather.mat_words();
    stage::load_materials(&mut mem, track_state, mat_image, &mat, wear.as_bytes(), wetness.as_bytes());

    // The damage host (`E60Flow::M569270`): its slots reset, then the car's handler (slot 0) as the car load leaves it
    // before `UpdateA720` (whose `UpdateMrr` runs the handler's `Slot4`).
    let damage_host = heap.take(car_body::layout::DAMAGE_HOST_SIZE);
    car_body::layout::construct_damage_host(&mut mem, damage_host);
    let slot = |k: u32| damage_host + DAMAGE_SLOTS + car_body::layout::DAMAGE_OPS_SIZE * k;
    for k in 0..4 {
        damage::reset_damage(&mut car_body::layout::ResetPorts, &mut mem, slot(k));
    }
    match &race.damage {
        None => {
            for _ in 0..2 {
                damage::reset_damage(&mut car_body::layout::ResetPorts, &mut mem, slot(0));
            }
        }
        // A championship stage after the first: the slot keeps the damage of the stage before
        // (`InitQuickRaceDamageHandler` 0x567640 does not reset it in mode 3), the words `CopyFrom` carries.
        Some(carried) => {
            for (at, word) in carried.words() {
                mem.set_u32(slot(0) + at, word);
            }
        }
    }
    // `InitQuickRaceDamageHandler` (0x567640): `Set3FB0` with the flow mode's damage level (`0x1660850` in a quick
    // race, the championship's `0x1660c70` in mode 3).
    damage::set_level(&mut mem, slot(0), race.damage_level());

    // The car's collision grids: the car loader's meshes (grids 1-8, their names HedgeHog3D's, here a block at the
    // same offsets from a base) and `E02FireCollisionMesh`'s (grid 9).
    let model = &car_files.model;
    let model_bytes = read(data, model)?;
    let bodies: Vec<sgc::Collision> = sgc::parse(&model_bytes)
        .map_err(|e| format!("{model}: {e:?}"))?
        .into_iter()
        .filter_map(|e| match e {
            sgc::Element::Collision(c) => Some(c),
            _ => None,
        })
        .collect();
    let meshes = car_grids::car_meshes(&bodies).ok_or(format!("{model}: no car meshes"))?;
    let (low, high) = car_grids::PARTS.iter().fold((u32::MAX, 0), |(lo, hi), part| (lo.min(part.2), hi.max(part.2)));
    let hh_base = heap.take(high - low + 0x20) - low;
    for &(name, id, at) in &car_grids::PARTS {
        let name = if id == 1 { "MrCarCollision" } else { name };
        mem.map(hh_base + at, &[name.as_bytes(), &[0]].concat());
    }
    load::emit_car_meshes(&mut mem, &mut heap.clone(), hh_base, &meshes);
    load::fire_collision_mesh(&mut mem, SCRATCH);
    built.push(("car collision meshes", meshes.len() as u64 + 1));

    // The car's placements (`E46_Notify67d0` on each `CSigStuff` the car loader emits).
    let (mut factory_heap, mut cell_heap) = (heap.clone(), heap.clone());
    let mut parts = load_layout::CarParts {
        factory: load_layout::Factories(&mut factory_heap),
        heap: &mut cell_heap,
        sync_scratch: SCRATCH + 0x1000,
    };
    let pose = start_pose(&start);
    let nodes = handles::CarNodes::new(&model_bytes).map_err(|e| format!("{model}: {e}"))?;
    let before = handles::objects_before_car(data, &mem, race.map)?;
    let mut car = 0;
    let mut detach = Vec::new();
    for (k, part) in CAR_PARTS.iter().enumerate() {
        let at = SCRATCH + 0x80 * k as u32;
        let handle = match part.node {
            Some(node) => nodes.handle(&mem, node, race.tyre, before).map_err(|e| format!("{model}: {e}"))?,
            None => 0,
        };
        let index = registry.index(part.name).ok_or(format!("{}: not in the registry", part.name))?;
        if let Some(part) = part.node.and_then(rbr_recording::detach::part_index) {
            detach.push((handle, part));
        }
        store_car_stuff(&mut mem, at, &pose, part, handle, index, slot(0));
        physics::notify_stuff(&mut mem, &mut parts, at)
            .map_err(|_| format!("the car placement {} is not ported", part.name))?;
        if k == 0 {
            car = mem.u32(at + STUFF_OBJECT);
        }
    }
    built.push(("car placements", CAR_PARTS.len() as u64));
    car_init::copy_tyre_defs(&mut mem, NGP, car);

    // The fence (`CFenceHandler::Read` on the stage's `.fnc`): its groups and their dummy objects, filed in the cells.
    let fence =
        fnc::parse(&stage_files::read(data, stage_name, "fnc")?).map_err(|e| format!("{stage_name}.fnc: {e}"))?;
    stage::load_fence(&mut mem, &mut heap.clone(), &fence);
    built.push(("fence posts", fence.groups.iter().map(|g| g.posts.len() as u64).sum()));
    let seed = heap.take(load_layout::SEED_SIZE);
    load_layout::store_seed(&mut mem, seed, complete);
    if race.flow_mode == Mode::CHAMPIONSHIP.value() {
        // Last, so a quick race's heap stays as it was.
        season_stand_in(&mut mem, &heap, race.championship.unwrap_or_default());
    }

    // NGP's edits to the exe, after the loads (whose ports take NGP's paths by the variant, and dispatch the readers
    // by 1.02's vtables).
    for (at, bytes) in exe_patches::PATCHES {
        mem.map(at, bytes);
    }
    let copies = [(); 3].map(|_| heap.take(stage::SYSTEM_SIZE));
    ngp_stage::collision_copies(&mut mem, copies);
    if race.head_gasket_durability > 1 {
        car_init::head_gasket(&mut mem, race.head_gasket_durability, heap.take(car_init::HEAD_GASKET_SIZE));
    }
    // NGP's heap objects: the speed ring as the DLL's load makes it, and the idle table's block.
    car_init::speed_ring(&mut mem, NGP, heap.take(car_init::SPEED_RING_SIZE));
    let idle_table = heap.take(car_init::IDLE_TABLE_SIZE);

    // The stack.
    mem.map(STACK_LO, &vec![0; (STACK_HI - STACK_LO) as usize]);
    built.push(("boot heap bytes", u64::from(heap.next.get() - HEAP_BASE)));
    Ok(Image {
        regions: pages.regions(),
        mem,
        regs: Regs { ecx: physics_at, esp: STEP_SLOT, ..Regs::default() },
        slot: STEP_SLOT,
        control_at: STACK_LO,
        car,
        seed,
        setup: complete,
        options,
        ngp: NGP,
        idle_table,
        ffb_sensitivity: race.ffb_sensitivity,
        built,
        blocks: heap.blocks.take(),
        detach,
    })
}

/// `CCar::UpdateA720(seed)` on the boot's car through its binding, as the setup's emit runs it, from below the step's
/// slot: the setups into the definitions and the car, and the bodies' first frames (`UpdateMrr`). Its callees are
/// bindings and stand-ins as a step's; one with neither stops it as in a step.
pub fn update_a720(pure: &Pure, image: &Image) {
    pure.call_at(image.slot - 0x100, UPDATE_A720, image.car, &[image.seed]);
}

/// NGP's per-car init (`car_init::init`) as the race starts, after `UpdateA720`.
pub fn ngp_init(mem: &mut Mem, image: &Image) {
    let car = car_init::Car {
        car: image.car,
        setup: image.setup,
        options: image.options,
        idle_table: image.idle_table,
        ffb_sensitivity: image.ffb_sensitivity,
    };
    car_init::init(mem, image.ngp, car);
    // The replay manager is the boot's stand-in (`load::new_replay_stand_in`, its first words only): NGP's copy of
    // the timestep into the manager (+0x18b908) would land in whatever block lies there (a stage block's word on
    // track-12_O), so the init runs with no manager.
    let replay = mem.u32(physics::TELEMETRY_GLOBAL);
    mem.set_u32(physics::TELEMETRY_GLOBAL, 0);
    ngp_stage::stage_init(mem, physics::cdef_global(mem));
    mem.set_u32(physics::TELEMETRY_GLOBAL, replay);
}

/// `--dump-image FILE` (a dev check, `harness/ngp_init_diff.py`): the image `mem` holds after `UpdateA720`, as records
/// `base size bytes` (u32 LE each) for the boot's pages and the pure allocator's, and `FILE.blocks`, one boot heap block
/// `address size` (hex) per line.
pub fn dump(mem: &Mem, image: &Image, path: &std::path::Path) -> std::io::Result<()> {
    use std::fmt::Write as _;
    let mut out = Vec::new();
    let allocator = (rbr_hybrid::pure::ALLOCATOR_HEAP_BASE..rbr_hybrid::pure::ALLOCATOR_HEAP_END)
        .step_by(PAGE as usize)
        .map_while(|at| mem.try_read(at, PAGE as usize).map(|_| (at, PAGE)));
    for (base, size) in image.regions.iter().copied().chain(allocator) {
        let bytes = mem.try_read(base, size as usize).expect("the image's regions are mapped");
        out.extend(base.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(bytes);
    }
    std::fs::write(path, out)?;
    let mut blocks = String::new();
    for (at, size) in &image.blocks {
        writeln!(blocks, "{at:08x} {size:x}").unwrap();
    }
    std::fs::write(path.with_extension("blocks"), blocks)
}
