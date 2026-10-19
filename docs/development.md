# Development

The one file a contributor reads first. The [README](../README.md#contributing) has the short version; the notes in
[`notes/`](../notes/) go deeper, one file per area.

## What ReRBR is

ReRBR is a standalone Rust program that plays Richard Burns Rally 1.02 with NGP 7 physics. No original code runs:
no emulator, no exe, no `NGP.dll`. It reads the game data and the retail exe's data sections (`.data`/`.rdata`, read
at run time, never transcribed). The goal, in parts:

- **1.02:** every retail stage and car, physics and gameplay bit-exact against recordings of the original.
- **NGP 7.4.778:** NGP's physics (`rbr-standalone --ngp-physics`), checked against the original running NGP
  ([ngp.md](../notes/ngp.md)).
- **Community content:** NGP car packs and modded stages as RallySimFans installs them, read in place
  (`--rsf-car`; [formats.md](../notes/formats.md#rsf-cars)).
- **Live play:** all of it together, with pacing, HUD, sound and force feedback.

The 1.02 UI, menus and renderer are not carried over. Rendering and sound are new and may look and sound better than
the original. Scope and milestones: [rust-plan.md](../notes/rust-plan.md). Where each system stands:
[status.md](../notes/status.md).

## How it stays exact

The original game is the oracle. Every physics and gameplay result is compared with what 1.02 itself computed.

1. **Recordings.** A plugin in the original game (`harness/plugin/`) writes one FRM1 frame per physics step: the
   control payload, the car and the wheels. Drives are recorded by hand or made by playing a drive's controls back on
   another stage and car. They live outside git in `RBR_RECORDINGS`.
2. **Snapshots and the emulator.** The plugin also dumps the game's memory at chosen steps. `harness/replay.py` runs
   one `StepForward` of the original from a snapshot in a unicorn emulator, bit-exact with the game (x87 at PC=24,
   the CPU's own `rsqrtss` tables).
3. **The hybrid check.** `harness/rust_step.py` runs the same step twice: run A all original, run B with every Rust
   port bound in place of its original (`rust/rbr-hybrid`, `bound.csv`). Every "run B vs run A" line must read equal.
4. **Vectors.** Branches no drive reaches get fuzzed direct-call records (`harness/direct_calls.py`, specs in
   `harness/vector_specs/`), replayed by each module's vector test from `RBR_VECTOR_STORE`.
5. **The standalone.** `rbr-standalone --boot` loads the stage and car from the retail files, runs the gameplay loop
   and the pure Rust step, and compares the car and wheels with the recording after every step.

Done means every drive in the gate matrix equal on every step, apart from a short list of documented difference
classes (default NaN sign, stale stack words the original never reads, heap addresses, frame-loop words). Details:
[verification.md](../notes/verification.md).

The C++98 reconstruction of the 1.02 binaries lives in its own repository, rbr-recomp
(`/Users/thomas/Projects/rbr-recomp-cpp`). It is frozen. It keeps every note of this repository as of the split,
with its full history, so any detail cut from these notes is there under the same path. Paths under `src/`,
`symbols/` and `types/` in comments (for example `src/rbr/RBR_E45_Material.cpp:149`), and scripts no longer here
(`scripts/rec.sh`, `scripts/tweak-keys.py`), refer to rbr-recomp. `data/rbr.recomp.csv` keeps its last source file,
reccmp score and name per function for the tools here.

## Crates

All in `rust/`, one workspace.

**rbr-formats.** Readers for the game's files, one module per format: archives (`.rbz`, CHELALIC `.dat`), stages
(`.trk`, `.col`, `.mat`, `.fnc`, `.dls`, `.lbs`), cars (`Cars.ini`, car `.ini`, `.sgc`, `.lsp` with the key tables in
`lsp_keys.rs`), textures, audio banks, saves, and the RallySimFans layouts (`rsf/`). Each module has a test over every
retail file it covers. [formats.md](../notes/formats.md).

**rbr-physics.** The physics ports, 1.02 and NGP. Pure functions over typed values; raw offsets and `Mem` only in each
subsystem's `layout.rs` or `layout/`. `ports.csv` lists every ported function. [physics.md](../notes/physics.md),
[crate README](../rust/rbr-physics/README.md).

**rbr-hybrid.** Binds the ports into the original's step in the emulator (`bound.csv`), and holds the pure step
(`src/pure/`) that runs the same bindings over a plain memory image with no emulator.

**rbr-gameplay.** The gameplay loop: input filter to control payload, frame clock and substeps, stage states,
timing and markers, race flow, championship, service park, saves. [gameplay.md](../notes/gameplay.md),
[crate README](../rust/rbr-gameplay/README.md).

**rbr-standalone.** The physics program. `--boot` builds the physics state from the retail files and checks a
recording; `--live` runs the loop for the viewer and answers each control payload with FRM1 plus HUD, damage, scene
and sound records.

**rbr-render.** The wgpu renderer and the `viewer` binary: stages, cars, sky and weather, particles, lights, damage,
characters, HUD. [renderer.md](../notes/renderer.md), [cameras-hud-launcher.md](../notes/cameras-hud-launcher.md),
[crate README](../rust/rbr-render/README.md).

**rbr-sound.** Gameplay sound from the SND1 record: engine, transmission, tyres, impacts, co-driver, ambient.
[sound.md](../notes/sound.md), [crate README](../rust/rbr-sound/README.md).

**rbr-launcher.** The egui launcher: car, stage, weather, settings pages, `launcher-settings.ini`. It starts the
viewer and `rbr-standalone --live --boot` as child processes.

**rbr-recording.** Readers for the harness files: FRM1 recordings, peek traces, capture traces, and the live records
(HUD2, DMG1, SND1, ...). [crate README](../rust/rbr-recording/README.md).

## Rules

- Physics and gameplay stay bit-exact against the original's recordings. The asm of the original decides, not the
  C++ reconstruction and not a nicer formula.
- Presentation (rendering, sound, HUD, launcher) never changes the physics. It reads the physics memory after a step,
  or copies a payload at an existing host sink, without writing to it.
- One module or crate per system. Shared files (`renderer.rs`, `viewer.rs`, both `live.rs`, `lib.rs`, `Cargo.toml`,
  the README tables) get small hooks only.
- Data parsers go in `rbr-formats`, with a test over every retail file they cover (`RBR_DATA`).
- Float rules (x87 precision, NaN, conversions): [physics.md](../notes/physics.md#x87-and-float-rules).
- Never commit game files: no binaries, data, RSF or NGP content, or anything extracted from them. Neither NGP nor
  modded content is committed; bring your own copy, as with the game.
- Portable code for macOS and Linux; English only.

## Checks

From the repository root, before a merge:

```sh
RBR_DATA=<retail install> RBR_PATCH=<1.02 patch Main_group> RBR_RECORDINGS=<recordings> scripts/rust-check.sh
```

It runs `cargo fmt --check`, clippy with `-D warnings`, the tests and the physics vectors for the crates the change
touches and their dependents, and prints PASS or FAIL. `RUST_CHECK_ALL=1` checks every crate,
`RUST_CHECK_VECTORS=none` skips the vectors. Tests that need data skip when its variable is unset (`RBR_RSF`,
`RBR_NGP_PHYSICS` too).

Also, by kind of change:

- **Physics port:** the hybrid check with every binding over the drive's snapshots and recording, every "run B vs
  run A" line equal; fuzzed direct-call records for branches no drive reaches.
- **`rbr-standalone` or `rbr-hybrid`:** `rbr-standalone --boot --recording $RBR_RECORDINGS/drive-tanner-corolla.bin
  --stage track-54_E --steps 13000` gives 13000/13000 equal.
- **Format:** a test that parses every retail file it covers.
- **Visible or audible system:** before/after captures (viewer `--screenshot`) or a short audio render.

Commands and the snapshot set: [verification.md](../notes/verification.md).

## Where things live

| Path | What |
| --- | --- |
| `rust/` | the workspace (crates above) |
| `harness/` | plugin, recorder, playback runs, step replay, hybrid check, vector capture ([README](../harness/README.md)) |
| `scripts/` | `rust-check.sh`, `rust-vectors.sh`, `cachy-rust.sh`, `launcher.sh`, generators of Rust tables from the binaries (`gen-*.py`), NGP tools, `progress.py` |
| `notes/` | one note per area (below); `progress/` holds the tracking CSVs; `physics/renames/` the C++ rename tables; `references/` third-party material with its licence |
| `data/` | the exe's function bounds (`rbr.functions.csv`), library functions and globals (`rbr.csv`), the C++ reconstruction's last state (`rbr.recomp.csv`), and generator inputs (`physics-lsp-fields.md`, `ngp-lsp-keys.md`) |
| `original/` | original binaries and game data, gitignored, `SHA256SUMS` only |
| `build/` | local caches, gitignored |
| `RBR_RECORDINGS`, `RBR_VECTOR_STORE` | recorded drives and vector records, outside git |
| `LOG.md` | current state and open decisions |

Notes:

| Note | Covers |
| --- | --- |
| [physics.md](../notes/physics.md) | the step, x87 and float rules, store seam, each physics subsystem, exe map |
| [ngp.md](../notes/ngp.md) | NGP 7.4.778: load path, replacements, NGP keys, force feedback, H-shifter |
| [formats.md](../notes/formats.md) | every file format and its parser |
| [renderer.md](../notes/renderer.md) | renderer architecture and every visual system |
| [sound.md](../notes/sound.md) | gameplay sound |
| [gameplay.md](../notes/gameplay.md) | control payload, race flow, timing, options, weather, season, saves |
| [cameras-hud-launcher.md](../notes/cameras-hud-launcher.md) | cameras, HUD, launcher |
| [verification.md](../notes/verification.md) | harness, recordings, hybrid check, vectors, masks |
| [status.md](../notes/status.md) | each system's state and open items |
| [rust-plan.md](../notes/rust-plan.md) | scope, "done means", milestones |

## The original binaries

| Source | Notes |
| --- | --- |
| Retail 1.0 disc | data extracted to `original/retail-1.0/install/`; the disc exe has a copy-protection wrapper and is not used |
| Official 1.02 patch | InstallShield SFX: `scripts/is_sfx_extract.py`, then `unshield`, into `original/patch-1.02/files/`. The target: `Main_group/RichardBurnsRally_SSE.exe` (game), `HedgeHog3D.dll` (3D engine), `LocoMotion.dll` (driver animation) |
| RSF install | `cachy:~/Games/Richard Burns Rally`: the user's, never modify. Its exe differs from 1.02 only by the large-address flag, the checksum, one code byte at file offset `0xb225` and trailing zeros |
| Vanilla 1.02 test install | `cachy:~/Games/rbr-vanilla-1.02`, ours to run: retail data with the 1.02 patch laid over it, launched with `rbr-vanilla`. It needs `trackinfo.dat` in the game root (29 bytes, `"rbr pc version 0.42"`; copy in `original/install-generated/`) |

The physics is `NPhysics::` in the exe: each component (`CCar`, `CWheel`, `CTyre`, `CEngine`, `CDrive`,
`CSpringDamper`, ...) splits into `CDef`, `CSetup`/`CSetupOptions`, `CState` and `CScratch`, with values set
through the `NTweak` key tables from the per-car files in `physics.rbz`.
