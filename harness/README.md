# Harness

Runs our code inside the original game on the vanilla 1.02 test install
(`cachy:~/Games/rbr-vanilla-1.02`, launcher `rbr-vanilla`, see `notes/binaries.md`).

## Emulator checks on cachy

`cachy.sh` runs the emulator scripts on cachy so their CPU and memory use stays
off the Mac. Give each simultaneous run a distinct name:

```sh
harness/cachy.sh check-a rust_step.py \
  /tmp/rbr-snaps/vcu-cov.snap-300.bin \
  /tmp/rbr-snaps/vcu-cov.snap-1000.bin \
  --recording ../recordings/drive-tanner-corolla.bin --steps 30
```

The wrapper syncs this worktree's `harness/` and `rust/` into
`~/rbr-emu/src/<name>/` (excluding Rust `target/`), builds `rbr-hybrid` in
release mode with four jobs, then runs the requested harness Python script with
`nice -n 10`. Snapshot and recording arguments are copied into the shared
`~/rbr-emu/snaps` and `~/rbr-emu/recordings` caches, and the wrapper rewrites
their paths for the remote run. `/tmp/rbr-snaps` on cachy is a symlink to the
shared snapshot cache so snapshot paths in the output match Mac runs. The
wrapper sets `RBR_ORIG_EXE` to the 1.02 exe in `~/rbr-recomp-test`, and sets
`RBR_REMOTE_HOST=` so missing x87 oracle values are computed locally on cachy.
It also copies the FPU cache from `RBR_REPLAY_FPU_CACHE`, this worktree's
`build/`, or the shared `rust-integration` build; each run gets a private cache
copy to avoid concurrent cache writes.

The remote runner holds one of four `flock` slots across the build and check,
and refuses to start if cachy's root filesystem is above 90% full. For
`long_run.py`, it also syncs the local `RBR_SNAPS` directory and the selected
drive recording, then points `RBR_SNAPS` and `RBR_RECORDINGS` at the shared
caches. `rust_step.py`, `replay.py`, `direct_calls.py`, and `vcu_vectors.py`
take snapshot paths directly and use the same argument rewriting.

## Which tool when (verifying and debugging physics functions)

1. Before touching a function, read its subsystem's notes, `notes/physics/<subsystem>.md`, section 5
   ("suspect code"), and `notes/physics/tweak-keys.md` for field names. Most bugs found so far were
   listed there with the asm address.
2. Is a ported group good? `harness/rust_step.py` with every binding (below: "Rust ports in the step
   replay"), over the snapshots and the drive's recording. `--only` and bisecting only to find the
   culprit after a divergence.
3. What does the original do in one step? `harness/replay.py` (step replay, below): one physics step
   from a snapshot in the emulator, ~0.3 s, checked against the real call with `--check`.
4. A diverging word you don't recognise: `python3 -I harness/explain.py OFFSET -- ADDR...` (object,
   field, original writers).

Running our C++ functions in the game (`transplant.py`, `bisect.py`, count runs) needs the C++
reconstruction's build and lives in the rbr-recomp repository; the plugin keeps those modes.

## Plugin (`plugin/`)

RBR 1.02 loads every DLL in `Plugins/` and calls its exported
`RBR_CreatePlugin(IRBRGame*)`, which must return an `IPlugin` (the official SDK
shipped with the patch: `original/patch-1.02/files/Main_group/Plugins/RBRTestPlugin/`).
`RBRRecompHarness.dll` uses that call to patch the running game in memory, so the exe
on disk stays the untouched reference. Each patch names the original bytes it replaces
and nothing is applied unless all of them match (the NoSSE exe is left alone). The
result is written to `RBRRecompHarness.log` in the game folder.

```sh
harness/plugin/build.sh            # MSVC 7.1 on cachy (RBR_REMOTE, RBR_PLUGIN_SDK)
harness/plugin/build.sh --install  # also copy into the vanilla install's Plugins/
```

Current patches:

- Force feedback under Wine. `CInputDevice::CreateFFB` (0x004c8c60) creates the
  constant force on every force-feedback axis the device reports, but
  `CInputDevice::SetFFBValue` (0x004c6b40) updates it with `cAxes = 1` and Cartesian
  direction `{0}`. Wine/SDL report two force-feedback axes for wheels (X and Y), and
  Wine rejects the mismatched update with `E_INVALIDARG`, so no force reaches the
  wheel. The patch sends both axes with direction (+x, 0).
- Unfocused runs. The plugin subclasses the game's `E03WndProc` (0x00447ab0) and
  swallows `WM_ACTIVATEAPP(FALSE)`, `WM_ACTIVATE(WA_INACTIVE)`, and `WM_KILLFOCUS`.
  The decompiled `CPC_Application::WndProc` clears `g_7c0bbc` on deactivation, and the
  main loop skips rendering and physics while that flag is zero. The plugin logs the
  first swallowed message of each kind.

