# Car model files (`Cars/<CAR>/<car>.sgc`)

One model per car, eight files, 1.9–2.4 MB each. The file is a banner followed by a flat list of
records. HedgeHog3D's car loader (`CHH_CarLoader::LoadCarFromBuf`, 1.02 `0x1001f2a0`) reads the
meshes, helpers, cameras, collisions and shadows from it, and skips the rest. The reader is
`rbr_formats::sgc`; the retail test is `all_retail_sgc_models_parse` (`tests/real_data.rs`).

Evidence labels: **loader** = read from the 1.02 decompile or disassembly (addresses below);
**file** = checked over all eight retail files with the reader or a script; **source** = our C++
reconstruction (`src/hedgehog3d`, ours, may be wrong); **inferred** = plausible, not proven.

Sources: `original/retail-1.0/install/Cars/*/*.sgc` (retail), `original/patch-1.02/files/Main_group/HedgeHog3D.dll`
(1.02 code; the retail `Root/HedgeHog3D.dll` is a different build and was not checked).

## 1. Summary

- Banner: 22 characters and a NUL (23 bytes). **Loader** compares 23 bytes at `0x1001f2a0`, against the literal
  at `0x101af738`.
- Records tile the rest of the file: a 12-byte header (`kind`, `subtype`, one word that is always 1), a u32
  body length, and the body. **File**: 5,199 records over the eight cars; the last record ends at EOF in every file.
- Node bodies start with a NUL-terminated name, then an id and a parent id (`-1` = root), a 56-byte transform and a
  3x4 world matrix. **File**: every non-root parent id names a node of the same car, and ids are unique per car
  (retail test, kinds 0, 2, 3, 16, 17 and 22).
- Mesh bodies: bounds, vertices (position + normal), optional UVs, an unused 4-byte stream, a `u16` triangle
  list, faces (name + draw range), and for damageable panels a morph block. **File**: every mesh body ends exactly
  at its length; faces partition the triangle list in order; all indices are in range; all normals are unit length.
- Materials and textures are not in this file's geometry records. Face names repeat the `FaceBlock<N>_Material`
  values of `<car>.ini`, and the shader and textures come from the INI files (section 8).

## 2. File layout

| Offset | Size | Field |
|---:|---:|---|
| 0 | 23 | Banner `\|-\|3<\|&3\|-\|()& 5<3\|\\\|3` and NUL. **Loader** (`0x1001f2a0`, compare of 23 bytes). |
| 23 | 4 | Record 0 header length. Always 12 (**file**). |
| 27 | 4 | `kind` (u32). **Loader**: switch on this word (section 3). |
| 31 | 4 | `subtype` (u32). **Loader**: passed as `local_50`; geometry and camera dispatch use it. |
| 35 | 4 | Third header word. Always 1 (**file**). Not read by the dispatcher (**loader**, inferred from the switch). |
| 39 | 4 | Body length `L`. |
| 43 | `L` | Body. The next record starts at 43 + `L`. |

The loader loops until the cursor reaches the end (`0x1001f340`), and each case sets the cursor to
body end before continuing (default case at `0x1001f644`). **Loader**. Records of any kind therefore
tile the file. **File**: checked for all 5,199 records.

## 3. Record kinds

Counts are records per car (`kind/subtype`). Disposition is from the 1.02 switch: byte map at
`0x1001f700`, jump table at `0x1001f6e4`, bound `cmp eax, 0x11` at `0x1001f389` (**loader**).

