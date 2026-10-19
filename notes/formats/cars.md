# Car data

Retail 1.0 ships eight cars. Each car has a folder under `Cars/` (rendering and UI data),
physics and setup scripts in `Root/physics.rbz`, and a line in `Misc/carinfo.ini`
(`Root/misc.rbz`). This note inventories the files, records the formats we read, and lists
what is still unknown. Physics `.lsp` grammar and key tree: `notes/formats/physics-lsp.md`.
Replay damage blocks: [Damage state](#damage-state-in-replays).

Sources: `original/retail-1.0/install` (`Cars/`, `Root/physics.rbz`, `Root/misc.rbz`,
`Root/ghosts.rbz`). Sizes are bytes. Physics file contents are unpacked from the ZIP with
the `rbz` tool (`notes/formats/rbz.md`).

## Car index

`Cars/Cars.ini` section `[CarNN]` gives the index `NN`. The physics folder for that index is
`Physics/<dir>/` (exe table `0x7c18d4`, `FUN_0047c7d0` @ `0x47c7d0`, see
`notes/formats/physics-lsp.md`). The tyre set is `pirelli` for indexes 5 and 7 only, and
`misc/carinfo.ini` agrees (`Tyres 1` for cars 5 and 7).

| Index | `Cars.ini` folder | Model (`.sgc`) | Physics dir | `carinfo.ini` name | Tyres |
|---:|---|---|---|---|---|
| 0 | `XSARA` | `xsara.sgc` | `c_xsara` | Citreon Xsara | michelin |
| 1 | `ACCENT` | `accent.sgc` | `h_accent` | Hyundai Accent | michelin |
| 2 | `ZR` | `zr.sgc` | `mg_zr` | MG ZR Super 1600 | michelin |
| 3 | `LANCER_EVO_VII` | `Lancer_Evo_VII.sgc` | `m_lancer` | Mitsubishi Lancer Evo VII | michelin |
| 4 | `PEUGEOT_206` | `Peugeot_206.sgc` | `p_206` | Peugeot 206 | michelin |
| 5 | `IMPREZA03` | `impreza03.sgc` | `s_i2003` | Subaru Impreza 2003 | pirelli |
| 6 | `COROLLA` | `corolla.sgc` | `t_coroll` | Toyota Corolla | michelin |
| 7 | `IMPREZA00` | `impreza00.sgc` | `s_i2000` | Subary Impreza 2000 | pirelli |

`Cars.ini` names some paths in a different case from the folders on disk (`lancer_evo_vII.sgc`
in a `Lancer_EVO_VII` section, `Corolla.sgc`), and `ShaderSettings` has no `.ini` suffix.
Every path resolves case-insensitively (checked by `all_retail_cars_resolve_and_ini_parse`).

## Files per car

### `Cars/` (per car folder, e.g. `Cars/XSARA/`)

| File | Role | Format | Size (XSARA) |
|---|---|---|---:|
| `<car>.sgc` | Model: meshes, materials, the 3-D body. Layout in `notes/formats/sgc.md`; read by `rbr_formats::sgc`. | Binary (see below) | 1,899,408 |
| `<car>.ini` | Cameras (`[Cam_*]`), part points (`[Point_*]`), and part/face definitions for each `e_*`, `i_*`, `pc_*`, `k_*` mesh and `[Plane_*]`. Rendering and effect anchors. | Text ini | 26,776 |
| `<car>_settings.ini` | Light definitions `[Light0]`…`[Light7]`: point name (`Car Part`), colour, size, flare. | Text ini | 2,315 |
| `<car>_shaders.ini` | Material and shader set per part (`[Default Material]`, `[TSDefault]`, `FaceBlock` names). | Text ini | 17,643 |
| `<car>_shaders.ini.bak` | Stale copy. Same length as `<car>_shaders.ini` but 186 bytes differ (for example `.tga` where the live file says `.dds`). Not referenced by `Cars.ini`. | Text ini | 17,643 |
| `<car>_shader_settings_clear.ini` | Shader constants for clear weather (`[CarColor]`, `[CarPaint]`, `[CarGlass]`, `[CarTire]`, …). | Text ini | 2,710 |
| `<car>_shader_settings_overcast.ini` | Same sections, overcast weather. Identical bytes to `_clear` in all eight cars. | Text ini | 2,710 |
| `textures/*.dds` | Diffuse, scrape, dirt and dynamic textures. All 104 start with `DDS `. Counts: ACCENT 11, COROLLA 11, IMPREZA00 14, IMPREZA03 12, LANCER_EVO_VII 14, PEUGEOT_206 14, XSARA 14, ZR 14. | Binary DDS | — |

Per-car sizes (bytes):

| Car | `.sgc` | `.ini` | `_settings.ini` | `_shaders.ini` | `_shader_settings_clear.ini` |
|---|---:|---:|---:|---:|---:|
| ACCENT | 2,098,326 | 25,474 | 2,311 | 17,770 | 2,710 |
| COROLLA | 2,029,757 | 26,275 | 2,315 | 17,815 | 2,710 |
| IMPREZA00 | 2,425,587 | 26,039 | 2,315 | 17,624 | 2,710 |
| IMPREZA03 | 2,107,972 | 24,487 | 2,309 | 18,376 | 2,710 |
| LANCER_EVO_VII | 2,214,331 | 28,914 | 2,313 | 18,056 | 2,710 |
| PEUGEOT_206 | 2,214,487 | 27,266 | 2,625 | 18,154 | 2,710 |
| XSARA | 1,899,408 | 26,776 | 2,315 | 17,643 | 2,710 |
| ZR | 2,156,351 | 27,545 | 2,315 | 16,334 | 2,710 |

Folder-level files: `Cars/Cars.ini` (2,019 bytes, text ini: index of the eight cars) and
`Cars/GhostCarSettings.ini` (71 bytes, text: `[Settings]` with `Ambient`, `FadeDistance`,
`MaxOpacity` for ghost cars).

### `Root/physics.rbz` (96 file entries, 8 car folders)

The archive is plain ZIP (`notes/formats/rbz.md`). Per car, `common.lsp` holds the body, car
root, drive, control unit and hinged/mounted bodies. `gravel`, `snow` and `tarmac` hold the
running gear for each surface. `r_*.lsp` holds the setup option ranges. `setups/d_*.lsp` holds
the default setup. `repair.ini` is the damage/repair part tree (text, not `.lsp`).
`michelin/tyres.lsp` and `pirelli/tyres.lsp` are shared. `school/` is the rally school
variant. `physics.lsp`, `localphysics.lsp` and `physics.dat` sit at the top of `Physics/`.

Binary entry: `Physics/physics.dat` (1,166,896 bytes), a `CHELALIC` archive. Its layout and
contents are in [Physics archive (`physics.dat`)](#physics-archive-physicsdat). The other entries
are text: `.lsp` (ASCII, CRLF) and `repair.ini`.

`Root/misc.rbz` also holds car-related text: `Misc/carinfo.ini` (8 cars: cc, bhp, cylinders,
doors, driven wheels, gears, weight, torque, tyre flag, suspension string), `Misc/prime.lsp`
(archetype `Solid` entries) and `Misc/state.lsp` (a saved state snapshot).


## Binary and text formats

| Kind | Files | Magic / first bytes | Read by us? |
|---|---|---|---|
| Model | `Cars/*/<car>.sgc` (8) | First 23 bytes `7c 2d 7c 33 3c 7c 26 33 7c 2d 7c 28 29 26 20 35 3c 33 7c 5c 7c 33 00`: the banner `\|-\|3<\|&3\|-\|()& 5<3\|\\\|3` and NUL, then 12-byte-header records (`notes/formats/sgc.md`). | Yes: `rbr_formats::sgc`. |
| DDS texture | `Cars/*/textures/*.dds` (104) | `DDS ` | No. Rendering. |
| Archive | `Root/physics.rbz!Physics/physics.dat` (1,166,896 bytes) | `CHELALIC` (`rbr_formats::dat`) | Yes, table only. Consumer not found in the exe. |
| Replay | `Root/ghosts.rbz!Ghosts/demo1.rpl` (32,664 bytes) | Damage block starts with `HGMD` (below). | No. Notes only. |
| Ini text | `Cars/*.ini`, `Cars/Cars.ini`, `Cars/GhostCarSettings.ini` | ASCII, CRLF, `[Section]` headers | Yes: `rbr_formats::ini`, `rbr_formats::cars`. |
| LSP text | `Root/physics.rbz!Physics/**/*.lsp`, `misc.rbz!Misc/*.lsp` | `((` | Yes: `rbr_formats::lsp` (see physics-lsp.md). |
| Repair text | `Physics/*/repair.ini` (8) | `74` then `; Num parts` (brace-nested part tree) | Yes: `rbr_formats::repair`. |
| Misc text | `Misc/carinfo.ini` | `;` comment, then one value per line | No. |

## Cars.ini (`Cars/Cars.ini`)

Eight sections `[Car00]`…`[Car07]`, each with five keys: `FileName` (`.sgc`), `IniFile`
(`<car>.ini`), `ShaderFile` (`<car>_shaders.ini`), `ShaderSettings` (prefix of the two
`_shader_settings_*.ini` files), `TexturePath` (`textures\`). Values are quoted DOS paths
relative to the install root. `rbr_formats::cars::parse` returns one `CarEntry` per section
(index from the section name), and `rbr_formats::cars::resolve` maps a path onto the disk
case-insensitively.

## Car `.ini` and `_settings.ini`: survey

Survey over all eight cars (scratch scripts, not committed; the reader is
`rbr_formats::ini`, which parses every file with no duplicate keys and no lines outside a section).

**`<car>.ini`** (149 sections for IMPREZA03, 157 for ACCENT/COROLLA/IMPREZA00, 164 for XSARA, 171 for LANCER_EVO_VII, 173 for PEUGEOT_206). Sections fall into fixed families:

| Family | Count per car | Keys (value kinds) |
|---|---|---|
| `Cam_*` (`Cam_backseat`, `Cam_bonnet`, `Cam_bonnet2`, `Cam_external`, `Cam_internal`, `Cam_front_l/r`) | 5–7 | `FOV`, `Near` (float); `Pos`, `Target`, `Up` (3 floats) |
| `Point_*` (`Point_engine`, `Point_exhaust1/2`, `Point_turbo`, `Point_gearbox`, `Point_light_*`, `Point_wing_*`, `Point_antenna_NN`, `Point_dirtspray_*`, …) | 25–37 | `Normal`, `Pos` (3 floats each) |
| `e_*` external and `i_*` internal parts (`e_body`, `e_door_l`, `e_wheel_tarmac_lf`, `e_light_rf_lit`, `i_wheel_lf`, `i_steeringwheel`, …) | 87–97 and 19–26 | `FaceBlock<N>_Material`, `FaceBlock<N>_Shader` (strings), `Predicate`, `SwitchBox` (strings), `Switch` (bool) |
| `pc_e_window_*`, `k_window_f`, `k_windscreen`, `pc_e_premove` | 8–9 | as above |
| `Plane_body`, `Plane_deepshadow` | 2 | as above |
| `ghost_external`, `texture_dirtmap`, `texture_scrapemap` | 1 each | as above |

No section in `<car>.ini` carries a gameplay value (mass, gears, tyres and springs live in
`physics.rbz`). `Point_engine`, `Point_exhaust*` and `Point_turbo` are positions. Whether the
game uses them for sound or particles is not verified here.

**`<car>_settings.ini`** (8 sections `[Light0]`–`[Light7]`; Peugeot 206 has 9). Keys in every
section: `Id` (int), `Car Part` (string: a `Point_*` name from `<car>.ini`, e.g.
`Point_light_brake_lb`), `Active` (bool), `Red`, `Green`, `Blue` (int 0–255), `Texture`
(`Light.dds`), `Size` (float), `Use Lens Flare` (bool), `Lens Flare Size`,
`Lens Flare Brightness` (float; integer-valued in some files), `Lens Flare Color` (4 floats).
Some sections also have `Fade In Time`, `Fade Out Time` (floats). Key names contain spaces.

**Shader files:** `<car>_shaders.ini` has 55–60 sections (material blocks; `[Default Material]`
and `[TSDefault]` are shared). `*_shader_settings_{clear,overcast}.ini` have 13 sections, one per
shader material class (`CarColor`, `CarColorNoDirt`, `CarGlass`, `CarIntGlass`, `CarLight`,
`CarPaint`, `CarPaintInt`, `CarPlastic`, `CarRim`, `CarTexture`, `CarTextureNoDirt`, `CarTire`, `nops<N>`),
each with `Ka`, `Kd`, `Kf`, `CookTorranceM/Ks/B`, `FresnelMin/Max/Power` (floats).

## Part, material and texture chain (`<car>.ini`, `<car>_shaders.ini`)

Used by `rbr_formats::car_parts` and drawn by `rbr-render` (milestone 3). **File** checks over the eight cars
(`all_retail_car_parts_and_diffuse_textures_resolve`, `all_retail_sgc_bounds_are_centre_and_half_extent`):

1. A model mesh `N` is the `[N]` section of `<car>.ini`. Its face `i` has `FaceBlock<i>_Material` equal to the
   face's name in the model and `FaceBlock<i>_Shader` naming a shader class (`CarPaint`, `CarTire`, ...). Shader
   classes are sections of `<car>_shader_settings_clear.ini`, except `GhostCar`, which only `ghost_*` parts use.
2. A material names its bitmap in `[<material>_diffuse_bitmap]`, `Filename = x.dds`. The game's car shaders
   (`CShaderCarBase::Load`, HH 0x1007b3e0) bind it whenever that `Filename` is set, whatever the material's own
   section (`DiffuseChannel`) says or whether there is one: some RSF cars (GR Yaris Rally2's `internal::hud`,
   46 materials over 15 cars) have only the bitmap sections. In the retail cars every bitmap with a `Filename`
   belongs to a `DiffuseChannel = true` material.
   The `[TextureSets]` section gives `TextureSet0 = TSDefault`, and `[TSDefault]` maps `x.dds` to a DOS path
   under `cars\<car>\textures\`. All 1,298 diffuse face blocks of the retail cars resolve this way to a DDS file
   that parses (every texture is DXT1 or DXT3).
3. `Switch = true` parts are shown only in some states (lights `Predicate = on/off`, brake discs, reverse
   lights). Gravel and snow wheels are the same nodes as the tarmac wheel (`_gravel_`, `_snow_` in the name).
   `texture_dirtmap` and `texture_scrapemap` are overlay meshes for dirt and scrape (not base paint).
   `Plane_*` are shadow planes. `ghost_external` is the replay ghost. `pc_e_premove`, `Plane_deepshadow` and some
   `k_window_f` and `k_windscreen` parts have no `FaceBlock` keys.
4. Materials without a diffuse bitmap (`DiffuseChannel = false`, for example `external::tire`) have no
   base texture. `rbr-render` draws them white, and does not use their `Diffuse` colour (open).

Shader classes and the slots they use (paint, reflection, dirt, scrape, glass) are in `notes/formats/shaders.md`.

## Damage state in replays

`CCarDamageHandler::ReadBinary` (exe `0x4eee00`, our `src/rbr/RBR_E35_DamageC.cpp:397`) does
not read a file of its own. It reads a block inside a replay (`.rpl`) stream. The caller is
`E19_O::M567b90` (`CGameflow::ReadDamageHandler`, `0x567b90`, `src/rbr/RBR_E60_Read.cpp:9`),
which the Ghidra output lists as the only caller. Retail `Root/ghosts.rbz!Ghosts/demo1.rpl`
contains one such block, at offset 6,724.

Layout, little-endian u32 words (checked against `demo1.rpl`):

| Offset in block | Word | Read by | Meaning |
|---:|---|---|---|
| 0 | `0x444d4748` (`HGMD`) | `M567b90` | block magic |
| 4 | `3` | `M567b90` | version. Other values log `Version number differs.` and continue. |
| 8 | `0x40` | `ReadBinary` | `nNumZones`, must equal `N_ZONES` (0x40) |
| 12 | `0x40` | `ReadBinary` | second zone count, must equal the first. Otherwise log `nNumZones != N_ZONES` and return 0. |
| 16 | 16 × 16 u32 | `ReadBinary` | zone table (16 rows of 16 words, 1,024 bytes; file 6,740–7,764) |
| 1,040 | `0x43444d47` (`GMDC`) | `ReadBinary` | section marker. Found at 7,764. |
| … | 1 × `CEngine::CState::LoadSub78`, 4 × `CU32Value8::Read`, 4 × `E51WheelDamage::Read`, 4 × `CU32Value::Read` | `ReadBinary` | |
| | `GMDC` | `ReadBinary` | marker at 8,232 |
| | 4 × `E44_Read3`, 4 × `E45Smalls::M1690`, 9 × u32 (`0x3dc4`…`0x3de4`) | `ReadBinary` | |
| | `GMDC` | `ReadBinary` | marker at 8,336 |
| | 1 × `CDrive::Read3610`, 1 × `E36LightBlock::Read`, 3 × u32 | `ReadBinary` | |
| end | `0x454f4353` (`SCOE`) | `ReadBinary` | end magic. Found at 8,688. A mismatch logs `Magic number check failed.` |

After `SCOE`, `M567b90` reads more words (for example 3, 3 and a float, from 8,692), which are
outside `ReadBinary`. The three `GMDC` markers and `SCOE` sit where the code expects them, so the
layout above is confirmed for this file.

Open points: the Ghidra output lists the `nNumZones` warning as a non-returning call, but our
reconstruction returns 0 after it. The behaviour after a failed check is therefore not verified.
Searching all `.rbz` entries and the loose `.dat`, `.ini`, `.txt`, `.lua`, `.rpl`, `.gho`, `.arp` files for `HGMD` found one block, in `demo1.rpl`. The loader path that reads the
replay file itself (`RBR_E59_Gameflow.cpp:969`, `Ghosts\demo%d.rpl`) is not covered here.

## Physics archive (`physics.dat`)

`Physics/physics.dat` is a `CHELALIC` archive, the same container as the loose
`Textures/{particles,splash,fence}.dat`. Its magic is compared with `_stricmp`
(`E15_FileInner::Check`, `src/rbr/RBR_E15_File.cpp:143-185`, `0x483580`).

Layout (`rbr_formats::dat`, verified byte-for-byte against `physics.rbz` and tiled exactly on all
four archives):

| Offset | Field |
|---|---|
| 0 | `CHELALIC` magic (8 bytes) |
| 8 | u32 record count `N` (121 for `physics.dat`), counting directories |
| 12 | u32 `data_start` (3,944). Records take `data_start - 16` bytes after the header; file data starts at `data_start`. |

Records are depth-first from offset 16. Each is 20 bytes, then the name (NUL-padded, length a
multiple of 4): u32 `offset` (data position from `data_start`), u32 `size`, u32 `sibling_count`
(the group size, set on the first record of each group), u32 `child_count` (non-zero for a
directory, whose children follow), u32 `name_len`. Directories have offset and size 0.
`Build` (`E15FileOps::Build`, `0x4834a0`) uses the same fields: `+0x08` as the group count and
`+0x0c` as the recursion flag. The offset/size pair is inferred from the data (it matches the
`physics.rbz` copies), not from the loader.

The tree is rooted at `L:/Rally_7/TargetMedia_Develop/Pc/Debug/World/`, the developer's build path,
then `Physics/`. It holds 95 files and 26 directories: the same tree as `physics.rbz`'s
`Physics/` folder plus `physics.lsp` and `localphysics.lsp`. The `physics.dat` entry itself is not
in the table.

Differences from `physics.rbz`: 91 files are byte-identical. Four differ:

| File | `physics.dat` | `physics.rbz` |
|---|---|---|
| `Physics/physics.lsp` | `MaxTimeStep 0.0100` | `MaxTimeStep 0.0070` (the value `ReloadPhysicsLsp` reads) |
| `Physics/michelin/tyres.lsp` and `pirelli/tyres.lsp` | `RubMyFct_T1 0.96` | `RubMyFct_T1 0.97` |
| `Physics/p_206/common.lsp` | hinge point `-0.206 -0.223 0.166` | `-0.206 0.223 0.166` |

**Consumer.** The 1.02 exe's only open-archive calls are five direct calls to `Check`:
`0x465768`, `0x465795`, `0x4657c1` (`E09_TexBlock::CheckAll`: `Textures\particles.dat`,
`splash.dat`, `fence.dat`), `0x4db284` (`SOUND::CMaster::Vf40`, `audio.dat`) and `0x5d62e0`
(`CSeasonStageManager::LoadData`, `season.dat`). The `.dat` strings in the exe are exactly those
archives plus `trackinfo.dat`. There is no `physics.dat` string and no format that builds it.
The physics loader, `CApplication::ReloadPhysicsLsp` (`0x47fcf0`), reads `Physics/physics.lsp`
from the archive and never this file. So **no code path that opens `physics.dat` was found**. It
is either unused by 1.02 or reached by a route this search missed (for example an indirect call).
Inferred: unused. Verify in the harness before relying on either copy.

**Harness check to do:** if any path loads `physics.dat`, `MaxTimeStep` would be 0.01 instead of
0.007. That changes the physics step, so it is a gameplay difference.

## Repair hierarchy (`repair.ini`)

Loader: `E75_Svc::M5b16f0` (`0x5b16f0`, `src/rbr/RBR_E73_List.cpp`), part reader
`E73_0dc0::M0dc0` (`0x5b0dc0`, `src/rbr/RBR_E73_GS.cpp`). The file is read through the
`%srepair.ini` path built in `src/rbr/RBR_E78_DrawsB.cpp:407`.

- First token: the part count (74 in all eight cars).
- Each part is a `{` block with, in order: physics ID (int), importance (int), debug name (one
  word), unmount time, unmount min variance, unmount max variance, mount time, mount min variance,
  mount max variance (floats). Child parts follow as nested `{` blocks; the block ends with `}`.
- `;` starts a comment. The file ends with `$`.
- The loader counts parts with children and reads top-level blocks until the count is reached.
  A block is read whole, so it can take the total past the header.
- Retail: seven cars (`c_xsara`, `h_accent`, `m_lancer`, `p_206`, `s_i2000`, `s_i2003`,
  `t_coroll`) have 76 parts against the header 74. All 76 are read; the loader logs a warning
  first (its first pass compares the count with the number of `{`).
- `mg_zr/repair.ini` has **69** parts against the header 74. The loader would expect another `{`
  and hit a debug break (`__debugbreak` in our reconstruction of `M5b16f0`, `src/rbr/RBR_E73_List.cpp`). The game's
  behaviour on this file is not verified; check it in the harness. Flagged as a possible defect.

Field semantics:

- The physics ID and importance are stored; their meaning (body index, repair order) is not
  verified.
- The loader keeps the two variance values in locals (`f0`, `f1`), which are overwritten before
  use. The stored unmount and mount values are then drawn with `rand()`: the code computes
  `t + (f0-1)*f0` or `t - f0*f0`, where `f0 = 2*rand/32768`. This sampling uses the times
  and not the variance fields as written, so the variance fields look unused. Inferred from
  the decompile, unverified.

## Rust readers

- `rbr_formats::ini`: generic `.ini` reader (`parse`, `Section::get`, case-insensitive keys).
- `rbr_formats::cars`: `Cars/Cars.ini` (`parse` → `Vec<CarEntry>`) and `resolve` (case-insensitive DOS path → disk path).
- `rbr_formats::car_parts`: `<car>.ini` part sections (`face_blocks`, `is_switched`, `section`) and the diffuse
  texture of a material (`diffuse_path`), see the chain above.
- `rbr_formats::dat`: the `CHELALIC` archive record table (`parse`, `file_data`), synthetic tests and
  `all_retail_chelalic_archives_parse` (physics.dat tree, its file contents against physics.rbz, and the three Textures archives).
- `rbr_formats::repair`: `repair.ini` parser (`parse`, `node_count`), synthetic tests and
  `all_retail_repair_ini_parse`.
- Tests: synthetic tests in both modules; `all_retail_cars_resolve_and_ini_parse` (gated on
  `RBR_DATA`) resolves all eight cars and parses every `IniFile`, `ShaderFile`, shader settings
  file and each folder's `_settings.ini`.

## Unknown

- `Physics/physics.dat`: no code path that opens it was found in the 1.02 exe (see its section). Whether the game ever reads it is unverified.
- `.sgc` model: layout and open items in `notes/formats/sgc.md` (section 12).
- Meaning of `Point_engine`, `Point_exhaust*`, `Point_turbo`, `Point_gearbox` in gameplay
  (sound or particles). Not verified.
- `Misc/carinfo.ini` consumer and the meaning of its suspension string (`C0STRUTS`).
- `repair.ini`: the importance, the physics ID and the timing values are read as written. The game samples the actual times from them; that sampling is not modelled. Variance values are read into locals and do not appear to be used after the sampling (see the section).
- Damage tables vs `repair.ini` (F5): the `InitTables` functor/threshold tables (`0x4ed370`, `0x4ef910`) are immediate constants in the exe — no file input. `repair.ini` feeds the damage system one level up: `CCarRepairHandler::GetDamage` maps the part physics IDs to damage severity (queried by `CRepairPartTree::GetDamageSeverity`, severity table `{0,1,3,6,10,10}` times the part word `+0x34`), and the mount/unmount times drive the repair timing. The `repair` parser exposes every stored field (header count, physics ID, importance, name, all six timing floats, nesting, `$`/`;` handling, whole-block-past-header and short-file semantics), checked over all eight retail files.
- Damage block: behaviour after a failed magic or count check.

## Physics table

Per-car file list from `Root/physics.rbz`, copied from the archive listing (bytes):

**`Physics/c_xsara/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 5,533 |
| `gravel.lsp` | 5,962 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,328 |
| `r_snow.lsp` | 17,334 |
| `r_tarmac.lsp` | 17,380 |
| `setups/d_gravel.lsp` | 9,185 |
| `setups/d_snow.lsp` | 9,189 |
| `setups/d_tarmac.lsp` | 9,000 |
| `snow.lsp` | 5,964 |
| `tarmac.lsp` | 5,974 |

**`Physics/h_accent/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 5,525 |
| `gravel.lsp` | 5,958 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,386 |
| `r_snow.lsp` | 17,397 |
| `r_tarmac.lsp` | 17,402 |
| `setups/d_gravel.lsp` | 9,182 |
| `setups/d_snow.lsp` | 9,184 |
| `setups/d_tarmac.lsp` | 9,172 |
| `snow.lsp` | 5,967 |
| `tarmac.lsp` | 5,962 |

**`Physics/m_lancer/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 5,531 |
| `gravel.lsp` | 5,942 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,417 |
| `r_snow.lsp` | 17,427 |
| `r_tarmac.lsp` | 17,415 |
| `setups/d_gravel.lsp` | 9,187 |
| `setups/d_snow.lsp` | 9,187 |
| `setups/d_tarmac.lsp` | 9,173 |
| `snow.lsp` | 5,951 |
| `tarmac.lsp` | 5,965 |

**`Physics/mg_zr/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 6,476 |
| `gravel.lsp` | 5,938 |
| `repair.ini` | 19,438 |
| `r_gravel.lsp` | 17,356 |
| `r_snow.lsp` | 17,365 |
| `r_tarmac.lsp` | 17,366 |
| `setups/d_gravel.lsp` | 9,181 |
| `setups/d_snow.lsp` | 9,185 |
| `setups/d_tarmac.lsp` | 9,178 |
| `snow.lsp` | 6,840 |
| `tarmac.lsp` | 6,846 |

**`Physics/michelin/`** — 1 files

| File | Bytes |
|---|---:|
| `tyres.lsp` | 27,605 |

**`Physics/p_206/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 6,055 |
| `gravel.lsp` | 6,830 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,143 |
| `r_snow.lsp` | 17,147 |
| `r_tarmac.lsp` | 17,131 |
| `setups/d_gravel.lsp` | 9,163 |
| `setups/d_snow.lsp` | 9,168 |
| `setups/d_tarmac.lsp` | 9,152 |
| `snow.lsp` | 6,823 |
| `tarmac.lsp` | 6,824 |

**`Physics/pirelli/`** — 1 files

| File | Bytes |
|---|---:|
| `tyres.lsp` | 27,605 |

**`Physics/s_i2000/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 5,588 |
| `gravel.lsp` | 5,971 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,422 |
| `r_snow.lsp` | 17,425 |
| `r_tarmac.lsp` | 17,092 |
| `setups/d_gravel.lsp` | 9,016 |
| `setups/d_snow.lsp` | 9,016 |
| `setups/d_tarmac.lsp` | 9,000 |
| `snow.lsp` | 5,976 |
| `tarmac.lsp` | 6,823 |

**`Physics/s_i2003/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 5,584 |
| `gravel.lsp` | 5,971 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,422 |
| `r_snow.lsp` | 17,425 |
| `r_tarmac.lsp` | 17,092 |
| `setups/d_gravel.lsp` | 9,016 |
| `setups/d_snow.lsp` | 9,016 |
| `setups/d_tarmac.lsp` | 9,000 |
| `snow.lsp` | 5,976 |
| `tarmac.lsp` | 6,833 |

**`Physics/school/`** — 3 files

| File | Bytes |
|---|---:|
| `gravel.lsp` | 9,023 |
| `sfgravel.lsp` | 9,023 |
| `tarmac.lsp` | 9,007 |

**`Physics/t_coroll/`** — 11 files

| File | Bytes |
|---|---:|
| `common.lsp` | 5,510 |
| `gravel.lsp` | 5,961 |
| `repair.ini` | 21,371 |
| `r_gravel.lsp` | 17,415 |
| `r_snow.lsp` | 17,430 |
| `r_tarmac.lsp` | 17,446 |
| `setups/d_gravel.lsp` | 9,180 |
| `setups/d_snow.lsp` | 9,185 |
| `setups/d_tarmac.lsp` | 9,178 |
| `snow.lsp` | 5,954 |
| `tarmac.lsp` | 5,958 |

**`Physics/`** — 3 files

| File | Bytes |
|---|---:|
| `localphysics.lsp` | 909 |
| `physics.dat` | 1,166,896 |
| `physics.lsp` | 80,519 |
