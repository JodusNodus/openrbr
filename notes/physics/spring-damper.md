# Spring-damper (E48): physics semantics

Pass 1 of the physics semantics brief. Notes only; no source edits. Names in the
"proposed" columns are ours. "Current" names are the C++ reconstruction's.

Confidence: `known` (the exe or LSP names it, or the equation is unambiguous),
`inferred` (strong argument from use), `guess`.

## 1. Overview

`CSpringDamper` is the per-wheel strut: a spring in series with an optional helper
spring, bump-stop and droop-stop rubbers, and a velocity-dependent damper with a
high-speed breakpoint. It is one object per wheel, placed at `wheel+0xec4`
(`src/rbr/RBR_E51_WheelB.cpp:56`, ctor `CSpringDamper(tdef = wheel_def+0x450,
state = wheel+0x670, mask = wheel)`).

Place in the step. Both wheel update paths (`src/rbr/RBR_E51_WheelD.cpp:538` and
`:807`) call `Step(x, y, z)` once per physics step, then integrate the strut:

```
inputs  x = wheel+0x5bc   strut travel (m), integrated from velocity
        y = x + def[0x41c]  strut coordinate (m); def[0x41c] is a static reference
        z = wheel+0x5cc   strut velocity (m/s), from the previous step
outputs o1 = spring + stop force (N)           -> Fs
        o2 = high-speed breakpoint offset (N)  -> Fo
        o3 = damping coefficient (N s/m)       -> c
then    v_new = (v + dt*(g + o1 + o2)*invmass) / (1 + dt*o3*invmass)   (g: see Open 5)
        x_new = x + v_new*dt
```

The damper force is solved implicitly: `F_damp = o2 - o3 * v_new`. `dt` is 0.01 s
(from the brief). `Step` itself uses no `dt`. `UpdateAndReset`
(`WheelB:146`) and `ResetOutputs` (`WheelB:273`) are the init and reset paths.
`CopyStateToDef` (`WheelB:109`) runs when a setup is adopted.

Sign convention (inferred from the equations): `x` and `z` grow with extension;
positive force pushes the strut outward (extend); damping force is `-c * z`.

## 2. Data

### 2a. CDef (tweak tree, `CSpringDamper::CDef`, 0x11c bytes)

Each entry is a `CReal` at object offset `obj`. The value is a float at `obj+0xc`,
so the value slot is `def[(obj+0xc)/4]`. Mapping `obj` to name: the exe dictionary
gives `id = obj/4`, and `CBase::Unk40(id)` returns `this + 4*id`
(`0x47b3f0`). The dictionary is in `.data`: pointers to name strings at
VA `0x7cc518`..`0x7cc618` (entries: name, 0, id, 0; stride 0x10).

