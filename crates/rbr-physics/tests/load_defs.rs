//! The load's physics objects built from the retail files, as the load builds them (`rbr_physics::load`), against the
//! game's: the physics definition (its tweaks and bit-fields, the 256 materials and the collision solver's definition),
//! the registry's definitions (the car's, with its control unit, drive, engine, light system and passenger definitions;
//! the four wheels', with their spring damper and tyre definitions; the solids, the ground, the car body and the hinged
//! and mounted bodies), and every object of `CCompleteCarSetup` and `CCompleteCarSetupOptions`.
//!
//! The vectors hold the objects from the snap-0 dump of a Corolla gravel run: `vectors/load-wheel-defs.txt`, cut by
//! `harness/load_wheel_refs.py` (the wheel definitions and the two setup owners, object by object),
//! `vectors/load-car-def.txt`, cut by `harness/load_car_refs.py` (the car's definition and the 15 blocks its engine
//! definition allocates), and `vectors/load-registry-defs.txt`, cut by `harness/load_registry_refs.py` (the physics
//! definition, part by part, and the registry's other definitions). The test builds them at the same addresses (the
//! heap returns the game's blocks, by name or in the game's order): the owners by their constructors, then each file's
//! root group walked on its owner: `Misc/prime.lsp`, `Physics/physics.lsp`, `Physics/t_coroll/common.lsp` and
//! `gravel.lsp` on the physics definition, `setups/d_gravel.lsp` on `CCompleteCarSetup`, `michelin/tyres.lsp` and
//! `r_gravel.lsp` on `CCompleteCarSetupOptions`. Then, with the race's tyre type 4 and the run's driver aids, it does
//! what the settings code does (the drive options' choices and the driver aids into the setups), builds the car
//! instance (below), and copies the setups into the definitions and the wheels as the car does. The vtables and name
//! words come from the retail exe. Runs only when `RBR_DATA` and `RBR_PATCH` are set.
//!
//! The car instance (step 4, `car_instance_matches_the_game`): the car's factory (`CCar::CDef::Vf50`), the four
//! wheels' (`E51Wheel::CDef::Vf50`), the four hinged bodies' (`CHingedRigidBody::CDef::Vf50`), the three mounted bodies'
//! (`CMountedRigidBody::CDef::Vf50`) and the car body's (`CCarBody::CDef::Vf50x`), called as the game called them, against
//! the same run's dumps at each factory's return (`vectors/load-car-instance.txt`, cut by `harness/load_instance_refs.py`):
//! the car and its exclusion set, the current car word, the five signal lists the car's connections join, each wheel,
//! and each body with its exclusion set. The heap returns the blocks the
//! run's allocation log (`RBR_ALLOCLOG`) shows inside the factory calls, in order, holding what they held at the
//! factory's entry: so every byte is compared, the ones no constructor writes included, apart from the name words.
//! The words of the game's the constructors read and nothing here builds come from the car factory's entry dump.
//!
//! In the definitions, every byte is compared except:
//! - the bytes nothing writes, found by building twice on blocks filled with 0x00 and 0xff: the entity definitions'
//!   link array `+0x1c..+0x11c` (64 registry indices; only the car's has links, and the counts at `+0x18` are
//!   compared) and the solid definitions' `+0x11c..+0x120`, the vectors' padding words (`+0xc` of each `CVector`), the
//!   gaps between tweaks, the wheel definitions' `+0x3dc` and tail (`+0x974..`), the car and car body definitions'
//!   tails, the mounted definitions' `+0x22c`. The game's blocks hold what the heap left there (random floats, text, and
//!   in the ground's link array matrices and pointers of an earlier object);
//! - the name words, which point into the game's token heap: the names they point to are compared instead;
//! - the bytes code not built here writes: the wheel definitions' `+0x880..+0x900`, the mount points in the car's frame
//!   that `E51Wheel::M3A8A0` (0x53a8a0) writes from the car body (`M3D030`, the car's update); the hinged definitions'
//!   `+0x310..+0x340` and `+0x3d0..+0x470`, the frames `SimEFE0` writes there (`VfE070` and `BodyCompose` outputs,
//!   `entities::layout::efe0_frame`; `+0x320` is the hinge point less the car's centre of gravity), and the mounted
//!   definitions' `+0x220..+0x22c`, the base point in the car body's frame that `E45_M1980` (0x521980,
//!   `physics_core::layout::mounted_pose`) has `CCar::BodyCompose` (0x4e6a30) write. `CCar::UpdateMrr` (0x4e9c70) runs both
//!   for each hinged and mounted body: the car instance's (step 4). And the physics definition's three bit-fields'
//!   `+0x10`: each points at a list of its bits (vtable 0x7473e8, the bit count at `+0x14`) for the tweak menu, which
//!   the bit-field's destructor releases as a `CBOOLMenuItem`. Not physics.

use rbr_formats::data::Data;
use rbr_formats::lsp::{self, Node};
use rbr_formats::{pe, rbz};
use rbr_physics::load::{self, Registry};
use rbr_physics::mem::Mem;
use rbr_physics::ntweak::{ApplyHost, Unsupported};
use rbr_physics::physics_core::layout as physics_layout;
use rbr_physics::physics_core::layout::construct::Heap;
use rbr_physics::physics_core::layout::setup::CDEF_SIZE;
use rbr_physics::snapshot::Snapshot;
use rbr_physics::{car_body, collision, entities, rigid_body};
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

#[path = "common/patch.rs"]
mod patch;

/// The lcb1-bodies load dump (`snap-0`, with `snapat/` beside it): `RBR_LCB1_BODIES_SNAPSHOT`, else the one under
/// `RBR_SNAPS` (`dumps/lcb1-bodies/`), else the maintainers' vector store next to the worktree.
fn lcb1_snapshot() -> PathBuf {
    let name = "dumps/lcb1-bodies/lcb1-bodies.snap-0.bin";
    let mut candidates: Vec<PathBuf> =
        std::env::var_os("RBR_LCB1_BODIES_SNAPSHOT").map(PathBuf::from).into_iter().collect();
    candidates.extend(std::env::var_os("RBR_SNAPS").map(|dir| Path::new(&dir).join(name)));
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../vectors").join(name);
    candidates.into_iter().find(|path| path.is_file()).unwrap_or(store)
}

const VECTORS: [&str; 3] = [
    include_str!("vectors/load-wheel-defs.txt"),
    include_str!("vectors/load-car-def.txt"),
    include_str!("vectors/load-registry-defs.txt"),
];
/// The car instance at its factories' returns, cut by `harness/load_instance_refs.py` from the same run, with the calls,
/// the blocks and the words of the game's the constructors read.
const INSTANCE: &str = include_str!("vectors/load-car-instance.txt");
const INSTANCE_LINES: [&str; 3] = ["call", "alloc", "env"];
/// The 13 car placements, captured at 0x5267d0 from the same tanner-corolla run. Their pointers are omitted: the
/// inputs contain only the CSigStuff fields; the test runs them against the lcb1 allocation layout.
const PLACEMENTS: &str = include_str!("vectors/load-car-placements.txt");

