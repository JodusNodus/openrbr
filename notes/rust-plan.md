# Rust plan

Goal (user, 2026-10-17): RBR with NGP 7 physics and its NGP cars, on the original and the modded stages.

- **Stage and car base:** a standalone Rust program that plays every 1.02 stage and car, NGP physics with the
  gameplay loop behaving exactly like the original (standalone steps NGP only, 2026-10-21).
- **NGP 7.4.778:** NGP's physics on top ([ngp.md](ngp.md); `rbr-standalone --ngp-physics`), compared with the
  original running NGP (`harness/matrix.py`).
- **NGP cars:** the community car packs as RallySimFans installs them (`Cars/<name>/`, `rsfdata/cars/*_ngp6`), read
  in place ([formats.md](formats.md#rsf-cars); `--rsf-car`).
- **Modded stages:** community stages in the original format, one folder per stage under `Maps/`, with RSF's
  `Tracks<id>.ini`, `TrackSettings<id>.ini` and `TextureFilenameMap<id>.ini` ([formats.md](formats.md#rsf-stages)).
- **Live play:** all of it together: pacing, HUD, sound, force feedback.

No original code runs. It reads the game data and the retail exe's data sections at run time, never transcribed. It is
not a port of the game: no 1.02 UI, menus or frontend, and a new renderer. NGP and force feedback are the user's code.

## Done means

For every drive in the gate matrix, the Rust program fed the drive's recorded inputs produces the original's car and
wheel state bit for bit after every physics step, over the whole drive, and the same stage events (splits, finish,
timing). The only allowed differences are the documented classes, each with its evidence
([verification.md](verification.md#masks)):

- the sign of a default NaN (x86 negative, arm64 positive), which no comparison or arithmetic can see;
- words the original fills from uninitialised stack that nothing later reads (`rust_step.py --excluded`);
- the lane-3 stack residue the gate itself masks (`harness/diverge.py` static mask), read only into other masked
  words;
- heap addresses, compared as placement in an allocation log; the standalone's own allocator only counts them;
- frame-loop words the game writes once per rendered frame outside the step (today car+0x670, `CCar::Vf28x`).

The stale-stack words are one list in two places, `harness/rust_step.py` (`CAR_STATIC`/`STACK_GARBAGE`) and
`crates/rbr-hybrid/src/pure/compare.rs` (`STALE_CAR`/`STALE_WHEEL`). A word joins only with run-A evidence, and
differing ones are printed apart, never dropped. The gate matrix must cover every car and every surface class
(gravel, tarmac, snow), plus damage and collisions, before the goal counts as reached.

## Milestones

| | Milestone | Proof |
| --- | --- | --- |
| R | C++ reference exact on tanner-corolla, then frozen (2026-10-15) | rbr-recomp |
| H | Hybrid step: every Rust port bound into the emulated step | `rust_step.py` with all bindings |
| B | Pure Rust step (`rbr-hybrid/src/pure/`), no emulator | `rbr-standalone` against the recording each step |
| L | Load path: Rust builds the physics state from the retail files | byte compare with the original's snap-0; `--boot` |
| G | Gameplay loop in Rust (`crates/rbr-gameplay`) | control payload, stage and time card per step against recordings |
| S | Standalone program, headless or live in the viewer | the gate matrix, in Rust only |
| V | Gate matrix covers every car and surface class | recordings and snapshots |

Current state: [status.md](status.md).

## Decisions in force

- The standalone builds the physics' and the gameplay loop's signal connections; the original sound, UI,
  replay-manager and DLL listeners do not exist in it. ReRBR replay playback feeds recorded frames to the live loop.
- Memory the original reads but never writes (uninitialised heap) is zero in the Rust load: filling it with 00, ff or
  7f changes nothing the original computes. The same holds for the car grids' face bytes 4-6.
- The pure step's `operator new`/`delete` (0x70bec2, 0x70c260) are host stand-ins, not ports; the hybrid keeps the
  original allocator.
- `rbr-hybrid` may depend on `rbr-gameplay` (one time keeper: `rbr_gameplay::timing`).
- The game's app state (CApplication+0x728, the car's control gate) is driven by `Game` through
  `PhysicsStep::app_state`.
- The C++ physics is frozen: no culprit fixes on new drives, no backports from Rust. Rust is checked against the
  original directly; the asm decides where a check fails.
- New drives are made by playing a recorded drive's controls back with other StartGame numbers
  (`harness/v1-drive.sh`); the original's recording of that run is the reference. Drives where splits and the finish
  matter are recorded by hand (`harness/record.sh`).
- Original 1.02 replays (`.rpl`), replay cameras and filters are out of scope. ReRBR records its own `.rrp` replays
  for playback and video export from the launcher.

## Scope

| Area | Approach |
| --- | --- |
| Physics and the gameplay loop | Behaviour-exact against the original's recordings and snapshots. |
| Data formats | Specs in [formats.md](formats.md), parsers in `rbr-formats`, each tested on every retail file. Exe and HedgeHog3D code is read to resolve fields, not matched. |
| Rendering | Every stage and car drawn with all its shapes and textures in the right place, through the renderer's own material description. The original shaders are read for what their inputs mean, not ported. |
| Sound | Behaviour from the game's data and signals; may sound better than the original. |
| UI, menus, LocoMotion | Out of scope, except a new launcher (car, stage, weather, tyres, settings) that starts the Rust game. |

## Order

The headline metric: how far `rbr-standalone` runs pure and equal to the recording on every drive, and its stop list
(the first unbound or declining function per run). Empty everywhere, booted from the retail files, means done.

1. Each stop is ported a group at a time (a caller and its unported callees, 4-8 functions) and bound whole.
2. A group is checked by one long run. A divergence is located with `--only` and bisect and fixed in Rust. Fuzzed
   records only for branches no drive reaches.
3. New drives are checked in Rust with the same runs; their divergences are fixed in Rust only.
4. Rust-only branches merge on `scripts/rust-check.sh` plus the group check.
