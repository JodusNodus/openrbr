# X8 exactness findings (exe 0x510000-0x541000: rigid bodies, engine, tyres, wheels)

Worker: match/exact-x8. Baseline (pre-change): Implemented 89.33%
(19860/22232), Accuracy 71.86%, Progress 64.20%, 0 ERROR.

## Verified call shapes (disasm evidence, fixed in batch)

- 0x499e60/70/80/90 are thiscall telemetry hooks, this = [0x893634]:
  `ret 4` / `ret 4` / `ret 8` / `ret`. Now `NPhysics::E21Telem` (RBR_E19.h,
  defined in RBR_E19_ReplayBody.cpp); all three 0% Nops now 100%.
  E46 callers use global this (CollideWork/StepForward used m_scratch).
  0x499e80 caller 0x4fc1a0 passes (esi, esp+0x80).
- E46ProfChan::Enter is thiscall on a stack sample (CollideWork,
  StepForward); ProfExit is __fastcall on the sample address.
- CPhysics::Work5640 is thiscall+1 (ret 4, arg unused).
- E47S16_0::S16 is vtable slot 0x40 (17th virtual); S08 helpers slot 8.
- E01Vec4::CopyFrom is thiscall (this, src); E01VecSetA::Cross this+2.
- E18_QuatNormalize is free __fastcall (1 arg); CCar::AppendA980 this+1;
  CCellGrid::LookupTwin this+2 (Vf17y passes 2; VfDx/VfEx push 3 across
  two calls with the Append arg pushed early -- replicate with care).
- E43PairWalk::MB560 is thiscall+3, still 100%; do not "fix" (47 callers).
- g_7cbb00mat (E50): `[0x7cbb00]` content-loads pair via `*(void**)` on it.
- 0x72fcac = 1.01f pool, 0x72e954 = 1.1f: no csv rows (csv out of scope);
  literals kept, loads unpaired until a csv batch rows them.

## Layouts confirmed

- CWheel: m_unk44 +0x44 (slot-0x7c sampler), m_scratch +0x800 (0x580).
  UnkA5a0 accumulates at scratch+0x1f0/+0x200, Cross-this at +0x140.
- E47Solid quads are 4-wide float lerps (VfDx blocks at +0x100/+0x140
  vs +0x2a0/+0x330); +0x290 holds float bits (no int convert);
  +0x44 is an int flag. The old acc[] int model is wrong for these.
- E45_M46A0: object iter starts c+0x30, strides 0x6b0; id iter starts
  c+1 (int stride); count on stack; `ret 0x10`.
- SimCF10 (0x51cf10): `float (int idx)`, thiscall+1, x87 tail.

## Ceilings (structural, not logic)

- C++ try/catch around Work6a70 forces an EBP frame; original has ESP
  frame + EH prologue + try-state stores. CollideWork keeps explicit
  ProfExit and documents the ~15-insn EH gap (66.46%).
- `lea edx,[base+0x1f0]` vs folded `base+0x23c` (SimCF10 e): MSVC folds
  our e+small uses; original materializes e. Same class as CopyDefFrom's
  `add eax,K` + `[eax+0xc]` triples (needs intermediate-address source).
- EBP + `and esp,-16` aligned frames require 16-byte (A16/__m128)
  locals; plain float[]/E01Vec4 (movups type) do not trigger it.
  Confirmed fix twice (SimD860 +20, UnkA5a0 frame). E01Vec4 itself is
  unaligned (CopyFrom uses movups); do not align it globally.
- IAT-call hoisting (isspace in Tokenize, rand in CrossTest) and
  callee-saved-reg coloring (ebx vs edi) resist source-level steering.
- CMaterial dtor (0x520700): 20 inline {CBase-vft,0} demotes + one
  CBitField call, no member calls. Incompatible with CReal/CUnsigned
  (non-trivial dtor) members -- needs an E44/E45 member-remodel
  decision, not attempted here.
- VfEx (0x52d110) and the CopyDefFrom middle region need full
  re-transcription (stack-buffer choreography / operand mapping);
  parked with evidence above. Top-gap giants (0x518e20, 0x51ead0,
  0x53eb80, 0x5360c0) untouched: each needs multi-hour transcription.
- Skipped per brief: 0x5403e0/0x540930 (E52 menu range), 0x53f7d0
  (screenshot UI), audio/input-device functions.

## X8b giants (all six attempted)

- Sim8E20 17.00->32.72: tail was entirely missing (FloorS32 x3,
  Push x2, mod10/div20). SetDirty this-args fixed. 0.05 pool is
  0x3D4CCCD0 not 0.05f. 8 float rows added; 2.1e9 rows withheld
  (E41 flap, revert-verified).
