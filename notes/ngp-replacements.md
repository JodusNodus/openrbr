# NGP replacements for physics call sites (RBR_E33..RBR_E51)

Scope: the 65 `call` rows of `original/ngp/analysis/patches-runtime.tsv` whose src
file is `RBR_E33..RBR_E51` (45 distinct NGP targets). No source changes; analysis only.
Inputs: `patches-runtime.tsv`, `NGP.dll.c` (Ghidra decomp of NGP.dll 7.4.778),
original `RichardBurnsRally_SSE.exe` (objdump), our `src/rbr` reconstructions.

## Method

- Original callee: disassembled the exe at `site-1` (site = rel32 operand address).
  64 sites are `E8 call`, 1 site (`004e6657`) is an `E9 jmp`. All 42 distinct
  original callees are reconstructed in `src/rbr` (names below from `// FUNCTION`).
- NGP target behaviour: read each target body in `NGP.dll.c` (+ one level of
  callees), compared against our reconstruction of the original callee.
- Read-only inspection of `original/ngp/7.4.778/Plugins/NGP.dll` with objdump
  (sections, constants); nothing was executed.
- Guesses are marked "(guess)".

## Shared mechanisms (read once; not repeated per row)

- **Trampoline to the original.** Many NGP helpers have the shape
  `FUN_101109d0(exe_addr)` → cache in `DAT_101775xx` → tail-call with identical
  args (e.g. `10060140`→`0x52e760`, `1005b370`→`0x5320c0`, `1005b540`→`0x5360c0`,
  `1005bb70`→`0x534090`, `10059e80`→`0x52e090`, `100598a0`→`0x52dd70`,
  `1004b800`→`0x518e20`, `10045df0`→`0x509bc0`, `10045d20`→`0x506b20`,
  `100466b0`→`0x504060`, `10047650`→`0x509210`, `10047850`→`0x509120`,
  `1004d060`→`0x51b4c0`, `1004dce0`→`0x520940`, `10033fc0`→`0x61e1b0`,
  `10063f10`→`0x601fb0`, `1005a9f0`-family→`0x5338a0`).
  `FUN_101109d0(a)` calls `FUN_100e8ac0(a - 0x400000)` (exe VA → file offset),
  i.e. it resolves the pristine original behind the patch. A target that reaches
  the *replaced* address this way is a `wrapper`; one that only resolves
  *helpers* the original also calls is a `rewrite` reusing original code.
- **Emit-wrapper shell.** 18/45 targets share one shape: get NGP car context
  (`1008cf10`, TLS-based), read flag byte at context+`0xd` (`1003c8a0` is the
  identity fn); if clear, call the reimplemented fan-out inline, else copy the
  signal struct to locals and hand (fn-pointer, args) to a per-signal
  dispatcher (`100ebb50`-family: checks thread-local state, walks a list —
  looks like marshal-to-physics-thread, exact queue mechanics unverified).
- **Reimplemented fan-out (all 18 inners byte-identical modulo names).**
  `100ed2c0`-family = our `E34MultiSig::Emit` on the same layout
  (guard +0, count-ptr +4, overflow head +0x208, slot[count].head +(count*16+8),
  virtual call at conn+4, overflow first then slot). Drops the original's
  `count >= 0x20` debug-break and the reentry log. Signal sizes vary (see table).
- **NGP sections.** `.rdata` @0x10145000 (fixed consts), `.data` @0x10171000
  (runtime tunables, rewritten per stage — file defaults differ from the TSV
  runtime values), `DAT_10174xxxx+` = runtime-allocated state (timers, tables,
  resolved pointers). `DAT_101459c0` = 0x7fffffff (fabs mask),
  `DAT_10146cd0` = 0x80000000 (sign mask), `DAT_10147118` = 1.0.

## Table (grouped by NGP target; all 65 sites listed)

`orig` = original callee verified by disassembly. Verdicts: `rewrite` / `wrapper` /
`new` / `unclear` (none). Signal byte counts are words-copied × 4.

