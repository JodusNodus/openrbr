# Car body (CCar, E33/E34/E35 body): physics semantics

Pass 3 of the physics semantics brief (`lead/briefs/physics-semantics.md`), section 1 of this
pass. Notes only; no source edits. "Current" names are the C++ reconstruction's. "Proposed" names
are ours.

Confidence: `known` (LSP key or exe string names it, or the code is unambiguous), `inferred`
(strong argument from use), `guess`. Asm checked in this pass (`objdump -d -M intel` on the
original exe, VAs): `CCar::Hook` (0x4e9930, whole function), `CCar::Sim7AD0` (0x4e7ad0, whole
function, with its helpers `Vec4::CopyFrom` 0x43c5b0, the `Scale` helper 0x473cd0 and the
constants), `CCar::Xform6870` (0x4e6870, whole function), `CCar::Xform68E0` (not read), the
slot-12 identity getter (0x519c00), `E33Mat34::ComposeA` (0x51b4c0, the ret form), and
`CReal::SetDirty` (0x52f450), and `Probe7730` (0x4e7730). Constants read from the exe's `.rdata`
(the float literals named below). Not asm-checked: everything else in this note, including
`Hook1FF0`, `Vf6cx`, `Update91F0`, `Update9C70`, `Sim7DC0`, `Clamp76C0`, and `Vf28x`.

Reached functions of this subsystem are in `notes/progress/called.csv`. Status labels are from
`notes/progress/gameplay-functions.csv`. Note that `Sim7AD0` is labelled `verified` there but is
not byte-exact (reccmp 51.41%), and `Hook` is labelled `bad` there, though its asm agrees with
the C++ in this pass (section 5, item C5). Those two labels disagree with the asm and need a
look by the owners.

## 1. Overview

`CCar` is one car. It is a `CBody` (the E33 rigid-body base, 0xd0 bytes), and it holds:
the car's setup (`CCar::CDef`, `def = this+0x34`), its four wheels (`this+0x470`, CWheel), four
hinged bodies (`this+0x480`, CHingedRigidBody, the doors and bonnet, from the LSP's `HingedBody`
blocks), four mounted bodies (`this+0x490`, E45Smalls, the LSP's `MountedBody` blocks), the
drive (`this+0x4b0`), the engine (`this+0x690`), the VCU scratch (`this+0x850`), the passenger
(`this+0x8b0`), the car state (`CCar::CState`, `this+0xa00`, 0xc70 bytes), the car scratch
(`this+0x1690`), the E38 ray object (`this+0x16b0`), the brake-wall/light systems (`this+0x838`),
and the damage object (`this+0x3cc`, `CCarDamageHandler`, ~0x3eac bytes; the same pointer is copied to `this+0x9f4`). `Update9C70` drives it and aliases the drive, engine, light and wheel damage records into it (see `damage.md` section 1a).

The car's own state that matters for the step is small. It is: the velocity (`this+0x1c0`, and
its speed `this+0x15fc`), the filtered steering torque for force feedback (`this+0xaec`; the steering slew toward `this+0x16a4`; `notes/ngp-ffb.md`), a
brake-wall penetration (`this+0x166c`), two timers (`this+0x1670`, `this+0x9e8`), and the
contact-grid pointer (`this+0x64`). Section 2 gives the fields.

### 1a. Place in the step

`CCar::Vf6cx(dt)` (vtable slot 108, `0x4ea300`) is the per-step update that runs the car's
own work. Its order in the C++ (`RBR_E34_CarD.cpp:610-744`):

1. Timer `this+0x1670` decreases by `dt` and is clamped at 0.
2. Passenger step, VCU scratch `NopA`, drive `Nop3B0`, E47 `Vf27x`. These are the sub-systems,
   not car physics.
3. Brake-wall raycast (`g_7c63c8->RaycastEntry(this+0x140, bw)`). If it hits, the impulse
   `Impulse640(bw, bw+0x14, bw+0x10, dt)` is applied to the car (E46), and the penetration signal
   is emitted. The wall-contact timer `this+0x9ec` grows by `dt` while the hit flag is set and the
   penetration exceeds 0.5, and is cleared otherwise. If there is no hit, the penetration is 0.
