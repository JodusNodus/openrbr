# Contacts: from detection to impulse (physics semantics, contact pass)

Deepens `notes/physics/collision.md` (sections 1b, 3, 5) on the path from a contact to a force.
Notes only; no source edits. "Current" names are the C++ reconstruction's; "proposed" names are ours.
Confidence: `known` (asm checked in this pass, or a string/LSP key names it), `inferred` (strong
argument from use or units), `guess`. Asm: `llvm-objdump -d -M intel` on the original exe, VAs.
Harness status comes from `notes/progress/gameplay-functions.csv`; reccmp % from `base/RBR.txt`.

Corrections to earlier notes (made in this pass):
- `collision.md` K1 is withdrawn: the 4th argument of `Impulse640` is a time, not a friction
  coefficient (section 3, `Impulse640`). See section 5.
- `step.md` 2e and `collision.md` 1b describe `CollideWork` as "slices of f*0.05". The slice
  loop runs its body **once** per call (asm `0x526d90`-`0x526da8`: a counter `ebp = 1` is tested and
  decremented on entry). The `f*0.05` value is only the loop-entry test. There are no further slices.

## 1. Overview

Three contact routes exist. Only (b) and (c) are the body-to-body and body-to-world path in
`StepForward`; (a) is the car's brake wall, which feeds the car's velocity directly.

**(a) Brake wall (E34 car, asm-checked call sites).** `CCar::Vf6cx` calls `RaycastEntry` on the car's
`(x, y)`, and on a hit `Impulse640(out, perp, _, dt)`. The impulse is a velocity clamp on the car
(no mass). Section 3, `F2` and `Impulse640`. Not reached on the reference drive (`called.csv` has no
`0x529640`, so the hit branch did not run, or the log misses it; open 1).

**(b) Body contacts (E45/E46 detectors, E37 events, E38 solver).** Per substep, when the
`CHECK_COLL` flag (0x10) is set, `StepForward` calls `CollideWork(4*dt)`, and when `CHECK_PENETR`
(0x20) is set, it calls `PenCheck()` and the E38 solver chain. The chain is:

```
StepForward (0x5273a0)
  CollideWork(4*dt) (0x526d20)                    one detection pass, see 1a
    Work6a70(0) (0x526a70)                        for each body in list450: vtable 0x73f7b0 slot 1
      CL1CollisionDetector::Vf04 (0x526ad0)       box of `other` -> grid cells -> candidate bodies
        E45_M40C0 (0x5240c0)                      cell lookup, up to 4 candidate links
        NotifyFiltered -> CL2 detector (vt 0x73f5a8 slot 1 = E46L2Detector::Vf04 0x5257e0)
          E45PhysBody::M4250 (0x524250)           pair walks (bad, open 4)
        Visit470 (0x526390)                       same CL2 pass over list470 (flag 0x40 skipped)
    E38_004FBC20 (0x4fbc20)                       E37 refresh and flag sweep (not read)
    E38_004FC500X (0x4fc500)                      min contact time over events (Get94)
    if min <= 1.1:  fc = clamp(min, 0, 1)
      Work5640(fc) (0x525640)                     VfDx(time) on contact bodies with flag 0x4
      E38_004FCA20 (0x4fca20)                     solve pass: M_C1A0 per due event (asm checked)
        E38Ops::M_C1A0 (0x4fc1a0)                 normal + friction impulse for one event
          E38_004FB720 (0x4fb720)                 material pick
          Query4390 (0x4f4390), PlaneDiff (0x4f40f0)
          Emit4DC0 (0x4f4dc0)                     impulse to both bodies
            E47Solid::Vf74x (0x52a0e0) for CSolid  v += J/m, w += 0.6 I^-1 (r x J)
      Work5760(window) (0x525760)                 VfEx(window) on contact bodies with time < 1
  PenCheck() (0x526cc0)                           L1 detector (vt 0x73f5b4 slot 1 = 0x525a20, bad)
  if hit (E38_004FBCA0 0x4fbca0):
      E38_004FBC20, E38_004FC590X (0x4fc590), E38_004FCA20
  S08 on the solver
```

**(c) Penetration correction (E38 `FC590X`).** After `PenCheck`, if the solver reports a hit, the
step runs `E38_004FC590X`. For each event with the flag gates, it moves the pair by a correction
vector split between the bodies by inverse-mass ratio: `fr = 1/(1 + f7/f8)`, body A takes `-(1-fr)*v`,
body B takes `fr*v` (`M_BFC0X`, `M_BD30`; section 3). The C++ is C++-read only. The status is
`verified` (12084 steps) and reccmp is 17.5%. Whether the correction is positional or a velocity is
not settled (open 8).

