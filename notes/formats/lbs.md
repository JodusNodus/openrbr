# Stage geometry (`Maps/*.lbs`)

Status: container, geometry (1,0), object (1,1), and interactive (1,6)
payloads parsed by `rust/rbr-formats/src/lbs/`. Other unhandled chunks are
kept raw. Retail data:
88 files (86 stages plus `Clouds.lbs` and `skyboxes.lbs`), 1.71 GB in total.
The largest is `track-12_O.lbs` at 45.3 MB; object lanes (1,1) are 1.15 GB
of it and geometry (1,0) is 0.54 GB.

Evidence: HedgeHog3D (`HedgeHog3D.dll`, decompile in `out/decomp/`), read with
`src/hedgehog3d` as the frozen reference, and checked on all retail files
through `rust/rbr-formats` tests (`all_retail_lbs_parse`). Labels: **verified**
= checked on the retail files; **loader** = read from the loader code;
**inferred** = fits the data but not confirmed.

## How the exe reaches the loader

- `CPC_TrackState::LoadTrack` (exe `0x466f10`) builds the path with
  `sprintf("%s%s%s.%s")` from the track directory, `TrackName`, a
  time-of-day part and the `TrackExt` key (`.lbs`) (**inferred** which
  `sprintf` result is the `.lbs` path; the format and the key are **loader**).
- It calls the HedgeHog3D world through vtable slot `+0x18` of the object at
  `DAT_007ea990+0x40` (`CHH_World::Unk18`, HH `0x100661c0`, thiscall, args:
  path string, two strings).
- `CHH_World::Unk18` calls `CHH_TrackLoader::LoadTrack` (HH `0x10052220`).
  That reads the path from its first argument (ecx, a `std::string`), opens
  it with `CDiskReader::Open` (HH `0x10089df0`, `ReadAll` into one heap
  buffer), then walks the chunks from the reader position.
- Our reconstruction (`src/hedgehog3d/HH_TrackLoaderLoad.cpp`) takes the path
  from the third argument and calls `HH21_ReaderInit` without a path, so it
  does not open the `.lbs` file. The original does (above). Flagged for the
  HedgeHog3D owner, not changed here.

## Container (verified)

Same framing as `.trk` (`rust/rbr-formats/src/trk.rs`, `notes/formats/stage-files.md`):

```
repeat to EOF (chunks tile the file exactly):
  u32 header_len = 8
  u32 type
  u32 subtype
  u32 payload_len
  u8  payload[payload_len]
```

The loader reads `header_len` bytes into `int header[2]`, then `payload_len`,
then dispatches on `(type, subtype)`. Its dispatch table
(`LoadTrack`, loader) and the chunk census over all 88 files:

| type,subtype | files | total bytes | handler (HH) | parsed |
|---|---:|---:|---|---|
| 0,23 | 88 | 172,856 | none (type 0 not dispatched) | yes: texture table |
| 1,0 | 86 | 541,839,298 | `HH21_Block10` `0x100553b0` | yes: geometry |
| 1,1 | 86 | 1,150,641,162 | `HH21_Block11` `0x100569f0` | yes: object lanes |
| 1,2 | 17 | 69,304 | `Block12` `0x100589a0` (clouds) | yes: clouds (sky layout, kind 1) |
| 1,3 | 81 | 7,720,636 | `Block13` `0x10058820` (sky dome) | yes |
| 1,4 | 66 | 106,920 | `HH_H23_Block14` `0x100590b0` | yes: clipping planes |
| 1,6 | 76 | 3,633,887 | `Block16` `0x10057fd0` (interactive) | yes: named objects, draws, visibility records |
| 1,7 | 10 | 274,834 | `Block17` `0x100578d0` (reflection) | yes: reflection faces |
| 1,8 | 33 | 1,892,112 | `Block18` `0x10057080` (water) | yes: water faces |
| 1,9 | 13 | 377,729 | none (not dispatched) | no |
| 2,10 | 86 | 317,024 | `Block2a` `0x10056d90` (visibility) | yes |
| 2,11 | 86 | 7,568 | `HH_H23_Block2b` `0x10059990` (car start location) | yes |
| 2,12 | 86 | 328,808 | `Block2c` `0x10056b20` (driveline points) | yes |
| 2,13 | 86 | 769,168 | none (not dispatched) | no |
| 2,14 | 86 | 1,282,682 | `HH_H23_Block2e` `0x10059230` | yes |
| 2,15 | 86 | 38,318 | `HH_H23_Block2f` `0x10059690` | yes |
| 2,16 | 86 | 2,064 | `HH_H23_Block210` `0x100597c0` | yes |
| 2,18 | 86 | 791,700 | `HH_H23_Block212` `0x10059850` | yes |

Chunks of types other than 1 and 2 are never dispatched. The loader walks
past their payloads all the same.

Two chunks are identified with the Blender track exporter's names
(`notes/references/README.md`) and checked on retail:

- `(2,10)` visibility, 24 bytes per entry: two vectors (loader
  `HH21_Block2a`, `0x10056d90`; **loader**). The lane index is the entry
  position. Filed per entry into the world's `+0x3a0` holder vector.
  Retail: `4 + 24*n` tiles the payload in 86/86 files (9..361 entries).
  Each entry is a visibility cell's box, centre then half extent (see
  Visibility cells).
- `(2,11)` car start location, 88 bytes: a 4x4 matrix, a Vec3 position and
  a Vec3 axis-angle rotation (loader `HH_H23_Block2b`, `0x10059990`, which
  skips the 0x40 matrix bytes and reads the two vectors; **loader**). The
  exporter says the game uses only the position and rotation. Retail: the
  position equals the matrix translation, and it lies 7 m (median) to
  39 m (max) from the nearest driveline point once y and z are swapped.
  Community exporters leave the matrix zero, so the renderer builds the
  start pose from the position and rotation (`notes/formats/rsf-stages.md`).
- `(2,12)` driveline points: u32 count and `count` Vec3 values (loader
  `HH21_Block2c`, `0x10056b20`; **loader**). Each
  sampled point equals a `.trk` `(3,0x14)` driveline position with y and z
  swapped (`.lbs` is y-up, `.trk` z-up), in all 86 files. The earlier
  "sky arcs" name came from the H22 holder that keeps them. `Block2c` does
  sum the arc lengths between points. The exporter says the game loads
  these points but does not use them.