4. `this+0x166c = bw+0x10` (the penetration, section 2).
5. `this+0x15f8 = -MB560(this+0x1c0)[1]`, the velocity transformed to the car frame, negated (guess
   for its use).
6. The hinged-body loop (`this+0x480`: `S80`, `S90`, `SimFD00`), the timer-card reads, and the
   speed block: `this+0x9e8` (time with fewer than three wheels on the
   ground), `this+0x15f4` (accumulator, section 2), the speed `this+0x1600`/`this+0x15fc` (`|v|`),
   the unit direction `this+0x1610`, and the `CSigCarPosition`/`CSigMrRCarState` Emits.

Then the engine-side part of the step, in `CCar::Hook(dt)` (vtable slot 22, `0x4e9930`, asm
checked): (a) the steering slew (section 3, F1); (b) the target `this+0xb0c` and its rate
`this+0xb1c` (F2); (c) `this+0xadc = 0`; (d) the VCU scratch step (`CScratchSim::Step(dt)`,
`0x537f50`); (e) the drive step (`CDrive::Step1E0`, `0x50a1e0`); (f) `CEntity::E43Vf58`
(`0x51a250`); (g) `Damp66D0` (`0x4e66d0`); (h) `Hook1FF0` (the FFB torque filter, `0x4e7ff0`, section 3,
F4); (i) `Snap9360` when `this+0x9e4` is set.

Aero, per step, is `Sim7DC0` (drag, `0x4e7dc0`, called from the update path) and `Sim7AD0`
(the gust/aero worker, `0x4e7ad0`), both driven through the car's slot-28/29 forces: `S70` (force
at the centre) and `S74` (torque). Their C++ and asm are in section 5.

The contact side (the car against the ground and the brake walls) and the collision grid are in
`notes/physics/collision.md`. The damage side is in `notes/physics/damage.md`.

### 1b. Call tree (reached, direct calls from the C++; asm where marked)

```
CCar::Hook (0x4e9930, asm ok)
  CScratchSim::Step (0x537f50)             VCU scratch (CVehicleControlUnit)
  CDrive::Step1E0 (0x50a1e0)               drive
  CEntity::E43Vf58 (0x51a250)              E43 base slot
  Damp66D0 (0x4e66d0)                      damping (not read)
  Hook1FF0 (0x4e7ff0)                      FFB torque filter (F4)
  Snap9360 (0x4e9360)                      snapshot when this+0x9e4 != 0
CCar::Vf6cx (0x4ea300)                     per-step (slot 108)
  CBrakeWallManager::RaycastEntry (0x7c63c8 global)
  E46Impulse::Impulse640
  CCar::Update91F0 (0x4e91f0)              seeds this+0x16a4, probes, Emits
  CCar::Probe7730 (0x4e7730)               standstill / problem probe
CCar::Update9C70 (0x4e9c70)                MrR update: drives the damage object (sets this+0x9f4)
CCar::Update9AF0 (0x4e9af0)                standstill/retire updater
CCar::Sim7DC0 (0x4e7dc0)                   drag and angular damping
CCar::Sim7AD0 (0x4e7ad0)                   aero gust (asm checked, section 5 C1)
CCar::Xform68E0/6950/6870/69C0/6A30        world/body transforms (section 3)
CCar::Update7270 (0x4e7270)                one-shot grid edge push (this+0x16a8)
CCar::Clamp76C0 (0x4e76c0)                 FFB torque normalised by MaxFFTorque (def+0x44a0)
```

