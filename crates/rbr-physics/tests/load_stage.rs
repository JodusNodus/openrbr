//! The stage world built from the retail files, as the load builds it (`rbr_physics::stage`), against the game's: the
//! Tanner stage (`Maps/track-54_E`) of the Corolla gravel run.
//!
//! The vectors (`vectors/load-stage-world.txt`, cut by `harness/load_stage_refs.py` from the run's snap-0) hold the
//! objects at the game's addresses: the collision system's fields and its relocated `.col` image, the brake wall and
//! its manager's word, the cell grid with its table's count and nonzero cells, the mesh array, its 33 water meshes and
//! the link blocks they hang in, the E44 material table, the `.mat` image and the track state's word for it, the
//! physics state's entity lists and the ground object filed before the wall, the drive line, the `.trk` collision
//! meshes (the track state's list, each mesh's descriptor, name, shape record, vertices and faces, and the collision
//! grid of each id), and the placements' objects (each `CSolid`, its entity and cell links, and the link blocks they
//! reach first). The images, the drive line past its count and each solid come as FNV-1a digests (of each 4 KiB page; a
//! solid's of its bytes with the vectors' mask zeroed). The test builds them at the same addresses (the heap returns
//! the game's blocks by size, in the game's order, per file; a placement's solid is the k-th after the waters in the
//! state's list), on the retail exe's `.rdata` and `.data`, from the state before the stage load: an empty collision
//! system, mesh array and material table, no brake wall, the cell grid an earlier `E45_MD60` built with no wall (the
//! snap's grid has the default area), free collision grids from id 10, the physics state's wall list holding the ground
//! object and its mesh list empty. The race's material words are `new` and `damp` (the entry the game chose). Inputs
//! from snap-0, not built: the physics object's words, its definition's debug words, its parent stack (empty), the
//! definition registry and the solids' definitions' vtables and names, the body event signal's listeners (only the
//! idle replay manager), and the placements' sort keys (the interactive objects' handles). Runs only when `RBR_DATA`
//! and `RBR_PATCH` are set.
//!
//! Every byte is compared except:
//! - the bytes nothing writes, found by building twice on blocks filled with 0x00 and 0xff (in a solid, its mask's
//!   bytes, which the test checks are those);
//! - the bytes the original leaves uninitialised and reads: bytes 4-6 of each face (the heap's), which the build
//!   takes from the game's blocks; the `w` lanes of the drive line's entries, which copy them; a solid's bounds' `w`
//!   lanes (`+0x8c`, `+0x9c`), which `SyncGrid` takes from the matrix's unset tenth word (`+0x134`);
//! - the bytes code not built here writes: the objects filed in the cells after the stage load (the fence's 40
//!   `CDummyObject`s, a wheel, and a solid made later, the state list's last): the cells and links the Rust build
//!   leaves empty, in a link the game holds more objects in its head and count and the previous word of the object
//!   that was its head (objects go in at the head), the last placement's next word in the entity list, that list's tail
//!   and count, and the later solid.
//!
//! `RBR_STAGE_REFS` names another run's vectors (`harness/load_stage_refs.py` on its snap-0) to use instead. With
//! `RBR_LOAD_OUT` the build is written as a patch for `harness/compose_load.py` (`common/patch.rs`); the entity lists
//! the stage leaves alone are classed as other load code's there.

use rbr_formats::data::Data;
use rbr_formats::{pe, trk};
use rbr_physics::collision::layout::grid_hosts::AllocHost;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout as physics;
use rbr_physics::physics_core::layout::construct::Heap;
use rbr_physics::stage::layout::{self as stage, StageHost};
use rbr_physics::stage::Stuff;
use std::collections::{BTreeMap, HashMap, VecDeque};

mod common;

use common::hex;

#[path = "common/patch.rs"]
mod patch;

