//! The car's collision grids built from the retail files, as the load builds them, against the game's, for every car:
//! the car loader's meshes from the collision bodies of its `.sgc` (`load::car_grids::car_meshes`), emitted with their
//! blocks to `E37SigFactory720`, which builds grids 1-8 (`load::emit_car_meshes`), and grid 9 from
//! `E02FireCollisionMesh` (`load::fire_collision_mesh`). Then `CCar::UpdateA720` moves the hinged bodies' grids (2-5)
//! by their `vecHingePoint_BF` (`Physics/<car>/common.lsp`, the hinged body of the same name as the collision body;
//! `CHingedRigidBody::SimEFE0` -> `E37GridOps::AddEdge`).
//!
//! The vectors (`vectors/load-car-grids.txt`, cut by `harness/load_car_grid_refs.py`) hold per run the grid vertices
//! at UpdateA720's entry (as emitted), and at snap-0 every byte of grids 1-9, each descriptor and the blocks it points
//! to (the maps as digests). The runs: lcb1-bodies and h1-tanner-corolla (the Corolla) and one short run per other car
//! (`snaps/lg2/lg2-car<N>`, Tanner, tanner-corolla's first controls). The heap gives the game's blocks in the loader's
//! order; HedgeHog3D's base is the run's.
//!
//! Every byte is compared. The original leaves a part's face bytes 4-6 unwritten (the heap's), and `Build9CD0` copies
//! them into its edges' bytes 4-5 and its sample-index table (+0x690). Those are read only through a sampled grid (the
//! body's alone). The test builds once on the game's bytes there, which must give the game's grids, and once on 0 (the
//! load's), which may differ from it only there. The vertex blocks' `w` lanes are unwritten and not compared.
//!
//! With `RBR_LOAD_OUT` the lcb1 build on 0 is written as the `car-grids` patch for `harness/compose_load.py`: the grid
//! bank's ids 1-9 at snap-0 (after the hinge moves), the descriptors and their blocks. The bytes built from the unwritten
//! ones, those, and the `w` lanes are `u`. Runs only when `RBR_DATA` and `RBR_PATCH` are set.

use rbr_formats::data::Data;
use rbr_formats::lsp::{self, Node};
use rbr_formats::{pe, rbz, sgc};
use rbr_physics::collision::layout::{col_grid_at, grid_add_edge};
use rbr_physics::load::{self, car_grids};
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::construct::Heap;
use rbr_physics::physics_core::layout::PHYSICS_GLOBAL;
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;

#[path = "common/patch.rs"]
mod patch;

const VECTORS: &str = include_str!("vectors/load-car-grids.txt");
const GRID_SIZE: u32 = 0x6b0;
/// The car's grids, ids 1 to 9.
const GRIDS: std::ops::RangeInclusive<u32> = 1..=9;
/// The descriptor (`CSigCollisionMesh`).
const DESC_SIZE: usize = 0x34;
/// Free memory for `E02FireCollisionMesh`'s stack descriptor and the hinge vector.
const SCRATCH: u32 = 0x7000_0000;
/// The grid's edges (6 bytes each, from +0x484) and sample-index table (+0x690, 0x20 bytes).
const EDGES: usize = 0x484;
const EDGE_COUNT: usize = 0x54;
const SAMPLE_INDEX: usize = 0x690;
/// The hinged bodies by the grid their collision body (`c_e_door_l`, ...) gives them.
const HINGED: [(u32, &str); 4] = [(2, "CAR_DOOR_L"), (3, "CAR_DOOR_R"), (4, "CAR_BONNET"), (5, "CAR_EXHAUSTPIPE")];

enum Bytes {
    Raw(Vec<u8>),
    Digest(u64),
}

struct Block {
    at: u32,
    len: u32,
    bytes: Bytes,
}

