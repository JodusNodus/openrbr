# Gameplay input and physics loop

The player's input is polled once in the application loop, then the filtered
control is delivered synchronously before each physics substep. The reference
drive starts with four frames of 40 substeps while the car settles after load;
later frames use one substep. Each substep uses the `PhysicsEngine.MaxTimeStep`
float at physics-definition offset +0x1608c. The active retail configuration
sets it to 0.0070 s, so that is the fixed physics step for this drive.

`CPC_Application::Vf0c` (0x44a3d0) calls `CInputManager::TicFwd` (0x4cc550)
before `CApplication::Tic` (0x4804f0). The input manager's
`USInputManagerImpl::Tic` (0x4cbf50) polls the devices, samples the configured
axes and buttons, and emits `CSigCarControlInput`. The input filter receives
that signal at 0x48ab40, which copies raw offsets +0x14, +0x18, +0x1c, +0x20,
and +0x24 into filter offsets +0x2f8, +0x2fc, +0x300, +0x304, and +0x308.
These are steering, brake, throttle, handbrake, and clutch in filter channels
0 through 4 respectively.

In the active driving state, `CApplication::Tic` calls
`TicPhysicsRecording` (0x47f420) when the replay manager is ready. For each
requested substep, `TicPhysicsRecording` reads the definition pointer from
`g_1659b70 + 0x24` and the fixed timestep at definition +0x1608c, runs
`E16Filter::Tic` (0x48c500), scans and fires the replay control, then invokes
the physics object's vtable slot at
0x73f994 with a count of one. That slot enters `CPhysics::StepForward`
(0x5273a0). The harness records one 0x20-byte `CSigCarControl` emitted by
`E16Quant::NotifyFilt` (0x48d130) at 0x48cd90 before its call returns to
0x48d253. The signal relay at 0x48cd90 passes the payload to connected car
handlers without changing it.

The float at definition +0x1608c is the `PhysicsEngine.MaxTimeStep` LSP
property. `TicPhysicsRecording` (0x47f420) reads it once per substep and passes
it to `E16Filter::Tic` (0x48c500); `CPhysics::StepForward` (0x5273a0) reads
the same field into `base` and passes `base` to the physics workers and
`Work5500`, then accumulates `base` as elapsed simulation time. The collision
call uses `base * 4`.
`CApplication::StartQuickRace` (0x481660) calls `ReloadPhysicsLsp`
(0x47fcf0), which asks file-service vtable slot 8 for
`Physics/physics.lsp`. The retail `Root/physics.rbz` copy sets
`MaxTimeStep 0.0070`; the separate CHELALIC `Physics/physics.dat` copy sets
0.0100, but archive inspection shows that this loader's logical file is
provided by the RBZ copy, not the DAT archive (see `notes/formats/cars.md`).
The live value is confirmed by the harness peek probe
`*0x1659b70 +0x24 * +0x1608c f32`, which follows the global host pointer to its
physics definition and reads `MaxTimeStep`. A plain run on 2026-10-13 recorded
12,084 samples after physics steps 40 through 12,240; every sample was 0.007.
This confirms the fixed integration step is 0.007 s per substep on this path.

`E13Host9B70` has a vtable at +0x00, then `m_unk20` and `m_unk24` at +0x20
and +0x24 (size 0x28). Its pad now spans +0x04 through +0x1f, accounting for
the compiler's implicit vtable pointer. `TicPhysicsRecording` uses the typed
`m_unk24` access, and the E13/E21 typed readers now address the same state and
definition pointers as the original. `E14ProfFrame` reserves the 8-byte
profiler channel passed to `E46ProfChan::Enter`/`ProfExit`: sample pointer at
+0x00 and flag at +0x04.

The payload is eight 32-bit words. The first four are normalized pedal values:
throttle, then brake. (Measured 2026-10-15: from step 2000 of tanner-corolla,
`harness/live.py --original` with word 0 at 1.0 speeds the car up and word 1 at
1.0 brakes it hard; the recorded launch holds word 0 at 1.0 with the clutch in.
The producer reading below, which named word 0 brake, needs re-checking.) The steering float occupies word 4 as
its raw IEEE-754 bits. Word 5 is a bit field. Words 6 and 7 are integers, even
though a recorder that formats every word as `f32` will print them as tiny or
zero floats.

