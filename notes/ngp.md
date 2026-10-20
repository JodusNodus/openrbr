# NGP 7.4.778

NGP (Next Generation Physics, WorkerBee) is a plugin that patches the 1.02 SSE exe in memory: the 1.02 physics plus
retargeted calls, repointed constants, code edits and per-car values in its `.data`. ReRBR targets 7.4.778
(`PhysicsRevision_NGP 778`; RSF's 7.5.779 has the same `physics.lsp` but for the revision). The author cannot share
source: behaviour comes from the binary (`ghidra/ngp`, `out/decomp-ngp/NGP.dll.c`) and recordings. History and
detail: rbr-recomp `notes/ngp*.md`.

No NGP.dll code runs in ReRBR. `rbr_physics::Variant::{Stock102, Ngp778}` is fixed at stage load; the 1.02 ports
take it where NGP patches them, NGP's functions are ports in `crates/rbr-physics/src/ngp/`, and NGP's `.data` is mapped
at the reference base `0x791a0000` (addresses here are file addresses, base `0x10000000`). Generated from the DLL or
a snapshot: exe edits `ngp/exe_patches.rs` (`scripts/gen-ngp-exe-patches.py`, `ngp-load11c.snap-40`), stage init
`ngp/stage_materials.rs` (`gen-ngp-stage-materials.py`), tyre types `ngp/tyre_types.rs` (`gen-ngp-tyre-types.py`),
key map `rbr_formats::lsp_keys_ngp` (`gen-ngp-lsp-keys.py` from [data/ngp-lsp-keys.md](../data/ngp-lsp-keys.md)).
Cars: NGP's `physics.rbz` (`--ngp-physics`) and RSF packs (`--rsf-car`, `rbr_formats::rsf`).

The launcher is NGP only (user decision). 1.02 physics stays for development and checks (`rbr-standalone` without
`--ngp-physics`, the viewer's `--live`); Tanner and the 1.02 vectors remain the exactness proof.

## Package and local data

- `original/ngp/7.4.778/`: the package (`NGP.dll` sha256 `95b6dbf5...edaaa4`, `physics.rbz` `831cf121...0c4b27`).
  Its `physics.rbz` differs from retail only in `physics.lsp` (stock cars); NGP's own cars are in
  `ngp-data/physics.rbz`. NGP refuses non-NGP cars (reverse and first gear only).
- `original/ngp/cars-rsf/`: the 42 RSF NGP cars, one slot folder each (`common.lsp`, surface and `r_*` files,
  `setups/`); RSF installs one into a slot of `physics.rbz`, the model stays the slot's.
- `original/ngp/analysis/patches-runtime.tsv`: NGP's edits, diffed from a running game (`scripts/ngp-dump.py`,
  rbr-recomp's `scripts/ngp-memdiff.py`). Physics: 65 calls/jmps into NGP (45 targets), 56 repointed floats, 43 other
  edits.

## How NGP changes the exe

- Repointed constants are fixed (NGP `.rdata`, `0x1014fbxx`) or state (`.data`, `0x10171xxx`-`0x101767xx`: per-car
  init values, per-step atmosphere). A 1.02 port under NGP reads the constant where the patched operand points.
- `.rdata`: `CBase::Parse` replaced in 8 `NTweak` vtables (key parsers); no physics vtable slot changes
  (`StepForward`'s `0x73f994` is untouched, the harness hook coexists).
- NGP reaches the original through `0x101109d0` (exe VA -> pristine code). Its trampolines and emit wrappers run the
  original they stand for (`ngp::original_for`); hybrid hosts take call targets from the call site
  (`ngp::call_target`), so bindings follow NGP's retargets.
- `g_7c755c` (collision system): all 21 operands reading it point at a heap variable holding NGP's own copy (list
  built at `0x1008c6d7`). The boot places three copies at `0x01a8f228` (`ngp::stage::collision_copies`).

## Shape in the code

A runtime variant, not a second backend: 142 of 157 1.02 bindings run unchanged in an NGP step. NGP state (per-car
`.data`, atmosphere, collision copy, idle table, speed ring) sits beside the car. NGP ports are SSE (each op rounds to
its type); exe ports keep the x87 model. Ports keep their 1.02 vectors and add NGP ones
(`crates/rbr-physics/tests/ngp_*.rs`). The hybrid hooks NGP functions at the snapshot's NGP base (`rust_step.py`
finds it from the `CCar::CDef` key dict, `Options.ngp_base`; `rbr-hybrid/src/ngp.rs`).

## Stepping

- NGP's stage init sets `MaxTimeStep` (`CDef+0x1608c`) to 1/720 s (`0x3ab60b61`); `StepForward` runs ~720 times a
  second. The load budget immediate at `0x482bc0` is 150 -> 1500: 38 settle frames of 40 (to step 1520).
- Collision: window `20 * dt` (`0x527447`) and mask `counter & 0x13` (`0x527508`: substeps 0, 4, 8, 12 of 32).
  Ported, not exercised (`SUB_STEPPING` off on every snapshot).

## x87 precision

Race steps run on NGP's thread at Windows' default control word `0x27f` (53 bits); vanilla runs at `0x7f` (24 bits,
D3D9). The load's settle runs on the game thread at 24 bits (`ngp-cw11`). `rbr_physics::fpu` holds the precision as
runtime state (`set_pc53`); the pure host sets it from the app state, the emulator from the snapshot
(`RBR_REPLAY_FCW` overrides). At 53 bits an x87 chain rounds once at its store and st0 returns stay doubles, so such
ports take a 53-bit path (`fpu::round_x87`). Files still rounding to 24 bits unconditionally are fixed as runs reach
them.

## Atmosphere

`0x1004b5a0` (`ngp::atmosphere`), every engine step, from h = car z (`+0x148`), f32 with `pow` in double:
T = 293.15 - 0.0065 h (`10171adc`), p = 101300 (1 - 0.0065 h / 288.15)^5.255 (`10171ad8`), rho = p / (287 T)
(`10171ae0`), `101767c8..d0` = 1/p, 1.15 p, 0.9871668 p. Weather does not enter. p feeds `DamperSolve` (wrapper
`0x10060230`) and the engine's air constants; T rebiases `DamperStep1` (`0x1004d2f0`). Drag factor `1017147c` = 0.5
rho / 1.20407. Checked on Tanner (z 684.1458: p 93349.91, T 288.7030).

## Controls under NGP

- NGP's singleton (`0x10177c88`) handles `CSigCarControlFiltered` (`0x1008e8a0`): filtered channels unquantized, flags
  mapped, a gear select only when it differs from the drive's gear (`+0x570`, inferred), then the car's control input
  `CCar::UpdateSeed` `0x4e91f0` directly and its own walk of the other listeners. Inline when byte `+0xd` is 0, else
  on an asio strand (untested). NGP's `NotifyFilt` emits only in replay mode, so replays stay quantized. Rust:
  `CarControl::ngp`; live payload is f32 device input (`rbr_recording::input`).
- IGNITION is level-triggered (`0x4cc3e5` -> `0x10006e50`), payload word 7 -> `CDrive+0x50`. The starter
  (`ngp::starter`) cranks while it is held 0.5-2.5 s. NGP times that by `QueryPerformanceCounter` (`0x10111ce0`); the
  port uses simulation time, so it differs whenever the game runs off real time. Reference drives must not press
  ignition (the test install starts idling, ~105 rad/s).
- Start hold: while car `+0x1670` > 0 the input is a fixed control (handbrake 1), ~1360 steps after step 1520.
- Clutch help (`LaunchClutch` `0x5375b0`): NGP's per-car engine speeds (`10171ae4`/`10172338`, 89.0/99.5 rad/s for the
  Celica; stock 200/275) make it an anti-stall at idle. VCU constants: `vcu::layout::Tuning` via `ngp::vcu_tuning`
  (rev cut margin `10171ab4`, 25 rpm for 300).
