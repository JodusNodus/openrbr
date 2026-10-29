# NGP 7.4.778

NGP (Next Generation Physics, WorkerBee) patches the 1.02 SSE exe in memory: 1.02 physics plus retargeted calls,
repointed constants, code edits and per-car values in its `.data`. ReRBR targets 7.4.778 (`PhysicsRevision_NGP`
778; RSF's 7.5.779 has the same `physics.lsp` but for the revision). The author cannot share source: behaviour comes
from the binary (`ghidra/ngp`, `out/decomp-ngp/NGP.dll.c`) and recordings. History and detail: rbr-recomp
`notes/ngp*.md`.

No NGP.dll code runs in ReRBR. The x87 precision is the thread's (`rbr_physics::fpu`, below); ports of
functions NGP patches have NGP's behaviour only, NGP's functions are ports in `crates/rbr-physics/src/ngp/`, and
NGP's `.data` is mapped at reference base `0x791a0000` (addresses here are file addresses, base `0x10000000`). The
boot reads immutable data from the player's NGP 7.4.778 DLL (`rbr_formats::ngp::Tables`, SHA-256 checked;
`RBR_NGP_DLL` overrides the install's `Plugins/NGP.dll`; the launcher's saved `ngp_dll` choice follows that path
while empty and blocks Start when missing or unsupported). No plugin instructions run; the standalone maps no exe
`.text` and applies no executable patches. Named callees select the NGP bindings, typed boot settings
(`ngp::layout::runtime::Runtime`) the collision copies, damage gate and head-gasket coefficients
(`rbr-hybrid/src/ngp/`). drive2 attempt 5 is physics PASS 84084/84084 with the DLL inputs and without code bytes.
Cars: NGP's `physics.rbz` (`--ngp-physics`) and RSF packs (`--rsf-car`, `rbr_formats::rsf`).

