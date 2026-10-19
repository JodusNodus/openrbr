# Damage (E35 CCarDamageHandler, E36 DamageOps, E39 NRepair): physics semantics

Pass 3 of the physics semantics brief (`lead/briefs/physics-semantics.md`), section 3. Notes only;
no source edits. "Current" names are the C++ reconstruction's. "Proposed" names are ours.

Coverage: partial. The damage code is about 3000 lines in E35/E36 and the E39 NRepair classes. This
pass covers the impact path (`Vf08x` -> `Slot1`), the level reset (`Slot4`), the object's aliasing
into the car's physics state (`CCar::Update9C70`), and the damage record that reaches the wheel. The
NRepair classes (E39), the functor tables and the binary save format were not traced.

Confidence: `known` (exe or LSP names it, or the code is unambiguous), `inferred` (strong argument
from use), `guess`. Asm checked in this pass (`objdump -d -M intel` on the original exe, VAs):
`CCarDamageHandler::Vf08x` (0x4ec140, the 0.08 scale), `E36DamageOps::Slot1` (0x4f12a0, the first
part: the `0x160cc & 0x80000` gate and the threshold multiply at `0x4f12f2-0x4f1315`), and
`CCarDamageHandler::ScaleBy` (0x4ec630, whole function). Not asm-checked: `Slot4`, `ZoneMin`,
`Dmg660`, and the rest of `Slot1`.

## 1. Overview

Damage is an accumulator. An impact (a magnitude `dmg`, from the car's contact code, see
`collision.md`) is turned into per-zone damage values. Each zone has a watermark (`zone`), and
each damage field is the watermark clamped to [0, 1]. The damage fields then feed the physics
through the car's state objects, which the damage handler owns (section 3).

The path, from the C++ with the asm where marked:

1. **Impact entry, slot 2** (`CCarDamageHandler::Vf08x`, 0x4ec140, asm checked: `dmg * 0.08f`).
   The raw impact is scaled by 0.08 and passed to slot 1.
2. **Impact dispatch, slot 1** (`E36DamageOps::Slot1`, 0x4f12a0). If the world flag `0x160cc &
   0x80000` is clear, the impact is ignored when `dmg <= 500` (`lim`). If the flag is set, the
   `threshold = this+0x3dec` times `dmg` is the scaled impact (asm: `mulss`), it runs the functors
   for the impact's zone and its link zones (`ApplyFunctors`), then the engine gate
   (`EngineGate(..., 11004.0f)`), then it stores the impact (`this+0x3e70 = dmg`, `this+0x3e64 = 2`,
   the position copy at `this+0x3e90`) and the damage emit.
3. **Zones** (`CCarDamageHandler::ZoneMin(idx, v)`, 0x4ec180, C++ read): zone `idx` (0x58 bytes each
   at `this+0x2214`) takes `v` when `v` is above its watermark. Its damage field (`this+0x3808 +
   4*idx`) is then `min(v, 1)`, and the flag words `this+0x3908 + 4*idx` and `this+0x3a08` are set.
4. **Level reset, slot 4** (`E36DamageOps::Slot4`, 0x4f1000, C++ read): the damage level
   (`this+0x3df0`, 0 to 3) sets three fractions `f7, f8, f9`: level 1 gives `(0.5, 0.5, 0.2)`, level 2
   `(0.8, 0.95, 0.6)`, level 3 `(1, 1, 1)`, otherwise 0. They are written to the wheel and drive
   damage slots (`+0x3c38`, `+0x3ce4`, `+0x3c70`, ...) and clamp the damage table to [0, 1]
   (`this+0x380c`/`+0x226c`, stride 0xb0).
5. **Reset, slot 3** (`CCarDamageHandler::Vf0Cx`, 0x4ec1e0, verified in the CSV): clears the
   zones and the damage floats, resets the car's sub-state objects (section 3), and sets the level
   words.

### 1a. Physics effect

The damage object is not a separate model. `CCar::Update9C70` (`RBR_E34_CarD.cpp:384-427`, C++ read)
aliases the car's physics state into it:

| damage object offset | pointer given to | role |
| --- | --- | --- |
| `+0x3a18` | drive (`this+0x4b0+0x170`) | drive state (`E39StateInit` is run on it in `Vf0Cx`) |
| `+0x3aac` | engine (`this+0x690+0x48`) | engine state (`ResetSub78`) |
| `+0x3b24` | light system (`this+0x838`) | lights (`E36LightBlock::Reset`) |
| `+0x3c04 + 0x38*k` | wheel `k` (`this+0x470`), first argument of `UnkA860` | 13-float wheel damage record (`E51WheelDamage`) |
| `+0x3ce4 + 8*k` | wheel `k`, second argument of `UnkA860` | `CU32Value` (the wheel's damage word pair) |
| `+0x3d04 + 0x20*k` | wheel `k`, third argument | `CU32Value8` |
| `+0x3d84 + 0xc*k` | hinged body `k` (`this+0x480`), `Set650` | per-door damage triple |
| `+0x3db4 + 4*k` | mounted body `k` (`this+0x490`), `M1910` | per-body damage float |

So damage reaches the physics through (a) the engine, drive and lights (the state objects above),
and (b) the wheels, through `CWheel::UnkA860` (`RBR_E50_WheelSim.cpp:556`, "damper and tyre tail
setup", unverified in `notes/physics/wheel.md`). The wheel side is the one that changes the
tyre and damper numbers. How a particular damage word scales a particular tyre or damper value was
not traced (open 2).

## 2. Data

### 2a. CCarDamageHandler (the car's damage object, `car+0x3cc`, also copied to `car+0x9f4`)

Offsets are from the handler's `this`. `Update9C70` reads `car+0x3cc` as the handler (its sub-object offsets go to 0x3db4, so it is the ~0x3eac-byte handler, not the 0x130-byte `CCarBody`). Layout
from the header (`RBR_E35_CarBody.h:43-55`), the zone and functor code (C++ read).

| offset | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| `+0x2214 + 0x58*i` (16 zones) | zone_i | 0x58-byte struct | - | zone `i`'s watermark is word 0 (init 0.5); words 6..0x15 are zeroed | inferred (strong) | `RBR_E35_DamageB.cpp:150-160` (`Vf0Cx`), `ZoneMin` |
| `+0x3808 + 4*i` (64) | damage_i | f32 | 0..1 | zone `i`'s damage, `min(watermark, 1)` | inferred (strong) | `ZoneMin` `RBR_E35_DamageB.cpp:180-196` |
| `+0x3908 + 4*i`, `+0x3a08` | flag_i, flag_any | int | 0/1 | set when a zone's damage is raised | inferred | `ZoneMin` |
| `+0x3dec` | threshold (multiplier and divisor, see K2) | f32 | (see open 1) | scale for the impact: multiplied in `Slot1` (asm), divided in `ScaleBy` (asm) | known (the two uses), meaning unknown | `Slot1` asm `0x4f12f2-0x4f1315`; `ScaleBy` asm `0x4ec630-0x4ec64b` |
| `+0x3df0` | damage_level | int | 0..3 | the damage level; 0 is none, 3 is full (sets `Slot4`'s fractions) | inferred (strong) | `Slot4` `RBR_E36_Damage.cpp:380-400`, `Dmg660` `RBR_E35_DamageB.cpp:318` |
| `+0x3e08 + 8*i` (10, ring), `+0x3e58` | ring, ring_cursor | - | - | ring of the last 10 event codes (`E35Ring`) | known (the code) | `RBR_E35_DamageB.cpp:300-315` |
| `+0x3e60`, `+0x3e64`, `+0x3e6c`, `+0x3e70`, `+0x3e74` | raised, impact_kind, impact_zone, impact_dmg, impact_a5 | int, int, unsigned, f32, f32 | - | the last impact's record, written by `Slot1` | inferred | `Slot1` `RBR_E36_Damage.cpp:520-532` |
| `+0x3e90` | impact_pos | 3-vector | m | the impact position (`CopyFrom(srcA)`) | inferred | `Slot1` |
| `+0x3a10` | reset_count | int | - | increments on each `Vf0Cx` | known (the code) | `RBR_E35_DamageB.cpp:136` |

### 2b. E51WheelDamage (`+0x3c04 + 0x38*k`, 0x38 bytes)

13 floats and a pad word (`RBR_E35_DamageB.cpp:12-18`). Zeroed in `Vf0Cx`, written and read by the
wheel code (`RBR_E51_WheelA.cpp`, not read). Fields not named.

## 3. Functions

| address | current name | proposed name | what it computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x4ec140 | CCarDamageHandler::Vf08x | impact_scale | `S04(a, ..., dmg * 0.08f, ...)`, then slot 1 | known (asm checked) | asm `0x4ec140`; `RBR_E35_DamageB.cpp:37-42` |
| 0x4f12a0 | E36DamageOps::Slot1 | impact_dispatch | gate on `0x160cc & 0x80000` and `dmg <= 500`; scaled = `threshold * dmg`; functors; engine gate; record | partial (asm checked to the scale) | asm `0x4f12a0-0x4f12f2`, `0x4f12f2-0x4f1315`; `RBR_E36_Damage.cpp:496-560` |
| 0x4ec630 | CCarDamageHandler::ScaleBy | threshold_divide | `threshold > 0 ? v / threshold : 0` | known (asm checked); no C++ caller | asm `0x4ec630-0x4ec64b`; `RBR_E35_DamageB.cpp:296-303` |
| 0x4ec180 | CCarDamageHandler::ZoneMin | zone_raise | watermark and damage update, section 1 step 3 | inferred (C++ read) | `RBR_E35_DamageB.cpp:175-202` |
| 0x4ec1e0 | CCarDamageHandler::Vf0Cx | damage_reset | clears zones and floats, resets sub-states | verified (CSV); C++ read | `RBR_E35_DamageB.cpp:127-170` |
| 0x4ec660 | CCarDamageHandler::Dmg660 | gearbox_wear | at level > 1, a wear word `+0x3b00` grows by `(a-b)*c` when `b < a`, capped at 0.6 (level < 3) or 0.9 (the rest of the function not read) | partial (C++ read) | `RBR_E35_DamageB.cpp:318-340` |
| 0x4f1000 | E36DamageOps::Slot4 | damage_level_fractions | section 1 step 4 | verified (asm and load dump) | asm `0x4f1000-0x4f1296`; `load_defs.rs::damage_ops_tables_match_lcb1_load_dump` executes the pure sequence. The C++ at `RBR_E36_Damage.cpp:361-420` is not trusted for behavior |
| 0x4ebb00 | CCarBody::CDef::Vf50 (factory) | (E35 body def factory) | news the 0x150-byte CDef of the car body | known (C++ read) | `RBR_E35_CarBody.h:24-28` |
| 0x4eb960 | CCarBody::Vf40 | (E35 body update) | `CBody::Vf40` plus the state's slot 0x44 | known (C++ read) | `RBR_E35_CarBody.h:23` |
| 0x4eb8c0 | CCarBody::VfA0 | scatter_add | add scatter (reached, labelled verified) | not read | - |
| 0x4eb820 | CCarBody::Vf9C | scatter_sub | subtract scatter (reached, labelled verified) | not read | - |
| 0x4ebbe0 | CCarBody::Vf28 | solid_state_emit | SolidState emit (reached, labelled verified) | not read | - |
| 0x4eb3e0 | E35CarOps::Vf50Def | (def factory) | (reached, labelled verified) | not read | - |
| 0x4ef410 | E36DamageOps::Ctor | `layout::construct_damage_ops` | zones, counters and E35 threshold tables | verified (asm and load dump) | asm `0x4ef410-0x4ef5b0`; `tests/load_defs.rs::damage_ops_tables_match_lcb1_load_dump` |
| 0x4ed370 | CCarDamageHandler::InitTables | `layout::init_damage_handler_tables` | first-free slots in the fixed E35 threshold tables | verified (asm and load dump) | asm `0x4ed370-0x4edede`; load test |
| 0x4ef910 | E36DamageOps::InitTables | `layout::init_damage_ops_tables` | resets 64 counts, appends 56 inline entries and makes 13 `AddFunctor` calls | verified (asm and load dump) | asm `0x4ef910-0x4f0ff0`; load test |
| 0x4ef720 | E36DamageOps::AddFunctor | `damage::add_functor` | appends a tagged table entry | verified (direct vectors and load dump) | `car_body_vectors::add_functor_matches_original_calls`; load test |
| 0x4f1000 | (see Slot4) | | | | |

### Load table check

`load_defs.rs::damage_ops_tables_match_lcb1_load_dump` builds the object from a blank 0x3eb0-byte region at the address
from `lcb1-bodies.snap-0`, runs the constructor, sets the run's level 1, and runs Slot4. It compares every stable byte
with `RBRRecompSnapAt-004ea720-1-after.bin`, the return from the same run's `UpdateA720`. The only excluded stable
write is `+0x3de8`: `UpdateA720` writes the seed there after Slot4. Four table values at `+0x2a88`, `+0x2ae0`,
`+0x2b38` and `+0x2b90` are 0.1f. The C++ reconstruction says 0.7f, but the original loads 0.1f from 0x72b068 in
`0x4ed370`; use the disassembly and the load dump as the reference.

### Runtime reachability

The recorded 30-step all-bindings run on the 9 vcu-cov snapshots plus `snapH.snap-1295` passed on 10/10: Run B was
equal apart from excluded bytes, Run A matched `drive-tanner-corolla.bin`, and there were no declines. It did not reach
the new damage bindings. One-step `tanner-damage` chains at 3140 and 6650 were equal apart from excluded bytes; 11820
had the known one-byte car `+0x1cc` divergence from the other worker's `0x52970b` fix. The recorded 30-step chain from
11820 stopped at 11827 after one missing cached x87 result, with two unexcluded bytes at car `+0x1cc` and `+0x166c`;
the first is the known `0x52970b` divergence. The 11827 chain stopped before a step on an unstubbed call from
`0x4d2520`. The 12000 chain completed 30 steps with 17 bytes differing from Run A, first at car `+0x166c`. Run A
also differed from the damage recording at frame 11821 and 12001 in words `0x32c` and `0x38c`; those are original-run
comparisons and do not depend on this Rust branch. None of these tanner chains reached the new damage bindings, so
their runtime entries remain marked `unreached` in `rust/rbr-hybrid/bound.csv`; the load dump and direct-vector checks
are the available evidence for their table behavior.

## 4. Open questions

1. **What `+0x3dec` is.** It is multiplied with the impact in `Slot1` (asm), but `ScaleBy` divides by it
   (asm). Both are in the original. The C++ comment calls it a "threshold". If it is a threshold for
   impact, the `Slot1` multiply is wrong in meaning. If it is a gain, the name is wrong and `ScaleBy`
   is an unused helper. Settled by reading the callers of `ScaleBy` in the asm (it has none in the C++).
2. **Units of `dmg`.** The impact reaches `Vf08x` in some unit that is scaled by 0.08, then compared
   with 500 in `Slot1` when the `0x80000` flag is clear. The unit is not established. Candidates are
   a force (N), an acceleration (m/s^2, with the 490.5 = 50 g threshold in `car-body.md`), or an
   impulse (N s). Settled by tracing the caller of `Vf08x` (the collision or the car's contact code)
   and checking one impact in a recording.
3. **Level 0 fractions.** `Slot4` gives 0 fractions at level 0 and at any level above 3 (the `else`
   branch), so level 0 and an invalid level are the same. Confirm that `this+0x3df0` is never above 3.
4. **Which damage words scale the tyre and damper.** The record at `+0x3c04` reaches `CWheel::UnkA860`,
   but the wheel code that reads it was not traced. Settled by reading `RBR_E51_WheelA.cpp` and the
   `UnkA860` asm.
5. **`E36 Slot1` `ApplyFunctors` and `EngineGate`.** Their bodies were not read. The `11004.0f` constant
   in the `EngineGate` call is not named.
6. **The `0x160cc & 0x80000` flag** is read from the world object (`g_1659b70+0x24`). Its name is not
   established.

## 5. Suspect code

Asm is the reference. Addresses are VAs.

**D1. `E36DamageOps::Slot1` multiplies the threshold (`RBR_E36_Damage.cpp:534-536`) and
`CCarDamageHandler::ScaleBy` divides by it (`RBR_E35_DamageB.cpp:296-303`).** The asm agrees with
each (`mulss` at `0x4f1315`; `divss` at `0x4ec645`). The two uses cannot both mean "threshold". One
is wrong in meaning, or the field has two roles. The C++ comment on `ScaleBy` ("threshold divider")
and on `Slot1` ("scaled") disagree with each other. Route to the E35/E36 owners with open 1.

**D2. `E36DamageOps::Slot1` (`RBR_E36_Damage.cpp:520-523`): the threshold gate.** The C++ returns
early when `dmg <= 500` and the world flag is clear. The `500` is a literal. The asm `cmp`/`comiss`
against it was not read here. Confirm the constant and the branch sense with the asm before the
units are settled (open 2).

**D3. `CCar::Update9C70` aliases the drive, engine and light state into the damage object
(`RBR_E34_CarD.cpp:391-405`).** The aliasing is consistent with `Vf0Cx` and `Slot4`, which reset those
objects through the same offsets. It is not asm-checked. A wrong offset here would move the drive or
engine state into the wrong damage slot, so the owners should check these four pointers against the
ctor (`RBR_E35_CarBody.h`) before any rename.

**D4. Stale declaration: `CCarDamageHandler` in `RBR_E34_CarD.cpp:47`.** The E34 file declares its own
local `struct CCarDamageHandler` (one member) while the E35 header owns the class. The E34 code calls
`Test3B20` on `this+0x9f4` (`RBR_E34_CarD.cpp:336`). This is a declaration-only mismatch; the call site
Not a physics bug. Mention to the owner.