- Direct gear actions (R, N, 1-6): flag 4 with word 6 = 0 R, 1 N, gear + 1 (`CDrive::M6840`; at or above the gear
  count gives N). With a gear bound and neutral not, releasing every gear selects N.

## Load path

All in `rbr-standalone --boot --ngp-physics`:

1. Files: `*_NGP` keys go through the stock parser (keys below); `ntweak::plan`/`apply` take the variant. NGP's
   load edits: `CEngine::CDef` builds `+0x2820` as a `CRealPair` (`0x50f247`, adds `+0x2830` `TorqueCut_NGP`) and
   NOPs two stores (`scripts/gen-engine-def.py`, `NGP_WORDS`); `BrakeWearLimit_NGP` (`+0x58c`) uses NGP's
   `CUnsigned` reader `0x1003c7f0` (the value, then up to 30 unnamed floats into `+0x59c`); `CTyre::CDef` NOPs
   `0x530dd3`, `0x530ea2`, `0x530ecc` (`+0xdc`, `+0xfc`, `+0x10c` stay 0); `CBrakesPage::vf48` returns at once.
   Test: `ngp_definitions_match_the_game` (`crates/rbr-physics/tests/load_defs.rs`, `RBR_NGP_PHYSICS`, `RBR_NGP_LOAD`).