- `(2,14)` animation objects: u32 count, then per item a NUL-terminated
  name, three words, two NUL-terminated material names, five Vec3 values
  and a packed-color word (loader `HH_H23_Block2e`, `0x10059230`, which
  splits the color into four `/255` floats; **loader**). Retail names are
  animation objects (marshals, birds, `... AnimObject`); the material
  names are `.LUA` script paths. The handler files each item opaquely under
  its name, so the vector roles are read off retail values, not loader names
  (**inferred**): `vectors[0]` is a track-scale position, `vectors[1]` euler
  angles in degrees (yaw-only in about half the items; pitch/yaw/roll in
  camera items), `vectors[2]` is a unit scale in 7,040 of 7,041 items,
  `vectors[3]` is a second point shared within object groups (up to ~4 km
  from the first), and `vectors[4]` is a group-shared extent triple. The
  name words read as (object id, flags, group id): the third word is shared
  within groups. Exact
  consumption in 86/86 files (7,041 items).
- `(2,15)` container objects: u32 count, then per item a NUL-terminated
  name and three words (loader `HH_H23_Block2f`, `0x10059690`;
  **loader**). Exact consumption in 86/86 files (1,525 items).
- `(2,16)` track vectors, 24 bytes: two Vec3 values (loader
  `HH_H23_Block210`, `0x100597c0`, into the loader head words;
  **loader**). No traced consumer reads the head words back. By value the
  first vector is a high-altitude track-scale point and the second a unit
  vector, and the unit vector is shared across stages of the same rally
  (consistent with a per-rally sun direction; **inferred**, unproven).
- `(2,18)` visible-object vectors: 60-byte records of four Vec3 values
  plus three words tiling the payload (loader `HH_H23_Block212`,
  `0x10059850`; **loader**). No count word: the loader iterates its
  visibility count, which equals the `(2,10)` entry count in 86/86 files
  (**verified**).
- Exporter names for the other subtypes: `(1,4)` clipping planes, `(1,9)`
  unknown, `(2,13)` unknown, `(2,14)` animation objects, `(2,15)`
  container objects, `(2,16)` track loader vectors, `(2,18)` visible
  object vectors. Not checked.

The first chunk of `track-10_O.lbs` is `(2,11)`, 88 bytes; its bytes are the
ones in the brief (`44 27 0f 3f ...`, floats). The parsed chunk walk reaches
EOF exactly in all 88 files.

## Interactive faces (1,6) (verified: all 76 chunks consume exactly)

`HH21_Block16` (`src/hedgehog3d/HH_TrackBlocksC.cpp:568`, address
`0x10057fd0`) reads an object count, constructs one
`CHH_KInteractiveObject` per entry, and calls `HH22_FillInteractiveItem`
(`:638`, `0x10058170`). The reader calls `HH21_ReaderReadString` for the
name; retail stores it NUL-terminated, followed by a byte at object offset
`+0x49` and a word. The zero path reads a count of records made from one
word and a 4x4 matrix. The nonzero path reads
draw flags, optional texture-manager keys (`0x10` and `0x20`), vertex/index
buffer fields, and builds `SHH_KDrawItem`s. The source's nonzero path only
shows one draw record and does not show the trailing visibility list. The
layout below records the repeated draw count and trailing list found by
exactly consuming every retail payload; those details are not represented in
the checked-in reader transcription.

```
u32 object_count
Object[object_count]:
  NUL-terminated name
  u8  flag                         (stored at CHH_KInteractiveObject +0x49)
  u32 draw_count
  Draw[draw_count]:
    u32 flags
    u32 unknown                    (not used by the reader)
    if flags & 0x10: u32 texture_key0
    if flags & 0x20: u32 texture_key1
    u32 vertex_size
    u32 pool_stride
    u32 index_count
    u16 index[index_count]
    u32 vertex_count
    u8  vertex[vertex_count * vertex_size]
  u32 visibility_count
  Visibility[visibility_count]:
    u32 unknown
    f32 matrix[16]
```

Retail findings (the `all_retail_lbs_parse` test checks each object and draw
in every `Maps/*.lbs` file): there are 76 stage chunks, 1,127 objects, 1,992
draw records, and 13,703 visibility records. Each draw count is 1–4; draw
flags are `0x15`; the unused word is 13 or 49; `vertex_size` is 24 and the
pool stride word is 322. All index counts are triangle lists, all indices
fall below their vertex counts, and all visibility matrices contain finite
floats. Interactive texture keys resolve through the runtime texture-manager
map (`m_infoTextures` in the reader), so they are not stage `[TextureInfo]`
indices.

The parsed records do not prove that an object is initially visible. The
reader stores their first word at object offset `+0` and initializes the
separate `+0x48` field to zero; `CHH_KInteractiveObject::UpdateVisibility`
(`src/hedgehog3d/HH_KInteractive.cpp:191`) passes that live `+0x48` field to
`CHH_World::Unk80` (`HH_World.cpp:267`), which reads a runtime
visibility-holder flag. `HH_H24_VisFill5` and `HH_H24_VisSelect`
(`HH_H24_Big.cpp:544`, `:634`) update visibility from a camera point or
selection box. The `.lbs` data does not contain the stage-start camera
visibility result. At load `CInteractiveObjectManager::Init` (0x100706c0)
asks the world (slot 0x7c, `CHH_WorldUnk3A0::Unk63270` 0x10063270) for the
first cell whose box holds the record's translation across (x and z, bounds
included) and stores that index at `+0x48`; `Unk80` (0x10066a80) then reads
that cell's per-frame visibility flag (`+0xc8`), so a prop shows while its
cell does (**loader**; see Visibility cells).

## Visibility cells (2,10) (visibility1)

**Data (verified, all 86 retail files and RSF 201, 204).** The `(2,10)` count equals the geometry block count and
the object lane count, and cell `i` belongs to block `i` and lane `i` (`all_retail_lbs_parse`). Each cell's box holds
its block's bounds; its top is raised by 0 to about 10 m over the block's, by a rule not traced, and the lane's
objects may stick out of it.

