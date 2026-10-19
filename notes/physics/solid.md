# Solid body (E47Solid, CSolid): physics semantics

Pass 4 of the physics semantics brief, E47 part (`lead/briefs/physics-semantics.md`). Notes only;
no source edits. Companion: `notes/physics/physics-core.md` (E46 CPhysics side). Slot numbers and
names follow `lead/reports/p2w8-table.txt` (the rewritten E47 vtable), with today's C++ name in
brackets where it differs. `rigid-body.md` covers the E43/E33 base classes and the CSolid field
roles; this file adds the asm checks and the suspect list for E47.

Confidence: `known` (exe or LSP key name, or the operand is unambiguous in asm), `inferred` (strong
argument from use or asm), `guess`. Reccmp % comes from `base/RBR.txt`, status from
`notes/progress/gameplay-functions.csv`. Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump -d` on
`RichardBurnsRally_SSE.exe` (coff-i386, VA addresses). Float constants read from the image: 1.0,
0.5, 0.2 (`0x72f428`), 0.8 (`0x72f610`), 1e-5 (`0x795e8c`), 1e-4 (`0x72fd28`), 5.0 (`0x72ae44`),
10.0 (`0x72b154`), -50.0 (`0x7304ac`), 5000.0 (`0x731054`).

## 1. Overview

A `CSolid` is a rigid body that is not a car: a wheel (`CWheel : CSolid`) and any mounted or
hinged part with the E47 vtable. It sits on `list450`/`list480` (`step.md` 1c) and takes part in
`StepForward` through its vtable. One substep for a CSolid, in order:

1. Force input (S5C, then S60): `E46_Dispatch240` calls `E43Vf5C`, then `E46_Accum250` adds
   gravity and the other terms into the force accumulator at `this+0x370` through S70
   (`E46_Accum340`, `v * this+0xd0` per component).
2. Force to acceleration (S64 = `Vf25x`): the vote (`Vf81x`), the contact pass `BigB` when body flag 8 is
   set, then the linear acceleration `0x380 = R(F * InverseMass)` from `0x370`, and the angular
   acceleration `0x320` from the torque shell at `0x310`. Clears `0x370` and `0x310`.
3. Integration (S6C = `Vf27x(dt)`): `w += alpha dt`, `v += a dt`; flag-8 zeroing; `VfC8(dt, lin, ang)`
   moves the pose; accel estimates and filters (`0x230..0x260`); history copies `0x190 -> 0x200`,
   `0x1c0 -> 0x220`; `E43Vf6C(dt)`; `Vf24x(dt)` (sleep); signal emit of the pose.
4. Contact window (`CollideWork`, `step.md` 1b): `VfDx(a)` steps the pose back to fraction `a` of the
   window, `VfEx(a)` advances the rest of it. Both set the progress fraction `this+0x290`.

Call tree of the reached E47 functions (addresses, reccmp %):

```
Work5500 (S5C, S64, S6C loops)                         0x525500  100%
  E46_Dispatch240 (S5C)                                0x529240   38%  (E43Vf5C, then S60)
    E46_Accum250 (S60)                                 0x529250   38%  -> E43Vf70 = Accum340
      E46_Accum340 (S70, slot 28)                      0x529340   33%  0x370 += v * 0xd0
  Vf25x (S64, slot 25)                                 0x52cb20   54%
    E43Vf64 (direct)                                   0x51a2c0
    Vf81x (vote)                                       0x52c510   53%  -> S70 / S74 when hit
    BigB (flag 8 of 0x60)                              0x52c220   43%  bad
    S30 / MB560 on Scale(0x370, InverseMass) -> 0x380  (C++ uses 0x380, see 5a)
    M51C8C0 on 0x310 with def+0x130 -> S30/MB560 -> 0x320
    E09Vec4::Clear 0x370 (x2), 0x310 (E47ClearPair 0x529e60, 96% reviewed, same clears)
  Vf27x (S6C, slot 27)                                 0x52cc30   25%
    VfC8(dt, lin, ang)                                 0x52c8f0   34%  verified (p2rb-beh-15)
    E43Vf6C(dt) (direct)                               0x51a330
    Vf24x(dt) (sleep)                                  0x52b740   40%  verified (p2rb-beh-15)
    emit CMultiSignal1<CSigMrRBodyState>::Emit         (CSignalManager, g_signalManager + 0xe438)
  Vf58 / S58 (child walk)                              0x51a250  (E43)
  Vf98 (S98, list480)                                  0x51a7c0  (E43)
