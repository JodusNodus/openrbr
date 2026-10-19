# Interactive scenery: trees, bushes, poles, cones, spectators, fences

How the 1.02 game treats stage scenery when the car touches it: which objects are fixed collision, which
become physics bodies, which carry soft or bendable flags, which are cloth, and what is drawn from physics
state. Addresses are `RichardBurnsRally_SSE.exe` 1.02 unless marked HH (HedgeHog3D) or LM (LocoMotion).
Labels as in [stage-characters.md](stage-characters.md): **loader** = read from the decompile or our
reconstruction of the named function; **file** = checked on retail data (counts from a throwaway survey of
the 88 retail `Maps/*.trk` files); **inferred** = plausible, not proven. File layouts: `formats/stage-files.md`
(`.trk`, `.fnc`), `formats/col.md` (surface ids, tags), `formats/physics-lsp.md` (Solid blocks).

## Summary

Scenery collision comes from the stage `.trk` collision-mesh records (kind `0x16`), not from `.col`.
Each mesh has a face tag per triangle, a flag byte (0 static, nonzero dynamic), a shape code (none, box,
sphere) and a material byte. The loader (`0x46e300`) turns each mesh into a `CSigCollisionMesh` and each
placement into a `CSigStuff`. Static meshes are emitted at load, so their triangles are hard collision in the
physics world. Dynamic meshes (only 4,708 spectator meshes in retail) are not emitted at load.

Placements of kinds 1 to 10 (cones, signs, banners, logs, poles, fence bars) and kind 15 (sphere bushes) carry
a HedgeHog3D interactive handle. The E46 physics callback `E46_Notify67d0` (`0x5267d0`) takes a `CSigStuff`
it receives and creates a physics entity through the entity factory (`E39_EntfCreate`, `0x502070`). The entity
kind is the placement kind, or `0x19` / `0x15` when the mesh name contains `CONE` / `BANNER_STAND`. This runs at
load for every placement, and every entity is a `CSolid`: the `GENERIC` ones (registry entry 1: trees, rocks, snowwall,
spectators) have inverse mass 0 and never wake, the props are sleeping bodies that a contact wakes (section 7). The handle is `0x9c40` plus the HH clock index, and
`CInteractiveObjectManager::OnSolidGeneralState` (HH `0x10070a10`) writes a physics pose into that clock, so a
physics-driven object is drawn from physics state. The emitter is `CSolid::Vf28` (`0x52c620`), once per frame
for each awake body (section 7).

Bendable trees (surface `Bendable_Tree`, id 121) set a contact-event flag (`0x2000` / `0x4000`) that changes
which solver branch handles the contact (`E37Flag4AC0` `0x4f4ac0`, `E38` solver). The exe has no vertex
bend, sway or swap-mesh code for scenery. Hinged bodies are car parts only. Fences (`.fnc`) are a cloth
simulation (`NStickSystem::CFenceHandler`) that reads collision triangles and writes nothing into the car.
The exporter's claim that spectator and soft shapes behave in a special way has no consumer found in the exe.

## 1. Data sources

### 1a. `.trk` collision-mesh record (kind `0x16`; **loader** `0x46e300`, **file** 88 stages)

Per mesh, in order (**loader**, `src/rbr/RBR_E10_Loaders.cpp`):

| field | read as | meaning used here |
| --- | --- | --- |
| name | NUL string | `"MAPS/<RALLY>/SHAPES/.../NAME_COL.KSH"` in retail; `u: <n>` for spectators |
| kind | u32 | object kind, table 1c |
| material byte | u8 | surface id in the `.col` id space (table 1d); stored at mesh `+0x30`, no reader traced in the loader |
| flag byte | u8 | 0 = static (one shared id for all placements), nonzero = dynamic (each placement points at its own mesh, `CSigStuff +0x44 |= 0x30`) |
| shape | u32 | 0 none, 1 box (two Vec3: min, max), 2 sphere (Vec3 centre, f32 radius); other values read no bytes |
| vertex count | u32 (loader keeps u16) | 16-byte Vec4 records (xyz, pad) |
| faces | count u32 (u16 kept) | 1 byte tag, then three u32 indices stored as low bytes (7-byte record: `[0..2]` = indices, `[3]` = tag) |
| placements | count u32 | u32 key, Vec3 position, Vec3 scale, quaternion x y z w |

