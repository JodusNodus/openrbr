# rbr-recording

Reader for the per-step physics recordings the runtime harness writes (`harness/plugin/RBRRecompHarness.cpp`,
`Record`). The frame layout is in `src/lib.rs`.

## Poses

`pose` reads the car and wheel poses from a frame. The offsets are named constants in `src/pose.rs`, each with its
evidence (`src/rbr` file and line, or `notes/physics/`). The API returns typed values only.

| value | block, offset | meaning | evidence |
| --- | --- | --- | --- |
| car position | `CCar` +0x140, 3 floats, m | world position (inferred) | `Snap9360` (`RBR_E34_CarD.cpp:1371`), `notes/physics/car-body.md` 2a |
| car orientation | `CCar` +0x100, 4 floats | unit quaternion `x, y, z, w`, active rotation | `Snap9360` (`:1372`), `rigid_body/layout.rs`; unit length and the axis checks on the drives |
| car velocity | `CCar` +0x1c0, 3 floats, m/s | velocity | `car-body.md` 2a (strong) |
| wheel anchor | `CWheel` +0x140, 3 floats, m | mount point in the car frame | `RBR_E51_WheelC.cpp:275-276`, `RBR_E51_WheelD.cpp:375`; geometry checks |
| wheel orientation | `CWheel` +0x100, 4 floats | quaternion; frame not proven | `RBR_E51_WheelB.cpp:268-276` |
| wheel roll | `CWheel` +0x5ac, rad | roll about the axle, wrapped at 20π | `RBR_E51_WheelD.cpp:747-760`, `:918-935`; `Snap9360` (`RBR_E34_CarD.cpp:1385`) |
| strut travel | `CWheel` +0x5bc, m | spring coordinate | `RBR_E51_WheelD.cpp:408-412`; `Snap9360` (`:1384`) |

Not in the recordings: the wheel block holds only the first 0xa00 bytes of `CWheel` (`kWheelBytes`), so the spin rate
(+0xa50) and the ground flag (+0xebc) are missing. Steering is not in the wheel block; the steering target is in the
car (`notes/physics/car-body.md` 2a, `+0xb0c`, inferred).

## Frame

Checked on the reference drives (`tests/drives.rs`, `drives_fit_the_body_frame`):

- The physics is z-up: `.lbs` files are y-up and `.trk` z-up (`rbr_formats::lbs`). Body `+Z` is world up (mean dot
  0.96 to 0.998 over the four drives).
- The car drives along body `-Y`: `CarPose::forward` against the velocity averages 0.95 to 0.99 for frames above
  5 m/s.
- World points are `position + R·local`, `R` the active rotation of the quaternion. `CarPose::to_world` does this.
- Wheel anchors are car-frame points. Wheels 0 and 1 are at `y ≈ -1.09` (front), wheels 2 and 3 at `y ≈ +1.37`
  (rear). The world track is 1.46 to 1.52 m and the wheelbase 2.46 m (Corolla) or 2.58 m (Lancer) on every frame.
  The anchor height stays within 0.21 m of the body origin, so the origin sits at hub height. The lateral sign of
  wheels 0 and 2 is not proven.

`CarPose::heading` is the yaw of the horizontal velocity: the course over the ground. It differs from the nose when
the car slides.

## Mapping a recording to its stage

`drive-<name>.auto` holds the `StartGame` numbers: `map car weather tyre [damage [transmission]]`
(`harness/README.md`). The map number `N` is the stage: `Maps/track-N_<variant>.lbs` in the retail install
(`RBR_DATA`). Variants `E`, `M`, `N`, `O` and `S` exist for some stages. The `(2, 11)` start location is the same in
every variant of a stage (checked for the four drives). The start is converted to the physics frame by swapping y and
z.

| drive | map | stage file | car |
| --- | --- | --- | --- |
| tanner-corolla, tanner-damage | 54 | `track-54_E.lbs` (Tanner) | 6 (Corolla) |
| arctic-snow | 13 | `track-13_O.lbs` | 6 |
| tarmac-lancer | 41 | `track-41_M.lbs` | 3 |

## Live records

A live stepping process answers each control payload with a frame and records after it: HUD1/HUD2 (`hud`), DMG1
(`damage`), then tagged records (`live`: 4-byte tag, u32 length, payload) in any order, and last SCN1
(`rbr-standalone`'s awake scenery poses: a count, then 32 bytes each), which ends them. `rbr-standalone --live`
writes PTC1 (`particles`), DET1 (`detach`), SND1 (`sound`: the engine and drivetrain sound payloads, the speed, the
tyre-noise blocks, the damage payload and the sensors), PCN1 (`pacenote`: the step's pace-note callouts), SPL1
(`splits`) and CLK1 (`clock`: the physics step and the load settle's steps, 1/720 s and 1520 under NGP) there, through `live::write_record`; `live::read_records` reads them. Readers skip tags they do not know.

## Tyre particles

`src/particles.rs` reads the game's `CSigTireParticles` payloads (0xB0 bytes, layout in `notes/particles.md`) that
`rbr-standalone --live` sends as a `PTC1` record: the tag, a u32 byte count, then one payload per wheel the step
signalled (up to 16).

## Tools

- `rec_info <file>`: frame count, substeps, first and last steps.
- `drive_path <file> [--every N] [--csv out.csv]`: step, position, speed and heading every `N` steps (default 100).

## Checks

`cargo test -p rbr-recording` runs the unit tests and `tests/drives.rs`. The drive tests run only when
`RBR_RECORDINGS` names the directory with `drive-<name>.bin` and `.auto`; the start check also needs `RBR_DATA`
(`original/retail-1.0/install`). They check that every frame is finite and the quaternions unit length, that wheel
anchors stay within 3 m of the car origin, that speed stays under 100 m/s, the start is within 3 m, and the frame
geometry above. Position jumps over 1 m per frame are listed; only `tarmac-lancer` has one (step 20415, 18.1 m, speed
zero before and after: a rescue reset, not checked against the harness log).
