# Collision

Source: `src/rbr/RBR_E33_*`, `RBR_E34_Wall.cpp`, `RBR_E36_Collide.cpp`, `RBR_E37_*`, and `RBR_E38_*`.
Notes: `notes/physics/collision.md`, `ground.md`, `contacts.md`, `tweak-keys.md`, and
`notes/physics/renames/collision.csv`. The asm and the original-call vectors decide float order,
branch behavior, and writes. Raw game offsets belong in `layout/`; the port functions take typed
values and return typed results.

## Recorded-call plan

The Tanner/Corolla drive reaches 134 functions in this scope (`notes/progress/called.csv`). The
Rust batch is ordered from small leaves to the collision step. Each listed function gets at least
one original-call vector before it is marked covered; records from all available snapshots are
needed for infrequent branches. The current C++ statuses are 113 `verified`, 20 untouched, and
one `fixed`; these describe the behavior reference, not Rust coverage.

| Batch | Reached functions | Role |
|---|---|---|
| C1: vectors, cell grid, and body setup | E33Vec4 `MinMaxA` (0x4e0ae0), `Scaled` (0x4e0700), `Min` (0x4e0750), `Max` (0x4e0780), `AllBelowZero` (0x4e07b0), `BoxHit` (0x4e0e40); `E33Dict::Unk18/Unk0C/Unk10` (0x4e0b70/0x4e0bd0/0x4e0c00), `E33MakeEvent` (0x4e0d00); `CBody::CDef` ctor/dtor, `CState` ctor/Unk44/Uninit/accessors, body ctor/Vf40, scratch helpers, and `CColGrid::SampleA` (0x4e0fd0-0x4e1200, 0x4e33e0, 0x4e3460, 0x4e0840); `CBodyLink::Unlink/NotifyFiltered`, `CCellLink` ctor, `CCellGrid::LookupTwin`/ctor (0x4e0020/0x4e0080/0x4e01d0/0x4e03b0/0x4e0550) | Shared typed primitives and layout used by the later batches. |
| C2: body collision and brake walls | `CBody::VfCC/VfE8/VfF4/VfD0/VfD8/VfDC/ClearGrid/Vf44/Vf11C` (0x4e1230/0x4e2f50/0x4e2e90/0x4e1eb0/0x4e3070/0x4e2850/0x4e11a0/0x4e1140/0x4e1200); `CBrakeWall` count, query, and slots (0x4e3810-0x4e3f10); `CBrakeWallManager` and `E34WallVf` setup, query, and raycast (0x4e40d0-0x4e49c0) | Wall broadphase and segment contact path, plus body/grid bookkeeping. |
| C3: contact collection | `E36SphereBox`, `E36ObbTest`, `E36Collect` (0x4f2ce0/0x4f3030/0x4f3950) | Candidate body tests and collection. |
| C4: event and list leaves | E37 plane/event/list/pool functions (0x4f40f0, 0x4f4390, 0x4f4a20, 0x4f4bc0, 0x4f4d40, 0x4f4dc0-0x4f56c0) | Event creation, filtering, and list lifetime. |
| C5: grid and edge contacts | E37 element, grid, collision, and solver helpers (0x4f5a90-0x4f9cd0) | Cell sync, edge insertion, candidate walks, contact tests, and material tagging. |
| C6: collision solver leaves | E38 vector kernels (0x4fcc70-0x4fd030), mesh and ray helpers (0x4fac50-0x4fb110, 0x4fd760/0x4fde40), SAT and pair helpers (0x4fd230-0x4fd480), solver passes (0x4fb220-0x4fca20), and collision-system entry points (0x4fd5f0-0x4fe810) | Low-level arithmetic first, then event solve and the collision-system pass. |

The per-function exact order will follow recorded direct calls and the asm-reviewed dependencies.
Constructor and load-time calls use entry dumps when ordinary step snapshots do not reach them.

## Names and unknowns

Use the names in `notes/physics/renames/collision.csv` where available. Unnamed fields keep a
descriptive working name and are listed here as their meaning is established. The overlapping
E34 wall point reads use the original 8-byte stride; do not normalize them into a 32-byte record.
The E38 ray worklist's source contains uninitialized-array reads (`collision.md`, S1 and S2), and
E36Collect passes indeterminate scales in the C++ reference (S3); vectors must preserve the
reference-visible results on recorded inputs rather than repair those behaviors.

