# Physics step (CPhysics::StepForward 0x005273a0): physics semantics

Pass 4 of the physics semantics brief (step-level). Notes only; no source edits.
"Current" names are the C++ reconstruction's. "Proposed" names are ours.

Confidence: `known` (exe string, LSP key, or the equation is unambiguous), `inferred`
(strong argument from use or asm), `guess`. Reccmp % comes from `base/RBR.txt`; 100% is
byte-exact. Harness status comes from `notes/progress/gameplay-functions.csv`
(`verified`, `reviewed`, `fixed`, `bad`; `bad` = diverges or crashes on the harness).

Asm: `llvm-objdump` (`/opt/homebrew/opt/llvm/bin/llvm-objdump`, the PE is `coff-i386`).
The C++ `CPhysics::StepForward` (`src/rbr/RBR_E46_Physics.cpp:296`) matches the asm
structurally: every call, vtable slot and gate below was checked against 0x5273a0-0x527695.

## 1. Overview

`CPhysics` is the physics world. Its vtable slot 2 is `StepForward` (0x73f98c+8, the
image holds `0x5273a0` there). `TicPhysicsRecording` (0x47f420) calls it once per requested
substep with `p1 = 1` (`notes/gameplay-loop.md`). `p1` is the substep count inside the call.

### 1a. Time step: where dt comes from

`base = dt = CDef.MaxTimeStep` (`*(float*)(CDef + 0x1608c)`, asm `0x527431`), loaded from the
`PhysicsEngine MaxTimeStep` LSP key (`notes/formats/physics-lsp.md`). The retail value is
**0.007 s**, not 0.01 s. The 0.01 value is in the separate CHELALIC `physics.dat` copy, which
this loader does not use (`notes/gameplay-loop.md`). The harness peek of `+0x1608c` read 0.007
on every sample (2026-10-13). The collision window is `scaled = 4 * dt` (`mulss 0x72d150`,
the constant is `4.0f`, `0x72d150` in `.rdata`).

Consequence: one substep is 7 ms of simulated time. The brief's "1/100 s" does not apply to
the retail drive. `notes/physics/spring-damper.md` says "dt is 0.01 s (from the brief)"; that
line is wrong for retail (see Open 1).

### 1b. Call tree (reached functions, addresses)

```
StepForward(p1)                                  0x5273a0   verified (harness), reccmp 44.75%
  [once] pacing gate, p1 := 0/1                  (flag 0x8000, see 2c)
  [once] if state.counter == 0: S48 over list450 (first-step init)
  for step in 0 .. p1-1:                         (break at step>0 when m_unk60 set)
    state.counter++ (CUnsigned, +0x18); telemetry Telem60   0x499e60 (replay hook)
    S20(dt) over list450                         per-entity, see 1c
    Work5500(dt)                                 0x525500   reccmp 100%
        S5C over list450                         per-entity
        S64 over list450                         per-entity
        S6C(dt) and S58(dt) over list450         per-entity (integration, car hook)
    S24(dt) over list450                         per-entity
    if (SUB_STEPPING == 0) or (counter & 3) == 0:
        if CHECK_COLL (0x10):
            CollideWork(4*dt)                    0x526d20   verified (c2-harness), reccmp 69.9%
                Work6a70 sweep (L1 detect)       0x526a70   bad
                E38_004FBC20, E38_004FC500X      0x4fbc20 verified, 0x4fc500 reviewed
                per slice: Work5640(fc), E38_004FCA20, Work5760(rem), S48 over list450
                after slices: S54, S48 over list450
        if CHECK_PENETR (0x20):
            PenCheck()                           0x526cc0   called only, reccmp 44.1%
                (Telem70, profiler "CheckPenetration")
            E38_004FB7F0 (row clear), E38_004FBCA0 (hit test -> m_unk1c)
            if hit: E38_004FBC20, E38_004FC590X (solver), E38_004FCA20 (pass),
                    then S08 on solver (CCollisionSolver::Vf08, 0x4fc0b0)
        S08 on solver again (unconditional inside the contact block)
        S54 over list450, then S48 over list450
    S98 over list480
    state.sim_time (double, +0x20) += dt
  if player car (g_8ef660) != 0:
      drive(+0x4b0).Set4510()                    0x504510   once per call, not per substep
  g_1659b74++ (call counter)
```

