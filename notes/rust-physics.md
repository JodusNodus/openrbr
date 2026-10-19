# Rust physics: float precision

Scope: x87 precision, default-NaN comparisons, and float-to-int behavior for Rust physics ports.
Sections 1-7 began with the CTyre pilot (`lead/briefs/rust-pilot.md`, step 1); section 8 records
the NaN and conversion rules shared by the tyre, spring-damper, and VCU ports. Evidence is the
original 1.02 exe (`RichardBurnsRally_SSE.exe`, disassembled with llvm-objdump) and the import
table, plus the committed original-call vectors cited below.

## Summary

- The game never runs the x87 with 64-bit precision. The only precisions in play are
  24-bit (D3D default) and 53-bit (Windows default, and two scoped `_controlfp` calls).
  So no extended-precision (80-bit) soft-float type is needed.
- Which of 24 or 53 is live during `CPhysics::StepForward` is **not yet confirmed**.
  Static evidence points to 24 on the main thread once the D3D device exists.
- Arithmetic still reduces to f64 in Rust, with a round-to-24 helper for the 24-bit
  case. SSE scalar code is f32 in both cases. The `fsin`/`fcos` result uses the narrow
  `math::trig::X87Ext80` type because those instructions return 64 significand bits.
- No CRT transcendental (sin, cos, atan2, pow, exp) is called by the tyre path. The
  wheel path in E50 uses `fsin`/`fcos`, now modeled in `math::trig`.

## 1. Control word

Import table (`MSVCR71`): `_control87` 0x729580, `_controlfp` 0x729600. `d3d9`:
`Direct3DCreate9` 0x729764.

- **Device creation** (CRT-free window/D3D init at 0x40b210). `Direct3DCreate9` is
  called with `D3D_SDK_VERSION` 0x1f at 0x40b22f. `IDirect3D9::CreateDevice` is the
  vtable call `call dword ptr [edx+0x40]` at **0x40c418** (slot 16).
  Behaviour flags (0x40c3a8-0x40c3e6): base 0x40 (hardware) or 0x50 (hardware+pure)
  or 0, `|= 0x20` if `[esi+0x6e0]`, and `|= 0x4` always. **0x02
  (`D3DCREATE_FPU_PRESERVE`) is never set.**
- **D3D behaviour.** Per Microsoft's `D3DCREATE` reference, without
  `D3DCREATE_FPU_PRESERVE` Direct3D sets the FPU to single precision, round to nearest.
  The docs are inconsistent on whether the flag applies to plain D3D9 (`D3DCREATE` lists
  it under 9Ex only). This is consistent with the flags above, so the default applies.
  Wine's d3d9 behaviour is not checked here; the runtime check (below) covers it.
- **E09 timer.** `E09Timer::Init` (0x465be0) and `E09Timer::Now` (0x465c90) call
  `_control87(0, 0)` to read, `_control87(0x10000, 0x30300)` (53-bit, nearest) for one
  computation, then `_control87(old, 0x30300)` to restore. Asm at 0x465c06-0x465c2a.
  Matches `src/rbr/RBR_E09_Math.cpp:23-28, 57-60`.
- **Two `_controlfp` scopes in D3D-side code**, both set 0x10000 under mask 0x30000,
  read the old value first and restore it: around 0x62dce6-0x62dd16 (inside the function
  at 0x62dc4f) and around 0x649a80-0x649b35 (inside the routine at 0x6499d4). Their
  callers are not identified. They are not called from the physics path as far as the
  survey covers, and they restore the value, so they only matter if a physics step runs
  inside one.
- **`fldcw`**: 115 sites, all save/restore pairs (`fnstcw`, then `fldcw` of a saved word
  or a rounding change, e.g. `_ftol`). No site loads a fixed 0x37f (64-bit) control
  word. The large `mov ..., 0x27f/0x37f` tables near 0x4bcf45 and 0x4bd3dc are data, not
  FPU words.