Inputs from other subsystems: the wheels (`wheel+0xa50` spin, the strut and tyre outputs, the
wheel's `on_ground` at `wheel+0xebc`), the drive and engine (`Get4540`, `GetState188`, ...), the
world (`g_1659b70`: the time keeper at `+0x40`, the timer card at `+0x20`, the frame at `+0x24`),
the brake walls (`g_7c63c8`) and the car's grid (`this+0x64`). Outputs: the forces and torques to
the body (`S70`/`S74`), the wheel and engine inputs, the signals (`g_signalManager`,
`CSigCarSensorData` in `Sample9800`, `CSigCarPosition`, `CSigMrRCarState`, `CSigCarBrakeWallPenetration`).

## 2. Data

Offsets are from the car (`this`). `def` is `this+0x34` (CCar::CDef). `this+0xa00` is the car
state (`CCar::CState`, 0xc70 bytes); its own fields are not in this pass.

### 2a. CCar (per-step state)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x34 | `def` | def | ptr | - | the car's setup (`CCar::CDef`) | known (the code reads def+0x16c, +0x44a0 ...) | `RBR_E34_CarA.cpp`, `CarC.cpp:56` |
| 0x64 | `*(void**)(this+0x64)` | contact_grid | ptr | - | the E37 grid object that takes the one-shot edge (`AddEdge`) | inferred | `RBR_E34_CarE.cpp:282-294` |
| 0x9e4 | `*(int*)(this+0x9e4)` | snapshot_on | int | 0/1 | when non-zero, `Snap9360` runs in `Hook` | known (the call) | `CarD.cpp:312` |
| 0x9e8 | `*(float*)(this+0x9e8)` | t_partial_contact | f32 | s | accumulated step time while fewer than 3 wheels have `on_ground`; reset to 0 otherwise | inferred (strong) | `CarD.cpp:717-728` (sum of `wheel+0xebc` over the four wheels) |
| 0x9ec | `*(float*)(this+0x9ec)` | t_wall_contact | f32 | s | time with the brake wall hit and penetration > 0.5 (reset when not) | inferred (strong) | `CarD.cpp:630-640` |
| 0x9f0 | `*(int*)(this+0x9f0)` | flag_retire | int | 0/1 | set when `9e8 > 1.0` (CarC) and read by the Quat step | guess | `CarC.cpp:791-795` |
| 0x9f4 | `*(int*)(this+0x9f4)` | damage | ptr | - | the damage object (`CCarDamageHandler`, the same pointer as `this+0x3cc`); `Test3B20` runs on it in `Update9AF0` | inferred | `CarD.cpp:386, 336` |
| 0x950 | `*(int*)(this+0x950)` | step_count | int | steps | frame counter; every 1000 steps the sample writes two snapshot words | inferred | `CarE.cpp:300-318` (`Update91F0`) |
| 0xa00 | `m_stateA00` | state | CCar::CState* | - | car state object (0xc70 bytes) | known (the ctor) | `CarE.cpp`, `Car.h` |
| 0xadc | `*(int*)(this+0xadc)` | steer_torque_sum | f32 | - | written 0 in `Hook` before the VCU step (C4) and read by `Hook1FF0` after it | guess | `CarD.cpp:300`, `CarC.cpp:207` |
| 0xaec | `*(float*)(this+0xaec)` | ffb_torque | f32 | N m (unit not fixed; see open 2) | the filtered steering torque for force feedback (`notes/ngp-ffb.md`), slewed by `Hook1FF0` (F4) and clamped by `Clamp76C0` | inferred | `CarC.cpp:207`, `CarE.cpp:132` |
| 0xaf0..0xafc | `(CBase)+0xc` | steer_slew | CReal (value at 0xafc) | - | value slewed toward `this+0x16a4` at most 20 units/s | inferred (strong), value unit inferred | `CarD.cpp:287-295` (F1), asm `0x4e9960-0x4e99c2` |
| 0xb0c | `*(float*)(this+0xb0c)` | rack_target (was: downforce_target) | f32 | - | `MaxSteeringLock (def+0x22c) * SteeringRackRatio (def+0x23c) * 0xafc`: the steering rack position (`notes/ngp-ffb.md`) | inferred | `CarD.cpp:297-300` (F2), asm `0x4e99e5-0x4e9a2f` |
| 0xb1c | `*(float*)(this+0xb1c)` | rack_rate | f32 | 1/s | `(0xb0c_new - 0xb0c_old) / dt` | inferred (strong) | `CarD.cpp:301-304`, asm `0x4e9a34-0x4e9a5b` |
| 0x15f4 | `*(float*)(this+0x15f4)` | accum_drive | f32 | - | `+= Get4540() * def+0x24c` each step (`0x15f4 * 0.001` is sent in the sensor record) | guess | `CarD.cpp:256, 723-726` |
| 0x15f8 | `*(float*)(this+0x15f8)` | v_mb_y | f32 | m/s | `-MB560(this+0x1c0)[1]` | guess | `CarD.cpp` (Vf6cx), `CarD.cpp:688-690` |
| 0x15fc | `*(float*)(this+0x15fc)` | speed | f32 | m/s | `|v|` where `v = this+0x1c0` (the second `Length`, after the first is stored at `0x1600`) | inferred (strong) | `CarD.cpp:739-747` and its use in `Hook1FF0` (`(0x15fc-7)/6`) |
| 0x1600 | `*(float*)(this+0x1600)` | speed_pre | f32 | m/s | `|v|` (the same value as 0x15fc; the two `Length` calls read the same vector) | inferred | `CarD.cpp:739-747` |
| 0x1610 | `*(float*)(this+0x1610)` | dir_back | 3-vector | - | `-v / |v|` (zero below 0.001 m/s) | inferred (strong) | `CarD.cpp:742-744` |
| 0x1c0 | (vector at `this+0x1c0`) | velocity | 3-vector | m/s | the car's velocity (the `Length` gives speed; `Sim7DC0`/`Sim7AD0` use it as v; `Probe7730` gates on `|v| > 1.5`) | inferred (strong) | `CarC.cpp:69-76`, `Sim7DC0` drag `-0.5 Cd A |v| v` (section 3 F7), `Probe7730` `CarE.cpp:154` |
| 0x190 | (vector at `this+0x190`) | angular_velocity | 3-vector | rad/s | `Sim7DC0` caps it: torque `-5(|w|-5)^2 w/|w|` above `|w| = 5` | inferred (strong) | `CarC.cpp:138-145`, `CarE.cpp:90` |
| 0x198 | `*(float*)(this+0x198)` | yaw_factor | f32 | - | `2.5 * x`, clamped to [-1, 1] in `Sim7AD0` | guess | `CarC.cpp:85-87` |
| 0x140 | (vector at `this+0x140`) | position | 3-vector | m | the car's position in the world (copied to `CSigCarPosition`, and the brake-wall ray origin) | inferred | `CarD.cpp:197, 519, 745`; `Vf6cx` raycast `this+0x140` |
| 0x166c | `*(float*)(this+0x166c)` | wall_penetration | f32 | m | brake-wall penetration from the raycast (`bw+0x10`); gates `Sim7AD0` (only when > 0) | inferred (strong) | `CarD.cpp:655`; `CarC.cpp:49` |
| 0x1668 | `*(int*)(this+0x1668)` | flag_50g | int | 0/1 | set when `|a|` exceeds `def+0x44b0` (490.5 = 50 g), where `a = (this+0x230, +0x234, +0x238)`; `Update9AF0` takes the retire path | inferred (strong; 490.5 = 50 x 9.81) | `CarD.cpp:709-714`, `Update9AF0` `CarD.cpp:332` |
| 0x1670 | `*(float*)(this+0x1670)` | t_override | f32 | s | timer, decreases by `dt` and clamps at 0; while it is > 0 the aero is off (`Hook1FF0` holds `aec` at 0) and `Update91F0` seeds a fixed vector | inferred | `CarD.cpp:627-633`, `CarC.cpp:189-192`, `CarE.cpp:228-231` |
| 0x16a4 | `*(int*)(this+0x16a4)` | steer_input | f32 bits | - | seed value copied from the seed vector (`seed[4]`) in `Update91F0`; `Hook` slews `0xafc` toward it | inferred | `CarE.cpp:236`, `CarD.cpp:851` |
| 0x1694, 0x1698 | `*(int*)(this+0x169x)` | seed_0, seed_1 | f32 bits | - | seed words 0 and 1 from `Update91F0` | guess | `CarE.cpp:237-238` |
| 0x16a8 | `*(int*)(this+0x16a8)` | grid_pushed | int | 0/1 | one-shot flag for `Update7270` | known (the code) | `CarE.cpp:282-294` |

### 2b. CCar::CDef (`def = this+0x34`), fields used here

Ctor defaults (`RBR_E34_CarA.cpp:318-324`) and LSP keys (`notes/formats/physics-lsp.md`,
`PhysicsEngine/Car/CAR_ROOT`) are the evidence. The C++ constructs the CReal members with no
name string, so the key names below come from the LSP value ranges and the ctor defaults.

| offset | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x16c | inverse_mass | f32 | 1/kg | InverseMass of the car (LSP range 0.000714..0.000869, so 1150-1400 kg) | known (LSP key `InverseMass` in `CAR_ROOT`), inferred (the offset) | physics-lsp.md; `CarC.cpp:56` |
| 0x22c | MaxSteeringLock | f32 | - | key name from the CCar::CDef key table (tweak-keys.md); multiplies `0xafc` into the 0xb0c target (F2), so F2 is steering | known | `CarD.cpp:297`, `CarD.cpp:1425` |
| 0x23c | SteeringRackRatio | f32 | - | key name (tweak-keys.md); multiplies the 0xb0c target (F2) and the `Hook1FF0` target (F4); F4 scales the steered wheels' torque with it (`notes/ngp-ffb.md`) | known | `CarD.cpp:298`, `CarC.cpp:204` |
| 0x24c | (accumulator gain) | f32 | - | multiplies the drive value in `0x15f4` | guess | `CarD.cpp:256, 733` |
| 0x4480 | frontal_area | f32 | m^2 | the car's frontal area, default 2.0 | known (LSP key `FrontArea:float[2]`, default 2.0 in ctor) | `CarA.cpp:320`; physics-lsp.md |
| 0x4490 | drag_coefficient | f32 | - | `Cd`, default 0.4 | known (LSP key `DragCoefficient:float[0.4..0.5]`) | `CarA.cpp:322`; physics-lsp.md |
| 0x44a0 | ffb_norm | f32 | - | normaliser for `aec` in `Clamp76C0`, default 10.0 | inferred (strong) | `CarA.cpp:318`, `CarE.cpp:132` |
| 0x44b0 | dangerous_accel | f32 | m/s^2 | the 50 g threshold, default 490.50003 | inferred (LSP key `DangerousAcceleration` in `CAR_ROOT`, sample value 1e6: see open 4) | `CarA.cpp:324`; `CarD.cpp:709` |

### 2c. Wheel-def fields read from the car (for reference)

| offset (wheel def) | proposed name | type | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x23c | wheel_index (unsigned) | u32 | the wheel's slot in the car's hinged or mounted list (`this+0x490`); an index, not the car's `0x23c` float | inferred | `CarA.cpp:193` (`k < 4`), `CarA.cpp:184` |
| 0x4fc | hinged_index (unsigned) | u32 | the slot in `this+0x480` | inferred | `CarA.cpp:184` |

## 3. Functions

The car functions, with the proposed names and the equations in proposed names. "Asm" means the
asm was read in this pass.

| address | current name | proposed name | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x4e9930 | CCar::Hook | car_step_engine | F1-F3 below, then the VCU, drive, E43 and damping steps (section 1a) | known (asm checked: F1, F2, the order) | asm, `CarD.cpp:281-318` |
| F1 (in Hook) | (inline) | slew_steer | `e = target - aec_slew`; `r = e/dt`; `d = (|r| > 20) ? 20*dt*sign(e) : e` ; `aec_slew += d` | known (asm checked) | asm `0x4e9960-0x4e99c2` |
| F2 (in Hook) | (inline) | rack_target | `t_new = def22c * def23c * aec_slew`; `rate = (t_new - t_old)/dt` | known (asm checked) | asm `0x4e99e5-0x4e9a5b` |
| 0x4e7ff0 | CCar::Hook1FF0 | ffb_torque_update | with `t_override <= 0`: `k = clamp((speed - 7)/6, 0, 1)`, `g = 0.5 + 0.2 k`; `aec = def23c * dadc * g + (1 - g) * aec`; else `aec = 0` (F4) | inferred (C++ read, not asm) | `CarC.cpp:182-209` |
| 0x4e76c0 | CCar::Clamp76C0 | ffb_norm | `clamp(aec / def44a0, -1, 1)`, 0 if slot 0x8c fires | inferred (C++ read, not asm) | `CarE.cpp:125-135` |
| 0x4e7730 | CCar::Probe7730 | motion_probe | 0 when slot 0x8c fires; 4 if `|v| > 1.5`; then the brake-wall/sampler verdict (1 = blocked forward, 2 = blocked and reversed, 0 = ok) | asm checked | `CarE.cpp:148-190` |
| 0x4e7ad0 | CCar::Sim7AD0 | aero_gust | section 3a (F5) | drive and forced wall-contact replay verified; reccmp 53.78% | asm `0x4e7ad0-0x4e7db7` |
| 0x4e7dc0 | CCar::Sim7DC0 | drag_and_spin_damp | F7 (drag) and F8 (spin cap) | inferred (C++ read, not asm) | `CarC.cpp:103-152` |
| 0x4e68e0 | CCar::Xform68E0 | to_world | `b = MB560(a, w, ·)` with `w = g_7cbb00`, the world transform (slot-12 identity, then `MB560`) | inferred (C++ read) | `CarB.cpp:104-117` |
| 0x4e6870 | CCar::Xform6870 | (body compose) | section 5 C2 | asm checked, does not match | asm `0x4e6870-0x4e68d5` |
| 0x4e69c0 | CCar::Xform69C0 | body_to_world_pair | `out = MB560(a, t, b)` with `t` = `this->S30()` and `u` = `body->S30()` (both identity getters, slot 12) | inferred (C++ read) | `CarB.cpp:140-151` |
| 0x4e6a30 | CCar::Xform6A30 | body_compose | `out = ComposeA(a, t, b)` (the same `t`/`u` pair as 69C0) | inferred (C++ read) | `CarB.cpp:160-171` |
| 0x4e6950 | CCar::Xform6950 | world_compose | `ComposeA(a, w, b)` with `w = g_7cbb00` | inferred (C++ read) | `CarB.cpp:121-133` |
| 0x4e72d0 | CCar::Vf20x72D0 | refresh_edge | `Update7270(); E43Vf20(a)` | known (C++ read) | `CarE.cpp:299-302` |
| 0x4e7270 | CCar::Update7270 | push_grid_edge | once per car (`this+0x16a8`): `loc = Xform6A30(0)` and `grid.AddEdge(loc)` | inferred (C++ read) | `CarE.cpp:282-294` |
| 0x4e91f0 | CCar::Update91F0 | seed_update | copies the seed (or the fixed seed when `t_override > 0`) into the VCU scratch and the drive; sets `aec_target_seed = seed[4]`, `seed_0`, `seed_1`; runs the probes and the Emits; writes the 1000-step snapshot | partial (C++ comment says 50%) | `CarE.cpp:219-279` |
| 0x4e9c70 | CCar::Update9C70 | mrr_update | points the drive, engine, light and damage records at the damage object (`this+0x3cc`), calls the four wheel `UnkA860`/`M3D030` steps and the hinged and mounted body steps, then emits `MrRCarInit` | partial (C++ comment says 45%) | `CarD.cpp:384-427` |
| 0x4e9af0 | CCar::Update9AF0 | standstill_retire | with `u != 0`: `Diff() >= 5 s`, then the retire or standstill signal from the damage and drive tests | partial (C++ comment says 55%) | `CarD.cpp:321-380` |
| 0x4e9800 | CCar::Sample9800 | emit_sensor | packs `CSigCarSensorData` from the engine, the drive, `this+0xaec`, `this+0x15f4 * 0.001` | partial | `CarD.cpp:246-279` |
| 0x4e9360 | Snap9360 | (snapshot) | not read | - | - |
| 0x4e8f00 | CCar::Copy8F00 | copy_state | copies 21 words of the car header | known (C++ read) | `CarE.cpp:126-148` |
| 0x4e7fb0 | CCar::Vf40x | (slot 64 forwarder) | `E47 Vf16Helper; m_stateA00->S44()` | known (C++ read) | `CarC.cpp:88-94` |
| 0x4e7f90 | CCar::Vf54x | (slot 84 forwarder) | `E47 Vf54Helper; m_scratch1690->S04()` | known (C++ read) | `CarC.cpp:80-86` |

### 3a. Aero gust (Sim7AD0), the equations after the asm correction

Asm-verified form (section 5 C1). Let `v = this+0x1c0` (m/s), `vh = (vx, vy, 0)`, `spd = |vh|`,
`m = def+0x16c` (the inverse mass; `s = -1/m`), `p = wind vector` (world, `g_1659b70+0x24` then
`+0x160a0 +0x10`), `x = Xform68E0(seed) + s*p`, `bz = x_z`, `w = clamp(2.5 * this+0x198, -1, 1)`,
`q = clamp(3 * this+0x166c, 0, 1)`, `a0 = clamp(2 spd, 0, 1)`, `k = clamp(0.01 * spd, 0, 1)`:

- Gate: `wall_penetration > 0`, `bz >= 0`, `spd > 0.001`.
- `nrm = v / spd` (the 3-vector over the horizontal length; C2).
- `dir = -(0.5 k + 0.3 a0) q bz * nrm`.
- `xout = (0, 0, -0.3) x dir`, then `xout_z += w a0 q bz (-0.3)`.
- `F_aero = MB560(dir)` into `S70`, `T_aero = MB560(xout)` into `S74` (force and torque, body frame).

### 3b. Drag (Sim7DC0), equations

`drag = -0.5 * Cd * A * |v| * v` with `Cd = def+0x4490` and `A = def+0x4480`, transformed by
`MB560` into `c20`, then `S70(e40)` (the force). The torque `zb x e40` with `zb = (0, 0, 0.05)`
(the drag acts 5 cm above the centre, `S74`). Then with `w = MB560(this+0x190)`:
`if |w| > 5: tau_damp = -5 (|w| - 5)^2 w / |w|` added to the torque. `Cd*A` is the LSP
pair `DragCoefficient`/`FrontArea`. There is no air-density factor (open 2).

## 4. Open questions

1. **Frame of `this+0x1c0`.** Its `Length` is used as speed (`0x15fc`), and `Sim7DC0` treats it as
   the body-frame or world-frame velocity. Which frame, and whether `Sim7AD0`'s `nrm` (the 3-vector
   over the horizontal length) is meant, is not settled. The asm of `Sim7AD0` does use the 3-vector.