| NGP target | site(s) + containing fn (TSV) | original callee | verdict + one-line change | evidence |
|---|---|---|---|---|
| 100ed470 | 004e4934 E34WallVf::Raycast; 004ea3b1 CCar::Vf6cx | 0x4e4490 Emit BrakeWallPenetration | `rewrite`: same fan-out, 32 B signal copy, +deferred-dispatch branch | shell + inner 100ecb50 = standard fan-out |
| 10016d40 | 004e5560 CCar::Split17C; 004e794a CCar::Test7910; 0050b1f7 CDriveLine::Sample | 0x50a650 BlendB | `rewrite`: driveline sample recomputed by NGP 10016aa0, stored via 10111840; original never called | calls 10016aa0/10111840 only; 10016aa0 reads NGP table (count at this+0x75310) |
| 10016960 | 004e5572 Split17C; 004e795c Test7910 | 0x50a920 LerpAt | `rewrite`: same as above via 100168b0 | same shape as 10016d40 |
| 1004b8e0 | 004e6657 (naked thunk, `jmp` patched) | 0x518e20 Sim8E20 | `wrapper`: calls original Sim8E20 (via 1004b800), then launch/limiter post-process w/ rev-history ring DAT_101767b0/c0/bc, NGP globals, final /1.05 | resolver ref 0x518e20 in 1004b800; ring-buffer loop + DAT_10171a98/10171aa0 |
| 100ee6e0 | 004e8fbe E34_RetireA; 004e9bba CCar::Update9AF0 | 0x4e8770 Emit RetireCar | `rewrite`: same fan-out, 4 B signal, +deferred branch | body ≡ shell; inner 100ed2c0 ≡ our Emit minus debug checks |
| 100ed8a0 | 004e9307 CCar::UpdateSeed | 0x4e8a70 Emit CarProblem | `rewrite`: same fan-out, 8 B signal | shell; inner 100ecca0 = standard |
| 100ee4e0 | 004e977c CCar::Vf180 | 0x4e8830 Emit Respawn | `rewrite`: same fan-out, 32 B signal | shell; inner 100ed1e0 = standard |
| 100eda50 | 004e9c5c CCar::Update9AF0 | 0x4e88f0 Emit StandStill | `rewrite`: same fan-out, 8 B signal | shell; inner 100ecd80 = standard |
| 10041f30 | 004ea1b3 CCar::Vf28x | 0x4e83f0 Emit CarGeneralState | `rewrite`: 224 B signal copy; new 4× per-wheel prologue, then calls fence handler 0x601fb0 directly (via 10063f10), then standard fan-out 1003f450 | loop w/ 1004cdd0/1005fc70/10061dc0; resolver ref 0x601fb0 = CFenceHandler::OnCarState |
| 100ee7a0 | 004ea1cc CCar::Vf28x; 004ebc8e CCarBody::Vf28x; 0051fc9f HingedRigidBody::SimFB40; 005220db (E45 mount); 0052c6f6 CSolid::E43Vf28 | 0x4e8330 Emit SolidGeneralState | `rewrite`: same fan-out, 84+8 B signal copy | shell; inner 1005e5d0 = standard |
| 100ed3a0 | 004ea1f7 CCar::Vf28x | 0x4e84b0 Emit float | `rewrite`: same fan-out, 4 B signal (body identical to 100ee6e0 modulo names) | normalized diff vs 100ee6e0 empty |
| 10015fc0 | 004eb286 E35CarOps::Step230; 0052caee CSolid::VfC8 | 0x50b300 SampleBig | `rewrite`: sample via NGP 10015a80 + cache write 101118a0; original never called | 28-line body; no resolver ref |
| 100edf40 | 004ef703 E36DamageOps::EngineGate | 0x4ef290 Emit EngineFire | `rewrite`: same fan-out, 4 B signal | shell; inner 1004a8d0 = standard |
| 100ed5e0 | 004f149b E36DamageOps::Slot1 | 0x4ef1d0 Emit CarDamage | `rewrite`: same fan-out, 68 B copy; additionally calls replay guard 0x61e1b0 first (not in original Slot1 path per fn.py callees) | resolver ref 0x61e1b0 = NMrReplay::E94GuardShell::E94GuardX |
| 100ed7d0 | 004f212d CCarLightSystem::EmitLights | 0x4f2000 Emit CarLight | `rewrite`: same fan-out, 24 B signal | shell; inner 100ecc30 = standard |
| 100456c0 | 00507a3e CDrive::M7A00; 0050a25d CDrive::Step1E0 | 0x506c90 M6C90 | `rewrite`: same skeleton (+0x45c threshold, +0x1c8/+0x1a0 writes); 400→DAT_10171a98×0.8956(const _DAT_10147c5c), new falloff curve w/ tunables, decay via 10049590/10049f30 helpers, +2M-tick time gate (10111ce0) | vs our M6C90: consts replaced, decay restructured, `lVar2 < 2000000` gate added |
| 100edd90 | 0050855c CDrive::M8480 | 0x506e60 Emit DrivetrainState | `rewrite`: same fan-out, 44 B signal | shell; inner 100ecf40 = standard |
| 10045b80 | 00509c34, 00509e94 CDrive::M9BC0 | 0x506260 M6260 | `rewrite`: same structure/offsets; core friction expr routed through 10045870 (axle-split factor: f×(idx<2 ? 1-split : split), the per-car split DAT_10171a90, `notes/ngp.md`) with DAT_10147c58 (0.5) replacing hardcoded 0.25/0.5 | vs our M6260 line-by-line: same offsets, `f` computation differs; SetDirty ≡ +0xa0=1 write |
| 100458c0 | 00509c86, 00509d74 CDrive::M9BC0 | 0x505fe0 M5FE0 | `rewrite`: same treatment as M6260 (10045870 factor) | same shape as 10045b80 |
| 10045fa0 | 0050a10c CDrive::M9BC0 | 0x507d60 M7D60 | `rewrite`: same 4-wheel flag loop (strides 0xf0/0x32); sign via 1003cc60 helper; reuses original M6B20 (via 10045d20→0x506b20); NGP averaging end-step 10045ec0 | vs our M7D60: same loop skeleton |
| 10046880 | 0050a2a9 CDrive::Step1E0 | 0x504160 E39_Hook4160 | `rewrite`: same inputs/offsets; core via original E39_Worker4060 (100466b0→0x504060); adds per-wheel telemetry store (DAT_1017662c table, stride 0x3c), pow() term, torque-redistribution branch (iVar6==3) | resolver ref 0x504060 = E39_Worker4060 (also called by original hook) |
| 10047300 | 0050a34b CDrive::Step1E0 | 0x5093a0 M93A0 | `rewrite`: same flow/offsets; reuses original M9120/M9210; fabs via bitmask, sign via 1003cc60; ADDS zero-crossing zeroing of accumulator +0x45c | vs our M93A0: extra `if sign(old)!=sign(new) 0x45c=0` block; rest matches |
| 10048d50 | 0050a46e CDrive::Step1E0 | 0x509bc0 M9BC0 | `wrapper`: big NGP shift-logic pre-pass (gear-state machine, modes 1–5, 10045870 factor), then calls ORIGINAL M9BC0 (10045df0→0x509bc0) at the end | `FUN_10045df0(param_2)` at line 355; resolver ref is the replaced addr itself |
| 10060230 | 0050e2f9 CEngine::SimE1F0; 00517e48, 00517f43 E43EngineSim::Sim7D30 | 0x52e760 DamperSolve | `wrapper`: identical args except arg3 := DAT_10171ad8 (file 101300.0; runtime 93349.9 = stage pressure) | 11-line body; 10060140 = trampoline to 0x52e760 |
| 1004d2f0 | 00518a33 E43EngineSim::Sim8890; 0053428d CScratch::HeatStep | 0x52e350 DamperStep1 | `wrapper`: 1004d2c0 ≡ original `(a-b)*c*d`; only change is b := (b−293.14)+DAT_10171adc (file 293.15 ≈ no-op; runtime = stage temp) | 1004d2c0 body identical maths; bias consts from .rdata/DLL |
| 100ee0f0 | 00518d71 E43EngineSim::Sim8CF0 | 0x50ee90 Exhaust Emit | `rewrite`: same fan-out, 24 B signal | shell; inner 100ed020 = standard |
| 100ee000 | 005191a9 E43EngineSim::Sim8E20 | 0x50ef50 EngineNoise Emit | `rewrite`: same fan-out, 44 B copy; additionally always calls NGP fns 100b3470/100b3b90 after (exhaust/sound state, NGP-only) | two extra calls after the branch rejoin |
| 100ee260 | 0051fceb HingedRigidBody::SimFB40 | 0x51f640 Emit HingedBody | `rewrite`: same fan-out, 4 B signal | shell; inner 100ed090 = standard |
| 100edcd0 | 0051feec CHingedRigidBody::SimFD00; 00522377 (E45 mount) | 0x51f580 Emit DetachFromParent | `rewrite`: same fan-out, 4 B signal (identical to 100ee6e0 modulo names) | normalized diff empty |
| 1004df40 | 00520265 E44MatTable::Pick | 0x520010 E44Entry8::Pick | `rewrite`: fixed 16×16 byte table (`table[y*0x10+x]`, scale 16.0, clamp 0..15) using BOTH a and b; original uses per-entry dims, ignores b, wrap-or-clamp per flags | vs our Pick: `(void)b` vs NGP `iVar1 from param_3` |
| 100ee330 | 005232e3 (E45 passenger) | 0x523010 Emit IncarShake | `rewrite`: same fan-out, 4 B signal | shell; inner 100ed100 = standard |
| 100edbf0 | 0052c501 CSolid::BigB | 0x52be70 CSolid::Emit | `rewrite`: same fan-out, 44 B signal | shell; inner 100ece60 = standard |
| 100edb20 | 0052e025 CSpringDamper::ResetOutputs | 0x52de60 DamperSignal::Emit | `rewrite`: same fan-out, 24 B signal | shell; inner 100ecdf0 = standard |
| 1004d3c0 | 005344eb CScratch::TableStep | 0x52e350 DamperStep1 | `wrapper`: factor = 1004d340() (damper-state fn), then DamperStep1-wrapper 1004d2f0 | chains the two wrappers above |
| 1004d380 | 00534531 CScratch::TableStep | 0x52e370 DamperStep2 | `rewrite`: original `-(a*b*c)` → `signmask(a)*b*1004d340()`; drops the c parameter, uses live damper-state factor ×0.125 (_DAT_10148adc), keeps negation via sign-bit flip | vs our DamperStep2 one-liner |
| 1004d410 | 00534639 CScratch::RelaxStep | 0x52e350 DamperStep1 | `new`: novel load-sensitive c (scratch +0x7c/+0x8c/+0xec, tyre state via +0x410→+0x7f4/+0xea0, pow(), gear tables DAT_10171470/74 = 6.0) fed into the DamperStep1-wrapper | 40-line body; no counterpart in original RelaxStep path |
| 1005b4a0 | 00535484 CScratch::StepInner | 0x5320c0 ComputeOutputs | `wrapper`: calls original ComputeOutputs (1005b370→0x5320c0), then `*o15 = min(*o15*4.25, 1.0)`, `*o17 = 1.0f` | post-call clamp + force in 26-line body |
| 100ee970 | 005358a7 CScratch::EmitNoise | 0x534f80 Emit TyreNoise | `rewrite`: same fan-out, 44 B signal | shell; inner 100ed330 = standard |
| 1005be40 | 005362ab CScratch::Step | 0x5338a0 Accumulate | `new`: replaces Accumulate with NGP slip-curve scaling of setup tables +0x204/+0x244 (double piecewise curve, asin normalization, consts 5.0/34.0/45.0/1.05/0.01/0.025/0.0025/0.1/π); Accumulate's outputs bypassed. NOTE: not a function entry — mid-range of Ghidra-merged FUN_1005bd80 (whose +0x0 entry is an unrelated lazy trampoline resolving original 0x5338a0) | objdump NGP.dll 0x1005be40: int3-padded entry, writes [eax+0x204/0x244] |
| 1005bc50 | 00536724 CScratch::Step | 0x534090 HeatStep | `wrapper`: pre-scales scratch +0x3c8 by asin-based factor (thresholds from .rdata doubles, FUN_100616a0 reader), calls original HeatStep (1005bb70→0x534090), restores +0x3c8 | save/call/restore in 50-line body |
| 1005e8a0 | 0053d49d E51Wheel::M3D460 | 0x53d230 M3D230 | `rewrite`: SSE, CRT sin; the sine terms weighted `1 - w` and `w` like the lerps (1.02 swaps them; `notes/ngp.md`); adds `if (tag2==0xfe) tag2=tag1` remap (no counterpart in Tag520940) + NGP-active gate (100591b0 reads exe global 0x1659b6c) | vs our M3D230: same offsets/fields, +0xfe branch + gate |
| 1005b660 | 0053e2a4, 0053f086 E51Wheel::M3DD30/M3EB80 | 0x5360c0 CScratch::Step | `wrapper`: pre-scales grip inputs (wear factor `1−min(|+0x360|×3.6,200)/200×0.025`, per-surface multiplier `(N/100)²` from NGP tables +0xe98/+0xea8/+0xeb8), spills 4×0x20 B blocks (1005a970=memmove) around the ORIGINAL Step call, restores after | 1005b540 = trampoline to 0x5360c0; pre/post code is all NGP |
| 1005f8a0 | 0053e317, 0053f0dd E51Wheel::M3DD30/M3EB80 | 0x53d0d0 E51MultiTire::Emit | `rewrite`: NGP particle pre-compute (3× original E33Mat34::ComposeA via 1004d060→0x51b4c0 on a 176 B signal copy), then standard fan-out 1005e640 | resolver ref 0x51b4c0 (a Compose helper, not the replaced Emit) |
| 10059f70 | 0053e548, 0053f168 E51Wheel::M3DD30/M3EB80 | 0x52e090 CSpringDamper::Step | `wrapper`: NGP spring pre-scale (+0x98/+0xd8 by fVar13, seeds +0x30/+0x34/+0x38; flag-gated +0xe38&0x20000, +0x69c&1), calls ORIGINAL Step (10059e80→0x52e090), restores +0xe8/+0xf8/+0x118 (+0x98/+0xd8), then original UpdateForces (100598a0→0x52dd70) | pre/post + two resolver refs, both originals |
| 1005f6f0 | 0053f386 CWheel::E43Vf28 | 0x4e8330 Emit SolidGeneralState | `rewrite`: standard fan-out 1005e5d0 + NGP telemetry prologue (1004cdd0/1005fc70/10061dc0/1002e840/1002e4a0/1002cc00/1002cbb0 — same idiom as 10041f30/1005f8a0) | 0x15-word copy + prologue before the shell branch |

