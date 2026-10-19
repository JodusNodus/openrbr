# Status

Every 1.02 gameplay system and where it stands in Rust. G marks gameplay-critical systems (they change times,
penalties, results or the car's motion). States: done, partial, out (the user's decision), user (the user's own work).
"Cachy" means a run of the original on cachy is still wanted to compare. How checks work:
[verification.md](verification.md). Detail per row: rbr-recomp `notes/progress/systems.md`.

## Driving and stage flow

| System | State | Open items |
| --- | --- | --- |
| G Physics step (all subsystems) | done: `rbr-physics`, `rbr-hybrid` pure step | gate matrix to every car and surface (`drives.csv`) |
| G Surfaces, grip, weather tables, tyres | done | - |
| G Damage model, physical detach | done | - |
| G Damage level option | done | cachy drives for levels 0 and 2 |
| G Brake walls, stuck detection, recovery | done | - |
| G Outside-assistance penalty | done | 20/35 s not compared with the original |
| G Retire and end pages | done | cachy: a championship retire and a wrecked finish |
| G Physics clock and frame scheduling | done | - |
| G Stage and car load | done | map 68 has no weather row |
| G Community stages (RSF) | partial | full-install sweep; textures from other variants; encrypted stages not read |
| G Community cars (RSF NGP packs) | partial | FMOD `wheel` events, `gear`/`state` parameters; option folders |
| G Load settle, start states, the go | done | cachy: the go's frame count |
| G Countdown, race clock, jump start | done | moving start checked only on playback |
| G Time keeper, splits, finish | done | speed start and split 1 have no peek; a hand drive held 20 s past the finish |
| G Marker ticker, pace-note callouts | done | fired counts on long drives (no SeekD state in FRM1) |
| G Stage result, records, personal best | done | cachy: a drive that beats a record |
| G Restart | done | in-process restart in live mode |
| Start and finish set pieces | done | cachy: the start marshal |

## Modes, setup, options

| System | State | Open items |
| --- | --- | --- |
| G Quick rally | done | - |
| G Game flow state machine | done | - |
| G Rally, service park, tyre and setup choice | done | cachy: time-out message, NGP tyre rows; recce/shakedown |
| G Championship season, AI, forecast, offers, trials | done | cachy: a season golden, the original reading our save |
| G Unlocks, rally school, RB Challenge, hot seat | out | - |
| Profiles and save files | done | which profile options the launcher shows (user) |
| G Default setup, option selection | done | - |
| G Player-edited and saved setups | done | cachy: a drive with a changed setup; a golden slot file |
| G Driver aids | done | cachy: a drive per changed aid; no auto-gears recording |
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
| Stage characters, in-car crew, car-flip marshals | done | cachy: game captures |
| Rescue helicopter, bird strike | done | cachy: clip orientation |
| Rain and snow particles | done | - |
| Windscreen, wipers | partial | crack normal map, camera dirt |
| Night mode, render enhancements | done (ours) | - |
| In-car and chase cameras | done | mode 3 ground probe; chase capture |
| Start orbit, helicam, seat shake | done | - |
| Replay cameras, replays, replay filters | out | - |
| Ghost car (1.02 and RSF) | done | best-ghost store |

## Input

| System | State | Open items |
| --- | --- | --- |
| G Input filter, quantisation | done | - |
| Devices, bindings, pause menu | done | the original's default map is not reproduced |
| Force feedback | user | - |
| G NGP physics | user | - |

## Next

1. Cachy runs of the original: damage levels 0 and 2, each driver aid, a changed setup, the go's timing, a season
   golden, a retire, a record-beating drive.
2. Extend the gate matrix (`drives.csv`) to every car and surface class.
3. Community stages and cars: the full RSF sweep and the FMOD leftovers.
4. Presentation leftovers: particle edges, door detach, windscreen cracks, game captures to compare.
