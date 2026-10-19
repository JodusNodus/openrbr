# Rigid body (E43-E47: CEntity, CBody, CSolid): physics semantics

Pass 4 of the physics semantics brief, rigid-body part. Notes only; no source edits.
Class chain (RTTI, per lead): CWheel : CSolid : CBody : CEntity : CEntityFrame : CFrame.
This note covers CEntity (E43), CBody (E33) and CSolid (E47). The E47 vtable is being
rewritten by another worker, so this note uses the physics roles, not the current slot
names. Raw offsets are CSolid-object offsets (`this`), taken from the asm.

Confidence: `known` (exe/LSP name, or the equation is unambiguous), `inferred` (strong
argument from use or asm), `guess`. Status: from `notes/progress/gameplay-functions.csv`.

Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump`. The `NTweak::CVector` convention is checked
in the asm: a CVector object at `X` keeps its xyz at `X+0x10` (the asm adds `0x10` to the
object pointer before reading, e.g. `0x52cd77`-`0x52cd84`).

## 1. Overview

A CSolid is a free rigid body: position, orientation, linear and angular velocity, a
force and torque accumulator, a sleep state, and a def with mass and inertia. It sits in
`CState::list450` (or `list480`) and takes part in `StepForward` through its vtable
(`notes/physics/step.md` 1c). Per substep, for a CSolid:

1. Force accumulation (S5C then S60, `E46_Dispatch240` / `E46_Accum250`).
2. Torque and force scaling (S64, `E47Solid::Vf25x`).
3. Integration (S6C, `E47Solid::Vf27x(dt)`): velocities += accel * dt, accel estimates and
   filters, then the pose integrator `VfC8(dt)` (quaternion and position), then the sleep
   check `Vf24x(dt)`.
4. Contact-time advance (`CollideWork`): `VfDx(a)` (back-step to a fraction of the step)
   and `VfEx(a)` (advance the remaining time), each sets the progress fraction (scratch
   `+0x290`).

Mass and inertia come from the def: `InverseMass` (CReal, value `def+0x16c`) and
`vecLocalInverseInertiaDiagonal` (CVector, `def+0x130`). Both keys are in the exe's key
table (2a), so the names are known, not inferred.

Coverage: the S6C path (`Vf27x`, `VfC8`, `Vf24x`) is read in full. The force
accumulator (S5C/S60, `Accum250`/`Accum340`) and the torque/force pass (`Vf25x`) are
read but the transform chain they call (`S30`, `MB560`, `M51C8C0`) is not. `VfDx`/`VfEx`
are read in full. CEntity and CBody slot bodies are named only (2c).

## 2. Data

### 2a. CSolid::CDef (0x1f0 bytes; `CSolid+0x34` points to it)

The exe key table (`.rdata` `0x7cc3f0`, 16-byte records `{name ptr, 0, offset/4, 0}`,
the same layout as the VCU dictionary in `vcu.md`) names these entries. The value is the
CReal/CVector `+0xc` (CReal) or `+0x10` (CVector) word.

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x120 (CVector, data 0x130) | `m_v120` | inverse_inertia_diag | vec3 | 1/(kg m^2) | local inverse inertia diagonal | known (key `vecLocalInverseInertiaDiagonal`, id 0x48) | key table; `Vf25x` reads `def+0x130` |
| 0x140 (CUnsigned, value 0x14c) | `m_u140` | bspecial | u32 | flag | key `bSpecial` (id 0x50); reader not found | known (name), meaning unknown | key table |
| 0x150 (CReal, value 0x15c) | `m_r150` | angular_threshold | f32 | rad/s | key `AngularThresholdVelocity` (id 0x54); reader not found | known (name), meaning unknown | key table |
| 0x160 (CReal, value **0x16c**) | `m_r160` | inverse_mass | f32 | 1/kg | key `InverseMass` (id 0x58) | known (key); unit from LSP range | key table; LSP car `InverseMass 0.000714..0.000869` = 1150..1400 kg |
| 0x170 (CReal, value 0x17c) | `m_r170` | linear_threshold | f32 | m/s | key `LinearThresholdVelocity` (id 0x5c); reader not found | known (name), meaning unknown | key table |
| 0x1a0 (CUnsigned, value 0x1ac) | `m_u1a0` | sleepable | u32 | bool | key `Sleepable` (id 0x68); read by `Vf24x` | known (key); code confirms | key table; `Vf24x` `def+0x1ac` |
| 0x1b0 (CReal, value 0x1bc) | `m_r1b0` | sleep_delay | f32 | s | key `SleepDelay` (id 0x6c); read by `Vf24x` | known (key); code confirms | key table; `Vf24x` `def+0x1bc` |
| 0x1c0 (CReal, value 0x1cc) | `m_r1c0` | linear_sleep_velocity | f32 | m/s | key `LinearSleepVelocity` (id 0x70); `Vf24x` compares `k1*k1` | known (key); code confirms | key table; `Vf24x` |
| 0x1d0 (CReal, value 0x1dc) | `m_r1d0` | angular_sleep_velocity | f32 | rad/s | key `AngularSleepVelocity` (id 0x74); `Vf24x` compares `k2*k2` | known (key); code confirms | key table; `Vf24x` |
| 0x1e0 (CUnsigned) | `m_u1e0` | very_soft | u32 | flag | key `VerySoft` (id 0x78) | known (name), meaning unknown | key table |

LSP ranges (`notes/formats/physics-lsp.md`): `InverseMass` for car bodies is 0.000714 to
0.000869 (1150 to 1400 kg). `vecLocalInverseInertiaDiagonal` is 0.0007 to 0.0025 per axis.
These units are inferred from the ranges and are plausible for a rally car.

### 2b. CSolid object (`this`, 0x3d0 bytes; CBody base 0xd0, CState at 0xe0, CScratch at 0x280)

Layout and roles from `Vf27x`, `VfC8`, `Vf24x`, `VfDx`/`VfEx`, `Vf25x`, `Accum250`. Vector
fields are `NTweak::CVector`, so the xyz data is at `+0x10` of the object listed.

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x34 | `m_def` | def | ptr | - | `CSolid::CDef` | known | `Vf24x`, `Vf25x` |
| 0x44 | - | parent | ptr | - | non-null: body is attached to a parent (no cell re-registration; parent frame in `VfC8`) | inferred | `VfDx`/`VfEx` gate on `==0`; `VfC8` reads parent flags at `+0x60` bit 4 |
| 0x58 | - | cell_link | link | - | target of `AppendA980` when free (cell grid re-registration) | inferred | `VfDx`, `VfEx` |
| 0x60 (byte) | - | body_flags | u8 | bits | bit 0x4: contact advance (`Work5640`/`Work5760`) and `Work5590`; bit 0x8: BigB and zeroing in `Vf27x`/`Vf25x` | known (code), bit names guess | `Work5640`, `Vf27x`, `Vf25x` |
| 0x0f0 (CVector4 object) | - | orientation_obj | vec4 | - | quaternion object; data at `0x100` | known (key `vecParentRotation`, state `+0x10`) | key table |
| 0x100 (data) | - | orientation | quat | - | body orientation, normalised; `buf[3] = 1` in `VfEx` suggests (x, y, z, w) | known (VfEx writes normalised quat here); layout inferred | `VfC8`, `VfDx`, `VfEx` |
| 0x140 (data xyz) | - | position | vec3 | m | world position; `0x144` is the second axis used by the cell lookup | inferred (signal payload +0x14) | `VfC8`, `VfDx`, `VfEx`, `Vf27x` emit |
| 0x180 (CVector object) | - | angular_velocity_obj | vec3 | rad/s | data at `0x190`; key `vecRelativeAngularVelocity` (state `+0xa0`) | known (key) | key table; `VfC8` reads `0x190` |
| 0x190 (data) | - | angular_velocity | vec3 | rad/s | body angular velocity (see Open 3 on frame) | known (key + `Vf24x` k2 compare) | `Vf27x`, `Vf24x`, `VfC8` |
| 0x1b0 (CVector object) | - | linear_velocity_obj | vec3 | m/s | data at `0x1c0`; key `vecRelativeLinearVelocity` (state `+0xd0`) | known (key) | key table |
| 0x1c0 (data) | - | linear_velocity | vec3 | m/s | body linear velocity | known (key + `Vf24x` k1 compare) | `Vf27x`, `Vf24x`, `VfC8`, `VfEx` |
| 0x1f0 (CVector object) | - | prev_angular_velocity_obj | vec3 | rad/s | data at `0x200`: previous-step angular velocity (copied at end of `Vf27x`) | inferred | `Vf27x` copy `0x190 -> 0x200` |
| 0x210 (CVector object) | - | prev_linear_velocity_obj | vec3 | m/s | data at `0x220`: previous-step linear velocity | inferred | `Vf27x` copy `0x1c0 -> 0x220` |
| 0x230 | - | accel_linear_est | vec3 | m/s^2 | `(v - v_prev) / dt` (see suspect 5a) | inferred | `Vf27x` |
| 0x240 | - | accel_angular_est | vec3 | rad/s^2 | `(w - w_prev) / dt` | inferred | `Vf27x` |
| 0x250 | - | accel_linear_filt | vec3 | m/s^2 | `0.2*old + 0.8*est` | inferred | `Vf27x` |
| 0x260 | - | accel_angular_filt | vec3 | rad/s^2 | `0.2*old + 0.8*est` | inferred | `Vf27x` |
| 0x270 (float) | - | sleep_timer | f32 | s | time accumulated below the sleep thresholds; reset to 0 when not | inferred | `Vf24x` (`this+0x270 += a`) |
| 0x274 (int) | - | sleep_armed | i32 | bool | non-zero: may sleep when the timer passes `SleepDelay` | inferred | `Vf24x` (`acc[1]`) |
| 0x280 | `CScratch` | scratch | struct | - | `CSolid::CScratch` (0x150 bytes); `+0x10` is the progress fraction (2c) | known (layout) | header |
| 0x290 (scratch +0x10) | `m_f10` | progress | f32 | 0..1 | fraction of the contact window already integrated; 1.0 at rest | inferred | `VfDx` (`a < 0x290`), `VfEx` (`0x290 < 1`), E47SolidScratch ctor |
| 0x2a0 | - | pose_end_quat | quat | - | end-of-window orientation used by `VfDx` | inferred | `VfDx` |
| 0x310 | `E44Shell` | - | - | - | used by `Vf25x` (`M51C8C0`) | guess | `Vf25x` |
| 0x320 | - | angular_accel | vec3 | rad/s^2 | angular acceleration added to `0x190` in `Vf27x`; `Vf25x` writes it from the torque via the inverse inertia | inferred | `Vf27x`, `Vf25x` |
| 0x330 | - | pose_end_pos | vec3 | m | end-of-window position used by `VfDx` | inferred | `VfDx` |
| 0x370 | - | force_accum | vec4 | N | force accumulator (`Accum340` adds `v * this+0xd0`) | inferred | `Accum340` |
| 0x380 (CFloat3) | - | linear_accel_in | vec3 | N (scaled to m/s^2 by `Vf25x`) | force input to linear accel: `Vf25x` scales it by `InverseMass` into `0x380` and `Vf27x` adds `0x380 * dt` to `0x1c0` | inferred | `Vf25x`, `Vf27x` |
| 0x3b4 (u32) | - | step_flags | u32 | bits | bit 0 cleared/set by `VfC8` (`dt > 0`, `dt < 0`); read by `E46_Flag3b4a/b` | inferred | `VfC8`, `E46_Flag3b4a` |
| 0x0d0 (float) | - | scale_d0 | f32 | - | multiplies `v` in `Accum340`; set by `E46_SetD0` | inferred | `Accum340`, `SetD0` |

### 2c. CState key map (LSP `PhysicsState`)

The exe key table also holds the state keys. Their offsets are inside the CState object,
which sits at `CSolid+0xe0`. Converting them to CSolid offsets gives the table in 2b:

| LSP key | key id | state offset | CSolid offset | confidence |
| --- | --- | --- | --- | --- |
| `vecParentRotation` (CVector4) | 0x04 | 0x10 | 0x0f0 (data 0x100) | known |
| `vecRelativeAngularVelocity` (CVector) | 0x28 | 0xa0 | 0x180 (data 0x190) | known |
| `vecRelativeLinearVelocity` (CVector) | 0x34 | 0xd0 | 0x1b0 (data 0x1c0) | known |
| `Progress` (CReal) | 0x24 | 0x90 | 0x110 | known (key); the float compared in `VfDx`/`VfEx` is `+0x290` (scratch), so the two may not be the same quantity (Open 2) |
| `VerySoft` (CUnsigned) | 0x78 | - | (def, see 2a) | known |

`vecWorldPosition` (from the LSP `PhysicsState` example) is not in this key table. The
position offset (`0x140`) is inferred from the signal payload and the cell lookup.

### 2d. Units and sign conventions

- Position m, velocities m/s and rad/s, accelerations m/s^2 and rad/s^2, mass 1/`InverseMass`.
- Quaternion stored at `0x100` with the derivative `q' = 0.5 * (w, 0) * q` (in `VfC8`,
  `VfEx`), applied as the first-order exponential `exp(0.5 * dt * w)`.
