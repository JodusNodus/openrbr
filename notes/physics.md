# Physics: the step and exact ports

What a porter needs to keep `crates/rbr-physics` bit-exact against the original under NGP 7.4.778 (the ports are of the 1.02
exe's functions with NGP's behaviour; no 1.02-only arm remains). Modules and `ports.csv`:
[crates/rbr-physics/README.md](../crates/rbr-physics/README.md); checks: [verification.md](verification.md); NGP:
[ngp.md](ngp.md). Full field tables and asm notes per subsystem: rbr-recomp `notes/physics/*.md`.

## Step

`TicPhysicsRecording` (0x47f420) calls `CPhysics::StepForward` (vtable slot 2, 0x5273a0) once per substep with
`p1 = 1`. `dt = CDef+0x1608c` (`MaxTimeStep`, 0.007 s retail, not 0.01).
`CDef = *(g_1659b70+0x24)`. The collision window is `4*dt` (`0x72d150`). Per substep, in order:

1. First call only (`counter == 0`): S48 over list450.
2. `counter++` (CState `+0x18`), `Telem60`; S20 over list450.
3. `Work5500(dt)` (0x525500): all S5C, then all S64, then S6C(dt)+S58(dt) per node.
4. S24 over list450.
5. Unless `SUB_STEPPING` is set and `counter & 3 != 0`: if `CHECK_COLL`, `CollideWork(4*dt)`; if `CHECK_PENETR`,
   `PenCheck` and, on a hit (`0x4fbca0`), `FBC20`, `FC590X`, `FCA20`, solver S08; then solver S08 again, S54 and S48
   over list450.
6. S98 over list480; `sim_time` (CState `+0x20`, f64) `+= dt`, the sum rounded to f32 at PC=24
   (`physics::advance_time`).

After the loop: the player car's `CDrive::Set4510` (0x504510), `g_1659b74++`.

CDef `+0x160cc` flags (exe names, table `0x7cbc10`): 0x10 `CHECK_COLL`, 0x20 `CHECK_PENETR`, 0x1000 `SUB_STEPPING`,
0x8000 `ENABLE_SINGLESTEPPING` (CPhysics `+0x5c` delay, `+0x60` pending). The wheel reads bit 0x10000 (ground probe),
damage and the gear logic bit 0x80000. Debug `+0x160e0` bit 0x20000 writes the CPhysics `+4/+8/+0xc` counters; log
`+0x160f4` bit 0x4000 (`WHEEL_FORCES`).

`CState::RegWall` (0x5263d0) files a body: `S8C != 0` to list480 (`+0x480`), else `S80 != 0` to list450, else list460
(`S88 == 0`) or list470. List nodes: `+0x10` next, `+0x14` prev, `+0x18` list, `+0x60` bit 0x40 skips `Visit470`.

| Slot | CSolid (vtable 0x73fc54) | CCar |
| --- | --- | --- |
| S20 | 0x529200, jmp to the child walk 0x51a3a0 | `Vf20x` 0x4e72d0 |
| S24 | 0x519bc0, `ret 4` | `Vf24x` 0x4e58e0 |
| S54 | 0x52b6b0 | `Vf54x` 0x4e7f90 |
| S58 | 0x51a250, child walk | `Hook` 0x4e9930 |
| S5C / S60 | 0x529240 (`E43Vf5C`, then S60 = `Accum250` 0x529250) | `Vf5cx` 0x4e5c60 |
| S64 | `Vf25x` 0x52cb20 | `Vf64x` 0x4e9020 |
| S6C | `Vf27x` 0x52cc30 | `Vf6cx` 0x4ea300 |
| S98 | 0x51a7c0 (unlink, slot 21 on the def) | `Vf98x` 0x4e6dd0 |

`CollideWork` (0x526d20) runs its body once per call (no slices): `Work6a70` detects, `FC500X` takes the minimum
event time (`Get94`, event `+0x94`; starts at 2.0, frees events with flags 0x6000); above 1.1 nothing happens. Else
`fc = clamp(t, 0, 1)`, `Work5640` (ignores its argument) back-steps each contact body (flag 0x4) to its event time,
`FCA20` solves every event with `time - 0.1 <= min(progress A, progress B)`, `Work5760` advances the rest. The dead
`f*0.0` term stays (NaN for inf/NaN `f`).