| obj | value slot | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0x0c | +0x18 | m_unk0C (CUnsigned) | debug_id | u32 | - | debug id for logs | known (name) | dict id 3, `0x7cc518` |
| 0x1c | def[0x0a] | m_unk1C | bump_stop_pos | f32 | m | strut coordinate where the inner bump rubber starts (`y < lo`) | known name; meaning inferred | dict `InnerBumpRubberPos` id 7; LSP wheel SpringDamper 0.12-0.19; Step `fLo` |
| 0x2c | def[0x0e] | m_unk2C | droop_stop_pos | f32 | m | strut coordinate where droop rubber starts (`y > hi`) | known name; meaning inferred | dict `OuterBumpRubberPos` id 0xb; LSP 0.32-0.455; Step `fHi` |
| 0x3c | def[0x12] | m_unk3C | spring_free_length | f32 | m | spring length at zero force (`L0`) | known name; meaning inferred | dict `SpringLength` id 0xf; Step `fT` |
| 0x4c | def[0x16] | m_unk4C | spring_rate | f32 | N/m | main spring stiffness `ks` | known name | dict `SpringStiffness` id 0x13; LSP setup 40000-75000 |
| 0x5c | def[0x1a] | m_unk5C | helper_min_length | f32 | m | strut length where the helper spring starts (`Hm`) | inferred (copied from setup key `HelperSpringMinLength`) | CopyStateToDef `0x38->0x68`; Step `x - def[0x1a]` |
| 0x6c | def[0x1e] | m_unk6C | helper_length | f32 | m | helper-spring top length (`Hl`) | known name; meaning inferred | dict `HelperSpringLength` id 0x1b; Step `fT` else branch |
| 0x7c | def[0x22] | m_unk7C | helper_rate | f32 | N/m | helper spring stiffness `kh` | known name; meaning inferred | dict `HelperSpringStiffness` id 0x1f; series with `ks` |
| 0x8c | def[0x26] | m_unk8C | bump_stop_rate | f32 | N/m | bump-rubber stiffness (`y < lo`) | known name | dict `BumpRubberStiffness` id 0x23; LSP 300000-500000 |
| 0x9c | def[0x2a] | m_unk9C | droop_stop_rate | f32 | N/m | droop-stop stiffness (`y > hi`); default 600000 in ctor | known name | dict `DroopStiffness` id 0x27; ctor `0x52d750` |
| 0xac | def[0x2e] | m_unkAC | damp_bump | f32 | N s/m | compression damping | known name | dict `DampingBump` id 0x2b; LSP setup 4200-6000 |
| 0xbc | def[0x32] | m_unkBC | damp_rebound | f32 | N s/m | extension damping | known name | dict `DampingRebound` id 0x2f; LSP setup 4700-9000 |
| 0xcc | def[0x36] | m_unkCC | stop_damp | f32 | N s/m | damping of the stop rubbers, capped (`fK2`) | known name; meaning inferred | dict `DampingBumpRubber` id 0x33; LSP 20000 |
| 0xdc | def[0x3a] | m_unkDC | hs_break_bump | f32 | m/s | compression speed where high-speed bump damping starts | known name; unit inferred | dict `HighSpeedBreakBump` id 0x37; Step `fW` |
| 0xec | def[0x3e] | m_unkEC | hs_break_rebound | f32 | m/s | extension speed where high-speed rebound damping starts | known name; unit inferred | dict `HighSpeedBreakRebound` id 0x3b; Step `def[0x3e] <= z` |
| 0xfc | def[0x42] | m_unkFC | hs_damp_bump | f32 | N s/m | high-speed compression damping | known name | dict `HighSpeedDampingBump` id 0x3f; UpdateForces `state[5]` |
| 0x10c | def[0x46] | m_unk10C | hs_damp_rebound | f32 | N s/m | high-speed extension damping | known name | dict `HighSpeedDampingRebound` id 0x43; Step `fB` |

Setup-side keys (CSetup, below) overwrite the CDef slots through `CopyStateToDef`.
Only the four wheel-section keys (`BumpRubberStiffness`, `DampingBumpRubber`,
`InnerBumpRubberPos`, `OuterBumpRubberPos`) are loaded straight into CDef from
`PhysicsEngine/Wheel/CAR_WHEEL_*/SpringDamper` (`notes/formats/physics-lsp.md`,
`physics.rbz`, e.g. `h_accent/tarmac.lsp:34-41`).

### 2b. CSetup (setup values, `CSpringDamper::CSetup`)

Dictionary group 2 (`0x7cc628`..) gives the 9 names. Object offsets are the CReal
members, values at `obj+0xc`. Names come from the `SpringDamperLF/RF/LB/RB` blocks
in `CarSetup` (e.g. `h_accent/setups/d_tarmac.lsp`, `SpringDamperLF`).