const VECTORS: &str = include_str!("vectors/load-stage-world.txt");
const STAGE: &str = "Maps/track-54_E";
const WEAR: &[u8] = b"new";
const WETNESS: &[u8] = b"damp";
const PAGE: usize = 0x1000;
/// The collision system's object before the stage load, and the brake-wall manager's (vtable, wall, a third word).
const SYSTEM_SIZE: u32 = 0x1014;
const MANAGER_SIZE: u32 = 0xc;
/// A cell link (head, tail, count, item).
const LINK: usize = 0x10;
/// The collision-grid bank, ids 0 to 31.
const GRID_BANK: u32 = 0x8f_7030;
const GRID_SIZE: u32 = 0x6b0;
/// The drive line's size (its count is the last word), entry stride and the tag and spare bytes of an entry.
const DRIVE_LINE_SIZE: u32 = 0x75314;
const DRIVE_COUNT: u32 = 0x75310;
const DRIVE_STRIDE: usize = 0x30;
/// The physics state's entity lists (from +0x450), the wall's (+0x470) and the waters' (+0x460).
const ENTITY_LISTS: u32 = 0x450;
const WALL_LIST: u32 = 0x470;
/// The placements of track-54_E's 21 meshes, and those of the meshes that ask for a handle (kinds 1, 3, 6 and 15).
const PLACEMENTS: usize = 3138;
const HANDLES: usize = 1064;
/// A placement's object (`CSolid`).
const SOLID_SIZE: u32 = 0x3d0;
/// Free memory for the original's temporaries, handed out in turn (each lives for one placement at most).
const SCRATCH: u32 = 0x7000_0000;
const SCRATCH_SIZE: u32 = 0x1_0000;
/// The vector kinds that are the load's inputs, not compared.
const INPUTS: [&str; 9] = [
    "physics_state",
    "physics",
    "physics_flags",
    "parent_top",
    "registry",
    "solid_def",
    "solid_mask",
    "solid_key",
    "env",
];

enum Payload {
    Bytes(Vec<u8>),
    /// The length and the digest of each page.
    Pages(usize, Vec<u64>),
}

struct Object {
    kind: String,
    position: u32,
    at: u32,
    payload: Payload,
}

impl Object {
    fn bytes(&self) -> &[u8] {
        match &self.payload {
            Payload::Bytes(b) => b,
            Payload::Pages(..) => panic!("{} is digested", self.kind),
        }
    }
}

fn objects() -> Vec<Object> {
    let text = std::env::var_os("RBR_STAGE_REFS").map(|p| std::fs::read_to_string(p).unwrap());
    text.as_deref()
        .unwrap_or(VECTORS)
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let f: Vec<_> = l.split(' ').collect();
            let payload = match f[3].strip_prefix("fnv:") {
                Some(pages) => {
                    let (len, digests) = pages.split_once(':').unwrap();
                    Payload::Pages(
                        len.parse().unwrap(),
                        digests.split(',').map(|d| u64::from_str_radix(d, 16).unwrap()).collect(),
                    )
                }
                None => Payload::Bytes(hex(f[3])),
            };
            Object {
                kind: f[0].to_string(),
                position: f[1].parse().unwrap(),
                at: u32::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap(),
                payload,
            }
        })
        .collect()
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// The game's blocks of the file being loaded, by size, in its order, each filled with `fill` when handed out (or
/// with the game's bytes, for the blocks the original reads unset); the calls to code not built here are counted.
struct Host {
    blocks: BTreeMap<u32, VecDeque<u32>>,
    fill: u8,
    game_bytes: HashMap<u32, Vec<u8>>,
    mem_fills: Vec<(u32, u32)>,
    mesh_list: u32,
    meshes: u32,
    names: usize,
    handles: usize,
    stuffs: usize,
    /// The next scratch block's offset.
    scratch: u32,
    /// The game's sort key of each placement's object (the handles the interactive objects give), by placement.
    keys: HashMap<usize, u32>,
}