2. Per-car init `ngp::car_init`: `0x1003f5b0` (from listener `0x100f06b0` as the race starts) and callees, plus the
   tyre-definition copy `0x100f1690`. It range-checks setups, converts `Tq_xx_RPM_NGP` to rad/s, maps
   `EngineType_NGP` 11 to 1, scales wheel and tyre values by the car factor `r` and fills `.data` (below). Constants
   need the DLL's exact bits (`9.1e-7`, `0.06615`). Check: `harness/ngp_init_diff.py IMAGE ngp-data/load40`.
3. Heap objects: idle table `0x101767e8` (from the `Tq` points; without it the engine's idle terms are skipped) and
   speed ring `0x101767b0` (`std::deque<float>`, 107 zeros).
4. Exe edits and stage init `0x1004e360` (`ngp::stage::stage_init`).

The damage push `0x4f13a6` becomes 11000 x `r` (`ngp::damage_tuning`). Not ported: the car CRC check, an
anti-tamper, not a load result: `car_init` CRCs the loaded car values (`0x100e88e0` mode 1, `0x10041810`) against
`CarId` (car def `+0x44d0`) and on a mismatch sets the drive def's gear count `+0x654` to 3; the frame listener
`0x100f1780` then calls `abort()` (`0x100ef4b0`) in gear `>= +0x654` above `40 + rand() % 20` km/h, and one call in 300
re-CRCs (`0x100eece0`, mode 2) and aborts on a change. Also not ported: listener unlinks.

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

All 421 NGP files parse with `lsp::parse` unchanged: 153 new `*_NGP` keys (`lsp_keys::NGP_KEYS`), 15 new materials
`Bendable_01..15_NGP` (ids 201-215, `NGP_SURFACES`); test `all_ngp_lsp_keys_resolve_or_are_ngp`
(`crates/rbr-formats/tests/lsp_keys.rs`, `RBR_NGP`). Every key, member and replaced stock key:
[data/ngp-lsp-keys.md](../data/ngp-lsp-keys.md).

- Renames (`0x10060db0`): 41 stock records get NGP names (`Gear0OptionsMin` -> `CenterDiffMapLockPctMin_NGP`).
- Extended tables: for 8 classes NGP copies the stock table into `.data`, renames and appends records (stock format
  `{name, 0, member / 4, 0}`) and points the class dict (`+0xc` table, `+8` count) at the copy.
- Shared members: `DriveTrain_NGP` = `DriveType`, `EngineType_NGP` = `Turbo`, `Tq_xx_NGP` = `VExx`.
- `SubTics` (-> `reserved_NGP`) and `vecDriveAxleJointPos_VF` (-> `vecPosOffset_NGP`) are renamed but NGP's files
  still use the old names, so the parser skips them.
- `Features_NGP` (`r_*`): drivetrain type + diff types x 4096 (centre, front, rear) + feature x 65536 (0 2/3-way
  dampers, 1 4-way, 2 progressive bump stops); the def's packed word `+0x640` (`ngp::diff`).

## Replacements

Of the 65 retargeted physics calls, 18 are emit wrappers (1.02's fan-out; listeners move to NGP's thread when
`+0xd` is set) that run as their 1.02 emits. The rest:

| NGP target | Replaces | Rust |
| --- | --- | --- |
| `1004b8e0` (jmp `0x4e6657`) | `Sim8E20` | `ngp::engine` (calls `ngp::atmosphere`) |
| `100456c0` | `CDrive::M6C90` | `ngp::starter` |
| `10046880` | `E39_Hook4160` | `ngp::curve_term` |
| `10047300` | `CDrive::M93A0` | `drive_worker::clutch_torque` (variant, `ngp::sign`) |
| `10048d50` | `CDrive::M9BC0` (wrapper) | `ngp::diff`, `ngp::downforce`, `ngp::axle` |
| `100458c0`, `10045b80`, `10045fa0` | `M5FE0`, `M6260`, `M7D60` | `ngp::drive` |
| `10015fc0`, `10016d40`, `10016960` | `SampleBig`, `BlendB`, `LerpAt` | `ngp::drive_line` |
| `10060230`, `1004d2f0`, `1004d3c0`, `1004d380`, `1004d410` | `DamperSolve`, `DamperStep1/2` | `ngp::damper` |
| `1005b4a0` | `ComputeOutputs` | `ngp::outputs` |
| `1005be40`, `1005bc50` | `Accumulate`, `HeatStep` | `ngp::inclination` |
| `1005b660`, `10059f70` | `CScratch::Step`, `CSpringDamper::Step` | `ngp::step_wrapper`, `ngp::spring` |
| `1005e8a0` | `E51Wheel::M3D230` | `ngp::tag_blend` |
| `1004df40` | `E44Entry8::Pick` | `ngp::material` |
| `10042120` | `CCar::FfbNorm` | `ngp::ffb::curve` |

Exe ports that take the variant (1.02 -> NGP): `StepForward`; `CScratch::StepInner` (0.99/0.96/0.87/0.7 ->
0.95/0.875/0.775/0.65); `CScratch::Step` (0.1 -> 0.025, `+0x1e8` -> `+0x1f8`); `ComputeOutputs` (3 -> 1);
`HeatStep` (60 -> 40); `RelaxStep`; `M3DD30` (load thresholds -> `10172f58/5c/60`); `M3D520` (`ProbeTuning`);
`CWheel::Unk96e0` (`WheelTuning`); `CSolid::E43Vf6C` (1e-5 -> 4e-7); `DragAndSpinDamp` (0.5 -> `1017147c`);
`E36DamageOps::Slot1` (`DamageTuning`); the engine stage (`EngineTuning`); `CSpringDamper::Step`.

## Force feedback

1.02 sends one constant force from the steered wheels' torque:

1. Per step: `CCar::StepEngine` `0x4e9930` clears car `+0xadc`; `E51Wheel::M3DD30` (`0x53e780`) adds each steered
   wheel's torque about axis `+0x9a0` times lever factors `+0x9c0`, `+0x9c4` (from `0x53b900`; names inferred).
   `CCar::UpdateFfbTorque` `0x4e7ff0` filters it into `+0xaec` with `f = 0.5 + 0.2 clamp((speed - 7) / 6, 0, 1)` and
   `SteeringRackRatio`; 0 during the start hold.
2. `CCar::FfbNorm` `0x4e76c0`: 0 if body `+0x1ec` is set, else `clamp(+0xaec / MaxFFTorque, -1, 1)` (def `+0x44a0`).
3. Per frame: `EmitForce` `0x491930` -> `E28_GetFFBVal` `0x4c5f90` (x `Strength`, negated when `Direction` is 0) ->
   `m = trunc(-10000 x)` when the gate holds (FFB on, racing, `IsDone(0)`) -> `SetFFBValue` `0x4c6b40` (clamp
   +-10000, one `GUID_ConstantForce` effect, `cAxes = 1`, which Wine rejects; the harness patches it). Rumble
   (`HandleDamage`, `HandleNoise`) never reaches DirectInput.

NGP keeps the torque and gate and changes the rest:

- Curve `0x10042120`: `sign(T) (1 - 0.975^(|T| 100 / S))`, `pow` in double with base `0.975f`; 0 with the body flag.
- `S` = `ForceFeedbackSensitivity_NGP` (renamed `MaxFFTorque`). At load the ini's
  `ForceFeedbackSensitivity<Surface>_Car<slot>`, else `ForceFeedbackSensitivity<Surface>` (surface from
  `SetupSpec`), if > 0 and not 100, scales `S` by value/100; floor 1. Higher `S` is weaker.
- Output: EmitForce stores `x`; an FFB thread (`ffbRate`, default 1000 Hz, 0 off) resends `m = floor(-10000 x)` via
  NGP's `SetFFBValue` `0x10064620` (adds `DIEP_DURATION`; `SendFFB2` is idle while the thread runs) with a dead
  zone (`m = 0` when `|steering| < ffbDeadZone` and `|m| < ffbDeadZoneThreshold * 10000`).

Rust: `car_body::steering_ffb::force(mem, car, variant)` (these 1.02 functions are steering FFB, not downforce),
`rbr_gameplay::ffb::{magnitude, dead_zone}`, `HUD2` record (force at 16, bit 0 of flags at 20 = NGP),
`rbr_render::ffb::Wheel` (SDL3 haptics, infinite constant effect, thread at `rate` Hz, level `m * 32767 / 10000`;
sdl3-sys 0.7 truncates the GAIN/AUTOCENTER bits, so they are checked as `1 << 16`/`1 << 17`). Launcher `[ffb]`: enabled,
strength, inverse, dead_zone, dead_zone_threshold, rate, device, and sensitivity_gravel/_tarmac/_snow
(`rbr-standalone --ffb-sensitivity G,T,S`, applied by `car_init::ffb_sensitivity` as NGP's init does at `0x1003f5b0`:
`(percent / 100f) * S` in f32; the per-slot `_Car<slot>` keys are not offered). Checked on a G923 under Linux.

## H-shifter

`crates/rbr-render/src/shifter.rs` (opt-in, [docs/playing.md](../docs/playing.md)) follows RBRControls 5.3.7.0's
advanced mode (`Plugins/RBRControls.dll`, MIT, no public source; `FUN_1002f200`):

1. Clutch in = clutch above the threshold. Selected gear = first held button (1-6, then R); dogleg drops button 1
   and maps 2-6 to 1st-5th. No button with auto-neutral: N at once.
2. Ignore a selection equal to the engaged gear (unless in N) or above the gear count.
3. Clutch in: engage. Else it fails when engine speed is outside the window or, upshifting, throttle > 0.5:

   | | upshift | downshift |
   | --- | --- | --- |
   | synchro | 0.90-1.05 x target | 0.98-1.25 x target |
   | dog | 0.60-1.60 x target | 0.80-1.80 x target |

4. Synchro engages after more than 4 matched ticks (a mismatch resets and grinds); dog has a 10-tick grace window,
   then grinds. A pending shift retries every tick.
5. Grind: looping `Shifter_Error.wav`, pitch `0.65 + 1.2 tanh((|rpm error| - 200) * 0.000667)` from 200 rpm.

RBRControls derives the target from parsed gear ratios and a calibrated speed factor; ours takes gear ratio
(`CDef +0x694 + 0x50 g`) times drive speed (state `+0x384` x `CDef +0xa54`) from the GBX1 record
(`rbr_recording::gearbox`), and assumes 60 ticks a second. The launcher's auto gearbox and dogleg come from
RBRControls' `CarInfo.ini` in the RSF folder (else synchro, no dogleg), the grind from its `Shifter_Error.wav` (or
`.wav.sample`; silent without one). The mode turns Clutch Help off.