| obj | key (dict) | copied to CDef | proposed | unit | confidence |
| --- | --- | --- | --- | --- | --- |
| 0x0c | SpringLength | obj 0x3c (spring_free_length) | spring_free_length | m | known name |
| 0x1c | SpringStiffness | obj 0x4c | spring_rate | N/m | known name |
| 0x2c | HelperSpringMinLength | obj 0x5c | helper_min_length | m | known name |
| 0x3c | HelperSpringLength | obj 0x6c | helper_length | m | known name |
| 0x4c | HelperSpringStiffness | obj 0x7c | helper_rate | N/m | known name |
| 0x5c | DampingBump | obj 0xac | damp_bump | N s/m | known name |
| 0x6c | DampingRebound | obj 0xbc | damp_rebound | N s/m | known name |
| 0x7c | BumpHighSpeedBreak | obj 0xdc | hs_break_bump | m/s (1.0-1.5 in LSP) | known name; unit inferred |
| 0x8c | DampingBumpHighSpeed | obj 0xfc | hs_damp_bump | N s/m | known name |

`CopyStateToDef` (`0x52dbd0`) copies nine CSetup value slots into CDef value slots.
Each pair is `src -> dst` (byte offsets of the value slots, as in the table above):
`0x18->0x48, 0x28->0x58, 0x38->0x68, 0x48->0x78, 0x58->0x88, 0x68->0xb8, 0x78->0xc8,
0x88->0xe8, 0x98->0x108`. It also copies 18 bookkeeping words (the non-value words of
each CReal). Those are not physics. The two `CSetup` ranges in the dictionary group (`Min`,
`Max`, `Step`, `CSetupOptions`) are UI-only and are not listed here.

### 2c. CState (sim state, `CSpringDamper::CState`, 0x3c bytes, `wheel+0x670`)

`state[k]` in the C++ is the float at `+4k`.

| obj | current | proposed | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| 0x0c | m_unk0C = state[3] | damp_bump_eff | N s/m | compression damping this step; floor 400 | known eq (`UpdateForces`) | `0x52dd70` |
| 0x10 | m_unk10 = state[4] | damp_rebound_eff | N s/m | extension damping this step | known eq | `0x52dd70` |
| 0x14 | m_unk14 = state[5] | hs_damp_bump_eff | N s/m | high-speed compression damping | known eq | `0x52dd70` |
| 0x18 | m_unk18 (int) | owner | ptr | wheel object; `+0xEB4` read as wheel id | inferred | `UpdateAndReset` reads `*(state+0x18)+0xEB4` |
| 0x1c | CReal "LastDeflection" | last_deflection | m | name only; writer not found | guess | dict id 7 `0x7cc618` |
| 0x2c | CUnsigned | unknown | - | not found in this pass | - | - |

### 2d. CSpringDamper (runtime object, `wheel+0xec4`, size >= 0x30)

| off | current | proposed | type | meaning | confidence |
| --- | --- | --- | --- | --- | --- |
| +0x04 | m_unk4 | def | ptr | CDef | known (ctor) |
| +0x08 | m_unk8 | state | ptr | CState | known (ctor) |
| +0x0c | m_scratch | - | CScratch | vtable only | known |
| +0x10 | m_unk10 | scratch | ptr to f32[2] | `s[0]`: multiplier in the fade test; `s[1]`: fade factor in [0,1] | `s[1]` inferred; `s[0]` guess. **NULL after ctor; see Open 1** |
| +0x14 | m_unk14 | mode | int | 1 when the helper spring is in series (`fT` else branch) | inferred |
| +0x18 | m_unk18 | hs_hold | int | 10 after a high-speed step, then counts down | inferred |
| +0x1c | m_unk1C | sig.id | int | owner id (`wheel+0xeb4`) for the signal | inferred |
| +0x20 | m_unk20 | sig.peak_v | f32 | largest-|z| velocity this step, signed | inferred |
| +0x24 | m_unk24 | sig.lo_v | f32 | `-z` when compressing at the bump stop | inferred |
| +0x28 | m_unk28 | sig.hi_v | f32 | `z` when extending at the droop stop | inferred |
| +0x2c | m_unk2C | sig.valid | int | reset to 1 | inferred |

