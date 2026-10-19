# Animated scenery files (`SGS/*.sgs`, `SGA/*.sga`)

153 `.sgs` stage-character models (marshals, spectators, animals,
driver/codriver, the Red Bull cans; each with `<name>.ini` and
`<name>_shaders.ini`) and 137 `.sga` animations for them. The readers are
`rbr_formats::sgs` and `rbr_formats::sga`; the retail tests are
`all_retail_sgs_parse`, `all_retail_sga_parse` and
`stage_objects_reference_sgs_and_sga` (`tests/real_data.rs`).

Evidence labels: **loader** = read from the 1.02 decompile or disassembly
(addresses below); **file** = checked over all retail files with the reader
or a script; **source** = our C++ reconstruction (`src/hedgehog3d`,
`src/locomotion`; ours, may be wrong); **inferred** = plausible, not proven.

Sources: `original/retail-1.0/install/SGS`, `SGA` and `Root/Scripts.rbz`
(retail); `original/patch-1.02/files/Main_group/HedgeHog3D.dll` and
`LocoMotion.dll` (1.02 code).

## 1. How stages reach these files (neither object lanes nor `.dls`)

Stage `.lbs` (2,14) animation objects do not name models: both material
names of all 7,041 items end in `.LUA` (**file**). Those scripts (in
`Root/Scripts.rbz`, per rally) call `app:LoadCharacter` on an `SGS/*.sgs`
triple (model, `.ini`, `_shaders.ini`) and `app:LoadAnimation` on
`SGA/*.sga` files, then `app:AddAnimationToCharacter`
(**file**: e.g. `Scripts/load_marshal_walk.lua` picks one of three
`<rally>_marshal_0[012].sgs` at random plus `anim_left_idle.sga` and
`anim_walk.sga`). The `.lbs` (1,1) object lanes hold static geometry and
the `.dls` holds drivelines/markers; neither references scenery files.

Script coverage (**file**): 117 of 153 `.sgs` stems are loaded by some
script (unreferenced: the `driver/codriver_*_1` and `*_3`-onwards variants,
the mechanics, the frontend/prize characters, `redbull_can_01`,
`richard`, `sheep_01_02`) and 117 of 137 `.sga` stems (unreferenced:
frontend/prize idles, the `Ps2` cans, `can2`, `candis`, the `playercan`
set, `bird_strike1`, `end_walker_02_b*`, `richard`). Dangling references
both ways exist (scripts naming `*_flip.sgs` and prize `.sga` files that
do not ship).

## 2. Container (shared with `.sgc`)

Same banner (23 bytes, `|-|3<|&3|-|()& 5<3|\|3` + NUL) and record framing
(`[u32 12][kind][subtype][u32 1][u32 len]`, tiling) as the car models
(**loader**: the HH character loader compares the same literal,
`HedgeHog3D.dll` 1.02 `0x100072a0`; the LM loaders compare it too,
`LocoMotion.dll` 1.02 `0x10015280` and `0x10018130`).

Retail record census (**file**):

| Kind/subtype | `.sgs` (153 files) | `.sga` (137 files) | Read by |
|---|---|---:|---|
| 0/1 static mesh | 8 | — | HH char loader (`0x1000b490`, plain) |
| 0/2 skinned mesh | 148 | — | HH char loader (`0x1000b490`, skinned) |
| 1/10 omni light | 18 | — | skipped (decoded by length) |
| 2/11 camera | 3 | — | not in char loader (checked by length) |
| 3/13 helper | 1,444 | — | HH char loader (`0x1000c310`) |
| 3/14 bone | 3,800 | — | HH char loader (`0x1000c430`) |
| 4/0 IK node | 18 | — | skipped (node header only) |
| 5/17, 5/18 material | 1,507 | — | skipped |
| 6/21 animation | — | 4,069 | LM scene loader (`0x10018130`) |
| 14/0, 15/54 | 3,758 | — | skipped |
| 18/0 hierarchy | 153 | — | LM `LoadNodes` (`0x10014c20`) |
| 20/0 range | 153 | 137 | LM named range (`.sga`); skipped (`.sgs`) |
| 22/0 point | 100 | — | HH char loader (`0x1000c550`) |

