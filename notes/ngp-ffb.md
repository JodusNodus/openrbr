# Force feedback in 1.02 and NGP 7.4.778 (2026-10-16)

Static reading only: the exe (`out/decomp`, objdump, `src/rbr/`) and `NGP.dll` (`out/decomp-ngp/NGP.dll.c`,
`/tmp/ngp-tools/dis.sh`, the runtime patch table `original/ngp/analysis/patches-runtime.tsv`). Nothing was run.
"Unverified" marks claims that need a runtime check (listed at the end).

In short: 1.02 sends one value to the wheel, a constant force proportional to the filtered steering torque of the
steered wheels. NGP keeps that torque, replaces the torque-to-force curve, scales its divisor per surface and car
from the ini, adds a dead zone, and writes the device from its own thread.

## 1.02: where the force comes from

The chain, per physics step, then per frame:

| Step | Where | What |
| --- | --- | --- |
| 1. Clear | `CCar::StepEngine` 0x4e9930 (0x4e9a66) | car `+0xadc` (value of the `CReal` at `+0xad0`) = 0, before the scratch, drive and entity steps. |
| 2. Accumulate | `E51Wheel::M3DD30` 0x53dd30, at 0x53e780-0x53e7d2 | For each wheel with `IsSteered` (wheel def `+0x21c`, the value of the `CUnsigned` at `+0x210`): `T = cross(wheel+0x140 - def+0x8a0, F) + M`, `v = dot(T, wheel+0x9a0) * wheel+0x9c0 * wheel+0x9c4`, then `CCar::AddADC(v)` 0x4e6660 (`+0xadc += v`). `F` and `M` are the frame vectors at `FR(-0x244)` and `FR(-0x254)` in `RBR_E51_WheelD.cpp:644-667` (the wheel's force and moment). |
| 3. Lever factors | `0x53b900` (wheel kinematics), 0x53c9c2-0x53ca9c | `+0x9c0 = 1/a`, `+0x9c4 = 1/b`, two dot products of the kinematic frame; both 0 when `|a|` or `|b|` <= 0.001 (`0x795e94`). `+0x9a0` is a unit axis stored at 0x53c81f. Meaning (steering axis, steering arm) not verified. |
| 4. Filter | `CCar::UpdateFfbTorque` 0x4e7ff0 (called last in 0x4e9930, 0x4e9aa8) | `f = 0.5 + 0.2 * clamp((speed - 7) / 6, 0, 1)` (speed = car `+0x15fc`), `+0xaec = f * (SteeringRackRatio * +0xadc) + (1 - f) * +0xaec`. `SteeringRackRatio` is def `+0x23c` (value of the `CReal` at `+0x230`). While the start hold `+0x1670` > 0, `+0xaec` = 0. The coefficient is per step, not per second. |
| 5. Normalise | `CCar::FfbNorm` 0x4e76c0 | `0` if the car's slot 0x8c (`CSolid::E43Vf8C` 0x5293e0, body `+0x1ec`) is nonzero, else `clamp(+0xaec / MaxFFTorque, -1, 1)`. `MaxFFTorque` is def `+0x44a0` (value of the `CReal` at `+0x4494`); retail `physics.lsp` sets 135.0 on `CAR_ROOT` and no car overrides it (ctor default 10.0). |
| 6. Per step | `CFFBController::EmitWheel` 0x4918c0, from `CApplication::TicPhysicsRecording` 0x47f4e3 | After each physics step: emits `CSigWheelForceFeedback` (manager `+0xdbd8`) with `{app+0xad0, app+0xad4, norm, crash jolt +0x70}`, then clears `+0x70`. No listener in the exe was found (only the signal's ctor 0x4622c0 references `+0xdbd8`); unverified that plugins use it. |
| 7. Per frame | `CFFBController::EmitForce(dt)` 0x491930, from `CApplication::Tic` 0x480723 (state 1, racing) | Emits `CSigForceFeedback` (manager `+0x7110`) with the norm at payload `+0x14`, plus the rumble flags below. |
| 8. Latch | `USForceFeedBackImpl::Notify` 0x4c5dd0 | The only static listener (connected in the ctor 0x4c61a0 at manager `+0x7128`): impl `+0x20` = payload `+0x14`. |
| 9. Scale | `E28_GetFFBVal` 0x4c5f90 | `x = impl+0x20` (or `impl+0x24` while the menu test runs), `x *= Strength` (`0x1660ed8`), `x *= -1` when `Direction` (`0x1660edc`) is 0. |
| 10. Send | `USInputManagerImpl::Tic` 0x4cbf50, 0x4cbfa1-0x4cbfdb | For the device bound to action 0 (steering), if it has FF axes (0x4c6f70: `+0x29c != 0`, flag `0x100`): `m = gate ? _ftol(x * -10000) : 0` (`m_coop` = `+0x20` = -10000, set in 0x4ccfc0; `_ftol` 0x70c2c4 truncates), then `SendFFB2` 0x4c6fb0 -> `CInputDevice::SetFFBValue` 0x4c6b40. |

The gate is `M5403e0(0xb)` 0x5403e0 case 11 (0x5407b2): true while the menu test runs (0x4c5e20), else only when
the app state `+0x728` is 1, the `FRCFEEDBK` option (`0x1660ed4`) is on, and `E89Manager::IsDone(0)` 0x5fe390 holds.

Rates: steps 1-4 run once per car step (`MaxTimeStep` 0.007 s in retail, `notes/gameplay-loop.md`); EmitWheel once
per physics step; EmitForce once per frame. `CPC_Application::Vf0c` 0x44a3d0 calls the input tick before
`CApplication::Tic`, so the device gets the value from the previous frame.

Rumble side channel, not sent to the wheel: `CFFBController::HandleDamage` 0x491480 (signal at manager `+0xa128`) sets
a timer `+0x38 = clamp(v * 1e-4, 0.2, 0.7)` when the payload `+0x10` exceeds 800 (`0x72fd2c`) and a crash jolt
`+0x70 = clamp((v - 4000) / 7000, 0, 1)` above 4000 (`0x72fd24`), once per 1 s (`+0x74`). `HandleNoise` 0x491520
(manager `+0x6470`) raises the timer to 0.3 when payload word 0 is below 2 and word 1 (an integer) exceeds 2.5
(`0x72f57c`). EmitForce turns the timer into payload `+0x8` (1.0) and byte `+0x11`; `Notify` ignores both. Payload `+0`/`+4` are `app+0xad0`/`+0xad4`, unidentified words never
written directly in the exe.

### DirectInput (1.02)

| Function | What it sends |
| --- | --- |
| `CInputDevice::CreateFFB` 0x4c8c60 | One `GUID_ConstantForce` effect (`0x766610`) over every FF axis the device reports: `DIEFF_CARTESIAN | DIEFF_OBJECTIDS` (0x11), gain 10000, infinite duration, all directions 0. |
| `CInputDevice::SetFFBValue` 0x4c6b40 | Clamps to [-10000, 10000], then `SetParameters(DIEP_START | DIEP_TYPESPECIFICPARAMS | DIEP_DIRECTION` = 0x20000140) with `DIEFF_CARTESIAN | DIEFF_OBJECTOFFSETS` (0x12), `cAxes = 1`, direction `{0}`, `lMagnitude` = the value. Every frame, restarting the effect. |
| `CInputDevice::StartFFB` 0x4c6c00 | `Unacquire`, `Start(1, 0)`; from `USInputManagerImpl` ctor 0x4ccfc0 and `EnableFFB` 0x4cb300. |
| `CInputDevice::SetAutoCenter` 0x4c6e20 | `DIPROP_AUTOCENTER` (prop 9) = the `FF_AUTOCENTER` option (`0x1660ee0`); from 0x4cd381, 0x4cb35b, 0x4cb71e. |
| Pause/unpause/state | 0x4c6d50 (`SendForceFeedbackCommand(DISFFC_PAUSE)`), 0x4c6d90 (`DISFFC_CONTINUE` when paused), 0x4c6d00. |

The `cAxes = 1` update against a multi-axis effect is what Wine rejects; the harness plugin patches 0x4c6bab/0x4c6bc2
to send two axes (`harness/README.md`).

Settings (menu builder `CGameMenuDaemon::M56e2d0`, `RBR_E61_BuildersC.cpp`): `FRCFEEDBK` on/off `0x1660ed4`,
`STRENGTH` slider `0x1660ed8` (0..1, step 0.05), `DIRECTION` `0x1660edc` (0 `INVERSE`, 1 `NORMAL`; 0x4c5f90 negates
at 0), `FF_AUTOCENTER` `0x1660ee0`. A second copy of the menu is at 0x595d3f. Where these are
saved (profile or ini) was not traced. The menu test (`USForceFeedBackImpl::StepFFB` 0x4c5c30) plays -1, +1, then
`-steering` for 2 s each (`0x72ad94`).

## NGP 7.4.778: what it changes

Patch-table sites on the FFB path (from `patches-runtime.tsv`; the 0x4804f0 and 0x4c6bab/0x4c6bc2 rows are the
harness, not NGP):

| Site | In | Now |
| --- | --- | --- |
| 0x4918ee, 0x4919ac | EmitWheel, EmitForce: `call FfbNorm` | NGP `0x10042120` (new curve, below) |
| 0x4919ce | EmitForce: `CSigForceFeedback::Emit` | NGP `0x100ee1c0`: the same fan-out, then `x = E28_GetFFBVal()` (original 0x4c5f90 via 0x1003c130) and an atomic store of `x` into the input object (`0x113c7a28 + 0x160`, setter 0x1008ebb0) |
| 0x4c6fb3 | `SendFFB2`: `call SetFFBValue` | NGP `0x10064900`: does nothing while the FFB thread runs (`0x113c78a0 + 0xc`, 0x10098670), else NGP's `SetFFBValue` 0x10064620 |
| 0x49148c, 0x4914c9 | HandleDamage thresholds 800, 4000 | NGP `.data` `0x10171310`, `0x10171314` = `r * 800`, `r * 4000`, set per car in 0x1003f5b0 (`r` is the "0.94195 group" factor of `notes/ngp-load-path.md`, 0.94195 for the Celica) |
| 0x4cc42e, 0x4cc45e, 0x4cc48e, 0x4cc553 | input tick (control emit and two others; `TicFwd`'s `jmp`) | NGP input handling (`0x100ed540`, `0x1000eff0`, `0x100ee410`; the `TicFwd` target `0x7919c650` lies outside `NGP.dll`'s image in that dump: unresolved). Not FFB. |

Not patched: the torque chain (0x4e9930, 0x53e780-0x53e7d2, 0x53b900's lever factors, 0x4e7ff0) and the 1.02 gate,
scale and menu. NGP changes the torque's inputs through its wheel and tyre changes (M3DD30's retargeted calls
0x53e2a4, 0x53e317, 0x53e548 and constants 0x53ea21-0x53ea45; `notes/ngp-replacements.md`), and runs the car step at
720 Hz, so the per-step filter in 0x4e7ff0 smooths over about a fifth of the 1.02 time (derived, unverified).

### The curve (`0x10042120`)

```
if body+0x1ec != 0: 0
else: sign(T) * (1 - pow(0.975, |T| * 100 / S))      T = car+0xaec, S = def+0x44a0
```

`pow` is `libm_sse2_pow_precise` in double (base `0x10147120` = 0.975f widened, exponent `float(|T| * (100 / S))`
widened), rounded to float; `1.0` (`0x10147118`), `100.0` (`0x10146b28`). `sign` is +1, -1 (sign bit, 0x1003cc60) or 0
when `T == 0`. No clamp is needed: the result stays in (-1, 1). With `S = 100` half force is at `|T|` about 27 and 90 %
at about 91 (in `+0xaec` units).

### `ForceFeedbackSensitivity_NGP` and the ini (`S`)

`ForceFeedbackSensitivity_NGP` is the renamed `MaxFFTorque` key (0x10060db0, `notes/ngp-load-path.md`), so it lands
in def `+0x4494` and `S` is its value. NGP's `physics.lsp` sets 100.0; the 42 RSF NGP cars use 75-130 (mostly
100-125). At car load, `0x1003f5b0` (0x1004151a-0x10041559) reads two ini integers keyed by the car's `SetupSpec`
(def `+0x44c0`; 0 gravel, 1 tarmac, 2 snow): `ForceFeedbackSensitivity<Surface>_Car<slot>` (0x1007a630) and
`ForceFeedbackSensitivity<Surface>` (0x1007a500). The car value wins if > 0, else the global if > 0; if the chosen
value is not 100, `S *= value / 100`. Then `S` is floored at 1.0. Defaults per `ReadMe.NGP.txt`: 100 global, 0
per car. Higher `S` means a weaker force.