Row coverage: 2+3+2+1+2+1+1+1+1+5+1+2+1+1+1+2+1+2+2+1+1+1+1+3+2+1+1+1+2+1+1+1+1+1+1+1+1+1+1+1+1+2+2+2+1 = 65. ✔

## NGP-only callees, grouped (one level deep)

- **Emit machinery (shared by all 18 shells + 3 variants):** inners
  `100ecb50 100ed2c0 100ecca0 100ed1e0 100ecd80 100ecae0 1004a8d0 100ecbc0
  100ecc30 100ecf40 100ed020 100ecfb0 100ed090 100eced0 100ed100 100ece60
  100ecdf0 100ed330 1003f450 1005e5d0 1005e640` (all = the one fan-out);
  dispatchers `100e9870 100ebb50 100e9ff0 100eb7d0 100ea3a0 100e96b0 100e9c10
  100e9e20 100eaad0 100eb080 100eaea0 100eb250 100ea920 100eb420 100ea730
  100ea560 100ebd00 1005e000 1005e230 1003ca10` (per-signal-size queue variants,
  89–176 lines); gate `1008cf10` (TLS car context), `1003c8a0` (identity),
  `1002a640` (context field +0x18b86c, guess), `10111ce0` (µs timer).
- **Original-resolver trampolines** (mechanism, not behaviour): listed in
  "Shared mechanisms". `10013cd0`/`100591b0` resolve exe *data* (0x7ecd04 /
  0x1659b6c) for NGP gates; `1004d130`→0x7cbb00 (unverified target).