### 1a. What one `CollideWork` pass does (asm checked)

1. Detect: `Work6a70(0)` fills the event list (`scratch+0x14`) and the solver rows (`scratch+0x10`).
2. `m_unk18` (detect count) and the telemetry hooks run as before.
3. `t_min = E38_004FC500X(solver)`: the smallest `Get94` (time) over the live events; suppressed
   events (flags `0x6000`) are unlinked and freed during the scan.
4. If `t_min > 1.1`: no contact in the window, the loop exits, no impulse.
5. Else `fc = clamp(t_min, 0, 1)` (the `f7 = f6 - f8/loc20` term has `f8 = f*0 = 0` for finite `f`;
   the asm keeps the multiply, so the term is dead but present, see 5).
6. Advance the contact bodies to the contact time (`Work5640(fc)`, which ignores its argument and uses
   each event's `Get94`).
7. Solve the due events (`E38_004FCA20`, section 3 F3). Then `Work5760(window)` sets each contact body's
   progress to the end of the window (`VfEx`, absolute, see `rigid-body.md` 1).
8. `loc20 = (1-fc)*loc20` is computed but the loop does not run again.

## 2. Data

### 2a. Contact event `CCollisionEvent` (0xf0 bytes, E37 pool)

Fields read by the asm or the C++ on the step path. Layout source: `RBR_E37_CollEvent.h` (header),
checked here where marked.

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| +0x60 | `Vf128x` arg `this+0x60`, `PlaneDiff` | contact_point | vec4 (w unused) | m (world) | contact point; the lever arm for each body is `point - centre` (`Vf74x` subtracts `+0x140`) | inferred (strong) | `E37_Event.cpp:16`, `E47_SimA.cpp:64-70`, asm `0x52a0e0` |
| +0x70 | `LoadBinary(ev+0x70)` (`v60`) | normal | vec3 | - (unit) | contact normal; `dot = n . rel`, impulse along `n` | inferred (strong) | `E38_SolverC.cpp:36-110`, asm `0x4fc1a0` (`dot` at `0x4fc1cd`) |
| (derived) | `PlaneDiff(ev)` | rel_point_velocity | vec3 | m/s | relative velocity of the two contact points (section 3); the `dot` with `n` is the approach speed | inferred (strong) | `E37_Event.cpp:16`, `E47_SimB.cpp:211-220`, asm `0x52a5de` |
| +0x80 | id triple `Tag(hi/lo)` | material_pair_a | u32, u32, f32 | id, id, weight | material ids `lo`, `hi` and a weight (float bits); the material is `hi` if weight > 0.5 | known (asm `E38_004FB720` `0x4fb720-0x4fb75a`) | asm `0x4fb72f-0x4fb75a` |
| +0x8c, +0x90 | `ids` for the body-side call | body_id_b (default) | u32, int | id, word | fallback pair used in `Emit4DC0` when `+0xa4 == 0` | inferred | `E37_Event.cpp:371-388` |
| +0x94 | `Get94()` | time | f32 | fraction of the window | contact time within the current window; min over events is the first contact | known (asm `0x4f3f80`: `fld [ecx+0x94]`), meaning inferred (strong, from `4fc500`, `Work5640`) | asm `0x4f3f80`, `0x4fc500` (C++ `E38_SolverC.cpp:120`) |
| +0x98 | `piece` / body B | body_b | ptr | - | second body; `partner` is `piece` only when `piece+0x60` bit 2 | known (asm `0x4fca20`) | asm `0x4fca40-0x4fca56` |
| +0xa0 | `obj` / body A | body_a | ptr | - | first body; the event is solved on it | known (asm `0x4fca40`, `0x4fca49`) | asm `0x4fca49` |
| +0xa4, +0xa8 | `Emit4DC0` `w2`, `idB` | body_id_a | int, u32 | id | id pair for body A's `Vf128x` | inferred | `E37_Event.cpp:371-388` |
| +0xac | id triple (second) | material_pair_b | u32, u32, f32 | ids, weight | used when `flags & 0x800` is clear | known (asm `0x4fb741`, `0x4fb73a`) | asm `0x4fb72f-0x4fb741` |
| +0xc | list link | next | ptr | - | next event in the solver list | inferred | asm `0x4fca40`-`0x4fcb27` (`[edi+0xc]`) |
| +0xe4 | `flags` | flags | u32 | bits | `0x800` picks the material pair (`+0x80` if set, `+0xac` if not); `0x1000` with `0x800` is a hit in `M_C1A0`; `0x6000` = suppressed (`E37Test4B20`) | inferred (strong), asm for `0x800` (`0x4fb735`) | `E37_Event.cpp:356`, `E38_SolverC.cpp:50-53` |

### 2b. Body fields used by the solver and the apply (CSolid / CBody)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| def+0x16c | `InverseMass` (via `Vf79x`) | inverse_mass | f32 | 1/kg | `w` in the solver's split and the impulse `J/m`; 0 for fixed bodies (the solver skips them, `f > 0` gate) | known (asm `0x5297d0`: `fld [eax+0x16c]`; LSP key `InverseMass`) | `E47_Solid.cpp:92`, asm `0x5297d0` |
| def+0x130 | `vecLocalInverseInertiaDiagonal` | inverse_inertia_local | vec3 | 1/(kg m^2) | diagonal inverse inertia used in the apply (`0x51c8c0`) | inferred (asm `0x52a165`, `0x52a16b`) | rigid-body.md 2 (`0x120`/`0x130`), asm `0x52a165` |
| +0x140 | position | centre | vec3 | m | the lever arm origin for the apply (`d = point - centre`) | inferred (asm reads `+0x140`, `+0x144`, `+0x148` in the C++; not asm-checked) | `E47_SimA.cpp:66-68` |
| +0x190 | `angular_velocity` | w | vec3 | rad/s | receives `0.6 * I^-1 (r x J)` | known (asm `0x52a19b`: `lea ecx,[esi+0x190]`) | asm `0x52a18f-0x52a19b` |
| +0x1c0 | `linear_velocity` | v | vec3 | m/s | receives `J * InverseMass` | known (asm `0x52a1db`: `lea eax,[esi+0x1c0]`) | asm `0x52a1d3-0x52a1db` |
| +0x60 bit 0x4 | `flags` bit 2 | contact_body | u8 bit | - | the body takes part in contact time and apply (`Work5640`/`Work5760`, `FCA20`, `FC590X`) | known (asm `0x4fca51`, `0x4fca72`) | asm `0x4fca51` |
| +0x290 | `progress` (scratch +0x10) | progress | f32 | 0..1 | window fraction already integrated; an event is solved when `time - 0.1 <= min(progress A, progress B)` | known (asm `0x4fca66` `movss xmm0,[ebp+0x290]`; the 0.1 is `[0x72b068]`) | asm `0x4fca5e-0x4fca8e` |
| +0x3cc, +0x3c8 | `hook` / `tail` | apply_hook, apply_log | ptr | - | optional signal and ring hooks in the apply; not on the reference path (flag `0x1000000`) | inferred | asm `0x52a20b`, `0x52a249` |

### 2c. Surface material `CMaterial` (0x160 bytes, `E45_Material`)

Reals read by `M_C1A0` (C++ reads, asm checked for the offsets). Names not established (open 3).

| offset (value) | current C++ name | proposed name | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| +0x108 | `m_rFC` value | friction_high (candidate) | - | friction coefficient at high approach speed: `mu = lerp(+0x118, +0x108, k)` | guess (the LSP has `CollMyHigh`/`CollMyLow`; mapping by shape) | asm `0x4fc303` (`movss xmm0,[eax+0x108]`), C++ `E38_SolverC.cpp:67-75` |
| +0x118 | `m_r10C` value | friction_low (candidate) | - | friction at low speed (the other end of the lerp) | guess | asm `0x4fc2fb` |
| +0x128 | `m_r11C` value | e_a (candidate) | - | term in `sx = 1 + lerp(+0x138, +0x128, k)` (C++); asm read inconclusive (open 4) | unknown | asm `0x4fc311`, C++ `E38_SolverC.cpp:76` |
| +0x138 | `m_r12C` value | e_b (candidate) | - | other end of the same lerp | unknown | asm `0x4fc2f0` |

The LSP keys `myKinetic`, `myStatic`, `eHigh`, `eLow`, `CollMyHigh`, `CollMyLow` (`formats/physics-lsp.md`
`surface types`) are the candidates; their ranges (`eHigh` up to 15000, `eLow` up to 600000) do not fit
a restitution, so `e_a`/`e_b` are not restitution coefficients by name alone.

### 2d. Solver and physics CDef fields

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CDef+0x1608c | `m_unk16080` | max_time_step | f32 | s | the substep `dt` | known (LSP `MaxTimeStep`, asm `0x527431`) | `step.md` 2a |
| CDef+0x1610c | `m_unk16100` value | impulse_tolerance (candidate) | f32 | m | subtracted from the penetration `perp` in `Impulse640` (`n' = perp - tol`) | inferred (asm `0x529656`); name open (open 2) | asm `0x529656`, `RBR_E46_Math.cpp:566` |
| CDef+0x1611c | `m_unk16110` value | impulse_damping (candidate) | f32 | - | multiplies the cap `dot^2/(2 n') * c * mu` in `Impulse640` | inferred (asm `0x52964e`, `0x5296d1`); name open | asm `0x52964e`, `0x5296d1` |
| CDef+0x16024 | `m_solver` (`CCollisionSolver::CDef`, 0x4c) | solver_def | struct | - | solver tuning; LSP `PhysicsEngine/collision solver`: `Restitution 0.1`, `HyperRestitution 0`, `MinInvEffRedMass 1e-5`, `nSteps 2` | inferred (strings at `0x7398b4`, `0x7398c8`, `0x7398ac`) | `RBR_E38_Collision.h` (CDef), `physics-lsp.md`; the mapping of keys to offsets is open (open 5) |

The three `CReal` at CDef+0x16100/0x16110/0x16120 are built with no name in `CPhysics::CDef::CDef`
(asm `0x524d53`-`0x524d7e` call `CReal::CReal` without a string). Their keys are set by the tweak
loader. The key table has `BrakeWallBuffer` (0x73f488), `BrakeWallDamping` (0x73f464) and
`BrakeWallFactor` (0x73f478), each `float[2]` in the LSP. The table entries are `{name, 0, id,
type}`; the ids (0x5840, 0x5844, 0x5848) are not read by any code in the exe we searched, so the key to
offset map is not established (open 2).

## 3. Functions

Asm checked is marked `asm`. "C++ read" means the reconstruction was read against the function's
behaviour; "verified" comes from the harness status.

| address | current name | proposed name | what it computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x526d20 | `CPhysics::CollideWork` | collide_window | one detection pass over the window `f = 4*dt`; time `fc`; advance, solve, advance remainder. Loop body once (asm). | known (asm `0x526d20`-`0x526da8`) | verified (c2-harness), 69.9% |
| 0x526a70 | `CPhysics::Work6a70` | detect_pass_l1 | for each body in list450: detector slot 1 (vt `0x73f7b0`) then `E43_Walk1F0` | inferred (C++ read) | bad, 59.3% |
| 0x526ad0 | `CL1CollisionDetector::Vf04` | pair_detect_l1 | box of `other` (`S120`, two `Mat34` combines), grid elements (`M6610` x2), `E45_M40C0` cells (up to 4 links, `NotifyFiltered`), `Visit470` | inferred (C++ read) | not verified, 37.1% |
| 0x5257e0 | `E46L2Detector::Vf04` | pair_detect_l2 | gates (`S110`, `E43_ExclF80`, `S80`/`S118`, `E45_P33F0`, `SCC`), grid elements, `M4250` (pair walks + `SD4`), two `E38RayClear` | inferred (C++ read) | verified, 74.7% |
| 0x524480 | `CL2PenetrationDetector::Vf04` | pen_detect_l2 | AABB sign tests on `this+0x10`/`+0x20`, `M6610`, `MD70`, slot 0xd0 on the body | inferred (C++ read) | verified, 42.7% |
| 0x5242c0 | `CL2GiganticPenetrationDetector::Vf04` | pen_detect_big | `SCC` (slot 0xcc) gate, `MinMaxA`, `Combine`, slot 0xd0 | inferred (C++ read) | verified, 31.0% |
| 0x525a20 | `E46L1PenDetector::Vf04` | pen_detect_l1 | `S118`, `S10C` gates; `S120`; `CL2PenetrationDetector::Ctor`; `E45_M40C0`; `NotifyFiltered`; list 0x470 | inferred (C++ read) | bad, 44.4% |
| 0x526390 | `CPhysics::Visit470` | visit_list470 | slot-1 visitor over list470 skipping flag 0x40 | inferred (C++ read) | bad, 100% reccmp |
| 0x526cc0 | `CPhysics::PenCheck` | penetration_check_l1 | L1 detector (vt `0x73f5b4`) over list450 | inferred (C++ read) | called, 44.1% |
| 0x5240c0 | `E45_M40C0` | cell_lookup4 | midpoint `s = (a+b)/2`; four corners (or the margin 4.7 when flag `0x2000` is set); four `LookupTwin` cells, de-duplicated | inferred (C++ read; constant `4.7` at `0x73f7a8` checked) | verified, 43.1% |
| 0x524250 | `E45PhysBody::M4250` | pair_walk_sd4 | `MD50` on two pair frames, then `SD4` (slot 0xd4) on `a` | inferred (C++ read) | bad, 55.2% |
| 0x4f3f80 | `CCollisionEvent::Get94` | get_time | `return [this+0x94]` | known (asm) | not listed (`-`), 100% reccmp |
| 0x4f40f0 | `E37EvOps::PlaneDiff` | rel_point_velocity | `out = V_A(point) - V_B(point)`, where `V(p) = v + w x (p - centre)` from the slot-80 target `E47Solid::Vf77x` (0x52a570; the non-car branch, asm `0x52a5de` adds `+0x1c0`) | inferred (strong): the `V` formula is C++ and asm-confirmed for the non-car branch; the car branch (`S80`) is not read | not listed (`-`), 42.7% reccmp |
| 0x4f4390 | `E37EvOps::Query4390` | pair_mass_terms | two `MB560` transforms of the event vectors into the body frame; slot 0x138 (`Vf138x`) gives the effective mass terms `f88` (normal) and `f80`/`f84` (tangent) | inferred (C++ read) | not verified (`-`), 25.7% |
| 0x4f4b20 | `E37Test4B20` | is_suppressed | `(flags & 0x6000) != 0` | known (C++, header) | not listed (`-`) |
| 0x4f4dc0 | `E37EvOps::Emit4DC0` | apply_event_impulse | when not suppressed: `Vf128x` on A with `(point, +J, id, p, a, b, +d)` and on B with `(point, -J, id, p, a, b, -d)` | inferred (C++ read; `-J` for B from the C++ `Scaled(-1)`) | verified, 40.8% |
| 0x4fc1a0 | `E38Ops::M_C1A0` | solve_event | normal impulse and Coulomb friction for one event, see 3a | inferred (C++ read; asm checked for the `dot`, `k`, `blend` and the `hit` gate; the `sx` term is open 4) | verified (harness), 28.6% |
| 0x4fb720 | `E38_004FB720` | pick_material | `q = (flags & 0x800) ? +0x80 : +0xac`; return `Tag(hi)` if the weight `> 0.5` else `Tag(lo)` | known (asm) | verified, 39.4% |
| 0x4fc500 | `E38_004FC500X` | min_contact_time | `min Get94` over the list, starting at 2.0; suppressed events are unlinked and freed | inferred (C++ read) | reviewed, 64.0% |
| 0x4fca20 | `E38_004FCA20` | solve_pass | for each event: `v = time - 0.1`; if `v <= min(progress(A), progress(B))`: slot 0x13c (`Vf79x`) gates slots 0x80/0x8c/0x90 on each body with `> 0`, then `M_C1A0` | known (asm `0x4fca20`-`0x4fcb35`; constants `0.1`, `0.0`) | verified (p2w), 30.8% |
| 0x4fc590 | `E38_004FC590X` | penetration_correct | per event, gated by flags `0x800`, `0x1000` and the suppress mask `0x6000`: `fr = 1/(1 + f7/f8)` and displacement split `-(1-fr) v` on A, `fr v` on B (`M_BFC0X`, `M_BD30`) | inferred (C++ read) | verified (12084 steps), 17.5% |
| 0x5297d0 | `E47Solid::Vf79x` | inverse_mass | `return [def+0x16c]` | known (asm) | not listed (`-`), 100% reccmp |
| 0x529640 | `E46Impulse::Impulse640` | clamp_velocity_along (candidate) | see 3b | known (asm checked, whole function) | not reached on the drive |
| 0x52a0e0 | `E47Solid::Vf74x` | apply_impulse | `r = point - centre`; `L = r x J`; `w += 0.6 * I^-1 L`; `v += J * InverseMass` | known (asm checked: `0x72f684`=0.6, `+0x190`, `+0x1c0`, `+0x16c`, `0x51c8c0` on `def+0x130`) | not listed (`-`), 30.0% reccmp |
| 0x52bf30 | `E47Solid::Vf36x` | - | solver slot 0x90, run after `0x80`/`0x8c` are clear | not read | `-` |
| 0x4fbca0 | `E38_004FBCA0` | solver_hit_count | count of hit events (drives `m_unk1c`) | not read | 100% (reccmp) |
| 0x4fbc20 | `E38_004FBC20` | refresh_events | E37 refresh and flag sweep | not read | verified, 73.97% |
| 0x4fd760 | `E38Ops::M_D760X` | mesh_ray_query | nearest triangle along the segment `b -> c` (used by the E43 ground probes, `E43_GroundC.cpp:57,111`) | inferred (C++ read; asm checked for the `this` pointer of its half-vector, open 6) | not verified, 31.3% |
| 0x4fde40 | `E38Ops::M_DE40X` | mesh_ray_worklist | same as `M_D760X` over a 257-entry BFS worklist (System raycast, `E38_System.cpp:746`) | inferred (C++ read; asm checked, see 5) | verified, 23.9% |
| 0x4fd2c0 | `E38Ops::M_D2C0` | box_point_sat | separating-axis test of the box `this` against the half-vector `c` at offset `b` | inferred (C++ read) | verified, 22.3% |
| 0x4fd2a0 | `E38_004FD2A0X` | box_point_wrapper | calls `M_D230` with ECX=`self`, `b=arg`, `c=arg+0xc`; returns with `ret 4` | known (asm and forced direct-call replay) | 100% reccmp; direct entry replay confirmed helper ECX and arguments |
| 0x4f58f0 | `E37GridOps::Set68A(v)` | set_grid_flag | sets/clears bit 0 at `this+0x68a` | asm preserves EDX; C++ may clobber it; no callers found in `src/` | 100%* reccmp; ABI-only observation, left unchanged |
| 0x4f3950 | `E36Collect` | collect_bodies | cell query over the box `+0x10 +- 0.1`, then for each candidate body with `+0x680` set: transform its OBB (`+0x20/+0x30/+0x40` scaled by `+0x10/+0x14/+0x18`), `E36ObbTest` or `E36SphereBox`, store up to 8 records (`+0xc4` flag byte from `rb+0x68f`) | inferred (asm partial: calls and scales checked, section 5) | bad, 17.9% |
| 0x4f2ce0 | `E36SphereBox` | sphere_box | sphere-vs-box test, one contact record | inferred (C++ read; asm prologue checked, section 5 S10) | bad, 25.0% |
| 0x4f3030 | `E36ObbTest` | obb_test | OBB overlap test, contact record | not read | verified, 6.7% |
| F2 (in `Vf6cx`) | `CCar::Impulse640` call | car_wall_impulse | `Impulse640(out, perp, _, dt)` on a brake-wall hit | known (asm call site `0x4ea372-0x4ea39a`) | not reached |

### 3a. `M_C1A0`: the normal and friction impulse for one event (C++ read, harness-verified)

Let `p = PlaneDiff(ev)` (relative contact velocity, m/s), `n = ev+0x70` (unit), `dot = p . n` (m/s;
negative means approaching). `hit` is set if `dot < -1` or flags `0x800 & 0x1000`; the function
acts only if not suppressed, `hit`, and `dot < 0`.

- Tangent: `t = p - n*dot`, `|t| = len`, unit tangent `u = t/len` (or zero if `len <= 0.001`).
- Blend: `k = clamp(-0.05*dot, 0, 1)` (fast approach gives `k = 1`).
- Material: `(mu_low, mu_high, e_a, e_b)` from `E38_004FB720`; `mu = lerp(mu_low, mu_high, k)`.
- Effective inverse mass along `n`: `f88` from `Query4390` (the normal denominator).
- Normal impulse: `a = -(1 + e) * dot / f88`, with `e = lerp(e_b, e_a, k)` in the C++ (`sx` term).
- Tangential residual: `f5 = f84*a + len` (speed left after the normal impulse), `q = f5 / f80`.
- Friction: `f6 = min(|a|*mu, |q|)`, sign set so it opposes `f5`'s direction along `u`.
- Total impulse: `J = a*n - f6*u`, applied by `Emit4DC0` (section 3 row `0x4f4dc0`).

Units check: `dot` in m/s, `f88` in 1/kg, so `a` is kg m/s (an impulse) and `J/m` is m/s. This is why
`dot` is read as a relative velocity, not a penetration (the `-1` gate is `m/s`).

## 4. Open questions

1. **Brake-wall hit on the drive.** `Impulse640` (`0x529640`) is absent from `called.csv`, so the
   car's wall hit branch was not taken on tanner-corolla, or the log misses it. Settled by a drive that
   crosses a wall, or by `called.csv` for a wall-heavy stage.
2. **Which CDef CReal is which tweak key.** CDef+0x1610c (tolerance, `Impulse640`), +0x1611c
   (coefficient), +0x16100/+0x16110/+0x16120 (three `CReal`s built with no name). The key table
   (`0x3cbb40`-`0x3cbb90`) lists `BrakeWallBuffer`, `BrakeWallDamping`, `BrakeWallFactor` but the ids are
   not read by any code found. Settled by the tweak loader (`NTweak::CBase::Parse` 0x52f0c0, reccmp 26%)
   or by a harness peek of the three values against the LSP.
3. **Material reals.** Which of `+0x108/+0x118/+0x128/+0x138` is `CollMyHigh`/`CollMyLow`/`eHigh`/`eLow`,
   and what `e` means in the impulse (restitution, or a stiffness with a different use). Settled by the
   material loader (`CMaterial` keys) and the `M_C1A0` asm around `0x4fc2f0`-`0x4fc3b1`.
4. **The `sx` term in `M_C1A0`.** The C++ reads `(x128 - x138)*k + x138 + 1`; a first asm read
   suggested `+x128` and a blend factor, but the stack slots are reused across the `Query4390` call
   (`0x4fc371`) and the read is not conclusive. Harness status is verified, so the C++ is the working
   reference. Settled by a full asm trace of `0x4fc2e3`-`0x4fc3b1`.
5. **Solver def.** The solver's `Restitution`, `HyperRestitution`, `MinInvEffRedMass` and `nSteps` are
   loaded into `CCollisionSolver::CDef` (+0x16024), but no read of those fields was found on the path
   (`M_C1A0` does not read them). Settled by tracing the reads of `m_def14` (`0x739978` solver) and the
   `CCollisionSolver::CDef` offsets.
6. **Angular factor 0.6 in `Vf74x`.** `w += 0.6 * I^-1 (r x J)` (constant `0x72f684`, asm checked). The
   physical formula has factor 1. Either the original applies a deliberate 0.6 damping on contact
   rotation, or the constant is a tuning value. Settled by its LSP key (none found) or by the designer's
   intent (lead).
7. **Car branch of the point velocity.** `E47Solid::Vf77x` (slot 80, `0x52a570`) takes a different path when
   the body has a car at `+0x44` (it calls `S80` and `MB560`). The non-car branch is the point velocity
   (asm checked). Whether a car-attached body is ever a contact body on the reference drive is not known.
   Settled by reading the car branch and the drive's `+0x44` values.
8. **Which bodies get `E38_004FC590X`.** It runs only after `PenCheck` reports a hit; the flag gates
   (`0x800`/`0x1000`/`0x2000`) are C++ reads. Settled by an asm pass over `0x4fc590`.

## 5. Suspect code

Asm is the reference. Addresses are VAs.

**S1. `E38Ops::M_D760X` reads an uninitialised array as the vector it passes (`RBR_E38_System.cpp:158-160`).**

> Lead correction (2026-10-14, P3R): the "ro" reading below is wrong. At the original call site `lea ecx,[esp+0x38]` points at the three contiguous `c-b` locals (completed by `movss [esp+0x40],xmm0`), i.e. the ray direction `rd`; passing `ro` diverged at step 40 in 445 words. Main passes `rd` (P3R fix 6fe0dc63), and D760 is verified with that.
`float m1[3]` is never written, and `((E38Vec*)m1)->M_D030(0.5f, m2)` computes `m2 = 0.5 * m1` (garbage).
The asm (`0x4fd7cc`) passes `ecx = esp+0x38` with `esp = frame-24`, which is the frame slot of `ro`
(`b`'s copy, written at `0x4fd76d`-`0x4fd77d`). So the original computes `m2 = 0.5 * ro`, the ray origin
halved, and passes it to `M_D2C0` as the box half-vector. The C++ should use `ro` as `this`, so the
behaviour matches the original. Whether `0.5*ro` is the intent (`0.5*rd` would be the half-segment) is
for the owner to decide, because the original may be wrong. Impact: this is the terrain ray used by
`E43_GroundC.cpp:57,111` (wheel and car ground probes), so it can change ground contact. Not on the
harness (status `-`).

**S2. `E38Ops::M_DE40X` has the same pattern (`RBR_E38_System.cpp:340-342`).** `m1` is uninitialised.
The asm (`0x4fde58`) passes `ecx = [esp+0x460] = S+8` at this frame depth, which is the second argument
`c`, so the original computes `m2 = 0.5 * c`. The C++ should pass `c` (the second argument) as `this`.
Status `verified` (23.9%). The harness result does not show whether the wrong half-vector changes any
result here. The C++ should pass `c` as `this` to match the asm; lead to decide the priority.

**S3. `E36Collect` passes the wrong operands to `E33Vec4::Scaled` (`RBR_E36_Collide.cpp:369-371, 418-428`).**
`t30`, `t40` and `t20` are declared and never written, so `Scaled(t30[0], ...)`, `Scaled(t40[0], ...)`
and `Scaled(t20[0], ...)` use garbage as the scale. The second argument `*(void**)(hit + 0x10)` reads a
float as a pointer, and the output role is wrong. The asm (`0x4f3b56`-`0x4f3bd5`) is:
`Scaled(this = hit+0x20, scale = float [hit+0x10], out = stack temp)`, then
`Scaled(this = hit+0x30, scale = float [hit+0x14])`, then `Scaled(this = hit+0x40, scale = float [hit+0x18])`.
Each scaled axis is then transformed (`0x51b560`) and its length taken (`0x473b30`, three calls at
`0x4f3bef`-`0x4f3c0f`). The C++ must use the three half-sizes as the scales and the axes at `+0x20/+0x30/+0x40`.
Status `bad` (17.9%); the C++ is not behaviourally exact.

**S4. `CPhysics::CollideWork` documented as sliced (`notes/physics/step.md` 2e, `collision.md` 1b).**
Not a C++ bug: the C++ `int i = 1` guard (`RBR_E46_Small.cpp:244, 258`) matches the asm (`0x526d90`-`0x526da8`).
The doc is wrong: one detection pass per call, no slices. Route to the lead to correct the doc.

**S5. `CollideWork`'s dead `f8` term (`RBR_E46_Small.cpp:238`, `f8 = f * 0.0f`; `f7 = f6 - f8/loc20`).**
The asm keeps the multiply (`0x526d42`, `mulss` by `0x795e50 = 0.0`) and the division. For finite `f` the
term is 0, so it has no effect. Harmless; the C++ can drop it if the reccmp owner wants a cleaner
function, but reccmp counts the instruction, so leave it unless the owner says otherwise.

**S6. `Impulse640`'s 4th parameter is named `mu` but is a time (`RBR_E46_Math.cpp:560`; withdraws K1).**
`mag/dot2` has units `(m/s)/(m/s^2) = s`, so the test `mag/dot2 > mu` compares seconds with the 4th
argument. The caller passes `dt` (`RBR_E34_CarD.cpp:644-646`). So the call is consistent and the
parameter should be renamed `dt` (or `tau`). The asm (`0x5296ac`-`0x5296cc`) agrees with the C++. Not a
behavioural bug. K1 in `collision.md` is withdrawn.

**S7. `Impulse640`'s 3rd parameter is unused in the asm (K2 confirmed).** The asm reads only `dir`,
`n`, `mu` and `this`. The caller passes `*(void**)(bw+0x10)`, a float's bit pattern. Harmless, but the
type `void*` is misleading. Keep, or name it `unused` in the header.

**S8. `E38_004FCA20`'s margin `- 0.1`.** Checked: the asm uses `[0x72b068] = 0.1` (`0x4fca5e`). Correct.
Not a bug; listed because it is a magic number the decomp owners may want to name (`kSolveLead`).

**S10. `E36SphereBox` evaluates its axis projections in a different order and precision.** The asm
(`0x4f2cf1`-`0x4f2d3d`) works the centre difference in x87 80-bit registers and sums
`dz*p38 + dy*p34 + dx*p30` (the C++ sums `dx*p30 + dy*p34 + dz*p38` in `float`). Both the rounding and
the summation order differ, so a bit-exact match needs the x87 sequence. Status `bad` (25.0%). The rest of
the function (the `pick` and normal stores) was not compared. Lead to decide whether this needs an exact
rewrite or only the behavioural fix.

**S9. `E45_M40C0`'s margin `4.7` (flag `0x2000`).** C++ `(4.7f)` matches `[0x73f7a8] = 4.7`. Grid
lookup order was not asm-checked; the function is 30% in the C++ source header. Lead's decision whether to
check.

Summary of what the decomp owners should take: S1 (`M_D760X`, high impact, not on harness), S3 (`E36Collect`,
bad, asm-checked), S2 (`M_DE40X`, asm-checked, verified status). S4 is a doc fix. S5-S9 are notes.
