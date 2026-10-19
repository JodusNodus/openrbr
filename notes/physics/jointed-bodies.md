# Jointed bodies (E44 CHingedRigidBody, E45 CMountedRigidBody, CPassenger): physics semantics

Pass 4 of the physics semantics brief, jointed-body part. Notes only; no source edits.
Confidence: `known` (exe/LSP name, or the equation is unambiguous), `inferred`, `guess`.
Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump`. `-` in the status column = no row in
`notes/progress/gameplay-functions.csv` (not harness-verified). reccmp % from `base/RBR.txt`.

Related: `rigid-body.md` (the CSolid base, which the hinged body extends), `entities.md`
(the CEntity base), `car-body.md` (the car that owns these bodies).

## 1. Overview

Three kinds of attached body hang off a car:

- **Hinged bodies** (`CHingedRigidBody : E47Solid`, 0x660 bytes, vtable 0x73d23c): doors,
  bonnet, exhaust pipe. The LSP block is `HingedBody/CAR_*` (`physics-lsp.md:206-209`), with
  `HingeMinAngle`, `HingeMaxAngle`, `HingeDefaultAngle` (radians), `InertiaHingeAxis`,
  `FaceArea[2]`, `BodyId[2]`, `BreakChain`, `InverseMass`, the sleep keys. A hinged body is a
  rigid body with one rotational degree of freedom about a hinge axis; the car is its owner.
- **Mounted bodies** (`CMountedRigidBody : CEntity`, 0x590 bytes, vtable 0x73e62c): bumpers and
  wing (`MountedBody/CAR_*`, `physics-lsp.md:210-212`): `BodyId`, `vecBasePoint_PRF` (the
  mount point in the car's PRF frame), `vecLocalInverseInertiaDiagonal`, sleep keys. A mounted
  body is rigidly attached: no hinge, no own DOF in the reached code.
- **Passengers** (`CPassenger`, ~0x44+ bytes, vtable 0x73e890): a damped spring that holds a
  point (the passenger's "seat" state) relative to its host (`+0x40`, the car). The LSP has no
  passenger block in the physics files read here; the passenger state is built from code.

Place in the step: the hinged body is a `CSolid` in `list450` and runs `Vf25x`/`Vf27x` like
any body (`rigid-body.md` 1). Its extra work is in its own `Sim*` functions, reached from
`Vf25x`, `SimEAD0` (the hinge solve) and the emit. The mounted body's `StepBig` (0x522100) is in
the step list as an E45 wall (`step.md` 1c). The passenger `Step` (0x523160) is called from the
car side (`Step` is not in `step.md`; the caller is open, Open 4).

Call tree (reached functions; addresses; reccmp % and status):

```
CHingedRigidBody::SimEAD0()                     0x51ead0  43.0%  -    hinge solve (2c)
  CHingedRigidBody::SimE1D0()                   0x51e1d0  48.2%  -    (not read here)
  CHingedRigidBody::SimCFF0()                   0x51cff0  54.4%  -    friction/slip term (2d) SUSPECT 5a
  CHingedRigidBody::SimEA80()                   0x51ea80  54.9%  verified
    CHingedRigidBody::SimCF10(k)                0x51cf10  32.7%  verified   per-hinge limit (2c)
CHingedRigidBody::SimFB40()                     0x51fb40  40.6%  -    state emit (2e)
  CHingedRigidBody::SimDB40()                   0x51db40  30.6%  -    (not read here)
  CSignalManager::CMultiSignal1::Emit           0x51f640/0x51f700/0x51f7c0  39.5%  -
