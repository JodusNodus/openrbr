# NGP (Next Generation Physics) 7.4.778: first exploration

Static look at `RBR_NGP-7.4.778.507` (2024-09-15, by WorkerBee). Nothing from the package was run. The goal it
serves: a later Rust physics that behaves like NGP instead of plain 1.02 (`notes/rust-plan.md`).
Reproduce the tables with `python3 -I scripts/ngp-scan.py PATH/NGP.dll`.

## Package

| File | What it is |
| --- | --- |
| `Plugins/NGP.dll` | The plugin. PE32 DLL, MSVC 2015+ (v140 CRT), boost 1.85, 1.3 MB code, not packed. PDB path `D:\develop\projects\rbr\build\x86\Release\NGP.pdb`. Exports `RBR_CreatePlugin` (RBR plugin) and `CreateProxy` (`sdk/rbr.plugin.IPhysicsNGProxy.h`). |
| `physics.rbz` | Same as retail's except `Physics/physics.lsp` (80 KB -> 197 KB): `PhysicsRevision_NGP 778`, reworked material/surface tables, `ForceFeedbackSensitivity_NGP`, `Bendable_01..15_NGP` and `Metal_Barrier` objects; the per-car `AntiRollBarStiffness*`, `Gear*Upshift/Downshift`, `EffWheelRadius`, `MaxFFTorque` entries are gone. The stock car files in it are byte-identical to retail. |
| `Plugins/NGP/sdk/` | Proxy, service (repair/damage buffer, 0x3EB0 bytes) and UDP telemetry structs. |
| `Plugins/NGP/bin/` | FMOD and telemetry daemons, `splitlbs.exe`, `DlsCheck.exe`. Out of scope. |
| `ReadMe.NGP.txt` | Feature list and history. |

NGP refuses non-NGP cars ("all other cars are limited to reverse and first gear"). NGP car physics uses keys the DLL
reads (`EngineType_NGP`, `EngineMass_NGP`, `TyrePerformancePct{T,G,S}_NGP`, `BumpStop*_NGP`,
`*DiffMapLockPct{Min,Max}_NGP`, `*DiffPreloadTorque`, `TyreHeatFlowFtr_NGP`, ...). These cars ship separately (RSF
car packs), not in this package, so we have no NGP car data yet.

Physics features per the readme: fixed 720 Hz physics rate (7.0), new engine model with arbitrary torque curves,
Salisbury and viscous-coupling diffs, RWD/FWD/torque-split/centre-spool drive types, progressive bump stops, 4-way
damping, aero downforce, barometric formula, engine/tyre/brake temperature models, 2D material maps (1.02 uses only the
first row of the 16x16 map), setup validation, starter logic, multi-threaded physics/input/FFB.

## How it changes the exe

NGP is a layer on the 1.02 code, not a separate engine. It works on the SSE exe in memory in three ways:

1. **Byte patches** (719 sites). A patch object `(exe address, new bytes, length)` is built through the helper at
   `0x100e8b70`, applied with `VirtualProtect`. 97 are in physics (E33-E51, 30 files); most of the rest are in sound
   (E23/E24), E31 and data.
2. **Vtable slot swaps** (210 calls to `0x10110c30`: `old = vt[i]; vt[i] = new`). The named vtables it references are
   UI/menu pages, `NTweak::CDict/CReal/CUnsigned/CInteger/CVector` (LSP parsing), `CCarSetupPageBase`, and the
   repair/damage classes (`CCarRepairHandler`, `CWheelDamage`, `CTyreDamage`, ...).
3. **Calls back into the original** (1,330 sites; 234 in physics). `static auto f = resolve(<exe address>)` through
   `0x101109d0`. NGP still runs the 1.02 tyre (`CScratch::Step`, `HeatStep`), spring-damper (`CSpringDamper::Step`),
   engine (`Sim8E20`), drive-line and wheel code and wraps or extends it.

The 97 physics patches, by the instruction they change:

| Change | Count | Examples |
| --- | --- | --- |
| `call` target redirected to NGP code | 42 | E40 `CDrive::M9BC0`/`Step1E0` calls into `M6260`, `M5FE0`, `M6C90`, `4160`; E34 `CCar::Update9AF0` calls; E49 tyre calls to `0x52e350`/`0x52e370`; `CScratch::StepInner` -> `ComputeOutputs` |
| float operand repointed to an NGP constant | ~45 | `CPhysics::StepForward` 0x527447: collision step multiplier 4.0 -> 20.0; E49 tyre and E48 damper constants; E43 engine |
| branch, flag or register changes | ~10 | StepForward 0x527508 `test [ecx+0x18], 3` -> `0x13`; damper `ja` changes; one 30-byte patch in `E37GridOps::EdgePair` (probably the readme's collision-mesh ratio fix) |

Touched physics functions: about 80 (engine E41/E43 the most, then drive E40, tyre E49, wheel E50/E51, damper E48,
`CCar` E34, solid E47, `CPhysics` E46). Exact list: `ngp-scan.py` rows with kind `patch`.

Not checked yet: the new bytes themselves (built at run time in stack buffers, so the call targets have to be read
from the code that fills them), and the vtables that only appear unnamed.

## Runtime patch table (2026-10-16)

The static scan above can't see the bytes NGP writes (built at run time). `scripts/ngp-dump.py` dumps the exe and
NGP.dll images from the running test game (cachy, `/proc/<pid>/mem`); `scripts/ngp-memdiff.py` diffs `.text`/`.rdata`
against the original exe and classifies each changed run (NGP addresses rebased to its 0x10000000 image base, matching
the Ghidra project `ghidra/ngp`, export `out/decomp-ngp/NGP.dll.c`). NGP.dll loaded at `0x791a0000` in every run. Two
dumps (stage loaded, Tanner, Celica) gave the same table, apart from the harness plugin's own hooks, which the script
labels. Copy of the table: `original/ngp/analysis/patches-runtime.tsv`.

| Kind | Physics (E33-E51) | Other |
| --- | --- | --- |
| call/jmp rel32 into NGP code | 65 (about 45 distinct NGP functions) | 92 |
| operand repointed to an NGP float | 56 | 18 |
| other code edits (NOPs, branches, masks, partial rel32) | 43 | 212 |
| `.rdata` | - | 10 |

- `.rdata`: `CBase::Parse` (`0x52f0c0`) replaced in 8 `NTweak` vtables by NGP parsers (its LSP extensions), one
  slot at `0x744ecc` repointed to another exe function (`0x548b20` -> `0x5488e0`), one float at `0x74682b`
  1800 -> 28800. No physics vtable slot changes; `CPhysics::StepForward`'s slot (`0x73f994`) is untouched, so the
  harness hook coexists.
- Constants come in two kinds. In NGP's `.rdata` (`0x1014fbxx`), fixed tuning: StepForward collision multiplier
  4 -> 20, tyre factors 0.99/0.96/0.87/0.7 -> 0.95/0.875/0.775/0.65 (`CScratch::StepInner`), tyre heat 60 -> 40,
  `CScratch::Step` 0.1 -> 0.025, `CSolid::E43Vf6C` 1e-5 -> 4e-7. In NGP's `.data` (`0x10171xxx`-`0x101767xx`),
  values NGP computes per stage: 101300 -> 93349.9 and 100000 -> 92151.9 (both x0.92152, ambient pressure at the
  stage), 293.14 -> 288.70 (temperature), and a group scaled by 0.94195 (27000, 28000, 60000, 800000, 500). So part
  of NGP's physics depends on the stage's altitude/weather (readme: barometric formula); the values above are
  Tanner's.
- The ground/material query object at `0x007c755c` is replaced in all 17 physics uses (`CollideInit` x7, `E51Wheel::M3D520` x4, `CGround`, `CSolid::Step77`,
  `CCar::Sampler54A0`, `CPhysics::Vf10`) by NGP's own object (heap `0x01a8f228` in the dump); probably the 2D material
  maps (guess).
- `CEngine::CDef` and the tyre `CDef` constructors: the default stores are NOPed (NGP sets its own values).

## Stepping

- NGP writes `MaxTimeStep` (`CDef+0x1608c`, the `base` dt of `StepForward`) = 1/720 s (`0x3ab60b61`) at
  `0x1004e3a7`, and the same value at `+0x18b908` of a second object (not identified). `physics.lsp` still says
  0.007. The game's own loop still calls `StepForward`; the harness sees one call per substep, about 720 per second.
- Collision: vanilla runs the contact block on substeps with `counter & 3 == 0` (when `SUB_STEPPING`) with window
  `4 * dt` = 28 ms. NGP: window `20 * dt` = 27.8 ms, mask `counter & 0x13` (patch at `0x527508`), which is true
  for counters 0, 4, 8, 12 of every 32, not every 20th substep. Behaviour to reproduce as is.

## Harness under NGP (idle runs)

`scripts/ngp-run.sh NAME [SECS]` records a live run of the parked car (auto-start Tanner, nobody driving).
Three runs (`cachy:~/rbr-ngp-runs/idle1..3.bin`, ~44,600 steps each, about 720 MB):
- The recorder works unchanged (same car class, vtable `0x738b44`; 40 settle substeps, then one per call).
- Car position, velocity and angular velocity are bit-identical between runs through the whole settle, until the
  car sleeps (~step 4,000).
- 936 words differ from the first step and stay different: 118 heap pointers (allocation shifted by 0x7c8), 362
  lane-3 vector words (the stack-residue class the vanilla gate masks), the rest per-run constants that look like
  uninitialised heap (ARGB greys like `0xff545454` in unused `CCar` fields). Same classes as vanilla, about 5x as
  many. Not a determinism verdict: the car doesn't move. That needs input (IGNITION, then a drive).

## Controls under NGP

How the player's controls reach the car (stock 1.02): `CSigCarControlFiltered` (signal manager `+0x648`) ->
quantizer -> `E16Quant::NotifyFilt` -> `CSigCarControl` (`+0xc90`) `Emit` `0x48cd90` -> the car's connection ->
the car's control input `0x4e91f0` (`CCar::UpdateSeed` in our source; it copies the 0x20-byte payload into the car:
VCU packet, throttle at car `+0x1694`, clutch at `+0x86c`, word 7 into `CDrive+0x50` = car `+0x500`).

