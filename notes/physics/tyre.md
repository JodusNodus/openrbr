# Tyre (CTyre, E48/E49): physics semantics

Pass 2 of the physics semantics brief (`lead/briefs/physics-semantics.md`). Notes only; no
source edits. "Current" names are the C++ reconstruction's. "Proposed" names are ours.

Confidence: `known` (exe or LSP names it, or the code is unambiguous), `inferred` (strong
argument from use), `guess`. Asm checked in this pass (`llvm-objdump`, VAs): `CScratch::Step`
(0x5360c0, whole function), `ComputeSlip` (0x532a30), `SolveDirs` (0x533640), `Sign` (0x531ba0),
`TyreClamp` (0x5322e0), `Accumulate` (0x5338a0, all of it), `VcuTableLookup` (0x536880),
`FloorS32` (0x5325f0), `WriteOutputs` (0x533080, the speed store), `CopyFrom` (0x535040),
`TyreFrictionCurve` (0x5334d0), `SplitSlip` (0x5346f0, entry only). Bit-level checks of the
first six are in `rust/rbr-physics` on the Rust pilot branch (vectors from a capture: 64 calls
each, 24-bit x87).

Not verified in this pass (C++ comment only, or asm not read): the bodies of `SolveA`,
`SolveB`, `SolveDispatch`, `SolvePatch`, `StepInner` beyond its first 40%, `Friction`, `Align`,
`ClampGrip`, `TableStep`, `RelaxStep`, `SolveForces`, `FillOutputs`, `HeatStep`, `BlendTable`,
`ContactPatch`, and the names of the E48 CDef and CState constructor fields.

## 1. Overview

A tyre is a contact-patch model. Each step, the wheel gives the tyre its spin rate `ω`, the
contact frame (`dir`, the wheel-plane direction vectors) and the car-frame velocity at the
contact. The tyre computes:

1. the contact-patch slip velocity (longitudinal and lateral, `ComputeSlip`),
2. the direction cosines of that slip (`SolveDirs`: absolute sine and cosine of the slip angle,
   and their L1 sum),
3. the tyre force from a friction curve of the normalised slip (`TyreFrictionCurve`, a
   magic-formula-like curve; `ClampGrip` limits it to the grip circle),
4. the accumulated force and heat, and relaxation of the carcass and tread state
   (`Accumulate`, `RelaxStep`, `HeatStep`), and
5. the outputs written to the wheel block (`WriteOutputs`, `FillOutputs`).

Where it sits: `CWheel::M3DD30` (0x53dd30, "slot-96 step") and `CWheel::M3EB80` (0x53eb80,
"E35-driven step") call `CTyre::CScratch::Step` (0x5360c0) once per wheel step, after the
wheel's ground probe. Those two wheel functions are for `notes/physics/wheel.md`.

Call tree of the reached tyre functions (direct calls, from the disassembly):

```
CScratch::Step 0x5360c0                          <- CWheel::M3DD30 0x53dd30, M3EB80 0x53eb80
  CopyFrom 0x535040                              (39 material words into CTyre+0x1a0)
  Align 0x531fc0                                 (calls 0x43c630, 0x473b30, 0x43c5b0 helpers)
  ContactPatch                                   (no contact: early exit)
  ComputeSlip 0x532a30                           (no calls)
  Accumulate 0x5338a0                            (no calls)
  SplitSlip 0x5346f0                             (no calls seen in the call scan)
  SolvePatch 0x535d60 -> StepInner 0x535310, TyreClamp 0x5322e0 (x2)
    StepInner 0x535310 -> SolveDirs 0x533640, TyreFrictionCurve 0x5334d0 (x3), 0x5320c0, 0x465d80
      TyreFrictionCurve 0x5334d0 -> FloorS32 0x5325f0
  SolvePair                                      (body not read)
  SolveForces 0x532b60 -> Friction 0x531c70, 0x473cd0
  Sign 0x531ba0                                  (no calls)
  HeatStep                                       (body not read)
  WriteOutputs 0x533080                          (-> 0x473cd0, 0x43c5b0)
  FillOutputs 0x532f10                           (-> 0x43c5b0)
SolveDispatch 0x5351a0 -> SolveA 0x533ae0, SolveB 0x533bf0, 0x533d40, 0x533e80
  (SolveA and SolveDispatch call each other: mutual recursion, open question 1)
RelaxStep 0x5345a0 -> DamperStep1/2, DamperAccumA, DamperForce (E48 spring-damper), TableStep 0x534480
TableStep 0x534480 -> DamperStep1/2, DamperAccumA
VcuTableLookup 0x536880 (VCU table; the tyre code also reaches it through SolvePatch's tail)
```

