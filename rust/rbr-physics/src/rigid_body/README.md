# Rigid body port (E47): plan, names, open questions

Scope: the `CSolid` family in `src/rbr/RBR_E47_*.cpp` (exe 0x529800-0x52d700), the functions the tanner-corolla
drive reaches (`notes/progress/called.csv`, file prefix `RBR_E47_`, as `scripts/progress.py` `rigid-body`). Out of
scope: the E46 accumulators (`E46_Accum250`/`Accum340`, `physics-core`), the E43 entity walkers and the vtable
glue in `src/rbr/RBR_E43_*` (`entities`). Numbers below are from the progress file and the C++ annotations; the
asm, not the C++, decides order and precision.

## Plan (step 1)

Grouped leaf -> step. "status" is `scripts/progress.py` (`called.csv` x `gameplay-functions.csv`); `-` means the
function has no row in `gameplay-functions.csv`.

### Leaves (no call into another physics function, or only accessors)

| Address | C++ name (src/rbr) | Rust name (`-`: no Rust item yet) | status |
| --- | --- | --- | --- |
| 0x52b740 | `CSolid::Vf24x` | `sleep::step` | verified |
| 0x52c8f0 | `CSolid::VfC8` | `pose::pose_step` | verified |
| 0x52c510 | `CSolid::Vf81x` | `vote::vote` | checked (the list path: see Ported) |
| 0x52cfd0 | `CSolid::VfDx` | `window::back_step` | verified |
| 0x52d110 | `CSolid::VfEx` | `window::advance_rest` | verified |
| 0x52a500 | `CSolid::Dot60` | `dot::dot60` | verified; 12 calls (see Dot60 and E43Vf138) |
| 0x529640 | `E46Impulse::Impulse640` | `layout::clamp_velocity_along`, `impulse::contact_impulse` | verified; preserves all four direction lanes, including the stale lane 3 from the caller's stack |
| 0x5297d0 | `CSolid::E43Vf13C` | `layout::solid_inverse_mass` | verified; x87-promoted f32 inverse mass from the definition's inverse-mass field |
| 0x529d30 | `CSolid::Step77` | `vote::probe_ground` | verified; 4 calls, 1 with a vote (see Step77) |
| 0x529ea0 | `CSolid::E43Vf74` | `scratch::accumulate` | ported; 133 calls (103 with the flush, fuzzed; see E43Vf74) |
| 0x52ac80 | `CSolid::E43Vf9C` | `layout::scale_add_rows` | bad |
| 0x52ad10 | `CSolid::E43VfA0` | `layout::solid_e43vfa0`, `rows` | verified; direct records cover the origin subtraction and matrix transform |
| 0x52ad40-0x52b380 | `E43VfAC`, `VfB0`, `VfBC`, `VfA4`, `VfA8`, `VfB4`, `VfB8` | `rows::*` (see Ported) | verified |
| 0x52c130 | `CSolid::E43Vf94` | `sleep::deactivate` (in-object part); whole: `layout::solid_sleep` | verified; whole on the stage's 3,138 placed solids (`load_stage`) |
| 0x52bf30 | `CSolid::E43Vf90` | `emit::emit_body_event`; on memory: `layout::solid_wake` | checked; 40 direct fuzzed calls (see E43Vf90); `solid_wake` is not reached on the stage; bound (`rbr-hybrid`, `e43vf90`): the tanner-damage steps reach it (`CCar::Vf6cx`), see `bound.csv` |
| 0x529af0 | `E47SolidScratch::Vf04` | `layout::store_scratch_reset` | verified |
| 0x52b930 | `E47_ScratchBaseInit` | `layout::store_scratch_base_init` | verified |
| 0x529a50 | `E47SolidState::Vf17x` | `rotation::refresh_rotation` | verified |
| 0x52b6b0 | `E47_Vf54Helper` (entity call, then the scratch reset) | `layout::scratch_reset_after_entity` (`host::Vf54Host`) | checked: 40 direct calls (`vf54h-direct-d1000.txt`, `fuzz_body_block`) for the reset; the entity call is the host's (`E43_EntityVf54Fwd` tail-jumps to slot 1 of the object at the body's offset 0x5c, `ret` on the tanner-damage steps). The scratch's slot 1 is taken as `E47SolidScratch::Vf04`; the binding declines another. Bound: see `bound.csv` |
| 0x52d280 | `E47SolidState::E47SolidState` | `layout::construct_state` | verified |
| 0x52b8b0 | `NewDef` | `layout::new_def` | verified |
| 0x529820, 0x529940 | `CDef::Unk20`, `CDef::Vf4c` | `-` (not ported) | - (reached) |
| 0x529840 | `CSolid::CDef::CDef` | `layout::solid_def_ctor` (the base is `collision::state::body_def_ctor`, the tweaks are `ntweak`'s) | 40 direct fuzzed calls (`fuzz_cdef`, `cdef_ctor_matches_original_calls`) |
| 0x52d640 | `CSolid::CDef::Vf50` | `layout::cdef_new_solid` (unit test only) | verified |
| 0x529950 | `E47_SolidDefDtor` | `-` (not ported) | verified |
| 0x52d480 | `CSolid::CSolid` | `layout::construct_solid` (the base `CBody::CBody` through `collision::layout::ctor` and its `CtorHost`) | verified; checked in the car and wheels by `load_defs::car_instance_matches_the_game` |
| 0x52b6d0 | `CSolid::Vf40` | `layout::vf16_helper` | - (reached) |
| 0x52a0e0 | `CSolid::E43Vf128` | `impulse::impulse`, `layout::impulse_tail` | checked; 40 direct fuzzed calls on the arithmetic path, 40 with the hook and the ring tail, 40 with the ring tail alone (see E43Vf128); the hook path also by the v1 drives' chains |
| 0x529100 | `E46_RingAppend` | `layout::ring_append` | checked; 80 direct fuzzed calls, 28 more from E43Vf128's ring tail |
| 0x52a570 | `CSolid::E43Vf140` | `response::response` | checked; the root path has direct fuzz records and the parent path is replayed through the original slot 80, `ComposeA` and `MB560` calls |
| 0x52be70 | `CMultiSignal1<CSigDampingCollision>::Emit` | `physics_core::signal::emit` | checked on the longer contact-list chains; guard and out-of-range log paths decline |
| 0x52b990 | `CSolid::E43Vf138` | `dot::e43vf138` | verified; 36 calls, 8 with dp=1 and 24 with dp=0 (fuzzed, see Dot60 and E43Vf138) |

### Step functions (call leaves; the per-step path)

| Address | C++ name | Rust name (`-`: no Rust item yet) | status |
| --- | --- | --- | --- |
| 0x52cc30 | `CSolid::E43Vf6C` (`Vf27x`, body_integrate) | `integrate::integrate` | ported; 2400 calls: no-f8 2130, clear-lin 195, clear-both 75, sleep trip 4 (see E43Vf6C) |
| 0x52cb20 | `CSolid::E43Vf64` (`Vf25x`, body_force_s64) | `force::force_pass` | partial; 245 calls (45 snapshots, 200 direct fuzzed), none with a contact list (see E43Vf64) |
| 0x52c700 | `CSolid::Vf44` | `layout::place_solid` (the damage-ops branch, flag 8 with no hook, is `Err`) | checked on the stage's 3,138 placed solids (`load_stage`) |
| 0x52c220 | `CSolid::BigB` | `contact_list::contact_list_step` | ported; 82 calls (54 with a contact list, fuzzed; see BigB) |
| 0x52b9f0 | `CSolid::BigA` | `contact::contact_step`, `layout::contact_worker` | checked (287 calls: 69 hits, 35 with the sample sink; see BigA) |

Order of work: Leaves first (sleep, pose, window, vote, math), then Vf27x/Vf25x. The state and def leaves
are plain field reads; they come last and need the most typed layouts.

## Conflicts found while planning (for the lead)

- `notes/progress/gameplay-functions.csv` marks `0x52ac80` and `0x52ad10` as `bad` (e2e-sweep). The brief says every
  reached non-exact function is verified, so the gate is not met for these two.
- `BigA` (0x52b9f0) and `BigB` (0x52c220) are `bad (harness)` in `notes/physics/rigid-body.md` but `verified` in the
  progress file (different runs). Re-check before trusting either.
- `Vf27x` (0x52cc30) is `verified` in progress, but `notes/physics/rigid-body.md` calls it `called only` and marks
  suspect 5a open (reads `this+0x1f0`/`+0x210` where the asm reads `+0x200`/`+0x220`). Our port follows the asm.
- Five reached functions have no row in `gameplay-functions.csv`: `0x5297d0`, `0x529820`, `0x529840`, `0x529940`,
  `0x52b6d0`. They have no progress status.

## Ported

| Address | Function | Port | Vectors |
| --- | --- | --- | --- |
| 0x43c6e0 | `E01Mat3::SetFromQuat` (quaternion to matrix) | `math::quat::from_quat` | 40 direct fuzzed calls (`setfromquat-direct-d1000.txt`, `fuzz_quat_mat`); no children |
| 0x483dd0 | `E15Shell::M83DD0` (quaternion product) | `math::quat::mul` | 512 calls |
| 0x4921a0 | `E18_QuatNormalize` | `math::quat::normalize` | 512 calls |
| 0x4e4c70 | `E34Quat::Norm` | `math::quat::normalize` (same math, output store) | 2 calls (the VfEx contact-step records) |
| 0x529200 | `CSolid::E43Vf20` (slot 8, a bare jump to `CEntity::E43Vf20`, 0x51a3a0) | `layout::entity_vf20_jump` (partial: the walk is the host's) | unit test only (`entity_vf20_jump_passes_through`); the walk needs a tree node the step supplies |
| 0x529340 | `CSolid::E43Vf70` (accumulate, slot 28) | `force::accumulate_force` | 40 direct fuzzed calls (`e43vf70-direct-d1000.txt`, `fuzz_accum340`): 27 with the slot-0 flush, 13 without |
| 0x529a50 | `E47SolidState::Vf17x` (rotation matrix from the orientation, times the diagonal when the gate is positive) | `rotation::refresh_rotation`, `layout::refresh_state_rotation` | 40 direct fuzzed calls (`vf17x-direct-d1000.txt`, `fuzz_state`): 24 with the gate on (the `E46Mat::Mul` child, through `MulHost`), 16 off; the drive does not take the gate |
| 0x529af0 | `E47SolidScratch::Vf04` (scratch reset, slot 1) | `layout::store_scratch_reset` | 40 direct fuzzed calls (`e47vf04-direct-d1000.txt`, `fuzz_block`: the 0x150-byte block random); the tail-jumped clear is its own record |
| 0x52ac80 | `CSolid::E43Vf9C` (slot 39, `Vf39x`) | `layout::scale_add_rows` | 40 direct fuzzed calls (`vf39x-direct-d1000.txt`, `fuzz_vf39`), windows checked |
| 0x52ad40 | `CSolid::E43VfAC` (row loop over the +0x110 matrix) | `rows::transform_rows`, `layout::transform_body_rows` | 40 direct fuzzed calls (`e43vfac-direct-d1000.txt`, `fuzz_acb`); each row has a `CopyFrom` child |
| 0x52add0 | `CSolid::E43VfB0` (three-lane row loop) | `rows::transform_rows3`, `layout::transform_body_rows3` | 40 direct fuzzed calls (`e43vfb0-direct-d1000.txt`, `fuzz_acb`); each row has a `CopyFrom` child |
| 0x52aec0 | `CSolid::E43VfBC` (origin-folded row loop) | `rows::fold_origin_rows`, `layout::fold_body_rows` | 40 direct fuzzed calls (`e43vfbc-direct-d1000.txt`, `fuzz_acb`); no children |
| 0x52afd0 | `CSolid::E43VfC0` (origin-folded row loop, reordered sums) | `rows::fold_origin_rows_reordered`, `layout::fold_body_rows_reordered` | 40 direct fuzzed calls (`e43vfc0-direct-d1000.txt`, `fuzz_acb`); no children |
| 0x52b110 | `CSolid::E43VfA4` (quaternion rows plus the vector at +0x330) | `rows::quat_plus_rows`, `layout::quat_plus_body_rows` | 40 direct fuzzed calls (`e43vfa4-direct-d1000.txt`, `fuzz_quat_rows`); children CopyFrom and SetFromQuat |
| 0x52b1f0 | `CSolid::E43VfA8` (quaternion rows less the vector at +0x330) | `rows::quat_diff_rows`, `layout::quat_diff_body_rows` | 40 direct fuzzed calls (`e43vfa8-direct-d1000.txt`, `fuzz_quat_rows`); child SetFromQuat |
| 0x52b2e0 | `CSolid::E43VfB4` (quaternion rows) | `rows::quat_transform_rows`, `layout::quat_transform_body_rows` | 40 direct fuzzed calls (`e43vfb4-direct-d1000.txt`, `fuzz_quat_rows`); children CopyFrom and SetFromQuat |
| 0x52b380 | `CSolid::E43VfB8` (quaternion rows, the rotated sums) | `rows::quat_rows_b8`, `layout::quat_body_rows_b8` | 40 direct fuzzed calls (`e43vfb8-direct-d1000.txt`, `fuzz_quat_rows`); children CopyFrom and SetFromQuat |
| 0x52b460 | `CSolid::E43VfC4` (quaternion rows, the offset fold) | `rows::quat_fold_rows_c4`, `layout::fold_quat_body_rows_c4` | 40 direct fuzzed calls (`e43vfc4-direct-d1000.txt`, `fuzz_quat_rows`); child SetFromQuat (once per call) |
| 0x52b580 | `CSolid::E43VfC8` (quaternion rows, the offset fold, other order) | `rows::quat_fold_rows_c8`, `layout::fold_quat_body_rows_c8` | 40 direct fuzzed calls (`e43vfc8-direct-d1000.txt`, `fuzz_quat_rows`); child SetFromQuat (once per call) |
| 0x52b6d0 | `CSolid::Vf40` (`E47_Vf16Helper`) | `layout::vf16_helper` | 40 direct fuzzed calls (`vf16-direct-d1000.txt`, `fuzz_vf16`), 48 product calls replayed; the tail's walk and `CBody::CState::Unk44` replayed from their records |
| 0x52b740 | `CSolid::Vf24x` (sleep step) | `sleep::step` | 932 calls: accum 83, off 693, reset 131, trip 25 (`--fuzz` seeds 1-4, 6, and the unfuzzed step 40) |
| 0x52b8b0 | `CSolid::NewDef` (the definition block) | `layout::new_def` | 40 direct fuzzed calls (`newdef-direct-d1000.txt`, `fuzz_newdef`): `operator new` (0x70bec2) gives the block (its size is not recorded), the `CDef` constructor (0x529840) is the port's `solid_def_ctor`, checked by its window |
| 0x52b930 | `E47_ScratchBaseInit` (scratch base) | `layout::store_scratch_base_init` | 40 direct fuzzed calls (`e47base-direct-d1000.txt`, `fuzz_block`); the nested `Vf04` is a child record |
| 0x52c130 | `CSolid::E43Vf94` (slot 37), in-object part | `sleep::deactivate`; the whole function is `layout::solid_sleep` (`sleep::fall_asleep`, `CEntity::E43Vf94` and the sleep event through `RestHost`) | the trips' windows (`IsActive`, the armed flag); the whole on the stage's placed solids (`load_stage`) |
| 0x52c510 | `CSolid::Vf81x` (contact vote) | `vote::vote`, `layout::contact_vote` (BigA per record through `ContactVoteHost`) | 1264 calls: the step records (no contact list) and 160 direct fuzzed calls (`g-contacts-vote-*`, `g-contacts-force-*`, `fuzz_vote`/`fuzz_contacts`): 124 with a list (269 contacts), 51 hits |
| 0x52c700 | `CSolid::Vf44` (a placement's pose) | `layout::place_solid`: orientation (`DbgFlags` 0x400, else the identity), extent (its fourth word, the state's scale gate, is 1.0 unless the extent is (1, 1, 1)), hook, position, sample tweak, grid cell (`LookupTwin`, `move_to_cell`), collision grid, then slot 16 through `PlaceHost` | 3,138 solids of track-54_E against snap-0 (`load_stage`) |
| 0x52c8f0 | `CSolid::VfC8` (pose step) | `pose::pose_step` | 864 calls: parent 690, root 174 (unfuzzed steps 40 (3 files), 80, 572, 1295, and fuzz seeds 4-7 of step 40) |
| 0x52cb20 | `CSolid::E43Vf64` (force pass) | `force::force_pass` | 245 calls: snapshots (`e43vf64-snap*.txt`, 45 calls: no list, flag 8 on 9) and direct fuzzed (`e43vf64-direct-d1000.txt`, 200 calls, `fuzz_force`) |
| 0x52cc30 | `CSolid::E43Vf6C` (integrator pass) | `integrate::integrate` | 2400 calls: snapshots (`e43vf6c-snap*.txt`, 45 calls: no-f8 36, keep 7, clear-both 2) and direct fuzzed runs (`e43vf6c-direct-*.txt`: clear-lin 195, clear-both 75, sleep trip 4) |
| 0x52cfd0 | `CSolid::VfDx` (back step) | `window::back_step` | 106 calls: child 26, root 24, done 56 (entry dumps `RBRRecompSnapAt-0052cfd0-{1,2}`, fuzzed) |
| 0x52d110 | `CSolid::VfEx` (advance the rest of the window) | `window::advance_rest` | 89 calls: child 12, root 20, done 57 (entry dumps `RBRRecompSnapAt-0052d110-{1,2}`, fuzzed with root and progress choices) |
| 0x52d640 | `CSolid::CDef::Vf50` (the body) | `layout::cdef_new_solid` (unit) | unit test only (`cdef_new_solid_builds_on_the_heap_block`): the `CSolid` constructor (0x52d480) runs on a block the recipes can't build (the recorded call faults in the constructor's base chain) |
| 0x52d280 | `E47SolidState::ctor` (the state, `E47SolidState(owner, name, unused)`) | `layout::construct_state` | 40 direct fuzzed calls (`e47-state-ctor-direct-d1000.txt`, `fuzz_state_ctor`): the NTweak base and the tweak members are `ntweak`'s constructors (the port's), the four vectors cleared by the port; bound, not reached on the snapshots |