CHingedRigidBody::SimD860(out)                  0x51d860  54.9%  -    matrix row project + pair walk
CHingedRigidBody::SimC960()                     0x51c960  36.4%  verified   rotation block (2b)
CHingedRigidBody::CState::CState(a,b,c)         0x51cc90  44.7%  -
CHingedRigidBody::CDef::CDef(outer)             0x51e680  50.3%  -    (E44CDefShell::Ctor)
CHingedRigidBody(def, a, b, flags)              0x51e9a0  68.9%  -    (E44HingedShell::Ctor)
E44_BlockCtor                                   0x51ca40  56.8%  -    Block60 ctor (2a)
CMountedRigidBody::CDef::CtorA(arg)             0x521480  68.8%  -
CMountedRigidBody::CState::Ctor                 0x5215a0  59.7%  -
CMountedRigidBody::CtorBig(a, b, c, d)          0x521c30  59.8%  -
CMountedRigidBody::CDef::Vf50                   0x521d20  49.1%  -
CMountedRigidBody::NewDef                       0x521b50  43.2%  verified
CPassenger::Ctor(a, b, c)                       0x5230d0  34.8%  -
CPassenger::Step(dt)                            0x523160  48.9%  -    (2f)
  E43PairWalk::MB560                            0x51b560  100%  -    transform
  E45PSmalls::M29E0(dt)                         0x5229e0  14.3%  -    damped spring (2g)
  E45PSmalls::M2BC0(dt)                         0x522bc0  25.8%  -