ReRBR steps NGP physics only (user decision): `rbr-standalone --boot` needs `--ngp-physics` and refuses a 1.02
snapshot; the launcher and the viewer's `--live` go through it. The exactness proof is `harness/rsf_compare.py
$RBR_RECORDINGS/rsf/drive2 --attempt 5 --sections physics` (84084/84084). The 1.02 behaviour arms are gone from
`rbr-physics`/`rbr-hybrid`; 1.02 direct-call vectors remain only for functions NGP shares unchanged.

## Package and local data

- `original/ngp/7.4.778/`: the package (`NGP.dll` sha256 `95b6dbf5...edaaa4`, `physics.rbz` `831cf121...0c4b27`).
  Its `physics.rbz` differs from retail only in `physics.lsp` (stock cars); NGP's own cars are in
  `ngp-data/physics.rbz`. NGP refuses non-NGP cars (reverse and first gear only).
- `original/ngp/cars-rsf/`: the 42 RSF NGP cars, one slot folder each (`common.lsp`, surface and `r_*` files,
  `setups/`); RSF installs one into a slot of `physics.rbz`, the model stays the slot's.
- `original/ngp/analysis/patches-runtime.tsv`: NGP's edits, diffed from a running game (`scripts/ngp-dump.py`,
  rbr-recomp's `scripts/ngp-memdiff.py`). Physics: 65 calls/jmps into NGP (45 targets), 56 repointed floats, 43
  other edits.

## Adding an NGP version

Only NGP 7.4.778 is supported: `rbr_formats::ngp::SUPPORTED` holds its `NGP.dll` SHA-256, and any other DLL is
refused (`validate`). RSF's 7.5.779 and later releases are checked, then added, like this:

1. `ngp-compat CANDIDATE.dll [--reference 7.4.778's NGP.dll] [--rbz candidate physics.rbz]`
   (`rbr_formats::ngp::compat`: reads found by content, tables and decoded code compared; "reads unchanged" does
   not prove the physics is the same, so step 2 decides).
2. Drives made with that version: record with the harness (as drive1 and drive2 were made with 779), then
   `harness/rsf_compare.py ... --sections physics` and the gate matrix ([verification.md](verification.md)) must all
   be equal. 779 (RSF's `RBR_NGP_v4.1`, `NGP.dll` 7.5.779.508, sha256 `cf351cdf...01ec`) compares as *reads
   unchanged*: all 25 ranges found (moved up to 0x14140), every table equal, `physics.rbz` identical but for the
   revision line, ~8 % of `.text` new or changed. Known equal only on drive2 attempt 5 (snow, Ford Focus RS WRC 06,
   RSF car 55) against 778's tables; more drives (other cars, surfaces, damage) are needed to add it.
3. Add the hash and version to `SUPPORTED` with the evidence (the compare's verdict, the drives); the launcher then
   accepts the DLL.

## How NGP changes the exe

- Repointed constants are fixed (NGP `.rdata`, `0x1014fbxx`) or state (`.data`, `0x10171xxx`-`0x101767xx`: per-car
  init values, per-step atmosphere). Rust adapters read those data words directly.
- `.rdata`: `CBase::Parse` replaced in 8 `NTweak` vtables (key parsers); no physics vtable slot changes
  (`StepForward`'s `0x73f994` untouched, the harness hook coexists).
- NGP reaches the original through `0x101109d0` (exe VA -> pristine code). Its trampolines and emit wrappers run
  the original they stand for (`ngp::original_for`); hosts select named callees (`ngp::Call`) and relocate the NGP
  addresses to the snapshot's base.
- `g_7c755c` (collision system): all 21 operands reading it point at a heap variable holding NGP's own copy (list
  built at `0x1008c6d7`). The boot constructs three copies (`ngp::layout::stage::collision_copies`) and supplies
  their addresses directly in its runtime settings; `stage::layout::CollisionQuery` decodes operands only for
  snapshots.

## Shape in the code

One behaviour (NGP's): patched ports hold only the NGP arm with NGP state required; unchanged ports keep
their 1.02 vectors at `Precision::Pc24`; NGP ports are SSE, exe ports x87 (`tests/ngp_*.rs`). NGP-only bindings
decline on 1.02 snapshots (`ngp::only()`; vcu-cov.snap-2000 and vcu-arctic-snow.snap-4000 identical with those
declined). NGP base from the `CCar::CDef` key dict (`Options.ngp_base`, `rust_step.py`; `rbr-hybrid/src/ngp.rs`).

## Stepping

- `MaxTimeStep` 1/720 s (`0x3ab60b61`, `ngp::layout::stage::stage_init`); load budget 150 -> 1500 at `0x482bc0`
  (`rbr_gameplay::stage::NGP_LOAD_SUBSTEPS`), 38 settle frames of 40 (to step 1520).
- The stage init writes one of two tables of 2740 words (`ngp::Tables::stage`): the first when `CDef+0x1607c`
  (`PhysicsRevision_NGP`) is 778, the second otherwise. RSF's 7.5.779 with its 779 drives as the first does, so ours
  takes it for 778 and 779 (`ngp::layout::stage`, `REVISIONS`). Wrong pairing (779 rbz, 778's rule) settles
  apart from step 40 with half standing-start traction; matched, rsf-input1 (Fiesta Rally3, Falstone, 2026-10-25)
  and drive2/2 hold STA1 every step.
- Collision: window `20 * dt` (`0x527447`, `physics_core::step`) and mask `counter & 0x13` (`0x527508`:
  substeps 0, 4, 8, 12 of 32). Ported, not exercised (`SUB_STEPPING` off on every snapshot).

## x87 precision

Race steps run on NGP's thread at Windows' default control word `0x27f` (53 bits); vanilla runs at `0x7f` (24 bits,
D3D9). The load's settle runs on the game thread at 24 bits (`ngp-cw11`). `rbr_physics::fpu` holds the precision as
thread-local state (`fpu::set_precision`, scoped `Precision::scope` / `with_precision`; PC=53 until set); the pure
host sets it from the app state, the emulator from the snapshot (`RBR_REPLAY_FCW` overrides; a 1.02 snapshot runs
at PC=24). At 53 bits an x87 chain rounds once at its store and st0 returns stay doubles, so such ports take a
53-bit path (`fpu::round_x87`). Files still rounding to 24 bits unconditionally are fixed as runs reach them.

## Atmosphere

`0x1004b5a0` (`ngp::atmosphere`), every engine step, from h = car z (`+0x148`), f32 with `pow` in double: T =
293.15 - 0.0065 h (`10171adc`), p = 101300 (1 - 0.0065 h / 288.15)^5.255 (`10171ad8`), rho = p / (287 T)
(`10171ae0`), `101767c8..d0` = 1/p, 1.15 p, 0.9871668 p. Weather does not enter. Read by the engine and heat
models. Drag factor `1017147c` = 0.5 rho / 1.20407. Checked on Tanner (z 684.1458: p 93349.91, T 288.7030).

## Controls under NGP

Filtered input to the car (`rbr_gameplay::control`: `CarControl::ngp`, `ngp_direct_gears`, `live::poll`;
`rbr_recording::input`): NGP's singleton (`0x10177c88`) handles `CSigCarControlFiltered` (`0x1008e8a0`) with
unquantized channels, mapped flags, a gear select only when it differs from the drive's gear, then
`CCar::UpdateSeed` `0x4e91f0` and its own listener walk. Inline when byte `+0xd` is 0, else on an asio strand
(untested). NGP's `NotifyFilt` emits only in replay mode, so replays stay quantized.

- IGNITION is level-triggered (> 0.99, payload word 7 -> `CDrive+0x50`); the starter (`ngp::starter`) cranks
  while held 0.5-2.5 s, timed by `QueryPerformanceCounter` in the game and simulation time in the port (they
  differ off real time). Reference
  drives must not press ignition (the test install starts idling, ~105 rad/s).
- Start hold: while car `+0x1670` > 0 the input is a fixed control (handbrake 1), ~1360 steps after step 1520.
- Clutch help (`LaunchClutch` `0x5375b0`): per-car engine speeds (`10171ae4`/`10172338`, 89.0/99.5 rad/s for the
  Celica; stock 200/275) make it an anti-stall at idle; rev cut margin `10171ab4` (25 rpm for 300).
- Input: NGP does not patch the filter (0x48c500); under RSF the grid is the built-in one or the profile's (see
  `notes/gameplay.md`, Input filter). With `threaded` the poll runs on NGP's input thread at `inputRate` (1000 Hz,
  `0x1003c650`); ours on its input thread at the bindings' rate.
- Unconfirmed without an NGP live recording: the direct gear action value (the binding's +0x30 raw value vs the
  viewer's pad value), the gear in use as the last sensor data's rather than the current step's, the threaded
  poll's timing, and the profile grid's effect on the payload.

## Load path

All in `rbr-standalone --boot --ngp-physics`:

1. Files: `*_NGP` keys go through the stock parser (keys below); `ntweak::plan`/`apply` are NGP's. NGP's load
   edits (`engine_driveline::engine_def::NGP_WORDS`, `ntweak::plan`, `tyre::layout::construct`,
   `CBrakesPage::vf48` in `load`: `scripts/gen-engine-def.py`): `CEngine::CDef` gains `+0x2830` (`TorqueCut_NGP`,
   +0x2820 a `CRealPair`, `0x50f247`) and two zeroed defaults; `BrakeWearLimit_NGP` (`+0x58c`) uses NGP's `CUnsigned` reader
   (value plus up to 30 unnamed floats into `+0x59c`); `CTyre::CDef` leaves `+0xdc`, `+0xfc`, `+0x10c` at 0;
   `CBrakesPage::vf48` returns at once. Test: `ngp_definitions_match_the_game`
   (`crates/rbr-physics/tests/load_defs.rs`, `RBR_NGP_PHYSICS`, `RBR_NGP_LOAD`).
2. Per-car init and `.data` fill (`ngp::layout::car_init`: `0x1003f5b0`, listener `0x100f06b0`, tyre copy
   `0x100f1690`; heap idle table `0x101767e8` and speed ring `0x101767b0`; damage push `0x4f13a6` to 11000 x `r`;
   maps `EngineType_NGP` 11 to 1). Check: `harness/ngp_init_diff.py IMAGE ngp-data/load40`.
3. Not ported: the car CRC check (anti-tamper, not a load result): CRCs the loaded values (`0x100e88e0` mode 1,
   `0x10041810`) against `CarId` (car def `+0x44d0`), on a mismatch sets the drive def's gear count `+0x654` to 3;
   the frame listener `0x100f1780` then calls `abort()` (`0x100ef4b0`) in gear `>= +0x654` above `40 + rand() % 20`
   km/h, and one call in 300 re-CRCs (`0x100eece0`, mode 2) and aborts on a change.

| Per-car `.data` words | From (`base` = 0.5712 / `InverseMass`) |
| --- | --- |
| `1017130c`, `10171310`, `10171314` | base x 0.625, 1, 5 (`r` x 500, 800, 4000: damage, FFB rumble) |
| `10172ca8`, `10172f58/5c/64`, `10172f60` | base x 33.75, 35, 75, 1000; 1 / (base x 40) |
| `10171aa0`, `10171aa4`, `10171acc`, `10171ad0` | `MaxTorque_NGP` (integer) x 1, 3, 1.3, 13 |
| `1017234c` | `BrakeCoefficient_NGP` / 100 |
| `10171a90` | axle split: 0.5, or type 7 split % (`+0xe48`) / 100, max 1 |
| `10171470/74`, `10171478` | convection (`+0xe58`/`+0xe68`, else 6), `RelaxStep` rate (`+0xe78`, else 800) |
| `10171300`, `10171304`, `10171308` | wheel probe fill and pose (`ProbeTuning`, `WheelTuning`; 1.02 0.4, 0.15, 0.2) |
| `10176934` + `0x314` per wheel | `HubOffset_NGP`, `TreadWidth_NGP`, `CarcassWidth_NGP`, `SideWallHeight_NGP` |

Boot check: `ngp-hf-11.bin` (stage 11, slot 5), settle 38/38 and all 1,962 driving steps of 2,000 equal. Recordings
made before the harness hook fix (`cov-*`, `ngp-load11c`, `ngp-s0-11`, `ngp-cw11`) miss `UpdateMrr`'s `UpdateSeed`
call (car step count, drive words `+0x48/4c/7c/80`, `car+0x864` start at 0).

## NGP .lsp keys

All 421 NGP files parse with `lsp::parse` unchanged: 153 new `*_NGP` keys, 15 new materials
`Bendable_01..15_NGP` (ids 201-215, read from the DLL); test `all_ngp_lsp_keys_resolve_or_are_ngp`
(`crates/rbr-formats/tests/lsp_keys.rs`, `RBR_NGP`). Every key, member and replaced stock key:
[data/ngp-lsp-keys.md](../data/ngp-lsp-keys.md).

- Renames (`0x10060db0`): 41 stock records get NGP names (`Gear0OptionsMin` -> `CenterDiffMapLockPctMin_NGP`).
- Extended tables: for 8 classes NGP copies the stock table into `.data`, renames and appends records (stock
  format `{name, 0, member / 4, 0}`) and points the class dict (`+0xc` table, `+8` count) at the copy.
- Shared members: `DriveTrain_NGP` = `DriveType`, `EngineType_NGP` = `Turbo`, `Tq_xx_NGP` = `VExx`.
- `SubTics` (-> `reserved_NGP`) and `vecDriveAxleJointPos_VF` (-> `vecPosOffset_NGP`) are renamed but NGP's files
  still use the old names, so the parser skips them.
- `Features_NGP` (`r_*`): drivetrain type + diff types x 4096 (centre, front, rear) + feature x 65536 (0 2/3-way
  dampers, 1 4-way, 2 progressive bump stops); the def's packed word `+0x640` (`ngp::diff`).

## Replacements

Of the 65 retargeted physics calls, 18 are emit wrappers (1.02's fan-out) that run as their 1.02 emits. Each NGP
target's binding in `rbr-hybrid/src/ngp.rs` names the NGP address, the function it replaces and the Rust port
(`crates/rbr-physics/src/ngp/`); the exe ports' NGP-changed constants are in each port's docs, and the engine
direct calls in `crates/rbr-physics/tests/ngp_engine.rs` (`scripts/ngp-engine-vectors.py`).

### Engine wrapper

Death and idle-table lookup (`1004b8e0`, jmp `0x4e6657`): `rbr-hybrid/src/ngp.rs` (`engine_wrapper`,
`engine_dies`), `ngp::engine`. Every car
has 10-15 idle knots (42 RSF cars, `physics.rbz` 13-15); 0 and 1 come only from patched snapshots.

### Unreachable guards

The bindings in `rbr-hybrid/src/ngp.rs` keep these as declines:

- another idle-table class: the table's only writer (engine init `0x1004ae53`) always builds vtable `10148930`.
- speed-ring growth: the ring is written by its static init (`0x100019b2`, map 32, 107 entries), the zeroing
  `0x1004b3b1` and the step's pop-then-push (`0x1004bb2e`); growth needs a map of 27 or fewer.
- settings or game object not made (`0x10013cd0`, `0x10011cb0`): order-of-construction guards (made on
  every NGP snapshot, load step 0 too). NGP itself does not null-check them (`0x100472be`, `0x10059fa5`,
  `0x10059fb8` would fault).
- axle blocks/packs out of range, a wheel's pack past the axles: packs `[1,1,2,2]` and pairs `[[1,2],[0,1],[2,3]]`
  are `CDrive::CDef`'s constructor defaults. No stock or NGP key sets them, and every snapshot has them.
- curve state not a wheel's: the only caller (`0x50a2a8`, `Step1E0`'s wheel loop) passes wheel states 0-3.

## Force feedback

One constant force from the steered wheels' torque, per step then per frame (`car_body::steering_ffb`,
`rbr_gameplay::ffb`): `StepEngine` 0x4e9930 clears `+0xadc`; `M3DD30` 0x53e780 adds each steered wheel's torque
about `+0x9a0` times `+0x9c0`/`+0x9c4` (`inv_arm`, `inv_rack` from 0x53b900; names inferred). Norm (`FfbNorm`
0x4e76c0). Per frame `EmitForce` 0x491930 -> `E28_GetFFBVal` 0x4c5f90 -> `m = trunc(-10000 x)` -> `SetFFBValue` 0x4c6b40. Rumble
(`HandleDamage`, `HandleNoise`) never reaches DirectInput.

NGP keeps the torque and gate and changes the curve and output (`ngp::ffb::curve`, NGP's `0x10042120`; `sensitivity`,
`rbr_gameplay::ffb::{magnitude, dead_zone, racing_gate}`, `rbr_render::ffb::Wheel`; launcher `[ffb]`,
`rbr-standalone --ffb-sensitivity G,T,S`, `--trace-out` `.ext` vs `harness/rsf_compare.py`):

- The racing gate's exe detail, past `0x5403e0` case 0xb (`0x5407b2`, called from 778 `0x1003c3eb`): it holds when `[[0x8951d0] + 4]` is set
  (not traced), or in app state 1 with FFB on (`0x1660ed4`) and `0x5fe390(0)` on `[0x167ea70]` (slot 0 not running,
  `+0x24 != 2`, or past its end `+0x4`; not shown to be the start timer). Ours gates on app state 1 alone, where
  the drives open it (cachy wheel trace, autopilot: 0 through load, countdown, after finish).
- Between attempts NGP sends the previous attempt's last stored force: the gate opens about 9 ms (8 sets) before
  the first driving StepForward (the last step's EmitForce ran in drive2/4, not in drive2/5), then the first step's
  own; 0 before (drive2 attempts 2-6). Ours sends the step's own (one 1.4 ms record the wheel thread did not
  reliably send on cachy; carrying it took a flag and a record field). `rsf_compare` classes it "carried (not
  copied, by decision)" against the recorded car's last two magnitudes (`ffb.last`).
- After a hitch the stored force stays until the burst's last step (drive2/4: 0.4 s, ~300 steps); ours sends each
  step's own. EFF1's QPC is taken after the call returns (e2e2 step 9466, 16 ms: sends the `x` of step 826 = ours
  at 9456); sessions from the plugin of 2026-10-21 on hold the call's QPC (`qpc_call`).
- Steering force travels in FFB1 (`rbr_recording::ffb`), not HUD2.

NGP 7.5.779's device calls (RSS1 EFF1 of drive1/2; ours in `rbr_render::ffb::Wheel`): the exe's `CreateFFB`
(`DIEFF_CARTESIAN | DIEFF_OBJECTIDS`, axes `0x1000002`, `0x1000102`, gain 10000, infinite); NGP's thread only
`SetParameters(0x20000141)` (`DIEFF_CARTESIAN | DIEFF_OBJECTOFFSETS`, two axes, {1, 0}), about every 1.15 ms
(Sleep(1) under Wine, ~810/s), the same value again 78 % of the time, no smoothing; no `DIPROP_AUTOCENTER` /
`DIPROP_FFGAIN`, no SendForceFeedbackCommand. Wine: `SetParameters` -> `Start(1)` -> `Download` (force report on a
change, update report always via `DIEP_DURATION`, then start); cartesian {1, 0} -> 9000 polar -> evdev direction
0x4000; acquire sends `DISFFC_RESET` and the gain, autocentre ON sends nothing; SDL bus `FF_GAIN` 0xffff,
`FF_AUTOCENTER` 0, per report one EVIOCSFF and one play (two EVIOCSFF on a change: 8523 calls of drive2
attempt 1).

Setup (RSF on cachy: Proton cachyos-11.0 SLR, G923 PS on new-lg4ff `hid-logitech-new`, sysfs `autocenter` 0,
`gain` 65535, `spring_level`/`damper_level` 30): ours matches RSF's levels in order over the same 10 s of drive2
attempt 1 (5779 of 5815; ~1.08 ms sampling); not replicated: duplicate EVIOCSFF on a change, second `FF_GAIN` at
acquire. Unattended: `FFB=1` binds the profile, `FAST=1` paces real time (playback default 8 steps per requested
step). End to end (2026-10-21, det-audi66 playback): all 34981 RSF driving sets reached the wheel at our carried
magnitude's level, the 23068 steps both sent matched; step end to wheel p50/p90/p99 0.59/1.06/1.22 ms RSF vs
0.58/1.04/1.22 ms ours (file, not live loop; Wine QPC ±0.1 ms).

`rotation` (ours, `rbr_render::wheel_rotation`): lock-to-lock degrees to the wheel sysfs `range`, launcher on
change and viewer at live start: the setting or the car's lock up to it. RSF sets it per car from
`cars_data.json` `steering_wheel` through vendor tools (`RSFControlConfig.exe`), outside the game (sysfs `range`
540 before and after drive2). Car lock = `MaxSteeringLock x 720` degrees (`wheel_rotation::car_degrees`); RSF's
`steering_wheel` matched for every NGP car in `cars-rsf` (Yaris Rally2 479.52 -> RSF 479).

### RSF settings

What an RSF drive's force feedback, wheel and aids take (`rbr_formats::rsf::settings`, CLI `rsf-settings`;
`harness/rsf_compare.py`; the launcher imports none of them). Keys, addresses, defaults and precedence are in the
module docs; drive observations only:

- Sensitivity: drive2 runs 280/250/280, S 110 x 250 % = 275 maps the recorded torque exactly. The driven cars'
  per-car keys are empty; how a per-car value would reach NGP is unconfirmed.
- Surface is the stage's, not the setup's name: drive2/1 is stage 176, tarmac.
- Unused FFB keys `[NGP]` `adaptiveFFB`, `ffbBufferCycles`, `ffbMinFPS`: dead in 778 and 779 (a key builder and
  accessor each at 779 `0x10072fa0`, `0x10073490`, `0x10073dd0` that nothing calls, jumps to or holds the address
  of; settings init 779 `0x1009c1c0` reads the others).
- Profile: on, 1.0, normal, autocenter off (not in the session). Rotation per car (`NGPCarMenu.ini` `[Default]`
  `AdjustSteeringRotation` (1), `[car<id>]` `steeringrotation`, else `cars_data.json` `steering_wheel`) not seen
  applied (sysfs `range` 540 before and after drive2).
- Aids: drive1/drive2 all off, the profile agrees; drive2/2 45276/45486 equal with them.
- Head gasket: RSF 10, test install 1 (RSF's toggle: on 10, off 1); scale and bias in
  `ngp::layout::runtime::head_gasket`.
- No reader found: `[Settings]` `ConstantForceMultiplier` (-60) and `ForceFeedback`. Input only: the profile's
  input filter and controller options, `input.ini` `Invert*Axis`, the `.acm`, `RBRControls.ini`,
  `NGPCarMenu.ini` `SplitCombinedThrottleBrakeAxis`, `DeadzoneByTimeMSecGearShift`.

## H-shifter

`crates/rbr-render/src/shifter.rs` (opt-in; model, thresholds and grind pitch in its docs), per-car gearbox by car
name (`[gearbox]`, `rbr-launcher/src/settings.rs`), grind sample `Shifter_Error.wav`
(`rbr-launcher/src/command.rs`). After real gearboxes and iRacing's two-step model (EdRacing's gear shift guide);
tolerances and rates are tuned, not measured. No RBRControls `CarInfo.ini` lookup (user decision): gvrc's car list
and packs carry no gearbox type.

## Test install and recordings

A separate NGP 7.4.778 install on the test machine (a Windows or Wine machine with the original game and NGP;
harness plugin installed), `scripts/ngp-run.sh NAME [SECS]` (`LIVE=1`, `SNAPSHOT=`, `REPLAY=`). Slots
(`scripts/ngp-install-car.py`): 2 BMW M3 E30 (RWD), 4 Fiesta Rally4 (FWD), 5 Fiesta Rally3 (AWD), 6 Celica ST185
(AWD, centre diff type 5). Profile `pfMULLIGATAWNY` (`ClutchHelp 1`). Runs are kept in `~/rbr-ngp-runs` on that
machine.

- Harness NGP mode (`HookCarControl`): with NGP loaded the recorder and playback hook `0x4e91f0` for the player's
  car, taking only calls from NGP.dll (`harness/matrix.py`: recordings stop past 2 GiB, pairs take 130k).
- Oracle: `harness/replay.py` runs a whole NGP step bit-exact from a snapshot (UCRT emulated, CRT calls stubbed;
  noise mask on long runs). NGP drives are deterministic in body state.
- `rbr-standalone --snapshot`/`--live` run NGP snapshots fully ported (launcher drives `ngp-rpl11/20/32/41`,
  `ngp-fwd41b`, `ngp-rwd41`).

### Replays

A `.rpl` does not run physics on playback (`scripts/ngp-rpl-playback.py`: entries, counts, quantized control,
NGP approximation). Map `0x1c`, car slot `0x20`; an `HGMD` block, the setup as `.lsp` text, body path chunks (one
32-byte pose per 5 steps under NGP); reference drives need live recordings.

## Sound

NGP patches ~120 1.02 sound sites; `rbr-sound` ports the audible ones (per-car words, road list, skid threshold,
`E30DXSound +0x74` as `SetMaxDistance`). NGP records and ghosts go under `NGP/`.

## Open items

- Asio strand path (`+0xd` set); other wall-clock reads; whether Windows gives the same control word as Wine.
- `.data` not read: `10171aa8`/`aac`, `10171468`/`146c` (per-car x, source unknown), `10178138..`, `10171a94`.
- Bound offsets past the last stock key (`CDrive`, `CEngine`, `CTyre`, `CWheel` defs): whether NGP enlarges them.
- Tyre copies differ from the boot's in words the init does not read.
- Checked against the asm only: the step wrapper's surface scale.
- Supplemental all-binding steps match for `ngp-rpl11.snap-6438` and `ngp-fwd41b.snap-10880`, but the harness
  reports unverified modelled x87 results; equality alone does not verify those results.
- `ClutchHelp 0`: both callers skip `LaunchClutch` when the scratch's launch mode (`+0x4c`, ClutchHelp with the
  launch flag off) is 0 (`0x537f5f`, `0x5380a4`). A real NGP reference drive per surface class is still needed.
- The engine's death on the inline emit path (`+0xd` clear): run A stops at a listener's file I/O, so it is
  unchecked.
- FFB at runtime: NGP 7.5.779 under Wine is checked by the mapping from the recorded torque (RSF drive2, ini
  percentages); 1.02, 7.4.778 and Windows are not. The H-shifter on a real shifter.