- **Threads.** The exe creates one extra thread (`CreateThread`, 0x4d46e0), which loops
  on a voice ping and sleeps. No physics runs there. The control word is per thread, so
  the main thread (where D3D was created and where `CApplication::Tic` steps physics)
  decides the precision. Confirm on the main thread at runtime.

Consequence: after device creation the main thread runs at 24-bit precision unless
something changes it. Only the scoped E09 and `_controlfp` regions run at 53-bit.

## 2. Classification of float operations

The tyre functions (64 verified or exact, `lead/briefs/rust-pilot-tyre.txt`) mix SSE
scalar and x87 code. None calls a CRT transcendental. Their external calls are tiny
helpers (copies, constructors, `FloorS32`, `TyreFrictionCurve`, `DamperStep1/2`,
`SplitSlip` and similar). Counts below are from a mnemonic scan (approximate).

| Class | What it is | Where (examples) | Rust rule |
| --- | --- | --- | --- |
| S | SSE scalar `addss/subss/mulss/divss/sqrtss` on f32 | most of `StepInner`, `Friction`, `TyreFrictionCurve` | `f32`, same operation order |
| M | one x87 op, then `fst/fstp dword` (rounds to f32 at the store) | `DamperStep1` (`fld,fsub,fmul,fmul` then return), `SolveDirs` store at 0x533656 | compute in f64 then `as f32` at the store |
| C | x87 chain, no store between ops (st0 carried) | `SolveA` 0x533ae0 (19 x87), `SolveB` 0x533bf0 (18), `ClampGrip` 0x533760 (12), `StepInner` 0x535602 (`fmul,fmul,faddp,fsqrt`) | per-op rounding at PC (see below) |
| R | float function whose result is returned in st0 without a store | `DamperStep1/2`, `TyreFrictionCurve` (branch 2 returns the chain `(d-e)*u+e` in st0), `FloorS32` | returns must carry the PC precision (f32 at 24-bit is exact; at 53-bit the caller needs the f64) |
| I | `fild/fistp`, `cvttss2si` | `FloorS32` 0x5325f0 (`fild`, `fadd`, `fmul`) | integer conversions are exact |
| T | `fsin/fcos`, `fpatan`, `fyl2x`, `f2xm1` | `__CIsin` 0x53f3b0, `__CIcos` 0x53f3a0 in E50 | `fsin/fcos` modeled in `math::trig`; other transcendentals deferred |
| Q | `fsqrt` | `SolveDirs` 0x533668, `SolveA` 0x533b57, `DamperSqrt4`, `Friction` 0x531f41 | correctly rounded at PC: f32 `sqrt` at 24, f64 `sqrt` at 53 |

Per function, approximate x87 instruction counts and SSE scalar arithmetic counts
(`/tmp`-scan, reproducible with the same asm dump):

| Function | x87 | SSE scalar | Notes |
| --- | --- | --- | --- |
| DamperStep1 0x52e350 | 4 (C+R) | 0 | `(a-b)*c*d` as one x87 chain, returned |
| DamperStep2 0x52e370 | 4 (C+R) | 0 | `-(a*b*c)`, returned |
| TyreFrictionCurve 0x5334d0 | 9 | 25 | returns st0 from branch 2 (chain) and branch 1 (stored to `[esp+0x14]`, reloaded) |
| SolveDirs 0x533640 | 13 | 6 | `c = a*s + c` (x87) stored at 0x533656; `sqrt(b*b+c*c)` (x87) stored; comparisons on x87 |
| Friction 0x531c70 | 13 | 25 | mix; `fsubr`/`fsqrt` at 0x531f3b-0x531f43 |
| FloorS32 0x5325f0 | 15 | 0 | `fild`, `fadd` of a constant at 0x795e10, `fmul`, `fsub st,st1`, `faddp`; out-of-range path calls the CRT error helper 0x43c8e0 with a double |
| StepInner 0x535310 | 22 | 41 | chains at 0x535602-0x53561a, 0x5356d5-0x5356fb, 0x535700-0x535721 (`fmul`,`fadd`,`fstp`), return via st0 at 0x535745 |
| SolveA 0x533ae0 | 19 | 7 | `fsqrt` then chain to `fstp`/`fst` |
| SolveB 0x533bf0 | 18 | 14 | chain with `fdiv`, `fsqrt`, `fst` |
| ClampGrip 0x533760 | 12 | 13 | `fsqrt`, `fdivp`, `fstp` |