Inputs from the wheel: `ω` (`spin`, rad/s), the contact-frame direction vectors, the car-frame
velocity at the contact, the surface blend (`m_state+0x98`, `m_state+0x90`) and the tyre setup
(CDef). Outputs to the wheel: the force and torque values written by `WriteOutputs`, the slip
values and speed written by `FillOutputs`, and the state-valid flag (`sti[3] = 1`) set at the
end of `Step`.

## 2. Data

`F[i]` is a float in `this` (the CScratch), `i` in hex as in the C++ source. Where that `this`
sits in the wheel is open: the C++ wheel passes `wheel+0xf00` to `Step` (`notes/physics/wheel.md`,
open question 2), while the harness README places the scratch at `CWheel+0x800` (from the peek
probes). The offsets below are relative to `this` of `Step`, whichever it is.

### 2a. CScratch fields used by ComputeSlip, Sign, Accumulate and Step

| offset | F index | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0x170 .. 0x198 | 0x5c .. 0x66 | `f[0x5C..0x66]` | slip_matrix (3x3; rows at 0x170, 0x180, 0x190) | f32 | per m | coefficients mapping the contact direction `dir` to three slip components | inferred | `ComputeSlip` 0x532a30: `s_i = row_i · dir` |
| 0x30c | 0xc3 | `f[0xC3]` | (tyre factor) | f32 | - | multiplies `f340` in `Accumulate`, and `f298` in `Sign` | guess | `Sign` 0x531ba0, `Accumulate` 0x5338a0 |
| 0x314 | 0xc5 | `f[0xC5]` | (tyre factor 2) | f32 | - | multiplies `Sign`'s product; `Step` writes `F[0xC5]*0.1*F[0xFC]` | guess | `Sign` 0x531ba0 |
| 0x340, 0x344 | 0xd0, 0xd1 | `f[0xD0]`, `f[0xD1]` | blend_hi, blend_lo | f32 | - | lerp pair: `(X0-X1)*s16 + X1` in `Accumulate` | inferred | `Accumulate` 0x5338a0 |
| 0x360 | 0xd8 | `f[0xD8]` | speed | f32 | m/s | magnitude of the contact-plane slip velocity, `sqrt(s0^2 + s1^2)` | inferred (strong) | `ComputeSlip`: `t = f[0xDB]^2 + f[0xDA]^2`, `f[0xD8] = sqrt(t)`. `Step` uses `(F[0xD8] - 10)*0.1` clamped to [0, 1] as a weight |
| 0x364 | 0xd9 | `f[0xD9]` | speed_sq | f32 | m^2/s^2 | `s0^2 + s1^2` | known (by formula) | `ComputeSlip` |
| 0x368 | 0xda | `f[0xDA]` | v_slip_x | f32 | m/s | component 0 of the slip velocity (`s0 = row0 · dir`) | inferred | `ComputeSlip` |
| 0x36c | 0xdb | `f[0xDB]` | v_slip_y | f32 | m/s | component 1 (`s1`) | inferred | `ComputeSlip` |
| 0x370 | 0xdc | `f[0xDC]` | v_slip_z | f32 | m/s | component 2 (`s2`) | inferred | `ComputeSlip` |
| 0x374 | 0xdd | `f[0xDD]` | v_long_contact | f32 | m/s | `R*ω + s1`: the longitudinal velocity of the contact point | inferred (strong) | `ComputeSlip`: `m_state[0x13]*spin + f[0xDB]` |
| 0x37c | 0xdf | `f[0xDF]` | slip_ratio_term | f32 | 1/s | `(-1/R)*s1 - spin` | inferred | `ComputeSlip` |
| 0x378 | 0xde | `*(int*)(this+0x378)` | spin (ω) | f32 bits | rad/s | the first argument of `Step`: the wheel's spin rate (`wheel+0xa50`, see `wheel.md`), stored as float bits. `ComputeSlip` and `SolveDirs` read it as `R·ω`; `Sign` tests its sign (the rolling direction) | inferred (strong) | `Step`: `*(int*)(this+0x378) = (int)a`; `ComputeSlip(..., *(float*)(this+0x378))`; `SolveDirs(a)`: `c = a·R + c` |
| 0x388 | 0xe2 | `*(int*)(this+0x388)` | (state int) | int | - | copied to the wheel output at `+0x94` | guess | `WriteOutputs` 0x533080 |
| 0x3f0 | 0xfc | `f[0xFC]` | (blend weight) | f32 | - | weights the `SolveForces` outputs | guess | `Step` |
| 0x3fc, 0x3ec | 0xff, 0xfb | - | - | f32 | - | `m_state` pair mixed into `stf[0x10..0x11]` | guess | `Step` |
| 0x47c, 0x480 | 0x11f, 0x120 | `*(int*)(this+0x47c)`, `(this+0x480)` | contact_flag | int | - | 0 when there is no contact (early exit); 1 after `ComputeSlip` | inferred | `Step` |