NGP (2026-10-16, from the decompilation and runs):
- Live driving bypasses the quantizer: NGP's singleton (`0x10177c88`, ctor `0x1008e1f0`) listens to
  `CSigCarControlFiltered` with `0x1008e8a0`, builds the payload, calls the car's control input `0x4e91f0` directly
  (through `0x10042260`, after direct gear selection) and runs the other `CSigCarControl` listeners with its own walk
  `0x1008e570` (HedgeHog3D, replay manager, car sound, LocoMotion; the car's connection is gone). Delivery is inline
  when the singleton's byte `+0xd` is 0 (as on the test install), else posted through a boost.asio strand
  (`0x1008e020`).
- NGP replaces `NotifyFilt` in its connection (`0x1000f3e0`): it only stores the unpacked values, and calls the
  original (whose `Emit` call NGP redirects to `0x1008e5e0`: gear selection, car input, walk) only in mode 8 (replay).
  So the `Emit` call at `0x48d24e` is never reached while driving.
- IGNITION: NGP patches `USInputManagerImpl::Tic` (call `0x4cc3e5`, `IsActionDown(0xc)`) to `0x10006e50`
  (`GetActionValue(0xc) > k`): level- instead of edge-triggered. The bit still travels as payload word 7 to
  `CDrive+0x50`. NGP's readme: the starter needs IGNITION held for 500 ms with the clutch depressed. The starter is
  NGP's `CDrive::M6C90` replacement `0x100456c0`, and it times itself by the wall clock: while `CDrive+0x50` is 0 it
  stores a `QueryPerformanceCounter` time (`10176718`, in us); with it set the starter cranks only while the time since
  then is 0.5 s to 2.5 s (and `0x10049600` allows it). So a playback started faster or slower than real time (the
  harness fast-forwards) cranks for a different number of steps than the live drive. It doesn't matter while the engine
  starts already running (the test install). The port (`ngp::starter`) counts simulation time instead: the
  hold since the gate was last clear, in us, kept in the stamp's eight bytes, so it cranks for the same steps at any
  playback speed (and differs from NGP by design whenever the game runs off real time). It is the only physics caller of NGP's timer `0x10111ce0`. On the test
  install the car has the engine idling (~105 rad/s, ~1000 rpm, car `+0x654`) from the first step, so playback does
  not need it.
- The start hold: while car `+0x1670` (`m_tOverride`, ~3.9 s from load) is positive the control input uses a fixed
  control (handbrake 1) and ignores the payload; with NGP that is the first ~1.9 s (1360 steps) after step 1520.
- Clutch: stock `CScratchSim::Step` `0x537f50` uses the payload clutch only when `m_launchMode == 0` (VCU `+0x4c`,
  car `+0x89c`), else clutch help (`LaunchClutch` `0x5375b0`): pressed below an engine speed, released above a
  second one, released at the rate `CDef+0x178`, pressed again at once. The test profile has `ClutchHelp 1` (also
  `AutoGears 0`, `GearGuard 1`), so the clutch input is ignored (runs with clutch 1.0 and 0.0 are bit-identical).
  NGP replaces the two engine speeds per car (`const` patches at `0x53761e`/`0x53763c`/`0x537652`): stock 200 and
  275 rad/s, NGP 89.0 and 99.5 rad/s for the Celica (850-950 rpm, just under its ~1000 rpm idle). So NGP's clutch
  help engages at idle and only presses as an anti-stall. Engaging 1st at idle stalls the engine; at half throttle
  it bogs down and stalls (the clutch releases faster than the engine recovers); at full throttle the car pulls
  away (19.5 m/s at the 1st-gear limiter). Measured with `harness/probes/ngp-clutch.peek` (`PEEK=` in
  `scripts/ngp-run.sh`).

Harness NGP mode (`harness/plugin/RBRRecompHarness.cpp`, `HookCarControl`): with `NGP.dll` loaded, the recorder and
playback hook the car's control input `0x4e91f0` (player car only) instead of `Emit`. What it gets is what the
physics gets, so it is what gets recorded and what playback feeds back (past the hook). In playback the other
listeners still get NGP's live controls (sound, driver animation; no physics). No NGP addresses are used. Without NGP
nothing changes. Verified with synthetic playbacks (`scripts/ngp-synth-playback.py`): throttle in neutral revs the engine to
5700 rpm, `launch:1.0` drives off, and two `launch` runs are bit-identical in body state for all 23,278 steps.

Determinism (2026-10-16): the vanilla reference drive's controls retimed to 720 Hz (`scripts/ngp-retime-playback.py`,
85,536 steps = 119 s) played three times (`drive1`-`drive3`): the car drives 155 m (up to 25.8 m/s), crashes at
~20 s and stays stuck. Body state (position, velocity, angular velocity) is bit-identical in all three runs at every
step. Of the 4,040 record words, 988 differ between runs: 928 never change within a run (uninitialised memory set
at load, heap pointers), 43 are the unused fourth lane of SSE vectors, 2 are heap pointers re-allocated at the
same step, and the rest are set at the same step in every run but cleared after a real-time delay (car `+0x658`,
`+0x674`, `+0x824`) or hold a frame time (`+0x814`, 1/60 vs 1/70 s): presentation-side state that varies with the
fast-forward frame rate and doesn't reach the physics. Noise mask: `diverge.py --mask drive1.bin drive2.bin` on
cachy (`~/rbr-ngp-runs/drive1.bin.mask`, 1,017 of 4,040 words: 381 car, 635 wheel, the control's heap word); with
it `diverge.py drive1.bin drive3.bin` is identical over all 85,774 steps (3,023 words compared). The masked body
words are lane 3 and unused fields; position, velocity and angular velocity lanes 0-2 stay compared.
The plugin's `StageStarted` callback never fires under NGP (vanilla logs it); the recorder starts from the first
step as usual, so nothing depends on it yet.

Runs: under NGP the harness step counter runs at 720 per second (the car clock `+0x1168` advances 1/720 s per step),
the load settle is 38 x 40 steps (to 1520), and fast-forward plays 32 s of driving in ~3 s wall time.

## Atmosphere (per step, from the car's height)

NGP's wrapper around the engine simulation (`0x1004b8e0`, before the original `Sim8E20`) calls `0x1004b5a0`
every engine step with the car. From h = car z (`+0x148`, metres), in float32 (the `pow` in double):

- T = 293.15 - 0.0065 h (K, `10171adc`)
- p = 101300 (1 - 0.0065 h / 288.15)^5.255 (Pa, `10171ad8`; note the 288.15 base next to the 293.15 one)
- rho = p / (287 T) (`10171ae0`), and `101767c8` = 1/p, `101767cc` = 1.15 p, `101767d0` = 0.9871668 p.

`10171ad8` is the `DamperSolve` argument NGP substitutes (`0x10060230`) and `10171adc` the `DamperStep1` rebias
(`0x1004d2f0`), so engine and heat models follow the altitude every step.

Checked to the last bit against the parked car on Tanner (z 684.1458: p 93349.91, T 288.7030, rho 1.12663), and on
tracks 10, 22 and 62 the stored p, rho and the three derived words agree with the height implied by T (-27 m, -2 m,
3 m) to float rounding (idle dump runs `st*`, 2026-10-16). Weather does not enter
(Tanner good and bad weather give the same values). So the earlier "per-stage" pressure and temperature (101300 ->
93349.9, 293.14 -> 288.70) are just the Tanner start height, and they change while driving uphill or downhill.
`1017147c`, the drag factor NGP puts into `CCar::DragAndSpinDamp` (`0x4e7e62`, stock 0.5), is 0.5 rho / 1.20407 (the
sea-level density at 293.15 K): 0.467842 on Tanner, 0.5013 on track 10. Other `.data` words that differed between
those runs, not yet explained: `10171a94` (0.74, 0.83 on track 10) and a tyre group at `10176934`..
(-0.041/0.164/0.205/0.131, other values on track 10, which ran snow tyres).

## Emulated NGP step (2026-10-16)

`harness/replay.py` runs a whole NGP physics step from a harness snapshot (`SNAPSHOT=` in `scripts/ngp-run.sh`): NGP.dll is
a module of the snapshot like the exe, and two changes make it run. The Universal CRT (`ucrtbase.dll`,
`vcruntime140.dll`, which NGP links; the barometric `pow`) now runs emulated instead of stubbed, and
`PostQueuedCompletionStatus` is stubbed: NGP's `Emit` shells hand the listener fan-out to its asio thread (5 posts per
step, e.g. the engine-noise emit `0x100ee000`), so the listeners (sound) don't run inside the step. Result on
`snap1.snap-10880` (full-throttle launch, 13 s in): the car and wheels after the step are bit-exact with the recording,
and all 19,040 bytes the step wrote match the real call (296 more depend on the stubbed `QueryPerformanceCounter`, 13
calls per step; the car and wheel frame is exact regardless). Vanilla snapshots replay as before.