- **Driveline:** `10015a80 10016aa0 100168b0` (NGP interpolation/sample),
  `101118a0/10111840` (NGP sample-cache getters/setters, 3 floats).
- **Damper core:** `1004d2c0` (same `(a-b)*c*d` as DamperStep1),
  `1004d340` (state-derived factor `(f8c+f7c)*2π*f_ac*0.125`).
- **Drivetrain helpers:** `10045870` (axle-split: `f×(i<2?1-split:split)`, the per-car split DAT_10171a90);
  field accessors `10049590/100495a0/10049f30` (+0x8c read/read/write),
  `1004cdd0` (+0x34), `1005c6b0` (+0x60), `1005fc70` (+0x2f0),
  `10061dc0` (+0x10), `100616a0` (+0xc float); `1003cc60` (x≫31 sign bit);
  `10043e80` (pow); `100472a0` (clamp-ish, guess); `10047040 10048410 10048380
  100487f0 10045ec0 10043eb0 1004a090` (shift/limiter/launch helpers, roles
  partly unverified); `10120630` (64-bit value stored to +0x7c/+0x8c —
  timer/random, unverified).
- **Tyre/particle/sound:** `1005a970` (memmove block spill/fill);
  `1002e7b0/1002e800/1002e820/1002e840/1002e4a0/1002cc00/1002cbb0/1002cbe0`
  (telemetry/prologue field ops); `1004cde0 1005fb80 1005fc50 1005fca0`
  (particle pre-compute temps); `100b3470/100b3b90` (engine-sound state,
  guess); `1002a600/1002a690` (damage-emit prologue).
