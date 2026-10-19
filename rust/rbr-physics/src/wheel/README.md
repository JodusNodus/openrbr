# Wheel port: plan and names

Scope: the wheel functions the reference drive reaches (`notes/progress/called.csv`, subsystem `wheel` of
`scripts/progress.py`): `CWheel` (`RBR_E50_WheelSim.cpp`, `RBR_E50_Wheel.cpp`) and `E51Wheel` (`RBR_E51_Wheel*.cpp`),
exe 0x5390d0-0x53f230. The meanings are from `notes/physics/wheel.md` and `notes/physics/renames/wheel.csv`.
The strut is `spring_damper`, the tyre step is `tyre` (`CScratch::Step`, 0x5360c0); the car's transforms are
`car_body::transform`. The callees are answered by the caller, never copied here.

A function is done when its vector test passes on every recorded call of the original
(`harness/vcu_vectors.py --set wheel`, `harness/direct_calls.py`, `tests/wheel_vectors.rs`). `layout/` is the only
code with raw offsets (`CWheel` `this+N`, the tyre tail `tail+N`).

## Record store

`scripts/rust-vectors.sh wheel` runs `tests/wheel_vectors.rs` on `vectors/wheel/`:

- `d-s<N>.txt` (nine files): step records of each local `vcu-cov.snap-<N>.bin`, `harness/vcu_vectors.py --set wheel`.
- `c-direct-unk9290-300.txt`: 200 fuzzed direct calls of `Unk9290` (`harness/direct_calls.py --addr 0x539290`).
- `c-direct300.txt`: direct calls of `Copy100` and `Blend4` on the s300 snapshot's wheel (`--this bf1f520`, four
  fuzzed calls each, `harness/direct_calls.py --addr 0x5390d0 --addr 0x539180`).
- `c-0x53d030.txt`: 40 fuzzed direct calls of `M3D030` on `vcu-cov.snap-300.bin` (`--this bf1f520`, `--seed 7`), kept
  to its own calls. Its `M3B900`, `GetName`, `SetDirty` and `UpdateAndReset` (a tail call: its own record) are inner records.
- `c-0x539900.txt`: 40 direct calls of `CWheel::Unk9900` (`--this bf1f520`, `--seed 7`). Its inner records are
  `Tag520940`, `BlendTable` (with its own `SetDirty` calls, which the test applies with the rest), and `Blend`.