The `+0x1c..+0x2c` group is a `CSigDamper` (`Init` at `0x52d6d0`; emitted through
`g_signalManager+0xc2b8` by `ResetOutputs`, `0x52e010`). Its consumer is not
traced here.

## 3. Functions

| address | current | proposed | computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x52d750 | CDef::CDef | - | name "SpringDamper"; `droop_stop_rate = 600000` | known | `RBR_E48_SpringDamper.cpp:116` |
| 0x52df20 | CSpringDamper ctor | - | sets def, state; `scratch = 0`, `mode = 0`, `hs_hold = 0`, `peak = 0` | known | asm 0x52df20 |
| 0x52dbd0 | CopyStateToDef | apply_setup | nine setup slots to CDef (see 2b) | known (offsets) | source and asm |
| 0x52dd70 | UpdateForces | update_damping | `damp_bump_eff = max(400, (1-0.7f)*damp_bump)`; `damp_rebound_eff = (1-0.3f)*damp_rebound`; `hs_damp_bump_eff = (1-0.8f)*hs_damp_bump`; `f = s[1]` | known (equations) | source and asm |
| 0x52dcc0 | NormalizeTravel | - | `clamp((droop_stop_pos - y)/(droop_stop_pos - bump_stop_pos), 0, 1)` | known eq; no src callers | source |
| 0x52dd20 | TravelRange | - | `lo = y - bump_stop_pos`, `hi = droop_stop_pos - y` | known eq | caller `WheelB:89` |
| 0x52dd50 | ComputeTravel | switch_travel | `L0 - (Hl - Hm)*kh/ks + Hm`: travel where the helper spring engages | known eq | caller `WheelB:89` (result unused there) |
| 0x52dfb0 | StepA | fade_only | fade update only, no outputs | inferred | no src callers |
| 0x52e010 | ResetOutputs | - | emit signal, reset signal fields | inferred | source |
| 0x52e060 | UpdateAndReset | - | `UpdateForces` then reset signal, no emit | inferred | `WheelB:146` |
| 0x52e090 | Step(x,y,z) -> o1,o2,o3 | damper_step | see below | known eq (for force); inferred (intent) | source and asm |

### Step(x, y, z) equations

Symbols: `L0 = spring_free_length`, `ks = spring_rate`, `Hm = helper_min_length`,
`Hl = helper_length`, `kh = helper_rate`, `Lo = bump_stop_pos`, `Hi = droop_stop_pos`,
`kb = bump_stop_rate`, `kd = droop_stop_rate`, `cs = stop_damp`, `cb, cr` = the
state damping (`damp_bump_eff`, `damp_rebound_eff`), `chb, chr` = `hs_damp_bump_eff`
and `hs_damp_rebound`, `vb = hs_break_bump`, `vr = hs_break_rebound`.

1. Main spring force, `Fm = ks * (L0 - (x - Hm))`.
   - If `(Hl - Hm) * kh <= Fm`: mode 0, `Fs = Fm`.
   - Else: mode 1, `Fs = ks*kh/(ks+kh) * (Hl + L0 - x)`.
2. Stop force, `y` against `Lo`, `Hi`:
   - `y < Lo`: `Fst = (Lo - y) * kb` (positive, pushes out).
   - `y > Hi`: `Fst = (Hi - y) * kd` (negative, pulls in).
   - Otherwise `Fst = 0`.
3. Stop damping `cst`:
   - `y < Lo`: if `z >= 0`: `cst = cs`, or `Fst/z` when `Fst <= cs*z`. If `z < 0`: `cst = 0.3*cs` and `lo_v = -z`.
   - `y > Hi`: if `z <= 0`: `cst = cs`, or `Fst/z` when `cs*z <= Fst`. If `z > 0`: `cst = 0.3*cs` and `hi_v = z`.
   - Otherwise `cst = 0`.
