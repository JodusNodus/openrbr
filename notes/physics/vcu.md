# VCU (vehicle control unit, E49/E50): physics semantics

Pass 2 of the physics semantics brief. Notes only; no source edits.
"Current" names are the C++ reconstruction's. "Proposed" names are ours.

Confidence: `known` (exe or LSP names it, or the code is unambiguous), `inferred`
(strong argument from use), `guess`.

Asm checked in this pass (`llvm-objdump`, VAs): `Update` control flow (0x537160,
gear 0/1/2 request paths and tail), `UpdateE` (0x537b70), `Step` pedal routing (0x537f50),
the `IsZero2C` call sites. The C++ headers of `Update`, `UpdateC`, `E50_QuerySlip`,
`CopyDef` and `VcuTableLookup` say "28%", "46%", "55%" and "57%". So those are C++-only
here and marked *unverified*.

## 1. Overview

The VCU is the per-car driver-aid unit. It takes the pedal and handbrake inputs and
decides:

- gear requests (rpm thresholds, neutral and launch rules),
- a launch clutch factor (first gear),
- a rev-limit cut,
- the diff-lock fractions (centre, front, rear), from throttle, brake, speed, and a
  left-foot-braking variant of the centre-diff maps,
- the pedal values sent to the engine, drive and lights.

`CScratchSim::Step(dt)` (`src/rbr/RBR_E50_Sim.cpp:506`) runs once per step. Order:

1. `Update(dt)`: gear logic (`:126`).
2. Clutch: `UpdateC(dt)` if `m_unk4C != 0`, else fixed `Set4820(this+0x1c)` (`:510-513`).
3. `UpdateCut()`: rev-limit cut (`:25`).
4. Pedal routing to engine (`SetScratch08`), drive (`Set44A0`, `Set30`) and the brake
   light (`Set1e40`), section 3f.
5. `UpdateE()`: diff locks to drive channels 0-2 (`:374`).
6. Gear-neutral light `Set1ed0`; shift-cooldown decay of `b+0x64`.

`StepBasic` (`:561`) is the same without step 1. It keeps the clutch, as the asm does (`0x5380a4`-`0x5380c3`, C++ `:565-571`).

Input: the per-step packet goes to `CopyFrom` (`:491`), which tail-calls `UpdateD`. The
setup object goes to `CopyDef` (`:418`), which fills the CDef maps and the scratch flags.

## 2. Data

### 2a. CDef (`CVehicleControlUnit::CDef`, 0xcac bytes; `car+0x250`)

Dictionary table `0x7ce080` lists names. Entry id = object offset / 4 (checked for every
named entry). The value is the CReal's `+0xc` word. The code reads `def + value_offset`.

| obj | value | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0x0c + 0x20k (k=0..9) | 0x18 + 0x20k | `m_unk0C[k].m_unk00` | gear_k_upshift_rpm | f32 | rpm | shift up when engine rpm > value; 0 = off | name known (dict `Gear{k}Upshift`, id 3+8k); rpm inferred | `Update` tail, `:235`-`:240` |
| 0x1c + 0x20k | 0x28 + 0x20k | `m_unk0C[k].m_unk10` | gear_k_downshift_rpm | f32 | rpm | shift down when rpm < value; 0 = off | name known (dict `Gear{k}Downshift`, id 7+8k) | `Update` tail, `:244`-`:249` |
| 0x14c | 0x158 | `m_unk14C[0]` | shift_lockout_up_s | f32 | s | lockout after an up request (ctor 0.7) | name none; meaning inferred | `RBR_E50_Vcu.cpp:160-161`; read by `Update`, `UpdateD` |
| 0x15c | 0x168 | `m_unk14C[1]` | shift_lockout_down_s | f32 | s | lockout after a down request (ctor 0.2) | name none; meaning inferred | `RBR_E50_Vcu.cpp:162-163`; `UpdateD` bit 1 |
| 0x16c | 0x178 | `m_unk14C[2]` | clutch_ramp_rate | f32 | 1/s | clutch falls at this rate (ctor 4.0) | name none; meaning inferred | `RBR_E50_Vcu.cpp:164-165`; `UpdateC` |
| 0x17c | 0x188 | `m_unk14C[3]` | rpm_limit | f32 | rpm | rev cut (ctor 7800) | known name (dict `RPMLimit`, id 0x5f) | `UpdateCut` |
| 0x18c | 0x198 | `m_unk14C[4]` | centre_diff_handbrake_release | f32 | handbrake 0..1 | centre lock = 0 when handbrake >= value | known name (dict `CenterDiffHandbrakeRelease`, id 0x63) | `UpdateE` `:381` |
| 0x19c | 0x1a8 | `m_unk14C[5]` | left_foot_brake_threshold | f32 | pedal 0..1 | left-foot maps when value != 0 and brake > value and throttle > value | known name (dict `LeftFootBrakeThreshold`, id 0x67) | `UpdateE` `:383` |