/// Where the test puts the names and the solid definitions later files replace.
const NAMES: u32 = 0x1000_0000;
const REPLACED: u32 = 0x1200_0000;
/// The setup owners' sizes (up to the last member's end).
const COMPLETE_SIZE: u32 = 0x1700;
const OPTIONS_SIZE: u32 = 0x2790 + 7 * 0x6ac;
/// The race the vectors come from: the variant and its `physics.rbz`, the car's folder and surface, the tyre brand, the
/// tyre type the race chose (`TyreType04`, as 0x566250 returns it) and the driver aids (0x1660814, 0x1660ea8,
/// 0x1660ea0 and 0x1660ea4).
struct Race {
    physics: Option<std::path::PathBuf>,
    car: &'static str,
    surface: &'static str,
    brand: &'static str,
    tyre_type: u32,
    aids: load::DriverAids,
}

/// The reference run's: the Corolla on gravel, intermediate tyres.
const COROLLA: Race = Race {
    physics: None,
    car: "t_coroll",
    surface: "gravel",
    brand: "michelin",
    tyre_type: 4,
    aids: load::DriverAids { auto_gears: 0, gear_guard: 1, neutral_lock: 0, clutch_help: 1 },
};

impl Race {
    /// The files of `archive` (a data path): NGP's `physics.rbz` in its place under NGP.
    fn archive(&self, data: &Data, archive: &str) -> std::path::PathBuf {
        match &self.physics {
            Some(path) if archive == "Root/physics.rbz" => path.clone(),
            _ => data.resolve(archive).expect(archive),
        }
    }
}
/// The bytes code not built here writes, by kind: the car instance's frames in the definitions, and the tweak menu's
/// lists in the physics definition's bit-fields.
const LATER: [(&str, std::ops::Range<u32>); 7] = [
    ("wheel_def", 0x880..0x900),
    ("hinged_def", 0x310..0x340),
    ("hinged_def", 0x3d0..0x470),
    ("mounted_def", 0x220..0x22c),
    ("phys_tweaks", 0x60..0x64),
    ("phys_tweaks", 0x74..0x78),
    ("phys_tweaks", 0x88..0x8c),
];
/// One object of the vectors: its kind, position, address, bytes and name words (offset, name).
struct Object {
    kind: String,
    position: String,
    at: u32,
    bytes: Vec<u8>,
    names: Vec<(u32, Vec<u8>)>,
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

/// The objects of vector files: every line but the header and the instance file's `call`, `alloc` and `env` lines.
fn objects_of<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<Object> {
    texts
        .into_iter()
        .flat_map(|v| v.lines())
        .filter(|l| !l.starts_with('#') && !INSTANCE_LINES.contains(&l.split(' ').next().unwrap()))
        .map(|l| {
            let f: Vec<_> = l.split(' ').collect();
            Object {
                kind: f[0].to_string(),
                position: f[1].to_string(),
                at: u32::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap(),
                bytes: hex(f[3]),
                names: f[4..]
                    .iter()
                    .map(|n| {
                        let (offset, name) = n.split_once('=').unwrap();
                        (u32::from_str_radix(offset, 16).unwrap(), hex(name))
                    })
                    .collect(),
            }
        })
        .collect()
}

fn objects() -> Vec<Object> {
    objects_of(VECTORS)
}

fn number(text: &str) -> u32 {
    u32::from_str_radix(text.trim_start_matches("0x"), 16).unwrap()
}

/// The instance file's lines of `kind`, split.
fn instance_lines(kind: &str) -> Vec<Vec<&'static str>> {
    INSTANCE.lines().map(|l| l.split(' ').collect::<Vec<_>>()).filter(|f| f[0] == kind).collect()
}

/// The heap of the car instance: the blocks the game's factories got, in the log's order (each size checked).
struct InstanceHeap(std::collections::VecDeque<(u32, u32)>);

impl InstanceHeap {
    fn take(&mut self, size: u32) -> u32 {
        let (logged, at) = self.0.pop_front().expect("a block the game allocated");
        assert_eq!(size, logged, "the size of the block at {at:#x}");
        at
    }
}

impl Heap for InstanceHeap {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.take(size)
    }
}

impl rbr_physics::entities::exclusion::Heap for InstanceHeap {
    fn allocate(&mut self, bytes: u32) -> u32 {
        self.take(bytes)
    }
}

/// What the instance's blocks hold before the constructors run: a fill, or what the game's held.
#[derive(Clone, Copy)]
enum Fill {
    Byte(u8),
    Game,
}

/// The registry's factory for an instance call's kind, and the body factory (none for the car and the wheels).
fn factory_of(kind: &str) -> (&'static str, Option<load::BodyKind>) {
    match kind {
        "car" => ("Car", None),
        "wheel" => ("Wheel", None),
        "hinged" => ("HingedBody", Some(load::BodyKind::Hinged)),
        "mounted" => ("MountedBody", Some(load::BodyKind::Mounted)),
        "car_body" => ("CarBody", Some(load::BodyKind::CarBody)),
        _ => panic!("an instance call of kind {kind}"),
    }
}

/// The car instance on `mem` (step 4): the words of the game's the constructors read, the logged blocks filled, then
/// the factory calls in the game's order (the car, its wheels, its hinged, mounted and car bodies), each on the
/// definition the files made for its name. Returns the wheels.
fn build_instance(mem: &mut Mem, fill: Fill, registry: &Registry) -> [u32; 4] {
    for f in instance_lines("env") {
        mem.map(number(f[2]), &hex(f[3]));
    }
    let mut blocks = std::collections::VecDeque::new();
    for f in instance_lines("alloc") {
        let (size, at) = (number(f[1]), number(f[2]));
        match fill {
            Fill::Byte(b) => mem.map(at, &vec![b; size as usize]),
            Fill::Game => mem.map(at, &hex(f[3])),
        }
        blocks.push_back((size, at));
    }
    let mut heap = InstanceHeap(blocks);
    let mut wheels = Vec::new();
    for f in instance_lines("call") {
        let (kind, position) = (f[1], f[2]);
        let (def, a, b, result) = (number(f[3]), number(f[4]), number(f[5]), number(f[6]));
        let (factory, body) = factory_of(kind);
        let name = match kind {
            "car" => "CAR_ROOT".to_string(),
            "wheel" => format!("CAR_WHEEL_{position}"),
            _ => position.to_string(),
        };
        assert_eq!(registry.def(factory, &name), Some(def), "the {kind} {position}'s definition");
        let made = match (kind, body) {
            ("car", _) => load::new_car(mem, &mut heap, def, a, b),
            ("wheel", _) => load::new_wheel(mem, &mut heap, def, a, b),
            (_, Some(body)) => Ok(load::new_body(mem, &mut heap, body, def, a, b)),
            _ => unreachable!(),
        };
        assert_eq!(made, Ok(result), "the {kind} {position}");
        if kind == "wheel" {
            wheels.push(result);
        }
    }
    assert!(heap.0.is_empty(), "blocks left: {:x?}", heap.0);
    wheels.try_into().expect("four wheels")
}