Face tags seen by the loader: `0x00` (undefined) and `0xff` (spectator, `file`: all 19,372 `0xff` faces
are in dynamic meshes). The mesh material byte is not copied into the E37 triangle records; only the face tag
is (`tris[k*7+3] = b0`, `0x46e300`).

### 1b. Placement key and HH handle

For kinds other than 0 and 5 the loader calls the HH world's interactive lookup
(`IHH_World` vtable `+0x78`, HH `CHH_World::Unk78` `0x10066ab0`), passing the placement key. HH searches its
borrowed clock list for the entry whose first dword equals the key (`CInteractiveObjectManager::Unk709c0`,
HH `0x100709c0`) and returns `index + 0x9c40`. That value goes to `CSigStuff +0x40`. If the lookup fails the
loader logs `Error getting interactive object handle` (**loader**, level 3). Kinds 0 and 5 skip the lookup, so
their handle stays 0 (**loader**).

### 1c. Kind to `CSigStuff` type (**loader**, switch in `0x46e300`; **file** for kinds seen)

| kind | retail meaning (exporter) | type `+0x38` | retail meshes / placements |
| --- | --- | --- | --- |
| 0, 5 | static / spectator (dynamic kind 0) | 1 | kind 0: 4,708 dynamic (spectators) + 235 static (vehicles, structures) |
| 1 | traffic cone | 0x19 (25) | 121 / 2,429 |
| 2 | concrete block | 0x18 (24) | 37 / 720 |
| 3 | street sign | 0x0d (13) | 94 / 819 |
| 6 | banner fence | 0x15 (21) | 65 / 1,849 |
| 7 | log | 0x1c (28) | 21 / 3,444 |
| 8 | log pole | 0x1e (30) | none in retail |
| 9 | wooden fence bar | 0x1f (31) | 15 / 2,920 |
| 10 | wooden fence pole | 0x20 (32) | 15 / 1,522 |
| other (4, 11-15) | default | 1 | kind 15: 12 / 24,341 (`HO_BUSH01`, sphere, Hokkaido) |

Note the overlap: `0x19`, `0x1e`, `0x20` are also tree and bush surface ids (table 1d). The number is a
factory index in one place and a surface id in the other. Only the factory use is traced here.

### 1d. Scenery surface ids (`.col` id space; **file** `formats/physics-lsp-fields.md`, ids from `E44_TableBuild` `0x520190`)

| id | name | id | name |
| --- | --- | --- | --- |
| 24-26 | Spret_Bush_Small / Medium / Large | 62 | Snowwall |
| 27-29 | Lov_Bush_Small / Medium / Large | 63 | Metal_Pole |
| 30-32 | Tree_Small / Medium / Large | 88 | Snowwall_Bottom |
| 56-58 | Rock_Small / Medium / Large | 120 | Tree_Stump |
| 59-61 | Trunk_Small / Medium / Large | 121 | Bendable_Tree |
| 165-168 | Very_Sparse / Sparse / Dense / Very_Dense_Bush | 255 | Spectator |

Script characters use material `0xc8` (200); no name in the physics table was found for it in this pass.

### 1e. Fences (`.fnc`; **file** `formats/stage-files.md`)

Groups hold a tile type (tape, net, barbed wire; 0-14), a pole type and texture indices, and posts with
positions and colours. The loader is `FUN_006038c0` (not in the physics path). See section 3.6.

### 1f. `.lbs` interactive faces (1,6)

HH reads a name, a flag byte and draw records per object (`formats/lbs.md`). The parsed layout has no key
field, so the link from a `.trk` placement key to a drawn object is not established (see Open).

## 2. Load path (per stage; **loader** unless marked)

1. **Gate.** `CPC_TrackLoaderDaemon::LoadCollisionMeshes` (`0x46e300`) returns at once unless
   `[Settings] SendShapeCollisionMeshes` is true (`FUN_00411670`, default argument 1). No file in
   `original/` contains that key (search of all `.ini` and `.rbz` text, **file**), so the default applies.
   The lookup itself is not traced.
2. **Count.** Reads `n` (u32). `n > 0x2000` logs `Too many collision meshes`. Each mesh gets a `baseId` from
   `E10_AllocCollisionId`; the loop stops at ids above `0x1fff`.
3. **Mesh object** (`CSigCollisionMesh`, 0x34 bytes): `+0x00` id (static only), `+0x04` flags (`0x10000`
   sphere, `0x20000` box, set from the shape code), `+0x08` name (`strdup`), `+0x0c` vertex count, `+0x0e` face
   count, `+0x10` vertices, `+0x14` faces (7 bytes each), `+0x2c` shape pointer, `+0x30` material byte.
