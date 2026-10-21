# Status

Every 1.02 gameplay system and where it stands in Rust. G marks gameplay-critical systems (they change times,
penalties, results or the car's motion). States: done, partial, out (the user's decision), user (the user's own work).
"Original run" means a run of the original game (on a Windows or Wine machine) is still wanted to compare. How checks work:
[verification.md](verification.md). Detail per row: rbr-recomp `notes/progress/systems.md`. Current state and decisions
in force are at the top and the bottom, one line each; history is in git, the log up to the split in rbr-recomp's
`LOG.md` and `notes/archive/`.

## Current state (2026-10-19)

- **Repository:** ReRBR, the Rust reimplementation only. The C++ reconstruction and its notes are in rbr-recomp
  (frozen).
- **Physics:** about 930 ported functions (`crates/rbr-physics/ports.csv`), about 660 bindings
  (`crates/rbr-hybrid/bound.csv`). The pure step runs NGP only (`rbr-standalone --boot` needs `--ngp-physics`; 1.02
  snapshots are refused); drive2 attempt 5 runs 84084/84084 equal (`harness/rsf_compare.py`).
- **Load path:** `rbr-standalone --boot` builds the physics state from the game files and NGP's `physics.rbz`.
- **Gameplay loop:** done on the recorded drives (control payload, stage states, timing, time card), plus the
  championship, service park and saves.
- **Gate matrix:** the 1.02 drives (4 hand-recorded, 35 playback-made) feed the hybrid check and playback only;
  `rbr-standalone` no longer steps them. The NGP gate is drive2 attempt 5 (snow, Ford Focus RS WRC 06, RSF car 55) and `harness/matrix.py` (NGP
  playback-made pairs on NGP's four car slots), see [verification.md](verification.md#drives-and-the-gate-matrix).
- **NGP 7.4.778:** physics, load path, force feedback and the H-shifter in Rust ([ngp.md](ngp.md)).
- **Presentation:** renderer, sound, HUD, cameras and launcher in live play; leftovers in the tables below.

## Driving and stage flow

| System | State | Open items |
| --- | --- | --- |
| G Physics step (all subsystems) | done: `rbr-physics`, `rbr-hybrid` pure step (NGP only) | NGP gate matrix to every car and surface |
| G Surfaces, grip, weather tables, tyres | done | - |
| G Damage model, physical detach | done | - |
| G Damage level option | done | original runs for levels 0 and 2 |
| G Brake walls, stuck detection, recovery | done | - |
| G Outside-assistance penalty | done | 20/35 s not compared with the original |
| G Retire and end pages | done | original run: a championship retire and a wrecked finish, a stop in the finish zone, a full replay buffer (945 s) |
| G Physics clock and frame scheduling | done | - |
| G Stage and car load | done | map 68 has no weather row |
| G Community stages (RSF) | partial | full-install sweep; textures from other variants; encrypted stages not read |
| G Community cars (RSF NGP packs) | partial | FMOD `wheel` events, `gear`/`state` parameters; option folders |
| G Load settle, start states, the go | done | original run: the go's frame count, a championship start (confirm, 2 s) |
| G Countdown, race clock, jump start | done | moving start checked only on playback |
| G Time keeper, splits, finish | done | speed start and split 1 have no peek; a hand drive held 20 s past the finish |
| G Marker ticker, pace-note callouts | done | fired counts checked at six snapshot checkpoints only |
| G Stage result, records, personal best | done | original run: a drive that beats a record |
| G Restart | done | in-process restart in live mode |
| Start and finish set pieces | done | original run: the start marshal |

## Modes, setup, options

| System | State | Open items |
| --- | --- | --- |
| G Quick rally | done | - |
| G Game flow state machine | done | - |
| G Rally, service park, tyre and setup choice | done | original run: time-out message, NGP tyre rows; recce/shakedown |
| G Championship season, AI, forecast, offers, trials | done | original run: a season golden, the original reading our save |
| G Unlocks, rally school, RB Challenge, hot seat | out | - |
| Profiles and save files | done | which profile options the launcher shows (user) |
| G Default setup, option selection | done | - |
| G Player-edited and saved setups | done | original run: a drive with a changed setup; a golden slot file |
| G Driver aids | done | original run: a drive per changed aid; no auto-gears recording |
| G Season difficulty, weather draw, pace-note settings | done | - |
| Particle and render quality options | out | always highest |

## Presentation

| System | State | Open items |
| --- | --- | --- |
| Co-driver phrases, countdown, damage calls | done | not listened to against the original; option gate inferred |
| Engine, transmission, turbo, pops | done | harness hook for `CEngine +0x101c` |
| Tyres, suspension, impacts | done | run the frame finish read-only |
| Wind, rain, ambient, scripted sounds | done | trace `E25Rain::Render90`; listen live |
| Music | out | - |
| Stage, sky, car rendering | done | - |
| HUD, messages, dash lamps, pace-note symbols, units | done | result page from a real finish |
| RSF working dials | done | oil, tyre health fixed; `Damage` unverified |
| Particles | partial | soft edges; rates against a capture |
| Skid marks | done | - |
| Car lights and flares | partial | recordings without live data approximate |
| Deformation, cracks, detached parts, dirt | partial | door detach |
| Interactive scenery, fences | done | - |
| Stage characters, in-car crew, car-flip marshals | done | original run: game captures |
| Rescue helicopter, bird strike | done | original run: clip orientation |
| Rain and snow particles | done | - |
| Windscreen, wipers | partial | crack normal map, camera dirt |
| Night mode, render enhancements | done (ours) | - |
| Cockpit mirrors | done (ours) | RSF Fiesta Rally2: none found; images not anti-aliased |
| In-car and chase cameras | done | mode 3 ground probe; chase capture |
| Start orbit, helicam, seat shake | done | - |
| Replay cameras, replays, replay filters | out | - |
| Ghost car (1.02 and RSF) | done | best-ghost store; original run: a ghost with a penalty |

## Input

| System | State | Open items |
| --- | --- | --- |
| G Input filter, quantisation | done | - |
| Devices, bindings, pause menu | done | the original's default map is not reproduced |
| Force feedback | user | - |
| G NGP physics | user | - |

## Next

1. Runs of the original: damage levels 0 and 2, each driver aid, a changed setup, the go's timing, a season
   golden, a retire, a record-beating drive.
2. Extend the NGP gate matrix ([verification.md](verification.md#drives-and-the-gate-matrix)) to whole stages for
   every car and surface class, with damage and collisions.
3. Community stages and cars: the full RSF sweep, the FMOD leftovers, encrypted stages.
4. Presentation leftovers: particle edges, door detach, windscreen cracks, game captures to compare.

## Decisions

- 2026-10-21 (user): ReRBR steps NGP physics only. `rbr-standalone --boot` needs `--ngp-physics`, the viewer and
  launcher run through it, and the oracle is drive2 attempt 5 (physics PASS 84084/84084). The 1.02 behaviour
  branches inside `rbr-physics`/`rbr-hybrid` are removed (NGP-only ports). The `Variant` type is gone: the x87 precision is the thread's (`fpu::precision()`), and the
  `ngp` flags of gameplay, render, recording and standalone with it (`ControlPath::Quantized` is the 1.02 leftover).
- 2026-10-20 (user): the porting workbench (the exe's function map, worklist, progress tracking, rename tables) moves
  to rbr-recomp's `rerbr/`. ReRBR keeps the Rust, the layouts and addresses that key the vectors and bindings, and
  the checks.
- 2026-10-19 (user): the repository becomes ReRBR, Rust only; the C++ reconstruction moves to rbr-recomp.
- 2026-10-19: the repository holds no private paths or hosts: the harness reaches the test machine through
  `RBR_REMOTE_*` variables ([verification.md](verification.md#test-machine)), and the test data comes from the
  player's disc and patch (`rerbr --extract-test-data`).
- 2026-10-19: the documentation is cut to one note per area; detail as of the split stays in rbr-recomp under the
  same paths.
- 2026-10-17 (user): the goal is RBR with NGP 7 physics and its NGP cars, on the original and the modded stages, all
  in live play ([notes/rust-plan.md](rust-plan.md)). NGP and force feedback are the user's code.
- 2026-10-17 (user): HUD modern and simple with every feature of the original; sound and visuals may be better than
  the original; macOS and Linux first, Windows later; English only.
- 2026-10-16 (user): particles aim to look good, not to match 1.02; the windscreen gets a modern water-on-glass
  system ([notes/renderer.md](renderer.md)).
- 2026-10-16 (user): a new basic launcher starts the Rust game; it is not the 1.02 frontend.
- 2026-10-16: the headline metric is how far `rbr-standalone` runs pure and equal on every drive, and its stop list.
- 2026-10-15: the C++ physics is frozen; Rust is translated a group at a time and checked against the original
  directly. A group is bound whole and checked by one run; `--only` and bisect only on a divergence; fuzzed records
  only for unreached branches. Rust matches every word the gate compares.
- 2026-10-15: the gameplay loop is written directly in Rust and checked against recordings; 1.02 replays deferred.
- 2026-10-13: no matching or coverage work for UI, menus, rendering code, HedgeHog3D or LocoMotion.
- The reference is the official 1.02 binaries, not the RSF build. Never modify a player's RSF install; runs of the
  original use a separate vanilla 1.02 install.
- Existing crates over hand-written parsers.
- Contributors work on a branch or worktree each and merge through `integration/handoff`.
