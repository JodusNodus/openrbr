# Agent guide

ReRBR is a Rust reimplementation of Richard Burns Rally 1.02 with NGP 7 physics: a standalone program, no original code running, that plays the 1.02 stages, the NGP cars and the modded stages with physics and gameplay bit-exact against recordings of the original (`notes/rust-plan.md`). The original game is the oracle; the C++98 reconstruction of the 1.02 binaries, the behaviour reference, lives in a separate repository, rbr-recomp (frozen). UI, menus and the 1.02 renderer are not carried over.

## Start here

Before changing anything:

1. Run `git status --short` and preserve existing changes.
2. Read `LOG.md` for current progress and open questions, and `notes/rust-plan.md` for the scope.
3. Read `docs/development.md` for how the parts fit together and the checks.
4. Read `harness/README.md` before recording drives or running the emulator checks.
5. Open the task-specific notes listed below.

`README.md` is the user-facing overview. Keep this file as an index; put detailed findings in `notes/`. Paths under `src/` in comments and notes refer to the rbr-recomp repository.

## Important paths

| Path | Purpose |
| --- | --- |
| `rust/rbr-formats` | Game file parsers, one module per format |
| `rust/rbr-physics` | Physics ports (1.02 and NGP); `ports.csv` lists them |
| `rust/rbr-hybrid` | Ports bound into the original's step in the emulator; `bound.csv` lists the bindings |
| `rust/rbr-gameplay` | Gameplay loop: input to car control, tick, stage events, timing, championship |
| `rust/rbr-standalone` | The physics program (`--boot`, `--live`) |
| `rust/rbr-render` | wgpu renderer and the driving window (`viewer`) |
| `rust/rbr-sound`, `rust/rbr-launcher`, `rust/rbr-recording` | Sound, launcher, harness recording readers |
| `harness/` | Plugin, recorder, playback runs, step replay, hybrid check, vector capture (`harness/README.md`) |
| `harness/vector_specs/` | Specs for fuzzed direct-call records |
| `RBR_RECORDINGS` | Recorded drives of the original (`drive-*.bin`); not in git |
| `RBR_VECTOR_STORE` | Physics vector records; not in git |
| `original/` | Original 1.02 binaries and game data (gitignored, `SHA256SUMS` only) |
| `data/` | The exe's function bounds, library functions, and the C++ reconstruction's last state |
| `notes/` | Behaviour notes per system and format; progress in `notes/progress/` |
| `scripts/` | `rust-check.sh`, `rust-vectors.sh`, `cachy-rust.sh`, generators of Rust tables, NGP tools |

Treat `original/`, `build/` and the recordings as local data. Do not commit their contents.

## Notes by task

- Scope and order of work: `notes/rust-plan.md`; every gameplay system and its Rust status: `notes/progress/systems.md`
- Binary inventory and provenance: `notes/binaries.md`
- Main executable subsystem map: `notes/exe-subsystems.md`
- Physics semantics per subsystem: `notes/physics/`; layouts and exactness findings: `notes/exe-x8.md`, `notes/rbr-physics-y16.md`
- Rust float precision, NaN and conversion rules: `notes/rust-physics.md`
- Phase B storage seam (store traits, field tables, twin, poison): `notes/phase-b-store.md`
- Gameplay input and physics loop: `notes/gameplay-loop.md`
- Runtime harness: `harness/README.md`
- Particle system (data, exe code map, signals, Rust plan): `notes/particles.md`
- Car lights (switching, signals, flares): `notes/car-lights.md`
- Stage and in-car characters (spectators, marshals, animals, driver, co-driver): `notes/stage-characters.md`
- Scripted events (car-flip marshals, rescue helicopter, bird strike, start/finish): `notes/scripted-events.md`
- Interactive scenery (static collision, props, bendable trees, fences): `notes/interactive-scenery.md`
- Car damage visuals (deformation, windows, detach, dirt): `notes/car-damage-visuals.md`
- Weather (state, grip tables, sky, precipitation, wipers): `notes/weather.md`
- Race options (damage level, driver aids, random weather draw): `notes/race-options.md`
- Cameras (player, replay, helicam, fly-in): `notes/cameras.md`
- Skid marks, world ambient sound, ghost car: `notes/marks-sound-ghost.md`
- Gameplay sound (engine, transmission, tyres, impacts, co-driver, countdown, Rust plan): `notes/sound.md`
- Visual enhancements (opt-in renderer additions, upscaled texture packs): `notes/visual-enhancements.md`
- File formats: `notes/formats/`
- NGP physics: `notes/ngp.md`, `notes/ngp-load-path.md`, `notes/ngp-replacements.md`
- Force feedback in 1.02 and NGP (torque source, curve, device output, Rust split): `notes/ngp-ffb.md`

## Rust rules

- Physics and gameplay stay bit-exact against the original's recordings.
- Presentation (rendering, sound, HUD, launcher) never changes the physics: it reads the physics memory after a step, or copies a payload at an existing host sink, without writing to it.
- One module or crate per system. Shared files (`renderer.rs`, `viewer.rs`, both `live.rs`, `lib.rs`, `Cargo.toml`, the README tables) get small hooks only. Data parsers go in `rbr-formats`, one module per format, with a test over every retail file it covers (`RBR_DATA`).
- Physics ports are pure functions over typed values; memory access (`Mem`, raw offsets) only in the subsystem's `layout.rs` or `layout/` directory.
- Never commit game files: no binaries, data, RSF content or anything extracted from them.

## Git hygiene

- Keep changes limited to the assigned system.
- Preserve unrelated dirty files in the main checkout.
- Commit small, logical batches on the worker branch. Do not push.
- Never use `git stash`: the stash is shared by every worktree, so another worker can pop your entry. Set work aside with a WIP commit or a patch file.
- Merge current `integration/handoff` into a worker branch before final verification when the branch has fallen behind.
- Report conflicts, failing checks and unresolved questions.

## Definition of done

A Rust batch includes:

- `scripts/rust-check.sh` from the repository root printing PASS, with `RBR_DATA`, `RBR_PATCH` and `RBR_RECORDINGS` set;
- for physics ports: the hybrid check (`harness/rust_step.py` with every binding over the drive's snapshots and recording) with every "run B vs run A" line equal, and fuzzed direct-call records for branches no drive reaches;
- for changes to `rbr-standalone` or `rbr-hybrid`: `rbr-standalone --boot --recording $RBR_RECORDINGS/drive-tanner-corolla.bin --stage track-54_E --steps 13000` giving 13000/13000 equal;
- for a format: a test that parses every retail file it covers;
- for visible or audible systems: before/after captures or a short audio render;
- concise notes for verified layouts and behaviour, and a report listing commits, checks and blockers.