Uniform pedal maps: 11 knots over pedal [0, 1] (step 0.1, `VcuTableBlend(…, 0, 1, 11)`).
Record stride 0x10, knot value at record `+0xc`.

| obj (record 0) | current name | proposed name | evidence |
| --- | --- | --- | --- |
| 0x1ac | `m_unk1AC` | centre_throttle_lock | `UpdateE` normal centre branch `:374-412`, `CopyDef` |
| 0x25c | `m_unk25C` | centre_brake_lock | `TableMax` arg 2 |
| 0x46c | `m_unk46C` | lf_centre_throttle_lock | `UpdateE` LF branch `:374-412` |
| 0x51c | `m_unk51C` | lf_centre_brake_lock | `UpdateE` LF branch `:374-412` |
| 0x72c | `m_unk72C` | front_throttle_lock | `UpdateE` front `:374-412` |
| 0x7dc | `m_unk7DC` | front_brake_lock | `UpdateE` front `:374-412` |
| 0x9ec | `m_unk9EC` | rear_throttle_lock | `UpdateE` rear `:374-412` |
| 0xa9c | `m_unkA9C` | rear_brake_lock | `UpdateE` rear `:374-412` |

Speed maps: 11 pairs, record stride 0x20. Key (x) at `+0xc`, value (y) at `+0x1c`. Read by
`VcuTableLookup` (`RBR_E49_VCU.cpp:144-205`).

| obj (record 0) | current name | proposed name | evidence |
| --- | --- | --- | --- |
| 0x30c | `m_unk30C` | centre_speed_lock | `TableMax` arg 3; key `f = Get4540 * 0.3` |
| 0x5cc | `m_unk5CC` | lf_centre_speed_lock | `UpdateE` LF branch `:374-412` |
| 0x88c | `m_unk88C` | front_speed_lock | `UpdateE` front `:374-412` |
| 0xb4c | `m_unkB4C` | rear_speed_lock | `UpdateE` rear `:374-412` |

Names for the maps come from the C++ member names only. The dictionary entries for them
are not confirmed (Open 1). Layout and stride are confirmed by `CopyDef`'s strides and by
the reads in `VcuTableBlend` and `VcuTableLookup`.

### 2b. CState (`CVehicleControlUnit::CState`, 0x68 bytes, `car+0x120`; `CScratchSim::m_unk4`, "b")

| off | current | proposed | type | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x2c | `b+0x2c` | shift_lockout | f32 s | shift requests are blocked while nonzero (`IsZero2C` / `IsZero2Cb` test == 0); set to a `def` lockout on each request; decays by dt in the tail | known (code and asm) | `Update`, `UpdateD` |
| 0x40 (value 0x4c) | `m_unk40` CReal | launch_clutch | f32 0..1 | clutch value sent to drive via `Set4820` (drive `+0x60`, where 1 = slipping, from `Get4550`): jumps up to target, falls at `clutch_ramp_rate` | known (code); meaning inferred | `UpdateC` `:295`-`:301`; `DriveB:Get4550` |
| 0x58 | `b+0x58` | pedal_swap | f32 flag | 1 swaps pedal routing in `Step` (see 3f). Written by two code paths with different meanings (3d, 3a) | flag known; meaning unknown | `Step`, `Update` `:202-205`, `UpdateD` `:355`, `:360` |
| 0x60 | `b+0x60` | gear_timer | f32 s | counts for the neutral (gear 0) and second-gear (gear 2) low-speed rules | known (code and asm) | `Update` gear 0 and 2 |
| 0x64 (100) | `b+100` | shift_cooldown | f32 s | 0.05 s set on every request; decays in `Step`; read by `UpdateCut` | known (code and asm) | `UpdateCut` `:25`, `Step` |

