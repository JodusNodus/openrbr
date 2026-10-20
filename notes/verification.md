# Verification

How ReRBR stays bit-exact against the original 1.02 game, the oracle: its runs are recorded by a plugin, replayed in
an x86 emulator, and compared with Rust per call (vectors), per step (hybrid check) and per drive (standalone). The allowed differences are listed in
`notes/rust-plan.md` ("Done means"); [Masks](#masks) explains how each is applied.

## Test machine

The original runs on a test machine with the original game, in a vanilla 1.02 install (retail data with the 1.02
patch over it; [development.md](../docs/development.md#the-original-binaries)). The scripts reach it over ssh and
start the game there with Proton's `umu-run` (Linux; a Windows machine would need its own launch lines). Paths on it
are relative to its home:

| Variable | What |
| --- | --- |
| `RBR_REMOTE_HOST` | its ssh host; every script that runs the original or its CPU needs it |
| `RBR_REMOTE_GAME`, `RBR_REMOTE_PREFIX` | the vanilla 1.02 install and its Wine prefix; slots add `-s<N>`, `record.sh` `-rec` |
| `RBR_REMOTE_PROTON` | the `PROTONPATH` `umu-run` starts the game with |
| `RBR_REMOTE_NGP_GAME`, `RBR_REMOTE_NGP_PREFIX` | the NGP 7.4.778 install and its prefix (`scripts/ngp-run.sh`, `matrix.py --ngp`) |
| `RBR_REMOTE` | a work dir (`scripts/remote-setup.sh`) linking `toolchain/` (MSVC 7.1 for the plugin, as rbr-recomp's `notes/toolchain.md` sets it up), `original/` (the 1.02 exe) and `.venv` from `~/rbr-recomp-test`: the x87 oracle and the plugin build run there |
| `RBR_REMOTE_RUST` | Rust runs (`scripts/remote-rust.sh`, `matrix.py`): `src/<worktree>/`, and the data in `data/retail-install`, `data/Main_group` |
| `RBR_REMOTE_BUSY` | optional: a `pgrep` pattern of other processes `matrix.py --ngp` waits for |

Fixed folders there: `~/rbr-harness-runs/` (recordings, snapshots), `~/rbr-harness-plugin/` (the installed plugin),
`~/rbr-ngp-runs/`, `~/rbr-emu/` (`harness/remote.sh`), `~/rbr-apitrace/`. Here, `RBR_ORIG_EXE` names the 1.02 exe
(default `$RBR_PATCH/RichardBurnsRally_SSE.exe`), `RBR_SNAPS` the snapshots (default `/tmp/rbr-snaps`).
`harness/slots.sh [N]` copies the install into parallel slots (default 4).

## Plugin

`harness/plugin/` builds `RBRRecompHarness.dll` (MSVC 7.1 on the test machine: `harness/plugin/build.sh [--install]`).
It patches the running game in memory only when every replaced byte matches; the exe on disk stays untouched. It logs to `RBRRecompHarness.log`. It always fixes force
feedback under Wine (`SetFFBValue` 0x4c6b40) and keeps running unfocused (`E03WndProc` 0x447ab0).

A mode is switched on by a file in the game folder, which `harness/play.sh` writes from an environment variable:

| File | Variable | Mode |
| --- | --- | --- |
| `RBRRecompRecord.on` | (record.sh) | Record every physics step to `RBRRecompRecord-<tick>.bin` (FRM1) |
| `RBRRecompSession.on` | (rsf-session.sh) | Record per stage attempt: FRM1 and an RSS1 session file `RBRRecompSession-<tick>.bin` ([RSF sessions](#rsf-sessions)) |
| `RBRRecompPlayback.bin` | `RBR_DRIVE` | Play recorded controls back, 8 steps per frame; quit 2 s after the last; recorded too |
| `RBRRecompAuto.txt` | `RBR_DRIVE` | Start the stage without menus: `map car weather tyre [damage [transmission]]` (damage at 0x1660850, transmission at 0x1660814) |
| `RBRRecompSnapshot.txt` | `RBR_SNAPSHOT="40 572"` | Dump the process before the listed steps' calls and after them ([Snapshots](#snapshots)) |
| `RBRRecompSnapAt.txt` | `RBR_SNAPAT` | Dump at a function's entry and return: lines `<addr> <count> [<arg0-off> <min> <max>]` (load-time dumps) |
| `RBRRecompPeek.txt` | `RBR_PEEK=<file>` | Pointer probes after each step ([Peek probes](#peek-probes)) |
| `RBRRecompEarlyStop.txt` | `RBR_EARLYSTOP` | Quit at the first step that differs from a reference ([Early stop](#early-stop)) |
| `RBRRecompAllocLog.on` | `RBR_ALLOCLOG=1` | Log msvcr71 heap calls and `operator new` (24-byte records, a marker per dump); rbr-recomp's `harness/allocs.py` prints them |
| `RBRRecompCamera.txt` etc. | `RBR_CAMERA`, `RBR_VOLUMES`, `RBR_POPS`, `RBR_FPS` | Sound captures: camera mode, volumes, pop logs, frame-rate cap |

The transplant, count, shadow, capture and load-time dump modes run the C++ reconstruction's functions and need its
build. They stay in the plugin, but `play.sh` here does not start them: run them from rbr-recomp (`harness/README.md`
there).

## Recordings

FRM1, one frame per `CPhysics::StepForward` call, little-endian, repeated to EOF:

| Field | Bytes | Meaning |
| --- | --- | --- |
| magic | 4 | `FRM1` (0x314d5246) |
| pre, post | 4 + 4 | step counter before and after the call |
| substeps | 4 | `StepForward` argument; one substep is 7 ms (1/720 s under NGP) |
| car_bytes | 4 | 0x1700 in 1.02 |
| control | 0x20 | the car-control payload fed to the step |
| car | car_bytes | `CCar` (`*0x8ef660`) |
| wheels | 4 × 0xa00 | the first 0xa00 bytes of each `CWheel` (car+0x470); zero where the pointer was invalid |

FRM1 has no frame boundaries. Reader: `crates/rbr-recording` (`rec_info`, `drive_path`).

Recordings live in `RBR_RECORDINGS` (not in git), as `drive-<name>.bin` with `drive-<name>.auto` (the `StartGame`
numbers). On the test machine each drive also has `drive-<name>.playback.bin`; the older drives also have two
identical playback runs `ref-<name>-1/2.bin` and `ref-<name>-1.bin.mask` (tanner-corolla: `ref1.bin`,
`ref1.bin.mask`), made by rbr-recomp's `drive-setup.sh`.

### Making a drive

```sh
harness/record.sh <name> "<map> <car> <weather> <tyre>"      # a person drives; quit to desktop to keep it
harness/v1-drive.sh v1-<stage>-<car> "<map car weather tyre damage trans>"   # another drive's controls, no driver
harness/drive-trial.sh <drive> "<numbers>" <tag>             # find StartGame numbers for a menu-started drive
```

`v1-drive.sh` also snapshots step 1000, mid-stage and the largest speed drop, and checks that the snapshot run equals
the drive. The references and noise mask of `drive-setup.sh` (count runs) need rbr-recomp. `harness/matrix.py` runs stage × car pairs
end to end (Rust autopilot, the original twice, `rbr-standalone --boot` against it; `--ngp` on the NGP install).

### Snapshots

A snapshot (`<name>.snap-<step>.bin`, about 200 MB) is every committed readable region of the process, the
registers, x87 control word, MXCSR and the stack, taken before the `StepForward` call that starts at the step; a
listed step is dumped again after its call (`-after.bin`). The load settle calls at 0, 40, 80 and 120, then every
step from 160. They live in `RBR_SNAPS` (default `/tmp/rbr-snaps`); a drive's chain is `vcu-<drive>.snap-<step>.bin`
(tanner-corolla: `vcu-cov.snap-*`). The published set (`data/test-data.snapshots`: the four reference chains, snapH,
the weather test's step-1000 snapshots and the load dumps under `dumps/`) comes with `scripts/fetch-test-data.sh DIR
snapshots`, with the x87 cache: the union of the maintainers' caches and the results the run-A caches used (141,591
cases, no conflicts). It holds every case of the merge gate and the tanner-damage contact snapshots; `long_run.py` on
the other chains still models some (UNVERIFIED steps, the recording still bit-exact; tarmac-lancer 2000-2099,
arctic-snow 1500-1599).

### Peek probes

`RBRRecompPeek.txt` lines are `name <chain> <type>`: the chain starts at `*0xADDR` and adds `+0xNN` or `*`; types
are `f32`, `u32`, `u8` and `i32`. A bad read (`IsBadReadPtr`) gives a marker (`f32` 0x7fc00000, all ones, or i32 minimum). The output `<name>.peek.bin` starts with `PEK1`, version 1 and the probe
count, then each probe's type and name, then records of a u32 step and one value per probe. Peeks pin gameplay
state FRM1 does not hold (race clock, timing, filter).

```sh
RBR_PEEK=harness/probes/surface-tags.peek harness/play.sh <name>
cargo run -p rbr-recording --bin peek_info -- <file>
```

### Early stop

`RBRRecompEarlyStop.txt` names a reference recording. Each step is compared with the reference's step of the same
counter, except the words in `<reference>.mask`, and the game quits at the first difference. `harness/diverge.py`
reads the same mask, so it reports the same step.

```sh
python3 -I harness/diverge.py REF RUN [NOISE...]          # first step where RUN departs from REF
python3 -I harness/diverge.py --mask REF NOISE...         # write REF.mask
```

### RSF sessions

Drives by hand in RallySimFans (NGP 7.5.779) on the test machine, recorded as fully as the harness can:

```sh
scripts/rsf-session.sh start <name>     # from the Mac (builds and installs the plugin; or `rsf-record start <name>` there)
# start RSF (`rbr` or `rbr game`), drive any number of stages, quit the game: that ends the session
scripts/rsf-session.sh fetch <name>     # to $RBR_RECORDINGS/rsf/<name>
.venv/bin/python -I -B harness/session.py $RBR_RECORDINGS/rsf/<name>/1/session.bin
```

`start` holds `/tmp/rbr-rsf-run.lock`, adds the plugin and the `.on` files to the install (a manifest lists them; a
session's end removes exactly those) and records the game's PipeWire output with `pw-record`. With
`RBRRecompSession.on` the plugin starts a new FRM1 and RSS1 pair at each stage attempt (the first `StepForward` of a
stage: no file yet, or the step counter went back). It hooks the IDirectInput8, device and effect vtables, so every
module's input and FFB calls are recorded. Per step it writes the engine, drive and tyre-noise state (`STA1`) and the
car control input's payload with its QPC (`CTL1`); each time the input filter takes the raw control it writes that
(`DLV1`, `E16Filter::NotifyB`; under NGP once per burst of steps, on the physics thread). Import hooks in the game
folder's modules count calls per calling module (`MOD1`, including the paths around DirectInput: raw input, HID
`ReadFile`/`DeviceIoControl`, XInput, `SendInput`) and copy the `.lsp` and `.acm` files the game opens (`OPN1`,
`FIL1`, up to 1 MiB each). RSS1's format is `harness/session.py`'s docstring. A session's folder:

| Path | What |
| --- | --- |
| `<n>/physics.bin`, `<n>/session.bin` | stage attempt n's FRM1 and RSS1 |
| `<n>/attempt.txt` | the plugin's tick, RSF's car in slot 5 when the attempt began (`rsf_car_id`, `rsf_car_physics`, `car_name` from `Cars.ini [Car05]`), `setup=` |
| `<n>/snap-0.bin`, `<n>/snap-1520.bin` | with `SNAPS=1` only: the process before the attempt's first step and before step 1520 (the first with the player's control), the snapshot format `harness/replay.py` and `rbr-standalone --snapshot` read; ~480 MB each in RSF, 0.2-0.8 s to write |
| `<n>/files/`, `<n>/setup.lsp` | the files the attempt opened, by lower-case relative path (`session.py --files`); the setup its car started on (`session.py --setup`: the last `SavedGames/<pack>/*.lsp` the stage load opened, else the car pack's or slot folder's `setups/d_<surface>.lsp`, the surface from RSF's `stages_data.json` for its stages, else the exe's map table; a change at service is in no file) |
| `audio.wav`, `audio.txt` | the game's output for the whole session (f32, native rate); `start_unix_ns` (when pw-record started, a rough baseline), `rate`, `channels`, `node` |
| `config/` | as the session started: the controller and game profiles (`SavedGames/*.acm`, `pf*.rbr`), `RBRControls.ini`, `CarInfo.ini`, `Plugins/NGPCarMenu.ini`, `rsfdata/cache/cars_data.json` (FFB, aids, rotation) |
| `devices.txt` | the `/dev/hidraw*`, `/dev/input/*` files the game's prefix holds 30 s after the game starts, with their device names |
| `meta.txt` | `before.` and `after.` keys: date, RSF launcher and NGP.dll versions, sha256 of NGP.dll and the exe, plugin commit, `RichardBurnsRally.ini [NGP]`, `RBRControls.ini`, the driven cars' `steeringrotation` and `forcefeedbacksensitivity*` (`rallysimfans_personal.ini [car<id>]`, those keys only), the wheel's and shifter's driver and lg4ff `range`, `autocenter`, `gain` |
| `harness.log`, `watch.log` | the plugin's log, the session's |

For tests without a driver `start` takes `AUTO="map car weather tyre"`, `PLAYBACK=<FRM1 under the home>` and `FAST=N`.

Replay an attempt as `rbr-launcher`'s command does for an RSF car: `rbr-standalone --boot --recording <n>/physics.bin
--stage track-<map>_<letter> --car 5 --weather W --tyre T --ngp-physics $RBR_NGP_PHYSICS --rsf-car
$RBR_RSF/rsfdata/cars/<pack>` (map, weather, tyre from `GAM1`, the pack from `attempt.txt`).

Checked (2026-10-20, no driver, `AUTO="54 5 0 4"`): 7.5.779 calls the car's control input `0x4e91f0` from the same
site as 7.4.778, so `HookCarControl` records its controls; RSF's exe differs from 1.02 only in the PE header, the byte
at 0x40b225 and appended data, so every plugin patch applies. RSF's NGP sends FFB as SetParameters, Download and Start
of one constant force about 600 times a second; the test install sent none while idle. Neither calls SetProperty or
GetDeviceData; the game makes each device twice. The RSF launcher left the plugin in place at its start (its "start
game" was not tried). The idle Evo IX run replays 8459 of 30000 steps equal: it departs at step 9941, the first non-zero
control (FFB turned the wheel), where `Game`'s control differs from the recorded payload (word 4, steering; open). Not
yet seen: a restart's or second stage's rotation (no driver), SetProperty, buffered input.

Force feedback in an AUTO start (2026-10-21): NGP 7.5.779's FFB thread (1000 Hz, send at 0x1003c410) sends only to the
STEERING action's device and only when it has force feedback (exe 0x4c6f70: `[dev+0x29c] > 0`, `[dev+8] & 0x100`).
The menus bind the profile's `pf<name>.acm` (`CInputManager::Bind` 0x4ccfb0, from the profile load at 0x4539cd); an
AUTO start skips them and keeps the default bindings, steering on the first device (the T500 shifter), so EFF1 holds
the effect's creation only (snapshot of sanity-new4). `FFB=1` writes the newest `SavedGames/pf*.acm` into
`RBRRecompFfb.on`; the plugin binds it at each attempt start and keeps the FFB fix in playback. ffb-e5 (det-audi66's
playback, `FFB=1`): 38338 SetParameters on the G923 from NGP's thread, all S_OK; FRM1 equal to det-audi66 through
31482 with the masks plus `car+0x65c` and `car+0x670`, which there lag a step at some frames (frame-timed, like
`+0x658`/`+0x674`).

Input paths (2026-10-20, Evo IX, G923 and T500 shifter): the exe's poll function feeds the car, called from NGP's input
thread at about 1000 Hz; the car control input runs on the step thread. RBRControls.dll makes its own devices and
reads properties and HID (`DeviceIoControl`); it bundles SDL (raw input, HIDAPI, XInput, DirectInput), and JoyKey sends
keys with `SendInput`: `MOD1` counts these, they are not recorded. Wine's dinput has one vtable per interface whose A
methods call the W ones through it, so the plugin's hooks pass nested calls through (a TLS guard) and record the outer
call only. Wine's winedevice holds the wheel and shifter as `/dev/input/event*` (evdev), so device-level FFB is only
visible to usbmon (root: `modprobe usbmon`, then read access to `/dev/usbmon<bus>`). A stage load opens the car pack's
three `setups/d_*.lsp`, RSF scans every `rsfdata/Physics/*/setups/`, and RSF's setup choice is a saved setup it opens
from `SavedGames/<pack>/` a few seconds before the first step. Check: the struts' effective damping in the first frame
(`wheel+0x670` `CSpringDamper::CState` +0xc bump, +0x10 rebound, the setup's values with no fade) matches the picked
file in all six drive2 attempts (four saved setups, two defaults).

Determinism and versions (2026-10-20). Masked in these comparisons: words that differ at the first step and stay
constant (pointers, uninitialised memory), the unused fourth word of 16-byte vectors and car+0x658, +0x674, +0x814,
+0x824, which two runs of the same controls differ in.
- RSF against RSF: rsf-drive1 (the Evo IX on Tanner from `AUTO`) and its FRM1 played back (rsf-drive2) are equal for
  all 38263 common steps.
- RSF against 7.4.778: with the car's pack in slot 5 (`scripts/ngp-install-car.py`) and its model (RSF's
  `Cars.ini [Car05]` and `Cars/<model>/` copied in), det-audi66's controls (Audi A2, map 66, from `AUTO`) are equal for
  every step the playback drove (40 to 31482; from 31483 the controls had run out and the two plugin builds feed the
  car differently). 7.5.779's physics equals 7.4.778's on that drive. With the slot's own model instead (the Impreza's),
  the car starts 2-6 cm away.
- A drive started from RSF's menus does not replay from `AUTO`: drive2 attempt 6 against its controls from `AUTO` (same
  car files and setup) has the position equal at step 40, the orientation and velocities a few ulp apart, and the
  position apart from step 80. No control reaches the car during the load settle (`CTL1` starts at step 1520), so the
  menu start leaves another state; `snap-0.bin` and `snap-1520.bin` keep it. RSF's snapshots do not step in
  `rbr-standalone` yet: it stops at the first NGP 7.5.779 call (not bound).

## Drives and the gate matrix

The gate matrix is the set of drives `rbr-standalone --boot` runs equal to the end of their recording. Today it holds
the first four below: tanner-corolla 13000/13000, tanner-damage 12120/12120, tarmac-lancer 20892/20892, arctic-snow
12490/12490 (2026-10-20). The goal counts as reached only when the matrix covers every car, every surface class
(gravel, tarmac, snow), damage and collisions.

| Drive | Map, car | Notes |
| --- | --- | --- |
| tanner-corolla | 54 (`track-54_E`), 6 Corolla | the reference; weather 0, tyre 4 |
| tanner-damage | 54, 6 | damage snapshots 11827, 12071 |
| tarmac-lancer | 41 (`track-41_M`), 3 | rescue reset at step 20415 |
| arctic-snow | 13 (`track-13_O`), 6 | snow |
| `v1-*` … `v4-*`, `v7-*` | every car and main-rally map | playback-made; mostly 3,000-step snapshot runs, V7 from the boot |

The standalone check for `rbr-standalone` and `rbr-hybrid` changes:

```sh
rbr-standalone --boot --recording $RBR_RECORDINGS/drive-tanner-corolla.bin --stage track-54_E --steps 13000
```

It must print 13000/13000 equal. `scripts/remote-rust.sh BIN [args]` runs a Rust binary on the test machine instead
(release build, that machine's copy of the retail data; paths are relative to its home, recordings in
`rbr-harness-runs/`).

## Step replay

`harness/replay.py` runs one `StepForward` of the original from a snapshot in unicorn. Windows calls are stubbed
with deterministic stand-ins; an unstubbed call stops the replay. rsqrtss/rcpss come from the CPU's
tables (`harness/replay-approx.bin`); x87 transcendentals and fscale from the CPU itself (`harness/replay-fpu.c` over
ssh to `RBR_REMOTE_HOST` in `RBR_REMOTE`, cached in `build/replay-fpu-cache.bin`). `RBR_REMOTE_HOST=` runs the oracle locally on the test machine.

`--check` compares the replay with the real call (the recording's frame and the after-dump). Bytes that depend on the stubs
are found by two more runs and left out.

```sh
python3 -m venv .venv && .venv/bin/pip install unicorn numpy
.venv/bin/python -I harness/replay.py snap-572.bin --check snap-572-after.bin <name>.bin
```

## Hybrid check

`harness/rust_step.py` is the main per-step check. Run A is the original in the emulator. Run B stops at every entry
`crates/rbr-hybrid` binds (`bound.csv`), runs the Rust port on the emulator's memory and returns as the original's
`ret` would (eax, x87 result, popped arguments). A binding may decline; the original then runs and the count is reported. It
refuses a `librbr_hybrid` older than the sources (`RBR_HYBRID` names another).

Run B is made twice, with the free stack filled with 0x00 and with 0x7f after each binding (`FILLS`). Both are
needed: 0x7f produces differences of its own and opens other read windows. By default each call's fill is the
bytes run A left there in the same call (`--full-fill` fills the whole free stack instead).

Every byte of writable memory and the stub heap is compared. A byte that differs is excluded only by these rules
(`classify`):

- diverge.py's static mask (`STATIC_FRAME_WORDS`: the car's and wheels' lane-3 words);
- a heap pointer in both runs (placement, not value);
- a NaN in both runs that differs only in sign;
- the original's stale-stack noise ([Masks](#masks));
- a byte run A never reads. A 16-byte copy is not a read: the difference follows the copy (`COPY_STORES`).

`--recording` also compares both runs with the drive's frame. `--steps N` chains N steps from each snapshot (Emit
0x48cd90 with the recording's control, then `StepForward(1)`). `--each` and `--only ADDR` are only for locating a
divergence.

Run A is cached in `RBR_STEP_CACHE` (default `cache/run-a/` next to the checkout; deletable; `--no-cache` skips it).
A missing fsin/fcos is modelled and the step reported unverified; with `RBR_REMOTE` it is fetched from the CPU.

Merge gate: all bindings over the nine tanner-corolla snapshots and snapH.snap-1295, and over tanner-damage's
contact snapshots, 30 steps, every "run B vs run A" line equal (`*[0-9].bin` leaves out the `-after` dumps):

```sh
cargo build --release -p rbr-hybrid
.venv/bin/python -I -B harness/rust_step.py $RBR_SNAPS/vcu-cov.snap-*[0-9].bin $RBR_SNAPS/snapH.snap-1295.bin \
    --recording $RBR_RECORDINGS/drive-tanner-corolla.bin --steps 30
.venv/bin/python -I -B harness/rust_step.py $RBR_SNAPS/vcu-tanner-damage.snap-{11820,11827,12000}.bin \
    --recording $RBR_RECORDINGS/drive-tanner-damage.bin --steps 30
```

`harness/long_run.py` runs the same rules over a drive's whole chain, one segment from each snapshot to the next. It
reports the first divergence per segment and ends with the worklist. Drives: tanner-corolla, tanner-damage,
tarmac-lancer, arctic-snow.

```sh
.venv/bin/python -I -B harness/long_run.py --drive tanner-corolla -j 2 --out /tmp/long-run.md
.venv/bin/python -I -B harness/long_run.py --drive tanner-corolla --from 2000 --to 2400
```

`harness/remote.sh <name> <script> <args...>` runs a harness script on the test machine (sync, `rbr-hybrid` build, snapshot and
recording paths rewritten to shared caches; at most four at once). Give each parallel run its own name.

## Vectors

Per-call records of the original, replayed by the `crates/rbr-physics` tests. Each record holds the arguments, the
bytes of each window before and after, the return (eax and st0), inner calls to `external` entries, and the branch
taken. Use them for branches no drive reaches, not as the main check.

| Source | Use |
| --- | --- |
| `harness/vcu_vectors.py SNAPSHOT OUT --set SET [--fuzz SEED]` | every call of the set's functions in one step; `--fuzz` draws inputs and restores them on return; `--vcu-on`, `--packets`, `--copy`, `--basic`, `--slip` for VCU paths the drive misses |
| `harness/direct_calls.py SNAPSHOT OUT --set SET [--addr A] [--count N] [--seed S]` | direct calls from a function's entry, with inputs built by the spec's `direct_fuzz` recipe |
| `harness/branch_cover.py FUNCS SNAPSHOT --set SET --addr A ...` | each conditional jump's two ways over the same direct calls (no records): the check that a function's records cover its branches |
| `harness/record_entities_b4.py` | the `entities_b4` set on load-time dumps |
| `harness/subtree_tape.py` | the struts' calls over N steps (`tests/pure_struts.rs`) |
| `harness/load_*_refs.py`, `lsp_tokens.py`, `ngp_data_refs.py` | load-time objects cut from snap-0 or entry dumps, committed under `crates/rbr-physics/tests/vectors/` |

Specs per set are in `harness/vector_specs/` ([its README](../harness/vector_specs/README.md): `fuzz`,
`direct_fuzz`, `fuzz_args`, `result`, `after`, inner windows).

Records are not committed. They live in the vector store `RBR_VECTOR_STORE` (the maintainers keep
one shared store; `scripts/fetch-test-data.sh` fetches its published copy, without `dumps/`), one subdirectory per
module, named after the test's `RBR_<MODULE>_VECTORS` variable (`RBR_ENT_VECTORS` is `entities`).

```sh
RBR_REMOTE=<dir on the test machine> .venv/bin/python -I harness/vcu_vectors.py snap-572.bin out.txt --set vcu --fuzz 1
scripts/rust-vectors.sh car_body tyre     # named modules; no argument runs every vector target
```

## Masks

The allowed differences, and where each one is applied:

| Class | Applied in |
| --- | --- |
| Frame artifacts: car+0x824, control word 0x18, car+0x658, car+0x674 (frame-driven, shift by whole frames) | `diverge.py` `static_mask` |
| Lane 3 of vectors filled from stale stack: car+0x8ec … +0x150c, car+0x1cc; wheel+0x99c, +0x9bc, +0x9dc | `diverge.py` `static_mask` (the C++ gate's mask) |
| Words that ever hold NaN, inf or \|x\| > 1e20 in the reference | `diverge.py` `static_mask` |
| Words that differ between two reference runs and the count runs (about 170, heap shifts) | `REF.mask`, `diverge.py --mask` |
| Proven stale-stack words (car+0x161c, 0x22c, 0x32c, 0x38c, 0x15dc, …; wheel+0x73c) | `rust_step.py` `CAR_STATIC`/`STACK_GARBAGE` = `rbr-hybrid/src/pure/compare.rs` `STALE_CAR`/`STALE_WHEEL` |
| Other stale-stack bytes: two more run As with dead stack read as 00 and 7f; a byte excluded only if its word differed between them at every step it differed in run B | `rust_step.py` `classify`; outside lane 3 it prints a WARNING |
| wheel+0x9ac (stack garbage the emulator cannot reproduce) | `RECORDING_GARBAGE`, recording compare only |
| Frame-loop words car+0x660, +0x670, +0x814 (`CCar::Vf28x` 0x4ea020, once a rendered frame) | `RECORDING_GARBAGE` = `compare.rs` `FRAME_LOOP_CAR`, recording compare only |
| Heap pointers | counted, not compared |
| Sign of a default NaN | `rust_step.py` `nan_sign_only` |

A word joins the stale lists only with run-A evidence: the original differs from its own recording, or its
stale-stack runs differ. Keep the two lists equal.

## Rust checks

`scripts/rust-check.sh` runs fmt, clippy, tests and vector targets for the crates changed since the merge base with
`integration/handoff` (or `main`), plus their dependents. It must print `rust-check: PASS` with `RBR_DATA`, `RBR_PATCH` and
`RBR_RECORDINGS` set. `RUST_CHECK_ALL=1` checks everything; `RUST_CHECK_VECTORS` picks vector modules (`none` skips).

## Measuring cost

Compare instructions and allocations (`rbr-counters`), not wall time. `rbr-standalone --boot` prints them per step,
`viewer --frames N` per frame with the GPU idle, and `scripts/perf-step.sh A B` compares two builds' step over
alternating runs. Instructions and cycles are per thread and macOS only (`thread_selfcounts`, no root).

- Boot drive (tanner Corolla, 13000 steps, M3 Pro, main 985e389): 2.34 M instructions, 190 allocations (14 KB) per
  step. With every core busy, stepping time rose 60-80 % and instructions under 0.1 %; the lowest of 3 runs is the
  figure, since load only adds. An oversubscribed machine still adds up to 0.15 %, and an edit outside the step
  moved it 0.2-0.3 % (likely code layout and inlining), so under 0.5 % is no change.
- Viewer, track-54_E start view, 1280x720, `--no-sound`: 4.16 M instructions (0.5 % spread), 462 allocations per frame.
- `perf/step-store` (17bb00c) against its base: -2.20 % instructions per step.
- GPU: `scripts/perf-gpu.py VIEWER_ARGS --frames N` gives GPU ms per pass per frame from Metal System Trace (the
  viewer's work only). The GPU's clock moves it 30 % between runs (states Minimum to Medium in one run), so it ranks
  passes; for build-to-build changes it needs a template with the performance state fixed (`PERF_GPU_TEMPLATE`, made
  in Instruments; `xctrace` sets neither that nor GPU counters). Start view: stage 2.0-3.6 ms of 2.6-4.5 ms.

Detail (plugin modes, C++ transplant flow, timings): rbr-recomp `harness/README.md`.
