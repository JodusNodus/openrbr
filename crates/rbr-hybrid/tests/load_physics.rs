//! The physics object as the load makes it, against lcb1's snap-0: the
//! `CPhysics` (`CPhysics::CDef::Vf4c`, 0x5284f0) on the physics definition, with its state (`CDef::Vf50`), scratch
//! (`CDef::Vf54`, the collision solver's constructor 0x4fc930 and `Vf08` inside) and time keeper. The keeper is the
//! gameplay loop's (`rbr_gameplay::timing::TimeKeeper::load` on the stage's markers, as `Game` makes it), stored
//! through the time keeper binding's layout (`store_new`: what `CTimeKeeper::Init` 0x4a0150 and the card's constructor
//! write, then the timing load's keys). The heap returns lcb1's blocks (the physics global's object and the pointers in
//! it, in the constructor's allocation order); the definition block is snap-0's (`load_defs.rs` builds it equal).
//!
//! The `.data` words the step reads (item 2) are stored too: the physics global, the profiler's frequency and queue (the
//! frame's `Tic` sample entered), the static constants (`load::store_statics`) and the signal manager's pointer. The
//! signal manager holds only the physics' connections (decision O1 (a)); at snap-0 each of the physics' five lists
//! holds the physics' connection alone, so the lists compare as bytes here. The replay manager the step reads through
//! its global (`CCar::UpdateSeed`'s `E19IsState2`) is a stand-in holding only its state word
//! (`load::new_replay_stand_in`), at snap-0's replay manager block. The constructor's collision event pool
//! (`E37Prefill5340`) is in `.data`.
//!
//! Runs only when `RBR_DATA` (the stage's `.dls`) and `RBR_PATCH` (the exe's sections) are set and lcb1's snap-0 is at
//! hand. With `RBR_LOAD_OUT`, writes the `physics` patch for `harness/compose_load.py`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use rbr_formats::{markers, pe};
use rbr_gameplay::timing::TimeKeeper;
use rbr_hybrid::timekeeper::layout as keeper_layout;
use rbr_physics::collision::layout as collision;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::construct::{self, Heap, ScratchHost};
use rbr_physics::physics_core::physics::{self, PhysicsHost};
use rbr_physics::physics_core::prof::{EnterHost, ProfHost};
use rbr_physics::physics_core::{layout, setup};
use rbr_physics::snapshot::Snapshot;
use rbr_physics::stage::layout as stage;
use rbr_physics::{car_body, load};

#[path = "../../rbr-physics/tests/common/patch.rs"]
mod patch;

/// The signal manager's pointer (`g_signalManager`) and its block's size.
const SIGNALS: u32 = 0x0089_3600;
const SIGNALS_SIZE: u32 = 0x109e8;
/// The physics' connections' lists in the signal manager (`physics::CONN_LIST`'s linked ones).
const LISTS: [u32; 5] = [0x3448, 0x4938, 0x4b50, 0x4f80, 0x5198];
/// `QueryPerformanceFrequency` in the reference run (Wine's).
const FREQUENCY: u64 = 10_000_000;
/// The profiler's frequency and queue (head, tail, count), the static constants, the collision mesh parameters and the
/// static collision system's vtable (its other words are the stage world's), and the replay manager's global.
const DATA_WORDS: [(u32, u32); 7] = [
    (layout::PHYSICS_GLOBAL, 4),
    (0x0165_b208, 8),
    (0x0165_f108, 12),
    (0x007c_7050, 0x2c),
    (0x007e_a250, 0x14),
    (0x0165_78a8, 4),
    (REPLAY_GLOBAL, 4),
];
/// The replay manager's global (`g_e19Ptr893634`).
const REPLAY_GLOBAL: u32 = 0x0089_3634;
/// The collision event pool's free list (head, tail, count) and its 0x80 events (`.data`, zero in the exe).
const EVENT_POOL_LIST: u32 = 0x008e_f7f4;
const EVENT_POOL: u32 = 0x008e_f810;
const EVENT_POOL_SIZE: u32 = 0x80 * 0xf0;
const STAGE: &str = "track-54_E";
/// The keeper's and the card's sizes.
const KEEPER_SIZE: u32 = 0x10;
const CARD_SIZE: u32 = 0x1e0;

/// The constructor's calls: the heap returns the given blocks in order, the solver's constructor and `Vf08` run their
/// ports, the hook stores the gameplay loop's keeper, and the links append to the signal manager's lists.
struct Host {
    blocks: VecDeque<(u32, u32)>,
    keeper: TimeKeeper,
    /// Set by `prefill_pool`.
    prefilled: bool,
}