| Word | Meaning | Producer evidence | Consumer evidence |
| --- | --- | --- | --- |
| 0, +0x00 | Throttle, normalized 0 to 1 (measured; see above) | `USInputManagerImpl::Tic` (0x4cbf50) writes throttle at input +0x1c. `E16Filter::Tic` (0x48c500) produces channel 2 at filtered +0x1c. `E16SteerState::Pack` (0x48c910) quantizes filtered +0x1c into byte 4; `E16Quant::NotifyFilt` (0x48d130) converts byte 4 back to float word 0. | `CScratchSim::ReadPacket` (0x537f20) writes word 0 to scratch +0x0c, whose reconstructed member name is `m_throttle`. The VCU later swaps pedal lanes in reverse/neutral or when brake exceeds throttle (`state.pedal_swap`; verified against the original). |
| 1, +0x04 | Brake, normalized 0 to 1 (measured; see above) | `USInputManagerImpl::Tic` (0x4cbf50) writes brake at input +0x18. `E16Filter::Tic` (0x48c500) produces channel 1 at filtered +0x18. `E16SteerState::Pack` (0x48c910) quantizes filtered +0x18 into byte 5; `E16Quant::NotifyFilt` (0x48d130) restores byte 5 as float word 1. | `CScratchSim::ReadPacket` (0x537f20) writes word 1 to scratch +0x10, whose reconstructed member name is `m_brake`. The VCU pedal swap described above explains why these destination labels do not change the serialized wire order. |
| 2, +0x08 | Handbrake, normalized 0 to 1 | Input +0x20 is handbrake at 0x4cbf50. Filter channel 3 is written at +0x20 by 0x48c500, packed into byte 6 by 0x48c910, and restored as word 2 by 0x48d130. | `CScratchSim::ReadPacket` (0x537f20) writes it into scratch +0x14. `CDrive::M5970` (0x505970) reads payload +0x08 into its handbrake lane at +0x4c and scales it into +0x80. |
| 3, +0x0c | Clutch, normalized 0 to 1 | Input +0x24 is clutch at 0x4cbf50. Filter channel 4 is written at +0x24 by 0x48c500, packed into byte 7 by 0x48c910, and restored as word 3 by 0x48d130. | `CScratchSim::ReadPacket` (0x537f20) writes it into scratch +0x1c, used by `CScratchSim::Step` (0x537f50) when setting the drive clutch lane. |
| 4, +0x10 | Signed normalized steering float, stored as integer bits | Filter channel 0 is written at filtered +0x14 by 0x48c500. `E16SteerState::Pack` (0x48c910) maps it to signed byte 3; `Unpack` (0x48c9c0) restores `(signed byte * 2 / 255)`. `NotifyFilt` (0x48d130) copies those float bits into this word. | `CCar::UpdateSeed` (0x4e91f0) stores the word in the car control state; `CScratchSim::ReadPacket` (0x537f20) preserves it at scratch +0x18. |
| 5, +0x14 | Integer control flags | `NotifyFilt` (0x48d130) maps input bits 0, 1, 2, 0x20, and 0x40 to output bits 0, 1, 2, 0x10, and 0x20. `USInputManagerImpl::Tic` (0x4cbf50) sets input bit 0 for action 7 and bit 1 for action 8. | `CScratchSim::UpdateD` (0x537a80) treats bit 0 as gear up and bit 1 as gear down. Bit 2 passes word 6 to `CDrive::M6840` (0x506840) to set the selected gear state. `CCar::Update91F0` (0x4e91f0) uses bits 0x10 and 0x20 for car-problem and reset branches. |
| 6, +0x18 | Conditional gear-selection value, not a float | `NotifyFilt` (0x48d130) writes `m_ch.m_b70` here only when flag bit 2 is set. The value comes from packed byte 8, restored at 0x48c9c0. | `CScratchSim::UpdateD` (0x537a80) passes it to `CDrive::M6840` (0x506840), which clamps and writes the drive's selected-gear state at +0x570. When flag bit 2 is clear, the original filler does not initialize this word, so its recorded bits may look like a very small float. |
| 7, +0x1c | Auxiliary one-bit control state, not a float | `NotifyFilt` (0x48d130) writes `b1 & 1`. `USInputManagerImpl::Tic` (0x4cbf50) sets raw input bit 0 from action 12. | `CDrive::M5970` (0x505970) copies payload +0x1c into drive state +0x50. The source does not establish a public control name for this bit. |

