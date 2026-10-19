# Rust plan

Goal: RBR with NGP 7 physics and its NGP cars, on the original and the modded stages (user, 2026-10-17). In parts:
- **1.02 base:** a standalone Rust program that plays every Richard Burns Rally 1.02 stage and car, with the physics
  and the gameplay loop behaving exactly like 1.02 ("Done means" below).
- **NGP 7.4.778:** NGP's physics on top (`notes/ngp.md`; `rbr-standalone --ngp-physics`), compared with the original
  running NGP (`matrix.py --ngp`).
- **NGP cars:** the community NGP car packs, loaded as RallySimFans installs them (`Cars/<name>/`,
  `rsfdata/cars/*_ngp6`), read in place (`notes/formats/rsf-cars.md`; `--rsf-car`).
- **Modded stages:** community stages in the original stage format, one folder per stage under `Maps/`, with
  RallySimFans's `Tracks<id>.ini`, `TrackSettings<id>.ini` and `TextureFilenameMap<id>.ini`.
- **Live play:** all of it working together in live play: pacing, HUD, sound, force feedback.
Neither the modded content nor NGP is committed; bring your own copy, as with the game.

No original code runs (no emulator, no exe, no `NGP.dll`); it reads the game data and the retail exe's data sections (`.data`/`.rdata`, read at run time, never transcribed). Not a port of the
game: no 1.02 UI, menus or frontend; a new renderer.

## Done means (behaviour 100% matched)

For every drive in the gate matrix (`notes/progress/drives.csv`), the Rust program fed the drive's recorded inputs
produces the original's car and wheel state bit for bit after every physics step, over the whole drive, and the same
stage events (splits, finish, timing). The only differences allowed are the documented classes, each listed with its
evidence:
- the sign of a default NaN (x86 negative, arm64 positive), which no comparison or arithmetic can see
  (`notes/rust-physics.md` section 8);
- words the original fills from uninitialised stack that nothing later reads (`harness/rust_step.py --excluded`);
- the lane-3 stack residue words the C++ gate itself masks (`harness/diverge.py` static_mask, the user's decision for
  the C++): read only lane-wise into other masked words, never into the car's motion. Rust is held to the C++ gate's
  standard there, not a stricter one; every word the gate compares must match;
- heap addresses (compared as placement in an allocation log, not as values). The standalone uses its own allocator
  (`rbr-hybrid/src/pure.rs`: one dedicated region, size free lists, zero-filled), so its pointer words are counted,
  not compared;
- frame-loop words: values the game writes once per rendered frame outside the physics step, which the recordings
  cannot place (FRM1 has no frame boundaries; frame pacing is wall-clock). Today only car+0x670 (`CCar::Vf28x`,
  `lead/reports/o670.md`). Not compared with the recording; the hybrid still compares them with the original.
The proven stale-stack words are one list in two places, `harness/rust_step.py` `CAR_STATIC`/`STACK_GARBAGE` and
`rbr-hybrid/src/pure.rs` `STALE_CAR`/`STALE_WHEEL`; a word joins only with run-A evidence (the original differs from
its own recording, or its stale-stack runs differ), and differing ones are printed apart, never dropped silently.
The gate matrix must cover every car and every stage surface class (gravel, tarmac, snow), plus damage and collisions,
before the goal counts as reached. Today it holds one drive (tanner-corolla).

## Milestones