4. Damper coefficient and breakpoint offset:
   - `z >= 0`: if `z > vr` (and `vr != 0`): `c = chr`, `o2 = (chr - cr) * vr`, `hs_hold = 10`. Else `c = cr`, `hs_hold -= 1`.
   - `z < 0`: if `z < -vb` (and `vb != 0`): `c = chb`, `o2 = (chb - cb) * (-vb)`, `hs_hold = 10`. Else `c = cb`, `hs_hold -= 1`.
5. Outputs: `o1 = Fst + Fs` (before clamping), `o2` as above, `o3 = cst + c`.
6. Fade. `F = max(Fst, 0) + o2 - c*z`. If `27000 < s[0]*F`: `q = s[1] + ((F - 27000)/18000)^2`, clamped to [0,1], written to `s[1]`. Then `UpdateForces`. (Units: 27 kN and the 18 kN span are inferred; see Open 2.)
7. Peak: if `|peak| < |z|`, `peak = z`.

Check against the damper's own sense: at the breakpoint, `o2 - c*z` with
`c = chr` and `z = vr` gives `(chr - cr)*vr - chr*vr = -cr*vr = -cr*z`, so the
force is continuous at `z = vr` (same for compression). This is why `o2` is the
offset. Confidence: known (algebra on the code).

Shared helpers (`NSpringDamper::Damper*`, `0x52e350`-`0x52e8a0`): these are called
by `RBR_E41_Engine.cpp`, `RBR_E43_EngineA/B.cpp`, `RBR_E49_TyreSim.cpp`,
`RBR_E41_State.cpp`, and `RBR_E48_TyreMenu.cpp`. They are not spring-specific, so
their meaning is deferred to the engine and tyre notes. Callers:

| helper | callers in src/rbr |
| --- | --- |
| DamperStep1, DamperStep2 | E43_EngineB, E49_TyreSim |
| DamperBiasSub | E41_Engine, E49_TyreSim |
| DamperAccumA | E43_EngineB, E49_TyreSim |
| DamperAccumB, DamperClamp, DamperMerge | E43_EngineB |
| DamperUpdateA | E41_Engine |
| DamperUpdateB | E41_State, E48_TyreMenu, E49_TyreSim |
| DamperUpdateC | E43_EngineA |
| DamperScale, DamperSqrt4 | E41_Engine |
| DamperCurve, DamperRelax | E43_EngineA |
| DamperCombine, DamperSolve | E41_Engine, E43_EngineA |
| DamperForce | E49_TyreSim |
| DamperBiasAdd, DamperUpdateD, DamperSquare, DamperIdentity | none |

## 4. Open questions

1. **Scratch pointer `+0x10` is NULL.** The ctor stores 0 there (asm `0x52df20`:
   `movl %edx, 0x10(%esi)`). `Step` loads it and reads `(%eax)` without a
   test (asm `0x52e2c0`). No writer to `wheel+0xed4` is in `src/`. Either a writer
   exists that the reconstruction does not have, or the reads are wrong. Settles
   with a peek of `wheel+0xed4` at runtime, or by finding the writer.
2. **Fade threshold.** The test uses `s[0] * F > 27000`, but the increment uses `F`
   alone. The meaning of `s[0]` is unknown. The 27000 and 18000 constants are inferred
   units (N), not verified.
3. **Fade reset.** `s[1]` only increases in `Step`, and `UpdateForces` reads it.
   Who lowers it is not found (`ResetScratch` does nothing to `s`).
4. **`def[0x41c]`** (`y = x + def[0x41c]`) is a per-car static reference. Its name is
   not determined; it is probably the static strut length. Not in the wheel LSP keys
   checked.
5. **Wheel integration** (`E51_WheelD:538-560`): the numerator uses `g5` twice
   (`(n26c + o1 + o2 + n26c)`). `g5` comes from `CCar::GetA48`. Its meaning
   (static load share?) is for the wheel notes, not this one.
