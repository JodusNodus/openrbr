# E43 engine continuation sims (turbo, intake, crank cycle): physics semantics

Partial pass. Notes only; no source edits. Scope: the `E43EngineSim` functions on the reached
path. They are CEngine methods (this = the E41 CEngine, def at `+4`, state at `+8`). The
engine kernels proper (`SimE010`..`SimE9F0`, drive, driveline) are in `engine-driveline.md`.
Confidence: `known` (exe string, constant with a clear unit, or an unambiguous equation),
`inferred`, `guess`. Asm: `/opt/homebrew/opt/llvm/bin/llvm-objdump`. `-` = no row in
`notes/progress/gameplay-functions.csv`. reccmp % from `base/RBR.txt`.

## 1. Overview

The `E43EngineSim` block models the air side of the engine: a turbocharger (shaft speed, compressor
and turbine work, a waste-gate), the intake pressure and temperature, and the crank-cycle count.
Evidence (constants in the code, read as physical values):

| constant | value | reading | confidence |
| --- | --- | --- | --- |
| `101300.0f` | Pa | ambient (atmospheric) pressure, the reset value at `Sim80D0` (`s+0x1b8`) | inferred |
| `293.14001f` | K | ambient temperature (20 °C), reset value (`s+0x1a8`) | inferred |
| `287.0f` | J/(kg K) | specific gas constant of air, used with temperature | inferred |
| `9.8716682e-06` | 1/Pa | `1/101300`: converts a pressure to atmospheres (`Sim7D30`, `SimE060`) | inferred |
| `20943.951f` | rad/s | 200 000 rpm: the turbo shaft speed clamp (`Sim7D30`, `s+0x5c`) | inferred (2π·200000/60) |
| `1151.91736f` | rad/s | 11 000 rpm: the engine speed clamp in `Sim8E20` | inferred (2π·11000/60) |
| `100000.0f` | Pa | a second pressure constant (`Sim80D0`, `fp[11]`) | guess |
| `116495.0f` | Pa | boost set-point when the config flag is 0 (`Sim80D0`, `fp[10]`) | guess |
| `20.371832f` | - | scale on the cycle-pressure difference (`Sim8070`, `Sim8E20`) | guess |
| `4.75f`, `1.15f`, `0.05f` | - | scales (damper input, boost limit, turbine term) | guess |
| `0.3f` / `0.7f` | - | first-order filter on the engine speed demand (`Sim8E20`, `state+0x188`) | inferred |

Turbo energy balance (`Sim7D30`, read from the C++). The shaft kinetic energy is
`E = ½ I ω²`, with `I = def+0x18` (inertia, guess) and `ω = s+0x5c`. The code updates it by the net
power `P_net` times `x` (the step, see the call), then recovers `ω = sqrt(2E/I)`, clamped to
`[0, 20943.951]`:

```
t4 = I * ω * ω * 0.5            (initial energy, discarded in the update below)
t4 = ((t8 + lc) - (k_a * p_a² + k_b * ω) * x) + t4      (energy change, waste-gate and damper terms)
if t4 > 0:  ω = sqrt(2 * (t4 / I)) clamped to [0, 20943.951]
else:       ω = 0
```

(`t8` is the waste-gate flow term, `lc` the second term; `k_a = def+0xc8`, `k_b = def+0xb8`, names
guessed from the offsets.)

## 2. Call tree (reached)

```
E43EngineSim::Sim7D30(x)            0x517d30  43.8%  verified   turbo energy step (section 1)
E43EngineSim::Sim8070(i, j)         0x518070  100%    -          cycle pressure difference
E43EngineSim::Sim80D0(i)            0x5180d0  29.1%  -          intake/boost state reset and damper
  CEngine::SimEBB0()                (not read)
  NSpringDamper::DamperUpdateC / DamperCombine   (spring-damper, p2ra notes)
  E41_CurveEta(d+0x2244, s+0x184, 0x10)  (curve lookup)
E43EngineSim::Sim8610(x)            0x518610  11.7%  verified   (not read in depth)
E43EngineSim::Sim8890(x)            0x518890  22.5%  verified   (not read in depth)
E43EngineSim::Sim8B20(x)            0x518b20  42.8%  verified   (not read in depth)
E43EngineSim::Sim8CF0()             0x518cf0  47.4%  verified   (not read in depth)
E43EngineSim::Sim8D90(i, j)         0x518d90  88.4%  -          (not read in depth)
E43EngineSim::Sim8E20(a, b)         0x518e20  30.3%  -          engine speed demand, cycles (2)
  CEngine::SimE370(a*b, &cycles, &delta)   crank count (engine-driveline.md 2c)
```

## 3. Functions