### E43Vf6C (integrator pass, `integrate.rs`)

`Vf27x` (0x52cc30, popped 4, dt). In the asm's order: `w += angular_accel * dt` (0x190 += 0x320) and `v += linear_accel *
dt` (0x1c0 += 0x380), each lane `mulps` then `addps` with the instruction's NaN rules (`math::sse`: lane w carries
the original's stack residue, and a woken placed solid takes it to `inf + -inf`, the negative indefinite NaN); then the flag-0x8 clears (|v|^2 below 1e-5 zeroes v, and |w|^2 below it zeroes w only when v was
cleared, the asm's nesting); `VfC8(dt, flagA, flagB)` (`pose_step`, `PoseHost`); the estimates `(v - prev) / dt` (0x230,
0x240) and their filters `0.2 * old + 0.8 * est` (0x250, 0x260); the history copies `prev <- v, w` (0x200, 0x220); the
entity walk (`CEntity::E43Vf6C`, 0x51a330, host); `Vf24x` (`sleep::step`, and on a trip `sleep::deactivate` plus the host's
slot-37 rest); and the state signal (`CMultiSignal1<CSigMrRBodyState>::Emit`, 0x51f700, host).

- Asm order: the estimate subtracts lanes x, y, z only, so lane w keeps the old estimate's value before the scale (the
  scale is `mulps` over all four lanes). The clear test is `comiss` with the threshold first: a NaN speed is not cleared.
- The state payload (the emit's stack buffer, 0x30 bytes): owner at +0x00, position at +0x10, orientation at +0x20. The
  C++ (`RBR_E47_Big.cpp`, `sig + 0x14`) is 4 bytes lower; the asm's `CopyFrom` destination is `lea 0x34(%esp)` after a
  `pushl`, which is +0x10 of the buffer (`cpp-fixes-from-rust.txt` item 24). The port follows the asm.
- Vectors: 45 calls from the snapshots (`vectors/rigid_body/e43vf6c-snap*.txt`, the step snapshots 300-11000, no fuzz).
  The test checks the branch-by-branch body bytes (the walk's refresh and the root link masked), the VfC8 host calls in
  order (the pose test's `Replay`), the walk's `dt`, and the emit payload (`StateSignal`).
- Direct runs: `harness/direct_calls.py SNAP OUT --set rigid-body --addr 0x52cc30 --this 0bf1d7b8 --count 40 --seed S`
  (the `direct_args` dt, the `fuzz_integrate` recipe; the x87 oracle needs `RBR_REMOTE`). Only this call is fuzzed there.
- Not fuzzed in a step run: `--fuzz` on a step also fuzzes the nested `VfC8` (the fuzz applies to every listed call
  outside Step/StepBasic/FlagDispatch), and the restored inputs do not match this body's windows. So the clear-lin and
  sleep-trip records come from the direct runs above only.

### E43Vf64 (force pass, `force.rs`)

`Vf25x` (0x52cb20, popped 0). In the asm's order: the entity walk (`CEntity::E43Vf64`, 0x51a2c0, `entity_walk`; its children
come before its record); the contact vote (`Vf81x` with two vectors in the pass's frame and range 1.0, `ForceHost::vote`)
and on a hit its two vectors applied through slots 28 and 29, then the body read again (`reread`); with flag 0x8, `BigB`
(`contact_list_step`) and then the body is read again (`reread`,
since BigB's applies write the accumulators at +0x310 and +0x370); then `linear_accel = MB560(Scale(force, inverse_mass))`
and `force = 0`; then `angular_accel = MB560(Mul(scratch, local inverse inertia))` and `scratch = 0`.

- The MB560 calls take the body as ecx (the slot-12 `S30` before them returns the body, 0x519c00) and the E43PairWalk
  global as the third argument. The pass's own calls follow the vote's (its record's inner records, which come first in
  the pass's) and the `E36Collect` record: the collector's children come before its record, so they are not the pass's.
  The binding makes the walk, the vote, its applies and the two transforms at the original's stack (`frame` is the aligned
  locals: the vote's two vectors, then the transforms' two sources, 16 bytes each from offset 0x10); BigB's calls use
  temporaries.
- Vectors: 45 snapshot calls (`vectors/rigid_body/e43vf64-snap*.txt`) and 200 direct fuzzed calls
  (`e43vf64-direct-d1000.txt`, `fuzz_force`: the flag, the force, the scratch, the inverse mass and inertia; the sleep-armed
  word stays 0), and 80 direct fuzzed calls with contact lists (`g-contacts-force-a{1,2}.txt` from the BigA entry dumps,
  `fuzz_force` with `fuzz_contacts`: 65 with a list, 24 vote hits). The test answers the vote from its record (the last
  `Vf81x` on the body before the pass), checks the body bytes (the BigB retire's writes masked), the MB560 sources in
  order, and the counts of the retires, applies and emits.
- Not reached: a contact list inside this pass (BigB's applies, the `reread` after them, and the retire mask). No snapshot
  has one, and the direct runs fuzz only the target, so `E36Collect` returns 0. BigB itself is covered by
  `contact_list_matches_original_calls`.

### E43Vf70 (accumulate, `force.rs`)

`Accum340` (0x529340, popped 4, slot 28). The force (`+0x370`, four lanes) gets `pp * scale` (`+0xd0`) added lane by lane,
the C++'s `v*k + old`. Then, when `(q | b) & 0x4000` (E43Vf74's gate, see that section), the body's slot 0 runs
(`CEntity::Vf00`, 0x519950): the flush, the `FlushHost` call.

- Vectors: 40 direct fuzzed calls (`e43vf70-direct-d1000.txt`, `fuzz_accum340`: the scale, the force, the vector and the
  gate words). 27 record the flush (the slot-0 child, with the body as its argument) and 13 do not. The test takes the gate
  bit from the flush record, since the gate words are not windows; E43Vf74's test checks the gate expression itself.
- Not checked: the flush's result (E43Vf70 is void and the original does not use it).

### E47SolidScratch::Vf04 (scratch reset, `scratch.rs`)

Slot 1 of the scratch block (`CSolid+0x280`, 0x150 bytes; `m_scratch.Vf04()` from the body's constructor, and from
`E47_ScratchBaseInit`). It clears ten vectors (`E09Vec4::Clear` each, 16 bytes) and three words: the four words at
`+0x20` (`m_zero20`) and the vectors at `+0x60, 0x70, 0x80, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0x100, 0x120`, and the words at
`+0x110, 0x130, 0x134`. The layout lists these (`store_scratch_reset`, the one memory write for the reset). The pointers at
`+0x140..0x14c` and the 1.0 at `+0x10` are left as they are.

- Vectors: 40 direct fuzzed calls (`e47vf04-direct-d1000.txt`, `fuzz_block`: the whole block random). The clears are
  children (nine of them); the tenth, at `+0x120`, is a tail jump, so the record has it as its own top-level call
  (`E09Vec4::Clear`, 40 records). The test checks the root's post window, which holds all of the block.
- Not checked: the snapshot calls (none reached by the snapshots); the caller's ctor and the base-init.

### E47_ScratchBaseInit (scratch base, `layout/scratch_state.rs`)

`E47_ScratchBaseInit` (0x52b930, `ecx` = the scratch block at the body's +0x280). It sets the vtable word (0x73fb6c), `1.0`
at `+0x10`, clears the vectors at `+0x90` and `+0xf0`, then runs `Vf04` (`store_scratch_reset`). The write is one
`layout/scratch_state.rs` store (R3), so `scratch.rs` has no code for it.

- Vectors: 40 direct fuzzed calls (`e47base-direct-d1000.txt`, `fuzz_block`). Each has the nested `Vf04` (nine clears) as a
  child and the tail clear at `+0x120` as its own record. The test checks the root's window, which covers the block.
- Not checked: the caller's constructor (`CState::Ctor`, 0x5215a0) and the body's constructor (0x52d480).

### E01Mat3::SetFromQuat (quaternion to matrix, `math/quat.rs`)

`E01Mat3::SetFromQuat` (0x43c6e0, ecx = the matrix, one stack argument: the quaternion). The reference forms the nine
products `x*q[i]`, `y*q[i]`, `z*q[i]` and writes `1 - 2*(a + b)` or `2*(a - b)` into the row-major matrix. The port is
`math::quat::from_quat`, with the same products and sums in the same order.

- Vectors: 40 direct fuzzed calls (`setfromquat-direct-d1000.txt`, `fuzz_quat_mat`): random quaternions (not unit) and a
  random matrix. The test writes the port's nine entries and checks the output window. 40 calls match.
- Not checked: the caller's loops (0x52ad40–0x52b380), which the C++ inlines, and `E01Mat3::SetFromQuat` as the
  caller of `Vf17x`.

### E43VfAC (row loop over the matrix, `rows.rs`)

`E43VfAC` (0x52ad40, popped 8: the rows and the count). Each row is replaced by the body's matrix times its first three
lanes, in the packed order the reference uses: `(c0 + c1) + c2` lane by lane, the matrix's columns from `+0x110` (the
fourth lane's `m[9]` is at `+0x134`). The port is `rows::transform_rows`; the memory is `layout::transform_body_rows`.

- Vectors: 40 direct fuzzed calls (`e43vfac-direct-d1000.txt`, `fuzz_acb`: a random matrix and 1 to 4 random rows). Each
  row has a `E01Vec4::CopyFrom` child (0x43c5b0, the row copied into a stack temporary). The test checks the body window
  and the rows. 40 calls match.
- The matrix and the origin are read again for each row (`layout::update_body_rows`), as the asm reloads them in the loop
  (`movups` at the top of the row loop, 0x52ad70). A row that overlaps the matrix sees the stores made before it.

### E43VfB0 (three-lane row loop, `rows.rs`)

`E43VfB0` (0x52add0, the same arguments as `E43VfAC`). Each row's `x, y, z` are replaced by the matrix times them, with
the scalar chain's sums: `x = (m0 x + m1 y) + m2 z`, `y = (m3 x + m4 y) + m5 z`, `z = (m8 z + m6 x) + m7 y`. The fourth lane is
not written. The port is `rows::transform_rows3`, through the same `layout` round trip as `E43VfAC`.

- Vectors: 40 direct fuzzed calls (`e43vfb0-direct-d1000.txt`, `fuzz_acb`). Each row has a `CopyFrom` child. 40 calls match.
- The matrix is re-read for each row, as in `E43VfAC`.

### E43VfBC and E43VfC0 (origin-folded row loops, `rows.rs`)

Both take `(rows, count)` like `E43VfAC` and fold the body's origin (`+0x140..0x148`) into the fourth lane. Their `x, y, z`
sums come from the asm, which the C++ transcription did not match:

- `E43VfBC` (0x52aec0): `f1 = (m0 x + m3 y) + m6 z`, `f2 = (m1 x + m4 y) + m7 z`, `f3 = (m2 x + m5 y) + m8 z`; the row's
  `x, y, z` become `f1, f2, f3`; `w = 0 - ((o.y f2 + o.z f3 + o.x f1) - w)`.
- `E43VfC0` (0x52afd0): `x = (m2 z + m1 y) + m0 x`, `y = (m5 z + m4 y) + m3 x`, `z = (m7 y + m6 x) + m8 z`, all from the old
  row; `w = 0 - ((0 - w) - ((o.y y + o.z z) + o.x x))`. The C++ transcription has `m[9]` where the asm has `m[7]`
  in the `z` sum; the port follows the asm, checked against its disassembly and the 40 calls.

- Vectors: 40 direct fuzzed calls each (`e43vfbc-direct-d1000.txt`, `e43vfc0-direct-d1000.txt`, `fuzz_acb`). Neither has
  children. 40 calls match each.
- The matrix is re-read for each row, as in `E43VfAC`.

### E43VfA4, E43VfA8, E43VfB4 (quaternion row loops, `rows.rs`)

Each row loop rebuilds the rotation matrix of the body's start orientation (`+0x2a0`, `SetFromQuat`, 0x43c6e0) for every
row and applies it to the row's `x, y, z`. `E43VfB4` is the plain product; `E43VfA4` adds the vector at `+0x330` to
`x, y, z`; `E43VfA8` subtracts it first and applies the transposed sums (`m2 d0 + m1 d1 + m0 d2` and so on).

- The fourth lane of the packed product uses the matrix's word after the nine (`lm[9]`). `SetFromQuat` never writes it, so
  the original reads whatever stack word the caller left there. Every recorded row reads it as 0 (the `w` lane is then
  `(m3 x + m6 y) + 0 z`), so the port uses 0 (`UNSET_MATRIX_WORD`). If the game ever has a nonzero word there, the `w` lane
  of `E43VfA4` and `E43VfB4` would differ; no record shows it.
- Vectors: 40 direct fuzzed calls each (`e43vfa4-direct-d1000.txt`, `e43vfa8-direct-d1000.txt`,
  `e43vfb4-direct-d1000.txt`, `fuzz_quat_rows`: a random quaternion, the vector, and 1 to 4 random rows). 40 calls match each.

### E43VfB8, E43VfC4, E43VfC8 (quaternion row loops, `rows.rs`)

- `E43VfB8` (0x52b380): the matrix is rebuilt per row (`SetFromQuat` in the loop) and applied with the sums
  `x' = (m2 z + m1 y) + m0 x`, `y' = (m5 z + m4 y) + m3 x`, `z' = (m8 z + m7 y) + m6 x`. The fourth lane is left as it was.