/// One run's vectors.
#[derive(Default)]
struct Run {
    label: String,
    /// `Cars/<dir>/<sgc>.sgc` and `Physics/<physics>/common.lsp`.
    dir: String,
    sgc: String,
    physics: String,
    hh: u32,
    emit: BTreeMap<u32, Vec<u32>>,
    grids: BTreeMap<u32, Vec<u8>>,
    descs: BTreeMap<u32, (u32, Vec<u8>)>,
    blocks: BTreeMap<(u32, String), Block>,
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn runs() -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for line in VECTORS.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |k: usize| f[k].parse::<u32>().unwrap();
        let addr = |k: usize| u32::from_str_radix(f[k], 16).unwrap();
        if f[0] == "car" {
            runs.push(Run {
                label: f[1].into(),
                dir: f[2].into(),
                sgc: f[3].into(),
                physics: f[4].into(),
                hh: addr(5),
                ..Run::default()
            });
            continue;
        }
        let run = runs.last_mut().unwrap();
        assert_eq!(f[1], run.label, "{line}");
        match f[0] {
            "emit" => {
                let words: Vec<u32> = f[4..].iter().map(|w| u32::from_str_radix(w, 16).unwrap()).collect();
                assert_eq!(words.len(), 3 * num(3) as usize, "{line}");
                run.emit.insert(num(2), words);
            }
            "grid" => {
                run.grids.insert(num(2), hex(f[3]));
            }
            "desc" => {
                run.descs.insert(num(2), (addr(3), hex(f[4])));
            }
            "block" => {
                let bytes = match f[6].strip_prefix("fnv:") {
                    Some(d) => Bytes::Digest(u64::from_str_radix(d, 16).unwrap()),
                    None => Bytes::Raw(hex(f[6])),
                };
                run.blocks.insert((num(2), f[3].into()), Block { at: addr(4), len: num(5), bytes });
            }
            _ => panic!("{line}"),
        }
    }
    runs
}

fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

/// `vecHingePoint_BF` of every named body (`("Kind" ("NAME" ...))`) that has one.
fn hinges(items: &[Node], out: &mut BTreeMap<String, [f32; 3]>) {
    let atom = |k: usize| match items.get(k) {
        Some(Node::Atom(a)) => Some(a.as_str()),
        _ => None,
    };
    if let (Some(Node::Quoted(name)), Some(h)) =
        (items.first(), (0..items.len()).find(|&k| atom(k) == Some("vecHingePoint_BF")))
    {
        let v = |k: usize| atom(k).unwrap().parse::<f32>().unwrap();
        out.insert(name.clone(), [v(h + 1), v(h + 2), v(h + 3)]);
    }
    for item in items {
        if let Node::List(inner) = item {
            hinges(inner, out);
        }
    }
}

/// The game's blocks in the loader's order, each filled with `fill`.
struct GameHeap {
    blocks: VecDeque<(u32, u32)>,
    fill: u8,
}

impl Heap for GameHeap {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32 {
        let (at, len) = self.blocks.pop_front().expect("more blocks than the game's");
        assert_eq!(size, len, "block {at:#x}");
        mem.map(at, &vec![self.fill; size as usize]);
        at
    }
}

/// The car's collision bodies and the meshes the loader emits from them.
fn meshes(data: &Data, run: &Run) -> Vec<car_grids::CarMesh> {
    let path = format!("Cars/{}/{}.sgc", run.dir, run.sgc);
    let bytes = std::fs::read(data.resolve(&path).expect(&path)).unwrap();
    let bodies: Vec<sgc::Collision> = sgc::parse(&bytes)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e {
            sgc::Element::Collision(c) => Some(c),
            _ => None,
        })
        .collect();
    car_grids::car_meshes(&bodies).expect("a body and transforms with inverses")
}

