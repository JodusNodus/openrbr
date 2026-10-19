# Physics core (E46: CPhysics, CDef, CState, detectors, notify, math helpers): physics semantics

Pass 4 of the physics semantics brief, E46 part (`lead/briefs/physics-semantics.md`). Notes only;
no source edits. Companion: `notes/physics/solid.md` (E47 body: `Accum250`/`Accum340` are listed
there because they write the body's force accumulator), `notes/physics/step.md` (the step
loop and the list of E46 gates), `notes/physics/tweak-keys.md` (field names).

Confidence: `known` (exe or LSP key name, or the equation is unambiguous in the asm), `inferred`
(strong argument from use or asm), `guess`. Reccmp % from `base/RBR.txt`, status from
`notes/progress/gameplay-functions.csv`. Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump -d` on the
retail exe (coff-i386, VA addresses). Slot numbers are the vtable offsets (slot = offset / 4).

## 1. Overview

`CPhysics` is the physics world. It owns the state (`CState`, lists of bodies), the static
definition (`CDef`, tweak keys from the `Physics` LSP block) and a scratch object (`CScratch`:
the collision solver and the handler stack). Its vtable slot 2 is `StepForward` (`step.md`).
Each substep it runs, in order: per-body force and integration (`Work5500`, see `solid.md`),
contact detection over the window `4*dt` (`CollideWork`), penetration check (`PenCheck`), the
solver (E38), and the list 480 pass. `step.md` 1b has the full tree.

The E46 code in this note falls into five groups:

1. **Entry and state**: `CPhysics::CPhysics` (0x527c60), `CState::CState` (0x5270c0), `CDef::CDef`
   (0x524c70), `CScratch::CScratch` (0x524e80), `CDef::Vf50/Vf54/Vf4c` (0x5278d0, 0x526760,
   0x5284f0), `E46_CreateDef` (0x526660), `E46_InitPhysics` (0x5266d0), `E46_AppInit` (0x526fd0).
2. **Wall registry**: `CState::RegWall` (0x5263d0), `CPhysics::RegWall` (0x526470), the list
   visitors `Visit450` (0x526360), `Visit470` (0x526390) and `Work6a70` (0x526a70).
3. **Detectors**: the L1 and L2 penetration/collision detectors (`E46L1PenDetector::Vf04`
   0x525a20, `E46L2Detector::Vf04` 0x5257e0, `CL1CollisionDetector::Vf04` 0x526ad0),
   `E46_Kernel5bd0` (0x525bd0), `PenCheck` (0x526cc0), `CollideWork` (0x526d20).
4. **Connections (notify)**: `E46_Notify67d0` (0x5267d0) creates a handler for a contact
   (through `E39_EntfCreate`), registers it, and pushes or pops it on a stack in the scratch.
   `E46_Dispatch69e0` (0x5269e0) walks a table of these records.
5. **Math helpers**: `E46Vec` (4 floats: `Add`, `Sub`, `CrossAcc`, `CrossCopy`, `CopyNeg`),
   `E46Mat` (10 floats: `Mul`, `ScaleRows`, `XformRows`, `MulVec`), `E46_IdxWrite` (0x528980)
   (table search with a fraction), `E46_Accum250/340` (0x529250, 0x529340, body side, `solid.md`).

Inputs from other subsystems: the bodies (`CSolid` lists, `solid.md`), the grid (`E37GridOps`),
the E38 solver, the E43 entity walk (`E43_Walk1F0`), the E45 and E36 detectors, the E39 entity
factory, the LSP tweak values (`CDef`). Outputs: the contact events and the handler list.

### 1a. Call tree (reached E46 functions)

```
StepForward (0x5273a0, 44.75%, verified)
  Work5500 (0x525500, 100%)                 bodies: S5C, S64, S6C/S58
  CollideWork(4*dt) (0x526d20, 69.9%, verified)
    Work6a70 (0x526a70, 59%, bad)           L1 sweep: S04 + E43 walk over list450
    CL1CollisionDetector::Vf04 (0x526ad0, 37%)   per hit: grid sweep, E45_M40C0, NotifyFiltered, Visit470
    E46L1PenDetector::Vf04 (0x525a20, 44%, bad)  (caller not checked in this pass)
    E46L2Detector::Vf04 (0x5257e0, 74.7%, verified)
    E46_Kernel5bd0 (0x525bd0, 35.7%, verified)
  PenCheck (0x526cc0, 44%)                  det over list450, walk per node (5c)
  E46_Notify67d0 (0x5267d0, 40.6%)          per contact: create handler, RegWall, callbacks
    CState::RegWall (0x5263d0, 63.6%, reviewed)
  Visit470 (0x526390, 100%, bad at runtime) list 470 visitor