Two rules from the asm, not the C++, decide the order:

- Some C++ expressions reassociate under MSVC (notes/shadow-exe.md: `ComputeSlip`,
  `Sign`, `DamperCombine`, `SimEC40` all mismatch by 1 ulp in shadow runs because of
  operation order). The Rust port must follow the asm's operation order, not the C
  expression.
- An SSE step that the asm stores to f32 (e.g. `Sign` 0x531bb3-0x531bbe rounds
  `f[0x30c]*f[0x298]*0.5` to f32 before the x87 part) is a rounding point in both
  precisions. Keep it.

## 3. Rust reproduction

The key result is that the x87 precision is either 24 or 53, never 64. Per x87 op the
result is correctly rounded to the precision, with the exponent range not limited
(only the store to memory limits it).

- **S, M, I, Q** at PC=24: plain `f32` ops. Exact.
- **S, M, I, Q** at PC=53: `f64` ops, `as f32` at stores. Exact (same two roundings as
  the hardware: round53 then round24).
- **C** at PC=24: each step is round24 of the exact result. Run the chain in f64 and
  apply `round24(x)` after each op (round to 24 significant bits, ties to even, exponent
  unbounded in f64 range). This is exact for every intermediate, including values below
  `f32::MIN_POSITIVE` where plain `f32` would lose bits. The double-rounding argument
  holds because 53 ≥ 2·24+2 for `+ - * / sqrt`. Plain `f32` arithmetic is exact only
  when no intermediate leaves the normal `f32` range; that can be checked on the traces.
- **C** at PC=53: plain `f64`.
- **R**: the function returns the chain's type (`f32` at 24, `f64` at 53). The caller
  stores it or keeps it, as in the asm.
- **T**: `fsin/fcos` ignore the arithmetic precision control for their returned value.
  `math::trig::{fsin,fcos}` returns `X87Ext80`, uses Intel's 66-significant-bit Pi for
  argument reduction, and rounds the reduced sine/cosine to 64 significand bits. Inputs
  with magnitude at least 2^63 are returned unchanged, as the x87 does when range
  reduction fails. Direct comparison with 1,283,725 cached FSIN/FCOS inputs gives
  1,242,777 exact matches and 40,948 results one extended ULP away; 12 of those also
  differ after conversion to f64, and none differ after conversion to f32. A separate
  cachy fuzz set of 3,955 f32 angles gives 7,720 exact and 190 one-ULP results, with no
  difference after f64 or f32 conversion. The remaining one-ULP cases are the microcode
  approximation's final-bit differences; test evidence is in
  `rust/rbr-physics/tests/data/x87-trig-fuzz.txt` and the cache comparison test.

There is no general soft-float type. `X87Ext80` covers only the trig result and its
conversions; `round24` remains a few lines of Rust (round-to-nearest-even on the
significand), not a library.

## 4. Open questions and the runtime check

1. Which precision is live in `CPhysics::StepForward` on the reference runs? Test: a
   logging capture (`fnstcw` at the entry of each captured function and at StepForward)
   in step 2. One run decides it.
2. Wine versus Windows. The reference recordings are made under Wine on cachy. Wine's
   d3d9 may or may not set single precision the way Windows does. The capture on the
   reference runtime settles it for the Rust port's purpose (match the recordings).
3. Intermediate range: confirm on the traces that no chain intermediate is subnormal in
   f32 (the `round24`-on-f64 plan does not need this, but pure `f32` does).
4. The E50 wheel path uses `__CIsin/__CIcos`; its `fsin/fcos` primitive is modeled, but
   callers still need their own checked port notes.