- **Replay:** exe `0x61e1b0` (E94GuardX) called via `10033fc0` before damage emit.

## Tuning globals (NGP addresses; file default → runtime value)

`.data` (writable, rewritten per stage) and `.rdata` (fixed) values read from the
DLL file; runtime values from the TSV `const` rows:

- **Ambient / per-stage air (the clearest per-stage set):** `10171ad8`
  pressure 101300 → **93349.9**; `10171adc` temp 293.15 → **288.703**;
  `101767c8` 9.87e-06 → 1.07e-05; `101767cc` 116495 → 107352;
  `101767d0` 100000 → 92151.9; `10171ae0` density 1.20407 → 1.12663
  (Tanner's start height; NGP recomputes them every step from the car's height, `notes/ngp.md` Atmosphere).
  `10171ad8` is also the DamperSolve arg3 override (10060230);
  `10171adc` is the DamperStep1 rebias (1004d2f0; ref `_DAT_10148ae8`=293.14).
- **Drivetrain:** `10171468` 0.0005→0.000480769, `1017146c` 0.0333333→0.0320513
  (M9210); `10171aa4` 900→1380 (Step1E0); `10171a90` 0.5 (axle split);
  `10171a98` 400.0 (M6C90 threshold); `10148a5c` 0.1047 = 2π/60;
  `10148ab4` 104.72 = 1000 rpm in rad/s; `10148a64/6c` 0.95/1.05 (limiter
  band, guess); `10171ab4` 300→25 (RevLimitCut); launch `10171ae4` 200→89.01,
  `10172338` 275→99.48, `101767c4` −0.0133→−0.0955; gearbox `10172db0`
  0.05→0.14, `10172db4` 5000→4807.7.