/// The names on a bump arena, 16-byte aligned like the token heap; the heap blocks at the game's addresses, filled with
/// `fill`: a registry definition's by its name (the last name block, which the factory gets) and size, the others by
/// size in the order the game allocated them. The solid definitions `prime.lsp` makes for the names later files give to
/// another factory are not in the game's registry: they go to `REPLACED`.
struct Host {
    names: u32,
    last_name: u32,
    defs: BTreeMap<(Vec<u8>, u32), u32>,
    replaced: u32,
    blocks: BTreeMap<u32, Vec<u32>>,
    fill: u8,
}

impl ApplyHost for Host {
    fn alloc_name(&mut self, _: &mut Mem, len: u32) -> u32 {
        let at = self.names;
        self.names += len.next_multiple_of(0x10);
        self.last_name = at;
        at
    }

    /// Every object `load` hands on has 0x52f480 as its slot 17, which returns.
    fn after_parse(&mut self, _: &mut Mem, _: u32) {}

    /// `load::definitions` answers the registry's `Find`; the setups link nothing.
    fn def_index(&mut self, _: &Mem, _: &str) -> Option<u32> {
        None
    }

    /// `load::physics_file` makes the definitions; the setups make none.
    fn define(&mut self, _: &mut Mem, _: &str, _: u32, _: &str, _: &[Node]) -> Result<(), Unsupported> {
        unreachable!("the setup owners have no definition manager")
    }
}

impl Heap for Host {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        let name = c_string(mem, self.last_name);
        let at = match self.defs.get(&(name, size)) {
            Some(&at) => at,
            None if size == rigid_body::layout::DEF_SIZE => {
                self.replaced += size;
                self.replaced - size
            }
            None => self.blocks.get_mut(&size).expect("a block the game allocated").remove(0),
        };
        mem.map(at, &vec![self.fill; size as usize]);
        at
    }
}

/// The kinds of the registry's definitions that `load_registry_refs.py` cuts (the car's and the wheels' are cut with
/// their owners).
const REGISTRY_KINDS: [&str; 5] = ["solid_def", "ground_def", "car_body_def", "hinged_def", "mounted_def"];

/// The registry definitions of the objects, by name and size.
fn registry_defs(objects: &[Object]) -> BTreeMap<(Vec<u8>, u32), u32> {
    objects
        .iter()
        .filter(|o| REGISTRY_KINDS.contains(&o.kind.as_str()))
        .map(|o| ((o.names[0].1.clone(), o.bytes.len() as u32), o.at))
        .collect()
}

fn lsp_root(data: &Data, race: &Race, archive: &str, name: &str) -> Vec<Node> {
    let path = race.archive(data, archive);
    let mut archive = rbz::open(&path).unwrap();
    let mut text = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut text).unwrap();
    let file = lsp::parse(&text).unwrap();
    load::root(&file).expect(name).to_vec()
}

/// The NUL-terminated string at `at`.
fn c_string(mem: &Mem, at: u32) -> Vec<u8> {
    let mut out = Vec::new();
    while out.last() != Some(&0) {
        out.push(mem.u8(at + out.len() as u32));
    }
    out
}

/// The heap blocks of the objects, by size in the game's allocation order: the physics definition, the car's
/// definition and its engine definition's blocks (from `misc/prime.lsp`), then the wheel definitions (from
/// `gravel.lsp`).
fn blocks(objects: &[Object]) -> BTreeMap<u32, Vec<u32>> {
    let mut blocks: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for object in objects {
        let size = match object.kind.as_str() {
            "phys_def" => CDEF_SIZE,
            "car_def" => car_body::layout::DEF_SIZE,
            "wheel_def" => 0x980,
            "engine_block" => object.bytes.len() as u32,
            _ => continue,
        };
        blocks.entry(size).or_default().push(object.at);
    }
    blocks
}

/// The load's objects built from the files on the retail exe's sections, every block filled with `fill` first: the car
/// instance as its factories leave it, and everything after the setups' copies. Without `instance` (a race with no
/// instance vectors) there are no wheels to copy the wheels' setups through, so only the car's are copied.
fn build(data: &Data, race: &Race, objects: &[Object], fill: u8, instance: Option<Fill>) -> (Mem, Mem, Registry) {
    let exe = data.resolve("RichardBurnsRally_SSE.exe").expect("exe");
    let image = pe::parse(&std::fs::read(exe).unwrap()).unwrap();
    let mut mem = Mem::new();
    for name in [".rdata", ".data"] {
        let section = image.section(name).expect(name);
        mem.map(section.address, &section.bytes);
    }
    let first = |kind: &str| objects.iter().find(|o| o.kind == kind).unwrap().at;
    let (complete, options) = (first("complete_setup"), first("setup_options"));
    mem.map(complete, &vec![fill; COMPLETE_SIZE as usize]);
    mem.map(options, &vec![fill; OPTIONS_SIZE as usize]);
    let mut host = Host {
        names: NAMES,
        last_name: NAMES,
        defs: registry_defs(objects),
        replaced: REPLACED,
        blocks: blocks(objects),
        fill,
    };
    // The physics definition is allocated before any name.
    mem.map(NAMES, &[0]);
    load::construct_complete_setup(&mut mem, complete);
    load::construct_setup_options(&mut mem, options);

    let mut registry = Registry::default();
    let physics = load::new_physics_def(&mut mem, &mut host);
    let car = format!("Physics/{}/", race.car);
    for (archive, file) in [
        ("Root/misc.rbz", "Misc/prime.lsp".to_string()),
        ("Root/physics.rbz", "Physics/physics.lsp".to_string()),
        ("Root/physics.rbz", format!("{car}common.lsp")),
        ("Root/physics.rbz", format!("{car}{}.lsp", race.surface)),
    ] {
        let root = lsp_root(data, race, archive, &file);
        load::physics_file(&mut mem, &mut host, &mut registry, physics, &root).unwrap();
    }
    let def = |factory: &str| -> Vec<(String, u32)> {
        registry.entries().iter().filter(|e| e.factory == factory).map(|e| (e.name.clone(), e.def)).collect()
    };
    let wheels = def("Wheel");
    let names: Vec<_> = load::POSITIONS.iter().map(|p| format!("CAR_WHEEL_{p}")).collect();
    assert_eq!(wheels.iter().map(|(n, _)| n).collect::<Vec<_>>(), names.iter().collect::<Vec<_>>());
    let setups_file = lsp_root(data, race, "Root/physics.rbz", &format!("{car}setups/d_{}.lsp", race.surface));
    load::setup_file(&mut mem, &mut host, complete, &setups_file).unwrap();
    for file in [format!("Physics/{}/tyres.lsp", race.brand), format!("{car}r_{}.lsp", race.surface)] {
        let root = lsp_root(data, race, "Root/physics.rbz", &file);
        load::options_file(&mut mem, &mut host, options, &root).unwrap();
    }
    let setups = load::setups(complete);
    load::select_tyre_type(&mut mem, &setups, load::tyre_type(options, race.tyre_type));
    load::select_drive_options(&mut mem, setups.drive, load::drive_options(options));
    load::select_top_mounts(&mut mem, &setups, options);
    load::set_driver_aids(&mut mem, &setups, &race.aids);

    if let Some(fill) = instance {
        let wheel_at = build_instance(&mut mem, fill, &registry);
        let instance = mem.clone();
        load::copy_setups(&mut mem, wheel_at, &setups);
        let car_def = registry.def("Car", "CAR_ROOT").expect("the car's definition");
        load::copy_car_setups(&mut mem, car_def, &setups);
        return (instance, mem, registry);
    }
    let instance = mem.clone();
    let car_def = registry.def("Car", "CAR_ROOT").expect("the car's definition");
    load::copy_car_setups(&mut mem, car_def, &setups);
    (instance, mem, registry)
}