4. **Static meshes** (flag 0): `CSignalManager::CMultiSignal1<CSigCollisionMesh>::Emit` at
   `g_signalManager+0x4300` right after the mesh is read. The E37 collision world takes it from there.
5. **Placement object** (`CSigStuff`, 0x50 bytes): `+0x00` position, `+0x10` quaternion, `+0x20` scale,
   `+0x34` shared mesh id (static) or mesh pointer (dynamic), `+0x38` type (table 1c), `+0x40` HH handle,
   `+0x44` flags (dynamic: `|= 0x30`).
6. **Emission.** Static placements are emitted at `g_signalManager+0x4730` during the load loop. Dynamic
   placements are kept back. After the loop the loader emits one more record, the first in its local list,
   with position and quaternion xyz zeroed and w = 1.0 (`0x46e9c0` block). Its meaning is inferred as an
   end-of-list marker. The decompile's control flow around the static delete is ambiguous (Ghidra marks
   `operator delete` as non-returning), so the record lifetimes are open.
7. **Mesh list.** Meshes are appended to the track state's list at `+0x360` (`E10Vec4`); `CPC_TrackLoaderDaemon::Vf20`
   (`0x46eab0`) frees that list at session cleanup.

## 3. Runtime behaviour

### 3.1 Physics callback (E46)

`CPhysics` (`0x527c60`) registers four `CSigStuff&` connections (`src/rbr/RBR_E46_Physics.cpp:95-145`, header
`RBR_E46_Physics.h:33`): at `g_signalManager+0x4938` (callback `0x5267d0`), `+0x4b50` (`0x5269e0`), `+0x4f80`
(`0x5272e0`) and `+0x5198` (`0x527330`). The `+0x4938` head is the overflow list of the `CSigStuff&`
multi-signal at `+0x4730`, by layout (32 slots of 0x10 bytes after an 8-byte prefix, so the overflow
`CSignal` sits at `+0x208`; `CMultiSignal1::Emit` `0x4402b0` calls overflow listeners before the slot list).
The other three are not traced to a signal instance. Dispatch `0x5269e0` walks a table until tag `-1`
(`E46_Dispatch69e0`).

`E46_Notify67d0` (`0x5267d0`, **loader** of the reconstruction `RBR_E46_Conn.cpp:531-600`):

1. `E45_MD60()`, then `E37SigEmit9B30(other)` (`0x4f9b30`). That emits the mesh at `+0x4300` only when
   `other+0x44 & 0x30` is `0x10` or `0x30`. `0x30` (dynamic) first allocates a new collision id and sets the
   mesh flag `+4 |= 4`. Static placements carry 0 there, so nothing is emitted.
2. `kind = other+0x38`. If `other+0x34 != 0` and `E36SelectTable(other)` gives a table whose name at `+0x684`
   contains `CONE` the kind becomes `0x19`; `BANNER_STAND` gives `0x15`. This resolves the exporter's
   "rewrites the kind" note: the rewrite is done here, at notify time, by name (**loader**).
3. `handler = E39_EntfCreate(0x7c8040, kind, other+0x40, current)`. `other+0x3c = handler`. The factory is a
   name dictionary; `kind` is an index into it (`E39_EntfCreate`, `0x502070`; `E39_EntfAppend` `0x5020d0`).
   Which entity each index creates is not traced (registration calls not found; Open).
4. If the handler exists: `handler.S44(other)`; then `current.S150(0, handler)` when a top handler is on the
   stack, else `RegWall(handler)`; then the slot `+0x90` if `other+0x44 & 1` else `+0x94`.
5. `other+0x44 & 2` pushes the handler on the scratch stack (`+0xe848`, top at `+0xe888`); `& 4` pops.

Settled in section 7: `Notify` runs at load for every placement (static ones during the loop, dynamic ones after
it), and each entity is a `CSolid` filed asleep (list `+0x460`).

### 3.2 Physics bodies for the placed objects (LSP, **file** `formats/physics-lsp.md:247-255`)

The LSP has `PhysicsEngine/Solid/<NAME>` blocks for six scenery props, all with `Sleepable` and `SleepDelay`:

| LSP block | InverseMass (value) | mass | exporter mass (object kind) | match |
| --- | --- | --- | --- | --- |
| BANNER_FENCE | 0.01 | 100 kg | 100 (6) | yes |
| LOG_POLE | 0.001 | 1000 kg | 1000 (8) | yes |
| STREET_SIGN | 0.2 | 5 kg | 10 (3) | no |
| TRAFFIC_CONE | 0.3 | 3.3 kg | 5 (1) | no |
| WOODEN_FENCE_BAR | 0.01 | 100 kg | 333 (9) | no |
| WOODEN_FENCE_POLE | 0.001 | 1000 kg | 2000 (10) | no |

`VerySoft:int[1]` appears on TRAFFIC_CONE and STREET_SIGN. The E47 solid reads no `very_soft` field
(`notes/physics/solid.md` 2a, `m_u1e0`). The factory index is the registry index, and the definitions hold the
LSP's values (section 7), so the LSP masses are the physics masses. Solid bodies are the
`CSolid` base (`notes/physics/solid.md`); hinges are not used (`notes/physics/jointed-bodies.md` 1: car doors,
bonnet and exhaust only).

### 3.3 Visual binding (HH)

`CHH_World::Unk78` (HH `0x10066ab0`) forwards to `CInteractiveObjectManager::Unk709c0` (HH `0x100709c0`, file
`src/hedgehog3d/HH_H27_A.cpp:106`). `OnSolidGeneralState` (HH `0x10070a10`, `HH_H27_A.cpp:133`) ignores ids
below `0x9c40` and otherwise builds a matrix from the state's quaternion block and writes it to
`m_clkBegin3C[id - 0x9c40]->m_mtx04`. So a body that emits `CSigSolidGeneralState` with
`id = handle` moves its HH object. Emitters seen: E44 `SimFB40` (hinge, `0x51fb40`, `jointed-bodies.md`), E35
body B (`RBR_E35_BodyB.cpp:361`), E88 wheel body (`RBR_E88_Body.cpp:300`, wheels only), E45 mounted body
(`RBR_E45_MountedB.cpp:290`). None of them carries a scenery id. Scenery's emitter is `CSolid::Vf28` (section 7).

Spectators have handle 0 (kind 0 skips the lookup), so they cannot be bound to a clock this way.

### 3.4 Bendable, soft and box flags

- **Surface flags.** A material entry's `+0x158` bits: 0 VALID, 1 SOFT, 2 HARD, 3 BENDABLE (`formats/col.md`,
  the bit-name table at `g_7caf18`).
- **Bendable contact.** `E37Flag4AC0` (`0x4f4ac0`, `src/rbr/RBR_E37_Event.cpp:342`) tests bit 3 of the entry for
  the tag of side A (`o+0xac`) and side B (`o+0x80`) and sets event flags `0x2000` and `0x4000` (**loader**).
- **Solver use.** `E37Test4B20` (`0x4f4b20`) returns `(flags & 0x6000) != 0`. In `E38` `SolverC` the plane
  branch runs only when that test is false (`src/rbr/RBR_E38_SolverC.cpp:50`). `E37EvOps::Emit4DC0`
  (`0x4f4dc0`) sends flagged events to the other body's `Vf128x` instead. `E38` `M_BD30` (`0x4fbd30`) stores
  record flags `rec[0] = 1` for flagged events (**loader**). What the bendable response does physically is
  inferred from this branch only. No tree-bend animation or shader path was found.
- **Shape flags.** `E37` solver (`src/rbr/RBR_E37_Solver.cpp:340-348`): a mesh with `+4 & 0x30000` stores its
  shape pointer in the solver record (`+0x680`) and sets the element byte `+0x68a` bit 8 (box) or bit 4
  (sphere). Readers of `+0x68a` box and sphere bits were not found beyond the element accessor
  (`src/rbr/RBR_E37_Elem.cpp:18,49`). So the exporter's "soft collision: a force on the car" has no found
  consumer.
- **Mesh material byte** (`+0x30`): `E37GridOps::Build9CD0` (`0x4f9cd0`) copies it to the collision grid's
  `+0x68f`, and the collision event takes its surface tags from there (section 7).

### 3.5 Static scenery (trees, rocks, poles, snowwall, bushes)

These are static meshes: triangles in the E37 world, emitted at load (`+0x4300`), with per-face tags `0x00`.
They do not move: each placement is a `GENERIC` `CSolid` with inverse mass 0 (section 7). Spheres (bushes, some
trees, bendable sphere meshes) add the shape flag of 3.4. A contact's surface tag is the mesh material byte
(section 7).