The table follows the serialized payload order, not the order of the raw input
fields. The raw input signal stores steering first, then brake, throttle,
handbrake, and clutch; the filter preserves those channel positions.
`E16SteerState::Pack` (0x48c910) emits throttle channel 2 before brake channel 1, then handbrake,
clutch, and steering. `E16Quant::NotifyFilt` (0x48d130) restores byte 4 as
word 0 and byte 5 as word 1. The live-mode wire order is throttle, brake,
handbrake, clutch, steering, flags, gear, aux (measured 2026-10-15).
`CScratchSim::ReadPacket` writes words 0 and 1 to members reconstructed as
`m_throttle` and `m_brake`, but this does not mean the payload order is reversed:
the VCU deliberately swaps the pedal lanes in reverse/neutral and when brake is
greater than throttle. This behavior is represented by `state.pedal_swap` in
`rust/rbr-physics/src/vcu/dispatch.rs` and `gear.rs` and was checked against the
original. The emitted wire order is throttle then brake (measured). Pedals are
byte-quantized to 1/255, steering to a signed byte scaled by 2/255, and the
emitted flag word already reflects the bit remapping shown above.

The behavior review also corrected several errors on this path. The details and
verification results are in the G1 report.

## Frame scheduling, stage events, and replay

`CPC_Application::Vf0c` (0x44a3d0, `src/rbr/RBR_E04_Main.cpp:125-193`) owns
the message-loop frame. It samples/clamps wall-clock `dt`, asks
`CApplication::M0047deb0` (0x47deb0, `src/rbr/RBR_E13_Application.cpp:1914-1923`)
for a scheduler interval, counts whole intervals into `nsteps`, and accumulates
their time into `facc`. If the interval is zero, it uses `g_7c0bc8`. It updates
animation state, polls `CInputManager::TicFwd(facc)`, samples TrackIR, then calls
`CApplication::Tic(facc, nsteps)`. Thus input polling precedes the application
tick and its physics loop. The interval formula is conditional: it returns 1.0
when `m_unk724` is clear, otherwise twice `g_7eac7c` after a 0.04 threshold.
The conversion from these outer-loop values to seconds is not established by
this source alone.

In driving state 1, `CApplication::Tic` (0x4804f0,
`src/rbr/RBR_E14_App.cpp:604-630`) requires replay state 2 to be clear and the
E89 manager to report ready, then passes `nsteps` to
`TicPhysicsRecording` (0x47f420). For each inner step that function reads
`MaxTimeStep` from the loaded physics definition, advances the E16 filter,
scans the replay recording, fires the current quantized-control entry, invokes
the physics host's `Vf08(1)` step, and emits wheel force feedback
(`src/rbr/RBR_E14_App.cpp:282-324`). The `1` is the `StepForward` count; the
physics worker reads the actual integration `dt` from the same definition
field. The harness drive and `Root/physics.rbz` establish 0.007 s for this
path. I found no 0.01 s value passed as `StepForward`'s argument. The 1/100 s
figure is only the frame loop's fallback interval with no physics loaded
(0x7c0bc8); the interval `Vf0c` uses is `MaxTimeStep` itself (see "Clocks"
below and `rust/rbr-gameplay/README.md`, Tick).

Quick-race initialization resets the E16 filter and countdown fields in
`CApplication::StartQuickRace` (0x481660, `src/rbr/RBR_E14_Init.cpp:317-330`)
and loads `Physics/physics.lsp` through `ReloadPhysicsLsp` (0x47fcf0,
`src/rbr/RBR_E14_App.cpp:403-419`). When state 10's countdown expires,
`CApplication::Tic` switches to state 1 and calls `CReplayManager::RecInit`
(0x49a1d0), or selects playback state 8 (`RBR_E14_App.cpp:669-698`). `RecInit`
snapshots weather/tweak state, clears replay cursors and the current CLS, and
sets replay state 0 (`src/rbr/RBR_E19_ReplayBody.cpp:447-478`). On each
recording substep, `E19ClsScan` (0x499ff0) appends or run-length-extends the
current 9-byte CLS value in 12-byte replay entries and updates the checksum and
cursors (`RBR_E19_ReplayBody.cpp:359-403`). `E19QccFire` (0x49af10,
`src/rbr/RBR_E19_ReplayMgr.cpp:136-145`) emits the quantized-control entry at
the current replay cursor. These are the game's replay structures. Harness
FRM1 files are a separate capture: one 0x20-byte control and a substep count
per recorded frame (`rust/rbr-recording/src/lib.rs` and
`harness/plugin/RBRRecompHarness.cpp`).

`CTimeKeeper::Load` (0x4a01f0,
`src/rbr/RBR_E21_TimeKeeper.cpp:76-93`) selects the DLS `Driveline` set and
uses tags 0x15, 0x16, and 0x17 as start, finish, and split distances. During
car stepping, `CCar::Vf6cx` converts the car's driveline position and calls
`CTimeKeeper::Record` (0x4a0540; `src/rbr/RBR_E34_CarD.cpp:677-682`). The
keeper emits checkpoint events when the start gate, split list, or finish is
crossed (`RBR_E21_TimeKeeper.cpp:162-275`). The DLS ticker
`E12_AnimSet::Load3b0` (0x4783b0, `src/rbr/RBR_E12_Load.cpp:15-288`) handles
the same marker stream: tag 0x15 runs start-line/gameflow handling; 0x16 emits
a pace-note callout and normally EndRace; 0x17 advances the game-host split
and emits a callout; 0x18 emits a callout and EndRace. Other tags can trigger
pace-note callouts based on look-ahead distance and a speed threshold. Marker
layout and remaining interpretation questions are in
[`notes/formats/stage-markers.md`](formats/stage-markers.md).