The `(SUB_STEPPING)` and `CHECK_*` gates read `CDef+0x160cc`. Their meaning is given by the
exe's flag-name table (2c). The contact block runs on every substep unless SUB_STEPPING is set.

### 1c. What each per-entity slot is (list450 members)

The `list450` (and `list480`) members are "walls" in the registry (`CState::RegWall`,
0x5263d0). They are rigid bodies that take part in the step. Registrations seen in the C++:
`CCollisionMeshWater::Vf15CW` (E38), `CMountedRigidBody::StepBig` (E45), `CBrakeWallManager::SetWall`
(E34), `E43HostRel::WallRel` (E43), `CPhysics::E46_Notify67d0` (E46 handler path). The car
(`CCar`) and the wheels (`CWheel`, a `CSolid`) reach the lists through the handler path; the
exact registration of a car is **open** (Open 3).

The slot is virtual, so the target depends on the class. The two that are established:

| slot (offset) | CSolid target (vtable 0x73fc54) | CCar target (`RBR_E34_Car.h`) |
| --- | --- | --- |
| S20 `0x20` | `0x529200` tail-jumps to `CEntity::E43Vf20` (0x51a3a0): walk of the child list at +0x48 | `Vf20x` 0x4e72d0 (verified) |
| S24 `0x24` | `0x519bc0` `CEntity::E43Vf24`: bare `ret 4` (no-op) | `Vf24x` 0x4e58e0 (verified) |
| S48 `0x48` | `0x529dc0` (no name in our code) | (not in `CCar`'s declared slots) |
| S54 `0x54` | `0x52b6b0` `E47_Vf54Helper` (reviewed) | `Vf54x` 0x4e7f90 |
| S58 `0x58` | `0x51a250` `CEntity::E43Vf58`: child walk | `Hook(dt)` 0x4e9930 (**bad**): VCU step, drive step |
| S5C `0x5c` | `0x529240` `E46_Dispatch240`: `E43Vf5C` then S60 | `Vf5cx` 0x4e5c60 (tail jump) |
| S60 `0x60` | `0x529250` `E46_Accum250`: force accumulator (2c, 2d) | (not overridden) |
| S64 `0x64` | `0x52cb20` `E47Solid::Vf25x` (reviewed/called) | `Vf64x` 0x4e9020 (**bad**) |
| S6C `0x6c` | `0x52cc30` `E47Solid::Vf27x(dt)` (integrator, see rigid-body.md) | `Vf6cx(dt)` 0x4ea300 (verified): brake-wall, wheels, sensors |
| S98 `0x98` (list480) | `0x51a7c0` `CEntity::E43Vf98`: self slot 0, unlink list head, slot 21 on +0x34 object | `Vf98x` 0x4e6dd0 (verified) |

The S-numbers in column 1 are byte offsets of the vtable slot (index = offset / 4). They are
the names the C++ uses (`E46Call20_1::S20` etc.). Only the car and CSolid targets are listed
because they are the ones the reconstruction has. Other classes (E34 brake wall, E45 mounted,
E38 water mesh) are not checked here.

Work5500's order matters: all `S5C` first, then all `S64`, then all `S6C`+`S58` (asm
`0x5274ca` calls `0x525500`; the loops sit inside `0x525500`-`0x52558e`).

### 1d. Stage summary (subsystem level)