### 3.6 Fences (`.fnc`; cloth, not a body)

`NStickSystem::CFenceHandler` (`src/rbr/RBR_E90_Handler.cpp`, `0x6015f0` range):

- `OnCarState` (`0x601fb0`) takes the car frame from the car-state signal and wind from the signal's wind vector.
- `Tic` (`0x602240`) runs a per-frame cloth step on spans near the car, culls by frustum and drops spans far
  from the car.
- `QueryGround` (`0x601a60`) collects collision triangles from the stage world (`E39_Query7F0` on `g_7c755c`)
  in a box around each span; `GroundSpan` (`0x6019d0`) drops a ray.

`CFenceRenderer::vf08` (`RenderShadows`, `0x605670`; visuals-2, asm and pseudo-C) draws the car's shadow decal, not a
fence shadow: when the car exists, `QueryGround(&(0, 2.3, -0.3), 2.0, 4.2)` (0x73b9bc, 0x72f668) collects the `.col`
triangles under the rectangle `x -1..1`, `y -1.9..2.3` of the car's body frame (y to the rear), within 10 m
(0x72b154) of its height; `0x603fe0` writes three vertices per collected triangle whose mean height is within the
window, with `Textures\car_dpshad2.tga` (loaded by `Init` 0x605100) mapped across the rectangle (u by `1/2.0`, v by
`1/4.2`), and one colour whose alpha falls with the car's height over the ground: a 50 m probe down (0x7304a0) gives
`clamp(1 - 2 (car z - ground z - 0.5), 0, 1)` times a tilt term (`asin` of a car-frame value, not traced). It draws with
a depth bias (render state 0xc3: 1, or `-0.05 / far`). The fences carry no part of it. (visuals-2 read
`0x6019d0` as four clip planes and left the tilt term untraced; both are below.)

Traced further (visuals-3, asm of 0x605670, 0x603fe0, 0x6019d0, 0x601a60, 0x601fb0):
- The rectangle's centre is `(0, 2.3, -0.3)` and `EmitShadows` gets `(w 2.0, l 4.2, side 0, alpha)` with `alpha` the
  sky block's word 21 (`[[0x7ea990]+0x54]+0x54`), `Car_Deep_Shadow_Alpha` (`HH_TrackLoaderLoad`'s `sky[21]`). The C++
  reconstruction's `(0, 0.25, -0.35)` and alpha 0 are wrong.
- The tilt term is `asin(m[8]) * alpha * 0.63662` (0x74f7e4): the car frame's `z` axis's `z`, so 1 upright, 0 on
  its side. `QueryGround` returns 0 (no shadow) when `m[8] < 0`.
- `0x6019d0` is no clip plane: it drops a segment from 0.5 m above to 10 m below a point (`CCollisionSystem::Vf00`)
  and widens a `[low, high]` window by the hit's height (`if z < low: low = z; else if z > high: high = z`). It runs at
  the car's origin `± 2 y` and `± 1 x`, from `low = high =` the rectangle centre's height; a triangle is drawn when
  the mean of its three heights (`* 0.3333`, 0x74f7dc) is within `[low - 0.5, high + 0.5]`.
- The triangles are `E39_Query7F0`'s: every triangle of the leaves meeting the box over the four corners, `z` within
  10 m of the car, at most 512 (0x8000 bytes of 0x40).
- Mapping: the frame `fwd = normalize(right x up)`, `right = normalize(up x v)`, `v` = the inverse matrix's first row
  (`OnCarState` writes the inverse at `+0x6e0`); a corner's `d = p - centre` turns to `fwd d.x + right d.y + up d.z`, so a
  level car gets its own `x` and `y`; `u = d.x / 2 + 0.5`, `v = (d.y - |side d.z|) / 4.2 + 1`.
- The colour is `0xffffff - (ftol(alpha * -255) << 24)`: white, alpha byte `trunc(255 alpha)`. FVF 0x142 (xyz, diffuse,
  one uv), a world identity, depth bias (state 0xc3) 1 or `[0x73996c] / camera +0x294`, a triangle list.
- `OnCarState`'s origin is the signal's position plus the rotated `CCarBody::Get34() + 0x130` vector; that vector is not
  identified. Rust (`rbr-render/src/car_shadow.rs`) uses the recorded body pose.