So the 1.02 method carries over: NGP snapshots plus the emulator are the oracle for Rust ports of NGP functions
(`rust_step.py` bindings), and NGP recordings with `drive1.bin.mask` for long runs.

## x87 precision (2026-10-16)

Under NGP the race's physics steps run with the x87 control word `0x27f` (53-bit precision); vanilla runs at `0x7f`
(24-bit, as D3D9 sets it at `CreateDevice`). NGP.dll has no `fldcw`: the race's steps run on another thread than the
game's (NGP's), which starts at Windows' default `0x27f`. The load's settle steps (state 0xd, `StepForward(40)`) still
run on the game's thread at `0x7f` (`ngp-cw11` snapshots: steps 40 and 1480 on the game's thread at `0x7f`, steps
1520.. on another at `0x27f`). So NGP's physics runs at 24 bits during the load and at 53 after it.
`rbr_physics::fpu` keeps the precision as runtime state (`set_pc53`, `precision`): the pure host sets it per step from
the application state (24 bits in the load), the emulator bindings from the emulator's control word. The emulator
takes the control word from the snapshot (`RBR_REPLAY_FCW` overrides it), so emulated NGP steps are exact either way.

The 1.02 ports assume 24-bit: every x87 operation rounds to single. At 53 bits a value kept in a register across
several operations keeps more precision, so a port can differ where the exe keeps intermediates in registers. Measured
on `snap1.snap-10880` with the control word set to `0x7f` (the same snapshot otherwise): the wheel refresh `0x53b900`
becomes exact (57 bytes before) and `StepForward` too (3 bytes before). The other 11 differing bindings still differ
at 24 bits by about as much as at 53 (`CScratch::Step` 267 -> 220 bytes, the rest within 5), so those are NGP's
patches. The 140 exact bindings are exact at 53 bits too on this step.

Coverage (2026-10-16): 12 more snapshots, from the first 12 s of four RSF replays as playbacks (stages 11 and 20
gravel, 32 and 41 tarmac; Fiesta Rally3) and full-throttle launches of the Fiesta Rally4 (FWD) and BMW M3 (RWD) on
stage 41 (`cov-*` in `cachy:~/rbr-ngp-runs`; `rwd41-10880` is the step the car hits a wall, so it takes the collision
path). All 24 have control word `0x27f`. Every binding one at a time (`rust_step.py --each`), then each difference
again at `0x7f`:
- 142 of the 157 bindings are exact on all 12 or not reached; 4 are never reached alone (`0x4f3ec0`, `0x50b300`, `0x50ef50`,
  `0x53d0d0`; why is not checked), the collision functions `0x521740`, `0x521b40` only on the wall hit.
- Precision only (exact at 24 bits wherever they differ): the wheel refresh `0x53b900` (11 snapshots), `ToQuat`
  `0x4e4d10` (5), `E51Wheel::M3AFB0` `0x53afb0` (1).
- Both: `StepForward` (exact at 24 bits on 7 of 12, 1-29 bytes left on 5) and `CScratch::Step` (less at 24 bits,
  never exact).
- Patches only (the same at 24 bits): `0x4e6650`, `CCar::Vf64x`, `CDrive::Step1E0`, `Pick`, `CSolid::E43Vf6C`,
  `RelaxCount`, `CWheel::Unk96e0`, `E51Wheel::M3D460`/`M3D520`/`M3DD30`.
So the precision work is local: a few ports that keep intermediates on the x87 stack, found by snapshot.

## Replays (2026-10-16)

A 1.02 replay (`.rpl`) doesn't run the physics when played: in app state 8 `CApplication::M0047f560` only fires the
stored controls (sound, driver animation) and the animated-replay manager moves the car along stored poses. What a
replay holds:
- header: map at `0x1c`, car slot at `0x20`; entry count at `0x44`, step count at `0x48`;
- from `0x4c`, 12-byte entries (`E19ClsScan`): u16 steps, u8 checksum (low byte of CRC-32 over the 12 bytes at replay
  manager `+0x18b8e0`, which `PosFwd` fills; not the car's position at that step, so not usable as a desync check
  yet), the 9-byte quantized control (CLS) of every physics step after the start. `E16Quant::NotifyFilt` unpacks it:
  b4-b7 throttle, brake, handbrake, clutch / 255, b3 steering (signed) * 2/255, b0 flags (1 up, 2 down, 4 select
  with b8 the gear, 0x20 -> 0x10, 0x40 -> 0x20), b1 bit 0 word 7;
- an `HGMD` block (64 entries), the car setup as `.lsp` text (with the driver aids `AutoGears`, `GearGuard`,
  `NeutralLock`, `ClutchHelp`), then the body path: chunks of a u32 count (<= 0x400) and 32-byte records (x, y, z,
  one more float, quaternion), one record per 5 steps under NGP.
RSF's NGPCarMenu writes a sidecar `.ini` per replay (car, physics folder, setup file, tyres, weather, damage, NGP
version, finish time).

`scripts/ngp-rpl-playback.py` turns a replay into a harness playback. In vanilla it is exact: `test2.rpl` (the Tanner
Corolla reference drive's replay) gives the live recording's controls for all 35,245 steps (replay step k = recording
step 161 + k), except word 6, which the live payload fills from the stack when no gear is selected. Under NGP it is not:
NGP's live handler (`0x1008e8a0`) passes the car the unquantized controls of `CSigCarControlFiltered`, and the replay
keeps only the quantized ones. Replaying RSF's `MULLIGATAWNY_rsf_practice_41.rpl` (Fiesta Rally3, Cote d'Arbroz, NGP
7.4.778; `AUTO="41 5 0 0"`, the RSF profile): the parked car is 1.5 mm from the stored start, the path stays within
2.4 cm for the first 2.8 s of driving, 0.66 m at 5.5 s, 9 m at 11 s, and the car leaves the road at ~14 s (450 m of
4.5 km). So RSF replays give realistic input for the first seconds and an approximate path, not reference drives.
References need the unquantized input, recorded live (`LIVE=1` in `scripts/ngp-run.sh`). Not checked: what causes the
1.5 mm at rest (car model folder, RSF's stage files, or the stored pose not being the body position).

The plugin can still play a replay unattended (`REPLAY=` in `scripts/ngp-run.sh`, `RBRRecompReplay.txt`): it calls
`CReplayManager::StreamFetch` `0x4999b0` as the replay menu (`0x57fe30`) does, and starts playback with
`CApplication::M0047ca40` in state 10. Under NGP the replay then ends at once (state 9), not investigated.

## Rust plan for NGP (2026-10-16)

NGP physics = 1.02 physics + NGP's replacement functions (`notes/ngp-replacements.md`) + repointed constants and code
edits in exe functions + per-car and per-step values in NGP's `.data` + 720 Hz stepping. The 1.02 port is the base;
NGP is a set of deltas on it, after milestone B for 1.02 (`notes/rust-plan.md`). The NGP author supports the project
but can't share source, so the behaviour comes from the binary and from recordings.

Oracle: NGP snapshots replayed in the emulator (Emulated NGP step) for single steps and bindings, NGP recordings with a
noise mask for long runs. The hybrid reads all state from the snapshot, NGP's `.data` included, so functions can be
ported and checked before their load-time inputs (per-car values NGP computes from the `*_NGP` keys) are understood.

Measured on `snap1.snap-10880` (`rust_step.py` with `main`'s bindings at `2287cebe`; the same on the older base): 157
bindings run in the NGP step. Bound one at a time, 140 give NGP's step exactly (two with the contact list declining as
on 1.02), 4 aren't reached alone (`SampleBig`, two emits, `0x4f3ec0`: NGP redirects their calls). 13 differ: 10 functions with NGP patch sites (`0x4e6650`, `CDrive::Step1E0`,
`E44MatTable::Pick`, `StepForward` (3 bytes), `CSolid::E43Vf6C`, `CScratch::Step`, `CWheel::Unk96e0`,
`E51Wheel::M3D460`/`M3D520`/`M3DD30`) and 3 whose ports inline a patched callee: `CCar::Vf64x` (inlines
`DragAndSpinDamp`, drag 0.5 -> `1017147c`), `RelaxCount` `0x5358c0` (inlines `RelaxStep`: redirected `DamperStep1`,
100 -> 800) and the wheel refresh `0x53b900`. The wheel refresh and `StepForward` differ only through the x87 precision (see x87
precision); the other 11 through NGP's patches. So the 1.02 port mostly carries over as is; the deltas are where the
patch table points, through the call tree of each port, plus the 53-bit precision.

Shape in the code (proposal, 2026-10-16): NGP is a runtime physics variant, not a second backend. Measured, 142 of
157 1.02 ports run unchanged under NGP, so a swapped-out backend would copy most of the physics, and a cargo feature
would keep one binary from checking both oracles (and players from choosing). Instead:
- `Variant::{Stock102, Ngp778}`, fixed when a stage loads, carried by the world. NGP's own state (its per-car
  `.data` values, the atmosphere, the driveline sample cache) is an `NgpState` beside the car, absent for 1.02.