E45_P3210 (incar shake emit)                    0x523210  42.3%  -
CPassenger::CDef::CtorA / CtorB                 0x522450 / 0x5224a0  58.3% / 66.7%
CPassenger::CState::CtorA / CtorB               0x522500 / 0x522550  -  / 74.2%
E45PhysBody::M4250 (bad)                        0x524250  55.2%  bad
```

## 2. Data

### 2a. CHingedRigidBody::CDef (0x510 bytes, vtable 0x73d1ac)

The base is the CSolid def (`rigid-body.md` 2a: `InverseMass` at `+0x16c`, `vecLocalInverseInertiaDiagonal` at
`+0x130`, `bSpecial`, `AngularThresholdVelocity`, sleep keys). The hinge fields follow.

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x1f0 + i*0x60 (i = 0, 1) | `m_b1f0[i]` (Block60) | hinge_axis_i | struct | - | one block per hinge limit (2 blocks) | inferred (two blocks, SimEA80 loop) | `RBR_E44_Hinged.h`; `SimEA80` (`off < 0xc0`, stride 0x60) |
| +0x00 (word 0x1fc + i*0x60) | `m_u00` (CUnsigned, value +0xc) | limit_mode | u32 | enum | `== 2` makes the limit active (SimEA80 sums SimCF10 for it) | inferred | `SimEA80` `def+off+0x1fc == 2` |
| +0x10 (value 0x20c) | `m_r10` | (unused here) | f32 | - | not read by SimCF10/SimEAD0 | unknown | - |
| +0x20 (value 0x21c) | `m_r20` | limit_stop_coef | f32 | N m / rad? | multiplies the limit penetration `f` in SimCF10 | inferred | `SimCF10` `-(f*def[+0x2c])` |
| +0x30 (value 0x22c) | `m_r30` | limit_damp_coef | f32 | - | multiplies the hinge rate `this+0x40c` in SimCF10 | inferred | `SimCF10` `-(this+0x40c)*def[+0x3c]` |
| +0x40 (value 0x23c) | `m_r40` | limit_max_step | f32 | rad | clamp on the per-step angle correction in SimCF10 (`w > f` branch) | inferred | `SimCF10` |
| +0x50 (value 0x24c) | `m_r50` | limit_angle | f32 | rad | reference angle: the limit is `r50 +/- ...`; compared with the hinge state `this+0x3e8+4i` | inferred | `SimCF10` `v = a - def[+0x24c]`, `e+0x5c + lim` |
| 0x2b0 (CVector object, data 0x2c0) | `m_v2b0` | hinge_def_vec_2c0 | vec3 | - | read as the scale vector `def+0x2c0..0x2c8` in SimCFF0 and SimEAD0's output | guess | `SimCFF0` (`def+0x2c0`, `0x2c4`, `0x2c8`) |
| 0x2d0 (CVector, data 0x2e0) | `m_v2d0` | (not read) | vec3 | - | - | unknown | - |
| 0x2f0 (CVector, data 0x300) | `m_v2f0` | hinge_pivot_def | vec3 | m | used by SimFB40 as the hinge pivot (`def+0x300`) | inferred | `SimFB40` else-branch `def+0x2f0 != 0 ? def+0x300` |
| 0x320 / 0x324 / 0x328 (floats) | (CVector at 0x310, data 0x320) | hinge_anchor | vec3 | m | subtracted from the body position (`this+0x140..0x148`) in SimCFF0 | inferred | `SimCFF0` `this+0x140 - def+0x320` |
| 0x34c | (float) | friction_coef | f32 | - | multiplies the slip term in SimCFF0 | inferred | `SimCFF0` `*def+0x34c` |
| 0x350 (CVector, data 0x360) | `m_v350` | hinge_axis | vec3 | - | the hinge axis used as `ctx` in `SimCFF0` (MB560 on `def+0x360`) | inferred | `SimCFF0` `ctx = def+0x350 ? def+0x360` |
| 0x370, 0x390, 0x3b0 (CVector, data +0x10) | `m_v370/v390/v3b0` | (partly used) | vec3 | - | `def+0x3b0` / `def+0x3c0` in SimFB40 (ComposeA ctx) | guess | `SimFB40` |
| 0x3c0 | - | - | vec3 | - | `Scale(-1)` in SimEAD0 | guess | `SimEAD0` |
| 0x3d0..0x3dc | (4 floats) | hinge_def_mat | vec | - | the vector `c` in SimEAD0's 3x3 product (`_mm_loadu_ps(def+0x3d0)`) | guess | `SimEAD0` |
| 0x3e0..0x400 (9 floats) | (floats) | hinge_mat3 | mat3 | - | 3x3 matrix in SimEAD0 (`d[0..8]` built from `def+0x3e0..0x400`) | guess | `SimEAD0` |
| 0x440..0x4d0 | `m_r470..m_r4d0` (CReal) | hinge_params | f32 | - | `0x49c` (reference angle in SimEAD0 and SimFB40), `0x4bc` (`b0`, guarded to 0.1 when 0), `0x4cc`, `0x4dc` in SimEAD0 | inferred | `SimEAD0`, `SimFB40` |
| 0x4e0 / 0x4f0 / 0x500 | `m_u4e0/u4f0/u500` (CUnsigned) | - | u32 | - | `0x4fc` (raw word in the CSigHingedBody payload) | inferred | `SimFB40` |

Block60 (`E44_BlockCtor`, 0x51ca40): a CUnsigned (+0x00) and five CReal (+0x10..+0x50). Value
word of each member at +0x0c (the `NTweak` convention, `rigid-body.md` 2a). Field meanings
above are inferred from use in `SimCF10`/`SimEA80`; the LSP has no key for them in the blocks
read, so the names are ours (Open 1).

### 2b. CHingedRigidBody (0x660 bytes; vtable 0x73d23c)

The header (`RBR_E44_Hinged.h`) says `def` at `+0x3d0`. The code does not use it that way:
`SimCFF0`, `SimFB40` and `SimEAD0` read `this+0x3d0` as the **owner car** (`CCar*`): they pass
it to `StateAtb()` (E34 `0x4e65b0`, `E44_CarStateAtb`) and to `MB560`. The CSolid def is at
`this+0x34` (`rigid-body.md` 2b). Suspect 5b.

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x34 | `m_def` (CSolid) | def | ptr | - | `CHingedRigidBody::CDef` | known | `rigid-body.md` 2b |
| 0x140..0x148 | (CSolid position) | position | vec3 | m | body position (SimCFF0 subtracts the anchor) | known (CSolid) | `rigid-body.md` 2b |
| 0x180 / 0x190, 0x1b0 / 0x1c0 | (CSolid velocities) | - | vec3 | rad/s, m/s | `SimFB40` emits `+0x190` and `+0x1c0` | known (CSolid) | `rigid-body.md` 2b |
| 0x0f0 / 0x100 | (CSolid orientation) | orientation | quat | - | `SimFB40` emits `+0x100` | known (CSolid) | `rigid-body.md` 2b |
| 0x3d0 | `m_def3d0` (comment: def) | owner | ptr | - | the owning car (`CCar*`): `StateAtb()` and `MB560` take it. Not a def (5b) | inferred (asm 0x51d00e-0x51d02a, 0x51d022-0x51d040) | `SimCFF0` |
| 0x3d4 (0x3c) | `m_state3d4` (CState) | state | struct | - | CReal at +0x04, floats at +0x14/+0x18, CReal +0x1c, +0x2c | known (layout) | `RBR_E44_Hinged.h` |
| 0x3e8 / 0x3ec | (array) | hinge_angle[i] | f32 | rad | per-hinge state `a` in SimCF10 (`this+0x3e8+4*idx`) | inferred | `SimCF10` |
| 0x3fc | (float) | hinge_angle | f32 | rad | the hinge angle: `lim` in SimCF10; the angle of the rotation block (SimC960) | inferred (SimC960 `cos`/`sin` of it) | `SimC960`, `SimCF10`, `SimFB40` |
| 0x40c | (float) | hinge_rate | f32 | rad/s | the hinge angular rate: `xc` in SimEAD0, the damping term in SimCF10 | inferred | `SimCF10`, `SimEAD0` |
| 0x410 (scratch, 0x150) | `m_scratch410` | scratch | struct | - | CSolid scratch (E47 compatible) | known | `RBR_E44_Hinged.h` |
| 0x598..0x5b8 (3x3, `[1 0 0; 0 c s; 0 -s c]`) | (floats) | rot_x | mat3 | - | rotation block about X by `hinge_angle` (SimC960) | known (eq) | `SimC960` |
| 0x5a8..0x5b8 | - | - | - | - | c, s, -s, c entries | known (eq) | `SimC960` |
| 0x600 (E01Vec3) | (pivot) | pivot_vec | vec3 | m | the lever arm used in `Cross(this+0x600, cr)` in SimEAD0 | inferred | `SimEAD0` |
| 0x620 / 0x624 | (floats) | hinge_acc_offset / hinge_acc | f32 | rad/s^2 | SimEAD0 writes `0x624 = b/b0 - 0x620` | inferred | `SimEAD0` (last line) |
| 0x650 (pointer) | - | - | ptr | - | `*(this+0x650)` read by SimEAD0 (early return when nonzero) and SimFB40 (`bool` payload) | inferred | `SimEAD0`, `SimFB40` |

### 2c. CMountedRigidBody (0x590 bytes)

| offset | proposed name | meaning | confidence | evidence |
| --- | --- | --- | --- | --- |
| 0x3d0 | owner | the car (E45 header: "owner +0x3d0") | known (exe-plan notes) | `exe-plan.md:850` |
| 0x3d4 (0x34) | state | CState: CReal at +0x04/+0x14/+0x24 | known (layout) | `RBR_E45_Material.h:249-265` |
| 0x410 | scratch | CScratch (vtable 0x73e61c) | known | header |
| 0x584 | - | a word (E45 header, not read) | unknown | header |

### 2d. CPassenger (`this`)

| offset | proposed name | meaning | confidence | evidence |
| --- | --- | --- | --- | --- |
| 0x00 | vtable 0x73e890 | | known | `CPassenger::Ctor` |
| 0x10 (`a`) | - | passed to the scratch Vf04 when nonzero | known | `CPassenger::Ctor` |
| 0x14 (`b`) | state | the passenger state block (`host` in M29E0/M2700): `+0x20..0x2c` velocity-like, `+0x40..0x4c` position-like, `+0x50..0x5c` and `+0x60..0x6c` accumulators, `+0x70..0x7c` and `+0x80..0x8c` and `+0x90..0x9c` and `+0xa0..0xbc` targets/outputs | inferred (from M29E0 and E45_P3210) | `M29E0`, `E45_P3210` payload |
| 0x20 | scratch | CScratch (vtable 0x73e884) | known | `Ctor` |
| 0x30 | local | written by `MB560` in `Step` (the passenger's position in the host frame) | inferred | `CPassenger::Step` |
| 0x40 (`c`) | host | the car (`CCar*`) whose frame and position the passenger uses (`+0x230..0x238`, `+0x20..0x48`) | inferred | `Step`, `M29E0` |

## 3. Functions

| address | current name | proposed name | what it computes | confidence | status and evidence |
| --- | --- | --- | --- | --- | --- |
| 0x51c960 | `SimC960` | hinge_rot_block | `s = sin(θ)`, `c = cos(θ)` (θ = `hinge_angle`); writes the rotation block `[1 0 0; 0 c s; 0 -s c]` at `0x598..0x5b8` | known (eq) | verified; 36.4%; asm read (rotation block) |
| 0x51cf10 | `SimCF10(idx)` | hinge_limit_i | angle-limit response for hinge `idx`: `v = a_i - r50_i`; if `v > lim` (lim = θ): `f = lim - v`, clamp to `-r40_i`, move `a_i`; else check the lower side (`a_i + r50_i` vs lim); return `-(f*r20_i) - rate*r30_i` when `f != 0`, else 0 | inferred | verified; 32.7%. Meaning from field use |
| 0x51ea80 | `SimEA80()` | hinge_limit_sum | sum of `SimCF10(k)` over hinges with `limit_mode == 2` | inferred | verified; 54.9% |
| 0x51cff0 | `SimCFF0()` | hinge_slip_term | `MB560(axis = def+0x360, owner, hit)`; `car.state.v[0xc10]` copied; `f1 = car.state[0xc00]`; `f4 = abs(((v70.y*hit.y + v70.z*hit.z) + v70.x*hit.x) * def[0x34c])`; `v20 = v70 * (f4*f1*f1*0.5)`; `l50 = [this+0x140 - anchor] × v20`; returns `(l[5]*def[0x2c4] + l[6]*def[0x2c8]) + l[4]*def[0x2c0]` | inferred | finite and NaN forced records returned the same x87 value as the original; reccmp 53.26% |
| 0x51efe0 | `SimEFE0()` | hinge_frame_update | copies the definition's angle block, sets the angle on the clear-flag path, builds two vectors, composes the body frame, runs `SimD160`/`SimD860`, then updates the grid edge and limit words | asm-checked; forced fourth-call replay | drive bisect's frame omits this separate hinged body; direct replay after fixes matches (see 5f) |
| 0x51ead0 | `SimEAD0()` | hinge_solve | early return if `*(this+0x650)`; if no owner: `Vf25x`; else build the 3x3 product `M·c` (`def+0x3d0`, `def+0x3e0..0x400`), `m × Cross(pivot)`, `dot = cr·pf` (`pf = def+0x2b0`), `fsum = -dot*f11 + (def[0x49c]-this[0x3fc])*def[0x4cc] - def[0x4dc]*rate + SimCFF0() + SimEA80()`, the sign/clamp branch on `rate`, optional Vf81x term, then `this+0x624 = b4/b0 - this+0x620` | inferred | `-`; 43.0%. Not asm-checked; see 5c |
| 0x51fb40 | `SimFB40()` | hinged_emit | `SimDB40()`; emit `CSigSolidGeneralState` (22 words: id, pivot vec, quat, lin vel, ang vel, 1.0, 0); emit `CSigHingedBody` (raw `def[0x4fc]`, `*(this+0x650)==0` as int, `hinge_angle - def[0x49c]`); `E43Vf28()` | inferred (payload layout from code) | `-`; 40.6%. Payload layout: 5d |
| 0x521c30 | `CMountedRigidBody::CtorBig` | | | | `-`, 59.8% |
| 0x5230d0 | `CPassenger::Ctor(a, b, c)` | passenger_init | vtable 0x73e890; `+0x10 = a`, `+0x14 = b`, `+0x40 = c`; if `a`: scratch `Vf04` | known (asm shape; C++ read) | `-`; 34.8% (`noinline`, C++ shape) |
| 0x523160 | `CPassenger::Step(dt)` | passenger_step | `g = CDef gravity (+0x160b0)`; `t = g + host[0x230..0x238]` (x and z lanes; y as in code); `pos_host_frame = MB560(t, host, this+0x30)`; `M29E0(dt)`; `M2BC0(dt)` | inferred | `-`; 48.9%. See 5d |
| 0x5229e0 | `E45PSmalls::M29E0(dt)` | passenger_spring_a | `F_s = 30000 ∘ (host[0x70..] - host[0x90..])` (stiffness, per lane); `F_d = 900 ∘ (host[0x20..] - host[0x40..])` (damping); `v += (10·this[0x30..] + F_s + F_d) * (-0.1·dt)`; `x += v·dt` | inferred (structure; the `30000`, `900`, `-0.1`, `10` constants are the C++ literal values) | `-`; 14.3%. The C++ is an approximation (its comment: "stack layout approximate") |
| 0x522700 | `E45PSmalls::M2700(dt)` | passenger_spring_b | same form with `host+0xa0`/`0x50` and constants 10000 (stiffness) and 50 | inferred | `-`; not reached in the report list (called from M2870 family) |
| 0x522bc0 | `E45PSmalls::M2BC0(dt)` | passenger_step2 | longer clamp chain (not read in full) | unknown | `-`; 25.8% |
| 0x523210 | `E45_P3210(self)` | incar_shake_emit | clears a 6-word payload, copies 20 words from the host (`+0x70..0xbc`), `CIncarShakeSignal::Emit` | inferred | `-`; 42.3% |
| 0x524250 | `E45PhysBody::M4250` | - | not read | - | **bad** (harness); 55.2% |

## 4. Open questions

1. **Hinge field names.** The Block60 values (`r20`, `r30`, `r40`, `r50`) and `limit_mode`
   are inferred from use in `SimCF10`/`SimEA80`. The LSP's `HingeMinAngle`/`HingeMaxAngle`/
   `HingeDefaultAngle` are angles in radians (door `HingeMinAngle -2.1`, `Max 0.01`). Which
   block member is which angle? Settles it: the `NTweak` key names bound to the Block60 members
   (the `E44` ctor binds none; find the loader that names them), or a peek of `def+0x24c`
   against the loaded `HingeMinAngle`.
2. **Owner or def at `+0x3d0`.** Settled by the asm (2b): owner. The header comment is wrong. Does
   the owner also give the hinge's car-side frame (`MB560` argument)?
3. **Units of `r20`/`r30`.** The `SimCF10` result is a torque-like term. Its unit depends on `r20`
   (stiffness-like?) and `r30` (damping-like). Settles it: the LSP `HingedBody` keys for
   stiffness, if any; none appear in the block listed in `physics-lsp.md`.
4. **Who calls `CPassenger::Step`.** Not in `step.md`. It uses `g_1659b70+0x24` (the CDef) and
   `host` = the car. Settles it: the `CPassenger` owner's call site (E34 car code).
5. **x87 return.** `SimCF10` and `SimEA80` return a float through `st0`; whether their C++ reproduces
   the original's extended-precision sequence is not checked here. `SimCFF0`'s forced finite and NaN
   cases match the original's x87 return (5a).
5. **`SimE1D0`, `SimDB40`, `SimD860`, `SimD980`.** Called by `SimEAD0`/`SimFB40`; read only at the
   call sites. Not analysed here.

## 5. Suspect code

### 5a. `SimCFF0` grouping and NaN sign (resolved, asm-confirmed)

- Previous C++ grouped the dot products as x+z+y and returned x+z+y. It now groups the dot product
  as `(y*c1 + z*c2) + x*c0`, then returns `(cross1*c1 + cross2*c2) + cross0*c0` to match the asm.
- Coefficients: no difference. In both the C++ and the asm, the product that uses `def[0x2c0]` is
  paired with cross-product component 0 (`l50[4]`, asm `[S+0x40]`), `def[0x2c4]` with component 1
  (`l50[5]`, asm `[S+0x44]`), and `def[0x2c8]` with component 2 (`l50[6]`, asm `[S+0x48]`). The
  asm loads `[S+0x40]` after `popl %edi` (`0x51d14b`) moved `esp` by 4, so `0x3c(%esp)` is `S+0x40`.
- Grouping: the asm sums the slip dot product as `(y*c1 + z*c2) + x*c0`. The C++ now preserves
  that source order. The returned torque projection is also ordered `(cross1*c1 + cross2*c2) +
  cross0*c0`.
- NaN: the asm's branch (`ja`) negates when the value is unordered; `if (!(f4 > 0.0f))` now does the
  same, while the former `f4 <= 0.0f` did not.
- Forced replay exercised both finite non-integral dot products and a NaN dot product; the original
  and transplant returned identical x87 values. The reference drive's 40 calls still return 0.0.

### 5b. `CHingedRigidBody.h` names `+0x3d0` as the def; it is the owner car (asm-confirmed)

- C++: `src/rbr/RBR_E44_Hinged.h` (`m_def3d0`, comment "def@0x3d0" in the header comment).
- What the asm does: `SimCFF0` (`0x51d00e`-`0x51d02a`) loads `0x3d0(%esi)` and passes it to
  `MB560` and to `E34 0x4e65b0` (`StateAtb`, `E44_CarStateAtb`); the def is `0x34(%esi)`. The
  C++ code uses the same pointers (`(CCar*)this->+0x3d0`), so behaviour is right; the name
  `m_def3d0` and the header comment are wrong. Rename to `owner` (naming only).

### 5c. `SimEAD0` reads four lanes from a nine-float array (inferred, not asm-checked)

- C++: `src/rbr/RBR_E44_SimB.cpp` (`SimEAD0`): `__m128 s2 = _mm_loadu_ps(d + 6);` reads `d[6..9]`,
  but `d` has nine elements (`d[0..8]`).
- Why it matters: lane 3 (`d[9]`) is read and discarded (`m` has four lanes, only `m[0..2]`
  feed `Cross`). It is an out-of-bounds read on the stack; harmless unless it faults or the
  lane is used. Not asm-checked (the asm was not read for SimEAD0). Route to the E44 owner with
  the asm check.

### 5d. Passenger constants differ between M2700/M2870 and M29E0 (not verified)

- C++: `src/rbr/RBR_E45_PassengerB.cpp:17-115`: `M2700`/`M2870` add `k4` (50.0) as the
  z-lane term (`(f5 + k4) * k`), while `M29E0` uses the fourth lane of its product
  (`(f5 + l40[3]) * k`, where `l40[3]` is `30000 * 0.0`, i.e. 0).
- Why it may be wrong: the two families have the same structure. In M2700 the constant
  stands where the other family has a zero lane. This could be original (a bias), or it could
  be a transcription error.
- What the asm does: `M2700` (`0x522700`-`0x5227c7`) uses `movss` constant loads into a 4-lane
  block (`0x72f420`, `0x7304a0`, `0x72b154`) and `shufps`/`mulps`/`addps` axpy updates. The
  constants were not decoded in this pass. Route to the E45 owner: decode the three constants and
  confirm the z lane.

### 5e. `CPassenger::Step`'s constant-address test (no behaviour)

- C++: `RBR_E45_PassengerB.cpp:396`: `((int)u24 == -0x160a0) ? 0 : ...`.
- The test compares the CDef pointer with a constant address that is never the CDef. Same class
  as step.md 5b / rigid-body.md 5d (artefact of the original's null checks). Keep.

### 5f. `SimEFE0` angle bits and rotation source (resolved, asm-confirmed)

- The clear-flag angle branch calls `SetDirty` on `this+0x3f0`, then stores the computed single-precision
  value to `this+0x3fc`. C++ previously converted the value numerically to `int` before storing it;
  it now writes through `float*`, preserving the float bits.
- The rotation copied to `this+0x2a0` comes from the quaternion at `this+0x100`. C++ previously read
  four words from `this+0x40`; it now reads `this+0x100`.
- The fourth drive call takes the angle branch. Drive bisect did not report it because the harness
  frame contains the car and wheels only; the hinged body is a separate allocation. In the direct
  fourth-call replay, all initialized outputs match. One word at `this+0x5ec` differs: `SimD860`
  writes only three floats to its stack `E01Vec4`, then `CopyFrom` copies all four lanes, so that
  fourth lane is stack residue. The drive does not compare this hinged-body allocation.

## Notes for the lead

- **5a** (`SimCFF0`) is resolved for the finite and NaN forced records. `SimCF10` and `SimEA80` still
  have their own x87 precision question.
- **5b** is naming only (`m_def3d0` is the owner).
- **5c** and **5d** need the asm check before anyone trusts those bodies.
- **Not covered in this pass:** `SimE1D0`, `SimDB40`, `SimD860`, `SimD980`, `E45PhysBody::M4250`
  (bad), and the mounted `StepBig`/`Vf6c` bodies. The E43/E44 engine-sim family (`E43EngineSim`,
  `Sim80D0`, `Sim8E20`) is not in this file; it is an engine concern (see `engine-driveline.md`).
