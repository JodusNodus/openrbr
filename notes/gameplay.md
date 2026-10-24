# Gameplay

The 1.02 gameplay around the physics step: input, the frame loop, the race flow, the options, the weather, the
scripted events, the championship and the saves. Code: `crates/rbr-gameplay/src/` (module map:
[crates/rbr-gameplay/README.md](../crates/rbr-gameplay/README.md)); the module doc comments carry the per-function
detail. File layouts are in [formats.md](formats.md); cameras and HUD in
[cameras-hud-launcher.md](cameras-hud-launcher.md). Addresses are `RichardBurnsRally_SSE.exe` 1.02. Detail dropped
here: rbr-recomp `notes/gameplay-loop.md`, `notes/scripted-events.md`, `notes/weather.md`.

## Frame and fixed step

- `CPC_Application::Vf0c` (0x44a3d0, `tick.rs`) per frame: read the wall timer, clamp the gap to 0.1 (0x795e30, the
  f32 0.1 widened), split into whole steps of `MaxTimeStep` (`step <= dt` in f64), carry the rest. `facc` is the f64
  sum rounded to f32 once. `nsteps` is 0 to 14.
- The step is `PhysicsEngine.MaxTimeStep`, physics definition +0x1608c, read through `E13Get9B70F` (0x47e7b0): 0.007 s
  from `Root/physics.rbz` (the CHELALIC `physics.dat` copy says 0.01 but is not the loaded file). Fallback with no
  physics: 0.01 (0x7c0bc8).
- Order in a frame: settle (`M004801d0`), input poll (`TicFwd` 0x4cc550), `CApplication::Tic` (0x4804f0), then
  `M0047ca70` (the physics' `Vf0C` 0x526a10, each entity's `Vf28`, which runs the marker ticker), then
  `M004825b0` (state switch, camera, HUD tick).
- `TicPhysicsRecording` (0x47f420) per substep: filter `Tic`, replay scan and fire, `StepForward(1)`, wheel force.
- Every clock is wall time quantized to 7 ms steps (about 142.9 steps/s): physics, the countdown, the camera orbit
  and the HUD race clock all take `facc`. Live play sends one payload per step (`facc` 0.007); the viewer takes
  `rbr_recording::STEP_SECONDS` (7 ms) per substep.
- Load settle: four `StepForward(40)` calls in state 0xd. Recordings show them as the first four frames, then one
  substep a frame (`tests/schedule.rs`, all four drives).

## Control payload

Input path: `USInputManagerImpl::Tic` (0x4cbf50) polls devices into `CSigCarControlInput`; `NotifyB` (0x48ab40)
copies raw +0x14..+0x24 to filter channels 0-4; `E16Filter::Tic` (0x48c500) filters; `E16SteerState::Pack`
(0x48c910) quantizes to nine bytes; `E16Quant::NotifyFilt` (0x48d130) emits the 0x20-byte `CSigCarControl`
(`control.rs`). Eight 32-bit words; FRM1 records this payload (`tests/recordings.rs`).