On tanner-corolla, the three timekeeper functions are verified. The marker
ticker and its key-list lookup are unreached in `notes/progress/called.csv`,
so the DLS trigger behavior is a static source map, not runtime verification on
that drive. `E25PaceHandler` (0x4b65b0) has no annotation row; slot-to-note
translation and its payload semantics remain open as described in the marker
format note.

Status for the mapped path below uses `exact` for a full reccmp match,
`verified` for a harness behavior check, and otherwise `reached` or `unreached`
from `notes/progress/called.csv` on tanner-corolla. A missing call row is
reported literally; it is not proof of dead code when the source caller is
present.

| Function | Source | Status |
| --- | --- | --- |
| `CPC_Application::Vf0c` (0x44a3d0) | `RBR_E04_Main.cpp:86` | reached |
| `CApplication::M0047deb0` (0x47deb0) | `RBR_E13_Application.cpp:1915` | unreached by call log; source-called by 0x44a3d0 |
| `CInputManager::TicFwd` (0x4cc550) | `RBR_E29_Manager.cpp:792` | exact |
| `USInputManagerImpl::Tic` (0x4cbf50) | `RBR_E29_Manager.cpp:273` | reached; reviewed |
| `E04CarControlSignal::Connect` (0x449440) | `RBR_E04_Signal.cpp:9` | unreached |
| `E16Filter::NotifyB` (0x48ab40) | `RBR_E16_Filter.cpp:69` | exact |
| `E16Filter::Tic` (0x48c500) | `RBR_E16_Signal.cpp:289` | reached; reviewed |
| `E16SteerState::Pack` (0x48c910) | `RBR_E16_SteerSig.cpp:91` | reached |
| `E16SteerState::Unpack` (0x48c9c0) | `RBR_E16_SteerSig.cpp:105` | exact |
| `E16Quant::NotifyQuant` (0x48d100) | `RBR_E16_Quant.cpp:80` | exact |
| `E16Quant::NotifyFilt` (0x48d130) | `RBR_E16_Quant.cpp:88` | reached; reviewed |
| `CSigCarControl::Emit` relay (0x48cd90) | `RBR_E16_SteerSig.cpp:414` | unreached; reviewed statically |
| `CApplication::Tic` (0x4804f0) | `RBR_E14_App.cpp:607` | unreached by call log; transplant skipped for unresolved references |
| `CApplication::TicPhysicsRecording` (0x47f420) | `RBR_E14_App.cpp:282` | verified |
| `CPhysics::StepForward` (0x5273a0) | `RBR_E46_Physics.cpp:295` | verified |
| `CReplayManager::RecInit` (0x49a1d0) | `RBR_E19_ReplayBody.cpp:448` | reached |
| `E19ClsScan` (0x499ff0) | `RBR_E19_ReplayBody.cpp:362` | reached |
| `E19QccFire` (0x49af10) | `RBR_E19_ReplayMgr.cpp:136` | reached |
| `CTimeKeeper::Load` (0x4a01f0) | `RBR_E21_TimeKeeper.cpp:77` | verified |
| `CTimeKeeper::Check` (0x4a0450) | `RBR_E21_TimeKeeper.cpp:163` | verified |
| `CTimeKeeper::Record` (0x4a0540) | `RBR_E21_TimeKeeper.cpp:218` | verified |
| `CCar::Vf6cx` (0x4ea300) | `RBR_E34_CarD.cpp:582` | verified |
| `E12_AnimSet::Load3b0` (0x4783b0) | `RBR_E12_Load.cpp:16` | unreached |
| `E25_SlotId` (0x4b55e0) | `RBR_E25_PaceA.cpp:43` | reached |
| `E25Handler::M5f70` (0x4b5f70) | `RBR_E25_PaceB.cpp:225` | unreached |
| `E25Handler::LoadNote` (0x4b69a0) | `RBR_E25_Handler.cpp:356` | reached |
| `CScratchSim::ReadPacket` (0x537f20) | `RBR_E50_Sim.cpp:500` | exact |
| `CScratchSim::GearboxUpdate` (0x537160) | `RBR_E50_Sim.cpp:127` | verified |
| `CDrive::M5970` (0x505970) | `RBR_E40_LeafA.cpp:48` | verified |