**Per frame (decompile and asm, HedgeHog3D).**
- `CHH_World` slot 0x28 (0x100671c0) copies the camera's plane set (`m_planeIndexB` = 6 and `m_planesB`, camera
  `+0x1e8`, 100 bytes) into the `+0x3a0` holder, then `HH_H24_VisSelect(holder, 0)` (0x10062dd0) classifies every
  cell's box against the planes (`FUN_100331c0`: 2 outside, 1 crossing, 0 inside). A cell that is not outside gets
  its flag `+0xc8` set and its index into the lane-0 list (`+0x230` count, `+0x284` list); each object item in the
  cell is classified again and listed (`+0x1a0`, `+0x1f4`) for `CKObjectBlock::RenderSetup` (0x1002d830).
- `HH_KObj_Unk2f600` (0x1002f600) draws only the listed cells' blocks (`FUN_10027590`, with the levels of detail
  above).
- The planes: `CHH_CameraFreeFly` 0x10013900 builds six view-space planes (`m_planesA`) from a point and a normal
  each (0x100a7e5d, a D3DX library routine by its arguments, unnamed in `data/hedgehog3d.csv`), using the field of
  view, `m_near` (`+0x16c`) and `m_far` (`+0x170`) (or the render options' `+0x24`/`+0x28`). `UpdateProjection`
  (0x10013b20) transforms them into `m_planesB` with the transposed view (`_D3DXMatrixTranspose@8` 0x100a5abd, then
  0x100a7f5d per plane). So the far plane is at `m_far`, while the
  projection itself uses a far of 10,000.
- `m_far` is `renderer m_unk24 * CSigCamera+0x48` (`CHH_World::OnCamera` 0x10066dc0). The exe's camera far `+0x294`
  is 300.0 (`DAT_0072f5e0`, camera init); its one other writer is message 0x10 of 0x477d80, the Lua `SetFarPlane`
  binding, which no retail script calls. `m_unk24` is 1.05 in `CRendererHigh` and 0.9 or 1.0 in `CRendererLow`.
- The holder keeps two lanes of lists: lane 0 is the main view's (above), lane 1 the environment cube's. Each lane has
  its cell count `+0x230 + lane*4` and list `+0x284 + lane*0x10`, its item count `+0x1a0 + lane*4` and list
  `+0x1f4 + lane*0x10`, and per-cell and per-item test codes (`+0x244`, `+0x1b4`). The draw calls take a three-word
  clip `{1, lane, 6}`: `HH_KObj_Unk2f600` (blocks, world `+0x184`) reads the lane's cell list from `clip+4`, and
  `HH_KObj_Unk2db00` (lane objects, world `+0x1cc`) its item list.

**The environment cube (visibility2; asm and decompile, HedgeHog3D 1.02).** Every path runs only while the texture
manager has no static env map (`m_envMap` null) and the world has a car (`+0x254`).

| function | called from | selection | reach | centre |
|---|---|---|---|---|
| `0x10069cf0` | `CRendererHigh::Render` 0x100763f0 (also with render option `+0xf`), `CRendererMedium::Render` 0x10077220, `CRendererDevelop::Render` 0x10075ce0; while camera mode `+0x64c` is not 2 and `+0x3c9` is set | `HH_H24_VisFill5(holder, centre, lane 1, 10.0)` | 10 m (`0x101b42e0`) | car `+0x170` plus the shader manager's `m_unk400` translation (uploaded as VS constant 0x32) |
| `0x1006a2b0` | `CHH_World::UnkAC` 0x1006bef0 (IHH_World slot 0xac), which the exe's application-state handler `CApplication::M004801d0` (0x4801d0) calls on its way out (how often not traced) | `VisFill5(..., lane 1, 400.0)` | 400 m (`0x101b43dc`) | car `+0x170` plus world `+0x654`, the first `(2,16)` track vector (`HH_H23_Block210` 0x10059990) |
| `0x100695c0` | `CRendererCustom::Render` 0x100755e0, `CRendererDevelop` and `CRendererMedium`, only while `+0x3cb` is set (slot `vf3c` 0x100695b0 sets it, the call clears it) | per face: the six planes at world `+0x3f8 + face*100` moved to the centre (`0x100a6267` translation, `0x100a7f5d` per plane) into the holder, then `HH_H24_VisSelect(holder, 1)` | the stored face planes (their fill not traced) | the shader manager's `+0x430` point |
| `0x1006a770` | no caller (no call, jump or pointer to it in the DLL) | per face planes, then `VisFill5(..., lane 1, 1.0)` | 1 m (`0x101b4184`) | as 0x10069cf0 |

- `HH_H24_VisFill5` (0x10062bc0; lane in `esi`, radius in `xmm0`, holder in `ecx`): for every cell, the squared
  distance from the centre to the cell's box (0 inside, per axis beyond the faces) against the squared radius,
  bounds included. A cell that passes goes into the lane's cell list with code 0 (inside) and all its lane objects
  into the item list with code 0: no plane test, no per-item test, and the cell flag `+0xc8` (the main view's, read
  by the interactive props) is left alone.
- `0x10069cf0` per frame: the face count `+0x674` steps up by one (to 6) after a frame shorter than 25 ms and down by
  one (to 1, then forced to 2) after one longer than 30 ms (`GetTickCount`); `+0x678` counts the frames that hit the
  bottom and stops the cube updating once it passes 125. It draws `+0x674` faces, cycling the face index `+0x3b8`.
  Each face: the selected skybox (`+0x1bc`, `HH_KObj_Unk2eaa0(.., 1)`), the cloud layer (`+0x1b8`), the superbowl
  (`+0x1c8`), then lane 1's blocks and objects. No water, reflection items or interactive objects (those draw from
  the renderers' `Render` through `CHH_KInteractiveObject::Render` 0x1002a790 only). The face projection is the
  view's (world `+0x360`).
- `0x1006a2b0` draws all six faces once the same way, plus the vertex sets (`+0x1d0`, `HH_RenderVertexSets`): a
  400 m cube from that point (by the vector's values high above the stage; inferred). Not ported.
- `0x100695c0` ends with `D3DXSaveTextureToFileA("l:\\cube.dds")`: a developer capture, not the game's look.
- **Sun shadows:** the game draws no sun shadow maps of the stage, so no lane feeds one; the renderer's cascades are
  ours and keep their own reach.
- **Ported** (`visibility::near_cube`, `CUBE_REACH`; `renderer/frame.rs`): our cube (one face per frame) draws the
  meshes whose cell passes the 10 m test from the cube's centre (the car's box centre) and no interactive props;
  meshes without a cell keep the renderer's 500 m reach. The adaptive face count is not ported. The renderer's own
  readers of the car's surroundings (its light, the wet roads, the water's missed rays) read a second cube of ours
  that keeps the 500 m reach.