- Rust: `car_shadow.rs`, drawn by default; the contact shadow is the `contact` enhancement in its place.

No call in the handler writes a force or an event into the car body or the E37 world. The cloth therefore reads
the ground, not the car (**loader** for the calls; "no force path" is inferred from the calls seen).

### 3.7 Spectators (dynamic kind 0, material 0, face tag `0xff`)

4,708 dynamic meshes, one placement each, in 63 stages (**file**). Each has about 10.7 faces, 19,372 of them tagged
`0xff`. They get a collision mesh only through the dynamic path (3.1 step 1), and their handle is 0. Each becomes
a fixed `GENERIC` `CSolid` with its own collision id (section 7). The
`0xff` tag's consumer (exporter: red-screen penalty) was not located (Open).

### 3.8 Persistence and replays

Everything in section 2 is rebuilt at each stage load and freed by `Vf20` (`0x46eab0`). Nothing in this
path stores scenery state between stage loads. Whether a stage restart in one session re-runs the loader,
and whether replays record scenery bodies, was not traced (Open).

## 4. Signals

- In (to scenery code): `CSigStuff&` connections of `CPhysics` (`+0x4938`, `+0x4b50`, `+0x4f80`, `+0x5198`).
  Emitters of `CSigCollisionMesh` at `+0x4300`: `E37SigEmit9B30` (`0x4f9b30`), `E46_Cb72e0` (`0x5272e0`), `E46_Cb7330` (`0x527330`).
- Out: `CSigCollisionMesh` (`g_signalManager+0x4300`, loader and E37), `CSigStuff` (`+0x4730` loader; `+0x4d78`
  script characters, `E02AnimEmitStuff` `0x443e40`), `CSigSolidGeneralState` (`+0x29e0`, emitters in 3.3),
  `CSigMrRBodyState` (`CSolid::Vf27x`, `0x52cc30`, `notes/physics/solid.md`).

## 5. Config and tweak values

- `[Settings] SendShapeCollisionMeshes`: default 1 (section 2 step 1).
- Mesh count limit `0x2000` (`0x46e300`); mesh id limit `0x1fff`.
- LSP `Solid` keys listed in 3.2.
- `CONE` / `BANNER_STAND` name checks and the constants `0x19` and `0x15` (3.1).

## 6. Retail data survey (**file**, 88 stages, 81 with collision meshes, 6,295 meshes, 0 parse failures)

Survey: `rbr-formats` `trk` parser, throwaway program under `/tmp/interactive-scenery-scripts/` (not in the repo).
Columns: meshes / placements.

| group | material | shape | flag | meshes / placements |
| --- | --- | --- | --- | --- |
| Tree (small/med/large) | `0x1e`, `0x1f`, `0x20` | none | static | 394 / 234,774 |
| Tree, sphere | `0x20` | sphere | static | 16 / 18,521 |
| Bush, Spret (medium) | `0x19` | none | static | 20 / 7,186 |
| Bush, Lov | `0x1b` / `0x1d` | sphere / none, sphere | static | 18 / 2,525; 37 / 9,911 (none + sphere) |
| Bendable tree | `0x79` | none | static | 154 / 43,831 |
| Bendable tree, sphere | `0x79` | sphere | static | 25 / 9,347 |
| Rock | `0x38`, `0x39`, `0x3a` | none | static | 163 / 6,947 |
| Trunk | `0x3b`, `0x3c`, `0x3d` | none | static | 50 / 2,095 |
| Snowwall | `0x3e` | box | static | 6 / 15,856 |
| Metal pole | `0x3f` | none | static | 4 / 288 |
| Tree stump | `0x78` | none | static | 28 / 10,395 |
| Script character | `0xc8` | box | static | 57 / 5,275 |
| HO_BUSH01 (kind 15) | `0x1c` | sphere | static | 12 / 24,341 |
| Kinds 1, 2, 3, 6, 7, 9, 10 (cones, signs, fences, logs, poles) | `0x00` | none | static | 368 meshes / 13,703 placements (table 1c) |
| Spectators | `0x00` | none | dynamic | 4,708 / 4,708 |
| Other static, material 0 (vehicles, barns, dirt clumps) | `0x00` | none | static | 235 / 4,344 |

Other facts: 63 stages have dynamic meshes; no placement key repeats within a stage (nonzero keys, 0 stages
with duplicates). Dynamic meshes have 50,456 faces, 19,372 of them `0xff`. No box or sphere shape exists on
a dynamic mesh.