## Rust main loop

The Rust application should own a per-frame accumulator and gameplay session
state; it should keep wall/frame time distinct from the fixed physics `dt`.
The following order is proven by the source path above unless marked open:

1. Sample player input once before the application tick, map configured axes
   and buttons to the raw input signal, then reproduce the E16 filter and
   quantization that emits the 0x20-byte car control. Proven at
   `0x44a3d0`, `0x4cc550`, `0x4cbf50`, `0x48ab40`, `0x48c500`, `0x48c910`,
   and `0x48d130`; exact Rust device/config mapping remains open.
2. Accumulate frame time into whole scheduler steps and pass the step count to
   the game tick. Proven at `0x44a3d0` and `0x4804f0`. The interval is the
   loaded `MaxTimeStep` (0.007 s), read through `E13Get9B70F` (0x47e7b0); the
   physics `dt` is the same value.
3. For each permitted gameplay substep, update filtered control, replay state,
   then physics with a count of one; update wheel feedback after the physics
   step. Proven at `0x47f420` and `0x5273a0`. The precise ownership split for
   replay input versus live input needs a dedicated replay playback run.
4. During car stepping, update stage position and timekeeper state. The DLS
   marker ticker also dispatches start, split, finish, and pace-note events,
   but its exact order relative to `StepForward` is open. Static source
   evidence: `0x4e91f0`/`0x4ea300`, `0x4a01f0`, `0x4a0450`, `0x4a0540`, and
   `0x4783b0`; marker ticker execution on the reference drive and exact split
   payload meaning remain open.
5. Own replay recording state and per-step CLS/quantized-control cursor data;
   capture harness FRM1 controls separately from the game's replay format.
   Static evidence: `0x49a1d0`, `0x499ff0`, `0x49af10`; comparison of a saved
   retail replay against these structures remains open.

## Clocks: wall time, frames and steps (clocks-1, 2026-10-17)

`Vf0c` (0x44a3d0, the asm; `rust/rbr-gameplay/src/tick.rs`) reads the wall
timer once per frame, cuts the gap to 0.1 s (0x795e30) and splits it into
whole steps of `MaxTimeStep` (0.007 s), carrying the rest to the next frame.
`nsteps` is 0 to 14 and `facc` is their time (the f64 sum rounded to f32).
Everything that keeps time reads one of the two:

- the physics: `Tic` runs `nsteps` `StepForward(1)` calls, each 0.007 s of
  simulation time (the time card's clock advances by `MaxTimeStep` per step);
- the countdown of state 10: the camera's start orbit (`M8840`, angle
  -= `facc` * 1.2, the go below 0) and `Tic`'s countdown, both by `facc`;