impl Heap for Host {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        let (at, expected) = self.blocks.pop_front().expect("an allocation the original did not make");
        assert_eq!(size, expected, "the allocation at {at:#x}");
        at
    }
}

impl ScratchHost for Host {
    fn ctor_solver(&mut self, mem: &mut Mem, solver: u32, def: u32) {
        collision::ctor_solver(mem, solver, def);
    }

    fn solver_s08(&mut self, mem: &mut Mem, solver: u32) {
        collision::drain_events(mem, solver).expect("an empty event list");
    }
}

impl PhysicsHost for Host {
    /// `E37Prefill5340` (0x4f5340) fills the collision event pool (`.data`; the step takes events from it): `build` runs
    /// it once `cdef_new_physics` returns (nothing between reads the pool).
    fn prefill_pool(&mut self) {
        self.prefilled = true;
    }

    fn name_or_null(&mut self, _: u32) {}

    /// `E21Stub::Hook` (0x4a0150, `CTimeKeeper::Init`): the card is the next block.
    fn hook(&mut self, mem: &mut Mem, object: u32) -> u32 {
        let card = self.alloc(mem, CARD_SIZE);
        keeper_layout::store_new(mem, object, card, &self.keeper);
        object
    }

    fn link(&mut self, mem: &mut Mem, list: u32, conn: u32) {
        let head = mem.u32(SIGNALS) + list;
        car_body::layout::push_back_connection(mem, head, conn);
    }
}

/// `Tic`'s enter: the counter is the run's (`QueryPerformanceCounter`), not compared; the sample was not linked.
struct Profiler;

impl ProfHost for Profiler {
    fn counter(&mut self, _: u32) -> u32 {
        1
    }
}

impl EnterHost for Profiler {
    fn unlink(&mut self, _: u32) {
        unreachable!("the Tic sample is not in the queue before")
    }
}

fn snapshot_path() -> PathBuf {
    std::env::var_os("RBR_LCB1_BODIES_SNAPSHOT").map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../vectors/dumps/lcb1-bodies/lcb1-bodies.snap-0.bin")
    })
}

/// The objects built: (kind, address, size).
fn objects(snap0: &Mem) -> Vec<(&'static str, u32, u32)> {
    let physics = snap0.u32(layout::PHYSICS_GLOBAL);
    let keeper = snap0.u32(physics + 0x40);
    let signals = snap0.u32(SIGNALS);
    let mut out = vec![
        ("physics", physics, physics::PHYSICS_SIZE),
        ("physics_state", snap0.u32(physics + 0x20), construct::STATE_SIZE),
        ("physics_scratch", snap0.u32(physics + 0x28), construct::SCRATCH_SIZE),
        ("time_keeper", keeper, KEEPER_SIZE),
        ("time_card", snap0.u32(keeper + 4), CARD_SIZE),
        ("signal_manager_pointer", SIGNALS, 4),
        ("replay_stand_in", snap0.u32(REPLAY_GLOBAL), load::layout::REPLAY_STAND_IN_SIZE),
    ];
    out.push(("event_pool_list", EVENT_POOL_LIST, 12));
    out.push(("event_pool", EVENT_POOL, EVENT_POOL_SIZE));
    out.extend(LISTS.map(|list| ("signal_list", signals + list, 12)));
    out.extend(DATA_WORDS.map(|(at, n)| ("data_word", at, n)));
    out
}

