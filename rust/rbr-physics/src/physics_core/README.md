# Physics core port (E45_*, E46_*): plan and status

Scope: the functions in `src/rbr/RBR_E45_*.cpp` and `RBR_E46_*.cpp` that the reference drive reaches
(`notes/progress/called.csv`, tanner-corolla): 94 functions. The E46 part is `notes/physics/physics-core.md`
(names, layouts, the suspect code 5a-5f). Layout and renames: `notes/physics/renames/physics-core.csv`.
The E45 files hold CMountedRigidBody, CPassenger and CMaterial code; the lead put them in group G (the entities and the
signal glue), after group D.

## Plan, leaf to step

Each group is ported after the one above it. Within a group, the order is the order the vectors cover.

| Group | Functions (address, C++ name) | Status |
| --- | --- | --- |
| A. Math leaves (pure, no `this` state) | 0x528980 `E46_IdxWrite`; 0x528b00 `E46Vec::Add`; 0x528b90 `E46Vec::Sub`; 0x528c20 `E46Mat::Mul`; 0x528dd0 `E46Mat::ScaleRows`; 0x528fa0 `E46Mat::XformRows` | ported (batch 1) |
| B. Accumulators on CSolid (call the body and the E43 walk) | 0x529250 `CSolid::E43Vf60` (= `E46_Accum250`); 0x529340 `CSolid::E43Vf70` (= `E46_Accum340`); 0x5293d0 `E43Vf80`; 0x5293e0 `E43Vf8C` | ported (`E43Vf70` in rigid_body) |
| C. CDef/CScratch/CState accessors and ctors | 0x5278d0 `CDef::Vf50`; 0x526760 `CDef::Vf54`; 0x5284f0 `CDef::Vf4c`; 0x524c70 `CDef::CDef`; 0x524e80 `CScratch::CScratch`; 0x5270c0 `CState::CState`; 0x527c60 `CPhysics::CPhysics`; 0x524f80 `CPhysics::Vf10` | ported (load path); `Vf10`'s tool path is not |
| D. Setup (load time, not per step) | 0x526660 `E46_CreateDef`; 0x5266d0 `E46_InitPhysics`; 0x526fd0 `E46_AppInit`; 0x5276a0 `E43HostRel::WallRel` | ported (load-time dumps, `RBR_SNAPAT`) |
| E. Detectors and kernels (status below) | 0x5240c0 `E45_M40C0`; 0x523d60 `E45_MD60`; 0x5246a0 `E45_M46A0`; 0x524250 `E45PhysBody::M4250`; 0x5242c0 `CL2GiganticPenetrationDetector::Vf04`; 0x524480 `CL2PenetrationDetector::Vf04`; 0x5235b0 `CL2PenetrationDetector::Ctor`; 0x5257e0 `E46L2Detector::Vf04`; 0x525bd0 `E46_Kernel5bd0`; 0x526ad0 `CL1CollisionDetector::Vf04`; 0x525a20 `E46L1PenDetector::Vf04` (bad, 5e) | all eleven ported; all but `MD60` and `M4250` bound |
| F. Contact step | 0x526d20 `CollideWork`; 0x526cc0 `PenCheck` (5a); 0x526a70 `Work6a70` (5b); 0x526390 `Visit470` (bad); 0x5263d0 `CState::RegWall`; 0x5267d0 `E46_Notify67d0`; 0x525640 `Work5640`; 0x525500 `Work5500`; 0x525760 `Work5760`; 0x5273a0 `StepForward` | all ten ported (`RegWall` and `Notify67d0` as `layout` sequences, see group E); all but `Notify67d0` bound (`Work5640` and `Work5760` partial in `bound.csv`); the callees the ports do not run stay host-dispatched |
| G. Signal glue and E45 entities | 0x526580 `CMultiSignal::Emit`; 0x527b40 `NotifyPad`; 0x527b90 `NotifyExitGame`; E45 Material, MountedRigidBody, PassengerA/B, PhysA flag getters | G1 to G6 done (see the group G plan); G7 open |

Status (group A): ported and vector-tested (`tests/physics_core_vectors.rs`). The recordings are the reference
drive's snapshots 40, 572 and 1295 (`/tmp/rbr-snaps`), plus snapC 572/1295 and snapT2 572, with `--fuzz` seeds 1-3 for
the branch the drive does not take (`harness/vector_specs/physics_core.py`).