### 2b. CState (`m_state`, `this+0x14`), fields the tyre code reads

| offset | index | current name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0x40, 0x44 | 0x10, 0x11 | `stf[0x10]`, `stf[0x11]` | force_x, force_y (contact frame) | f32 | N | written at the end of `Step` from the `dir` · `outA` products; `stf[0x11]` is updated by `- F[0xDF]*b` | inferred | `Step` 0x5360c0 |
| 0x4c | 0x13 | `stf[0x13]` | effective_radius | f32 | m | `R` in `R*ω + v` (`SolveDirs`, `ComputeSlip`) and in `(-1/R)*s1` | inferred (strong) | `SolveDirs` 0x533640: `c = a*stf[0x13] + c`. Value range: the LSP `Radius` is 0.315 m and `CarcassRadius` 0.30-0.305 m (`physics-lsp.md`) |
| 0x58 | 0x16 | `stf[0x16]` | weight_a | f32 | - | lerp weight in `Accumulate`: `(X0-X1)*s16 + X1` | inferred | `Accumulate` 0x5338a0 |
| 0x64 | 0x19 | `stf[0x19]` | tyre_temperature | f32 | K | `DamperBiasSub(stf[0x19])` subtracts 273.15; that turns kelvin into celsius | known for the subtraction (273.15 is 0 °C in kelvin); inferred for the field | `RBR_E49_TyreSim.cpp:537` (`DamperBiasSub` call) |
| 0x90, 0x94, 0x9c, 0xa8, 0xb4 | 0x24, 0x25, 0x27, 0x2a, 0x2d | `m_state` index words | table indices | int | - | index words of the table lerps in `Step` (written by `E46_IdxWrite`) | inferred | `Step` 0x5360c0 |
| 0x98, 0xa4, 0xb0, 0xbc | 0x26, 0x29, 0x2c, 0x2f | `m_state` blend words | table fractions | f32 | - | the fraction of each table lerp in `Step` | inferred | `Step` 0x5360c0 |

### 2c. CDef (`m_def`, `this+0x10`): the tweak setup

The CDef constructor (0x530c50) sets default values: `+0xdc = 25000`, `+0xec = 0.01`, `+0x118 = 1`,
`+0x110 = 0.02`, `+0x11c = 1.5`, `+0x114 = 2000`, `+0x120 = 2000`, `+0x124 = 2`, `+0x128 = 9`,
`+0x12c = 2000`, `+0x130 = 2`, `+0xfc = 0.00157` (0.09 degrees in radians), `+0x10c = 5`, and a long
run of table defaults at `+0x188..0x2c0`. Among them, `+0x244..0x25c` are 1.10390, 0.96753,
0.90909, 0.85065, 0.79221, 0.73377, 0.58442, and `+0x264` is 1.2013, `+0x268` 1.0974. Most field
names are not established (open question 2).