## Transplant files

The plugin loads `RBRRecompTransplant.bin` from the game folder when present: our compiled C++
functions, cut out of the C++ reconstruction's build and rebound to the original image by
`harness/transplant.py` in the rbr-recomp repository. It checks 8 original bytes per function
(nothing is applied on any mismatch), patches each original entry with a jump to a stub that
counts the call and continues into our copy, and rewrites `RBRRecompCalls.log` (address, calls)
every 5 s. `play.sh`'s transplant argument and the shadow, count and capture runs below take
such a file. Delete the .bin to run the plain game.

## Recorder, playback and shadow runs

- `RBRRecompRecord.on`: every physics step (CPhysics::StepForward, fixed 1/100 s) appends
  the player's control and the car + wheel state to `RBRRecompRecord-<tick>.bin`.
- `RBRRecompPlayback.bin` (an earlier recording): the recorded controls drive the car
  instead of the player, 8 physics steps per requested step (fast-forward: a 2-minute
  drive plays in ~25 s, bit-identical to real time); the run is recorded too, force
  feedback stays off, and the game quits 2 s after the last control.
- `RBRRecompAuto.txt` (`map car weather tyre [damage [transmission]]`: IRBRGame::StartGame arguments, plus the optional damage setting the plugin writes to 0x01660850 first, 0 off, 1 safe, 2 reduced, 3 realistic, and the transmission at 0x01660814, 0 manual, 1 automatic; the harness log prints both at each stage start) starts the
  stage without the menus. It must match the recording: the reference drive is Tanner
  (54), Corolla (6), good weather (0), gravel intermediate (4) - the menu's default tyre.