#[derive(Debug)]
struct Placement {
    position: String,
    object_type: u32,
    bytes: Vec<u8>,
}

fn placements() -> Vec<Placement> {
    PLACEMENTS
        .lines()
        .filter(|line| line.starts_with("placement "))
        .map(|line| {
            let fields: Vec<_> = line.split(' ').collect();
            let bytes = hex(fields[6]);
            assert_eq!(bytes.len(), 0x64, "{} CSigStuff bytes", fields[1]);
            let object_type = number(fields[2]);
            assert_eq!(u32::from_le_bytes(bytes[0x38..0x3c].try_into().unwrap()), object_type);
            Placement { position: fields[1].to_string(), object_type, bytes }
        })
        .collect()
}

#[derive(Clone, Copy)]
struct FactoryCall {
    kind: &'static str,
    position: &'static str,
    definition: u32,
    key: u32,
    parent: u32,
    result: u32,
}

fn factory_calls() -> VecDeque<FactoryCall> {
    INSTANCE
        .lines()
        .filter(|line| line.starts_with("call "))
        .map(|line| {
            let f: Vec<_> = line.split(' ').collect();
            FactoryCall {
                kind: f[1],
                position: f[2],
                definition: number(f[3]),
                key: number(f[4]),
                parent: number(f[5]),
                result: number(f[6]),
            }
        })
        .collect()
}

/// The factories' calls, answered from the original's records: each placement's definition, sort key and parent must
/// be the recorded call's, and its object is the one the game made (already in the dump).
struct ReplayFactory(VecDeque<FactoryCall>);

impl load::layout::PartFactory for ReplayFactory {
    fn make(
        &mut self,
        _: &mut Mem,
        definition: u32,
        key: u32,
        parent: u32,
    ) -> Result<u32, rigid_body::host::HostUnavailable> {
        let call = self.0.pop_front().expect("the matching factory call");
        assert_eq!(definition, call.definition, "{} {} definition", call.kind, call.position);
        assert_eq!(key, call.key, "{} {} sort key", call.kind, call.position);
        assert_eq!(parent, call.parent, "{} {} parent", call.kind, call.position);
        Ok(call.result)
    }
}

/// The cell blocks the car's placements would take: the dump's cells already hold blocks for them.
struct NoCellBlocks;

impl entities::exclusion::Heap for NoCellBlocks {
    fn allocate(&mut self, _: u32) -> u32 {
        panic!("the car's pre-existing cell blocks cover its placements")
    }
}

fn overlay(mem: &mut Mem, source: &Mem, object: &Object) {
    mem.map(object.at, &source.read(object.at, object.bytes.len()));
    for (offset, _) in &object.names {
        let name = c_string(source, source.u32(object.at + offset));
        mem.map(source.u32(object.at + offset), &name);
    }
}

/// Apply the factory's W/U patch to the A720 entry, retaining every word the original load rewrote later.
fn overlay_factory(mem: &mut Mem, source: &Mem, object: &Object, entry: &Mem) {
    let name_bytes: BTreeSet<u32> = object.names.iter().flat_map(|(at, _)| *at..*at + 4).collect();
    let end = object.at + object.bytes.len() as u32;
    let mut word = object.at & !3;
    while word < end {
        // `CCar` leaves the fourth lane of its position vector unwritten. The game's NotifyHost copies that old heap
        // lane to the window-start vector on wake; keep the entry value so the fixture models the same input.
        if object.kind == "car" && word == object.at + 0x14c {
            word += 4;
            continue;
        }
        let start = word.max(object.at);
        let finish = (word + 4).min(end);
        let offsets: Vec<_> = (start..finish).map(|at| (at - object.at) as usize).collect();
        if offsets.iter().all(|offset| !name_bytes.contains(&(object.at + *offset as u32)))
            && offsets.iter().all(|offset| entry.u8(object.at + *offset as u32) == object.bytes[*offset])
        {
            mem.map(start, &source.read(start, (finish - start) as usize));
        }
        word += 4;
    }
}

/// Start at the original's 0x4ea720 entry, remove the car cluster's prior filing/cell links, and rebuild the cluster
/// through `notify_stuff` using the Rust factory outputs and captured CSigStuff inputs.
fn place_car_cluster(data: &Data, objects: &[Object], expected: &Snapshot, fill: Fill) -> Mem {
    let (built_instance, built_defs, _) = build(data, &COROLLA, objects, 0, Some(fill));
    let instance = objects_of([INSTANCE]);
    let mut mem = expected.mem();
    let factory_guard = expected.mem();
    let expected_factory_objects: Vec<_> = instance
        .iter()
        .filter(|o| matches!(o.kind.as_str(), "car" | "wheel" | "hinged" | "mounted" | "car_body"))
        .collect();
    for object in objects {
        overlay(&mut mem, &built_defs, object);
    }
    for object in &instance {
        overlay_factory(&mut mem, &built_instance, object, &factory_guard);
    }
    for object in &expected_factory_objects {
        let link = object.at + 0x10;
        let list = mem.u32(link + 8);
        if list != 0 {
            physics_layout::unlink_item(&mut mem, link);
            mem.set_u32(list + 8, mem.u32(list + 8).wrapping_sub(1));
        }
        // CCarBody::Vf44 is the CBody path and has no cell-link member at +0x160; those bytes are
        // opaque data in the snap. The solids alone carry CSolid::CELL_LINK.
        if object.kind != "car_body" {
            collision::layout::move_to_cell(&mut mem, object.at + 0x160, 0);
        }
        // The placements fill the exclusion sets (`CCar::Vf150x`): they start empty.
        let set = mem.u32(object.at + 0x2c);
        if set != 0 {
            mem.set_u32(set, 0);
        }
    }

    let physics = physics_layout::physics(&mem);
    let scratch = mem.u32(physics + 0x28);
    mem.set_u32(scratch + 0xE888, u32::MAX);
    let metadata = expected.metadata();
    let stuff_base = metadata.stack_lo + 0x2000;
    let mut host = load::layout::CarParts {
        factory: ReplayFactory(factory_calls()),
        heap: &mut NoCellBlocks,
        sync_scratch: stuff_base + 0x2000,
    };
    for (i, placement) in placements().iter().enumerate() {
        let stuff = stuff_base + i as u32 * 0x80;
        let mut bytes = placement.bytes.clone();
        if placement.object_type != 0x27 {
            // The capture and lcb1 dump use separate Wine heap layouts; rebase the solid's hook pointer to
            // this dump's corresponding instance field before replaying the placement.
            let hook =
                u32::from_le_bytes(expected.mem().read(expected_factory_objects[i].at + 0x3cc, 4).try_into().unwrap());
            bytes[0x48..0x4c].copy_from_slice(&hook.to_le_bytes());
        }
        mem.map(stuff, &bytes);
        physics_layout::notify_stuff(&mut mem, &mut host, stuff)
            .unwrap_or_else(|_| panic!("{} type {:#x} placement", placement.position, placement.object_type));
        let expected_object = expected_factory_objects[i].at;
        assert_eq!(mem.u32(stuff + 0x3c), expected_object, "{} object", placement.position);
    }
    assert!(host.factory.0.is_empty(), "factory calls left: {}", host.factory.0.len());
    mem
}