fn build(snap0: &Mem, exe: &pe::Image, keeper: TimeKeeper, fill: u8) -> Mem {
    let mut mem = Mem::new();
    for name in [".rdata", ".data"] {
        let section = exe.section(name).expect(name);
        mem.map(section.address, &section.bytes);
    }
    let physics = snap0.u32(layout::PHYSICS_GLOBAL);
    let def = snap0.u32(physics + 0x24);
    mem.map(def, &snap0.read(def, setup::CDEF_SIZE as usize));
    mem.map(EVENT_POOL, &vec![fill; EVENT_POOL_SIZE as usize]);
    let signals = snap0.u32(SIGNALS);
    mem.map(signals, &vec![0; SIGNALS_SIZE as usize]);
    mem.set_u32(SIGNALS, signals);
    let blocks: VecDeque<(u32, u32)> = objects(snap0)[..5].iter().map(|&(_, at, size)| (at, size)).collect();
    for &(at, size) in &blocks {
        mem.map(at, &vec![fill; size as usize]);
    }
    let mut host = Host { blocks, keeper, prefilled: false };
    let made = physics::cdef_new_physics(&mut mem, &mut host, def);
    assert_eq!(made, physics);
    assert!(host.prefilled);
    collision::prefill_pool(&mut mem);
    assert!(host.blocks.is_empty(), "blocks left: {:x?}", host.blocks);
    layout::set_physics_global(&mut mem, made);
    layout::store_frequency(&mut mem, FREQUENCY);
    load::enter_tic_sample(&mut mem, &mut Profiler);
    load::store_statics(&mut mem);
    let system = mem.u32(stage::COLLISION_SYSTEM);
    mem.map(system, &vec![fill; stage::SYSTEM_SIZE as usize]);
    stage::collision_system_ctor(&mut mem, system);
    let replay = snap0.u32(REPLAY_GLOBAL);
    let size = load::layout::REPLAY_STAND_IN_SIZE;
    mem.map(replay, &vec![fill; size as usize]);
    host.blocks.push_back((replay, size));
    assert_eq!(load::new_replay_stand_in(&mut mem, &mut host), replay);
    mem
}

/// The bytes later load code writes in the objects, from snap-0 against the build. The step reads none of them before
/// writing them (the read sets of 300 steps from the composed snap-0) apart from the state's lists, which the stage and
/// car filing writes, `l` in the stage-world patch too.
fn later(kind: &str, o: u32) -> bool {
    match kind {
        // The entity list (+0x10), and the words at +0x54 and +0x58 the car load sets.
        "physics" => (0x10..0x14).contains(&o) || (0x54..0x5c).contains(&o),
        // The entity lists the stage and car filing link (+0x10, +0x450..+0x4b8).
        "physics_state" => (0x10..0x14).contains(&o) || (0x440..0x4b8).contains(&o),
        // The car (+0xe848, in the block `CScratch::Vf04` clears) is stored by the car load.
        "physics_scratch" => (0xe848..0xe84c).contains(&o),
        _ => false,
    }
}

#[test]
fn physics_matches_lcb1_snap0() {
    let (Some(data), Some(patch_dir)) = (std::env::var_os("RBR_DATA"), std::env::var_os("RBR_PATCH")) else {
        eprintln!("skipped the physics object: RBR_DATA or RBR_PATCH is not set");
        return;
    };
    let path = snapshot_path();
    if !path.is_file() {
        eprintln!("skipped the physics object: {} is absent", path.display());
        return;
    }
    let snap0 = Snapshot::open(&path).expect("lcb1-bodies snap-0").mem();
    let exe = std::fs::read(Path::new(&patch_dir).join("RichardBurnsRally_SSE.exe")).expect("the exe");
    let exe = pe::parse(&exe).expect("the exe's sections");
    let dls = std::fs::read(Path::new(&data).join("Maps").join(format!("{STAGE}.dls"))).expect("the stage's markers");
    let keeper = TimeKeeper::load(&markers::parse(&dls).expect("the markers"));
    let (zeros, ones) = (build(&snap0, &exe, keeper, 0), build(&snap0, &exe, keeper, 0xff));
    let mut patched = Vec::new();
    let mut differences = Vec::new();
    for (kind, at, size) in objects(&snap0) {
        let (ours, noise, game) =
            (zeros.read(at, size as usize), ones.read(at, size as usize), snap0.read(at, size as usize));
        let mut class = Vec::new();
        for (o, ((&a, &b), &g)) in ours.iter().zip(&noise).zip(&game).enumerate() {
            let o = o as u32;
            class.push(if later(kind, o) {
                b'l'
            } else if a != b {
                b'u'
            } else {
                if a != g {
                    differences.push(format!("{kind} {at:#x} +{o:#x}: built {a:02x}, snap-0 {g:02x}"));
                }
                b'w'
            });
        }
        let count = |c: u8| class.iter().filter(|&&k| k == c).count();
        eprintln!(
            "{kind} {at:#x}: {} bytes built and equal, {} unwritten, {} later",
            count(b'w') - differences.iter().filter(|d| d.starts_with(&format!("{kind} {at:#x} "))).count(),
            count(b'u'),
            count(b'l')
        );
        patched.push(patch::Patched { kind: kind.into(), at, ours, class, game });
    }
    assert!(differences.is_empty(), "{} bytes differ:\n{}", differences.len(), differences.join("\n"));
    patch::write("physics", &patched);
}