### 2c. CScratchSim (per-step VCU scratch, `car+0x850`)

| off | current | proposed | meaning | confidence |
| --- | --- | --- | --- | --- |
| 0x00 | `m_unk0` | def | CDef pointer | known (ctor, `*(int*)this` reads) |
| 0x04 | `m_unk4` | state | CState pointer | known (ctor) |
| 0x0c | `m_unk0C` | throttle | pedal 0..1, packet word 0 (`CopyFrom` `:491-499`) | known (copy); meaning from the reads |
| 0x10 | `m_unk10` | brake | pedal 0..1, packet word 1 | known (copy) |
| 0x14 | `m_unk14` | handbrake | packet word 2 | known (copy) |
| 0x18 | `m_unk18` | unknown_18 | packet word 4 (packet `+0x10`), copied by `ReadPacket`; no read found in this pass (`vcu/README.md`) | guess (unused) |
| 0x1c | `m_unk1C` | fixed_clutch | packet word 3; sent as clutch when `m_unk4C == 0` | known (copy and `Step` `:510-513`) |
| 0x40 | `m_unk40` | launch_enabled | `CopyDef` from setup +0x18; ctor 1 | known (copy) |
| 0x44 | `m_unk44` | slip_gate_rpm | `CopyDef` from setup +0x28; `E50_QuerySlip` gear > 2 test | known (copy); meaning from use |
| 0x48 | `m_unk48` | slip_gate_speed | `CopyDef` from setup +0x38; `E50_QuerySlip` 2nd-gear test | known (copy); meaning from use |
| 0x4c | `m_unk4C` | launch_mode | 1 if setup +0x18 != 0, else setup +0x48. 1 selects `UpdateC` | known (`CopyDef`) |
| 0x50 | `m_unk50` | vcu_enabled | setup +0x18. Gear logic runs only when nonzero (tail-only otherwise) | known (`CopyDef`, `Update` `:146`) |
| 0x54 | `m_unk54` | engine | `CEngine*` | known |
| 0x58 | `m_unk58` | drive | `CDrive*` | known |
| 0x5c | `m_unk5C` | lights | `CCarLightSystem*` | known |

Packet words used by `CopyFrom` / `UpdateD` (per-step input): 0 throttle, 1 brake, 2 handbrake,
3 fixed clutch, 4 copied to scratch `+0x18` (`unknown_18`, unread), 5 flag byte (bit 0 up request, bit 1 down request unless
`E50_QuerySlip`, bit 2 drive call), 6 E40 argument (`Unk06840`).