/// The bytes of `object` that differ in `mem`, outside `skip` (offsets) and its name words; its names that differ.
fn differences(mem: &Mem, object: &Object, skip: &BTreeSet<u32>) -> Vec<String> {
    let ours = mem.read(object.at, object.bytes.len());
    let mut out = Vec::new();
    for (offset, name) in &object.names {
        let mine = c_string(mem, mem.u32(object.at + offset));
        if mine != *name {
            out.push(format!(
                "+{offset:#x}: name {:?}, the game's {:?}",
                String::from_utf8_lossy(&mine),
                String::from_utf8_lossy(name)
            ));
        }
    }
    for (i, (a, b)) in ours.iter().zip(&object.bytes).enumerate() {
        let offset = i as u32;
        if a != b && !skip.contains(&offset) {
            out.push(format!("+{offset:#x}: ours {a:02x} the game's {b:02x}"));
        }
    }
    out
}

/// Compares `objects` with the build on 0x00 and 0xff fills (`zeros`, `ones`), outside `later` and the bytes the two
/// builds disagree on; prints the counts by kind and panics on a difference. Returns the objects as built, each byte
/// classed (`patch`): compared `w`, never written `u`, in a name word `n`, written by a step not built here `l`.
fn compare(objects: &[Object], zeros: &Mem, ones: &Mem, later: &[(&str, std::ops::Range<u32>)]) -> Vec<patch::Patched> {
    let mut counts: BTreeMap<&str, [u32; 4]> = BTreeMap::new();
    let mut differing = Vec::new();
    let mut patched = Vec::new();
    for object in objects {
        let len = object.bytes.len() as u32;
        let a = zeros.read(object.at, len as usize);
        let b = ones.read(object.at, len as usize);
        let count = counts.entry(object.kind.as_str()).or_default();
        let mut skip = BTreeSet::new();
        let mut classes = Vec::new();
        for offset in 0..len {
            let class = if later.iter().any(|(kind, range)| object.kind == *kind && range.contains(&offset)) {
                3
            } else if a[offset as usize] != b[offset as usize] {
                1
            } else if object.names.iter().any(|(o, _)| (*o..*o + 4).contains(&offset)) {
                2
            } else {
                0
            };
            count[class] += 1;
            classes.push(b"wunl"[class]);
            if class != 0 {
                skip.insert(offset);
            }
        }
        patched.push(patch::Patched {
            kind: object.kind.clone(),
            at: object.at,
            ours: a,
            class: classes,
            game: object.bytes.clone(),
        });
        differing.extend(
            differences(zeros, object, &skip).into_iter().map(|d| format!("{} {} {d}", object.kind, object.position)),
        );
    }
    for (kind, [compared, unwritten, named, later]) in &counts {
        eprintln!(
            "{kind}: {compared} bytes compared; excluded: {unwritten} never written, {named} in name words, \
             {later} written by a step not built here"
        );
    }
    assert!(
        differing.is_empty(),
        "{} bytes differ:\n{}",
        differing.len(),
        differing.iter().take(200).cloned().collect::<Vec<_>>().join("\n")
    );
    patched
}

/// The same definitions under NGP: NGP's `physics.rbz` (`RBR_NGP_PHYSICS`, with its cars in their slots) parsed with
/// NGP's keys, against the objects `load_*_refs.py` cut from an NGP snapshot (`RBR_NGP_LOAD`: a folder with
/// `load-wheel-defs.txt`, `load-car-defs.txt` and `load-registry-defs.txt`). The reference is `ngp-rpl11`'s snapshot
/// at 6438: the Fiesta Rally3 in slot 5 (`s_i2003`, pirelli) on stage 11's gravel.
#[test]
fn ngp_definitions_match_the_game() {
    let (Some(data), Some(physics), Some(vectors)) = (
        Data::from_env().filter(|d| d.patch().is_some()),
        std::env::var_os("RBR_NGP_PHYSICS"),
        std::env::var_os("RBR_NGP_LOAD"),
    ) else {
        eprintln!("skipping: RBR_DATA, RBR_PATCH, RBR_NGP_PHYSICS or RBR_NGP_LOAD not set");
        return;
    };
    let dir = std::path::PathBuf::from(vectors);
    let texts: Vec<&'static str> = ["load-wheel-defs.txt", "load-car-defs.txt", "load-registry-defs.txt"]
        .iter()
        .map(|f| &*std::fs::read_to_string(dir.join(f)).unwrap().leak())
        .collect();
    let objects = objects_of(texts);
    let race = Race {
        physics: Some(physics.into()),
        car: "s_i2003",
        surface: "snow",
        brand: "pirelli",
        tyre_type: 3,
        ..COROLLA
    };
    let ((_, zeros, _), (_, ones, _)) =
        (build(&data, &race, &objects, 0, None), build(&data, &race, &objects, 0xff, None));
    let later: Vec<_> =
        LATER.iter().chain(&NGP_LATER).cloned().chain(ngp_torque_rpms()).chain(wheel_setup_copy(&data)).collect();
    patch::write("ngp-definitions", &compare(&objects, &zeros, &ones, &later));
}