- Orientation normalised after each update (`E18_QuatNormalize`, `E34Quat::Norm`).

## 3. Functions

| address | current name | proposed name | what it computes | confidence | status |
| --- | --- | --- | --- | --- | --- |
| 0x52cc30 | `E47Solid::Vf27x(dt)` | body_integrate | `w += alpha*dt`, `v += F*dt` (F = `0x380`), accel estimates and filters, `VfC8(dt, flagA, flagB)`, `E43Vf6C`, `Vf24x(dt)`, emit `CSigMrRBodyState` | inferred | called (tanner-corolla); **suspect 5a** |
| 0x52c8f0 | `E47Solid::VfC8(a, b, c)` | body_pose_step | `q <- normalize(exp(0.5*a*w) q)` if `c`; `p <- p + a*v` if `b`; parent case if `this+0x44` | known (eqs from code) | verified (p2rb-beh) |
| 0x52b740 | `E47Solid::Vf24x(a)` | body_sleep_step | sleep timer: `t += a` while `|v|^2 < k1^2` and `|w|^2 < k2^2`; at `t > SleepDelay` clear `v`, `w` and call slot 37 | known (eq) | verified (p2rb-beh) |
| 0x52cb20 | `E47Solid::Vf25x` | body_force_s64 | `E43Vf64`; contact vote `Vf81x`; `BigB` if flag 8; scales `0x380` by `InverseMass`; transforms through `S30`/`MB560` | inferred | called (tanner-corolla) |
| 0x52cfd0 | `E47Solid::VfDx(a)` | body_back_step | if `a < progress`: `q <- normalize(a*q + (1-a)*q_end)`, `p <- a*p + (1-a)*p_end`, `progress <- a`; cell re-register; `S16` | inferred (direction 5b) | asm-confirmed: root link is `this+0x160`; root forced replay matches; reccmp 27.14% |
| 0x52d110 | `E47Solid::VfEx(a)` | body_advance_rest | `s = (1-progress)*a`: `q <- normalize(exp(0.5*s*w) q)`, `p <- p + s*v`, `progress <- 1`; cell re-register; `S16` | known (eq) | asm-confirmed: position uses full `s`; root forced replay matches; reccmp 51.0% |
| 0x52d280 | `E47SolidState::ctor` | - | state object init | known | called |
| 0x529250 | `E46_Accum250` (S60) | body_force_accum | `out = (1/InverseMass) * MB560(gravity copy from CDef+0x160b0, S30(this))`; `E43Vf70(out)` | inferred | verified (p2-harness) |
| 0x529340 | `E46_Accum340` | body_force_add | `this+0x370.. += v * this+0xd0`; log hook on flag 0x4000 | known (eq) | verified (p2-harness) |
| 0x5293a0 / 0x5293c0 | `E46_SetD0` / `E46_GetD0` | - | get/set `this+0xd0` | known | fixed (p2, not confirmed) |
| 0x529240 | `E46_Dispatch240` (S5C) | body_s5c | `E43Vf5C()` then S60 | inferred | fixed (p2, not confirmed) |
| 0x52b6b0 | `E47_Vf54Helper` (S54) | body_s54 | `CEntity` slot 1 on self, then scratch slot 4 | inferred | reviewed (Y22) |
| 0x52b6d0 | `E47_Vf16Helper` (S16) | body_s16 | state slot 17 twice, then `CBody::Vf40` tail | inferred | called |
| 0x52c220 | `E47Solid::BigB` | - | extra contact path on flag 8 | - | **bad** (harness) |
| 0x52b9f0 | `E47Solid::BigA` | - | contact-related big worker | - | **bad** (harness) |
| 0x52c510 | `E47Solid::Vf81x` | body_vote | contact vote used by `Vf25x` | guess | called |
| 0x51a3a0 | `CEntity::E43Vf20` (S20) | child_walk_s20 | walk the child list at `+0x48` with slot 8 | inferred | called |
| 0x519bc0 | `CEntity::E43Vf24` (S24) | - | no-op (bare `ret 4`) | known (asm) | called |
| 0x51a250 | `CEntity::E43Vf58` | child_walk_s58 | walk children with slot 22 | inferred | called |
| 0x51a7c0 | `CEntity::E43Vf98` (S98) | list480_step | self slot 0; unlink list head; slot 21 on the `+0x34` object | inferred | called |
| 0x529200 | `E46_Unknown9200` | - | naked `jmp CEntity::E43Vf20` | known (asm) | called |
| 0x52b8b0 / 0x52d640 | `NewDef` / `CSolid::CDef::Vf50` | - | factory for `Solid/<NAME>` LSP blocks | known (registry) | called |
| 0x529840 | `CSolid::CDef::CDef(p)` | - | def ctor | known | called |
| 0x4e1120 | `CBody::Vf40` | - | tail target of `E47_Vf16Helper` (E33) | inferred | not read |