## x87 and float rules

The 1.02 physics runs the x87 at PC=24 (control word 0x007f on every captured call; `CreateDevice` at
0x40c418 omits `FPU_PRESERVE`). 64-bit precision never occurs. Only `E09Timer` (0x465be0,
0x465c90) and two D3D `_controlfp` scopes switch to 53 bits and restore. Under NGP the race runs at PC=53 (0x27f) and
the load at PC=24: `fpu::precision()` is the thread's, set by the host (`fpu::set_precision`, `Precision::scope`).

| Class | Asm | Rust |
| --- | --- | --- |
| S | SSE scalar on f32 | `f32`, same operation order |
| M | one x87 op, then `fst dword` | `f32` at PC=24 (f64 then `as f32` at PC=53) |
| C | x87 chain, st0 carried | f64 with `fpu::round24` (or `round_x87(v)`) after each op |
| R | float returned in st0 | return the chain's type; the caller stores or keeps it as the asm does |
| Q | `fsqrt` | correctly rounded at the precision (`f32::sqrt` at PC=24) |
| T | `fsin`/`fcos` (`__CIsin` 0x53f3b0, `__CIcos` 0x53f3a0) | `math::trig` (`X87Ext80`: 64 significand bits, 66-bit Pi reduction, `|x| >= 2^63` returned unchanged) |

- Precision control rounds arithmetic only: `fld`/`fst` of a double is exact. The x87 exponent range is wider than
  f64's; `round24` keeps it unbounded within f64, so check any new chain whose values can leave about 1e±308.
- Follow the asm's operation order, not the C++ expression (MSVC reassociates). An SSE result stored to f32 before
  an x87 part is a rounding point.
- SSE NaN and infinity: `math::sse::{add, sub, mul, div}` return the quieted NaN operand (right one first) and
  `0xffc00000` for invalid operations. Use them where an operand can be NaN or infinite.
- `rsqrtss`/`rcpss` are 12-bit table approximations: `math::rsqrt`, `math::rcp` (tables measured on the reference CPU).
- Comparisons copy the asm's branch sense so NaN goes the same way: `!(a > b)` is not `a <= b`; `comiss` + `jbe`
  is taken when unordered (`math::le_or_unordered`). No `f32::clamp`, `min` or `max` on values that can be NaN.
  `math::abs_asm`, `math::sign` copy the asm forms.
- Float to int: never `as`. `fpu::trunc_i32` (`cvttss2si`) and `fpu::round_i32` (`cvtss2si`, `fistp`) give
  `INT_INDEFINITE` (`i32::MIN`) for NaN and out of range; `math::ftol` is `_ftol2` (0x70c2c4, low word of the int64).
- Constants keep the exe's bit patterns (some pools differ from the literal, e.g. 0x3d4cccd0 for 0.05). A punned
  word (float bits in an int) stays `u32`; build floats with `f32::from_bits`.