/// The bytes NGP writes after the files (layer 2 of `notes/ngp.md`'s plan, not built here), and one word that holds an
/// address in NGP's memory: its copy of `CUnsigned`'s vtable at the drive definition's +0x58c (`NGP_TABLE_VTABLE`, the
/// copy's address in NGP's file here). NGP's per-car init (`0x1003f5b0`) rewrites the drive type (+0x153c), the engine
/// definition's tables (+0x1bac..+0x3bc8, `0x1005ab10` scaling them), the words after `CylinderVolume_NGP`, the engine
/// type (`EngineType_NGP` 11 becomes 1) and the real at +0x40ac, and scales the wheels' and tyres' values (by the car's
/// factor, 1.00705 for this car); the materials' grip comes from its per-stage tables; `MaxTimeStep` becomes 1/720
/// (`0x1004e3a7`).
const NGP_LATER: [(&str, std::ops::Range<u32>); 19] = [
    ("car_def", 0x1488..0x148c),
    ("car_def", 0x153c..0x1540),
    ("car_def", 0x1bac..0x3bc8),
    ("car_def", 0x3fe8..0x4024),
    ("car_def", 0x4088..0x408c),
    ("car_def", 0x40ac..0x40b0),
    ("wheel_def", 0x538..0x53c),
    ("wheel_def", 0x5b8..0x5cc),
    ("wheel_def", 0x608..0x60c),
    ("wheel_def", 0x678..0x698),
    ("wheel_def", 0x6b8..0x6d8),
    ("wheel_def", 0x6f0..0x710),
    ("wheel_def", 0x730..0x7b0),
    ("wheel_def", 0x7dc..0x7e0),
    ("wheel_def", 0x810..0x81c),
    ("wheel_def", 0x930..0x944),
    ("wheel_def", 0x960..0x974),
    ("material", 0x18..0x15c),
    ("phys_tweaks", 0x1c..0x20),
];

/// The values of `Tq_01_RPM_NGP` to `Tq_15_RPM_NGP` (engine definition +0x2270, 0x20 apart), which NGP's per-car init
/// turns from rpm into rad/s.
fn ngp_torque_rpms() -> impl Iterator<Item = (&'static str, std::ops::Range<u32>)> {
    (0..15).map(|k| ("car_def", 0x3c00 + 0x20 * k..0x3c04 + 0x20 * k))
}

/// The bytes of a wheel definition `load::copy_setups` writes, from the 1.02 build, which has the car instance the copy
/// needs: the NGP test builds none, so it leaves them out.
fn wheel_setup_copy(data: &Data) -> Vec<(&'static str, std::ops::Range<u32>)> {
    let objects = objects();
    let mut offsets = BTreeSet::new();
    for fill in [0, 0xff] {
        let (before, after, _) = build(data, &COROLLA, &objects, fill, Some(Fill::Byte(fill)));
        for wheel in objects.iter().filter(|o| o.kind == "wheel_def") {
            let len = wheel.bytes.len();
            let (a, b) = (before.read(wheel.at, len), after.read(wheel.at, len));
            offsets.extend((0..len as u32).filter(|&k| a[k as usize] != b[k as usize]));
        }
    }
    offsets.into_iter().map(|k| ("wheel_def", k..k + 1)).collect()
}

#[test]
fn car_instance_matches_the_game() {
    let Some(data) = Data::from_env().filter(|d| d.patch().is_some()) else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let objects = objects();
    let instance = objects_of([INSTANCE]);
    let ((zeros, _, _), (ones, _, _)) = (
        build(&data, &COROLLA, &objects, 0, Some(Fill::Byte(0))),
        build(&data, &COROLLA, &objects, 0xff, Some(Fill::Byte(0xff))),
    );
    let mut kept: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for object in &instance {
        let len = object.bytes.len();
        let (a, b) = (zeros.read(object.at, len), ones.read(object.at, len));
        let count = kept.entry(object.kind.as_str()).or_default();
        count.0 += a.iter().zip(&b).filter(|(a, b)| a != b).count();
        count.1 += len;
    }
    for (kind, (kept, len)) in kept {
        eprintln!("{kind}: {kept} of {len} bytes not written (or copied from such bytes)");
    }
    let (game, _, _) = build(&data, &COROLLA, &objects, 0, Some(Fill::Game));
    // The bytes as the 0x00 build has them, those the two builds disagree on (the blocks' old bytes) classed `U`, the
    // others `W`: both only stand while the game's bytes are still these (`harness/compose_load.py`).
    let mut patched = compare(&instance, &game, &game, &[]);
    for p in &mut patched {
        let len = p.ours.len();
        let (a, b) = (zeros.read(p.at, len), ones.read(p.at, len));
        for (i, c) in p.class.iter_mut().enumerate() {
            if *c == b'w' {
                *c = if a[i] == b[i] { b'W' } else { b'U' };
            }
        }
        // This old heap lane is read by NotifyHost's window-start copy. Preserve snap-0's value rather than zeroing
        // it as an unwritten constructor byte; the placement test proves the copy into car+0x33c.
        if p.kind == "car" {
            p.class[0x14c..0x150].fill(b'g');
        }
        p.ours = a;
    }
    patch::write("car-instance", &patched);
}

#[test]
fn car_placements_match_lcb1_a720_entry() {
    let Some(data) = Data::from_env().filter(|d| d.patch().is_some()) else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let snapshot_path = lcb1_snapshot();
    if !snapshot_path.is_file() {
        eprintln!("skipped car placement parity: {} is absent", snapshot_path.display());
        return;
    }
    let entry_path = snapshot_path.parent().unwrap().join("snapat").join("RBRRecompSnapAt-004ea720-1.bin");
    if !entry_path.is_file() {
        eprintln!("skipped car placement parity: {} is absent", entry_path.display());
        return;
    }
    let objects = objects();
    let entry = Snapshot::open(&entry_path).expect("lcb1 UpdateA720 entry");
    let expected = entry.mem();
    let ours = place_car_cluster(&data, &objects, &entry, Fill::Game);
    let zero = place_car_cluster(&data, &objects, &entry, Fill::Byte(0));
    let ones = place_car_cluster(&data, &objects, &entry, Fill::Byte(0xff));
    let instance = objects_of([INSTANCE]);
    let mut differences = Vec::new();
    let mut patched = Vec::new();
    let mut class_counts = [0usize; 4];
    for object in instance
        .iter()
        .filter(|o| matches!(o.kind.as_str(), "car" | "wheel" | "hinged" | "mounted" | "car_body" | "exclusions"))
    {
        let built = ours.read(object.at, object.bytes.len());
        let zero_bytes = zero.read(object.at, object.bytes.len());
        let one_bytes = ones.read(object.at, object.bytes.len());
        let mut class = Vec::with_capacity(object.bytes.len());
        let mut names = BTreeSet::new();
        for (at, _) in &object.names {
            names.extend(*at..*at + 4);
        }
        for offset in 0..object.bytes.len() {
            let offset = offset as u32;
            let c = if names.contains(&offset) {
                b'n'
            } else if zero_bytes[offset as usize] == one_bytes[offset as usize] {
                b'W'
            } else {
                b'U'
            };
            class_counts[match c {
                b'W' => 0,
                b'U' => 1,
                b'n' => 2,
                _ => unreachable!(),
            }] += 1;
            class.push(c);
            if c == b'n' {
                continue;
            }

            if built[offset as usize] != expected.u8(object.at + offset) {
                differences.push(format!(
                    "{} {} +{offset:#x}: Rust {:02x}, lcb1 A720 entry {:02x}",
                    object.kind,
                    object.position,
                    built[offset as usize],
                    expected.u8(object.at + offset)
                ));
            }
        }
        patched.push(patch::Patched {
            kind: object.kind.clone(),
            at: object.at,
            ours: zero_bytes,
            class,
            game: expected.read(object.at, object.bytes.len()),
        });
    }
    assert!(
        differences.is_empty(),
        "{} car placement bytes differ:\n{}",
        differences.len(),
        differences.iter().take(100).cloned().collect::<Vec<_>>().join("\n")
    );
    eprintln!("car placements: {} W, {} U, {} name bytes", class_counts[0], class_counts[1], class_counts[2]);
    patch::write("car-placements", &patched);
}