## Test install and recordings

A separate NGP 7.4.778 install on the test machine (a Windows or Wine machine with the original game and NGP;
harness plugin installed), `scripts/ngp-run.sh NAME [SECS]`
(`LIVE=1`, `SNAPSHOT=`, `REPLAY=`). Slots (`scripts/ngp-install-car.py`): 2 BMW M3 E30 (RWD), 4 Fiesta Rally4
(FWD), 5 Fiesta Rally3 (AWD), 6 Celica ST185 (AWD, centre diff type 5). Profile `pfMULLIGATAWNY` (`ClutchHelp 1`).
Runs are kept in `~/rbr-ngp-runs` on that machine.

- Harness NGP mode (`HookCarControl`): with NGP loaded the recorder and playback hook `0x4e91f0` for the player's car,
  taking only calls from NGP.dll. Recordings stop past 2 GiB (~133k steps); `harness/matrix.py --ngp` pairs take 130k.
- Oracle: `harness/replay.py` runs a whole NGP step bit-exact from a snapshot (UCRT emulated;
  `PostQueuedCompletionStatus`, CRT locks stubbed); long runs use a noise mask. NGP drives are deterministic in body
  state.
- `rbr-standalone --snapshot`/`--live` run NGP snapshots fully ported (launcher drives `ngp-rpl11/20/32/41`,
  `ngp-fwd41b`, `ngp-rwd41`).