| stage | reads | writes | notes |
| --- | --- | --- | --- |
| pacing gate | CDef flags, CPhysics m_unk5c/m_unk60 | p1, m_unk5c | single-step mode (open 4) |
| S20 / S24 | body state, child list | child state (S20 only) | CSolid: child walk, S24 nop |
| Work5500 S5C | body, CDef gravity (+0x160a0) | body force accumulator (via S60) | E46_Accum250 scales by mass, 1/InverseMass (rigid-body.md 5b, consistent) |
| Work5500 S64 | body inverse inertia, body+0x16c | body torque/force (CCar: S74 gyroscopic torque) | CCar::Vf64x bad (suspect 2c) |
| Work5500 S6C | body velocities, accelerations | velocities (+0x190, +0x1c0), filtered accel, emitted signal | CSolid: Vf27x. CCar: wheels and brake walls |
| Work5500 S58 | car state | VCU scratch, drive state | CCar::Hook bad (open 6) |
| S24 | none | none | body nop on CSolid |
| CollideWork | contact list (scratch+0x14), solver (scratch+0x10) | solver events, body state (VfDx/VfEx) | contact detection over the window 4*dt, once per call (2e, contacts.md) |
| PenCheck + solver | body geometry, contact list | solver events, m_unk1c (hit) | penetration correction |
| S54 / S48 | body | body state | CSolid: Vf54 helper |
| S98 | list480 nodes | nodes | CSolid: E43Vf98 (self slot 0, unlink) |
| sim time | dt | state.sim_time (+0x20) | double, seeded 43185.0 (E46 header) |
| drive Set4510 | player car drive | drive | once per call; owner of drive+0x10 unresolved (open 5) |

## 2. Data

### 2a. CDef (`CPhysics::CDef`, 0x16160 bytes; `g_1659b70 + 0x24` points here)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x16080 (+0xc = 0x1608c) | `m_unk16080` (CReal) | max_time_step | f32 | s | fixed physics substep dt; `base` in StepForward | known (LSP `MaxTimeStep`), value 0.007 | `RBR_E46_Physics.cpp:323`; `physics-lsp.md`; harness peek (gameplay-loop notes) |
| 0x160cc | `m_flags.value` (CFlags, `+0xc`) | physics_flags | u32 | bits | step gates (2c) | known (bit names from the exe table) | `RBR_E45_PhysA.cpp:217`, table `0x7cbc10` |
| 0x160e0 | `m_dbg.value` (CDbgFlags, `+0xc`) | dbg_flags | u32 | bits | debug render flags | known (names) | `CollideWork` `0x20000`; table `0x7cbcb0` |
| 0x160f4 | `m_log.value` (CLogFlags, `+0xc`) | log_flags | u32 | bits | logging flags | known (names) | `E46_Accum250`, `0x4000`; table `0x7cbd90` |
| 0x160a0 (CVector, 0x20) | `m_unk160A0` | gravity (proposed) | vec | m/s^2 | the copy at +0x160b0 is applied in S60 | guess (vecGravity inferred) | `RBR_E46_Math.cpp:422`; LSP `vecGravity` |
| 0x16024 | `m_solver` (CCollisionSolver::CDef, 0x4c) | solver_def | struct | - | solver tuning (`CCollisionSolver` keys) | inferred | `RBR_E46_CDef.cpp:39` |

Other CDef fields (`0x16070`, `0x16090`, `0x16100`-`0x16150`) are not used on the step path,
so they are not named here.