| Function | Calls | Branches covered | Not covered |
| --- | --- | --- | --- |
| `E46_IdxWrite` | 3360 | `down`, `up`, `first`, `last` (fuzzed table around the drive's value) | |
| `E46Vec::Add` | 3064 | one (no branch) | |
| `E46Vec::Sub` | 10 | one | |
| `E46Mat::ScaleRows` | 3064 | one | `n == 0` (the original returns early; the port loops zero times) |
| `E46Mat::XformRows` | 10 | one | `n == 0` |
| `E46Mat::Mul` | 104 (inner calls of `E47SolidState::Vf17x`) | one | the drive never takes the caller's gate (`+0x7c > 0`); `--fuzz` sets it |

`E46Mat::Mul` is recorded as an inner call of `E47SolidState::Vf17x` (0x529a50, the gate's caller, listed in the spec for
its fuzz). The Vf17x record itself is not ported here (it is an E47 function).

Status (group B, accumulators on `CSolid`): ported. `E46_Accum250` (0x529250) is split: `math::gravity_force` takes
the walk's output (the `E43PairWalk::MB560` call, recorded as an inner call of Accum250, its out window), scales it by
`1 / inverse_mass` (None at 0). The walk itself (`entities::walk::mb560`, another port) and the virtual calls stay in the
caller, so the test checks the scaled vector against the one the original passed to `Vf70`.

| Function | Calls | Covered | Not covered |
| --- | --- | --- | --- |
| `E46_Accum250` (`gravity_force`) | 108 (98 with the walk and accumulate; 10 with a zero inverse mass, fuzzed) | both gate outcomes | |
| `E46_Accum340` (`accumulate_force`) | 98 inner calls of Accum250 | one | |
| `CSolid::E43Vf80` (`layout::cache_1dc`) | 5235 | one | |
| `CSolid::E43Vf8C` (`layout::cache_1ec`) | 0 | none (not reached on the recorded steps) | all |

Not ported (side effects outside the arithmetic): the debug log hook, `S00` on the flag `0x4000` in both accumulators
(`LogFlags`; the hook logs and does not change the force), and the base call `CEntity::E43Vf60` in Accum250.

Status (group C, constructors and the debug hook): load-time functions. The per-step recorder does not reach them,
so their records come from the load-time entry dumps (`RBR_SNAPAT`, a count run with `transplant.py --count`), replayed
by `vcu_vectors.py` (`--set physics_core`). The entry dumps were deleted from cachy after their records were written; rerun the count run for a new record. `vcu_vectors.py` takes an entry dump only as `--set entities`; the
recording used a local wrapper that registers this table as that set (`/tmp/pc-wrap/entry_vectors.py`, not committed,
see the report). Harness feature asked of the lead: let `--set <name>` take an entry dump directly.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `CPhysics::CState::CState` (`construct::new_state`, `layout::store_state`) | 2 (the dump's own, and the `Vf50` call's nested one) | the whole 0x4b8 image against the real pre-state | |
| `CPhysics::CDef::Vf50` (`construct::cdef_new_state`) | 2 (the dump's, and the `CPhysics` ctor's nested one) | the state object (`after` run) and its allocation | the allocation (`operator new`, 0x70bec2) is the caller's (trait `Heap`) |
| `CPhysics::Vf10` (`debug::is_idle`) | 1 (flags 0x6000: idle) | the idle path | the tool path (flags in `debug::TOOL_FLAGS`: keyboard orbit with sin/cos, the collision probe, wall visitors, draws) is not ported |
| `CPhysics::CScratch::CScratch` (`construct::construct_scratch`) | 1 (dump), 1 nested in the `CPhysics` ctor's record | the vtable word and the `Vf04` block at +0xe840..+0xe888 (`layout::clear_scratch_block`) against the real bytes, and the solver calls in order | external, caller's (trait `ScratchHost`): `CCollisionSolver::CtorSolver` (0x4fc930, E38) and the solver's `S08` (0x4fc0b0, slot 2 of `0x739978`, called by `Vf04`); the solver's bytes (+0x10..+0xc450) are not checked |
| `CPhysics::CScratch::Vf04` (`construct::scratch_vf04`) | 1 (dump), 1 nested | as above | as above (`S08` through the trait) |
| `CPhysics::CDef::Vf54` (`construct::cdef_new_scratch`) | 2 (the dump's, and the `CPhysics` ctor's nested one) | the block (`after` run) and the solver calls, through the scratch's bytes outside the solver | the allocation (`operator new`, 0x70bec2, not in the physics spec) is the caller's (trait `Heap`); the solver as above |
| `CPhysics::CPhysics` (`physics::construct_physics`) | 1 (dump), 1 nested in the `Vf4c` record | the object's window (0xf8) and the calls in order: `E37Prefill5340`, `GetNameOrNull`, the state and scratch factories, the pool allocation, `E21Stub::Hook` | externals, caller's (trait `PhysicsHost`): the prefill, the name, the hook, the allocations, and the five signal links into `g_signalManager` (`link`). Their words (the connections' next, prev and list: +0x68..+0x74, +0x80..+0x8c, +0x98..+0xa4, +0xb0..+0xbc, +0xc8..+0xd4) are not checked; the hook's own allocation (0x1e0, inside `Hook`) is dropped from the list |
| `CPhysics::CDef::Vf4c` (`physics::cdef_new_physics`) | 1 (dump), 1 nested | the object (`after` run, 0xf8) with the same exclusions as the ctor | as the ctor |
| `CPhysics::CDef::CDef` (`construct::construct_cdef`) | 1 (dump) | the def's words (the base, the NTweak tweaks at +0x16070..+0x16150, the three flag storages) | externals, caller's (trait `CDefHost`): `E39_DefMgrCtor` (+0x0c, 0xc bytes), `CMaterialManager` (+0x18, 0x1600c bytes) and the solver's `CDef` (+0x16024, 0x4c bytes); those regions are not checked  the solver's own calls (`E09Vec4::Clear`, seen flattened in a notify-run record) are the caller's, and the test cuts them at the solver's constructor |

Status (group D, load-time setup, `setup.rs`): the entry dumps come from a count run (`transplant.py --count` with
`RBR_COUNT=1 RBR_SNAPAT=<list>`, `harness/play.sh`); the dumps are kept only until their vectors are written.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `E46_CreateDef` (`setup::create_def`) | 1 (dump) | the calls in order (the def's allocation, its constructor, the slot 2 call with the argument, `E45_PhysUnk34`), and the global `g_165b200` | externals, caller's (trait `CreateHost`): the def's slot 2 (0x52f0c0) initialises the def's sub-objects, and its nested calls and writes are not checked; `E45_PhysUnk34`'s result is the mock's |
| `E46_InitPhysics` (`setup::init_physics`) | 2 (the first call, which creates the def, and a later one, which steps it) | the calls in order (strip, tokenize, the create or the step, free), and the globals `g_1659b6c` and `g_1659b70` | the script reader (trait `TokenHost`, `NTweak`) and the nested calls of `StepForward` and of `Tokenize` are the caller's; the tokenizer's buffers are the caller's allocations |
| `E46_AppInit` (`setup::app_init`) | 1 (dump) | the calls in order (the wall clear, the collision clear, the water reset, the vector clear, the token scratch init), the callbacks, and both registrar links into the signal manager (`layout::link_registrar`), with the manager's list words | externals, caller's (trait `AppHost`): `E37SigInit800` is a tail call, so its record is the file's own top-level record and the mock does not record it; the token scratch's allocations are the caller's |
| `E43HostRel::WallRel` (`setup::wall_rel`) | 1 (dump) | the calls in order (the target count lowered, `UnlinkA`, `RegWall` with the relay), and the count one less than the original's before `RegWall` | externals, caller's (trait `WallHost`); the target's count is not byte-checked (`RegWall` adds the relay back to the same list); the early return (no target) comes from a fuzz (`wall_rel_fuzz`: the relay's +0x18 zeroed on half the calls, `--fuzz`), since the first four recorded calls all have a target |

Status (group G, signal glue, `signal.rs`): `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580) is ported. Its
entry dumps come from a count run with `RBR_SNAPAT` listing `526580 4` (the first four calls). Those calls have no
listeners (the overflow list and the count's slot are empty), so the vectors check the windows, the guard and the count
only. The drive calls `Emit` 23723 times per playback (`RBR_COUNT=1` run), but `RBR_SNAPAT` takes the first N calls, so
the listeners are not reached. The list walk, the recursion and the out-of-range paths are unit tests in `signal.rs`.
Open: a way to dump the N-th call (or a start ordinal) would give vectors with listeners. Step snapshots (snapB 40/572/1295, vcu-cov) do not reach `Emit`: its only caller in the recordings is `CPhysics::Vf0c` (0x526a10), which runs between steps. The integration track can record it from the frame tick later; until then the listener walk is a gap, covered by the unit tests in `signal.rs`.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `CMultiSignal1<const CSigLocalTime&>::Emit` (`signal::emit`) | 4 (dumps, no listeners) | the guard, count and slot words, after the call | the listeners: externals, caller's (trait `SignalHost`: `Invoke` on each connection, and the two `E01Log` messages); the list walk, recursion and range paths are unit tests only |

Group G plan (step 1; calls per playback of tanner-corolla from a count run over all 54 listed functions, `RBR_COUNT=1`):

| Batch | Functions (calls per playback) |
| --- | --- |
| G1 (done) | `CMultiSignal1<CSigLocalTime>::Emit` 0x526580 (23723) |
| G2 (done) | flag getters: `CDef::CFlags` 0x523b90 (250), 0x523ba0 (20), 0x523bb0 (296), 0x523bc0 (389), 0x523bd0 (27), 0x523be0 (446), 0x523bf0 (49), 0x523c00 (20), 0x523c10 (80), 0x523c50 (1); `CDef::Unk20`/`Unk1C` 0x523b10 (112), 0x523b20 (100); `CDef::Vf58`/`Vf5c`/`Vf60` 0x523b30, 0x523b50, 0x523b70 (0 each: not reached); `CMaterial::CFlags` 0x520840 (544), 0x520860 (813); `CMaterialManager::Unk20` 0x5208d0 (536), `Tag520940` 0x520940 (306656) |
| G3 (done) | `E46FuncConn::NotifyPad` 0x527b40 (5), `NotifyExitGame` 0x527b90 (1): owner callbacks, cdecl |
| G4 (done) | `CMaterial::CMaterial` 0x520980 (264); `CMaterialManager::Tag520940` 0x520940 (306656, see the G4 status) |
| G5a (done) | mounted def and state constructors: `CDef::CtorA` 0x521480 (3), `NewDef` 0x521b50 (3), `CState::Ctor` 0x5215a0 (3), `CDef::Vf4c` 0x521500 (6) |
| G5b (done) | mounted body: `CtorBig` 0x521c30 (3), `CDef::Vf50` 0x521d20 (3), `E45_M1980` 0x521980 (3) |
| G6a (done) | passenger constructors: `CDef::CtorA` 0x522450 (8), `CDef::CtorB` 0x5224a0 (6), `CState::CtorB` 0x522550 (2), `CPassenger::Ctor` 0x5230d0 (2) |
| G6b (done) | passenger: 0x5233f0 (51491, `E45_P33F0`), 0x523010 (16277, `Emit`), 0x523210 (16277, `E45_P3210`), 0x523360 (16277, `M3360`), 0x5229e0 (9544, `M29E0`), 0x522bc0 (9544, `M2BC0`), 0x523160 (9544, `Step`) |
| G7 | `E45_PhysUnk34` 0x523a20 (1; already the caller's trait of `CreateDef`) |

The dumps of a function are its first N calls (`RBR_SNAPAT`), so the frequent functions are covered only at the start of
the stage unless their inputs repeat there. Functions with fewer calls than the dump count are covered in full.

Status (group G2, flag classes and mounted flags, `flags.rs`, `mounted.rs`): ported. The entry dumps are the first call
of each function (`RBR_SNAPAT <addr> 1`, count run). Their vectors check the return value (eax) and the windows.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `GetBitName` (`CDef::CFlags` 0x523b90, `CDbgFlags` 0x523bc0, `CLogFlags` 0x523bf0, `CMaterial::CFlags` 0x520840) (`flags::bit_name`) | 1 each | index 0 (the first call); the table words are checked | other indices: unit test (`flags.rs`) |
| `GetBitDetail` (0x523ba0, 0x523bd0, 0x523c00) (`flags::bit_detail`) | 1 each | index 0 | other indices: unit test |
| `GetBitCount` (0x523bb0, 0x523be0, 0x523c10, 0x520860) | 1 each | the constant | |
| `CDef::Unk20` (0x523b10), `Unk1C` (0x523b20), `CMountedRigidBody::CDef::Unk20` (0x521550), `CMaterialManager::Unk20` (0x5208d0) | 1 each | the address returned | `Unk1C` of the mounted def (0x521560) and `CMaterialManager::Unk1C` (0x5208e0): not reached |
| `E45_M1920` (`mounted::flag_is_one`), `E45_M1940` (`mounted::flag_is_two`) | 1 each | the flag at 0 (the first call) | states 1 and 2: unit test (`mounted.rs`) |
| `E45Smalls::M1910` (`mounted::set_state`), `E45_M1640` (`mounted::clear_state`) | 1 each | the object's bytes after the call | |

Not reached by the count run (no vectors, no port): `CDef::Vf58`, `Vf5c`, `Vf60` (0x523b30, 0x523b50, 0x523b70),
`CMaterial::CFlags::GetBitDetail` (0x520850), `CState::Unk20`/`Unk1C` (0x523c20, 0x523c30), `CMaterialManager::Rate`
(0x520920), `E45_MaterialCount` (0x5208f0), `E45_RandDir` (0x520be0). `E45_Rand01` (0x520bc0) and the two `DeleteThis` (0x520b10,
0x520ba0) are not in the count log at all.

Status (group G3, notify, `signal.rs`): ported. `NotifyPad` and `NotifyExitGame` are one port (`signal::notify_owner`):
the connection's owner (+0x10) is called cdecl with the signal. The owners in the recordings are `E46_InitPhysics`
(0x5266d0, the pad connection at 0x165a338) and `E46_Shutdown` (0x524c60, the exit connection at 0x165a39c).

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `E46FuncConn::NotifyPad` (0x527b40) | 2 (dumps, the first two calls) | the owner call (the owner the window names), the connection's bytes | the owner's own work: it is a listed function, so the recorder keeps its records as its own (`E46_InitPhysics`, checked in its group D vectors) |
| `E46FuncConn::NotifyExitGame` (0x527b90) | 1 (dump) | as above | as above (`E46_Shutdown` is not ported) |

Status (group G4, materials, `material.rs`): ported. `CMaterial::CMaterial` (0x520980, 264 calls per playback) is
straight-line: its 0x160 bytes are the CBase header, 19 `CReal` tweaks (16 bytes each from +0x0c), the `CUnsigned` (+0x13c)
and the flags storage (+0x14c). The recorded first call covers every word; the other 263 take the same path. The vtables
are 0x73d81c (`CMaterial`) and 0x73d7bc (its `CFlags`).
`CMaterialManager::Tag520940` (0x520940) takes its tag from a stack word: the recorder replaces it per record (`fuzz_args`), so the
288 tags of the table (0..287) are covered; `tag_entry` reads the flag byte (+0x164) of tags 1..255 only.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `CMaterial::CMaterial` (`material::construct`) | 1 (dump) | the whole 0x160-byte object | the other 263 calls (same path, no branch) |
| `CMaterialManager::Tag520940` (`material::tag_entry`) | 3 dumps (tag 0) and 288 (tags 0..287, `tag520940.txt`) | the tag-0 path; the tags 1..255 with their flag byte set and clear; the out-of-range tag; the manager bytes unchanged (the port reads only the flag byte, the record window is that byte) | the stack argument is replaced by the spec `fuzz_args` (`tag_args`, `harness/vector_specs/README.md`) |

Status (group G5a, mounted def and state constructors, `mounted_ctor.rs`): ported. `CDef::CtorA` (0x521480) makes the
solid base through the caller (`MountedHost::solid_def`, E47 0x529840, with the def's name), writes the vptr (0x73e59c)
and makes the tweaks: `unsigned` (0x52fec0 at +0x1f0 and +0x230) through the caller, and `vector` (0x52ff80 at +0x200) as
`ntweak::construct_vector`.
`NewDef` allocates 0x240 bytes (`Heap`) and runs `construct_def` on the block. `CState::Ctor` (0x5215a0) writes its
vptr (0x73e614) and makes three `real` tweaks (0x52fea0) at +4, +0x14 and +0x24, as `ntweak::construct_real`. `Vf4c` returns 0x73e5fc.

The solid base's and the `unsigned` tweaks' constructors are the callers' (`MountedHost`). The `vector` and `real` tweaks are
`ntweak`'s constructors, checked by their own records (`wheel_vectors`). The test checks the calls, in order, and the vptr;
the bytes the callers write (the solid base, the `unsigned` tweaks) are skipped, so they are not checked here.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `CDef::CtorA` (`mounted_ctor::construct_def`) | 3 (dumps) | the calls in order (the solid base with the name, the three tweaks) and the vptr | the bytes of the solid base and the `unsigned` tweaks |
| `NewDef` (`new_def`) | 3 (dumps) | the allocation, `CtorA` on the block, the block's `after` bytes (vptr) | the null path: unit test (`mounted_ctor.rs`) |
| `CState::Ctor` (`construct_state`) | 3 (dumps) | the calls in order (three `real` tweaks) and the vptr | the tweak bytes (`ntweak` records check them) |
| `CDef::Vf4c` (`vf4c`) | 3 (dumps) | the returned string | |

Status (group G5b, mounted body, `mounted_body.rs` and `mounted_pose.rs`): ported. `CtorBig` (0x521c30) builds the body on
the caller's solid base (`BodyHost::solid_base`, E47 0x52d480, with `d | 0x800`); the name (`base_name`, 0x52f0b0) when `a`
is set; the state (`state_ctor`, 0x5215a0, with `this + 0x3d4`, `this`, the name and `c`); the scratch base (`scratch_base`,
0x52b930) and its vptr (0x73e61c) at +0x410; a zero word at +0x584; the owner at +0x3d0 (`c` when its byte at +0x60 has bit
3, else 0); and `E43_ExclA000(2)` (`exclusion`, 0x51a000). `Vf50` (0x521d20) allocates 0x590 bytes and runs `construct_body`
with the def as `a`, `b` and 0.

`E45_M1980` (0x521980, `pose_update`) reads the state word through +0x580, calls `SetDirty` (0x52f450) on the base at
+0x1d0, sets the dirty flag at +0x1dc (1 when the state is set), has the owner compose the car's block (`CCar::BodyCompose`,
0x4e6a30, owner at +0x3d0 with car+0x210 and car+0x220), makes two `E01Vec4::CopyFrom` (0x43c5b0) to +0x140 and +0x330, and
copies 16 bytes from +0x100 to +0x2a0.

Recording notes: `CState::Ctor` is a listed function, so its call is its own top-level record, not an inner record of
`CtorBig`. The test's expected list adds it (`body_expected`). `CCar::BodyCompose` makes a vector copy before its own
record; that copy is the owner's and is dropped from the expected list (`port_calls`).

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `CtorBig` (`mounted_body::construct_body`) | 3 (dumps) and 3 nested in `Vf50` | the calls in order, the vptrs, the zero word, the owner word (`c`, the flag set in the recordings) | the bytes the callers write (solid base, state, scratch, exclusion) are skipped; the owner's 0 cases are unit tests (`mounted_body.rs`) |
| `CDef::Vf50` (`mounted_body::new_body`) | 3 (dumps) | the allocation, the body's calls (through `CtorBig`) | the null allocation: unit test |
| `E45_M1980` (`mounted_pose::pose_update`) | 3 (dumps, state 0) | the calls in order (`SetDirty`, `BodyCompose`, two `CopyFrom`), the dirty flag (0), the copy to +0x2a0 | the dirty flag for a set state (1): unit test (`mounted_pose.rs`), no recording has one; the callers' writes (the vectors at +0x140 and +0x330) are skipped |

Status (G-mounted, the mounted body's step, `layout::mounted_vf6c`, pure `math::mounted_step`): ported and bound.
`CMountedRigidBody::Vf6c` (0x521d90) on a body without an owner calls `CSolid::E43Vf6C`. With one, the linear velocity
takes the pull `Vf64` stores times `dt`, and the position the new velocity times `dt` (four lanes each); the gap to the
mount point is clamped per lane to `[-0.02, 0.02]`, `[-0.02, 0.005]`, `[-0.03, 0.01]` and added back to it. On the
body's first step (its event word is zero) the body event is emitted (0x51f7c0; the payload: 1, the key, the position
twice, the rotation twice) and the word set; then the state is emitted (0x51f700; the key, the position, the rotation).
The payloads sit in the original's frame (`((esp - 4) & !0xf) - 0xa0`), the emits enter at its `frame - 8`; the words
the asm leaves unwritten stay the stack's. Records: 50 direct calls (`harness/vector_specs/g_mounted.py`,
`physics_core_vectors::mounted_vf6c_matches`): 21 event, 19 state (every clamp bound hit), 10 unowned. `Vf138`
(0x521a50) is `SimE4F0`'s asm and binds to its port; `Vf128` (0x521b20) jumps to `CSolid::E43Vf128` without an owner and
returns the owner otherwise.

Status (group G6a, passenger constructors, `passenger.rs`): ported. `CDef::CtorA` (0x522450) makes its base through the
caller (`PassengerHost::base`, 0x52f090, with the name 0x73e824), stores its vptr (0x73e7d4) and makes its unsigned tweak at
+0xc (`unsigned`, 0x52fec0). `CDef::CtorB` (0x5224a0) stores the demoted words only. `CState::CtorB` (0x522550) makes its
base, stores its vptr (0x73e834), makes its real tweak at +0xc (`real`, 0x52fea0) and clears ten vectors (`clear`, 0x465d80)
in the original's order. `CPassenger::Ctor` (0x5230d0) stores the body's vptr (0x73e890), the scratch's vptr (0x73e884, at
+0x20), `b` (+0x14), `a` (+0x10) and `c` (+0x40), then runs the scratch's `Vf04` (vtable slot 1, 0x5224e0, `E45_P24E0`, a
no-op) when `a` is set.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `CDef::CtorA` (`passenger::construct_def`) | 3 (dumps, 8 per playback) | the calls in order (base with the name, unsigned at +0xc) and the vptr | the callers' bytes (base, tweak) are skipped |
| `CDef::CtorB` (`passenger::demote_def`) | 3 (dumps, 6 per playback) | the four stores | |
| `CState::CtorB` (`passenger::construct_state`) | 2 (dumps, 2 per playback) | the calls in order (base, real, ten clears) and the vptr | the callers' bytes are skipped |
| `CPassenger::Ctor` (`passenger::construct_body`) | 2 (dumps) | the stores and the `Vf04` call (`a` is set in both) | `a == 0` (no `Vf04`): unit test (`passenger.rs`) |

Status (group G6b, passenger flag, incar shake, time decode and spring, `passenger.rs`): ported `flag_67c_set`
(0x5233f0: `obj != 0` and the `Get67C` call, cdecl with self on the stack), `incar_shake` (0x523210: `Clear6` of the
payload, the host's vectors copied in, then `signal::emit` through the Emit at 0x523010), `decode_time` (0x523360: sign,
`ftol` of |v|, days, hours, minutes, seconds, the fraction at +0x10) and `spring` (0x5229e0: per lane,
`f = 900*(h8 - h16) + 30000*(h28 - h36) + 10*v`, `h8 += f*k`, `h28 += h8*dt`, `k = -0.1*dt`, on the host's words). `CPassenger::Damp` (0x522bc0, `M2BC0`) and `Step` (0x523160) were left out of this batch and are ported since (the
passenger step below). At this batch, Damp's source arithmetic did not reproduce the recorded state (packed 4-lane SSE
with a shuffled lane layout; lane 2: recorded delta 0.0687, source 551). Step's dump is only the entry for the spring's
records.

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `E45_P33F0` (`passenger::flag_67c_set`) | 31 (dumps and the flag fuzz: outputs 0, 1 and random) | the `Get67C` call with the object and the result | the `obj == 0` branch: the recorder does not reach it (unit test: none yet) |
| `CIncarShake::Emit` (`passenger::incar_shake`, `Emit` 0x523010 in the set) | 41 each (`g6b-shake.txt`) | `Clear6` (six vector clears, then the clear at the payload), the payload words 0..24 (the host's +0xbc is word 23, word 24 stays 0), the invoke sequence | the listener 0x7583cd0 is not recorded, so its invokes are excluded in the test (`UNRECORDED_LISTENER`); the copies (`CopyFrom` 0x43c5b0) are externals and their writes are checked through the payload |
| `E45_P3210` (`passenger::incar_shake`) | 41 (`g6b-shake.txt`) | the call sequence and the payload (above) | the payload words 1..3 come from the Emit's pre-window only |
| `E45PSmalls::M3360` (`passenger::decode_time`) | 61 (`g6b-time.txt`, log-uniform 1e-3..1e7, both signs) | sign, days, hours, minutes, seconds, the fraction; the CRT `_ftol2` is `math::ftol::ftol2` (pure, no host call) | values outside the fuzzed range are not recorded |
| `E45PSmalls::M29E0` (`passenger::spring`) | 41 (`g6b-spring.txt`, `spring_fuzz`) | the spring forces and damping on lanes 0..2 | lane 3 (host +0x2c and +0x7c) is written by the original from stack garbage; the port does not write it (`host_skip` in the test) |

Vf10 sin/cos: not in the ported path. The orbit runs only under the flag `0x2000000`, which the drive does not set
(DbgFlags 0x6000 at load, `c-00524f80`). Open precision item: the orbit's `sin`/`cos` (CRT, x87 `fsin`/`fcos`
results) would need the x87 model of `notes/rust-physics.md` for that path; not started.

Status (group E, detectors, `detect.rs` and `layout/detector_calls.rs`): all eleven are ported. These four came first. `E45_M40C0` (0x5240c0, `detect::box_cells`,
9 calls per playback) and `E45_M46A0` (0x5246a0, `detect::e45_m46a0`, 12 per playback) are reached by the recorded steps.
`CL2PenetrationDetector::Ctor` (0x5235b0, `detect::l2_penetration_ctor`) and `E45_MD60` (0x523d60, `detect::build_cell_grid`)
are not reached by any recorded step: they are vector-tested on direct calls only (`harness/direct_calls.py`, records
`d-ctor-*` and `d-md60-*`), so they say nothing about the drive's values.

| Function | Calls | Covered | Not covered |
| --- | --- | --- | --- |
| `E45_M40C0` (`detect::box_cells`, cell lookups of a box's corners) | 171 (45 plain, 126 widened, the flag 0x2000 of the CDef) | the four lookups in order (`LookupTwin` inner records), the duplicate clearing, both branches (`--fuzz` flips the flag) | none |
| `E45_M46A0` (`detect::e45_m46a0`, edge-pair sweep) | 324 (126 empty, 54 null, 144 swept with hits and misses) | the null list, the count-0 path, the sweep with `EdgeEdge` hits (`--fuzz` draws the outputs and the count; the ids stay zero) | the nonzero id return: see below |
| `E37GridOps::EdgeEdge` (external, `EdgeHost`) | inner records of the sweeps | the outputs (the windows of the record) | its own code |
| `CL2PenetrationDetector::Ctor` (`detect::l2_penetration_ctor`, direct calls) | 24 (12 at snapshot 1000, 12 at 2000) | the object's stores, `E38RaySeeds` at +0x30 (its recorded bytes), the two `CopyFrom`s (the port's copy of the recorded sources), the return | `E38RaySeeds`'s own code and `Vec4::Clear` (inside it), the callers' |
| `E45_MD60` (`detect::build_cell_grid`, direct calls) | 48 (24 built, 24 present: 16 built from QueryWall's corners, 8 from the default bounds when QueryWall answers 0) | both gates, the walls' width and height (`max - min`, f32), the defaults `-5200`, `-7200`, `10000`, `15000` (checked as the recorded `CCellGrid` arguments), the stored grid | `operator new` failing (answers 0 in no record) |

`E45_M46A0` returns the id of the best hit (`*ids`, the asm stores it at esp+0x24, the return slot). The recorded lists have
zero ids (the drive's lists are empty, and the fuzzed lists keep zero ids), so the vectors check 0 only. A nonzero id is
not checked by the vectors: the step's wheel code (`RBR_E51_WheelD.cpp`, the callers) uses the return as a pointer, so a
fuzzed nonzero id faults the fuzzed step, and the harness has no direct-call path with the EdgeEdge result. The unit test
(`detect::tests::sweep_keeps_the_nearest_hit_below_the_start`) checks the id of the best hit.

The detector group (`detectors.rs`, the sequences in `layout/detector_calls.rs`) is ported and bound: `CL2GiganticPenetrationDetector::Vf04`
(0x5242c0, `giant_penetration_detect`), `E46L1PenDetector::Vf04` (0x525a20, `l1_penetration_detect`), `E46_Kernel5bd0`
(0x525bd0, `kernel_collect`), `CL1CollisionDetector::Vf04` (0x526ad0, `l1_collision_detect`), `PenCheck` (0x526cc0) and
`CollideWork` (0x526d20). The detectors are stack objects the entity walk and the cells' `NotifyFiltered` call back, so
the frames are the original's (`detector_frame`: `and esp, -0x10`, the SEH record, the locals, three saved registers) and
every call is the host's at the asm's entry esp (`DetectorHost`, `CollideHost`). `E33Mat34::Combine`/`MD70` (pair
stores), `E37Get67C` (the grid word 0x67c) and `CopyFrom` are inlined; the frame words nothing passes by address (the
cookie, the SEH record and state, saved registers, temporaries) are not written. The kernel leaves lane 3 of its two box
corners as the stack had it, as the original does: the entities' `SCC` copies it and compares only xyz
(lead/reports/rust-collision-handover.md). `CollideWork`'s step branch (a solver fraction at or below 1.1) is reached only
on the damage drive (`vcu-tanner-damage` snapshots); its hit counts (CDef flag 0x20000), the kernel's eight-entity cap and
the detectors' early gates are unit-tested only (`tests/detectors.rs`).
Ported since in the same way (`layout/detector_calls.rs`, unit-tested in `tests/detectors.rs`): `E45PhysBody::M4250`
(0x524250, `body_pair_detect`, not bound), `CL2PenetrationDetector::Vf04` (0x524480), `E46L2Detector::Vf04` (0x5257e0); and in
group F `Work6a70` (0x526a70, `run_collision_detectors`) and `Visit470` (0x526390, `visit_list470`), bound. `CState::RegWall` (0x5263d0) is `layout::entity_list` and `push_item`;
`E46_Notify67d0` (0x5267d0) is `layout::notify_stuff`, with `E39_EntfCreate` (0x502070, `layout::entf_create`) and
`CEntity::E43Vf90`/`E43Vf94` (0x519a60/0x519a80, `layout::entity_rel`); its object's virtual calls go through
`NotifyHost` (the stage's host answers them for `CSolid`; a dynamic placement, `E37SigEmit9B30`'s signal, and the debug
branch are `Err`), checked on the stage's 3,138 placements (`load_stage`). The car's placements (the car, flags 3; its
wheels and bodies under it; the car body last, flags 5) take the same paths with another host: the parent's slot 0x150
and slot 0x90 are not reached on the stage.

No direct recipe exists for the Vf04 detectors and M4250: it would need each detector object's layout (its owner, the
vectors, the solver and its state). Their checks are the unit tests and, for the bound ones, the all-bindings runs.

Status (group F, contact step, `contact.rs` and `physics.rs`): `Work5500` (0x525500, `contact::step_bodies`) is reached once per playback
(`StepBodies`, list 450) and ported. The nine snapshots hold one node each, so the vectors check the list walk with one
node and the windows (the callees write nothing the windows hold, in these records); the three passes over several
nodes are checked by the unit test (`contact::tests::three_nodes_in_three_passes`). The records are a flat trace (the
node calls and their nested calls in call order, with no depth), so the four slot numbers come from the asm only.
`Work5640` (0x525640, `layout::advance_contact_bodies`) and `Work5760` (0x525760, `layout::advance_contact_bodies_remainder`) walk the scratch's event list
(the scratch at the physics' field 0x28, its list head at field 0x14, the links at field 0xc). Each solid of an event
(fields 0xa0 and 0x98) gets the slot 0x80 and, when it answers and its flag byte (field 0x60) has bit 4, `VfDx`
(`Work5640`, with the event's `Get94`) or `VfEx` (`Work5760`, with the window, for an event below 1.0). The asm (llvm-objdump) fixes the order and the gates; the float argument of `Work5640` is
unused. The `Get94` call is the ported `collision::sort::event_get_94`; the slot 0x80 is the solid's own, and `VfDx`
and `VfEx` are host calls. Not reached by the 30-step chains (10 snapshots); reached on the tanner-damage drive (snapshots
11820 and 12000: 6 and 5 calls each), where they run equal (see `rbr-hybrid/bound.csv`).

`StepForward` (0x5273a0, `layout::step_forward`, bound in `rbr-hybrid`) now runs the substep loop and its original
call order. It calls the Rust `SetDirty` and `Work5500` ports directly and dispatches the remaining reached callees
through typed host methods at the original stack depths. The clock rounds each `elapsed + base` sum to `f32` before
widening back to `f64`; its profiler samples use the shared `scope()` helper.

The unit-only `EDGE_START_DIST` (1.01) is the sweep's start: a hit replaces the best only when it is closer than that.

## Unknown meaning

- `CPhysics`: the log storage at +0x2c (0x14 bytes; a vtable `0x73f6cc` and a name `0x7404ac`, then zeros) and the function pointers at +0x44..+0x50 (`0x73cbe8`, `0x5236e0`); the connections' callbacks are `0x523d40`, `0x5267d0`, `0x5269e0`, `0x5272e0`, `0x527330`, `0x523e80`. Not named.
- `CPhysics` +0x10 and +0x58 are not written by the constructor.

- `CState` `NTweak` headers: the name words (`0x73f7b8` "Physics" for the state's CBase, `0x740580` and `0x740528` for
  the `CUnsigned` and `CReal` headers) and the vtables (`0x740534` `CUnsigned`, `0x73b644` `CReal`, `0x73f884` the
  state) are the addresses the original stores. The name words are the literal's address by the source's
  `CBase("Physics")`; the others are not checked against the .rdata strings.
- `CObjIdQueue` head words (`ObjIdQueueHead`): the vtable `0x7404dc` and the word at +0x2c (`0x73e920`) are stored by
  the state's constructor; their meaning is not known.
- `CState` +0x34..+0x3c, +0x43c and the value word of `m_r490` (+0x49c) are not written by the constructor.
- `CSolid` +0x1dc and +0x1ec (`cache_1dc`, `cache_1ec`): the words `E43Vf80` and `E43Vf8C` return; they are
  cleared with the body's dirty flags (`notes/physics/solid.md`, `Vf37x`), and nothing else is known.
- `E46Mat` convention: `Mat::mul_rows` and `Mat::scale_rows` use row vectors, `Mat::transform_xyz` column vectors, on
  the same `m` layout. Open question 2 of `notes/physics/physics-core.md`; the names above are the asm's operations.
- `Mat::scale_rows` lane 3: `x*m3 + y*m6 + z*m9` (flat stride 3), kept as written.
- `E46_IdxWrite` input `out[0]`: the search hint. The port takes it as `hint`; the original reads it from memory and
  writes `lo` back, so the caller's previous answer changes the walk (not the result, only the path).

## Platform: default NaN

Batch 1 has no tolerated NaN site: the tests compare the written bytes exactly, and the recorded calls match. The
NaN path of `math::table::table_index_frac` (value NaN) is not in the recordings. The test's index closure returns
NaN for an index outside the table; that only happens on a NaN value (the walk reads `hint - 1`), and the NaN propagates
from the value, so its sign follows the input, not the default NaN.

## Raw bits and float-to-int

None in batch 1: no float is read as an integer, no `as` cast from a float. `table_index_frac` compares and divides
floats; `layout::set_table_index` stores the i32 indices as their u32 bits (integers, not floats).

## Ported-as-asm notes

- `Mat::mul_rows`: each output sum takes one of two orders (`(p0 + p2) + p1` or `(p2 + p1) + p0`), chosen per output
  element, as the C++ expressions (verified by the harness) write them. Kept per element (`SUM_FIRST_PAIR_IS_X_Z`).
- `Mat::transform_xyz`: each sum is `(M[r][1]*y + M[r][2]*z) + M[r][0]*x`, the order of the C++ and the asm. The C++
  locals are named x, y, z from base[1], base[2], base[0]; here they are the Vec4 fields (x = base[0]).
- `Mat::scale_rows`: the stride-3 rows (`stride3_rows`) are the flat-matrix loads of the asm, and the 4th lane of each row
  is the next row's first float (`extra` for the last row).

Status (group G6b, passenger step, `passenger.rs` and `layout/passenger_step.rs`): ported and bound with the spring (`M29E0`). `Step`
(0x523160) walks the gravity point through `E43PairWalk::MB560` (the walker `g_7cbb00`, the point in a temporary), then
calls the spring and `M2BC0` (0x522bc0, the pull: `pull_to_rest`) through the host, so their bindings run inside the
step. The original's gravity pointer sentinel (-0x160a0 in the CDef's place) makes `Step` read address 0; the binding
declines before any call. Lane 3 of the work vector is 0 (C++); the original's is stack garbage, and the lane-3 words it
reaches are in `harness/diverge.py`'s mask. `M29E0` returns the eax the original leaves (host +0x70).

| Function | Records | Covered | Not covered |
| --- | --- | --- | --- |
| `Step` (`passenger_step`) | 9 snapshots x 30 steps (`rust_step`, all bindings) | the walk, spring and pull calls in order; run B equal apart from the excluded bytes | the sentinel path: no recording has it (decline only) |
| `M2BC0` (`passenger_pull`, `pull_to_rest`) | as above; the step's own calls | the dead band, the sign rule, the update (`pull_to_rest_drops_a_pull_against_the_dead_band`) | the other dead-band zones: unit test `dead_band_keeps_only_the_part_past_its_zone` |