### Replays

A `.rpl` does not run physics on playback. Map `0x1c`, car slot `0x20`, entries `0x44`, steps `0x48`; from `0x4c`
12-byte entries (u16 steps, u8 checksum, 9-byte quantized control: b4-b7 throttle, brake, handbrake, clutch / 255, b3
steering * 2/255, b0 flags, b8 gear, b1 bit 0 word 7); an `HGMD` block, the setup as `.lsp` text, body path chunks
(one 32-byte pose per 5 steps under NGP). `scripts/ngp-rpl-playback.py` is exact in 1.02, approximate under NGP
(quantized input); reference drives need live recordings.

## Sound

NGP patches ~120 1.02 sound sites; `rbr-sound` ports the audible ones (per-car words in NSD1): engine low rpm
`10171ad4`, whine caps `10171acc`/`10171ad0`, tyres' second surface, road list `10171280`, skid threshold,
`E30DXSound +0x74` as `SetMaxDistance`. NGP records and ghosts go under `NGP/`.

## Open items

- Asio strand path (`+0xd` set); other wall-clock reads; whether Windows gives the same control word as Wine.
- `.data` not read: `10171aa8`/`aac`, `10171468`/`146c` (per-car x, source unknown), `10178138..`, `10171a94`.
- Bound offsets past the last stock key (`CDrive`, `CEngine`, `CTyre`, `CWheel` defs): whether NGP enlarges them.
- Tyre copies differ from the boot's in words the init does not read.
- Checked against the asm only: `Sim8890`/`SimE970`/`SimECF0` NGP paths, the step wrapper's surface scale.
- `ClutchHelp 0` untested; a real NGP reference drive per surface class is still needed.
- FFB at runtime (both variants' `SetFFBValue` arguments, `S` with an ini override, Wine, Windows); H-shifter tick
  unit and the real shifter.