impl Host {
    fn take(&mut self, size: u32) -> u32 {
        let at = self
            .blocks
            .get_mut(&size)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| panic!("a block of {size:#x} bytes the game allocated"));
        self.mem_fills.push((at, size));
        at
    }

    /// Maps the blocks handed out since the last call, filled.
    fn fill(&mut self, mem: &mut Mem) {
        for (at, size) in self.mem_fills.drain(..) {
            match self.game_bytes.get(&at) {
                Some(bytes) => mem.map(at, bytes),
                None => mem.map(at, &vec![self.fill; size as usize]),
            }
        }
    }

    /// The next file's blocks: the objects of `kinds`, in the vectors' order.
    fn load(&mut self, objects: &[Object], kinds: &[&str]) {
        assert!(self.blocks.values().all(VecDeque::is_empty), "blocks left over");
        self.blocks.clear();
        for o in objects.iter().filter(|o| kinds.contains(&o.kind.as_str())) {
            let size = match o.kind.as_str() {
                "cell_grid" => 0x14,
                "brake_wall" => 0x21c,
                "water" => 0xc0,
                // the table: its count word, then a word per cell
                "cell_table" => 4 + 4 * word(o.bytes(), 0),
                "solid" => SOLID_SIZE,
                _ => o.bytes().len() as u32,
            };
            self.blocks.entry(size).or_default().push_back(o.at);
        }
    }
}

impl Heap for Host {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        let at = self.take(size);
        self.fill(mem);
        at
    }
}

/// The grid's table and link blocks are fully written before anything reads them; their fill is not needed.
impl AllocHost for Host {
    fn allocate(&mut self, bytes: u32) -> u32 {
        let at = self.take(bytes);
        self.mem_fills.clear();
        at
    }
}

impl StageHost for Host {
    fn wet_meshes(&mut self) -> bool {
        false
    }
    fn delete(&mut self, _: &mut Mem, object: u32) {
        panic!("nothing to delete on the first stage load ({object:#x})")
    }
    fn send_shape_collision_meshes(&mut self) -> bool {
        true
    }
    fn keep_mesh_name(&mut self, _: &[u8]) {
        self.names += 1;
    }
    /// The handle is the sort key the game's object of this placement has.
    fn interactive_handle(&mut self, _: u32) -> Option<u32> {
        self.handles += 1;
        Some(self.keys.get(&self.stuffs).copied().unwrap_or(0))
    }
    fn stuff(&mut self, mem: &mut Mem, stuff: &Stuff) {
        stage::place(mem, self, stuff).expect("the placement's calls are ported");
        self.stuffs += 1;
    }
    fn scratch(&mut self, mem: &mut Mem, size: u32) -> u32 {
        if self.scratch + size > SCRATCH_SIZE {
            self.scratch = 0;
        }
        let at = SCRATCH + self.scratch;
        self.scratch = (self.scratch + size + 0xf) & !0xf;
        mem.map(at, &vec![self.fill; size as usize]);
        at
    }
    /// The vector's last buffer is the game's (its growth is the STL's).
    fn push_mesh(&mut self, mem: &mut Mem, mesh: u32) {
        mem.set_u32(self.mesh_list + 4 * self.meshes, mesh);
        self.meshes += 1;
    }
}

fn first<'a>(objects: &'a [Object], kind: &str) -> &'a Object {
    objects.iter().find(|o| o.kind == kind).unwrap()
}

