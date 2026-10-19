# Wheel (CWheel, E50/E51): physics semantics

Pass 2 of the physics semantics brief (`lead/briefs/physics-semantics.md`). Notes only; no
source edits. "Current" names are the C++ reconstruction's. "Proposed" names are ours.

Confidence: `known` (exe or LSP names it, or the code is unambiguous), `inferred` (strong
argument from use), `guess`. Asm checked in this pass (`llvm-objdump`, VAs): `CWheel::M3DD30`
(0x53dd30, the whole function: 3664 bytes), with its calls. The strut step is `CSpringDamper::Step`
(0x52e090), described in `notes/physics/spring-damper.md`. The tyre step is `CScratch::Step`
(0x5360c0), described in `notes/physics/tyre.md`.

Reached wheel functions in the drive (`notes/progress/called.csv`): `M3DD30` and the
`E51Wheel` helpers listed in section 3. `M3EB80` (0x53eb80, the second wheel step) is not reached
in the drive that produced `called.csv`, so it is described from its source only.

## 1. Overview

`CWheel` is one wheel: its setup (`CDef`, `def = wheel+0x34`), its vertical strut
(`CSpringDamper`, `wheel+0xec4`), its tyre (`CTyre` container and `CScratch::Step`, `wheel+0xf00`
in the C++), the car it belongs to (`wheel+0x7f4`), and the outputs the car reads.