- NGP's replacement functions live in `rbr_physics::ngp` (SSE code: plain f32/f64, no x87 model). The exe functions
  NGP patches take the variant where the patch table points (a repointed constant, a branch, a call into `ngp`).
- The x87-precision ports (`0x53b900`, `ToQuat`, `M3AFB0`, part of `StepForward`, `CScratch::Step`) get a 53-bit
  path under NGP.
- Loading: `lsp_keys` takes the variant (NGP's renamed and appended keys, `notes/ngp-load-path.md`); the step rate
  (720 Hz under NGP) is the variant's.
- Tests: every port keeps its 1.02 vectors; NGP's add NGP vectors (`tests/ngp_*.rs`), so both stay exact.
First port: the atmosphere, `rbr_physics::ngp::atmosphere`, bit-exact on 2,007 direct calls of `0x1004b5a0`
(`scripts/ngp-atmosphere-vectors.py`, `tests/ngp_atmosphere.rs`). The hybrid binds NGP.dll functions too
(`rbr-hybrid/src/ngp.rs`): they are listed at file addresses and hooked at the snapshot's NGP base, which
`rust_step.py` finds from the `CCar::CDef` key dict (`ngp_base`) and passes in `Options.ngp_base`; on a 1.02 snapshot
they aren't hooked. The atmosphere binding gives the original's step exactly on `snap1.snap-10880`,
`cov-rpl11.snap-9520` and `cov-rwd41.snap-15920`, and a vanilla snapshot runs as before.
Second port: the downforce, `rbr_physics::ngp::downforce` (`0x10047040`, reached from `CDrive::Step1E0` through
NGP's `0x10048d50`): `FrontDownforce_NGP`/`RearDownforce_NGP` are forces at 41.67 m/s (150 km/h) and 1.20407
kg/m^3, scaled by (v / 41.67)^2 and density / 1.20407, halved and turned into lift when reversing, scaled by the two
aero parts' damage (1, 0.5, 0); the force goes into `car+0x378` and a pitch moment from the wheels' `+0x8b4` into
`car+0x310`. It also sets the drag factor `1017147c` (0.5 density / 1.20407). Exact on 3,000 fuzzed direct calls
and in the step on six snapshots.
Damper helpers, `rbr_physics::ngp::damper`: the `DamperSolve` wrapper `0x10060230` (1.02's port with the air
pressure as the limit), NGP's own `DamperStep1` `0x1004d2f0` (SSE, `(a - b') * (c * d)`, b shifted from 293.14 K to
the air temperature), `0x1004d3c0` (the same with the scratch's factor `0x1004d340` as d) and `DamperStep2`
`0x1004d380` (x87 at 53 bits). The last two take the scratch in `esi`, a register the LTCG'd callers pass it in.
Exact on 3,000 fuzzed direct calls at NGP's control word (a NaN's sign not compared) and in the step on six
snapshots.
The `ComputeOutputs` wrapper `0x1005b4a0`, `rbr_physics::ngp::outputs`: NGP also repoints the exe function's cap
scale of outputs 14 and 16 (`0x5321d8`: `0x72b2a0`, 3, to `1014fbc4`, 1), which the first try missed (the bytes
differed on three of four snapshots). A 1.02 port reused under NGP has to take these repointed constants
(`notes/ngp-replacements.md`, "Tuning globals"). Identical in the step on five snapshots; no direct-call vectors, since
the wrapper reaches the exe function through a thread-local lookup the emulator lacks. The exe function itself now takes the variant
(`tyre::outputs::compute_outputs`), since NGP-patched callers other than the wrapper see the same scale; its binding
is identical under NGP on three snapshots.
The engine stage, `engine_driveline::layout::EngineTuning`: NGP repoints `Sim7D30`'s and `Sim80D0`'s air constants
(101300, 1/101300, 116495, 100000, 293.14, 1.2040719) at its atmosphere words, and the load band (10000/20000),
`SimE470`'s rev gate (500/3500/1/4500), `Sim80D0`'s misfire gate and heat (80, 2215000) and `SimECF0`'s stall speed
(40) at per-car words in its `.data`. Its byte patch at `0x517ec6` makes `Sim7D30`'s second band read `+0x78` instead of
`+0x68`. `DamperSolve` in `Sim7D30` and `SimE1F0` goes to the wrapper that solves against the air pressure, and the
second `DamperStep1` in `Sim8890` to NGP's with the air temperature. Most of the earlier engine differences were x87
precision, though: `DamperScale`, `Sqrt4`, `Curve`, `Step1/2`, `BiasSub`, `CurveEta`, `TableLerp`, `SimEC40` and the
rev gate return st0, so they now return the unrounded double under NGP, and `Combine`, `Relax` and `Solve` (its
inlined relax) round their chains once. Identical on the NGP snapshots that reach them; `Sim8890`, `SimE970` and
`SimECF0`'s NGP paths are not reached there.
The drive curves (`TabE00`, `WorkerF60`, `Worker4060`, `Hook4160`, `engine_driveline::drive_curve`) are x87 chains
whose counters truncate st0, and `CWheel::Unk9290`'s unit normal is one; both take the variant. `fild` is exact at
any precision, so the unsigned loads are exact doubles and only their `+2^32` rounds. With these, the full scan
(all 561 bindings) is clean on the two vanilla snapshots; under NGP the bindings that still differ are `0x4e6650`
(NGP's engine wrapper `0x1004b8e0`), Step1E0 (`10048d50`) and the drag (`0x4e7dc0`, `0x4e9020`). The drag then took
NGP's factor (`car_body::drag`, its `1017147c` in place of 0.5) and its `Cd * A * |v| * factor` x87 chain; identical on
the five NGP snapshots and both vanilla ones.
`CCar::Fwd18E20` (`0x4e6650`, `add ecx, 0x690; jmp Sim8E20`): NGP retargets the `jmp` to its engine wrapper
`0x1004b8e0` (rev history ring, launch and limiter post-processing, the atmosphere update, then the original
`Sim8E20` through `0x1004b800`). The binding follows the `jmp` to the wrapper's port (`ngp::engine`, bound at
`0x1004b8e0`). While the engine lives (`*(engine+0x48)+0x74`), a speed above 1.05 times the rev limit
(`*(car+0x850)+0x188` rpm, times `pi / 30`) costs its health (`+0x58`) `0.05 dt`; below 0.101 the engine dies through
NGP's UI calls (the port declines). After the atmosphere and `Sim8E20` (value `r`), a live, running (`SimECF0`)
engine gets NGP's terms, with `x = sign(w) min(limit, |w|)` and the braking term `x / limit * -1017234c * 10171aa0`.
The mode word `101767d4` picks them: with bit 8 and the idle table (`101767e8`, a double lerp table, slot 5
`0x1004a700`) the speed goes into a ring (`101767b0`, a `std::deque<float>` kept at its length); if no entry is above
the stall speed `10171a9c` and the speed is below it, the value is `-sign(w) 0.1 * 10171aa0` (halved by the def's
`+0x26f8`). Else the idle torque is `table(x) * damage * (rho / 1.2040719) * 10171aa0` (`damage` from the health
object, `0x1004c740`; times the def's `+0x283c` or 0.5 when `*(car+0x854)+0x64` is above 0), less
`sign(w) * *(engine+8)+0x270`; below the idle band `10171a98` it is capped at `0.05` of its scale (below
`min(104.72, 0.95 band)`) and added to the braking term, above the band it blends with it by the engine's `+0x20`
(and is 0 past the limit). Without bit 8, bit 2 adds `(1 - blend)` times the braking term to `r`. The value is
divided by 1.05. Identical on the five NGP snapshots and on twelve patched copies reaching every path (mutations of
each made its copies differ).
`CDrive::Step1E0` (`0x50a1e0`): NGP retargets its calls of `M6C90`, `Hook4160` and `M9BC0` to `0x100456c0` (the
starter), `0x10046880` and `0x10048d50` (shift logic, then `M9BC0`), points the clutch input's 900 at its per-car
`10171aa4`, and its `M93A0` is the ported `0x10047300`. `layout::step1e0` takes the variant and the clutch scale; the
three rewrites are `Step1Host` calls: the starter and the curve term run their ports, the wheel worker runs in the
original until it is ported. Fwd18E20's st0 is 53-bit
under NGP, so the ring's input is one x87 product of the double. Identical on the five NGP snapshots and both vanilla
ones.
NGP's `M9BC0` wrapper `0x10048d50` (`ngp::diff`, bound): the downforce (`0x10047040`), then the drive gain
(`state+0x37c`) loses the percentage at `+0xe88` of NGP's settings object (`*(*10174bb0)`, `0x100472a0`; positive gains
only). The def's `+0x640` is a packed word: its low nibble is the drivetrain type (`0x10048410`, 0 above 7) and its
next three nibbles the types of diffs 0 to 2 (`0x10048380`, 0 above 5; 1 keeps the lock, 2 open, 3 locked, 4 torque
biasing, 5 viscous). For drivetrain types 4 to 7 it sets the diffs' lock torques (the state's terms 0 to 2, `+0x18`)
from each wheel's force and torque, the car's lock values (`*(car+0x850)` `+0x1b8`/`+0x268` per diff, under drive and
on the overrun, as bias ratios `(x + 1) / (1 - x)`), the def's preloads (`+0x58`), the axle split (`10171a90`) and the
viscous gain (`10171aa0`); diff 0 opens above the speed limit (`car+0x864` against `*(car+0x850)+0x198`), and a diff
whose worker-object word (`+4`, `+0xc`, `+0x14`) is 0 opens unless locked. Types 4 and 5 then run NGP's axle worker
`0x100487f0` (`(dt, 1 or 2)`, `ngp::axle`, bound), the others `M9BC0` through NGP's trampoline `0x10045df0`. The test cars
use types 6 (`0x4436`: diffs 3, 4, 4), 4 (`0x404`) and 5 (`0x4005`). Identical on the five NGP snapshots and on
copies of them with the word, gain, flags, terms, speed limit and viscous gain patched to reach diff types 0, 1, 2 and
5 and drivetrain types 3 and 7 (`/tmp/ngp-probe/patch_snap.py`, not kept).
NGP's axle worker `0x100487f0` is the 1.02 pair worker (`M96C0`) rewritten for axle `g` (1 for type 4, 2 for type
5): the axle's two blocks (`axles[g].block_pair`) and their 2x2 clutch matrix (the state's `+0x470`, `0x10046e50`),
inverted in doubles into `+0x480` (`0x10046d10`, the reset), term `g`'s solve repeated up to 5 times, its slip values
(`0x100465d0`), latch (`0x10047720`) and sync (`0x10046f40`), then the held step for enabled wheels and
`E39_Step3870` (through the trampoline `0x100482b0`) for the others and for the other axle's wheels. Unlike 1.02 it
writes lane values only (the dirty words stay), indexes the active flags by `g + 1` (and the rows by `pack + 1`),
scales a block's force as `value * weight / scale` (1.02: `value / scale * weight`), and keeps `(scale * 0.5)` and
`(direction * gain) * reach` as separate products. Identical on cov-fwd41b and cov-rwd41 (types 4 and 5) and on six
patched copies reaching the ungated branch, the reset and repeated solve, the held steps and the sign bump; a broken
inverse made three of them differ.
NGP's `Hook4160` rewrite `0x10046880` (`ngp::curve_term`, bound; `Step1E0` runs the port for its call at
`0x50a2a8`): the speed term is 1.02's (`e_scale * w60` plus the handbrake term `term_gain * w70 * 10171a94`, 0 for the
front wheels when the drive object's gate is clear), and `row_value` (1.02's `Worker4060`, through NGP's trampoline
`0x100466b0`) runs on it. The value is `(1 - slip * k) * r * x * gain` with `slip = |slope| * gain` and `k` the
settings' `+0xfe8` (front) or `+0x1168` (rear). The two words at `+0x7c` and `+0x8c` (microkelvin) step by heat: the
friction `value * |slope| * +0x8c`, conduction between them, convection to the air (`10171adc`) scaled by
`(max(|speed|, slip)^0.78 + 1) * +0x4c` (the second word also by `+0xac`), and radiation on the first,
`(T_air^4 - T^4) * +0x9c * 1.7011119e-8` in doubles; `+0x5c` splits the heat capacity `+0x7c * +0x3c` between them.
They are stored raw through the CRT's `_ftol` (the low word of the 64-bit truncation), and fourteen terms go to a
telemetry row at `1017662c + 0x3c u`. On the rear wheels with centre diff type 3 and `car+0x864` at most the locks'
`+0x198`, half the handbrake's share of the value moves to the word at `state + 200 u - 0x98`. Identical on the five
NGP snapshots and on copies with the handbrake (`drive+0x80`) and the gate patched; breaking the split or the gate in
the port made them differ.
`CDrive::M93A0`, NGP's rewrite `0x10047300`: the first exe function ported under `rbr_physics::Variant`.
`drive_worker::clutch_torque` takes the variant; under `Ngp778` it uses NGP's sign (`ngp::sign`, `0x10043eb0`: 0 for a
zero, else ±1 by the sign bit, NaN included, where 1.02 gives 0) and zeroes the engine speed (`state+0x45c`) when
working off the sign word flips its sign. Its callees are still the exe's `M9120`/`M9210`, through NGP's
thread-local call of the original. Exact on 3,000 fuzzed direct calls of the paths without callees (390 of them zero
the speed; at most one NaN input, since which NaN an x86 add keeps isn't modelled) and in the step on eight
snapshots (one call each); a 1.02 snapshot runs `0x5093a0` as before. `M9210`'s x87 chains are modelled at 24 bits
but run at 53 under NGP; whether the eight snapshots reach it is not checked.
`E51Wheel::M3D230`, NGP's rewrite `0x1005e8a0` (`rbr_physics::ngp::tag_blend`): the tyre block's material blend in
SSE, and a fix. 1.02 weighs the first material's sine term by `w` and the second's by `1 - w`, but its lerps the
other way (`(b - a) w + a`); NGP gives both `1 - w` and `w`. It also takes the first tag for a second tag of `0xfe`
(written back), skips the blend while `g_1659b6c` is 0, and rounds each CRT `sin` (f64) to f32. Exact on 3,000
fuzzed runs entered past the two material lookups (`scripts/ngp-tag-blend-vectors.py`; Rust's `sin` agreed with
NGP's CRT after the rounding on all of them) and in the step on six snapshots (12 calls each).
`CDrive::M5FE0` and `M6260`, NGP's rewrites `0x100458c0` and `0x10045b80` (`rbr_physics::ngp::drive`): the drive
term splits the axle torque by a per-car split `10171a90` (`0x10045870`: `1 - split` for wheels 0 and 1, `split`
for 2 and 3), where 1.02 takes a fixed quarter. `0x10045230` sets it to 0.5, or for drive type 7 to the car's split
percentage (`+0xe48`) / 100, capped at 1; NGP also saves it (`0x10047c6c`), so it belongs to `NgpState`. It is 0.5
on every snapshot so far. The factor comes back on the x87, so the product with the drive gain and the sum or
difference after it run at 53 bits and round to f32 once (f32 arithmetic fails the vectors). In `M6260` a NaN latches
`+1`, where 1.02 zeroes the output. Exact on 3,000 fuzzed runs, entered past the functions' call of `0x10013cd0` (a
thread-local lookup whose result they drop), and in the step on the AWD replays (FWD and RWD don't reach them).
The material sheet lookup: NGP points the exe's indexed lookup `0x520240` (its call at `0x520265`) at its own
`0x1004df40` (`rbr_physics::ngp::material`), which reads every sheet as 16 by 16 bytes at `(16 a, 16 b)`, clamped
to 0..15, where 1.02's `0x520010` reads the slot's own size and wrap flags and only `a`. `0x520240` takes the
variant, the first patched exe function to do so; the hybrid sets it from the NGP base (`ngp::variant`). That
removes the `0x520240` differences of the coverage run (6 to 167 bytes on four snapshots). Exact on 3,000 fuzzed
direct calls and in the step on six snapshots; vanilla snapshots run as before.
`E51Wheel::M3D460` takes the variant for its one patch, the blend call at `0x53d49d` (NGP's `0x1005e8a0`): identical
on the nine NGP snapshots it differed on (up to 843 bytes), and on vanilla as before.
`E51Wheel::M3D520`, the ground probe, takes a `ProbeTuning` (`wheel::layout`). NGP changes three things in it:
- The collision system. NGP collects every exe operand that reads `g_7c755c` (21 sites; the list is built at
  `0x1008c6d7`) and points them at a heap variable of its own, which holds a second `CollisionSystem` (same vtable and
  data pointers as the exe's `0x16578a8`; on the snapshots it differs in the query scratch from `+0x1014` and in
  `+0x1814..0x181c`, 0.25 in the exe's and 0 in NGP's, and `+0x182c`/`+0x1844`). The probe's casts run on it. In
  Rust that copy belongs to `NgpState`; the hybrid reads it through the patched operand at `0x53d5e0`.
- The test vector's fill, `0x72f664` (0.4) repointed to the per-car `10171300` (0.385 to 0.4 on the snapshots).
- The middle block's pose, the push at `0x53daa6` (0.2), which NGP rewrites to the per-car `10171308` (0.185 to 0.2).
`CWheel::Unk96e0`'s table lerps take the same per-car pose as their base (`0x72f428`, 0.2, for both columns) and
`10171304` as the first column's weight (`0x72d154`, 0.15): `wheel::layout::WheelTuning` holds the three values.
Identical on six NGP snapshots and on vanilla (both functions). The other 17 sites that read `g_7c755c` (stage collision, entities,
the rigid body's vote) need the same treatment where a binding reaches them.
What the "damper" helpers are: `DamperStep1` (`0x52e350`) is `(T - T_ref) * (h * area)`, a heat flux, so the
helpers NGP rewrites are the tyre's thermal model; `RelaxStep` (`0x534638`) calls it with the tyre temperature,
298.15 K, 20 and 0.54 for the flux to the air. NGP's `0x1004d410` (`rbr_physics::ngp::damper::convection`) replaces
that call: the air temperature as the reference (its `DamperStep1`), the coefficient raised by a convection term
`max(|car speed|, |spin| * mean radius)^0.78 * factor`, and the tread's area `2 pi (r_o^2 - r_i^2)` from the tyre's
radii in place of 0.54. The factor is per car and tyre kind (`10171470` for kinds 0 and 1, `10171474` for 2 and 3;
the car's `+0xe58`/`+0xe68`, else 6, set by `0x10045230`, which also sets `RelaxStep`'s `10171478` from `+0xe78`,
else 800), times `1 + scratch+0x1c8` when `scratch+0x480` is set. Exact on 3,000 fuzzed direct calls (Rust's `powf`
agreed with NGP's CRT `pow` after the f32 rounding on all of them) and in the step on six snapshots.
The rest of the thermal cluster takes NGP's state as `Option<&ngp::damper::Thermal>` (`None` for 1.02, loaded by the
hybrid's `ngp::thermal`): `HeatStep` (threshold 40, NGP's `DamperStep1`), `TableStep` (NGP's `0x1004d3c0`/`0x1004d380`
with the scratch's factor), `RelaxStep` (the rate `10171478`; the carcass flux `0x1004d550`, `(T - T_carcass) * c *
area` with no temperature shift and the inner area `2 pi (r^2 - r_i^2) + 2 pi r wac`, `r = w7c - 2 w9c`; the
convection above; the exe's `DamperStep2` at 53 bits), and every x87 chain through `fpu::round_x87` (53 bits under
NGP). All four bindings (with `RelaxCount`) identical on four NGP snapshots and on vanilla. `0x1004d550` has no direct
vectors; it is only checked in the step.
`CScratch::StepInner` (`0x535310`) diverged under NGP although no summary listed it: NGP repoints four words of
its friction-curve table (`0x535317`..`0x53536a`: 0.99, 0.96, 0.87, 0.7 become 0.95, 0.875, 0.775, 0.65,
`1014fbc8..d4`), its `ComputeOutputs` is NGP's wrapper, and it and its x87 callees (`FloorS32`, `TyreFrictionCurve`,
`SolveDirs`, `ComputeSlip`) run at 53 bits. All take the variant now; identical on five NGP snapshots and on vanilla.
`CScratch::Step` (`0x5360c0`) under NGP (`tyre::step::step(f, a, Some(&thermal))`): NGP's `Step` wrapper
(`0x1005b660`) saves the def's tables `+0x204..0x2a4` and restores them after the call, so the scaling inside is
per call. `0x1005be40`, in place of `Accumulate` (`0x5362ab`), scales the blend pairs `+0x244` (read by `StepInner`)
and, past 10 degrees, the heat lerps `+0x204` by a factor of the inclination `asin(scratch+0x3ac)` in degrees
(positive: `1 - 0.01 d - 0.0025 (d - 5)`, capped at 45; negative: up to 1.05, falling past 10, capped at 34), then runs
1.02's `Accumulate`. `0x1005bc50`, around `HeatStep`, scales `+0x3c8` by `(1 + 0.1 min(10, |d|)^2)` times the def's
`+0x10c` (if above 0) for the call and stores `|+0x3c8 * spin| * factor` at `+0x4e8`. Two code patches move the
second `e3` product from `+0x1e8` to `+0x1f8` (`0x5365cb`, `0x5365ec`), and the output scale's 0.1 becomes 0.025
(`0x536703`). `ngp::inclination`; `asin` is Rust's (the CRT's `asin` agreed in every Step checked, no direct vectors).
Identical on six NGP snapshots and on vanilla, with its callees (`ContactPatch`, `SolveDispatch`, `SolvePair`,
`SolveForces`, `Friction`, `Sign`) at 53 bits; `SolveA`/`SolveC` were not reached under NGP.
`E51Wheel::M3DD30` (`0x53dd30`, `wheel::layout::wheel_step(.., ngp_slip)`): NGP repoints the tyre load thresholds of
the `tyre+8` flag (28000, 60000, 3.125e-5) to its per-car `10172f58/5c/60`, and retargets three calls: `Step` to its
wrapper `0x1005b660` (the def tables' save and restore around the per-surface scaling), `E51MultiTire::Emit` to
`0x1005f8a0`, and the strut's spring step `0x52e090` to `0x10059f70`. The hybrid's host follows each call site's rel32,
so those three run as NGP code. Identical on five NGP snapshots and on vanilla. The `Step` wrapper is ported
(`ngp::step_wrapper`, bound): besides the save and restore it scales the step's fifth argument's `+0x48`, `+0x58` and
`+0x78` for good by `1 - min(200, 3.6 |v|) / 200 * 0.025` (`v` the scratch's `+0x360`), and for the call the heat
lerps `+0x204..0x244` by the square of a settings percentage picked by the surface class (the argument's `+0x148`)
and the tyre type (the def's `+0xc`): class 2 with types 0 to 2 (`+0xe98`), class 1 with 3 to 5 (`+0xea8`), class 3
with 6 (`+0xeb8`); 0 leaves them. Identical on the five NGP snapshots; the surface scale is not reached there (class
0 on every contact), so it is checked against the asm only.
The spring step wrapper `0x10059f70` (`ngp::spring`, bound) is a bump stop around 1.02's `CSpringDamper::Step`:
when the travel is within `0.05 - g` below the def's top (`+0x28`; `g` the car def's `+0x41c`, 0 outside
`[0, 0.05]`), the rate `+0x98` scales by a ratio (the depth over the gap with NGP's setting bit `0x20000` and the
game object's `+0x69c` bit 1, else 1) and the damping `+0xd8` by its root, then by `1 / 0.3` on a negative speed or
by the car def's `+0x1fc`; the ratio, rate and damping go to the strut's `+0x30..0x38`. The rebound value `+0x118`
scales by `1 - 0.4 *(strut+0x10)+4`, the def's `+0xe8`/`+0xf8` may latch the strut object's `+0xc`/`+0x10` (and be
cleared for the call), the limits are copied to `*(obj+0x18)+0xe80..0xe8c`, and after `Step` the def's words are
restored and 1.02's `UpdateForces` runs. Identical on the five NGP snapshots and on five patched copies.
`CPhysics::StepForward` (`0x5273a0`): the clock's `fadd` runs at 53 bits under NGP (the only difference on the
snapshots). NGP also repoints the collision window's 4 to 20 (`0x527447`) and patches the contact mask of the
substep counter from 3 to 0x13 (`0x527508`: the counts 0, 4, 8, 12 of every 32); both are ported but not exercised,
since `SUB_STEPPING` (0x1000) is off on every snapshot and the contacts run on every substep.
`E51Wheel::M3B900` (`0x53b900`) and `E34Quat::ToQuat` (`0x4e4d10`) diverged only by precision: an x87 chain of
several operations rounds once at its store under NGP, where 1.02 rounds each operation to 24 bits (a single x87
operation stored to f32 gives the same f32 either way, since 53 >= 2 * 24 + 2). M3B900 has three such chains (`S`
and two `sqrt(1 - c*c)`), ToQuat its root's argument. Identical on five NGP snapshots and on vanilla.
Retargeted call sites in bound functions: NGP retargets 157 `call rel32`s, 35 of them in functions the hybrid binds.
A binding whose host calls the exe callee by address skips NGP's code there, so the hybrid's hosts take the target
from the call site (`ngp::call_target`) where the port leaves the call to the host: `VfC8`'s `SampleBig` (NGP's
track sampler `0x10015fc0`), `M3DD30`'s three, and the signal emits of `Raycast`, `Vf6cx`, `Update9AF0`, `UpdateSeed`,
`Sim8E20`, `Sim8CF0`, `ResetOutputs` and `BigB` (NGP's emit wrappers, which also feed its own listeners).
`EmitNoise` ports its emit, so it declines when the call is retargeted. `CSolid::E43Vf6C`'s clear threshold (1e-5)
is NGP's 4e-7 (`0x52cd05`, `0x52cd4e`).
The load side: `rbr_formats::lsp_keys_ngp` (`notes/ngp-load-path.md`).

Order of the deltas (each checked like a 1.02 group: bind it, `rust_step.py` on NGP snapshots, then long runs):

1. Variant switch: an NGP mode in `rbr-physics` that changes behaviour only where the patch table
   (`original/ngp/analysis/patches-runtime.tsv`) says, with the exe ports taking each repointed constant from where the
   patched operand points (NGP's `.rdata` is fixed and can be read from the DLL like the exe's sections; `.data`
   values are state). Then the code edits (43 in physics: StepForward's collision mask `& 0x13` and window `20 dt`,
   damper branches, `E37GridOps::EdgePair`). The 11 bindings above that differ through patches are the first list.
   The NGP mode also needs 53-bit x87 intermediates (x87 precision): the ports that keep values in registers across
   operations (`0x53b900`, `StepForward`) take an f64 path there, checked on NGP snapshots.
2. The 18 `Emit` rewrites need no port: same fan-out, only the listener calls move to NGP's thread. Keep them as the
   1.02 emits; the telemetry prologues (`0x10041f30`, `0x1005f6f0`, `0x1005f8a0`) only if they write car state.
3. Atmosphere (`0x1004b5a0`, formula above) with the `DamperSolve`/`DamperStep1` wrappers: exact, small.
4. Small rewrites and wrappers: `ComputeOutputs` clamp, `M93A0` zero crossing, `M3D230` `0xfe` remap, `M6260`/`M5FE0`
   axle factor, `M7D60`, `DamperStep2`, the new `RelaxStep` term, `Pick` (16x16 map; needs NGP's ground object that
   replaces `0x7c755c`).
5. Tyre: the `Accumulate` replacement (`0x1005be40`, slip curve), the `HeatStep`, `CScratch::Step` (wear and surface
   multipliers) and `CSpringDamper::Step` wrappers.
6. Drivetrain and engine: the starter (`M6C90`, ported, in simulation time), the `E39` hook, the gear
   state machine before `M9BC0`, the launch/limiter wrapper after `Sim8E20`, the engine model (torque curves).
7. Driveline sample cache (`BlendB`/`LerpAt`/`SampleBig` -> `0x10016aa0`/`0x100168b0`/`0x10015a80`): the lookups first,
   reading NGP's table from the snapshot; the table build (at load) with 8.
8. NGP load path: the `*_NGP` keys go through the stock parser into the 1.02 definition members (renamed and
   extended key tables, `notes/ngp-load-path.md`), so the key map is data. Then the per-car `.data` values NGP
   computes from them, the exe constants NGP writes per car (clutch help speeds) and the NOPed `CDef` defaults; checked
   byte for byte against a snapshot after load.

Recordings needed: real NGP drives by a person (one per surface class to start, as the 1.02 gate matrix), each with two
reference runs and a noise mask; the starter case (engine off at the start) recorded live; NGP times it by the wall clock
and the port by simulation time, so compare it at real-time playback. Snapshots at chosen steps of each (`SNAPSHOT=`).

Open risks: the threaded path (NGP's `+0xd` flags: listener posts are seen, the control strand isn't); whether other
NGP code reads the wall clock in situations the test drive didn't reach; NGP's ground object; how many ports change
at 53-bit precision over a whole drive (one snapshot shows 2) and whether Windows gets the same control word as Wine.

## Pure step under NGP (2026-10-17)

`rbr-standalone --snapshot` runs an NGP snapshot with every function a port (`Variant::Ngp778`, from the NGP base
found in the image). What it takes beyond the hybrid:
- NGP's drive line sampler `0x10015fc0` (`ngp::drive_line`, bound): 1.02's `SampleBig` search in doubles, up to 40
  bisection steps to a dot of 0.002 (1.02: 9, 0.01).
- NGP's trampolines into the exe and its emit wrappers run the exe original they stand for (`ngp::original_for`), so
  the pure host needs no port of them. In the hybrid, a trampoline whose exe target is bound is hooked too (it enters
  the target past the hooked entry), and the wheel's tyre-particle emit goes through NGP's wrapper with `esi` set
  (`Host::call_at_esi`).
- Controls: NGP hands the payload to the player car's control input `CCar::UpdateSeed` (`0x4e91f0`) itself, so the
  pure `Emit` calls it directly under NGP (Controls under NGP).
- The VCU takes NGP's repointed constants (`vcu::layout::Tuning`, read by `ngp::vcu_tuning`): cooldown, launch rpm,
  the clutch ramp and the rev cut's margin over the limit (`10172db0`, `10172db4`, `10171ae4`, `10172338`,
  `101767c4`; `10171ab4`, 25 rpm for 1.02's 300: `RevLimitCut` `0x536f11`, ngp-gravel-206-23 35625).
- More 53-bit chains (x87 precision), each found as the first drift of a long run: `SolveDirs`, the turbo energy
  step's root, the collision `normal_impulse`, `E43Vf138`'s dots, `Vf6cx`'s `+0x15f4` update, and `ClampGrip`'s
  `s b / sqrt(A)` (the two operands stay on the stack). `CScratch::Step`'s `1 - cos` now subtracts from the 64-bit
  `fcos` result (`trig::one_minus_x87_cos_f64`); it was not the drift there but is what the x87 does. Files that
  still round to 24 bits unconditionally (wheel `block`, `select`, `frame`, `prof`, VCU tables, `dummy`, part of
  `e38` and the spring-damper) are fixed as runs reach them; `E38_004FB890`'s offsets (ngp-snow-zr-14, a prop) and
  the brake wall's `segment_hit` (ngp-gravel-206-23), `E38_004FC590`'s two-sided split (`split_offsets`,
  ngp-gravel-zr-31 at 128103) and `M_E2D0`'s face normal and reciprocals (`point_triangle`: a woken prop's mesh
  contact in ngp-gravel-206-23, seen at 40363 only as the awake list's link at car+0x10) now follow the variant. `edge_pair` (`Store5570`'s gap sums,
  returned unstored in st0, and `Walk5FF0`'s `t`) now follows the variant.

Runs on `cov-rpl11.bin` (stage 11 gravel, Fiesta Rally3): from `snap-9520` all 1000 steps with recorded controls equal
the recording; from `snap-6438` all 4082 do, to the end of the recorded controls. Step 8903 (a contact with a prop,
`CBody::VfD0` -> `E33MakeEvent` kind 5) was `EdgePair` (0x4f8560) at 24 bits: bisected by binding subsets with VfD0 and
`E33MakeEvent` left to the original and comparing the event's impulse at entry (`/tmp/ngp-dbg/bisect5.sh`). The
impulse's w lane still differs from the original's at that entry and nothing reads it. A playback recording runs 2 s past its last control without emitting and logs zero controls
there; the standalone stops at that tail.

Live mode (2026-10-17): the launcher lists the NGP snapshots as the drives `ngp-rpl11`, `ngp-rpl20`, `ngp-rpl32`,
`ngp-rpl41` (Fiesta Rally3, slot 5), `ngp-fwd41b` (Fiesta Rally4, slot 4) and `ngp-rwd41` (BMW M3, slot 2): their
`.auto` (map, car, weather) in the recordings folder and the snapshots in the snaps folder (each snapshot holds its
stage name: 11_O, 20_N, 32_M, 41_M). The car is drawn with the slot's stock model. `rbr-standalone --live` fed
`cov-rpl11`'s controls equals the headless run (all 1000 steps from 9520, 2465 from 6438); the other replay snapshots
run to the end of their playback controls without a stop (those controls come from other runs, so they are not
compared). The pure host runs NGP's 21 emit wrappers as their 1.02 emits. The general-state pair (`0x10041f30`,
`0x1005f6f0`) adds a prologue that writes a vector per wheel from NGP's getters into its copy of the signal, and
`0x10041f30` passes the copy to `CFenceHandler::OnCarState` (cloth only, `notes/interactive-scenery.md` 3.6); neither
writes car state. The Rust `Vf28x` calls the 1.02 emits directly anyway.

H-shifter (2026-10-17): the live bindings have NGP's direct gear actions (reverse, neutral, gear 1 to 6). A press sends
the payload's select bit (flag 4) with word 6 = 0 R, 1 N, gear + 1, which `CDrive::M6840` takes; with a gear bound and
neutral not, releasing every gear selects neutral. A gamepad binding can name its device (`@vendor:product`), set
when the launcher captures it: cachy's T500 RS shifter (`044f:b660`, buttons 288 to 297) shares its codes with the
G923's buttons. Checked headless on `ngp-rpl11` 6438 (car slot 5): selecting 3rd, neutral and reverse each takes within
100 steps; 6th on the five-speed gives neutral (`M6840` keeps a value below the gear count, else 1). Not checked
with the shifter itself.

Advanced H-shifter (2026-10-18, opt-in, ours; `rbr_render::shifter`, `docs/playing.md`): RBRControls' advanced mode
(`references/rbrcontrols-hshifter.md`) in the viewer, sending the same select. The live process's GBX1 record
(`rbr_recording::gearbox`) gives each gear's ratio (`CDef` +0x694 + 0x50 g) and the drive speed (state +0x384 times
`CDef` +0xa54, `CDrive::M5A10`'s term), so a gear's engine speed is `ratio * drive speed`. Checked headless on
track-54_E, car 0, autopilot: in gear it is within ~3% of the engine speed under 1.02 (the clutch's slip) and within
1 rad/s under NGP (617 against 616.6 in 3rd). A scripted drive under NGP grinds 1st from a stop without the clutch,
takes it with the clutch, grinds an upshift at full throttle, and a dog box takes it once the throttle lifts.

Tools: `rbr-hybrid/examples/pure_snapshot.rs` writes the pure state at a later step as a snapshot (with the pure
allocator's window as one more region), so `rust_step.py` and the original can start just before a drift.
`replay.py` stubs NGP's CRT lock and condition-variable calls (its worker queue).

Damage (2026-10-17): NGP scales `E36DamageOps::Slot1`'s two constants per car at load by the "0.94195 group" factor
`r`. It patches the immediate of the push at `0x4f13a6` (`EngineGate`'s limit, 11000 x r) and repoints the compare
at `0x4f13d3` from 500 to its `1017130c` (500 x r); `ngp-rpl11`'s car (slot 5) has 11077.574 and 503.526
(`r` = 1.00705). `Slot1` takes them as `DamageTuning` (`ngp::damage_tuning` reads both sites under NGP). Its two calls
(`0x4f149b` to `100ed5e0`, and `EngineGate`'s `0x4ef703` to `100edf40`) are emit wrappers the pure host runs as their
1.02 emits. `rust_step.py` from `snap-9520` now runs all 640 steps (it stopped at 10119 before). NGP's
`HandleDamage` thresholds (800, 4000 x `r`) only drive force-feedback rumble, which the pure host skips.

## Live play under NGP (2026-10-18, ngp-live-2)

- **Data.** `original/ngp/7.4.778/physics.rbz` is NGP's stock-car archive (no `*_NGP` keys); NGP's own cars (Fiesta
  R3 in slot 5, ...) are in `ngp-data/physics.rbz`. With them idle is 105 rad/s (1004 rpm), as the NGP snapshots; the
  1.02 Corolla idles near 327 rad/s (3130 rpm) in the original 1.02 recording too.
- **Controls.** The live payload is the device input at f32 precision (`rbr_recording::input`, INP1); the standalone
  runs the game's filter (`PC_InputFilter.ini`) and then 1.02's quantizer (`E16SteerState::Pack` 0x48c910) or NGP's
  handler of `CSigCarControlFiltered` (`0x1008e8a0`, `CarControl::ngp`): the filtered channels unquantized, the flags
  mapped, a gear select only when it is not the drive's gear already (`dri+0x570`, inferred). Recorded payloads
  (word 7 of 0 or 1) still replay the quantized path, as NGP's replay mode does.
- **Records.** NGP's stage records, personal bests and ghosts go under `NGP/` beside 1.02's (launcher).
- **Launcher: NGP only (2026-10-19, user decision).** The launcher requires NGP's `physics.rbz` and offers the cars
  NGP drives (the retail slots with NGP cars in the archive, and the RSF cars); it has no physics choice. 1.02 physics
  stays available for development and the checks through `rbr-standalone` (no `--ngp-physics`) and the viewer's
  `--live` command line: Tanner and the 1.02 vectors remain the core's exactness proof.
- **Finish.** The autopilot drives NGP's 1/720 s steps (its timers scaled by `round(0.007 / step)`) and waits out the
  countdown revving in neutral on the handbrake; on track-11_O with the Fiesta it finishes, the race ends on the end
  page after the finish timer, and the personal best is written. One-step headless frames run the f32 race clock
  about 1% fast (`time += dt` with 1/720), not changed. The ghost records one game frame per 1/60 s of one-step
  frames (else the buffer is full after 85 s).
- **Sound.** NGP without FMOD keeps the 1.02 sound and patches about 120 sites of it: mostly null checks of failed
  voice creation, stream buffer sizes and the listener path. The ones that change what is heard, as `rbr-sound`
  ports them: the engine voice's low rpm (0x4b085c, 0x4b08da: 500 to NGP's `10171ad4`, 0.75 of the idle band in rpm,
  750 for the Fiesta, 381.97 for stock cars), the whine caps (0x4ae593, 0x4ae5b5: 600/6000 to `10171acc`/`10171ad0`,
  1.3 x the max torque and ten times it), the tyres' second surface when the pair's share is above 0.5, NGP's road
  list (`10171280`) and skid threshold (settings, default 1.0, against 1.25), and `E30DXSound` +0x74 made
  `SetMaxDistance` (0x4d2961) with the four double `+0x78` calls changed to `+0x74` (fire 1-20 m, helicopter 50-100
  m, ignition, cooler). The per-car words travel in the live NSD1 record. Not modelled: the in-car engine filter
  gain fix (`Mcb80` 0x4acbb1), the DirectSound floor (-9800 mB), the pad rumble events swap (0x4ae2eb, 0x4ae3c0).

## Decisions (2026-10-16)

- Target: NGP 7.4.778 (`PhysicsRevision_NGP 778`). RSF runs 7.5.779 (2025-03-05: FFB, pause on loop failure, FMOD);
  its `physics.lsp` matches 7.4.778's except for the revision line, so the pin costs nothing on the data side.
- Car data: the NGP cars from the RSF install (user-approved copy, read-only on the RSF side).

## Local data

- `original/ngp/7.4.778/`: the package as downloaded (`NGP.dll` sha256 `95b6dbf5...edaaa4`, `physics.rbz`
  `831cf121...0c4b27`).
- `original/ngp/cars-rsf/`: the 42 NGP cars RSF had downloaded (`rsfdata/cars/*_ngp6`/`_ngp7`; physics files only,
  6.1 MB). Each car is a slot folder: `common.lsp`, surface and `r_*` files, `setups/`, `repair.ini`, and a text file
  named after the car (name, revision, spec date, 3D model). RSF installs a car by putting these into a slot directory
  of `physics.rbz` (for example `Physics/t_coroll/`). Car models stay the slot's stock ones.

## Test install (cachy)

`~/Games/rbr-ngp-7.4.778`, prefix `~/Games/prefixes/rbr-ngp`, launcher `rbr-ngp` (as `rbr-vanilla`). A copy of slot 0
(without the harness state files) plus `Plugins/NGP.dll`, `Plugins/NGP/`, `AudioFMOD/`, and NGP's `physics.rbz` with
the Celica ST185 Group A (`Toyota_Celica_2000GT-FOUR_RC_ST185_GrpA_ngp6`) in slot 6 (`t_coroll`, the reference drive's
slot). The harness plugin is installed alongside.
- Menu: NGP loads (writes the `[NGP]` section of `RichardBurnsRally.ini`, `AudioFMOD/AudioFMOD.ini`); the harness
  plugin applies its FFB patches.
- `RBRRecompAuto.txt` `54 6 0 4` starts Tanner with the Celica; the car sits in neutral with the engine idling.
- NGP cars by slot (`scripts/ngp-install-car.py`; the Celica-only `physics.rbz` is kept as `physics.rbz.celica-only`),
  for one drive per drivetrain (`Features_NGP` in the `r_*` files: drivetrain type + diff types):
  slot 2 (`mg_zr`, menus "MG ZR"): BMW M3 E30 Group A, RWD, mechanical rear diff;
  slot 4 (`p_206`, "Peugeot 206"): Ford Fiesta Rally4, FWD, mechanical front diff;
  slot 5 (`s_i2003`, "Subaru Impreza 2003"): Ford Fiesta Rally3, AWD 50:50, spool centre diff;
  slot 6 (`t_coroll`, "Toyota Corolla"): Toyota Celica ST185 Group A, AWD 50:50, centre diff type 5 (not in the
  files' own list; NGP's readme has viscous couplings).
  All four idle on stage 41 with the engine running.
- Profile: the user's RSF profile `pfMULLIGATAWNY` (copied 2026-10-16; clutch help on, as its replays' setups say).
  The earlier profile (`pfthomas`, copied from slot 0: `ClutchHelp 1`, `AutoGears 0`, `GearGuard 1`) is in
  `~/rbr-ngp-profiles/thomas`.
- `~/rbr-ngp-replays`: the seven RSF replays on retail stages recorded with NGP 7.4.778 (stages 11, 20, 21, 24, 32,
  33, 41; Fiesta Rally3), with their `.ini`.

## Open questions

1. Step hook and recorder: answered, they work unchanged (720 calls/s, same car layout). Controls: through the
   harness NGP mode (Controls under NGP).
2. Is a moving NGP drive deterministic? Yes in body state for a retimed vanilla drive (3 runs, 20 s of driving
   and a crash). A full stage needs a real NGP drive; the strand case (`+0xd` set) is untested.
3. Answered for pressure, temperature, density and the drag factor: the car's height every step (Atmosphere).
   Open: the 0.94195 group and the other `.data` words that differ by stage or tyre.
4. Answered: the automatic clutch is stock clutch help with NGP's per-car engine speeds (Controls under NGP).
   Untested: a profile with `ClutchHelp 0`, where the payload clutch should drive the car.

## Next steps

1. Done: `notes/ngp-replacements.md`, the 65 redirected physics calls (49 rewrites, 14 wrappers, 2 new), and the
   automatic clutch.
2. Done: NGP's LSP keys, `notes/formats/ngp-lsp.md`. All 421 NGP files parse with `lsp::parse` unchanged; 153 new
   `*_NGP` keys (`lsp_keys::NGP_KEYS`) and 15 new surfaces (`NGP_SURFACES`), checked by
   `all_ngp_lsp_keys_resolve_or_are_ngp` in `rust/rbr-formats/tests/lsp_keys.rs` (`RBR_NGP=original/ngp`).
3. Done: harness NGP mode, IGNITION, synthetic playback, determinism of a retimed drive. Next: a real NGP reference
   drive (the user drives: `LIVE=1 scripts/ngp-run.sh NAME`), then playbacks of it and a noise mask. RSF replays
   can't replace it (Replays).
4. Done: noise mask for NGP runs (Harness NGP mode). Count runs (transplant) don't apply under NGP yet.
5. Done: emulated NGP step and the 1.02 bindings measured on it (Emulated NGP step, Rust plan for NGP). The wheel
   refresh is attributed (x87 precision). Next: the deltas in plan order, starting with the variant switch and the
   atmosphere.

NGP's starter `0x100456c0` (`ngp::starter`, bound), its rewrite of `CDrive::M6C90`: while the gate word (`+0x50`, the
IGNITION bit) and the worker object's ready word (`+0x7c`) are set and the hold is more than 0.5 s and at most 2.5 s,
the factor is the object's term (`+0x8c`), above a knee of `0.75` of the idle band (`10171a98`) scaled by
`1 - (w - knee) / (band - knee)`. The term loses `rate (+0x74) * 0.02 * factor * dt` and stays at or above 0, the
result is `factor * 80` (kept at `+0x1a0`, the factor at `+0x1c8`), and a positive result on an engine below 0.001
rad/s sets its speed (`state+0x45c`) to 0.001. Otherwise both outputs are 0. 1.02's gear-1 gate and 400 rad/s limit
are gone. The hold is NGP's wall clock (above); the port uses simulation time. Identical on the five NGP snapshots
(gate clear) and on 2,000 direct calls of the original with the wall-clock hold set to the simulation hold
(`scripts/ngp-starter-vectors.py`, `tests/ngp_starter.rs`); mutations of every term but `maxss`'s signed-zero tie
were caught.
