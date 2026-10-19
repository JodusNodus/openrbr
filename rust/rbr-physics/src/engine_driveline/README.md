# Engine and driveline port: plan

Scope: the functions the reference drive (`tanner-corolla`, `notes/progress/called.csv`) reaches in
`src/rbr` `RBR_E39_*` (drive, minus the damage, collision and manager files), `RBR_E40_*` (drive workers
and drive line), `RBR_E41_*` (engine, engine drive line) and `RBR_E43_Engine*` (engine sim). Source and
semantics: `notes/physics/engine-driveline.md`, `notes/physics/renames/engine-driveline.csv`,
`notes/physics/tweak-keys.md`. The asm decides the operation order and NaN branches; the C++ in `src/rbr`
is a guide.

Nothing stays C++ by default. Constructors and `Init*` loaders set the physics state and defaults, and the
`Vf` slots that compute something are in scope too. Pure dispatch slots and signal `Emit`s go through a
caller-implemented trait in the original's call order (the VCU `Send` and the entities `Walk` pattern), and the test
checks the call sequence. Those constructors and loaders come last (group 9): they are verified with load-time
dumps (`RBR_SNAPAT` and `replay.py --after`) or traces.

## Plan (leaf -> step)

Calls are from the C++ bodies (a rough scan), not the asm; each group's order is the call order.

