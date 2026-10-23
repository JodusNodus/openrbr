# Development

The one file a contributor reads first. The [README](../README.md#contributing) has the short version; the notes in
[`notes/`](../notes/) go deeper, one file per area.

## What ReRBR is

ReRBR is a standalone Rust program that plays Richard Burns Rally 1.02 with NGP 7 physics. No original code runs:
no emulator, no exe, no `NGP.dll`. It reads the game data, the retail exe's data sections and NGP.dll's immutable tables
at run time. The DLL is read as a file, never loaded or executed. The goal, in parts:

- **1.02:** every retail stage and car, physics and gameplay bit-exact against recordings of the original.
- **NGP 7.4.778:** NGP's physics (`rbr-standalone --ngp-physics`), checked against the original running NGP
  ([ngp.md](../notes/ngp.md)).
- **Community content:** NGP car packs and modded stages as RallySimFans installs them, read in place
  (`--rsf-car`; [formats.md](../notes/formats.md#rsf-cars)).
- **Live play:** all of it together, on the stepping process's clock, with HUD, sound and force feedback.

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
   port bound in place of its original (`crates/rbr-hybrid`, `bound.csv`). Every "run B vs run A" line must read equal.
4. **Vectors.** Branches no drive reaches get fuzzed direct-call records (`harness/direct_calls.py`, specs in
   `harness/vector_specs/`), replayed by each module's vector test from `RBR_VECTOR_STORE`.
5. **The standalone.** `rbr-standalone --boot` loads the stage and car from the retail files, runs the gameplay loop
   and the pure Rust step, and compares the car and wheels with the recording after every step.

Done means every drive in the gate matrix equal on every step, apart from a short list of documented difference
classes (default NaN sign, stale stack words the original never reads, heap addresses, frame-loop words). Details:
[verification.md](../notes/verification.md).

The C++98 reconstruction of the 1.02 binaries, matched function by function with reccmp and read with Ghidra, lives in
its own repository, rbr-recomp. It is frozen. It keeps every note of this repository as of the split, with its full
history, so any detail cut from these notes is there under the same path. Paths under `src/`,
`symbols/` and `types/` in comments (for example `src/rbr/RBR_E45_Material.cpp:149`), and scripts no longer here
(`scripts/rec.sh`, `scripts/tweak-keys.py`), refer to rbr-recomp. Its `rerbr/` directory holds the porting
workbench, run against a ReRBR checkout: the exe's function map, the worklist and the progress tracking.

## Crates

One Cargo workspace at the root, each crate in `crates/<name>/`. `cargo build --release && target/release/rerbr`
builds what a player needs (the default members: the launcher `rerbr`, `viewer` and `rbr-standalone`) and starts the
launcher; `cargo build --workspace` builds every crate and tool.

| Crate | What |
| --- | --- |
| [`crates/rbr-formats`](../crates/rbr-formats) | Readers for the game's files, one module per format: archives (`.rbz`, CHELALIC `.dat`), stages (`.trk`, `.col`, `.mat`, `.fnc`, `.dls`, `.lbs`), cars (`Cars.ini`, car `.ini`, `.sgc`, `.lsp` with the key tables in `lsp_keys.rs`), textures, audio banks, saves, and the RallySimFans layouts (`rsf/`). Each module has a test over every retail file it covers. [formats.md](../notes/formats.md) |
| [`crates/rbr-physics`](../crates/rbr-physics) | The physics ports, 1.02 and NGP. Pure functions over typed values; raw offsets and `Mem` only in each subsystem's `layout.rs` or `layout/`. `ports.csv` lists every ported function. [physics.md](../notes/physics.md), [crate README](../crates/rbr-physics/README.md) |
| [`crates/rbr-hybrid`](../crates/rbr-hybrid) | Binds the ports into the original's step in the emulator (`bound.csv`), and holds the pure step (`src/pure/`) that runs the same bindings over a plain memory image with no emulator |
| [`crates/rbr-gameplay`](../crates/rbr-gameplay) | The gameplay loop: input filter to control payload, frame clock and substeps, stage states, timing and markers, race flow, championship, service park, saves. [gameplay.md](../notes/gameplay.md), [crate README](../crates/rbr-gameplay/README.md) |
| [`crates/rbr-standalone`](../crates/rbr-standalone) | The physics program `rbr-standalone`. `--boot` builds the physics state from the retail files and checks a recording; `--live` runs the loop for the viewer and answers each control payload with FRM1 plus HUD, damage, scene and sound records |
| [`crates/rbr-render`](../crates/rbr-render) | The wgpu renderer and the `viewer` binary: stages, cars, sky and weather, particles, lights, damage, characters, HUD. [renderer.md](../notes/renderer.md), [cameras-hud-launcher.md](../notes/cameras-hud-launcher.md), [crate README](../crates/rbr-render/README.md) |
| [`crates/rbr-sound`](../crates/rbr-sound) | Gameplay sound from the SND1 record: engine, transmission, tyres, impacts, co-driver, ambient. [sound.md](../notes/sound.md), [crate README](../crates/rbr-sound/README.md) |
| [`crates/rbr-launcher`](../crates/rbr-launcher) | The egui launcher, binary `rerbr`: car, stage, weather, settings pages, `launcher-settings.ini`. It starts the viewer and `rbr-standalone --live --boot` as child processes |
| [`crates/rbr-recording`](../crates/rbr-recording) | Readers for the harness files: FRM1 recordings, peek traces, capture traces, and the live records (HUD2, DMG1, SND1, ...). [crate README](../crates/rbr-recording/README.md) |
| [`crates/rbr-counters`](../crates/rbr-counters) | Work counts for measuring: the thread's retired instructions and cycles (macOS) and the process's allocations, which other load on the machine leaves alone. `rbr-standalone --boot` prints them per step, `viewer --frames` per frame, `scripts/perf-step.sh` compares two builds' step |

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

## Physics code

`rbr-physics` is a physics codebase in its own terms that produces the original's bits; it is not a transcription of
the decompile. [`ngp/heat.rs`](../crates/rbr-physics/src/ngp/heat.rs) is the reference.

- Model code (a subsystem's modules outside `layout`) names physical quantities and roles: `carcass_radius`, `spin`,
  `slip_ratio`, `heat_flow_to_air`, `TyreShape`, with units in the docs. Inputs that belong together are a type.
- No addresses, offsets, original function or class names, calling conventions, registers or `st0` in model code or
  its docs. They live in `layout`/`layout/` (memory), the `rbr-hybrid` bindings (named after the original's
  functions), `ports.csv`/`bound.csv` (address to Rust item), the tests and the notes.
- A name says what the evidence shows: a tweak key, the formula's role, units and values. Where the meaning is not
  known, a neutral role name (`blend_weight`) documented as named by use, never `f_1e8` or `w7c`, and no invented
  physics.
- Constants are plain literals or `std` constants (`TAU`, `293.14`, `0.78_f32 as f64`), not bit patterns, where the
  value is exactly the same.
- Exactness first: every kept computation keeps its operation order and rounding (`fpu` helpers for the x87 chains,
  comments for unordered compares like `maxss`). A rename never changes a result; the vectors and the drive prove it.

## Set up

Tools: Rust through rustup (`rust-toolchain.toml` pins the version CI uses), SDL3, `pkg-config`, and on Linux the
ALSA and udev development packages ([README](../README.md#build-from-source) lists them). The harness tools need
Python 3 with `unicorn` and `numpy` in `.venv/` (`python3 -m venv .venv && .venv/bin/pip install unicorn numpy`).

On Windows (10 or 11, x86_64): rustup's MSVC toolchain with the Visual Studio Build Tools ("Desktop development with
C++"), and cmake instead of SDL3 and `pkg-config`: add `--features rbr-render/sdl-static` to every cargo build, test
and clippy, which builds SDL3 from source and links it in, as CI does.

```sh
git clone https://github.com/JodusNodus/rerbr.git && cd rerbr
cargo test --workspace
```

That needs nothing else. The first run builds everything (the tests at `opt-level = 3`): about 5 minutes with `-j 4`
on an M3 Pro (1932 tests). A test that reads game data or recordings prints "skipping" and passes when its variable
is unset; `scripts/rust-check.sh` says which are unset. Set them to run those tests too:

| Variable | What it is | How to make it |
| --- | --- | --- |
| `RBR_DATA` | the extracted retail 1.0 install: every installer group in its own folder (`Root/`, `Cars/`, `Maps/`, ...) | `rerbr --extract-test-data DISC PATCH OUT` (below): `OUT/retail-install` |
| `RBR_PATCH` | the official 1.02 patch's `Main_group` | the same command: `OUT/Main_group` |
| `RBR_NGP_DLL` | the original NGP 7.4.778 `Plugins/NGP.dll`; overrides the install's plugin | take it from the NGP 7.4.778 package; the reader checks its SHA-256 |
| `RBR_NGP_PHYSICS` | an NGP 7.4.778 `physics.rbz` with NGP cars in slots 2, 4, 5 and 6 | below |
| `RBR_NGP` | a folder with `7.4.778/physics.rbz` and NGP car `.lsp` files under `cars-rsf/` (key test in `rbr-formats`) | the NGP package's `physics.rbz`, plus the car packs' folders |
| `RBR_RSF` | a RallySimFans install, or a folder of sample packs from one | your own RSF install; it is only read |
| `RBR_SAVES` | the `SavedGames` files the original 1.02 wrote: `pf<name>.rbr`, `.acm`, `pfresult.rst` | play the original 1.02 once (make a profile, set no records) and copy the folder |
| `RBR_RECORDINGS` | recorded drives (`drive-*.bin`) of the original | `scripts/fetch-test-data.sh` (below) |
| `RBR_VECTOR_STORE` | per-call physics records of the original | the same script |

**Test data from your disc.** `DISC` is the retail disc's `.iso` or folder, `PATCH` the official 1.02 patch
(`RBR_v102.zip` or the patch `.exe`); `OUT` must be new or empty, and takes 2.8 GB:

```sh
cargo run --release -p rbr-launcher -- --extract-test-data DISC PATCH OUT
export RBR_DATA=OUT/retail-install RBR_PATCH=OUT/Main_group
```

`cd OUT/retail-install && shasum -a 256 -c <repo>/original/retail-1.0/install.SHA256SUMS` checks the install.

**The NGP archive and DLL.** Choose the package's `Plugins/NGP.dll` on the launcher's Settings page, or set
`RBR_NGP_DLL`, when using a separate archive or when the install has another NGP version. The data reader supports the verified 7.4.778 DLL only. Take `physics.rbz` from the NGP package (`RBR_NGP-7.4.778.507.7z`) and put four NGP cars in
it, each from its gvrc.de physics pack (`<Car> - NGP Physics.zip`, unzipped; RSF installs the same folders under
`rsfdata/cars/`). No RSF install is needed:

```sh
cp <NGP package>/physics.rbz ngp-physics.rbz
python3 -I scripts/ngp-install-car.py ngp-physics.rbz mg_zr    "BMW_M3_E30_GrpA_ngp6"                         # slot 2
python3 -I scripts/ngp-install-car.py ngp-physics.rbz p_206    "Ford_Fiesta_Rally4_ngp6"                      # slot 4
python3 -I scripts/ngp-install-car.py ngp-physics.rbz s_i2003  "Ford_Fiesta_Rally3_ngp6"                      # slot 5
python3 -I scripts/ngp-install-car.py ngp-physics.rbz t_coroll "Toyota_Celica_2000GT-FOUR_RC_ST185_GrpA_ngp6" # slot 6
export RBR_NGP_PHYSICS=$PWD/ngp-physics.rbz
```

**Recordings, vectors and snapshots.** The set the tests and the checks read is published as release assets;
`data/test-data.SHA256SUMS` names the release and checks them. The recordings and vectors download about 345 MB and
unpack to about 5.5 GB; the snapshots, for the hybrid check and the vector tools, add about 155 MB (9.2 GB
unpacked) and the x87 results cache:

```sh
eval "$(scripts/fetch-test-data.sh ~/rerbr-test-data)"              # RBR_RECORDINGS, RBR_VECTOR_STORE
eval "$(scripts/fetch-test-data.sh ~/rerbr-test-data snapshots)"    # RBR_SNAPS, RBR_REPLAY_FPU_CACHE
```

The x87 cache holds the hardware results of the transcendentals (`fsin`, `fcos`, ...) the merge gate reaches,
measured on the CPU the recordings ran on. A run that reaches a case it lacks models it and reports those steps
UNVERIFIED; the maintainers measure them on the test machine and publish a new cache, so name them in the pull
request.

New recordings and snapshots come from the original game through the harness: the plugin in a vanilla 1.02 install
on a test machine with the original game, driven over ssh ([verification.md](../notes/verification.md#test-machine):
the machine, its variables, making a drive); the maintainers make them. `scripts/pack-test-data.sh` packs a new set
for a new release: the recordings `data/test-data.recordings` lists, the vector store without its dumps, and the
snapshots `data/test-data.snapshots` lists. NGP's snapshots hold NGP's code and are not published.

## Running the launcher

`scripts/launcher.sh` builds the release programs and starts the launcher with the overrides below set from the main
checkout's `original/` folder. Its arguments go to the launcher.

The launcher's NGP DLL field follows the RSF or game folder's `Plugins/NGP.dll` while empty. An explicit choice is
saved as `ngp_dll` and passed to child processes as `RBR_NGP_DLL`. Missing or unsupported DLLs block Start.

The overrides, for development or split setups, replace parts of the player's game folder:

| Override | Replaces |
| --- | --- |
| `RBR_DATA`, or `rerbr --data DIR` | the game data. The extracted retail 1.0 install works too (its `Root/` holds what an installed game has at the top). |
| `RBR_PATCH` | the 1.02 files: the official patch's extracted `files/Main_group` |
| The Settings page's NGP physics.rbz field (`ngp_physics`; unsaved: `RBR_NGP_PHYSICS`) | NGP's archive |
| The Settings page's RSF folder field (`rsf_root`; unsaved: `RBR_RSF`) | the RallySimFans folder |

The launcher's options (`rerbr --help` lists them; the viewer and `rbr-standalone` have `--help` too):

| Option | What it does |
| --- | --- |
| `--data DIR` | the game data, instead of the install folder's (or `RBR_DATA`) |
| `--viewer PATH`, `--standalone PATH` | the two programs, if they are not next to the launcher |
| `--list` | prints how many maps can start, every map and RSF car that cannot, with the reason, the added cars, and what the install lacks; no window |
| `--command` | prints the viewer command Start would run with the saved selection: `RBR_DATA=` and `RBR_PATCH=` lines, the program, then one argument a line |
| `--screenshot DIR` | draws the pages offscreen to PNGs in DIR; without a game, the first start's page |
| `--add-cars DIR` | adds gvrc.de's NGP car packs downloaded into DIR to `cars/` in the player's files folder |
| `--download-ngp GAME` | downloads NGP 7.4.778 from gvrc.de into the game folder GAME (replaces `physics.rbz`, keeping the retail one as `physics.rbz.bak`) |
| `--download-cars [NAME...]` | downloads the named cars from gvrc.de's list and adds them like `--add-cars`; without names, prints the list |
| `--install-from-disc DISC PATCH NGP OUT` | sets up a game folder from the disc, `-` for NGP downloads it ([install-from-disc.md](install-from-disc.md)) |
| `--extract-test-data DISC PATCH OUT` | writes the test data (Set up, above) |

Every option but `--data`, `--viewer` and `--standalone` exits when done. `--list`, `--command` and `--screenshot`
only read `launcher-settings.ini`. The AppImage takes the same options, for example `./ReRBR-*.AppImage --list`.

On Windows the launcher and the viewer are GUI programs (`windows_subsystem`), so they open no console window. Started
without a stderr (from Explorer or `cmd`), the launcher writes its stderr to `rerbr.log` in the player's files folder
(`%APPDATA%\ReRBR\rerbr.log`), rewritten at each start; the viewer and `rbr-standalone` it starts write there too
(`crates/rbr-render/src/console.rs`). Started with one (Git Bash, a redirect), they keep it. What the options print
on stdout shows only when redirected: `rerbr.exe --list > list.txt`.

## Checks

From the repository root, before a merge:

```sh
RBR_DATA=<retail install> RBR_PATCH=<1.02 patch Main_group> RBR_RECORDINGS=<recordings> scripts/rust-check.sh
```

It runs `cargo fmt --check`, clippy with `-D warnings`, the tests and the physics vectors for the crates the change
touches and their dependents, and prints PASS or FAIL. `RUST_CHECK_ALL=1` checks every crate,
`RUST_CHECK_VECTORS=none` skips the vectors, as does an unset `RBR_VECTOR_STORE`. Tests that need data skip when its
variable is unset (`RBR_RSF`, `RBR_NGP_PHYSICS` too). CI (`.github/workflows/ci.yml`) runs fmt, clippy and the tests
on macOS, Ubuntu and Windows (`sdl-static` on the last two); on macOS (arm64) with the published recordings and
vectors, and the vector targets. The retail files are not published, so CI skips the tests that need them; run those
locally with your own `RBR_DATA` and `RBR_PATCH`. The vector targets hold on an arm64 host only (notes/physics.md,
"Default NaN sign").

Also, by kind of change:

- **Physics port:** the hybrid check with every binding over the drive's snapshots and recording, every "run B vs
  run A" line equal; fuzzed direct-call records for branches no drive reaches. Both run on the published snapshots
  ([verification.md](../notes/verification.md#hybrid-check) has the commands).
- **`rbr-standalone` or `rbr-hybrid`:** `harness/rsf_compare.py $RBR_RECORDINGS/rsf/drive2 --attempt 5 --sections
  physics --out DIR` gives physics PASS 84084/84084 (release `rbr-standalone` and `rsf-settings`; `RBR_NGP_PHYSICS`
  and `RBR_RSF` set).
- **Format:** a test that parses every retail file it covers.
- **Visible or audible system:** before/after captures (viewer `--screenshot`) or a short audio render.

Commands and the snapshot set: [verification.md](../notes/verification.md).

## Packaging

`scripts/package.sh` builds the players' download into `dist/` (git-ignored) with `cargo build --profile play`; the
version is the launcher's (`crates/rbr-launcher`). Every shipped program prints it with `--version`. SDL3 is built
from source and linked in (`rbr-render` feature `sdl-static`; needs cmake), so nothing else needs installing.

- **macOS:** `dist/ReRBR-<version>-macos.dmg` with `ReRBR.app`: the launcher (`rerbr`) is the bundle's executable,
  the viewer and `rbr-standalone` sit beside it in `Contents/MacOS`, built for macOS 11 (`MACOSX_DEPLOYMENT_TARGET`)
  and linking only the system's libraries. The app is ad-hoc signed, not notarised (no Developer ID), and Apple
  silicon only: Homebrew's Rust has no x86_64 standard library.
- **Linux:** `dist/ReRBR-<version>-<arch>.AppImage` (needs the ALSA and udev headers). appimagetool and its runtime
  are downloaded and checked against pinned SHA-256 sums. Build in an old distribution so the glibc floor stays low,
  for example in Docker from the repository root:

  ```sh
  docker run --rm -v "$PWD":/src -w /src -e CARGO_TARGET_DIR=/target rust:1-bookworm bash -c \
    'apt-get update && apt-get install -y cmake libasound2-dev libudev-dev && scripts/package.sh'
  ```
- **Windows:** `dist/ReRBR-<version>-windows-x86_64.zip`: `rerbr.exe`, `viewer.exe` and `rbr-standalone.exe` in a
  `ReRBR` folder. Run the script in Git Bash (it comes with Git for Windows) with the MSVC toolchain and cmake set up
  (Set up). The C runtime is linked in (`crt-static`, and SDL3 with it through
  `scripts/packaging/windows-static-crt.cmake`), so players need no Visual C++ redistributable. Unsigned: the
  README says how to get past SmartScreen.

The icon is in `scripts/packaging/`: `icon.icns` for the macOS app, `icon.png` for the AppImage, `icon.ico` for
Windows (embedded in `rerbr.exe` and `viewer.exe` by their crates' `build.rs`, `winresource`), and `icon-256.png` on
the windows themselves (`rbr_render::icon`). `.github/workflows/release.yml`
runs the script on a pushed `v*` tag and attaches the packages to a draft GitHub release.

The player's files go to the platform's data folder, not next to the programs (`crates/rbr-launcher/src/home.rs`;
[playing.md](playing.md), "Where your files go"), since an app bundle is read-only.

## Where things live

| Path | What |
| --- | --- |
| `Cargo.toml`, `Cargo.lock`, `rustfmt.toml`, `crates/` | the workspace (crates above) |
| `harness/` | plugin, recorder, playback runs, step replay, hybrid check, vector capture ([README](../harness/README.md)) |
| `scripts/` | `rust-check.sh`, `rust-vectors.sh`, `fetch-test-data.sh` and `pack-test-data.sh`, `remote-rust.sh` (runs on the test machine), `launcher.sh`, `package.sh` and the icon (`packaging/`), generators of Rust tables from the binaries (`gen-*.py`), NGP tools |
| `notes/` | one note per area (below); `references/` third-party material with its licence |
| `data/` | the test data's hashes and file lists, and generator inputs (`physics-lsp-fields.md`, `ngp-lsp-keys.md`) |
| `original/` | original binaries and game data, gitignored apart from `SHA256SUMS` (the binaries the harness runs) and `retail-1.0/install.SHA256SUMS` (checks an extracted `RBR_DATA`, see Set up) |
| `build/`, `target/`, `dist/` | local caches (the emulator's x87 cache), the build, the packages; gitignored |
| `RBR_RECORDINGS`, `RBR_VECTOR_STORE`, `RBR_SNAPS` | recorded drives, vector records and snapshots, outside git (`scripts/fetch-test-data.sh`) |

Notes:

| Note | Covers |
| --- | --- |
| [physics.md](../notes/physics.md) | the step, x87 and float rules, store seam, each physics subsystem |
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