- **Engine:** `10171aa8/ac/b0` 500/3500/0.000222→468.75/3281.25/0.000237
  (SimE470); `10171abc` 40→39.27; `10171ab8` 80→78.54; `10171ac0`
  2.215e6→7.93e6 (Sim80D0); damage `1017130c` 500→470.98; drag `1017147c`
  0.5→0.467841.
- **Tyre/suspension:** RelaxStep `10171478` 100→800; HeatStep `1014fbc0`
  60→40; StepInner grips 0.99/0.96/0.87/0.7→0.95/0.875/0.775/0.65;
  Step `1014fbd8` 0.1→0.025; ComputeOutputs `1014fbc4` 3→1;
  spring `10172ca8/ac` 27000/5.55e-5→25432.7/5.90e-5; wheel
  `10172f58/5c/60` 28000/60000/3.125e-5→26374.7/56517.2/3.32e-5,
  `10172f64` 800000→753562, `10171300/04/08` 0.4/0.15/0.2→0.405/0.155/0.205;
  solid eps `1014fbf0` 1e-5→4e-7; physics substeps `1014fbdc` 4→20.
- **Scales used inside replacements:** `10147c58`=0.5, `10147c44`=0.02,
  `10147c70`=80.0, `10148ae4`=2π, `10148ef8`=4.25, `10148eec`=3.6,
  `10148e98`=0.025, `10146b28`=100.0, `10148ca0/ca4`=0.3/0.4,
  `10148bdc`=16.0 (Pick), `10148adc`=0.125 (DamperStep2 factor),
  `10171470/74`=6.0 (RelaxStep gear tables), `101459b4`=2⁻²³ (M9BC0-wrapper
  epsilon). Curve doubles at 10148e**/10148a90: π, 0.01, 0.025, 45.0, 1.05,
  34.0, 5.0, 0.0025, 0.1 (slip-curve 1005be40 / HeatStep wrapper).