- SimEAD0 0.74->38.48: full transcription (dots, 4-wide via
  intrinsics, x87 fsum, sign/abs, Vf81x block). SimE1D0 noinline.
  Frame/layout (+4) and dot term order remain.
- M3EB80 25.40->25.67: Fwd6DB0 float(int) (was double M46600).
  Cross car+400 / pf lea verified but ripple-reverted. MB560
  0-push is cross-TU arity ceiling (Xform6800 same).
- Step 39.80->39.95: *o (not *p), else *n=0, ContactPatch &e,
  m_tail int store. m_sub loads (vs this+K lea) remain.
- CopyDefFrom held 79.74: add+load unfolding resists all forms
  (named/unnamed intermediates fold). Hard ceiling.
- VfEx 14.63->46.07: full transcription. LookupTwin 3rd push,
  AppendA980 0-push are arity ceilings (decls carry fewer).
- Fwd6DB0 40->33: float decl correct (fstp sites); def can't
  jmp (ceiling). Enables M3EB80/M3DD30.
- Flaps (untouched source, recovered or pending): E25LangTag,
  E59_M0562140. E41_TableLerp drop was real (row-induced);
  fixed by withholding 2 rows.

## X8c (named four + next gaps)

- Friction 18.00->19.52: shared || early-out, -900/2.22e-05 rows.
  ClampInt is E16_Sign address.
- M2BC0 35.63->35.79: 8 rows, or/xor signs. M51C8C0 this+2 both.
- StepForward 15.21->15.70: ProfExit sample args. EH/EBP ceiling.
- SimEFE0 1.09->32.56: full transcription, Vf36x slot 0x90,
  SimD160 noinline.
- ContactPatch 17.09->19.66: 4 rows, double u for x87 _ftol2.
- Vf27x 0.74->19.29: byte-offset rewrite, VfC8/Vf24x/E43Vf6C.
  Emit skipped (signal incomplete).
- HeatStep held 17.41: 7 rows safe; int/float rework ripples.
- BigB parked: needs E36Collect global-this + virtuals 80/12/28/29.

## X8d (StepForward EH + sub-20% E44)

- StepForward 15.70->21.63: try/catch gives EH prologue (EBP
  penalty accepted). __try/__finally worse. Handler 0x717080 is
  SEH funclet (separate bytes).
- SimF350 1.28->33.54: full transcription, Vf128x slot 74.
- SimF950 1.49->20.83: clamp/signs, Vf27x(dt). SimF280 noinline.
- SimCFF0 2.08->60.44: MB560/Cross/Scale/StateAtb, x87 ret.
  MB560 this+2 here (dummy 3rd); arity varies by TU.
- SimE330 3.75->20.75 (partial): Vf76x early-out, slot-0x30s.
  Main x87 + slot-0x134 pending.
- BigB skipped: E36Collect (0x4f, out of range) + virtuals.

## X8e (SimE330 finish + SimFD00)

- SimE330 20.75->42.35: E34Call134_2 slot-77 helper, pf/f70,
  Cross/M51C8C0, x87 return. 1e-06 row.
- SimFD00 2.31->32.80 (partial): structure, Xform/Copy/SimDA60.
  Virtuals 0x54/0x48 + Emits pending.

## X8f (SimFD00 finish + VfC8)

- SimFD00 32.80->39.42: E44Call54_0/48_0 helpers, UnlinkA,
  Vf36x, InitBD0, S48, CopyFrom. Emits/LookupTwin/E47ClearPair
  pending (signal types incomplete; grid not visible).
- VfC8 5.62->26.89: full transcription, E47Call30_3 helper,
  SampleBig (E41_Engine.h include). M83DD0/ComposeA arities vary.
- Row rule noted: grep address before adding (0x795e70 dup removed).

## X8g (SimFB40 + dummy audit)

- SimFB40 1.49->39.82: SimDB40, ComposeA this+3 (this=param_1 per
  asm), ToQuat decl fixed (this+1). No dummies. Emits pending.
- Dummy audit (per parent): MB560 (E43, multi-TU callers) and
  ComposeA (E33/E37 pass 3, E44/E47 pass 0/2) are NOT single-TU
  LTCG; dummies are stopgaps for unresolved arity (likely
  Ghidra misread or overload confusion). Marked in source.
  ToQuat fixed via decl (model case). ComposeA this verified.
- Vf6c parked: fully written, scheduling ceiling (6.54%).

## X8h (MB560/ComposeA audit)

- Both 0x51b560 and 0x51b4c0 end ret 0xc (this+3 fixed). All
  sites must push 3; differing counts were misreads.
- SimCFF0 both fixed (this=param_1; this=[0x7cbb00] value not
  address), dummies removed, 60.44->60.87.
