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
| `RBRRecompPlayback.bin` | `RBR_DRIVE` | Play recorded controls back, 8 steps per frame; quit 2 s after the last; recorded too |
| `RBRRecompAuto.txt` | `RBR_DRIVE` | Start the stage without menus: `map car weather tyre [damage [transmission]]` (damage at 0x1660850, transmission at 0x1660814) |
| `RBRRecompSnapshot.txt` | `RBR_SNAPSHOT="40 572"` | Dump the process before the listed steps' calls and after them ([Snapshots](#snapshots)) |
| `RBRRecompSnapAt.txt` | `RBR_SNAPAT` | Dump at a function's entry and return: lines `<addr> <count> [<arg0-off> <min> <max>]` (load-time dumps) |
| `RBRRecompPeek.txt` | `RBR_PEEK=<file>` | Pointer probes after each step ([Peek probes](#peek-probes)) |
| `RBRRecompEarlyStop.txt` | `RBR_EARLYSTOP` | Quit at the first step that differs from a reference ([Early stop](#early-stop)) |
| `RBRRecompAllocLog.on` | `RBR_ALLOCLOG=1` | Log msvcr71 heap calls and `operator new` (24-byte records, a marker per dump); `harness/allocs.py` prints them |
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

FRM1 has no frame boundaries. Reader: `crates/rbr-recording` (`rec_info`, `drive_path`). `harness/explain.py OFFSET`
names the object and field of a frame offset.

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
(tanner-corolla: `vcu-cov.snap-*`).

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

## Drives and the gate matrix

`notes/progress/drives.csv` is the gate matrix: one row per end-to-end run, with identical steps out of total.
Today it holds tanner-corolla only (12084/12084, the C++ gate). The goal counts as reached only when the matrix
covers every car, every surface class (gravel, tarmac, snow), damage and collisions.

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

Merge gate: all bindings over the tanner-corolla chain plus the impact snapshots, 30 steps, every "run B vs run A"
line equal:

```sh
(cd rust && cargo build --release -p rbr-hybrid)
.venv/bin/python -I -B harness/rust_step.py $RBR_SNAPS/vcu-cov.snap-*.bin \
    --recording $RBR_RECORDINGS/drive-tanner-corolla.bin --steps 30
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

## Worklist

`harness/worklist.py` lists the original physics functions a hybrid step still runs in the emulator, by subsystem,
and what each waits for. `harness/pure_check.py` lists the host calls a pure step
would stop at. `harness/step_reached.py` counts the functions the original step reaches (for `progress.py step`).

```sh
.venv/bin/python -I -B harness/worklist.py $RBR_SNAPS/vcu-cov.snap-*.bin --out worklist.md
```

## Vectors

Per-call records of the original, replayed by the `crates/rbr-physics` tests. Each record holds the arguments, the
bytes of each window before and after, the return (eax and st0), inner calls to `external` entries, and the branch
taken. Use them for branches no drive reaches, not as the main check.

| Source | Use |
| --- | --- |
| `harness/vcu_vectors.py SNAPSHOT OUT --set SET [--fuzz SEED]` | every call of the set's functions in one step; `--fuzz` draws inputs and restores them on return; `--vcu-on`, `--packets`, `--copy`, `--basic`, `--slip` for VCU paths the drive misses |
| `harness/direct_calls.py SNAPSHOT OUT --set SET [--addr A] [--count N] [--seed S]` | direct calls from a function's entry, with inputs built by the spec's `direct_fuzz` recipe |
| `harness/record_entities_b4.py` | the `entities_b4` set on load-time dumps |
| `harness/subtree_tape.py` | the struts' calls over N steps (`tests/pure_struts.rs`) |
| `harness/load_*_refs.py`, `lsp_tokens.py`, `ngp_data_refs.py` | load-time objects cut from snap-0 or entry dumps, committed under `crates/rbr-physics/tests/vectors/` |

Specs per set are in `harness/vector_specs/` ([its README](../harness/vector_specs/README.md): `fuzz`,
`direct_fuzz`, `fuzz_args`, `result`, `after`, inner windows).

Records are not committed. They live in the vector store `RBR_VECTOR_STORE` (the maintainers keep
one shared store; there is no default for outsiders), one subdirectory per module, named after the test's
`RBR_<MODULE>_VECTORS` variable (`RBR_ENT_VECTORS` is `entities`).

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

## Progress tracking

`scripts/progress.py` reads and writes the CSV files in `notes/progress/`:

| File | Holds | Written by |
| --- | --- | --- |
| `drives.csv` | the gate matrix | by hand |
| `gameplay-functions.csv` | status per gameplay function: verified, reviewed, fixed, bad (unlisted: untouched) | `progress.py mark STATUS EVIDENCE ADDR...` |
| `called.csv` | functions each drive reaches | `progress.py called DRIVE CALLS.LOG` |
| `step-reached.csv` | functions the physics step reaches, with entries | `progress.py step DRIVE REACHED.txt` |
| `unreached.csv` | functions no drive reaches, with the condition | by hand |
| `formats.csv` | formats, parse status, retail files passing | by hand |

`progress.py report` prints the scorecard, the Rust state per subsystem (`ports.csv`, `bound.csv`) and the
reached-but-unported worklist; `steplist DRIVE` lists a drive's reached functions. System status:
[status.md](status.md).

Detail (plugin modes, C++ transplant flow, timings): rbr-recomp `harness/README.md`.