## 7. Settled by the scenery drives (2026-10-16, `lead/reports/scenery.md`)

- **When `E46_Notify67d0` runs, and what it makes** (Open 1, 2). At load. The original's snapshot at step 1000 of
  tanner-corolla (track-54_E, car still on the start line) holds the stage's 3,139 placement solids in the physics
  state's list `+0x460` (filed asleep), all `CSolid` (vtable `0x73fc54`): 3,104 `GENERIC`, 21 `TRAFFIC_CONE`, 11
  `BANNER_FENCE`, 3 `STREET_SIGN`. The car is the only entry in the awake list `+0x450`. `rbr-standalone --boot`
  builds the same objects from the files and runs the scenery drives of `lead/reports/scenery.md` equal.
- **Factory index** (Open 1). `E39_EntfCreate`'s index is the definition registry's (`0x7c8040`) index, in the order
  of `Physics/physics.lsp`'s `DefManager` blocks: 1 `GENERIC`, 13 `STREET_SIGN`, 21 `BANNER_FENCE`, 24 `PIG`, 25
  `TRAFFIC_CONE`, 28 `LOG`, 30 `LOG_POLE`, 31 `WOODEN_FENCE_BAR`, 32 `WOODEN_FENCE_POLE` (no retail placement
  names the other solid entries). So kind 2 (concrete block) is `PIG` (4,000 kg), kind 15 and every static mesh of
  kinds 0, 4, 5, 11-14 are `GENERIC`.
- **Fixed or moving** (Open 1, 12). The definitions in memory hold the LSP values: `GENERIC` has inverse mass 0
  and `Sleepable` 0 (its block sets only `iCollisionMesh 3`); the props have the masses and sleep settings of
  table 3.2. `E38_004FCA20` (`0x4fca20`) wakes a due event's body and partner through slot `0x90`
  (`CSolid::E43Vf90`) only when its inverse mass (slot `0x13c`) is not 0 and its slots `0x80` and `0x8c` both
  return 0. So trees, rocks, poles, snowwall, bushes and spectators are fixed walls that never wake; the
  props wake on their first contact, move as rigid bodies in list `+0x450`, and go back to sleep through
  `CSolid::Vf24x`'s trip (`0x52b740` → `E43Vf94`). Seen in the Rust run of the banner drive: a street sign, three
  banner fences and a cone woke; one went back to sleep.
- **What wakes them** (Open 1). No signal: the collision solver itself, on contact (above).
- **The drawing's pose** (Open 3). `CPhysics::Vf0c` (per rendered frame) walks the awake list `+0x450` and calls
  slot `0x28`; `CSolid::Vf28` (`0x52c620`) emits `CSigSolidGeneralState` (`+0x29e0`) with id = the solid's sort key
  (`+0x30`, the placement's HH handle), position `+0x140`, orientation `+0x100` and velocities. HH
  `OnSolidGeneralState` moves the clock. Sleeping and fixed solids emit nothing. The emit writes no physics state.
- **Handles** (Open 4). The `.trk` key is the first word of the `.lbs` `(1, 6)` record; the handle is `0x9c40` plus
  that record's global index (`standalone-s17.md`, matched on 35 captured keys).
- **Contact surface** (Open 5). `E37GridOps::Build9CD0` copies the mesh material byte (descriptor `+0x30`) into the
  collision grid (`+0x68f`); the collision event takes both sides' tags from the grids' `+0x68f`
  (`collision/event.rs`, the payload's `data[8..9]` and the event's `+0xac`/`+0xb0`). So a tree contact's surface is
  its mesh material byte (for example `0x79` Bendable_Tree), and that is what `E37Flag4AC0` tests.
- **Dynamic placements** (Open 2, 9). The loader keeps them and emits them after the mesh loop with values
  `(0, 0, 0, 1)`; `E46_Notify67d0`'s `E37SigEmit9B30` gives each its own collision id first (`standalone-s17.md`
  item 6; the boot runs on the spectator stages are equal).
- **A woken prop's lane w.** `E38Ops::M_C1A0` passes `CSolid::E43Vf128` an impulse whose lane w is the original's
  stale stack, so a woken prop's velocity lane w (`+0x1cc`, `+0x19c`) is garbage that reaches `-inf` and then
  `inf + -inf`. It never reaches lanes x-z. `E43Vf6C`'s port adds it with the SSE NaN rules (`math::sse`), so the
  indefinite NaN is the original's `0xffc00000` (the sign drive's `rust_step` from step 7905).