| | Milestone | Proof | State (2026-10-16, afternoon) |
|---|---|---|---|
| R | C++ reference exact on tanner-corolla, then frozen | harness gate runs, `drives.csv` | done; frozen 2026-10-15 (new drives are checked in Rust only) |
| H | Hybrid step: every Rust port bound into the emulated step | `rust_step.py --steps` with all bindings | 604 bindings (damage path, functors, mounted bodies and the car recovery path added 2026-10-16); the merge gate: 10 tanner-corolla snapshots + 2 impact snapshots, 30 steps |
| B | Pure Rust physics step (`rbr-hybrid/src/pure.rs`: the bindings on a pure `Mem`, no emulator) | `rbr-standalone` compared with the recording each step | tanner-corolla runs the whole stage pure; on the 31 drives' snapshots most runs are 300/300; open: HedgeHog3D CHH_Car::OnDetachFromParent on damage 11827 (`g-cdb0`) |
| L | Load path: Rust builds the physics state from the retail files | byte compare with the original's snap-0 | definitions, car, placements and UpdateA720, stage world, car grids (all 8 cars), physics/damage/ground objects, event pool, replay stand-in; over 300 frames the boot reads 29,981 bytes from the Rust load and 2,110 still from the snapshot (`standalone-s11.md`); the boot still starts from a composed snap-0; a snapshot-free `--boot` is in progress (`standalone-s13`) |
| G | Gameplay loop in Rust (`rust/rbr-gameplay`) | control payload, stage, time card bit-exact per step against recordings | done on the 4 recorded drives except pace-note marker inputs (`lead/reports/gameplay-g10.md`) |
| S | Standalone program (`rust/rbr-standalone`): loop + pure physics, headless or live in the viewer | the gate matrix, in Rust only | from the stage start, tanner-corolla: every frame of the recording pure and equal, time card equal every step, no stop to the end (`g-hhlisten`); only Corolla on one stage so far |
| V | Gate matrix covers every car and surface class | recordings + snapshots | 4 recorded drives + 35 playback-made drives (`v1-drives.md` to `v4-drives.md`): every car at least twice, all 36 main-rally map IDs, damage 1 and 3, weather 0-2; most are 3,000 steps (snapshots, not whole stages) |

## Decisions in force (details in `lead/LEAD-NOTES.md` and the reports named)

- The standalone builds only the physics' and the gameplay loop's signal connections; sound, UI, replay and DLL
  listeners do not exist in it (`standalone-s4.md` O1).
- Memory the original reads but never writes (uninitialised heap) is zero in the Rust load: filling it with 00, ff or
  7f changes nothing the original computes (`standalone-s7.md` O4); the same for the car grids' face bytes 4-6
  (`load-grids2.md`).
- The pure step's `operator new`/`delete` (0x70bec2, 0x70c260) are host stand-ins, not ports; the hybrid keeps the
  original allocator (`standalone-s6.md`).
- `rbr-hybrid` may depend on `rbr-gameplay` (one time keeper: `rbr_gameplay::timing`).
- The game's app state (CApplication+0x728, the car's control gate) is driven by `Game` through
  `PhysicsStep::app_state` (`standalone-s2.md`).

## Scope

| Area | Approach |
|---|---|
| Physics (`NPhysics::`, `NRepair::`, `NRegulation::`, exe E33-E51) and the gameplay loop (tick, input to car control, stage load, timing, pace notes) | Behaviour-exact. The original is the oracle: harness recordings and snapshots of 1.02 (`harness/README.md`). The C++ reconstruction is frozen as a readable reference (exact on tanner-corolla); Rust is translated from it and checked against the original directly. |
| Data formats: stages (geometry, collision, `.mat` surfaces, `.fnc` fences, `.trk`, splits/checkpoints, pace notes), cars (physics `.lsp`, tyres, setups, damage), packed archives (`.rbz`) | Written specs in `notes/formats/` plus parsers in `rust/rbr-formats`, each tested against every retail file (`RBR_DATA`). The exe/HedgeHog3D code is read to resolve unknown fields, not matched. |
| Rendering data (meshes, materials, textures for stages and cars) | Only what a modern renderer needs to draw them; HedgeHog3D is read, not matched. The original shaders are read for what their inputs mean (texture roles, vertex attributes, render states), not ported. See Rendering below. |
| Sound | Later, behaviour only (engine sound from the drivetrain state). |
| UI, menus, frontend, LocoMotion (driver animation), rendering code | Out of scope. No matching or coverage work. One exception (user, 2026-10-16): a new basic launcher (car, map, original good/bad weather, tyre wear, optional tyre, resolution) that starts the Rust game; it is not the 1.02 frontend. |

Byte-matching stays useful only where it helps verify behaviour; the score is not a goal.

## Order

Headline metric (since 2026-10-16): how far `rbr-standalone` runs pure and equal to the recording on every drive and
snapshot, and its stop list (the first unbound or declining function per run). Empty everywhere, booted from the
retail files, means done. Ported and bound counts are not goals.