Recommendation: target 24-bit (confirm in step 2), implement classes S/M/I/Q with
`f32`, implement class C with f64 plus `round24`, keep the 53-bit variant as one
function parameter so the port can switch once the runtime is known. Capture mode (step 2)
should log the control word per call, and the test vectors record the outputs in the
chosen precision.

## 5. Runtime result (step 2)

The capture (`RBR_TRACE`, `harness/README.md`) logged 1,322 calls across 24 eligible
tyre leaves on the reference drive (Tanner, Corolla). Every call has `cw_in = cw_out =
0x007f`: precision control is 24-bit on the physics path, as the D3D default predicts.
Check: `DamperStep1` (0x52e350, `((a-b)*c)*d` as an x87 chain) matches an f32 chain for all
64 logged calls; an f64 chain would differ on all 64. `trace_info` reports the same.

Two properties of the x87 that the plan depends on:

- Precision control rounds arithmetic results only. `fld`/`fst` of doubles are exact.
  So a chain can carry a double value through a store with no rounding; only the
  arithmetic is rounded to the precision.
- The x87 exponent range is wider than f64's. Chains that can overflow or underflow f64
  (intermediates beyond about 1e±308) are not covered by an f64 emulation. The tyre
  inputs give no such values in the trace, but the port must check any new chain.

## 6. Ports and vectors (step 3)

`rust/rbr-physics` (x87 model in `x87.rs`, game memory in `mem.rs`, leaves in `damper.rs`
and `tyre.rs`). `tests/vectors.rs` replays every logged call of a ported function from a
capture (`RBR_PHYSICS_TRACE=<trace>`; skipped without it): the pre-state is mapped, the port
runs on the recorded arguments, and the st0 bits and every region's post-state must match.
Result on the reference capture (`pilot-on4`, 64 calls per function, cw 0x007f throughout):
all 24 eligible tyre-path leaves match, 1,300+ calls in total, bit for bit.

| Address | Function | Port |
| --- | --- | --- |
| 0x52d960 | CU32Value::Clear | `damper::clear_value` |
| 0x52da40 | CSpringDamper::CState::SetUnk10 | `damper::set_unk10` |
| 0x52e350, 0x52e370 | DamperStep1, DamperStep2 | `damper::step1`, `step2` (x87) |
| 0x52e390, 0x52e590, 0x52e5a0, 0x52e5e0 | DamperBiasSub, DamperScale, DamperSqrt4, DamperCurve | `bias_sub`, `scale`, `sqrt4`, `curve` (x87) |
| 0x52e3a0, 0x52e3d0 | DamperAccumA, DamperAccumB | `accum_a`, `accum_b` (SSE) |
| 0x52e4c0, 0x52e4f0, 0x52e530 | DamperUpdateA, B, C | `update_a`, `update_b`, `update_c` (SSE) |
| 0x52e630, 0x52e6d0, 0x52e8a0 | DamperCombine, DamperRelax, DamperForce | `combine`, `relax` (SSE + x87), `force` |
| 0x531ba0 | CScratch::Sign | `tyre::sign` (x87) |
| 0x5322e0 | TyreClamp | `tyre::tyre_clamp` (SSE) |
| 0x532490 | CScratch::Set414 | `tyre::set_414` |
| 0x532a30 | CScratch::ComputeSlip | `tyre::compute_slip` (SSE + x87 sqrt) |
| 0x533640 | CScratch::SolveDirs | `tyre::solve_dirs` (x87) |
| 0x5338a0 | CScratch::Accumulate | `tyre::accumulate` (SSE) |
| 0x535040 | CTyreMatData::CopyFrom | `tyre::copy_mat_data` |
| 0x536880 | VcuTableLookup | `tyre::vcu_table_lookup` (x87 interpolation) |

Ports follow the asm's operation order, not the C++ expressions. Where the C++ and the asm
differ, the asm won (`ComputeSlip` stores in a different order; `Accumulate` multiplies by
0.0f in the source and in the asm).