VfDx (window back-step, CollideWork)                   0x52cfd0   27%  asm-checked (5c)
VfEx (window advance, CollideWork)                     0x52d110   49%  asm-checked (below)
Vf36x / Vf37x (slot 36/37, event emit)                 0x52bf30 / 0x52c130  33% / 34%  not read
Vf74x (slot 74, ComposeA)                              0x52a0e0   30%  not read
Step77 (slot 77, ground probe, S00 call)               0x529d30   48%  crashes on harness (5b)
BigA (flag-8 contact worker, not on step path yet)     0x52b9f0   34%  bad, not read
```

Inputs from other subsystems: the car and drive (through `E43Vf5C` and the forces the car
adds to the accumulator), the terrain probe `g_7c755c` (S00 in `Step77`), the track driveline
(`CDriveLine::SampleBig`, `0x50b300`, used by `VfC8` and `BigB`), the cell grid (`LookupTwin`,
`AppendA980`). Outputs: the pose, the velocities, the emitted `CSigMrRBodyState`, and the cell
registration.

## 2. Data

### 2a. CSolid::CDef (`this+0x34`; key table `0x7cc3f0`, `tweak-keys.md` CSolid)

| offset (record / value) | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x120 / 0x130 | `m_v120` | inverse_inertia_diag | vec3 | 1/(kg m^2) | local inverse inertia diagonal (`M51C8C0` input in `Vf25x`) | known (key `vecLocalInverseInertiaDiagonal`) | key table; asm `0x52cbc4`-`0x52cbd1` |
| 0x140 / 0x14c | `m_u140` | b_special | u32 | flag | key `bSpecial`; no reader found | known (name), meaning guess | key table |
| 0x150 / 0x15c | `m_r150` | angular_threshold | f32 | rad/s | key `AngularThresholdVelocity`; no reader found | known (name), meaning guess | key table |
| 0x160 / 0x16c | `m_r160` | inverse_mass | f32 | 1/kg | key `InverseMass`; `Vf25x` multiplies the force by it | known (key, asm `0x52cb75`-`0x52cb7b`) | key table; asm |
| 0x170 / 0x17c | `m_r170` | linear_threshold | f32 | m/s | key `LinearThresholdVelocity`; no reader found | known (name), meaning guess | key table |
| 0x1a0 / 0x1ac | `m_u1a0` | sleepable | u32 | bool | read by `Vf24x` | known | key table; `Vf24x` |
| 0x1b0 / 0x1bc | `m_r1b0` | sleep_delay | f32 | s | read by `Vf24x` | known | key table; `Vf24x` |
| 0x1c0 / 0x1cc | `m_r1c0` | linear_sleep_velocity | f32 | m/s | `Vf24x` compares `|v|^2` with its square | known | key table; `Vf24x` |
| 0x1d0 / 0x1dc | `m_r1d0` | angular_sleep_velocity | f32 | rad/s | `Vf24x` compares `|w|^2` with its square | known | key table; `Vf24x` |
| 0x1e0 / 0x1ec | `m_u1e0` | very_soft | u32 | flag | no reader found in E47 | known (name) | key table |

LSP ranges (`notes/formats/physics-lsp.md`): car `InverseMass` 0.000714 to 0.000869 (1150 to 1400 kg).

### 2b. CSolid object (`this`, 0x3d0 bytes; CBody base 0xd0, CState at 0xe0, CScratch at 0x280)

Vector members are `NTweak::CVector`: the xyz data is at `+0x10` of the object. Offsets are
CSolid-object offsets (`this`). Corrections to `rigid-body.md` are marked **(corr.)**.

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x34 | `m_def` | def | ptr | - | `CSolid::CDef` | known | `Vf25x`, `Vf27x` asm `0x52cb72` |
| 0x30 | - | owner | ptr | - | first word of the emitted signal (`Vf27x`) | inferred | `Vf27x` asm `0x52cf5b` |
| 0x44 | - | parent | ptr | - | non-null: attached to a parent frame; no cell re-registration | inferred | `VfC8`, `VfDx`, `VfEx` (`cmp 0x44, 0`) |
| 0x58 | - | cell_link | link | - | target of `AppendA980` when free | inferred | `VfDx`, `VfEx`, `VfC8` |
| 0x60 (byte) | - | body_flags | u8 | bits | bit 0x8: BigB contact pass in `Vf25x`, zeroing in `Vf27x`, track sampling in `VfC8`; bit 0x4: window contact (`Work5640`) | known (asm), bit names guess | `Vf25x` `0x52cb65`, `Vf27x` `0x52ccb9`, `VfC8` `0x52cacf` |
| 0x0d0 | - | force_scale | f32 | - | multiplies the force input in `Accum340` (`v * k`); set by `E46_SetD0` | guess (meaning) | `Accum340` asm `0x529343` |
| 0x0f0 / 0x100 | - | orientation (obj / data) | quat | - | body orientation (x, y, z, w; w written as 1 in `VfEx` before the product) | inferred | `VfC8`, `VfDx`, `VfEx`; key `vecParentRotation` (state 0x10) |
| 0x140 | - | position | vec3 (+pad) | m | world position; `+0x144` is used as the second cell axis | inferred | `VfC8`, `VfDx`, `VfEx`, `Vf27x` emit (0x14 payload) |
| 0x180 / 0x190 | - | angular_velocity (obj / data) | vec3 | rad/s | body angular velocity (frame: Open 2) | known (key `vecRelativeAngularVelocity`, state 0xa0) | `Vf27x`, `VfC8` |
| 0x1b0 / 0x1c0 | - | linear_velocity (obj / data) | vec3 | m/s | body linear velocity | known (key `vecRelativeLinearVelocity`, state 0xd0) | `Vf27x`, `Vf24x`, `VfC8`, `VfEx` |
| 0x1f0 / 0x200 | - | prev_angular_velocity (obj / data) | vec3 | rad/s | previous-step `w` (copied at the end of `Vf27x`) | inferred | `Vf27x` asm `0x52cf0e`-`0x52cf26` |
| 0x210 / 0x220 | - | prev_linear_velocity (obj / data) | vec3 | m/s | previous-step `v` | inferred | `Vf27x` asm `0x52cf2b`-`0x52cf43` (reads at `0x220` in asm, suspect 5d) |
| 0x230 | - | accel_linear_est | vec3 | m/s^2 | `(v - v_prev) / dt` | inferred | `Vf27x` (asm `0x52cdbd`-`0x52ce0b`) |
| 0x240 | - | accel_angular_est | vec3 | rad/s^2 | `(w - w_prev) / dt` | inferred | `Vf27x` |
| 0x250 | - | accel_linear_filt | vec3 | m/s^2 | `0.2 * old + 0.8 * est` | inferred (constants asm-read) | `Vf27x` asm `0x52ce52`-`0x52ce7e` |
| 0x260 | - | accel_angular_filt | vec3 | rad/s^2 | `0.2 * old + 0.8 * est` | inferred | `Vf27x` asm `0x52ce81`-`0x52ced9` |
| 0x270 | - | sleep_timer | f32 | s | time below the sleep thresholds, `+= dt` in `Vf24x` | inferred | `Vf24x` |
| 0x274 | - | sleep_armed | i32 | bool | the `acc[1]` flag in `Vf24x` | inferred | `Vf24x` |
| 0x280 | `CScratch` | scratch | struct | - | window scratch (0x150 bytes); `+0x10` is `progress` (0x290) | known (layout) | `E47SolidScratch` ctor |
| 0x290 | `m_f10` | window_progress | f32 | 0..1 | fraction of the contact window already applied (1.0 = at end) | inferred | `VfDx`, `VfEx` (compare and store) |
| 0x2a0 | - | pose_window_start_quat | quat | - | orientation at window start (`SetFromQuat` in `E47_Loops`) | inferred | `VfDx` (`q_end`), `Loops` `0x52ad..0x52b3` |
| 0x310 | `E44Shell` | torque_shell | object | - | torque input (through `M51C8C0`, cleared with `0x370`) | guess | `Vf25x` asm `0x52cbe3`, `0x52cc15` |
| 0x320 | - | angular_accel | vec3 | rad/s^2 | angular acceleration added to `w` in `Vf27x` | inferred | `Vf25x` (`S30`/`MB560` on `0x310` result), `Vf27x` |
| 0x330 | - | pose_window_start_pos | vec3 | m | position at window start, used by `VfDx`/`VfEx` path | inferred | `VfDx`, `Loops` |
| 0x370 | - | force_accum | vec4 | N | force accumulator: `Accum340` adds `v*k`; `BigB` adds through slot 28; `Vf25x` consumes and clears | known (asm of `Vf25x`, `E47ClearPair`) | `Accum340`, `Vf25x` (asm `0x52cb80`-`0x52cbbd`), `E47ClearPair` |
| 0x380 | - | linear_accel | vec3 | m/s^2 | world linear acceleration from `R(F * InverseMass)`; `Vf27x` adds `a dt` to `v` | known (asm `Vf27x` `0x52cc7e`-`0x52cc9b`) | `Vf25x` asm `0x52cb9c`-`0x52cbae`, `Vf27x` |
| 0x3b4 | - | step_flags | u32 | bits | bit 0: set when `dt < 0` in `VfC8`, cleared when `dt > 0` | known (code) | `VfC8` (C++ `0x52c8f0`) |
| 0x17c (state Progress) | `Progress` (CReal) | track_progress | f32 | unknown (driveline distance?) | state key `Progress` (CReal, value at CState 0x9c). Written in `VfC8` flag-8 path by `CDriveLine::SampleBig(pos, prev)` with `SetDirty` on `0x170` | inferred (name known, meaning from SampleBig) | key table; `VfC8` asm `0x52cacf`-`0x52cafe` **(corr.)** |

**(corr.)** `rigid-body.md` 2c and `tweak-keys.md` finding 2 put `Progress` at CSolid `+0x110`. The
key table (`CState` `Progress` at state `+0x90`, value `+0x9c`) and `CState` at `CSolid+0xe0` put it at
object `+0x170`, value `+0x17c`. The `VfC8` asm confirms: `addl $0x170, %esi` then `SetDirty`, and the
flag-8 write goes to `+0x17c`. `VfDx`/`VfEx` still use the scratch `+0x290` (unchanged).

### 2c. Constants used by the E47 functions (asm-read)

| value | use | where |
| --- | --- | --- |
| 1e-5 (`0x795e8c`) | squared-speed "at rest" test (flag 8 zeroing) | `Vf27x` |
| 1e-4 (`0x72fd28`) | BigB: unit vector only when `|W| > 1e-4` | `BigB` `0x52c36e` |
| 0.2 / 0.8 | accel filter | `Vf27x` `0x52ce52`, `0x52ce81` |
| 5000 (`0x731054`) | BigB contact stiffness on the direction term | `BigB` `0x52c346` |
| 10 (`0x72b154`) | BigB clamp of `|W|` | `BigB` `0x52c394` |
| 5 (`0x72ae44`) and clamp 1 | BigB second term weight | `BigB` `0x52c3ad`-`0x52c3c7` |
| -50 (`0x7304ac`) | BigB spring coefficient on the second term | `BigB` `0x52c3d0` |

## 3. Functions

Status: `verified` = harness identical; `reviewed` = read, no difference; `bad` = diverges or crashes;
`-` = no status. `asm` = the function was compared with the asm in this pass. Current names first,
slot numbers from the rewritten vtable in brackets.

| address | current name | proposed name | what it computes | confidence | status and evidence |
| --- | --- | --- | --- | --- | --- |
| 0x52cc30 | `E47Solid::Vf27x(a)` [slot 27] | body_integrate | `w += alpha*a` (`0x320`); `v += linear_accel*a` (`0x380`); if flag 8 and `|v|^2 < 1e-5` zero `v` (lin=0), then if `|w|^2 < 1e-5` zero `w` (ang=0); `VfC8(a, lin, ang)`; `acc_est = (x - x_prev)/a` for `v` (`0x230`) and `w` (`0x240`); filters `0x250`, `0x260`; copy `w -> 0x200`, `v -> 0x220`; `E43Vf6C(a)`; `Vf24x(a)`; emit `{owner, quat, pos}` | inferred | asm checked (242 lines). Flag wiring is right (linear test sets `b`, angular test sets `c`; see 5d). The history read is the known 5d bug. Harness: `-` (stalls after 4,060 calls in p2rb-beh-18) |
| 0x52c8f0 | `E47Solid::VfC8(a, b, c)` [internal] | body_pose_step | `a <= 0`: set step flag 1 if `a < 0`, return. Else clear flag 1; if `c`: `q <- normalize(M83DD0(buf, q) )`, `buf = (0.5*a*w, 1)`; `l50 = b ? pos + a*v : pos`; `pi = parent`: if parent byte `+0x60` has flag 4, pose from the parent (`S30`, `ComposeA`); `AppendA980` when no parent; flag 8: `sv = SampleBig(l60, track_progress)`, `SetDirty`, `track_progress = sv`; `pos = l50` | known (quaternion step) / inferred (parent path) | forced parent-bit-clear replay completed the frame with zero non-stack state diffs; reccmp 33.54%. Flag 8 path separately spot-checked (`0x52cacf`-`0x52cafe`) |
| 0x52b740 | `E47Solid::Vf24x(a)` [internal] | body_sleep_step | `|v|^2 < k1^2 and |w|^2 < k2^2` (k1 = `LinearSleepVelocity`, k2 = `AngularSleepVelocity`): `t += a`; at `t > SleepDelay` (and `Sleepable`) zero `v`, `w` and call slot 37 (`Vf37x`); otherwise `t = 0` | known (eq, key names) | verified (p2rb-beh-15, 54,400 calls); reccmp 40.25% |
| 0x52cfd0 | `E47Solid::VfDx(a)` [internal] | body_window_back | if `a < window_progress`: `q <- normalize(a*q + (1-a)*q_start)`; `p <- a*p + (1-a)*p_start`; `window_progress <- a`; cell re-register if `parent == 0`; `S16` | known (eq; asm checked) | root-path forced replay matches; reccmp 27.14%. Asm uses `this+0x160` for the `AppendA980` link (`0x52d0d3`); C++ corrected in item 13 |
| 0x52d110 | `E47Solid::VfEx(a)` [internal] | body_window_advance | if `window_progress < 1`: `s = (1 - window_progress) * a`; `buf = (0.5*s*w, 1)`; `q <- normalize(M83DD0(buf, q))`; `p <- p + s*v`; cell re-register; `S16`; `window_progress <- 1` | known (eq; asm checked) | root-path forced replay matches; reccmp 51.02%. Asm uses half `s` only for quaternion (`0x52d161`) and full `s` for position (`0x52d200`-`0x52d233`); C++ corrected in item 13 |
| 0x52cb20 | `E47Solid::Vf25x()` [slot 25] | body_force_to_accel | see 5a: `E43Vf64`; vote (`Vf81x`) and S70/S74 when hit; BigB if flag 8; `linear_accel = S30/MB560(Scale(force, InverseMass))` into `0x380`; clear `force`; `angular_accel = S30/MB560(M51C8C0(torque_shell, inverse_inertia_diag))` into `0x320`; clear `force` and `torque_shell` | inferred | reccmp 54.17%, behaviour unverified (crashes in p2rb-beh-13). **Suspect 5a** |
| 0x52c220 | `E47Solid::BigB()` | contact_pass_b | for each 0x40-byte record: `W = S140(point)`; `f6 = clamp(0.5*factor, 0, 1)`; `F = direction*(5000*f6)`, then, if `|W| > 1e-4`, add `(W/|W|)*(-50*min(5*weight,1)*min(|W|,10))`; torque is `(point-pos) x F`; transform and apply through slots 28/29; emit the selected record | inferred | verified on the drive, 40 calls; forced one-contact replay matched initialized state fields. Only unwritten w lanes at `+0x31c`/`+0x37c` and event stack padding differed; reccmp 43.14%. Forced replay: see 5b |
| 0x52b9f0 | `E47Solid::BigA()` | - | flag-8 contact worker (not on the step path yet) | - | bad (harness). Not read in this pass; C++ line 312 still tests `this+0x18` and needs a separate asm check |
| 0x52a0e0 | `E47Solid::Vf74x` [slot 74] | - | ComposeA of the body frame with the cross product; slot 1 for `t >= 0.5` | - | reccmp 29.97%, not read in this pass |
| 0x52bf30 | `E47Solid::Vf36x` [slot 36] | body_emit_a | emits the 0x50-byte event (two vector pairs) through `CMultiSignal1::Emit` | - | reccmp 32.62%, not read in this pass |
| 0x52c130 | `E47Solid::Vf37x` [slot 37] | body_emit_b | `SetDirty` on `+0x1d0` and `+0x1e0`, zero `+0x1dc` and `+0x1ec`, then copies and emit (from the C++ comment at `RBR_E47_Slots.cpp:243`) | inferred | reccmp 34.38%, not read in asm |
| 0x529d30 | `E47Solid::Step77(a, b)` [slot 77] | probe_ground_s00 | `v = pos` with one component minus `*b`; `ok = S00(pos, v', out)` on the terrain object; if ok: `*a = out[+0x20]`, `*b *= out[+0x30]`, return 1; else `*a = 1.0`, return 0 | inferred (see 5e) | reccmp 47.62%, **crashes on the harness** (+0x4a after 3 of 12,084 steps). Suspect 5e |
| 0x529e60 | `E47ClearPair(o)` | clear_forces | `if o+0x280`: clear `o+0x370` (force), then `o+0x310` (torque shell) | known (asm) | reviewed, 96%. Same clears as `Vf25x` end (5a) |
| 0x529340 | `E46_Accum340` [slot 28] | body_force_add | `force[i] += v[i] * this+0xd0` for i = 0..3; log hook on flag `0x4000` | known (eq, asm checked) | verified (p2-harness); reccmp 33.33%. The 33% is the packed/scalar form (`mulps`/`addps`, asm `0x52936e`-`0x52937a`) |
| 0x529250 | `E46_Accum250` [slot 24] | body_gravity_force | `out = (1/InverseMass) * MB560(gravity, S30(this))`, `E43Vf70(out)` | inferred | verified (p2-harness); reccmp 38.03%. (The `1/InverseMass` is a mass; step.md 2b withdrawn, rigid-body.md 5b) |
| 0x529ea0 | `E47Solid::Vf29x(p)` [slot 29] | body_scratch_add | (from the C++ comment) gated accumulate of `p` into scratch `m_v90`, then a conditional slot-0 call | guess | verified; reccmp 35.29%, not read in asm |
| 0x52c700 | `E47Solid::Vf17y(a)` [slot 17] | - | member copy gated by the 0x400 bit (see the comment at `RBR_E47_Big.cpp:409`) | - | verified; reccmp 53.54%, not read |
| 0x52c510 | `E47Solid::Vf81x` [vote] | body_vote | contact vote: outputs two vectors (`l50`, `l40`) and returns a hit flag; `Vf25x` passes them to S70/S74 on a hit | guess | reviewed; reccmp 52.50%, not read in asm |
| 0x52b6b0 | `E47_Vf54Helper` [slot 21] | body_s54 | `CEntity` slot 1 on self, then scratch slot 4 | inferred | reviewed; 53.33% |
| 0x52b930 | `E47_ScratchBaseInit` | - | scratch base init | - | reviewed; 76.92% |
| 0x52b8b0 | `E47Solid::NewDef` | make_solid_def | factory for `Solid/<NAME>` LSP blocks | known (registry) | 23.81%, not read; `CSolid::CDef` key table is `tweak-keys.md` |
| 0x52d640 | `CSolid::CDef::Vf50` | - | `E47Solid` call with the owner (`this`) first (p2rb fix) | - | 58.82%, not read |
| 0x52d480 | `E47Solid::E47Solid` (ctor) | - | ctor; unpaired `E47SolidState` dtor (transplant blocker) | - | 60.56%, not read |
| 0x52d280 | `E47SolidState::E47SolidState` | - | state object ctor | - | 49.66%, not read |
| 0x529950 / 0x5299f0 | `E47_SolidDefDtor` / `CSolid::CDef::Vf54` | - | dtor / key reader | - | reviewed (88%, 62%) |
| 0x529a50 / 0x529af0 | `E47SolidState::Vf17x` / `E47SolidScratch::Vf04` | - | state / scratch helpers | - | verified / reviewed |
| 0x52a500 | `E47Solid::Dot60` | - | `CDef` at `this+0x34` read directly (p2rb fix) | - | verified (32 calls); 40.51% |
| 0x52a570 | `E47Solid::Vf77x` [slot 80] | - | - | - | verified; 34.45% |
| 0x52ad40 / 0x52add0 / 0x52aec0 | `Vf43x` / `Vf44x` / `Vf47x` | - | packed 4-wide loops (`Vf44x` the p2rb order fix) | - | verified; 34.78 / 52.63 / 12.86% (scalar vs packed, score only) |
| 0x52b110 / 0x52b1f0 / 0x52b2e0 / 0x52b380 | `Vf41x` / `Vf42x` / `Vf45x` / `Vf46x` | - | packed 4-wide loops (p2rb fixes for 41x, 45x) | - | 53.66 / 57.14 / 50.49 / 54.70%, not read |

## 4. Open questions

1. **`Vf25x` operand.** The asm reads the force from `0x370` (5a); `E47ClearPair` clears `0x370`
   and `0x310`. Confirm that `0x380` is the output only, by a harness peek of `+0x370` and `+0x380`
   around one `Vf25x` call on a wheel.
2. **Angular velocity frame.** Is `0x190` in the body frame or the world frame? `VfC8` multiplies the
   buffer `(0.5 a w, 1)` on the left of `q` by `M83DD0(buf, q)`; the operand order is what
   `M83DD0` does, which is not read here.
3. **`0x310` (torque shell).** `M51C8C0(0x310, inverse_inertia_diag)` produces the angular
   acceleration. What is the shell's content (a rotation, or the torque)? `E44Shell` is the C++ name,
   which is not a physical name.
4. **`BigB` meaning.** The record layout (`+0x10` direction, `+0x30` float, `+0x34` tag `200`,
   `+0x38` fraction) and the contact type (`E36Collect`) are not checked against a writer. The
   constants (5000, 10, -50, 5) suggest a contact spring; confirm with a harness run once the
   asm is rewritten.
5. **`VfDx` and `VfEx` poses.** `0x2a0` (quat) and `0x330` (pos) are the window-start pose
   (`Loops` builds a matrix from `0x2a0` and adds `0x330` to a position). Confirm with a writer
   that the start pose is stored at `progress = 0`.
6. **Signal payload.** `CSigMrRBodyState` is not defined in our source (forward declaration only,
   `RBR_Signal.h`). Its layout decides whether the overlap in 5f is a bug. Who consumes it?
7. **`Step77`'s terrain object.** `g_7c755c` (slot 0 `S00`) is the ground probe in `E34Call0_3`. Which
   class owns it and what does it return in `out[+0x20]` and `out[+0x30]`?
8. **`Accum340` `k`.** `this+0xd0` is set by `E46_SetD0`. Its meaning (a scale per substep, a mass
   factor, a weight) is not read.

## 5. Suspect code

Each item: C++ (file:line), why it is inconsistent, what the asm does. Confidence is that of the
asm operand, not of the physics.

### 5a. `Vf25x` uses `this+0x380` as the force and clears the acceleration (high, asm-checked)

- C++: `src/rbr/RBR_E47_Slots.cpp:221` (`recv = this + 0x380`), `:223` (`Scale(k, l30)` on `recv`),
  `:228` (`Clear` on `recv` after the `MB560` into `m380`), `:238` (`Clear` on `recv` at the end).
  `:239` clears `0x310` (correct).
- Why inconsistent: `0x380` is the output of the transform on the same line (`:226`, `MB560(l30, g,
  m380)`), so the function reads last step's acceleration as the force, and clears the new
  acceleration (`:228`) before `Vf27x` reads it. The force in `0x370` is never read, and `Accum340`
  (`0x529340`) and `BigB`'s `S28` add to `0x370`.
- What the asm does: `0x52cb80`: `leal 0x370(%esi), %edi`; `0x52cb89`: `movl %edi, %ecx` and
  `calll 0x473cd0` (CFloat3::Scale, reads `ecx` = the vector, writes `out`); so the receiver is
  `0x370`. The output is `m380` (`0x52cb98`-`0x52cbae`). `0x52cbb9`-`0x52cbbf`: `movl %edi, %ecx`,
  `calll 0x465d80` (Clear) on `0x370`. At the end, `0x52cc0c`-`0x52cc10` clears `[esp+0xc]` (= `0x370`)
  and `0x52cc15`-`0x52cc17` clears `0x310`. The Scale and Clear constants: `Scale` multiplies by
  `InverseMass` (`0x52cb75`).
- Effect: the force never produces linear acceleration. `Vf27x` adds `0x380 * dt` to `v`, but
  `0x380` is zero at that point in the C++. So a `CSolid`'s linear velocity is never driven by
  gravity or contact forces. `E47ClearPair` (96%) already clears `0x370` and `0x310`, which matches
  the asm and not the C++. Route to the E47 owner first: `recv` should be `this + 0x370` at `:221`,
  `:223` and `:228`, and `:238` should clear `0x370` (line 239 keeps `0x310`).

### 5b. `BigB` contact list, force and event match the asm on a forced contact (asm-checked)

- `E36Collect` writes the record-list pointer to the second stack argument and the count to the
  third. `BigB` now passes `&dc` then `&d8`, matching the `push &dc; push &d8` sequence at
  `0x52c231`-`0x52c244`.
- For each 0x40-byte record, the asm gets `point` through slot 20 and its length. The direction
  force is `direction * (5000 * clamp(0.5 * fraction, 0, 1))`. When `len > 1e-4`, it adds
  `(point / len) * (clamp(5 * weight, 0, 1) * min(len, 10) * -50)`. The C++ keeps that multiply
  order (`spring *= len; spring *= -50`) and crosses `point - body_position` with the combined
  force for the torque.
- Slot 12 returns its receiver but leaves its pushed output argument on the stack. In the original,
  that residue serves as MB560's third argument. The C++ passes all three MB560 arguments itself,
  then removes the one leftover stack word after each slot-12 call to keep the caller's stack aligned.
- The emitted contact payload copies aux/contact/point vectors to `event+0x10`, `+0x20`, and `+0x30`.
  A forced one-record replay captured at `CSolid::Emit`: initialized fields and vector xyz values
  matched. Event padding at `+0x8..+0xf`, point.w at `+0x3c..+0x3f`, and tail bytes at
  `+0x40..+0x43` differed as uninitialized stack contents. Body state matched except the
  uninitialized w lanes at `+0x31c` and `+0x37c` (torque shell and force accumulator); Rust's contact
  test also excludes these unwritten lanes. The ordinary drive's 40 observed calls had no contact
  record.

### 5e. `Step77` reads and writes the wrong components (medium, asm-checked, arithmetic to be re-checked)

- C++: `src/rbr/RBR_E47_Math.cpp:79` `v.m[2] -= *b;`; `:83` `*(int*)a = *(int*)(local50 + 20)`;
  `:84` `*b *= *(float*)(local50 + 36)`.
- What the asm does: the copy goes to `v` at `esp_T-0x50` (`0x529d42`, `leal 0x14(%esp)` with
  `esp = T-0x64`). `0x529d4b`: reads `0x18(%esp)` (= `T-0x4c`, the copy's `m[1]`) and `0x529d68`
  writes it back, so the component changed is `m[1]` (y), not `m[2]`. The `S00` argument is the
  next pointer `0x529d63` (`leal 0x14(%esp)` with `esp = T-0x68`), which is `copy - 4`. So `S00`
  receives a vector shifted by one float. The output `local50` is at `T-0x44`: `0x529d7a`
  (`0x40(%esp)` = `out+0x20`) gives `*a`, and `0x529d88` (`0x50(%esp)` = `out+0x30`) gives the
  scale. The C++ reads `+20` and `+36`.
- Effect: the probe point and the two outputs differ from the original, so the ground probe is
  wrong. This fits the harness crash at `+0x4a` (the function is on the path). Stack arithmetic is
  inferred from the push order; re-check with the call before routing.

### 5f. The emitted pose payload overlaps: the 16-byte position copy at `+0x14` overwrites the quaternion at `+0x20` (medium, asm-checked)

- C++: `src/rbr/RBR_E47_Big.cpp:639` `*(E01Vec4*)(sig + 0x20) = quat`; `:640`
  `((E01Vec4*)(sig + 0x14))->CopyFrom(self + 0x140)`.
- Why inconsistent: the position is copied as 16 bytes at `sig+0x14`, so it covers `sig+0x14..0x23`,
  which overlaps the quaternion at `sig+0x20..0x2f`. The copy happens after the quaternion copy, so
  the quaternion's first float becomes position `[3]` (`self+0x14c`).
- What the asm does: `0x52cf93`-`0x52cf9e` do the same two copies in the same order (position
  after quaternion). The C++ matches the asm, so the overlap is in the original. Either
  `CSigMrRBodyState` has a 3-float position (and the copy is wider than the struct), or the consumer
  reads the quaternion's x as position `[3]`. The consumer is not in our source (open 6). Route to
  the signal owner: the emitted struct is not defined in our code.

### 5g. Verified-as-consistent items (no bug)

- `VfDx` (`0x52cfd0`) and `VfEx` (`0x52d110`): the asm operand order matches the C++ (receiver,
  arguments, `E18_QuatNormalize`/`Norm` target `0x100`, `LookupTwin(x, 0x144)`, the extra `0x160`
  stack argument for `AppendA980`, `S16` at slot 16, progress stored last). C++ now uses `this+0x160`
  for the root link in VfDx, and the full step for the VfEx position while halving only the quaternion turn;
  both root paths match forced replays. See F2 item 13.
- `Vf27x` flags: the linear test (`0x1c0`) controls the position (`VfC8` arg `b`), the angular test
  (`0x190`) the orientation (arg `c`); `VfC8` gates them the same way. Earlier reading of the asm
  (pushes) looked swapped; re-reading the `0x52ccb9`-`0x52cd66` block shows it is not.
- Accum340 and E47ClearPair: match the asm.

### 5h. Not a bug, but a warning

- `Vf27x` divides by `a` with no guard (`inv = 1/a`, asm `0x52cdd0`-`0x52cde3`, `divss`). `a` is
  `MaxTimeStep` (0.007 retail). A zero `MaxTimeStep` would give inf estimates. Note only.

## Notes for the lead

- Route first: 5a (`Vf25x`, E47 owner). It explains why a `CSolid` does not accelerate from forces,
  and `BigB`'s contact forces are added to the same accumulator (`S28`), so they are lost too.
- 5b (`BigB`) is now asm-checked on a forced one-contact replay; the drive still takes its empty-list path.
- 5e (`Step77`) is on the drive path and crashes; the stack arithmetic should be re-checked by the
  owner, or with a harness peek of the `S00` arguments.
- 5f: the signal owner needs `CSigMrRBodyState`'s layout.
- Corrections to other notes: `rigid-body.md` 2c and `tweak-keys.md` finding 2 (`Progress` at
  `+0x170`/`+0x17c`, not `+0x110`); `rigid-body.md` 4 (open 2) and 5c (VfDx) are now answered in part
  (5g). `rigid-body.md` 5a (`0x1f0`/`0x210` reads) is confirmed by the asm (`0x52cdbd`, `0x52cd77`).
- Not read in this pass (reached, not exact): `BigA`, `Vf74x`, `Vf36x`, `Vf41x`/`Vf42x`/`Vf45x`/`Vf46x`,
  `Vf81x`, `NewDef`, `Vf50`, the two ctors, `Vf43x`/`Vf44x`/`Vf47x`, `Vf17y`. Their rows are status only.