/// The car's grids built on the retail exe's sections, the heap's blocks filled with `fill`; with `game_garbage` a
/// part's face bytes 4-6 are the game's. Then the hinge moves.
fn build(data: &Data, run: &Run, fill: u8, game_garbage: bool) -> Mem {
    let exe = data.resolve("RichardBurnsRally_SSE.exe").expect("exe");
    let image = pe::parse(&std::fs::read(exe).unwrap()).unwrap();
    let mut mem = Mem::new();
    for name in [".rdata", ".data"] {
        let section = image.section(name).expect(name);
        mem.map(section.address, &section.bytes);
    }
    // The state before the car load: the grid bank (in `.bss`) empty, the physics made (only tested for nonzero).
    mem.map(col_grid_at(0), &vec![0; 10 * GRID_SIZE as usize]);
    mem.map(PHYSICS_GLOBAL, &1u32.to_le_bytes());
    mem.map(SCRATCH, &[0; 0x100]);
    // HedgeHog3D's names.
    for &(name, id, at) in &car_grids::PARTS {
        let name = if id == 1 { "MrCarCollision" } else { name };
        mem.map(run.hh + at, &[name.as_bytes(), &[0]].concat());
    }

    let mut meshes = meshes(data, run);
    assert_eq!(meshes.len(), run.descs.len(), "{}: meshes", run.label);
    if game_garbage {
        for m in meshes.iter_mut().filter(|m| m.id != 1) {
            let Bytes::Raw(game) = &run.blocks[&(m.id, "faces".to_string())].bytes else { unreachable!() };
            for (face, game) in m.faces.iter_mut().zip(game.chunks(7)) {
                face[4..].copy_from_slice(&game[4..]);
            }
        }
    }
    let block = |id: u32, kind: &str| run.blocks.get(&(id, kind.to_string())).map(|b| (b.at, b.len));
    let mut blocks: VecDeque<(u32, u32)> =
        meshes.iter().flat_map(|m| [block(m.id, "map_b"), block(m.id, "map_a")]).flatten().collect();
    for m in &meshes {
        blocks.push_back((run.descs[&m.id].0, DESC_SIZE as u32));
        blocks.extend(block(m.id, "vertices"));
        blocks.extend(block(m.id, "faces"));
        if m.samples.is_some() {
            blocks.push_back(block(m.id, "samples").expect("the body's samples"));
        }
    }
    let mut heap = GameHeap { blocks, fill };
    let descs = load::emit_car_meshes(&mut mem, &mut heap, run.hh, &meshes);
    assert!(heap.blocks.is_empty(), "{}: blocks left", run.label);
    assert_eq!(descs, run.descs.values().map(|d| d.0).collect::<Vec<_>>());
    load::fire_collision_mesh(&mut mem, SCRATCH);

    // As emitted: the vertices at UpdateA720's entry.
    for id in GRIDS {
        let grid = col_grid_at(id);
        let n = u32::from(mem.u8(grid + 0x68d));
        let words: Vec<u32> = (0..n)
            .flat_map(|k| (0..3).map(move |lane| grid + 0x40 + 16 * k + 4 * lane))
            .map(|at| mem.u32(at))
            .collect();
        assert_eq!(words, run.emit[&id], "{}: grid {id} as emitted", run.label);
    }

    let lsp_path = format!("Physics/{}/common.lsp", run.physics);
    let mut archive = rbz::open(&data.resolve("Root/physics.rbz").unwrap()).unwrap();
    let mut text = String::new();
    archive.by_name(&lsp_path).expect(&lsp_path).read_to_string(&mut text).unwrap();
    let mut by_name = BTreeMap::new();
    hinges(&lsp::parse(&text).unwrap(), &mut by_name);
    for (id, name) in HINGED {
        let v = by_name[name];
        for (k, x) in v.into_iter().enumerate() {
            mem.set_f32(SCRATCH + 0x40 + 4 * k as u32, x);
        }
        grid_add_edge(&mut mem, col_grid_at(id), SCRATCH + 0x40);
    }
    mem
}

/// Grid byte `i` (from grid 1) is one `Build9CD0` takes from a part's face bytes 4-6.
fn from_face_garbage(i: usize) -> bool {
    let (id, at) = (1 + i / GRID_SIZE as usize, i % GRID_SIZE as usize);
    let edge_byte = (EDGES..EDGES + 6 * EDGE_COUNT).contains(&at) && (at - EDGES) % 6 >= 4;
    (2..=8).contains(&id) && (edge_byte || (SAMPLE_INDEX..SAMPLE_INDEX + 0x20).contains(&at))
}