Coverage caveats, from the trace (counted per branch):

- Only the first branch of each conditional is exercised on this drive: `relax` (p0 > x),
  `curve` (1.0 > a), `sqrt4` (a > 0), `update_a/c`, `force` (p0 != 0), `update_b` (p1 != 0,
  10 calls). The other branches (zero guards, curve's `a >= 1`, sqrt4's zero path, relax's
  constant branch) are ported from the asm and are NOT verified by the vectors.
- `Sign`, `TyreClamp`, `SolveDirs` branches: `TyreClamp` and `SolveDirs` exercise several
  branches, not all.
- x87 chains are checked at 24 bits only (the reference's precision). A 53-bit build of
  the same code would need `x87::r24` removed; nothing here reads the precision.

Not ported (no vectors yet):

- Not eligible for capture (transplant.py leaves them out): `FloorS32`, `TyreFrictionCurve`
  (calls FloorS32), `SolveA`, `SolveB`, `SolveDispatch`, `SolvePatch` (they call), `ClampGrip`
  (stores through a loaded pointer), `SplitSlip` (register inputs differ), `TableStep`,
  `RelaxStep`, `SolveForces`, `FillOutputs`, `WriteOutputs`, `Friction`, `Align`, `Init`,
  `BlendTable`, `SetState48`, `SetStateField`. These need a capture that doesn't require
  shadow eligibility (a trampoline is enough: capture runs the original once).
- The wheel path (E50, `fsin`/`fcos`), outside this pilot.

Other finding: the reccmp scores in this worktree's build (main at 3754fd2e) are not the
progress statuses in `lead/briefs/rust-pilot-tyre.txt` for many of these functions
(`Sign` 19.6%, `FloorS32` 38.5%, `SolveDirs` 45.4%, `Accumulate` in the 30s-50s). The
status list may come from another branch. The ports don't depend on the byte matches, but
the transplant's "exact" claims do.

## Sources

- `IDirect3D9::CreateDevice` / `D3DCREATE` reference (`learn.microsoft.com`): default
  is single precision unless `D3DCREATE_FPU_PRESERVE` is passed.
- `src/rbr/RBR_E09_Math.cpp:23-28, 57-60` (E09 53-bit scope).
- exe: 0x40b22f, 0x40c3a8-0x40c3e6, 0x40c418, 0x465c06-0x465c2a, 0x62dce6-0x62dd16,
  0x649a80-0x649b35, the tyre asm listed above.
- Intel 64 and IA-32 Software Developer's Manual, Vol. 1, §8.3.8, “Approximation of Pi”.

## 7. What replay can and can't prove

Capture records, per call of a function with an eligible trampoline: the registers on
entry, the first 16 stack argument words, the x87 control word, and each input window before
and after the call. Windows are the default ones the plugin takes around each register and
stack-argument word that points into writable memory: `[v - 0x40, v + 0x800)`, capped at 0x880
bytes and at the writable region, at most 16 windows; stack pointers clip to the caller's
argument area; one hop through `this + 0x10` and `this + 0x14` as well. A call also records
its returns, popped bytes, st0 and cw_out.

Measured with a capture-all run over the tyre list (the mode is dropped from the harness
now; the drives were identical to ref1 over 12,084 steps, 32 calls per function):

- `TyreFrictionCurve` (13 arguments) records all 13 (popped = 52).
- Its output pointers, args 9 and 11, are stack addresses (0x1a4f894, 0x1a4f898) that no
  captured window covers, so its writes to them are not compared. Cause not settled; the stack
  clip in `AddRegion` is the first suspect.

Replay soundness:

- A port reads only through `Mem`, which panics on an unmapped byte. A read outside the
  windows fails the test loudly.
- A port that skips a read the original makes (a global the asm loads, say) isn't caught by
  the trace. Only the asm review catches that.
- A write outside the windows is not compared. Until the stack-pointer windows are settled, a
  port of a function whose outputs go to such pointers can't be verified by the trace.
- Nesting: a captured function that runs inside another captured one is logged by neither
  (the plugin's capture lock). The tyre run that had a nested entry (SolveDispatch under the
  RelaxStep/StepInner level) showed no static call path to it; cause not settled.

## 8. Platform: default NaN

The original (x86) and the host (arm64) can produce different default-NaN signs for invalid operations such as
`inf - inf`, `0 * inf`, or `sqrt` of a negative. Comparisons and arithmetic on NaN do not see the sign. Per rule 6
of `lead/briefs/rust-readability.md`, ports do not reproduce a particular sign; tests accept NaN for NaN only at
the sites below. Finite values and all other words remain bit-exact.

Tolerated sites (`rust/rbr-physics/tests/tyre_vectors.rs`):

- `solve_a`, `solve_b`, `solve_c`, `solve_d`, `solve_dispatch` (`replay_solve`, `same_words`): the contact
  solvers divide and take `sqrt` of values that can be NaN from finite operands.
- `step_inner`, `solve_patch` (`replay_inner(.., true, ..)`): the `arg3` and `arg4` words (`this+0x3b8`, `+0x3e0`)
  of fuzz call 149 of `step_inner`, and the same two words plus `this+0x4c0` of fuzz call 152 of `solve_patch`. The
  original writes a negative NaN (`0xffc00000`) where ours is positive; only the sign bit differs.
- `contact_patch` (`replay_inner(.., true, ..)`): the `this+0x308` and `this+0x30c` words of fuzz call 233. The
  `sqrt` of a negative `s8` gives the other NaN sign; the bits differ only in the sign.
- `solve_forces` (`replay_inner(.., true, ..)`): the `out` vectors at `arg1`/`arg2`, one lane of one call (drive call
  989 of `solve_forces`). The original writes the negative NaN (`0xffc00000`) where ours writes the positive one;
  only the sign bit differs. The NaN's source is not traced.
- `friction` (`replay_inner(.., true, ..)`): the `sqrt(1 - local²)` step, where `1 - local²` can be negative.
  The window `m_state+0x80` holds the result.
- `heat_step` (`replay_inner(.., true, ..)`): the window words of a fuzzed call. The original writes a negative
  NaN (`0xffc00000`) at `m_wear` where ours writes the positive one; the difference is only the sign bit, found
  by the strict comparison (call 142 of the fuzz set).
- `table_step` and `relax_step` (`replay_inner(.., true, ..)`): the `m_state+0x12c` word of one fuzz call (call
  120 of TableStep, 121 of RelaxStep). The original has a negative NaN where ours is positive; the difference is
  only the sign bit, found by the strict comparison.
- `friction_curve` (`replay_inner(.., true, ..)`): the out-pointer word `args[9]` of one fuzz call (call 1037 of the
  fuzz set). The original writes a negative NaN where ours is positive; only the sign bit differs, found by the
  strict comparison.
- `sign` and the st0 check in every x87 test: `ext_to_f64` in `harness/vcu_vectors.py` writes every NaN as
  `+NaN`, so the sign of a recorded st0 is not known. The test accepts any NaN there.

Step runs (`harness/rust_step.py`, `nan_sign_only`): a word that is NaN in run A and run B and differs only in bit 31
is the class "default-NaN sign", excluded from the run-B comparison (with `--steps`, only a byte that differed so at
every step it differed). From lcb1's snap-0 the lane-3 garbage of the car's vectors grows to `inf` and then to an
invalid operation's NaN (the scratch lane at `0x7c706c`: 414.9, `inf`, then `0xffc00000` in the original, written by
`ComposeA` 0x51b4c0 and read by `E46Mat::ScaleRows`): `0x7fc00000` here. It spreads through lane 3 under
`CSolid::E43Vf6C` 0x52cc30 and `CCar::Vf64x` 0x4e9020 (car+0x14c, car+0x15dc, wheel+0xa2c, ...). The operation that
makes the first NaN is not traced. A difference the sign causes in other bits still fails.