### Output, thread and dead zone

- The input object `0x113c7a28` (ctor 0x1008e1f0, rate from the ini, clamped to 0..1000) keeps the raw steering
  input in `+0x15c` (atomic store in its `CSigCarControlInput` handler 0x1008e8a0) and the latest `x` in `+0x160`.
- The FFB thread object `0x113c78a0` (ctor 0x1008dc30, rate from the ini, clamped to 0..1000; the readme's `ffbRate`,
  default 1000 Hz; 0 disables the thread). Its tick 0x1008dfd0 calls `0x1003c390`, which repeats 1.02's send step:
  input impl present and not rebinding, steering device FF-capable (original 0x4c6f70), the original gate
  `M5403e0(0xb)`, then `m = (int)floor(-10000 * x)` (1.02 truncates) with `x` from `+0x160`, then `0x10064620`.
  Which ini key each of the two rate objects (`0x1014c458`, `0x1014c478`, both default 1000) reads is inferred from
  their callers, not read from the key objects.
- `0x10064620` (NGP's `SetFFBValue`): on entering state 1 (`0x10177c88` state, via 0x10098360; meaning inferred),
  re-reads `ffbDeadZone` (float) and `ffbDeadZoneThreshold` (stored as `int(t * 10000)`). Clamps `m` to
  [-10000, 10000]; sets `m = 0` when `|steering input| < ffbDeadZone` and `|m| < threshold`. Then `SetParameters`
  with flags 0x20000141 (1.02's plus `DIEP_DURATION`), `dwDuration` INFINITE, `DIEFF_CARTESIAN | DIEFF_OBJECTOFFSETS`,
  `cAxes = 1`, direction `{0}`. NGP's own call never goes through 0x4c6b40, so the harness's two-axis Wine fix does not
  apply under NGP (unverified at run time).
- So under NGP the force value still changes once per rendered frame (EmitForce); the thread only resends the latest
  value at up to 1000 Hz and applies the dead zone between frames as the steering input moves.
- The ini key names have std::string builders: `adaptiveFFB` 0x10072f60, `ffbBufferCycles` 0x100734e0, `ffbDeadZone`
  0x10073a10, `ffbDeadZoneThreshold` 0x10073ba0, `ffbRate` 0x1007a4b0, `ForceFeedbackSensitivity` 0x1007a950
  (`ffbMinFPS` is only a string). The readme says adaptive FFB was removed in 7.2.770; whether anything still reads
  `adaptiveFFB`, `ffbBufferCycles` or `ffbMinFPS` was not checked.

## Rust: proposed split

Already ported, under wrong names: `car_body::step_engine` (0x4e9930, the clear), the wheel step's `add_adc` host
call (`wheel/layout.rs`), `car_body::steering_ffb::update` (0x4e7ff0, the filter) and `car_body::steering_ffb::ffb_norm`
(0x4e76c0). The evidence above says these are steering force feedback, not downforce: `+0x16a4` ("target seed") is the
control payload's steering word (`CCar::UpdateSeed` 0x4e926f), `+0xafc`/`+0xb0c`/`+0xb1c` are the slewed steering,
the rack position (`* MaxSteeringLock * SteeringRackRatio`) and its rate, `+0xadc` is the summed wheel steering
torque, `+0xaec` the filtered torque, `+0x44a0` is `MaxFFTorque`. `notes/physics/car-body.md` (0x44a0, 0x4e76c0)
should be corrected; renaming the code is a separate, mechanical change.

1. **Physics (in `rbr-physics`, variant-independent code, already bit-exact work):** the steered-wheel torque sum and
   the filter, producing `T = car+0xaec` each step. Nothing new except exposing `T` (and the body flag `+0x1ec`) in the
   step output.
2. **Pure FFB mapping (new, small, no platform code):** a function per frame
   `ffb_command(T, body_flag, S, settings, variant, steering_input) -> i32` with
   - 1.02: `n = clamp(T / MaxFFTorque, -1, 1)`; NGP: the curve above with `S` after the ini scaling (load-time pure
     function of `ForceFeedbackSensitivity_NGP`, `SetupSpec`, slot, the two ini values);
   - `x = n * strength * (direction ? 1 : -1)`; 1.02 `m = trunc(-10000 x)`, NGP `m = floor(-10000 x)`; clamp to
     +-10000; NGP dead zone (`ffbDeadZone`, `ffbDeadZoneThreshold`, current steering input);
   - the gate (FFB on, racing state, start finished) as an input flag.
   Constants: 0.975 (as `0.975f as f64`), 100, 1.0 floor, 10000, -10000. Testable like the other ports: records of
   `+0xaec`/`+0x44a0` and the value passed to 0x4c6b40 (1.02) or 0x10064620 (NGP).
3. **Device (platform-specific, outside `rbr-physics`):** one constant-force effect on the steering device, updated
   with `m` (start on update, infinite duration), autocenter off or on per setting, pause/continue; DirectInput8 on
   Windows, or SDL3 haptics / evdev `FF_CONSTANT` elsewhere (send all FF axes, as the harness fix does). Rate: per
   frame for 1.02; for NGP a thread at `ffbRate` (cap 1000) re-sending the latest atomic value, with the dead zone
   evaluated there against the latest steering input. The rumble timers (HandleDamage/HandleNoise) are optional
   (gamepads); 1.02 never sent them to DirectInput.

## Rust (2026-10-17)

The split above, as built:

- `rbr_physics::ngp::ffb::curve` is `0x10042120`; `car_body::steering_ffb::force(mem, car, variant)` gives the force
  per step: `FfbNorm` under 1.02, the curve under NGP, both gated by car `+0x1ec`.
- `rbr_gameplay::ffb::{magnitude, dead_zone}`: strength, inverse, `trunc`/`floor` of `x * -10000`, clamp, NGP's
  dead zone.
- `rbr-standalone --live` sends the force in its HUD record (`HUD2`: 24 bytes, force at 16, flags at 20, bit 0 NGP);
  the viewer reads `HUD1` or `HUD2`.
- `rbr_render::ffb::Wheel`: SDL3 haptics (`sdl3-sys`, system SDL3), one infinite constant effect on
  `SDL_HAPTIC_STEERING_AXIS`, a thread resending the level at `rate` Hz with the dead zone applied there, level
  `m * 32767 / 10000`. sdl3-sys 0.7 truncates `SDL_HAPTIC_GAIN`/`AUTOCENTER` to the 16-bit effect type (0), so the
  feature bits are checked as `1 << 16` and `1 << 17`.
- The launcher's `[ffb]` section (enabled, strength, inverse, autocenter, dead_zone, dead_zone_threshold, rate,
  device) reaches the viewer through `--bindings`.

Checked:

- The NGP live run from rpl11's 6438 snapshot stays bit-exact for 2400 steps with the HUD force. The force is
  within +-0.53, opposite to the steering at every step with `|steering| > 0.2` (13 of 13).
- `Wheel` on cachy's Logitech G923 (Linux, SDL 3.4.18): force +0.2 (what left steering gives) turns the free
  wheel right (axis 576 to 31174) and -0.2 turns it back left, so the default direction centres the wheel. The 2400
  recorded forces played at 400 Hz without errors.

Not checked: a drive with hands on the wheel, the 1.02 force at runtime against `SetFFBValue`, and Windows.

## Runtime checks (not done)

- 1.02: record car `+0xaec`, `+0xadc`, the 0x4e76c0 result and the `SetFFBValue` argument over a drive; check
  `m = trunc(-10000 * clamp(+0xaec / 135, -1, 1) * strength * dir)` and the one-frame lag.
- NGP: the same with 0x10042120's result and 0x10064620's argument; confirm `S` after load (`def+0x44a0`) for a car
  with a surface ini override; confirm which ini keys feed `0x1014c458`/`0x1014c478`; whether FFB reaches the wheel
  under Wine (one axis); whether the menu FFB test works with the thread on (`+0x160` is only written by EmitForce).
- What `app+0xad0`/`+0xad4` hold, and whether any plugin listens to `CSigWheelForceFeedback`.
- `wheel+0x9a0`, `+0x9c0`, `+0x9c4` and def `+0x8a0`: names (steering axis, lever factors, steering mount) inferred.
