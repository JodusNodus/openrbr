# Entities (exe E43 except the engine sims, E44)

Source: `src/rbr/RBR_E43_*.cpp` (not `RBR_E43_Engine*`) and `src/rbr/RBR_E44_*.cpp`. Notes:
`notes/physics/entities.md` (frames, CEntity), `ground.md`, `jointed-bodies.md`, `rigid-body.md`,
`tweak-keys.md`, `notes/physics/renames/entities.csv`. The asm (not the C++) decides operation order and
precision; `notes/rust-physics.md` has the x87 model.

## How each function is checked

- `harness/vcu_vectors.py --set entities SNAPSHOT OUT` records every call of a listed function during
  one StepForward of the original (from a `RBR_SNAPSHOT` dump), or, given a load-time entry dump
  (`RBRRecompSnapAt-<addr>-1.bin`, `harness/replay.py`), the one call that dump starts. Windows are the
  bytes the call reads and writes (`this`, pointer arguments), before and after. Records from an entry dump
  are the original's own run (not ours), so the B1 vectors come from the 12 load-time dumps.
- `tests/entities_vectors.rs` replays the records through the port (`RBR_ENT_VECTORS=a:b:c`), checking
  eax (where the original returns one) and every window. The port's typed functions are called through
  `layout` (the load/store layer), as the VCU's tests do.
- A function with no record has no evidence. Load-time dumps do not work for `CFrame::CFrame`
  (0x51af70) and `CGround::Vf88` (0x51bb60): the entry stubs are installed and the calls are counted, but
  no dump is written, even with the function alone in the list, while another listed function dumps in the
  same run. The capture trace (`RBR_TRACE`, shadow-eligible leaves, original only) does record them:
  - `CFrame::CFrame`: 40 calls, one stack argument (ret 4) that is 0 in all of them. Each call writes
    vtable 0x73ce4c, parent 0 and depth 0 at this+0/+4/+8, and returns this in eax. Only the null-parent path is
    covered; the `Frame::under` depth for a non-null parent is unverified.
  - `CGround::Vf88`: 2 calls, no stack argument, eax 1, no writes: consistent with `present()`.

## Plan

Reached in the drive (`notes/progress/called.csv`): 106 in scope. Batches, smallest first:

| batch | functions | what it needs |
|---|---|---|
| B1 (done) | 14: `E44MatTable::Build`, `E44_FillOnes`, hinged and ground CDef constants, `CMaterial::Unk20`, the ground flag slots (`Vf88`, `Vf110`, `Vf114`, `VfCC`), `CHingedRigidBody::Set650`, `E44Shell::M51C8C0`, `CFrame::CFrame` | window records only |
| B2 (done apart from the gaps in "B2 gaps") | frames and hooks (13: `CEntityFrame`/`CEntityOldFrame` ctors and `Vf04..Vf18` hooks, `E43_Tail40` (gap), `E43PairWalk::MF8B0`), matrix pair walkers (`W9D0`..`WB410`, `MD50`/`MD70`, `MB560`, `MatrixC` compose/fetch: about 23) | slot dispatch recorded as inner calls (`external` entries) |
| B3 (done apart from the gaps in "B3 status") | CEntity tree: walks (`E43_Walk1F0`, `Vf20`, `Vf58`..`Vf6C`, `Vf40`, `Vf48`), child list (`Vf150`), exclusion set (`ExclF80`, `ExclA000`), ctor/dtor, def (about 23) | the same, plus allocation |
| B4 (done, see "B4 status" and "B4b") | CGround: `CDef` ctor, `GroundDefNew9`, `Def::Vf50`, `VfE8`, `CollideInit` (175 lines), and `FetchX` (its matrix copy and walk) | window records; the NTweak ctors are four-word writes; the host and the grid are external |
| B5 (in progress, see "B5 status") | CHingedRigidBody sims: `SimA`/`SimB` (16 functions), states, defs, signals (`CMultiSignal1::Emit` x3), `E44Entry8::Pick`, `LinkC`, `ConnInvoke` | x87 model, sin/cos (`0x53f3a0`, `0x53f3b0`), signal emitters |

Not yet reached by the drive (no evidence, not in this plan): the `RBR_E43_Engine*` files.

## Notes for the port

- `CEntity::E43Vf150` (child append): the asm writes the list head only when the list is empty, and the C++
  on main does the same (`RBR_E43_EntityB.cpp:385-387`). The old 5a note (C++ writes it every time) is stale.
  The B3 vectors agree: 4 load-time appends to one parent, 2 onto a non-empty list, keep the head.
- `E44MatTable::Build`: the original stores the tag's low byte in a stack slot and reads back the dword
  (asm 0x5201fa-0x520205), so bytes 1-3 of the read come from the stack. They are not stale: the function
  stores zeros there itself before the loop (0x5201c2, 0x5201c6, 0x5201ca, `movb %al` with eax = 0), and
  the entry dump's bytes there are zero too. So the port's zero-extension matches the original, and the
  C++ agrees on the table writes (checked against 12 load-time dumps, see the B1 record test).
- `E44MatTable::Build`, the stack slot above the call: the original's loop walks the argument slot
  (E+4) from `arg+0x300` to `arg+0x32c` (set at 0x5201ce, incremented at 0x52020c). The C++ leaves other
  bytes there (`0x10004115` vs `0x01a4f5ac` in the 00520190 dump). Its only caller (0x4689de, a virtual
  call with one pushed argument) does not read the slot after the call: ABI-only.
- Scratch registers the C++ leaves different: `E44Shell::M51C8C0` returns a different eax (original
  `0x01a4fbe0`, C++ `0x0a27b310`); `E44MatTable::Build` leaves ecx and edx different. Checked against the
  18 call sites of `M51C8C0` in the original: each overwrites eax, ecx or edx before any read, and reads only
  the output buffer. The `Build` caller reads only `esi`, which `Build` preserves. ABI-only.

## B2 frames

- Ported: the 12 frame hooks (`CEntityFrame::Vf04..Vf18`, `CEntityOldFrame::Vf04..Vf18`, `Hook` One to Six).
  An entity frame calls the owner's hook (X form); an old frame calls the owner's gate (slot 32) and then
  X (gate 0) or Y. The port returns `a`, as the original does. The ports call the owner through the `Owner`
  trait; `layout::entity_hook_at` and `layout::old_hook_at` load the frame.
- Records: `harness/vcu_vectors.py --set entities` on the reference-drive steps 40, 572 and 1295 (the
  `snapH` dumps). The recorder reads each frame's owner and vtable slots at entry (`dispatch`), records the
  owner's hook as an inner call. The test checks eax, the windows, the inner calls' arguments and that the
  port makes exactly the recorded calls.
- Not checked: which slot and form the original called (a record has only the arguments and the answer).
- Calls checked (steps 40, 572, 1295): `CEntityFrame` `Vf04` 1,576, `Vf08` 70, `Vf0C` 1,774, `Vf10` 1,124; `CEntityOldFrame`
  `Vf04` 342, `Vf08` 68, `Vf0C` 100, `Vf10` 68. All match.
- No records at these steps: `CEntityFrame::Vf14` (0x5195f0, since bound and identical in the all-bindings run), `Vf18`
  (0x519620), `CEntityOldFrame::Vf14` (0x519810) and `Vf18` (0x519860). The same port covers them; the last three are
  `unit` or have no `ports.csv` row.
- Constructors (`CEntityFrame::CEntityFrame` 0x5194b0, `CEntityOldFrame::CEntityOldFrame` 0x519650), in
  `layout::construct_entity_frame`. The root case (no entity: the parent is the identity object at 0x7cbb00,
  depth 1) is verified: 5 calls each, from 5 load-time entry dumps each; all windows match and the port makes no
  owner call. The entity case is unverified: no recorded call has an entity.
- Unknown meaning: `Hook::One`..`Six` (their owner slots are 39..48 and 41..50 for the X and Y forms; the
  base `CEntity` returns the first argument, subclasses may override them).

## B2 walkers

- Names (`walk.rs`): a walker is named for the hooks it calls and the depth relation it handles, not its address.
  `PairOneTwo` (`W9D0`), `PairThreeFour` (`WA20`), `PairFiveSix` (`WA70`); `...Deeper` and `...Shallower` are the
  receiver's depth against `o2` (`WFE0`, `WB050`, `WB0C0`, `WB130`); `RoutePairOneTwo` (`WB320`),
  `RoutePairThreeFour` (`WB370`), `RoutePairFiveSix` (`WB3C0`) are the dispatchers. The hooks are `frame::Hook`
  (numbered by the frame slot, as in the frame ports; their meaning is only known as far as the frame ports say).