/// The stage world built from the files on the retail exe's sections, every block filled with `fill` first.
fn build(data: &Data, objects: &[Object], fill: u8) -> (Mem, Host) {
    let exe = data.resolve("RichardBurnsRally_SSE.exe").expect("exe");
    let image = pe::parse(&std::fs::read(exe).unwrap()).unwrap();
    let mut mem = Mem::new();
    for name in [".rdata", ".data"] {
        let section = image.section(name).expect(name);
        mem.map(section.address, &section.bytes);
    }
    // The state before the stage load: the static objects (in `.bss`) empty.
    let system = mem.u32(stage::COLLISION_SYSTEM);
    let manager = mem.u32(physics::WALL_MANAGER_GLOBAL);
    mem.map(system, &[0; SYSTEM_SIZE as usize]);
    mem.map(manager, &[0; MANAGER_SIZE as usize]);
    mem.map(physics::CELL_GRID_GLOBAL, &[0; 4]);
    mem.map(stage::MESH_ARRAY, &[0; 0x804]);
    mem.map(stage::MATERIAL_TABLE, &[0; 0x808]);
    mem.map(GRID_BANK, &vec![0; 32 * GRID_SIZE as usize]);
    // The physics and its state: the other lists as they were, the waters' empty, the wall's holding the ground.
    let physics_at = first(objects, "physics_state").at;
    let state = word(first(objects, "physics_state").bytes(), 0);
    mem.set_u32(physics::PHYSICS_GLOBAL, physics_at - 0x20);
    // The physics object's words, its definition's debug words, its parent stack's top, the definition registry and
    // the solids' definitions' vtables and names, and the body event signal, as snap-0 has them.
    for o in objects.iter().filter(|o| {
        matches!(o.kind.as_str(), "physics" | "physics_flags" | "parent_top" | "registry" | "solid_def" | "env")
    }) {
        mem.map(o.at, o.bytes());
    }
    mem.map(state + ENTITY_LISTS, &[fill; 0x40]);
    mem.map(state + ENTITY_LISTS + 0x10, &[0; 0xc]);
    let ground = first(objects, "ground_link").at - 0x10;
    let wall_list = state + WALL_LIST;
    for (at, value) in [(wall_list, ground), (wall_list + 4, ground), (wall_list + 8, 1)] {
        mem.set_u32(at, value);
    }
    mem.map(ground + 0x10, &[0; 8]);
    mem.set_u32(ground + 0x18, wall_list);
    let drive_line = first(objects, "drive_count").at - DRIVE_COUNT;
    mem.set_u32(stage::DRIVE_LINE, drive_line);
    mem.map(drive_line, &vec![fill; DRIVE_LINE_SIZE as usize]);
    let mut host = Host {
        blocks: BTreeMap::new(),
        fill,
        game_bytes: objects.iter().filter(|o| o.kind == "mesh_faces").map(|o| (o.at, o.bytes().to_vec())).collect(),
        mem_fills: Vec::new(),
        mesh_list: first(objects, "mesh_list").at,
        meshes: 0,
        names: 0,
        handles: 0,
        stuffs: 0,
        scratch: 0,
        keys: objects
            .iter()
            .filter(|o| o.kind == "solid_key")
            .map(|o| (o.position as usize, word(o.bytes(), 0)))
            .collect(),
    };
    let read = |ext: &str| std::fs::read(data.resolve(&format!("{STAGE}.{ext}")).unwrap()).unwrap();

    host.load(objects, &["cell_grid", "cell_table", "link_block", "brake_wall", "water"]);
    // The grid an earlier `E45_MD60` built, with no wall.
    stage::build_cell_grid(&mut mem, &mut host);
    let col = first(objects, "col_image").at;
    stage::load_col(&mut mem, &mut host, col, &read("col")).unwrap();

    host.load(
        objects,
        &["mesh_desc", "mesh_name", "mesh_shape", "mesh_vertices", "mesh_faces", "solid", "placement_block"],
    );
    let track = trk::parse(&read("trk")).unwrap();
    stage::load_trk(&mut mem, &mut host, &track);

    let track_state = first(objects, "track_mat").at - 0x148;
    let mat = first(objects, "mat_image").at;
    stage::load_materials(&mut mem, track_state, mat, &read("mat"), WEAR, WETNESS);
    (mem, host)
}

/// How a byte counts: compared, written by nothing, unset in the original (and read), or written by code not built
/// here.
#[derive(Clone, Copy)]
enum Class {
    Compared,
    Unwritten,
    Unset,
    Later,
}

