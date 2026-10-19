# Engine and driveline (E39 drive, E41 engine and driveline): physics semantics

Pass 3 of the physics semantics brief. Notes only; no source edits.
Confidence: `known`, `inferred`, `guess`. C++ fidelity labels in the source are quoted.

## 0. Scope of this pass (partial)

Covered:
- CDrive accessors and setters: `RBR_E39_DriveA.cpp`, `RBR_E39_DriveB.cpp`.
- CEngine sim kernels: `RBR_E41_Engine.cpp` (all `Sim*`, `SetState4`, `SetScratch08`).
- DriveLine sampler: `RBR_E41_DriveLine.cpp` `MixDown`, `ClampMix`, `Sample`.
- Gear list and drive/engine tuning keys from the LSP (`physics.rbz`, `h_accent/`).

Covered later in section 7: the E40 drive workers (`RBR_E40_Big`, `Mid`, `LeafA/B/C`), at the
level of the formulas read. Most are named only.

Not covered in this pass (no semantics written):
- `RBR_E40_Blend`, `RBR_E40_DriveLine` (table entry workers), `RBR_E40_Def`, `RBR_E40_State`,
  `RBR_E40_Signal` (ctors and emits).
- `RBR_E39_DriveC/D/E/F/G` beyond the names listed in section 3c.
- `RBR_E39_Mgr*`, `RBR_E39_Collide`, `RBR_E39_Conn`, `RBR_E39_Damage*` (damage and
  collision, out of scope for this brief).
- `RBR_E41_Setup`, `RBR_E41_State`, `RBR_E41_Signal`, `RBR_E41_Def`, `RBR_E41_Scratch`,
  `RBR_E41_Sim` (graph kernels), `DriveLine` `SampleLerp`, `SampleBig*`.
- Any asm check of the engine C++ except where noted.

## 1. Overview

Per-step engine and driveline chain, as far as read:

1. The VCU (`notes/physics/vcu.md`) writes the pedal values into the engine scratch
   (`CEngine::SetScratch08`, which stores `this+0x20`) and into the drive (`Set44A0` for
   throttle at drive `+0x20`, `Set30` for brake at drive `+0x30`).
2. The VCU's launch clutch goes to the drive through `Set4820` (drive `+0x60`).
   `Get4550` reads `(1 - [drive+0x60])`, so `+0x60` is a slip fraction (1 = slipping).
3. The VCU's diff locks go to `Set4570(a, lock)`, which writes the diff torque record
   (see 2b).
4. The engine state carries the rpm (`st+0x188`, rad/s), the rev-cut flags (`+0x168..+0x174`,
   `SetState4`), and the crank counter (`SimE370`). Torque and friction come from the
   `Sim*` kernels (section 3b).
5. The drive gear is `drive+0x54c` (current) and `drive+0x570` (commanded, changed by
   `Set47B0`). `Get4530()` returns `drive+0x54c`.

## 2. Data

Object pointers: `CEngine` `this+4` = def, `this+8` = state, `this+0x14` = scratch CReal,
`this+0x20` = engine pedal input, `this+0x48` = option block. `CDrive` `this+4` = def,
`this+8` = state, `this+0x170` = sub-object (`+0x84`, `+0x90` read by `Get4550`/`Set4570`).
The `CEngine` state at `+8` and the `CDrive` state at `+8` are different classes (different
layouts), so offsets are given per class.

### 2a. CDrive state (`this+8`)

| off | current | proposed | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x54c | gear | gear | i32 | index | current gear (`Get4530`) | known | `DriveA:201` |
| 0x570 | - | commanded_gear | i32 | index | target gear; `Set47B0` writes it | known | `DriveB:Set47B0` |
| 0x564 | - | gear_request | CReal (value at +0xc = 0x570) | - | `Set47B0` dirties this and writes +0x570 | known | `DriveB:Set47B0` |
| 0x45c | - | engine_speed | f32 | rad/s | returned by `Get4520`; VCU uses it as rpm after `*9.5492964` | known (VCU), inferred (unit) | `DriveA:193` |
| 0x384 | - | speed_key | f32 | unknown | returned by `Get4540`; VCU speed-map key is `Get4540 * 0.3` | known (read), unit guess | `DriveB:11` |
| 0x68 | - | car_speed | f32 | m/s | read by the VCU as `CarGet15F8(drive+0x68)` | known (VCU) | `VCU Update` |
| 0x60 | - | clutch_slip | f32 (direct, dirty flag at +0x54) | 0..1 | set by `Set4820`; 1 = slipping | inferred | `DriveB:Set4820`, `Get4550` |
| 0x20 | - | throttle_in | CReal (value at +0x2c) | 0..1 | `Set44A0`: pedal to drive (throttle in normal routing) | inferred | `DriveA:Set44A0` |
| 0x30 | - | brake_in | CReal (value at +0x3c) | 0..1 | `Set30` lane; brake in normal routing | inferred | `DriveA:Set30` |
| 0x64 | - | brake_torque | CReal (value at +0x70 = `k*v`) | - | `Set30` writes `v * def[+0x620]` here | inferred | `DriveA:Set30` |
| 0x40 | - | pedal_2 | CReal (value at +0x4c) | 0..1 | `Set44C0` lane | inferred | `DriveA:Set44C0` |
| 0x74 | - | brake_torque_2 | CReal (value at +0x80 = `k*v`) | - | `Set44C0` writes `v * def[+0x630]` here | inferred | `DriveA:Set44C0` |
| 0x170 | - | drive_flags | pointer | - | sub-object; `+0x84` (lane) and `+0x90` (enable) | known (offset), meaning guess | `DriveB:Get4550, Set4570` |