### 2b. CState (`CPhysics::CState`, 0x4b8 bytes; `CPhysics +0x20`)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x18 (CUnsigned `m_u0c`, value at +0xc) | `m_u0c.m_value` | substep_count | u32 | count | incremented once per substep (before S20); 0 means first step | known (code) | asm `0x52746f`-`0x52748b` |
| 0x20 | `m_d20` | sim_time | f64 | s | `+= dt` per substep; seeded 43185.0 | inferred (accumulates dt); seed known from `RBR_E46_Physics.h` | asm `0x527631` |
| 0x450 | `m_list450` (E34Drain) | bodies_contact | list | - | walls with `S80 != 0` (RegWall) | inferred | `RBR_E46_Detectors.cpp:45` |
| 0x460 | `m_list460` | bodies_overflow_a | list | - | walls with `S80 == 0`, `S88 == 0` | inferred | same |
| 0x470 | `m_list470` | bodies_overflow_b | list | - | walls with `S80 == 0`, `S88 != 0` | inferred | same |
| 0x480 | `m_list480` | bodies_late | list | - | walls with `S8C != 0`; S98 pass | inferred | same |

Note: `RegWall` tests `S8C` first, then `S80`. For CSolid, `S8C` is `E46_Get1ec` (`+0x1ec`).
Its value decides the list. This is a layout question (Open 3).

### 2c. CPhysics (`CPhysics`, 0xf8 bytes)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x1c | `m_unk1c` | contact_hit | i32 | bool | result of `E38_004FBCA0` in PenCheck | inferred | asm `0x52758f` |
| 0x18 | `m_unk18` | detect_passes | i32 | count | reset and counted in CollideWork | inferred | `RBR_E46_Small.cpp:248` |
| 0x4, 0x8, 0xc | `m_unk4/8/0xc` | solver_stats_* | i32 | count | written only under dbg flag 0x20000 | known (gate) | `RBR_E46_Small.cpp:249`-`250`, `271`, `275`-`277` |
| 0x5c | `m_unk5c` | single_step_delay | i32 | steps | countdown under the single-step gate | inferred | asm `0x5273ee`-`0x527401` |
| 0x60 | `m_unk60` | single_step_pending | i32 | bool | set means one-step mode | inferred | asm `0x5273e9` |
| 0x20 / 0x24 / 0x28 | `m_state` / `m_def` / `m_scratch` | - | ptr | - | state, def, scratch objects | known | ctor |