- `E43VfC4` (0x52b460) and `E43VfC8` (0x52b580): the matrix is built once before the loop, and the offset (`+0x330..0x338`)
  is read per row. `C4`'s packed sums are the matrix's columns: `x' = (m0 x + m3 y) + m6 z`, `y' = (m1 x + m4 y) + m7 z`,
  `z' = (m2 x + m5 y) + m8 z`. `C8` uses the `B8` sums. The fourth lanes fold the offset against the old row:
  `C4`: `0 - ((o.x x + o.y y + o.z z) - w)`; `C8`: `0 - ((0 - w) - ((o.x x + o.y y) + o.z z))`.
- The C++ transcription of `C4` and `C8` did not match the asm (the columns and the fold were in other orders, and `C8`'s
  `x` and `z` sums were transposed); the port follows the asm, checked against its disassembly and the 40 calls each.
- Vectors: 40 direct fuzzed calls each (`e43vfb8-direct-d1000.txt`, `e43vfc4-direct-d1000.txt`, `e43vfc8-direct-d1000.txt`,
  `fuzz_quat_rows`). 40 calls match each.
- The `lm[9]` word is not used by these three (they stop at `m[8]`).

### E47SolidState::Vf17x (rotation matrix, `rotation.rs`)

Slot 17 of the state at the body's `+0xe0`. It builds the matrix at the state's `+0x30` (the body's `+0x110`, the matrix the
row loops read) from the orientation at `+0x20` (`SetFromQuat`). When the gate at `+0x7c` is positive, it multiplies that
matrix by `diag(+0x70, +0x74, +0x78)` through `E46Mat::Mul` (0x528c20, physics-core: ported as `physics_core::layout::mat_mul`, called through `MulHost`, R4) and stores
the product.