The exe holds these tyre key names (`physics-lsp.md`; they are in the exe string table):
`CarcassRadius`, `CarcassWidth`, `CarcassStiffnessLateral/Torsional/Vertical`,
`CarcassThickness`, `HubOffset`, `ReferenceFriction`, `TreadDepth`, `TreadWidth`,
`TreadCoverage`, `TreadElasticity`, `SideWallHeight`, `PressureRatio`, `RimRadius`,
`RimInnerRadius`, `Type`, `WetnessDesign`, `WetnessWearFtr`, `WetSlope_T0-5`, `WearFtr_T0-5`,
`SFric_T0-5` (surface friction per tread state), `RubMyFct_T0-7`, `RubWear_T0-7` (rubber
multiplier and wear per tread state), `CrnMyFct_L0-7`, `TrctMyFct_L0-7` (cornering and traction
multiplier per load level L), `CrnStf_L0-7`, `TrctStf_L0-7` (cornering and traction stiffness per
L), `SlpPkCrn_L0-7`, `SlpPkTrct_L0-7` (peak slip per L), `StfFct_P0-3` (stiffness factor per
pressure P).

Ranges across the car files (`physics-lsp.md`): `CarcassRadius` 0.30-0.305 m, `CarcassWidth`
0.155-0.25 m, `CarcassStiffnessLateral` 256,000-410,000, `Torsional` 75,000-80,000, `TreadDepth`
0.01-0.015 m, `RimRadius` 0.2-0.24 m, `ReferenceFriction` 1, `SlpPkCrn` 0.13-0.47 (peak cornering
slip, rad), `SlpPkTrct` 0.09-0.35 (peak traction slip, ratio), `SFric_T*` 0.2-1, `CrnStf_L*`
6,000-86,000.

Their C++ fields are not named. The tyre dictionary table (`0x7cd080`-`0x7cd3b0`, records
`{name, 0, id, 0}`) gives `CrnMyFct_L0` id 0x10f, `SFric_T0` id 0x4b and `SlpPkCrn_L0` id 0x14f,
while the CDef constructor's value words sit at other offsets (open question 2). So the id-to-offset
rule the VCU uses (id x 4 is the CReal object offset) does not give these fields.

## 3. Functions

Reached tyre functions. The equations use the proposed names. UI menu functions (E48 0x5302b0-0x5303a0,
0x530690, 0x530a20, 0x530090, 0x5317f0) are out of scope and are not listed.