- Default NaN sign. Where a port makes a NaN with host arithmetic, the arm64 host gives `0x7fc00000` where the
  original wrote `0xffc00000`. Tests accept "NaN for NaN" only at named sites: `tyre_vectors` (`same_words`: solvers,
  `step_inner`, `solve_patch`, `contact_patch`, `solve_forces`, `friction`, `heat_step`, `table_step`, `relax_step`,
  `friction_curve`, `write_outputs`), `car_body_vectors` (`same_word`: `to_quat`, `steering_ffb`), `entities_solve`
  (`+0x624`), `entities_slip`, `spring_damper_vectors` (st0 returns, `check_spring_windows`),
  `engine_driveline_vectors` (`check_st0`, `check_windows_nan`), `wheel_vectors` (`split_on_sub`, sign only). Recorded
  st0 NaNs carry no sign (`harness/vcu_vectors.py`). `harness/rust_step.py` excludes words that are NaN in both runs and differ only in bit
  31. A new site gets its own entry, never a blanket tolerance. An x86_64 host makes `0xffc00000` instead, and
  `engine_driveline_vectors` `sim_e660` then fails (call 558, window 2's last word: the record holds `0x7fc00000`;
  CI run 39436445583), so the vector targets are checked on arm64 (CI's macOS job).
- Stack residue is behaviour. Callees that leave arguments on the stack feed the next call: the slot-12 identity
  getter (0x519c00, `mov eax, ecx; ret`) keeps its pushes for the following `ComposeA`; `MB560` (0x51b560) and
  `ComposeA` (0x51b4c0) are thiscall `ret 0xc` with three arguments, and some callers reuse earlier pushes;
  `CSolid::E43Vf6C` leaves three words for 0x51b4c0. Unwritten lanes (a w lane after a three-float write, then a
  four-lane `CopyFrom`) hold stack garbage; tests exclude those words by name.

## Memory and porting rules

- Ports are pure functions over typed values; raw offsets and `Mem` only in the subsystem's `layout.rs` or
  `layout/`. `Mem` panics on an unmapped byte; a skipped read or a write outside the recorded windows is caught only
  by reading the asm.
- Block copies use `Mem::copy` (a `memmove`, as a read then a write) or `Mem::copy_words` (a forward `rep movsd`);
  halfwords `Mem::u16`. On pages they are one range check and a word loop (the tanner boot: 7 % fewer instructions).
  The page marks stay: with unmapped reads as zero only 1178 of the tanner boot's 13000 steps are equal.
- Grep `src/` for the address before porting: a ported function is called, not copied (pure math in `src/math/`).
- Another subsystem's callee is called through its typed function when it has a `ports.csv` row, else it is a host
  method (one per callee, address in the doc comment). A subsystem's own functions are ported, not hosted.
- The `ports.csv` row goes in the port's commit. No rows (no records): 0x51c810, 0x51be56, 0x51bf3c.
- Pure-Rust gaps: tyre `Step`, `Locate` and `HeatStep` (0x534090) read tables at indices outside the recorded
  window (the binding returns `Outside` and runs the original); pure Rust needs the def's raw words there.
- The asm decides, not the C++: where the reconstruction has a bug (uninitialised arrays in the E38 ray worklist,
  indeterminate scales in `E36Collect`), port what the original does.
- Dispatch slots and signal emits go through a host trait in the original's call order; tests check that order.

## Store seam

One physics code over two storages (Phase B). A subsystem's store trait names its layout's load and store groups;
`Mem` implements each with the layout's bodies. Sequence functions are generic over the trait (`fn step<S:
SpringStore>`), never `dyn`, so the `Mem` path compiles as before.

| Trait | File | Mem impl |
| --- | --- | --- |
| `store::Raw` (supertrait: words, `mem()`) | `store/mod.rs` | same |
| `SpringStore` | `spring_damper/store.rs` | `spring_damper/layout.rs` |
| `VcuStore` | `vcu/store.rs` | `vcu/layout.rs` |
| `TyreStore` | `tyre/store.rs` | `tyre/layout/kernels.rs` |
| `WheelStore` | `wheel/store.rs` | `wheel/layout/fields.rs` |
| `CarStore` | `car_body/store.rs` | `car_body/layout/car.rs` |
| `EngineStore: SpringStore` | `engine_driveline/store.rs` | `engine_driveline/layout/mem_store.rs` |

- A calls trait takes the store as a parameter defaulting to `Mem` (`Vf6cxCalls<S = Mem>`); clashing names use UFCS;
  what a trait does not name yet goes through `Raw`. Constructors and setup copies stay on `&mut Mem`.
- `object!` (`store/layout.rs`) tables an object once (struct, `Field` per row, `lift`/`lower`); tabled so far
  (`store::car_objects`): tyre scratch, strut and state, VCU scratch and state.
- Mem-direct dispatch (`--dispatch direct`, live default): a call within the `IN_PLACE` subsystems
  (`rbr-hybrid/src/pure/mod.rs`) to a leaf (`pure/leaves.rs`, from `scripts/pure_leaves.py`) runs as a plain call
  when it passes exactly the words the leaf pops. A non-leaf binding runs as a plain call too (B1b,
  `Pure::call_plain`, `call_framed`): its words are written as the host writes them, nothing is read back and it runs
  in its own frame, so the image is the host path's word for word; `args` must hold the words it pops
  (`Pure::stack_words`). The watched emits and the tyre steps always go through the host (`NOT_DIRECT`).
  `Host::call_direct` leaves the words unwritten where the callee reads only its arguments (the ground probe's
  callees, `E45_M40C0`'s `LookupTwin`, the CRT vector iterators). The bindings get `Direct` (`pure/mod.rs`) as their
  host; `Pure`'s own `Host` is the fact-check's (`--dispatch pure`). The 176-step NGP bench: 739.8e9 to 678.3e9
  instructions.
- Checks: `round_trip` (`rbr-hybrid`), `rbr-standalone --twin mem|mem-direct`, the `poison` feature.
- Left: traits for rigid_body, physics_core, collision, entities, stage, load, ntweak; `--twin` in the live run.

## Physics core

`src/physics_core/`. `CPhysics` (0xf8): `+0x1c` contact hit, `+0x20/+0x24/+0x28` state, def, scratch. CScratch
`+0x10` is the solver, `+0xe848` the handler stack (top `+0xe888`, -1 empty). Detectors walk the node, not `this`: `PenCheck` det = `{0x73f5b4, 1.0,
scratch+0x10, state}`, `Work6a70` det = `{0x73f7b0, f, scratch+0x10}`. `E46Mat` (10 floats): `Mul`, `ScaleRows` use row vectors, `XformRows`, `MulVec` column vectors;
`ScaleRows` lane 3 is `x*m3 + y*m6 + z*m9`. `E46_IdxWrite` (0x528980) clamps below to `(0,1,0)`, above to
`(n-2,n-1,1)`.

Signals: every `CMultiSignal1::Emit` is one template (the instances differ in the logged name, `EMIT_NAMES` in
`rbr-hybrid`). A set guard logs `E01Log(6, 0, ...)` and returns; a count of 32 or more logs `E01Log(7, 0, ...)` and
runs `int3` (entry + 0x3d), so the bindings decline only that (`layout::emit_traps`); `physics_core_vectors`
`emit_*_matches` replay the guard path. `E01Log` (0x43c8e0) is `GameLog` (0x490f50): the sink of (channel, level) from
0x7c29d8 and 0x7c615c; the debugger's sink (0x4dedb0) formats into 0x7ece58 and copies the text to an `operator new`
block for `OutputDebugStringA`, the file sink (0x4ded80) writes nothing with no log file open. `game_log::format` is
msvcr71's `vsprintf`, `%f` through `$I10_OUTPUT`'s 96-bit arithmetic (`game_log::ld12`, its power tables read from the
DLL in a test): `game_log_vectors`, 4000 direct calls.

## Rigid body

`src/rigid_body/`. `CSolid` (0x3d0). NTweak values sit at `+0xc` (CReal) or `+0x10` (CVector) of the member. Def
(`+0x34`): inverse inertia `+0x130`, `InverseMass` `+0x16c`. Body: flags byte `+0x60` (0x4 contact body, 0x8 contact
list), parent `+0x44`, quaternion `+0x100` (w last), position `+0x140`, `w` `+0x190`, `v` `+0x1c0`, history
`+0x200`/`+0x220`, progress `+0x290`, window start `+0x2a0`/`+0x330`, force `+0x370`, accel `+0x320`/`+0x380`.

- `Accum250` (S60): `F = MB560(gravity CDef+0x160b0, S30) / InverseMass`, into `Accum340` (`+0x370 += v * +0xd0`).
- `Vf25x` (S64): vote `Vf81x`, contact list `BigB` (0x52c220) when flag 8, `+0x380 = R(F * InverseMass)` from `+0x370`,
  `+0x320` from the torque shell, then clears `+0x370` and `+0x310`.
- `Vf27x` (S6C): `w += alpha dt`, `v += a dt`; with flag 8, `|v|^2 < 1e-5` zeroes `v` and `|w|^2 < 1e-5` zeroes `w`;
  `VfC8(dt, lin, ang)` (`q = normalize(M83DD0((0.5 dt w, 1), q))`, `p += dt v`, parent path, flag-8 track progress
  via `SampleBig` 0x50b300); estimates `(x - x_prev)/dt`, filters `0.2 old + 0.8 est`; history copies; `E43Vf6C`;
  `Vf24x` sleep; emit `CSigMrRBodyState` (its 16-byte position copy at `+0x14` overwrites the quaternion's x: original).
- `VfDx(a)` (0x52cfd0): if `a < progress`, `q = normalize(a q + (1-a) q_start)`, `p = a p + (1-a) p_start`; root link
  `this+0x160`. `VfEx(a)` (0x52d110): `s = (1 - progress) a`, quaternion turned by `0.5 s w`, position by the full `s v`,
  progress 1.
- `Vf74x` (0x52a0e0), impulse: `v += J * InverseMass`, `w += 0.6 * I^-1 (r x J)`.
- `BigB` per record: `dir * 5000 * clamp(0.5 fraction)` plus, when `|W| > 1e-4`, `(W/|W|) * clamp(5 weight) *
  min(|W|, 10) * -50`, in that order.

## Car body

Rust: `crates/rbr-physics/src/car_body/` uses physical names; offsets and callee addresses are in `layout/`. Original to Rust
modules: `Vf6cx` `step.rs` (`step_car`), `Vf28x` `publish.rs`, `Vf64x` `forces.rs`, `UpdateSeed` `control.rs`, `ApplySeed`
`setup.rs`, `UpdateMrr` `wiring.rs`, `Update9AF0` `retire.rs`, `Sample9800` `sensors.rs`, E36 helpers `helpers.rs`, aero
`wall_friction.rs`; `ports.csv` maps each address.

`src/car_body/`. `CCar`: def `+0x34`, grid `+0x64`, damage handler `+0x3cc` (copied to `+0x9f4`), wheels `+0x470`,
hinged `+0x480`, mounted `+0x490`, drive `+0x4b0`, engine `+0x690`, lights `+0x838`, VCU scratch `+0x850`, passenger
`+0x8b0`, state `+0xa00`. `Vf6cx`: timer `+0x1670 -= dt` (floor 0), brake-wall ray, `+0x166c` penetration, speed
`+0x15fc`. `Hook`: steer slew of `+0xafc` toward `+0x16a4` at 20/s, rack `+0xb0c`, VCU, drive (0x50a1e0), `E43Vf58`,
`Damp66D0`, FFB filter `Hook1FF0`. Drag `Sim7DC0`: `-0.5 Cd A |v| v`
(`def+0x4490`, `+0x4480`), arm `(0,0,0.05)`, spin damping `-5(|w|-5)^2 w/|w|` above 5 rad/s. `Sim7AD0` weighs by
`clamp(0.01 speed)` of the raw speed. `Xform6870` calls `ComposeA` with no pushes of its own.
`Snap9360` (0x4e9360, StepEngine with `+0x9e4` set; 0 in every snapshot) emits the animated replay's state
(`CSigCarStateAnimatedReplay`, 0x4e85b0); its payload's first 16 bytes and an absent wheel's pair are stale stack. The car's
`CMultiSignal1::Emit` template logs `E01Log(6, 0, 0x730358, name)` and returns when busy; an index `>= 0x20` logs and
traps (`int3`). Debug graphs (`graph`): `CGraph2D::Push` (0x495090, car `+0x16ac`, CDef `+0x160e0` bit 0x10) and
`CGraph1D::Push` (0x494cf0, engine, bit 0x200) sample every period-th push into a ring; the bits are clear in retail.

## Jointed bodies

`src/entities/` (hinged; the model is in `hinge_*.rs` by quantity: pose, limit springs, end stops with restitution, pivot and slip torques, solve; `layout/` has the offsets) and `src/physics_core/` (mounted, passenger). `CHingedRigidBody` (0x660, vtable 0x73d23c):
`+0x3d0` is the owner car, not a def. Angles `+0x3e8+4i`, `+0x3fc`, rate `+0x40c`. `SimCF10(idx)` limit term per Block60 (any idx: SimEA80 passes 0 and 1)
(`def+0x1f0+0x60i`, mode 2 active); `SimCFF0` sums `(y*c1 + z*c2) + x*c0` and returns `(r1*c1 + r2*c2) + r0*c0`,
negating when unordered. `SimEFE0` stores the angle as float bits at `+0x3fc` and copies the rotation from `+0x100`.
`CMountedRigidBody` (0x590): owner `+0x3d0`. `CPassenger::Step` (0x523160): spring `M29E0` (30000, 900).

## Wheel

`src/wheel/`. `M3DD30` (0x53dd30, slot 96): clamp travel `+0x5bc` to the strut range, frames, spin `+0xa50` from
`car.Fwd6DB0(+0xeb4)`, ground probe (`M3D520`; off ground sets `+0xebc = 0`), tyre step on `wheel+0xf00` (the wheel's
own scratch is `+0x800`), strut: `v = (v + dt m (g + o1 + o2 + f34)) / (1 + dt m o3)` with `m = def+0x16c`, `g` from
`CCar::GetA48` (0x4e6730), clamped to ±100, `x += dt v`. The asm has no int conversions. Host calls take the callee's
entry stack pointer from the asm's pushes.

- `Unk94d0` (0x5394d0, `layout::split_on_sub`): splits `v` (xyz only) on the sub-object's axis `sub+0x20`; the remainder
  goes to `wheel+0xe40`, its lane 3 the stale stack word at `((esp-4)&~0xf)-0x14`; returns `wheel+0xe40`. Unicorn picks
  between two NaNs the x87 way (larger significand, then positive), not SSE's first operand, so its records differ from
  `math::sse` in NaN sign only (seen both orders).
- `M3B880` (0x53b880, `refresh_travel`): only the clamp of `+0xedc` to 0 has an effect; its overlay (0x539bb0) and the
  ClearBias/ComputeTravel/TravelRange calls write only their own frame (direct records with every flag combination:
  no other write). Returns the flag word when bit 0x400 is clear, else the old `+0xedc`.
- `E51Wheel::Ctor` (0x53b6e0): with an owner it wakes the solid through `CSolid::E43Vf90` (0x52bf30) on the wheel.
- `CSolid::Step77` (0x529d30, `vote::probe_ground`): plain thiscall `ret 8`; the old "ABI-only" note was the C++
  transplant's stack alignment. `Step77` and `M3B880` run from `CWheel::Vf28` (per frame), not in the physics step.

## Tyre

`src/tyre/`. `CScratch::Step` (0x5360c0) on `wheel+0xf00`: `CopyFrom` (39 material words), `Align`, `ContactPatch`,
`ComputeSlip` (`s = M dir`, `v_long = R w + s1`, `slip = -(s1/R) - w`), `Accumulate`, `SplitSlip`, `SolvePatch`
(`StepInner`, `TyreClamp`), `SolveForces`, `Sign`, `HeatStep`, outputs. `this+0x378` is the spin. Original quirks: `F[0x7A]` is multiplied by `e3` twice; `Accumulate` and `WriteOutputs` multiply by a
zeroed register (output `+0x98` is always 0); `F[0xC5]*0.1` rounds before `*F[0xFC]`; `SolveA` and `SolveDispatch`
recurse into each other. `HeatStep`: SSE `k*0.5 + b + a`, `e-n` rounded to f32, `DamperStep1*c` kept double.

## Spring-damper

`src/spring_damper/`. `CSpringDamper` at `wheel+0xec4` (def `+4`, state `+8`, fade pointer `+0x10` read without a
check; its writer is not found). `Step(x, y, z)` (0x52e090) with `y = x + def[0x41c]`: main spring `ks (L0 - (x - Hm))`,
series helper `ks kh/(ks+kh) (Hl + L0 - x)` when the helper is engaged; bump stop `(Lo - y) kb`, droop `(Hi - y) kd`;
high-speed damping above the breakpoints with offset `(ch - c) v_break`; fade when `27000 < s[0] F` adds
`((F - 27000)/18000)^2`. `UpdateForces` (0x52dd70): `max(400, (1-0.7f) bump)`, `(1-0.3f) rebound`, `(1-0.8f) hs_bump`.
The `Damper*` kernels (0x52e350-0x52e8a0) serve the engine and tyre too.

## Engine and driveline

`src/engine_driveline/`: the model modules name quantities (`engine::next_random`, `drive_line::hermite`,
`sim::find_bracket`); the table in memory (`layout/drive_line.rs`) and every address stay in `layout/`, `ports.csv` and
the hybrid bindings. Drive state: gear `+0x54c`, commanded gear `+0x570`, engine speed `+0x45c` (rad/s), clutch
slip `+0x60`. `Set4570` (0x504570): a NaN lock takes the upper clamp and stays NaN. Clutch `M93A0` (0x5093a0): lock and
slip with latch `+0x368`. Engine state: speed `+0x188`, cut flags `+0x168..+0x174` (`SimE010` sets one when
`x <= limit` or unordered). LCG `s*0x2000081 + 0xe623f41d`. `Sim8E20` (0x518e20) clamps `b` to ±1151.917, filters `+0x188 =
0.3 b + 0.7 old`, and returns the clamped `b` in st0 (callers multiply by `0x73a2e0`); `Sim7D30` is the turbo energy
step (shaft clamp 20943.951 rad/s).

## VCU

`src/vcu/`. `CScratchSim::Step` (0x537f50): `Update` (gears), clutch (`UpdateC` when launch mode, else the fixed
clutch), `UpdateCut`, pedal routing (swapped when `vcu_enabled && b+0x58`), `UpdateE` (diff locks), neutral light,
cooldown decay. Per the asm: the gear-2 and gear-0 requests jump to the tail with lockout `def+0x168`; the
first-gear neutral request needs `rpm > 5000`; `b+0x58 = brake > throttle` (strict); the lockout is written only on a
request; `UpdateE` uses one speed key `Get4540 * 0.3` for all three maps; `E50_QuerySlip` rounds the rpm to f32 before
the ratio. The pedal and speed tables (`table_blend`, `lookup_pair_table`, `lock_max`) are x87 chains: they round at the
thread's precision (PC53 in NGP's race; RSF drive2 attempt 5 parted at its first launch without it).

## Ground and surfaces

`src/entities/ground.rs`. `CGround` is one static node; queries go to the host at `g_7c755c` (slot 0 `S00`).
`VfE8` (0x51c6f0) record: `o[0..7] = out[0..7]`, `o[8] = out[10]`, `o[9] = out[11]`, `o[10] = out[9]`, `o[11..12] = 0`,
`o[13] = out[12]`, `o[14] = this`. `CollideInit` (0x51bd30) takes its second grid pass on bit 3 of `bodyA+0x60`.

## Collision and contacts

`src/collision/`, `src/rigid_body/`. `src/collision/` is a model plus its layout: the geometry and arithmetic are in
physical terms (`overlap`, `triangle`, `obb`, `face_sweep`, `edge_pair`, `mesh_grid`, `mesh_hit`, `cells`, `grid_contact`,
`brake_wall`, `contact_solver`) and the game memory, the original's calls and their sequencing are in `layout/` (the
`*_hosts`, `body_slots`, `event_pool`, `e36_collect`, `wall_tree`, `list`/`links`/`sort`/`state` modules). `ports.csv`
maps each original function to its Rust item. Brake wall: `RaycastEntry` (0x4e4960) tests `(x, y)`; `out` = normal, `+0x10`
penetration `1 - perp/along`, `+0x14` perp, `+0x18` flags (bit 0 from index bit 15). The wall table is read eight floats
at an 8-byte stride (original; keep). `Impulse640` (0x529640): its 4th argument is a time (`dt`), the 3rd is
unused. Event solve `M_C1A0` (0x4fc1a0): `dot = PlaneDiff . n`; acts when `dot < -1` or flags 0x800 and 0x1000; `k = clamp(-0.05
dot)`, `mu` and `e` lerped by `k` from the material (`FB720` picks `+0x80` or `+0xac` by flag 0x800, `hi` when the
weight > 0.5); `a = -(1+e) dot / f88`; friction `min(|a| mu, |q|)`; `Emit4DC0` applies `+J` to A and `-J` to B.
`M_D760X` halves the ray direction, `M_DE40X` its argument `c`. `E36SphereBox` sums `dz + dy + dx` in x87. ABI:
`MinMaxA` writes min to `p2`, max of the upper vector to `p3`; `Scaled` takes out, then scalar; `BoxHit` sets only `AL`.

## Entities

`src/entities/`. Frames carry no transform (hooks return their input). `E90SegPool::Init` (0x601270) owns a `new[]` array with the
count word before it, built and torn down by the CRT vector iterators with `E90Seg::E90Seg`/`~E90Seg` (0x600ff0,
0x601030). `CEntity` (0x64): siblings `+0x10/+0x14/+0x18`,
exclusion set `+0x2c` (count, pointers), sort key `+0x30`, def `+0x34`, parent `+0x44`, children `+0x48` head, `+0x4c`
tail, `+0x50` count, flags `+0x60 = arg | 1`. Walks are pre-order over children whose `S80() != 0`.

## Scenery

Scenery collision comes from the `.trk` collision meshes (kind `0x16`, loader 0x46e300), not `.col`. Static meshes
are emitted at load as hard collision. `E46_Notify67d0` (0x5267d0) makes one `CSolid` per placement through
`E39_EntfCreate` (0x502070; index = `physics.lsp` `DefManager` order: 1 `GENERIC`, 13 `STREET_SIGN`, 21
`BANNER_FENCE`, 24 `PIG`, 25 `TRAFFIC_CONE`, 28 `LOG`, 30 `LOG_POLE`, 31/32 wooden fence bar/pole). `GENERIC`
(trees, rocks, poles, snowwall, bushes, spectators) has inverse mass 0 and never wakes; props sleep in list460
until a contact wakes them (`E38_004FCA20` 0x4fca20), then emit `CSigSolidGeneralState` from `CSolid::Vf28`
(0x52c620) with id = handle `0x9c40` + the `.lbs` (1,6) record index. A contact's surface is the mesh material
byte (e.g. `0x79` Bendable_Tree, tested by `E37Flag4AC0` 0x4f4ac0). A woken prop's velocity lane w is stale stack
(reaches inf/NaN, never x-z): `E43Vf6C` adds it with `math::sse`. Fences (`.fnc`) are cloth and never touch the car.

## Damage

`src/car_body/damage*.rs`. `Vf08x` (0x4ec140) scales the impact by 0.08; `Slot1` (0x4f12a0) ignores `dmg <= 500` when
flag 0x80000 is clear, else multiplies by `+0x3dec`. Zones at `+0x2214` (0x58 each), damage `min(v, 1)` at `+0x3808`.
`Slot4` (0x4f1000) level `+0x3df0`: 1 gives (0.5, 0.5, 0.2), 2 (0.8, 0.95, 0.6), 3 (1, 1, 1). Four table values at
`+0x2a88..+0x2b90` are 0.1f: `load_defs::damage_ops_tables_match_lcb1_load_dump`.

## Tweak keys

The `.lsp` keys per class: [data/physics-lsp-fields.md](../data/physics-lsp-fields.md); the exe's NTweak key tables
come from rbr-recomp's `scripts/tweak-keys.py`.

`CBase::Parse` (0x52f0c0) runs at load (the setup 0x48d940, the tyres 0x48d9f0, `E46_InitPhysics` 0x5266d0, the file
emits of `CMultiSignal1<char *>` 0x47e840), in the season (0x5d6310, 0x5d6450) and in the tweak menus (the bit-field
menu 0x530233). The load builds from the reader's items (`ntweak::plan`); the hybrid binding walks the token tables
(`ntweak::layout::parse`): each group, and each inner or member group again, is cut in place by `Tokenize` into a
0x4000-byte `operator new` table that is freed after it, its name and items go to the token heap (`E11_StackAlloc`,
which logs and traps when full). The class is the object's vtable (RTTI; `CLASSES` in `rbr-hybrid` `ntweak.rs`); the
season's two classes have no key table and decline. The five lp-snap0b emit dumps and the h1 setup dump are identical
with no declines (253 calls, 2026-10-20).

## Coverage gaps

The reference drive (tanner-corolla, damage 1) reaches 792 of 2,651 physics functions. Unreached: damage and crash
paths (wall penetration, retire, detach, `Impulse640`), VCU aids (off on that car; `harness/vcu_vectors.py --fuzz`),
load and teardown (`tests/load_*.rs`), snow and tarmac data, replay and ghost paths, NTweak menus, and about 170
functions with no callers. Branches no drive reaches get fuzzed direct-call records (`harness/vector_specs/`), else
the port is `partial` in `ports.csv`.
A binding no drive reaches is `identical` in `bound.csv` once its records take every conditional jump both ways
(`harness/branch_cover.py`), apart from the original's traps: a signal index at or past 0x20 (`CMultiSignal1::Emit`,
int3) and a drive-line index at or past the count (`CDriveLine::LerpAt`, int3; `Split17C`, `Vf98x` and `Test7910`
decline there). Callees a recipe must steer (the vote's miss, slot 36 clearing +0x274 in `Vf180`) return at once
without running in a third of the calls (`proofs_respawn.py`).