| address | current name | proposed name | what it computes | confidence | status and evidence |
| --- | --- | --- | --- | --- | --- |
| 0x517d30 | `Sim7D30(x)` | turbo_energy_step | section 1 | inferred | verified; 43.8%. The first `t4` is used later (the `(void)t4` is only a warning shape, not a dead value) |
| 0x518070 | `Sim8070(i, j)` | cycle_pressure_diff | `u = (d[0x108 + i*0x40] + j) & 0xff` (d = def); `p[0] = u`; `a = SimEC40(u)`, `b = SimEC40(u-1)`; returns `(a - b) * 20.371832` | inferred | 100%, `-` |
| 0x5180d0 | `Sim80D0(i)` | intake_state_reset | resets slot `i` (`p[0..1] = 0`), calls `SimEBB0`; if config `+0x26f8 == 0`: sets ambient pressure (`s+0x1b8 = 101300`), ambient temperature (`s+0x1a8 = 293.14`), `fp[10] = 116495`; else: a damper update (`DamperUpdateC`, `DamperCombine` into `s+0x70`, `s+0x1dc`) and `fp[10] = s+0x78 * 1.15`, `s+0x1b8 = s+0x68`, `s+0x1a8 = s+0x64`; then `fp[11] = 100000`, `f11 = s+0x1a8 * 287` (gas constant times temperature), `E41_CurveEta` lookup | inferred | `-`; 29.1%. Second `DamperCombine` pointer: see p2ra-final.md |
| 0x518e20 | `Sim8E20(a, b)` | speed_demand_cycles | clamp `b` to ±1151.917 (11 000 rpm); `state+0x184 = b`; `state+0x188 = 0.3*b + 0.7*state+0x188` (filtered demand); `state+0x234++`; if config `+0x26f8`: `Sim7D30(a)`; `SimE370(a*b, &cycles, &delta)`; `e+0x10 = delta`; cycle loop with `×20.371832` | inferred | `-`; 30.3%. Step-40 mismatch: see p2ra-final.md |

## 4. Open questions

1. **Meaning of `config+0x26f8`.** It gates the turbo (`Sim7D30` and the damper branch of `Sim80D0`).
   Settles it: the LSP key that the loader writes there (engine options: a turbo flag, if any).
2. **`def+0x18` (inertia) and `+0xb8`, `+0xc8`.** The energy balance uses these; their key names are
   not identified in this pass. Settles it: the `CEngine` def key table (`engine-driveline.md` 2b).
3. **`SimEBB0`, `SimEC40`, `E41_CurveEta` (table `d+0x2244`).** Called, not read.
4. **Units of `SimEC40` output.** The cycle difference is scaled by `20.371832`. Its unit is not known
   (guess: a torque or pressure per crank step).
5. **The unread reached functions** (`Sim8610`, `Sim8890`, `Sim8B20`, `Sim8CF0`, `Sim8D90`): their
   verified/`-` status is taken from `gameplay-functions.csv` and reccmp; their meaning is not written
   here.

## 5. Suspect code

Asm-checked with `llvm-objdump` on `RichardBurnsRally_SSE.exe`. C++ is in `src/rbr/RBR_E43_EngineA.cpp`
(`Sim80D0`) and `RBR_E43_EngineC.cpp` (`Sim8E20`).

### 5a. `Sim8E20` returns its result in st0, not eax (asm-confirmed; suspect, gameplay)

- Asm: the last x87 action is `flds 0x24(%esp)` at `0x519421`, which loads the clamped `b` (the second
  float argument, slot `entry+8`; rewritten at `0x518e46` when clamped). Nothing pops it. Then
  `popl %edi; addl $0x18,%esp; retl $0x8`. So the caller's return value is st0 = clamp(b). `eax` is
  `cnt/20` only on the path where `cnt % 20 != 0`; on the other path `Sim8B20` clobbers it. No caller
  reads `eax`.
- Callers: `CCar::Fwd18E20` (`0x4e6650`) is `add ecx,0x690; jmp Sim8E20`. The two call sites in the
  original (`0x507a20`, `0x50a239`) run `fmuls 0x73a2e0` directly on st0 (`0x507a25`, `0x50a23e`) and
  then `fstps`. So the original computes `clamp(b) * k`, with `b` the shaft-speed argument (`car+0x45c`).
- C++: `unsigned E43EngineSim::Sim8E20(float, float)` returns `u = cnt/20` in eax (`RBR_E43_EngineC.cpp`
  line 214). `CCar::Fwd18E20` is declared `float` (`RBR_E34_Car.h:637`). The C++ callers
  (`RBR_E40_Mid.cpp:50`, `RBR_E40_Big.cpp:878`) read st0, which holds whatever the last x87 operation
  left. That differs from `clamp(b)` unless the stale value happens to equal it.
- Conflict: `notes/progress/called.csv` and `gameplay-functions.csv` mark `0x4e6650` as verified
  ("identical over 12084 steps vs ref1/ref2", p3r-a1-final2). Either the C++ st0 equals `clamp(b)` on
  that drive, or the gear path is not exercised there. I did not resolve this. Lead to check.