Diff records (`this+8 + a*0x14`, `a` = 0 centre, 1 front, 2 rear; per the VCU's `Set4570` index):

| off | current | proposed | type | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0xc (per record) | - | diff_ratio | f32 | `s = |value|`; gates the torque factor (3c) | inferred | `DriveB:Set4570` |
| 0x18 (per record) | - | diff_torque | f32 | written `def[a*0x60+0x58] * v` (lock torque) | known (write), inferred (meaning) | `DriveB:Set4570` |

### 2b. CDrive def (`this+4`)

| off | current | proposed | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x58 + a*0x60 | - | diff_max_torque | f32 | N·m (inferred) | max lock torque per diff; `a` as above | inferred (LSP names) | `DriveB:Set4570`; LSP `CenterDiffMaxTorque 800`, `FrontDiffMaxTorque 200`, `RearDiffMaxTorque 900` |
| 0x620 | - | brake_max_front? | f32 | pressure? | scales `Set30` (writes `k*v` to drive `+0x70`) | guess | `DriveA:Set30`; LSP `MaxBrakePressureFront 10000000` (mapping not checked) |
| 0x630 | - | brake_max_rear? | f32 | pressure? | scales `Set44C0` | guess | `DriveA:Set44C0`; LSP `MaxBrakePressureRear 5000000` |
| 0x654 | - | gear_count | i32 | count | upper clamp for `Set47B0` (`count-1`) | known (code) | `DriveB:Set47B0`; LSP `Gears 8` |

### 2c. CEngine state (`this+8`), as read by the kernels

| off | current | proposed | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0x168..0x174 | - | cut_flags[4] | i32 | 0/1 | written by `SetState4` (VCU rev cut) and by `SimE010` (`x <= f2`); see Suspect 1 | known (write); meanings in the engine notes | `Engine.cpp:65`, `SimE010` |
| 0x17c | - | crank_count | u32 | ticks (256 per 4π) | integer crank position | inferred (from constants) | `SimE370` |
| 0x178 | - | crank_angle | f32 | rad | accumulated angle, wrapped at 4π | inferred | `SimE370` |
| 0x180 | - | crank_count_prev | u32 | ticks | previous count | known (copy) | `SimE370` |
| 0x184 | - | torque_sign_in | f32 | - | `f6 = |x|`, sign picks a factor (`SimE660`) | guess | `SimE660` |
| 0x188 | - | engine_speed | f32 | rad/s | rpm via `*9.5492964`; gates at 40 (`SimE980`), 100 (`SimE660`) | inferred (unit: rpm-based formulas) | `SimE470`, `SimE980`, `SimE660` |
| 0x198 | - | misfire_threshold? | f32 | - | `SimE010` compares the input against this | guess | `SimE010` |
| 0x200 | - | smoothed_angle? | f32 | - | `SimE9F0`-area, updated in `SimE1F0` | guess | `SimE1F0` |
| 0x23c | - | - | f32 | - | read by `SimD870` (`-600`) and `SimE660` (damper term) | guess | `SimD870`, `SimE660` |
| 0x238 | - | torque_accum | f32 | - | `+= |f4|` when speed > 100 | inferred | `SimE660` |
| 0x240, 0x24c | - | abs_value | CReal, f32 | - | `Sim410` writes `|def[0x282c] * st[0x188]|` | known (code) | `Sim410` |
| 0x250, 0x25c | - | scaled_value | CReal, f32 | - | `Sim4A0(x)` writes `st[0x26c] * x` | known (code) | `Sim4A0` |
| 0x260, 0x26c | - | compliance_inverse | CReal, f32 | - | `1/Σ(1/x_i)` from `SimE9F0` | inferred (series combine) | `SimE9F0` |
| 0x270 | - | torque_out | f32 | N·m (guess) | `SimE660` return value copy (`f6`) | guess | `SimE660` |
| 0x1f0, 0x1e4 | - | smooth_state | f32 | - | `SimE060` 0.9/0.1 low-pass | known (code) | `SimE060` |

### 2d. CEngine def (`this+4`), as read

| off | current | proposed | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x118, 0x128 | - | interp_a_lo, hi | - | `SimE9F0` interpolates by `o+0xc` | inferred | `SimE9F0` |
| 0x158, 0x168 | - | interp_b_lo, hi | - | `SimE9F0` by `o+0x1c` | inferred | `SimE9F0` |
| 0x198, 0x1a8 | - | interp_c_lo, hi | - | `SimE9F0` by `o+0x2c` | inferred | `SimE9F0` |
| 0x1d8, 0x1e8 | - | interp_d_lo, hi | - | `SimE9F0` by `o+0x3c` | inferred | `SimE9F0` |
| 0x138, 0x178, 0x1b8, 0x1f8 | - | normalisers | - | divisors in `SimE660` | inferred | `SimE660` |
| 0x284c, 0x285c, 0x286c | - | torque_terms | - | `SimE660` penalty and slope | guess | `SimE660` |
| 0x2718 | - | speed_scale | - | `Sim3C0` | inferred | `Sim3C0` |
| 0x282c | - | abs_scale | - | `Sim410` | inferred | `Sim410` |
| 0x2698 | - | flag_count | count | `SimE010` loop count | known (code) | `SimE010` |
| 0x26f8 | - | enable | flag | `SimEE60`, `SimE9F0` | known (code) | `SimEE60`, `SimE9F0` |
| 0x2708 | - | rpm_fraction_lo | - | `SimEBB0` | guess | `SimEBB0` |
| 0x21dc..0x2234 | - | quadratic_tables | - | `SimED70`: `a*x^2 + b*x + c` times scales | inferred | `SimED70` |

### 2e. CEngine option block (`this+0x48`)

| off | proposed | meaning | confidence | evidence |
| --- | --- | --- | --- | --- |
| 0x14, 0x24, 0x34, 0x44 | term_1..4 | `SimE660` inputs, scaled by 400 | guess | `SimE660` |
| 0x4, 0x8, 0x18, 0x28, 0x38 | - | `SimD870` / `SimECF0` inputs | guess | `SimD870`, `SimECF0` |
| 0x58, 0x5c | stall_inputs | `SimECF0` stall test (> 0.1) | known (code) | `SimECF0` |
| 0x70, 0x74 | pick_flag, gate | `SimE9F0` pick, `SimE470` gate | known (code) | `SimE9F0`, `SimE470` |
| 0xc, 0x1c, 0x2c, 0x3c | interp_t | `SimE9F0` parameters | inferred | `SimE9F0` |
| 0x100 | gate_speed | `Sim3C0` gate | known (code) | `Sim3C0` |
| 0x0 (pointer to the word) | rng_seed | `SimE520`, `SimD870`: LCG state, updated as `s*0x2000081 + 0xe623f41d` | known (code) | `SimE520`, `SimD870` |

## 3. Functions

### 3a. Drive accessors and setters (`RBR_E39_DriveA/B.cpp`)

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x504520 | `Get4520` | engine_speed | `state+0x45c` | known | `DriveA:193` |
| 0x504530 | `Get4530` | gear | `state+0x54c` (current gear) | known | `DriveA:201` |
| 0x504540 | `Get4540` | speed_key | `state+0x384` | known (read); meaning unknown | `DriveB:11` |
| 0x504550 | `Get4550` | clutch_torque | `(1 - slip) * lane[0x84] * 900` | known (code); constant 900 is in code | `DriveB:19` |
| 0x504570 | `Set4570(a, lock)` | set_diff_lock | clamp `lock` to [0,1]; if `flags+0x90`: `s=|diff_ratio|`; `s<1 → 0`, `1<=s<2 → (s-1)*lock`, `s>=2 → lock`; `diff_torque = def.max * lock'`; else write 0 | asm-checked (`0x504570`-`0x504611`); NaN lock differs (Suspect 2) | `DriveB:29-58` |
| 0x504820 | `Set4820(v)` | set_clutch_slip | `slip = v` | known | `DriveB:97` |
| 0x504450 | `Set30(v)` | set_brake_1 | `lane+0x3c = v`; `lane+0x74 = def[0x620]*v` (lane +0x64 holds `k*v`) | known (code); meaning inferred | `DriveA:145` |
| 0x5044a0 | `Set44A0(v)` | set_throttle | `lane+0x2c = v` | known (code); meaning inferred | `DriveA:159` |
| 0x5044c0 | `Set44C0(v)` | set_brake_2 | `lane+0x4c = v`; `lane+0x74 = def[0x630]*v` | known (code); meaning inferred | `DriveA:168` |
| 0x5047b0 | `Set47B0(v)` | request_gear | `commanded = clamp(commanded + v, 0, count-1)` (0 if the sum < 0); dirties `+0x564` and writes its value at `+0x570` | asm-checked (`0x5047b0`-`0x504815`) | `DriveB:80` |
| 0x504840 | `Test4840` | gear_ready | counts state floats > 0.2 in `+0x50..` steps, then tests `+0x78`, `SimE980`, `+0x8c`, `+0x84 >= 0.2` | C++ only (partial; not read in full) | `DriveB:107` |

### 3b. Engine kernels (`RBR_E41_Engine.cpp`)

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x50d340 | `SetState4(a,b,c,d)` | set_cut_flags | writes `state+0x168..0x174` | known | `Engine.cpp:65` |
| 0x50eb90 | `SetScratch08(v)` | set_engine_pedal | `this+0x20 = v` (int bits of a float) | known | `Engine.cpp:134` |
| 0x50d3c0 | `Sim3C0` | speed_drag | `max(0, (speed - 5.2) * def[0x2718] * o[0x100] * 0.05)` when `o[0x100] != 0` and speed > 5.2 m/s | known (code); meaning inferred (drag) | `Engine.cpp:93` |
| 0x50d410 | `Sim410` | abs_rpm_term | `|def[0x282c] * st[0x188]|`, returned when `o[0x58] != 0` and `st[0x188] > 0`, else 0 | known (code) | `Engine.cpp:106` |
| 0x50d4a0 | `Sim4A0(x)` | scaled_compliance | `st[0x250] = st[0x26c] * x` | known (code) | `Engine.cpp:123` |
| 0x50e980 | `SimE980` | rpm_over_40 | `1` if `st[0x188] > 40` (rad/s) | known (code) | `Engine.cpp:147` |
| 0x50e470 | `SimE470` | rev_gate | rpm `r = 9.549*st[0x188]`; `0` if `r < 500` or `o[0x74] == 0`; `f2 = clamp((r-3500)/4500, 0, 1)`; `f2' = def[0x20]-f2` (pedal-ish minus rpm term); `1 - 10*f2'` if `f2' < -0.4`; `1 + 0.2*f2'` if `f2' > 0.4`; else 1 | C++ only (header "60%") | `Engine.cpp:188` |
| 0x50e920 | `SimE920` | rev_gate_out | `b = SimE470()`; `d = a*0.1`; `c = b*d` (+0.0005 if b >= 2) | C++ only (header "55%") | `Engine.cpp:208` |
| 0x50e9f0 | `SimE9F0` | compliance_series | four interpolated stage values; `1/tot = 1/(Σ 1/x_i)` (+ one more term when `def[0x26f8]`); stored at `st[0x26c]` | C++ only (header "60%") | `Engine.cpp:222` |
| 0x50ebb0 | `SimEBB0` | cos_blend | `f4 = clamp((pedal - f1)/(1 - f1))`; `phase = f1 + (1-f1)*f4`; `p[0x30] = 1 - cos(phase * π/2)` | C++ only (header "55%") | `Engine.cpp:263` |
| 0x50ecd0 | `SimECD0` | torque_quadratic | `def[0xb8]*a + a*def[0xc8]*a`, `a = st[0x5c]` | C++ only | `Engine.cpp:295` |
| 0x50e370 | `SimE370(a, b, c)` | crank_tick | `angle += a` wrapped at 4π (0x73b6f8); `count = (int)(angle * 20.37)` (256 per 4π); `b` = signed tick delta wrapped at 256; `c = b * 4π/256` | known (constants checked vs. 256 ticks and 4π) | `Engine.cpp:391` |
| 0x50e520 | `SimE520(a)` | misfire_roll | if `st[0x10] == 0`: `st[0x3c] = 0`, return. Else, when `st[0x18] <= st[0x14]` and they differ: LCG step on `*o[0]`, `r = (u % 100001)*1e-5`; event when `r < min(st[0x10]*999.99994, 0.9) * min(st[0x14]*40, 1)`, which sets `st[0x3c]` and clears `st[0x10/0x14/0x18]`. Otherwise `st[0x14] += a`, `st[0x3c] = 0` | C++ only (header "55%"); meaning guess | `Engine.cpp:430` |
| 0x50e660 | `SimE660(a)` | engine_torque | `f5` sums four `o*400` terms (`o+0x14`, `+0x24`, `+0x34`, `+0x44`), each preceded by a penalty `(1 - clamp(st[..]*f4/def[..])) * def[0x284c]` (`st+0xbc`/`def+0x138`, `st+0xf4`/`def+0x178`, `st+0x12c`/`def+0x1b8`, `st+0x164`/`def+0x1f8`); adds a slope at `def[0x286c]`/`def[0x285c]`; `f6 = f5 + (st[0x184] < 0 ? 20*|st[0x184]| : 0)`; writes `st[0x270] = f6`; returns `-f6` or `-(0 or 1)*f6` by sign of `st[0x184]` | asm-checked, matches (section 6, item 3) | `Engine.cpp:475` |
| 0x50ecf0 | `SimECF0()` | stall_guard | `1` if `st[0x188] > 40`; else `1` if `o[0x5c]` and `o[0x58]` >= 0.1 and at least 3 of `o[8], o[0x18], o[0x28], o[0x38]` > 0.7; else `0` | C++ only (header "60%") | `Engine.cpp:550` |
| 0x50ed70 | `SimED70(x, u)` | torque_table | computes the torque curve when `q[2] == 0`, otherwise 0; always writes the result to `q[7]` | asm-checked (`0x50ed70`-`0x50ee52`) | `Engine.cpp:576`; `q[7] = 0` on the `q[2] != 0` path |
| 0x50e010 | `SimE010(x)` | cut_flags_from_input | for `k < def[0x2698]`: `st[0x168+4k] = (x <= st[0x198])` | known (code); NaN differs from asm (Suspect 1) | `Engine.cpp:603` |
| 0x50d870 | `SimD870(i)` | table_index_update | LCG-driven update of `o[i*0x10+8]` and the global table at `0x7ca668`; returns a pointer | C++ only (header "55%") | `Engine.cpp:625` |
| 0x50e060 | `SimE060` | smooth_damper | `BlendW`, `BlendX`, `DamperSqrt4`; writes `st[0x1f0] = 0.9*st[0x1f0] + 0.1*b` | C++ only (header "50%") | `Engine.cpp:300` |
| 0x50e1f0 | `SimE1F0` | solve_damper | `BlendW`, `BlendY`, `DamperSolve`; writes `st[0x200] = solved/d` | C++ only (header "50%") | `Engine.cpp:353` |

### 3c. DriveLine (`RBR_E41_DriveLine.cpp`)

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x50b0b0 | `MixDown()` | mix_chain | `BlendA(e, e+0x30)` for each consecutive pair of entries | C++ only (header "62%") | `DriveLine.cpp:20` |
| 0x50b0f0 | `ClampMix(x, out)` | sample_clamped | clamp `x` to `[0, 2^31)`, split into integer index and fraction, `BlendB(i, frac, out)` | C++ only (header "70%") | `DriveLine.cpp:42` |
| 0x50b180 | `Sample(x, out)` | sample_table | first entry `j` with `x < key[j+1]`; `t = (x - key[j]) / (key[j+1]-key[j])`; `BlendB(j, t, out)`; past the end, copy the last entry | known (asm verified: search slot `i+1`, interpolation slots `i`,`i+1`); meaning of table (torque vs rpm?) unknown | `DriveLine.cpp:81`; asm `0x50b180`-`0x50b20d` |

### 3d. Helpers (not physics)

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x50a4f0 | `Thunk4F0` | - | trampoline to E12 `0x475ea0` (table sampler) | known | `Drive.h:24` |
| 0x503ca0 | `E39_RatioCA0(v, ring)` | ring_mean | stores `v` in a ring of `n` slots and returns the mean (`Σ/n`, double) | C++ only (header "60%") | `DriveF.cpp:32` |
| 0x504990 | `E39_Step4990` | mat4_inverse | 4×4 matrix inverse by cofactors, scaled by `1/det` (not drive physics; the name is historical) | C++ only | `DriveG.cpp:78` |
| 0x50ce20 | `E41_GraphFill` | graph_row_blend | two rows blended by `f1`, `f2` into 7 graph points | C++ only (E41_Sim) | `Sim.cpp:13` |
| 0x50d220 / 0x50ced0 / 0x50d090 | `E41_BlendW` / `BlendX` / `BlendY` | bracket_blend | index search and linear fraction over a table; BlendW uses unordered-aware `!(x > key)` search and `!(h > 1)` clamp; BlendX/Y bilinear over two tables | BlendW asm-checked (`0x50d220`-`0x50d2d1`); BlendX/Y C++ only | `Sim.cpp:39`, `:83`, `:154` |

## 4. Gear list (LSP) and the gear index

`h_accent/r_gravel.lsp` (and `r_tarmac.lsp`, `r_snow.lsp`, `m_lancer`, `s_i2000` likewise) define
13 gear-list slots `GearList00..12` as `Pinion`, `Gear`, `TorqueMax 10000`, `TorqueMin -8000`.
Ratio `Gear/Pinion` (computed here, not checked against the game):

| list | pinion | gear | ratio | note |
| --- | --- | --- | --- | --- |
| 00 | 14 | -29 | -2.071 | reverse? (negative gear) |
| 01 | 14 | -32 | -2.286 | reverse? |
| 02 | 0 | 0 | - | neutral (zero teeth) |
| 03 | 13 | 38 | 2.923 | forward |
| 04 | 16 | 34 | 2.125 | forward |
| 05 | 18 | 30 | 1.667 | forward |
| 06 | 21 | 28 | 1.333 | forward |
| 07 | 23 | 25 | 1.087 | forward |
| 08 | 28 | 25 | 0.893 | forward |
| 09 | 9 | 35 | 3.889 | final drive |
| 10 | 28 | 21 | 0.750 | drop gear |
| 11 | 28 | 22 | 0.786 | drop gear |
| 12 | 26 | 22 | 0.846 | drop gear |

Setup mapping (`d_tarmac.lsp` `Drive`): `GearId0..7 = 1..8`, `FinalDriveId = 9`, `DropGearId = 11`.
Options ranges (`r_gravel.lsp` `Gear*OptionsMin/Max`): slot 0 → list 2, slot 1 → list 0, slot 2 → list 3,
..., slot 7 → list 8, final → 9, drop → 10..12.

Open: this gives two candidate reads of the drive gear index (`+0x54c`): (a) slot = index,
which makes 0 = neutral, 1 = reverse, 2 = first, ...; (b) list = index + 1 (`GearId`), which
makes 0 = reverse, 1 = neutral, 2 = first. The VCU code treats 0 as neutral and 1 as first
gear. Reading (a) matches 0 = neutral but puts reverse at 1; reading (b) matches neither. The
mapping needs a runtime check (see open 1).

## 5. Open questions

1. **Gear index mapping.** `Get4530()` = `drive+0x54c` (known). Which LSP list slot each index
   selects is not settled (section 4). The VCU's neutral/first/second rules rely on 0 = neutral.
   A recorded gear trace from the harness would settle this.
2. **Two engine-speed fields.** The drive state `+0x45c` (`Get4520`, used by the VCU) and the
   engine state `+0x188` (used by `SimE470`, `SimE980`, `SimE660`) are both rad/s-scale.
   Whether one is copied into the other is not determined.
3. **Speed-map key.** `Get4540` (`drive+0x384`) is used as `Get4540 * 0.3` by the VCU. The LSP
   velocity axis of the diff speed maps is in m/s (5.56 .. 55.6), so the key is likely a speed,
   but the scale factor 0.3 and the unit of `+0x384` are not confirmed.
4. **Brake fields.** `Set30` and `Set44C0` scale by drive `+0x620` and `+0x630`, which the LSP
   names as `MaxBrakePressureFront` and `MaxBrakePressureRear` (in the order given). The
   mapping is inferred.
5. **Who writes `st[0x168..0x174]`.** `SetState4` (VCU rev cut) and `SimE010` both write these
   slots. The order and whether both run each step are not determined.
6. **Misfire and damper paths.** `SimE520`, `SimD870`, `SimE060`, `SimE1F0` are C++-only
   (50-55%). Their meaning as misfire or damper behaviour is a guess.
7. **`SimE660`** (engine torque): asm-checked, see section 6, item 3. The meaning of its input `st[0x184]` is open.
8. **`Test4840`** is partly read. Its gate semantics need the full function.

## 6. Suspect code

Confirmed by asm:

1. **`SimE010` NaN (`RBR_E41_Engine.cpp:603`).**
   - C++: `(x < f3 || x == f2)`. For NaN input, both compares are false, so the flag is 0.
   - Asm `0x50e010`-`0x50e05c`: `comiss st+0x198, x` with `jbe` selects 1 when `x <= st+0x198`.
     For NaN the compare is unordered, so `jbe` is taken and the flag is 1.
   - Effect: C++ writes 0 for a NaN input, asm writes 1. Same class as E33 `BoxHit` in
     `lead/briefs/cpp-asm-diffs.md`. Not yet added to that brief.

Checked and not a bug:

- `SimE660` offset `st + 300` (decimal for `0x12c`). The asm (`0x50e770`) reads `0x12c`, so it matches.
- `DriveLine::Sample` has no off-by-one. The asm compares slot `i+1` and interpolates `i`..`i+1`,
  the same as the C++.

C++-only observations (verify before relying on them):

2. **`Set4570` NaN lock (`DriveB:31-36`).** Asm `0x504570`-`0x504592`: the first clamp is `comiss`+`jbe`,
   so an unordered (NaN) input takes the upper-clamp path and stays NaN. C++ `if (0.0f <= v) … else v = 0`
   gives 0 for NaN. The effect is NaN into `diff_torque` (asm) versus 0 (C++). Same class as the
   other NaN items; add to `lead/briefs/cpp-asm-diffs.md`.
   The `|s| >= 2` factor branch matches the asm (`0x5045d5`-`0x5045f4`).
3. **`SimE660` (`Engine.cpp:475`): asm-checked, matches.** Checked: `|f6|`; `f4 = 1/st[0x24c]` gate
   (`> 1e-5`, NaN → 0); the four penalty blocks in order (`0x50e6a5`-`0x50e7fc`, each clamped
   to [0,1]); the `o*400` terms; the slope at `def[0x286c]`; the `f6 = (st[0x184] < 0 ? 20 : 0) * |f6| + f5`
   term; the `ω > 100` integrator (`st[0x238] += |f6*a*ω|`); and the return `-f6` for
   `st[0x184] > 0`, `-(sign)*f6` otherwise (`0x50e8c7`). Constants checked: 400, 1e-5, 20, 100.
   The engine torque returned is negative (retarding) for positive `st[0x184]`. Meaning of
   `st[0x184]` (inferred: signed speed or torque input) is still open.
4. **`SimECF0` stall guard (`Engine.cpp:550`).** The count of options above 0.7 is computed by
   adding `unsigned char` values. Asm not checked.
5. **`E39_Step4990` name.** It is a 4×4 inverse, not a step. The name should change.
6. **E41 and drive corrections (item 11).** `BlendW` now keeps the first knot and NaN weights for NaN `x`; `SimED70` now writes `q[7] = 0` when `q[2] != 0`; `Set47B0`'s comment now states that it writes the value word at `+0x570` (asm confirms the existing code). Forced direct replays and the final drive run are recorded in the F2 report.

## 7. E40 drive workers (`RBR_E40_Big.cpp`, `Mid.cpp`, `LeafA/B/C.cpp`)

Called from the car with `this` = the embedded CDrive (`car+0x4b0`). Names are the header's
(`RBR_E39_Damage.h`) and the comments in the sources. The `M` numbers are the address suffix.
Fidelity labels are the source headers'.

### 7a. Read in this pass

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x508770 | `M8770(src)` | state_snapshot | copies 189 dwords from `src` into the drive's def block, then derives: `+0xa48 = s[0x368]/s[0x3b8]`, `+0x614 = s[0x618]+s[0x568]`, and several values `s[0x568]/k`, `s[0x618]/k` with `k = dst[0x620]` (the field `Set30` scales by; the store targets are approximate, see 55-70% header note) | C++ only (header 70%) | `Big.cpp:20-260` |
| 0x509120 | `M9120(dt)` | gear_guard | returns 1 and latches the slip flag `+0x55c = 0`, `+0x578 = 1`, `M79B0(0)` when `dt < lower` or `dt > upper`; `lower/upper = tbl[+0x88] * gear_rec[+0x3c / +0x4c] * tbl[+0x48 + 4*gear]`, with `gear_rec = gear*0x50 + 0x688 + lane` | read; C++ only (no header %) | `Big.cpp:255-290` |
| 0x509210 | `M9210(dt)` | gear_logic | (a) if `g_1659b70+0x24 .160cc & 0x80000`: `f = dt*tbl[+0x74]*K`, `u = LCG(dri+0x57c)`, `tbl[+0x48+4*gear] = clamp01(old - f*f*(u % 100001)*1e-5)` and `dri+0x174+4*gear = dt`; (b) `p+0x1c4 = sqrt(clamp(|dt*K2|, 0, 0.8) + K3)`; (c) emit `CSigMrRCarGearChange {Get30, gear, slip, sqrt}` | C++ only (header 60%; LCG multiply written as a subtraction) | `Big.cpp:292-337` |
| 0x5093a0 | `M93A0(a, out, b, c, d)` | clutch_torque | see 7b | C++ only (no header %); arg order checked against the caller (`Big.cpp:904`) and the asm | `Big.cpp:352-450` |
| 0x5096c0 | `M96C0(dt)` | clutch_pack | structure only: sets a 2-element latch from `dri+0x510..0x51c` (2x2 matrix `p+0xe0/+0xe4`), loops `M7AA0`/`M5F10` while the latch flips, integrates `dri+0x20 += loc8*dt` and `dri+0x24 += loc4*dt`, and writes `dri+0x30`, `dri+0x20`, `dri+0x24`, `dri+900` | 55% (header) | `Big.cpp:451-632` |
| 0x509bc0 | `M9BC0(dt)` | gear_pack | structure only: four-lane latch set on `E39_MatVec(lane+0x530, p+0xa0)`; integrators `dri+0xc/+0x10/+0x24/+0x38 += loc20[i]*dt`; writes `dri+0x1c/+0x30/+0x44`; sign latch of `dri+0x10` | 55% (header) | `Big.cpp:634-944` |
| 0x506840 | `M6840(x)` | set_commanded_gear | `dri+0x570 = (x >= gear_count) ? 1 : x` (not clamped below); dirties `+0x564` | read | `LeafC.cpp:209-216` |
| 0x506410 | `M6410(x)` | sign_latch | if `dri+0x524 == 1`: `g = |p+0xdc|`; when `slot[x] <= g` and `g != slot[x]`: writes `dri+0x14+0x14*x = sign(p+0xdc)` (1, 0, or -1) and returns 1 | read; C++ only | `LeafC.cpp:9-31` |

### 7b. The clutch torque worker (`M93A0`)

Inputs: `a` = torque from the driveline, `b` = friction limit, `c` = ratio (per-gear `+0x694`
times `lane+0xa54`, from the caller), `d` = dt. Output `out` = torque transmitted to the
driveline. State: `dri+0x368` = latch sign (-1, 0, 1), `dri+0x578` = "slipping" flag,
`dri+0x45c` = engine speed (rad/s).

Gate: per-gear value `(gear*0x50 + 0x694 + lane) != 0`, `dri+0x55c` (slip flag) `!= 0`, and
`dri+0x368 == 0`.

- Not slipping (`dri+0x578 == 0`):
  - If `|a| <= b` and `M9120(a) == 0`: locked. Engine speed `dri+0x45c = Get4540 * c`
    (`dri+900` is decimal for `0x384`); `out = a`.
  - If `|a| > b`: latch `dri+0x368 = sign(a)`, `dri+0x578 = 1`, `out = sign * b`, call `M9120(...)`.
- Slipping (`dri+0x578 != 0`): `f7 = Get4540 * c` (shaft speed times ratio), `f5 = (J_a + J_b) *
  (f7 - engine_speed)` with inertias at `lane+0x674` and `lane+0x664`, `f6 = a - f5/d`. If
  `|f6| <= b`: engine speed = `f7`, `out = f6`; otherwise latch the sign and `out = sign * b`.
  Then `M9210(f5)`.
- Latched tail (`dri+0x368 != 0`, flag gate still on): `engine_speed += (a - latch*b) / lane+0x664 * d`,
  `out = latch*b`. If the engine-to-shaft difference changes sign, unlatch (`dri+0x578 = 1`,
  `dri+0x368 = 0`, `engine_speed = f5`).
- Gate off, or not latched: `engine_speed += a / (J_a + J_b) * d`, `dri+0x368 = 0`, `out = 0`.

So this is a torque-limited clutch between engine and driveline, with a lock/slip state. The
meaning of `J_a`, `J_b` (`lane+0x674`, `+0x664`) is inferred: engine and driveline inertia.

Note that `M9210(f5)` passes a slip torque as `dt`. The asm passes the same stack slot
(`0x509465`, `0x5094ef` store `xmm0`, `0x50956a` reads it), so the C++ and asm agree on the
value. Whether the original intends a torque there is open.

### 7c. Not analysed (names and headers only)

- `E40_Mid`: `M79B0` gear-change emit (`0x5079b0`), `M7A00` gear-ratio via `E43` (`0x507a00`,
  "Fwd18E20" in the header, 'x87 residue'), `M7AA0` clutch matrix (`0x507aa0`), `M7C10` reset with
  NaN guard (`0x507c10`), `M7D60` clutch state machine (`0x507d60`, header "40%"), `M8480`
  drivetrain-state emit (`0x508480`), `M8570` gear trim (`0x508570`, takes `dt`).
- `E40_LeafA/B/C`: `M5970`, `M59D0`, `M5A10` (lane ratio, double), `M5A80` (lane energy, double),
  `M5B00` (two-lane distribution), `M5BE0` (clutch pack), `M5D50` (flag matrix), `M5F10` (torque
  split), `M5FE0` (slip matrix), `M60D0`/`M6260` (clamp and latch), `M6410`/`M6490` (sign latch,
  read for `M6410`), `M6520` (tri-lane OR), `M6540`/`M6630` (axle sums, `M6630` header 50%),
  `M6880` (dual-lane flag count), `M68D0` (sign-change), `M6920` (lane rate integrator), `M6950`,
  `M69A0` (blend kernel), `M6A00`, `M6B20` (twin-lane resolver), `M6C90` (slip ratio, double),
  `M6DA0`..`M6DF0` (getters and a block copy).

### 7d. Link to the VCU

- VCU `UpdateD` bit 2 calls `E40Proxy::Unk06840` = `CDrive::M6840`, which sets the commanded
  gear directly (7a). `notes/physics/vcu.md` 3d is updated.
- VCU `CheckSlip` and `E50_QuerySlip` read the drive slip flag `+0x55c`, which `M93A0`, `M9120`
  and `M9210` set and clear (7b).