- `harness/record.sh <name> "<map> <car> <weather> <tyre>"` records a new reference
  drive. Run it from the Mac; the game shows on cachy's display. It copies slot 0 to
  its own game folder (`~/Games/rbr-vanilla-1.02-rec`, prefix
  `~/Games/prefixes/rbr-vanilla-rec`, holding slot 0's lock while copying, as
  `harness/slots.sh` does), arms the recorder (`RBRRecompRecord.on`), writes the 4
  numbers to `RBRRecompAuto.txt`, installs the plugin, and launches the game the way
  `play.sh` does. Drive the stage to the end or crash, then quit to desktop (ESC) -
  the script waits until the game quits (no timeout) and keeps the recording as
  `cachy:~/rbr-harness-runs/drive-<name>.bin` with the 4 numbers in
  `drive-<name>.auto`, refusing to overwrite an existing one. The rec folder holds no
  `RBRRecompPlayback.bin` (with it the plugin would drop the player's inputs and
  replay the old drive instead), no transplant, early-stop, shadow/count/trace/snapshot
  files. `RBRRecompAuto.txt` is present so the stage starts without menus and the
  playback later uses exactly these StartGame arguments; the plugin needs nothing
  else for a live recording (without a playback file it applies the force-feedback
  fix and passes the player's inputs through, and focus handling is always on).
  How long is fine: the Tanner drive's stage portion is ~12,040 physics steps
  (~85 s at dt 0.007); longer is OK but every harness run of the drive takes longer.
  The slots keep running while the user drives (the game may stutter; the recording
  stays valid). If the Mac disconnects mid-drive, don't re-run: the game keeps
  going, and after quitting move the recording by hand on cachy:
  `mv ~/Games/rbr-vanilla-1.02-rec/RBRRecompRecord-*.bin ~/rbr-harness-runs/drive-<name>.bin`
  and write the 4 numbers to `~/rbr-harness-runs/drive-<name>.auto`.
- `harness/play.sh <name> [transplant.bin]` does one unattended run and keeps the
  recording as `cachy:~/rbr-harness-runs/<name>.bin`, with the plugin's logs next to it
  (`<name>.harness.log`, `.calls.log`, `.shadow.log`). `RBR_DRIVE=<name>` (default
  tanner-corolla) picks the reference drive: the slot's `RBRRecompPlayback.bin` and
  `RBRRecompAuto.txt` come from `~/rbr-harness-runs/drive-<drive>.playback.bin` and
  `drive-<drive>.auto` (copied when they differ; tanner-corolla's are today's files),
  and plain transplant runs early-stop against `ref-<drive>-1.bin` (tanner-corolla:
  `ref1.bin` with `ref1.bin.mask`). Each run first deletes recordings older
  than 6 h (about 195 MB per full run), except `ref*`, `drive-*` (the recorded drives) and
  step-replay snapshots (`*.snap-*`); logs stay. Up to `RBR_SLOTS` (default 4) runs
  go at once, each in its own copy of the vanilla install and prefix (`harness/slots.sh`
  makes slots 1..N-1 from slot 0 and refreshes them, e.g. after a new playback or auto-start
  file); a run takes the first free slot's lock on the calling machine
  (`/tmp/rbr-harness.lock`, `/tmp/rbr-harness-sN.lock`) and waits while all are busy.
  Parallel runs stay bit-identical to the reference. Leftover processes of the slot's prefix
  are killed before and after each run; `harness/diverge.py REF RUN NOISE`
  reports the first step where RUN departs from REF (ignoring words that differ between
  REF and a NOISE run of the same thing, and words that ever hold garbage).
  `RBR_WRITEWATCH=1` arms 4-byte hardware write watches at `0x01660b74` and `0x01660b80`,
  then records each old/new word, writer EIP, stack code addresses and frame in the harness log.
  It also samples both words at `Load3b0` entry on frames 0, 1, 300, 1000, 2000 and 5000,
  and keeps playback running through frame 5000.
- `harness/drive-trial.sh <drive> "<numbers>" <tag>`: for a drive started through the menus
  (`RBR_REC_MENUS=1 harness/record.sh`), checks candidate StartGame numbers by playing the drive
  back with them and counting the words that differ from the live recording at sampled steps: the
  right numbers keep a flat heap-noise count, a wrong map, car or tyre makes it grow. Fix
  `drive-<drive>.auto` before `drive-setup.sh`, and afterwards check
  `diverge.py ref-<drive>-1.bin drive-<drive>.bin` (identical = the playback reproduces the drive).
- `harness/drive-setup.sh <name> [last]` (rbr-recomp repository: its count runs need the C++ build)
  turns a recorded drive into a reference drive:
  `drive-<name>.playback.bin` from `drive-<name>.bin` (the trimming rule of
  `lead/tools/lead-trim.py`: keep records with post step <= last; without last the whole
  file - pass last when the user kept driving past the stage end, as in the Tanner raw
  drive whose playback is the lead's cut through post 12040), then two plain reference
  runs without transplant (`ref-<name>-1/2`) and three count runs
  (`transplant.py --count` + `RBR_COUNT=1`) for the noise mask
  (`diverge.py --mask ref-<name>-1.bin ref-<name>-2.bin <count runs...>`, as
  `ref1.bin.mask` was made), and a check that ref-1 and ref-2 are identical. Needs
  `build/rbr` for the count transplant file. For the diverge command on another drive,
  copy this checkout's diverge.py to cachy first:
  `python3 -I harness/diverge.py` becomes e.g.
  `python3 -I /tmp/bisect-diverge-TAG.py ref-<name>-1.bin RUN.bin ref-<name>-2.bin`
  on cachy in `~/rbr-harness-runs`.
- `harness/v1-drive.sh <name> "<map car weather tyre damage transmission>"` makes a reference drive
  without a driver: tanner-corolla's controls (`drive-tanner-corolla.playback.bin`) are played back on
  another stage and car (`name` is `v1-<stage>-<car>`, e.g. `v1-tarmac-zr` `"43 2 0 0 1 0"`). Its one
  playback run's recording becomes `drive-<name>.bin` (the original's behaviour on those numbers, a crash
  included), then `drive-setup.sh` makes the references, and one snapshot run makes 3 step snapshots
  (`snaps/vcu-<name>.snap-<step>.bin`, linked from `/tmp/rbr-snaps`) at step 1000, mid-stage and the largest
  speed drop (`drive_path`, from the recording on this Mac). It refuses names with existing cachy files.
  Its references step runs `drive-setup.sh`, so make a new drive from an rbr-recomp checkout;
  `V1_REPEAT=1` and `V1_SURVEY=1` (no references) run here.
  The map/car/tyre numbers: `lead/reports/v1-drives.md`.
- Early stop (`RBRRecompEarlyStop.txt`, the Windows path of a reference recording): each
  recorded step is compared with the reference step of the same counter, ignoring the words
  in `<reference>.mask` (`harness/diverge.py --mask ref1.bin ref2.bin refnoise-count1.bin
  refnoise-count2.bin refnoise-count3.bin`: the frame artifacts, garbage and noise words; the
  count runs supply the ~170 words that differ in any harness mode); the run quits
  at the first divergence and logs it. play.sh turns it on for every plain transplant run
  against the drive's reference (`RBR_EARLYSTOP=0` to run to the end, or another
  reference's path). A failing run ends at its divergence instead of at 12,084 steps, and
  diverge.py, which also reads `REF.mask`, reports the same step; matching runs go to the end.
- Failures don't hang a run: crashes are logged with the transplanted function they
  happened in (also via return addresses on the stack), pure virtual calls (R6025) and
  physics stalls ("physics stalled ... (hung, not crashed)") kill the game. The fail
  path uses Win32 calls only and TerminateProcess, since the stuck thread may hold a
  CRT lock and ExitProcess can block in DLL detach under Wine.
- `RBRRecompCount.on` (`RBR_COUNT=1 harness/play.sh`, with `transplant.py --count`):
  each listed original is counted (`RBRRecompCalls.log`) and runs unchanged; ours never
  runs. One safe run measures which functions a drive reaches, exact ones included. The file must come from `transplant.py --count`: a normal transplant file has no
  trampolines for most functions, which then aren't counted (the plugin logs a WARNING, and their 0 means not measured).
  The harness's own hooks (CApplication::Tic 0x4804f0, the control Emit 0x48cd90) are
  never patched in any mode.
- `RBRRecompAllocLog.on` (`RBR_ALLOCLOG=1 harness/play.sh ...`): the exe's heap calls are logged (IAT hooks on
  msvcr71's `malloc`, `calloc`, `realloc`, `_strdup`, `operator delete`, `delete[]` and `free`; `operator new` is
  static and is told by its return address 0x70bee0 inside it, the caller then read from its frame). 24-byte records
  (kind, thread, return address, size or block, second argument, result), and a marker at each dump the plugin closes,
  so a run's dumps and its heap order line up. Kept as `~/rbr-harness-runs/<name>.allocs.bin`; `allocs.py` prints it
  with the caller functions.