- Vectors: 40 direct fuzzed calls (`vf17x-direct-d1000.txt`, `fuzz_state`: the orientation, the gate on about half the
  calls, the diagonal). The `SetFromQuat` child is pure (`from_quat`), so it is not replayed. The test replays the 24
  recorded `E46Mat::Mul` children through the host and checks their inputs. 40 calls match.
- The drive does not take the gate on the recorded steps; only the fuzzed calls cover the product.

### E47_Vf54Helper (`layout/scratch_state.rs`, partial)

`E47_Vf54Helper` (0x52b6b0, `ecx` = the body) makes one entity call and then the scratch reset. The call is
`E43_EntityVf54Fwd` (0x519ca0): a tail-jump into the slot 1 of the body's `+0x5c` object, which is not ported and has no
record (the recorder lists nothing for it). The reset is the tail jump to the scratch's slot 1, `E47SolidScratch::Vf04`,
which `store_scratch_reset` ports. The port makes the entity call through `host::Vf54Host` and then resets the block at
`+0x280`.

- Vectors: 40 direct calls (`vf54h-direct-d1000.txt`, `fuzz_body_block`: the body's scratch block random, except its
  vtable word, which the tail jump reads). The test counts one entity call and checks the body window, which is the reset
  alone: the entity call wrote nothing in the body in any of the 40 calls. The row is `partial` for that reason.