Engine and driveline (`tests/engine_driveline_vectors.rs`, `check_st0`): the st0 of the x87 returns (`E41_TableLerp`,
`E41_CurveEta`, `CEngine::Sim410`, `CDrive::Get4520`, `CDrive::Get4540`) accept any NaN. The windows of
`CEngine::SimE060` (0x50e060) and `CEngine::SimE1F0` (0x50e1f0) accept any NaN for a NaN word (`check_windows_nan`): the
`0/0` of `DamperCombine` (`denominator / target.scale`, both zero) writes the default NaN, `0xffc00000` in the original. The windows are compared bit for
bit; a NaN there comes only from propagation of a NaN input (no 0/0 or inf-inf in the batch-1 fuzz), so the sign is not
tested yet. A port that divides in a window-writing path needs its own entry here.

Float-to-int: `floor_s32` (`curve.rs`) takes the table index as `cvttss2si` does: `fpu::trunc_i32` for an
in-range value, with the original's clamps (`i32::MAX` from `2^31` up, the integer indefinite below `-2^31` and
for NaN) written as comparisons. `friction` and the other ports have no `as` conversion from a float, and
`E16_Sign` is inlined as a float sign (see `friction.rs`).

Tolerated sites (`rust/rbr-physics/tests/spring_damper_vectors.rs`):

