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

The C++98 reconstruction of the 1.02 binaries lives in its own repository, rbr-recomp. It is frozen. It keeps every note of this repository as of the split,
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

## Set up

Tools: Rust through rustup, plus SDL3, `pkg-config` and on Linux the ALSA and udev development packages (the
[README](../README.md) lists the packages). The harness tools need Python 3 with `unicorn` and `numpy` in `.venv/`.

`cargo test --manifest-path rust/Cargo.toml --workspace` needs nothing else. Every test that reads game data or
recordings prints "skipping" and passes when its variable is unset. Set the variables to run those tests too:

| Variable | What it is | How to make it |
| --- | --- | --- |
| `RBR_DATA` | the extracted retail 1.0 install | from your own disc: `rbr-launcher --install-from-disc` ([install-from-disc.md](install-from-disc.md)), or copy an installed game's folder |
| `RBR_PATCH` | the official 1.02 patch's `files/Main_group` | `scripts/is_sfx_extract.py` on the patch, then `unshield` the inner cab |
| `RBR_NGP_PHYSICS` | an NGP 7.4.778 `physics.rbz` with NGP cars in slots 2, 4, 5 and 6 | from the NGP package (`RBR_NGP-7.4.778.507.7z`), cars put in with `scripts/ngp-install-car.py` |
| `RBR_NGP` | a folder with `7.4.778/physics.rbz` and NGP car `.lsp` files under `cars-rsf/` (key test in `rbr-formats`) | the NGP package, plus the `.lsp` files of RSF's NGP cars |
| `RBR_RSF` | a RallySimFans install, or a folder of sample packs from one | your own RSF install; it is only read |
| `RBR_SAVES` | profile and save files the original 1.02 wrote | play the original 1.02 once and copy its save files (the profile test reads `pfthomas.rbr`, a maintainer's profile) |
| `RBR_RECORDINGS` | recorded drives (`drive-*.bin`) of the original | the harness plugin in the original game ([verification.md](../notes/verification.md)) |
| `RBR_VECTOR_STORE` | per-call physics records of the original | the harness's vector tools in the emulator, from snapshots of the original |

Recordings, snapshots and vectors come from the original game through the harness, on a Windows or Wine machine
with the game. For now only the maintainers make them and they are not published, so outsiders run the data tests
and leave the recording and vector tests skipped.

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

## Packaging

`scripts/package.sh` builds the players' download into `dist/` (git-ignored) with `cargo build --profile play`; the
version is `rbr-launcher`'s. Every shipped program prints it with `--version`.

- **macOS:** `dist/ReRBR-<version>-macos.dmg` with `ReRBR.app`: the launcher is the bundle's executable, the viewer
  and `rbr-standalone` sit beside it in `Contents/MacOS`, and SDL3's dylib (Homebrew's, `brew install sdl3`) is copied
  into `Contents/Frameworks` with its install name rewritten. The app is ad-hoc signed, not notarised (no Developer
  ID), and Apple silicon only: Homebrew's Rust has no x86_64 standard library. Its minimum macOS is the dylib's.
- **Linux:** `dist/ReRBR-<version>-<arch>.AppImage`. SDL3 is built from source and linked in (`rbr-render` feature
  `sdl-static`; needs cmake and the ALSA and udev headers). appimagetool and its runtime are downloaded
  and checked against pinned SHA-256 sums. Build in an old distribution so the glibc floor stays low, for example in
  Docker from the repository root:

  ```sh
  docker run --rm -v "$PWD":/src -w /src -e CARGO_TARGET_DIR=/target rust:1-bookworm bash -c \
    'apt-get update && apt-get install -y cmake libasound2-dev libudev-dev && scripts/package.sh'
  ```

The icon is ours: `packaging/icon.py` draws `packaging/icon.png`. `.github/workflows/release.yml` runs the script on
a pushed `v*` tag and attaches the packages to a draft GitHub release.

The player's files go to the platform's data folder, not next to the programs (`rbr-launcher/src/home.rs`;
[playing.md](playing.md), "Where your files go"), since an app bundle is read-only.

## Where things live

| Path | What |
| --- | --- |
| `rust/` | the workspace (crates above) |
| `harness/` | plugin, recorder, playback runs, step replay, hybrid check, vector capture ([README](../harness/README.md)) |
| `scripts/` | `rust-check.sh`, `rust-vectors.sh`, `cachy-rust.sh` (runs on the test machine), `launcher.sh`, generators of Rust tables from the binaries (`gen-*.py`), NGP tools, `progress.py` |
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
| RSF install | a player's RallySimFans install: read it, never modify it. Its exe differs from 1.02 only by the large-address flag, the checksum, one code byte at file offset `0xb225` and trailing zeros |
| Vanilla 1.02 test install | on a Windows or Wine machine, ours to run: retail data with the 1.02 patch laid over it. It needs `trackinfo.dat` in the game root (29 bytes, `"rbr pc version 0.42"`; copy in `original/install-generated/`) |

The physics is `NPhysics::` in the exe: each component (`CCar`, `CWheel`, `CTyre`, `CEngine`, `CDrive`,
`CSpringDamper`, ...) splits into `CDef`, `CSetup`/`CSetupOptions`, `CState` and `CScratch`, with values set
through the `NTweak` key tables from the per-car files in `physics.rbz`.
