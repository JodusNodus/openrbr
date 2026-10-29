# Gameplay

The 1.02 gameplay around the physics step: input, the frame loop, race flow, options, weather, scripted events,
championship and saves. Code: `crates/rbr-gameplay/src/` (module map:
[crates/rbr-gameplay/README.md](../crates/rbr-gameplay/README.md)); each section names the module that states the
detail. File layouts are in [formats.md](formats.md); cameras and HUD in
[cameras-hud-launcher.md](cameras-hud-launcher.md). Addresses are `RichardBurnsRally_SSE.exe` 1.02. Detail dropped
here: rbr-recomp `notes/gameplay-loop.md`, `notes/scripted-events.md`, `notes/weather.md`.

## Frame and fixed step

Mechanics in `tick.rs` (clock, clamp, whole steps, `facc`, settle counts) and `game.rs` (frame order, per-substep
work); 7 ms steps and the 160 settle steps in `rbr_recording::clock`. Check: `tests/schedule.rs` (all four drives).

- The step is 0.007 s from retail `Root/physics.rbz` (the CHELALIC `physics.dat` copy's 0.01 is not loaded).
- The frame's physics call is the physics' `Vf0C` (0x526a10).

## Control payload

Words, pack bytes, flag map and the NGP handler in `control.rs` (`QuantizedControl::pack`, `car_control`,
`FLAG_MAP`, `CarControl::ngp`); the car side in `rbr-physics/src/vcu/` (`layout.rs` `read_packet`); FRM1 in
`tests/recordings.rs`. Words: throttle, brake, handbrake, clutch, steering (float), flags, gear, aux.

- Measured: throttle 1.0 speeds the car up; brake 0 to 1 (`control.rs` cites this note for both).
- Steering comes back out through `Unpack` (0x48c9c0): `byte * 2 / 255`.

### Device poll

Mechanics in `poll/mod.rs` (devices, actions, `GetActionValue` sets, `Changed`, `Invert*Axis`, steering cells, emit)
and `poll/wine.rs` (Proton path, G923 lattice, rest values); NGP additions in `control.rs` and `notes/ngp.md`. Check:
`rbr_render::bin/input_check` (RSF sessions, "Input chain check").

- NGP version addresses (code states 778's only): 779 polls at 0x10006e70, its direct gears at 0x1003c4d0. The strand
  the 1000 Hz thread posts to is 0x10098270 -> 0x10094200 (not in code).
- RBRControls (RSF) sets properties and FFB on device instances of its own (MOD1: PRP1 9, DEV1 5, no POL1); the
  chain replays exactly without it.
- Live: checked on cachy (autopilot, `rec/input-own`): G923 and T500 RS found, read at rest (0, gears bound). The
  keyboard state is the frame's held keys: a sub-frame tap is missed (unverified).
- No import of RSF's controls or settings (user decision: the player sets them again); removed with
  `rbr-launcher/src/rsf_import.rs`, `rbr_formats::wine`.

### Input filter

Mechanics in `filter.rs` (grid, curve, `Step`, `Pack`, `notify_a`, `Reset`, profile grid, `without_wheel_fade`,
`is_made_up`) and `ini.rs`; the NGP-path-only live rule in `rbr-standalone/src/live/mod.rs` (`Grid::from_profile`,
`new_game`). Checks: `tests/filter_peek.rs` (grid and zero-input path per step), `tests/filter_vectors.rs` with
`harness/vector_specs/gameplay_filter.py` (curve and `Pack` fuzzed).

- RSF renames the install's file to `PC_InputFilter.ini.bak`, leaving the built-in grid, whose digital steering runs at a tenth of the retail file's rates (not in code).
- Live play takes the profile grid from `--saves`/`--profile` on the NGP path only; FRM1 holds the payload after the filter, so `--boot` never runs it.

## Stage states

Machine in `stage.rs`, the go in `camera_go.rs`, the live accept in `rbr-standalone/src/live/mod.rs`
(`ACCEPT_MESSAGE`). Check: `tests/stage_peek.rs` (every step of `peek/g7-tanner-corolla`).

- The mode rule (`M8840` 0x4888bf) and the 2.0 s arm are asm only: no recording starts a championship stage.

## Timing and markers

Mechanics in `markers.rs` and `timing.rs` (keys, `CTimeKeeper`, `Record`, f32 clock). Checks: `tests/timing.rs`
(tanner-corolla steps), `tests/timing_peek.rs` (every field, every step, two drives).

- Between the six fired-bit checkpoints (tarmac-lancer, arctic-snow, tanner-damage) unchecked: the snapshots' key
  list is not read. The old `tests/game_loop.rs` assertion is removed with the test (it needed 1.02's f32 clock).

## Race clock and start

Mechanics in `race_clock.rs` (countdown, go, jump start, penalties, outside assistance, HELPCALL). Check:
`tests/race_clock_peek.rs` (three peeked assist drives, bit for bit).

## Race end

Mechanics in `flow.rs` (flow words, finish, replay limit, retire codes, end pages, restart), `result.rs` and
`pause_menu.rs` (page 1 has RESTART, page 2 none; no OPTIONS item is ours). Checks: `tests/` drive expectations,
`live_restart_answers_each_frame_again` in `rbr-standalone/src/live/tests.rs` (500 autopilot frames answer byte for
byte again).

- tanner-corolla runs 874 steps past the finish with the clock stopped (evidence for the 20 s timer rule).
- tanner-corolla: finish 233.67441 s, splits 69.84889 and 143.56372 s, the go frame getting no time; boot and live give it a step (233.68141, 69.855896, 143.57072 s).
- Full replay buffer (`ReplayRuns`), not in code: `E19ClsScan` (0x499ff0) keeps runs of equal nine-byte controls
  (max 0xffff steps each); at run index 0x20f56 `E19QccFire` (0x49af10) emits no control, the step still runs, then
  `TicPhysicsRecording` (0x47f4fb) emits `CSigEndRace` with 1 and zero times (a control changing every step fills
  it after 945 s). NGP replaces both calls (0x47f4c3, 0x47f4ce) with a growing buffer. Unreached by any recording:
  unverified.
- Ours: a race left early (`RaceFlow::end_now`) ends with cause `Left`.
- Not compared with an original ghost (outside-assistance penalty in the ghost clock).

## Scripted events

Gameplay effects in `stage.rs`/`game.rs` (HELPCALL path, `CSigCarProblem`, stuck test) and
`rbr_render::characters::scripted` (flip spectators, helicopter, bird hit, marshals); randomness in the race clock
path (`--seed` is the viewer's). Evidence drives: `drive-rcv-help-corolla-34`, `-stuck-206-64`,
`-retire-impreza00-45` (checks in `rbr-gameplay/tests/race_clock_peek.rs`,
`rbr-render/tests/scripted_data.rs`).

- The flip spectators are five `load_car_flipN.lua`, cosmetic; `run_car_flip.lua` never runs.
- Bird strike feeds only the sound. Crash-site scripts are unused in retail.

## Race options

Mechanics in `race_options.rs` (menu words, damage level, aids, penalties, gate). Checks: level 1 with default aids
(tanner-corolla 13000/13000), level 3 (tanner-damage 12120/12120), championship Rookie and Champion on
tanner-corolla.

- The aids go into the control unit's setup through `InitializeCarSetting` (0x547530).

- Not checked: levels 0 and 2, non-default aids, auto gears.

## Weather

Mechanics in `weather.rs` (daemon, season-table draw, grip path); ours in `rbr-standalone/src/args.rs` (`--weather`,
`--season-row`, `--wear`, `--wetness`) and the launcher. Sky, fog, precipitation, windscreen and wipers are
presentation: [renderer.md](renderer.md), [sound.md](sound.md).

- Any season-table row starts like Random with that row (`--weather 1 --season-row K`); good rows come from `misc/nice_w.ini`, bad rows from `bad_w.ini`.
- Wet surfaces are separate physics records with lower friction (wet/dry static 0.71-0.88, not in code).

## Championship and service

Mechanics in `championship.rs` (time, sectors, retire rule), `season/` (position, clock, rank, result views;
byte-exact save), `season_events.rs`, `season_store.rs`, `ai_season/`, `service/`, `setup.rs`. The `rand()` order is the asm's
(`ai_season`); the driver classes' draws are ours (not in code).

## Damage

Mechanics in `car_damage.rs` (carried damage, part severity, repair, service refresh). Physics internals:
[physics.md](physics.md).

## Saves

Mechanics in `record.rs` (submit, personal best, `times.ini`) and `season_store.rs`; layouts in [formats.md](formats.md).
The viewer compares against `misc/times.ini` without `--saves` (`rbr_render::hud::splits`).

- The submit rule is not checked against the original.