So the game draws nothing of a cell whose box lies wholly beyond about 300 m along the view, and a cell that crosses
that plane whole. `rbr-render` (`src/visibility.rs`) ports this for the main view with the high renderer's 315 m.
Without it, RSF 201's forest cards 400–500 m out hung against the sky (`lead/reports/visibility1/`).

Not checked against the running game: the far plane's value in a live frame (the plane builder was read from the
asm, not traced at run time).

## Animation and container items (2,14), (2,15): what is proven (render-m7)

Survey over the 86 stages (`/tmp/render-m7/scripts/lbs_items.py`, `script_models.py`; the item census
matches the table above: 7,041 animation items, 1,525 container items). Labels as in the rest of this file.

**Item layout (file, verified by exact consumption).** Animation item: name (34 distinct, for example `bird`
3,500 + 3,084 `Untitled`), dwords (`[id, flags, group]`), `material_a` and `material_b` (`.LUA` paths; 46
distinct script pairs), five vec3, packed colour. Container item: name (59 distinct, for example `marshal_stand`,
`birdflock`, `RandomGroup01`..`12`, `start_race`) and three dwords.

**Containers draw nothing by themselves (inferred).** A container carries no model, script or texture
reference, only name and three words. 6,034 of 7,041 animation items have `group` equal to the `id` of a
container in the same stage (for example `Untitled [34293, 0, 34287]` under `RandomGroup01 [34287, 1, 3]`), so
the containers look like parents of the animation items. Not traced: which listener of
`HH_H24_FlushF` (`src/hedgehog3d/HH_H24_Blocks.cpp`, 0x10060260; called from `HH_H23_Block2f` with the name only)
uses the container's words.

**Animation item to script (inferred, not traced).** `material_a` names a script. All 46 script names exist in
`Root/Scripts.rbz` as `Scripts/<name>.lua` (lower case; rally folders `Scripts/<rally>/` hold variants of
some, for example `montblanc/load_bird.lua`). The game's path resolution from `MAPS/GENERIC OBJECTS/SCRIPTS/...`
to that archive entry is not traced (no such folder in the install). The flush (`HH_H24_FlushE`) carries only
the item name; the materials are read into locals and dropped in `HH_H23_Block2e`
(`src/hedgehog3d/HH_H23_Blocks.cpp:101`). So how a script binds to its item is unproven.

**Script to model (proven from the script text, `Scripts.rbz`).** `startup.lua` sets `rootpath = "D:\\"`. The
scripts call `app:LoadCharacter(rootpath.."Sgs\\<model>.sgs", ...ini, ..._shaders.ini, x, <tag>)`. Every
referenced model exists in `SGS/` (the install path from `rootpath` to `SGS/` is inferred, as is the match to the
item).

| Script (items) | Models called | Kind | Status |
|---|---|---|---|
| `LOAD_BIRD` (3,500), `LOAD_BIRDSTRIKE` (88) | `bird_03.sgs` only, unconditional (same in both rally variants) | skinned 0/2 | one model |
| `LOAD_MEDIA_STAND` (1,198), `LOAD_MEDIA_*_SLOPE` (879), `LOAD_MARSHAL_STAND` (344), `LOAD_MARSHAL_WALK` (231), `LOAD_MEDIA_*_FLAT` (294), others | 2 or 3 models chosen by `LuaRandom` | skinned 0/2 | random pick (3,309 items in total) |
| redbull, mechanic, frontend, prize, driver, deer scripts (about 115 items) | one model each | skinned 0/2; static 0/1 for `rabbit_01` and `ANIM2_REDBULL_CAN1a_01`; `moose_01` has no mesh record (it does not tile) | one model; driver and deer scripts have `if` blocks around the call, not checked |

**Not proven, so nothing is drawn:**

1. Placement. `vectors[0]` is a track-space position (y about -25 to -28, the ground in the y-up `.lbs` frame), and
   `vectors[1]` is yaw in degrees (the loader `E02LuaLoader::Load`, `src/rbr/RBR_E02_Game.cpp:1340`, multiplies by
   0.01745, but that loader is not traced to the item). `vectors[3]` is a second point shared in groups (possibly
   a flight path for birds; inferred). The scripts' `scale` global (0.01 for birds, 0.1, 0.001 for marshals) is read
   by `E02LuaLoader::Load` and discarded (`(void)sc`); its use elsewhere is not traced.
2. Rest pose. Most referenced models are skinned (0/2). The static exceptions are `rabbit_01` and `ANIM2_REDBULL_CAN1a_01` (0/1); `moose_01` has no mesh record. Skin weights
   (32-byte records, `rbr_formats::sgs::Skin::weights`) are not decoded, so the bind or rest pose of the vertices is
   unproven. Animations (`anim_bird_00` ...) are not applied.
3. Textures. The bird's `bird_03_shaders.ini` names `bird_06.dds` through `sgs\textures\bird_06.dds`. The
   `Textures` folder has no `sgs` folder in the install (checked: no `bird_06*` under `Textures/`), so the path is
   not resolved (see `notes/formats/shaders.md` for the file layer).

**Consequence.** No (2,14) or (2,15) item resolves to a drawable mesh by proof, so the count of drawn items is 0 on
every stage. The bird model (3,588 items, 51% of animation items) is the one proven model class; drawing it would
need the item-to-script binding, the placement, the skin pose and the `sgs\textures` path, none of which is proven.

## Geometry (1,0) (verified: whole payload consumed exactly, all 86 files)