### E43Vf90 (body event, `emit.rs`)

`body_emit_a` (slot 36, 0x52bf30, no arguments). In the SSE build's asm order: `SetDirty` on the tweak at `+0x1d0` (its
`+0x1d8` word is set to 1), then `+0x1dc` (the active flag) is set to 1; `SetDirty` on the tweak at `+0x1e0` (`+0x1e8` = 1),
then `+0x1ec` is set to 0; the armed word at `+0x274` is stored as 0; the body's slot 18 (`CSolid::S18`, 0x529dc0); the host
notify (`CEntity::E43Vf90`, 0x519a60, which calls `WallRel` only without a parent); then the 0x50-byte event (kind 1, the
owner at `+0x30`, the position and the orientation, each twice) through `CMultiSignal1<const CSigMrRBodyEvent&>::Emit`
(0x51f7c0, the signal at `g_signalManager+0xec98`).

- The asm, not the C++ in `src/rbr`, decides the order: the event's owner is read after the notify.
- Slot 18 copies the pose into the window start (`+0x330`, `+0x2a0`, ...). The port reads the body as slot 18 left it: the
  test puts slot 18's post-window into memory first, because the port does not run slot 18.
- Vectors: 40 direct fuzzed calls (`vectors/rigid_body/e43vf90-direct-d1000.txt`, `fuzz_emit`), all matching. No snapshot
  step reaches the call (`harness/vcu_vectors.py --set rigid-body` reports it as not reached).
- The `SetDirty` calls are the in-object writes above; `E43Vf90` does not list them as externals.

### E43Vf140 (slot-80 response, `response.rs`)

`CSolid::E43Vf140` (0x52a570, popped 8, `(point, out)`): the body's velocity at a point. With no parent (`+0x44` = 0) it
writes `(w * d + v) - w * d` lane by lane, `w` the angular velocity (`+0x190`), `d` the point less the position (`+0x140`),
`v` the linear velocity (`+0x1c0`), in the C++'s order (`(w1*d2 + v0) - w2*d1`, and so on). Asm and C++ agree on the
terms; the test checks the bytes.

- With a parent the call takes the parent's slot 80 (`E47S80_2::S80`, `ResponseHost::parent_response`), `ComposeA` of the
  point (`compose`) and the same term with the composed point, transformed by the parent (`parent_transform`, MB560), and
  adds the transform to the parent's response. The binding replays those calls from the original's aligned frame slots;
  longer chains exercise this path.
- Vectors: 40 direct fuzzed calls (`vectors/rigid_body/e43vf140-direct-d1000.txt`, `fuzz_vf140`: a body in the stub heap
  with no parent). No snapshot step reaches it. The test skips root records that lack the body or the output window, and
  any body with a parent.

### E43Vf128 (impulse, `impulse.rs`)

`CSolid::E43Vf128` (0x52a0e0, popped 0x20: the point at arg 1, the impulse at arg 2, then the hook and tail arguments).
The original's calls, in order (the direct records): `Cross` of the lever arm `a - position` with the impulse (`E01V3::Cross`,
pure); `S30` (the body, returned); `MB560` with the E43PairWalk global as ecx, the torque as the source and the body as the
matrix; `E01Vec4::Mul` of that result with the local inverse inertia (`def+0x130`, pure); `S30`; `MB560` with the body as
ecx, the scaled torque as the source and the global as the matrix. Then `w += 0.6 * T` (`+0x190`) and `v += impulse *
InverseMass` (`+0x1c0`, `def+0x16c`).

- The port makes the two transforms through `ImpulseHost` (`transform_by_body`, `transform_by_walker`). The test checks each
  source against the port's, and the body's bytes exactly.
- The tail (`layout::impulse_tail`, after the body is stored): with a hook (at 0x3cc), the body's slot 12, `ComposeA` of
  the point with its pose into the frame's 0x70, then the hook's slot 1 with `(c, d, word, f, g, that vector, point, h)`,
  where `word` is `e[0]` when 0.5 (0x795e18) is above `e[2]`, else `e[1]` (`impulse::hook_takes_first`). With the ring bit
  of the physics global's debug word (the block at 0x24, its word at 0x160e0, bit 0x1000000) and a ring tail (at 0x3c8):
  `ComposeA` of the point with the body, then `E46_RingAppend(that vector, 0, 1.0, 0)`. `frame` is the original's esp after the
  prologue, `((entry - 4) & !0xf) - 0x80`; the binding runs the transforms there too.
