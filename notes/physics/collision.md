# Collision (E33 body tests, E34 brake walls, E36-E38 grids, kernels and solver): physics semantics

Pass 3 of the physics semantics brief (`lead/briefs/physics-semantics.md`), section 2 of this
pass. Notes only; no source edits. "Current" names are the C++ reconstruction's. "Proposed" names
are ours.

Coverage: this note is partial. The E36-E38 range is about 8000 lines and 126 functions in E38
alone. This pass covers the reached functions that decide contacts (section 3 says which were read),
the brake-wall path in full (it feeds the car's velocity, see `car-body.md` section 1a), and the
data layouts named by the headers. Everything not read is marked as such.

Confidence: `known` (exe or LSP names it, or the code is unambiguous), `inferred` (strong
argument from use), `guess`. Asm checked in this pass (`objdump -d -M intel` on the original exe,
VAs): `CBrakeWallManager::RaycastEntry` (0x4e4960, whole function), `E34WallVf::Raycast`
(0x4e4570, up to the first index mask and stride), `E37GridOps::M9B80` (0x4f9b80, whole function),
the call site of `E46Impulse::Impulse640` in `CCar::Vf6cx` (0x4ea39a), and the start of
`CCar::Vf6cx` (for the `dt` argument). `E33Vec4::BoxHit` is covered by `lead/briefs/cpp-asm-diffs.md`
item 2 and is not repeated here.

## 1. Overview

Collision has three parts, and they are not connected in one place.

**(a) Brake walls (E34): the car against the track's wall segments.** Each car has one
`CBrakeWall` (0x21c bytes, mesh at `+0x218`). Each step, `CCar::Vf6cx` calls
`CBrakeWallManager::RaycastEntry(car position, out)`. The position is the car's `(x, y)` only.
`E34WallVf::Raycast` tests the point against the wall's segments and, on a hit, writes a normal,
a penetration fraction and a flag word into `out` (28 bytes). `Vf6cx` then applies the hit to the
car (`Impulse640`, section 3, F2), emits `CSigCarBrakeWallPenetration`, and updates two car
timers (section 2a of `car-body.md`: `0x166c`, `0x9ec`). This path is the only contact response in
this pass that is traced all the way to a velocity change.

**(b) Body and grid contacts (E33 `CBody::VfCC`, E36 collect, E37 grids, E38 solver).** A body
(`CBody`, E33) has a box. The car's E37 grid object is at `CCar+0x64` (`E37GridOps`). Its grid elements
(`CColGrid`, 0x6b0 bytes each) hold the edges and walls of a region. The E37 kernels (edge-plane tests, pair walks)
build contact events (`CCollisionEvent`, 0xf0 bytes) in a pool. The E38 solver (`CCollisionSolver`,
0xc424 bytes; `CCollisionSystem`) turns the events into corrections. How an E38 event becomes a
force on the body is traced in `contacts.md` (section 1: `M_C1A0` -> `Emit4DC0` -> `Vf74x`).

**(c) Ground contacts** are not here. The wheel's ray probes (`E51Wheel::M3D520`) decide
`on_ground`, see `notes/physics/wheel.md`.

### 1a. Call tree (reached functions, direct calls from the C++)

```
CCar::Vf6cx (0x4ea300, per step)
  g_7c63c8 (CBrakeWallManager)::RaycastEntry (0x4e4960, asm checked)
    E34WallVf::Raycast (0x4e4570)                 segment test, normal, penetration
      CBrakeWall::QueryPoint (E33)                broadphase over the wall's cells
  E46Impulse::Impulse640 (0x529640)               velocity clamp (F2), asm call site checked
  Signal CSigCarBrakeWallPenetration (g_signalManager+0xa780)

CBody::VfCC (0x4e1230, E33)                       AABB pre-test of two bodies (section 3, F3)
CBody::VfE8 (0x4e2f50, E33)                       hit + grid bookkeeping (grid+0x688/+0x689 cleared)
E37GridOps::M9B80 (0x4f9b80, asm checked)         insert and tag edge material
E37GridOps::AddEdge (0x4f65b0)                    grid edge insertion (reached, not read)
E37GridOps::EdgeEdge (0x4f8280)                   edge-edge test (reached, not read)
E37ListOps::Drain (0x4f4ec0)                      event list drain (reached, not read)
E38Ops::M_* (0x4fac50-0x4febc0)                   solver kernels (reached, not read)
```

## 2. Data

### 2a. Brake wall output word (`out`, 28 bytes; `CBrakeWallManager::RaycastEntry`, `Vf6cx`)

| offset | current C++ name | proposed name | type | unit | meaning | confidence | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| +0x00 | `out[0]` | normal_x | f32 | - | unit normal of the hit segment (x) | inferred (strong) | `RBR_E34_Wall.cpp:355-357`; asm `0x4e4570` write order not read |
| +0x04 | `out[1]` | normal_y | f32 | - | unit normal (y) | inferred (strong) | same |
| +0x08 | `out[2]` | normal_z | f32 | - | always 0 (2D test) | known (the code writes 0.0f) | `RBR_E34_Wall.cpp:358` |
| +0x10 | `out[4]` | penetration | f32 | fraction (0 to 1) | `1 - perp/along`, or 0 when `along <= 0`; read by `Vf6cx` as the penetration (`0x166c`) | inferred (strong) | `RBR_E34_Wall.cpp:351-353`, `CarD.cpp:655` |
| +0x14 | `out[5]` | perp | f32 | m | signed distance of the point from the segment's line, along the normal (`nny*py + nnx*px`) | inferred (strong) | `RBR_E34_Wall.cpp:347-349`, `CarD.cpp:644` (the `n` argument of Impulse640) |
| +0x18 | `out[6]` (int) | flags | int | bits | bit 0: the index word's bit 15 (`>>15`), read by `Vf6cx` as the penetration gate; bit 1 (`0x2`): the previous hit's `m_mesh214` flag; bit 2 (`0x4`): segment bit 14 (see 2b) | inferred (strong for bit 0, guess for 1 and 2) | `RBR_E34_Wall.cpp:359-362`, `CarD.cpp:651` |

### 2b. Wall segment index word (`seg+5` table, `unsigned short`)

| bits | proposed name | meaning | confidence | evidence |
| --- | --- | --- | --- | --- |
| 0..13 (`& 0x3fff`) | vertex_index | index into the wall's point table at `mesh + 8*index` (asm checked, `0x4e45f2-0x4e4604`) | known (asm) | asm `0x4e45f2`, `0x4e45ff`, `0x4e4604` |
| 14 (`0x4000`, byte 1 bit 6) | reject_emit | when set, the hit emits `CSigCarBrakeWallPenetration` and returns 0 (no hit) | inferred | `RBR_E34_Wall.cpp:359-366` |
| 15 (`0x8000`) | gate_flag | copied to `out[6]` bit 0, the penetration gate in `Vf6cx` | inferred (strong) | `RBR_E34_Wall.cpp:354`, `CarD.cpp:650` |

The point table at `mesh + 8*index` is read at offsets `0x00..0x1c` (eight floats). With the
8-byte stride, neighbouring records overlap. The asm does the same (`8*eax + 0x1c`, asm
`0x4e4604-0x4e46f7`), so this is how the original reads, not a C++ error. Open 2 asks what the
table really is.

### 2c. Grid and event layouts (from the headers; not asm-checked unless marked)

| object | size | layout (offset: meaning) | confidence | evidence |
| --- | --- | --- | --- | --- |
| `CColGrid` (E33 owns) | 0x6b0 per element | `+0x40` edges, `+0x400` bitmap, `+0x410` (pointer to the material table, 7 bytes per entry, `byte +3` is the material id), `+0x484` pairs, `+0x688`/`+0x689` flags | inferred (header), `+0x410` and `+3` asm-checked via `M9B80` | `RBR_E37_CollEvent.h:18-27`; `RBR_E37_Solver.cpp:180-200`; asm `0x4f9b9c-0x4f9bcd` |
| `CMaterialManager` | - | `Tag520940(material id)`, called at `this+0x18` of the world's `+0x24` object | known (asm call) | `0x4f9bb9`, `0x4f9bd6` |
| `CCollisionEvent` | 0xf0 | `vptr +0x00`, `words +0x0c/+0x10/+0x14`, planes `+0x60/+0x70/+0xc0/+0xd0` (E09 vec4), id triples `+0x80/+0xac`, penetration `+0x94`, bodies `+0x98/+0xa0`, `flag +0xe0`, `flags +0xe4`, secondary list `+0xe8` | inferred (header), not asm-checked | `RBR_E37_CollEvent.h:26-35` |
| `CCollisionList` | - | free list head `0x008ef7f4`, tail `0x008ef7f8`, count `0x008ef7fc`; live count `0x008ef7f0`; pool `0x008ef810-0x008f6f20` | inferred (header) | `RBR_E37_CollEvent.h:14-18` |
| `CCollisionSolver` | 0xc424 | `CDef` 0x4c bytes; solver 3 vtable slots | inferred (header) | `RBR_E38_Collision.h:1-12` |

## 3. Functions

| address | current name | proposed name | what it computes | confidence | evidence |
| --- | --- | --- | --- | --- | --- |
| 0x4e4960 | CBrakeWallManager::RaycastEntry | wall_probe | copies `(x, y)` of the point, then `E34WallVf::Raycast` if a wall is bound; returns the hit | known (asm checked, matches the C++) | asm `0x4e4960-0x4e4996`; `RBR_E34_Wall.cpp:367-378` |
| 0x4e4570 | E34WallVf::Raycast | wall_segments | for each candidate cell: for each segment, the point is on the inner side of the two edges (`0 < py*(-ex1) + px*ey1`, `0 < py*nx + px*ey2`, `0 < qy*(...) + qx*(...)`, and one more) gives a hit; normal `(ey2, nx)` normalised, `perp = n·p`, `along = -n·e1`, penetration `1 - perp/along` | inferred (C++ read; asm checked to the index mask and stride); reccmp 13% is codegen | `RBR_E34_Wall.cpp:276-357`; asm `0x4e4570-0x4e46f7` |
| 0x4e42f0 | E34WallVf::VfCC | wall_vfcc | not read | unverified | `gameplay-functions.csv` says verified |
| 0x4e41a0 | E34Link10::UnlinkA | unlink | list removal (not collision physics) | not read | - |
| F2 (in `Vf6cx`) | CCar::Impulse640 call | car_wall_impulse | see below | inferred (C++ read; asm call site checked) | `RBR_E34_CarD.cpp:644-646`; asm `0x4ea372-0x4ea39a`; `RBR_E46_Math.cpp:560-583` |
| 0x529640 | E46Impulse::Impulse640 | velocity_clamp | `n' = perp - g_tol`; `dot = d·v` (`d` = the hit normal, `v = car+0x1c0`); if `dot < 0`: `mag = -dot`; if `n' > 0.01`: `dot2 = dot²/(2n')`, and if `mag/dot2 > mu`: `mag = dot2*c*mu`; then `v += d*mag` (x, y, z, w) | inferred (C++ read, not asm) | `RBR_E46_Math.cpp:560-583` |
| 0x4e1230 | CBody::VfCC | aabb_pretest | `tA = FetchX(a)`, `tB = FetchX(b)`: returns 1 when `tA - m_vecB < 0` on all three axes and `m_vecA - tB < 0` on all three | inferred (C++ read: an axis-aligned box overlap on the two fetched corners) | `RBR_E33_Collide.cpp:9-22` |
| 0x4e2f50 | CBody::VfE8 | grid_hit | when `VfF4` hits, saves the grid's flags `+0x688/+0x689` into `d+0x2c/+0x30` and clears them, stores `this` at `d+0x38` | inferred (C++ read) | `RBR_E33_Collide.cpp:27-44` |
| 0x4f9b80 | E37GridOps::M9B80 | edge_tag | `r = M9780(a + 24*b)`; `Tag520940(material[7*r + 3])`, twice | known (asm checked: the second call is the same argument, as in the C++) | asm `0x4f9b80-0x4f9bdf`; `RBR_E37_Solver.cpp:180-192` |
| 0x4f9cd0 | E37GridOps::Build9CD0 | grid_build | builds the grid from a descriptor (reached, labelled `bad` in the CSV, 7.5% reccmp) | unverified | `RBR_E37_Solver.cpp:292` |
| 0x4f65b0 | E37GridOps::AddEdge | add_edge | grid edge insertion (reached, labelled `reviewed`) | not read | - |
| 0x4f8280 | E37GridOps::EdgeEdge | edge_edge | edge-edge contact test (reached, labelled `verified`) | not read | - |
| 0x4f8a20 | E37GridOps::M40 | (edge proxy) | target of `E91ExtCol::M40` (`RBR_E37_CollEvent.h`) | not read | - |
| 0x4f4ec0 | E37ListOps::Drain | event_drain | drains the event list (labelled `reviewed`) | not read | - |
| 0x4f4dc0 | E37EvOps::Emit4DC0 | event_emit | emits one event (labelled `verified`) | not read | - |
| 0x4f3d40 | E36VecU::CrossNorm | cross_norm | normalised cross product (labelled `verified`) | not read | - |
| 0x4f3ec0 | E36Triple::MinPick | min_pick | smallest of three (labelled `verified`) | not read | - |
| 0x4fa880 | E37SigFunc880::Call880 | sig_call | signal call (labelled `verified`) | not read | - |
| 0x4fac50, 0x4fb220, 0x4fbd30, 0x4fbfc0, 0x4fc1a0 (and the E38Vec kernels 0x4fcc70-0x4fd030) | E38Ops / E38Vec | solver kernels | solver steps (labelled `reviewed` or `verified`) | not read | - |
| 0x51afa0, 0x51b460, 0x51b500 | E37E43::M_* | matrix helpers | E43 matrix helpers used by E37 | not read | - |

Reached, in this subsystem, and not read in this pass: `E38Ops::M_D760X`, `M_DE40X`,
`M_Sweep3A0`, `E37GridOps::M74A0`, `M9780`, `E37VecSet::Store5570`, `E37GridOps::Walk5FF0`,
`E37GridOps::Copy6770`, `E37EvOps::Query4390`, `E37EvOps::PlaneDiff` (not reached). See
`notes/progress/called.csv`.

### 3a. The brake-wall step as one sequence

1. `RaycastEntry` copies the car's `(x, y)` (not z) into a 2-float temporary and calls `Raycast`
   (asm: two floats, `0x4e4967-0x4e497c`).
2. `Raycast` takes the wall's candidate cells (`QueryPoint`). For each segment the point must be on
   the inner side of its edges. If the segment is hit: the normal `n = (ey2, nx)/|.|`, the signed
   distance `perp = n·p`, the travel to the edge `along = -n·e1`, and `penetration = 1 - perp/along`
   (or 0). The index word bit 14 decides whether the hit is a "hit with signal" (returns 0 after
   emitting) or a plain hit (returns 1).
3. `Vf6cx` adds the velocity impulse (`Impulse640`) with `n = perp` and `mu = dt` (C1).
4. `Vf6cx` sets `0x166c = penetration` and `0x9ec`, and emits the penetration signal.

### 3b. Initial Rust vector ports

The first Rust batch under `rust/rbr-physics/src/collision/` covers the E33 vector leaves, the CBody
grid queries and bounds copy, and the CColGrid status/sampling helpers. `tests/collision_vectors.rs`
replays the original-call records. The drive snapshots exercised 154 `MinMaxA`, 158 each of `Min`
and `Max`, 43 `Scaled`, 56 `AllBelowZero`, 37 `BoxHit`, and hundreds of the CBody queries. The CColGrid
helpers not reached in those drive snapshots were captured as direct calls from the step-40 grid; its
eight `SampleA` inputs also match byte-for-byte.

Two ABI details matter when reading these vectors. `MinMaxA(this, p1, p2, p3)` writes
`min(this[0], p1[0])` to `p2`, then writes `max(this[1], p1[1])` to `p3`; the second source is the
upper vector at `p1 + 0x10`. Its final EAX is `p3`. `Scaled` reads its output pointer from the first
stack word and its scalar from the second, despite the reversed C++ declaration. `BoxHit` writes only
`AL` on its return paths, so the high 24 bits of EAX are leftover pointer values; the vector test
checks the defined low byte and every pointed-to input window.

The next link slice adds `CBodyLink::Head`, both link constructors, and `CBodyLink::Unlink`. Head and
the constructors match 20 direct calls apiece. Unlink matches one drive call plus 40 direct calls
covering an unattached link, sole member, head, tail, and interior removal. The link is 0x10 bytes:
next, previous, owner, item; `Unlink` clears the first three words and leaves item unchanged.

The E33 dictionary overrides now expose parent `Unk0C`/`Unk10` calls through `DictHost`, so the Rust
port can preserve the original virtual-call order. The direct vectors cover 30 `Unk18` calls, 88
`Unk0C` calls, and 38 `Unk10` calls. `Unk10` makes a second count call when it selects the local
table; the tested `Unk18` E33 vtable inherits the base `Unk00` that returns zero, so it exits after
the first slot-0 call.

## 4. Open questions

1. **How an E38 contact event reaches the body.** Settled in `contacts.md`: the solver's `M_C1A0`
   computes the impulse, `Emit4DC0` applies it to both bodies through slot 0x128 (`Vf74x` on CSolid).
   Still open: the car's own slot 0x128 target, and the callers of `CCollisionSystem` (E38 `System`),
   which `contacts.md` does not cover. See `contacts.md` open 6 and 7.
2. **What the wall table is.** The point table at `mesh + 8*index` is read at offsets 0 to 0x1c
   (overlapping). Either the table holds pairs of points with a 16-byte stride and the index is a pair
   index, or the layout is a 4-point record with a stride that the C++ mis-states. The asm does
   the same, so the code is faithful; the layout is not known. Settled by the wall loader (E34
   `CBrakeWall` load, not read) or a dump of one wall's table.
3. **`g_tol` and `c` in `Impulse640`.** `n' = perp - g_1659b70[0x24]+0x1610c` is a tolerance and
   `c = +0x1611c` a coefficient. Their roles are fixed by asm (`contacts.md` 2d) but their key names are
   not (`contacts.md` open 2).
4. **Body-body contacts, the E36 collect pass and the E37 grid build.** Not traced: `Build9CD0` is
   labelled `bad` and `M9B80` is correct in the asm. Needs the owners' pass.
5. **`CBody::VfCC`** is a C++ read only. Its asm was not checked.

## 5. Suspect code

Asm is the reference. Addresses are VAs. `E33Vec4::BoxHit` is not repeated here (see
`lead/briefs/cpp-asm-diffs.md` item 2).

**K1. WITHDRAWN (`contacts.md` section 5, S6).** This item said the 4th argument of `Impulse640` is a
friction coefficient and that `dt` as friction is a bug. That is wrong: `mag/dot2` has units of seconds
(`(m/s)/(m/s^2)`), so the 4th argument is a time, and the caller passes `dt` consistently. The asm
(`0x529640`-`0x52970f`) matches the C++ with that reading. Only the parameter name (`mu`) is misleading;
rename it to `dt`. No behaviour change.

**K2. `E46Impulse::Impulse640`'s third argument (`unused`) is the penetration bit pattern and is not
read** (`RBR_E34_CarD.cpp:645` passes `*(void**)(bw + 0x10)`, a `float` as `void*`). The C++ ignores it
(`RBR_E46_Math.cpp:563-583`). Asm-checked (`0x529640`-`0x52970f`): the original never reads the third
argument, so the parameter is dead and the type is misleading. Confirmed (contacts.md S7); no bug, route
to the E46 owner only for the rename.

**K3. `E34WallVf::Raycast` reads eight floats per index with an 8-byte stride (`RBR_E34_Wall.cpp:306-313`).**
Faithful to the asm (`0x4e4604-0x4e46f7`). Listed so nobody "fixes" it to a 32-byte stride without
the table's real layout (open 2). Do not change.

**K4. `E37GridOps::M9B80` (`RBR_E37_Solver.cpp:185-190`).** The two `Tag520940` calls with the same
argument look like a bug, and the status CSV calls the function `bad`. The asm (`0x4f9bb9`,
`0x4f9bd6`) makes the same two calls. So the C++ is faithful. Keep. Change the CSV label to match the
asm, once the owner has checked it.

**K5. Status labels.** `E37GridOps::M9B80` (labelled `bad`) and `E37GridOps::Build9CD0` (labelled
`bad`, 7.5%) need the owners to re-check. `E37GridOps::M9B80` is asm-faithful (K4).