- the HUD's race clock (the 7 s start countdown, then the race time) by
  `facc`, once per frame after the steps (`tests/race_clock_peek.rs` shows a
  tick of whole steps' time per frame).

So in the game every clock is wall time quantized to 7 ms steps, and the
physics runs about 142.9 steps a second. A frame shorter than a step has
`facc` 0 and moves no clock. The recordings cannot show the frame pacing: the
harness hooks `StepForward`, which `TicPhysicsRecording` calls once per substep,
so every driving frame of every recording reads one substep (the four settle
calls read 40).

Live play (`rbr-render` `live`, `rbr-standalone --live`) sends one payload per
step of wall time, each a game frame of one step (`facc` 0.007): the game at a
frame per step. Countdown payloads run no step and tick the orbit by 7 ms, so
the go comes after 375 payloads, 2.625 s. The viewer, the HUD model, playback
and the presentation models take 7 ms per substep (`rbr_recording::STEP_SECONDS`).
Until 2026-10-17 they took 10 ms, so live play ran the game at 0.7 times real
speed while the viewer's drive time ran at wall speed.

## Reached loop candidate verification

The tanner-corolla bisector originally selected these 13 reached, non-exact
loop-range functions. The two startup ABI failures at 0x47fcf0 and 0x4800c0
were fixed and verified after the merge; each ran once. Six prior transplant
skips are now eligible: three replay-global users, two `/GS` cookie users, and
the setup constructor whose compiler-generated wrapper calls were removed.
The fresh transplant check accepted all six; four verified, and single-function
runs stalled at 0x481660 and 0x4825b0. `harness/diverge.py` now reports
empty/short runs as `short run: X of N steps` and exits 2.

| Address | Function | Source | Current status | Verdict |
| --- | --- | --- | --- | --- |
| 0x449570 | `CMultiSignal1<CSigInit>::Emit` | `RBR_E04_Signal.cpp:73` | reached, non-exact | verified; 1 call |
| 0x449650 | `CMultiSignal1<CSigInput>::Connect` | `RBR_E04_Signal.cpp:107` | reached, non-exact | verified; 5 calls |
| 0x4496e0 | `CMultiSignal1<CSigMenuActivate>::Connect` | `RBR_E04_Signal.cpp:137` | reached, non-exact | verified; 3 calls |
| 0x47ed20 | `CDrive::CSetup::CSetup` | `RBR_E13_Application.cpp:1049` | reached, non-exact | verified; 6 calls |
| 0x47f370 | `CApplication::SetAppState` | `RBR_E14_App.cpp:96` | reached, non-exact | verified; 5 calls |
| 0x47fcf0 | `CApplication::ReloadPhysicsLsp` | `RBR_E14_App.cpp:423` | reached, non-exact; file-service slot 8 uses its thiscall stack argument | verified; 1 call |
| 0x4800c0 | `CApplication::Init` | `RBR_E14_App.cpp:471` | reached, non-exact; race-host slot 4 receives `m_view` on the stack | verified; 1 call |
| 0x4801d0 | `CApplication::M004801d0` | `RBR_E14_App.cpp:505` | reached, non-exact; replay alias mapped; host slots use the original receiver and stack arguments | verified; 26,454 calls |
| 0x481530 | `CApplication::PostPlatformInit` | `RBR_E14_App.cpp:822` | reached, non-exact; replay alias and service/race-host call ABIs mapped | verified; 1 call |
| 0x4825b0 | `CApplication::M004825b0` | `RBR_E14_App.cpp:1140` | reached, non-exact; original caller at `0x44a770` pushes `edi` (`dt`) and sets ECX to the application, matching the source | original state-6 and state-0xc startup transitions restored; runtime verification pending |
| 0x481660 | `CApplication::StartQuickRace` | `RBR_E14_Init.cpp:318` | reached, non-exact; receiver/arguments and pre-call member setup match the original assembly; details below | source order fix and action-argument fix rebuilt; combined runtime verification pending; prior loader stall cause open |
| 0x47fd60 | `CApplication::LoadCarLsp` | `RBR_E14_Init.cpp:445` | reached, non-exact; `/GS` cookie mapped | verified; 1 call |
| 0x47f770 | `CCompleteCarSetup::CCompleteCarSetup` | `RBR_E14_Setup.cpp:32` | reached, non-exact; raw setup storage and +0x120c pad preserve documented member offsets | verified alone; 6 calls; EH metadata copied |

The three replay-global users now reference the canonical `g_e19Ptr893634`;
the two `/GS` users carry the existing `__security_cookie` GLOBAL annotation.
`StartQuickRace` calls `E19Nop61e190` with the replay manager in ECX, passes the
selected car to `LoadCarLsp`, and calls the E53-owned
`LoadQuickRaceSetup(car, surface, replayState)`; `LoadQuickRaceSetup` and
`E19Nop61e190` each reccmp at 100%. `CCompleteCarSetup` uses direct constructor
calls into raw member storage so the compiler no longer emits unmatched setup
wrapper calls in its EH cleanup. Its isolated transplant verifies with 6 calls
and its EH handler copied, though its current reccmp score is 55.1%. In a
two-function run with 0x4825b0, its address appeared in the stall stack, but its
standalone run completed; 0x4825b0 also stalled by itself.

`CApplication::Tic` (0x4804f0) remains outside this candidate set because its
transplant has other unresolved references. Do not treat that absence as proof
that the original-game call path is dead; the source caller above reaches it.
`TicPhysicsRecording` (0x47f420) is already verified.

The original `StartQuickRace` assembly at `0x4818f4` clears the application
member at `this+0x64` after `LoadQuickRaceSetup` and before resetting the HUD.
The reconstruction omitted this write; it is now represented by `m_unk64 = 0`.
At `0x481906-0x48190f`, the original resets the HUD, loads the E16 save-manager
receiver from global cell `0x7c28cc`, pushes the selected car from
`[ebp+0x10]`, and calls `LoadTyreBrand` at `0x48df00`. The source uses the same
annotated global (`DAT_007c28cc`, whose runtime value is `0x7ecd10`) and car
argument `c`. The fresh singleton rerun still stalled before the first step.
The original assembly's argument setup is: `esi = [ebp+0x10]` (car `c`),
`edi = [ebp+8]` (surface `a`); call `LoadQuickRaceSetup` through
`[0x7cf398]` with `(car, surface, replayState)`, clear `[this+0x64]`, reset
the HUD through `[0x165fc68]`, then call `LoadTyreBrand` through
`[0x7c28cc]` with the same car. The source uses those same objects and values.
Before the tyre call, the direct stores are to application fields only:
`+0x714=0.0`, `+0x718=0`, `+0x734=car`, `+0x730=surface`, `+0x728=5`, and
`+0x64=0`; there are no direct absolute-global stores in this function before
the call. The preceding global reads and side effects are through the file,
view, signal, race, gameflow, settings, and HUD objects described by the
matching source calls. In original `LoadTyreBrand`, ECX receives the
`[0x7c28cc]` object and is passed to `0x48d9f0`; that file-loader body replaces
ECX with the global file-service pointer before using it. This confirms the
receiver and path-argument setup but does not explain the stall. The updated
`LogStall` capture sampled EIP `0x7b3b1248` eight times; its stack includes
`[0x2c0] 0x0048df68` (the return address in original `LoadTyreBrand`) and
`[0x3d0] 0x02ca03a3` (the transplanted `StartQuickRace` copy at `+0x3a3`).
This suggests a wait beneath the tyre loader, but does not identify why the
file-service path is waiting.
For `M004825b0`, its original caller at `0x44a770-0x44a773` pushes `edi` as
the time delta and puts the application receiver in ECX, matching
`app->M004825b0(facc)`; its isolated rerun also stalls before any reference
step is recorded, with no divergence sample. `LogStall` sampled EIP
`0x7befe0e0`, `0x00470dc1`, then `0x7befe0e0` for the remaining samples. Its
stack begins `[0x42c] 0x0044aa9c`, `[0x478] 0x0047bd60`, and
`[0x4bc] 0x0049408d`; no clear frame identifies the stalled inner call. The
The source comparison found that the copied switch had empty cases 6 and 0xc
while the original performs race-host, weather, and view setup in case 6, then
waits for race-host view readiness in case 0xc before transitioning to driving
state 3. `CApplication::Init` enters state 6, so the missing transitions
prevented quick-race startup. Both paths are now reconstructed in
`RBR_E14_App.cpp`; action 1 also passes the original `[0x7eac6c]` fourth
argument to `StartQuickRace` instead of zero. The refreshed build accepts both
`0x4825b0` and `0x481660` for transplant. Combined runtime verification is
pending because the single harness slot is currently occupied.

Additional regression spot-checks:

| Address | Result | Note |
| --- | --- | --- |
| 0x47f890 | unreached; static-only | Setup teardown is not reached by the tanner drive's direct stage-start path. `harness/play.sh` auto-starts a stage; menu recording is interactive and has no unattended startup mode. |
| 0x47c7c0 | unreached; static-only | Same startup-path limitation; no behavior verdict from this run. |
| 0x43d300 | verified; 23 calls | Completed the tanner-corolla drive. |
| 0x445380 | skipped | Transplant reports `0x400023: not in our image file`. |

The CBase-vtable follow-up keeps `NPhysics::NTweak::CBase`'s `VTABLE` annotation
at `0x72e444`. E44 and E50 teardown code now writes that address directly for
base demotions, and the `g_demVtab` compatibility alias and its cross-TU
references are gone. The CWheel tyre teardown calls use the paired
`CTyre::CState`/`CTyre::CDef` destructors at `0x532550`/`0x5324a0`, rather than
unresolved E49 proxy symbols. CBase and E14 raw-wrapper size checks guard the
subobject layouts.

Static transplant verification of wheel teardown:

| Address | Transplant result | CBase-vtable stores | Runtime coverage |
| --- | --- | --- | --- |
| 0x538560 | copied; 462 bytes plus 328 bytes of support, 151 fixups total for both functions | 9 immediate `0x72e444` stores retained in the copied blob | unreached on the counted tanner drive; stage-load/teardown path |
| 0x538b70 | copied; 1,523 bytes plus 907 bytes of support | 33 immediate `0x72e444` stores retained in the copied blob | unreached on the counted tanner drive; stage-load/teardown path |

These addresses are listed as `stage-load-teardown` in
`notes/progress/unreached.csv`, so no drive bisect applies. The literal vtable
address is present in the transplanted code bytes; `transplant.py` records the
paired tyre destructor calls as relocations to their original addresses.
Against `../lead/m91-RBR.txt`, the final build reports exact functions
9,638 -> 9,638, compared functions 20,367 -> 20,367, with 0 drops.

The full-exe build and reccmp refresh on the merged integration branch
succeeded. Against the pre-batch m88 report (`../base/RBR.txt`),
`check-batch.py` reports exact functions 9,633 -> 9,638 and compared functions
20,366 -> 20,367. The only score drops are:

| Address | Score | Reason and behavior evidence |
| --- | --- | --- |
| 0x47f770 | 68.33% -> 55.07% | Raw setup storage, explicit constructors, and restored padding remove unmatched compiler-generated wrapper calls; verified alone with 6 calls and EH metadata copied. |
| 0x47f890 | 63.49% -> 56.41% | Typed CBase teardown changes the destructor code shape; unreached on the direct-stage drive, with teardown order/offsets supported by original assembly; static-only because the menu route is interactive. |
| 0x481530 | 63.09% -> 62.67% | Replay/service/race-host ABI corrections are a behavior fix; singleton verified with 1 call. |

`0x43d300` (74.42%) and `0x445380` (41.07%) are unchanged against m88:
`0x43d300` verified with 23 calls; `0x445380` is skipped because target
`0x400023` is not in the image. The duplicate annotation is resolved; the
batch check still lists 12 pre-existing unpaired annotations.

`harness/plugin/RBRRecompHarness.cpp` now opens the game thread with only the
query/context/suspend rights and logs the thread id or Win32 failure code. This
made `LogStall`'s EIP and stack capture appear in both singleton run logs. The
updated plugin built successfully on Cachy; the runs still stall and need
further call-path analysis.

## Stage state machine (G7, 2026-10-15)

- The application pointer is the global at 0x7eac48 (`DAT_007eac48`). The `CApplication` allocation (0x1bd0 bytes) at
  0x473779 stores to it. The state is at +0x728, the countdown `m_unk714` at +0x714 with its armed flag `m_unk718` at
  +0x718, the load countdown `m_unk72c` at +0x72c, and the quick race's track and car at +0x730 and +0x734.
- The load starts in `M004825b0` (0x4825b0, the state switch called from `Vf0c` at 0x44a773), state 5: when the race
  host reports ready (`M4a1430`, asm 0x482ba2), it writes 0x96 to +0x72c and sets state 0xd. This is the copied switch's
  case 5 (`RBR_E14_App.cpp`). The asm agrees with the note on 6 and 0xc: state 6 (0x482cb8) does the race-host, weather
  and view setup, and state 0xc (0x482f11) waits for the view, then sets state 3. The C++ copy in this branch still has
  empty cases 6 and 0xc.
- The settle calls are the 0xd branch of `M004801d0` (0x4801d0, asm 0x4802ec: the single-step gate `M0047cc30(0x28)`,
  then `StepForward` through the physics vtable slot +8). `Vf0c` calls `M004801d0` each frame at 0x44a6eb, before `Tic`.
  The countdown drops by each call's substeps after the call, so the probes see 150, 110, 70, 30 before the four calls.
- State 0xd ends at 0 by setting state 10 (the sound gate `M00544e60` is not modelled). The go is `M0047ca40`
  (0x47ca40), called from the camera host `E15CamHost::M8840` when its angle timer runs out; the timer is wall time.
  `Tic`'s state 10 subtracts the frame's `dt` and sets state 1 below 0.
- `tests/stage_peek.rs` checks these against `peek/g7-tanner-corolla` (4 load frames, 12,080 driving frames, every step).
  The finish and the end key are not in that run (see `rust/rbr-gameplay/README.md`, "Stage (G7)").

## Input filter (G5, 2026-10-15)

- `E16Filter` (app +0x7e0) and the quantizer's nine bytes (app +0x738 +0x74, app +0x7ac) are peekable through the app
  pointer (0x7eac48). The filter's per-channel curve and rate grid comes from `PC_InputFilter.ini` (`E16_LoadINI`) and
  the quick race's `E16_Reset` (which recomputes the active and nonlinear flags from the values); a saved profile grid
  (`E16Filter::ReadBin`) would replace it and the profile is encrypted.
- `E16SteerState::Pack` (0x48c910) reads the filtered channels at +0x14 to +0x24 (steering, then channels 1 to 4 from raw
  +0x18 to +0x24): byte 4 is channel 1 (filtered +0x18), byte 5 channel 2 (+0x1c). The table above names byte 4 "throttle",
  which the car confirms (word 0 is the throttle); the poll's names for raw +0x18 and +0x1c are the other way round.
- The playback (`harness/plugin`, `OnPlayerControl`/`CallEmit`) emits the recorded control after the filter, so a playback
  run's filter sees the player's idle input. `tests/filter_peek.rs` (`rust/rbr-gameplay`) checks the grid at every step and
  the filter on the run's zero input; the payload against the recording differs from step 161. The curve, rates, fade and
  steering byte are checked by fuzzed direct calls of `Step` and `Pack` (`tests/filter_vectors.rs`,
  `harness/vector_specs/gameplay_filter.py`).