```
u32  block_count
f32  sky_value                  (float bits; 0, 3, 4, 8, 12, 15, 20 seen; stored to the sky holder +0x18 and never read back, so no render effect)
Block[block_count]:
  View[9]                       (index i = render slot = chunk `view`)
    u32 index_count             (bit 31 never set)
    u16 index[index_count]      (little-endian; D3DFMT_INDEX16)
    u32 vertex_count            (top 4 bits never set)
    u8  vertex[vertex_count * stride(i)]
  u32  render_chunk_count
  RenderChunk[render_chunk_count]     (0x70 bytes each)
  f32  center[3], f32 half[3]         (block bounds)
```

An empty view is `u32 0` followed by one more `u32` (always 0). The loader
skips it.

Strides are fixed per slot by the loader (`HH_H24_ReadBlock1e0`..`fd60`, which
set the stride at `obj+0xc`), not stored in the file:

| slot | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| stride | 0x10 | 0x18 | 0x30 | 0x24 | 0x3c | 0x20 | 0x38 | 0x2c | 0x44 |

Only slots 0, 5, 6, 7 and 8 occur in track-10_O; retail uses these shader
pairs (below).

### Render chunk (0x70 bytes; offsets from the record start)

The loader copies 0x70 raw bytes to its `CHH_RenderChunk` at `+0x38`
(`HH21_Block10`), so the offsets below are record offsets (`+0x38` in the
class). Drawing: `CHH_RenderChunk::Render` (`HH_Primitive.cpp:156`) calls
`DrawIndexedPrimitive(TRIANGLELIST, base 0, min_index, vertex_count,
start_index, prim_count)` on the slot's buffers.

| off | type | field | evidence |
|---|---|---|---|
| 0x00 | i32 | `view` (render slot 0..8; every retail record is 0,5,6,7 or 8) | verified: `chunk.index ≤ view` checks; loader files it by `type` |
| 0x04 | u32 | vertex shader id (9, 12, 13, 16, 17 seen) | verified (pairs below) |
| 0x08 | u32 | pixel shader id (0, 5, 6, 7, 8 seen) | verified (pairs below) |
| 0x0c | u32 | `start_index` | loader + Render |
| 0x10 | u32 | `prim_count` (triangles) | loader + Render |
| 0x14 | u32 | `vertex_count` (draw's vertex span) | loader + Render |
| 0x18 | u32 | `min_index` | loader + Render |
| 0x1c | f32[3] | bounds center | verified: the vertices a chunk draws lie within center ± half (all 103 chunks checked in track-10_O) |
| 0x28 | f32[3] | bounds half-extent | as above |
| 0x34 | u8 | not read by loader; always 1 in retail | verified value |
| 0x35 | u8 | shadow flag (0/1; 9,800 set) | loader |
| 0x38 | u32 | shadow texture index (into `NumShadowTextures`), used if 0x35 | loader + range check |
| 0x3c | u8 | specular flag (0/1; 11,643 set) | loader |
| 0x40 | u32 | specular texture index (into `NumSpecularTextures`), used if 0x3c | loader + range check |
| 0x44 | u32 | texture count: 2 (70,284 chunks) or 0 (8) | loader |
| 0x48 | u32 | texture 0 (index into `[TextureInfo]`, `Texture<N>=`) | loader + range check |
| 0x4c | u32 | texture 1 | loader + range check |
| 0x50 | u8 | vertex-shader-constant flag (always 0 in retail) | loader |
| 0x54..0x64 | f32 x4, u32 | vertex-shader constants, used only if 0x50 is set (not decoded) | loader |
| 0x68..0x6b | u32 | always 0 in retail | verified value |
| 0x6c | u8 | draw list, the level of detail: 1 near, 2 always, 3 far (37,246 / 17,193 / 15,853; see Levels of detail) | loader: 1 → RenderSet list at `+0x94`, 2 → `+0x04`, 3 → `+0x124`; render (0x10027590); capture |
| 0x6d | u8 | read twice by `HH21_Block10` (a debug `sprintf` and a texture-manager map-find gate); varies (0xff most often) | loader |
| 0x6e..0x6f | u8[2] | not read by the loader or `Render`; varies | observed only |

#### Levels of detail (render chunk 0x6c, render-m10)

A block holds most of its ground twice: a near version in list 1 and a far version in list 3, with fewer, larger
triangles whose plate-atlas tiles stretch over many metres. List 2 holds what both share. The render set's draw
(0x10027590, `HH_H12D_7590_Render`; the rule is in the decompile, not in the reconstruction) takes the distance from
the camera (`m_unk228`) to the block's box centre and compares it with `T = g_renderQuality.m_unk10`:

- `distance >= T`: list 3 only;
- otherwise list 1, and list 3 as well while `distance > T - 50`;
- list 2 always.

`T` comes from the quality preset (`HH_Renderer.cpp`: low 200, medium 300, custom 500, high 600). The reference
installs run `RenderQuality = low`, so `T = 200`; ours always uses high's 600 (`rbr_render::stage::LOD_DISTANCE`; user
decision: always the highest quality). The other path (0x10027900) draws list 3 and then list 2.

Evidence. In the `apitrace-check` capture (track-54_E, damp; `notes/formats/shaders.md`, Mip LOD bias), stage draws
were matched to render chunks by vertex buffer, start index and count. In frames 175 and 250, the drawn set agrees
with the rule at the frame's camera for every chunk of every block that was drawn: 0 mismatches over 226 chunks (92 and
134). The matched draws' textures and vertex bytes equal the file's. Drawing all three lists at every distance (the
renderer before render-m10) stacked the far ground over the near ground. That was the stretched texture on the meshes.

The loader's remap of texture keys (its `m_infoTextures`, `m_unk8C`,
`m_unk98` maps) is not in the file. The file value is the index, and the
loader maps it to a loaded texture (see Textures).

### Shader and vertex layout (verified pairs; declarations from `ShaderManager.cpp`)

Every track chunk uses one of five (vs, ps, slot) triples. The declaration is
the one `g_vsFogDescs` gives for the vs id, and its byte size equals the
slot's stride:

| chunk type (slot) | vs | ps | decl | size | elements (stream 0 unless noted) |
|---|---|---|---|---|---|
| 0 | 9 | 0 | `g_vsDecls[66]` | 0x10 | POSITION float3 @0, COLOR @12 |
| 5 | 12 | 5 | `g_vsDecls[78]` | 0x20 | POSITION @0, COLOR @12, TEXCOORD0 float2 @16, TEXCOORD1 float2 @24 |
| 6 | 16 | 6 | `g_vsDecls[89]` | 0x38 | POSITION @0, NORMAL float3 @12, COLOR @24, TEXCOORD0 @28, TEXCOORD1 @36, TEXCOORD2 @44, TEXCOORD3 float1 @52 |
| 7 | 13 | 7 | `g_vsDecls[104]` | 0x2c | POSITION @0, COLOR @12, TEXCOORD0 @16, TEXCOORD1 @24, TEXCOORD2 @32, TEXCOORD3 float1 @40 |
| 8 | 17 | 8 | `g_vsDecls[111]` | 0x44 | POSITION @0, NORMAL @12, COLOR @24, TEXCOORD0 @28, TEXCOORD1 @36, TEXCOORD2 @44, TEXCOORD3 float1 @52, TEXCOORD4 float2 @56, TEXCOORD5 float1 @64 |

Counts over all 86 stages: (0,9,0) 8, (5,12,5) 50,728,
(6,16,6) 9,756, (7,13,7) 7,913, (8,17,8) 1,887 (all chunks). Slots 1-4
(strides 0x18, 0x30, 0x24, 0x3c) do not occur in retail geometry. Candidate
declarations by stride from `g_vsDecls`: 0x18 is `RLSingleTexture.vso`
(vs 10), 0x30 is `RLSingleTextureSpecular.vso` (vs 14), 0x24 is
`RLSingleTextureSway.vso` (vs 19), 0x3c is
`RLSingleTextureSpecularShadow.vso` (vs 15). Which declaration (and shader)
each slot uses is unverified: it only shows in retail `(vs, ps, slot)`
triples, and none use slots 1-4. (The old note matching slot 3 to vs 39 /
`g_vsDecls[8]` was wrong: that declaration is 0x18 bytes.) POSITION is the
first float3 of every vertex (verified against the bounds).

## Object lanes (1,1) (verified: all 86 files consume exactly; all 24,444 items)

```
u32 lane_count
Lane[lane_count]:
  u32 flag
  if flag != 0:
    u32 n;  Item[n]
    u32 m;  Item[m]
Item:
  u32 flags
  u32 kind                       (render-state flag byte, see below)
  u32 extra10                    if flags & 0x10 (texture index for flags 0x15)
  u32 extra20                    if flags & 0x20
  u32 has_box                    (0 in all retail items)
  u32 vertex_size                (36 or 28 in retail)
  u32 unknown                    (loader advances past it without reading; 322 with 36-byte items, 66 with 28)
  u32 index_count
  u16 index[index_count]
  u8  flag2                      0: no second index buffer; 1: second buffer = first's indices (no bytes); 2: own buffer
    u32 n2; u16 index2[n2]       only if flag2 == 2
  u32 vertex_count
  if has_box:
    f32 box_a[6]
    u32 extra[4]                 only if flags & 0x10
  u8  vertex[vertex_count * vertex_size]
  f32 center[3], f32 half[3]     (the item's bounds)
```

The loader's H22 `HH22_BuildBlockItem` (`0x10056150`) reads the same fields
in the same order.

Decoded from the loader and the shader tables (verified against the data
where stated):

- **Texture.** For flags `0x15` (24,436 items) `extra10` is a texture index
  into `[TextureInfo]`. The factory `HH_TM2_0DA0_Factory` looks it up, and
  it is below `NumTextures` in all 24,436 items. For flags `0x05` there is
  no texture.
- **Kind.** `kind` is a render-state flag byte. `HH_TM2_0D10_FlagDecode`
  (`0x10050d10`) reads it: bit 0 sets a stage flag. Slot 0 mode is 3 with
  bit 0x10, else 2 with bit 0x40, else 1. Slot 1 mode is 3 with bit 0x20,
  else 2 with bit 0x80, else 1. Retail values: 49, 13, 37, 25. The mode
  names are not resolved.
- **Vertex layout.** The loader picks the vertex shader from the flags:
  `0x15` uses shader 19 (`RLSingleTextureSway.vso`) and `0x05` uses shader 18
  (`ShaderManager.cpp`, `g_vsFogDescs`). The declarations give:
  - flags `0x15`, 36 bytes: POSITION float3 @0, COLOR @12, TEXCOORD0 float2
    @16, TEXCOORD1 float3 @24. TEXCOORD0 is in [0,1] for 96.7% of vertices.
    TEXCOORD1 is not decoded: across 24,436 items 13,573 hold x = 0 with
garbage-magnitude y/z (uninitialised exporter output), 702 varying items
hold sane values (x in [0, 1], y in [0.8, 1.2], z in [0.003, 3.12]), and
the rest mix garbage into one component. No height correlation; the
consuming shader use is untraced.
  - flags `0x05`, 28 bytes: POSITION @0, COLOR @12, TEXCOORD0 float3 @16
    (inferred UV in the first two components).
  Both strides match the declarations. The bounds at the end are the center and half-extent of
the item's vertex positions: checked for all 24,444 retail items (max error
2.4e-4). POSITION is the first float3 of each vertex (inferred from that
check; the 36-byte layout beyond it is not decoded). `flags` is 0x15 for
almost every item (24,436) and 0x05 for 8. Flag 0x35 is handled by the
loader but not present. Every index is below the item's `vertex_count`.

## Sky dome (1,3) (verified: all 81 files consume exactly; 897 items)

```
NUL-terminated name (for example MAPS/ARCTIC/10 KAIHUAVAARA/10 SUPERBOWL.KSH)
u32 count
Item[count]:
  f32 centre[3]                  (before each item; loader skips it; looks like a position)
  u32 flags                      0x15 (623 items) or 0x35 (274)
  u32 kind                       (render-state key, not resolved)
  u32 texture0                   if flags & 0x10 ([TextureInfo] index)
  u32 texture1                   if flags & 0x20 ([TextureInfo] index)
  u32 vertex_size                24 (flags 0x15) or 32 (flags 0x35)
  u32 unknown                    (not decoded)
  u32 index_count;  u16 index[index_count]
  u32 vertex_count; vertex[vertex_count * vertex_size]
```