- `c-0x539eb0.txt`: 40 direct calls of `CWheel::E43Vf6C` (a tail call, so each `CSolid::E43Vf6C` record is a top-level
  call of its own, before the wheel's call).
- `c-0x539a90.txt`: 40 direct calls of `CWheel::E43Vf5C` (`--this bf1f520`, `--seed 7`); its inner records are the car's
  slot 28 (0x529340) and slot 29 (0x529ea0), each with a vector. `E09Vec4::Clear` (0x465d80) is not in the spec, so its
  zeroing is in the wheel's post window.
- `c-0x539660.txt`: 40 direct calls of `CWheel::Unk9660` (`--this bf1f520`, `--seed 7`, `vcu-cov.snap-300.bin`), with `n` random
  (4 as `Unk96e0` passes it, or others). No inner calls. Its window 0 is the 0x5b0 bytes that hold `this+0x5ac`.
- `c-0x5396e0.txt`: 40 direct calls of `CWheel::Unk96e0` (`--this bf1f520`, `--seed 7`). Its inner record is `CE36SubBlock::Init`
  (0x4f3e80, the local's three words); the file also holds 40 `Unk9660` records, which the direct calls make as well.
- `c-0x53b5b0.txt`: 40 direct calls of `E51Wheel::M3B5B0` (`--this bf1f520`, `--seed 7`; the inputs are the snapshot's, so the
  calls are identical). Its inner record is the base `E47_Vf16Helper` (0x52b6d0). The tail jump has no record: see the table.
- `c-0x53cf30-v4.txt`: 40 direct calls of `E51Wheel::CopyCF30` (`--this bf1f520`, `--seed 7`). The inner records are
  `CopyStateToDef` (0x52dbd0), `CopyDefFrom` (0x534760), `SetDirty` (0x52f450) and the CRT sine and cosine. The source table
  is zeroed (a random table fault in `CopyDefFrom`); the callees' windows reach the CDef through the strut's pointers.
- `c-0x5397a0.txt`: 40 direct calls of `CWheel::Unk97a0` (`--this bf1f520`, `--seed 7`; the gates are fuzzed, so 23 calls
  take no branch and 17 call `Unk94d0`). Its inner records are `Unk94d0` (0x5394d0): ecx is the wheel, the stack is the
  sub-object, `v` (window 2, the stack vector before the call), `a`, `b`, `c`. The sub-object's window runs to the wheel's end.
- `c-0x539090.txt`: 40 direct calls of `CWheel::E43Vf20` on `vcu-cov.snap-300.bin` (`--this bf1f520`, `--seed 7`); its
  inner records are `wheel_vf40` (0x53b5b0) and `solid_vf20` (0x529200).
- `c-0x53d230.txt`, `c-0x53d460.txt`: 40 fuzzed direct calls each of `M3D230` and `M3D460` on `snapH.snap-572.bin`
  (`--seed 7`); their inner records hold `InitRealLerp` (0x50f010) and the sines. Their recipes (`direct_fuzz`) put the tyre block in the replay's heap and fuzz the snapshot's CDef and
  material entries 0..3; the windows `nodes` record the global chain (`g_1659b70`, `+0x24`) and the manager.

The step does not reach `Copy100`; its records are direct calls only. Blend4 is reached four times per step.

## Ported

A port is a pure function of typed values (its module), and the sequence of the original function is an edge function in
`layout/`: it reads the inputs and stores the results between the host calls, in the asm's order, and the tests (and the
hybrid binding) call the edge. Host calls are traits with typed arguments and results; they take no `Mem`. The table gives
the name, the address and the C++ name.

| Address | C++ name | Edge (`layout/`) | Pure part |
| --- | --- | --- | --- |
| 0x5387e0 | CWheel::CDef::CDef | `layout::cdef_ctor` (the solid definition is `rigid_body::layout::solid_def_ctor`, the spring damper definition is `spring_damper::layout::construct_definition`, the tyre definition is `tyre::layout::construct_cdef`; the rest are `ntweak`) | |
| 0x538b40 | CDef::Unk4C (vtable slot) | `layout::cdef_unk4c` | |
| 0x538d70 | CDef::Unk20 (vtable slot) | `layout::cdef_unk20` | |
| 0x538f50 | CScratch::Vf04 | `layout::scratch_vf04` | |
| 0x539090 | CWheel::E43Vf20 (vtable slot 8) | `clear_and_forward` (`Vf40` is the `SlotCalls` host method `wheel_vf40`, 0x53b5b0; the forward is `solid_vf20`, 0x529200) | |
| 0x539660 | CWheel::Unk9660 | `layout::uint_select` (`select::pick`: the position of `this+0x5ac` in a cycle of `n`) | `select::pick` |
| 0x53a860 | CWheel::UnkA860 (tyre tail setup) | `layout::tyre_tail_setup` (`SetUnk10` is `spring_damper::layout::set_rebound_damping`, `Set414` is `tyre::layout::store_414`) | |
| 0x538dd0 | CWheel::CState::CState | `layout::state_ctor` (inline CBase, CVector4, CReal, CVector and CUnsigned members via `ntweak`; the owner is kept when flags bit 2 is set) | |
| 0x53b050 | E51WheelDamage::Zero | `layout::damage_zero` | |
| 0x53b550 | E51Setup::Vf20 (vtable slot) | `layout::setup_vf20` | |
| 0x53b570 | E51Setup::Vf28 (vtable slot) | `layout::setup_vf28` | |
| 0x53b5b0 | E51Wheel::M3B5B0 (slot 16, `wheel_vf40`) | `layout::embedded_dispatch` (`CSolid::Vf40` on the wheel is the `solid_vf40` host call, bound as `rigid_body::layout::vf16_helper`; the tail is `embedded_slot17`) | |
| 0x53b610 | CWheel::M3B610 (new definition) | `layout::new_cdef` (the block is the `Heap` host's `alloc` of 0x980 bytes, the `operator new` call; the definition is `cdef_ctor`) | |
| 0x53b670 | CWheel::CScratch::CScratch | `layout::scratch_ctor` | |
| 0x53b6e0 | E51Wheel::Ctor (CWheel::CWheel) | `layout::wheel_ctor` (partial: the owner's body event, `E43Vf90` 0x52bf30, is the `emit_body_event` port with `EmitHost`, not checked by records: with an owner the direct run runs the wheel's own slot 18 through its vtable, which the records do not list; the owner is kept only with flag bit 0x8). The solid base (0x52d480) is the `SolidCtorHost` host call; the name is `ntweak::name_or_null`; the spring damper's third stack word (the wheel or 0) is not read by `construct_controller` and is not in any window | 40 direct calls, owner zero (`wheel_ctor_fuzz`) |
| 0x53b880 | E51Wheel::M3B880 (refresh travel) | `layout::refresh_travel` (partial: the overlay `Unk9bb0` 0x539bb0 is the `TravelCalls::debug_overlay` host call, not ported; it runs only under bit 2 of the car flags, which the records keep clear. `ClearBias` and the strut's `ComputeTravel` and `TravelRange` have no effect on the wheel, see the function's doc) | |
| 0x53b900 | E51Wheel::M3B900 | `layout::refresh` (the binding runs `state_lerp` and `turn_axis` first, as the asm does; the host calls are `RefreshCalls`: `SetDirty` 0x52f450, `TyreTail_Get10` 0x5326b0, and `Vf40` on the wheel) | `refresh::strut_frame`, `refresh::lift_frame`, `refresh::frame`, `refresh::after_tyre` |
| 0x5396e0 | CWheel::Unk96e0 | `layout::table_lerp_pair` (`CE36SubBlock::Init` is the `SubBlockInit` host call; `uint_select` into the local; the lerps and their outputs) | `select::lerp_column`, `select::lerp_output` |
| 0x5397a0 | CWheel::Unk97a0 | `layout::update_sub_objects` (the gates, `Cross` as `Vec4::cross3` on `+0xa70` and `+0xa80`, then `Unk94d0` through `sub::SubCalls::sub_unk94d0`; the asm passes the wheel as `this`, which the C++ does not) | |
| 0x53cf30 | E51Wheel::CopyCF30 | `layout::copy_cf30` (the strut, scratch and base calls are `layout::DefCopyCalls`; sin and cos are `CrtTrig`) | |
| 0x539900 | CWheel::Unk9900 | `layout::blend_tags` (`BlendCalls`: `blend_coefficients` is `CScratch::BlendTable` 0x534e70, `blend_materials` is `E50BlendOps::Blend` 0x5381f0; the tags' lookup is `material::tag_entry`) | |
| 0x539a20 | CWheel::Hook (E43Vf58, vtable slot 22) | `slots::hook` (the wheel's slot 96, `wheel_vf180`), bound in rbr-hybrid inside its profiler scope (`physics_core::scope`: `E46ProfChan::Enter` on the frame's channel, `ProfExit`); no record, since the callee is the step | bound (`BINDINGS`); identical on the nine vcu-cov snapshots, 30 steps, all bindings |
| 0x539a90 | CWheel::E43Vf5C (vtable slot 23) | `layout::forward_to_car_and_clear` (the car's slots 28 and 29 are `CarSlotCalls`: `car_vf70` 0x529340, `car_vf74` 0x529ea0) | |
| 0x539eb0 | CWheel::E43Vf6C (vtable slot 27) | `slots::forward_vf6c` (a tail call: the solid's record is its own; `solid_vf6c` is `CSolid::E43Vf6C`, 0x52cc30) | |
| 0x5390d0 | CWheel::Copy100 | `transform_and_copy` (the car's `Xform6870` is the `xform_6870` argument) | `copy::scaled_sum` |
| 0x539180 | CWheel::Blend4 | `blend_pair` | `blend::blend` |
| 0x539290 | CWheel::Unk9290 | `pick_contact` | `frame::picks` |
| 0x53a8a0 | E51Wheel::M3A8A0 | `car_frames` (the car's `BodyToWorldPair` and `BodyCompose` are the `CarXform` trait) | `xform::transform_call` |
| 0x53a9e0 | E51Wheel::M3A9E0 | `axes::turn_axis` (sin and cos are `CrtTrig`; the normalise is `math::vec::normalize`) | |
| 0x53ab00 | E51Wheel::M3AB00 | `axes::axis_from_angle` | |
| 0x53ab90 | E51Wheel::M3AB90 | `axes::state_lerp` | |
| 0x53ac50 | E51Wheel::M3AC50 | `axes::set_input` (the input setter; calls `axis_from_angle`) | |
| 0x53aef0 | E51Wheel::M3AEF0 | `walk_result` (`MB560` is the `WalkCalls` trait; `GetB1C` is `car_body::layout::rate_b1c`) | `walk::walk_gain`, `walk::offset_walk` |
| 0x53afb0 | E51Wheel::M3AFB0 | `spin_cross` (runs `walk_result`; `CFloat3::Scale` is `Vec4::scale`; `E01VecSetA::Cross` is `Vec3::cross3`) | `walk::spin_product` |
| 0x53d230 | E51Wheel::M3D230 | `tag_blend` (`InitRealLerp` is the `RealLerp` host method; the tags' lookup is `material::tag_entry`) | `block::phase_sines`, `block::first_term`, `block::second_term` |
| 0x53d460 | E51Wheel::M3D460 | `pose_flag` (runs `tag_blend`; the CDef's flags and pair tag are read before it) | `block::flag_outcome` |
| 0x53d030 | E51Wheel::M3D030 | `update_strut` (the strut's `GetName`, `SetDirty`, `M3B900` and `UpdateAndReset` are the `StrutCalls` host) | `update::strut_blend` |
| 0x53d520 | E51Wheel::M3D520 | `ground_probe` (three rays through the `GroundCalls` host, the blend, and the copies to +0xdd0, +0xe10 and +0xdf0; the `PROBE_*` offsets are in `layout/probe.rs`) | |
| 0x53d1c0 | E51Wheel::CDef::Vf50 (the wheel factory) | `layout::new_wheel`: the block, then `wheel_ctor` on it with the car as its owner. The car instance's test (`load_defs::car_instance_matches_the_game`) runs `wheel_ctor`'s owner branch (the body event, with `load::layout`'s host: `construct_solid`, `window_start`, the idle replay manager as the only listener): every byte of the four wheels equal at the factory's return. The window start (0x330, `CSolid::S18` through `emit::window_start`) is a copy of the position (0x140), which no constructor writes: the block's old bytes | load test |
| 0x53dd30 | E51Wheel::M3DD30 | `layout::wheel_step` (the slot-96 step: travel, the car walks, the ground probe, the contact and the spring) with `StepCalls` as the host, one method per original callee, each called at the callee's stack depth | bound (`BINDINGS`); identical on the nine vcu-cov snapshots when bound for the check |
| 0x53f230 | CWheel::Vf28 (E43Vf28, the finish) | `layout::finish` (`FinishCalls`: `ResetOutputs`, `TyreTail_Get10`, `Step77` (0x529d30), the signal emit on the signal manager's 0x29e0 signal (0x53d0d0, the tire emit's code), `EmitNoise`, `M3B880`; the car transform, sin, cos, quaternion product and two `Vec4::Clear`s write frame locals only and are not run) | |

`M3B5B0`'s tail jump (slot 17 of the embedded object at `this+0x3d0`; the vtable at 0x7427b4 gives 0x529a50, `E47SolidState::Vf17x`) leaves no record: the recorder finds calls by return address. Its writes are checked by rust_step only (the binding is bound on that check). The record checks the base call and the edge's order. The binding runs the bound rigid-body ports directly
(`rigid_body::vf16`, then `rigid_body::vf17x` for the tail) and declines unless the tail is `Vf17x`, checked before anything runs.

`Hook` (0x539a20) is bound in rbr-hybrid with its profiler scope: `physics_core::scope` runs `E46ProfChan::Enter` (0x528880)
on the frame's channel (the sample 0x165f268, the name "CWheel::Hook"), then the slot 96 call, then `ProfExit` (0x528690).
Without the scope the binding left exe globals (0x8ef668, 0x165f268...) unwritten, and `rust_step` read them.

`CopyCF30` (0x53cf30) is bound with status `unreached`. Its only drive caller is `CCar::UpdateA720` (0x4ea720, the seed
applier, which calls it for each of the four lanes). Ghidra shows no static caller of 0x4ea720, so the path is indirect.
No snapshot reaches it: single steps from steps 0, 40, 300, 1000, 1600, 2000, 2400, 2800, 3200, 10000 and 11000 show no
call, and 200-step chains from 300 to 11000 (eight snapshots) show none either. The trigger (a restart or a stage start
not in these windows) is not found yet. The record `c-0x53cf30-v4.txt` checks the port itself (40/40).

The record order of `M3D030` (`c-0x53d030.txt`) is: the car calls (with their walks), the sin and cos, `GetName` twice,
`SetDirty`, `M3B900`, `UpdateAndReset`. The C++ reconstruction lists the names first; the records are the original's order,
and the port follows them. `M3B900` reads the wheel memory (its window reaches `this+0xec4`), so the sum is stored before
that call.

`M3B900` (0x53b900) is checked by `refresh_matches_original_calls` (`c-0x53b900.txt`, 40 of 40 calls, and the M3A9E0 and M3AB90 calls it makes). The original copies four lanes of uninitialised stack into the wheel: lane 3 at wheel+0x99c, +0x9bc and +0x9dc, and the tail words at +0xe5c and +0xe6c (lane 3 of the `CopyFrom`ed vectors at `E+0x1f0` and `E+0x130`). The port writes 0 there, and nothing reads them. `harness/rust_step.py` leaves them out of the comparison (`STACK_GARBAGE`, `WHEEL_TAIL_GARBAGE`); this is the documented lane-3 class, not a new rule.
`M3D520` (0x53d520) has its scratch in the body's frame `S` (`((esp-4)&~0xF) - 0x36B0`): the E46 output that `E45_M46A0` reads is at offset 0xe0 from `S`, the eight ray objects at offset 0x110. It has no vector test yet: E39 (0x4ff170), E46 (0x525bd0), the sweep (0x4fad90), `TyreTail_Get14`, `M_D760X` and `E45` have no specs in `harness/vector_specs/wheel.py`, so `direct_calls` cannot record them. It is checked by `rust_step` only: identical after one step on the nine vcu-cov snapshots, and in 30-step chains from snap-300, -1000 and -2000 against `drive-tanner-corolla`. The cross at +0xa90 writes three lanes; lane 3 (+0xa9c) keeps the snapshot's value, which `0x473cd0` reads through +0xa98. Writing 0 there changed the step, so this is not the lane-3 class above: the port keeps the old value.

`M3DD30` (0x53dd30) is bound. Checked with the binding registered for the run: one step is identical on all nine vcu-cov snapshots, and 30-step chains from snap-300, -1000 and -2000 against drive-tanner-corolla are clean. Every host call takes the callee's entry stack pointer from the asm's pushes (`sp` is `frame + depth - 4`). With one fixed `sp` for all host calls, the callees' uninitialised locals held other stale stack, and 8 to 24 heap bytes differed on seven snapshots. `Clear`, `CopyFrom`, `Cross` and `Scale` are host calls too: they leave the return address the original leaves. Inline versions at the exact depths also matched the nine snapshots, so this is for the stack writes, not a measured difference. The two x87 multiplies go through `fpu::round24`, as in `select.rs`. `Unk97a0`'s lane 3 is not reproduced (see `update_sub_objects` in `layout/wheel_calls.rs`): a sentinel in that lane changes no compared byte on the nine snapshots, which is evidence, not proof.

`Vf28` (0x53f230) is bound (`layout::finish`, `FinishCalls`). Its asm (frame at `esp` after `and esp, -16; sub esp, 0x94` and three pushes, 0xa0 below the aligned `ebp`) calls `CSolid::Step77` directly on the wheel (`ecx`), not through a vtable. The payload is at frame offset 0x40: the original writes only word 0 (the wheel's key, 0x30), and no call in `Vf28` writes words 1..3, so they hold the entry residue in both runs. The car transform, sine, cosine, quaternion product and `Vec4::Clear`s write frame locals nothing reads and are not run. Checked by `rust_step --each` on the three `RBRRecompSnapAt-0053f230` entry dumps (identical); the 30-step chains from the nine `vcu-cov` snapshots never reach its entry.

Read order: the edges read and store in the asm's order, so a read after a host call sees what the asm sees. `M3D230`
loads each material entry at the point the asm reads it (the lerp values after the first `InitRealLerp` call, the `+0xb8`
values after it), and `M3D460` reads its target after `M3D230` returns.

Bound: `M3D520`, `M3DD30` (the wheel step), `Vf28` and `E51MultiTire::Emit` (0x53d0d0, see the Emit note under Platform
notes), identical in `bound.csv`; `M3D1C0` (the factory) is bound but no snapshot reaches it. `M3D230` and `M3D460` are
bound with their post-fix check pending (`bound.csv`). `M3B880` is ported in part (its overlay is not ported) and is not
bound: the emulator runs it.

## Calls

Callees are answered by the caller through a trait (one method per original callee, its address in the doc comment)
or by the ported function: `CarXform` (`CCar::BodyToWorldPair` 0x4e69c0, `CCar::BodyCompose` 0x4e6a30) and
`WalkCalls` (`E43PairWalk::MB560` 0x51b560). The x87 sin and cos (0x53f3a0, 0x53f3b0) are `math::trig`, the
`rsqrtss` is `math::rsqrt`. `E01Vec4::CopyFrom` (0x43c5b0, a 16-byte copy) is written inline as a vector copy.

## Names

| Name | Original | Meaning |
| --- | --- | --- |
| `TYRE_TAIL` (`CAR`) | `this+0xf10`, `this+0x7f4` | the tyre tail pointer read by `TyreTail_Get10` (`*(container+0x10)`), and `m_car` |
| `load_copy_scale` | `tail+0x6c` | the scale `Copy100` multiplies the scaled input by |
| `load_blend_scale` | `tail+0xcc` | the scale `Blend4` halves (`* 0.5`) |
| `COPY_SCALED`, `COPY_OFFSET`, `COPY_BLOCK` | `this+0x9e0`, `this+0x140`, `this+0x100` | `Copy100` operands (see unknown meaning) |
| `BLEND_*_COPY_*` | `this+0xa20`, `+0xa40`, `+0xde0`, `+0xe00` | copies of `Blend4`'s `d` and `c` |
| `SCALED_VEC`, `COPY_OFFSET`, `COPY_BLOCK` | `this+0x9e0`, `+0x140`, `+0x100` | the `Copy100` operands; `SCALED_VEC` also feeds the spin |
| `FRAME_AXIS`, `CONTACT_A70`, `CONTACT_A80` | `this+0xa90`, `+0xa70`, `+0xa80` | `Unk9290`'s axis and contact vectors (working names) |
| `AXIS_OUT`, `AXIS_BASE`, `AXIS_TURN`, `STATE_LERP`, `STATE_ANGLE` | `this+0x5f0`, `+0x600`, `+0x610`, `+0x570`, `+0x5ac` | the axis vectors and the state lerp of `M3A9E0`, `M3AB00` and `M3AB90` |
| `WALK_OFFSET`, `WALK_OUT`, `WALK_RATIO_NUM`, `WALK_RATIO_DEN`, `SPIN_RATE` | `this+0x9a0`, `+0xc10`, `+0x9c0`, `+0x9c4`, `+0xa50` | the walk offset and output, the gain's ratio, and the spin rate (`M3AEF0`, `M3AFB0`) |
| `INPUT_CHANGED` | `this+0xec0` | the flag `M3AC50` sets (working name `input_changed`) |
| `BLOCK_INPUT`, `BLOCK_PHASE`, `BLOCK_WEIGHT`, `BLOCK_TAG_1`, `BLOCK_TAG_2` | `block+0x10`, `+0x14`, `+0x34`, `+0x38`, `+0x3c` | the tyre block's phases, weight and material tags (`M3D230`) |
| `BLOCK_TARGET`, `BLOCK_BLEND`, `BLOCK_TOTAL`, `BLOCK_DIFF`, `BLOCK_CLAMP_A`, `BLOCK_CLAMP_B`, `BLOCK_B`, `BLOCK_ACC` | `block+0x40`, `+0x54`, `+0x58`, `+0x64`, `+0x5c`, `+0x60`, `+0x4`, `+0x0` | the target, blend, total, difference and clamps of `M3D460`; `BLOCK_B` and `BLOCK_ACC` are working names |

## CDef and the tyre block

`layout/` names the CDef (`this+0x34`) offsets the functions use, with their asm or C++ line. Meanings are the
C++ comments' or unknown:

- `DEF_LERP_A` (`+0x8e0`), `DEF_LERP_B` (`+0x8a0`), `DEF_LERP_C` (`+0x8f0`) and their scales `DEF_LERP_A_SCALE`
  (`+0x920`), `DEF_LERP_B_SCALE` (`+0x910`): the `M3AB90` lerp into `STATE_LERP` (`this+0x570`).
- `DEF_ANGLE_SCALE` (`+0x950`) and `DEF_ANGLE_OFFSET` (`+0x900`): the axis angle `w * 950 + 900` of `M3AB00`.
- `DEF_INPUT_MIN` (`+0x960`) and `DEF_INPUT_MAX` (`+0x970`): the setter's range in `M3AC50`.
- The CDef vectors `+0x880..+0x8e0` that `M3A8A0` writes (`PAIRS` in `xform.rs`): the C++ comment calls them axes.
  Working names only.

`M3D230` and `M3D460` take the tyre block itself (`this+0x7f0`) as their argument. Their names (working names, meanings
not in the notes): `BLOCK_WEIGHT` (`+0x34`, the blend weight of the two tags), `BLOCK_TAG_1`/`BLOCK_TAG_2` (`+0x38`,
`+0x3c`, the material tags), `BLOCK_TARGET` (`+0x40`), `BLOCK_BLEND` (`+0x54`), `BLOCK_TOTAL` (`+0x58`), `BLOCK_DIFF`
(`+0x64`) and the clamp words `BLOCK_CLAMP_A`/`BLOCK_CLAMP_B` (`+0x5c`, `+0x60`). The CDef's `WORLD_DEF` (`+0x24` of the
CPhysics that `g_1659b70` points at), `DEF_MATERIALS` (`+0x18`), `DEF_FLAGS` (`+0x160cc`, bit `FLAG_PAIR_TAG`) and
`DEF_PAIR_TAG` (`+0x1614c`) are read through the global, not through an argument.

The tyre block (`this+0x7f0`, a float block; `RBR_E51_WheelA.cpp:74`) has `BLOCK_ACC` (`+0`, the `M3AB90` factor, and the
`M3ACD0` accumulator that clamps to 1), `BLOCK_B` (`+4`) and `BLOCK_INPUT` (`+0x10`, the `M3A9E0` and `M3AB00` input, set
by `M3AC50` into [-1, 1]). The meanings are unknown: working names.

The car's `+0x44` (`XFORM_CAR`), `+0x60` (`CAR_FLAGS`, bit 3 `CAR_FLAG_XFORM`) and `+0x7f4` (`CAR`) are the car and its
flags. `+0x9c0`, `+0x9c4` (`WALK_RATIO_NUM`, `WALK_RATIO_DEN`), `+0x9a0` (`WALK_OFFSET`), `+0xc10` (`WALK_OUT`) and
`+0x9e0` (`SCALED_VEC`) are the walk and spin inputs of `M3AEF0` and `M3AFB0`.

`M3D030` (the strut update, `this+0xec4`): `STRUT` (`this+0xec4`, a `CSpringDamper`; its name is `CBASE_NAME`, `m_name` at
+4), `STRUT_TWEAK` (`this+0x5b0`, the `CBase` whose dirty flag is set), `STRUT_LENGTH` (`this+0x5bc`, the blend `0.67 * name(+0x38)
+ 0.33 * name(+0x28)` that `M3B900` takes). `NAME_F28`, `NAME_F38` are the name object's floats (working names).

## Unknown meaning

- `this+0x9e0`, `this+0x140`, `this+0x100` and the four blend copies: the wheel.md notes give `+0x140` as the car
  `Xform6950` target and `+0x100` as the `Copy100` block, but not their meaning. Working names only.
- `tail+0x6c` and `tail+0xcc`: the tyre tail's scales. The tail type is not in `src/tyre` yet.
- `Unk9290`: `frame.lane_a`/`frame.lane_b` (`+0x50`, `+0x64`) and the record's vector at `+0x20` (`*a != 0` gates it).
  The frame's meaning is not in the notes; the `FRAME_AXIS`, `CONTACT_A70`, `CONTACT_A80` names are working names too.
- `TyreTail_Get10` (0x5326b0) is read as a plain load (`*(container+0x10)`, `tyre::layout::load_tyre_tail10`).
- The tyre block's `+0`, `+4`, `+0x10` and the `+0x600`/`+0x610` axis pair, `+0x5f0` output, `+0x570` lerp, `+0x9a0`,
  `+0x9c0`, `+0x9c4`, `+0xc10` and `+0xec0` flag: named by use only (see above), meanings not in the notes.

## Platform notes

- No float-to-integer conversion and no raw-bit test in these two ports.
- `Copy100` tests `this+0xf0` as an address (the C++ `(char*)this + 0xf0 ? ... : 0`); the test is always true, so the
  block at `+0x100` is always copied. The port takes that branch without a test.
- `Unk9290` compares as the asm does: `!(d > 0)` negates, `e1 > e2` picks `d1`, and `f1 > 0` takes the root. NaN takes
  the asm's branch (the C++ `<=` differs).
- `Blend4` reads `a` and `b` again for `d` after storing `c`, as the asm does: `c` may alias them.
- `M3A8A0` passes a source as null when `def + guard == 0` (the asm's `lea`/`test`); the guards are not zero in
  the records; the port keeps the branch.
- `M3A9E0`'s clamp of `c = 2w` passes NaN through (the asm's `comiss` pairs); the C++ gives -1 (`cpp-fixes-from-rust.txt`).
- `M3A9E0`'s normalise is `rsqrtss` (`math::rsqrt`, the reference CPU's table), not `1/sqrt`: the same as `collision`.
- `M3AEF0`'s gain is computed in f32 steps (SSE and x87 at 24-bit, each product rounded), not in f64 as the C++ writes it.
- `E51MultiTire::Emit` (0x53d0d0) is `CMultiSignal1<CSigTireParticles>::Emit`: the asm matches the physics core's signal layout, and its
  out-of-range path goes on to the overflow list (slot 32) if the log returned; the log calls leave their arguments on the
  stack, so the port stops after each one. The edge is `layout::tire_emit`; the binding runs the physics core's emit
  (`physics_core::signal::emit`) and is bound: `rust_step --only 53D0D0 --steps 30` from the nine tanner-corolla snapshots
  is equal apart from the excluded bytes (`bound.csv`). The first, unbound check found 8 heap bytes different on six of the
  nine snapshots (at `0x7d33c34` and `0x7d33d04`, cleared by the listener at 0x5e3c70 in the port's run only; the listeners
  read stack words the harness counts as stack-dependent).
- `M3D030` ends with a tail jump to `UpdateAndReset`: its record ends there, and the strut's update is a record of its own. The
  tests' wheel windows for `M3D030` therefore stop below the strut (`+0xec4`); `M3B900`'s window is the same (its writes stop there).
- `M3D230` calls `E41Plain::InitRealLerp` (0x50f010) through the `RealLerp` host method: `(b - a) * t + a` in f32, which the
  caller reads at the buffer's +0xc. Its `NTweak::CBase` constructor (0x52f090) writes only the buffer's header and is
  inside the host's model; the tests answer from the callee's recorded buffer. The sine products follow the asm: `sin(+0x14 * r)` is rounded to f32 when stored (`fstps`),
  `sin(+0x10 * r)` stays unrounded in the sum, and the products are formed in f64 and rounded once.
  Bound in rbr-hybrid (status `bound`, post-fix check pending): before the fix, `rust_step --only 0x53d230` on snap-2000 found
  one bit of the block's total (offset 0x58) different within 30 steps (87 bytes, the same with `M3D460`, which runs this
  blend). The fix rounds to PC=24 after each x87 operation (`bound.csv`).
- The C++ `M3AFB0` marks the `Cross` operand order a guess; the asm is `buf x s30`, written to `p` (the C++ order).