#[test]
fn car_grids_match_the_game() {
    let Some(data) = Data::from_env().filter(|d| d.patch().is_some()) else {
        eprintln!("skipping: RBR_DATA or RBR_PATCH not set");
        return;
    };
    let bank = col_grid_at(1);
    let bank_len = GRIDS.count() * GRID_SIZE as usize;
    for run in runs() {
        let game = build(&data, &run, 0, true);
        let ones = build(&data, &run, 0xff, true);
        let ours = build(&data, &run, 0, false);

        // Every grid byte, on the game's face bytes; on 0 only the bytes built from them differ.
        for id in GRIDS {
            let got = game.read(col_grid_at(id), GRID_SIZE as usize);
            let want = &run.grids[&id];
            let differ: Vec<usize> = (0..got.len()).filter(|&i| got[i] != want[i]).collect();
            assert!(differ.is_empty(), "{}: grid {id} differs at {:x?}", run.label, differ);
            assert_eq!(got, ones.read(col_grid_at(id), GRID_SIZE as usize));
        }
        let (a, b) = (game.read(bank, bank_len), ours.read(bank, bank_len));
        let zeroed: Vec<usize> = (0..bank_len).filter(|&i| a[i] != b[i]).collect();
        assert!(
            zeroed.iter().all(|&i| from_face_garbage(i)),
            "{}: grid bytes built from 0 differ elsewhere: {:x?}",
            run.label,
            zeroed
        );

        let mut patched = vec![patch::Patched {
            kind: "car-grids".into(),
            at: bank,
            ours: b.clone(),
            class: (0..bank_len).map(|i| if from_face_garbage(i) { b'u' } else { b'w' }).collect(),
            game: run.grids.values().flatten().copied().collect(),
        }];

        // The descriptors and their blocks.
        for (&id, (at, want)) in &run.descs {
            assert_eq!(&game.read(*at, DESC_SIZE), want, "{}: descriptor {id}", run.label);
            patched.push(patch::Patched {
                kind: "car-mesh-desc".into(),
                at: *at,
                ours: ours.read(*at, DESC_SIZE),
                class: vec![b'w'; DESC_SIZE],
                game: want.clone(),
            });
        }
        for ((id, kind), block) in &run.blocks {
            let len = block.len as usize;
            let (got, got_ones, zero) = (game.read(block.at, len), ones.read(block.at, len), ours.read(block.at, len));
            let unwritten = |i: usize| got[i] != got_ones[i];
            let (game_bytes, class) = match &block.bytes {
                Bytes::Digest(d) => {
                    assert_eq!(fnv64(&got), *d, "{}: {kind} of grid {id}", run.label);
                    (got.clone(), vec![b'w'; len])
                }
                Bytes::Raw(want) => {
                    let differ: Vec<usize> = (0..len).filter(|&i| !unwritten(i) && got[i] != want[i]).collect();
                    assert!(differ.is_empty(), "{}: {kind} of grid {id} differs at {:x?}", run.label, differ);
                    let garbage = |i: usize| kind == "faces" && *id != 1 && i % 7 >= 4;
                    let differ: Vec<usize> =
                        (0..len).filter(|&i| zero[i] != got[i] && !unwritten(i) && !garbage(i)).collect();
                    assert!(differ.is_empty(), "{}: {kind} of grid {id} on 0", run.label);
                    let class = (0..len).map(|i| if unwritten(i) || garbage(i) { b'u' } else { b'w' }).collect();
                    (want.clone(), class)
                }
            };
            // Unwritten: the vertices' `w` lanes alone.
            for i in (0..len).filter(|&i| unwritten(i)) {
                assert!(kind == "vertices" && i % 16 >= 12, "{}: {kind} {i:#x}", run.label);
            }
            patched.push(patch::Patched {
                kind: format!("car-mesh-{kind}"),
                at: block.at,
                ours: zero,
                class,
                game: game_bytes,
            });
        }
        eprintln!(
            "{}: grids 1-9 equal ({} bytes from the unwritten face bytes), {} descriptors, {} blocks",
            run.label,
            zeroed.len(),
            run.descs.len(),
            run.blocks.len()
        );
        if run.label == "lcb1" {
            patch::write("car-grids", &patched);
        }
    }
}