- **Renderer** (not built). The renderer parses the `(1, 6)` interactive records but draws none. It could draw each
  at its placement and, while its solid is awake, at the solid's `+0x140`/`+0x100` (the `CSigSolidGeneralState`
  payload), keyed by the handle.

## Open

1. *(Settled, section 7.)* **Physics type of kind 1-15 entities.** Which factory index is which entity type (dictionary registrations
   not traced), so whether cones, signs and fences are fixed walls (`RegWall`) or moving bodies. Settled by
   tracing `E39_EntfAppend` callers and the entity types' `S44`/`S90`/`S94` slots.
2. *(Settled, section 7.)* **When `E46_Notify67d0` runs.** Static placements reach it through the `+0x4938` overflow list at the load
   emit, if the `CPhysics` connection exists at that point (ordering vs `CPhysics` creation not traced). Dynamic
   placements never reach the load emit; the path that reaches `E37SigEmit9B30` for them is not found.
3. *(Settled, section 7.)* **Scenery emitter of `CSigSolidGeneralState` with id >= 0x9c40.** Needed to know whether a physics object
   moves its HH drawing. Check the other `CSigStuff&` connections (`+0x4b50`, `+0x4f80`, `+0x5198`) and the
   `E45`/`E44` bodies that take the handle as owner.
4. *(The key-to-handle link is settled, section 7.)* **Clock list source.** Where HH fills `m_clkBegin3C` (borrowed clocks, first dword = key). Needed to link
   `.trk` keys to drawn objects. The `.lbs` (1,6) layout has no key field (section 1f), so the note in
   `formats/stage-files.md` that keys match `.lbs` interactive objects is unproven.
5. *(Settled, section 7.)* **Surface id for static tree contacts.** Mesh material byte (`+0x30`), `.col` cell tag, or face tag: which
   one the E37 event uses for a `.trk` mesh (Tag520940 input `o+0xac` / `o+0x80`).
6. **Bendable response.** What the flagged branch in `E38` `SolverC` and `Emit4DC0` does to the car (a force,
   an impulse, or nothing) and whether `rec[0] = 1` is a bendable kind.
7. **`+0x68a` box/sphere bits.** Readers beyond the accessor; if none, the exporter's soft-collision claim is
   wrong for shapes.
8. **Spectator `0xff` faces.** Consumer (red-screen penalty per exporter). Not located.
9. **Dynamic record lifetime.** The decompile's control flow around the static `operator delete` in the loader
   is ambiguous; the local `E10Vec4` of dynamic records is not shown to persist.
10. **Stage restart and replay.** Whether scenery bodies persist or reset within a session and whether E91
    replay records them.
11. **Fence forces.** Whether any fence cloth result reaches car physics (none found; `CFenceHandler` writes
    only its own spans).
12. *(Settled, section 7: the LSP's.)* **Mass mismatch.** LSP masses vs exporter masses for four of six props (3.2): which one the game uses for a
    placed object.

## Relevance

Gameplay: the static tree, rock, pole, snowwall and bush triangles are hard collision in the physics world
(stage data, loader), and the physics entities made from placement kinds 1-10 and 15 are collision bodies
whose mass and sleep behaviour come from the `Solid` LSP blocks (contact and motion). Presentation: the
fence cloth (3.6), the HH drawing of physics-driven objects (3.3), and any tree sway (none found).

## Other systems seen

- `CSigStuff&` signal slots `+0x4938`, `+0x4b50`, `+0x4f80`, `+0x5198` (E46 Stuff connections): `RBR_E46_Physics.cpp:95-145`.
- Track session cleanup `CPC_TrackLoaderDaemon::Vf20` (`0x46eab0`), which frees the mesh list at `+0x360`.
- `CSigStuff` script-character emitter `E02AnimEmitStuff` (`0x443e40`), material `0xc8`, see
  [stage-characters.md](stage-characters.md).
- Entity factory `NTweak`/`E39` dictionary (`0x502070`, `0x5020d0`, `0x7c8040`): section 3.1.
- Collision-event flags `0x2000`/`0x4000` consumer `E38` `SolverC` (`0x4fc500` neighbourhood, `src/rbr/RBR_E38_SolverC.cpp:50`).
- Red-screen penalty (spectator tag consumer, not located).
