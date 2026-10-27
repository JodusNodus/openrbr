# Game file formats

Parsers: `crates/rbr-formats/src/`; retail tests: `crates/rbr-formats/tests/` (`real_data.rs` unless named, need
`RBR_DATA`). Addresses: 1.02 exe; `HH` = 1.02 `HedgeHog3D.dll`. Little-endian. Detail: rbr-recomp `notes/formats/`.

## Index

| Format | Files | Module | Retail test | Parsed |
|---|---|---|---|---|
| ZIP archive | `*.rbz` (93) | `rbz` | `all_retail_archives_read_with_valid_crc` | yes |
| `CHELALIC` archive | `audio.dat`, `season.dat`, `Textures/*.dat` | `dat` | `all_retail_chelalic_archives_parse` | yes |
| physics stage | `.trk` | `trk` | `all_retail_trk_frame` | yes |
| surface grids | `.mat` | `mat` | `all_retail_mat_parse` | yes |
| fences | `.fnc` | `fnc` | `all_retail_fnc_parse` | yes |
| AnimData | `.dls` | `dls` | `all_retail_dls_parse` | fields raw |
| markers | `.dls` `Driveline` set | `markers` | `markers.rs` | yes |
| collision | `.col` | `col` | `all_retail_col_parse` | yes |
| geometry | `.lbs` | `lbs` | `all_retail_lbs_parse` | (1,9), (2,13) raw |
| texture names | stage `.ini`, `TextureFilenameMap.dat` | `stage_textures` | `all_retail_stage_textures_resolve` | yes |
| textures | `.dds` | `dds` | `all_retail_dds_parse` | yes |
| car inis | `Cars.ini`, `<car>*.ini` | `cars`, `ini`, `car_parts` | `all_retail_cars_resolve_and_ini_parse` | yes |
| car model | `.sgc` | `sgc` | `all_retail_sgc_models_parse` | yes |
| characters | `.sgs`, `.sga` | `sgs`, `sga` | `all_retail_sgs_parse` | skin raw |
| physics script | `.lsp` | `lsp`, `lsp_keys` | `all_retail_lsp_parse_to_one_list` | yes |
| repair tree | `repair.ini` | `repair` | `all_retail_repair_ini_parse` | yes |
| menu sprites | `PC_Generic_<w>.bnk` | `bnk` | unit `retail_banks_parse` | yes |
| sound | `audio.dat` inis, `.eng` | `audio`, `eng` | `audio.rs` | yes |
| FMOD bank | RSF `.bank` | `fmod` | unit `every_rsf_bank_reads` | partial |
| saves | `SavedGames/*` | `saves` | golden files (`RBR_SAVES`) | season partly raw |
| ghosts | `.gho`, `.ghc` | `ghost` | `ghost.rs` | yes |
| reference times | `Misc/times.ini` | `times` | `all_retail_times_ini_parse` | yes |
| RSF | cars, stages, RBRHUD, Pacenote plugin | `rsf` | `rsf_data.rs` (`RBR_RSF`) | see below |

## Archives

`.rbz` is plain ZIP (stored or deflate; minizip 1.00, no CRC check). `CFile::OpenExe` (0x401ba0) tries the loose path, then swaps the extension for `.rbz` and walks directory
prefixes as archive names; lookups ignore case.

`CHELALIC` (`dat`): magic, u32 record count (directories included), u32 `data_start`. Records depth-first from 16:
u32 offset (from `data_start`), u32 size, u32 sibling count (on a group's first record), u32 child count (non-zero =
directory), u32 name length, name padded to 4. The root is a developer path the game drops. 1.02 never opens
`Physics/physics.dat`, which differs from `physics.rbz`.

## Stage files