Flag bits (the exe's own names, `CFlags` table `0x7cbc10`, `GetBitName` 0x523b90):

| bit | value | name (exe) | where used on the step path |
| --- | --- | --- | --- |
| 4 | 0x10 | `CHECK_COLL` | `CollideWork` (profiler "CPhysics - DetectCollisions") |
| 5 | 0x20 | `CHECK_PENETR` | `PenCheck` and the E38 solver block (profiler "CPhysics - CheckPenetration") |
| 12 | 0x1000 | `SUB_STEPPING` | contact block only when `counter & 3 == 0` |
| 15 | 0x8000 | `ENABLE_SINGLESTEPPING` | pacing gate on p1 (m_unk5c / m_unk60) |
| 17 | 0x20000 (dbg) | `RENDER_COLLSOLVER_INFO` | CollideWork counters |
| 14 | 0x4000 (log) | `WHEEL_FORCES` | `E46_Accum250`, `E46_Accum340` log hook |

The names are the exe's own. The mapping from name to use is `inferred` from the profiler
labels and the single-step/substep structure. The bit values in the retail file are
**not** checked here (Open 2).

### 2d. Force accumulator (`E46_Accum250`, S60 on CSolid)

Computes `out = MB560(copy of CDef+0x160b0 (4 floats), S30(self), out) / k`, where
`k = body_def+0x16c` = `InverseMass` (rigid-body.md 2a: key id 0x58, CReal at 0x160). Dividing by
`k` multiplies by mass, so `out` is a force. Then the result goes to `E43Vf70(out)`.

## 3. Functions

| address | current name | proposed name | what it computes | confidence | evidence and status |
| --- | --- | --- | --- | --- | --- |
| 0x5273a0 | `CPhysics::StepForward` | step_world | the substep loop in 1b | known (structure) | verified (harness); reccmp 44.75% |
| 0x525500 | `Work5500` | step_body_phase | S5C loop, S64 loop, S6C(dt) + S58(dt) loop | inferred | reccmp 100% |
| 0x525640 | `Work5640` | advance_contact_bodies | for each contact pair, bodies with flag 0x4 get VfDx(Get94) | inferred | verified (lead harness fix); reccmp 100%. **Arg unused** (suspect 2a) |
| 0x525760 | `Work5760(p)` | advance_contact_bodies_rem | VfEx(p) on bodies with flag 0x4 for contacts with Get94 < 1 | inferred | reccmp 100%, called only |
| 0x525590 | `Work5590(f)` | damp_contacts | S6C(-f) on contact bodies | inferred | reccmp 100%, not on the step path |
| 0x526a70 | `Work6a70(p)` | detect_L1 | L1 detector S04 over list450 (E43 Walk1F0) | inferred | **bad** (harness) |
| 0x526d20 | `CollideWork(f)` | collide_window | contact detection over window f (4*dt), run once per call (asm 0x526d90-0x526da8; the earlier "slices of f*0.05" reading was wrong, see contacts.md) | known | verified; reccmp 69.9%. See 2e |
| 0x526cc0 | `PenCheck()` | penetration_check | L1 penetration detector (slot 1) over list450 | inferred | called only; reccmp 44.1% |
| 0x5263d0 | `CState::RegWall` | register_wall | route wall to list480/450/460/470 by S8C/S80/S88 | known (code) | called (tanner-corolla); reccmp 63.6% |
| 0x528880 | `E46ProfChan::Enter` | prof_enter | profiler sample push | known | 78% |
| 0x528690 | `ProfExit` | prof_exit | profiler sample pop | known | 100% |
| 0x499e60 / e70 / e90 | `E21Telem::Telem60/70/90` | telem_* | replay telemetry hooks (not physics) | known | 100% |
| 0x52f450 | `CBase::SetDirty` | - | dirty flag for NTweak values | known | 100% |
| 0x504510 | `CDrive::Set4510` | drive_step | naked forward to drive+0x10 slot 1 | inferred | 100%, owner open |
| 0x4fb7f0 | `E38_004FB7F0` | solver_clear_rows | clears solver rows | inferred | verified; 100% |
| 0x4fbca0 | `E38_004FBCA0` | solver_count_hits | returns hit count (drives m_unk1c) | inferred | 100% |
| 0x4fbc20 | `E38_004FBC20` | solver_sweep_events | E37 refresh and flags sweep over event list | inferred | verified (Y20); 74% |
| 0x4fc500 | `E38_004FC500X` | solver_min_event_key | min of event keys (time-of-impact fraction) | inferred | reviewed; 64% |
| 0x4fc590 | `E38_004FC590X` | solver_step | solver step (B890/BD30/BFC0 callees) | inferred | verified (12084 steps); 17% |
| 0x4fca20 | `E38_004FCA20` | solver_pass | pass over the event list; slots 0x80/0x8c/0x90 gated by 0x13c | inferred | verified (p2w); 31% |
| 0x4fc0b0 | `CCollisionSolver::Vf08` | solver_post | S08 target; run after each block | guess | not checked |
| 0x4f8280 | `E37GridOps::EdgeEdge` | - | edge-edge contact kernel | inferred | verified; 38% |
| 0x525bd0 | `E46_Kernel5bd0` | - | kernel from CollideWork | inferred | verified; 36% |
| 0x529200 | `E46_Unknown9200` | - | naked `jmp CEntity::E43Vf20` | known (asm) | slot S20 on CSolid |
| 0x51a3a0 | `CEntity::E43Vf20` | child_walk_s20 | walk +0x48 children with slot 8 | inferred | called (tanner-corolla) |
| 0x529240 | `E46_Dispatch240` | body_accum_s5c | `E43Vf5C` then S60 | inferred | 50% (C++ comment) |
| 0x529250 | `E46_Accum250` | body_force_accum | 2d | inferred | verified (p2-harness) |
| 0x52cb20 | `E47Solid::Vf25x` | body_torque_s64 | E43Vf64, Vf81x vote, BigB if flag 8, scale by k | inferred | called (tanner-corolla) |
| 0x52cc30 | `E47Solid::Vf27x(dt)` | body_integrate | velocities += accel*dt; estimates; emits CSigMrRBodyState | inferred | called; rigid-body.md |
| 0x4e9930 | `CCar::Hook(dt)` | car_vcu_step | VCU step (0x850), drive Step1E0, E43Vf58, Damp66D0, Hook1FF0 | inferred | **bad** |
| 0x4ea300 | `CCar::Vf6cx(dt)` | car_step | brake walls, wheel loops, sensor/MrR emits | inferred | verified |
| 0x4e9020 | `CCar::Vf64x` | car_gyro_s64 | `S74(cross(w, -I w))`, 0.7/0.3 blend into 0x15d0 | inferred | **bad** |
| 0x4e72d0 | `CCar::Vf20x` | car_s20 | S20 target for cars | guess | verified |
| 0x4e58e0 | `CCar::Vf24x` | car_s24 | S24 target for cars | guess | verified |

Status notes: `verified` = harness identical; `reviewed` = read, no difference; `bad` = diverges
or crashes on the harness.

### 3a. Verification of the step path

- Byte-exact (reccmp 100%): `Work5500`, `Work5640`, `Work5590`, `Work56C0`, `Work5760`,
  `Set4510`, `SetDirty`, `Telem60/70/90`, `ProfExit`, `E38_004FBCA0`, `E38_004FB7F0`.
- Harness-verified, not byte-exact: `StepForward` (44.75%), `CollideWork` (69.9%),
  `E38_004FC590X` (17.5%), `E38_004FCA20` (30.8%), `E38_004FBC20` (74%).
- On the step path and **bad** on the harness: `CCar::Hook` (`0x4e9930`),
  `CCar::Vf64x` (`0x4e9020`), `Work6a70` (`0x526a70`), `E47Solid::BigB` (`0x52c220`, called from
  `Vf25x` when body flag 8 is set), `E47Solid::BigA` (`0x52b9f0`, not on the step path yet).
  These are the first places to look when a step diverges.

## 4. Open questions

1. **dt.** The spring-damper note and the brief say 0.01 s. The retail value is 0.007 s
   (`physics.rbz`, peek-confirmed). The brief's text should say `MaxTimeStep` (0.007 retail).
   Is any other code path using a hardcoded 0.01? Not checked here.
2. **Flag values in retail.** Which bits are set in `CDef+0x160cc`
   (`SUB_STEPPING`, `ENABLE_SINGLESTEPPING`, `CHECK_COLL`, `CHECK_PENETR`)? The LSP example
   `+SUB_STEPPING -SETI` suggests SUB_STEPPING is on. A peek of `+0x160cc` would settle it.
3. **Who is in list450 and list480.** Cars and wheels reach the lists through a path I did
   not find (`E46_Notify67d0` only handles connections). Also: `CSolid::S8C` returns
   `+0x1ec` and `S80` returns `+0x1dc`. Which fields are set for a wheel decides the list.
4. **Single-step pacing.** The meanings of `m_unk5c` (`SingleStepperDelay` exists as a
   name in the exe table at `0x73f3d0`) and `m_unk60` are only inferred. Where are they
   written?
5. **Drive `Set4510`.** `CDrive::Set4510` forwards to `drive+0x10`'s slot 1. The owner
   of `drive+0x10` is not identified. It runs once per StepForward call. Which object is
   the per-substep drive update for AI cars?
6. **CCar::Hook.** Its VCU step (`CScratchSim::Step(dt)`) runs per substep through S58, but
   it is `bad` on the harness. If the bad status is real, every car's VCU and drive are off
   from the first step.
7. **Gravity vector.** `CDef+0x160a0` is copied by `E46_Accum250` (+0x160b0, 4 floats).
   Confirm it is `vecGravity` and in which layout (vec3 or vec4).
8. **Work5640's argument.** The original never reads its float argument (asm `0x525640`-`0x5256c0`
   has no stack read). The caller passes `fc`. Is the parameter a leftover from an
   inlined version?

