# Verification

How ReRBR stays bit-exact against the original 1.02 game (the oracle): a plugin records its runs, an x86 emulator replays them, Rust compared per call (vectors), per step (hybrid check), per drive (standalone). Allowed
differences are in `notes/rust-plan.md` ("Done means"); [Masks](#masks) tells where each is applied.

## Dev setup

One Linux x86_64 PC. Copy `dev.env.example` to `dev.env` (gitignored; worktrees read the main checkout's);
`scripts/dev-env.sh` loads it for the scripts and the harness (`harness/dev_env.py`), filling the folders below
from `RBR_DEV` (`~/rerbr-dev`). All values are absolute paths. The original runs vanilla 1.02 (retail data plus the
1.02 patch; [development.md](../docs/development.md#the-original-binaries)), started with Proton's `umu-run`.

| Variable | What |
| --- | --- |
| `RBR_GAME`, `RBR_PREFIX` | vanilla 1.02 install and Wine prefix; slots add `-s<N>`, `record.sh` `-rec` |
| `RBR_NGP_GAME`, `RBR_NGP_PREFIX` | NGP 7.4.778 install and prefix |
| `RBR_RSF_PREFIX` | the Wine prefix `RBR_RSF`'s game runs in |
| `RBR_PROTON` | `PROTONPATH` `umu-run` starts the games with |
| `RBR_TOOLCHAIN` | MSVC 7.1 (`msvc71/`) and its Wine prefix (`wineprefix/`) for the plugin build, per rbr-recomp's `notes/toolchain.md` |
| `RBR_RUNS`, `RBR_NGP_RUNS`, `RBR_RSF_RUNS` | recordings, playbacks, snapshots and run outputs |
| `RBR_HARNESS_PLUGIN`, `RBR_RSF_PLUGIN` | the harness DLL (`RBRRecompHarness.dll`) `play.sh` installs, rsf-session's plugin build |
| `RBR_APITRACE`, `RBR_FFB_TRACE` | capture outputs |
| `RBR_BUSY` | optional `pgrep` pattern other processes `matrix.py` waits for |

`RBR_ORIG_EXE` names the 1.02 exe (default `$RBR_PATCH/RichardBurnsRally_SSE.exe`), `RBR_SNAPS` the snapshots (default `/tmp/rbr-snaps`). `harness/slots.sh [N]` copies the install into parallel slots (default 4).

## Plugin

`harness/plugin/` builds `RBRRecompHarness.dll` (MSVC 7.1 locally: `harness/plugin/build.sh [--install]`), patching the running game in memory only when every replaced byte matches; the exe on disk stays untouched. Log: `RBRRecompHarness.log`. Always on: Wine force feedback fix (`SetFFBValue` 0x4c6b40) and unfocused running (`E03WndProc` 0x447ab0).

One file in the game folder, written by `harness/play.sh` from an environment variable, switches a mode on:

| File | Variable | Mode |
| --- | --- | --- |
| `RBRRecompRecord.on` | (record.sh) | every physics step to `RBRRecompRecord-<tick>.bin` (FRM1) |
| `RBRRecompSession.on` | (rsf-session.sh) | per stage attempt: FRM1 plus RSS1 session file `RBRRecompSession-<tick>.bin` ([RSF sessions](#rsf-sessions)) |
| `RBRRecompPlayback.bin` | `RBR_DRIVE` | play recorded controls back, 8 steps per frame; quit 2 s after the last; recorded too |
| `RBRRecompAuto.txt` | `RBR_DRIVE` | start the stage without menus: `map car weather tyre [damage [transmission]]` (damage at 0x1660850, transmission at 0x1660814) |
| `RBRRecompSnapshot.txt` | `RBR_SNAPSHOT="40 572"` | dump the process before the listed steps' calls and after ([Snapshots](#snapshots)) |
| `RBRRecompSnapAt.txt` | `RBR_SNAPAT` | dump at a function's entry and return: `<addr> <count> [<arg0-off> <min> <max>]` (load-time dumps) |
| `RBRRecompPeek.txt` | `RBR_PEEK=<file>` | pointer probes after each step ([Peek probes](#peek-probes)) |
| `RBRRecompEarlyStop.txt` | `RBR_EARLYSTOP` | quit at the first step differing from a reference ([Early stop](#early-stop)) |
| `RBRRecompAllocLog.on` | `RBR_ALLOCLOG=1` | msvcr71 heap calls and `operator new` (24-byte records, a marker per dump); rbr-recomp's `harness/allocs.py` prints them |
| `RBRRecompCamera.txt` etc. | `RBR_CAMERA`, `RBR_VOLUMES`, `RBR_POPS`, `RBR_FPS` | sound captures: camera mode, volumes, pop logs, frame-rate cap |

The transplant, count, shadow, capture and load-time dump modes run the C++ reconstruction's functions and need its build; `play.sh` here does not start them (run from rbr-recomp, `harness/README.md` there).

## Recordings

FRM1, one frame per `CPhysics::StepForward` call. Layout: `crates/rbr-recording/src/lib.rs` (little-endian; `FRM1` magic, pre/post step counters, the `StepForward` argument substeps, car block, the 0x20 car-control payload, the car, four 0xa00 wheel blocks). The car is `CCar` (`*0x8ef660`); each wheel the first 0xa00 bytes of a `CWheel` (car+0x470), zeroes where the pointer was invalid. No frame boundaries. Reader: `crates/rbr-recording` (`rec_info`, `drive_path`).

Recordings live in `RBR_RECORDINGS` (not in git) as `drive-<name>.bin` with `drive-<name>.auto` (the `StartGame` numbers). Each drive also has `drive-<name>.playback.bin`; the older drives have two identical playback runs `ref-<name>-1/2.bin` and `ref-<name>-1.bin.mask` (tanner-corolla: `ref1.bin`, `ref1.bin.mask`), made by rbr-recomp's `drive-setup.sh`.

### Making a drive

```sh
harness/record.sh <name> "<map> <car> <weather> <tyre>"      # a person drives; quit to desktop to keep it
harness/v1-drive.sh v1-<stage>-<car> "<map car weather tyre damage trans>"   # another drive's controls, no driver
harness/drive-trial.sh <drive> "<numbers>" <tag>             # StartGame numbers for a menu-started drive
```

`v1-drive.sh` also snapshots step 1000, mid-stage and the largest speed drop, and checks the snapshot run equals the drive. The references and noise mask of `drive-setup.sh` (count runs) need rbr-recomp. `harness/matrix.py` runs stage × car pairs end to end (Rust autopilot, the original twice, `rbr-standalone --boot`; `--ngp` on the NGP install).

### Snapshots

A snapshot (`<name>.snap-<step>.bin`, about 200 MB) is every committed readable region of the process, registers, x87 control word, MXCSR and stack, taken before the `StepForward` call starting at the step; a listed step is dumped again after its call (`-after.bin`). Load settle calls at 0, 40, 80 and 120, then every step from 160. They live in `RBR_SNAPS` (default `/tmp/rbr-snaps`); a drive's chain is `vcu-<drive>.snap-<step>.bin` (tanner-corolla: `vcu-cov.snap-*`). The published set (`data/test-data.snapshots`: the four reference chains, snapH, the weather test's step-1000 snapshots, load dumps under `dumps/`) comes with `scripts/fetch-test-data.sh DIR snapshots`, with the x87 cache: union of the maintainers' caches and the run-A caches used (141,591 cases, no conflicts). It holds the merge gate and the tanner-damage contact snapshots; `long_run.py` on the other chains still models some (UNVERIFIED steps, recording still bit-exact; tarmac-lancer 2000-2099, arctic-snow 1500-1599).

### Peek probes

`RBRRecompPeek.txt` lines are `name <chain> <type>`: chain starts at `*0xADDR`, adds `+0xNN` or `*`; types `f32`, `u32`, `u8`, `i32`. A bad read (`IsBadReadPtr`) gives a marker (`f32` 0x7fc00000, all ones, or i32 minimum). Output `<name>.peek.bin` starts with `PEK1`, version 1, probe count, then each probe's type and name, then records of a u32 step and one value per probe. Peeks pin gameplay state FRM1 does not hold (race clock, timing, filter).

```sh
RBR_PEEK=harness/probes/surface-tags.peek harness/play.sh <name>
cargo run -p rbr-recording --bin peek_info -- <file>
```

### Early stop

`RBRRecompEarlyStop.txt` names a reference recording. Each step is compared with the reference's same-counter step except the words in `<reference>.mask`; the game quits at the first difference. `harness/diverge.py` reads the same mask, so it reports the same step.

```sh
python3 -I harness/diverge.py REF RUN [NOISE...]          # first step where RUN departs from REF
python3 -I harness/diverge.py --mask REF NOISE...         # write REF.mask
```

### RSF sessions

Hand drives in RallySimFans (NGP 7.5.779), recorded as fully as the harness can:

```sh
scripts/rsf-session.sh install          # builds the plugin into $RBR_RSF_PLUGIN (after plugin changes)
scripts/rsf-session.sh start <name>
# start RSF (`rbr` or `rbr game`), drive any number of stages, quit the game: that ends the session
scripts/rsf-session.sh fetch <name>     # to $RBR_RECORDINGS/rsf/<name>
.venv/bin/python -I -B harness/session.py $RBR_RECORDINGS/rsf/<name>/1/session.bin
```

`start` holds `/tmp/rbr-rsf-run.lock`, adds the plugin and `.on` files to the install (a manifest lists them; the session's end removes exactly those) and records the game's PipeWire output with `pw-record`. With `RBRRecompSession.on` the plugin starts a new FRM1+RSS1 pair at each stage attempt (first `StepForward` of a stage: no file yet, or the step counter went back). It hooks the IDirectInput8, device and effect vtables, recording every module's input and FFB calls. Per step: engine, drive, tyre-noise state (`STA1`); the car control input's payload with its QPC (`CTL1`); each input-filter take of the raw control (`DLV1`); per-calling-module call counts (`MOD1`) and copies of opened `.lsp`/`.acm` files (`OPN1`, `FIL1`) via import hooks. Hooked calls append to a memory buffer a clock thread writes out every 100 ms (16 MiB a buffer; drops and a full 1 MiB setup buffer logged). Setup PRP1s keep the last per device, property, how and object. Record formats are `harness/session.py`'s docstring. A session's folder:

| Path | What |
| --- | --- |
| `<n>/physics.bin`, `<n>/session.bin` | attempt n's FRM1 and RSS1 |
| `<n>/attempt.txt` | plugin tick, RSF car in slot 5 at attempt start (`rsf_car_id`, `rsf_car_physics`, `car_name` from `Cars.ini [Car05]`), `setup=` |
| `<n>/snap-0.bin`, `<n>/snap-1520.bin` | `SNAPS=1` only: process before the attempt's first step and before step 1520 (first with player control), in the snapshot format `harness/replay.py` and `rbr-standalone --snapshot` read; ~480 MB each in RSF, 0.2-0.8 s to write |
| `<n>/files/`, `<n>/setup.lsp` | opened files by lower-case relative path (`session.py --files`); starting setup (`session.py --setup`: last `SavedGames/<pack>/*.lsp` the stage load opened, else the pack's or slot folder's `setups/d_<surface>.lsp`, surface from RSF's `stages_data.json` else the exe map table; a service change is in no file) |
| `audio.wav`, `audio.txt` | whole session's game output (f32, native rate); `start_unix_ns` (pw-record start, rough baseline), `rate`, `channels`, `node` |
| `config/` | at session start: `SavedGames/*.acm`, `pf*.rbr`, `RBRControls.ini`, `CarInfo.ini`, `Plugins/NGPCarMenu.ini`, `rsfdata/cache/cars_data.json` (FFB, aids, rotation) |
| `devices.txt` | `/dev/hidraw*`, `/dev/input/*` files the prefix holds 30 s after game start, with device names |
| `meta.txt` | `before.`/`after.` keys: date, RSF launcher and NGP.dll versions, sha256 of NGP.dll and exe, plugin commit, `RichardBurnsRally.ini [NGP]`, `RBRControls.ini`, driven cars' `steeringrotation`, `forcefeedbacksensitivity*` (`rallysimfans_personal.ini [car<id>]`, those keys only), wheel/shifter driver and lg4ff `range`, `autocenter`, `gain` |
| `harness.log`, `watch.log` | plugin log, session's |

Driverless tests: `start` takes `AUTO="map car weather tyre"`, `PLAYBACK=<FRM1 under the home>`, `FAST=N`.

Replay an attempt as `rbr-launcher`'s command does for an RSF car: `rbr-standalone --boot --recording <n>/physics.bin --stage track-<map>_<letter> --car 5 --weather W --tyre T --ngp-physics $RBR_NGP_PHYSICS --rsf-car $RBR_RSF/rsfdata/cars/<pack>` (map, weather, tyre from `GAM1`, the pack from `attempt.txt`).

Behaviour (2026-10-20, no driver, `AUTO="54 5 0 4"`, Evo IX): 7.5.779 calls the car control input `0x4e91f0` from the same site as 7.4.778, so `HookCarControl` records its controls; RSF's exe differs from 1.02 only in the PE header, the byte at 0x40b225 and appended data, so every plugin patch applies. RSF's NGP sends FFB as SetParameters, Download and Start of one constant force about 600 times a second (none while idle); never SetProperty or GetDeviceData; each device made twice. The RSF launcher left the plugin in place at its start (its "start game" untried). The idle run replays 8459 of 30000 steps equal, departing at step 9941 (first non-zero control: FFB turned the wheel), where `Game`'s control differs from the recorded payload (word 4, steering; open). Not yet seen: a restart's or second stage's rotation (no driver), SetProperty, buffered input.

Polling: the exe feeds the car from NGP's input thread (about 1000 Hz); the car control input runs on the step thread. RBRControls.dll makes its own devices, reads properties and HID (`DeviceIoControl`), bundles SDL (raw input, HIDAPI, XInput, DirectInput); JoyKey sends keys with `SendInput`: `MOD1` counts these, none recorded. Wine's dinput has one vtable per interface whose A methods call the W ones through it, so hooks pass nested calls through (a TLS guard) and record the outer call only. Wine's winedevice holds wheel and shifter as `/dev/input/event*` (winebus's SDL bus); FFB ioctls and writes traced without root by `harness/ffb_trace.sh` (notes/ngp.md "Force feedback").

Force feedback in an `AUTO` start (2026-10-21): NGP 7.5.779's FFB thread (1000 Hz, send at 0x1003c410) sends only to the STEERING action's device with force feedback (exe 0x4c6f70: `[dev+0x29c] > 0`, `[dev+8] & 0x100`). Menus bind the profile's `pf<name>.acm` (`CInputManager::Bind` 0x4ccfb0, from profile load at 0x4539cd); `AUTO` keeps default bindings, steering on the first device (T500 shifter), so EFF1 holds the effect's creation only (snapshot sanity-new4). `FFB=1` writes the newest `SavedGames/pf*.acm` into `RBRRecompFfb.on`, bound at each attempt start with the FFB fix kept in playback. ffb-e5 (det-audi66's playback, `FFB=1`): 38338 SetParameters on the G923 from NGP's thread, all S_OK; FRM1 equal through 31482 with the masks plus `car+0x65c`, `car+0x670` lagging a step at some frames (frame-timed, as `+0x658`/`+0x674`).

A stage load opens the car pack's three `setups/d_*.lsp`, RSF scans every `rsfdata/Physics/*/setups/`, and its choice is a saved setup opened from `SavedGames/<pack>/` seconds before the first step. Check: struts' effective damping in the first frame (`wheel+0x670` `CSpringDamper::CState` +0xc bump, +0x10 rebound, the setup's values with no fade) matches the picked file in all six drive2 attempts (four saved setups, two defaults).

Determinism and versions (2026-10-20). Masked here: words differing at the first step and constant after (pointers, uninitialised memory), the unused fourth word of 16-byte vectors, and car+0x658, +0x674, +0x814, +0x824, (two runs of the same controls differ in them).
- RSF vs RSF: rsf-drive1 (Evo IX on Tanner from `AUTO`) and its FRM1 played back (rsf-drive2) equal all 38263 steps.
- RSF vs 7.4.778: with the car pack in slot 5 (`scripts/ngp-install-car.py`) and its model (RSF's `Cars.ini [Car05]` and `Cars/<model>/` copied in), det-audi66's controls (Audi A2, map 66, from `AUTO`) are equal every step the playback drove (40 to 31482; past that the controls ran out and the two plugin builds feed the car differently). 7.5.779 physics equals 7.4.778's on that drive. With the slot's own model (Impreza) the car starts 2-6 cm away.
- A menu-started drive does not replay from `AUTO`: drive2 attempt 6 vs its `AUTO` controls (same car files, setup) has position equal at step 40, orientation/velocities a few ulp apart, position apart from step 80. No control reaches the car during the load settle (`CTL1` starts at step 1520), so the menu start leaves another state kept in `snap-0.bin`/`snap-1520.bin`. RSF snapshots do not step in `rbr-standalone` yet: first NGP 7.5.779 call unbound.

## Drives and the gate matrix

The gate matrix is the NGP drives `rbr-standalone --boot` runs to the end of their recording. It holds drive2 attempt
5 (`rsf/drive2`, snow, Ford Focus RS WRC 06, RSF car 55: 84084/84084) and the NGP playback-made pairs of
`harness/matrix.py` (NGP's four car slots).

The 1.02 drives below no longer run in `rbr-standalone` (it steps NGP physics only); they feed the hybrid check,
snapshots and viewer playback. Last standalone results (2026-10-20): tanner-corolla 13000/13000, tanner-damage
12120/12120, tarmac-lancer 20892/20892, arctic-snow 12490/12490.

| Drive | Map, car | Notes |
| --- | --- | --- |
| tanner-corolla | 54 (`track-54_E`), 6 Corolla | reference; weather 0, tyre 4 |
| tanner-damage | 54, 6 | damage snapshots 11827, 12071 |
| tarmac-lancer | 41 (`track-41_M`), 3 | rescue reset at step 20415 |
| arctic-snow | 13 (`track-13_O`), 6 | snow |
| `v1-*` … `v4-*`, `v7-*` | every car and main-rally map | playback-made; mostly 3,000-step snapshot runs, V7 from the boot |

Standalone check for `rbr-standalone` and `rbr-hybrid` changes:

```sh
.venv/bin/python -I -B harness/rsf_compare.py $RBR_RECORDINGS/rsf/drive2 --attempt 5 --sections physics --out DIR
```

It must print physics PASS 84084/84084 and exit 0, as only when every exact check of the run sections passes (release
builds of `rbr-standalone` and `rsf-settings`). Rust binaries run from the local `target/release/` with the local `RBR_DATA`/`RBR_PATCH`.

## Step replay

`harness/replay.py` runs one `StepForward` of the original from a snapshot in the emulator; usage, stub, table and
x87-oracle rules are its docstring. `--check` compares the replay with the real call (recording frame plus after-dump).

`harness/live.py` (replay stepped live, FRM1 per control payload) is harness-only: the viewer drives
`rbr-standalone --live` alone.

```sh
python3 -m venv .venv && .venv/bin/pip install unicorn numpy
.venv/bin/python -I harness/replay.py snap-572.bin --check snap-572-after.bin <name>.bin
```

## Hybrid check

`harness/rust_step.py` is the main per-step check: run A is the original in the emulator; run B stops at every entry
`crates/rbr-hybrid` binds (`bound.csv`) and runs the Rust port on the emulator's memory. Mechanics (declining
bindings, the two stack fills, `classify`'s exclusion rules) are its docstring; allowed stale-stack words are
[Masks](#masks).

`--recording` also compares both runs with the drive's frame. `--steps N` chains N steps from each snapshot (Emit with
the recording's control, then `StepForward(1)`). `--each` and `--only ADDR` only locate a divergence.

Run A is cached in `RBR_STEP_CACHE` (default `cache/run-a/` next to the checkout; deletable; `--no-cache` skips it). A
missing fsin/fcos is modelled and the step reported unverified; on x86_64 new results are measured locally into the cache.
The local CPU must give the reference CPU's x87 results: an i9-13900HX matches all 141,591 cached results of the
published cache (test-data v3, 2026-11-01).

Merge gate: all bindings over the nine tanner-corolla snapshots and snapH.snap-1295, and over tanner-damage's contact
snapshots, 30 steps, every "run B vs run A" line equal (`*[0-9].bin` leaves out the `-after` dumps):

```sh
cargo build --release -p rbr-hybrid
.venv/bin/python -I -B harness/rust_step.py $RBR_SNAPS/vcu-cov.snap-*[0-9].bin $RBR_SNAPS/snapH.snap-1295.bin \
    --recording $RBR_RECORDINGS/drive-tanner-corolla.bin --steps 30
.venv/bin/python -I -B harness/rust_step.py $RBR_SNAPS/vcu-tanner-damage.snap-{11820,11827,12000}.bin \
    --recording $RBR_RECORDINGS/drive-tanner-damage.bin --steps 30
```

`harness/long_run.py` runs the same rules over a drive's whole chain, one segment per snapshot to the next. It reports
the first divergence per segment and ends with the worklist. Drives: tanner-corolla, tanner-damage, tarmac-lancer,
arctic-snow.

```sh
.venv/bin/python -I -B harness/long_run.py --drive tanner-corolla -j 2 --out /tmp/long-run.md
.venv/bin/python -I -B harness/long_run.py --drive tanner-corolla --from 2000 --to 2400
```

## Vectors

Per-call records of the original, replayed by the `crates/rbr-physics` tests. Each holds the arguments, the bytes of
each window before and after, the return (eax and st0), inner calls to `external` entries, and the branch taken. They
cover branches no drive reaches, not the main check.

| Source | Use |
| --- | --- |
| `harness/vcu_vectors.py SNAPSHOT OUT --set SET [--fuzz SEED]` | every call of the set's functions in one step; `--fuzz` draws inputs, restored on return; `--vcu-on`, `--packets`, `--copy`, `--basic`, `--slip` for VCU paths the drive misses |
| `harness/direct_calls.py SNAPSHOT OUT --set SET [--addr A] [--count N] [--seed S]` | direct calls from a function's entry, inputs built by the spec's `direct_fuzz` recipe |
| `harness/branch_cover.py FUNCS SNAPSHOT --set SET --addr A ...` | each conditional jump's two ways over the same direct calls (no records): the check a function's records cover its branches |
| `harness/record_entities_b4.py` | the `entities_b4` set on load-time dumps |
| `harness/subtree_tape.py` | struts' calls over N steps (`tests/pure_struts.rs`) |
| `harness/load_*_refs.py`, `lsp_tokens.py`, `ngp_data_refs.py` | load-time objects cut from snap-0 or entry dumps, committed under `crates/rbr-physics/tests/vectors/` |

Specs per set are in `harness/vector_specs/` ([its README](../harness/vector_specs/README.md)).

Records are not committed. They live in the vector store `RBR_VECTOR_STORE` (maintainers keep one shared store;
`scripts/fetch-test-data.sh` fetches its published copy, without `dumps/`), one subdirectory per module, named after
the test's `RBR_<MODULE>_VECTORS` variable (`RBR_ENT_VECTORS` is `entities`).

```sh
.venv/bin/python -I harness/vcu_vectors.py snap-572.bin out.txt --set vcu --fuzz 1
scripts/rust-vectors.sh car_body tyre     # named modules; no argument runs every vector target
```

## Masks

Allowed differences, and where each is applied:

| Class | Applied in |
| --- | --- |
| Frame artifacts: car+0x824, control word 0x18, car+0x658, car+0x674 (frame-driven, shift by whole frames) | `diverge.py` `static_mask` |
| Lane 3 of vectors filled from stale stack: car+0x8ec … +0x150c, car+0x1cc; wheel+0x99c, +0x9bc, +0x9dc | `diverge.py` `static_mask` (the C++ gate's mask) |
| Words ever NaN, inf or \|x\| > 1e20 in the reference | `diverge.py` `static_mask` |
| Words differing between two reference runs and the count runs (about 170, heap shifts) | `REF.mask`, `diverge.py --mask` |
| Proven stale-stack words (car+0x161c, 0x22c, 0x32c, 0x38c, 0x15dc, …; wheel+0x73c) | `rust_step.py` `CAR_STATIC`/`STACK_GARBAGE` = `rbr-hybrid/src/pure/compare.rs` `STALE_CAR`/`STALE_WHEEL` |
| Other stale-stack bytes: two more run As with dead stack read as 00 and 7f; a byte excluded only if its word differed between them at every step it differed in run B | `rust_step.py` `classify`; outside lane 3 it prints a WARNING |
| wheel+0x9ac (stack garbage the emulator cannot reproduce) | `RECORDING_GARBAGE`, recording compare only |
| Frame-loop words car+0x660, +0x670, +0x814 (`CCar::Vf28x` 0x4ea020, once a rendered frame) | `RECORDING_GARBAGE` = `compare.rs` `FRAME_LOOP_CAR`, recording compare only |
| Heap pointers | counted, not compared |
| Sign of a default NaN | `rust_step.py` `nan_sign_only` |

A word joins the stale lists only with run-A evidence: the original differs from its own recording, or its stale-stack
runs differ. Keep the two lists equal.

## Comparing RSF sessions

`harness/rsf_compare.py SESSION_DIR --out REPORT_DIR [--attempt N]` compares each attempt of an RSF session (the tree rec/capture fetches: `<n>/physics.bin`, `<n>/session.bin`, `audio.wav`, `audio.txt`, `meta.txt`) with ReRBR; writes `REPORT_DIR/<n>/report.md`, `summary.json`, PNG plots, `REPORT_DIR/index.md`. Tests: `.venv/bin/python -I -B harness/tests/test_rsf_compare.py` (synthetic RSS1 files, drive2 attempt 5 end to end).

- Physics: `rbr-standalone --boot --recording` with the launcher's flags from GAM1 (map, car slot, weather, tyre, damage level when recorded; NGP physics, no NGP.dll is a FAIL; `--rsf-car`, else the stage-load OPN1 pack under `rsfdata/cars/<pack>/` (noted when differing from `<n>/attempt.txt`, whose `Cars.ini [Car05]` can lag: drive1/1), else `attempt.txt`'s `rsf_car_id` or `meta.txt`'s one `[carNN]`; retail variant is the weather's; `<n>/setup.lsp`), `--recorded-payload` (NGP live payloads are unquantized; without it the entry rounds the payload, and the game side clamps to the quantized range: NGP full lock 1.0 rounds past step 127), `--trace-out` (our FRM1 and `.ext`: force, engine speed, gear, tyre slip from STA1 where held, settle left out; words differing from the first differing step on listed apart, with the first step another word differs), `--head-gasket-durability` (`[NGP] headGasketDurability`, NGP's stall-roll edit), `--extra-payloads` (CTL1's earlier payloads at a step, a call for help FRM1 lacks), `--wall-clock` (STP1's QPC in us: NGP's starter holds 0.5 s by the wall clock, crossed a step apart in simulation time).
- Pass: `rbr-standalone` exits 0 (every step, Game's control, time card and state equal, no stop), runs the whole recording (ending ours where the recording steps on is a FAIL), STA1 bit-equal past the settle where held. Unheld words are missing coverage in the verdict (`PASS (STA1: slip only; engine, gear not recorded)`); fixed since a529d299e (drive2's dd0c14677 read engine/drive through the wrong pointers; e2e4: every word equal to the parting).
- Input, exact: [Input chain check](#input-chain-check) per attempt.
- FFB, exact per value: magnitudes are rbr-standalone's (`.ext`, `--ffb-settings`); each constant-force SetParameters (EFF1 by DIEP flags; gain != 10000 is a FAIL) matches by class (`ffb_candidates`), `s` the post of the last STP1 before the call: plain, ours at `s`/`s - 1` (EmitForce stores after STP1); burst, ours before a catch-up burst, at its last step that or the step's; gate, ours not driving, 0; carried (not copied, by decision; counted, no FAIL), settle-to-first-driving-step sets sending RSF NGP's kept force from the previous attempt (recorded car's last two magnitudes, `ffb.last`; [0] first), never 0 after it in the gate window where ours sends the step's own; gap/after, over 0.1 s without a step or past the last, 0 or the plain values; not_run, past our run (a FAIL). Without EFF1's call QPC a miss equal to the force 2 steps back stays a FAIL, named a possible late call. Settings from `rsf-settings --session` (`notes/ngp.md` "RSF settings"; `--ffb-sensitivity G,T,S` overrides); game strength/direction from `--profile pf<name>.rbr` (not in the session; else on, 1, normal); non-own sensitivity (`rallysimfans_personal.ini`) noted. Mapping alone: the same of NGP's curve of FRM1 recorded torque (car `+0xaec`, gate `+0x1ec`) with our S must equal the car's `common.lsp` S times the stage surface percentage (correlation, RMS error, lag secondary). Feel plots: both forces over the drive, 3 s at the start, the biggest impact (speed drop over 0.1 s) and slide (sideways speed), each with its lag.
- Sound, approximate only (rough baseline, no check): `rbr-sound` (chase camera default; NGP payloads at 7 ms) against `audio.wav` by CLK1, `start_unix_ns`, then loudness envelopes; per-second loudness and spectrograms side by side around the attempt only. Holds every played sound (camera, menus). Not run on RSF stages (retail maps only).
- `summary.json` `checks` gives each area's expectation, pass/fail and reason (physics, STA1, input, FFB and mapping exact; sound approximate, no verdict). An error, missing record or unrun section is a FAIL with its reason.
- Not in the session: the driven setup unless `<n>/setup.lsp` (else default boot; `--setup FILE`), race options (gearbox aids; damage before GAM1), game profile (FFB strength/direction), `NGPCarMenu.ini`, `cars_data.json` (per-car rotation).
- Gate results, 2026-10-21 (drive2: NGP 7.5.779, RSF cars, setups and aids from the capture):

  | Attempt | Physics | FFB / mapping | Input |
  | --- | --- | --- | --- |
  | drive2/5 (snow) | PASS 84084/84084 (STA1: slip only) | FFB pass (8 carried); mapping pass | PASS-BY-SEARCH |
  | drive2/2 (gravel) | exact to 23425 but `car+0x160` (broadphase link next pointer; left as is) | FFB from our physics misses only its 2 late-call sets; mapping misses 2 sets | PASS-BY-SEARCH |
  | drive2/1,3,4 (tarmac, saved setups) | part in the settle (steps 120, 40, 80) at first ground contact, mm apart (body z +2.4, -7.6, -2.6 mm): a different car or start, not drift | mapping misses 1, 1, 3 sets, each the recorded force 2 steps back (no call QPC in drive2 to tell a late call) | PASS-BY-SEARCH |
  | drive1/2 | PASS 103802/103802 `--damage off` (STA1 not recorded); on safe parts at 22803 (impact damage words `car+0x11a8`, `+0x1288..0x1290`) | FFB pass | - |
  | drive1/1 (map 127, car 24) | exact to 66628 `--damage off` (62192 on safe); parts standing, engine stalled (ours 0.002-0.007 rad/s), every FRM1 word equal but frame-loop `car+0x670` | mapping pass | - |
  | e2e4 (`FFB=1 FAST=1`, call QPC) | physics and STA1 (every word) equal to the parting at 31682 | ours every set before the parting; mapping PASS | - |

  drive1/drive2 predate GAM1's damage level (`0x1660850`): drive2 on safe (attempt 5 on off parts at 18311), drive1/2 on off with drive2/1's setup (drive1 captured no setup). The original disagrees with itself at the tarmac settle (drive1/2 vs drive2/1 part at step 120), so our tarmac physics and saved-setup load hold; what drive2's tarmac attempts ran is in no file (uncaptured service setup change; `SNAPS=1` would show the definitions). drive1/1's `attempt.txt` names car 88 (RSF's `Cars.ini [Car05]` lag); car 24 matches frame 0 (car 88 parted at step 0, body 34.9 mm high, wheel anchors `wheel+0x140` apart). drive1 lacks STA1 and snapshots, so its parting cause is in state FRM1 does not hold (not the wall clock, head-gasket edit, or a missing control call). Start pose is the original's (`0x46b160`: `(x, z, y + 0.5)`, yaw from HedgeHog `CHH_World::Unk88`, `.lbs` (2,11) position/rotation; zero matrix skipped).

`rbr-standalone --twin mem-direct --extra-payloads` delivers every earlier control payload to both playbacks before stepping; drive2 attempt 5's first extra (pre 20331, compared frame 18850) must reach both sides, else the connection-list pointer at car+0x18 differs. Both fed, all 84084 frames match with no differing car, wheel or owned word.


### Input chain check

`target/release/input_check SESSION_DIR [ATTEMPT...]` (`crates/rbr-render/src/bin/input_check`, `cargo build
--release -p rbr-render --bin input_check`; `scripts/rust-check.sh` runs it over every `$RBR_RECORDINGS/rsf/*` session
with CTL1 and an `.acm`): each CTL1 payload against the chain's for polls delivered before it. Chain, delivery rules
and assumptions for what the session lacks are `session.rs`'s docstring
(`crates/rbr-render/src/bin/input_check/session.rs`).

Verdicts: PASS needs DLV1 with every delivery's raw control equal to our poll group's (`deliveries equal D of N`; CTL1
is no gate there, a playback supplies the steering); without DLV1 every call equal after the delivery search is
PASS-BY-SEARCH (weaker evidence, gates as a pass, shown as such); anything else FAILs, as does a DLV1 session whose
check does not pair DLV1. Calls with flags 0x30 (pause menu, reset) are skipped. (`deliveries equal D of N`: D == N on
all 7 DLV1 sessions with an `.acm`.)

drive2 (G923 + T500 RS, record 6, no DLV1): every call of attempts 1-6 equal after the search (PASS-BY-SEARCH); by the
delivery estimate alone 45097/45138, 45398/45448, 61030/61118, 39997/40355, 83953/84046, 195467/195689. Through our
bindings (the `.acm` imported) drive2 gives the same numbers. drive1, ngp-pair1, rsf-test1/2: no CTL1 or no `.acm`.
DLV1 sessions on cachy (ffb-e5/e6, ffbtrace-e2e1..e4): deliveries all equal (e2e2 37455 of 37455); their calls take the
playback's controls, not the wheel's (CTL1 word 4, steering, differs: e2e2 19183 of 30363).

## Rust checks

`scripts/rust-check.sh` runs fmt, clippy, tests and vector targets for crates changed since the merge base with
`integration/handoff` (or `main`), plus dependents. It must print `rust-check: PASS` with `RBR_DATA`, `RBR_PATCH` and
`RBR_RECORDINGS` set. `RUST_CHECK_ALL=1` checks everything; `RUST_CHECK_VECTORS` picks vector modules (`none` skips).

## Measuring cost

Compare instructions and allocations (`rbr-counters`), not wall time. `rbr-standalone --boot` prints them per step,
`viewer --frames N` per frame with the GPU idle, `scripts/perf-step.sh A B` compares two builds' step over alternating
runs. Instructions and cycles are per thread and macOS only (`thread_selfcounts`, no root).

- Boot drive (tanner Corolla under 1.02, 13000 steps, M3 Pro; `scripts/perf-step.sh` now measures drive2 attempt 5):
  2.34 M instructions, 190 allocations (14 KB) per step. Cores busy raises stepping time 60-80 %, instructions under
  0.1 %; the lowest of 3 runs is the figure, since load only adds. An oversubscribed machine still adds up to 0.15 %,
  and an edit outside the step moved it 0.2-0.3 % (likely code layout and inlining), so under 0.5 % is no change.
- Viewer, track-54_E start view, 1280x720, `--no-sound`: 4.16 M instructions (0.5 % spread), 462 allocations per frame.
- GPU: `scripts/perf-gpu.py VIEWER_ARGS --frames N` gives GPU ms per pass per frame from Metal System Trace (viewer's
  work only). The GPU clock moves it 30 % between runs (states Minimum to Medium in one run), so it ranks passes; for
  build-to-build changes it needs a template with the performance state fixed (`PERF_GPU_TEMPLATE`, made in
  Instruments; `xctrace` sets neither that nor GPU counters). Start view: stage 2.0-3.6 ms of 2.6-4.5 ms.

Detail (plugin modes, C++ transplant flow, timings): rbr-recomp `harness/README.md`.