| Kind/subtype | Name in this note | Retail count (8 cars) | Loader |
|---|---|---:|---|
| 0/1 | static mesh (wheels, glass, lights, ...) | 718 | Read. Dispatch `0x1001f483` → `0x1001fde0` → GeomCar `0x1000acf0`. |
| 0/3 | damageable panel (body, doors, bonnet, ...) | 287 | Read, as 0/1 with the morph block. |
| 2/11 | camera | 42 | Read. `0x1000bed0` (subtype 11, `0x1001f52d`). |
| 3/13 | helper (`parent`, `Cam_*.Target`, `D_*`, ...) | 367 | Read. `0x1000c310` (subtype 13, `0x1001f5b4`). |
| 16/0 | collision (`c_` + part name) | 65 | Read. `0x1000a3f0` (`0x1001f3a7`). |
| 17/0 | shadow volume (`s_` + part name) | 152 | Read. `0x1000a7c0` (`0x1001f3ea`). |
| 22/0 | point (`Point_*`) | 247 | **Not read** from this file (kind 22 > 0x11, default case). Points come from `<car>.ini`. |
| 5/17 | material record (`external::carpaint`, texture name) | 65 | Not read (default). |
| 5/18 | material record (`Default Material`) | 218 | Not read. |
| 5/19 | material record | 2 | Not read. |
| 6/21 | `<node>::PRS` record: the node's rest matrix for LocoMotion | 1,777 | Not read by the HH loader. Read by LocoMotion as the rest TM (section 12). |
| 14/0 | `car=<name>` record | 8 | Not read. |
| 15/54 | effect/parameter record (`enabled`, `effect`) | 1,235 | Not read (default, kind 15 is in range and maps to the default case). |
| 18/0 | LocoMotion node table: count, then per node its type, name, id, parent id, matrix and flag (`parent`, `e_light_lf`, ...) | 8 | Not read by HH. LocoMotion's `LoadNodes` (0x10014c20) reads it (section 14, `sgs::node_table`). |
| 20/0 | 8 bytes: u32 0 and a float (3.333 in every car) | 8 | Not read. |

Per car (kind/subtype columns in the table order above):