## 5. Suspect code

Each item is a place where our C++ uses a field or argument in a way the asm or the meaning
does not support. Items marked `(path)` sit on the step path and are `bad` or unverified.

### 5a. Stage-level

| address | C++ (file:line) | why inconsistent | what the asm does |
| --- | --- | --- | --- |
| 0x525640 | `RBR_E46_Physics.cpp:221`, `Work5640(float f)` | parameter `f` is ignored (`(void)f;`); the caller `CollideWork` passes `fc` (time fraction) | never reads the stack argument; each contact sample uses `Get94()` instead |
| 0x526d20 | `RBR_E46_Small.cpp:279`, `f7 = f6 - f8 / loc20` | `f8 = f * 0.0f` is always 0 (or NaN for inf/NaN f), so the term is a no-op | the original has the same term; keep for exactness, document only |
| 0x525500 | `RBR_E46_Physics.cpp:185`, `for (tail...) {}` | a loop with no body over list450 | the asm has the same walk (`0x525577`, next-pointer load, no call); no behaviour, kept for exactness |

### 5b. Body accumulator: withdrawn

Earlier draft flagged `E46_Accum250` (`RBR_E46_Math.cpp:429`) for dividing by `body_def+0x16c`
and `Vf25x` for multiplying by it. `0x16c` is `InverseMass` (rigid-body.md 2a), so both uses
are right: the accumulator makes a force from mass, and `Vf25x` makes an acceleration from
force. Not a bug.