## 3. Functions

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x537f50 | `Step(dt)` | vcu_step | section 1 order; pedal routing (3f) | known (asm for routing) | `E50_Sim.cpp:506`, asm `0x537f8d`-`0x538028` |
| 0x5380a0 | `StepBasic(dt)` | vcu_step_basic | as `Step` without `Update` (the clutch stays) | asm, checked against the port (`vcu/step.rs`) | `:561`, asm `0x5380a0`-`0x5381da` |
| 0x537160 | `Update(dt)` | gearbox_update | gear requests (3a) | asm for gear 0/1/2 control flow; C++ header "28%" | `:126-259`, asm `0x537160`-`0x537593` |
| 0x5375b0 | `UpdateC(dt)` | launch_clutch | clutch factor (3b) | unverified (C++ header "28%") | `:271-308` |
| 0x536ee0 | `UpdateCut()` | rev_limit_cut | rev cut flags (3c) | C++ only | `:25-43` |
| 0x5375a0 | `Relay(x)` | - | `Set4820(x)` | known | `:263-266` |
| 0x536f80 | `UpdateEB(p)` | - | `SetScratch08(p)` and `Set44A0(p)` | C++ only | `:53-57` |
| 0x536fa0 | `UpdateFA(p)` | - | `Set30(p)`; brake light `Set1e40(p > 0.1)` | C++ only | `:61-68` |
| 0x536fe0 | `E50_QuerySlip(o)` | slip_gate | `1` if `m_unk44` != 0 and gear > 2 and `(lim/div) * f32(eng * 9.549296)` > rpm_limit (`lim` at `drv+0x644`, `div` at `+0x694`, `0x50` per gear); or `m_unk48` != 0 and gear == 2 and \|speed\| > 2.7777779 m/s (`0x741378`, 10 km/h); else `0`. The asm rounds the rpm to f32 before the ratio multiply; the C++ does not | asm checked; `query_slip` matches 1320 recorded calls | `:75-106` |
| 0x537110 | `CheckSlip(x)` | slip_window | 0 if `m_unk50 == 0`; else `1` if drive `+0x55c` != 0 and 0 < x < 7 | C++ only | `:110` |
| 0x5376e0 | `TableMax(t1,t2,t3,f)` | lock_max | max of `VcuTableBlend(t1, throttle)`, `VcuTableBlend(t2, brake)`, `VcuTableLookup(t3, f)`; ties go to t1, then t2 | unverified (C++ comment: lookup "57%" in `VcuTableLookup`) | `:313-323` |
| 0x537a80 | `UpdateD(fl)` | flag_dispatch | requests from packet flags (3d) | C++ only | `:327-365` |
| 0x537b70 | `UpdateE()` | diff_locks | centre, front, rear locks to drive channels 0, 1, 2 (3e) | asm-checked (one difference, Suspect 1) | `:374-412`, asm `0x537b70`-`0x537c8f` |
| 0x5367f0 | `CSetup::IsZero2C()` | lockout_zero | 1 if `scratch->b+0x2c == 0` | known (asm call with scratch in ecx) | `RBR_E49_VCU.cpp:47` |
| 0x536810 | `CSetup::IsZero2Cb()` | lockout_zero_b | same as `IsZero2C` (twin) | known | `RBR_E49_VCU.cpp:56` |
| 0x537c90 | `CopyDef(src)` | copy_setup | copies the setup's knot records into the CDef maps (strides: 0x10 uniform, 0x20 speed), then the scalars in 2c | C++ only (header "55%") | `:418-487` |
| 0x537f20 | `CopyFrom(src)` | read_packet | words 0-4 to scratch; tail-calls `UpdateD` | C++ only (offsets) | `:491-499` |
| 0x536880 | `VcuTableLookup(t,x,n)` | pair_lookup | piecewise linear, x at +0xc, y at +0x1c | unverified (header "57%") | `RBR_E49_VCU.cpp:144-205` |
| 0x536950 | `E49Proxy::VcuTableBlend(t,x,lo,hi,n)` | uniform_lookup | `f=(x-lo)*(n-1)/(hi-lo)`, interpolate knots at +0xc, clamp at ends | C++ only | `RBR_E49_VCU.cpp:212-240` |

### 3a. Gear logic (`Update`, 0x537160)

Let `rpm = engine_speed * 9.5492964` (Get4520), `v = |car speed|` (`CarGet15F8`), `g` =
drive gear (`+0x54c`), `g*` = commanded gear (`+0x570`), `t` throttle, `p` brake.

Constants (asm and C++ agree): 0.3 (`0x72afcc`), 0.5 (`0x795e18`), 0.9 (`0x795e90`), 0.1
(`0x72b068`), 5000 (`0x731054`), 0.05 (`0x72dd78`), -0.4 (`0x73b6fc`).

- Entry: if `g != g*`, return the pending pointer; no logic runs.
- `Set47B0(v)` (`RBR_E39_DriveB.cpp:80`) does not change the current gear. It sets the commanded gear
  `+0x570 = clamp(+0x570 + v, 0, count-1)` (count = `def+0x654`), and the current gear follows
  later (not traced in this pass).
- Gear logic runs only when `vcu_enabled`. Otherwise go to the tail (table shifts only).
- **Gear 2** (asm `0x537209`-`0x53729a`): if `v < 0.5` and `t <= 0.3`: `b+0x60` counts up
  to 0.4 s. When the timer passes 0.4 and the lockout is zero: `Set47B0(-1)`, lockout =
  `def+0x168`, `b+100 = 0.05`, then **the tail** (Suspect 2).
- **Gear 0** (asm `0x5372b4`-`0x537360`): if `v < 0.5` (`E15_84400` of speed) and `p < 0.3`:
  `b+0x60` counts down to -0.4 s. When it reaches -0.4 and the lockout is zero:
  `Set47B0(+1)`, lockout = `def+0x168`, `b+100 = 0.05`, then **the tail** (Suspect 3).