| Car | file bytes | records | 0/1 | 0/3 | 2/11 | 3/13 | 16/0 | 17/0 | 22/0 | 5/* | 6/21 | 15/54 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| ACCENT | 2,098,326 | 637 | 85 | 36 | 5 | 45 | 8 | 19 | 31 | 35 | 217 | 153 |
| COROLLA | 2,029,757 | 636 | 85 | 37 | 5 | 45 | 8 | 19 | 30 | 36 | 211 | 157 |
| IMPREZA00 | 2,425,587 | 633 | 92 | 30 | 5 | 46 | 8 | 19 | 30 | 35 | 213 | 152 |
| IMPREZA03 | 2,107,972 | 633 | 87 | 32 | 5 | 45 | 8 | 19 | 25 | 37 | 211 | 161 |
| LANCER_EVO_VII | 2,214,331 | 666 | 90 | 41 | 5 | 45 | 9 | 19 | 35 | 35 | 232 | 152 |
| PEUGEOT_206 | 2,214,487 | 686 | 88 | 41 | 7 | 51 | 8 | 19 | 37 | 36 | 242 | 154 |
| XSARA | 1,899,408 | 646 | 90 | 35 | 5 | 45 | 8 | 19 | 34 | 35 | 220 | 152 |
| ZR | 2,156,351 | 662 | 101 | 35 | 5 | 45 | 8 | 19 | 25 | 36 | 231 | 154 |

Every car has one each of `14/0`, `18/0` and `20/0`. Kind 5 subtype 19 occurs only in IMPREZA03 and ZR.

Other subtypes (kind 0 subtype 2 or 4, kind 3 subtype 14) are not in retail files. The reader rejects
them with `UnknownSubtype`.

## 4. Node header

Used by kinds 0, 2, 3, 16, 17 and 22 (the camera, collision and shadow headers only differ after the
ids):

| Offset | Size | Field |
|---:|---:|---|
| 0 | n + 1 | Name, NUL-terminated (`CDiskReader::ReadString`, `0x1008a0a0`; loader). |
| n+1 | 4 | `id` (i32). First int of the node header. **Loader**: stored as `m_unkC0[0]`; `HH_H7FindNodeByIdBFS` matches on it (source). |
| n+5 | 4 | `parent` (i32). Second int. `-1` = root. **File**: every non-root value is an `id` of another node in the same car (1,598 of 1,598). Used as a parent link in our reader; the loader only stores it (`m_unkC0[1]`, source). |
| n+9 | 56 | Transform (section 4.1). **Loader**: `0x1000c7a0` (`HH_H7ReadTransform`). |
| n+65 | 48 | World matrix, 3 rows of 4 floats `[R | t]` (**file**: rows have `1, 0, 0` identity-like values with `t` in column 3, e.g. the wheel `(0.767, 0.187, -2.529)`). **Loader**: `0x1000c6c0` (`HH_H7ReadMatrix`) stores it transposed. |

### 4.1 Transform block (56 bytes, 14 floats)

**Loader** (`HH_H7ReadTransform`, `0x1000c7a0`): translation (words 0-2), angle (3), axis (4-6), scale
(7-9), second angle (10), second axis (11-13); each rotation becomes the quaternion `(axis * sin(angle/2),
cos(angle/2))`. (The older reading, still the field names of `rbr_formats::sgc::Transform`, was scale, axis,
angle, translation, second axis, second angle.) `NLB_ComposeTransformEH` (`0x1008d420`) composes the matrix
`inverse(R2) * S * R2 * R1 * T` (row vectors). Ported for the collision bodies in
`rbr_physics::load::car_grids` (`lead/reports/load-grids.md`).

## 5. Geometry records (kinds 0/1 and 0/3)

Dispatch: `0x1001fde0` reads the name, classifies it (section 9), then calls GeomCar `0x1000acf0`
with `flags = 4` for subtype 1 and `flags = 6` for subtype 3 (**loader**, `bVar6` mapping in the decompile).
GeomCar tests `flags & 2` to set `m_deformEnabled`, so subtype 3 is the deformable panel class.

Body after the name:

| Order | Size | Field | Evidence |
|---|---:|---|---|
| 1 | 8 | node ids (section 4) | loader, file |
| 2 | 56 | transform | loader, file |
| 3 | 48 | world matrix | loader, file |
| 4 | 24 | bounds: centre vec3, then half-extent vec3 (section 5.2) | loader (two 12-byte reads, decompile of `0x1000acf0`), file (1,005 of 1,005 meshes match the vertex box) |
| 5 | 4 | `vertexCount` (i32) | loader, file |
| 6 | 4 | `streamFlags` (u32) | loader, file |
| 7 | 24 × n | vertices if `streamFlags & 1`: position vec3, normal vec3 | loader, file (normals unit length, 268,461 of 268,461) |
| 8 | 8 × n | UVs if `streamFlags & 4`: two f32 | loader, file, capture (render-m10: the 15 car uv vertex-buffer uploads of the `apitrace-check` capture equal COROLLA's streams byte for byte, so the uvs reach the GPU unchanged) |
| 9 | 4 × n | unused stream if `streamFlags & 0x10`; always `0xffffffff` (white) in retail; the loader reads and drops it | loader, file |
| 10 | 4 | `triangleCount` (i32) | loader, file |
| 11 | 6 × t | triangle list: three `u16` indices into the vertices | loader, file (every index < vertexCount) |
| 12 | 4 | `faceCount` (i32) | loader, file |
| 13 | var × f | faces: name (NUL-terminated), then i32 `primCount`, `minVertex`, `numVertices`, `startIndex` | source (`HH_3DSMaxLoaderGeom.cpp` GeomCar; drawn by `HH_FaceBlock.cpp:60` as `DrawIndexedPrimitive(TRIANGLELIST, 0, minVertex, numVertices, startIndex, primCount)`), file (partition check) |
| 14 | var | morph block, subtype 3 only (section 5.1) | file (all 287 subtype 3 meshes have it; no subtype 1 mesh does) |
| 15 | 4 | `listCount` (u32) | loader, file |
| 16 | var × listCount | per entry: `pairs` (i32); if `pairs > 0`, `pairs` × (u32 vertex index, f32 weight) | loader (count, size 8), file (indices < vertexCount; weights in 0.25…1.0) |

Faces are contiguous: face *i* starts where face *i−1* ends, `startIndex` counts indices (three per
triangle), and each face's indices stay in `[minVertex, minVertex + numVertices)`. **File**: 1,005 of
1,005 meshes; the sum of `primCount` equals `triangleCount` in every mesh.

### 5.1 Morph block (subtype 3)

Order: `targetCount` (i32), `perTarget` (i32, equal to the vertex count in all 287 blocks), then per
target: one byte (always 1) and `perTarget` vec3 deltas. **Loader**: `HH_H7LoadMorph`, `0x1000bdd0`
(count, count2, per-target skipped byte, then vec3s, source). **File**: 287 blocks; 279 have one
target and 8 have none.

How the loader decides to read the block is not traced. The decompile of GeomCar tests `unaff_EDI >> 24`,
and the dispatcher's `EDI` at the call site is its own `ecx` argument (`mov edi, ecx` at `0x1001fe11`), so the
test does not look like a subtype check. The mapping subtype 3 → morph block is observed in the bytes, not
explained by the code.

### 5.2 Bounds and placement of the vertices

The two vec3 are the centre of the vertex box, then its half-extent on each axis. **File**: for all
1,005 meshes with vertices, the box of the vertex positions has that centre and half-extent to 2e-3
(`all_retail_sgc_bounds_are_centre_and_half_extent`). The loader reads them as two 12-byte blocks and
the 1.02 use is not traced. The source (`HH_3DSMaxLoaderGeom.cpp`) calls them min and max; that is
wrong for these files (section 13).

Vertices are stored relative to their mesh's `centre`, so the placement is not the world matrix alone.
Rendering (`rbr-render`) puts a mesh's vertices at `F * P * v` (render-m11), where:

- `F` is the node's chained world matrix (`sgc::chained_worlds`): its stored matrix (section 4), read as
  `[R | t]` on column vectors, composed with its parents'. The stored matrix is relative to the parent. The car
  root (the `parent` helper, `parent = -1`) is the origin and contributes nothing, so its children use their own
  matrices. **File**: the rear door's exterior and interior panels (`e_door_b`, and `i_door_b` which is its child)
  coincide under this rule in all eight cars, 0.00 m (`all_retail_cars_rear_doors_match_their_interior_twins`).
  Without the chain the interior panel is 1.2 m off, and the mirrors (children of `e_door_*`) sit at the wrong
  height. Points and wheels are children of the root: `Point_dirtspray_lf` (0.97, -0.12, -2.45) and the front
  wheel (0.77, 0.19, -2.53) agree. Checked on the wheels, which land on their `Point_e_wheelrep_*` nodes, and on
  the body and lights. The root's own matrix is dropped, so the car origin stays at the rear axle (the rear
  wheels sit at `z = 0`).
- `P` is the transform block (section 4.1) composed as the game does: `T * R1 * R2 * S * inverse(R2)` on column
  vectors (`coords::part_to_car`), `T` from words 0-2, `R1` from words 3-6, `S` from words 7-9 and `R2` from
  words 10-13. `CGeomObject::Update` sets `m_world = m_local * animObj` (row vectors), so `P` applies first. **File**
  (1,005 meshes): `T` is zero for 653 and equals minus the centre for 49 (the bumpers and wheels). Without it the front
  bumper sits 1.8 m too far forward (XSARA `e_bumper_f`). `S` is `(1, 1, 1)` in 993 meshes. The tarmac wheels carry
  `R1` = a half turn about `(0, -1, 0)`: without it the rim face (`CarRim`) faces inboard in COROLLA, LANCER_EVO_VII,
  PEUGEOT_206, ZR and IMPREZA03's right rear wheel, and back-face culling showed the hub through it. With it every
  rim faces out (`all_retail_cars_turn_their_rims_outwards`; COROLLA `lf`: centre (0.826, 0.160, -2.496), normal
  `+x`). In the side doors `R1 * R2` undoes the node's rotation, so the door lies in the body frame (COROLLA `e_door_r`:
  `F * P` is the translation (0, 1.028, -3.476)). This is the same compose as the collision grids in
  `rbr_physics::load::car_grids`, which match the game bit for bit.
- A mirrored `F` (negative determinant; XSARA's right-hand mudflaps `e_mudflap_rb`, `e_mubflap_rf`) keeps its
  triangles: the game draws the node through `m_world` with the shader's fixed cull mode (`D3DCULL_CCW`,
  `shaders.md`), so the mirror turns which side is culled. Evidence: RSF models mirror whole parts that read right
  only that way (Celica ST185 `i_body`, its dashboard; Lada VFTS `i_internal_analog`; Escort Mk II `i_door2_l`;
  Xsara WRC 2006 `i_mirrornew_*`); reversing them, as `rbr-render` did before, showed their back faces (the
  see-through dashboard, the flat blue dash).

## 6. Helpers and points (kinds 3/13 and 22/0)

Body: name, then the node header (section 4). Nothing else. **File**: 367 helpers and 247 points tile
exactly (119 and 127 bytes for the first examples). **Loader**: helpers `0x1000c310`, source
`HH_H7LoadHelper` (name, ids, transform, world). Points are not read from this file (section 3).

Points: `Point_*` names are sections of `<car>.ini` in all 247 cases (**file**). Light setup reads `Pos` of
the section named by a light's `Car Part` (`CHH_Car::InitLights`, `0x10016fd0`; source `HH_CarInit.cpp`).

## 7. Camera, collision and shadow records

Each body is name, node ids, then a layout whose end is checked against the body length.

- **Camera (2/11):** name; ids; transform (56); `near`, `far`, `fov` (3 f32); world (48); one int; one vec3
  (153 bytes for `Cam_external`). **Loader**: `0x1000bed0`. **File**: 42 of 42 tile.
- **Collision (16/0):** name; ids; transform; world; vertex count + vec3 list; `sampleCount` (u32) + samples
  (two f32 each); `faceCount` (i32) + faces of three vertex indices (then three sample indices, 24 bytes, if
  `sampleCount != 0`); then a flag byte. If the flag is 1, two
  named blocks follow, each a name, an int `m`, a flag byte and, when set, `m*m` bytes. **Loader**: `0x1000a3f0`
  (source reads the same fields). **File**: 65 of 65 tile. The name is `c_` + a car part (65 of 65 are sections of `<car>.ini`).
  The loader keeps the first map at mesh +0xfc/+0x100 (size, data) and the second at +0xd8/+0xdc. The car loader's
  emit (0x1001ecc0, `rbr_physics::load::car_grids::car_meshes`) sends `c_e_body` and the parts of HedgeHog3D's table
  (0x101d7eb0) as `CSigCollisionMesh`es. Only the body gets its sample indices (face bytes 4-6), its samples (a byte
  each, `255 * x` truncated) and the maps (the second at +0x20/+0x24, the first at +0x22/+0x28). A part's face bytes
  4-6 stay unwritten (`lead/reports/load-grids2.md`).
- **Shadow (17/0):** name; ids; transform; world; vec3 list; `faceCount` + 24-byte faces (three ints and a
  vec3); `elementCount` + 16-byte elements. **Loader**: `0x1000a7c0` (`HH_H7LoadShadow`, source). **File**: 152 of 152
  tile. The name is `s_` + a car part; 8 names are `s_e_*` sections directly, the rest map to `e_*` sections.

## 8. Mapping to `<car>.ini` and shaders

- **Mesh and point names** are `[section]` names of `<car>.ini` (`e_*`, `i_*`, `pc_*`, `k_*`, `Point_*`).
  **File**: all 1,005 mesh names and all 247 point names exist as sections in all eight cars. The loader
  does not look up the section by name for geometry; the shader keys below are read from the same section
  name by the face setup.
- **Faces to shader slots:** `HH_H7SetupFaceShader` (`0x1000d0b0`) formats `FaceBlock%d` with the face
  slot, then reads `FaceBlock<N>_Shader` (shader class, e.g. `CarTire`, `CarGlass`) and `FaceBlock<N>_Material`
  (material name, e.g. `external dynamic::tire`) from the car's section (**loader**; the caller that supplies the
  slot index was not traced). **File**: for each named face *i* of a mesh, the section has
  `FaceBlock<i>_Material` equal to the face name (1,412 of 1,412 named faces). Unnamed faces (57) belong to
  meshes with no `FaceBlock` keys.
- **Textures and material settings** are not in this file. They come from `<car>_shaders.ini` and the
  `Textures\` folder (see `notes/formats/cars.md`). The material records of kind 5 are not read by the loader.
- **Lights:** `<car>_settings.ini` names the `Car Part` point of each light (`notes/formats/cars.md`).

## 9. Loader behaviour that drops or classifies parts

- `0x1001e090` classifies a name (source `HH_H11_1001e090`: `pc_e_premove` → 10, `sparewheel` → 9,
  `mudflap` → 8, `_wheel_` → 3, `e_`/`pc_` → 1, `i_` → 0, `d_` → 4, `ghost_` → 2, `k_windscreen` → 5,
  `k_` → 6, else 7). The class is used by the dispatcher; the table is from the source, not checked against
  the asm.
- **Loader:** when the class is 0 (an `i_` name), `0x1001fde0` returns without building the part if the
  name contains `i_wing`, `i_bumper`, `i_window_lf` or `i_window_rf` (`strstr` in the decompile). These parts
  are in the file but are not built.
- The remaining conditions use the car builder flag at `+0xc8` (`this+200`). Not traced.
- **Decompile** (`FUN_1001fde0`, render-m10): only classes 0, 1 and 8 build a drawn `GeomCar` (`FUN_1000acf0`).
  Classes 3 (wheels) and 9 (spare wheels and straps) build it only when the name contains the builder's surface
  string (`+0xd8`), and class 2 (ghost) under its own flag. Class 5 (`k_windscreen`) goes to `FUN_100201b0`, and the
  result is freed after `FUN_10021c00`. Classes 4 (`d_`), 6 (`k_`) and 7 (the rest) have no case, so they are never
  built.
- **Decompile** (`FUN_1001e430`): each face's shader goes into a car list by class. Class 0 (`i_`) goes to `+0xd4`
  or `+0xe4` (opaque or transparent; `CarRim` and `CarTire` to `+0x104`). Classes 1, 2, 3, 8 and 9 go to `+0xb4` or
  `+0xc4` (rims and tyres to `+0xf4`). `CHH_Car::Render(mode)` (0x10015530) draws the `+0xd4/+0xe4` lists for mode 0
  and the `+0xb4/+0xc4` lists otherwise (plus the lights in mode 1). So an outside view draws no `i_` part. The
  reference capture's bonnet view (render-m10) drew only `i_` meshes: its 15 car uv uploads are all `i_*` streams.

## 10. Shared with the character loader

`CHH_3dsMaxCharacterLoader` (`0x100072a0`) reads the same banner literal (`0x101af738`). **Loader**: the
literal has one copy in `.rdata`, and its 32-bit operand appears in code at `0x100072ea` (character loader)
and `0x1001f2ed` (car loader). The H7 record readers (matrix, transform, node header) are shared by both. The stage
loaders (`.lbs`) were not checked for this container. Any `.lbs` note should compare the banner and record
framing before reusing this reader.

## 11. Reader

`rust/rbr-formats/src/sgc.rs`:

- `records(bytes)` checks the banner and returns `Record { offset, kind, subtype, body }`, with the body
  slices tiled exactly.
- `parse(bytes)` returns `Element::{Mesh, Node, Collision, Unparsed, Ignored}`. `Unparsed` covers cameras
  and shadows (name and ids read, the rest checked by length). `Ignored` covers kinds the loader skips.
- Every read is bounded by the body. Counts that are negative, or products that exceed the body, are errors.
  Unit tests cover truncation at every length and single-byte edits (no panics).

Retail test (`RBR_DATA`, `tests/real_data.rs`): record counts per kind; node id uniqueness and parent
resolution; face partition; index range; unit normals; morph shape; mesh and point names in `<car>.ini`;
collision and shadow names; `FaceBlock<i>_Material` equal to face names. The OBJ test
`xsara_meshes_export_to_obj` writes the XSARA static and damageable meshes, placed by their world matrices,
to `/tmp/rbr-sgc/xsara-meshes.obj` (125 meshes, 26,865 triangles). The placement is checked only by its
bounding box (car-sized, wheels mirrored at about ±0.8 m); not checked in the game.

## 12. Unknown

- Meaning of the `listCount` lists (section 5, step 16). The bounds are a centre and half-extent (section 5.2).
- The transform block (section 4.1) is decoded (render-m11, section 5.2). The field names of
  `rbr_formats::sgc::Transform` still follow the older reading. The layout is right, and the renderer uses `words()`.
- Rest pose source (found, asm not checked). The car's LocoMotion character is the `.sgc` itself: `E09_LoadCar`
  (`0x0046abb0`) calls `AddCharacter(file, file, "Car")`, `AddAnimation(file, "Mr. CarNN_Anim")` and `LoadGhost`;
  the app loader reaches `CLM_LocoMotion::Unk14` (`0x100275a0`), `CLM_CharacterLoader::Load` (`0x10015550`) and
  `LoadHierarchy` (`0x10015280`). The file's kind 6/21 records named `<node>::PRS` are the node's rest TM: after the
  NUL-terminated name come seven ints (`12, 7, 33, 1, 56, 1, 0`), then twelve floats, a row-major 3x4 `[R | t]`.
  **File**: that 3x4 equals the node's stored world matrix (section 4) in all 1,346 PRS records of the eight cars
  (max difference below 1e-3). In XSARA the door, mirror, wiper and steering-wheel nodes all have one. Car nodes are found by
  name in `CLM_CharacterCar::Unk1d630` (`0x1001d630`, `D_driver_use`, `i_steeringwheel`, `e_wiper_r`, ...), which reads
  their matrices (`LM_GetTranslation(&node->m_unkA0)`).
- Per update: `CLM_NodePRS::Update` (`0x10029620`) sets `m_unkA0 = EvaluateTM(controller, time, parent)`, and
  `CGeomObject::Update` (`0x100029d0`) sets `m_world = m_local * animObj` (source `HH_GeomObject.cpp:115`). So at rest
  `animObj` is the stored world matrix. Whether `EvaluateTM` composes with the parent exactly as
  `sgc::chained_worlds` does is not checked in the asm.
- `m_local` = `NLB_ComposeTransformEH(HH_H7ReadTransform)` (`0x1000c7a0`, `0x1008d420`), section 4.1. `k` at
  `DAT_101b41d8` is 0.5. Used for the vertices (section 5.2).
- The side doors and their interior panels (`e_door_l/r`, `i_door_l/r`) are hinged bodies (physics `HingedBody/CAR_DOOR_L|R`,
  `common.lsp`). At rest the panel keeps the car's orientation (the block's rotation undoes the node's, section 5.2) and sits at the
  hinge point, the exterior door's node translation (the LSP `vecHingePoint_RF` equals it: XSARA `(0.836, -2.175, 0.445)`;
  world `(0.836, 0.445, -2.175)`). The hinge axis is up (`vecHingeAxis_RF` z = +1 left, -1 right) and `HingeDefaultAngle` is
  0.01 rad in all eight cars, applied about the hinge point. Rule: `coords::hinged_panel_to_car`, `T(hinge) * Ry(-side * 0.01) *
  T(-hinge) * F * P`. Sign of the default angle: assumed from the LSP axis in the physics frame (it is
  about 1.4 cm at the door's rear edge).
- **Fit** (`side_doors_and_interior_panels_match_in_all_retail_cars`): exterior and interior boxes coincide within 0.05 m in all
  eight cars. (Before the block's rotation was applied COROLLA's right interior panel was 0.33 m off; it now fits.) The exterior hinge-line distance (min horizontal distance from the hinge point to the panel's
  vertices) is 0.013-0.071 m on every side. The door box lies between the front tyre (physics y -2.45 to -2.57) and the rear
  tyre (y 0), within y -2.12 to -0.67 in every car.
- Mirrors: no LSP body (the LSP has only the door, bonnet, exhaust hinges and the bumper and wing mounts). Their chains keep
  the door's rotation only in ACCENT and ZR right (the chained rotation is not identity); not checked against a twin. Left
  unchanged.
- Bumpers and wing (`MountedBody`, `vecBasePoint_PRF`) and the bonnet (identity chain in all eight cars) are unchanged.
- Wipers and steering wheels are placed with the chain and their block (section 5.2); not checked against a twin.
- The aux stream (4 bytes per vertex, always white): unused by the loader, meaning unknown.
- The morph-block flag byte (always 1) and the morph delta space (object or tangent space, scale).
- Why subtype 3 gets the morph block: the mechanism is not traced (section 5.1).
- Kinds 5, 14, 15 and 20: layouts partly observed (see section 3), not read by the 1.02 car loader. Kind 18 is
  LocoMotion's (section 14).
  Kind 6/21 is read by LocoMotion (section 12).
- The third header word (always 1): unused by the dispatcher as far as traced.
- Behaviour of the 1.0 build of `HedgeHog3D.dll` (in `Root/`): not compared with 1.02.

## 13. Discrepancies in our source

- `src/hedgehog3d/HH_3DSMaxLoaderGeom.cpp` (GeomCar, `HH_H7LoadGeomCar`) reads an `int` and then a
  center and extents before the vertex count. Its bounds are min and max in the notes, but the files hold a
  centre and a half-extent (section 5.2). The decompile of `0x1000acf0` and the files show a 24-byte
  bounds block (two vec3) followed directly by the vertex count. The source is off by 4 bytes in the bounds
  block and its fields are named wrong. Not changed here (source is read-only for this task).
- `notes/formats/cars.md` says the `.sgc` is "not parsed". Now `rbr_formats::sgc` parses it.

## 14. Node table and the car's section map handles

The kind 18 record has the `.sgs` hierarchy layout (`notes/formats/sgs.md` section 3; `rbr_formats::sgs::node_table`
reads it from either file). LocoMotion builds the car character's node tree from it, each node under the earlier node
whose id is its parent (`-1`: under the character's root). The car load's handles (`CBinarySectionMap::Lookup(1,
name)`, 0x46b160) are LocoMotion animation-object indices of these nodes. HedgeHog3D registers each built node's
name with `LocoMotion::Vf28(Vf24(character, name))` (HH 0x10034770). That is the node's last object, numbered when
`AddAnimationToCharacter` walks the tree on a stack (LM 0x1001abb0): pop the last node, give it its objects (type
0x2f PRS one, 0x30 PS two), and push its children in table order. The objects before the car's are the session's
dummy and the car-flip spectators' (`rbr-standalone/src/handles.rs`, `lead/reports/car-handles.md`). **Loader** and
**snapshots**: every value of the section map in 25 snapshots of the original.
