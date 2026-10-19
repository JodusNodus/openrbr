# rbr-gameplay

The Richard Burns Rally 1.02 gameplay loop in Rust: the pieces the loop owns, each checked against the original.
The plan and order are in `lead/reports/gameplay-loop-rust.md` (section 5); the original's path is in
`notes/gameplay-loop.md`.

| module | original | what it does | checked by |
| --- | --- | --- | --- |
| `control` (G1) | `E16SteerState::Pack` 0x48c910, `E16Quant::NotifyFilt` 0x48d130 | quantized control bytes to the 0x20-byte payload | every step of the four drives (`tests/recordings.rs`) |
| `driveline` (G2/G9b) | `E12_SampleX` 0x475ea0, `E12_Sampler::Sample` 0x4762a0, `Dot150` 0x476450 | stage distance and normalized track-direction dot from `.trk` data | unit tests; `tests/timing.rs`; direct vectors in `tests/marker_vectors.rs` |
| `timing` (G2) | `CTimeCard` (0x49ff80 to 0x4a0130), `CTimeKeeper::Load` 0x4a01f0, `Check` 0x4a0450, `Record` 0x4a0540, `Update` 0x4a0320 | the time card and the timekeeper: start, splits, finish; the still counter | unit tests from the asm; `tests/timing.rs`; `tests/timing_peek.rs` (every card field, both drives) |
| `timing` clock (G4) | `StepForward` 0x5273a0, `m_d20 += base` (decompile line 102) | the elapsed time per substep, `advance_time`, from `CLOCK_START` | unit test; `tests/timing_peek.rs` (every step, both drives) |
| `tick` (G6) | `CPC_Application::Vf0c` 0x44a3d0 (the clock and the split), `E13Get9B70F` 0x47e7b0 (the step), `CApplication` 0x47cc30/0x47cd50/0x47cd70 (the gate), `TicPhysicsRecording` 0x47f420 (the loop head and the substep loop) | the frame clock: wall time to whole steps, `facc`, the carry; the single-step gate; the substep loop; the load settle | unit tests from the asm; `tests/schedule.rs` (every frame of the four drives: the settle, then one substep each) |
| `stage` (G7) | `CApplication::StartQuickRace` 0x481660 (state 5), `M004825b0` 0x4825b0 (case 5: the load, state 0xd), `M004801d0` 0x4801d0 (case 0xd: the settle), `CApplication::Tic` 0x4804f0 (state 10 countdown), `M0047ca40` 0x47ca40 (the go), `CCar::UpdateSeed` 0x4e91f0 (flags 0x10/0x20) | the stage state machine: set-up, load settle, countdown, driving, end of race; the car requests of the payload flags | `tests/stage_peek.rs` (every peeked step of tanner-corolla: state, load and go countdowns, track, car); unit tests from the asm |
| `camera_go` | `E15CamHost::M8840` 0x488840 (mode 10 in state 10), `M487680` 0x487680 (mode 10 sets pi), `M0047ca40` 0x47ca40 | the camera's start orbit and the go it gives after `pi / 1.2` s of frame time; `Game::with_camera_go` runs it (live play), the recorded runs keep the host's go | unit tests from the asm; `game` unit test (375 orbit frames of 7 ms); `rbr-standalone` live boot test (the steps after the timed go equal the recording) |
| `markers` (G8) | `E12_AnimSet::Load3b0` 0x4783b0, `E25_SlotId` 0x4b55e0, `E25_BitId` 0x4b5540 | one crossed marker per tick, pace-note lookahead/fired bits, typed start/split/finish/end events, and pace-note word maps | unit tests from the decompile; `tests/marker_vectors.rs` (fuzzed direct calls and captured signal events) |
| `race_clock` | `E62GameMenu` (the HUD): `0x5772e0` (reset), `M576710` 0x576710 (assistance timer, penalty), `M573d90` 0x573d90 (countdown, go, clock), `M573900` 0x573900 (`CSigOutsideAssistance`), `M579430` 0x579430 (jump start), `M579590` 0x579590 (moving start), `0x56a620` (the HELPCALL item's predicate) | the race clock the result is taken from: the countdown, the go, the outside-assistance penalty, the jump start; the call-for-help gate | unit tests from the asm; `tests/race_clock_peek.rs` (every HUD frame of three peeked assist drives, bit for bit) |
| `flow` | `E59Host90::M00565430` 0x565430 (mode, sub-state), `M00565e80` 0x565e80, `E60Flow::M566640`/`M5665f0` (gates), `M5662c0` 0x5662c0 (`CSigRetireCar`), HUD `M573530` 0x573530 and tick 0x579df0 (finish timer, incident, off-stage, time limit), `M72da0` 0x572da0 (splits), `CRescueHelicopter` | the flow word's modes and gates; the end of the stage race: the finish, the retire's effects, `CSigEndRace` as a typed `StageEnd` | unit tests from the asm; `rbr-standalone --boot` on a finished drive (the run ends at the finish step) |
| `pause_menu` | `CApplication::M0047f9c0` 0x47f9c0 (state 1 opens it), `FUN_0056c360` 0x56c360 (the page by mode), `CGameMenuDaemon::M56eb40` 0x56eb40 and `M56ee00` 0x56ee00 (pages 1 and 2), predicates 0x56a620, 0x56a6c0, 0x56a6e0 (`M584ce0` ops 0x25 drawn, 0x26 selectable), `RESTART` 0x56a600, `EXIT2MM` 0x56a6a0, retire 0x56eac0 (boxes 0x19/0x1a, callbacks 0x56e8a0/0x56e820) | the in-race pause menu's items per mode (no restart and no exit in a championship), their confirm boxes, and the retire's end (`RaceFlow::retire_from_menu`, `FrameInput::retire`, `EndCause::Retired`); `Game::help_offered` adds the helicopter's idle test to the help gate | unit tests from the decompile; `rbr-standalone`'s `the_pause_menu_s_retire_ends_the_race_without_a_step` |
| `championship` | `CApplication::M00480810` 0x480810 (mode 3), `CSeasonStageManager` 0x5d6020-0x5d68a0 | a rally of `season.dat` as stages on their season-table rows with service stops between them; times, penalties, the damage and tyre wear carried; where the setup and tyres are picked (after a service) | unit tests; every retail rally's stages installed |
| `service` | park budget 0x5c70c0, part tree 0x5b0dc0-0x5b1ae0, `CSelectPartsUI::Vf04` 0x5c5920, timer 0x5c5600, `M5b1220`, `E80Daemon::Penalty` 0x5cf330 | the service park: budget, crew scales, part times drawn on `rand()`, shown estimate, `CSelectPartsUI` orders, finish-now and manager's advice (msvcr71 `qsort`), repairs at 30x, time-out, messages, late penalty; 1.02's or NGP's rules | unit tests; the retail Corolla tree |
| `crt` | msvcr71's `rand()` (IAT 0x729540) and `qsort` (0x7c36695f) | the CRT stream the weather, the season, the AI and the service draw on; the `qsort` the service's advice sorts with | unit tests |
| `car_damage` | `CCarDamageHandler::CopyFrom` 0x4ec470, refresh 0x4ec3b0, the part classes' severity and repair (C++ reconstruction) | the damage carried between stages | unit tests; tanner boot 13000/13000 with the carry hook |
| `ai_season` | `E78HostCC::Ev094e0` 0x5c94e0 and helpers; `Ev09f80` 0x5c9f80, `Ev9ec0` 0x5c9ec0, `SetA0Rnd` 0x5cd4b0 | one AI driver's stage: par draws, mistakes, accidents, damage, injuries, false starts, bulletins; the AI service: manager orders, temperament rolls, repairs within the minutes and budget, late penalty; on `rand()` (`crt::CrtRand`) in the asm's order | unit tests (rand order of a clean section, service picks, temperament roll) |
| `season` | `DaemonInit` 0x5d34b0, `SetupAI` 0x5d1080, `E80Team::Parse` 0x5cd890 (the driver class), `Init6` 0x5cea80, points 0x5d2070 | season creation with the player in the picked team's seat, teams, cars, ranks and names, forecast weather with the slots' start times, start orders, the AI field (budgets, styles, spending) in the save's stores, points and standings on the `RBR7` season; the driver classes' draws ours | unit tests; a retail rally through the queue |
| `season_events` | `M4990` 0x5d4990, `M3800` 0x5d3800, the pushes 0x5cf6f0-0x5d1f50, stage loop `M4c90` 0x5d4c90, `Distribute` 0x5cfa10, `FillPairs` 0x5d2910, `ScoreAll` 0x5d23d0, `E78Entry::Dmg` 0x5d6df0, `M568cf0` (the player's turns) | the season's clock and event queue: every event kind, the AI's stages and services, the accident pairs, the overnight 08:00, the points, the teams' budgets, the next rally; the player's turns; the queue in the save | unit tests (order and times, clock, turns, budgets, pairs); a retail rally; the queue byte for byte in the original profile |
| `season_store` | `E80Daemon::M005d14d0`, `M005d15b0`, `FillNeg`, `Penalty` 0x5cf330, `GetPlayerFalseStarts` 0x5d07a0, `IncT174` 0x5ce2e0 | the season in the profile file, the player's stages, the service's late penalty at the park's close, false starts, the stores, damage block and setup after it | write and read back; a played season byte for byte in the original profile |
| `record` | `M5653c0` 0x5653c0 (a stage's row of the records table), `E69Profile::FindEntry` 0x5994b0 and `InsertTime` 0x599580 (the profile's record), `M00567330` 0x567330 (the end-of-race submit, called from `M00480810`), `CQRResultBaseUI::DrawGS` 0x5c3880 and `CQRResultUI::Vf40` 0x5c3b10 (the result page's message and rows) | the stage record and the personal best in the game's save files (`pfresult.rst`, `pf<name>.rbr`): the row lookup with its default, the submit (profile first, then the row when faster, the holder's name copied to its NUL), the page's verdict (`MSG_QRBEST`, `BEAT_PERSONAL`), and `Book`, the two files loaded and written | unit tests; the retail table's rows equal `times.ini` (`RBR_SAVES`); not run against the original (cachy items in `lead/reports/splits-1.md`) |
| `result` | `CTimeCard` (the splits, the finish), `E60Flow` reads the race clock at the end (`notes/gameplay-loop.md`), `CQRResultUI` and `CHotSeatRallyStageResultsUI` (the layout, `src/rbr/RBR_E78_QRResult.cpp`) | the stage result: `Running`, `Finished` or `Retired` (DNF) with the end's cause, the race time with the penalties, the penalties, each split from the card; `ended` takes the race flow's `StageEnd` | unit tests; the time is the race clock, the splits the card (traced); the outcome is the flow's `CSigEndRace` (traced) |
| `ghost` | `CGhostCarManager`: `Start` 0x492b60, `Update` 0x493240, `Record1` 0x492730, `Record2` 0x4928c0, `Accumulate` 0x4939f0, `Select` 0x492c20, `Playback` 0x493650, `BeginWrite` 0x492ca0, `SetFlag` 0x492ae0, `E18_QuatSlerp` 0x492160, `E11_VecLerp` 0x473c90 | the ghost car's recording (a key, then a delta every second frame, in-between deltas past 1.95 m, a key after a respawn) and playback (lerp and slerp between samples, keys not interpolated into), the pause flag (bit 31), and which drive becomes the ghost to race (`replaces`: a finished drive on another map or faster) | unit tests from the decompile and the C++; `tests/ghost.rs` (every retail ghost plays to its end; the Tanner drive recorded and played back within 3 cm) |
| `filter`, `ini` (G5) | `E16Filter::Tic` 0x48c500, `Step` 0x48af90, `StepB` 0x48b1d0 (source only), `E16_Reset` 0x48ac60, `E16_InitGrid` 0x48bb70, `E16_LoadINI` 0x48b310, `E16SteerState::Pack` 0x48c910 | the input filter: per channel the curve, rise and fall limits and speed fade of its selected cell; the grid from `PC_InputFilter.ini` and the quick race's reset; `Pack` to the nine bytes | `tests/filter_peek.rs` (grid at every step; filter and pack at every step of the run, on the run's zero input); `tests/filter_vectors.rs` (`Step` and `Pack` on fuzzed direct calls, non-zero input); `control` unit tests |
| `setup` | the setup pages' `Vf38` (0x550750, 0x550990, 0x550fd0, 0x551610, 0x554b30, 0x554de0, 0x554f80, 0x555750, 0x555b60, 0x556120), `E55_MakeSliderA/B` 0x550130/0x5501f0, `CTweakSliderMenuItemReal/Unsigned` `Vf34`/`Vf38` 0x54ee80-0x54f140, `E16_BuildSetup` 0x48d850, `CCarSaveFileUI::Scan` 0x546a30, `CSeasonChooseSetupUI` 0x5460b0 | the typed `Setup` (every value the pages edit), its ranges from `r_<surface>.lsp`, the defaults from `setups/d_<surface>.lsp`, the sliders' step, clamp and wrap, and the setup file text the game's loader layers over the default; the season's setup rows before a stage (`SeasonSetup`: the surface's default, the current one, a save slot) | `tests/setup.rs` (every value is one of the pages' 239 sliders by the key tables' offsets; every retail car and surface parses and reads back); unit tests from the asm; the tanner boot with the default written as a file (13000/13000) |
| `game` (G9a/G9b) | `CPC_Application::Vf0c` 0x44a3d0, `M004801d0` 0x4801d0, `CApplication::Tic` 0x4804f0, `TicPhysicsRecording` 0x47f420, `CCar::Vf6cx` 0x4ea300, `StepForward` 0x5273a0, `M004825b0` 0x4825b0 | one composed frame: settle, poll, stage tick, filter/Pack/replay payload, physics substeps, in-step time record/clock advance, post-tick marker ticker; Dot150 uses recorded car and stage data | `tests/game_loop.rs` (all four full recordings; stage peek and both time-card peeks) |

## Marker ticker

`markers::MarkerTicker` copies the sorted keys from `rbr_formats::markers::StageMarkers` and keeps the runtime fired bit
in each key's `word1`. A tick returns typed host actions and signal events: the backwards pass handles at most one
crossed start/split/finish/end key, while the forward pass marks pace notes inside the speed-based lookahead and emits
the ones that pass the speed gate. `slot_word_id` and `flag_word_id` reproduce E25's tag and word1 mapping. Host return
values, `E12_SeekD` and the two lookahead globals are explicit `TickInput` values supplied by the runtime host. G9b
adds a pure `driveline::dot150` port using `CCar+0x17c`, the velocity lanes at `CCar+0x1c0/+1c4/+1c8`, and the
stage `.trk` second vector.

The direct-call check uses `/tmp` records generated by `harness/direct_calls.py` with
`harness/vector_specs/gameplay_markers.py`; set `RBR_MARKER_VECTORS` to the record path when running the test. The
record captures the marker-key words and the original's emitted pace-callout/EndRace payloads.

## One frame together (G9a)

`game::Game` composes the pieces in the application's frame order. The frame clock and load-state settle run first
(`Vf0c` 0x44a3d0 and `M004801d0` 0x4801d0); raw input is stored once (`TicFwd` 0x4cc550 and `NotifyB` 0x48ab40); the
application tick selects countdown or driving (`Tic` 0x4804f0). Each driving substep runs filter, `Pack`, and payload
unpack before replay can replace the payload (`TicPhysicsRecording` 0x47f420; `Tic` 0x48c500, `Pack` 0x48c910,
`NotifyFilt` 0x48d130, replay fire 0x49af10). `PhysicsStep` invokes its callback at the car's in-step sample point;
`Record` 0x4a0540 runs there before `StepForward` 0x5273a0 advances elapsed time. The post-tick animation update
(`M004825b0` 0x4825b0) runs the marker ticker after the physics loop (`Tic570` 0x47a570, `Load3b0` 0x4783b0).

`tests/game_loop.rs` drives the four FRM1 recordings through this composition. It checks each frame's settle/driving
schedule and recorded replay payload, compares the tanner-corolla stage peek, and compares the whole available timing
peeks for tanner-corolla and tarmac-lancer. The frame records cover 35,249 tanner-corolla frames, 12,120 tanner-damage,
12,490 arctic-snow, and 20,892 tarmac-lancer frames. The marker ticker's fired-bit count is printed at the six G8
snapshot checkpoints. Dot150 now uses the recorded car and stage data. The NotifyA speed fade is ported (`filter::notify_a`); this
test passes no vector (`notify_a_vector: None`, the filter's speed `|CCar+0x15f8|`), and the standalone's pure physics
gives it (`Playback::notify_a_vector`), which equals the original's (`tests/filter_peek.rs`). The counts also still differ because the active
animation-host SeekD state and lookahead settings are not recorded in FRM1.

### Not done

- The raw device source is external: `USInputManagerImpl::Tic` (0x4cbf50) maps devices, axes, buttons, selectors and
  actions into the raw signal. FRM1 records only the final control payload. The integration test supplies idle raw input
  and replays the recorded payload after the local filter/Pack path, matching the order at `TicPhysicsRecording`
  (0x47f420) without asserting a raw-to-payload match.
- `CCar::Vf6cx` provides `CCar+0x15f8`, clamped nonnegative by `CCar::Vf28x` (0x4ea020) before it emits
  `CSigCarToCarInputFilter` (signal manager +0x860) with `MB560` of the car's vector at +0x220 and `GetDir`.
  `E16Filter::NotifyA` (0x48aa10, `filter::notify_a`) turns that into `m_36c`: the speed, faded by the sideways share of
  the vector. Only the speed and the vector's x and y matter (`GetDir`'s vector is stored, read by `StepB`'s unused
  branch). Checked: `filter_peek::speed_fade_input_matches_the_original` gives every one of the 1,372 `m_36c` changes in
  `peek/g5-filter` bit for bit from the recorded car the step before left. Between steps the walk is the car's one
  hook, `CSolid::E43VfB0` (0x52add0) on the +0x110 matrix (a trace of `MB560` on a tanner snapshot reads only the car's
  +0x110..+0x134 and +0x220..+0x22c); `Vf28x` runs once a frame after the frame's steps (`CPC_Application::Vf0c`:
  `CApplication::Tic` 0x4804f0, then `M0047ca70` → the physics' `Vf0C` 0x526a10, which calls each entity's `Vf28`),
  and nothing it calls before its walk changes what the walk reads. The composed test still passes `None` (its
  placeholder `|CCar+0x15f8|`); the replayed payloads make the filter's output unused there.
- `E12_SeekD` (0x476bf0) reads the active animation-host key list and the current holder's `this+0xc` key state. FRM1
  records neither. The lookahead settings at 0x1660b74/0x1660b80 are external configuration globals, not car-frame
  words (the supplied snapshots contain 4.0 and 25.0). These values affect the forward pass's fired bits, so the test
  leaves them explicit and reports the G8 checkpoint deltas: tanner-damage 39 vs 40 at step 11820; arctic-snow 40 vs
  42 at 11880; tarmac-lancer 1 vs 2 at 5000, 14 vs 16 at 9000, 21 vs 24 at 12000, and 50 vs 51 at 20500.
- Natural `Load3b0` call count is not captured. `Cb930` (0x478930) can call it when animation-state words change, and
  `Tic570` (0x47a570) can call it after `Tic`; FRM1 contains neither callback timing nor those state words.
- Wall-clock readings, single-step gate changes, race-host readiness, the camera go (unless `Game::with_camera_go`), and the later game-flow
  dispatch from an `EndRace` signal are host events. `FrameInput` exposes those events, but the recordings do not give
  their timing. Replay cursor advancement for frames with multiple driving substeps is also host-owned; `FrameInput`
  supplies one replay control value for the substeps of that frame.

## Timing

`TimeCard` is a `Copy` value; each operation returns the new card. `TimeKeeper::load` builds the card from
`rbr_formats::markers`: the last start key, the last finish key, and up to 16 splits in ascending order. `record` takes
the car's driveline distance `x`, the elapsed time `t` and `MaxTimeStep`, and returns at most one `Checkpoint`.

Not ported: `CTimeKeeper::Init`, `Release`, `Vf00`, `SetD` (0x4a0270), `Diff` (0x4a0290), `Clear0c`, `Get0`, `Get0b`,
the getters `LoadDbl` and `Cur`, the signal dispatch `CMultiSignal1::Emit` (0x4a0390; the Rust returns the checkpoint),
and `CTimeCard::Set`/`Assign` (see below).

### Where the C++ and the assembly disagree (the assembly is followed)

- `Check` (0x4a0450), the checkpoint's time word: the C++ puts the distance `f` in it (`eventTime.f = f`); the assembly
  stores `cur`, the elapsed time since the origin, as `Record` does. The port emits `cur`.
- `CTimeCard` constructor (0x49ff80), arguments `a` and `b`: the C++ stores them at +4 and +8; the assembly at +8 and
  +0xc. Both are zero for the keeper (`CTimeKeeper::Init` passes 0, 0, 0), so the port does not keep them.
- `CTimeCard::Set(double)` (0x49ff70) and `Assign` (0x49ff60): the C++ and the assembly both write the double at offset
  0, the vtable slot. Not ported. Callers not traced.
- `CTimeCard` +0x20 (`flag20`): neither the C++ nor the assembly initialises it in the constructor. The port keeps no
  such field: `Check` sets it only together with `started` (+0x40), so the latch is the same as `started`. The peek
  shows it is not zero before the start (1068149419 on tarmac-lancer); what matters is that it is not 1, which the port
  assumes. Were it 1 before the start, `Check` would emit a start the port does not make; no peek shows that case.
- `Refresh` (0x4a0040): the asm and the C++ agree (next pending split distance, else the finish). The header's field
  names differ from the decompile (`notes/formats/stage-markers.md`, open item on the split threshold); the asm settles
  it: the threshold is `splits[passed]`, then `finish`.

### Checked against the original (`tests/timing_peek.rs`)

The time-card peek (`peek/g3-timecard.peek`, one sample after each `StepForward`) was taken on two drives, 2026-10-15:
`h1-tanner-corolla` (12,084 frames, steps 40 to 12,240; the start and split 0) and `g3-tarmac-lancer` (21,092 frames,
steps 40 to 21,248; the start by the time rule at step 1921, and split 0). The test drives the card from each FRM1
recording and compares, after every frame: the clock, `flag40` (started), `flag20`, `i24` (hold), `f1c` (last), `cur`,
`count2` (passed), `d38` (origin), `flag58` and `d50` (finish), and `split0`/`split1`. The events are the peek's
transitions (start, each split, finish) against the emitted checkpoints, by step. Every field and event matches on both
drives, bit for bit.

Settled by the peek:
- **Where `Record` sits** (design note section 2): inside the step, before that substep's clock advance. The card sees
  the clock as it was before the substep: the start at step 1 is `d38 = 43185.0`, the time-rule start at step 1921 is
  `d38 = 43200.0`, and split 0 at step 11139 is `43185 + 11138/128`. Run after the advance, the origin at step 40 is
  wrong (the test fails there). `CCar::Vf6cx` reads the clock before it calls `Record`, the advance comes at the end of
  the substep.
- **The position `Record` uses** is `sample_x(car +0x17c)` of the frame's car block: `f1c` equals it on every frame
  before the finish. So the `.trk` kind-0x14 values are the driveline, and `+0x17c` does not move between `Record` and the
  end of the step on these drives. After the finish the C++ (and the port) stop setting `f1c` when `x >= finish`; no
  peek reaches the finish.
- **The clock (G4).** The original adds in f32 and widens: `m_d20 = (double)((float)dt + (float)m_d20)`. Near 43185 s the
  f32 ulp is 1/256 s, so `0.007` (`MaxTimeStep`) advances the clock by 2/256 s = 0.0078125 s on every substep, not
  0.007 s. The seed is 43185.0. `advance_time` reproduces every peeked clock value on both drives. Plain f64 addition
  fails at step 40. The physics `dt` itself is not measured here; this is the clock's increment.
- **`flag20`** is uninitialised before the start (1068149419 on tarmac-lancer); only the value 1 counts as started, and
  the port's `started` latch matches it. The garbage is not zero, so the port's constructor cannot assume zero for it.
- **`hold` (`i24`)** is 0 on both drives, so `Set24` was not called there; what sets it is still open.

Not covered by these peeks: the speed start (`cur > -1.5` and the speed test, the second branch of `Check`), split 1,
the finish and the end key on the peeked drives (the `tests/timing.rs` run on `drive-tanner-corolla.bin` covers the
split, finish and end crossings, with the clock), `Set24`, `SetD`, and any stage whose start is not passed by step 1921.

### Not verified

- `CTimeCard::Set(double)` and `Assign` (see above): callers not traced.
- The start's `fired` payload: the peek holds state, not the signal, so which of the two start payloads the signal
  carries is only the asm's reading (`Check` fires with `fired = 1` only on the speed or distance branch).
- `Load` assumes the loader's `ScanTag` walks the key list in the same ascending order as `markers` (`qsort`). Not
  checked against the iterator.
- `Load` builds a new card. `CTimeKeeper::Load` (called from `E45PhysBody::MCE0`) on a card that already has keys would
  add splits up to 16; whether the card is recreated per stage is not traced.
- `Update` is called with the step counter read from `CState` +0x18; that the counter is the step count is inferred from
  the `% 100`. Not in the peek.
- The settle frames (40 substeps each, car at rest) are fed to `Record` once per substep with the frame's car distance,
  since the car block is only recorded after the frame. `f1c` is the same at step 40 and 160, which is consistent with
  rest but does not prove the distance was constant within each settle frame.

## Tests

- Unit tests (`src/`): the asm's behaviour for each operation, the speed start, the hold, the split and finish order,
  `Load` (last start key, 16 splits, missing start), `Update`, and `sample_x`.
- `tests/timing.rs`: tanner-corolla over `track-54_E`. Needs `RBR_RECORDINGS` and `RBR_DATA`; skipped otherwise. Checks
  the start (first frame, fired), splits at steps 11139 and 21664, the finish at 34530 and the end key at 35404 (a plain
  crossing) against the predicted crossing table.
- `tests/timing_peek.rs`: the card and the clock against the peek, on `h1-tanner-corolla` (`track-54_E`) and
  `g3-tarmac-lancer` (`track-41_M`). Needs `RBR_DATA` and `RBR_RECORDINGS`, which must hold `peek/<drive>.bin` and
  `peek/<drive>.peek.bin`. Mutation checks (not committed): advancing the clock before `Record` fails at step 40 on the
  origin; plain f64 addition fails at step 40 on the clock.
- `tests/recordings.rs`: the control payload (G1). Needs `RBR_RECORDINGS`.

## Tick (G6)

`tick` is the frame's arithmetic and the substep schedule, not the loop's host calls. `Vf0c` reads the wall timer,
clamps the gap to 0.1 s, takes the physics step from `E13Get9B70F`, and splits the time into whole steps; `Tic` passes
the count to `TicPhysicsRecording`, which runs one `StepForward(1)` per substep. The host (the filter, the replay scan,
the step, the wheel force) runs inside `run_substeps`' closure.

Checked against the assembly (`objdump -D` of `RichardBurnsRally_SSE.exe`, from `Vf0c` at 0x44a5f0 to 0x44a6e0):

- The step is `E13Get9B70F` (0x47e7b0), the `MaxTimeStep` f32 at physics definition +0x1608c, or 0 with no physics, which
  takes the fallback 0x7c0bc8 = 0.01 f32 (1/100 s). The C++ calls `CApplication::M0047deb0` (0x47deb0). Nothing in the
  exe calls or references 0x47deb0 (no direct call, no vtable slot), so it is not on this path.
- The clamp is 0x795e30, an f64 holding the f32 0.1 widened (0.10000000149011612), not the f64 0.1.
- The first test and the loop's test are `step <= dt` in f64 (the step widened). The loop subtracts in f64.
- `facc` is the f64 sum of the steps, rounded to f32 once at the end (`fstp dword` at 0x44a6e0). The C++ sums in f32;
  the two differ from six steps on (`sum_is_rounded_once_not_per_step`: 0.042000003 against 0.041999999).
- The carry: `last` becomes `now - rest` (0x44a625 onwards, `last -= dt` at the frame's end). The fixed-dt branch
  (`0x7ea68c != 0`) is not ported: nothing in the exe writes that flag, and it is 0 in the image.
- The single-step gate (`m_unkC38`, `m_unkC3C`): `TicPhysicsRecording`'s head counts one substep per credit while the
  gate is on. `M0047cd50` (0x47cd50) and `M0047cd70` (0x47cd70) have no direct caller in the exe; their addresses are
  in `.rdata`, so they are reached through a vtable. Which object calls them is not traced.

`SETTLE_SUBSTEPS` are the four `StepForward(40)` load-time calls. The caller is not traced; the recordings show them as
the first four frames (the hook sees them as `substeps > 8`).

### Checked against the recordings (`tests/schedule.rs`)

On all four drives (`RBR_RECORDINGS`), the frames are the four settle calls (pre 0, 40, 80, 120; 40 substeps each), then
one substep per frame with the counter continuous: tanner-corolla 35,249 frames to step 35,405; tanner-damage 12,120 to
12,276; arctic-snow 12,490 to 12,646; tarmac-lancer 20,892 to 21,048.

The frame headers hold neither the wall time nor the count each tick asked for: `TicPhysicsRecording` makes one
`StepForward(1)` per substep, so every driving frame is one step whatever the tick did. The frame clock's split and the
gate are therefore checked against the assembly's arithmetic in the unit tests only; a gated tick (at most one substep)
looks the same as an ungated one-step tick in the headers.

### Not verified

- The frame clock's split, the carry and the f32 sum against the original: no direct-call records. 0x47deb0 has no
  caller, so fuzzing it checks nothing on the path; the arithmetic that matters is in `Vf0c` itself (0x44a3d0), which a
  direct call cannot run without the whole frame.
- The single-step gate: a direct call of `TicPhysicsRecording` (0x47f420) with fuzzed `m_unkC38`/`m_unkC3C` would need a
  spec for its filter, replay and physics calls. Not written.
- The settle caller.
- The `E09Timer` reading and the host's wall clock: outside the crate.

## Stage (G7)

`stage` is the application state the frame loop switches on (`CApplication` +0x728) from the quick race's set-up to the
end of the race. `Stage::quick_race(track, car)` is `StartQuickRace`'s state 5 (`m_unk730` = `track`, `m_unk734` =
`car`). `load_ready` is the race host's ready reply in state 5 (`M004825b0` case 5, asm 0x482ba2): the load countdown
`m_unk72c` becomes 0x96 and the state 0xd. Each frame of the load runs its settle steps and `load_tick` spends them (the
0xd branch of `M004801d0`, asm 0x4802ec, called from `Vf0c` at 0x44a6eb every frame); at 0 the state is 10. `go` is the
camera's (`M0047ca40`: the countdown `m_unk714` = -1, armed `m_unk718`). `countdown_tick(dt)` is `Tic`'s state 10: the
countdown takes `dt` off, and below 0 driving (1) starts. `race_over` is the `CSigEndRace` receiver's state change
(`CApplication::M00480810` → `M0047f3a0`): while driving, the end page (9), in which `Tic` steps no physics. State 2 is
the pause menu.

`car_request` decodes the payload flags the way `CCar::UpdateSeed` (0x4e91f0) branches on them: 0x20 is a reset, which
takes the bit over 0x10, and 0x10 is the call for help (the motion probe, then `CSigCarProblem` or the assist; the
physics side is rbr-physics'). Only the pause menu's HELPCALL sets 0x10 (see Race clock); nothing in the exe emits the
request that sets 0x20. While the replay is in its state 2 (`E19IsState2`) the flags ask nothing.

### Checked against the original (`tests/stage_peek.rs`)

`peek/g7-tanner-corolla.bin` and `.peek.bin` (2026-10-15, `harness/play.sh` with `peek/g7-stage.peek`). The probes are
read after each `StepForward`, so a record's state is the one during that call, before the caller's updates. The test
drives the Rust stage from the first record's track and car and compares, on every frame, the state (+0x728), the load
countdown (+0x72c), the countdown (+0x714) with its armed flag (+0x718), and the track and car (+0x730, +0x734). The
peek holds 4 settle frames of 40 substeps (state 0xd; the load countdown 150, 110, 70, 30 before each call) and 12,080
driving frames (state 1). Every step matches.

Settled by the peek and the asm:
- The app pointer is the global at 0x7eac48 (`DAT_007eac48`): the `CApplication` allocation (0x1bd0 bytes) at 0x473779
  stores to it, and the peek reads the state through it.
- The settle calls are the 0xd branch of `M004801d0`, from `Vf0c` each frame; that answers the `gameplay-loop-rust.md`
  open item on the settle caller. The load countdown drops by each call's substeps after the call.

Not covered by this run:
- The countdown's go and its timing: the camera's timer is wall time. The test gives the go as soon as the countdown
  starts. No record is taken in state 10 (the physics does not step there), so the countdown is checked only at its end.
  The peek shows -1.0 on the first driving step, which only a `dt` below about 6e-8 s leaves exact; the test passes 0.
- EndRace from the finish line or the end of the stage: this run ends at the recording's last control (step 12,240),
  before the finish (34,530) and the end key (35,404). The response (driving to the menu) is ours, from the source: the
  dispatcher in `M0047f9c0` moves to 2 once the game flow has left the race. Not peeked.
- The end-race signals from the replay (`TicPhysicsRecording` when `E19QccFire` reports done, and `M0047f560`): the
  replay is not modelled. The marker ticker that emits EndRace on the 0x16 and 0x18 keys is G8.
- The set-up before the first step (`StartQuickRace`'s tyre and sound calls, the race host's ready reply, states 6
  and 0xc): no step runs there, so no peek covers them. The sound host's gate on the end of the load (`M00544e60`) is
  not modelled.
- The payload flag 0x20 against the car: no run sets it. The car's response to 0x10 (the assist and the teleport) is
  checked by `rbr-standalone --boot` on `drive-rcv-help-corolla-34` (see Race clock).

## Race clock

`race_clock::RaceClock` is the HUD's clock (`E62GameMenu`, `*0x165fc68`), the time the race result is taken from: the
start countdown +0x244 (7 s at the reset), the clock +0x140 and its stop word +0x144, the assistance timer +0x270 with
its fade +0x274 and code +0x278, and the jump-start flag +0x248. `Game` owns it and, in driving, runs each frame after
the substeps and the marker ticker: the start line's effects (below), then `tick(facc)` (`M576710`, then `M573d90`),
then the moving-start test.

- The go: the countdown takes each frame's `facc`; below 6 s it fires `start_animation2` once; when it passes 0 the
  clock starts with that frame's whole `dt`. The rally school (flow state 7) has no countdown; its start line starts
  the clock.
- Outside assistance: the car's `CSigOutsideAssistance` (`CCar::Assist8DF0`, rbr-physics) restarts the timer with its
  code, 1 when the car was not on its wheels (`+0x9f0`), else 2 or 3. `StepSample::outside_assistance` carries it from
  the step (`rbr_hybrid::pure::Pure::take_outside_assistance`, the emit 0x4e89b0's argument). When the timer passes 1 s
  the penalty is added if the clock runs: 10, 20 or 35 s by the setting (`AssistSettings`: in a rally, flow states 3
  and 5, `*0x7d6128` +0x8e84 0/1/2; otherwise the global 0x1660850 0 or 1/2/3). The spectators' start and end (1 s and
  4.5 s) and the fade are returned for the presentation. The physics' time card (`timing`) never sees the penalty.
- Jump start (`M579430`): the clock takes the countdown's rest plus 10 s and the countdown ends; once per race. The
  start line (marker key 0x15) takes it while the countdown runs (`StartLineContext::menu_host_result`, `M572fc0`);
  `M579590`, the HUD's `CSigCamera` receiver, takes it when the car moves faster than 1 m/s in the countdown's last 4 s
  (`StepSample::speed`).
- The finish's marker stops the clock (+0x144).
- Call for help: `FrameInput::help_call` is the pause menu's HELPCALL (`E61_M0056e6d0`, 0x56e6d0). Its predicate
  (`0x56a620`) offers it outside the assistance window (`[0, 8.5)` s, `M573920`) from 2 s of race time; the request
  (`CSigRespawnRequest(1)` -> `E16Quant::NotifyRespawn` 0x48d080) puts quantized flag 0x20 on the next driving
  substep's control, once (payload 0x10).

### Checked against the original (`tests/race_clock_peek.rs`)

`peek/rcv-{help-corolla-34,stuck-206-64,retire-impreza00-45}.peek.bin` (2026-10-16, `harness/play.sh` with
`peek/rcv-race-clock.peek`, slot 0), the peeks of the three recordings `drive-rcv-*.bin`: corolla-34 with the help flag
at step 1956 (the car on its side), 206-64 (the brake wall, `CCar::Update9AF0`) and impreza00-45 (a script character's
box, `E34_RetireA`). The playback's HUD ticks one step's time about every 8 steps, so its clock is not the
standalone's; the test checks each HUD frame instead: every record is the previous one with no tick, or with one
`tick` of a whole number of steps' `facc` (one stalled frame of 14 steps in an earlier run), with the jump start before
it, and with the assistance code on the emitting step. Every field, bit for bit, on every record (3,040 per run).
Seen: one jump start per drive (the start line at 1377, 1401, 1417: +10 s and the countdown's rest), the codes 1, 2
and 2 on steps 1957, 2700 and 2144, and on corolla-34 the +10 s penalty at step 3097 (the timer past 1 s). Flow state
1 and the global 1 in all three: `AssistSettings::QUICK_RACE`, `Game`'s default.

`rbr-standalone --boot` on the three recordings runs 2844 of 2844 steps equal to the original, the time card equal
after each step, and gives the emits on the same steps with the same codes; its own clock (one frame per step) adds the
penalty 1 s of frames later.

### Not verified

- The moving start (`M579590`): no run reaches it (the recorded controls wait for the go). Its place after the tick is
  `M004825b0`'s call order (the camera after the HUD), not a peek.
- The finish's clock stop and its place before the tick: no peeked run reaches the finish.
- In a rally the jump-start penalty is the season's (`E80Daemon::IdxFloat`) and three false starts disqualify; neither
  is ported (10 s is used).
- The HELPCALL item's other conditions (the rescue helicopter idle, `*0x166e378` +0x844 at or below 0) and the pause
  around it.

## Race options

`race_options` holds a quick race's menu options as one typed value, `RaceOptions`: the damage level, the four driver
aids and the weather preset. It maps them to the game's menu globals (`globals`) and to the penalty settings
(`assist_settings`: the damage level is the quick race's penalty level). `weather::random_draws` is `E74_FindBest`'s
random row and the wear draw, run on `crt::CrtRand` (msvcr71 `rand()`). `notes/race-options.md` has the trace and
which boot words each option changes. Checked: the defaults are the reference drives' values (tanner-corolla boot
13000/13000), and damage level 3 matches tanner-damage's boot over 12120/12120 steps. Levels 0 and 2 and the
non-default aids still need drives.

A championship has its own options, `ChampionshipOptions`: the skill level (`Skill`: Rookie, Professional, Champion;
`0x1660c5c`, which New season stores in the season daemon's +0x8e84) and the car damage (`0x1660c70`). The defaults
are `InitSEA`'s (Rookie, Realistic). In mode 3 the skill gives the outside-assistance penalty (10, 20, 35 s) and, at
Champion, shuts the assistance gate; `race_options::damage_level` is `InitQuickRaceDamageHandler`'s pick of the level
per mode (the quick race's in 1, 2, 4 and 5, the championship's in 3, 2 in the school). The season save keeps both
(`Season::set_options`, `season::options`); `rbr-standalone --championship` runs a stage in mode 3 on them.

## Game flow

`flow` is the part of `CGameflow` (E60, the object at `*0x165fa90`) a stage race reads, and the end of the race.

The flow word (+0) is the game mode, `Mode`: 1 the quick rally (the boot's; physics reads the word), 3 the
championship (the boot's with `--championship`, with a season daemon stand-in for the gate's +0x8e84), 7 the rally
school; 2 is the Richard Burns Challenge, 4-6 and 8 the multiplayer modes and 9 the
season's start, which are out of scope. `M00565430` (0x565430) stores the sub-state (+4) with it: 3 for the
championship, 11 otherwise (`Mode::sub_state`; the boot leaves +4 at 0, as before). The gates:
- `M00565e80` (0x565e80), `Mode::finish_to_menu`: modes 1-5 and 8. In these the finish line's marker goes to the HUD's
  finish; in the others it emits `CSigEndRace` at once.
- `M566640` and its twin `M5665f0`, `assistance_gate`: open unless the mode is 3 with the rally's assistance level
  (`*0x7d6128` +0x8e84) at 2 or more. When the flow's host (+0xc98) starts with the half-word 0x7b, both read the mode
  and the level from that host: `SavedRally`, the seam for the saves.

The end of the race (`RaceFlow`, one call a frame after the HUD's clock ticked):
- **Finish.** The finish line's marker (0x16) stops the clock (+0x144, `RaceClock`) and `E62GameMenu::M573530` arms a
  20 s timer (+0x2c0/+0x2c4). The HUD tick (0x579df0) runs it down by the frame's `dt` and, once below 0, emits
  `CSigEndRace` with the state word 0. Until then the car is still driven: the application state stays 1, the clock
  stopped. The end-of-stage marker (0x18) emits `CSigEndRace` at once.
- **Splits.** `E12_GameHost::M72da0` (0x572da0) stores the race clock at each split marker (+0x258, +0x25c); they go
  into the payload. (The payload's word 3, +0x260, is not kept.)
- **Retire.** `CSigRetireCar`'s receiver (`E60Flow::M5662c0`) does not end the race itself. In the school it only sets
  the replay's filter flag. Code 0 does nothing. Codes 1 and 2 call the rescue helicopter (`*0x893638`, a
  `CPC_RescueHelicopter`: slot 0x18 `Vf18` 0x452290 calls `CRescueHelicopter::Activate` 0x49bc90 with 150.0, which
  `Activate` never reads, then loads the rescue Lua and the stretcher). `Activate` places it 30 m back on the
  driveline and zeroes its time (+0x14), so a second call starts the 24 s again. Its `Tic` (0x49c170) posts
  `MSG_RETIRE_BYSTAND` after 24 s, whose callback (0x5693c0) emits `CSigEndRace` with 1. Code
  3 is the rally incident (`HUD::EmitRallyIncident`, +0x27c = 1.0, and `CSigRallyIncident` when the gate is shut); the
  HUD tick runs +0x27c down at 0.98 `dt` and calls the helicopter on the frame it passes 0 when the gate is shut. Code 4
  sets +0x280 = 1.0; the HUD tick runs it down and on the frame it passes 0 posts `MSG_RETIRE_OFFSTAGE`, whose callback
  (0x5797e0) emits `CSigEndRace` with 1. In a quick race (the gate open) code 3 ends nothing.
- **The message box.** Posting (`M0058b060`) queues the box at 0x7d2344 with its callback and emits
  `CSigMenuActivate(0)`; `CApplication::M0047f9c0` in state 1 opens the pause page (`FUN_0056c360`) and sets state 2,
  so the physics stops while the box is up. Its accept (`M0058a170`, the button at +0x28) calls the callback, whose
  `CSigEndRace` moves the game to the end page. `RaceFlow` holds the posted `Message` (`Phase::Message`), `Game`
  pauses (`Stage::pause`), and `RaceFlow::acknowledge` (`FrameInput::acknowledge`) is the accept.
- **Tints.** `HUD::Update` draws the red (0x573630, alpha `1 - t` of +0x27c) and the black (0x5736b0, of +0x280) over
  the 641x482 screen while each timer is not below 0; it returns before them while the helicopter runs
  (`RaceFlow::tints`).
- **Time limit.** The HUD tick emits `CSigEndRace` with 1 once the race clock reaches 1800 s with the helicopter idle.
- `CSigEndRace` is `EndRace`: the race time (+0x140), the two split times, and `retired` (the state word). The
  application's receiver (`CApplication::M00480810`) takes the time as the result for 0 and a retire for 1. `StageEnd`
  is the payload with its cause.
- **After the end.** The receiver calls `M0047f3a0` with the end page's mode, which sets the application state 9 in
  the same call; `Tic` steps no physics in 9 (it ticks the animations and the helicopter), so the frame that emitted
  `CSigEndRace` is the last the car is stepped in. `Game` moves to `State::Ended` (9) there. `EndPage::of` is the
  receiver's page switch: in a quick rally 0x10 (the result) or 0x12 (ENDRACE, after a retire: replay, save replay,
  exit, restart); in the championship 0x13 (`CSeasonDropInUI`, the stage standings), 0x18 (FAILED: finished, but
  `Test3B20` says the car can't drive and the stage index +0x8e8c is not 5; `M005d0960(rally, stage + 1)` retires the
  later stages) or 0x17 (FAILED after a retire; `M005d09b0` retires this and the later stages). Both are
  `E80Entry::FillNeg` (0x5ce300): each stage from the given one whose time is below 0.1 gets -1.0 and the retired
  flag; the rally total (+0x168) is not touched (`season::fill_neg`). Page 0x18
  (`CFailedSeasonFinishedWithBadCarUI`, `vf0c` 0x5b6900) says `MSG_UNFINISH_SERVICE` when the season's queue holds a
  service event (kind 5 or 6), else `MSG_UNFINISH_STAGE`; its CONTINUE goes to 0x17. Any finished race outside modes
  6, 7 and 9 also calls `M00567330` (the stage record), so the championship offers its times to the records.
- **Between the finish line and the end** nothing takes the car: no reader of the clock's stop word (+0x144) or the
  finish timer touches the control, so the player drives on in state 1 (tanner-corolla is bit-exact through these
  874 steps, below).
- **Restart** (the pause menu's RESTART and the ENDRACE page's): a confirm box (0x58dc60, message 0x3b), then
  `SetUnk724_3`, whose action 3 runs after the fade: `InitQuickRace` (0x481d00) → `StartQuickRace` with the menu's
  track and car (`Stage::restart`, state 5). `StartQuickRace` reloads the physics, the car and its setup and tyres
  (`LoadCarLsp`, `LoadQuickRaceSetup`, `LoadTyreBrand`, so the tyre wear is new), resets the HUD (the race clock, the
  flow's timers and splits) and, in modes 1, 2, 4, 7 and 10, the damage handler (`InitQuickRaceDamageHandler`
  0x567640), which also reseeds both rand streams from `time(0)` (`E01SetSeed`, `srand`; a saved rally's host
  reuses its stored seed). The weather is the menu's pick, which the restart does not draw again (`CWeatherDaemon`
  slot 11 only sets the temperatures from the weather kind). So a restart is the first start again: the viewer's R
  in live mode starts the stepping process again from the same command, which boots the same `StartQuickRace`.

`championship` follows that receiver in mode 3: a finished stage gives the season the race time and the three sectors
(split 1, split 2 - split 1, time - split 2), then the next stage; a retire ends the rally (`RallyOutcome::Retired`,
page 0x17). A finished stage whose car can't drive (`Test3B20` 0) before the last stage counts, but ends the rally
(`RallyOutcome::Wrecked`, page 0x18).
The stages come from `season.dat` on their season-table rows, the service stops from the season's events (`service`:
budget, repairs over the car's `repair.ini` tree at 30x, late penalty), and the damage the car ends a stage with is
carried into the next (`car_damage`), the tyres' wear (+0x3d04) with it until a service's refresh clears it. The
setup and the tyres are picked only after a service or the rally's start (page 0x1a's `CONTINUE_SEASON`, then
`CSeasonChooseSetupUI` and `SELECTTYRES`, `championship::choice_before`); a stage with no service before it starts
on the car as it is. `season_store` writes the player's times into the `RBR7` season, and a service's late penalty
as the park closes (`record_penalty`, `Penalty` 0x5cf330 adds it at once).

### Ours

- The pause page behind a message box is not modelled (the viewer dims the screen); recorded payloads accept the box
  at once (`rbr-standalone`), as a recording has no accept.
- The helicopter's `Tic` runs after the HUD tick here.
- A race left before its end (the viewer closed, `RaceFlow::end_now`): finished when the finish line was passed, else a
  retire, both with the cause `Left`. The game has no such end: its pause menu's exit leaves no result.
- The pause menu (`pause_menu`): no `OPTIONS` item and no multiplayer item; it also opens outside the driving state
  (the start orbit, the end page), offering Resume, page 1's Restart and exit, and after a championship stage's end
  Continue. The exit closes the viewer; a quick race's records are written at its end only, so it leaves none, as
  the game's.
- The rally's total time and the service stop.

### Checked

- `tests/game_loop.rs`: the tanner-corolla recording passes the finish line at step 34,530 and the end-of-stage marker
  (0x18) at step 35,404, where the flow ends the race (finished, race time 233.67441 s, splits 69.84889 and 143.56372 s: the test
  gives the go frame no time, as the original's playback peek has it; the boot and live play give it a step and
  read 233.68141, 69.855896 and 143.57072 s, `lead/reports/race-clock.md`). The
  original's recording ends one step later (post 35,405). That step is the frame's second substep, not a step after
  the end: `CPC_Application::Vf0c` runs `CApplication::Tic` (0x44a764; state 1 steps the frame's substeps in
  `TicPhysicsRecording`) before `M0047ca70` (0x44a76b; the physics' `Vf0C` 0x526a10 → `CCar::Vf28x` → `Cb930` →
  `Load3b0`), so the marker tick sees the car only after all of a frame's substeps. The end key (4269.9297 m) is crossed
  at post 35,404 (4269.9995 m); the hand drive's last frame ran 35,404 and 35,405 (their controls are equal, the step
  before differs), and its tick then ended the race. FRM1 has no frame boundaries, so the boot steps one substep a
  frame and ends after 35,404 (`lead/reports/vf28x.md`). `rbr-standalone --boot` over the whole recording: 35,248 of 35,248 steps equal,
  the 874 after the finish line included.

### Not verified

- No peeked run reaches the finish timer, a retire's end or the time limit; the constants and branches are the asm's.
- The ramp at HUD +0x294..+0x2a4 (the "WARNING" box 0x575460 above 0, and the HUD tick's `CSigEndRace` after 5 s of
  it) is dead in 1.02: only the HUD reset writes its rate +0x298 (0x5774c8, zero), so it stays 0 (setpieces).

`rbr-standalone` stops a run through the flow: at the finish line (`RaceFlow::racing`), or at the race's end. The
v6 Accent drive (`drive-v6-gravel-accent-54`, track-54_E, finish at step 36,971) stops there with `--boot`, every step
equal.

## Filter (G5)

`filter` is the input filter between the poll and the quantizer: `Filter::notify` (`NotifyB`) stores a poll's raw control
and its cell selectors, `Filter::tic` (`Tic`) filters each channel from its raw value and its last output and clears the
action bits, and `QuantizedControl::pack` (`Pack`, in `control`) writes the nine bytes. The arithmetic follows the asm:
`Step`'s curve is `((c3 * m2 + c2) * m2 + lin * m) * sign` with `m2 = m * m` (the decompile's `(c3 * m * m ...)` rounds
differently), and the steering byte is `(s + 1) * 127.5 - 128` truncated, with each operation rounded to f32. The pedal
bytes are `x * 255 + 0.5` truncated, per operation in f32: the game's x87 runs at 24-bit precision after device creation
(`notes/rust-physics.md`).

Where the parameters come from. The grid is 5 channels by 4 cells, each group 0x94 bytes: the selector word, then four
cells of nine words (active, nonlinear, linear, quadratic, cubic, rise, fall, speed fade, fade speed). The quick race
(`StartQuickRace`, 0x481660) starts from `E16_InitGrid` (zero, linear 1.0) and `E16_LoadINI`, which fills steering
cells 0 (`STEER::AXIS`), 1 (`STICK`) and 3 (`BUTTON_DIGITAL`) from `PC_InputFilter.ini` in the game folder, then
`E16_Reset`. `Grid::from_settings` does the same. `E16Filter::ReadBin` (0x48bd60) replaces the grid with the saved
one from the profile (`SavedGames/pf<name>.rbr`, `E20ReadProfile`). That profile is encrypted and is not read; the
grid here is the INI's.

What the check shows (`tests/filter_peek.rs`, `peek/g5-filter`, one playback run of tanner-corolla, 12,084 steps):
- The grid equals the peek at every step (every probed field of cells 0 to 3 of steering, and the active flag of the
  other channels' cell 0). `Reset` changes one value's meaning: the AXIS cell is active although the INI says
  `Active = false`, because its fade is 0.3, and `Reset` sets the flag from the values. Both the peek and the code agree.
- The filter and the quantizer's bytes equal the peek at every step, on the run's inputs. Those inputs are zero (the
  player's device is idle in the playback): the raw channels, the quantized bytes and the flags are 0 on every step,
  and the selectors are 0 for steering and pedals and 3 (the button cell) for handbrake and clutch from the first driving
  step. So this check exercises the copies, the grid lookup and the zero path, not the curve or the rates.
- The payload against the recording (`payload_matches_the_recording`, ignored) differs from step 161, the first driving
  step: the recorded word 0 is 1.0, ours 0. The playback emits the recorded control after the filter
  (`harness/plugin`, `OnPlayerControl` and `CallEmit`), so the filter never sees it.

Not checked, and why:
- The curve, the rise and fall limits, the speed fade, and the steering byte on non-zero input: no drive puts a non-zero
  input through the filter, so they are checked by direct calls only. `tests/filter_vectors.rs` replays fuzzed calls of
  `Step` (0x48af90) and `Pack` (0x48c910) from `harness/vector_specs/gameplay_filter.py` (`harness/direct_calls.py
  --set gameplay_filter`, recorded on cachy). The grid is the playback's header (read from the snapshot's app); half the
  calls give every cell random parameters, because the playback's pedal cells are inactive. `Tic` is not called
  directly; its flags and outputs are checked on the zero run only.
- `StepB`'s other branch (`f` nonzero, which reads `NotifyA`'s vectors), `EvalA` (0x48bbe0; its caller is not traced),
  `ReadBin`/`WriteBin` (the profile), and the filter's counters (+0x3d0, +0x3d8, +0x390) which the outputs do not read.
- `StepB` (f = 0) is written from the decompile only; nothing on a drive's path calls it.
- The key lookup's case rule in `CConfig` is not traced. The vanilla file's only mixed-case keys (`maxSpeedFade`) carry
  the code's defaults, so the grid is the same either way.
- Byte-to-name, settled: byte 4 is the throttle. It is filtered channel 1 (+0x18), the payload's word 0. The viewer
  measured word 0 as the throttle (at 1.0 it speeds the car up, `notes/gameplay-loop.md`), and the physics port reads
  packet +0x00 as its throttle lane (`rbr-physics` `vcu/layout.rs`, `Packet`), which is matched against the original's
  calls. The poll's names for raw +0x18 (brake) and +0x1c (throttle) in `notes/gameplay-loop.md` are the other way round
  from the car; whether the physical pedals reach those offsets as those names say is not measured here.
