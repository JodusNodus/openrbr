# VCU port: names and open questions

`layout.rs` is the only file with raw offsets. Each `load` and `store` there is the typed view of the game
memory; the logic and the `copy_def` copy work on the fields. The vector tests (`tests/vcu_vectors.rs`) check
every port against the original's recorded calls.

For the car instance (load step 4) `layout.rs` also has `CVehicleControlUnit::CState::CState` (0x536d50) and
`CScratchSim::CScratchSim` (0x536e50), checked in the car by `tests/load_defs.rs` (`car_instance_matches_the_game`).

## Names

| Name | Original | Meaning |
| --- | --- | --- |
| `step::Send::EnginePedal` | `CEngine::SetScratch08` (0x50eb90) | the engine's pedal value |
| `Send::DriveThrottle` | `CDrive::Set44A0` (0x5044a0) | throttle to the drive (`Reals.throttle`) |
| `Send::DriveBrake` | `CDrive::Set30` (0x504450) | brake to the drive (`Reals.brake`, scaled into `brake_scaled`) |
| `Send::DriveClutch` | `CDrive::Set4820` (0x504820) | clutch slip to the drive (`Reals.clutch_slip`) |
| `Send::DiffLock` | `CDrive::Set4570` (0x504570) | diff lock of a channel: 0 centre, 1 front, 2 rear |
| `Send::CutFlags` | `CEngine::SetState4` (0x50d340) | the four rev-limit cut flags |
| `Send::BrakeLight` / `NeutralLight` | `Lights::Set1e40` / `Set1ed0` | brake light; neutral light |
| `Input.engine` | `CDrive::Get4520` (0x504520) | engine speed, rad/s |
| `Input.car_speed` | `CCar::Get15F8` (0x4e6770) | car speed, m/s |
| `Input.drive_speed` | `CDrive::Get4540` (0x504540) | the diff-lock speed key (times 0.3) |
| `Locks.ratio` | D8 record `+0x0c` | the channel's `diff_ratio` (`notes/physics/engine-driveline.md`) |
| `Locks.torque` | D8 record `+0x18` | the channel's lock torque |
| `Locks.max_torque` | definition `+0x58` + 0x60 per channel | the channel's maximum torque |
| `Locks.enabled` | `drive + 0x170` → `+0x90` | the lock gate |
| `Reals.brake_scale` | definition `+0x620` | scales the brake (`Set30`) |

## Unknown meaning

- `Scratch.unknown_18` and `Packet.unknown_10`: ReadPacket copies packet `+0x10` to scratch `+0x18`. The port
  stores it but nothing reads it.
- `KnotWords.first` and `KnotWords.second`: the two words before each knot's last word (`+0x4`, `+0x8`, and the
  speed map's `+0x14`, `+0x18`). CopyDef copies them, with no known meaning.
- `SetupFlags.launch_mode_when_off`: setup `+0x48`, used as the launch mode when the launch flag (`+0x18`) is 0.
- `Input.drive_speed`: the drive's speed (`Get4540`, drive `+0x384`) is read as the lock key; what it is is not known.
- `GearRatios.lim` and `GearRatios.div`: the C++ names. The tables are at definition `+0x644` and `+0x694`,
  0x50 bytes per gear; their use is known only from the slip query.
- `Reals.throttle` and `Reals.brake`: the routing is known, the names are inferred from `notes/physics/engine-driveline.md`.