No retail `.sgs` holds kind 0/3, 6/21, 16/0 or 17/0; no retail `.sga`
holds anything but 6/21 and 20/0. Kinds 1 and 4 fall into the HH
loader's default case (skip by length, `src/hedgehog3d/HH_H6_Loader.cpp`).

## 3. `.sgs` layouts

All offsets are body-relative. Layouts follow the loader functions named
below; every body must be consumed exactly (reader-checked on all files
that tile; see section 6 for the two that do not).

- **Node header** (kinds 0, 1, 2, 3, 4, 22): NUL-terminated name, two i32
  (`id`, `parent`, `-1` = root), the 56-byte transform block and the 3x4
  world matrix — the same H7 readers (`0x1000c7a0`, `0x1000c6c0`) and the
  same field order as `.sgc` (see `notes/formats/sgc.md`, section 4).
- **Static mesh (0/1)**: the `.sgc` mesh (centre and half-extent bounds,
  vertex/UV/aux streams, `u16` triangle list, faces) but the body ends
  after the faces: no morph block, no morph lists (**loader**: the generic
  path returns after the faces for a non-deformable object). **File**: all
  8 meshes; bounds are centre/half-exact (0.0 error, one 0.001); faces
  partition the triangles.
- **Skinned mesh (0/2)**: node header, then two vec3, a vertex count and
  stream flags (`FUN_1000b490`, source `HH_H7LoadGeom`). The flags are 23
  in all 148 meshes; bit 0 = 24-byte position/normal, bit 2 (value 4) =
  8 bytes, bit 4 (value 16) = 4 bytes; bit 1 (value 2) is set but the
  loader tests only bits 0/2/4, so it reads zero bytes. Then the `u16`
  triangle list, faces (name + four range words, as `.sgc`), then the
  skin block (`HH_H7LoadSkin`, `0x1000bb50`): a 3x4 head matrix, a bone
  count, per bone a name and a 3x4 matrix, then a count word with that
  many 32-byte records running to the body end. **File**: all 148 consume
  exactly; the count word always equals the vertex count. The decompile
  reads no file word after the tail (scratch comes from the in-memory
  bone count); our source's trailing scratch-count read matches no retail
  bytes.
- **Light (1/10)**: node ids and transform, then color vec3, world matrix
  and an enable word (`HH_H7LoadLight`, `0x1000c090`). **File**: all 18
  have the flag 0 (nonzero would add two floats).
- **Hierarchy (18/0)**: a node count, then per node `[u32 12][u32 19]`
  `[u32 type][u32 1]`, a skip word, a name, two ints, a 3x4 world matrix
  and a flag word (`LoadNodes`, `0x10014c20`). **File**: all 153 tables
  consume exactly; chunk is always `(19, 47, 1)` except three `(19, 48,
  1)` nodes (type 47 = 0x2f PRS node, 48 = 0x30 PS node).
- **Camera (2/11), point (22/0), IK node (4/0)**: name and ids, then the
  `.sgc` tail lengths (camera 132, point 104, IK nothing further).
  **File**: exact in all cases.
- **Kinds 5, 14, 15, 20**: same bodies as in the car files, skipped by
  both loaders here too.

## 4. `.sga` layouts

- **Range (20/0)**: two floats passed to `AddNamedRange`
  (**loader**, `0x10018130`). 53 distinct pairs in retail.