| address | current name | proposed name | what it computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- | 
| 0x5360c0 | CScratch::Step | tyre_step | the per-wheel tyre step (section 1). Stores `e3` into `F[0x12F]` (suspect S2) and applies `e3` twice to `F[0x7A]` (suspect S3) | asm verified for the structure; equations partial | `RBR_E49_TyreStep.cpp:381-520` and the asm |
| 0x532a30 | CScratch::ComputeSlip | compute_slip | `s = M·dir` (3x3 at 0x170-0x198); `speed = sqrt(s1^2 + s0^2)`; `v_long = R·ω + s1`; `slip_term = -(s1/R) - ω` | verified (pilot: 64 calls, bit-exact) | pilot vectors |
| 0x533640 | CScratch::SolveDirs | solve_dirs | `c' = a·R + c`; `e = sqrt(b^2 + c'^2)`. If `e > 0.001`: `(|b|/e, |c'|/e, |b|/e + |c'|/e)`, else `(1, 1, 2)` | verified (pilot: 64 calls) | pilot vectors |
| 0x531ba0 | CScratch::Sign | signed_term (proposed; the name Sign is misleading) | `-(t·f314·f3dc·sgn(f378))` with `t = f30c·f298·0.5` in f32, and `sgn` = +1, -1 or 0 | verified (pilot: 64 calls) | pilot vectors |
| 0x5322e0 | TyreClamp | clamp_by_stiffness (proposed) | `t = -b·c`. For `a > 0`: if `a < t`, `-a/b` (0 when `b <= 0.001`); else 0 if `t < 0`, else `c`. For `a <= 0`: if `a > t`, `-a/b` (0 when `b <= 0.001`); else 0 if `t > 0`, else `c` | verified (pilot: 64 calls) | pilot vectors |
| 0x5338a0 | CScratch::Accumulate | accumulate_force_heat | 45 steps (suspect S4). Includes the term `(q·Y)·0.0`, always zero. Writes `0x350, 0x314, 0x348, 0x354, 0x504, 0x508, 0x50c, 0x34c` | verified (pilot: 64 calls) | pilot vectors |
| 0x535040 | CTyreMatData::CopyFrom | copy_material | copies a fixed list of 39 words from the source to `this`; the order does not matter | verified (pilot: 64 calls) | pilot vectors |
| 0x536880 | NPhysics::VcuTableLookup | table_lookup_8 | entries of 8 floats (key at word 3, value at word 7). Finds the first entry `u` (searched from 1) with `x <= key(u)`, then `value = w·value(u) + (1-w)·value(u-1)`, `w = (x-key(u-1))/(key(u)-key(u-1))`. Returns the value from `value(0)`, `value(n-1)` outside the range | verified (pilot: 64 calls) | pilot vectors (the VCU's table; the tyre path reaches it too) |
| 0x5325f0 | FloorS32 | table_index_blend (proposed) | an index from `x` (clamped to the 32-bit range, with `0x80000000` or `0x7fffffff` on overflow and an error call `0x43c8e0` out of range) and a two-entry blend of the table | partial (asm read, not verified) | `RBR_E48_TyreMenu.cpp`; asm 0x5325f0-0x5326a1 |
| 0x5334d0 | TyreFrictionCurve | friction_curve | the normalised slip `a` with a peak threshold `v = (d·g/f)·1.2` (and `3v`). Branch 1 (`a < b`): a rational blend in `a/b`. Branch 2: a linear table blend `(d-e)·u + e` with `u = FloorS32(...)`. Branch 3 returns `e` | partial (asm read; no vectors: it calls FloorS32) | asm 0x5334d0-0x53363a |
| 0x535310 | CScratch::StepInner | (contact patch inner step) | direction solve (`SolveDirs`), three `TyreFrictionCurve` evaluations, and the output blend into `m_state` | partial (first 40% of asm) | asm to 0x5355fc |
| 0x535d60 | CScratch::SolvePatch | (contact patch) | calls `StepInner` and `TyreClamp` twice. The C++ comment says recursive contact or direct output | unverified | `RBR_E49_TyreStep.cpp:209` comment |
| 0x5351a0 | CScratch::SolveDispatch | solve_dispatch | routes to `SolveA`, `SolveB`, `0x533d40` or `0x533e80` by flags (`idx = a6+1`, route on `(a4, a5)`) | unverified | `RBR_E49_CTyre.cpp:153` comment |
| 0x533ae0, 0x533bf0 | CScratch::SolveA, SolveB | (contact solve A and B) | not read | unverified | |
| 0x5345a0 | CScratch::RelaxStep | relax_state | relaxation of the tread and carcass damper states (DamperStep1/2, AccumA, Force), then `TableStep` | unverified | calls only; `RBR_E49_TyreSim.cpp:671` comment |
| 0x534480 | CScratch::TableStep | table_step | table index step; calls DamperStep1/2 and AccumA | unverified | `RBR_E49_TyreSim.cpp:647` comment |
| 0x532b60 | CScratch::SolveForces | solve_forces | friction-state blend and force accumulation into two output vectors; calls `Friction` | unverified | `RBR_E49_TyreSim.cpp:45` comment |
| 0x531c70 | CScratch::Friction | friction | the friction solve (C++ comment: 70% match) | unverified | `RBR_E48_TyreMenu.cpp:572` comment |
| 0x532f10, 0x533080 | FillOutputs, WriteOutputs | fill_outputs, write_outputs | write the output block: `WriteOutputs` stores `F[0xD7]·1000` at `+0x88`, copies `0x340` and `0x388` to `+0x90` and `+0x94`, and stores `F[0xD8]·0.0` at `+0x98` (suspect S5) | partial (asm read for the `+0x88` and `+0x98` stores) | `RBR_E49_TyreSim.cpp:225`; asm 0x533080 |
| 0x5346f0 | CScratch::SplitSlip | split_slip | splits the slip by the spin range (C++ comment) | unverified | `RBR_E49_TyreSim.cpp:695` comment |
| 0x534e70 | CScratch::BlendTable | blend_table | table blend with the wear array (C++ comment) | unverified | `RBR_E49_TyreSim.cpp:723` comment |
| 0x534090 | CScratch::HeatStep | update_temperature (proposed) | calls `DamperBiasSub(stf[0x19])`, kelvin to celsius | partial semantics; runtime verified | `RBR_E49_TyreSim.cpp:537`; `c3-heat-final` |
| 0x5320c0 | (no name) | unnamed | called by `StepInner`; no direct calls; C++ not found | unknown | |

## 4. Open questions

1. **Mutual recursion.** `SolveA` (0x533ae0) calls `SolveDispatch` (0x5351a0), and `SolveDispatch`
   calls `SolveA` and `SolveB`. The pair is recursive, so the depth is set by the data. What stops
   it, and the bound, are not read.
2. **CDef names.** The tyre dictionary table (`0x7cd080`-`0x7cd3b0`) names the tyre keys, but its
   ids do not match the CDef value offsets the constructor writes (`CrnMyFct_L0` is id 0x10f, while
   the matching-looking defaults sit at `+0x244`). The mapping (a CDef layout, or a second object) is
   not found. Until it is, the CDef fields in section 2c are placeholders.
3. ~~`this+0x378`~~ **Resolved (see correction C1):** it is the spin rate ω.
4. **`0x5320c0`** is called by `StepInner` and has no direct calls. Its role is unknown.
5. **Unread calls in `Step`:** `SolvePair`, `HeatStep`, `ContactPatch` and `Align`'s body. The
   description in section 1 skips them.
6. **Units of the radius and stiffness fields.** `stf[0x13]` as `R` is inferred from the formulas. A
   check: sample `m_state[0x13]` in a recording and compare with the wheel's `Radius`.
7. **Speed threshold 10.** `Step`'s clamped weight `(speed - 10)·0.1` assumes `F[0xD8]` is m/s. If
   it is not, the weight's meaning changes.

## 5. Suspect code

Each item is a place where the reconstruction's C++ and the original's asm differ, or where a
value's use contradicts its meaning. All are verified against the asm unless marked otherwise.

**S1. `CScratch::Step`, output arguments (resolved).** The asm stores `Sign()+F[0xE9]` through
`[esp+0x48]` to arg `n`, then stores `F[0xC5]*0.1*F[0xFC]` through `[esp+0x4c]` to arg `o`
(0x5366ec-0x53670f). The C++ writes those same outputs. At the final call, the original passes arg `j`
as `FillOutputs`'s first argument (it reloads it into EBX at 0x536223 and pushes it at 0x53673a); the
C++ now passes `outA`/`j`, while arg `k` seeds the working vector.

**S2. `CScratch::Step`, `F[0x12F]` store (resolved).** The asm stores `e3` as a float with
`movss xmm0,[esp+0x24]` / `movss [esi+0x4bc],xmm0` (0x5365ed-0x5365f5). The C++ now does
`F[0x12F] = e3` without an integer conversion.

**S3. `CScratch::Step`, `RBR_E49_TyreStep.cpp:490-494`: `e3` applied twice to `F[0x7A]`.**
`e6 = F[0x7A]*e3; F[0x7A] = e6; F[0x7A] = F[0x7A]*e3;`. The asm does the same: it stores `e6` into
`[esi+0x1e8]` through the CBase setter (`call 0x52f450`, then `movss [edi+0xc], xmm0`), reads
`[esi+0x1e8]` back and multiplies by `e3` again (0x5365c7-0x5365e8). So the original squares the
factor. It may be a duplicate in the original source; the owner should confirm what was meant.

**S4. `CScratch::Accumulate`, `RBR_E49_TyreSim.cpp:493` and `:504`: zero multipliers.**
`c = c*b*f[0x8E]*f[0xDB]*0.0f;` (line 493) and `f[0xD4] = g*0.0f;` (line 504). The asm multiplies by a
register zeroed with `xorps` (0x5338c7, reused at 0x5339ab and 0x533a2c), so the term is always ±0.
Its contribution to `c`, `g` and `f[0xD4]` is therefore zero, which the C++ also gives. Either the term
was disabled in the original (a coefficient set to 0) or it is a source mistake. The behaviour matches
either way, but the formula's intent is lost; the owner should confirm the coefficient.

**S5. `CScratch::WriteOutputs`, `RBR_E49_TyreSim.cpp:261` (`o[0x26] = f[0xD8] * 0.0f;`).**
The asm loads the speed (`movss xmm0, [esi+0x360]`), multiplies by `xmm3 = 0` (zeroed at 0x53313e), and
stores to `[edi+0x98]` (0x533161-0x53316d). The output word `+0x98` is therefore always 0. If the
consumer (the wheel) reads `+0x98` as a speed, it gets 0. The wheel note checks the consumer.

**S6. Naming: `CScratch::Sign` is not a sign function.** It returns `-(t·f314·f3dc·sgn(f378))`, a signed
force term. The name misleads; the code is correct (vectors match). Proposed: `signed_term`.

**C1. Correction (from `wheel.md`).** `this+0x378` is the wheel's spin rate ω, not a mode: the C++
wheel passes `*(int*)(wheel+0xa50)` as `Step`'s first argument (`RBR_E51_WheelD.cpp:503`), and
`ComputeSlip`/`SolveDirs` multiply it by the radius (`R·ω`), so the unit is rad/s. `Sign`'s sign
test is then the sign of the rolling direction. The row for 0x378 in section 2a is corrected to match.
The `this` of `Step` is `wheel+0xf00` in the C++ (`wheel.md`, open question 2); the offsets here are
relative to it.

**S7 (ordering).** `ComputeSlip` and `DamperCombine` showed 1-ulp mismatches in the shadow runs
(`notes/shadow-exe.md`) from operation order. The Rust port follows the asm, not the C expression
order, so the C++ expression order is not the spec for these functions.

**C2. Verification update (2026-10-14).** `Step` preserves the original `F[0xC5] * 0.1f` rounding
before multiplying by `F[0xFC]`; a volatile intermediate prevents MSVC from reassociating the SSE
operations (0x5366f7-0x53670f). ContactPatch uses `dff[0x27] + F[0xEF]` in its quadratic coefficient
(the original loads `m_def+0x9c` at 0x535b3c), and retains the direct-root x87 operation order. Step
and ContactPatch pass the full-drive `c3-final-step-contact`
bisect on current main, with 41,161 calls each and identical output over 12,084 steps. The focused
replay's remaining ContactPatch register differences are caller-saved ECX/XMM values; its return
value and full 0x600-byte scratch state matched over 160 calls.

**C3. Verification update (2026-10-14).** `CTyre::CState` 0x531500 stores its `CBase::m_name`
pointer at `this+4`; the original uses the specific `.rdata` string at 0x740b60. The same `Tyre`
bytes occur earlier at 0x735c3b, so the transplant's first-match string resolver chose the wrong
pointer for all four wheels. The ctor now names the original instance. Its extra `m_unk20.m_value`
zero store was also removed because the original ctor does not write that word. `ComputeOutputs`
0x5320c0 writes the same outputs, but its generated C++ clobbered XMM5/XMM6, which `StepInner`
carries live across the call; save/restore now preserves the original's caller-visible registers.
The ctor and `ComputeOutputs` pass `c3-main-1cbc` (four and 34,677 calls respectively, identical
over 12,084 steps).

`CScratch` ctor 0x5328d0 was initially skipped because its class vtable paired only one slot.
Declaring the deleting destructor as slot 0 and keeping the container destructor nonvirtual makes
the original two-slot vtable at 0x740bdc transplantable; the ctor now passes `c3-main-1cbc` with
four calls over 12,084 identical steps.

`HeatStep` 0x534090 passes `c3-main-1cbc` (39,753 calls) and the earlier full-drive
`c3-heat-final` (37,138 calls). The fix preserves the
original `k*0.5 + b + a` SSE addition order, rounds `e-n` to float before using it in the x87
expression, and retains `DamperStep1*c` as a double for the final subtraction. A volatile double
for `(1 - 0.6*a)*t` prevents MSVC from swapping the following multiplication with `e-n`; the
focused replay of the former failure at snapshot 1392 is identical.