- Ported: `PairOneTwo`, `PairThreeFour`, `PairOneTwoDeeper`, `PairOneTwoShallower`, `PairThreeFourDeeper`,
  `PairThreeFourShallower`, `RoutePairOneTwo`, `RoutePairThreeFour`, and `mb560` (`MB560`: a 16-byte copy from `a`
  to `c`, then `RoutePairThreeFour` on this and `b`). Each walker calls hooks on its nodes and walks its children
  through `Walk`; the port reads the node depths and child links itself (`layout::load_node`). `Walk` is what the
  caller supplies: the reads, the hook calls, the child calls and the copy. The test answers the calls from the
  record and does not recurse.
- Records: `harness/vcu_vectors.py --set entities` on steps 40, 572 and 1295 (the `snapH` dumps). Each walker
  has a `direct` spec: its inner records are its own direct calls (its hooks and its child walks), and the child
  walks are also kept as records of their own. A `nodes` spec windows the two `+4` chains of the walk's nodes, so
  a record holds the whole walk's input. `pair_walkers_match_original_calls` and `mat_walk_matches_original_calls`:
  16,094 pair-walker calls (`W9D0` 140, `WA20` 140, `WFE0` 3,577, `WB050` 92, `WB0C0` 3,195, `WB130` 2,158,
  `WB320` 4,269, `WB370` 2,523) and 2,314 `MB560` calls match, with eax, the call sequence and the windows. A flipped gate fails them.