struct DamageOpsTestCalls;

impl car_body::layout::DamageOpsCalls for DamageOpsTestCalls {
    fn call(&mut self, mem: &mut Mem, callee: u32, this: u32, args: &[u32]) {
        match callee {
            0x4ef720 => {
                let [index, scale, value, tag, ..] = args else { panic!("AddFunctor arguments") };
                let _ = car_body::damage::add_functor(mem, this, *index as i32, *scale, *value, *tag);
            }
            0x4ef910 => car_body::layout::init_damage_ops_tables(self, mem, this),
            0x465d80 => mem.map(this, &[0; 16]),
            0x43c5b0 => {
                let value = mem.read(args[0], 16);
                mem.map(this, &value);
            }
            _ => {}
        }
    }
}

fn build_damage_ops(fill: u8, at: u32) -> Mem {
    let mut mem = Mem::new();
    mem.map(at, &vec![fill; car_body::layout::DAMAGE_OPS_SIZE as usize]);
    mem.map(0x893600, &0x2000_0000u32.to_le_bytes());
    mem.map(0x2000_0000, &[0; 0x10000]);
    mem.map(0x3000_0000, &[fill; 0x40]);
    car_body::layout::construct_damage_ops(&mut mem, at);
    car_body::damage::set_level(&mut mem, at, 1);
    car_body::layout::damage_level_fractions(&mut DamageOpsTestCalls, &mut mem, at, 0x3000_0000);
    mem
}