2. **Air density.** The drag `-0.5 Cd A |v| v` has no `rho`. Either the LSP coefficients absorb it
   (`rho = 1`), or a factor is missing. Settled by a recording: `FrontArea*DragCoefficient = 0.8`
   against a known top speed.
3. **`s = -1/InverseMass` in `Sim7AD0`.** This is `-mass`, the mass in kg. It multiplies a world
   vector `p` in the gust sum, which gives a force-times-mass term, not a velocity term. Both the C++
   and the asm do this. The owner of `Sim7AD0` should check which vector `p` is (a force, an
   acceleration or a velocity) and whether `p * InverseMass` was meant.
4. **`def+0x44b0` (dangerous acceleration).** The ctor default is 490.5 (50 g). The `CAR_ROOT` LSP
   sample has `DangerousAcceleration:float[1e+06]`. If the key is bound to `0x44b0`, the sample
   value disables the 50 g flag in `Hook`/`Vf6cx`. The name binding is not checked (the CReal ctor
   has no name string here).
5. **`Hook1FF0` (F4) and `Vf6cx` (the timers) are C++ reads only.** Their asm was not read. Their
   C++ comments are not reliable (the 0.2f bit pattern comment is a known-good exception).
6. **`0x15f4`, `0x15f8`, `0x198`, `0x9f0`, `0x1694`, `0x1698`** are guesses (section 2a).
7. **Aero slot numbering.** The `Vf28x` comment says "slot-40 broadphase worker", while the header
   says slot 10 is `0x4ea020`. One of them is a label error. Not changed here.