- `E46_RingAppend` (0x529100, `layout::ring_append`): the three words at 0x410 plus 12 per index, the vector at
  16 per index plus one; a wrapping ring (its first word) steps the index mod 64 and raises the count to the index plus one, a
  non-wrapping one stops at 64 (`impulse::ring_step`).
- Vectors: 40 direct fuzzed calls (`vectors/rigid_body/e43vf128-direct-d1000.txt`, `fuzz_vf128`: a body in the stub heap
  with its vtable copied from the snapshot's body, the definition at `heap+0x2000`, hook and tail null). No snapshot step
  reaches it in these records. The tail: 40 direct calls on the hinged bodies of tanner-damage 12000 (one free, three
  owned; the car's damage hook, 0x4f12a0), the hook dropped or a ring tail added on some (`g-hinge3-vf128.txt`,
  `harness/vector_specs/g_hinge3.py`): 26 hook calls, 23 ring appends. 40 more with a ring tail and no hook
  (`g-v1stops-ring-d1000.txt`, `harness/vector_specs/g_v1stops.py`, the flag set on most; there `E46_RingAppend` is a
  call of its own, so the test runs the port's). The hook path is reached by the v1-gravel 2326 and v1-snow 1851 chains.
  The ring append alone: `g-hinge3-ring.txt` and the direct calls of `g-v1stops-ring-d1000.txt`.

### BigA (contact worker, `contact.rs`)

Calls outside the body (`ContactHost`), in the asm's order: `E33Mat34::ComposeA` (0x51b4c0) on the contact's vector; the
body's slot-80 response (0x51e5c0, the three floats `x, y, z`); `E43PairWalk::MB560` (0x51b560) on the scaled normal
(its matrix is the body, `S30` = 0x519c00 returns `this`). The test host takes each result from the original's record
(the first ComposeA, the first slot-80 call, the MB560 whose `a` argument is this call's). The port's source for MB560
is checked (x, y, z). `CFloat3::Length` (0x473b30), `CFloat3::Scale` (0x473cd0) and `E01V3::Cross` (0x43c630) are
pure and ported inline.

- Sample sink (the hook object non-null and flag 8), after the cross product: `0x4e6000` with the contact's vector and
  two stack words it writes (`ContactHost::sample`), then `Length` of the transform's source, then the slot-2 virtual of
  the hook object (target 0x4ec140, `ContactHost::sample_hook`) with the two words, the contact's id word, that length,
  `len2`, and pointers to the contact's vector, the frame point and the offset (divided by `len2` when above 0.0001; its
  lane w is the stack's). The test takes the two words from the hook's record and checks the id, length and `len2`; the
  hook's own effects are the host's.
- MB560's fourth lane (`a.w`) is the original's stack garbage: the port's `w` lanes of the scaled normal and the offset
  are not the original's, so `a.w` comes from the host. Only x, y, z of the source are checked.
- Coverage: the 42 calls include 12 hits, 17 calls on the second force branch (`e+0x14 > 0`) and 23 on the first, 11
  with `e+0x10 == 0`. Earlier counts (before the fuzz was biased toward the second branch) had fewer second-branch
  calls. Not counted separately: each early return and the reach clamp.
- Field names are by offset (`f10`, `f14`, `f18`, `f1c`, `point`, `normal`, `vector`): their meaning is open.
- The tuning float (`g_1659b70 -> +0x24 -> +0x1608c`) is a tweak the test reads from memory (a node window); its name is
  open.
- Direct vote runs (`g-contacts-vote-a{1,2}.txt`: `direct_calls.py RBRRecompSnapAt-0052b9f0-N.bin out --set rigid-body --addr
  0x52c510`, seeds 7101 and 7102, `direct_vote` and `fuzz_vote`) and force passes (`g-contacts-force-a{1,2}.txt`, `--addr
  0x52cb20 --this 0c521048`, seeds 7201 and 7202) add 269 BigA calls on fuzzed lists (`fuzz_contacts`: one to three records
  built on the stack far below the call), 33 of them with the sample sink.
- Calls in the vector set come from the load-time entry replay (two recorded calls, `RBRRecompSnapAt-0052b9f0-{1,2}`)
  and 40 fuzz seeds: `vcu_vectors.py RBRRecompSnapAt-0052b9f0-1.bin out --set rigid-body --fuzz SEED`. `--fuzz` applies
  the spec's `fuzz` key to the entry replay too (`run_entry`). The seeds' BigA records are the same as the earlier
  wrapper's, except for the inner E43Vf9C records (0x52ac80), which the merged `entities` set listed and this set does not.

### BigB (contact list step, `contact_list.rs`)

Called from `CSolid::E43Vf64` (0x52cb20) whenever the body's flags have bit 0x8, after the vote (`Vf81x`), whatever the
vote returned. Calls outside the body, in the asm's order: `E36Collect` (0x4f3950, `collect`; the list and the count
come back in two cells, and a return of 0 ends the step); `E34_RetireA` (0x4e8fa0, `retire`, once per kind-200 record
when the body's flag 8 is set, before any other per-record call); then per record the slot-80 response (`response`),
MB560 on the force and on the torque (`transform`, the body as the matrix), and the body's slots 28 and 29
(`apply_force`, `apply_torque`); finally `Emit` (0x52be70, `emit`, on the event). `CFloat3::Length`, `Scale` and
`E01V3::Cross` are pure and shared with BigA.

- E36Collect's argument names are swapped in the source. The asm pushes `&dc` (the list pointer) as its second stack
  argument and `&d8` (the count) as its third; the source passes `&d8` then `&dc`. The port follows the asm.
- Constants come from the PE `.rdata`: factor `f*0.5` clamped to [0, 1] (a NaN passes through, as the asm's compares
  do), force scale 5000, length floor 1e-4, length clamp 10, spring weight `*5` clamped to 1, spring `-50`.
  The spring is `(weight * 5) * len * -50` in that order (the asm); the source's `-50 * spring * len` rounds differently.
- The normal is taken before the length clamp; the spring uses the clamped length.
- The event (`Emit`'s argument): weight (`+0x00`), kind (`+0x04`), the record's `aux` (`+0x10`), the record's point
  (`+0x20`), the response to it (`+0x30`). The C++ names `+0x14`, `+0x24`, `+0x34`: the asm's `CopyFrom` destinations
  are 4 bytes lower (a push comes before each `lea`), as the Emit window shows. The event goes to `Emit` once, after the
  loop. The record is the last one with a positive weight (the first record when none has one), as the asm's loop keeps
  `best`.
- The slot-80 target of BigB's body is `CSolid::E43Vf140` (0x52a570): both the vtable at 0x73fc54 and the one the dump
  has (0x738b44) list it at slot 80. BigA's dumps call the thunk at 0x51e5c0, which jumps to it. The test takes the
  response from either address.
- The fourth lane of the response is the original's stack garbage (the response writes three floats), so the port's
  `w` lanes of the response, the normal and the event point are not the original's. Only x, y, z are checked.
- Field names are by offset (`direction`, `aux`, `weight`, `kind`, `factor`): their meaning is open. `aux` and `contact`
  are names by what the event copies them into.
- Vectors: the two entry dumps (`RBRRecompSnapAt-0052c220-{1,2}`, E36Collect returns 0 in both) and 80 fuzz seeds on
  them (`vcu_vectors.py <dump> out --set rigid-body --fuzz SEED`). The fuzz makes E36Collect return 0 or 1 to 3 random
  records (`collect_result`, the spec's `result` key: the call is not run, its outputs are written); the body's flag 8
  is random. 54 seeds have a list. The test checks each call: the port's response and MB560 sources against the
  original's, the retire, transform and apply counts, the event against Emit's argument, and that the body's bytes match
  except those the original's callees write.
- Callee writes (masked, the port writes no body bytes): `E34_RetireA` takes the body out of its cell and the active set
  (`+0x14..+0x1a`, IsActive at `+0x1dc` and `+0x1ec`) when it is called; the slot-28 and slot-29 applies write the
  accumulators at `+0x310..+0x320` and `+0x370..+0x380`.
- Not recorded: E36Collect returning non-zero with zero records (the original then emits the event buffer's stack
  garbage, the port emits zeros; the fuzz never draws it), and a list from the real world (the fuzz writes the list).

### E43Vf74 (scratch accumulate, `scratch.rs`)

Slot 29 (the BigB torque apply, 0x529ea0, popped 4). The body's scale (`+0xd0`, `m_fD0` in the C++) times the vector
is added lanewise to the scratch at `+0x310` (the asm's `mulps`, then `addps`). Then, when the gate has bit 0x4000, the
body's slot 0 runs: `CEntity::Vf00` (0x519950), which returns the entity's name and writes nothing. The port takes the
gate as one word, `q | b`: `q` is the word at `*(app+0x24) + 0x160f4` and `b` is `app+0x38`, with `app` the global at
0x1659b70 (the asm tests `(q | b) & 0x4000`). The caller reads these; the port does not.

- Vectors: 133 calls, 103 with the flush: 106 in the 80 BigB fuzz seeds, and 27 in the 8 gate-fuzz seeds of step 40
  (`fuzz_gate` draws q and b from 0, 1, 0x4000, 0x4001). The flush record is the slot-0 call the original makes just
  before E43Vf74's line (its args are the body).
  The test checks the flush against that record, the scratch and the other windows (the body, the vector, the gate
  words). The record of the flush has no windows: it writes nothing, which the entity's code shows.
- Not checked: the flush's result (E43Vf74 is void and the original does not use it).
- With `--fuzz` the gate fuzz changes step 40's path for some seeds: seeds 1-3 and 8 of snapB's step 40 then record no
  VfC8. Seeds 4-7 keep it. The cause is not traced; the unfuzzed step is the one for the VfC8 vectors.

### Dot60 and E43Vf138 (`dot.rs`)

Dot60 (0x52a500, x87) is the dot of two cross products scaled by 0.6, with the three floats `c`. The game's x87 runs in
single precision (the recorded st0 values have 24-bit mantissas), so the port is `f32` in the asm's order: the lanes
`mulps` by the scale, the sum is `(o2.y*c1 + o2.z*c2) + o2.x*c0`, then `*0.6`. Lane 3 of the temporaries is never read,
so the stack words the caller leaves there are not inputs.

- Vectors: 12 calls, recorded from the entry dumps of Dot60 and E43Vf138 (`RBR_SNAPAT` in a drive run; the callers run
  on the drive, at steps the snapshots do not cover). The Dot60 calls inside the E43Vf138 dumps are top-level records,
  since Dot60 is listed.
- E43Vf138 (0x52b990, popped 24): when `dp` is non-zero, `f` += Dot60(a, c); then `e` = (Dot60(a, b) + `e`) + `k` (the
  def's float at +0x16c). The port returns both outputs.
- `dp == 0` is fuzzed (the spec's `fuzz_args` sets argument 4 to 0 or 1): the 24 calls with dp=0 check `f` unchanged
  and `e` from `Dot60(a, b)` alone.

### Step77 (`vote.rs`)

Step77 (0x529d30, popped 8) votes on the position `this+0x140` shifted by `*b` on z. The vote is S00 of the global at
0x7c755c (0x4fe810), behind `VoteHost::vote`. On a vote, `*a` is the output's int (+0x20) and `*b` is scaled by its
float (+0x30); otherwise `*a` is 1.0 and `*b` is unchanged. eax is the vote flag.

- Vectors: 4 calls from the entry dump of `CWheel::E43Vf28` (0x53f230, `RBR_SNAPAT`; Step77 can't be transplanted, so
  its caller is): 3 with no vote, the drive's own, and 1 with a vote from `--fuzz 5` (the vote's result key, see
  `harness/vector_specs/rigid_body.py`, `vote_result`). The vote is replayed from its recorded inner call (position,
  point and output checked).
- Not recorded from the drive: a vote. The drive's three wheel calls return 0 at the snapshot steps, so the vote-yes
  path is one fuzzed record (random index in 0..8, scale in -2..2).

### Host calls (`PoseHost`)

`VfC8` makes calls outside the body. The port takes them through `PoseHost`, in the original's order, with the
arguments the port computes. The vector test checks each host call against the recorded callee and its arguments.

- Root bodies: `CCellGrid::LookupTwin(x, y)` (0x4e03b0), then `CCar::AppendA980(link)` (0x4ea980) with `link = this+0x160`.
  AppendA980 writes the link node (`this+0x160..+0x16b`) and the cell's neighbours and counts. The test masks the link
  bytes (`CELL_LINK`) and checks the rest; the neighbours are outside the body.
- Child bodies: the parent's slot 0x30 (S30, three stack words) and `E33Mat34::ComposeA` (0x51b4c0). The result is not
  in any window, so the test's host returns zeros. It only feeds the contact sample. The transform runs when bit 0x4 of
  the parent's flags byte at `+0x60` is set: the asm tests `testb $0x4, 0x60(%ecx)`. The C++ reads `pi + 6` (+0x18),
  which the asm does not; the port follows the asm. All 690 child records have the bit set.
- The contact sample: `CDriveLine::SampleBig(pos, value)` (0x50b300) returns a float on the x87 stack. The test
  returns the recorded value (`Inner::st0_bits`).
- `E09Vec4::Clear` (0x465d80) records between LookupTwin and SampleBig are made inside the cell code, not by VfC8.
  `E01Vec4::CopyFrom` (0x43c5b0, the position copies before LookupTwin) and `CFloat3::Length` (0x473b30, in the sample
  path; the SampleBig argument is the body's `+0x17c`, not its result) are not host calls either. The replay skips all
  of them, unchecked.
- The tweak `SetDirty` on `CSolid+0x170` (0x52f450) is in-object (`m_dirty` at `+0x178`); the port sets it directly.
- Cell registration (`CellHost`, root bodies only): `VfC8`, `VfEx` and `VfDx` register the body in its cell after the
  position moves. The link node is `this+0x160` in all three: the original's `CCar::AppendA980` argument in VfDx is
  `lea eax, [esi+0x160]` (0x52d0d3); the C++ now uses this offset too (item 13).
- State refresh (`StateHost`): `VfEx` and `VfDx` end with the body's slot 16 (`CSolid::Vf40`, 0x52b6d0), which calls
  the entity walkers (`CEntityFrame::Vf04`, `E43PairWalk::WFE0`/`WB320`, `CEntity::E43_Walk1F0`, `CEntity::Vf40`). The
  port does not model it: the host does the work, the test masks the bytes it writes (`REFRESH_WRITES`: `+0x80..+0xa0`
  and `+0x110..+0x140`). The port's own writes (orientation, position, progress) are checked exactly.

### Not verified (no recorded calls)

- `CSolid::Step77`: the vote-yes path has one record (fuzzed, see Step77). Its other callers, `CSolid::E43Vf28`
  (slot 10, 0x52c620, when `this+0x30` is non-zero) and `CCar::Vf28x` (0x4ea020), are not recorded.
- `CSolid::VfEx` and `CSolid::VfDx`: the entry dumps are child bodies (`advance,child`, `back,child`; their linear velocity
  is zero, so they do not test the position step). The root path and the `done` branch are fuzzed (`fuzz_window`: the
  parent is cleared on half the seeds, progress and `a` vary). The dumps come from count-run entry dumps (`RBR_COUNT=1
  RBR_SNAPAT`); their "step" field is the function's entry address (0x52d110, 0x52cfd0), not a physics step number.
- `CSolid::BigB` contact path: verified on fuzzed lists only (54 seeds, see BigB). The real E36Collect output has not
  been recorded: a drive with a body in contact with the world would show it. E36Collect with zero records and a
  non-zero return is not drawn by the fuzz; the original's emit there reads the event buffer's stack garbage.
- `CSolid::Vf81x` list path and BigA's sample sink: checked on fuzzed lists (the direct runs above) and the tanner-damage
  12000 chain; the lists the game builds were not recorded beyond the two BigA entry dumps.
- Root-body cell registration (`CellHost`): checked in `VfC8` (161 root calls), `VfEx` (20) and `VfDx` (24): the
  `LookupTwin` and `AppendA980` arguments, and the registered position (the post-move `x` and `y`).
- VfEx's move: the original's position step is `pos += v * (1 - progress) * a` (the full `s`, asm 0x52d1ef-0x52d233); the
  turn is the half-turn of the angular velocity `w * (s * 0.5)`. The reconstructed C++ now uses full `s` for the move and
  half `s` for the quaternion. VfDx's root path now appends the link at `this+0x160` (asm 0x52d0d3); the Rust port follows
  the same asm and root-path records.
- BigB's fuzzed list (E36Collect's records) is placed in the collector's own list (`args[0] + 0x90`, as the original sets
  `*out = p + 0x90`). An earlier fuzz wrote them below the list cell, on the stack, where BigB's callees overwrote them
  before BigB read them: that was a harness error, not the original's behaviour. The stack-placed records were removed
  from the store and regenerated; the clobber skip was dropped when nothing needed it.

### Unverified

- `VfC8`, child body with the contact sample (`f8=1` with a parent): the sample's position is the parent-frame
  image (S30 + ComposeA), which the test can't see. The sample value itself is the recorded one.
- `VfC8`, child body whose parent's flags byte at `+0x60` has bit 0x4 clear: the C++ reads an uninitialised stack word
  for the sample's position. The port uses the new position (`next`). Not recorded (all 690 child records have the bit).
- `VfC8`, `a <= 0` (the early return) and `b == 0` with `c == 0` other than the one recorded call: the caller's
  arguments, which the fuzz can't change.
- `E34Quat::Norm` (0x4e4c70): verified through the VfEx records (2 calls). Its math is `E18_QuatNormalize` (the same
  operations, the asm reads the same lanes), so `normalize` is verified through E18.
- `Vf24x`: trips change the rest of the step (the body leaves the active set, so the step makes fewer calls); the trip
  window masks `+0x14..+0x19`, the cell removal (`0x519a80` -> `0x5276a0`) when the body has no parent.

### E47 slot helpers (`layout/`)

- `scale_add_rows` (0x52ac80, `Vf39x`): `E46Mat::ScaleRows` (0x528dd0) on the matrix at `+0x110`, then `E46Vec::Add` (0x528b00)
  on the origin at `+0x140`, over the same rows. Both are the physics-core math (`Mat::scale_rows`, `Vec4Rows::add_each`), so the
  port makes no host call; the test checks the rows and the return value (`dst`) against the recorded windows. `dst` must not
  overlap the matrix or the origin (the recorded calls never do).
- `vf16_helper` (0x52b6d0, `Vf16Helper`): `refresh_state_rotation` on the state at `+0xe0`, twice, then the tail jump to
  `CBody::Vf40` (0x4e1120), which is ported (`collision::body::vf40`). The recipe gives the body a heap copy with no parent
  and no children: the snapshot's tree calls slot 16 on its children, which runs this helper again. The entity walk
  (0x51a4b0) is a record of its own, before the helper's record. Its `Unk44` tail call (0x4e33e0) is the owner's slot 12
  and the body's embedded objects' slot 17; the recipe stubs slot 12 with the bare `ret` at 0x401040, and the `+0x38` slot 17
  (its `Unk44` would write the walk's child pointer at `+0x48`).
- `entity_vf20_jump` (0x529200, slot 8): a bare `jmp` to `CEntity::E43Vf20` (0x51a3a0, the entity walk; `entities::tree::walk_gated`
  is the port, bound through the host). The jump keeps the stack argument and returns the walk's result. Unit test only.

- `new_def` (0x52b8b0, `CSolid::NewDef`): the heap block of `DEF_SIZE` (0x1f0) from physics-core's `Heap` trait, then the
  `CDef` constructor (`solid_def_ctor`, 0x529840, the port's: the body definition's base, then the tweaks). A null
  block runs no constructor and returns 0.
- `construct_state` (0x52d280, `E47SolidState::ctor`): the NTweak base (`ntweak::construct_base`), the vtable at +0 after it,
  the member tweaks at +0x10, +0x90, +0xa0, +0xd0, +0xf0, +0x100, +0x110 and +0x130 (`ntweak`'s constructors), the owner kept
  only with bit 2 of its flags, the four vectors cleared, and the scalars. `E09Vec4::Clear` is the port's `clear_vec`, not a
  host call.
- `cdef_new_solid` (0x52d640, `CSolid::CDef::Vf50`): the block of `SOLID_SIZE` (0x3d0), then the `CSolid` constructor
  (0x52d480, through `SolidCtorHost`; `layout::construct_solid` ports it, not bound). Unit test only: the recorded call faults inside the constructor's base chain
  (a virtual call on an object the recipe can't build), so no direct vectors yet.

### Assumptions

- `UNSET_MATRIX_WORD` (`rows.rs`, the tenth matrix word that the row loops read): the C++ `SetFromQuat` writes nine words,
  and the asm reads a stack word after them that nothing writes. Every recorded call reads 0, so the port uses `0.0`.
  If a record ever shows a non-zero value at that word, the quaternion row loops are wrong there.

## Floats and bits (rule 7)

- `math/rsqrt.rs`: `rsqrtss` reads its operand's bits (sign, exponent, top mantissa bits pick the table entry), so
  it is ported on `to_bits`. Its NaN and zero results are the reference CPU's (`0xFFC00000` for a negative operand).
- No other ported site reads float bits as integers. Float-to-int: none.
- Platform default NaN: none tolerated yet. The recorded NaN cases are not in any set.

## Unknown meaning

- `Body.state_owner` (`CSolid+0x30`): the first word of the state signal `E43Vf6C` emits. The body's own field there is
  not named by any key or note; the consumer of `CSigMrRBodyState` decides what it is.

- `Body.sleep_armed` (`CSolid+0x274`): `Vf24x` reads it as a flag, `Vf81x` reads it as a pointer to the contact list
  (count at +4, `layout::CONTACT_LIST`). The two readings are not reconciled.
- `Body.sampled` (`CSolid+0x17c`): the value `CDriveLine::SampleBig` returns at the body's position, with the body's
  flags bit 0x8. It is the `LinearThresholdVelocity` tweak's value slot (`0x170`), not a known key.
- `Body.body_flags` (`CSolid+0x60`): bit 0x8 takes the sample; bit 0x4 is the contact advance (`notes/physics/rigid-body.md` 2b).
- `Body.progress` (`CSolid+0x290`): the fraction of the window taken. VfEx sets it to 1, VfDx to `a`.
- `Body.start_orientation` (`CSolid+0x2a0`) and `Body.start_position` (`CSolid+0x330`): the pose VfDx blends with (`a` of
  the current pose, `1 - a` of these). Names are ours, from that blend; no key or RTTI name.
- `Body.is_active` (`CSolid+0x1dc`) is named by the state key `IsActive`; the port only clears it in slot 37.
- The sleep step's `sleep_armed` (`CSolid+0x274`) is shared with `Vf81x`, which reads that word as a pointer to a
  contact list (count at +4). The notes give it as a flag. Vf24x treats it as a flag (non-zero = armed).