E46_CreateDef / E46_InitPhysics / E46_AppInit (0x526660, 0x5266d0, 0x526fd0)  setup, not per step
CPhysics::CPhysics (0x527c60, 39%), CState ctor (0x5270c0, 61.5%), CDef ctor (0x524c70, 62%)
```

Unreached: `Visit450` (0x526360), `E46_Dispatch69e0` (0x5269e0), `E46_Energy3f0` (0x5293f0),
`CPhysics::Vf10` (0x524f80, reached but verified only). See 5 for the first two.

## 2. Data

### 2a. CPhysics::CDef (`CPhysics + 0x24`, 0x16160 bytes)

Keys and offsets from the key table (`tweak-keys.md`, `CPhysics::CDef`): the value is at the
record offset + 0xc.

| record / value | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x16080 / 0x1608c | `m_unk16080` | max_time_step | f32 | s | substep dt (`base` in `StepForward`): 0.007 s retail | known (key `MaxTimeStep`) | tweak-keys; `step.md` 1a; peek |
| 0x16090 / 0x1609c | `m_unk16090` | min_time_delta | f32 | s | key `MinTimeDelta`; reader not found in E46 | known (name), meaning guess | tweak-keys |
| 0x16070 / 0x1607c | `m_unk16070` | max_expirable_count | u32 | count | key `MaxExpirableCount`; reader not found in E46 | known (name) | tweak-keys (exe-only key) |
| 0x160a0 / 0x160b0 | `m_unk160A0` (CVector) | gravity | vec3 | m/s^2 | key `vecGravity`; copied into the body accumulator by `Accum250` | known (key), used as gravity by `Accum250` | tweak-keys; `solid.md` 3 |
| 0x160c0 / (0x160cc) | `m_flags` (CBitField) | physics_flags | u32 | bits | step gates (`step.md` 2c) | known (names) | `Flags` key |
| 0x160d4 / (0x160e0) | `m_dbg` | dbg_flags | u32 | bits | render and counters (dbg `0x20000`) | known (names) | `DbgFlags` key |
| 0x160e8 / (0x160f4) | `m_log` | log_flags | u32 | bits | log hooks (`0x4000` in `Accum340`, `0x100` in `Notify67d0`) | known (names) | `LogFlags` key; asm `0x529388`, `0x526897` |
| 0x16100 / 0x1610c | `m_unk16100` | brake_wall_buffer | f32 | m | key `BrakeWallBuffer` | known (name) | tweak-keys |
| 0x16110 / 0x1611c | `m_unk16110` | brake_wall_factor | f32 | - | key `BrakeWallFactor` | known (name) | tweak-keys |
| 0x16120 / 0x1612c | `m_unk16120` | brake_wall_damping | f32 | - | key `BrakeWallDamping` | known (name) | tweak-keys |
| 0x16130 / 0x1613c | `m_unk16130` | damage_zone | u32 | - | key `DamageZone`; exe-only | known (name) | tweak-keys |
| 0x16140 / 0x1614c | `m_unk16140` | force_material | u32 | index | key `ForceMaterial` | known (name) | tweak-keys |
| 0x16150 / 0x1615c | `m_unk16150` | single_stepper_delay | u32 | steps | key `SingleStepperDelay` | known (name) | tweak-keys; `step.md` open 4 |
| 0x16024 | `m_solver` (CCollisionSolver::CDef, 0x4c) | solver_def | struct | - | key `CollisionSolver` | known (key) | tweak-keys |

### 2b. CPhysics::CState (`CPhysics + 0x20`, 0x4b8 bytes)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x0c (CUnsigned `m_u0c`, value at 0x18) | `m_u0c.m_value` | substep_count | u32 | count | incremented per substep; 0 on the first | known (code) | `step.md` 2b; asm `0x52746f` |
| 0x20 | `m_d20` | sim_time | f64 | s | `+= dt` per substep; seeded 43185.0 | inferred (accumulates dt) | `step.md` 2b |
| 0x28 | `m_queue` (CObjIdQueue, key `ObjIdQueue`) | obj_id_queue | struct | - | queue of object ids (runtime) | known (key) | tweak-keys (CPhysics::CState) |
| 0x450 | `m_list450` | bodies_contact | list | - | walls with `S80 != 0` (not `S8C`) | inferred | `RegWall` |
| 0x460 | `m_list460` | bodies_overflow_a | list | - | walls with `S80 == 0`, `S88 == 0` | inferred | `RegWall` |
| 0x470 | `m_list470` | bodies_overflow_b | list | - | walls with `S80 == 0`, `S88 != 0` | inferred | `RegWall`, `Visit470` |
| 0x480 | `m_list480` | bodies_late | list | - | walls with `S8C != 0`; the S98 pass | inferred | `RegWall` |

`E34Drain` list nodes: `+0x10` next, `+0x14` previous (tail), `+0x18` owning list, `+0x60` flags
(bit 0x40 skips a node in `Visit470`). `RegWall` sets these.

### 2c. CPhysics (`this`, 0xf8 bytes)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x04 / 0x08 / 0x0c | `m_unk4/8/0xc` | solver_stats_* | i32 | count | zeroed at the start of `CollideWork`; written only under dbg `0x20000` | known (gate), inferred (meaning) | `Small.cpp:249`-`250`, `271`-`277`; `step.md` 2c |
| 0x18 | `m_unk18` | detect_passes | i32 | count | reset and counted in `CollideWork` | inferred | `Small.cpp:248` |
| 0x1c | `m_unk1c` | contact_hit | i32 | bool | result of `E38_004FBCA0` in `PenCheck` | inferred | asm `0x52758f` |
| 0x20 / 0x24 / 0x28 | `m_state` / `m_def` / `m_scratch` | - | ptr | - | state, def, scratch | known | ctor |
| 0x5c | `m_unk5c` | single_step_delay | i32 | steps | countdown under the single-step gate | inferred | `step.md` 2c |
| 0x60 | `m_unk60` | single_step_pending | i32 | bool | one-step mode | inferred | `step.md` 2c |

### 2d. CScratch (`CPhysics + 0x28`, at least 0xe890 bytes)

| offset | current C++ name | proposed name | type | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x10 | `m_solver` (CCollisionSolver) | solver | struct | the E38 solver; `PenCheck` and `Work6a70` use `+0x10` as the progress word (`det[2]`) | known (layout), inferred (progress) | `CScratch` ctor (`p2rb` fix); asm `0x526cd`, `0x526a7x` |
| 0xe848 + 4*i | `scratch+0xe848` (array) | handler_stack[i] | ptr | the connection handlers in use; top at `count` | inferred | `Notify67d0` asm `0x526848`-`0x526880`, `0x526995`-`0x5269a5` |
| 0xe888 | `scratch+0xe888` (int) | handler_stack_top | i32 | index of the top handler; -1 means empty | inferred | `Notify67d0`, `Dispatch69e0` |

### 2e. Math types

| type | layout | operations |
| --- | --- | --- |
| `E46Vec` | `float m_v[4]` (xyz + 1 lane) | `Add`/`Sub` (`0x528b00`, `0x528b90`, lane-wise: scalar in C++, packed in asm: score 3%), `CrossAcc`(`0x528e30`), `CrossCopy` (`0x528ea0`), `CopyNeg` (`0x528f30`, `out.w = -v`) |
| `E46Mat` | `float m_m[10]` (3x3 row-major + 1 extra, only `m[9]` beyond the 3x3) | `Mul` (`0x528c20`), `ScaleRows` (`0x528dd0`), `XformRows` (`0x528fa0`), `MulVec` (`0x529060`) |

## 3. Functions

Status: `verified` = harness identical; `reviewed` = read, no difference; `bad` = diverges or
crashes; `-` = no status. "asm" = compared with the asm in this pass.

| address | current name | proposed name | what it computes | confidence | status |
| --- | --- | --- | --- | --- | --- |
| 0x5273a0 | `CPhysics::StepForward(p1)` | step_world | substep loop (`step.md` 1b) | known (structure) | verified; 44.75% (see `step.md`) |
| 0x526d20 | `CPhysics::CollideWork(f)` | collide_window | contact detection over `4*dt` once per call (`step.md` 2e); the `f8 = f*0` term is a no-op (5h) | known | verified; 69.9% |
| 0x526cc0 | `CPhysics::PenCheck()` | penetration_check | `det = {0x73f5b4, 1.0f, scratch+0x10, state}`; for each node of `list450`: `det.S04(node)`, `node.E43_Walk1F0(det)` (see 5a) | inferred | asm checked (walk on the node); 44.07%, called only |
| 0x526a70 | `CPhysics::Work6a70(p)` | detect_l1 | `det` = a `CL1CollisionDetector` (vtable 0x73f7b0, 0x0c bytes: `m_f04 = p` (float), `m_unk08 = scratch+0x10`, 5b); for each node of `list450`: `det.S04(node)`, `node.E43_Walk1F0(det)` | inferred | asm checked; **bad** (harness). 5b |
| 0x526360 | `CPhysics::Visit450(visitor)` | visit_450 | for each node of `list450`: `visitor.S04(node)`, then `E43_Walk1F0(visitor)` on the **node** (asm) | inferred | not reached; **the C++ walks `this` (5a)** |
| 0x526390 | `CPhysics::Visit470(visitor)` | visit_470 | for each node of `list470` without flag 0x40: `visitor.S04(node)`, `node.E43_Walk1F0(visitor)` | inferred | asm checked, equal; reached, **bad** at runtime (cause outside this function) |
| 0x5263d0 | `CState::RegWall(wall)` | register_wall | `S8C != 0` -> list480; else `S80 != 0` -> list450; else overflow lists 460 (`S88 == 0`) or 470 (`S88 != 0`) via `PushItem`; append to the chosen list | known (code) | reviewed; 63.55% |
| 0x526470 | `CPhysics::RegWall(wall)` | - | forwards to `m_state->RegWall` | known | not drive-reached; p2rb: 40.00 -> 100.00 |
| 0x5267d0 | `CPhysics::E46_Notify67d0(other)` | notify_contact | see 3a | inferred | asm-spot-checked (call order and slot numbers match); 40.62%, not harness-run |
| 0x5269e0 | `CPhysics::E46_Dispatch69e0(self)` | notify_table | loop: `tag = self.+0x38`; while `tag != -1`: `Notify67d0(self)`, then `self += 0x50` (asm: `tag` from `+0x88` is read before the add) | inferred | not reached; C++ order is wrong (5d) |
| 0x525a20 | `E46L1PenDetector::Vf04(other)` | pen_l1_dispatch | stack detector `detBuf` (`CL2PenetrationDetector::Ctor`) over the pair; `E45_M40C0`; `NotifyFiltered` for hits; then for each node of `list470`: a gig object `{0x73f5d0, other, ..., this+4, this+8}`, `S04(node)`, `other.E43_Walk1F0(gig)` | inferred | asm checked (walk on `other`, consistent); **bad** (harness). 5e |
| 0x526ad0 | `CL1CollisionDetector::Vf04(other)` | collision_l1_dispatch | `s118 = other.S118()`; return if `s118 && E37Get67C(s118)` or `other.S10C() == 0`; two grid sweeps (`E37GridOps::M6610`) with `S34`/`S30` of `other`; `other.S120(dd0, dc0)`; detector `det` (0x73f5a8); `E45_M40C0(dd0, dc0, hits)`; `NotifyFiltered` for hits; `Visit470(det)`; clears | inferred | asm checked (order matches); 37.07%. 5f |
| 0x5257e0 | `E46L2Detector::Vf04(other)` | collision_l2_dispatch | (not read in this pass) | - | verified; 74.67% |
| 0x525bd0 | `E46_Kernel5bd0` | pair_sweep | broadphase: gathers grid hits between the sum/diff boxes, filters by mask, then ray casts (C++ comment) | inferred | verified; 35.71%; not read in asm |
| 0x528980 | `E46_IdxWrite(table, value, count, out)` | table_index_frac | `out = {lo, hi, frac}`: index of `value` in the sorted table `t` with `frac = (value - t[lo]) / (t[lo+1] - t[lo])`; clamps: below -> `(0, 1, 0)`; above -> `(n-2, n-1, 1)` | known (eq) | verified; 17.24%; not read in asm. Walks from the hint `out[0]` |
| 0x528b00 / 0x528b90 | `E46Vec::Add/Sub(dst, n)` | - | per 4-float element: `dst += m_v` / `dst -= m_v` | known (eq) | verified; 3% (scalar vs packed, score only; `RBR_E46_Math.cpp:94`) |
| 0x528c20 | `E46Mat::Mul(out, b)` | mat_rows_mul | three row vectors `b_k` (3 floats each, `b[0..2]`, `b[3..5]`, `b[6..8]`): `out_k = b_k * M` (row vector times M, `M` row-major) | known (eq, C++) | verified; 28.14% (operand order, score only) |
| 0x528dd0 | `E46Mat::ScaleRows(dst, n)` | mat_rows_xform4 | per 4-float row `v`: `out = v.x*(m0,m1,m2,m3) + v.y*(m3,m4,m5,m6) + v.z*(m6,m7,m8,m9)` (5g: lane 3 uses the flat index) | known (eq) | verified; 18.75% |
| 0x528fa0 | `E46Mat::XformRows(base, n)` | mat_xform3 | per 4-float row, xyz only: `out = M * (x,y,z)` (column vector) | known (eq) | verified; 11.11% |
| 0x529060 | `E46Mat::MulVec(v, out)` | mat_vec | `out = M * v` | known (eq) | not reached (asm-read: rows `(m0,m1,m2)`, `(m3,m4,m5)`, `(m6,m7,m8)`) |
| 0x528e30 / 0x528ea0 / 0x528f30 | `E46Vec::CrossAcc/CrossCopy/CopyNeg` | - | `out = b x c + this` (CrossAcc); CrossCopy via `CopyFrom`; `CopyNeg`: `out.w = -v` | known (eq) | reviewed (no status) |
| 0x529340 | `E46_Accum340` | body_force_add | `F += v * k` (solid.md) | known | verified; 33.33% |
| 0x529250 | `E46_Accum250` | body_gravity_force | `F = MB560(gravity, S30(this)) / InverseMass` (solid.md) | inferred | verified; 38.03% |
| 0x524f80 | `CPhysics::Vf10` | - | (not read) | - | verified; 33.73% |
| 0x527c60 | `CPhysics::CPhysics` | - | ctor: vtable, state/def/scratch pointers, flags, `m_unk1c`/`m_unk60` init (`0x5c`, `0x60`) | inferred | not read in asm; 39.36% |
| 0x5270c0 | `CState::CState` | - | ctor of the state object (lists, counters) | inferred | not read in asm; 61.54% |
| 0x524c70 | `CDef::CDef` | - | ctor; the keys above | inferred | not read in asm; 62.34% |
| 0x5278d0 / 0x526760 / 0x5284f0 | `CDef::Vf50` / `Vf54` / `Vf4c` | - | Vf50: `E47Solid` owner arg (p2rb fix), Vf54: `CDef` key reader, Vf4c: solver | - | 64.15 / 40.58 / 71.19%, not read |
| 0x526660 / 0x5266d0 / 0x526fd0 | `E46_CreateDef` / `E46_InitPhysics` / `E46_AppInit` | - | setup (`E46_AppInit` fixed by p2) | - | not read; 71.6 / 71.05 / 63.83% |
| 0x5276a0 | `E43HostRel::WallRel` | - | relay into `RegWall` | inferred | verified; 80% |

### 3a. `E46_Notify67d0` in order (asm-spot-checked)

1. `E45_MD60()` (0x523d60), `E37SigEmit9B30(other)` (0x4f9b30).
2. `kind = other.+0x38`. If `other.+0x34 != 0` and `E36SelectTable(other)` gives a table whose
   `+0x684` string contains `"CONE"` the kind is `0x19`, `"BANNER_STAND"` gives `0x15` (the strings
   are known; the kind numbers are inferred from the entity types, not checked).
3. `current` = the top handler (`scratch.+0xe848[top]`, or none when `top < 0`).
4. `handler = E39_EntfCreate(0x7c8040, 0, kind, other.+0x40, current)`; `other.+0x3c = handler`.
5. If `handler`: `handler.S44(other)` (`+0x44` slot). Then: `current` (if any) `.S150(0, handler)`;
   otherwise `RegWall(handler)`. Then `handler.S90(0)` if `other.+0x44 & 1`, else `handler.S94(0)`.
6. Debug (`(physics_flags.log | g.+0x38) & 0x100`): `handler.S118()` twice when the first is
   non-zero, then `handler.+0x34.S4C()`. Same slot in both calls (matches the C++).
7. Stack update: `other.+0x44 & 2` pushes `handler`, `& 4` pops (clamped at -1).

## 4. Open questions

1. **Vtable names.** The detector vtables used here are `0x73f7b0` (`CL1CollisionDetector`, RBR_E46_Physics.h:432),
   `0x73f5a8` (the detector `det` in `CL1CollisionDetector::Vf04`), `0x73f5b4` (PenCheck's det),
   `0x73f5d0` (the L1 pen gig) and `0x73f5c0` (its tail). Only `0x73f7b0` is named in our headers;
   the others need their RTTI, which the lead can take from the vtable accessors.
2. **`E46Mat` convention.** `Mul` and `ScaleRows` use row vectors (`v * M`), `XformRows` and
   `MulVec` use column vectors (`M * v`). If the same `m_m` is passed to both, one of them is
   reading the transpose. Check the callers (`E34`/`E43` matrix users) before any rename.
3. **`ScaleRows` lane 3.** The 4th output lane is `x*m3 + y*m6 + z*m9` (the flat stride-3 load).
   Keep for exactness, but a Rust port must write it this way.
4. **`E46_Notify67d0` kind numbers.** `0x19` (CONE) and `0x15` (BANNER_STAND): check against the
   entity factory's type table.
5. **`CL1CollisionDetector` Combine results.** Which `E33Mat34::Combine` result is `det+8` and
   which is `det+0x10`? The asm copies two 8-byte halves from two results (5f).
6. **Single-step pacing**: still open (`step.md` open 4).

## 5. Suspect code

### 5a. `PenCheck` walks the children of `CPhysics`, not of the node (high, asm-checked)

- C++: `src/rbr/RBR_E46_Small.cpp:210`, `((CEntity*)this)->E43_Walk1F0(det);`.
- What the asm does: in the `PenCheck` loop, after the slot-1 call on `det` with `node`, the code
  does `leal 0x4(%esp), %eax; pushl %eax; movl %esi, %ecx; calll 0x51a1f0`. `esi` is the node
  (`movl 0x450(%ecx), %esi`, then `movl 0x10(%esi), %esi` to advance), so the walk runs on the node.
- Why inconsistent: `CPhysics` has no child list at `+0x48`; the walk on `this` reads the wrong
  object. `Work6a70` (`:225`) and `Visit470` (`:36`) walk the node, so the three should agree.
- Effect: the penetration pass visits the physics world's children instead of each body's. Reached
  (`called only` in the step notes), reccmp 44.07%. Route to the E46 owner: `((CEntity*)node)`.

### 5b. `Work6a70` builds a three-word detector; the C++ has two (reached, bad)

- C++: `src/rbr/RBR_E46_Small.cpp:218` `void* det[2];`, `:221` `det[1] = p;`. The object is a
  `CL1CollisionDetector` (`RBR_E46_Physics.h:432`, vtable `0x73f7b0`, 0x0c bytes: `m_f04` at +4,
  `m_unk08` at +8).
- What the asm does: the `Work6a70` prologue: `movl 0x28(%ecx), %eax` (`m_scratch`), `addl $0x10, %eax`,
  `movl %eax, 0xc(%esp)`. So `det = {0x73f7b0, f, m_scratch + 0x10}`, and `det[1]` is the float
  argument (`movss 0x10(%esp)`), which the C++ stores as `void*` (bit-equal).
- Why inconsistent: `det+8` (`m_unk08`) is the scratch progress pointer, as in `PenCheck`'s
  `det[2] = m_scratch + 0x10`. The C++ stack object has no such word, so `S04` and `E43_Walk1F0`
  read stack garbage at `det+8` in the C++.
- Effect: `bad` on the harness. Fix: `void* det[3]`, `det[2] = (char*)m_scratch + 0x10`.

### 5c. `Visit450` walks `this` (unreached; the Y22 review says no difference)

- C++: `src/rbr/RBR_E46_Detectors.cpp:22`, `((CEntity*)this)->E43_Walk1F0(visitor);`.
- What the asm does: `0x52637c`: `movl %esi, %ecx; calll 0x51a1f0`, `esi` = the node. Same bug as 5a.
- Status: reccmp 79.07%, not in `called.csv`. The `reviewed` status (`Y22 review, no behaviour
  difference`) is wrong on this line. Fix with 5a.

### 5d. `E46_Dispatch69e0` reads the next tag after advancing (unreached, asm-checked)

- C++: `src/rbr/RBR_E46_Conn.cpp:613`, `for (tag = self.+0x38; tag != -1; tag = self.+0x88)`,
  `:615` `E46_Notify67d0(self)`, `:616` `self = self + 0x50`.
- What the asm does: `0x5269e0`-`0x5269f0`: `movl 0x88(%esi), %eax` then `addl $0x50, %esi`. The next
  tag is read from the current element, before the advance.
- Why inconsistent: the C++ update runs after `self += 0x50`, so it reads `element + 0xd8`.
- Effect: the loop ends on the wrong element (not reached on the drive). Fix: read `+0x88` before
  the advance.

### 5e. `E46L1PenDetector::Vf04` copies its own vectors (reached, bad; asm-checked)

- C++: `src/rbr/RBR_E46_Kernels.cpp:118`, `((E01Vec4*)s780)->CopyFrom((E01Vec4*)s780);`,
  `:119` the same for `s790`.
- What the asm does: `0x525b18`-`0x525b45`: `CopyFrom` of the source at `0x30(%esp)` into the
  object at `0x54(%esp)`, and of the source at `0x20(%esp)` into the object at `0x64(%esp)`. The
  two 16-byte vectors are copies inside the stack gig object, not self-copies.
- Why inconsistent: a self-copy does nothing. The gig object's `+0x10` and `+0x20` fields in the
  asm are the copies of the two source vectors, which the C++ never makes. The C++ also writes
  `gig[4] = this+4`, `gig[5] = this+8`, while the asm stores the `this+4` float at `0x7c(%esp)`
  (`0x525b6b`) and does not write `this+8` there.
- Effect: the gig object given to `S04`/`E43_Walk1F0` differs from the original. This fits the
  `bad` status. Route to the E46 owner: rebuild the gig from the asm (`0x525b18`-`0x525b7b`).

### 5f. `CL1CollisionDetector::Vf04` fills `det+8..0x2f` from two different results (reached, 37%; asm-checked, not resolved)

- C++: `src/rbr/RBR_E46_Detectors.cpp:120-121` (`Combine(dd0, g)` into `det+8`, `Combine(dc0, g)` into
  `det+0x10`); `:124-125` take `m_f04` and `m_unk08` into `det+0x20` and `det+0x24`.
- What the asm does: the two `E33Mat34::Combine` results are returned in `eax` (second call) and
  `edi` (first call). The asm copies `(eax)[0..1]` to `det+0x20/0x24`, then `(edi)[0..1]` to
  `det+0x28/0x2c`: two 8-byte halves from two results, not one 16-byte copy per field.
- Why inconsistent: either `Combine` returns a 16-byte value in a register pair, or the C++ types
  are wrong. Needs the `Combine` asm (`0x51ab90`) to settle. Not checked in this pass.

### 5g. Verified-as-consistent items (no bug)

- `Visit470` (`0x526390`): asm equal to the C++ (node walk, flag 0x40 test, `S04` slot 1).
- `PenCheck`'s `det[1] = 1.0f` (asm loads `0x795e14`) and `det[3] = m_state` (asm `movl %ecx, 0x10(%esp)`).
- `E46_Notify67d0` call order and slots (3a).
- `E46Mat::ScaleRows` lane 3 (keep; note 4.3).

## Notes for the lead

- Route 5a and 5b first (both in `Small.cpp`, both reached). 5a is a one-word fix; 5b needs
  `det[3]` plus the scratch word.
- 5c is in `Detectors.cpp:22`; the Y22 `reviewed` status on `Visit450` should be withdrawn.
- 5d is unreached; fix with the same batch.
- 5e (L1 pen gig) needs the asm rebuild, not a line fix.
- 5f is open until the `Combine` asm is read.
- Consistent with other notes: `CScratch` solver at `+0x10` (`p2rb` fix); `CPhysics` solver
  stats at `+4/+8/+0xc` (`step.md` 2c).
- Not read in this pass: `CPhysics::CPhysics`, the CState/CDef ctors, the `CDef` Vf50/Vf54/Vf4c,
  `E46_CreateDef`/`InitPhysics`/`AppInit`, `E46L2Detector::Vf04`, `E46_Kernel5bd0`, `CPhysics::Vf10`.
  Their rows are status only.