`Maps/Tracks.ini`: `[General]` extensions, `[MapNN] TrackName`. A stage is `track-NN_<L>` (time of day E, M, N, O)
with `.lbs .trk .dls .col .mat .fnc .ini`; `.bws` is never read. Loader: `CPC_TrackLoaderDaemon::vf1c` 0x46f330.
Read as bytes, not UTF-8 (the game's `GetPrivateProfileStringA`): RSF installs carry non-UTF-8 bytes in it, which
left every retail stage without particles and skid marks (`skidmarks::for_stage`, 2026-10-27).

`.trk` (`FUN_0046ed20`): chunks `u32 header_len (8), u32 type, u32 subtype, u32 size, payload`, tiling the file
(same framing as `.lbs`). Each stage has one `(3,0x14)` and one `(3,0x16)`.
- `(3,0x14)` driveline (`E09_LoadDriveLine` 0x46a530): u32 count, 32-byte points `{Vec3 position, Vec3 tangent,
  f32 distance, u16, u16}`, a Hermite spline (z-up); the u16s are unread zeros.
- `(3,0x16)` meshes (0x46e300): NUL name, u32 object kind, u8 material (`.col` surface id), u8 dynamic
  flag, u32 shape (0 none, 1 box = 2 Vec3, 2 sphere =
  Vec3 + f32), vertices, faces (u8 tag `0x00`/`0xff` spectator, three u32 of which the low byte is kept),
  placements `{u32 key, Vec3 position, Vec3 scale, quat x y z w}`. Kinds: 0 static, 1 cone, 2 block, 3 sign,
  6 banner, 7 log, 8 log pole, 9 fence bar, 10 fence pole.

`.mat` (`E09_LoadMaterials` 0x46a290): u32 count; per entry NUL name, u32 run count,
runs of 264 bytes `{u32 16, u32 16, u8 cells[256]}`. The first entry whose name contains the wear word
(`new`/`normal`/`worn`) and the wetness word (`dry`/`damp`/`wet`) wins (`mat::select`). Cells are surface ids; the
sampler (0x520010) reads only row 0: `cells[clamp(floor(16 * a))]`, `a` from the triangle's material nibbles.

`.fnc` (`CFenceHandler::Read` 0x6038c0, loose `fopen`): u32 magic 2, u32 group count; per group u32 post count,
`{fence type, post type, material, rail material}` u32, 6 f32 bounds, posts `{Vec3 position, Vec3 centre, Vec3
half extent to the next post, u32 BGRA}`. Non-empty files end with u32 count, u32 width (32) and
that many padded texture names.

`.dls` AnimData (0x479020): `MINAATAD`, version 18, minor 0, ten table offsets: 0 animation sets (17-word
records), 1 splines, 2 triggers (14 words), 3 cameras (6), 4 track emitters, 5 helicams (unused), 6 sound spheres
(Vec4), 7 name block, 8 finish zone (`0xffffffff` absent), 9 animation ids.

Stage `.ini` `[TextureInfo]`: `Texture<N>`, `ShadowTexture<N>`, `SpecularTexture<N>` with counts, and a section
per texture (`MipLevels`, `IsGroundTexture`). `TextureFilenameMap.dat`: u32 count, `request\0target\0` pairs.

## Markers

Start, splits, finish and pace notes are keys of the `.dls` animation set `Driveline` (count word 6, offset word
14): 12-byte `{u32 tag, u32 flags, f32 distance}`, sorted by distance, in metres along the `.trk` driveline. Tags:
`0x15` start, `0x16` finish, `0x17` split (at most 16; `CTimeKeeper::Load` 0x4a01f0), `0x18` end of stage, other
values pace notes. Bit 31 of the flags is set at run time when a key fires.

## Collision

`.col` (`FUN_004fe570`): header `{u32 'OC7R' 0x5237434f, u32 root offset (0x10), u32 cell count, u32 table offset}`;
the table holds absolute cell offsets.
- Root (0x28 bytes, type 4 mode 3): u16 slot, u32 flags, u32 vertex count (0), u32 vertex rel, u32 tree rel, u32
  brake-wall rel (0 = none), u32 list A count/rel, u32 list B count/rel. Lists are 0x40-byte records of four
  padded Vec3 corners: A wet puddles, B deep water.
- Cell (mode 1): u16 slot, u32 graft path (top byte depth, low bits the path, LSB first), u32 vertex count, u32
  vertex rel (0x14), u32 tree rel, Vec3 vertices.
- Node (0x20): 6 f32 bounds (min, extent), u32 flags (bits 0-20 triangle count, bit 21 graft placeholder in the
  root tree, 22-31 slot set on load), u32 pointer relative to the tree root. Inner node: count 0, pointer to a child
  pair. Leaf: `count` triangles, array padded to 4.
- Triangle (18 bytes): 3 u16 vertex indices; u16 attr A, u16 attr B (three 5-bit per-vertex values, first vertex in
  bits 0-4, bit 15 a flag); u8 material 0, u8 material 1 (`.mat` run indices); 3 u8 per-vertex nibbles per material
  (low nibble sampled, /15). Attr A interpolated /31 is the tyre blend weight between the two surfaces.
- Graft: each cell's root node overwrites the root placeholder its path reaches; later cells win.
- Brake wall: `{u32 size, u32 n, u32 points rel, u32 root rel}`, `n` (x, y) points as (inner, outer) pairs, 0x18-byte
  nodes `{centre xy, half xy, u32 items, u32 child}`; u16 leaf items: bits 0-13 point, 14 sensor only, 15 respawn.

Surface ids name `physics.lsp` `surface types` keys through the exe table at 0x7caf38 (`{name, 0, 88*id + 3, 0}`);
an id without `VALID` reads as 0 `Undefined`.

## Stage geometry

`.lbs` (HH `CHH_TrackLoader::LoadTrack` 0x10052220) uses the `.trk` framing; types 1 and 2 are dispatched. The frame
is y-up (the `.trk` z-up with y and z swapped).

| Chunk | Content |
|---|---|
| (0,23) | texture table copy (unread; the `.ini` rules) |
| (1,0) | geometry: u32 block count, f32 (unused), blocks of 9 views `{u32 n, u16 indices, u32 m, vertices}`, u32 count, 0x70-byte render chunks, centre, half |
| (1,1) | object lanes, one per block: u32 flag; if set two item lists |
| (1,2), (1,3), (1,4) | clouds, sky dome, clipping planes |
| (1,6) | interactive objects: name, u8, draws, records `{u32, 4x4}` |
| (1,7), (1,8) | reflection, water faces; flag 0x80 adds a UV-scroll pair per texture key |
| (2,10) | visibility cells: count, 24-byte `{centre, half}`; cell `i` is block `i` and lane `i` |
| (2,11) | car start: 4x4 matrix (skipped), Vec3 position, Vec3 axis-angle rotation |
| (2,12) | driveline points (unused) |
| (2,14) | animation objects: name, 3 words `{id, flags, group}`, two `.LUA` script names, 5 Vec3, colour |
| (2,15), (2,16), (2,18) | containers `{name, 3 words}`; two Vec3; 60 bytes per cell |
| (1,9), (2,13) | not dispatched |

Render chunk (0x70): 0x00 view slot, 0x04 vs id, 0x08 ps id, 0x0c start index, 0x10 prim count, 0x14 vertex count,
0x18 min index, 0x1c centre, 0x28 half, 0x35 shadow flag, 0x38 shadow texture, 0x3c specular flag, 0x40 specular
texture, 0x44 texture count, 0x48/0x4c textures 0/1 (`Texture<N>` indices), 0x50 VS-constant flag (0 in retail),
0x6c draw list (1 near, 2 always, 3 far). Level of detail by distance to the block centre: list 3 at or beyond `T`, list 1 below `T`
(and list 3 above `T - 50`); `T` = 600 (`stage::LOD_DISTANCE`).

Texture resolution: stem = last component of `Texture<N>` without `.tga`; ground textures request
`<track>_textures\dry\new\<stem>.dds`, others `<track>_textures\dry\<stem>.dds` (lower case; weather and wear
replace `dry` and `new`), then through `TextureFilenameMap.dat`. Shadow textures are
`<track>_textures\shadow_N.dat`: u32 width, u32 height, L8 texels. A missing file is a hard error in the game.

## Shaders and vertex layouts

Stage geometry slots (`ShaderManager.cpp` `g_vsFogDescs`); all vertices start with POSITION f32x3 at 0:

| Slot | VS / PS | Stride | Attributes after position | Textures |
|---|---|---|---|---|
| 0 | 9 / 0 | 0x10 | COLOR @12 | none |
| 5 | 12 `RLDoubleTexture` / 5 | 0x20 | COLOR @12, uv0 @16, uv1 @24 | t0, t1 diffuse |
| 6 | 16 `...Specular` / 6 | 0x38 | NORMAL @12, COLOR @24, uv0 @28, uv1 @36, spec uv @44, f32 @52 | + t2 specular |
| 7 | 13 `...Shadow` / 7 | 0x2c | COLOR @12, uv0 @16, uv1 @24, shadow uv @32, f32 @40 (unread) | + t2 shadow |
| 8 | 17 `...SpecularShadow` / 8 | 0x44 | as 6, shadow uv @56, f32 @64 (unread) | + t2 specular, t3 shadow |
| lane | 19 `RLSingleTextureSway` | 36 | COLOR @12, uv @16, sway `{amplitude, freq, phase}` @24 | t0 |

Slots 1-4 do not occur in retail. Pixel: `lerp(vcol * t0, vcol * t1, t1.a * vcol.a)` (vertex colour is baked light),
times the shadow map's red on 7 and 8. Samplers wrap, back faces culled (CCW),
opaque; list-1 chunks use mip LOD bias -3 (`TrackSettings.ini` `MipMapBias`). Object lanes: alpha test > 60/255, no
blend, z write; kind bits `0x01` no culling, `0x10`/`0x40` clamp/mirror U, `0x20`/`0x80` clamp/mirror V.

Car classes (from `FaceBlock<N>_Shader`; `CShaderCarBase::Load` HH 0x1007b3e0 binds the material diffuse to t0):

| Class | Shader files | t1 | t2 | t3 |
|---|---|---|---|---|
| `CarPaint` | `CarPaint1` | reflection cube | `Dirt` | `Scrape` |
| `CarPaintInt` | `CarPaintInternal` | cube | `InternalScrape` | |
| `CarPlastic` | `CarPlastic` | cube (unread) | | |
| `CarRim`, `CarLight` | `CarRim` | cube (unread) | `Gloss` | |
| `CarGlass` | `CarGlass` | cube | | |
| `CarIntGlass` | `General/CarGlassInternal` | | | |
| `CarTexture`, `CarTire` | `CarTexture` | `Dirt`/`TireDirt` | | |

`CarTextureNoDirt` and `CarColor*` use t0 only. Car vertices: position, normal, uv (`TEXCOORD0`), damage weight
(`TEXCOORD1.x`). Opacity-mapped materials blend. Car shaders are the 1.02 patch's (`RBR_PATCH` wins per file).
Projection is D3D left-handed, y up.

## Cars

`Cars/Cars.ini` `[CarNN]`: `FileName`, `IniFile`, `ShaderFile`, `ShaderSettings` (prefix of
`_shader_settings_{clear,overcast}.ini`), `TexturePath`; DOS paths, resolved ignoring case. Physics folder by
index (exe table 0x7c18d4): 0 `c_xsara`, 1 `h_accent`, 2 `mg_zr`, 3 `m_lancer`, 4 `p_206`, 5 `s_i2003`, 6
`t_coroll`, 7 `s_i2000`. Tyres are `pirelli` for 5 and 7, else `michelin`.

`<car>.ini` holds cameras (`Cam_*`), points (`Point_*`) and one section per mesh with `FaceBlock<i>_Material` (the
face's name), `FaceBlock<i>_Shader` (class), `Predicate`, `Switch`. `<car>_settings.ini` holds `[LightN]`.
`<car>_shaders.ini`: `[<mat>_diffuse_bitmap] Filename` and `[TSDefault]` mapping it to `cars\<car>\textures\`.

`repair.ini` (`E75_Svc::M5b16f0`): part count (74), then nested `{` blocks of physics id, importance, name, unmount
time/min/max, mount time/min/max; `;` comments, ends with `$`. Seven cars hold 76 parts, `mg_zr` 69.

## Car models

`.sgc` and `.sgs`/`.sga` share the container (`sg_framing`): 23-byte banner `|-|3<|&3|-|()& 5<3|\|3` + NUL, then
records `{u32 12, u32 kind, u32 subtype, u32 1, u32 length, body}` tiling the file. Car loader HH 0x1001f2a0 reads
0/1 static mesh, 0/3 damageable panel, 2/11 camera, 3/13 helper, 16/0 collision (`c_<part>`), 17/0 shadow; it skips
22/0 points (taken from `<car>.ini`), 5/* materials, 14, 15, 20. LocoMotion reads 18/0 (node table) and 6/21
(`<node>::PRS`, the rest matrix).

Node header: NUL name, i32 id, i32 parent (-1 root), 56-byte transform (translation, angle, axis, scale, angle 2,
axis 2), 3x4 world matrix `[R | t]` relative to the parent. Mesh: header, centre[3], half[3], i32 vertex
count, u32 stream flags (1 position + normal, 4 uv, 0x10 4 unused bytes), triangles (u16), faces `{name, primCount,
minVertex, numVertices, startIndex}`, a morph block on subtype 3, then weighted vertex lists.

Placement: vertices go to `F * P * v`. `F` is the chained world matrix (`sgc::chained_worlds`; the root contributes
nothing, so the car origin is the rear axle). `P` is `T * R1 * R2 * S * inverse(R2)` from the transform
(`rbr-render/src/coords.rs` `part_to_car`; same compose as the collision grids). A mirrored `F` (negative
determinant) keeps its triangles: the game draws through `m_world` with the shader's fixed `D3DCULL_CCW`, so the mirror
turns which side is culled (XSARA's right mudflaps; RSF models mirror whole parts that read right only that way:
Celica ST185 `i_body`, Lada VFTS `i_internal_analog`, Escort Mk II `i_door2_l`, Xsara WRC 2006 `i_mirrornew_*`).
Built: `i_` (outside views skip them), `e_`/`pc_`, mudflaps, wheels naming the surface; never `d_`, `k_` (but
`k_windscreen`).

## Characters

`SGS/*.sgs` and `SGA/*.sga`, loaded by stage Lua scripts (`app:LoadCharacter`), same container.
`.sgs`: 0/1 static mesh (no morph), 0/2 skinned mesh (stream flags 23; after the faces a skin block: 3x4 head, bones
`{name, 3x4}`, one 32-byte record per vertex), 1/10 light, 3/13 helper, 3/14 bone, 4/0 IK node, 18/0 hierarchy,
22/0 point. `.sga`: 6/21 animations (`(7,33)` sampled PRS, 52-byte samples; `(8,31)` XYZ; `(10,0)` scale; `(11,24)`
float; `(22,0)` empty IK) and 20/0 ranges. Two `.sgs` carry trailing bytes (`Sgs::trailing`).

## Physics scripts

`.lsp` is Lisp-like text, `(("Name" key value ("Child" ...)))`. The exe tokenizer (0x52ec80) cuts one level: an atom
runs to whitespace (a `)` stuck to it is part of it), a string to its closing `"`, a group to its closing `)`, each end
byte made NUL; a `)` between tokens ends the level (`ntweak::tokens`). It knows no comments; the
apply walk (`CBase::Parse` 0x52f0c0) skips unknown atoms, so stripping `;` comments is equivalent. A buffer not
starting `((` is XOR-decoded (key 0x7d12c8). Car load order: `physics.lsp`, `common.lsp`, `<surface>.lsp`,
`r_<surface>.lsp`, `<brand>/tyres.lsp`, `setups/d_<surface>.lsp`; later files override per key. Values are stored
raw (`atof`/`atoi`) at the member offset of the key table: [`data/physics-lsp-fields.md`](../data/physics-lsp-fields.md),
typed in `lsp_keys.rs`.

## Textures and sprites

`.dds`: standard header, no DX10. Retail is DXT1/DXT3 with 4 mips (count stored without its flag); levels are
`ceil(w/4) * ceil(h/4)` blocks, no padding. Community files add DXT5, 32/24-bit (`dds::to_rgba8`), sub-block sizes
and uncounted extra mips (ignored, as D3DX). RSF cars ship 2048x2048 `A8R8G8B8` with one level (Porsche 911 GT3 RS
RGT: 12 of 14 files); the car and stage loaders convert them and add the box-filtered chain (`dds::with_mips`).

`Generic/PC_Generic_<width>.bnk` (0x4c1100): layout in `bnk.rs`; stage pictures are sprites 0x520-0x538.

## Audio

`Root/audio.dat` (`CHELALIC`, `Audio/`): PCM16 mono `.wav`, `.ogg` (music may start with a 132-byte `INFO` block),
`.eng`, inis (`audio.rs`; a key missing from the patch's `audio.ini` reads 0). Every car plays `subaru.eng`.

`.eng` (`CEngine::Vf04` 0x4bc370): `ENGI`, version, bank count, parts per bank, parts `{PART, version, rpm, load,
unused, WAVE, version, rate, i16 samples (counted), firing positions (counted, 16.16)}`, then for 0x101 two counted
tables. Files with two waves per part do not load in 1.02.

FMOD Studio banks (RSF `AudioFMOD/<audio>.bank` + `.strings.bank`): RIFF `FEV ` objects and FSB5 sound banks
(Vorbis setup headers in `crates/rbr-sound/data/fsb-vorbis/`); layout in `fmod.rs`.

## Saves and times

`SavedGames/` (`CPC_SaveFileManager`, names by `E20MakeName` 0x49e230): `pf<name>.rbr` profile (0x26000 bytes,
encrypted), `<car>slot<n>_<name>.lsp` setups, `pf<name>.rst` records (0x4000), `pf<name>.acm` action map;
`GhostCars/<name>_<map>.ghc`. Encrypt: `b = ~((b + 0xa0) ^ key[i % 500])` (key 0x7d12c8). Word 0 is a CRC-32 with the
game's broken table `t[i] = reverse32(i)`. Streams are u32 words, tags byte-reversed. Layouts in `saves/*.rs`.

`Misc/times.ini` (`E60_5680e0`): stage count (37), then per stage its number and three `mm:ss:cc` times (split 1,
split 2, finish); `;` comments, ends with `$`.

## RSF cars

An RSF install races a chosen car in slot 5 (`rsf::cars`: the pack copied into `Physics/s_i2003/`, `[Car05]`
rewritten as `rsf::model_entry` builds it; read in place, nothing written). `.sgc` quirk: duplicate node ids
(first wins). The launcher's added gvrc.de packs (`crates/rbr-launcher/src/car_packs.rs`) unpack into `cars/` in
the player's files folder in this layout, with a `cars.json` and `carmodels.json` of ours. Files starting
`01 a0 00 00` (`.ini` `01 a1 00 00`) are RSF-encrypted and never decrypted (162 of 443 stage folders of a full
install); needing one, or a pack missing from `rsfdata/cars`, lists the car unavailable.

## RSF stages

`Maps/<id>-<name>/`: `Tracks<id>.ini`, per-variant `track-<id>_<L>.{lbs,col,trk,dls,mat,fnc,ini}` (M N E O S),
`track-<id>_<L>_textures.rbz` or a loose `_textures/` folder, `TextureFilenameMap<id>.ini`,
`TrackSettings<id>.ini`, `CustomData/<AnimationPath>/Scripts/`, `PS_*.ini`: layout, surface, weathers and texture
lookup in `rsf::stages`. Quirks: `.col` cells may share a placeholder, `.mat` may end in a `CMAP` block, `(2,11)`
matrices are zero.

## RSF ghosts

Not supported (user decision): every race uses the game's own `.ghc` ghosts, by map number.

## RBRHUD gauges

RSF cockpit dials: [cameras-hud-launcher.md](cameras-hud-launcher.md#rbrhud-gauges-rsf-cockpit-dials) (read by
`rsf::gauges`, drawn by `rbr_render::dials`).

## Pacenote plugin

Read from the plugin's files and `PaceNote.dll` (Ghidra; nothing of it is copied or run). Voices, styles, ranges
and the note file layout (`[PACENOTES]`, `[P<k>] type distance flag`): `rsf::codriver`, `rsf::pacenotes`. Luppis V3
notes start with `PACENOTES_FILE`/`RANGES_FILE` notes spelling `Rbr.ini` and set 0x100 on every note. MyPacenotes
(`Plugins\NGPCarMenu\MyPacenotes` by default): 1038 files of 525 folders parse.

## NGP plugin data

`ngp` reads the SHA-256-verified NGP 7.4.778 PE32 DLL: `.data`/`.rdata` records and constant setter arguments in
`.text`, decoded without execution. Complete stage, tyre and key tables have oracle-derived hash checks
(`tests/ngp.rs`); the 421-file NGP Lisp test resolves keys through the DLL schema. Details: [ngp.md](ngp.md).