- **Animation (6/21)**: `<node>::<slot>` name, an inner header `[u32
  12][u32 type][u32 subtype][u32 1]`, a controller length word (checked
  against the bytes that remain), and the controller (**loader**,
  `LM_LoadScene` switch; `src/locomotion/CLM_SceneLoader.cpp`).
  - `(7, 33)` sampled PRS: a count word and the samples in one of three
    shapes, told apart by the remaining length (52 bytes per sample):
    `52 * count` is times-then-matrices — the order `LoadPRSSampled`
    (`0x10014660`) reads — `52 * (count + 1)` is interleaved `(time,
    matrix)` pairs, and a lone 52 bytes are two unexplained words and one
    matrix. **File**: 3,859 block, 41 interleaved, 19 single-matrix; times
    run forward in every multi-sample binding.
  - `(8, 31)` XYZ position: three nested float controllers (the loader's
    `::x`, `::y`, `::z`; the wrapper chunk is unchecked, as in the
    loader). **File**: 20 bindings (the `Camera ROTO` positions).
  - `(10, 0)` scale and `(11, 24)` float (the `Camera ROTO` roll): one
    track each. Track types are linear (`0x2e` float, `0x2d` scale) and
    Bezier (`0x26` float, `0x25` scale); each track starts with two
    out-of-range words and a key count (**loader**: `CLM_TrackLoader.cpp`,
    `0x100160f0`/`0x10016c90`). **File**: 20 + 20 bindings.
  - `(22, 0)` IK chain (`IK ChainNN::IK_CHAIN`, 90 records): the loader
    has no switch case for inner type 22 and skips them. **File**: the
    length word is always 0, so the payload is empty.

## 5. Open fields and anomalies (all in script-unreferenced files except §6)

- Skinned-mesh bounds (0/2): the two vec3 match the stored-vertex box
  neither as min/max nor as centre/half-extent (checked on all 148).
  Meaning unknown.
- Skinned-mesh 8-byte and 4-byte streams, 32-byte skin records: kept raw.
- Interleaved PRS order (41 bindings in `anim2_REDBULL_BurnsCAN_01` and
  `anim2_REDBULL_playerCAN_a_01`): the traced loader reads block order,
  so how the game plays these is open. Neither file is referenced by any
  script.
- Single-matrix bindings (19 in `anim2_REDBULL_playerCAN_b_01`,
  unreferenced): count word 293 with one matrix; the second word runs `i
  * 293 * 52` in file order, as if each binding held 293 samples. Both
  meanings open; the loader would over-read (its reads are unchecked).
- Kinds 5/14/15/20 bodies, the kind-20 `.sgs` word, light attenuation
  floats (no nonzero retail flag), hierarchy flag word semantics beyond
  the bone test: not decoded.
- Behaviour of the 1.0 `Root/HedgeHog3D.dll` build (a different build
  from 1.02): not compared.

## 6. Files that do not tile

- `SGS/REDBULL_prize_01.sgs`: 77 records tile, then 7,635 non-record bytes
  (floats, ending with a `Bone_handL` name block). Referenced by the
  prize-can scripts.
- `SGS/moose_01.sgs`: all records tile except 24 trailing bytes (floats).
  Referenced by the moose/deer stage scripts.
- The parser accepts both and keeps the tails verbatim in
  `Sgs::trailing` (pinned with exact lengths in `all_retail_sgs_parse`;
  151/153 have empty tails). What the game does with the trailing bytes
  is unverified (the traced record loop would read past them; its reads
  are unchecked).

## 7. Readers

- `rust/rbr-formats/src/sgs.rs`: `parse` into `Sgs { elements, trailing }`
  (`records` still requires exact tiling),
  `Element::{StaticMesh, SkinnedMesh, Helper, Bone, IkNode, Light,
  Hierarchy, Unparsed, Ignored}`. Static meshes reuse the `sgc::Mesh`
  type (no morph data); everything else is decoded by length against the
  loader functions above. Unit tests cover truncation and single-byte
  edits (no panics).
- `rust/rbr-formats/src/sga.rs`: `records`, `Element::{Animation,
  Range}`, `Controller::{SampledPrs, InterleavedPrs, SingleMatrix,
  PositionXyz, Scale, Float, SkippedIk}` with float/scale linear+Bezier
  tracks. Rotation, look-at, Euler, link-constraints, plain-PRS and IK-load
  bindings never occur in retail files and are rejected.
- Retail tests (`RBR_DATA`, `tests/real_data.rs`): record counts per kind;
  static-mesh bounds centre/half; face partitions; index ranges; one skin
  record per vertex; hierarchy chunk shapes; light flags; PRS time order
  and the three sample shapes; camera track key counts; the two non-tiling
  files as named failures; every (2,14) material name ends in `.LUA`.
