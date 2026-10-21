# rbr-gameplay

The 1.02 gameplay loop around the physics step, pure and checked against the original. Behaviour and evidence:
[notes/gameplay.md](../../notes/gameplay.md). Each module's doc comment names the functions it ports.

| Module | What it does |
| --- | --- |
| `game` | One composed frame: settle, poll, stage tick, filter and payload, substeps, time record, marker ticker, race clock |
| `tick` | Frame clock: wall time to whole steps, `facc`, carry, single-step gate, settle |
| `stage` | Application states: set-up, load settle, countdown, driving, pause, end page; payload car requests |
| `camera_go` | Start orbit timer that gives the go (live play) |
| `control` | Quantized control bytes to the 0x20-byte payload |
| `filter`, `ini` | Input filter grid from `PC_InputFilter.ini`, `Step`, `Pack` |
| `poll` | Device poll (`USInputManagerImpl::Tic`, NGP's ignition and gears), RSF session replay, Wine's view of a Linux device |
| `ffb` | Wheel force-feedback command, 1.02 and NGP |
| `driveline` | Stage distance and track-direction dot from `.trk`; wrong-way test |
| `timing` | Time card and timekeeper: start, splits, finish, still counter; the f32 clock |
| `markers` | Marker ticker: start/split/finish/end events, pace-note lookahead |
| `race_clock` | HUD race clock: countdown, go, jump start, assistance penalty, help gate |
| `flow` | Flow modes and gates; finish timer, retires, time limit, `CSigEndRace`, end pages |
| `pause_menu` | Pause menu items per mode, confirm boxes, retire |
| `result` | Stage result: running, finished or retired, time, penalties, splits |
| `record` | Stage record and personal best in `pfresult.rst` and `pf<name>.rbr` |
| `race_options` | Quick race and championship options to menu globals and penalty settings |
| `weather` | Quick race weather row and wear draw; default tyre kind |
| `crt` | msvcr71 `rand()` and `qsort` |
| `setup` | Typed car setup, ranges, defaults, slider steps, setup file text |
| `ghost` | Ghost car recording and playback |
| `car_damage` | Damage carried between stages; part severity and repair |
| `championship` | A rally's stages, end-of-stage handling, carried damage and tyres |
| `service/` | Service park: budget, part tree, orders, repairs, late penalty |
| `season/` | Season creation, teams, forecast weather, points |
| `season/layout` | Named view over the season save: position, clock, ranks, results, orders; the save-layout adapter |
| `season_events` | Season clock and event queue, AI stages and services, player turns |
| `season_store` | Season in the profile's `RBR7` block |
| `ai_season/` | AI driver's stage and service on `rand()` |

## Tests

`tests/` (most need `RBR_DATA` and `RBR_RECORDINGS`; peeks need `peek/<drive>.bin` and `.peek.bin`):
`recordings` (payload), `schedule` (frame schedule), `stage_peek`, `timing`, `timing_peek`, `race_clock_peek`,
`filter_peek`, `filter_vectors` and `marker_vectors` (direct-call records, `harness/vector_specs/`), `ghost`, `setup`, `weather`. Vector records: see
[harness/README.md](../../harness/README.md).