- The ST0 returns for `DamperStep1` (`0x52e350`), `DamperStep2` (`0x52e370`), `DamperBiasSub` (`0x52e390`),
  `DamperClamp` (`0x52e400`), `DamperScale` (`0x52e590`), `DamperSqrt4` (`0x52e5a0`), `DamperCurve`
  (`0x52e5e0`), and `DamperSolve` (`0x52e760`) accept any NaN sign/payload.
- `check_spring_windows` accepts a differing dword only when both expected and actual decode as f32 NaN. The
  arithmetic output sites are the totals from `DamperAccumA/B` (`0x52e3a0`, `0x52e3d0`); the weighted value
  from `DamperClamp/Merge` (`0x52e400`, `0x52e460`); the response, scale, or numerator written by
  `DamperUpdateA/B/C` (`0x52e4c0`, `0x52e4f0`, `0x52e530`); the weight/value/response written by
  `DamperCombine` (`0x52e630`); the relaxation state written by `DamperRelax/Solve` (`0x52e6d0`, `0x52e760`);
  force terms written by `DamperForce` (`0x52e8a0`); and the three outputs written by `CSpringDamper::Step`
  (`0x52e090`). Setup, constructor, copy, and reset fields have no arithmetic NaN exception.

### Default NaN: car-body and entities sites

Tolerated sites (rule 6 of `lead/briefs/rust-readability.md`). The sign of a default NaN is not compared.