## Renamed in the typed pass

| Address | C++ name | Rust item (`ports.csv`) |
| --- | --- | --- |
| 0x4fcc70 / 0x4fd030 | E38Vec::Scale / M_D030 | `math::vec::scale` |
| 0x4fccb0 | E38Vec::M_CCB0 | `collision::layout::vec3_add_sub` (`(a + b) - c`, memory edge over `math::vec`) |
| 0x4fcdd0 | E38Vec::M_CDD0 | `collision::layout::vec3_add` (`a + b`) |
| 0x4fcd10 | E38Vec::M_CD10 | `collision::layout::vec3_sub` (memory edge over `math::vec::sub`) |
| 0x4fcd60 | E38Vec::M_CD60 | `collision::layout::vec3_sub_dot` (memory edge over `math::vec::sub` and `dot`; store first) |
| 0x4fcee0 | E38Vec::M_CEE0 | `collision::layout::vec3_dot` (memory edge over `math::vec::dot`; st0 return) |
| 0x4fcf30 | E38Vec::M_CF30 | `collision::layout::vec3_cross` (memory edge over `math::vec::cross3`) |
| 0x4fcfa0 | E38Vec::M_CFA0 | `collision::layout::vec3_cross_dot` (memory edge over `math::vec::cross3` and `dot`; store first) |
| 0x4fd0f0 | E38_004FD0F0 | `collision::layout::vec3_normalize` (memory edge over `math::vec::Vec3::normalize_sse`) |
| 0x4fd230 | E38Ops::M_D230 | `collision::e38::world_axes_overlap` |
| 0x4fd2c0 | E38Ops::M_D2C0 | `collision::e38::box_sat_overlap` |
| 0x4fd480 | E38Ops::M_D480 | `collision::e38::ray_triangle` |
| 0x4fac50 | E38Ops::M_AC50 | `collision::e38::mesh_ray_test` (vertices and tags read lazily by index); `collision::layout::mesh_test` is its memory edge, used by SegmentSweep |
| 0x4fad10 | E38_004FAD10 | `collision::layout::store_zero_mesh_scratch` |
| 0x4fb330 | CCollisionSolver::CDef::Unk20 | `collision::e38::shared_solver_table` |
| 0x4fb720 | E38_004FB720 | `collision::e38::pick_material_tag` (the store, manager read and `Tag520940` call stay in the binding) |
| 0x4f5530 | E37VecSet::DotX | `collision::vec_set::cross_then_dot` |
| 0x4f5630 | E37VecSet::Store5630 | `collision::vec_set::cross_from_origin` |
| 0x4f5d00 | E37Compose9 | `collision::elem::triangle_plane_terms` |
| 0x4f5a90 | E37Tri5A90 | `collision::elem::segment_triangle` |
| 0x4f56c0 | E37Run56C0 | `collision::elem::ray_triangle_distance` |
| 0x4e0750 / 0x4e0780 | E33Vec4::Min / Max | `math::vec::min_lanes` / `max_lanes` |
| 0x4e0820 | CColGrid::Flag68A | `collision::grid::status_flag_bit1_clear` (byte: `layout::ColGrid::status_flag`) |
| 0x4e0830 | CColGrid::Flag68F | `collision::layout::tag` |
| 0x4e0a20 | CColGrid::ReadClear | `collision::grid::read_clear` (taken with `layout::ColGrid::take_event_flags`) |
| 0x4e0a50 / 0x4e0a60 | CColGrid::StoreVec / EdgeAt | `collision::layout::vector` / `layout::edge` |
| 0x4e0a70 / 0x4e0a80 / 0x4e0ad0 | CColGrid::Flag68D / 68B / 68C | `collision::layout::edge_count` / `active` / `pair_count` |
| 0x4e0a90 / 0x4e0ab0 | CColGrid::EdgeForA / EdgeForB | `collision::grid::edge_for_a` / `edge_for_b` |
| 0x4e11b0 / 0x4e11e0 | CBody::Vf10C / Vf114 | `collision::body::has_grid` (Vf114 is an identical twin) |
| 0x4e11c0 | CBody::Vf110 | `collision::body::grid_active` (the byte is read only when a grid is attached) |
| 0x4e11f0 / 0x4e1200 | CBody::Vf118 / Vf11C | `collision::layout::grid` / `layout::bounds` |
| 0x4e1230 | CBody::VfCC | `collision::layout::aabb_pretest` (corners through the host's `FetchX` into the frame; the test is `collision::body::aabb_pretest`) |
| 0x4f6550 | E38RaySeeds | `collision::layout::ray_seeds` (the two `E09Vec4::Clear` calls through `e38::RayHost`) |
| 0x4f6740 | E38RayClear | `collision::layout::ray_clear` (eax left as the caller had it) |
| 0x4f7090 | CColGrid::SyncGrid (binding `E37WallSync`) | `collision::layout::sync_grid` (`a` is the transform's ecx; `GridSyncHost`; unit test `grid_sync_caller_logic`, rust_step identity; with the walk on game memory on the stage's placed solids, `load_stage`) |
| 0x4fd760 | M_D760X (`FUN_004fd760`) | `collision::layout::mesh_ray_query` (nearest face of the record list; `e38::ray_triangle` and `e38::box_sat_overlap` on values; the surface byte is the `e38::SampleHost` call (0x520240, ecx = 0x16592b8); the constants are read from the image: 2.0, 1.0, 0.0, 1/15, 1/31; normal normalised with `rsqrtss`) |
| 0x4fde40 | E38Ops::M_DE40X (`FUN_004fde40`) | `collision::layout::mesh_ray_worklist` (the worklist over the tree with a `Vec`; `e38::box_sat_overlap` and `e38::ray_triangle` on values; the half direction is `direction * 0.5`, and the table is `this + 0xc + 4 * (word >> 22)`) |
| 0x4fdd60 | E38NodeWalk (`FUN_004fdd60`) | `collision::layout::node_walk` (leaf collection on an explicit stack, box tests through `e38::world_axes_overlap`; the 257-entry frame is a `Vec`) |
| 0x4ff070 | E39_SegTest70 | `collision::layout::segment_test` (the query box `e38::segment_box`, midpoint and half span, in the original's frame; then `node_walk`) |
| 0x4fe2d0 | E38Ops::M_E2D0 | `collision::layout::point_triangle` (`e38::point_triangle`: a point 1e-5 to 0.5 behind the face whose foot lies inside it; the normal's length and the reciprocals x87 at PC24 (`round24`); the fields written before a miss are stored) |
| 0x4febc0 | E38Ops::M_EBC0 | `collision::layout::face_query` (the first face of the record list that `point_triangle` hits, which runs in Rust; the surface bytes are `e38::SampleHost` calls (0x520240) at the original's esp; channel and nibble sums in asm order) |
| 0x4fad90 | E38SegmentSweep (`FUN_004fad90`) | `collision::layout::segment_sweep` (cell query through `e38::CellQueryHost`, 0x5240c0 a host call; each flagged object through `layout::mesh_test`, which is 0x4fac50). Identity checked on the nine snapshots and by `segment_sweep_tests_flagged_meshes`; vector records are blocked, because the E45 cell query (0x5240c0) has no recipe that fills its cell lists at call entry |
| 0x4fb7f0 | E38_004FB7F0 | `collision::layout::clear_contact_rows` (eax: the count as last read) |
| 0x4fbc20 | E38_004FBC20 | `collision::layout::refresh_events` (`event::refresh_materials`, then 0x4f4ac0 below; slot 0x13c through `e38::SolverHost`, gate `e38::movable`) |
| 0x4f4ac0 | E37Flag4AC0 | `collision::layout::mark_suppressing_materials` (`physics_core::material::tag_entry` for `Tag520940`) |
| 0x4fbca0 | E38_004FBCA0 | `collision::layout::event_count` |
| 0x4fc0b0 | CCollisionSolver::Vf08 | `collision::layout::drain_events` (`list::drain`; declines an event whose slot 0x1c is not `CCollisionEvent::Vf1Cx`) |
| 0x4fc590 | E38_004FC590X | `collision::layout::correct_penetrations` (per event `e38::plan_correction`, two-sided split `e38::split_offsets`; the asm, not the C++: the margin -0.001 is kept for the rest of the pass once a two-sided contact sets it, and the body-only rows are built with side 1. `M_BD30` (0x4fbd30) and `E38_004FB890` (0x4fb890) are host calls at the original's stack pointers, so their bindings run) |
| 0x4fbd30 | E38Ops::M_BD30 | `collision::layout::contact_rows` (row in `e38::contact_row`; the material through `E38_004FB720` and `tag_entry`; `ComposeA` (0x51b4c0) through `e38::ImpulseHost` into the original's frame. The asm, not the C++: side 0 stores its ids in the order 0x90, 0x8c, side 1 in the order 0xa4, 0xa8) |
| 0x4fb890 | E38_004FB890 | `collision::layout::apply_offsets` (`e38::offsets_move`: the sum's length on the x87 against the double 1e-5; the projections unrolled by four sum `z, x, y` for the first of each four and the rest, `z, y, x` for the others) |
| 0x4fbfc0 | E38Ops::M_BFC0X | `collision::layout::add_body_offset` (called only by 0x4fc590) |
| 0x4fca20 | E38_004FCA20 | `collision::layout::solve_due_events` (`e38::event_due`; slots 0x13c/0x80/0x8c/0x90 and `M_C1A0` (0x4fc1a0) through `e38::SolverHost`) |
| 0x4fc1a0 | E38Ops::M_C1A0 | `collision::layout::solve_event` (gate `e38::impulse_due`, `e38::slip`, `speed_blend`, `friction`, the x87 `normal_impulse`, `friction_impulse`; `PlaneDiff` 0x4f40f0, `Query4390` 0x4f4390 and `Emit4DC0` 0x4f4dc0 through `e38::ImpulseHost` with the vectors in the original's frame. The relative velocity's lane 3 is the stack's, as in the original. The empty `E21Telem::Telem80` (0x499e80) is left out) |
| 0x4e1eb0 | CBody::VfD0 | `collision::layout::body_vf_d0` (the walks `slot_contacts`, `point_contact`, `edge_contact` and `probe`; the contact math in `collide_big`: `midpoint`, `probe_direction`, `plane_offset`, `edge_ratio`. Every callee is a `collide_big::VfD0Host` call at the original's stack pointer with its vectors in the original's frame; `CopyFrom` and `CFloat3::Length` are the Rust ones. `EdgeEdge` (0x4f8280), `E33MakeEvent`, `SampleA/B`, `Normalize`, `Add`, `Sub`, `FetchY`, `CombineTwin`, `ComposeB` and `CollideInit` are host calls) |
| 0x4f40f0 | E37EvOps::PlaneDiff | `collision::layout::relative_point_velocity` (`event::relative_velocity`; each side's slot 0x140 through `event::PlaneDiffHost` into the original's frame, `E09Vec4::Clear` of a missing side's in Rust; lane w of `out` is not written) |
| 0x4e3970 | CBrakeWall::VfF0 | `collision::layout::brake_wall_vf_f0` (`CE36SubBlock::Init` and `CopyFrom` in Rust; the wall's slot 0xe8 through `wall::VfF0Host` at frame - 0x1c. The asm, not the C++: `d` gets the fourth vector's `y`, `e` the second vector) |
| 0x4e3a10 | CBrakeWall::VfE8 | `collision::layout::brake_wall_vf_e8` (`FetchX` of both ends through `wall::VfE8Host` at frame - 0xc; the segment against each edge of the wall's leaf list, `wall::sweep` and `wall::edge_cross`: the x87 length and cross term at 24 bits, the rest SSE) |
| 0x51b460 | E37E43::M_B460 | host call `GridSyncHost::copy_transform` (its walker 0x51b320 is inside the host; not ported) |
| 0x4e1120 / 0x4e1140 | CBody::Vf40 / Vf44 | `collision::layout::reset_state_and_base` / `layout::select_grid` (host calls in the original's order) |
| 0x4e11a0 | CBody::ClearGrid | `collision::layout::clear_grid` |
| 0x4f2ce0 / 0x4f3030 | E36SphereBox / E36ObbTest | `collision::layout::sphere_box` / `layout::obb_test` (separation and contact math in `e36::sphere_overlaps`, `sphere_contact`, `obb_overlaps`, `obb_contact` on an `ObbFrame`) |
| 0x4e33e0 | CBody::CState::Unk44 | `collision::layout::state_unka44` (wall sync through the host, with the sentinels stored first) |
| 0x4e1070 | CBody::CState::CState | `collision::layout::state_ctor` |
| 0x4e3460 | CBody::CBody | `collision::layout::ctor` (E43 base constructor through the host, then the state and grid) |
| 0x4e03b0 | CCellGrid::LookupTwin | `collision::layout::lookup_twin` (cell and link index from `grid::twin_cell`; the block through `grid::AllocHost`) |
| 0x4e0550 | CCellGrid::CCellGrid | `collision::layout::cell_grid_ctor` (table size from `grid::cell_counts`, `grid::table_bytes`) |
| 0x4e0840 | CColGrid::SampleA | `collision::layout::sample_a` (blend and index from `grid::blend_sample`, `grid::sample_index`) |
| 0x4f5ca0 | E36SelectTable | `collision::layout::select_table` |
| 0x4e0700 | E33Vec4::Scaled | `collision::layout::scaled` (the `math::scale_vec4` wrapper is gone) |
| 0x4f3950 | E36Collect | `collision::e36::collect` (the scratch frame is a parameter; host calls are `CollectHost`) |
| 0x4f5480 / 0x4f54b0 | E37Vec16::Min / Max | `collision::layout::vec16_min` / `vec16_max` (`Vec4::min_lanes` / `max_lanes` over the two loads) |
| 0x4fcd10 / 0x4fcdd0 / 0x4fd030 | E38Vec::M_CD10 / M_CDD0 / M_D030 | `collision::layout::vec3_sub` / `vec3_add` / `vec3_scale` (memory edges; the Vec3 math is `math::vec`) |
| 0x4fd230 / 0x4fd2c0 / 0x4fd480 | E38Ops::M_D230 / M_D2C0 / M_D480 | `collision::layout::world_axes_overlap` / `box_sat_overlap` / `ray_triangle` (the pure math stays in `e38`) |

## Unknown meaning (working names)

| Item | Field | Working name | What is known |
| --- | --- | --- | --- |
| `elem::triangle_plane_terms` | p5 | `cross_sum` | `n1 + (n2 + n3)` of the three cross products (asm-exact) |
| `elem::triangle_plane_terms` | p6, p7, p8 | `dot_p4_n1`, `dot_p4_n2`, `dot_p4_n3` | `p4 . n` for each cross product |
| `elem::triangle_plane_terms` | p9 | `dot_p3_n1` | `p3 . n1`, y and z summed first (x87 order) |
| `elem::segment_triangle` | +0x18, +0x1c | `u`, `v` | `f1` and `f1'` raw on a miss, scaled by `1/f2` on a hit; the tests check `0 <= f1 <= f2` |
| `e38::ray_triangle` | +0x20, +0x24, +0x28 | `distance`, `u`, `v` | Möller-Trumbore order: `dot(e2, q)/det`, `dot(s, p)/det`, `dot(q, d)/det` (inferred) |

| `layout::ColGrid::status_flag` | byte +0x68a | `status_flag` | Only the bit-1 test (`status_flag_bit1_clear`) is known; the byte's writers are not read |
| `grid::read_clear` | bytes +0x688 / +0x689 | `event_flags` | Returned as `(m_unk689, m_unk688)`; the header calls them `m_unk688/689`, and their meaning is not known |

## Still on raw memory (not yet converted)

These public functions still take `Mem` and addresses. The rest of the module is in the same state; the typed pass
continues file by file.

- `elem.rs`: `elem_5f20` and the E37 element accessors (`t68a` to `get_5a80`, `zero_5a70`, ...).
- `e36.rs`: `collect`, `active_shape`, `flag_bit2`, and `collide_init`.
- `state.rs` (`state_owner`, `state_uninit`, `body_def_ctor`, `body_def_dtor`, `scratch_ctor`), `list.rs`, `event.rs`, `wall.rs`, `links.rs`, `dictionary.rs`, `sort.rs`, and the E37 element
  accessors in `elem.rs`: every function not listed above.

## NaN and raw-bit notes

Record any output location where the tests accept any NaN in `notes/rust-physics.md`. For each
collision function, inspect the asm for integer operations on float bits and use `to_bits()` for
those branches. Float-to-integer conversions use `crate::fpu::trunc_i32` or `round_i32`; do not
cast floats with Rust's `as`.