- Sound checks, for A/B captures of the original's audio (`notes/sound.md`, `lead/reports/sound-a.md`):
  `RBRRecompCamera.txt` (`RBR_CAMERA=N`) switches to camera mode N in the race (1 chase, 3 and 4 bonnet, 5
  internal); `RBRRecompVolumes.txt` (`RBR_VOLUMES="e c f m t r"`, a number or `-` each) writes the six volume
  options every race frame, to hear one group at a time; `RBRRecompPops.txt` (`RBR_POPS=1`) hooks the exhaust
  pops' call (0x4b0720) and `E30DXMixer::Vf48`/`Vf4c` where they make or refuse a sound, and logs the non-zero
  misfire values, every 1000 emits the sound count, limit and totals, the live sound list at emits 5000 and 9000,
  and at 12000 each caller's made and refused sounds (the caller of `E32Factory::Make3` when it made the call), and
  each change of `CCarSound`'s engine-stopped flag (m_data `+0x41750`, which NGP's FMOD daemon pauses the engine on).
  `RBRRecompFps.txt` (`RBR_FPS=N`) caps the game at N frames a second in `CApplication::Tic`: the windowed game
  under Wine runs at about 2000 despite `WaitRetrace`, and the original's per-frame sounds follow the frame rate.
  The pops log gives the frame count with the emits. The plugin also logs each app state change (the race is 1, a
  replay 8).
- `RBRRecompShadow.on` (`RBR_SHADOW=1 harness/play.sh ...`): shadow-eligible functions
  run twice on the same live input, the original (through a trampoline of its displaced
  prologue) and ours. Return value, x87 st0, callee-saved registers, bytes popped and
  the memory either copy may write are compared, the game continues with the original's
  results (so the drive stays on the reference trajectory), and `RBRRecompShadow.log`
  lists calls and mismatches per function with the first difference and that call's
  eax, ecx, edx and first 4 stack arguments. A crash in our copy counts as a mismatch.
  One run tests every eligible function the drive reaches.

  transplant.py marks a function eligible only when running it twice can't change the
  game: a leaf (no calls except pure CRT math, no virtual calls or tail jumps, in either
  copy), not a fragment, the same register inputs and bytes popped in both copies
  (LTCG picks a calling convention per function; a mismatch would hand our copy garbage),
  and every store of both copies goes to the stack or within [-0x40, 0x800) of an input
  register or one of the first 8 stack arguments: nothing through a pointer loaded from
  memory, nothing global (those writes would happen twice and stay). It also records
  which bytes around which input each copy can write; the plugin snapshots and restores
  only those, so it never rewrites memory the function can't touch (other threads write
  the heap meanwhile; blanket 2 KB windows around every register crashed stage loads).
  The plugin also hands each copy the caller's xmm0-7 and returns the original's (LTCG
  callers keep values in xmm registers a callee leaves alone), lets one thread shadow at
  a time (others run the original), and probes memory with VirtualQuery.

## Capture (`RBRRecompTrace.on`)

`RBR_TRACE=N harness/play.sh <name> transplant.bin` (or `RBRRecompTrace.on` with N in it,
set by hand) logs test vectors for the eligible functions in `transplant.bin`
(shadow flag set: leaves, see the shadow section). Each shadow entry runs the original
only; ours never runs, and the game keeps the original's results, so the drive is the
reference. The first N calls of each function go to `RBRRecompTrace-<tick>.bin`
(`play.sh` keeps it as `<name>.trace.bin`), read by `rust/rbr-recording`
(`rbr_recording::trace`, `trace_info <file>`). Each call records:

- the registers on entry, the first 16 stack argument words, and the x87 control word
  (`cw_in`) and whether an x87 value came in (`fpu_in`);