Coverage: drives beyond the recorded ones are made without a driver by playing a recorded drive's controls back with
other StartGame numbers (`harness/v1-drive.sh`); the original's recording of that run is the reference, whatever the
car does. Where they matter (splits, finish), drives are recorded by hand (`harness/record.sh`).

1. The C++ physics is frozen (2026-10-15): no culprit fixes on new drives, no backports from Rust, no reccmp,
   check-batch or rename batches for physics. Rust is translated from it a group at a time (a file or a call cluster
   from the worklist); the asm decides only where a check fails.
2. Pure Rust (B), the main track: `rbr-standalone` runs the bindings without the emulator from a snapshot or the
   stage start; each stop is ported a group at a time (a caller and its unported callees, 4-8 functions) and bound
   whole. No subtree tapes (B0 stays as a test).
3. Checking a group: bind it whole, one long run (until that exists: `rust_step.py --steps` on the 9 tanner-corolla
   snapshots). Identical and called marks the group checked; a divergence is located with `--only` and bisect and
   fixed in Rust. Existing record tests stay; fuzzed direct-call records only for branches no drive reaches. When a
   port only differs by stack placement, write down which residue words the original reads instead of tuning call
   depths.
4. New drives (V): recorded and snapshotted from the original, then checked in Rust with the same long runs; their
   divergences are fixed in Rust only.
5. Merge gate for Rust-only branches (nothing under `src/`): `scripts/rust-check.sh` (fmt, clippy, the workspace tests
   and every vector target) plus the group check. Branches that touch `src/` keep the C++ build, reccmp and
   unresolved checks.
6. Gameplay loop (G): written directly in Rust from the decompilation and `notes/gameplay-loop.md`, no C++ fixing
   first; checked against recordings (control payload bit-exact every step; start, splits, finish and timing exact).
   1.02 replay (`.rpl`) is out of scope for now. Load path (L): checked against snap-0 of the original.
7. Standalone (S), with the renderer (below) alongside.

## Rendering

Goal: every stage and car drawn with all its shapes and textures in the right place, through the renderer's own
material description, so modern shaders can replace the simple ones later without touching the loaders. RBR's
original look (lighting, fog, car paint, water) is not a goal.

- `rust/rbr-render`: wgpu + winit, library plus a `viewer` binary; reads only `rbr-formats` until milestone 4.
  DDS textures upload as BC1/BC2 (supported on Apple Silicon).
- Coordinate conventions (D3D9 is left-handed; up axis to check) are written down once and converted in one place.

Milestones:

1. One stage (`track-10_O`) in a free-fly viewer: `.lbs` block buffers, render chunks drawn with texture 0 times
   vertex colour, unlit; culling by block bounds; missing textures reported.
2. Every stage: object lanes, sky and clouds, weather and wear selection; an `RBR_DATA` test loads all 86 stages
   and reports missing textures and bad indices.
3. The car: `.sgc` meshes, face shader and material from `<car>.ini`, textures from `<car>_shaders.ini`, placed at
   the stage start location (lbs 2,11).
4. A reference drive played back from a harness recording (body and wheel transforms, chase camera); later fed by
   `rbr-physics` instead.
5. Material roles, in `notes/formats/shaders.md`: from the `.vso`/`.pso` disassembly and HedgeHog3D, what each
   texture slot (diffuse, second texture, shadow, specular; car paint, glass, dirt, scrapes) and vertex attribute
   (TEXCOORD1-5, float1 channels) means, mapped onto the renderer's material description.

Needed for correct shapes, so not deferred: render states (alpha test, blending, double-sided faces, decals; the
object-lane `KindState` values) and a check that no vertex shader moves texture coordinates.

Stage texture roles, vertex attributes, render states and the camera conventions are in
`notes/formats/shaders.md` (from the shader disassembly and an apitrace capture), as are the car texture stages.

## Status

- Progress scorecard: `python3 -I scripts/progress.py report` (notes/progress/).
- `rust/rbr-formats`: every stage and car file type parses on all retail files (.mat, .fnc, .trk, .col, .dls, .lbs, .lsp, car .ini, .sgc, repair.ini, .dat, .rbz, stage textures); field meanings partly open (notes/formats/).
