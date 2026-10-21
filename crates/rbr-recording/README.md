# rbr-recording

Readers and writers for the harness recordings and for the live records `rbr-standalone --live` sends the viewer.
FRM1 layout: `src/lib.rs`; context: [notes/verification.md](../../notes/verification.md#recordings).

## Record tags

FRM1 frames are the recordings (`drive-*.bin`). A live process answers each control payload with an FRM1 frame,
then HUD1 or HUD2 and DMG1, then tagged records (4-byte tag, u32 length, payload; `live`) in any order, and last
SCN1, which ends them. Readers skip tags they do not know.

| Tag | Module | Payload |
| --- | --- | --- |
| `FRM1` | `lib` | one physics step: counters, substeps, control, `CCar`, four `CWheel` blocks |
| `HUD1`, `HUD2` | `hud` | gear, engine speed, car speed; HUD2 adds the race clock |
| `DMG1` | `damage` | damage zones and the last damage event |
| `DSH1` | `dash` | warning lamps and dash values |
| `PTC1` | `particles` | each signalling wheel's `CSigTireParticles` (0xB0 bytes, up to 16) |
| `PEV1` | `particles` | particle and light events (exhaust, lamps) |
| `DET1` | `detach` | detached part poses |
| `SND1` | `sound` | engine, drivetrain, tyre-noise, damage and sensor payloads |
| `NSD1`, `FMB1` | `sound` | NGP sound words; the FMOD block |
| `PCN1` | `pacenote` | the step's pace-note callouts |
| `SPL1` | `splits` | splits and the finish |
| `CLK1` | `clock` | step length and load-settle steps (1.02: 7 ms, 160; NGP: 1/720 s, 1520) |
| `CAM1` | `camera` | start camera |
| `FLW1` | `flow` | race-flow events (helicopter, tints, message box, end page) |
| `FFB1` | `ffb` | steering force |
| `GBX1` | `gearbox` | gearbox state for rev-matching |
| `SHF1` | `shift` | shift points |
| `CDM1` | `damage` | the service park's damage words (the HUD's car status) |
| `MNU1` | `menu` | pause-menu state |
| `SCN1` | `live` | awake scenery poses (a count, 32 bytes each); always last |
| `INP1` | `input` | the viewer's device input, sent to the process |
| `PEK1` | `peek` | harness pointer-probe log |

`pose` reads car and wheel poses from a frame (offsets with evidence in `src/pose.rs`); `trace` reads capture traces.

## Tools

- `rec_info <file>`: frame count, substeps, first and last steps.
- `drive_path <file> [--every N] [--csv out.csv]`: position, speed and heading every N steps.
- `peek_info <file>`, `trace_info <file>`: summaries of peek logs and traces.

## Checks

`cargo test -p rbr-recording`. `tests/drives.rs` needs `RBR_RECORDINGS` (and `RBR_DATA` for the start check): every
frame finite, unit quaternions, wheel anchors within 3 m, speed under 100 m/s, start within 3 m, and the body frame
(z-up, forward is body -Y, front wheels at y ≈ -1.09).