`M3DD30` (`E51Wheel::M3DD30`, the wheel's vtable slot 96) is one physics step of one wheel. Its
order, from the asm and the C++ (the C++ is 45%-55% in its own comments; the order below follows
the asm where they differ):

1. Refresh the strut travel `x = wheel+0x5bc` into the strut's range. The range is the strut's
   travel limits, `name+0x28 - 0.05` (low) and `name+0x38 + 0.02` (high), where `name` is the
   strut's `CBase` name. The refresh (`M3B880`, "travel-range clamp") does this at the start of the
   step (`RBR_E51_WheelD.cpp:408-412`).
2. Copy the wheel's frame vectors (`wheel+0x128..0x130` and `+0x11c..0x124`) through the car's
   `Xform6950` (`wheel+0x140`), `Xform68E0`, and the `s12r` hooks.
3. Forward speed: `wheel+0xa50 = car->Fwd6DB0(wheel+0xeb4)` (`:449`). This is the wheel's
   spin rate (see section 4, open question 1, and section 5, correction C1 in `tyre.md`).
4. Contact. If the ground-probe flag (`*(game+0x24)+0x160cc`, bit 0x10000) is clear, or
   `M3D520` (the ground probe) returns 0, the wheel is off the ground (`wheel+0xebc = 0`): clear the
   vectors and zero the tyre state (`SetState48(0)`, `SetStateField(0)`). Otherwise the wheel is on
   the ground (`wheel+0xebc = 1`) and the contact frame is set up.
5. Tyre step (on the ground): `CScratch::Step` on `wheel+0xf00`, with the spin (`wheel+0xa50`), the
   step `dt`, the strut's outputs and the contact vectors. The tyre's force and torque come back
   through its output pointers. The tyre noise signal is then emitted from `c4` (`:513`).
6. Strut integration (the vertical step). `CSpringDamper::Step(x, y, z)` gives three numbers:
   `o1` (spring and stop force), `o2` (high-speed breakpoint force), `o3` (damping coefficient).
   The new strut velocity is
   `v_new = (v + dt * (F_sum) * m_inv) / (1 + dt * o3 * m_inv)`, where `v = wheel+0x5cc`,
   `m_inv = def+0x16c` (the wheel's `InverseMass`, section 2), and `F_sum` is the sum of the
   force-like terms. `v_new` is clamped to ±100 (`:551`, asm 0x53e5ab-0x53e5d8). The new travel is
   `x_new = x + dt * v_new` (`:554`). The asm integrates exactly this form (section 4, C1 for the
   C++ difference).
7. Car outputs: the blended tyre outputs go to the car (the `MB560` calls after the tyre step), and the tyre
   accumulators `wheel+0xa60`..`0xa80` are updated for the next step.

`M3EB80` (the second step, from the E35 path) follows the same order without the steps the
comments mark as E35-specific (its source is `RBR_E51_WheelD.cpp:664`).

Call tree of the reached and referenced wheel functions (direct calls, the asm of `M3DD30`):
`CSpringDamper::Step` 0x52e090 (strut); `CScratch::Step` 0x5360c0 (tyre, `wheel+0xf00`);
`CCar::Fwd6DB0`, `CCar::Xform6950`, `CCar::Xform68E0` (car, not read); `M3D520` (ground probe,
not read); `M3B900` (not read); `Unk97a0` (a twin dispatch, `RBR_E50_WheelSim.cpp:183`);
`E51MultiTire::Emit` (tyre noise). `E47Solid::Step76` runs on the `wheel+0xd7c` path (not read).

## 2. Data

Offsets are from the wheel (`this` of `M3DD30`). `def` is `wheel+0x34`.

### 2a. CWheel (per-step state and outputs)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x34 | `*(int*)(o+0x34)` | def | pointer | - | the wheel's setup (CDef) | known (the code reads `def+0x41c`, `def+0x16c`) | `M3DD30` `:338` |
| 0x5bc | `*(float*)(o+0x5bc)` | strut_travel (x) | f32 | m | the strut's travel coordinate, clamped to the travel range each step | inferred (strong) | `M3DD30` `:408-412` (the clamp), `:554` (the update); spring-damper note: `x` |
| 0x5b0 | `((CBase*)(o+0x5b0))->SetDirty()` | (dirty flag of x) | CBase | - | the tweak dirty word of `x` | known (the call) | `M3DD30` |
| 0x5cc | `*(float*)(o+0x5cc)` | strut_velocity (v) | f32 | m/s | the strut's velocity, clamped to ±100 | inferred (strong) | `:551`, `:554`; spring-damper note: `z` |
| 0x7f4 | `*(int**)(o+0x7f4)` | car | pointer | - | the car this wheel belongs to | known (the calls on it) | `M3DD30` `:339` |
| 0xa50 | `*(float*)(o+0xa50)` | spin_rate (ω, the wheel's angular speed) | f32 | rad/s | returned by `car->Fwd6DB0(wheel index)`; the tyre multiplies it by the radius | inferred (strong, see open question 1) | `M3DD30` `:449`; `SolveDirs` and `ComputeSlip` use it as `R·ω` |
| 0xa60, 0xa70, 0xa80 | `o+0xa60`, `o+0xa70`, `o+0xa80` | contact vectors | 3-vector | m, m/s | vectors copied from the car transforms; `0xa70` is the input to `Step`'s `Blend4`; `0xa80` takes the negated `Blend4` result | guess | `M3DD30` `:441-462` |
| 0xaa0, 0xaa4 | `*(int*)(o+0xaa0)`, `o+0xaa4` | tyre inputs | f32 / pointer | - | passed to the tyre step as `c` (stored as `F[0xEA]`) and `o+0xaa4` as the next argument | guess | `M3DD30` `:503` |
| 0xbc4 .. 0xc0c | `o+0xc04`, `o+0xc08`, `o+0xc0c` | tyre output words | int | - | copied to the tyre noise signal `c4[4..6]` | guess | `M3DD30` `:510-513` |
| 0xd78 | `*(int*)(o+0xd78)` | tyre input (d) | f32 bits | - | the tyre step's `d` argument (stored as `F[0xF6]`) | guess | `M3DD30` `:503` |
| 0xd7c | `*(int*)(o+0xd7c)` | flag: E47 path | int | - | selects `E47Solid::Step76` for the contact vector | guess | `M3DD30` `:498` |
| 0xeb4 | `*(int*)(o+0xeb4)` | wheel_index | int | - | the car's index for this wheel (argument of `Fwd6DB0`) | inferred | `M3DD30` `:449`, `c4[0]` |
| 0xebc | `*(int*)(o+0xebc)` | on_ground | int | 0/1 | 1 when the ground probe found a contact | inferred (strong) | `M3DD30` `:456-470` |
| 0xec4 | `CSpringDamper` | strut | object | - | the vertical strut (its own note) | known (the call) | `M3DD30` `:538` |
| 0xf00 | `CTyre::CScratch` | tyre | object | - | the tyre step's `this` (section 4, open question 2) | inferred | `M3DD30` `:503` |

### 2b. CDef (`def = wheel+0x34`) fields used by the step

| offset | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x16c | inverse_mass | f32 | 1/kg | `m_inv` in the strut step (`v_new`); the LSP key `InverseMass` (0.04 on the car wheel blocks, so about 25 kg) | known (the LSP key), inferred (the use) | `physics-lsp.md`: `PhysicsEngine/Wheel/CAR_WHEEL_*/InverseMass:float[0.04]`; `M3DD30` `:545` (`/ (f16c·l274·dt + 1)`) |
| 0x41c | strut_static_offset | f32 | m | added to `x` to give the strut coordinate `y = def+0x41c + x` passed to `Step` | inferred (spring-damper note: "a static reference") | `M3DD30` `:538` |
| 0x43c, 0x44c | (car index pair) | int | - | the arguments of the car call `0x4e6730` whose result is a force term (`F_sum`) | guess | `M3DD30` asm 0x53e54c-0x53e560 |

The exe holds the name string `WheelInertia` (0x73b3d0). The vertical step uses `def+0x16c` as an
inverse mass, and the LSP calls the same wheel field `InverseMass`. The name `WheelInertia` may be
the spin's inertia, a different field.

### 2c. Strut travel range

The travel clamp reads the strut's `CBase` name at `wheel+0xec4` (`name+0x28`, `name+0x38`). These are
the strut's setup range limits. The LSP `SpringDamper` keys with ranges (`BumpRubberStiffness`,
`InnerBumpRubberPos`, `OuterBumpRubberPos`) belong to the same block (the spring-damper note holds
the mapping).

## 3. Functions

| address | current name | proposed name | what it computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x53dd30 | E51Wheel::M3DD30 | wheel_step (vtable 96) | the per-step wheel update (section 1) | asm verified for the order and the strut step; equations partial | `RBR_E51_WheelD.cpp:334-661`; asm |
| 0x53eb80 | E51Wheel::M3EB80 | wheel_step_e35 | the same step on the E35 path | unverified | `RBR_E51_WheelD.cpp:664` |
| 0x53b880 | E51Wheel::M3B880 | refresh_travel | the refresh: sim overlap, tyre bias clear, travel-range clamp | partial (C++ comment; the clamp matches the asm of `M3DD30`) | `RBR_E51_WheelB.cpp:79` |
| 0x53d520 | E51Wheel::M3D520 | ground_probe | three ray-cast blocks with best-hit tracking, then the blend and output | unverified (C++ comment: 55%) | `RBR_E51_WheelD.cpp:39` |
| 0x53b900 | E51Wheel::M3B900 | set_travel_derived (proposed) | called with the clamped travel `x`; the body is not read | unverified | `RBR_E51_WheelC.cpp:14` |
| 0x53d460 | E51Wheel::M3D460 | setup_sub_update | setup-driven sub-update called from the big sim | unverified | `RBR_E51_WheelB.cpp:203` |
| 0x53d230 | E51Wheel::M3D230 | (wheel angle) | "ratio is 2·pi over +0xd8": two sine pairs and two interpolated values; looks like the wheel's visual rotation, not the physics | unverified | `RBR_E51_WheelB.cpp:149` |
| 0x53d030 | E51Wheel::M3D030 | per_frame_update | per-frame update from `CCar::Update9C70` | unverified | `RBR_E51_WheelB.cpp:131` |
| 0x53cf30 | E51Wheel::CopyCF30 | seed_copy | seed copy from the car update path | unverified | `RBR_E51_WheelB.cpp:98` |
| 0x53ac50, 0x53aef0 | M3AC50, M3AEF0 | (wheel setters) | `M3AC50(v)` sets a wheel value from `v` | unverified | `RBR_E51_WheelA.cpp:145`, `:222` |
| 0x53a9e0 | M3A9E0 | (sin and cos block) | sin and cos through libm, with the x87 ST0 handoff the C++ can't reproduce | unverified | `RBR_E51_WheelA.cpp:69` |
| 0x53a8a0 | M3A8A0 | xform_caller | the car `Xform` caller: keeps the `+0x44` object when its `+0x60` flags carry bit 3, then pushes eight def and source pairs through the car `Xform` calls | partial (C++ comment) | `RBR_E51_WheelA.cpp:41` |
| 0x53b610 | E51Wheel::M3B610 | cdef_factory | news a 0x980-byte CDef and constructs it from the setup argument | known (the C++ comment) | `RBR_E51_WheelA.cpp:448` |
| 0x53d1c0 | E51Wheel::M3D1C0 | cdef_vf50_factory | news 0x1420 bytes for the wheel and runs the extended constructor | known (the C++ comment) | `RBR_E51_WheelA.cpp:461` |
| 0x539090 (slot 8) | CWheel::Vf20 family | (dispatch) | clears two blocks, re-arms slot 16, forwards | partial (C++ comment) | `RBR_E50_Wheel.cpp:443` |
| 0x539a20 (slot 22) | CWheel::Hook | profiler dispatch | profiler-scoped dispatch to slot 96 (the step) | partial (C++ comment) | `RBR_E50_Wheel.cpp:494` |
| 0x539a90 (slot 23) | CWheel::Vf5C | (forwarder) | forwards two sub-calls through the `+0x44` object, clears two blocks, zeroes `+0xa10` | partial (C++ comment) | `RBR_E50_Wheel.cpp:513` |
| 0x5390d0 | CWheel::Copy100 | copy_block | copies the `+0x100` block (or zeros) to the output | partial (C++ comment) | `RBR_E50_WheelSim.cpp:19` |
| 0x539180 | CWheel::Blend4 | tyre_scaled_blend | two tyre-scaled blends into two output vectors, normalised into four wheel blocks | partial (C++ comment: scalar blend in place of mulps) | `RBR_E50_WheelSim.cpp:43` |
| 0x539290 | CWheel::Unk9290 | dot_frame_pick | a dot-versus-frame pick with a min-abs select, a sqrt clamp and a normalise | unverified | `RBR_E50_WheelSim.cpp:66` |
| 0x539660 | CWheel::Unk9660 | uint_select | a hash and select over `n` (x87 code); used by the wheel's random pick (for tyre noise), not the physics | partial (asm and C++ read; judged not physics) | `RBR_E50_X87.cpp` |
| 0x5396e0 | CWheel::Unk96e0 | table_lerp_pair | table lerp of the `+0xd70`/`+0xd74` pair | unverified | `RBR_E50_WheelSim.cpp:163` |
| 0x5397a0 | CWheel::Unk97a0 | twin_dispatch | `Unk94d0` dispatch, gated by the `+0xd04`/`+0xc94` flags (called by `M3DD30` with the tyre outputs) | unverified | `RBR_E50_WheelSim.cpp:183` |
| 0x539900 | CWheel::Unk9900 | state_blend | E45 state blend via E49, then the E50 blend into `+0xaa4`, plus the `+0xc04`/`+0xc08`/`+0xc0c` stores | verified over the 12,084-step tanner-corolla drive | `RBR_E50_WheelSim.cpp:214`; bisect `p2w-un9900-postfix` (40,779 calls) |
| 0x539bb0 | CWheel::Unk9bb0 | debug_dump | a gated debug dump (matrix chain through the `+0x7f4` car); not physics | not physics | `RBR_E50_WheelSim.cpp:262` |
| 0x53a860 | CWheel::UnkA860 | tyre_tail_setup | damper and tyre tail setup | unverified | `RBR_E50_WheelSim.cpp:554` |
| 0x53f230 | CWheel::Vf28 | finish | tyre noise and the `M3B880` finish; `ResetOutputs`, `EmitNoise` (50%) | partial (C++ comment) | `RBR_E51_WheelB.cpp:250` |

## 4. Open questions

1. **Spin rate `ω`.** `wheel+0xa50` is set from `car->Fwd6DB0(wheel index)`, and the tyre code uses
   the same value as `R·ω` (the tyre's `ComputeSlip` and `SolveDirs`). The units agree with `ω` in
   rad/s. The name `Fwd6DB0` says "forward", so the value may be the rolling speed in m/s. If it
   were m/s, `R·ω` would have the wrong dimension, so the tyre code and the car function disagree
   on the unit, or the car returns `ω`. Not resolved here: `CCar::Fwd6DB0` is not read.
2. **The tyre's `this`.** The C++ passes `wheel+0xf00` to `CTyre::CScratch::Step`, but the harness
   README places the tyre scratch at `CWheel+0x800` (from the peek probes, which read real values
   at `+0xc58`). If the scratch is at `+0x800` inside a container at `+0xf00`, the `F[i]` offsets in
   `tyre.md` are relative to the container's scratch, not to `+0xf00`. Not resolved.
3. **The fourth force term.** The asm sums four force-like terms in the strut step: the car call
   result (`0x4e6730`, with `def+0x43c`, `+0x44c`), two strut outputs (`[esp+0x24]`, `[esp+0x28]`), and
   `[esp+0x34]` (asm 0x53e54c-0x53e57b). The spring-damper note's equation has three terms (`g + o1 +
   o2`). The fourth is `[esp+0x34]`, which its note does not mention. The owner of the spring-damper
   note should check it.
4. **`0x4e6730`** (the car call that gives a force term, with the two `def` pairs). Not read.
5. **The ground probe** (`M3D520`). Its ray-cast blocks decide `on_ground` and the contact vectors. Not read.
6. **`M3EB80`'s difference from `M3DD30`.** Its source has the same order; the asm was not read.
7. **Where the tyre's torque reaches the spin.** The tyre returns force and torque outputs, but the
   spin update that uses them (`ω_new = ω + dt·(T_drive - T_tyre)/I`) was not found in `M3DD30`. It
   is probably in `CScratchSim`/VCU (E50) or in the car's drive code. Not established.

## 5. Suspect code

**W1. `E51Wheel::M3DD30`, `RBR_E51_WheelD.cpp:525, 545, 556, 559, 588` and the other `(int)`
casts in the function (11 in all): float and pointer round trips the original does not have.**
The asm of `M3DD30` contains no integer-to-float or float-to-integer conversion
(`cvtsi2ss`/`cvttss2si`: 0 occurrences in the 3664 bytes). The C++ casts `(float)(int)&v200[0]`
(address of a stack array converted to a float), `(float)(int)n26c` (a `float*` converted to an int
and then to a float), and `(float)(int)p268b` have no counterpart in the original. The values the
original uses are plain floats. So the C++ computes different numbers from the original, and the
vertical step `f6` (C++ `:545`) is wrong as written. The asm for the same step is in section 1.6.
This is the strongest suspect in the wheel code: the vertical dynamics, the travel update and the
tyre-output blends all pass through these lines.

**W2. `E51Wheel::M3DD30`, `RBR_E51_WheelD.cpp:545-547` (the step's force sum).** The C++ sum is
`(int(n26c) + l280 + s27c + int(n26c))`: the first term repeated, and the fourth term from the asm
(`[esp+0x34]`, 0x53e57b) missing. The asm sums `T(0x38) + [esp+0x24] + [esp+0x28] + [esp+0x34]`, then
multiplies by `m_inv·dt` (section 4, open question 3). Both the term set and the first term differ.

**W3. `E51Wheel::M3DD30`, `RBR_E51_WheelD.cpp:503`: a float passed as `void*`.** The tyre step's `a`
parameter receives `*(int*)(o+0xa50)`, the spin's bit pattern passed through a pointer-sized integer,
and `c`, `d` are the same. The types are the source of the confusion (the tyre code reads `a` as a
float through `this+0x378`). The bits are correct, but the types hide the meaning. Propose `float`
parameters.

**W4. `E51Wheel::M3DD30`, `RBR_E51_WheelD.cpp:408-412`: the travel clamp is correct.** The asm (0x53dd96
and onwards) and the C++ agree. No change; listed because section 1.1 depends on it.

**W5. `E51Wheel::M3DD30`, `RBR_E51_WheelD.cpp:554`, `*(float*)(o + 0x5bc) = f6 * dt + f8;`.** `f8` at
that point is the clamped travel from step 1, which is correct only if `f8` was not overwritten by
the intervening `f8 = f6 - (float)(int)n26c` (line 556). The asm stores `x_new` through `xmm0` computed
from the travel (0x53e5ed), so the C++ reuses `f8` for two values. Check against the asm before
fixing; the C++ may be right by accident.