- SimE330/VfC8 ComposeA + SimE330 MB560 use stack-sharing
  (callee reuses virtual's pushes, 0 fresh); inexpressible in
  C++, documented as ceiling (dummies give count only).

## X8i (VfE070 + SimE1D0)

- VfE070 2.15->14.71: orthonormal basis, transpose, mat-vec,
  -1.0 scale (rowed). E29Quat decl (math only).
- SimE1D0 5.71->50.54: MB560 x3 (this=[0x7cbb00] value),
  Xform6A30/Cross, 9.81 row. No dummies. SimEAD0 held.

## X8j (R1 + SolveForces)

- R1 confirmed: MB560/ComposeA this+3 (my sites already unify;
  no E34 decls in range). No action.
- SolveForces: pool refs (-5.0/1.0) score 8.87 vs literals
  13.79 (reverted). Orig may schedule pool loads differently;
  literal/pool puzzle parked.

## X8j

- R1: MB560/ComposeA this+3 confirmed; my sites unify. No E34
  decls in range. No action.
- SolveForces parked: pool refs score lower than literals
  (scheduling puzzle, reverted).
- M29E0 18.18->18.28: literals to pool refs (neutral, faithful).
  30000 row (0x73e894).

## X8k (X8j revert + DtorBody)

- X8j raw pool reads reverted to literals (anti-pattern per
  toolchain.md). 30000 row kept (no-drop verified).
- DtorBody 7.89->46.04: CBase vft (0x72e444) restores + try/catch
  SEH prologue. Vf04 parked (needs EH + 3.4KB types).

## X8l (symbols + Vf04)

- DtorBody vptrs via (int)&CWheel::g_demVtab (held 46.04).
  E39 *(void**) reads skipped (not my range).
- Vf04 2.44->18.38 (partial): try/catch EH + 3.4KB E37GridOps
  stack arrays + early outs. M6610/virtuals pending.

## X8m (Vf04 finish)

- Vf04 18.38->42.22: slot helpers, Sub/AllBelowZero, Get67C/
  ExclF80. M6610/MD70/RayClear pending.
- Range has 28 entries still <20% (not running out).

## X8n (Vf04 ceiling + Vf64 cleanup)

- Vf04 M6610/MD70/S0D0/RayClear: MD70 naked (ecx+stack vs ret 8)
  resists source form; M6610 alone -0.55 (reverted). Held 42.22.
- Vf64 raw 0x73e7a8 to -1000.0f literal + row (compliance).

## X8o (0x51-0x53; E51GeomCBD0 is X5's)

- SpringDamper::Step parked: fully written, UpdateForces/ECX +
  scheduling ceiling (10.05%). 27000/5.5e-05 rows added.
- GiganticVf04 2.86->32.48: EH/stack, S118/S110/S34/S30,
  M6610 x2, MinMaxA (method), RayClear. Combine/MD70/0xcc pend.

## X8p (0x51-0x53)

- M40C0 16.37->43.05: grid is g_8eb794 (not host flag ptr),
  4.7f literal + row, dedup tail.
- Parked: M2700 (fStack_54), Vf47x (scheduling), Step
  (UpdateForces), Vf17y (unowned 0x4e1140).

## X8r (BigB + signals)

- BigB 1.01->10.08 (partial): E36Collect 4-arg + first loop.
  Second loop + Emit pending.
- CSigDetachFromParent modeled 4B in RBR_Signal.h (SimFD00
  iStack_90 evidence). MrRBodyEvent size unclear (left fwd).
  Emit wiring needs LookupTwin/RegWall flow (pending).
- Vf17y untouched (0x4e1140 is X11's).

## X8s (BigB + signal search)

- BigB 10.08->19.59: second loop (S140, Length/Scale).
  -50.0 row. Cross/MB560/slots 0x30/0x70/0x74 + Emit pend.
- Signals: csv has CConnection vftables (0x74fa14 etc.), not
  signal POD sizes. No member-reading receivers found (only
  Emit templates). Detach 4B stands; others still fwd.

## X8t (BigB finished, Emit waits)

- BigB 19.59->40.83: Cross, MB560 x2 (this=value, no dummies),
  slots 0x30/0x70/0x74. Range done pending merge.

## X8-new (0x43-0x46 slice)

- Target list exact-targets-rbr-x8b.txt (80 entries, gap-ranked,
  NSDK_Insert skipped).
- E02 Load 21.32->23.77: string literals, float scale. X9 note:
  ConvertCopy src-base u1+0xbc scores lower (avoid).

## X8-new2 (0x43-0x46)

- E02AnimUpdate 22.63->26.54, Init 34.73->39.82.
- E59 flap (untouched).