Status `verified`: `Vf24x`, `VfC8`, `Accum340`, `Accum250`. The Vf27x body itself is
`called` only, so the suspect below is not yet confirmed by a harness run.

## 4. Open questions

1. **`Progress` vs `+0x290`.** The state key `Progress` (CReal at CSolid `+0x110`) and the
   scratch float `+0x290` both could be the window progress. `VfDx`/`VfEx` only read `+0x290`.
   Does anything write `+0x110`? A `guess` for now: `+0x110` is another quantity.
2. **Quaternion and velocity frame.** Is `w` (`0x190`) in the body frame or the world frame?
   `VfC8` multiplies `exp(0.5*dt*w)` on the left or right of `q` (`E15Shell::M83DD0` operand
   order not read). The car's `Vf64x` (suspect in step.md 5c) assumes `w` is body frame.
3. **Gyroscopic term.** `Vf27x` has no `w x (I w)` term; the car's `Vf64x` has one. Is the
   solid's gyroscopic term folded into `alpha` at `0x320` by `Vf25x`, or missing?
4. **Force chain.** `Accum250` (`S30`/`MB560`) and `Vf25x` (`M51C8C0`, `S30` at `0x320`) build
   `0x320` and `0x380` from the accumulator at `0x370`. The `S30`/`MB560` transform frames
   (world/body, and which matrix) are not read here.