| Group | Functions (address, C++ name) | Calls | Vectors |
| --- | --- | --- | --- |
| 1. Drive accessors | `Set30` 0x504450, `Set44A0` 0x5044a0, `Get4520` 0x504520, `Get4530` 0x504530, `Get4540` 0x504540, `SetTwo` 0x5043c0, `Set4570` 0x504570, `Set47B0` 0x5047b0, `Set4820` 0x504820, `Test4840` 0x504840 | Test4840 -> SimE980 | all ten ported (`SetTwo` and `Set4570` in batch 14, `Test4840` in batch 15) |
| 2. Engine kernel leaves | `E41_TableLerp` 0x50d580, `E41_CurveEta` 0x50d7a0, `E41_UnkD6F0` 0x50d6f0, `E41_BlendX` 0x50ced0, `E41_BlendY` 0x50d090, `E41_BlendW` 0x50d220, `Sim410` 0x50d410, `SimD870` 0x50d870, `SimE370` 0x50e370, `SimE470` 0x50e470, `SimE520` 0x50e520, `SimE660` 0x50e660, `SimE9F0` 0x50e9f0, `SimEBB0` 0x50ebb0, `SimEC40` 0x50ec40, `SimECF0` 0x50ecf0, `SimED70` 0x50ed70, `SetScratch08` 0x50eb90 | none (leaves) | ported (`SimEBB0` in batch 9) |
| 3. Engine composites | `SimE060` 0x50e060 (BlendX, BlendW), `SimE1F0` 0x50e1f0 (BlendY, BlendW) | group 2 | ported (batch 3), bound |
| 4. Drive line | `BlendB` 0x50a650, `Last` 0x50ad40, `Thunk4F0` 0x50a4f0, `SetDriveLine` 0x50aec0, `ClampMix` 0x50b0f0 (BlendB), `SampleBig` 0x50b300 (BlendB, ClampMix) | group 4 | six ported (`SampleBig` in batch 8) |
| 5. Drive leaves (E40) | `M5970` 0x505970, `M5A10` 0x505a10, `M5BE0` 0x505be0, `M5D50` 0x505d50, `M5FE0` 0x505fe0, `M6260` 0x506260, `M6630` 0x506630, `M6A00` 0x506a00, `M6B20` 0x506b20, `M6C90` 0x506c90 (ten, batch 5); `M8480` 0x508480 (blocked: it calls `M79B0` (group 6) and `CMultiSignal1::Emit` 0x506e60 (group 10)) and `M8770` 0x508770 (load path) | none or Emit | ten ported, and `M8770`; `M8480` blocked |
| 6. Drive workers (E40) | `M79B0` 0x5079b0, `M7C10` 0x507c10, `M7D60` 0x507d60, `M8570` 0x508570, `M9120` 0x509120, `M9210` 0x509210, `M93A0` 0x5093a0, `M9BC0` 0x509bc0, `Step1E0` 0x50a1e0 (the drive's step) | groups 1, 5, 6 | eight ported on the typed worker (`M79B0`, `M7C10`, `M7D60`, `M8570`, `M9120`, `M9210`, `M93A0`, `M9BC0`); `M7C10` takes `Step4990` (group 7), `M9BC0` takes `Step4990`, `E39_MatVec` and `E39_Step3870` through host traits; `Step1E0` ported (`layout::step1e0`), bound |
| 7. E39 helpers | `RatioCA0` 0x503ca0 (ported, `ring_average`), `TabE00` 0x503e00 (`unsigned_table_lerp`), `WorkerF60` 0x503f60 (`count_down`), `Worker4060` 0x504060 (`row_value`) and `Hook4160` 0x504160 (`wheel_curve_term`) (ported); `Step4990` 0x504990 (`mat4::inverse`) with its copy `RowsCopy` 0x502570 (`mat4::transpose`) (ported, batch 17); `Hook5140`, `Hook51D0`, `Hook5260` | group 2 | the first five and the matrix pair ported; the three hooks not yet |
| 8. Engine sim (E43) | `Sim7D30` 0x517d30, `Sim80D0` 0x5180d0, `Sim8610` 0x518610, `Sim8890` 0x518890, `Sim8B20` 0x518b20, `Sim8CF0` 0x518cf0, `Sim8D90` 0x518d90, `Sim8E20` 0x518e20 | groups 2, 3 | `Sim8D90` ported (batch 8); `Sim80D0` and `Sim7D30` ported (batch 10; identical on rust_step); `Sim8610` and `Sim8B20` ported (batch 11; identical on 30-step rust_step); `Sim8890` ported (batch 12; identical on 30-step rust_step; the walk's `S30` and `MB560` are host calls); `Sim8E20` ported (batch 12; identical on 30-step rust_step; `Emit`, `CGraph1D::Push` and `E01Log` are host calls); `Sim8CF0` ported (batch 16; not reached in the 30-step runs; its emit `0x50ee90` is a host call) |
| 9. Constructors, loaders, Vf slots | `E39_CDefCtorA` 0x502780, `E39_CDefDtorA` 0x502800, `E39_Ctor870` 0x502870, `E39_CDefInit940` 0x502940, `E39_Ctor9C0` 0x5029c0, `E39_StateInitC70` 0x502c70, `E39_CDefCtor160` 0x503ad0, `CDrive::CState::CState` 0x505480, `CDrive::CDef::Init58D0` 0x5058d0, `CDrive::CDef::CDef` 0x506fe0, `E41` `CEngine::CState` 0x50bed0, `CEngine::CState::ResetSub78` 0x50c370, `CEngine::CDef` 0x50f080 (2846 lines), `CEngine::CEngine` 0x50df30, `CDriveLine::CDriveLine` 0x50b8a0, `CreateDriveLine` 0x50b970, the `DtorBody` and `Unk20` slots (0x50bbd0, 0x50bbf0, 0x50bc70, 0x50bce0, 0x50bd90, 0x50dc00, 0x50dc80), `E41_ConnVf04` 0x50b840 | groups 1 to 8 (the state they set) | the load path's part ported (load step 4, see batch 5): `CDrive::CDef::CDef`, `Init58D0`, `E39_CDefCtor160`, `CDrive::CState::CState`, `CEngine::CDef`, `CEngine::CEngine` and `CEngine::CState`; the rest not yet |
| 10. Dispatch and emits (trait) | `CMultiSignal1::Emit` 0x506e60, 0x506f20; `E41ExhaustSignal::Emit` 0x50ee90; `E41EngineNoiseSignal::Emit` 0x50ef50; `E41BlockF0` 0x50db40; `E41_Setup`/`E41_Def` slots without a computation | the callers | call-sequence test |

The table covers 74 of the reached functions, and none of the rest is out of the port: the pure step needs all of
them, in this order. (1) The computations: the rows above and the "reached but not in a group yet" list below. (2) The
vtable slots and the wiring the step dispatches through: `Vf04`/`Vf20`, `Unk20`, the `DtorBody` slots, `E41_ConnVf04`,
`CreateDriveLine` and the drive-state ctor/`ResetSub78`. The emits (`E40_Signal`, `E41_Signal`, `CMultiSignal1::Emit`,
`E41EngineNoiseSignal::Emit`, `E41ExhaustSignal::Emit`) stay host sinks. (3) The constructors, destructors and `Init*`
loaders (`E39_CDef*`, `E39_Ctor*`, `E41_Setup`/`E41_Def`), which come with the load path later.

Reached but not in a group yet: `LerpAt` 0x50a920, `SampleBig2` 0x50b580. `M6DB0` and `M6DD0` are ported in batch 14
(the table above). The eight CEngine accessors of bundle 13 (`Set48`, `SetState4`, `Get48_58`, `SimE970`, `SimE980`,
`SimE9A0`, `GetState188`, `SimEE60`) are ported too. Their status in `gameplay-functions.csv` is untouched or reviewed,
so each is checked only through its callers until it gets a group.

## Status

Vectors: nine snapshots of the reference drive (`vcu-cov.snap-300`, `1000`, `1600`, `2000`, `2400`, `2800`, `3200`,
`10000`, `11000`). Per snapshot, the step's own calls (`harness/vcu_vectors.py SNAP OUT --set engine-driveline`) and 40
direct calls per function with a `direct_fuzz` recipe (`harness/direct_calls.py SNAP OUT --set engine-driveline
--count 40 --seed 3`). The records are not committed; the test reads them through `RBR_ENGINE_DRIVELINE_VECTORS`
(a `:`-separated list of files). All 43 tests pass on the nine snapshots (7,114 matched calls for batches 1 and 2, 0 mismatches; batches 3 to 6 below).

Batch 1 (engine kernel leaves and the drive accessors):

| Function | Address | Port | Matched calls (all 9 snapshots) |
| --- | --- | --- | --- |
| `E41_TableLerp` | 0x50d580 | `sim::table_lerp` | 374 |
| `E41_UnkD6F0` | 0x50d6f0 | `sim::unk_d6f0`, `UnitSplit::store` | 370 |
| `E41_CurveEta` | 0x50d7a0 | `sim::curve_eta` | 383 |
| `E41_BlendW` | 0x50d220 | `sim::blend_w`, `layout::store_bracket` | 378 |
| `CEngine::Sim410` | 0x50d410 | `sim::abs_rpm_term` | 360 |
| `CDrive::Set30` | 0x504450 | `drive::set_brake_in` | 9 |
| `CDrive::Set44A0` | 0x5044a0 | `drive::set_throttle_in` | 9 |
| `CDrive::SetTwo` | 0x5043c0 | `layout::set_two` (identical on rust_step --only and the 30-step chain, 9/9; 240 direct records, vector test set_two_matches_original_calls) | 1080 |
| `CDrive::Set4570` | 0x504570 | `layout::set_diff_lock` (lock from `vcu::locks::lock_output`; not called in the 30-step runs; 240 direct records, vector test set4570_matches_original_calls) | 0 |
| `CDrive::M6DB0` | 0x506db0 | `layout::m6db0` (identical on rust_step --only and the chain, 9/9; 240 direct records, vector test m6db0_matches_original_calls) | 3240 |
| `CDrive::M6DD0` | 0x506dd0 | `layout::m6dd0` (identical on rust_step --only and the chain, 9/9; 240 direct records, vector test m6dd0_matches_original_calls) | 1080 |
| `E39_MgrB_Word8` | 0x501fa0 | `layout::mgr_b_word8` (name ours; not called in the 30-step runs; 240 direct records, vector test mgr_b_word8_matches_original_calls) | 0 |
| `E39_MgrB_Entry` | 0x501fb0 | `layout::mgr_b_entry` (name ours; not called in the 30-step runs; 240 direct records, vector test mgr_b_entry_matches_original_calls) | 0 |
| `E39_MgrB_FieldAt` | 0x502060 | `layout::mgr_b_field_at` (name ours; not called in the 30-step runs; 240 direct records, vector test mgr_b_field_at_matches_original_calls) | 0 |
| `E39_MgrD_Mat4Vec4` | 0x502400 | `layout::mat4_mul_vec4` (name ours; identical on rust_step --only and the chain, 9/9; unit test mat4_mul_vec4_adds_the_columns_left_to_right) | 272 |
| `E39_MgrD_CopyRow` | 0x502460 | `layout::copy_row` (name ours; not called in the 30-step runs; 240 direct records, vector test copy_row_matches_original_calls) | 0 |
| `E43EngineSim::Sim8CF0` | 0x518cf0 | `layout::sim8cf0` (not called in the 30-step runs; 240 direct records and vector test sim8cf0_matches_original_calls, the emit a host sink) | 0 |
| `CDrive::Test4840` | 0x504840 | `layout::drive_gate` (its `SimE980` call is `layout::rpm_above_40`; not called in the 30-step runs; 240 direct records, vector test test4840_matches_original_calls) | 0 |
| `CDrive::Set4820` | 0x504820 | `drive::set_clutch_slip` | 9 |
| `CDrive::Set47B0` | 0x5047b0 | `drive::request_gear` | 360 |
| `E39_ClearLanes` | 0x502c00 | `layout::clear_lanes` | 9 |
| `E43EngineSim::Sim8D90` | 0x518d90 | `layout::sim8d90` (the host call `Sim80D0`, 0x5180d0) | 36 |
| `E11_VecLerp` | 0x473c90 | `math::vec::lerp` | 89 |
| `CDriveLine::SampleBig` | 0x50b300 | `layout::sample_big` (step 9 + direct fuzz 120; the loop 0x50b580 and 0x50b690 are its interior; a negative start `ix - k` is 0: the `jns` at 0x50b3e2 tests the `sub`, the C++ tests `ix + k`) | 129 |
| `CEngine::SimEBB0` | 0x50ebb0 | `layout::sim_ebb0` (`math::trig::one_minus_x87_cos`, stored as f32) | 134 |
| `E43EngineSim::Sim80D0` | 0x5180d0 | `layout::intake_state_reset` (calls SimEBB0 and SimD870) | 120 |
| `E43EngineSim::Sim7D30` | 0x517d30 | `layout::turbo_energy_step` (rust_step identical on all 9 vcu-cov snapshots) | 120 |
| `E43EngineSim::Sim8610` | 0x518610 | `layout::sim8610` (the lerp `InitRealLerp` 0x50f010 computed inline; rust_step identical with 30-step chains) | 240 |
| `E43EngineSim::Sim8890` | 0x518890 | `layout::sim8890` (host: `Sim8890Host`, the car's slot 12 and `MB560`; the records cover the walk off, the rust_step runs the walk) | 240 |
| `E43EngineSim::Sim8E20` | 0x518e20 | `layout::engine_step` (hosts: `EngineStepHost` for the sinks `Emit`, `CGraph1D::Push` and `E01Log`, `E43Host` for `Sim80D0`, `Sim8890Host`; the records cover the walk off; rust_step identical with 30-step chains) | 240 |
| `CEngine::Set48` | 0x50d310 | `layout::set_options` (not called in the 30-step runs (unverified in the step); direct records and vector test: set48_matches_original_calls) | 240 |
| `CEngine::SetState4` | 0x50d340 | `layout::set_cut_flags` (identical on 30-step rust_step (9 snapshots); direct records and vector test: set_state4_matches_original_calls) | 240 |
| `CEngine::Get48_58` | 0x50d380 | `layout::options_gate_58` (not called in the 30-step runs (unverified in the step); direct records and vector test: get48_58_matches_original_calls) | 240 |
| `CEngine::SimE970` | 0x50e970 | `layout::sim_e970` (not called in the 30-step runs (unverified in the step); direct records and vector test: sim_e970_matches_original_calls) | 240 |
| `CEngine::SimE980` | 0x50e980 | `layout::rpm_above_40` (not called in the 30-step runs (unverified in the step); direct records and vector test: sim_e980_matches_original_calls) | 240 |
| `CEngine::SimE9A0` | 0x50e9a0 | `layout::sim_e9a0` (not called in the 30-step runs (unverified in the step); direct records and vector test: sim_e9a0_matches_original_calls) | 240 |
| `CEngine::GetState188` | 0x50eb80 | `layout::engine_state_speed` (not called in the 30-step runs (unverified in the step); direct records and vector test: get_state188_matches_original_calls) | 240 |
| `CEngine::SimEE60` | 0x50ee60 | `layout::sim_ee60` (not called in the 30-step runs (unverified in the step); direct records and vector test: sim_ee60_matches_original_calls) | 240 |
| `E43EngineSim::Sim8B20` | 0x518b20 | `layout::fuel_air_terms` (rust_step identical with 30-step chains) | 240 |
| `CDrive::Get4520` | 0x504520 | `drive::engine_speed` | 18 |
| `CDrive::Get4530` | 0x504530 | `drive::gear` | 360 |
| `CDrive::Get4540` | 0x504540 | `drive::drive_speed_key` | 18 |

`E39_Nop3860` (0x503860) and `CDrive::Nop3B0` (0x5043b0) are `ret 4` with no body: bound in rbr-hybrid (`nop_ret4`) with no port, and checked by rust_step only.

Branches covered by the records: TableLerp interp, neg, i_hi, big, NaN; UnkD6F0 zero, low, high; CurveEta below,
search, above; BlendW hit and end, in, below and above, short, and NaN `x` and `h`; Sim410 gate and off; Set47B0 clamps
(negative, in range, above the last gear). The step alone reaches few of these; the fuzz supplies the rest.

Batch 2 (engine kernels, group 2):

| Function | Address | Port | Matched calls (all 9 snapshots) |
| --- | --- | --- | --- |
| `CEngine::SetScratch08` | 0x50eb90 | `engine::set_pedal` | 369 |
| `CEngine::SimE370` | 0x50e370 | `engine::crank_tick` | 369 |
| `CEngine::SimE470` | 0x50e470 | `engine::rev_gate` | 374 |
| `CEngine::SimE520` | 0x50e520 | `engine::misfire_roll` | 369 |
| `CEngine::SimE9F0` | 0x50e9f0 | `engine::compliance` | 360 |
| `CEngine::SimEC40` | 0x50ec40 | `engine::exhaust_blend` | 410 |
| `CEngine::SimECF0` | 0x50ecf0 | `engine::stall_guard` | 360 |
| `CEngine::SimED70` | 0x50ed70 | `engine::torque_curve` | 374 |
| `CEngine::SimE660` | 0x50e660 | `engine::torque_sum`, `layout::LoadDef` | 369 |
| `CEngine::SimD870` | 0x50d870 | `engine::slot_roll`, `RollRet` | 374 |
| `E41_BlendX` | 0x50ced0 | `sim::blend_x`, `layout::blend_records` | 369 |
| `E41_BlendY` | 0x50d090 | `sim::blend_y` | 369 |

Batch 2 branches: not classified yet (no branch labels in the records for these), so only the fuzz's spread is
known. The fuzz runs each kernel 40 times per snapshot with random state, options and def values.

Batch 3 (group 3, the engine composites):

| Function | Address | Port | Matched calls (all 9 snapshots) |
| --- | --- | --- | --- |
| `CEngine::SimE060` | 0x50e060 | `engine::smooth_damper` | 9 step + 360 direct |
| `CEngine::SimE1F0` | 0x50e1f0 | `engine::solve_damper` | 9 step + 360 direct |

Branches: SimE060 is `ratio > 1` or not, crossed with whether `BlendX` finds a bracket: `gt1/bracket` 27,
`gt1/none` 108, `le1/bracket` 117, `le1/none` 108 (direct calls). SimE1F0's `ratio > 1` is reached both ways (12 and 28
direct calls on the first snapshot's records; the blend's bracket is not classified). A NaN `ratio` takes the `le1`
branch. The NaN sign of `DamperCombine`'s 0/0 is tolerated (see the platform section).

Batch 4 (drive line, `drive_line.rs`): `BlendB`, `Last`, `Thunk4F0`, `ClampMix` and `SetDriveLine` are ported. Step records
(`s<N>.txt`, regenerated with the batch's calls) and direct records (`d<N>-driveline.txt`, five functions, 40
each) on the nine snapshots; the counts are in the table below.

| Function | Address | Port | Matched calls (all 9 snapshots) |
| --- | --- | --- | --- |
| `CDriveLine::BlendB` | 0x50a650 | `drive_line::blend_b` | 818 (98 step; 720 direct, 360 of them inside ClampMix) |
| `CDriveLine::Last` | 0x50ad40 | `drive_line::last` | 360 direct (not called in the drive) |
| `CDriveLine::Thunk4F0` | 0x50a4f0 | `drive_line::thunk_4f0` | 369 (9 step; 360 direct) |
| `CDriveLine::ClampMix` | 0x50b0f0 | `drive_line::clamp_mix` | 369 (9 step; 360 direct) |
| `CDriveLine::SetDriveLine` | 0x50aec0 | `drive_line::set_drive_line` | 360 direct (not called in the drive) |

Branches: ClampMix is `interp` (0 <= x < 2^31), `negative` (integer part below 0), `above` (x >= 2^31, index 0x7fffffff)
and `log` (NaN or x < -2^31, index 0). The direct records reach all four. BlendB has no branch; its records include the
calls that ClampMix makes inside itself (the ClampMix files list them as `call` lines). SetDriveLine's windows are
bounded: the copied entries, one entry past them, and the entries at 0, 1000, ..., 9000 and 9999. The fill of the other
entries (`n..10000`, `q = v = 0`, `s = -1`) is checked only at those samples; the tags of filled entries are left as they
were, and the test checks them where they are windowed. Thunk4F0's E12 side (`E12_SampleX`) is not ported: its result is
the record's st0, and its input is the drive line at the global `0x1659184`, whose entries' `s` are windowed. A count over
10000 traps in the original, and so does `Last` on an empty table; the port asserts both.

Batch 5 (group 5, the E40 drive leaves, `drive_leaf.rs`): ten of the twelve are ported. The records are the step's
calls (`s<N>.txt`) and 40 direct calls per function per snapshot (`d<N>-leaf.txt`). Each count is step plus direct.

| Function | Address | Port | Matched calls (all 9 snapshots) |
| --- | --- | --- | --- |
| `CDrive::M5970` | 0x505970 | `drive_leaf::set_lane_scale` | 360 (direct) |
| `CDrive::M5A10` | 0x505a10 | `drive_leaf::gear_speed_error` (f64 on x87) | 362 (2 step; 360 direct) |
| `CDrive::M5BE0` | 0x505be0 | `drive_leaf::clutch_values` | 368 (8 step; 360 direct) |
| `CDrive::M5D50` | 0x505d50 | `drive_leaf::clutch_flags` | 362 (2 step; 360 direct) |
| `CDrive::M5FE0` | 0x505fe0 | `drive_leaf::wheel_slip_value` | 396 (36 step; 360 direct) |
| `CDrive::M6260` | 0x506260 | `drive_leaf::wheel_latch` | 396 (36 step; 360 direct) |
| `CDrive::M6630` | 0x506630 | `drive_leaf::axle_pair_stages` | 362 (2 step; 360 direct) |
| `CDrive::M6A00` | 0x506a00 | `drive_leaf::block_lane_step` | 360 (direct) |
| `CDrive::M6B20` | 0x506b20 | `drive_leaf::block_pair_gate` (eax) | 374 (14 step; 360 direct) |
| `CDrive::M6C90` | 0x506c90 | `drive_leaf::speed_term` (x87) | 369 (9 step; 360 direct) |

Branches (direct records): M5A10 `busy`, `zero`, `compute` and `nan`; M5BE0 all eight gate classes (`g`, `c`, `c`);
M5D50 `none`, `all` and `other`; M5FE0 `idle`, `coast` and `drive`; M6260 `gated`, and `flag0`/`flag1` with `up`, `down`
and `none`; M6630 the stage-one and stage-two sign classes of each pair sum (`p`, `m`, `n` NaN, `z` with the gate's sign)
and the total's sign; M6A00 `gate0` and `gate1`; M6B20 `zeros0`, `zeros2`, `hold`, `call` and `store`; M6C90
`zero_speed`, `zero_gate`, `zero_obj`, `low` and `high`. M5970 has one path.

`drive_leaf::zero_block` is the body of `CDrive::Step4620` (0x504620), which only `M6B20` calls. Its C++ is an empty
stub (cpp item 19), so it is verified through `M6B20`'s windows. `M6630`'s C++ disagrees with the asm in two places
(cpp items 17 and 18); the port follows the asm. The C++ of `M6A00` and `M6C90` agrees with the asm.

Not ported in group 5: `M8480` 0x508480. It calls `CDrive::M79B0` (0x5079b0, group 6) and the `CMultiSignal1::Emit`
slot 0x506e60 (group 10). `M8770` 0x508770 calls only `SetDirty` (the asm; the earlier note that it calls M79B0 and the
emits was wrong): it is ported for the load path (`layout::drive_setup_into_def`, with `drive::setup_terms`), checked
against snap-0 by `tests/load_defs.rs`, and not bound. `M6B10` (0x506b10) is a `ret 4` stub, not in the plan.

For the car instance (load step 4) `layout/construct.rs` has the drive's and engine's constructors: `CDrive::Init58D0`
(0x5058d0) with its part `E39_CDefCtor160` (0x503ad0) and `Set4510` (0x504510), `CDrive::CState::CState` (0x505480),
`CEngine::CEngine` (0x50df30) with its scratch (0x50d4f0), and `CEngine::CState::CState` (0x50bed0). They are checked
in the car by `tests/load_defs.rs` (`car_instance_matches_the_game`), not bound. The engine state's branch on the
physics definition's `DbgFlags` bit 0x200 calls `CGraph::SetA` on a null graph in the original: the port returns
`Err`. The drive state's words at 0x458 and 0x45c depend on the game mode (0x566690: mode 7, sub-mode 0 or 10, gives 0);
the reference run has the other mode (400.0).

Not yet ported: `SimE4A0` and `M8480` (group 5; `M8480`'s `M79B0` is ported since, its emit 0x506e60 is not). Groups 2
to 4 are ported; group 5 is ported apart from `M8480`; groups 6 to 8 are ported apart from the three group 7 hooks.

Batch 6 (group 6, the E40 drive workers, `drive_worker.rs`): seven of the nine are ported, on the typed `DriveWorker`
(`layout/worker.rs`; `M9BC0` and `Step1E0`, the other two, are below). None of them is reached by the step run (`s<N>.txt`), so their records are the direct calls only
(`d<N>-worker.txt`, 40 per function per snapshot). The gear workers also load `GearWork` (the band and the gear's words);
the calls to `CCar::Get30` (0x4e67f0) and the gear-change emit (0x506f20, the payload `[Get30, gear, x, flag]`) go through
`GearHost`, which the tests answer from the record.

| Function | Address | Port | Matched calls (all 9 snapshots) |
| --- | --- | --- | --- |
| `CDrive::M79B0` | 0x5079b0 | `drive_worker::signal_gear_change` | 360 (direct; the emit payload checked against the record) |
| `CDrive::M8570` | 0x508570 | `drive_worker::gear_trim` (reads `gear_speed_error`) | 360 (direct) |
| `CDrive::M9120` | 0x509120 | `drive_worker::gear_band_guard` (calls `signal_gear_change`) | 360 (direct) |
| `CDrive::M9210` | 0x509210 | `drive_worker::gear_logic` (calls `signal_gear_change`; x87 chain at 24 bits) | 360 (direct) |
| `CDrive::M93A0` | 0x5093a0 | `drive_worker::clutch_torque` (calls `gear_band_guard` and `gear_logic`; returns the value written to `out`) | 360 (direct) |
| `CDrive::M7C10` | 0x507c10 | `drive_worker::reset_wheels` (calls `clutch_flags`, `clutch_values`, `Step4990` through `Step4990Host`) | 360 (direct) |
| `CDrive::M7D60` | 0x507d60 | `drive_worker::clutch_state` (calls `block_pair_gate` and `block_lane_step`) | 360 (direct) |

Branches (direct records): M9120 `off`, `lower`, `upper` and `band`; M9210 `off`, `gate`, `off-capped`, `gate-capped` (the
speed term above 0.8) and `nan`; M8570 `hold`, `step`, `skip`, `trip`, `relay` and `accumulate`; M93A0 `A1a`, `A1b`, `A2a`, `A2b`, `B-set`
and `tail` (`B-clear` only after block A clears the word); M79B0 one path.

`M7C10` calls `Step4990` (0x504990, `Mat4Inverse`, group 7), which is a `Step4990Host`: the vector test answers it from the record, and
the binding answers it with `mat4::inverse_transposed` (batch 17, the port). In the vector test the matrix it is given must equal the record's input window, and its result is the record's output window.

Batch 6 history: the group 6 functions and the leaves they call (`drive_leaf.rs`, batch 5) were first written over raw
memory with offsets in the port. They are typed now; the vector tests replay the same records and did not change.
Batch B: `M9BC0` (`wheel_worker`) and `E39_RatioCA0` (`ring_average`) are typed, and so are the two leaves that still
worked on memory: `M5970` (`set_lane_scale`, input `LaneScale`) and `M6C90` (`speed_term`). `rbr-hybrid` binds all four through
`with_worker`. `E39_MatVec` is a host
call: `MatVecHost` takes the matrix and vector and returns the product (the test checks both inputs against the record).
`E39_Step3870` is ported (`free_wheel_step`, group G-drive96 below) and stays a trait for `M9BC0` (`Step3870Host`, which
`rbr-hybrid` answers with the port): the test checks the worker it is given against the record's input windows, checks the
port's result against the record's output windows and goes on with the latter. Term 3 and block 0's slip share their words (see `WheelTerm`),
so block 0's slip is written through `WorkerState::set_slip`, which keeps the term in step.

Group 6 closes with `Step1E0` 0x50a1e0 (`layout::step1e0` is the memory sequence; the decisions are in `drive_step.rs`), which
calls `M6C90`, `RatioCA0`, `Hook4160` four times (each edge is `layout::curve_term_at`: the block's curve state at `dri+0x48+200k` and the def's block at `lane+0x12c+240k`
are loaded, the term runs, and the state is stored back), `M8570`, `M93A0`, then `M9BC0` or `M96C0`. The car's `Fwd18E20`
(0x4e6650) is the host call of `Step1Host`; `M96C0` (`pair_worker`) runs only when the mode (the definition's word at offset 0x640, the
tweak `DriveType`) is 3, which only the MG ZR has (below). Its vector test (`step1e0_matches_original_calls`) replays the nine step records (one call each, from the snapshots of
`vcu_vectors.py --set engine-driveline`): the windows and the external calls in order. rust_step on the nine snapshots is
identical too.

Group G-drive96 (`lead/reports/g-drive96.md`): the pair worker `CDrive::M96C0` 0x5096c0 (`drive_worker::pair_worker`)
and its callees `E39_Step3870` 0x503870 (`free_wheel_step`), `M7AA0` 0x507aa0 (`reset_pair`), `M5B00` 0x505b00
(`pair_clutch_values`), `M5F10` 0x505f10 (`pair_slip_value`), `M60D0` 0x5060d0 (`pair_latch`) and `M6540` 0x506540
(`pair_axle_stage`, `pair_stage` of axle 1's pair reversed). `M96C0` is the wheel worker of a drive of `DriveType` 3, the
MG ZR's two-wheel drive (of the v1 and v2 drives only the ZR's snapshots reach it; the Corolla, Impreza, Xsara, Lancer and
Accent take `M9BC0`): term 1 and a 2x2 matrix (the state's words at offsets 0x500, its inverse 0x510, the flags 0x520 and 0x524) where `M9BC0`
has three terms and the 4x4 matrix. Wheels 0 and 1 are held (their step by the enable word), 2 and 3 always take
`E39_Step3870`. From the asm, against the C++: the tail loop runs twice (`0x18` to `0x30` by `0xc`), not four times, and
the drive speed key is `0 - term1.value` (the register is zero there, not `f14`). The solve loop repeats while term 1's
active flag is 1 and `this.loc[gi]` is beyond the scale; nothing in the loop writes either, so in the original that state
never ends (the fuzz recipe keeps out of it). Records: `vcu_vectors.py --set g-drive96` on vcu-v1-tarmac-zr 1000 and 1739
(the ungated and the gated branch) and `direct_calls.py --set g-drive96` (fuzzed drives, every branch the classifiers name);
the tests compare a NaN word with any NaN (an infinite inverse writes the default NaN).

ABI, from the asm (ground truth for the C++ `CDrive::Step1E0`, RBR_E40_Big.cpp:859):
- `CCar::Fwd18E20(float dt, float v)` (RBR_E34_Car.h:669) is the thunk 0x4e6650 (`add ecx,0x690; jmp 0x518e20`) into E43's
  `Sim8E20`, which is thiscall with two stack words (`dt`, then `v`) and ends `ret 0x8` (the function spans 0x518e20-0x51942c).
- Step1E0 pushes three words before the call: the ring pointer `dri+0x388`, the speed `dri+0x45c`, then `dt`. The callee pops
  only the last two, so the ring pointer stays on the stack. The caller's `push ecx` (0x50a244) is then overwritten by the
  `fstp` of `fr` (0x50a247), so the stack holds `(fr, ring)` for `E39_RatioCA0` (stdcall, `ret 0x8`, 0x503ca0).
- The C++ calls `fc->Fwd18E20(dt, speed)` and `E39_RatioCA0(fr, dri+0x388)`; the two-argument call matches the asm, so the
  C++ has no behaviour gap here and no cpp-fixes item is needed. The ring is passed through the callee's leftover word, not
  through `ecx` (the value of `ecx` after the call is dead).
- `Hook4160`'s stack is `(state, block, n, x, s)`: the first word is the curve state (`dri+0x48+200k`, read at `+0x1c`,
  `+0x70`, `+0xac`), the second is the def's block (`lane+0x12c+240k`, read at `+0x6c`, `+0xec`). The C++ shows `v5` in
  `edx` as well, which the port does not use; the stack order is the one the records confirm.

Typed layout (`layout/worker.rs`): the worker's objects are read and written as `DriveWorker` (the definition's axles and wheels,
the state, `this`'s words, the object `this+0x170`) and `GearWork` (the gear's band, scale and time step). The loads take
every word a worker can read, so the test windows cannot be read as they are: `worker_mem` in the vector test zero-maps
the objects and maps the recorded windows over them. Two words are shared in the original and are written only when a
port changes them: term 3's sign and scale are block 0's slip lane (`WorkerState::store`).

Batch 17 (group 7, the matrix pair, `mat4.rs`): `E39_RowsCopy` 0x502570 (`mat4::transpose`, `layout::rows_copy`) and
`E39_Step4990` 0x504990 (`mat4::inverse`, `mat4::inverse_transposed`, `layout::mat4_inverse_transposed`). The all-bindings 30-step `rust_step` on the
nine snapshots (recording `drive-tanner-corolla.bin`) is identical: run B equal to run A on all nine. The chain reaches
`Step4990` through the wheel worker's `Step4990Host`, which the binding now answers with `mat4::inverse_transposed`, so the binding's
own entry is not run in the chain. `RowsCopy` is not entered (its only caller is the original `Step4990`); it has a unit
test and no record.

## Names

| Name | Original | Meaning |
| --- | --- | --- |
| `sim::table_lerp` | `E41_TableLerp` (0x50d580) | linear table lookup over evenly spaced knots |
| `sim::unk_d6f0`, `UnitSplit` | `E41_UnkD6F0` (0x50d6f0) | split of a unit fraction into three weights (see unknown meaning) |
| `sim::curve_eta`, `CurveKnot` | `E41_CurveEta` (0x50d7a0) | curve lookup on keys at +0x0c, values at +0x1c, 0x20 bytes apart |
| `sim::blend_w`, `Bracket` | `E41_BlendW` (0x50d220) | knot bracket of x with the two weights (`w_upper` = fraction, `w_lower` = 1 - fraction) |
| `sim::abs_rpm_term` | `CEngine::Sim410` (0x50d410) | abs(abs_scale * engine speed), behind a gate |
| `drive::*`, `Drive`, `DriveState`, `DriveDef` | `CDrive` accessors (this+4 def, this+8 state) | the drive's input lanes and the commanded gear |
| `Lane`, `IntLane` | NTweak value with a dirty flag at +8 and the value at +0xc | a write sets the flag |
| `EngineObjects` | `this+4`, `this+8`, `this+0x48` | the engine's def, state and option block |
| `EngineDef.abs_scale` | def `+0x282c` | scale of the abs term |
| `EngineState.speed` | state `+0x188` | engine speed, rad/s |
| `EngineState.abs_value` | state `+0x24c` (CReal, dirty `+0x248`) | the abs term, written by `Sim410` |
| `EngineOptions.abs_gate` | option `+0x58` | gate of the abs term (name is ours) |
| `sim::blend_x`, `sim::blend_y`, `BlendRec` | `E41_BlendX` (0x50ced0), `E41_BlendY` (0x50d090) | blend of two tables of (key, y1, y2) records at 0xc bytes: the search on the combined key `b * d + c * e`, then the bracket's two records mixed by a weight |
| `layout::blend_records` | the table of a blend argument | the table's count (`desc + 4`) and records (pointer at `desc + 8`); names ours |
| `engine::torque_sum` | `CEngine::SimE660` (0x50e660) | the torque from four load ratios, the threshold term and the option words (see unknown meaning) |
| `layout::LoadDef` | def `+0x138`, `+0x178`, `+0x1b8`, `+0x1f8`, `+0x284c`, `+0x285c`, `+0x286c` | the load denominators, span, threshold slope and threshold of `SimE660`; read separately because they lie past the 0x2830 window the other kernels use |
| `engine::slot_roll`, `RollRet` | `CEngine::SimD870` (0x50d870) | the roll for stall term `i`; `RollRet` is what eax holds (the quotient, the term's address, the options block, or 0) |
| `layout::BlendKnot`, `layout::blend_table` | the outer table of `SimE060`/`SimE1F0` (count at +0, records at +4; each record is key, sub-table count, sub-table records) | names ours |
| `layout::load_blend_terms`, `load_relaxation`, `store_blend_terms`, `store_relaxation` | the four-word blocks at the `f` argument of `SimE060` (`BlendTerms`) and the `e` argument of `SimE1F0` (`Relaxation`) | the types are `spring_damper`'s |
| `EngineState.smoothed`, `EngineState.solved` | state `+0x1e4` (CReal, value `+0x1f0`) and `+0x1f4` (CReal, value `+0x200`) | names ours; the damper outputs |
| `engine::smooth_damper`, `engine::solve_damper` | `CEngine::SimE060` (0x50e060), `CEngine::SimE1F0` (0x50e1f0) | the two damper steps; names ours |
| `drive_leaf::set_lane_scale`, `gear_speed_error`, `clutch_values`, `clutch_flags`, `wheel_slip_value` | `CDrive::M5970`, `M5A10`, `M5BE0`, `M5D50`, `M5FE0` (0x505970 to 0x505fe0) | the lanes and flags of the drive's slip and clutch terms (names ours; see the module doc) |
| `drive_leaf::speed_term` | `CDrive::M6C90` (0x506c90) | the speed term: the factor from the speed (`+0x45c`, below 400 only) and the object's speed term `+0x8c`, which it updates; the result is the factor times 80 |
| `drive_leaf::wheel_latch` | `CDrive::M6260` (0x506260) | the clamp-and-latch of wheel `x`: the drive term less the slip term, bounded by the wheel offset and the block's latch bound; a latch sets `pack_out[x]` to the wheel's weight |
| `drive_leaf::axle_pair_stages` | `CDrive::M6630` (0x506630) | two axle-pair stages (sum, weighted sum and sign of the pair's forces) into terms 1 and 2, then the sums into term 0 |
| `drive_leaf::block_lane_step` | `CDrive::M6A00` (0x506a00) | the force of block `k` (the axle's first block index) when its gate is set: the wheel weight over the axle scale times term `x`'s value, plus half its term |
| `drive_leaf::block_pair_gate` | `CDrive::M6B20` (0x506b20) | the gates of axle `x`'s two blocks: one zero gate sets term `x`'s sign to the gates' sum (or zeroes a block when both pack outputs are zero); returns the eax |
| `drive_leaf::clutch_flags` | `CDrive::M5D50` (0x505d50) | the active flags (`lane_active`, one per axle index) from the four block gates and the axles' pair gates; the branch word |
| `drive_leaf::wheel_slip_value` | `CDrive::M5FE0` (0x505fe0) | the slip value of wheel `x` into `step_vec[x]`: the latch bound and offset, the axle's drive term, less the slip term |
| `drive_leaf::clutch_values` | `CDrive::M5BE0` (0x505be0) | the clutch-pack values of wheel `x` into `clutch_matrix[x]` and the slots `x + 4 c` (c the axle 0 index) and `x + 4 c'` (c' the wheel's axle index) |
| `drive_leaf::gear_speed_error`, `GearWork` | `CDrive::M5A10` (0x505a10) | the engine speed over the gear's ratio, less the drive speed scaled by `ratio_gain`; zero while the gear flag is set or the ratio is zero (f64 on x87) |
| `drive_worker::signal_gear_change`, `GearHost` | `CDrive::M79B0` (0x5079b0) | the gear-change signal: `[Get30(car), gear, x, flag]` emitted (the car is `state+0x580`); `GearHost` answers `Get30` and the emit |
| `drive_worker::gear_trim` | `CDrive::M8570` (0x508570) | the gear trim: the accumulator at `+0x574` against the definition's slope and reach limits; the trim `+0x560` steps from `gear_speed_error` |
| `drive_worker::gear_band_guard` | `CDrive::M9120` (0x509120) | the gear guard (only when the CDef's gear-guard flag is set): a `dt` outside the gear's band (`lower`, `upper` from the band and the gear scale) clears the gear flag, sets the trigger and signals `x = 0` |
| `drive_worker::reset_wheels`, `Step4990Host` | `CDrive::M7C10` (0x507c10) | the reset worker: the clutch flags, the matrix clear, the clutch values for wheels 0 to 3, the pack outputs cleared, the matrix stepped by `Step4990` (repeated when its first word is NaN) |
| `drive_worker::wheel_worker`, `MatVecHost`, `Step3870Host` | `CDrive::M9BC0` (0x509bc0) | the wheel worker: the latch and slip terms, the matrix step (`E39_MatVec`, whose output is `this.loc`), the blocks' slips (`E39_Step3870` for a wheel whose enable word is zero, else the force's change over `dt`) and their previous values |
| `drive_curve::wheel_curve_term`, `layout::HookBlock`, `CurveInput`, `obj_gate` | `E39_Hook4160` (0x504160) | the car's speed term `e` (the input's two words, the gate zeroes it), `aval = gain * r * w` with `r` from `row_value`, and the position and lane each counted down by `-1e6 * num / den` and then set to 500000000 (names ours; the asm's writes are kept) |
| `drive_curve::unsigned_table_lerp`, `count_down`, `row_value`, `layout::CurveRow`, `CurveTables`, `CurveState` | `E39_TabE00` (0x503e00), `E39_WorkerF60` (0x503f60), `E39_Worker4060` (0x504060) | the curve lerp (`(a-b)*n/(c-b)` on unsigned words, the table's segment and fraction); the counter step (the row's scaled value counts `b+0xc4` down, `IntLane` dirty at `b+0xc0`); the row's interpolation over the position (names ours; the row and the tables are in `CurveRow` and `CurveTables`) |
| `drive_worker::ring_average`, `layout::RatioRing` | `E39_RatioCA0` (0x503ca0) | stores `v` in the ring's next slot and returns the mean of the first `count` values; the ring is at the caller's pointer (`Step1E0` keeps it at the state's `+0x388`) |
| `layout::step1e0`, `layout::curve_term_at`, `drive_step::*` | `CDrive::Step1E0` (0x50a1e0) | the drive's step (names ours): the sequence and its edges in `layout/step.rs`; the decisions and arithmetic (`drive_step.rs`: the forwarder scale, the clutch input and ratio, the trip, the drive gain, the speed copy, the magnitude) are pure; checked by `step1e0_matches_original_calls` (nine step records) and rust_step |
| `drive_worker::clutch_state` | `CDrive::M7D60` (0x507d60) | the clutch-state machine: gate words fire on a sign change while the pack output is zero; `block_pair_gate` (packs 1 and 2) and the sync of the speed terms; `block_lane_step` steps the clutch |
| `drive_worker::clutch_torque` | `CDrive::M93A0` (0x5093a0) | the clutch-torque worker: returns the value written to `out` (`word * b` on the trip paths, else `a` or the integrated `f6`), the speed term `+0x45c` integrates over `d`; `gear_band_guard` and `gear_logic` run on the trip |
| `drive_worker::gear_logic` | `CDrive::M9210` (0x509210) | the gear-logic worker: with the guard, an RNG step (`+0x57c`) moves the gear's scale; the level `|dt| * 0.0333` capped to `[0, 0.8]` gives `sqrt(level + 0.2)`, kept as the gear level and emitted |
| `layout::DriveWorker`, `WorkerDef`, `WorkerState`, `WorkerThis`, `WorkerObj`, `GearWork`, `GearBand` | the CDef (`this+4`), the state (`this+8`), `this` and the object at `this+0x170`; the gear's band (`+0x688`), scale (`+0x48` table) and time step (`this+0x174`) | typed views loaded and stored by `DriveWorker::load/store` and `GearWork::load/store` (offsets in `layout/` only); the names are ours where the original has none |
| `layout::Axle`, `Wheel`, `WheelTerm`, `WheelBlock` | the CDef's axle records (0x60 bytes from `+0x0`), wheel records (0xf0 from `+0x148`), the state's wheel terms (`+0xc + 0x14 i`) and wheel blocks (`200 i`) | names ours (see unknown meaning) |
| `AIR_GAS_CONSTANT`, `STANDARD_PRESSURE` | the damper constants `287.0` (`0x438f8000`) and `101300.0` (`0x47c5da00`) | the gas constant of dry air and one standard atmosphere (from the C++ and the value); names ours |
| `EngineState.signed_rate`, `load_bc`, `load_f4`, `load_12c`, `load_164`, `threshold_value`, `torque_accum`, `torque` | state `+0x184`, `+0xbc`, `+0xf4`, `+0x12c`, `+0x164`, `+0x23c`, `+0x238`, `+0x270` | the `SimE660` inputs and outputs (names ours) |
| `EngineState.slot_a`, `slot_b` | state `+0x94 + 0x38 i`, `+0xac + 0x38 i` (`i < 4`) | the slot pair `SimD870` reads (names ours) |
| `drive_line::Entry`, `entry_at`, `load_entry`, `store_entry` | the table entry at `this + 0x30 k + 0x10`: `q` (vec4, +0), `v` (vec4, +0x10), `s` (float, +0x20), tag (dword, +0x24); the count at `this + 0x75310` | names ours; the layout is from `CDriveLine::Fetch`, `BlendB` and `SetDriveLine` |
| `drive_line::blend_b` | `CDriveLine::BlendB` (0x50a650) | the cubic Hermite between entries `idx` and `idx + 1` at `t` (positions `q`, tangents `v`) |
| `drive_line::clamp_mix` | `CDriveLine::ClampMix` (0x50b0f0) | `BlendB` at the integer part of `x` and the fraction |
| `drive_line::last`, `drive_line::thunk_4f0`, `drive_line::set_drive_line` | `CDriveLine::Last`, `Thunk4F0`, `SetDriveLine` | the last entry's `s`; the trampoline to `E12_SampleX` (host `0x7eaba8`, the caller supplies E12); the table load from a signal (count at +0, base at +4) |
| `EngineOptions.load_terms`, `roll_gate` | option `+0x14`, `+0x24`, `+0x34`, `+0x44` (scaled by 400), `+0x4` | the `SimE660` terms and the `SimD870` gate; names ours |

## Unknown meaning

- `WorkerThis.lane_gate` (`+0x50`), `gate_level` (`+0x2c`), `lane_scale` (`+0x40`), `lane_scaled` (`+0x74`), `scaled_term`
  (`+0x1a0`) and `speed_factor` (`+0x1c8`); `WorkerObj.ready` (`+0x7c`) and `speed_term` (`+0x8c`); and
  `WorkerDef.lane_scale_gain` (`+0x630`): the `M5970`/`M6C90` words. Their meanings are not known; names are ours.
- `WorkerThis.loc` (`+0xa0`, the original's `loc20`): the output of `E39_MatVec` in `M9BC0`, read as the per-lane values.
  The name is ours.
- `E39_Step3870` (`free_wheel_step`, G-drive96): `Step3870Host` edits the worker in place.
- `WorkerState.sign_word` (`+0x368`), `WheelBlock.clutch_load` (`+0xb4`) and `latch_bound` (`+0xf8`), `Wheel.pack`
  (`+0x158`, read as an axle index), and the counters `count_a` (`+0x5a8`), `latch_count` (`+0x5a4`), `repeat_count`
  (`+0x5ac`) and `dirty_count` (`+0x5b0`): the workers use them as written; their meanings are not known, names are ours.
- `WorkerState.pending` (`+0x5bc`): set by a latch and the clutch pass, cleared by the reset. The name is ours.
- `WorkerState.trigger` (`+0x578`): set when the gear or the clutch changes; the name is ours.
- `UnitSplit`: the record `E41_UnkD6F0` writes at its pointer (+0x08 `at_zero`, +0x0c `w_low`, +0x10 `w_mid`,
  +0x14 `w_high`). The three weights are a split of a unit fraction; what reads them is not checked yet.
- `EngineOptions.abs_gate`: option block `+0x58`, the gate of `Sim410`. Its meaning beyond that is not known.
- Misfire (`SimE520`): `EngineState.misfire_rate` (+0x10), `misfire_time` (+0x14), `misfire_count` (+0x18) and
  `misfire_out` (+0x3c). The roll uses a 32-bit LCG whose state is the word the option block points at; the level
  goes to `EngineThis.misfire_level` (+0x194). The rate and the counters' meanings are not known, names are ours.
- Compliance (`SimE9F0`): four stages at state +0xb8, +0xf0, +0x128, +0x160 from def stage low/high pairs, and a
  fifth at +0x80 added when the def's enable (+0x26f8) is set. The output is `1 / sum(1 / stage)` at +0x26c. What the
  stages stand for is not known.
- Torque (`SimED70`): three coefficient arrays (constant, linear, quadratic) at def +0x2214..+0x221c, +0x21fc..+0x2204,
  +0x222c..+0x2234, scaled by def +0x21e0, +0x21dc, +0x21e4. Their physical meaning is not known.
- Stall guard (`SimECF0`): option `stall_terms` (+0x8, +0x18, +0x28, +0x38) and gates (+0x58, +0x5c); the 0.7 and
  0.1 thresholds are read from the asm. Meaning not known.
- Rev gate (`SimE470`): option `rev_gate` (+0x74, u32) and a 500 rpm floor; meaning of the gate value not known.
- Torque sum (`SimE660`): four load ratios (state `+0xbc`, `+0xf4`, `+0x12c`, `+0x164` over the def denominators),
  each clamped to 0..1 and taking `1 - ratio` of the span; the option words (`+0x14`..`+0x44`, times 400) and the
  threshold term (state `+0x23c` against def `+0x286c`) are its other inputs. What the loads are is not known.
- Stall roll (`SimD870`): slot `i`'s stall term (option `stall_terms[i]`, +0x8 + 0x10 i) rises when a draw from the
  LCG is below a fraction built from the slot pair (`slot_a`, `slot_b`) and the threshold; the global table at
  `0x7ca668` (one word per slot) is scaled by `1 - fraction^2` on every call. Their physical meaning is not known.
- Blend tables (`BlendX`/`BlendY`): the record fields are key, y1 and y2 (names ours); what they index is not known.
- Damper steps (`SimE060`, `SimE1F0`): the arguments are `a` (the outer table's key), `b` (`ratio`), `c` (`scale`), `e`
  (`dt`, the caller's step time) and the blocks. The meaning of `ratio`, `scale` and the smoothed and solved values is
  not known; the names give the role in the code. `DamperSqrt4/Scale/UpdateA/Combine/Solve` are the spring-damper ports.

## C++ against the asm

The port follows the asm. Where the C++ in `src/rbr` disagreed, it has been corrected, except where noted:

- `E41_BlendX` (0x50ced0): the C++ breaks the search on `f10 <= a`. The asm breaks on `!(f10 > a)`, so NaN `a` stops at
  the first record (`h = 1`), as the records show. The weight's clamp (`0 <= w`, NaN to 0) is the C++'s. The C++ is
  not corrected yet.
- `E41_BlendY` (0x50d090): the C++ and the asm agree in the recorded cases: the search breaks on `!(a > key)` and the
  weight's clamp is `!(w < 0)` (NaN kept).
- `SimED70` (0x50ed70): the C++ now writes `q[7] = 0` when `q[2] != 0`, matching the asm's unconditional output store;
  the port does that, and the test checks it.
- `E41_BlendW` (0x50d220): the old C++ search broke on `x < key || x == key`, so a NaN `x` ran to the last knot;
  the asm stops at knot 1 (`hi = 1`, `lo = 0`), i.e. `!(x > key)`. For NaN `h`, the asm keeps both NaN weights;
  corrected C++ uses `!(h > 1)` to preserve them.
- `CDrive::Set47B0` (0x5047b0): `+0x570` is the value word of the lane at `+0x564` (dirty at `+0x56c`), and the
  existing C++ store matches the asm. Its comment now says so.
- `CEngine::SimE060` and `SimE1F0` (0x50e060, 0x50e1f0): the C++ chains the sqrt, the `DamperScale` and the divisions in
  double (`(double)` casts). The asm runs them at 24 bits, rounding each x87 operation (f32 steps, as `rust-physics.md`
  section 3 says). The port follows the asm. The C++ is not corrected; it is listed in `lead/briefs/cpp-fixes-from-rust.txt`.
- `SimE1F0`: the asm reads the solve block's accumulated word (`e+4`) after `DamperSolve` for the average, and before it
  for the ratio's scale and the solve's input path. The C++ says the same; the port keeps both reads. The asm's `_isnan`
  calls (on `b` and `a`) have unused results; the port has no calls for them.

- `CDriveLine::ClampMix` (0x50b0f0, RBR_E41_DriveLine.cpp:42-63): for NaN `x` the C++ takes the `else` path with
  `i = (int)x` (INT_MIN on x86) and blends at that index. The asm takes its log path for NaN (`comiss` unordered) and
  blends at index 0 with fraction 0, as it does for `x < -2^31`. The port follows the asm. The C++ is not corrected; it is
  listed in `lead/briefs/cpp-fixes-from-rust.txt`.
- `CDriveLine::BlendB`, `Last`, `SetDriveLine`, `Thunk4F0`: the C++ agrees with the asm in every recorded case (the port's
  order of the Hermite terms is the C++'s, and the vectors agree).
- `CDrive::M6630` (0x506630, RBR_E40_LeafC.cpp:134-208): three disagreements, all listed in
  `lead/briefs/cpp-fixes-from-rust.txt` (items 17, 18, 20). Stage one's zero gate sum skips the `+0x28` write in the C++
  (the asm always writes the sign). Stage two pairs `def[l0]` with block `b1` in the C++ (the asm pairs `b0`). A NaN
  stage-two sum gives 1 in the C++ and 0 in the asm. The port follows the asm.
- `CDrive::M6B20` (0x506b20, RBR_E40_LeafC.cpp:343-372): the C++ calls `Step4620()` with no argument. The asm passes the
  block index and does the lane writes of `0x504620`, whose C++ body is empty (item 19). The port has that body as
  `drive_leaf::zero_block`.
- `CDrive::M8570` (0x508570, RBR_E40_Mid.cpp:463-525): for NaN `k` (the trim after the step, `k = trim - h*dt`) the C++
  tests `0.0f <= k` and writes 0; the asm's `comiss` takes the not-`>` path for unordered values and keeps the NaN. The
  records show the asm's result (16 NaN cases, e.g. `step`, `dt` NaN, trim 3.0 to NaN). The port follows the asm. The
  C++ is not corrected; it is listed in `lead/briefs/cpp-fixes-from-rust.txt`.
- `CDrive::M9210` (0x509210, RBR_E40_Big.cpp:297-353): the C++ agrees with the asm in shape. The x87 chains run at 24-bit
  precision (each op rounds to f32), which the records confirm: f64 chains give a different last bit in the square root.
  The port uses f32 arithmetic in the original's order.
- `CDrive::M9120` (0x509120, RBR_E40_Big.cpp:253-295) and `CDrive::M79B0` (0x5079b0): the C++ agrees with the asm.
- `CDrive::M7D60` (0x507d60, RBR_E40_Mid.cpp:143-420): three places where the C++ and the asm differ, and the port follows the asm.
  The pack word of wheel `k` is read at `lane+0x158+0xf0 k` (the asm, `addl $0xf0` at 0x507ea8); the C++ steps `0x3c`,
  and the records fail with that stride (the first call of snapshot 300). A NaN gain gives sign 0 (`comiss`/`jbe` at 0x507df2-0x507e05); the C++ `g <= 0 ? ... : 1` gives 1.
  The `f15` term is `sum * (lane48 * 0.5)` in the asm; the C++ `sum * lane48 * 0.5f` rounds a denormal sum differently
  (snapshot 300, call 54: 2 ulp against the recorded 1).
- `CDrive::M9BC0` (0x509bc0, RBR_E40_Big.cpp:632-860): the wheel-sign NaN takes 0 in the asm (`sign`), where the C++ `v <= 0 ?
  ... : 1` gives 1. The wheel pack index is read at `lane+0x158+0xf0 k`, as in M7D60. The C++ `E39_Step3870` is marked as
  approximate (55%); the port does not use it: `M9BC0` calls it through `Step3870Host`, which rbr-hybrid answers with
  the asm-based port `free_wheel_step` (G-drive96). Its per-wheel
  `wheel_enabled` word picks the host call or the slip from the force's change.
- `CDrive::M7C10` (0x507c10, RBR_E40_Mid.cpp): the C++ passes an extra `0` as edx to `E39_Step4990`; the asm pushes only the
  out pointer and the callee ignores edx, so the port passes no edx. `E09Mat44::Clear` (0x465e00) is inlined (the clutch matrix set to zeros in `reset_wheels`).
- `CDrive::M6A00` (0x506a00) and `CDrive::M6C90` (0x506c90): the C++ agrees with the asm. `M6A00` writes its lane twice
  in both (the port writes it once, which leaves the same state).

## Two-level blend tables

`SimE060` and `SimE1F0` take a table whose count is at +0 and whose records are at +4. Each record is 12 bytes: the key,
the sub-table's count and the sub-table's records pointer. `BlendW` searches the keys; `BlendX`/`BlendY` take the two
records at the bracket as their descriptors (count at +4, records at +8). `layout::blend_table` loads the whole table.
The asm reads the second sub-table with the first's count, and the port does too; the recipes keep the counts equal.
`BlendW` reads the knot after the last when the table has one knot, so the recipes use two or more.

## Platform: default NaN

Tests accept any NaN in an st0 return (`check_st0`). The windows are compared bit for bit, except `SimE060` and
`SimE1F0`: their windows use `check_windows_nan`, which accepts any NaN for a NaN word. `DamperCombine`'s `0/0`
(`denominator / target.scale` with both zero) gives the default NaN, `0xffc00000` in the original and a positive one here.
Sites are listed in `notes/rust-physics.md` ("Platform: default NaN").