/// The class of byte `i` of `object`, given ours from the two builds (`a` on 0x00, `b` on 0xff) and the game's.
fn class(object: &Object, i: usize, a: &[u8], b: &[u8], game: &[u8]) -> Class {
    let later = match object.kind.as_str() {
        // A cell, or a link of a link block, that the Rust build leaves empty and the game's does not; the head and
        // count of a link the game has more objects in (they go in at the head).
        "cell_slot" => a.iter().all(|&x| x == 0),
        // What filing the solid made after the load wrote: the last placement's next word, the previous cell link of
        // the solid it displaced as its cell's head.
        "solid_link" | "solid_cell" => {
            let at = if object.kind == "solid_link" { 0 } else { 4 };
            (at..at + 4).contains(&i) && word(a, at) == 0 && word(game, at) != 0
        }
        "link_block" | "placement_block" if i >= 4 => {
            let link = 4 + (i - 4) / LINK * LINK;
            let (ours, theirs) = (&a[link..link + LINK], &game[link..link + LINK]);
            (ours.iter().all(|&x| x == 0) && ours != theirs)
                || (word(a, link + 8) < word(game, link + 8) && (i - link) / 4 != 1)
        }
        // The previous cell link of the water that was a head; the entity-list next word of the last water.
        "water" => {
            let filled = |at: usize| (at..at + 4).contains(&i) && word(a, at) == 0 && word(game, at) != 0;
            filled(0x94) || filled(0x10)
        }
        // The waters' list's tail and count.
        "entity_lists" => (0x14..0x1c).contains(&i),
        _ => false,
    };
    let unset = match object.kind.as_str() {
        "mesh_faces" => i % 7 >= 4,
        "drive_entry" => (0xc..0x10).contains(&i) || (0x1c..0x20).contains(&i),
        _ => false,
    };
    if later {
        Class::Later
    } else if unset {
        Class::Unset
    } else if a[i] != b[i] {
        Class::Unwritten
    } else {
        Class::Compared
    }
}