#[test]
fn damage_ops_tables_match_lcb1_load_dump() {
    let snapshot_path = lcb1_snapshot();
    if !snapshot_path.is_file() {
        eprintln!("skipped damage table parity: {} is absent", snapshot_path.display());
        return;
    }
    let snapshot = Snapshot::open(&snapshot_path).expect("lcb1-bodies snap-0");
    let snap0 = snapshot.mem();
    let after_path = snapshot_path.parent().unwrap().join("snapat").join("RBRRecompSnapAt-004ea720-1-after.bin");
    if !after_path.is_file() {
        eprintln!("skipped damage table parity: {} is absent", after_path.display());
        return;
    }
    let after = Snapshot::open(&after_path).expect("lcb1-bodies A720 return dump");
    let reference = after.mem();
    let car = snap0.u32(0x8ef660);
    assert_eq!(reference.u32(0x8ef660), car, "the lcb1 car address");
    let damage = reference.u32(car + 0x3cc);
    let zeros = build_damage_ops(0, damage);
    let ones = build_damage_ops(0xff, damage);
    let ours = zeros.read(damage, car_body::layout::DAMAGE_OPS_SIZE as usize);
    let noise = ones.read(damage, car_body::layout::DAMAGE_OPS_SIZE as usize);
    let game = reference.read(damage, car_body::layout::DAMAGE_OPS_SIZE as usize);
    assert_eq!(zeros.u32(damage + 0x6e8), reference.u32(damage + 0x6e8));
    assert_eq!(zeros.u32(damage + 0x770), reference.u32(damage + 0x770));
    // UpdateA720 writes the seed word after Slot4 returns.
    let written_later = |offset: usize| (0x3de8..=0x3deb).contains(&offset);
    // Snap-0 holds the A720 return's bytes (standalone-s4: the step reads this object from snap-0 on).
    let snap0_bytes = snap0.read(damage, car_body::layout::DAMAGE_OPS_SIZE as usize);
    let mut class = Vec::new();
    let mut differences = Vec::new();
    for (offset, ((&a, &b), (&expected, &at_snap0))) in
        ours.iter().zip(&noise).zip(game.iter().zip(&snap0_bytes)).enumerate()
    {
        class.push(if written_later(offset) {
            b'l'
        } else if a != b {
            b'u'
        } else {
            b'w'
        });
        for (dump, value) in [("A720 return", expected), ("snap-0", at_snap0)] {
            if a == b && a != value && !written_later(offset) {
                differences.push(format!("+{offset:#x}: built {a:02x}, {dump} {value:02x}"));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "{} stable damage-op bytes differ:\n{}",
        differences.len(),
        differences.iter().take(80).cloned().collect::<Vec<_>>().join("\n")
    );
    patch::write(
        "damage-ops",
        &[patch::Patched { kind: "damage_ops".into(), at: damage, ours, class, game: snap0_bytes }],
    );
}

/// The damage host as the game makes it (`E60Flow::M569270`: `E60DamageHost::Ctor` 0x566840, then each slot's `Vf0c`,
/// `CCarDamageHandler::ResetDamage` 0x4ec1e0), then the car's handler (slot 0) as the car load leaves it: two more
/// `ResetDamage` calls (its counter, +0x3a10, is 3 at snap-0; those callers are not identified), `Set3FB0(1)` and
/// `Slot4`, built on 0x00 and 0xff fills at lcb1's host block (`[car + 0x3cc] - 0x10`), against lcb1's snap-0.
fn build_damage_host(fill: u8, host: u32) -> Mem {
    let mut mem = Mem::new();
    mem.map(host, &vec![fill; car_body::layout::DAMAGE_HOST_SIZE as usize]);
    mem.map(0x893600, &0x2000_0000u32.to_le_bytes());
    mem.map(0x2000_0000, &[0; 0x10000]);
    mem.map(0x3000_0000, &[fill; 0x40]);
    car_body::layout::construct_damage_host(&mut mem, host);
    let slot = |k: u32| host + 0x10 + car_body::layout::DAMAGE_OPS_SIZE * k;
    for k in 0..4 {
        car_body::damage::reset_damage(&mut car_body::layout::ResetPorts, &mut mem, slot(k));
    }
    let car_ops = slot(0);
    for _ in 0..2 {
        car_body::damage::reset_damage(&mut car_body::layout::ResetPorts, &mut mem, car_ops);
    }
    car_body::damage::set_level(&mut mem, car_ops, 1);
    car_body::layout::damage_level_fractions(&mut DamageOpsTestCalls, &mut mem, car_ops, 0x3000_0000);
    mem
}

/// `build_damage_host` against lcb1's snap-0 (the step reads 484 bytes of the
/// car's handler, 436 of them `ResetDamage`'s). Every byte of the car's handler (slot 0) the build writes is equal to
/// snap-0's, apart from one word `UpdateA720` clears (`l`). The other slots and the host's header and tail are compared
/// too; the bytes where snap-0 differs from a stable build are later code's (`l`, counted).
#[test]
fn damage_host_matches_lcb1_snap0() {
    let snapshot_path = lcb1_snapshot();
    if !snapshot_path.is_file() {
        eprintln!("skipped the damage host: {} is absent", snapshot_path.display());
        return;
    }
    let snap0 = Snapshot::open(&snapshot_path).expect("lcb1-bodies snap-0").mem();
    let car = snap0.u32(0x8ef660);
    let host = snap0.u32(car + 0x3cc) - 0x10;
    let size = car_body::layout::DAMAGE_HOST_SIZE as usize;
    let ours = build_damage_host(0, host).read(host, size);
    let noise = build_damage_host(0xff, host).read(host, size);
    let game = snap0.read(host, size);
    let ops = car_body::layout::DAMAGE_OPS_SIZE as usize;
    let car_ops = 0x10..0x10 + ops;
    // The fourth `E44_FillOnes` triple's first word (+0x3da8) is 1 after ResetDamage and 0 at snap-0: a hinged body's
    // flag link (+0x650, `Set650`) points at it, and `CHingedRigidBody::SimEFE0` (0x51efe0, under `UpdateA720`'s
    // `UpdateMrr`) clears it (the A720 run builds it). The step reads it.
    let cleared = 0x10 + 0x3da8..0x10 + 0x3dac;
    let mut class = Vec::new();
    let mut differences = Vec::new();
    let mut later = BTreeMap::new();
    for (o, ((&a, &b), &g)) in ours.iter().zip(&noise).zip(&game).enumerate() {
        class.push(if cleared.contains(&o) {
            b'l'
        } else if a != b {
            b'u'
        } else if a != g {
            if car_ops.contains(&o) {
                differences.push(format!("+{:#x}: built {a:02x}, snap-0 {g:02x}", o - 0x10));
            }
            *later.entry((o.max(0x10) - 0x10) / ops).or_insert(0) += 1;
            b'l'
        } else {
            b'w'
        });
    }
    let count = |c: u8| class.iter().filter(|&&k| k == c).count();
    let read_by_step = |o: usize| (0x10 + 0x3a10..0x10 + 0x3dd0).contains(&o);
    let reset_bytes = (0..size).filter(|&o| read_by_step(o) && class[o] == b'w').count();
    eprintln!(
        "damage host {host:#x}: {} bytes built and equal to snap-0 ({reset_bytes} in the handler's +0x3a10..+0x3dd0), {} \
         unwritten, {} later (by slot: {later:?})",
        count(b'w'),
        count(b'u'),
        count(b'l'),
    );
    assert!(
        differences.is_empty(),
        "{} of the car's handler bytes differ:\n{}",
        differences.len(),
        differences.iter().take(80).cloned().collect::<Vec<_>>().join("\n")
    );
    patch::write("damage-host", &[patch::Patched { kind: "damage_host".into(), at: host, ours, class, game }]);
}

/// A root entity's constructor makes no owner calls, and the singleton is empty: no call reaches this.
struct NoCalls;

impl rbr_physics::entities::frame::Owner for NoCalls {
    fn hook(
        &mut self,
        _: u32,
        _: rbr_physics::entities::frame::Hook,
        _: rbr_physics::entities::frame::Form,
        _: u32,
        _: u32,
    ) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
    fn gate(&mut self, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
    fn frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
    fn old_frame_of(&mut self, _: u32) -> u32 {
        unreachable!("a root entity's frames call no owner")
    }
}

impl rbr_physics::entities::ground::Replace for NoCalls {
    fn delete(&mut self, _: u32) {
        unreachable!("the singleton is empty at the first ground")
    }
}

/// The ground object (`CGround::CDef::Vf50`, 0x51c810) on the ground definition the files made, at lcb1's ground block,
/// against lcb1's snap-0 (the step reads 45 of its bytes). A root entity (no parent,
/// sort key 0). Bytes later load code writes are `l`: the entity-list link words +0x10 and +0x18 (the filing after the
/// constructor; `load_stage.rs` compares the ground's link). The options' word +0x60 is the constructor's
/// (`CEntity::CEntity` 0x51a640 stores the options with the low bit set, `or ecx, 1` at 0x51a6da: 0x81). The name word
/// is `n`.
#[test]
fn ground_matches_lcb1_snap0() {
    let snapshot_path = lcb1_snapshot();
    if !snapshot_path.is_file() {
        eprintln!("skipped the ground: {} is absent", snapshot_path.display());
        return;
    }
    use rbr_physics::entities::layout as entities_layout;
    let snap0 = Snapshot::open(&snapshot_path).expect("lcb1-bodies snap-0").mem();
    let def = objects().into_iter().find(|o| o.kind == "ground_def").expect("the ground definition").at;
    let at = snap0.u32(entities::layout::GROUND_SINGLETON);
    const IDENTITY: u32 = 0x7cbb00;
    let build = |fill: u8| {
        let mut mem = Mem::new();
        mem.map(at, &vec![fill; entities::layout::GROUND_SIZE as usize]);
        mem.map(entities::layout::GROUND_SINGLETON, &[0; 4]);
        mem.map(IDENTITY, &snap0.read(IDENTITY, 4));
        mem.map(def + 4, &snap0.read(def + 4, 4));
        let mut heap = |size: u32| {
            assert_eq!(size, entities::layout::GROUND_SIZE);
            at
        };
        struct Block<'a>(&'a mut dyn FnMut(u32) -> u32);
        impl rbr_physics::entities::exclusion::Heap for Block<'_> {
            fn allocate(&mut self, bytes: u32) -> u32 {
                (self.0)(bytes)
            }
        }
        let made = entities_layout::ground_make(&mut mem, def, 0, 0, &mut Block(&mut heap), &mut NoCalls);
        assert_eq!(made, at);
        assert_eq!(mem.u32(entities::layout::GROUND_SINGLETON), at);
        mem.read(at, entities::layout::GROUND_SIZE as usize)
    };
    let (zeros, ones) = (build(0), build(0xff));
    let game = snap0.read(at, entities::layout::GROUND_SIZE as usize);
    let later = |o: usize| (0x10..0x14).contains(&o) || (0x18..0x1c).contains(&o);
    let name = |o: usize| (0x3c..0x40).contains(&o);
    let mut class = Vec::new();
    let mut differences = Vec::new();
    for (o, ((&a, &b), &g)) in zeros.iter().zip(&ones).zip(&game).enumerate() {
        class.push(if later(o) {
            b'l'
        } else if name(o) {
            b'n'
        } else if a != b {
            b'u'
        } else {
            b'w'
        });
        if a == b && a != g && !later(o) && !name(o) {
            differences.push(format!("+{o:#x}: built {a:02x}, snap-0 {g:02x}"));
        }
    }
    let count = |c: u8| class.iter().filter(|&&k| k == c).count();
    eprintln!(
        "ground {at:#x}: {} bytes built and equal to snap-0, {} unwritten, {} later, {} name",
        count(b'w'),
        count(b'u'),
        count(b'l'),
        count(b'n')
    );
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    assert_eq!(snap0.u32(at + 0x34), def, "snap-0's ground is on the built definition");
    patch::write("ground", &[patch::Patched { kind: "ground".into(), at, ours: zeros, class, game }]);
}