- `math::quat::to_quat` (`E34Quat::ToQuat`, 0x4e4d10): `sqrt` of a negative argument in the diagonal branch.
  The recorded outputs are `0x7fc00000` (the emulator's x87 `fsqrt`); the cachy CPU would give `0xffc00000`.
  `tests/car_body_vectors.rs` (`same_word`) accepts any NaN there.
- `math::quat::to_quat` and `car_body::steering_ffb::update` (`CCar::UpdateFfbTorque`): the vector test compares NaN
  outputs with any NaN (`same_word`, `tests/car_body_vectors.rs`); the port keeps the asm's NaN branches.
- `CHingedRigidBody::SimEAD0` (`entities` `hinge_solve`, 0x51ead0): the settle value at +0x624 (`b4 / b0 - settle`); a fuzzed call
  makes a NaN there (the operation that makes it is not traced). The original writes a negative NaN, the port the host's positive one.
  `tests/entities_solve.rs` (`hinge_solve_matches_original_calls`) stores the recorded NaN at that word, after checking
  that the port's value is a NaN exactly when the record's is.

- `CHingedRigidBody::SimCFF0` (`hinge_slip::slip_term`, 0x51cff0): its st0 is a NaN in the fuzzed SimEAD0 runs
  (`ead0-f1`, `ead0-f3`, call 100), where the original's is negative and the port's positive. `tests/entities_slip.rs`
  accepts any NaN when the record's st0 is NaN.

- `tyre::write::write_outputs` (`CScratch::WriteOutputs`, 0x533080): fuzz call 3858 writes six default NaNs
  (`0x7fc00000` in the port, `0xffc00000` in the record) at window offsets 1248 to 1276. Every differing dword is a
  NaN that differs only in its sign bit. The NaN's source is not traced.
  `tests/tyre_vectors.rs` (`write_outputs_matches_original_calls`) compares that window NaN-tolerantly (`same_words`).

Float-to-int: `vcu::tables::knot_index` uses `fpu::trunc_i32`, with the asm's explicit upper-bound case retained.
The spring and tyre ports have no float-to-int conversion; `E16_Sign` is inlined as a float sign (see
`friction.rs`).

Raw bits: the tyre ports read no float as an integer, except `ComputeOutputs` (`tyre/outputs.rs`), which copies
two dwords from `src+0x36c` and `+0x370` to the outputs with no float operation. Their type is not known, so the port
keeps them as `u32`. The reached spring-damper ports have none. The spring `CState::SetUnk10`
entry accepts integer bits and creates an `f32` with `from_bits`; it does not read an existing float
as an integer. Spring layout code otherwise reads and writes `f32` values with `mem.f32` and
`set_f32`. See the spring-damper README for the function-level audit.

## Vector store

`scripts/rust-vectors.sh` runs every `rust/rbr-physics` vector test against the record store
(`$RBR_VECTOR_STORE`, default `/Users/thomas/worktrees/rbr-recomp/vectors`, one subdir per module;
not committed). Each module's subdir holds the current record files for its `RBR_*_VECTORS` variable.

## Before porting: grep for the address

Before porting an original function, grep `rust/rbr-physics/src` for its address
(`grep -rn "0x......." rust/rbr-physics/src`). Several functions already have a Rust
implementation in another module (kept in the owning module, or in `src/math/` for pure
math); a second copy must call the first instead of reimplementing it. Other hits are
usually host-trait references naming the callee as an external, which stay as they are.

## Port manifest

`rust/rbr-physics/ports.csv` lists each ported function: its original address, Rust module and item, the vector
test that checks it, and its status (`checked` against original records, `unit` for unit tests only, `partial` when
some paths have no record; the module README says which). Each port adds its row to `ports.csv` in the same commit,
kept sorted by address. `tests/ports_manifest.rs` checks that every row's item and test exist, and
`scripts/progress.py report` prints the Rust section per subsystem with the reached-but-unported worklist.
No row exists for functions with no record in the store: 0x51c810 (`ground_make`), 0x51be56 and 0x51bf3c
(`first_pass_hit`). 0x528980 is one function, not two copies: `tyre::locate::locate_at` (the row) calls
`math::table::table_index_frac`, which `physics_core_vectors::table_index_frac_matches` replays.

Callees: a function a port calls that belongs to another subsystem is called through its typed function when it has a
row in `ports.csv`, and is a host method otherwise (one method per original callee, its address in the doc comment),
answered from the inner records in the tests. A subsystem's own functions are ported, not hosted.
C++ items found while porting go to `lead/briefs/cpp-fixes-from-rust.txt` with the file's highest number plus one.

## Gaps for pure Rust

- Tyre `Step` (0x5360c0) and `Locate` (0x528980) decline on table indices outside the window: a NaN walk that reads past
  -1, or a hint below -1. The fuzz records reach a few (3 each). Pure Rust will need the def's raw words there. See
  `rust/rbr-physics/src/tyre/README.md`.
- Tyre `HeatStep` (0x534090) reads the mix and shape tables through the state's indices with no check; the test declines
  indices past each table (25 calls). The original reads the snapshot there, so pure Rust needs those words.