- Second item at the same call sites (not verified): the caller pushes three stack words (`dt`, the
  `car+0x45c` value, and the pointer `car+0x388`), but the callee returns with `ret 8`. The third word is
  not popped by the callee, so the caller must clean it up later. The caller is outside this pass.
- What a C++ reproduction needs: st0 = the clamped `b`, bit-for-bit (the clamp is an exact compare, so
  there is no rounding). The `Fwd18E20` thunk, or a wrapper, has to load that value onto x87; a plain
  `return` of an integer or float from C++ does not do it.

### 5b. `FloorS32` diagnostic logs: NaN differs (asm-confirmed; log only)

- Three checks, each logging `E01Log(6, 5, "FloorS32: Invalid argument %f\n", v)` with `v` =
  `state+0x220 * 0.00136`, `state+0x210 * 0.73746318`, and `state+0x210`. The gate is
  `g_1659b70+0x24 -> +0x160e0` bit `0x200` (`testb $0x2,%dh`) and `g_1659b70+0x58 == 0`. Both match C++.
- Asm (`0x519279`-`0x51930c`, `0x5192d9`-`0x519326`, `0x519331`-`0x519376`): logs when `v < -2147483648.0`
  or `v` is NaN (the `jb` path is taken for unordered compares and reaches the log). It does not log
  `v >= 2147483648.0`.
- C++ (`RBR_E43_EngineC.cpp:190-191` and the two copies): `(v < 0 || 2^31 <= v) && v < 2^31 && v < -2^31`,
  which reduces to `v < -2^31`. NaN is not logged.
- No effect on state. Recorded so that the diagnostic text is not treated as a gameplay difference.

### 5c. Checked, no difference

- `Sim8E20` clamp of `b` to `+-1151.91736` (`0x518e20`-`0x518e4c`); NaN passes unchanged.
- Filter: `state+0x188 = b*0.3 + old*0.7` (`0x518e7c`-`0x518ea3`).
- `Sim7D30` gate, the `SimE370(a*b, &cycles, &delta)` argument order (`0x518ec0`-`0x518f25`), and the
  `cycles < 1` gate.
- Cycle loop (`0x518f90`-`0x51917c`): `u6 = (cfg[0x108+off] + state[0x17c]) & 0xff`, `u8` stride `0x38`,
  `s7 = cfg + u6*0x20`, `f11`/`f12` gating on `state+0x90` (alternate `0x208`/`0x1e8`), both brackets, and
  `f13` accumulation. `b = f13`. For `cycles >= 1`, `divss` by `delta` at `0x518f5f`.
- Tail (`0x519189`-`0x519421`): `SimE520(a)` (`0x519190`); `Emit` on `g_signalManager+0x6050` with
  `*(e+8)+0x2c` (`0x519198`-`0x5191a8`); `SimE660(a)` = `g9` (`0x5191b0`); `t8 = state+0x184*(g9+b)`;
  `t4 = state+0x210*0.95 + (g9+b)*0.05`; `f10 = t8*0.05`; `state+0x220 = state+0x220*0.95 + f10`; `SetDirty`
  on `+0x204` and `+0x214`. The `g9+b` is an x87 add stored with `fstps`, so it is rounded once to float,
  the same as the C++ float add. Constants `0.95` (`0x7355d4`), `0.05` (`0x73cadc`), `0x3af4fe91` (graph
  scale, `0x73cad0`), `4.484305e-06` (`0x73cacc`). Graph `Push` order `0x274` then `0x278`.
  `cnt % 10 == 0` gives `Sim8890(a*10.0)` (`0x72b154` = 10.0); `cnt % 20 == 0` gives `Sim8B20(a*20.0)`
  (`0x72f614` = 20.0).
- `Sim80D0` damper branch (`0x518109`-`0x51817e`): `DamperUpdateC(damperState, 287)` and
  `DamperCombine(state+0x70, damperState, 287)`, with the readback `state+0x1dc = damperState[1]`. This
  answers the P2RA second-`DamperCombine`-pointer question: the pointer is `damperState` (`B+0x14`), and the
  readback slot is `B+0x18`. Matches C++.

### Not checked in this pass

- `Sim8D90` (`RBR_E43_EngineC.cpp:43-64`) and `SimEC40`: not asm-checked.

## Notes for the lead

- Partial pass. `Sim80D0` and `Sim8E20` are asm-checked end to end (section 5). Their step-40 mismatches
  are in `p2ra-final.md`, which this pass does not repeat. The one new gameplay item is 5a (st0 return).
- The turbo identification (energy balance, 200k rpm clamp, air gas constant) is an inference from the
  constants; confirm with the turbo key names in the LSP before renaming.