- Not checked: the hook targets (the vtable is not in a record's windows), and the child walks' own memory
  beyond the windows of the walk. `MB560`'s `c` is written after the copy by the walk's hooks (through their `a`,
  which is `c`), which the port does not model: the test checks the copy itself and the other windows.
- Ported earlier: `MD50`, `MD70` (`store_pair`), from the capture trace (only leaves are capturable there).
- Ported: `MF8B0` (`walk::lookup_factory_name`, 0x5198b0): the factory count loop, `CDictBase::Find`, the append to the
  walker's name list and the cursor advance. Its records are 16 load-time calls (count-run entry dumps), all of which
  find the name, so the not-found path (no append) has no record. The test checks each table call's target and
  arguments and the list entry; it skips the cursor window's post-state, which the advance writes in the original's
  callee. Find's output index (through `&n` in the C++, `RBR_E43_Misc.cpp`) is not recorded, so the test takes the
  stored index from the walker's after-state. The C++ stores the same `n`.
- Ported since (B4, `walk::run`): the Five-Six family (`PairFiveSix` `WA70`, `RoutePairFiveSix` `WB3C0`, the depth
  variants `WB1A0` and `WB210`) and the grid family (`WAC0`, `WB280`, `WB2D0`, `WB410`, with `E37Walk5E40` and
  `E37Walk5EB0` as `walk::grid_hooks`). See the B4 status table. `MB5A0`..`MB960`: not ported.
- Not reached by any step at the time (no `called.csv` entry; `WB210` is reached in the all-bindings run since): `WB280`, `WB2D0`, `MB5A0`, `MB640`, `MB690`,
  `MB700`, `MB740`, `MB7B0`, `MB7F0`, `MB830`, `MB870`, `MB8B0`, `MB8F0`, `MB930`, `MB960`.

## B2 gaps

Ported and checked: the frame hooks, the root case of both entity-frame constructors, the pair walkers (the Five-Six
and grid families since B4), `MB560`, `MD50`/`MD70` and `MF8B0`. Still open:

- `E43_Tail40` (0x5194a0, the functor callback the `CEntity::Vf40` walk stores, which tail-jumps to `child.Vf40()`):
  ported inside its caller, the unary functor `CPlainUnaryFunction::Vf04x` (0x51a120), as a plain call of the child's
  slot 16 (`functor::call_stored`, which also covers `Vf00x`, 0x51a130). No records: status `unit`. `Vf04x` is bound
  (identical in the all-bindings run); `Vf00x` is not bound.
- `CEntityFrame::Vf18` (0x519620) and `CEntityOldFrame::Vf14`, `Vf18` (0x519810, 0x519860): no record in the drive's
  entity dumps, so their rows are `unit` or missing (the test passes vacuously).
- `WAC0`, `WB280`, `WB2D0` and `E37Walk5E40`/`5EB0`: ported, but no all-bindings snapshot calls them (`unreached` in
  `bound.csv`); direct-call records only.
- `MB5A0`, `MB640`, `MB690`, `MB700`, `MB740`, `MB7B0`, `MB7F0`, `MB830`, `MB870`, `MB8B0`, `MB8F0`, `MB930`, `MB960`:
  not ported, no records (no `called.csv` entry).
- The plan's `MatrixC` compose and fetch functions: `FetchX`, `FetchY`, `ComposeB` and `CombineTwin` are ported (B4);
  the rest are not.
- The owner's slot and form for each frame hook: not checked (a record holds the arguments and the result only).
- The entity-owned constructor case, and `MF8B0`'s not-found path: no records.
- Hook targets of the frames and walkers: not checked (the vtable is outside the records' windows).

## Names and the load/store layer

- `layout/` is the only code that reads or writes game memory (offsets, the vtable word, the stores in
  the original's order). The port modules take and return typed values: `Frame`, `MaterialEntry`,
  `MaterialTable`, `MaterialSlot`, `[f32; 4]`, `[u32; 3]`.
- Constants are named by meaning, the address in a trailing comment. Names come from
  `notes/physics/renames/entities.csv` and the notes: `Frame.parent`/`depth` (`m_frameParent`,
  `m_frameDepth`), `DEF_NAME` (the class-name strings, `DefGround` and `DefHingedRigidBody`), `TWEAK_DICT`
  (the class's tweak-key dictionary, `tweak-keys.md`), `componentwise_mul` (`M51C8C0`).

### Unknown meaning (working names)

- `MaterialEntry.tag`, `low`, `high`: the source record's per-entry fields by position (`src+0x200`,
  `src+0`, `src+0x300`). Their meaning is not in the notes.
- `MaterialTable.self_ref` (+0x800, the table's own address) and `count` (+0x804, the source count).
- `layout::set_hinge_link` (+0x650, the `CHingedRigidBody` pointer; a store, no port logic): read by
  `SimEAD0` (early return when set) and `SimFB40`; its meaning is unknown.
- `hinged::unit_triple` (`E44_FillOnes`): three words set to 1; the meaning is unknown.
- `frame::Hook` One to Six: the owner's hook numbers (see B2 frames).


## B3 plan (CEntity tree)

Functions (asm addresses; "drive" = in `notes/progress/called.csv`, tanner-corolla):

| group | functions | evidence | what it needs |
|---|---|---|---|
| G1 leaves | `AppendChild` (`Vf150`, 0x51a370, drive), `ExclusionTest` (`ExclF80`, 0x519f80, drive; recurses on the parent), `AllocExclusion` (`ExclA000`, 0x51a000, drive; `operator new` at 0x70bec2 is external), `E43EntityDef` ctor/dtor (0x51a530/0x51a560, drive; `CBase` ctor 0x52f090 external) | steps 40/572/1295 | external heap and CBase calls answered from the record |
| G2 gated walks | `WalkChildrenS20` (`Vf20`, 0x51a3a0), `Vf28` (0x51a3e0), `Vf48` (0x51a440), `Vf58` (0x51a250), `Vf5C` (0x51a290), `Vf64` (0x51a2c0), `Vf68` (0x51a2f0, not in the drive), `Vf6C` (0x51a330, self `NameWalk` first), `NameWalk` (`Vf40`, 0x51a4b0; `Unk44` on the state is external), `WalkChildrenPre` (`E43_Walk1F0`, 0x51a1f0; the functor call is a vtable call) | steps 40/572/1295 | the gate (S80, slot 32) and the child calls as inner records, the walkers as direct specs (as the pair walkers) |
| G3 lifecycle | `CEntity` ctor (0x51a640), dtor (0x51a810), `DetachFromParent` (`Vf98`, 0x51a7c0; no drive record), `ExclRemove` (`ExclA060`, 0x51a060; no drive record) | load-time dumps (`RBR_SNAPAT`, `harness/play.sh`) for the ctor; the dtor is outside the drive (stage teardown) | needs the entry dumps; gaps otherwise |

Working decisions:
- The child-list head (`Vf150`): the asm writes the head only when the list was empty (0x51a37a-0x51a38d); the
  C++ on main already does the same (`RBR_E43_EntityB.cpp:385-387`, an `else`). The 5a note in
  `notes/physics/entities.md` is stale; the B3 vectors check it.
- Names come from `notes/physics/renames/entities.csv` (`AppendChild`, `WalkChildrenPre`, `WalkChildrenS20`,
  `ExclusionTest`, `AllocExclusion`, `RemoveExclusion`, `DetachFromParent`, `NameWalk`). The other gated walks take
  the step's pass names (`step.md` 1c): `Vf58` is the car hook pass (S58), `Vf5C` (S5C) and `Vf64` (S64) the force
  and torque passes, `Vf6C` the integrator pass (S6C). Unknown meaning: `Vf28`, `Vf48`, `Vf68`.
- Records: `harness/vcu_vectors.py --set entities` on snapH steps 40, 572, 1295. Walkers are `direct` (their gate and
  child calls are their inner records, the child walks are also records of their own), the child list is a `nodes`
  window (each child's +0x10 link). The ExclF80 parent recursion is a direct call too.

## B3 status

Checked (vectors from snapH steps 40/572/1295 and load-time entry dumps, `RBR_ENT_VECTORS`):

| function | port | records | branches seen |
|---|---|---|---|
| `E43Vf150` AppendChild | `tree::append_child`, `layout::append_child` | 4 (load dumps) | empty list (1), onto a list (3) |
| `E43_ExclF80` ExclusionTest | `exclusion::exclusion_test` | 4578 | parent refuses, same entity, member found, not found is unit-tested only |
| `E43_ExclA000` AllocExclusion | `exclusion::alloc_exclusion`, `layout::alloc_exclusion` | 1 (load dump) | allocates; has-a-set is unit-tested |
| `E43EntityDef::E43EntityDef` | `layout::construct_entity_def` (base: `layout::construct_base`) | 1 (load dump) | the root-list case; the base's writes (vtable, name, zero at +8) are in the windows |
| `E43EntityDef::~E43EntityDef` | `def::release`, `layout::release_entity_def` | 1 (load dump) | list 0 only: no unlink (every load dump has list 0) |
| `E43Vf20` WalkChildrenS20 | `tree::walk_gated` | 294 | children of several depths, gates 0 and 1 |
| `E43Vf48`, `Vf58`, `Vf5C`, `Vf64` | `tree::walk_gated` | 154, 126, 126, 252 | as above |
| `E43Vf6C` Integrate | `tree::integrate` | 252 | |
| `E43_Walk1F0` WalkChildrenPre | `tree::walk_pre_order` | 1609 | functor and descent |
| `CEntity::Vf40` NameWalk | `tree::name_walk` | 840 | |
| `CEntity::CEntity` ctor | `layout::construct_entity` (base: `layout::construct_base`) | 1 (load dump -2) | the root case (no parent, no definition): the frame and old-frame constructions and the state's `CBase` are inner records, checked by their arguments; the windows check every store |

Gaps (no record in the drive; the port exists or not as noted):
- `E43Vf28` (slot 10), `E43Vf68` (slot 26): ported as gated walks, no records at 40/572/1295 or the load dumps.
- `E43Vf98` (DetachFromParent, slot 38 of the list 480 pass): no record. `E43_ExclA060` (RemoveExclusion) has direct
  records since (`layout::remove_exclusion`) and is bound.
- `CEntity` ctor: the record's arguments are all 0 except `options` (0x40), so the stores of `definition` (+0x34),
  `sort_key` (+0x30) and `parent` (+0x44) are checked only as zeros. The asm store map (0x51a640-0x51a6f3) is the
  evidence for their slots.
- `CEntity` ctor, the named case (a definition: its name is read at `definition+4`, `GetNameOrNull`'s non-null
  branch) and the nested case (a parent: the frames' `Vf30`/`Vf34` owner calls): no record; both are unit-level only.
- `CEntity` dtor (0x51a810): not ported; the dtor runs at stage teardown, which the load dumps do not reach.
- The def's dtor with a list (the unlink and the count): no record (every dump has list 0).
- `E43Vf6C` (Integrate) passes its `dt` on as the raw dword the original pushes: the port carries it as a `u32` and
  never reads it as a float (rules 6-7). The walk's float use is in the callees, which are not ported here.
- The recorder's own `Heap` answer: the allocated array's count (+0) is not in a window, so only the pointer is checked.

Recorder changes (`harness/vcu_vectors.py`, `entities` set and the shared return path):
- `pops(spec)`: a `cdecl` spec (operator new) does not pop its argument on return.
- A tail-jump thunk and its callee are finished together (`on_return` / `finish`): the callee of a tail jump is not
  an external call of its caller; the thunk is. The VCU sets still pass (`RBR_VCU_VECTORS`, 177 files).
- Gate and child-call dispatch use the vtable index (`slot`), not the byte offset.

## B4 status (CGround)

Checked against the original's calls (`rust/rbr-physics/tests/entities_vectors.rs`, `RBR_ENT_VECTORS`). The records come
from `harness/vector_specs/entities_b4.py`: the drive's steps (`snapH` 40, 572, 1295) for `FetchX` and `VfE8`, and the
load-time entry dumps (`RBR_SNAPAT`) for the def's constructor and the factory.

| function | port | records | branches seen |
| --- | --- | --- | --- |
| `E33Mat34::FetchX` 0x51b6c0 | `transform::fetch_x` (`layout::load_transform`) | 3,388 (steps 40/572/1295), one `WB320` walk each | the walk is answered from the record |
| `E33Mat34::FetchY` 0x51b780 | `transform::fetch_y` (`layout::copy_grid`, the 0x4f6770 copy) | 80 (snapH 1295, plain and `--fuzz 5111`, set `g-vfd0`), one `WB410` walk each | the walk is answered from the record |
| `E33Mat34::ComposeB` 0x51b600 | `transform::compose_b` | 40 direct (set `g-vfd0`) | one `WB3C0` walk each |
| `E33Mat34::CombineTwin` 0x51ab70 | `layout::store_pair` (the same code as `Combine`) | 44 (snapH 1295) | |
| `E43PairWalk::WA70`, `WB1A0`, `WB210`, `WB3C0` 0x51aa70/0x51b1a0/0x51b210/0x51b3c0 | `walk::run` (the Five-Six family) | direct, set `g-vfd0` | equal, deeper, shallower and routed walks |
| `CGround::CDef::CDef` 0x51b990 | `layout::construct_ground_def` | 2 (load dumps) | the whole def (base, vtable, four members) |
| `GroundDefNew9` 0x51baa0 | `layout::ground_def_new` | 1 (load dump; the new def's 0x15c bytes checked by its `after` run) | allocation succeeds |
| `CGround::VfE8` 0x51c6f0 | `layout::box_probe`, `ground::contact` | 242 (steps 40/1295) | the miss path only (every query returns 0) |
| `CGround::CDef::Vf50` 0x51c810 | `layout::ground_make` | none | unit tests: old ground deleted, no memory |
| `E33Mat34::CollideInit` 0x51bd30 | `layout::collide_init` (the sequence in the original's frame, every call at the original's esp, `collide::CollideHost`; the contact arithmetic in `collide`: `fallback_point`, `normal_sum`, `normals_cancel`, `probe_point`, `contact_plane`) | 287 (`collide-*`: steps 40/1295 and the fuzzed step-40 seeds) | gate pass and fail, both grid passes, first-pass hits, fallback query, `RayClear`; the pair and fallback hit tails: fuzzed in-step records (set `g-collide`) |
| `E43PairWalk::WAC0`, `WB280`, `WB2D0`, `WB410` 0x51aac0/0x51b280/0x51b2d0/0x51b410, `E37Walk5E40`/`5EB0` 0x4f5e40/0x4f5eb0 | `walk::run` (the grid family: hooks One/Three and Two/Four per contact grid, `walk::grid_hooks`; the deeper and shallower walkers call `WAC0`, as the asm does) | direct, set `g-collide` | |

Not checked, and why:
- `VfE8`'s hit path: no record hits (the host's query misses on all 242 drive calls). The host's 13 floats are in the
  query's inner window (`S00`, `windows` on its out buffer), so a hit record would be checked end to end; until one
  exists, the port's scatter is checked by a unit test with synthetic floats, against the asm's stores
  (0x51c74b-0x51c797). The asm returns 1 on a hit.
- `CDef::Vf50`: its one call is counted (the count run) but no entry dump was written (the same symptom as the
  `CFrame::CFrame` and `CGround::Vf88` notes above). The port's writes are checked by unit tests only.
- The ground's contents for `Vf50`: no record (see above). The def's contents for `GroundDefNew9` are checked by its
  `after` run (0x15c bytes at the returned pointer, mapped as zeros first: the allocation starts zeroed, as its CDef
  record's pre bytes show).
- `CGround::CollideInit` (0x51bd30): ported whole and bound (g-collide). The drive reaches only the miss paths and the
  first-pass hit; the pair hit (both queries hit, asm 0x51c175-0x51c4f8) and the fallback hits (0x51bf93, 0x51c54d) are
  checked by fuzzed in-step records only (lead/reports/g-collide.md).

Unknown meaning (working names):
- `TWEAK_MEMBERS` (`TweakKind::Integer` at 0x11c and 0x12c, `Unsigned` at 0x13c and 0x14c): the four `GROUND` keys, by
  type only (`notes/physics/ground.md` 2a, open 2). Their value word (+0xc) is zero after the constructor.
- `SINGLETON` (0x1659268): the ground `Vf50` keeps. Its consumers (who clears it besides `Vf50` and `Vf54`) are not checked.
- `ProbeScratch` (`t60`, `t70`, `out`): the original's stack addresses. The port takes them from its caller.

Platform and bits: no float-to-int conversion, no NaN is produced or compared, no raw float bits are read (the record
words are copied as f32 values). The NTweak constructors' words are integers (vtables, zeros).

C++ disagreements: none in B4. `CGround::VfE8` (`RBR_E43_GroundB.cpp`) matches the asm: `out[4..8]` to `o[4..8]`,
`out[10]`, `out[11]`, `out[9]` to `o[8..11]`, `out[12]` to `o[13]`, `this` to `o[14]`. `CDef::Vf50` calls the old ground's
deleting destructor through vtable slot 7 (0x51c7f0), as its C++ `Vf00` does.

Recording notes:
- The load-time dumps need `vcu_vectors.py --set entities` (the check at `main`), and the `entities` set has no B4
  function. Until the harness takes any set for entry dumps, run the set through a local wrapper that maps
  `SETS["entities"]` to `entities_b4` (not committed; `harness/vcu_vectors.py` is unchanged).
- `CDef::Vf50` (0x51c810) is counted once in a count run, and no entry dump is written for it (the same as
  `CFrame::CFrame` and `CGround::Vf88`); reported to the lead.
- The dumps: `harness/play.sh` with `RBR_COUNT=1 RBR_SNAPAT=<list>` and `harness/transplant.py --count` over the
  four functions; the list is `51baa0 2`, `51b990 2`, `51c810 2` (one line each).

## B4b CollideInit (0x51bd30)

Port (g-collide): `layout::collide_init`, the whole function, with the callees behind `collide::CollideHost` at the
original's stack pointers; the contact arithmetic of the hit tails is in `collide` (`fallback_point`, `normal_sum`,
`normals_cancel`, `contact_plane`, `probe_point`). Bound in rbr-hybrid (`entities/collide_init.rs`). Parts of the notes
below are from the first (miss-path) port: its slot 12 and slot 70 are bodyA's slots 0x30 and 0x118, which take no
argument (the words they appear to get are the next callee's pushes).
Test: `tests/entities_collide.rs`, with `RBR_ENT_VECTORS` set to the records of `harness/record_entities_b4.py <snapshot> <out>
--set entities` (step 40 and step 1295 for the drive; the spec's `nodes` windows are in them). The test answers each external call
from the next recorded call (callee and arguments checked, its post bytes written), checks eax, the call count and bodyA's window.
The frame (`CollideFrame`) is the original's, from the record's entry esp (aligned down to 16, as the prologue does).

Checked:
- Drive (step 40 and 1295, 77 calls, no hit): gate fail (55 calls: `CE36SubBlock::Init`, slot 72, the gate; no `RayClear`),
  gate pass (22 calls, 11 with the second pass: bodyA's flags have bit 3), fallback query (0 in every call), `E38RayClear`.
- Fuzzed (step 40, `--fuzz 2`, `4`, `7`; 70 calls each, one call with a hit in each seed): the first-pass hit tail (the event
  is allocated, initialised, sampled and reported; seed 2's call has two hits), a second-pass hit whose reverse query misses, and
  a second pass with no hit.
- Mutation checks: the second-pass bit, the hit kind (4 to 5) and the reverse query's argument order each fail the test.

Control flow as the asm has it (all verified by the records):
- Gate: `E39_SegTest70` (0x4ff070) on the two boxes that bodyA's slot 72 filled. Fail: return 0, no `RayClear`.
- Grid: bodyA's slot 12 (given the identity object as a stack argument), slot 70 (given slot 12's result), then
  `M6610(grid, slot70, slot12, identity)`. Both slots are `retl`: the argument stays on the stack for the next callee.
- `CopyFrom(point, grid + 0x20)`. First pass: `count` = grid+0x68D queries of `(point, grid + 0x40 + 16i)`. A hit runs the
  hit tail (below), and the pass goes on.
- Second pass, only when bodyA's flags have bit 3: `count2` = grid+0x68C pairs `(a, b)` from the bytes at grid+0x484+6i and
  +0x485+6i. Query `(edge a, edge b)` into `out`; only on a hit, the reverse query `(edge b, edge a)` into the reverse output (`REVERSE`). Both
  hitting is the contact arithmetic (below). (The asm's loop; the C++ has no second pass.)
- Fallback query `M_EBC0` (0x4febc0) on `(point, out)`, only when the first pass missed; then `RayClear(grid)`.

First-pass hit tail (`report_contact`, asm 0x51be56-0x51bf3c): `CopyFrom(HIT, out)`; bodyA's slot 12 with the contact;
`ComposeA(identity, edge, transform, CONTACT)`; `E36AllocEvent(0xf0)`; if non-zero, `InitFull(event, 4, bodyA, CONTACT,
ZERO_VEC4, HIT, 0)`, and the event is its return; `SampleB(grid, index, event + 0xa4, event + 0xa8)`; the sink's slot 1 with the event.
The asm also copies the output's normal (`out[4..8]`) and words (`out[9..11]`) into the frame (`+0x80`, `+0x90`) inline;
the port makes the same copies (`store_query_normal`, `store_query_words`), since `InitFull` reads the contact they fill.

Ported since the first port (g-collide), checked by fuzzed in-step records only (lead/reports/g-collide.md):
- A second-pass hit whose reverse query also hits (`pair_hit`, asm 0x51c175-0x51c4f8): the midpoint (`E11_VecLerp`
  0x473c90), the direction from the summed normals or the midpoint, `Normalize` 0x43c6a0, the probe query, `Length`
  0x473b30, `SampleA` 0x4e0840, the `InitFull` of kind 5 and its sink.
- A fallback query that hits (`fallback_hit`, asm 0x51bf93 and 0x51c54d): the copies, the moved point without the
  second pass and the `InitFull` of kind 4.
- Not recorded: an allocation that returns 0 (the port passes event 0 to `SampleB` and the sink, as the asm does; unchecked).

Frame (`CollideFrame`, offsets from esp after the prologue): the sub-block words (`WORDS`, +0x90), the gate boxes
(`GATE_A` +0xc0, `GATE_B` +0xb0), the grid (`GRID`, +0x190, 0x6B0 bytes), the point (`POINT`, +0x20), the outputs (`OUT`
+0xd0, `REVERSE` +0x150), the contact (`CONTACT`, +0x60) and the hit point (`HIT`, +0x70); the rest are in the struct. The original's `out` is 16 bytes in the records; a
hit writes more, which the windows of the query do not cover (only the first four floats are read, by `CopyFrom`).

Reads outside the calls: bodyA's flags (+0x60) and the grid's counts and pairs (`grid_count` and the pair bytes in
`collide_init`); the ground host (`g_7c755c`, through the operand of the read at 0x51bda4) and the identity object
(`IDENTITY_GLOBAL`). A record holds these only through the `nodes` windows of the
spec (bodyA's 0x64 bytes and the two globals), so an old record without them cannot run this test.

Platform and bits: the drive and the first-pass hit tail move floats only by copy and call. The flags and counts are read
as bytes. The pair and fallback tails' arithmetic is f32 in the asm's operand order (`collide`); `Length`, `Normalize`
and the lerp are the original's calls.

Not modeled (the frame and the runtime, no effect on the checked outputs): the frame's exception-handler chain (`fs:0`,
0x51bd3d-0x51bd44 and 0x51c633-0x51c63a), the stack cookie (0x7e9db0 at entry, 0x70c2b5 at exit).

Recording the fuzzed records: `--fuzz` alone turns the launch control on and fuzzes the step's other functions, which sends the
step-40 snapshot off its recorded path (an unstubbed import, `0x7b0cf5b0`). `harness/record_entities_b4.py --no-launch` keeps
the launch and those fuzzers off; the spec's own `fuzz` hooks run. The hooks apply to CollideInit's call sites only (the return
addresses 0x51bdad, 0x51be4e, 0x51c14e, 0x51c16d, 0x51c28f): the same query is also called by the wheel (E51Wheel::M3D520), and
fuzzing that call sends the step off its path. Seeds 2, 4 and 7 record; seeds 1, 3 and 6 record with no hit; seed 5 stops on
the same unstubbed import (not investigated).

Retl trap: slots 12 and 70 pop nothing, so their specs are `cdecl=True, popped=4` (the recorder reads one argument and pops
none). With `popped=4` and no `cdecl` the recorder's frame never closes and later records are dropped. Slot 12 is called with
the identity object (the grid) and, in the hit tail, with the output box: both are one stack argument left on the stack.

C++ disagreements (`src/rbr/RBR_E43_GroundC.cpp`; the asm is the reference, nothing changed in `src/rbr`):
- The gate reads `t7b8`, `t7c8` (its C++ boxes); slot 72 fills `t7b0`, `t7c0`. The asm passes the same two boxes to both.
- Slot 12 is called with no stack argument; the asm pushes the identity object (the argument stays on the stack).
- One grid pass; the asm has two (the second is taken from bodyA's flag bit 3).
- The query's output is `t7a8` in the C++; the asm passes the frame's own output (+0xd0).

## B5 status (CHingedRigidBody limit and rotation sims)

Done: `SimC960` (0x51c960, the rotation block from the hinge angle), `SimCF10` (0x51cf10, one limit's residual), `SimEA80`
(0x51ea80, the sum over the two limits) and the CRT sine and cosine they call (0x53f3b0, 0x53f3a0). Port: `hinge_sim.rs`
(pure functions on `HingeLimit`, `HingeState` and `RotationBlock`); `layout/` (`load_hinge_limits`, `load_hinge_state`,
`hinge_limit_residual`, `hinge_limit_sum`, `hinge_rotation`). Vectors: `harness/vector_specs/entities_b5.py`, checked by
`tests/entities_hinge.rs` (`RBR_ENT_VECTORS`).

Matched calls (the drive's records at steps 40, 572 and 1295, plus three fuzzed step-40 runs, seeds 1 to 3):
- `SimC960`: 162 of 162, with the rotation block's window.
- `SimCF10`: 162 of 162 on the residual (st0 as f64 bits) and every window. Branches: `Above` 47, `AboveClamped` 47,
  `Below` 4, `BelowClamped` 29, `Settled` 35.
- `SimEA80`: 162 of 162 on the sum and the windows; its inner `SimCF10` calls are the active limits, in order.
- CRT sine and cosine: 324 calls match at the f32 store (the port's f32 value equals the record's f32 store). As f64 they
  are equal in 316 of 324; the rest differ in the last bits and round to the same f32.

Fuzzing: `SimEA80`'s spec sets its inputs at entry, before its windows are taken (the two limit states, the hinge angle
and rate, each limit's reference, step clamp, stop and damping coefficients). Its port and its inner `SimCF10` calls
then read the same values. The fuzzed `SimC960` angles (`fuzz_angle`) are the source of the extra CRT calls. A fuzz hook
on `SimCF10` itself was tried and dropped: the recorder reaches only the calls `SimEA80` makes, and fuzzing those changes
the state between its two calls, which its port cannot see.

Record files: `RBR_ENT_VECTORS` = the drive's `b5-40`, `b5-572`, `b5-1295` and the fuzzed `h-40-f1`..`h-40-f3`.
The step-40 snapshot is read from `/private/tmp/snapH/snapH.snap-40.bin`: the `/tmp/rbr-snaps` symlink points to a path
that has moved.

x87 model: the residual's operations are f32 in asm order (`-(f * stop)`, then minus `rate * damp`), with the zero case
(`f == 0`) giving +0.0. A NaN compares as the asm's `jbe` does (`!(lim > t)`: the settled branch); this is a unit check only (`hinge_limit_nan_settles_like_the_asm`), since no record holds a NaN. The sine and cosine are
computed in f64 and rounded to f32 where the asm stores them (`fstps`/`fsts`); the record's f64 agrees in 234 of 244 calls.

Unknown meaning (working names): `HingeLimit.mode` (`LIMIT_ACTIVE` is 2, the value the asm tests), `stop_coef`, `damp_coef`,
`max_step`, `ref_angle`; `HingeState.limit_state`, `hinge_angle`, `hinge_rate`. The names are descriptive, not from the notes.

Constants read from the exe's image (PE section table, read-only): 0x795e50 is 0.0 (the zero the asm loads for the settled
residual, which the 35 `Settled` records confirm as +0.0), 0x795e18 is 0.5 (`SimCFF0`'s scale). The sign
of the zero from `0 - s` in `SimC960` (the asm subtracts from zero, not negating) is per the asm; the records check it only
where `s` is zero in a recorded call.

C++ (`src/rbr/RBR_E44_SimA.cpp`): `SimC960` and `SimCF10` match the asm. No C++ for `SimEA80` was found; the asm was used.

Ported since: SimD860, SimDB40, SimEFE0, SimD160, VfE070, SimF280, SimF950, SimFD00 (below), the CState, CDef and hinged
body constructors and `E44MatTable::Pick` (see "B5 pending"). Remaining, reached on the drive: `LinkC`, `ConnInvoke` (no
address in the notes yet). `SimD980` is not reached on the drive.

## B5 pending (what was open at B5, and where it went)

At B5 these had no local records, so no port and no vector test; most are ported since, as noted.

- `CHingedRigidBody` slot 36, `E47D36y` (0x52bf30, `RBR_E47_Slots.cpp:73`, `CSolid::E43Vf90`): ported since in rigid_body
  (`rigid_body::emit::emit_body_event`) and bound (partial in `bound.csv`). Reached on the drive by SimEFE0 when its gate is
  set (five inner records in `efe0-4-*`, all in the window of the body). Its calls, at B5 not recorded: `CReal::SetDirty` x2
  (0x52f450, on +0x1d0 and +0x1e0), the body's virtual slot 18 (`*(vtable + 0x48)`), `E46_WallRel6a0` (through 0x519a60, when
  +0x44 is zero), `E01Vec4::CopyFrom` x2 (0x43c5b0) and `CMultiSignal1<CSigMrRBodyEvent>::Emit` (0x51f7c0). The slot-18 target
  and the emit's payload were not known; the window alone cannot check them.
- `SimF280` (0x51f280, `RBR_E44_SimB.cpp:621`): ported (`limit.rs`). `SimF950` (0x51f950): ported, see "B5 status, SimF950".
- Constructors: `CState` (0x51cc90) and the hinged body (0x51e9a0) are ported for the load path
  (`layout::construct_hinged_state`, `layout::construct_hinged`, checked against the game by `tests/load_defs.rs`), as are
  `CDef` (0x51e680, with `E44_BlockCtor` 0x51ca40) and its factory (0x51f880, `layout::construct_hinged_def`,
  `layout::hinged_def_new`).
- `E44MatTable::Pick` (0x520240, `RBR_E44_Mat.cpp:178`): ported (`layout::sample_indexed`, with `mat::texel_offset` for
  0x520010) and bound. `E44_LinkC` (0x520310, called from `RBR_E14_App.cpp:785`): reached, no record in any store, not
  ported. `E44_LinkA` (0x520290) and `LinkB` (0x5202d0) are in the same file;
  not on the drive's list.
- `ConnInvoke`: no address in the sources or the notes (`called.csv` has none). Not pursued.
- `SimD980` (0x51d980): not reached on the drive. Not ported.

## SimFD00, the detach (0x51fd00)

`layout::sim_fd00`, with `hinge_fd00.rs` for the lanes. A body with a parent (at 0x44) and an entity list (at 0x18) leaves
its car: `SimDA60` (0x51da60, `layout::sim_da60`: the position through `ComposeA` with the car, the orientation through
`M83DD0` with the car's, both copied to the start pose), `CCar::ToWorld` of the linear and angular velocity, each lane the
car's plus the transformed one; slot 21; `E43ClearAcc` (0x529e60, `layout::clear_accumulators`); the list's count down and
`UnlinkA` (0x4e41a0, `physics_core::layout::unlink_a`); parent and car cleared; slot 36; `E43_InitBD0` (0x519bd0,
`layout::init_bd0`); `LookupTwin` at the position and the cell move (0x4faeb0, `collision::layout::move_to_cell`);
`RegWall` (0x5263d0, `physics_core::layout::reg_wall`, its slots asked through the host); the detach signal (0x51f580,
the physics core's emit template) with the word at 0x30; slot 18; the body event (kind 3, the owner word, the pose after and before)
0x40 into the frame. Returns 1, or 0 with no parent or list. `frame` is `((entry - 4) & !0xf) - 0x90`. Every call is a host
call at the original's esp.

Vectors (`harness/vector_specs/g_hinge3.py`, direct calls on tanner-damage 11827 and 12000): 70 SimFD00 calls (58
detaches; velocities and position moved, some without a parent or a list), 30 SimDA60, and the leaves (ClearAcc 20,
InitBD0 20, PushItem 40, UnlinkA 40 on lists of 0 to 4, the cell move 40, RegWall 30 with all four lists).
`entities_vectors::sim_fd00_matches_direct_records` answers the calls in order and checks each one's arguments and the
memory it sees.

## StepBig, the mounted body's detach (0x522100)

`layout::step_big` (`CMountedRigidBody::StepBig`, called from `CCar::Vf6cx`). The same gate and the same tail as SimFD00
(`detach_tail`: the calls and locals are at the same depths), with its own pose step in place of `SimDA60`: the position
through the car's `WorldCompose` (0x4e6950), the orientation through `M83DD0` with the car's and `E34Quat::Norm`
(0x4e4c70), then the start pose takes the new pose. The event is 0x60 into the frame and the detach payload 0x18;
`frame` is `((entry - 4) & !0xf) - 0xb0`.

Vectors (`harness/vector_specs/g_stepbig.py`, direct calls on the mounted bodies of the V2 detach snapshots zr-25 1991,
zr-42 1731, impreza03-33 1943 and impreza00-63 2730): 60 calls, 46 detaches. `entities_vectors::step_big_matches_direct_records`
checks them as SimFD00's.

## B5 status, VfE070 (the frame from a quaternion and a vector, 0x51e070)

Ported: `hinge_e070.rs` (`frame` and `project`, pure, over `E070Frame`'s pointers in `layout::vf_e070`). Its callers are the
two calls in SimEFE0 (`VfE070(a, b, c, d, e, f)`, with the definition's pointers: +0x300, +0x2c0, +0x2e0 into +0x310 (`d`),
+0x3e0 (`e`), +0x404 (`f`); and +0x3c0, +0x380, +0x3a0 into +0x3d0, +0x44c, +0x428). SimEFE0's test runs this port on the
record's windows (`tests/entities_efe0.rs`, `vf_e070`), as well as its own test.

The frame: `q` is the normalised xyz of `b`, `n` is the normalised `b x c` (the raw `b`), and `m = n x q`. `e` is the rows
`q`, `m`, `n`. `f` is the transpose of `e`, and `d = -(F a)` with `d[k] = F[k] a0 + F[k+3] a1 + F[k+6] a2`: `F` is `f` plus the
word after it, `f[9]`. The normalises are the asm's `rsqrtss` of `(x*x + y*y) + z*z` (`math::rsqrt`), and the cross products are
the asm's `E01VecSetA::Cross` (`math::vec`). The asm's `E29Quat::Normalize`, `E01Vec4::Normalize` and `E01VecSetA::Cross` are
inline here (the asm calls them; their operands and order are the asm's). The asm's `-1.0 * d` is a negation here: the two differ
only in the sign of a NaN (platform NaN, not compared).

Order (the layout layer): `e` is stored, then `f[9]` is read, then `a`, then `f` and `d` are stored. The asm stores `f` before it
reads `f[9]` and reads `a` after that: the windows don't overlap, so the order of `a` and `f` changes nothing. `f[9]` is the word
after `f`: in the second call `f` is +0x428 and `e` is +0x44c, so `f[9]` is `e[0]` (stored just before it is read). The C++
(`RBR_E44_SimB.cpp:238`) reads `f[9]` the same way; that is not a disagreement.

Records: the inner calls of the `efe0-1..4-f0..f6` files (`tests/entities_e070.rs`): 56 calls (two per SimEFE0 record, 28
records), each with its six windows before and after. All 56 match: every window after the call. A change to the tail word
(`f+32` for `f+36`) fails the test. Reading the tail before `e` is stored does not fail the test: `d[3]` multiplies `f[9]` by `a[2]`,
and `a[2]` is zero in every record, so the order of that alias is not checked by the records (it follows the asm, from Ghidra).

## B5 status, SimCFF0 (the slip term, 0x51cff0)

Ported: `hinge_slip.rs` (`slip_term`, over `SlipInput`, `SlipFrame` and the `SlipCalls` trait: the two `MB560` walks and the
copy of the car's vector) and `layout::slip_input`. `SimCFF0` reads the owner car (+0x3d0; its state object is car+0xa00,
`car_body::layout::state_address`; `StateAtb` is a plain `lea`, so the port does not call it), the body's position, the
definition's axis object (+0x360), anchor (+0x320), scale (+0x2c0) and friction (+0x34c), and the state word at +0xc00.

Checked: 160 calls of `SimCFF0` match (the drive's 40 at step 40, with three fuzzed step-40 runs, seeds 1 to 3), with all 320
walks answered from the record. The drive's 40 are all 0.0: the definition's friction is 0 and its axis object is zero, so
the term is 0 whatever the walks return. The fuzz sets the friction (half the calls), the scale, the axis and the state
word at entry; 58 of the 120 fuzzed results are nonzero, 27 of them negative.

Order (asm; the C++ differs): the dot product is `(v2*h2 + v1*h1) + v0*h0` (same as the C++). The result is
`(y*c1 + z*c2) + x*c0` in x87 at f32 (the C++ sums `x*c0 + z*c2 + y*c1`); with the C++ order the test fails on a recorded
call. `f4 <= 0` is tested as the asm's `ja`, so a NaN is negated (the C++ keeps it); no record holds a NaN.

Unknown meaning (working names): `car_state_c00` (the car's state word at +0xc00, the asm's `f1`, used as a squared factor
of the scale); the axis object (`def+0x360`, the walk's `a`; the C++ names it `ctx`).

## B5 status, SimEAD0 (the hinge solve, 0x51ead0)

Ported: `hinge_solve.rs` (`hinge_solve` over `SolveCalls`; `SolveTerms`, `EntryTerms`, `SolveFrame`) and the reads in
`layout/` (`hinge_linked`, `hinge_owned`, `solve_entry_terms`, `solve_terms`). The sub-calls are the caller's: `SimE1D0`
(0x51e1d0), `SimCFF0` and `SimEA80` (ported; answered from the record here), and the contact vote `E47Solid::Vf81x`
(0x52c510, answered from the record). The early return (the link word at 0x650 is set) is ported. The unowned case (the owner
word at 0x3d0 is zero) is ported as the original's tail call: `CSolid::E43Vf64` (0x52cb20) on the body, eax returned
(`Outcome::Unowned`). The force pass runs the contact list (the list word at 0x274, the vote's BigA list, `vote::vote`),
which the tanner-damage chain reaches at step 12000. Unowned direct records: `g-contacts-ead0-a{1,2}.txt` (`direct_calls.py`
on the BigA entry dumps `RBRRecompSnapAt-0052b9f0-{1,2}`, `--this 0c521048`, seeds 7301 and 7302; `fuzz_solve` clears the
owner word on half the calls, and `CSolid::E43Vf64` is an external with the body read on return): 80 calls, 48 unowned, the
32 owned ones all with a vote hit on the dump's own list.

Checked: 160 calls (the drive's 40 at step 40 plus fuzzed step-40 runs, seeds 1 to 3), with the four sub-calls answered from
the record in order (E1D0, SimCFF0, SimEA80, vote), the windows and the value at +0x624. Rate signs in the 160: positive 65,
zero 56, negative 39 (the zero case takes the `ab > af` test). The vote hits in 8 calls, so the Scale and Cross path is
checked there; the drive's 40 are all misses.

Fuzz: the entry fuzz (`fuzz_solve`) sets the angle, rate, settle, mass, b0, the definition's matrix, vectors and scalars,
and the rotation block. It does not set the vote's inputs, so the vote's hit is the original's own decision.

Gating (a finding): every hinge fuzz hook (`fuzz_hinge` on SimEA80, `fuzz_slip` on SimCFF0, `fuzz_solve`) now returns when
its caller is hinge code (`called_from_hinge`, return address in 0x51c000..0x520000). Before, SimEA80's entry fuzz also
fired inside SimEAD0, rewriting the rate and angle mid-call, and the record showed the mid-call values.

NaN: the settle value at +0x624 can be NaN from fuzzed inputs; the original's sign is negative, the port's positive. The
test stores the record's NaN after checking both are NaN (`notes/rust-physics.md`, section 8, "entities sites").

Order: `d = rotation * def_matrix` with the C++'s term order per element, the dot product, and the sum
`((0 - dot) + (ref_angle - angle) * spring_gain) - damping * rate + slip + limit_sum`. The C++ was the source; the asm was
read for the b0 and mass defaults (equal and NaN branches) and the sign test. The rest of the operation order is not yet
checked against the asm line by line; the records agree on 160 calls, both rate signs, the zero branch and the vote path.

Unknown meaning (working names): `ref_angle` (def+0x49c), `spring_gain` (def+0x4cc), `damping` (def+0x4dc), `settle`
(this+0x620), `column_weights` (def+0x3d0), `pf` (def+0x2c0, the `pf` of the dot product), `vote_weights` (def+0x380),
`vote_lever` (def+0x3c0), `pivot` (this+0x600), `def_matrix` (def+0x3e0..0x400), `rotation` (this+0x598..0x5b8, the rotation
block of `SimC960`).

## B5 status, SimE1D0 (the pivot, 0x51e1d0)

Ported: `hinge_pivot.rs` (`pivot_and_settle` over `PivotInput`, `PivotFrame` and the `PivotCalls` trait: the three `MB560`
walks and `BodyCompose`), `layout::pivot_input` and `layout::store_pivot` (the pivot at +0x600 and the settle value at +0x620).
The walks and `BodyCompose` are the caller's calls; `BodyCompose`'s `ComposeA` is the record's, under it.

Checked: 40 calls at step 40 (`e1d0-40.txt`, SimE1D0 recorded on its own, direct), with its 160 sub-calls answered from the
record (three walks and the ComposeA, in order). `tests/entities_pivot.rs` checks the calls, the stored pivot and settle, and
every window.

C++ (`src/rbr/RBR_E44_SimB.cpp`): `l40.Cross(&l40.m0, m18 + 2)` crosses `m0` with itself: the asm passes `&l40.m0` as both
operands (0x51e296). The first cross is zero, so the second cross is zero, and the pivot is the first walk's hit. The record
agrees: the test fails if the pivot is not the hit.

Not reached: the gates. `car+0x180` and `def+0x2f0` are nonzero in all 40 calls, so the zero case (the wheel and `ComposeA`'s
`a` set to 0) has no record. The arithmetic has no other branch.

Unknown meaning (working names): the car's words at +0x250, +0x254 (`reference`), the height at +0x258, the walk vector at
+0x260 (`car_offset`), the car's +0x190 (`car_probe`, the third walk's `a`; the asm gates it on +0x180).

## B5 status, SimFB40 (the hinge signals, 0x51fb40)

Ported: `hinge_signals.rs` (`emit_state` over `SignalInput`, `SolidGeneralState`, `HingedBodySignal` and the `SignalCalls`
trait: `SimDB40`, `ComposeA` or `ToQuat`, the two emits and `E43Vf28`), `layout::signal_input`. The emits are the caller's
calls. Their template is the one in `physics_core::signal` (`CMultiSignal1::Emit`): the normalised asm of 0x4e8330 and 0x51f640
is the same as 0x526580, and only the out-of-range message differs, so the caller's host supplies it.

Checked: 16 records, `fb40-<k>-f<s>.txt` (the four load-time entry dumps, the first four calls at the stage start, unfuzzed
(`f0`) and with fuzz seeds 1 to 3). The dumps come from a play with our SimFB40 copy transplanted (`transplant.py 0x51fb40`);
the records are the original's calls on those inputs. `f0` takes the `ToQuat` branch (key and owner nonzero). Seeds 1 to 3
take the `ComposeA` branch (the fuzz zeroes the key). `tests/entities_signals.rs` answers the 80 sub-calls from the record and
checks the emits' words, except the padding words 1..=3 that the original does not write.

Order (asm, checked): the branch is `key == 0 || owner == 0`; the link flag is `sete` on the word the link pointer
(+0x650) points to; the one subtraction is `angle - reference`. No x87.

Gaps:
- The drive calls SimFB40 23640 times, all from `CCar::Vf28x` (0x4ea020, slot 40) through `E43Vf28` (virtual slot 10). That
  path is outside the physics step, so no step snapshot reaches it (40, 572, 1280, 1295, 2000, 5000, 9000 have no call). The
  snapat list records only the first N calls, all at the stage start, so the drive's other calls are unrecorded.
- Unreached: the owner-zero case (the fuzz sets the key, not the owner), and the `ToQuat` branch with fuzzed inputs (seeds 1
  to 3 all zero the key).

Unknown meaning (working names): the key at +0x30 (`key`); the vectors at +0x100, +0x190 and +0x1c0 (`body_rotation`,
`second`, `first`); the solid payload's last two words (1.0 and 0; the 1.0 is the unit `unit`).

## B5 status, SimDB40 (the hinge's debug walks, 0x51db40)

Ported: `hinge_db40.rs` (`sim_db40` over `DbInput`, and the `DbCalls` trait: the `MB560` walks and `CCar::Xform8130`),
`layout::db40_input` (the guard and the reads). SimFB40 makes the call (`SignalCalls::sim_db40`); this port is the function
itself, tested on its own records.

The guard: the CPhysics global (`g_1659b70`, `car_body::layout::WORLD`) gives the CDef at +0x24, whose debug flags word at
+0x160e0 must have 0x400000 set, and the owner car at +0x3d0 must be nonzero. The drive has the flag clear, so SimDB40 makes no
calls there. The fuzz sets the flag at the SimFB40 entry (`fuzz_fb40`), either way, so both branches are recorded.

Calls, in order: three walks over the definition's matrix rows (+0x3e0, receiver the car); three walks over the products
`A*M`, `B*M`, `C*M` (the rotation rows at +0x598 times the matrix); `Xform8130(car, E44_MulM33(+0x3d0 vector, +0x574 matrix)
+ the definition's pose at +0x300)`; the walk over the definition's axis (+0x360, receiver `this`); the walks over the x, y
and z unit vectors (receiver `this`). Their outputs are scratch: nothing reads them, so the test checks the calls, their
input vectors, and every window of the hinged body.

Asm checks (against 0x51dc58-0x51de5e and 0x51defe-0x51df4d, the order of each sum): `A*M` is (a2 m20 + a1 m10) + a0 m00
(column 0), (a2 m21 + a1 m11) + a0 m01 (column 1), (a2 m22 + a0 m02) + a1 m12 (column 2); `B*M` is (b1 m10 + b0 m00) + b2 m20,
(b1 m11 + b2 m21) + b0 m01, (b1 m12 + b0 m02) + b2 m22; `E44_MulM33` is (m0k x + m1k y) + m2k z. The C++ (RBR_E44_SimB.cpp)
writes the same sums in the same order. The pose is added after the product (`pose + product`, commutative).

Records: `fb40-*`, 28 of them (the four SimFB40 entry dumps, fuzz seeds 0-4, 6 and 7; seed 5 faults in the original, see the gaps). The guard passes in 12 of them, which is 132 walk and transform calls. Their input vectors match the records bit for bit (the first 12 bytes of each walk's 16-byte window), and the hinged body's window is unchanged. Swapping the order of one product sum (column 2 of `A*M`) is caught on these records.

Gaps:
- The walks' and the transform's outputs are not used, so they are not checked. Only their inputs and their side effects are.
- Fuzz seed 5 faults in the original for every dump (`write to unmapped 0x31306d65`): `fuzz_fb40` writes a random 32-bit
  word at the definition's +0x4fc, which the original dereferences. Those four records are not recorded. The word needs a valid draw.
- The owner car is never zero in the records: the guard's car test is checked by reading, not by a record.
- Null pointers: the asm's `def+0x2f0`/`+0x350` tests (in SimDB40 the pose and axis pointers) read address 0 when they are zero,
  so the port takes the pose and the axis as set. No record reaches that.
- Unknown meaning: the 3x3 at hinge +0x574 (`hinge_matrix`), the vector at def +0x3d0 (`definition_vector`), the rotation rows
  at +0x598 (`rotation`), and the debug flag bit 0x400000 (`DBG_HINGE_FLAG`, inferred from the guard).
- Platform: the sums are single precision, so there is no x87 model here. A NaN input gives a NaN in both, and the NaN sign is
  not compared (notes/rust-physics.md, "Default NaN").

## B5 status, SimD860 (the rotation row's walk, 0x51d860)

Ported: `hinge_d860.rs` (`sim_d860` over `D860Input`; the `D860Calls` trait, whose `walk` returns the walk's output),
`layout::d860_input`. Its caller on the drive is SimEFE0 (0x51efe0): a count run shows 4 calls of SimEFE0 and 4 of SimD860,
and none of SimD980 (0x51d980, the other caller), so the records come from the SimD860 entry dumps (`d860-snapat.txt`, a
transplant with SimEFE0 and SimD860). The step replays did not reach it.

Calls: one walk, `E43PairWalk::MB560` on the car with the vector `row` and the identity, where `row[k] = (m[1][k]*b[1] +
m[2][k]*b[2]) + m[0][k]*b[0]` for the definition's matrix `m` (+0x3e0) and the rotation row `b` (+0x5a4). The output is
`out = (this+0x5c0) + walk output`. The sum order is the asm's (RBR_E44_SimA.cpp:156 writes the same). A swapped term is caught on
the records (one ulp).

Records: `d860-*`, 28 of them (the four SimD860 entry dumps, fuzz seed 0 and seeds 1-6). All match: the walk's input, its
receiver and `b`, the stored `out`, and every window.

Gaps:
- SimD980 (0x51d980) is never called on the drive, so its use of SimD860's output is not ported and not recorded.
- SimEFE0 (0x51efe0) is ported in `hinge_efe0.rs` (`hinge_frame_update`, test `entities_efe0`) and replays its `efe0-*` records; its call
  of SimD860 is covered by `entities_d860`.
- Unknown meaning: the hinged body's vector at +0x5c0 (`offset`), which is the base of `out`.

## B5 status, SimEFE0 (the hinged body's frame update, 0x51efe0)

Ported: `hinge_efe0.rs` (`hinge_frame_update` over `EfeFrame`, with the `EfeCalls` trait for the calls below), `layout::efe0_frame`,
`efe0_state` and `store_efe0_state` (the body and definition words the port reads and writes). Its caller on the drive is
CCar's per-step loop (RBR_E34_CarD.cpp:396).

Calls, in order: `E47D36y` (vtable slot 36, `this`) only when the definition's gate (+0x4ec) is set; `VfE070` twice (six
vectors, pointers into the definition); `CCar::BodyCompose(car, a, out)`; `SimD160(this)`; `SimD860(this, out)` (ported,
`hinge_d860.rs`); `E37GridOps::AddEdge(grid, vec)` when the grid is set. The body is loaded back after every call that can
change it, and the port's values are stored before each one.

The angle store (+0x3fc) is written as float bits: `(f6 + f7) * d47c`, from the asm's `movss` after the SetDirty call. The
C++ (RBR_E44_SimB.cpp:568) casts it to int, which the asm does not do; that is a C++ disagreement for the lead. The
angle branch runs only when the flag word at the link (+0x650) is zero.

Records: `efe0-1..4-f0.txt`, one entry dump each (the SimEFE0 entry of a drive step), plus fuzz seeds 1-6 for each
(`fuzz_efe0`: the gate, the link flag, the three angle words and the grid, set at entry). The drive's four records: 1-3 take
the dirty branch (gate 0, no slot-36 call) and keep the flag at 1, so their angle branch is skipped; 4 takes the slot-36
branch (gate 1), clears the flag, and runs the angle branch. The seeds cover both gates, both AddEdge cases, and the
angle branch with the flag at zero. All 28 match: the calls in order, their arguments, the body before each call, and every
window. Changing the SetDirty word from 1 to 2, or the flag test to `!= 0`, fails the test on the records.

SimD860 inside SimEFE0 is not fuzzed (its fuzz hook skips its return address, 0x51f1fd): its rotation row and matrix are
SimEFE0's inputs, and SimEFE0's own fuzz sets them, so the nested call must see the values the port reads.

Two fixes found by the records: the `VfE070` windows widened to 64 bytes (the callee writes the vectors and the
definition's words beyond the vector windows, e.g. def+0x424); and the angle store as float bits (above).

Copy source: the asm copies the body's rotation block (+0x100) into +0x2a0; the C++ reads +0x40 (RBR_E44_SimB.cpp:593).
The port follows the asm, and that is a C++ disagreement for the lead.

Gaps:
- `E47D36y`, `SimD160`, `BodyCompose`, `AddEdge` are answered from the record in this test, so their writes outside the
  windows are not visible in it. Each has its own port since (`rigid_body::emit::emit_body_event`, `hinge_d160.rs`,
  `car_body::transform::body_compose`, `collision::layout::grid_add_edge`), checked by its own vectors. (`VfE070` is
  ported: the SimEFE0 test runs it.)
- The fourth lane of the SimD860 copy (+0x5ec) is the original's stack garbage; the test copies it from the record.
- Unknown meaning: the flag at the link (+0x650, the angle branch's test); kept as a working name in `layout/`.

## B5 status, SimD160 (the body's frame products, 0x51d160)

Ported: `hinge_d160.rs` (`sim_d160` over `D160Calls`; typed `D160Def`, `D160Body` and `D160Words`; `products` and
`translation`), `layout::d160_frame`, `d160_words` and `store_d160_words` (the only code that reads the offsets). Its caller on the drive is SimEFE0 (0x51f1a7).

Calls, in order: `CCar::Xform8130` (the car, the definition's pose at +0x300, the body's +0x5c0), `E43PairWalk::MB560` (the
car, +0x2c0, the identity global, +0x5f0), `SimC960` (the body; the rotation block at +0x598..+0x5b8 from the hinge angle), then
the products: `v = second_mat * rot_x`, `frame = v * hinge_mat` (+0x574..+0x594), and `quat_matrix = rot_x * hinge_mat`
(+0x550..+0x570). Then `E34Quat::ToQuat` on `frame` into +0x100, and the translation (the frame's columns against
`column_weights` +0x3d0..+0x3d8, plus the anchor +0x320..+0x328) into +0x140.

Names (working names where the meaning is unknown; the addresses are in the doc comments): `hinge_mat` (+0x3e0, SimEAD0's
`d`), `column_weights` (+0x3d0, the same vector as SimEAD0's `c`), `anchor` (+0x320, the hinge anchor), `rot_x` (+0x598, the
SimC960 block). Unknown meaning: `second_mat` (+0x428, a 3x3 matrix read only here and by SimEFE0's VfE070 pointers). The
translation's fourth lane is the asm's own sum: its last term is `rot_x[0][0]`, not a frame entry (kept as read).

The translation is `E01Vec4::CopyFrom` in the original, and the port stores it inline. `CopyFrom` is not in the spec: SimFB40
calls it too, and recording it would change SimFB40's (ported) records. The final window check covers the stored bytes.

Order: each product sum keeps the asm's term order (`sum3` takes the pairs in that order); this changes the bits. The translation's order is the asm's, not the C++'s: the asm sums the three
matrix terms, then adds the pose word (`(a + b + c) + pose`, a vector sum), where the C++ adds the pose first. The port follows
the asm; that is a C++ disagreement for the lead.

Records: `d160-1..4-f0.txt` (the four SimD160 entry dumps, from a transplant of SimD160 alone) and fuzz seeds 1-6 for each
(`fuzz_d160`: the definition's frame block +0x320..+0x448 and the hinge angle, at entry). The nested SimC960 is not fuzzed
(the angle is SimD160's input), and neither is SimD160 when SimEFE0 calls it (its record has the inputs at entry). All 28 match:
the calls, their arguments, the body before each call (every window in the body), and every window at the end. A change to one
product sum (`v0` scaled by 1.0001) fails the test at the `ToQuat` pre-check.

Gaps:
- The C++'s null checks on the frame pointers (`d == -0x2f0 ? 0 : d + 0x300` and the like) are not in the port: the drive's
  pointers are never null. Not checked against the asm.
- `CCar::Xform8130` and `E43PairWalk::MB560` are external (answered from the records); `SimC960` is the ported sim, called
  through the trait here. `E34Quat::ToQuat` is external too.
- Only the four entry dumps and their seeds: the step's other SimD160 calls are not covered.
- The db40 records (the FB40 dumps) now carry `Xform8130`'s output window, so `SimDB40` and `SimFB40` records were regenerated.

## B5 status, SimF950 (the hinge's per-step update, 0x51f950)

Ported: `hinge_f950.rs` (`next_rate`, `advance`, the `Calls` trait) and `layout::sim_f950_at` (the call order, the stored
words and the payload). Its bindings are in `rbr-hybrid/src/entities/hinge.rs` (`sim_f950_run`, `state_emit_run`). The state emit
(0x51f700, `CMultiSignal1<CSigMrRBodyState>::Emit`) is the physics core's emit (`physics_core::signal::emit`, the same
template as 0x526580), with the connections' invokes at the original's depth (entry esp - 24).

Order (asm, checked): the link flag test; the owner car (+0x3d0) zero gives `CSolid::E43Vf6C(dt)` and returns its eax;
`x = (+0x624) * dt + (+0x40c)`, clamped to [-10, 10] (a NaN takes the unclamped branch with sign 0, as the C++); the sign
of `x` against the old rate's, with the old rate nonzero, zeroes the rate (`SetDirty` on +0x400 comes before the store to
+0x40c); `CFloat3::Scale` of the rate into def+0x2c0 with `pa` (+0x190, absent when `this` is -0x180); the zero store of
+0x1c0; the angle `(+0x40c) * dt + (+0x3fc)` after `SetDirty` on +0x3f0; `SimF280`, `SimD160`; the payload (key at +0x30,
the rotation block's vector at +0x20, the position at +0x14 over the rotation's first lane, the padding words not written);
the emit on `[0x893600] + 0xe438`. eax is the emit's (the last invoke's, or 0 when no connection is called).

The asm writes the rate through the tweak at +0x400 (its value word is +0xc), which is the C++ `*(float*)(t + 0x40c) = s`;
the C++ and the asm agree, and the Scale reads the stored rate.

Checked: `rust_step --each --only 0x51f950 --only 0x51f700` on the nine vcu-cov snapshots: identical each step, one SimF950
call and six emit calls per snapshot, all aligned (call fill). Strict 30-step chains from 1600 and 2400 with all bindings.
`tests/entities_vectors.rs` (`sim_f950_runs_the_calls_in_order_and_writes_the_payload`) checks the call order, the stored
words and the payload; the pure parts have unit tests in `hinge_f950.rs`. No records: rust_step is the evidence.

Not checked: the range path of the state emit (count >= 32) is declined (the original logs and traps); the null cases
(`this` of -0x180 or -0xf0) are in the asm's tests but no record or snapshot holds them. A null link or rotation pointer
faults in the original; no record holds one.