5. **Writers of the history buffers.** `Vf27x` copies `0x190 -> 0x200` and `0x1c0 -> 0x220`
   at its end. The reads are at `0x200`/`0x220` in the asm (suspect 5a).
6. **CEntity hooks.** `E43Vf5C`, `E43Vf64`, `E43Vf6C`, `E43Vf70` (called from the force and
   integrate paths) are not read. They are the next step if the force chain is needed.
7. **Threshold keys.** `AngularThresholdVelocity` (`0x150`) and `LinearThresholdVelocity`
   (`0x170`) have no reader in the code read here. Which code uses them?
8. **`VfDx` direction.** `VfDx` blends `q` with `q_end` (`0x2a0`) as `a*q + (1-a)*q_end`.
   Whether `q_end` is the pose at progress 1 or 0 is not settled. The rest of `VfDx` assumes 1.

## 5. Suspect code

### 5a. `E47Solid::Vf27x` reads the previous-velocity history from the wrong address (verified in asm)

- C++: `src/rbr/RBR_E47_Big.cpp:603-604`: `float* p210 = (float*)(self + 0x210);` and
  `float* p1f0 = (float*)(self + 0x1f0);`, used at lines 606-613 as the previous values in
  `p230 = v - p210` and `p240 = w - p1f0` (the estimates).