## 5. Suspect code

Asm is the reference. Addresses are VAs. C1 is resolved below; C2 remains an unresolved call-sequence
question for the decomp owner.

**C1. `CCar::Sim7AD0`: the drag weight used the clamped speed ramp (resolved, asm-checked).** The
asm at `0x4e7c73` multiplies the unmodified speed by `0.01` before clamping. C++ used `a0 * 0.01`,
where `a0 = clamp(2 * speed, 0, 1)`, which flattened the weight at speed 0.5. It now uses
`speed * 0.01`. The gust vector update and subsequent `bz` gate agree with the asm: `x` is updated
in place by `s*p`, and the gate reads that updated value. A forced wall-penetration replay with
nonzero speed ran through `ToWorld` and matched the original's non-stack state writes; the 12,084-step
reference drive also stayed identical.

**C2. `CCar::Xform6870` (`RBR_E34_CarB.cpp:84-100`): the C++ does not match the call sequence.**
The C++ calls `S30(t, b)` then `ComposeA(b, t, b)`. The asm (`0x4e6870-0x4e68d5`) pushes `t` then
`b`, so the slot-12 call is `S30(b, t)`, and the slot-12 target (`0x519c00`, `mov eax, ecx; ret`)
does not pop, so both pushes stay on the stack. `ComposeA` (`0x51b4c0`, `ret 0xc`) is then called
with no pushes of its own. Its three stack arguments are therefore `b`, `t` and the caller's saved
`edi` (pushed at `0x4e687b`, the value `b` is loaded into `edi` after that). The C++ has `ComposeA(b,
t, b)`. So the third argument (the destination) is `edi_old`, not `b`, and the function returns
`b` (`mov eax, edi`) after a stack that is 4 bytes out of balance. I have not confirmed that this
is what runs (the function is labelled `fixed` in `gameplay-functions.csv`). Check with a peek or
a shadow run before fixing. Route to: the `Xform6870` owner and the lead.