#[test]
fn stage_world_matches_the_game() {
    let Some(data) = Data::from_env().filter(|d| d.patch().is_some()) else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let objects = objects();
    let ((zeros, host), (ones, _)) = (build(&data, &objects, 0), build(&data, &objects, 0xff));
    assert_eq!((host.meshes, host.names, host.stuffs, host.handles), (21, 21, PLACEMENTS, HANDLES));

    let mut counts: BTreeMap<&str, [u32; 4]> = BTreeMap::new();
    let mut differing = Vec::new();
    let mut patched = Vec::new();
    // The solid's bytes its digest leaves out: the links (compared in their own lines) and the bytes nothing writes.
    let mask: Vec<bool> = first(&objects, "solid_mask").bytes().iter().map(|&m| m != 0).collect();
    let links = |i: usize| (0x10..0x1c).contains(&i) || (0x160..0x16c).contains(&i);
    let unset = |i: usize| (0x8c..0x90).contains(&i) || (0x9c..0xa0).contains(&i);
    for object in &objects {
        if INPUTS.contains(&object.kind.as_str()) {
            continue;
        }
        let count = counts.entry(object.kind.as_str()).or_default();
        // The solid made after the load (the list's last, not a placement).
        if object.kind.starts_with("solid") && object.position as usize >= PLACEMENTS {
            count[3] += match &object.payload {
                Payload::Pages(len, _) => (0..*len).filter(|&i| !links(i)).count(),
                Payload::Bytes(game) => game.len(),
            } as u32;
            continue;
        }
        match &object.payload {
            Payload::Pages(len, digests) if object.kind == "solid" => {
                let (a, b) = (zeros.read(object.at, *len), ones.read(object.at, *len));
                let mut ours = a.clone();
                let class = (0..*len).map(|i| match (mask[i], unset(i), links(i)) {
                    (false, ..) => b'w',
                    (true, true, _) => b'g',
                    (true, _, true) => b'x',
                    _ => b'u',
                });
                patched.push(patch::Patched {
                    kind: object.kind.clone(),
                    at: object.at,
                    ours: a.clone(),
                    class: class.collect(),
                    game: a.clone(),
                });
                for i in 0..*len {
                    if mask[i] {
                        ours[i] = 0;
                        if unset(i) {
                            count[2] += 1;
                        } else if !links(i) {
                            count[1] += 1;
                            if a[i] == b[i] {
                                differing.push(format!("solid {} +{i:#x}: masked, but written", object.position));
                            }
                        }
                    } else if a[i] != b[i] {
                        differing.push(format!("solid {} +{i:#x}: written by nothing, not masked", object.position));
                    }
                }
                if fnv(&ours) == digests[0] {
                    count[0] += mask.iter().filter(|&&m| !m).count() as u32;
                } else {
                    differing.push(format!("solid {} ({:#x}): digest", object.position, object.at));
                }
            }
            Payload::Pages(len, digests) => {
                let mut ours = zeros.read(object.at, *len);
                let mut class = vec![b'w'; *len];
                if object.kind == "drive_tail" {
                    // The tag and the spare bytes of each entry, which the cut zeroes.
                    for (entry, class) in ours.chunks_mut(DRIVE_STRIDE).zip(class.chunks_mut(DRIVE_STRIDE)) {
                        entry[0x24..].fill(0);
                        class[0x24..].fill(b'g');
                    }
                }
                patched.push(patch::Patched {
                    kind: object.kind.clone(),
                    at: object.at,
                    ours: ours.clone(),
                    class,
                    game: ours.clone(),
                });
                for (i, (page, digest)) in ours.chunks(PAGE).zip(digests).enumerate() {
                    if fnv(page) == *digest {
                        count[0] += page.len() as u32;
                    } else {
                        differing.push(format!("{} page {i} ({:#x})", object.kind, object.at + (i * PAGE) as u32));
                    }
                }
            }
            Payload::Bytes(game) => {
                let a = zeros.read(object.at, game.len());
                let b = ones.read(object.at, game.len());
                let mut classes = Vec::new();
                for (i, &g) in game.iter().enumerate() {
                    let c = class(object, i, &a, &b, game);
                    count[c as usize] += 1;
                    // The lists the stage leaves alone hold what other load code (the car's placements) files.
                    let shared = object.kind == "entity_lists" && matches!(c, Class::Unwritten);
                    classes.push(if shared { b'l' } else { b"wugl"[c as usize] });
                    if matches!(c, Class::Compared) && a[i] != g {
                        differing.push(format!(
                            "{} {} +{i:#x}: ours {:02x} the game's {g:02x}",
                            object.kind, object.position, a[i]
                        ));
                    }
                }
                patched.push(patch::Patched {
                    kind: object.kind.clone(),
                    at: object.at,
                    ours: a,
                    class: classes,
                    game: game.clone(),
                });
            }
        }
    }
    // The `.data` pointers the step reads: the cell grid the build made
    // (`E45_MD60` stores it) and the drive line block the build filled, each at the game's object.
    for (global, object) in [
        (physics::CELL_GRID_GLOBAL, first(&objects, "cell_grid").at),
        (stage::DRIVE_LINE, first(&objects, "drive_count").at - DRIVE_COUNT),
    ] {
        assert_eq!(zeros.u32(global), object, "the pointer at {global:#x}");
        assert_eq!(ones.u32(global), object, "the pointer at {global:#x}");
        let ours = zeros.read(global, 4);
        patched.push(patch::Patched {
            kind: "stage_global".into(),
            at: global,
            ours: ours.clone(),
            class: vec![b'w'; 4],
            game: ours,
        });
    }
    patch::write("stage-world", &patched);
    let mut total = [0; 4];
    for (kind, c) in &counts {
        eprintln!(
            "{kind}: {} bytes compared; excluded: {} never written, {} unset in the original, {} written by code \
             not built here",
            c[0], c[1], c[2], c[3]
        );
        for (t, n) in total.iter_mut().zip(c) {
            *t += n;
        }
    }
    eprintln!(
        "total: {} bytes compared; excluded: {} never written, {} unset in the original, {} written by code not \
         built here",
        total[0], total[1], total[2], total[3]
    );
    assert!(
        differing.is_empty(),
        "{} differ:\n{}",
        differing.len(),
        differing.iter().take(200).cloned().collect::<Vec<_>>().join("\n")
    );
}