### 5c. Car torque and blend (path, bad)

| address | C++ (file:line) | why inconsistent | what the asm does |
| --- | --- | --- | --- |
| 0x4e9020 | `RBR_E34_CarD.cpp:1357`-`1358`, blend `0x15d0 += f321[2] * 0.3`, `0x15d4 += f321[3] * 0.3` | the S30 output is `f321+3` (4 floats, call at `RBR_E34_CarD.cpp:1351`); the blend reads `f321[2..5]`, one float before that block. `f321[2]` is also the pre-call `z` term | the asm blends the four floats written by the S30 call (stack `0x40(%esp)`), so the C++ lane indexing looks shifted by one. Stack layout check needed; the function is `bad` |
| 0x4e9020 | `RBR_E34_CarD.cpp:1343`-`1346`, `-1.0f / inertia * L` | `vecLocalInverseInertiaDiagonal` is an inverse, so `-1/invI` is `-I`. The gyroscopic term `w x (-I w)` is right if `loc20` is `w` (body angular velocity), not `L` | consistent with an Euler term if `loc20` is `w`. The field name at `+0x190` is open |

### 5d. Step-path state written where the meaning is debug-only

| address | C++ (file:line) | why inconsistent | what the asm does |
| --- | --- | --- | --- |
| 0x526d20 | `RBR_E46_Small.cpp:249`-`250`, `271`, `275`-`277`, `this+4/+8/+0xc` writes | only written under dbg flag 0x20000 (`RENDER_COLLSOLVER_INFO`); they look like state but are render counters | matches the asm (gated); they are not physics state. Keep them out of any Rust port |

## Notes for the lead

- `notes/physics/spring-damper.md` says dt is 0.01 s. Retail is 0.007 s (Open 1). That
  affects its `v_new` equation only through the `dt` value, not its structure.
- Item 5c needs an owner before the car notes are finalised.
- rigid-body.md 5a (Vf27x reads the velocity history from `+0x1f0`/`+0x210`, the asm reads `+0x200`/`+0x220`) is a confirmed C++ bug on the S6C path of every solid. Route it to the E47 owner.