6. **`hs_hold` consumer**, **`LastDeflection` writer**, and **CState `+0x2c`** are
   not found.
7. **`StepA` and `NormalizeTravel` have no callers in `src/`.** They may be dead
   code or called through a vtable not reconstructed.
8. **Units of the velocity thresholds** (`hs_break_*`, m/s) rest on the LSP range
   1.0-1.5 and on the equations. The sign of `z` (positive = extension) follows from
   `x` integrating `z` and from the damping sign. Neither has been checked against a
   recording. A check is possible with `~/rbr-harness-runs/ref1.bin` (wheel `+0x5bc`
   and `+0x5cc` per step) but not run in this pass.
9. **Which dictionary is which** (group 1 = CDef, group 2 = CSetup) is inferred from
   the object offsets matching each layout, and from the LSP sections the keys appear
   in. The runtime `g_springDefTable`/`g_springSetupTable` globals (annotated at
   `0x0165f180`/`0x0165f158`) are not in the exe image.

## 5. Suspect code

Each item is a place where the C++ uses a field or argument inconsistently with its
meaning. The asm is the reference. Highest first.

1. **Scratch pointer read while NULL.** `RBR_E48_SpringDamper.cpp:583` (ctor) sets
   `m_unk10 = 0`. `:704` (`Step`) and `:565` (`StepA`) read `float* s = m_unk10` and
   dereference it. The asm of `Step` loads `0x10(%ecx)` and reads `(%eax)` with no test
   (`0x52e2c0`). No writer to `wheel+0xed4` is in `src/`. Either a writer is missing from the
   reconstruction, or the field is not the scratch pointer. A peek settles it.
2. **Aliased outputs at the second `Step` call.** `RBR_E51_WheelD.cpp:807-809` passes
   `b1b4, &c4[0], b1b4` as `o1, o2, o3`. `Step` writes `o1` then `o3`, so `o3` overwrites the
   spring force. The asm pushes three distinct slots (`0x53f12b`-`0x53f152`: `E'+0x5c`,
   `+0x54`, `+0x58`). The first call (`WheelD.cpp:538`, `&l280, &s27c, &l274`) is consistent with
   the asm (`0x53e50b`-`0x53e532`, distinct slots). The outputs of the second call are not read
   in the lines shown, so the effect may be nil there.
3. **Result discarded, lo/hi unused.** `RBR_E51_WheelB.cpp:89-93`: `ComputeTravel()`'s result
   is dropped, and `TravelRange` writes `lo`/`hi`, which are not read in the shown function
   (only `+0xedc` is updated). The call looks like a stub, or the output is missing.
4. **CState `+0x18` is an int and a pointer.** `RBR_E48_SpringDamper.cpp:447` stores the
   ctor argument `c` as `int` (`m_unk18 = c`). Lines `:597` and `:608` dereference the same
   field as `*(void**)(state+0x18) + 0xEB4`. The field works as a pointer only if the ctor
   argument is one. Check the damper-state construction site for what `c` is.
5. **Floor on one damping term only.** `:558` clamps `state[3]` (compression damping) to at
   least 400. `state[4]` and `state[5]` have no floor. The asm of `UpdateForces` (`0x52dd70`)
   was not checked for this line. If the floor is intended for all three, it is missing.
6. **Fade test and increment use different scales.** `:706` tests `27000 < s[0]*F`. The
   increment `:708-709` uses `F` alone: `((F - 27000)/18000)^2 + s[1]`. The asm agrees
   (`0x52e2b0`-`0x52e2e8`: `mulss %xmm0,%xmm2` with `*s`, then `subss 27000` and the
   `1/18000` constant, with no `s[0]` in the increment). Either `s[0]` belongs in the increment
   too, or the test is wrong. Its meaning is unknown.
7. **Dead duplicate.** `StepA` (`:563-578`) repeats the fade block of `Step` and has no
   callers in `src/`. It may be dead code or called through a table not reconstructed.