- **Gear 1** (asm `0x53737a`-`0x537459`): `b+0x58 = 1 if p > t else 0` (Suspect 5).
  - Up request: `t > 0.9`, `p < 0.5`, lockout zero, `rpm > 5000`: `Set47B0(+1)`, lockout =
    `def+0x158`, `b+100 = 0.05`, then the tail.
  - Neutral request: `t < 0.5`, `p > 0.9`, lockout zero, `rpm > 5000` (asm; the C++ tests
    `rpm <= 5000`, Suspect 4; the asm is the behaviour): `Set47B0(-1)`, lockout = `def+0x158`, `b+100 = 0.05`, then the
    tail.
- **Other gears** (3+ or negative): clear `b+0x60` and `b+0x58`, then the tail.
- **Tail** (asm `0x537459`): if `g == g*`, let `k = Get4530()` (index into the gear table; the
  dictionary names it `Gear k`). Upshift when `def[k].up != 0`, `rpm > def[k].up`, lockout
  zero, and `CheckSlip(v) == 0`: `Set47B0(+1)`, lockout = `def+0x158`. Downshift when
  `def[k].down != 0`, `rpm < def[k].down`, and lockout zero: `Set47B0(-1)`, lockout =
  `def+0x168`. Both set `b+100 = 0.05`. Otherwise decay the lockout by dt (floor 0).

Note the tail requires the lockout to be zero. So after a request the table shifts stay
blocked until the lockout decays.

### 3b. Launch clutch (`UpdateC`, 0x5375b0), C++ only

Runs when `m_unk4C != 0`. If `g != 1`, or `g == 1` and `g*` is neither 0 nor 2: with
`w = engine_speed` (rad/s, per the C++ constants): `f5 = 1` below 200; `(275 - w)/75` for
200 ≤ w ≤ 275; `0` above. Otherwise `f5 = 1`. The clutch `c = b+0x4c` rises to `f5` at
once. It falls toward `f5` at `clutch_ramp_rate` per second, not below `f5`. Then
`Set4820(c)`. Units are inferred from the 200-275 range (1900-2600 rpm if rad/s).

### 3c. Rev-limit cut (`UpdateCut`, 0x536ee0), C++ only

`over = rpm - rpm_limit`. If `over > 0`: `SetState4(0,0,0,0)` when `over > 300`, else
`SetState4(1,0,1,0)`. If `over <= 0` and the cooldown `b+100 > 0`: `SetState4(0,1,0,1)`. Else
`SetState4(1,1,1,1)`. The flags' meaning belongs to the engine notes.

### 3d. Request flags (`UpdateD`, 0x537a80), C++ only

Flag byte bit 0: `Set47B0(+1)`, lockout `def+0x158`. Bit 1: unless `E50_QuerySlip`,
`Set47B0(-1)`, lockout `def+0x168`. Bit 2: `Unk06840(packet+0x18)` on the drive
(`CDrive::M6840`, `RBR_E40_LeafC.cpp:209-216`), which sets the commanded gear `+0x570` to the
packet value, or to 1 if that is at or above the gear count (`def+0x654`). The lockout
`def+0x168` is set as well. This is a direct gear selection, not a relative shift; the value is
not clamped below. When any bit fired and `vcu_enabled`: `b+0x58 = 1` if the commanded gear < 1,
`0` if > 0. Cooldown `b+100 = 0.05`.

### 3e. Diff locks (`UpdateE`, 0x537b70)

`f = Get4540 * 0.3` (asm `0x537b78`-`0x537b7d`), used as the speed key for all three
speed maps. The asm passes this `f` (in `edi`) to every `TableMax` call (Suspect 1).

- Centre: if `!(centre_diff_handbrake_release > handbrake)`, lock = 0. Else, if
  `lft == 0`, or `!(brake > lft && throttle > lft)` (ordered, asm `0x537b9b`-`0x537bca`), use
  the normal centre maps. Else use the LF maps. Result: `Set4570(0, lock)`.
- Front: `TableMax(front maps, f)` → `Set4570(1, …)`.
- Rear: `TableMax(rear maps, f)` → `Set4570(2, …)`.

Drive setters the step calls, asm-checked (`vcu/locks.rs` `lock_output`, `vcu/layout.rs` `Reals`):
- `Set4570(ch, v)` (`0x504570`): clamps `v` to [0, 1]. The drive's gate is `drive+0x170` → `+0x90` (`Locks.enabled`).
  With the gate off, the channel's torque (D8 record `+0x14·ch+0x18`, `Locks.torque`; record ratio at `+0x0c`,
  `Locks.ratio`) is 0. With it on, `x = |ratio|` and the torque is `max_torque · v`, where `max_torque` is
  `def+0x58+0x60·ch` (`Locks.max_torque`). `v` is kept for `x ≥ 2`, `(x-1)·v` for `1 ≤ x < 2` and 0 below 1
  (the 2.0 at `0x72ad94`).