- on return, the registers, bytes popped, st0 and the control word (`cw_out`);
- every input region as bytes before and after the call: the default window `[-0x40, 0x800)`
  around each register and stack-argument input (reads as well as writes), and one hop
  through `this + 0x10` and `this + 0x14` (CScratch's tyre parameters and `m_state`).

The control word tells the x87 precision the game runs at (bits 8-9: 0 = 24-bit,
2 = 53-bit).

Which functions: `transplant.py N` (shadow flags, leaves only, as above). Capture runs the
original once per call, so a function can be captured whatever it calls, but transplant.py only
writes input windows for the leaves it can shadow.

Limits of what is recorded (for replay, see notes/rust-physics.md):

- Only the first 16 stack argument words are recorded. A function with more arguments
  loses the rest.
- Memory outside the windows (globals, the caller's frames beyond the window, anything
  reached through a pointer chain longer than one hop) is not recorded. A replay that
  reads it panics (Mem is strict); a write outside the windows is not compared at all.

To turn a capture trace into vcu-format vectors for the Rust tests: `python3 -I -B harness/trace_to_vectors.py <trace.bin> <out.txt> <function address>...`.

## Pointer probes

`RBRRecompPeek.txt` enables optional reads after each hooked `StepForward` call. Each
non-comment line is `name <chain> <type>`, where type is `f32`, `u32`, `u8`, or `i32`.
Chains start with `*0xADDRESS` (read a pointer at an absolute address), then add
hexadecimal offsets (`+0xNN`) or use `*` to read another pointer. The final address is
read as the declared type. Every pointer and final read is checked with `IsBadReadPtr`;
an invalid read is written as a type-specific marker (`f32` quiet NaN bits `0x7fc00000`,
`u32`/`u8` all ones, or `i32` minimum).

The output `RBRRecompPeek-<tick>.bin` has a 12-byte header: `PEK1` magic, version 1,
probe count. Each probe then has a type byte, a little-endian `u16` name length and UTF-8
name bytes. Records follow to EOF; each is a `u32` step counter, then one typed value per
probe in header order. Four-byte values and the step counter are little-endian.

`RBR_PEEK=<local file> harness/play.sh <name>` copies the config into the locked slot,
then saves the output beside the recording as `~/rbr-harness-runs/<name>.peek.bin`.
Runs without `RBR_PEEK` remove any leftover config from the slot. The reader summarizes
step ranges, integer tag counts, and float ranges:

```sh
cargo run --manifest-path rust/Cargo.toml -p rbr-recording --bin peek_info -- <file>
```

`probes/surface-tags.peek` is the first example. `*0x8ef660` loads the player car,
`+0x470` selects the four-wheel pointer array, and each slot offset selects one wheel.
The wheel's scratch is inline at `CWheel+0x800`; surface ids x/y and the blend factor z
are therefore at `CWheel+0xc58`, `+0xc5c`, and `+0xc54` respectively.

## Step replay (`replay.py`)

Replays one `CPhysics::StepForward` call of the original game in an x86 emulator (unicorn),
from a dump of the running game, in seconds and without a game run.

Snapshots: `RBR_SNAPSHOT="40 572" harness/play.sh <name>` lists step counters
(`RBRRecompSnapshot.txt`). Before the StepForward call that starts at a listed step (load
settles step 40 at a time: calls start at 0, 40, 80, 120, then every step from 160) and
before the call after it, the plugin writes every committed readable region of the process,
the registers, x87 control word and MXCSR (fxsave) and the stack exactly as the call sees it
to `~/rbr-harness-runs/<name>.snap-<step>.bin` (~200 MB, ~60 ms each); a listed step is dumped
again right after its call (`<name>.snap-<step>-after.bin`). The step copies uninitialised
stack into the car, so the plugin keeps its esp and the stack below it as without snapshots
(the hook's frame size matters too: check its disassembly after changing it). Runs with and
without snapshots stay identical to ref1 over the whole drive (count-run mask).

```sh
python3 -m venv .venv && .venv/bin/pip install unicorn numpy
.venv/bin/python -I harness/replay.py snap-572.bin --check snap-572-after.bin <name>.bin
```

The replay maps the dump, points fs at the TEB, restores the registers, control word, MXCSR
and xmm registers, and calls StepForward like the hook does (thiscall, the substeps argument,
a sentinel return address). Calls into Windows/Wine (modules under `\windows\`) are stubbed
at the import boundary with stand-ins that behave the same in every run (counting timers, a
bump heap); a call without a stub stops the replay and names it. Two things the emulator
computes differently from the CPU the references ran on (cachy, i9-10980HK) are replaced:
rsqrtss/rcpss (hardware approximations; `replay-approx.bin`, made by `replay-approx.c`, holds
the CPU's 2048-entry tables) and the x87 transcendentals and fscale (the emulator goes
through host doubles: 53 of 64 bits; at 24-bit precision it rounds fscale). Their results come
from the CPU itself: `replay-fpu.c` runs each new operand set on cachy over ssh, cached in
`build/replay-fpu-cache.bin`. `replay-fpu.py` measures the emulator's x87 against the CPU
(arithmetic and fsqrt match at 24 and 64-bit precision; transcendentals don't).

`--check` compares the replay (run A, all original code) with the real call: the car + wheel
frame of the recording, and every byte run A wrote against the after-dump. Bytes that depend
on the stubs (timer values in the profiler, uninitialised locals that hold whatever the real
Windows calls left on the stack) are found by two more runs with other timer values and stack
garbage, and left out; so is the stack below the call (the after-dump's own frames). Run A is
bit-exact at calls 40 -> 80 (a 40-substep settle), 572 and 1295.

Run B patches functions in: `rust_step.py` (below) runs the Rust ports there. Run B with our C++
functions (ADDRs through `transplant.py`, or `--transplant FILE`) needs the rbr-recomp repository's
build and transplant.py; its use and how to read its divergence report are documented there.

Snapshots of the reference drive, one-time setup:

```sh
python3 -m venv .venv && .venv/bin/pip install unicorn numpy
mkdir -p /tmp/snapH && for f in snapH.bin snapH.snap-{40,572,1295}.bin; do
  ssh cachy "zstd -1 -q -c ~/rbr-harness-runs/$f" | zstd -d -q -f -o /tmp/snapH/$f; done
```

snapH holds the reference drive's snapshots. Use 40 for the 40-substep load settle (call 40 -> 80), and 572 and
1295 for single steps while driving.

## Rust ports in the step replay (`rust_step.py`)

`rust_step.py` refuses to run when `rust/target/release/librbr_hybrid` is older than the Rust sources (rebuild with
`cargo build --release -p rbr-hybrid`; `RBR_HYBRID` names another library and skips the check). It replays a step as `replay.py` does, with the functions `rust/rbr-hybrid` ports run in Rust: the
library hooks their entries in the harness's unicorn engine (its C API, handed over by `rust_step.py`), runs the
port on the emulator's memory, and returns to the caller with the port's eax, x87 result and popped arguments. Host
calls back into the original run as nested emulator runs. Python only sees logged writes and failures, so a hybrid
step runs as fast as the original's (about 170 steps/s on the Mac). It compares every byte with the original's run and,
with `--recording`, the car and wheels with the drive's frame. Design: `lead/reports/rust-integration-design.md`.

```sh
(cd rust && cargo build --release -p rbr-hybrid)
.venv/bin/python -I -B harness/rust_step.py /tmp/rbr-snaps/vcu-cov.snap-10000.bin --recording drive-tanner-corolla.bin
```

With `--steps`, run A (the original) is cached: it depends only on the snapshot, the recording's controls, the step
count, the bound entries (its call log feeds the call fill), `replay.py`, run A's code in `rust_step.py`, the exe and
the unicorn version, and the file under `RBR_STEP_CACHE` (default `cache/run-a/` next to the worktree, here
`/Users/thomas/worktrees/rbr-recomp/cache/run-a/`) is named by a hash of all of them. Per step it holds run A's memory
changes, its call log, the x87 results it used (checked against the oracle when loaded) and the accesses that can
decide a read window (about 13,000 of up to 380,000 a step): about 1.5 MB per 30-step chain. A cached chain runs only
the two run Bs; the output is the same with or without it (`--no-cache` runs run A). The directory can be deleted at
any time. `long_run.py` does not cache (its segments are thousands of steps). The all-bindings check (9 snapshots, 30
steps) on a loaded Mac (2026-10-15): 180-280 s before the cache (one run A for the call log, one for the windows),
155-225 s with a cold cache (a single run A logs both: about the same) and 60-95 s warm.

Both run Bs (the free stack filled with 0x00 and with 0x7f after each binding, `FILLS`) are needed on every run, even
a clean one. The second fill does more than label a byte fill-dependent. On snap-300, B(0x7f) differs from run A at up
to 362 bytes after a step and B(0x00) at 197-225, a subset of them. So B(0x7f) produces candidates of its own,
and its writes open run A's read windows. Every clean chain of the 9 snapshots reports about 300 "fill-dependent,
never read" bytes. Running the second fill only where B(0x00) differs would change that output, and it could miss a
residue failure that only shows under 0x7f. B(0x7f) also can't be modelled as B(0x00) plus taint on the filled
bytes. Its extra bytes come from copies (0x43c5c0), from other instructions (0x5e4aa5, 0x53af9d) and from bindings
(0x533490), and the hooks can't see a binding's reads, which go through the bridge.

`live.py` steps the same hybrid live: one step per control payload on stdin, one FRM1 frame on stdout, for the
viewer's live mode (`rust/rbr-render/README.md`, Live mode). `worklist.py` lists the physics functions a hybrid step
still runs in the emulator and what each waits for (`lead/reports/integration-worklist.md`).

```sh
.venv/bin/python -I -B harness/worklist.py /tmp/rbr-snaps/vcu-cov.snap-*.bin --out worklist.md
```

`long_run.py` is the main check of the Rust step (`lead/reports/pure-step.md`): `rust_step.py --steps` with every
binding over a drive's whole snapshot chain, each segment from its snapshot to the next one (the last to the end of
the recording), with rust_step's rules. A segment keeps going after a divergence and reports the first one (run B vs
run A, run A vs the recording); a fault in run B ends the segment, and the steps before it are still classified. The
report ends with the worklist: the physics functions run B still runs in the emulator over the chain, ranked by
entries (the headline count). A segment runs about 2.5 steps/s on an idle Mac, mostly Python hook callbacks. `-j 2`
runs two segments at once, niced. tanner-corolla's last segment (steps 11,000-35,404) alone takes about 2.7 hours idle,
and took 4.7 hours under the usual load.

```sh
.venv/bin/python -I -B harness/long_run.py --drive tanner-corolla -j 2 --out /tmp/long-run.md
.venv/bin/python -I -B harness/long_run.py --drive tanner-corolla --from 2000 --to 2400   # one segment
```

`step_reached.py` counts the functions the original step reaches and their callers, over chains from a drive's
snapshots, for the pure-step coverage (`scripts/progress.py step DRIVE REACHED.txt`, then `progress.py report` or
`steplist DRIVE`).

`subtree_tape.py` records a subtree's life in the original for a pure-Rust run (`lead/reports/pure-rust-milestone.md`):
the four struts' `Step` and `ResetOutputs` calls with their inputs and outputs, and the struts' bytes after every
step, over a chain like `rust_step.py --steps`. `rbr-physics/tests/pure_struts.rs` plays its tape with nothing else.

```sh
.venv/bin/python -I -B harness/subtree_tape.py /tmp/rbr-snaps/vcu-cov.snap-300.bin \
    rust/rbr-physics/tests/vectors/struts-snap-300.txt --recording drive-tanner-corolla.bin --steps 200
```

Load-time bindings (`NTweak::CBase::Parse`) run from `RBR_SNAPAT` entry dumps the same way (`--only 52f0c0`).
Two scripts feed the Rust load path's tests (`lead/reports/rust-load-path.md`): `lsp_tokens.py` runs the game's
tokenizer over every retail `.lsp` for `rbr-formats/tests/lsp_tokens.rs`, and `load_refs.py` cuts object bytes out of
a load-time dump as committed vectors (`rbr-physics/tests/vectors/load-spring-def.txt`). `load_wheel_refs.py` cuts the
car's four wheel definitions, its twelve wheel setups, the drive setup, the seven tyre types and the drive options out
of a snap-0 dump, with the names their name words point to, and `load_car_refs.py` the car's definition and the 15
blocks its engine definition allocates (both for `rbr-physics/tests/load_defs.rs`). `load_instance_refs.py` cuts
the car instance (the car, its wheels and its hinged, mounted and car bodies at their factories' returns, with the
factories' heap blocks from the run's allocation log) out of `RBR_SNAPAT` dumps of 0x4eb3e0, 0x53d1c0, 0x51f8e0,
0x521d20 and 0x4ebb00 (list `4eb3e0 1` / `53d1c0 4` / `51f8e0 4` / `521d20 3` / `4ebb00 1`, with `RBR_ALLOCLOG=1`). `load_stage_refs.py` cuts the stage world (the collision system and its `.col` image, the brake
wall, the cell grid's cells, the water meshes and their link blocks, the E44 material table and the `.mat` image, the
physics state's entity lists, the drive line, and the `.trk` collision meshes with their collision grids; the images
and the drive line's unused entries as page digests) for `rbr-physics/tests/load_stage.rs`.

`RBRRecompSnapAt.txt` lines normally hold `<address> <count>`. A line may add
`<arg0-offset> <min> <max>` (hex words) to dump only calls whose first pointed argument has a 32-bit value in that
inclusive range. For example, `5267d0 13 38 21 31` captures up to 13 placement notifications whose object type at
`CSigStuff+0x38` is between `0x21` and `0x31`, skipping unrelated stage placements.

```sh
RBR_DATA=... .venv/bin/python -I -B harness/lsp_tokens.py SNAPSHOT /tmp/lsp-tokens.txt
.venv/bin/python -I -B harness/load_refs.py lp-snap0b.snap-0.bin OUT 0xbc19600:0x11c '*0xbc19604'
.venv/bin/python -I -B harness/load_wheel_refs.py lp-snap0b.snap-0.bin rust/rbr-physics/tests/vectors/load-wheel-defs.txt
.venv/bin/python -I -B harness/load_car_refs.py lp-snap0b.snap-0.bin rust/rbr-physics/tests/vectors/load-car-def.txt
.venv/bin/python -I -B harness/load_instance_refs.py lcb1-bodies/snapat lcb1-bodies.allocs.bin \
    rust/rbr-physics/tests/vectors/load-car-instance.txt
.venv/bin/python -I -B harness/load_stage_refs.py h1-tanner-corolla.snap-0.bin \
    rust/rbr-physics/tests/vectors/load-stage-world.txt $RBR_DATA/Maps/track-54_E.col $RBR_DATA/Maps/track-54_E.mat
```

## Function vectors from a snapshot (`vcu_vectors.py`)

For a function that the capture can't record (not a shadow-eligible leaf), `vcu_vectors.py` runs
one step of the original from a snapshot (the same emulator and oracle as replay.py) and records
every call of the listed functions: the stack arguments, the bytes of the windows they point at
before and after, and the return value (eax, or st0 for x87 returns). Each record names the
branch the asm took (a classifier per function in `SPECS`), so the output says which branches the
step covers. A Rust test replays the records (`rust/rbr-physics/tests/vcu_vectors.rs`).

```sh
RBR_REMOTE=<your remote dir> .venv/bin/python -I harness/vcu_vectors.py snapH.snap-572.bin out.txt
RBR_VCU_VECTORS=out.txt cargo test --manifest-path rust/Cargo.toml -p rbr-physics --test vcu_vectors -- --nocapture
```

`RBR_REMOTE` must name a build directory on cachy (replay.py's x87 oracle compiles there, see
above); the default `rbr-recomp-test-replay` does not exist on every checkout. On cachy itself, set
`RBR_REMOTE_HOST=` (empty) and the oracle runs there without ssh. Only the functions
in `SPECS` are recorded: each entry gives the function's name, the bytes its stdcall pops, the
windows (argument index and length as a function of the arguments, or the pointer stored at
that argument plus an offset), and the branch classifier. An entry marked `external` (a drive
accessor) is recorded only inside a listed caller, as its `inner` record: the arguments and
return value, in call order.

`record_entities_b4.py` runs the same recorder with the B4 set (`entities_b4`) under `--set entities`, which the load-time dumps need (`vector_specs/entities_b4.py`).

The reference drive runs with the launch control and the VCU off, so the gear logic
(`GearboxUpdate`) never runs on it. `--vcu-on` turns both on at each of its calls, and `--fuzz SEED`
also draws its inputs at random (pedals, gear and request, timer, lockout, speeds, thresholds), so
the recorded calls cover the branches the drive does not reach. The inputs go back when the call
returns, after its windows are taken, so the rest of the step runs on the drive's own values. The
recorded values are the original's behaviour on the inputs the record shows, not a reading of the
real drive.

Some calls are not made by the car in a step, so their dumps are started by hand from the step's own
scratch: `--packets N` (ReadPacket, random packets), `--copy N` (CopyDef, random setups), `--basic N`
(StepBasic, random inputs from `--fuzz SEED`) and `--slip N` (QuerySlip, random gates, gears, rpm limit and
speed). FlagDispatch's QuerySlip call is recorded with `--packets`. Their records start at the function's entry. A call made
inside Step or StepBasic runs on the step's own values, so the fuzz does not touch it. `Step` records hold
their callees' setter calls in call order, so the port's sends can be compared with them.

`harness/direct_calls.py SNAPSHOT OUT.txt --set SET [--addr ADDR ...] [--this HEX] [--count N] [--seed S]` runs N calls of
each listed function (default: every function of the set with a `direct_fuzz` recipe) from a snapshot, each from its
entry, and writes them in the same format. A `direct_fuzz` recipe builds the call's object and inputs in the replay's
stub heap; a spec without one takes its stack arguments from `direct_args` and its inputs from `fuzz`, on the object
`--this` names. Use it for a function no step reaches with the values it needs.

## Probes (`probes/`)

CRT-free DirectInput test programs for input problems, built with clang against
Wine's headers and import libraries on cachy:

```sh
clang -target i686-w64-mingw32 -O1 -ffreestanding -fno-builtin -fno-stack-protector \
  -mno-stack-arg-probe -nostdlib -I/usr/include/wine/msvcrt -I/usr/include/wine/windows \
  axes.c -fuse-ld=lld -Wl,-e,_start@0 -Wl,--subsystem,console \
  -L/usr/lib/wine/i386-windows -ldinput8 -ldxguid -luser32 -lkernel32 -o axes.exe
```

Run them with the `rbr-vanilla` environment (`WINEPREFIX`, `PROTONPATH`, `GAMEID`,
`umu-run`). `axes` lists every controller's axes, flags and ranges and samples their
values for 10 s; `ffb` plays a constant force left then right on the first
force-feedback device (it moves the wheel).