- Why inconsistent: `0x1f0` and `0x210` are `NTweak::CVector` objects. Their xyz data sits at
  `+0x10` (`0x200` and `0x220`). The copies at the end of the same function (lines 632-633,
  `CopyFrom(self+0x190) -> self+0x200`, `CopyFrom(self+0x1c0) -> self+0x220`) write the
  data, so the reads at `+0x1f0`/`+0x210` pick up the object header, not the history.
- What the asm does: `leal 0x210(%esi),%eax; ... addl $0x10,%eax` (`0x52cd77`-`0x52cd84`), and
  `leal 0x1f0(%esi),%eax; ... addl $0x10,%eax` (`0x52cdbd`-`0x52cdcc`). Both read `self+0x220`
  and `self+0x200`. The current values (`0x1c0`, `0x190`) are read directly, as in the C++.
- Effect: the accel estimates `0x230`/`0x240` and the filtered accel `0x250`/`0x260` are wrong
  for every CSolid step. Whether anything reads `0x250`/`0x260` is not checked. Route to the
  E47 owner: fix the two pointers to `self + 0x200` and `self + 0x220` (asm-confirmed), then
  run the harness.

### 5b. `E46_Accum250` multiplies by mass (consistent; was flagged in step.md as suspect 2b)

- C++: `src/rbr/RBR_E46_Math.cpp:429`, `scale = 1.0f / body_def+0x16c`.
- Resolved: `0x16c` is `InverseMass` (2a, key id 0x58 x 4 = 0x160, value at +0xc). So
  `1/InverseMass` is the mass, and the accumulator multiplies by mass, which is right for a
  force. `Vf25x` then scales `0x380` by `InverseMass` to get an acceleration. The two uses are
  consistent. Step.md suspect 2b is withdrawn.