| Word | Meaning | Producer | Consumer |
| --- | --- | --- | --- |
| 0, +0x00 | Throttle, 0 to 1 (measured: 1.0 speeds the car up) | Filter channel 1 (filtered +0x18, from raw +0x18: the poll's THROTTLE, action 3, or the combined axis's positive part); `Pack` byte 4; `NotifyFilt` word 0 | `CScratchSim::ReadPacket` (0x537f20) to scratch +0x0c (`m_throttle`). The VCU swaps the pedal lanes in reverse/neutral or when brake exceeds throttle (`state.pedal_swap`, `crates/rbr-physics/src/vcu/`) |
| 1, +0x04 | Brake, 0 to 1 (measured) | Filter channel 2 (filtered +0x1c, raw +0x1c: BRAKE, action 5, or the combined axis's negative part); `Pack` byte 5; word 1 | `ReadPacket` to scratch +0x10 (`m_brake`) |
| 2, +0x08 | Handbrake, 0 to 1 | Raw +0x20, channel 3, byte 6, word 2 | `ReadPacket` to scratch +0x14; `CDrive::M5970` (0x505970) reads it into its handbrake lane +0x4c, scaled into +0x80 |
| 3, +0x0c | Clutch, 0 to 1 | Raw +0x24, channel 4, byte 7, word 3 | `ReadPacket` to scratch +0x1c, used by `CScratchSim::Step` (0x537f50) for the drive clutch lane |
| 4, +0x10 | Steering, signed float stored as bits | Channel 0 (filtered +0x14); `Pack` signed byte 3; `Unpack` (0x48c9c0) `byte * 2 / 255` | `CCar::UpdateSeed` (0x4e91f0) to the car control state; `ReadPacket` keeps it at scratch +0x18 |
| 5, +0x14 | Flags (integer) | `NotifyFilt` maps input bits 0, 1, 2, 0x20, 0x40 to 0, 1, 2, 0x10, 0x20; the poll sets input bit 0 for action 7, bit 1 for action 8 | `CScratchSim::UpdateD` (0x537a80): bit 0 gear up, bit 1 gear down, bit 2 passes word 6 to `CDrive::M6840` (0x506840). `UpdateSeed`: 0x10 call for help, 0x20 reset |
| 6, +0x18 | Gear selection (integer) | Written only when flag bit 2 is set, from byte 8; otherwise uninitialized | `UpdateD` to `CDrive::M6840`, which clamps it into drive +0x570 |
| 7, +0x1c | Aux bit (integer) | `b1 & 1`; the poll sets raw bit 0 from action 12 | `CDrive::M5970` copies it to drive +0x50 |

Pedals quantize to 1/255 (`x * 255 + 0.5` truncated, each op in f32: the x87 runs at 24-bit precision), steering
`(s + 1) * 127.5 - 128` truncated.

### Device poll

`rbr_gameplay::poll` (module doc has the detail), from the exe's asm and NGP 7.4.778/7.5.779; replayed bit-exact
over RSF sessions (`notes/verification.md`, "Input chain check").

- Devices: `CInputDevice` (0x4ca7e0, `EnumObjectsCb` 0x4ca440) sets its own format, axis DWORD, other objects one
  byte, entry `dwType` the enumerated one; the keyboard keeps `c_dfDIKeyboard`, entries `DIK << 8 | 4` (0x4ca486).
  Only `GetProperty(DIPROP_RANGE)`; the game sets no property. A failed `GetDeviceState` (`Poll` 0x4c8500) keeps the
  values.
- `E29AxisEntry::Update` (0x4cad30): axis `((raw - lo) / (hi - lo)) * 2 - 1` (SSE f32), button 1/0. Counter +0x2c
  starts -1, the first update sets 1, each change of 0.001 or more (0x795e94) counts, past 2 it is 0;
  `GetActionValue` gives 0 for an axis while it is not 0 (`Bound` 0x4cad20).
- Actions (21 slots, `.acm` `FindRec` 0x4c4b50: first record with as many devices as there are, all present;
  `ApplyMaps` 0x4c4ee0: entry with `dwType` equal to the binding's): `GetActionValue` (0x4c2610) negates an axis for
  actions 3, 4, 5, 6, 11 (`InitImpl` 0x4c28c0) and maps it `(v + 1) * 0.5` for 1, 2, 3, 5, 6, 11 (table 0x4c26c0);
  `IsActionDown` (0x4c25d0) = value > 0 and `Changed` (0x4cacb0: button |d| >= 0.2, axis |d| / rate >= 1.25).
- `Tic` (0x4cbf50): combined axis (4) split into +0x1c (negative) and +0x18 (positive); BRAKE (5) and THROTTLE (3)
  replace them when larger, cell 0 for an axis else 3; `input.ini` `Invert*Axis` (+0x254..+0x268, all off in RSF);
  HANDBRAKE (6) +0x20, CLUTCH (11) +0x24; steering action 0's axis else RIGHT - LEFT (cell 3); flags: down(7) bit 0,
  down(8) bit 1, down(12) aux bit 0. Emit 0x4cc42e (`this+0x1c8`).
- NGP: IGNITION is a level, `GetActionValue(12) > 0.99` (778 0x10006e50, 779 0x10006e70); the emit wrapper
  (778 0x100ed540) adds `control::ngp_direct_gears` (778 0x1003c450, 779 0x1003c4d0, same logic) with the automatic
  gearbox off. A 1000 Hz thread polls and posts each control to a strand (0x10098270 -> 0x10094200) that the physics
  thread runs once per burst of steps: a payload takes the last poll before its burst. The handler (0x1008e8a0) drops
  a selection of the gear the drive already commands.
- RBRControls (RSF) sets properties and FFB on device instances of its own (MOD1: PRP1 9, DEV1 5, no POL1); the
  chain replays exactly without it.
- Live (`rbr_render::devices::Input`, the viewer's input thread `window/feed.rs`): every drive, at the bindings'
  `input rate` (1000 Hz, RSF's `inputRate`), polls the keyboard (the held keys as DIK state) and the bound joysticks
  through `Poll::tic` over our bindings (`rbr_render::bindings::Bindings::slots`: one object per driving slot, pad
  over key; camera and pause take both), then `ngp_direct_gears` under NGP. A device is vendor:product plus its index
  among the joystick nodes of that id (ABS_X and ABS_Y, or a key in `BTN_JOYSTICK..BTN_DIGI` or `BTN_TRIGGER_HAPPY`
  up), its objects in DI order; on Linux read through evdev (`EVIOCGABS`/`EVIOCGKEY`; a failed read keeps the last
  state, the node reopened on the 50 ms keepalive), elsewhere through gilrs-core. Under Proton the game's devices
  come from winebus's SDL backend: SDL scales evdev from its `absinfo` (`floor((v - min) * (65535 / (max - min)) -
  32768 + 0.5)` in f32), dinput maps -32768..32767 to `+ 32768`; axes in ABS code order (digital hats left out),
  buttons from `BTN_JOYSTICK` up then below (checked: RSF's G923 pedal values are exactly that lattice for Y 25..255,
  Z 13..255, RZ 51..255; FMT1 layouts match the evdev capabilities). An axis reads 32768 until it first changes, a
  device 32767 until its first report (the T500 RS). Checked on cachy (autopilot run, rec/input-own): the G923 and
  the T500 RS found and read at rest (0, gears bound). The keyboard's state is the frame's held keys, so a key tap
  shorter than a frame is missed (unverified in practice).
- The launcher's "Import from RSF" (`rbr-launcher/src/rsf_import.rs`) turns the newest `.acm` into the bindings once
  (GUIDs to vendor:product:index through the prefix's `user.reg`, `rbr_formats::wine`; the record naming exactly the
  connected joysticks, as `FindRec`, else the one naming the most), with the rest of RSF's settings
  (`notes/ngp.md`, RSF settings).

## Input filter

`filter.rs`, `ini.rs`. Grid: 5 channels x 4 cells, stride 0x94 (selector word, then four cells of nine words: active,
nonlinear, linear, quadratic, cubic, rise, fall, speed fade, fade speed). The quick race starts from `E16_InitGrid`
(zero, linear 1.0), `E16_LoadINI` (`PC_InputFilter.ini`: steering cells 0 `AXIS`, 1 `STICK`, 3 `BUTTON_DIGITAL`),
then `E16_Reset`, which recomputes the active flags from the values (so the AXIS cell is active despite
`Active = false`). An RSF install renames the file to `PC_InputFilter.ini.bak`, so there the built-in grid applies, as
in RSF's own game: digital steering at a tenth of the retail file's rates. A profile's grid replaces it:
`E20ReadProfile` reads it whole with `ReadBin` (0x48bd60, at 0x49eb38), then the race's `E16_Reset` (at 0x4816cc).
Live play takes it from `--saves`/`--profile` (`Grid::from_profile`) on the NGP path only, with no AXIS fade override;
the 1.02 path keeps the INI grid and the override (ours). A new profile holds `ResetAll`'s (0x586150) grid,
`InitGrid`+`LoadINI` of the install's INI (`Grid::loaded`; `ProfileFile::fresh` the built-in values): the drives'
`pfthomas.rbr` does (`tests/filter_peek.rs`), and FRM1 holds the payload after the filter, so `--boot` never runs it.
The launcher's old made-up grid (every cell linear 1.0, rates 0.3/0.5/0.5, speed 27, select = channel) is read as no
grid (`Grid::is_made_up`). `Step`'s curve is
`((c3 * m2 + c2) * m2 + lin * m) * sign`. The speed fade input `m_36c` comes from `NotifyA` (0x48aa10,
`filter::notify_a`). Checked: grid and zero-input path per step (`tests/filter_peek.rs`), curve and `Pack` on fuzzed
direct calls (`tests/filter_vectors.rs`, `harness/vector_specs/gameplay_filter.py`).

## Stage states

`stage.rs`. The app is `*0x7eac48` (`CApplication`, 0x1bd0 bytes): state +0x728, countdown +0x714 with armed flag
+0x718, load countdown +0x72c, track +0x730, car +0x734.

| State | Meaning |
| --- | --- |
| 5 | `StartQuickRace` (0x481660) set-up; on the race host's ready, load countdown 0x96 and state 0xd |
| 6, 0xc | Race-host, weather and view setup; wait for the view, then 3 |
| 0xd | Load settle: 40-substep calls until the countdown reaches 0, then 10 |
| 10 | Countdown: in modes 1 and 2 the camera's start orbit gives the go (`camera_go.rs`, after `pi / 1.2` s, the 375th frame of 7 ms; driving from `Tic` on the 376th); in modes 3-5 the orbit turns at 0.3 rad/s and the player's `CSigMenuActivate` arms 2.0 s (`M0047f9c0` case 10, 0x47fc25; `Stage::confirm`). `Tic` takes `dt` off and below 0 sets 1 |
| 1 | Driving |
| 2 | Pause menu or message box (no physics) |
| 8 | Replay playback |
| 9 | End page (no physics; animations and helicopter tick) |

Checked every step of `peek/g7-tanner-corolla` (`tests/stage_peek.rs`). Recorded runs keep the host's go;
`Game::with_camera_go` times it in live play, where the viewer's accept (`ACCEPT_MESSAGE`) is the championship's
confirm and a recorded payload confirms at once. The mode rule (`M8840` 0x4888bf) and the 2.0 s are asm only: no
recording starts a championship stage.

## Timing and markers

- Markers (`markers.rs`, `Load3b0` 0x4783b0) walk the `.dls` `Driveline` keys once per frame after the substeps: 0x15
  start line, 0x16 finish (callout, then the HUD's finish), 0x17 split, 0x18 end of stage (`CSigEndRace` at once).
  Pace notes fire inside a speed-based lookahead: callout distance 0x1660b74 (4.0) and speed gate 0x1660b80 (25.0),
  profile values. Forward plain keys with flag bit 0 clear fire `CSigTrigger` (start, finish and bird items).
- The time card (`timing.rs`, `CTimeKeeper` 0x4a01f0..0x4a0540): last start key, last finish key, up to 16 splits.
  `Record` runs inside the car's step, before that substep's clock advance, on `sample_x(car +0x17c)`. Where the C++
  and asm differ the asm is followed (`Check` stores the time, not the distance).
- The clock: `m_d20 = (double)((float)dt + (float)m_d20)` from 43185.0, so each 0.007 step adds 2/256 s. Plain f64
  fails at step 40. Checked on every field, every step of two drives (`tests/timing_peek.rs`).
- tanner-corolla: start at step 1, splits at 11139 and 21664, finish 34530, end key 35404 (`tests/timing.rs`).
- Fired-bit counts (one `Load3b0` call a frame, `Dot150` as `d`) equal the original's snapshots at all six
  checkpoints of tarmac-lancer, arctic-snow and tanner-damage (asserted by the removed `tests/game_loop.rs`, which
  needed 1.02's f32 clock). Between them unchecked: the snapshots' key list is not read.

## Race clock and start

`race_clock.rs`: the HUD's clock (`E62GameMenu`, `*0x165fc68`), the time the result is taken from. Ticked once a
frame after the substeps with `facc`. Checked bit for bit on three peeked assist drives (`tests/race_clock_peek.rs`).

- Countdown +0x244 starts at 7.0 s. Below 6.0 s it fires `start_animation2` once (the countdown marshal). Crossing 0
  is the go: +0x144 = 0 and the clock +0x140 runs, the go frame's `dt` included.
- Jump start (`M579430`, once per race): the clock takes the countdown's rest plus 10 s and the countdown ends.
  Triggers: the start line key 0x15 while counting down, or speed over 1 m/s in the last 4 s (`M579590`). In a rally
  (modes 3, 5) the penalty is the season's by the false starts so far (10, 15, 30, 600 s; `IdxFloat` 0x5d0810), and a
  fourth false start excludes the player (`CSigEndRace` with 1, 0x5794f5).
- Outside assistance (`CSigOutsideAssistance`, code 1 car not on its wheels, else 2): the timer +0x270 restarts; at
  1 s the penalty is added while the clock runs (table in Race options). The physics time card never sees it.
- Call for help: HELPCALL (0x56e6d0) is offered when the assistance timer is outside `[0, 8.5)`, the helicopter is
  idle, `*0x166e378` +0x844 <= 0 and the clock is at least 2.0 s (predicate 0x56a620). It sets quantized flag 0x20,
  which the next substep's payload carries once as 0x10.

## Race end

`flow.rs`, `result.rs`, `pause_menu.rs`. Flow word (`*0x165fa90` +0) modes: 1 quick rally, 3 championship, 7 school
(2, 4-6, 8, 9 out of scope). Sub-state +4 is 3 in a championship, else 11.

- Finish key: the clock stops (+0x144) and a 20 s timer runs; then `CSigEndRace` with 0. The car drives on in state
  1 meanwhile (tanner-corolla equal through these 874 steps). Modes outside 1-5 and 8 end at the finish at once.
- End key 0x18: `CSigEndRace` at once. Time limit: 1800 s on the clock with the helicopter idle ends with 1.
- Finish zone (`FinishZone`, `E12_Rep::CbB50` 0x478b50): `CCar::Vf28x`'s `CSigCarGeneralState`, once a frame before the
  marker ticker, ends the race finished (state word 0, the HUD's clock and splits) when the car is at 1 km/h or less
  (velocity length times 3.6) inside the 3D sphere of `.dls` table 8 (`Load020` 0x47a419: xyz, radius squared;
  without the table (2473, -3261, 10) radius 3). Retail zones: 61 of 86 `.dls`, radius 10-30 m, 66-203 m of driveline
  past the finish line, at most 93 m before the end key. Of the recordings only tanner-corolla enters one (track-54_E,
  steps 35172-35353, after the finish at 34530), never slower than 1 km/h: unverified against the original.
- Full replay buffer (`ReplayRuns`): `E19ClsScan` (0x499ff0) keeps runs of equal nine-byte controls (at most 0xffff
  steps each); on the step whose run index is 0x20f56 `E19QccFire` (0x49af10) emits no control, the step runs, and
  `TicPhysicsRecording` (0x47f4fb) emits `CSigEndRace` with 1 and zero times. A control changing every step fills it
  after 945 s. NGP replaces both calls (0x47f4c3, 0x47f4ce) with a growing buffer: no limit. No recording reaches
  it: unverified against the original.
- `CSigRetireCar` (`M5662c0`): code 0 nothing; 1 and 2 call the rescue helicopter, whose 24 s end posts
  `MSG_RETIRE_BYSTAND`; 3 the rally incident (red tint +0x27c, the helicopter when the assistance gate is shut,
  nothing in a quick race); 4 off stage (black tint +0x280, then `MSG_RETIRE_OFFSTAGE`). Accepting the box emits
  `CSigEndRace` with 1. The box pauses the game (state 2).
- Splits: the clock is stored at each split marker (+0x258, +0x25c).
- `CSigEndRace` (`CApplication::M00480810`) sets state 9 in the same frame; that frame is the last stepped. End pages:
  quick rally 0x10 result or 0x12 after a retire; championship 0x13 standings, 0x18 FAILED (finished but `Test3B20`
  says the car can't drive, not the last stage) or 0x17 after a retire. Any finish outside modes 6, 7, 9 submits the
  stage record (`M00567330`).
- Restart: `InitQuickRace` (0x481d00) to `StartQuickRace` with the menu's track and car: physics, car, setup, new
  tyres, HUD reset, damage handler reset and both `rand` streams reseeded from `time(0)`. The weather is not drawn
  again. Ours restarts in process (`Live::restart`, the RTS1 message): the settled pages, heap bookkeeping and game
  cloned at the stage start are written back, the race counters cleared; the setup (saves, start clock, rally
  choice) stays. 500 autopilot frames answer byte for byte again (`live_restart_answers_each_frame_again`).
- tanner-corolla: finish 233.67441 s, splits 69.84889 and 143.56372 s with the go frame given no time (playback);
  boot and live give it a step: 233.68141, 69.855896, 143.57072 s.
- Ours: a race left early (`RaceFlow::end_now`) ends with cause `Left`. The pause menu has no OPTIONS item.
- Ghost: the outside-assistance penalty is also `CGhostCarManager::Accumulate`d (`M576710` 0x576828, after the frame's
  own at 0x482677): the ghost clock jumps by it and it counts as a frame, so the samples' times and the best ghost's
  ranking include it (`ghost::Recorder::penalty`). The jump start's penalty does not reach it. Not compared with an
  original ghost.

## Scripted events

Gameplay effects only; the characters' drawing is in [renderer.md](renderer.md).

- `CCar::Assist8DF0` (0x4e8df0) emits `CSigOutsideAssistance` and arms the countdown +0x15e4 at 0.5 s (72 steps of
  physics time). At 0 `CCar::Vf98x` (0x4e6dd0) teleports the car to the driveline ground + 0.4 m (upright and 0.4 m
  higher with the flip flag +0x9f0), clears the velocities and emits `CSigRespawn`. The flip spectators (five
  `load_car_flipN.lua`, from 1.0 to 4.5 s of HUD time) are cosmetic.
- Three callers put the car back: the player's HELPCALL (via `CCar::MotionProbe` 0x4e7730: faster than 1.5 m/s gives
  `CSigCarProblem` instead), the brake-wall stuck test (`Update9AF0`, 5 s gap and +0x9ec >= 1; with the assistance
  gate shut it retires with code 4), and a script character's box (material 200, `E34_RetireA`: `CSigRetireCar(3)`
  then the assist, or no assist with the gate shut). Evidence: `drive-rcv-help-corolla-34`, `-stuck-206-64`,
  `-retire-impreza00-45`. Otherwise `CSigCarStandStill` only shows HUD messages.
- Rescue helicopter (`CRescueHelicopter::Activate` 0x49bc90): placed 30 m back on the driveline; `Tic` (0x49c170)
  posts `MSG_RETIRE_BYSTAND` after 24 s; a second call restarts the 24 s. While it flies the HUD is hidden and CH_CAM
  is blocked.
- Bird strike feeds only the sound. Crash-site scripts are unused in retail.
- Start and finish marshals are stage items, fired by `start_animation2` and by driveline keys.
- Randomness: `LuaRandom` is `E01_RandomRange` (0x43bdb0, LCG `* 0x2000081 + 0xe623f41d`), not `rand()`. The race start
  seeds both from `time(NULL)`; other systems draw `rand()` every frame, so race-time draws cannot be reproduced and
  ours take `--seed`.

## Race options

`race_options.rs`. Menu settings at 0x16607f8 (`E66MenuSettings`):

| Global | Option | Values (default) |
| --- | --- | --- |
| 0x1660814 | automatic gearbox (per player) | 0/1 (0) |
| 0x1660848 | weather | 0 good, 1 random, 2 bad (0) |
| 0x166084c | damage selection | constant 1, not an option |
| 0x1660850 | damage level (`CARDAM`) | 0 Off, 1 Safe, 2 Reduced, 3 Realistic (1) |
| 0x1660ea0 | neutral lock | (0) |
| 0x1660ea4 | clutch help | (1) |
| 0x1660ea8 | gear protection | (1) |
| 0x1660c5c | championship skill: Rookie, Professional, Champion | (0); not saved in the profile's settings |
| 0x1660c70 | championship damage level | (3) |

- Damage level: `InitQuickRaceDamageHandler` (0x567640) picks 0x1660850 in modes 1, 2, 4, 5, 0x1660c70 in mode 3, 2 in
  the school; `Set3FB0` stores level +0x3df0 and threshold +0x3dec (0, 0.2, 0.7, 1.0); then seeds `rand` from the
  time.
- Aids: `CCarSettingsManager::LoadGlobal` (0x547080) and `InitializeCarSetting` (0x547530) put them in the control
  unit's setup (`AutoGears`, `GearGuard`, `NeutralLock`, `ClutchHelp`; car +0x890..+0x89c).
- Outside-assistance penalty (`M576710`): quick race by the damage level (0, 1: 10 s; 2: 20 s; 3: 35 s); modes 3 and 5
  by the season's +0x8e84 skill (10, 20, 35 s). Champion (+0x8e84 >= 2 in mode 3) shuts the assistance gate
  (`M566640`/`M5665f0`): a stuck car retires instead of being helped.
- Checked: level 1 with default aids (tanner-corolla 13000/13000), level 3 (tanner-damage 12120/12120), championship
  Rookie and Champion on tanner-corolla. Not checked: levels 0 and 2, non-default aids, auto gears.

## Weather

`weather.rs`. `CWeatherDaemon` (`*0x7c36a8`): map +0x10, track letter +0x14, cloud +0x18, wetness +0x1c (0 dry,
1 damp, 2 wet), wear +0x20 (0 new, 1 normal, 2 worn), weather id +0x44, wind +0x60..+0x90. Weather ids: 0 crisp,
1 hazy, 2 norain, 3 lightrain, 4 heavyrain, 5 nosnow, 6 lightsnow, 7 heavysnow, 8 lightfog, 9 heavyfog.

- Ours: the launcher can start a stage in any of its rows (`--weather 1 --season-row K`) and override the road's
  wetness (`rbr-standalone --wetness`, the `.mat` entry and wet meshes follow) and wear; the game takes the row's.
- Quick race (`StartGame` 0x626a70, `Update` 0x4aab40) takes a row of the season table (0x13f rows of 0x18 bytes at
  0x7d34c8: map, letter, cloud, cloud letter, weather, wetness). Good: first row of `misc/nice_w.ini`'s list for the
  map, wear new. Bad: same from `bad_w.ini`, wear left as it was (`--wear`, default new). Random (`E74_FindBest`
  0x5b53b0): row `rand() % n`, then wear `(rand() % 10 > 6) + 1`. Map 4 is all zero. Map 68 has no good or bad row
  (the exe hits `int3`). The row's letter picks the stage variant `track-<map>_<letter>`.
- Grip: `E11_TyreCond` (0x46f6c0) forms the wetness and wear words; `E09_LoadMaterials` picks the stage `.mat` entry
  `<rally> standard <wetness> <wear>` (9 per file, `rbr_formats::mat::select`). Its cells name surfaces, and wet
  surfaces are separate physics records with lower friction (wet/dry static 0.71-0.88). Map 4 is always new and dry.
  Puddle meshes (collision list A) load only when wet.
- The tyre reads no weather state: `BlendTable` (0x534e70) gives `base + WetSlope * |WetnessDesign - WaterFactor|`
  of the contact surface's `WaterFactor`, scaled by wear.
- `E74_WeatherKind` (0x5b5ab0, `weather::tyre_kind`): default tyre by surface and wetness.
- Sky, fog, precipitation, windscreen and wipers are presentation: [renderer.md](renderer.md), [sound.md](sound.md).

## Championship and service

`championship.rs`, `season/` (with `season/layout.rs`, the adapter for the save's
position, clock, ranks and result words), `season_events.rs`, `season_store.rs`,
`ai_season/`, `service/`, `setup.rs`. A rally of `season.dat` runs stage by
stage on its season-table rows. A finished stage gives the season the time
and three sectors (split 1, split 2 minus split 1, time minus split 2); a
retire ends the rally (time -1 and the retired flag on this and later
stages). A finish in a car that cannot drive before stage index 5 also ends
it. Gameplay uses named position, clock, rank and result views; the save
stays byte-exact, unknowns preserved.

- Service events: kind 7 (20 min) before the first stage, kind 5 (20 min) after stage indices 0 and 2, kind 6
  (40 min, then the clock goes to 08:00) after index 3. Late penalty 10 s per started minute past the budget,
  added as the park closes (`E80Daemon::Penalty` 0x5cf330). Repairs run at 30x after "Repair!".
- Setup and tyres are picked only after a service or the rally's start; otherwise the car runs as it is.
- The season, AI field, forecast weather, event queue and points follow the asm's `rand()` order; the driver classes'
  draws are ours.

## Damage

`car_damage.rs`. In a championship the damage handler slot outlives the stage (mode 3 does not reset it); carried
damage is `CopyFrom` (0x4ec470): the 64 zones' first words, the 64 zone floats and 0xfa words from +0x3a0c. The tyre
wear (+0x3d04) is carried with it until a service's refresh (0x4ec3b0). Part severities and repairs follow each
class's `Vf00`/`Vf04`. Physics internals: [physics.md](physics.md).

## Saves

`record.rs`, `season_store.rs`; layouts in [formats.md](formats.md).

- Profile `SavedGames/pf<name>.rbr` (encrypted): menu settings, controls, input filter, camera, and the season
  (`RBR7`) with the AI stores, carried damage and setup after it. The season is written after each player turn.
- Records `pfresult.rst`: 37 stage rows. The submit writes the profile's personal best first, then the row when
  faster, copying the holder's name to its NUL. The result page says `MSG_QRBEST` or `BEAT_PERSONAL`. Not checked
  against the original.
- `misc/times.ini` holds each stage's reference splits and finish (`mm:ss:cc`); the viewer compares against it
  without `--saves`.