## Counts

- 65 rows covered in 45 groups. Verdicts by row: **rewrite 49, wrapper 14,
  new 2, unclear 0.** By target: rewrite 34, wrapper 9 (1004b8e0, 10048d50,
  10060230, 1004d2f0, 1004d3c0, 1005b4a0, 1005bc50, 1005b660, 10059f70),
  new 2 (1004d410, 1005be40).
- Original callee verified against exe disassembly for all 65 sites (64× E8,
  1× E9 at 004e6657).

## 5 most important behavioural differences

1. **Driveline resampled from NGP data.** BlendB/LerpAt/SampleBig replacements
   never call the original; interpolation runs over an NGP-side cache
   (10015a80/10016aa0/100168b0 + 101118xx getters/setters).
2. **Tyre Accumulate bypassed.** CScratch::Step's Accumulate() call now runs an
   NGP slip-curve scaler on the +0x204/+0x244 setup tables (doubles curve).
3. **Every signal Emit reimplemented** with identical fan-out/layout but a
   deferred-dispatch path when the car's NGP flag is set; debug checks dropped.
4. **Per-stage atmosphere.** Pressure/temp/density retuned (≈0.92 derate) and
   plumbed into DamperSolve (arg3) and damper-temp rebias, not just the engine.
5. **Drivetrain wrapped, not just retuned.** Original M9BC0/Sim8E20/Step/HeatStep/
   ComputeOutputs/SpringDamper::Step still run, but inside NGP pre/post logic
   (shift machine, launch/limiter, wear + surface multipliers, spring
   rescaling); M93A0 additionally gains a zero-crossing clamp.

## Unresolved / guesses

- Dispatcher internals (which thread, queue depth, ordering vs inline path).
- Which value of context+`0xd` means "NGP active" (polarity inferred only).
- Roles: 10120630, 10047040/10048410/10048380/100487f0/10045ec0/10043eb0/
  1004a090 (shift/launch helpers), 100b3470/100b3b90 (sound), 1002a600/1002a690,
  1004d130→0x7cbb00, DAT_1017662c telemetry table consumer.
- `_DAT_10147c50` (≈2e-26 as float) is likely half of a double; typed as float
  by Ghidra — the 10046880 term it scales may be intended larger.
- Downstream effect of dropping Accumulate's f[0x141..0x143] outputs.