### 5c. `E47Solid::VfDx` / `VfEx` lerp direction (open, not a confirmed bug)

- C++: `src/rbr/RBR_E47_Big.cpp:648-680` (`VfDx`).
- The blend `a*q + (1-a)*q_end` is right only if `q_end` is the pose at progress 1. The
  reads from `0x2a0` and `0x330` are the open question 8. Route to the owner with the
  asm, to check the pose the window started from.

### 5d. Constant-address tests (no behaviour)

- `RBR_E47_Big.cpp:599` (Vf27x), `RBR_E47_Slots.cpp:` Vf25x: `if ((char*)this + 0x280 != 0)`,
  `vec = (char*)def + 0x120 != 0 ? ... : 0`, `((char*)this + 0x1b0 != 0) ? ... : 0`. These
  tests are always true (a non-zero object address). They are compiled-in artefacts of the
  asm's own null checks. Keep for exactness; do not read them as a condition.

### 5e. Divide by dt with no guard

- `RBR_E47_Big.cpp` Vf27x: `float inv = 1.0f / a;`. `a` is `dt` (`MaxTimeStep`, 0.007 retail).
  If a config sets `MaxTimeStep` to 0, the estimates become inf. Not a retail issue; note
  only.

## Notes for the lead

- Step.md suspect 2b (Accum250 vs Vf25x) is withdrawn; see 5b here. Step.md should also
  record `InverseMass` at `def+0x16c` (step.md 2a has the offset as unlabelled).
- Route 5a to the E47 owner first. It is a real C++ bug on the S6C path of every solid,
  and it is asm-confirmed.
- The key table `{name, 0, id, 0}` at `0x7cc3f0` (and the flag-name tables) also name the
  def and state fields for other classes. Worth extracting for CCar and CWheel next.