- `Set44A0`, `Set30`, `Set4820` (`0x5044a0`, `0x504450`, `0x504820`): each sets its CReal's dirty flag (`+8` = 1)
  and value (`+0xc`). `Set30` also writes `def+0x620 · v` to the CReal at `drive+0x64` (its value at `+0x70`).
  Rust names: `Send::DriveThrottle`, `DriveBrake`, `DriveClutch`, `DiffLock`, `CutFlags`, `EnginePedal`; the CReals are
  `Reals.throttle`, `brake`, `brake_scaled`, `clutch_slip`, `brake_scale` (`vcu/layout.rs`).

### 3f. Pedal routing (`Step`, 0x537f50), asm-checked

`Step` branches on `vcu_enabled && b+0x58 != 0` (asm `0x537f83`-`0x537f92`):

- Not swapped (`vcu_enabled == 0` or `b+0x58 == 0`): throttle → engine and `Set44A0`; brake →
  `Set30`.
- Swapped: brake → engine and `Set44A0`; throttle → `Set30`.

The brake light is on when the `Set30` argument > 0.1 (`Set1e40`). So the light follows the
value sent to `Set30`, not the brake pedal. In swapped mode it follows the throttle.

`Set1ed0(gear == 0)` sets the neutral light. The `b+100` cooldown decays by dt.

## 4. Open questions

1. **Flat table `0x7cd550` vs CDef records.** The dictionary table `0x7cd550` (flat CReal names,
   from the earlier pass) does not match the CDef record offsets used by `TableMax`,
   `UpdateE` and `CopyDef`. The gear and scalar entries in `0x7ce080` match the CDef layout
   (checked for every named entry). The flat table is most likely the setup side. That is
   consistent with `CopyDef`'s strides, but not checked entry by entry.
2. **Unnamed CDef scalars.** `0x14c`, `0x15c`, `0x16c` (ids 0x53, 0x57, 0x5b) have no dictionary
   name. Their names here are inferred from use and ctor defaults.
3. **Gear index (partly resolved).** `Get4530()` returns the drive's current gear `+0x54c`
   (`RBR_E39_DriveA.cpp:201`). The tail uses it as the gear-table index, and the dictionary names
   the entries `Gear0`-`Gear9`. Which LSP gear-list slot each index selects is open
   (`notes/physics/engine-driveline.md` section 4): one reading makes 0 neutral, the other
   does not.
4. **Speed-map key `f = Get4540 * 0.3`.** `Get4540` reads drive `+0x384`. The LSP speed-map axis
   is in m/s (5.56 to 55.6, so 20 to 200 km/h), which suggests a speed key. The scale 0.3 and the
   unit of `+0x384` are not confirmed (engine-driveline section 5, item 3).
5. **`b+0x58` has two writers.** `UpdateD` writes it from the commanded gear. `Update` writes it
   from the pedals in first gear and clears it in gears 3+. Which meaning is intended
   (reverse or neutral use, or braking in first gear) is not determined.
6. **Packet words 3, 4, 6, 10, 14 and the flag byte** are named only from use (`CopyFrom`,
   `UpdateD`, `CopyDef`). No source names them.
7. **Launch constants** (200, 275, 1/75) and their unit (rad/s) are inferred from the range.
   Unverified against asm.
8. **`E15_84400`** (speed in the neutral test) is not examined.
9. **`E50_QuerySlip`** gating (`m_unk44`, `m_unk48`) is checked against the asm (see the QuerySlip row); the C++ rpm order differs.
10. **Knot triples**: each CDef map record's words at `+0x4` and `+0x8` (and the speed maps' `+0x14`, `+0x18`) are copied
    by `CopyDef` with no known meaning (`KnotWords.first` and `second`). Only the value and key words are named.

## 5. Suspect code

C++ vs asm differences (asm is the reference), confirmed in this pass:

1. **UpdateE: the rear diff uses the front lock as its speed key.**
   - C++: `RBR_E50_Sim.cpp:404` and `:410`. `f` is reassigned from `TableMax(…, f)` at `:404`
     and passed as the key again at `:410`.
   - Asm: `0x537b70`-`0x537c79`. The speed key stays in `edi` (`movl 0x8(%esp), %edi` at
     `0x537b95`), pushed for the front call at `0x537c43` and the rear call at `0x537c74`.
   - Reason: the front result overwrites the speed key. The rear lock is evaluated at the
     front lock's value.
2. **Gear 2 → downshift: the C++ returns with the wrong lockout.**
   - C++: `:165-167` sets `Set47B0(-1)` and the lockout, then falls through to the common
     exit at `:221-223`, which writes `def+0x158` and returns `b`. The tail is skipped.
   - Asm: `0x537272`-`0x53729a`. `Set47B0(-1)`, lockout = `def+0x168`, cooldown 0.05, then
     `jmp 0x537459` (the tail). So the lockout is 0x168, and the tail's rpm rules still run.
3. **Gear 0 → first request: the C++ returns early.**
   - C++: `:180-186`. `Set47B0(+1)`, lockout, cooldown, then `return b`.
   - Asm: `0x537338`-`0x537360`. Same stores (lockout `def+0x168`), then `jmp 0x537459` (the
     tail).
4. **Gear 1 neutral request: the C++ rpm gate differs from the asm.** Observed behaviour, not
   a bug in the original: the project reproduces the original exactly, quirks included.
   - C++: `:214-216` requires `!(5000 < rpm)` (rpm ≤ 5000).
   - Asm: `0x537421`-`0x53742e`. `comiss $5000` with `jbe` skipping on rpm ≤ 5000, so the
     request fires only at `rpm > 5000`, the same gate as the up request. Constant `0x731054`
     = 5000.0 checked. Reading: the original tests the neutral request with the same rpm
     threshold as the upshift, so heavy braking at high rpm in first gear selects neutral.
   - The asm then writes `def+0x158` and goes to the tail (`0x53743c`-`0x537284`); C++ returns `b`.
5. **Gear 1 `b+0x58`: equality and NaN differ.**
   - C++: `:202-205` sets 1 when `!(brake < throttle)`, so at brake == throttle, and for NaN,
     it is 1.
   - Asm: `0x537383`-`0x537393`. `comiss throttle, brake`, `jbe` to `0`. So 1 only when
     `brake > throttle`. Equal or NaN gives 0.
6. **Gear 1 first-frame lockout: the C++ refreshes it every frame.**
   - C++: `:214-219`. Every first-gear frame without an up request writes the lockout and
     returns, so the lockout never decays while in first gear.
   - Asm: the lockout is written only when a request fires, and the tail decays it. An up
     request can re-fire after the lockout expires.
   - Effect: C++ blocks the first-gear upshift for as long as the car stays in first gear.
     The asm does not.

Unverified C++ (the file's own header comments flag low fidelity). Check these against
the asm before relying on them:

7. **`Update` (`:126-259`)**: header "28%: skeleton simplified". The gear-2 and gear-0 timer
   arithmetic and the gear-1 `IsZero` order are C++ only.
8. **`UpdateC` (`:271-308`)**: header "28%". The branch at `:281-283` reads `drv+0x570`
   twice in one condition.
9. **`E50_QuerySlip` (`:75-106`)**: header "46%". The port follows the asm order (see the QuerySlip row); the C++ rpm order differs.
10. **`TableMax` / `VcuTableLookup`**: `VcuTableLookup` header "57.53%" (outer compares and the
    x87 interpolation differ). `TableMax` ties are C++ only.
11. **`CopyDef` (`:418-487`)**: header "55%". The stride mapping is checked above, but not
    the loop body in asm.
12. **`StepBasic` (`:561-609`)**: checked in asm. It is `Step` without the gear logic, and the clutch is kept (the C++ agrees).
13. **`UpdateCut` (`:25-43`) and `UpdateD` (`:327-365`)**: asm not checked.

Not behavioural:

14. `CSetup::IsZero2C` / `IsZero2Cb` are called on a `CScratchSim*` through a cast
    (`:162-244`). The asm agrees (the scratch in `ecx`, reads `[scratch+4]+0x2c`). The typing
    is misleading but correct. A named scratch helper would be clearer.
15. `Update` returns `drv+0x564` or `b`, and `Step` (`:509`) discards it. The return value is dead.