Vertex shaders from the loader (`HH22_FillSkyItem`): 0x15 uses shader 10
(`RLSingleTexture.vso`, decl POSITION, COLOR, TEXCOORD0 float2 = 24 bytes), and
0x35 uses shader 12 (`RLDoubleTexture`, the same plus TEXCOORD1 float2 = 32
bytes). Both sizes match. Every texture index is below `NumTextures` and every
index is below `vertex_count` (checked in the retail test).

## Water (1,8) and reflection (1,7) (verified: all 43 chunks consume exactly; 141 items)

Parsed in `lbs/` (`Water`, `Reflection`, `SurfaceFace`, `SurfaceItem`)
from the two handlers (`HH21_Block18` `0x10057080` water, `HH21_Block17`
`0x100578d0` reflection). One shared file layout; reflection advances past
the sign-gated pairs without storing them, water stores them into the item
at `+0x48`..`+0x5c`. Layout:

```
u32 face_count
Face[face_count]:
  NUL-terminated name   (RIVER*, WATER*, STREAM*, LAKE WATER, WATERBOWL, WATER_REFLECTION, REFLEXSKY, REFLECTION)
  u8  (skipped)
  u32 item_count
  Item[item_count]:
    u32 flags
    u32 key0                       (unconditional; 13 or 49 in retail)
    if flags & 0x10: u32 key; if flags & 0x80: f32 pair (8 bytes)
    if flags & 0x20: u32 key; if flags & 0x80: f32 pair (8 bytes)
    if flags & 0x40: u32 key; if flags & 0x80: f32 pair (8 bytes)
    u32 vertex_size; u32 unknown; u32 index_count; u16 index[index_count]
    u32 vertex_count; vertex[vertex_count * vertex_size]
```

Decoded from the handlers and retail (all 43 chunks tile exactly):

- **Flags.** Seen: 0x15 (38 items), 0x35 (13), 0x57 (15), 0xd7 (71),
  0xf7 (4). The low bits (0x01/0x02/0x04) are never read; bit 0x80 gates
  the pairs (checked against the low byte, same result).
- **Stride and unknown follow the flags exactly**: 0x15 gives 24 bytes and
  322, 0x35 gives 32 and 578, 0x57 and 0xd7 give 48 and 338, 0xf7 gives
  56 and 850. The strides match the water vertex-shader declarations
  (`ShaderManager.cpp`): 48 is `WaterSingleTexture.vso` (vs 29),
  56 is `WaterDoubleTexture.vso` (vs 30). The loader's texture-slot count
  (`local_c4`: 0x10 and 0x20 each add one) picks the single- vs
  dual-texture object, consistent with the stride split.
- **Keys.** The unconditional word is 13 or 49; the 0x10/0x20 keys are
  small integers (25..38 style); the 0x40 key is 0 or 1. All resolve
  through the texture manager's runtime maps (`m_infoTextures`,
  `m_unk98`), whose lookup keys are hidden in the decompile, so the roles
  (texture vs specular) are unproven and the keys are **not** checked
  against `NumTextures` (word0 = 49 occurs in stages with 35 textures).
- **Pairs.** Small floats: (0.1, -0.1) and (0.04, 0.015) style for the
  0x10 group, (0.0, 0.0) and (0.04, 0.02) style for the 0x40 group. Texture scroll per unit of time:
  the 0x10 pair moves `TEXCOORD0`, the 0x20 pair `TEXCOORD1` of the 56-byte items, and the 0x40 pair is
  unused (`HH_KWaterRenderItems` 0x10030170 uploads +0x48 and +0x50 as c85 and c86 for the water vertex
  shaders; `notes/visual-enhancements.md`, `water`).
- Every u16 index is below its item's `vertex_count` (checked in the
  retail test).

## Textures and the `.rbz` link

- The stage `.ini` `[TextureInfo]` gives `NumTextures`, `Texture<N>=<name>`,
  `NumShadowTextures` / `ShadowTexture<N>`, and `NumSpecularTextures` /
  `SpecularTexture<N>`, with one section per texture (`MipLevels`, `TextureFormat`, ...).
  The loader reads these (`HH_TrackLoaderTextures.cpp`). Chunk texture
  indices are indices into these lists (verified: every retail chunk's
  texture, shadow and specular index is in range).