**C3. `CCar::Sim7DC0` and `CCar::Sim7AD0`: `this == (CCar*)0xfffffe50` / `0xfffffe80` tests
(`RBR_E34_CarC.cpp:69, 120, 137`), and `Vf6cx`'s `this+0x1b0` null test (`RBR_E34_CarD.cpp:656`).**
`0xfffffe50 + 0x1b0` and `0xfffffe80 + 0x190` both wrap to 0, so these tests are the asm's null
checks on `this+0x1b0` and `this+0x190` (`Sim7AD0` asm `0x4e7b9f-0x4e7bac`), written as `this ==`.
They are equivalent to the asm and never true for a real object. Not a behaviour bug; rewrite as
`this+0x1b0 ? ... : 0` when the owner next touches these functions.

**C4. `CCar::Hook` (`RBR_E34_CarD.cpp:300`): `*(int*)(this+0xadc) = 0`.** `0xadc` is read as a
float by `Hook1FF0` (`CarC.cpp:207`). The stored bits are 0, which is `0.0f`, so this is correct,
but the field is typed `int` on the write and `float` on the read. Propose `float`.

**C5. Status labels (`notes/progress/gameplay-functions.csv`).** `CCar::Hook` (0x4e9930) is
labelled `bad`, but its C++ matches the asm (F1, F2, the call order, the setter `0x52f450` is
`SetDirty`). If the `bad` label comes from a callee, name it. `CCar::Sim7AD0` is labelled
`verified`; its prior 51.41% score predates the speed-weight correction, and the corrected build is
53.78%.