- Resolution (**verified** by `all_retail_stage_textures_resolve`; loader
  `HH_TrackLoaderTextures.cpp` and the decompile at `0x10054680`):
  - The name is `Texture<N>`. `<stem>` is its last path component without its
    extension (`CZipPath::m_base`). The `.tga` suffix is dropped. For
    example `MAPS\...\AR_SUPERBOWL_11.tga_0.tga` gives `ar_superbowl_11.tga_0`.
  - Ground textures (`IsGroundTexture=true`, flag byte `+0x1f`) request
    `<track>_textures\dry\new\<stem>.dds`. Others request
    `<track>_textures\dry\<stem>.dds`. Default weather `dry` and wear `new`
    (wear index 0). Both strings are lower-cased.
  - `TextureFilenameMap.dat` (6,970 pairs, keys unique) maps the request to a
    target path. Requests with no entry stay as they are. The wear folder is
    handled this way: `dry\normal\ice_patch.dds` maps to `dry\new\ice_patch.dds`.
  - Shadow textures (`ShadowTexture<N>`): `<track>_textures\<name>`, a raw
    stream. Specular (`SpecularTexture<N>`): `<track>_textures\dry\new\<stem>.dds`,
    then `<track>_textures\dry\<stem>.dds` when that is missing (both in
    the loader's probe order).
  - Result for the 86 stages (default weather and wear): all referenced diffuse
    (1,861 distinct stage-texture references), shadow (56) and specular (63)
    textures resolve to an entry in some `*_textures.rbz`. 1,305 diffuse
    references are in the stage's own archive. 556 resolve through the map
    into a sibling variant's archive (for example `track-20_O` to
    `track-20_N`). No mounting step is needed: the mapped path goes through
    the same archive-transparent file layer (`CFile::OpenExe`, RBR
    `0x401ba0`, `src/rbr/RBR_N0_FileCore.cpp:53`), which rewrites the
    extension to `g_archiveExtension` (`".rbz"`, `.data` pointer
    `0x7e9f28`, used at `0x401d28`/`0x401e35`) and walks directory prefixes
    as candidate archives (`FindOrCreateFileEntry`), opening the sibling
    archive on demand.
  - The global clouds (`Clouds.ini`, `Clouds_textures.rbz`) request
    `clouds_textures\<weather>\<stem>.dds`, with no wear folder. The `skyboxes` set has one texture (`WHITE.tga`).
  - Weather and wear are both in `TextureFilenameMap.dat`, not in the loader. Over its 6,970 pairs, 92 keys
    per wear name have no weather folder (`track-4_m_textures\new\X`) and map to `dry\new\X`:
    - Ground textures: `normal` and `worn` requests map to `new` when the wear folder has no file (for example
      `dry\normal\ice_patch.dds` to `dry\new\ice_patch.dds`). `TrackTexturePlateOpaque` exists in all three wear
      folders, so it needs no mapping.
    - Non-ground textures: a `new` request maps to the plain weather folder (`dry\new\chair 01.dds` to
      `dry\chair 01.dds`).
    - Weather: some `dry` and `wet` requests map to `damp` files (for example `track-20_n`), and some stages have
      no `damp` or `wet` folder at all.
  - Install survey (`Maps/*_textures.rbz`, all 86 stages, `stage_data` test): every stage has its `dry` textures
    (no missing textures for `dry`, any wear). 51 stages load every `damp` and `wet` selection with no missing
    texture. The other 35 have no `damp` or `wet` file for 15 to 37 of their textures, for example
    `track-10_O` (only `dry` in its archive) and `track-4_E` (no `damp` and `wet` in the archive and no map entry).
    A missing file is a hard error, not a silent fallback: the diffuse path
    opens the mapped name without probing and throws `CHH_Exception("error
    loading texture")` when the `D3DX` upload fails
    (`CHH_TrackLoader::LoadTextures`, `src/hedgehog3d/HH_TrackLoaderTextures.cpp`),
    and the specular path throws `CHH_Exception("Error loading Specular map
    texture: %s")` after both its probes miss. The 35 stages' missing
    `damp`/`wet` files only bite when that weather is selected.
  - Example: `CHAIR 01.tga` gives `track-10_o_textures\dry\chair 01.dds`,
    which is in `track-10_O_textures.rbz` as `track-10_O_textures/dry/CHAIR 01.dds`.
- The `(0,23)` chunk is an embedded copy of the texture list: `u32 count`,
  then for each texture four words and a NUL-terminated name (parsed in
  `lbs/` as `TextureTable`). The words equal the stage `.ini` section of
  the same name in all 2,932 retail records: word 0 is `MipLevels`, word 1
  is `Dynamic`, word 2 is `OneBitOpacity`, word 3 is `OpacityMap` (checked
  against every stage `.ini` in the retail test). Seven `FENCE01.tga`
  records carry a `MipLevels` the ini disagrees with (`(2, 1)` six times,
  `(1, 2)` once); the loader never reads these words, so the chunk only
  takes effect through the ini. The loader does not dispatch `(0,23)`.
  It reads the `.ini` instead, so the chunk is redundant to it.

## Clipping planes (1,4) (verified: all 66 chunks consume exactly)

Parsed in `lbs/` (`ClippingPlanes`) from `FUN_100590b0`
(`HedgeHog3D`, `0x100590b0`). Each of the three arrays is prefixed by its
own count (not three counts up front): `u32 n`, `n` six-byte pairs,
`u32 m`, `m` six-byte pairs, `u32 k`, `k` float triples. 11 files have
`n > 0` (up to 12); `m` reaches 198. The handler copies all three into
heap buffers, builds vertex buffers from the counts via `FUN_10028c80`,
then frees all three buffers, so the triples never reach a stored object.
The six-byte pairs read as u16 triples (triangle-soup patterns like
`0, 1, 2, 3, 2, 1, ...`) whose maximum is below the triple count in every
retail file; the triples read as track-scale positions. Neither use is
proven (checked in the retail test as index-range and finite-float
asserts).

## Shared container

The framing is the `.trk` container. `sgc` car files were not checked here.
If they use the same framing, `trk::parse` gives owned copies, and `lbs`
frames slices itself. A shared helper is a candidate if `sgc` needs it.

## Checks run

- Chunk framing: all 88 files tile exactly (one-off script over the retail data).
- `all_retail_lbs_parse` (`rust/rbr-formats/tests/real_data.rs`): 88 files, 86 geometry chunks,
  13,195 blocks, 70,292 render chunks, 86 object chunks, 24,444 items.
  Asserts each draw lies in its view's index and vertex ranges, each index in
  `[min_index, min_index + vertex_count)`, texture/shadow/spec indices in range,
  and every object index below its vertex count. Type-2 checks: every
  `(2,10)` payload is `4 + 24*n`, every `(2,12)` is `4 + 12*n`, every
  `(2,18)` tiles in 60-byte records with the count equal to the `(2,10)`
  entry count, every `(2,11)` matrix translation equals its position, and
  every `(2,16)` second vector is unit length.
- One-off checks (not in the test, too slow in debug): render chunk bounds
  (103 chunks in track-10_O), block bounds against every vertex of the first
  60 blocks of track-10_O (109,402 vertices, all inside), object item bounds
  (24,444 items in all stages, max error 2.4e-4).
- `lbs_track_10_o_writes_obj` writes `/tmp/lbs-track-10_O.obj` (one OBJ group
  per geometry view and per object item; 544,089 vertices in track-10_O).

## Unknowns

- The vertex shader constants (0x50..0x64: four floats plus a word, always 0
  in retail) and the 0x6e..0x6f record bytes (unread, varying).
- Object item mode names (kind byte; values and decode verified, names
  unresolved) and TEXCOORD1 (36-byte items; const-vs-garbage split
  characterised, use untraced).
- Slots 1-4 shader/declaration mapping (strides loader-fixed; no retail
  geometry uses them; candidates listed above).
- Water/reflection key roles (texture vs specular map keys hidden),
  `(2,14)` vector roles beyond the value reading, `(2,16)` vector meaning
  (consumer untraced), `(1,4)` pair/triple use (buffers freed after the
  vertex-buffer build), the seven `FENCE01.tga` `MipLevels` disagreements.
- Unparsed chunk payloads: (1,9) and (2,13) (not dispatched).