**C6. `CCar::Update91F0` (`RBR_E34_CarE.cpp:229-232`).** When `t_override > 0`, `seed` is replaced
by the local `loc`, but `src[5]` (the flags, `if ((u & 0x20) == 0)` ...) is still read from the
caller's buffer `p`. If the asm reads the flags from the seed, the gate differs. Not asm-checked.

**C7. `CCar::Vf6cx` (`RBR_E34_CarD.cpp:656-658`).** `MB560` is given `this+0x1c0` (the `+0x10`
after the null test on `this+0x1b0`, as in C3). The null branch is dead for a real object. Cosmetic,
same as C3.

**C8. `CCar::Update9C70` (`RBR_E34_CarD.cpp:391-398`).** The loops use `q1 = this+0x480` and
`q2 = e35+0x3d84` with `q2 += 3` (3 words per hinged body) and `e35 += 0x3db4` (E45 block). These
strides are not checked against the E35 layout (`notes/physics/damage.md` section 1a). Not
asm-checked.

**C9. Labelling of `CCar::Vf28x` as "slot-40".** See open 7. Not a physics bug.

**C10. `CCar::Probe7730` (`RBR_E34_CarE.cpp:148-190`): the sampler verdict words are at
`bigBuf+0x44` and `bigBuf+0x48`.** The asm calls `Sampler54A0` at `0x4e7815`, then reads
`[esp+0x74]` and `[esp+0x78]` at `0x4e7820` and `0x4e783f`. Its buffer base is `esp+0x30` at the
sampler call, so these are offsets `+0x44` and `+0x48`, respectively. The C++ now uses those
offsets. A forced replay from the captured entry made the sampler return 1 with only `+0x44` set,
then with only `+0x48` set and a negative dot product; both output verdicts (1 and 2) matched the
original exactly. The normal drive does not take either branch.
