# Shaders, texture roles and render states

What the renderer needs to draw stages and cars with every shape and texture in place (`notes/rust-plan.md`,
Rendering). The original look is not ported; this records what the inputs mean.

Sources: the shader bytecode (`VertexShaders/`, `PixelShaders/` in the vanilla install, 61 files, `General` and
`fog` variants; the 1.02 patch replaces only the nine car shaders in `fog/`), an apitrace capture of the reference
drive on cachy (2026-10-14, frames 150-199 of the stage, 200 frames traced), and the Blender track exporter
(`notes/references/README.md`, cited, not copied).

## Shader files

All are `vs_1_1` / `ps_1_1` assembly (`.nvv` sources under `HedgeHog3D\VertexShaders\Fog\`, with shared
`ScatteringVSFragmentKanga.h` and `LinearFogVSFragmentKanga.h` fragments), built with the D3DX9 4.09.00.1126
assembler. Each file starts with a `DBUG` comment block (source paths, line map), no constant names. A few pixel
shaders are not files: HedgeHog3D assembles them from text at run time.

Vertex shader constants (from the disassembly; openRBRVR patches the same two matrices):

| Registers | Meaning |
|---|---|
| c0-c3 | world-view-projection matrix (`m4x4 oPos, v0, c0`) |
| c20-c23 | world-view matrix; `c22` gives view depth for linear fog (`c45` fog start/scale) and scattering |
| c32-c36 | sway: time and strength (`c32`), sine approximation (`c33`, `c34`), wind direction (`c36`) |
| c41, c50, c55-c66 | atmospheric scattering: sun direction (`c41`), camera position (`c50`), extinction and in-scatter terms |
| c85, c86 | UV velocity times time for the `*Anim` shaders (`oT0 = v7 + c85.x * c85.yz`) |

## Stage geometry (`.lbs` geometry blocks)

The chunk's slot is the render type; the shader pair follows from it (exporter names, matching our retail pairs):

| Slot | Render type | VS / PS | Textures (stage) | Vertex after position and colour |
|---|---|---|---|---|
| 0 | vertex colour | 9 / 0 | none | none |
| 5 | double texture | 12 `RLDoubleTexture` / 5 | diffuse 1, diffuse 2 | uv diffuse 1, uv diffuse 2 |
| 6 | double texture specular | 16 / 6 | + specular (2) | normal (before colour), + uv specular, f32 specular strength |
| 7 | double texture shadow | 13 / 7 | + shadow (2) | + uv shadow, f32 shadow strength |
| 8 | double texture specular shadow | 17 / 8 | + specular (2), shadow (3) | normal, + uv specular, specular strength, uv shadow, shadow strength |

Pixel shader of the double-texture types:

```
base  = lerp(vcol.rgb * diffuse1.rgb, vcol.rgb * diffuse2.rgb, diffuse2.a * vcol.a)
spec  = specular.rgb * specular_term * 4         (term from the VS, uses the vertex specular strength)
color = (base + spec) * shadow.rgb + inscatter   (shadow types multiply; then fog towards c4)
```

So: vertex colour rgb is baked lighting, vertex alpha weights the second diffuse texture (and is the output
alpha), the shadow texture is a multiplicative light map. Plain shaders pass UVs through unchanged; only the
`*Anim` variants move them (UV velocity, the chunk's vertex-shader constants at render chunk `0x54`, flag `0x50`
always 0 in retail). Single-texture stage shaders exist but retail stages do not use them.

Render states for every stage-geometry type: opaque, no alpha test, z test and write, back faces culled
(`D3DCULL_CCW`), samplers wrap (a few chunks clamp). Samplers 0 and 1 of draw-list 1 chunks have a mip LOD bias of -3
(see Mip LOD bias and mip levels).

## Mip LOD bias and mip levels (render-m10)

Checked against the `apitrace-check` capture (track-54_E, damp, 800x600; `~/rbr-apitrace/apitrace-check.trace` on
cachy). Draws were matched to `.lbs` render chunks by vertex buffer, start index and count in frames 175 and 250 (141
draws; `/tmp/render-m10/trace/f*-match.txt`, not kept).

- `D3DSAMP_MIPMAPLODBIAS` is `0xc0400000` (-3.0) on samplers 0 and 1 for all 74 draws of draw-list 1 chunks (render
  chunk 0x6c = 1, the near ground, `lbs.md`). It is 0 for all 67 draws of lists 2 and 3 and for every shadow and
  specular sampler. The source is `MipMapBias` in `Maps/TrackSettings.ini`: -3 in 360 of 387 sections, -2.88 in the
  other 27. `CHH_World::Unk60` passes it through `CHH_SkyHolder::Unk2faf0` to `HH_H12D_7500_FillFloat`, which stamps
  `m_mipLodBias` (`+0x1c`) into the samplers of the render set's list-1 (`+0x94`) chunks. Which section a run reads
  is not decoded, so the renderer uses -3 for all stages (`stage::MIP_MAP_BIAS`).
- Cars: `CShaderCarBase::Load` (`ShaderCar.cpp:87`) sets -3 on the diffuse sampler. The renderer biases every
  textured car draw.
- Without the bias, the ground's detail texture (texture 1, uv1 0..1 per quad) and the 256-px plate tiles blur a few
  metres from the camera. With the bias they stay sharp, as they do in the capture's snapshots.
- Mip levels: the game creates each texture with the `MipLevels` from the stage `.ini` (`CreateTexture` in the
  capture: the plate atlas 3, the 128-px `STONE_SPECIAL` 5, 256-px textures 6). The retail DDS files store 4 levels,
  and the missing ones are generated at load. The renderer uploads the DDS levels as stored (not reproduced).

## Object lanes (`.lbs` 1,1: trees, bushes, props)

Vertex (36 bytes): position, colour, uv, then sway `amplitude, angular frequency, phase` (`TEXCOORD1`). The
28-byte variant has no uv. `RLSingleTextureSway` moves the position in the vertex shader:
`pos += wind_dir * strength * amplitude * sin(time * frequency + phase)` (polynomial sine). Without sway the
shape is the rest pose.

Drawn with alpha test `alpha > 60/255`, no alpha blend, no culling, z write on (the exporter describes blending
up close and clipping at range; the capture, at the default quality, shows only the test). Item render-state
flags (exporter): `0x1` no culling, `0x10`/`0x20` clamp U/V, `0x40`/`0x80` mirror U/V; the capture's clamp
samplers match.

## Other draws in the capture

| Shaders | States |
|---|---|
| `SkyDome` | no pixel shader, no culling, z test, no z write |
| `CloudsSingleTexture` | alpha blend, alpha test > 0, no culling, no z test or write, clamp |
| `WaterSingleTexture` | alpha blend, alpha test > 0, back-face culling, z write |
| `IOSingleTexture` / `Particle` | alpha blend with test > 10 or > 1 (particles, no z write), or test > 60 opaque |
| car `CarPaint1`, `CarPaintInternal`, `CarPlastic`, `Car*NoDirt` | opaque, back-face culling |
| any car class with an opacity map (the `CarTexture` tyre, the `CarGlass` panes), `CarGlassInternal` | alpha blend, test > 0, no z write, back-face culling (internal glass: no culling); see `SetupRenderState` below |

## Cars

A face's shader class and material come from `<car>.ini` (`FaceBlock<N>_Shader`, `FaceBlock<N>_Material`, e.g.
`CarPaint`, `external::carpaint`); the material's `_diffuse_bitmap` section in `<car>_shaders.ini` names the
texture (`Filename = xsara_external.dds`), and `_opacity_bitmap` sections exist for the transparent sets.
`<car>_shader_settings_{clear,overcast}.ini` hold per-class lighting constants (`Ka`, `Kd`, Fresnel and
Cook-Torrance terms).

Texture stages, from the capture (Corolla, frame 175; every texture identified by matching its uploaded mip 0
byte for byte with the files):

| Shader | t0 | t1 | t2 | t3 |
|---|---|---|---|---|
| `CarPaint1` | material diffuse | reflection cube | dirt (`<car>_external_mud.dds`) | scrape (`<car>_external_scrape.dds`) |
| `CarPlastic` | material diffuse | reflection cube | dirt | scrape |
| `CarPaintInternal` | material diffuse | reflection cube | scrape (`<car>_internal_scrape.dds`) | |
| `CarGlass` | material diffuse (window texture) | reflection cube | | scrape |
| `CarTexture` | material diffuse (`*_transparent.dds`) | dirt | | |
| `CarTextureNoDirt`, `CarGlassInternal` | material diffuse | | | |

Which glass is blended (`rbr-render` milestone 3, `car::pass_for`): `CShaderCarGlass::IsTransparent`
(`src/hedgehog3d/ShaderCar.cpp:1051`) is true only when the material's opacity channel is on (`OpacityChannel = true`
in `<car>_shaders.ini`, as on `external transparent::carpaint`); `CShaderCarGlassInternal::IsTransparent`
(`ShaderCar.cpp:1089`) is always true. The car data names the internal shader `CarIntGlass` (registered at
`ShaderCar.cpp:440`), not `CarGlassInternal`. The render states above are from the capture, not from the source.

The reflection cube is a 128-pixel `X8R8G8B8` render target the game draws each frame. `CarPaint1`'s pixel
shader: diffuse lerped towards the dirt colour (`c3`) by the dirt alpha, times the vertex lighting, plus the
reflection scaled by the specular term and reduced where dirt covers it, then lerped to the scrape texture by
the per-vertex damage (`TEXCOORD1.x`, passed as vertex alpha) times the scrape alpha. For shapes and textures
only, t0 with the vertex lighting is enough; dirt, scrapes and reflections are later look work.

Stage textures in the same capture: the shadow texture (t3 of the shadow types) is a 2048x2048 `L8` texture,
specular masks are 512x512 DXT3.

## Shadow light map (slots 7 and 8, milestone 3, slot 8 in milestone 5)

- Render chunks with the shadow flag (`0x35`) on view 7 draw the light map: track-4_E 53 chunks, track-20_N 305,
  track-22_N 313, track-34_E 254. Their `shadow_texture` (`0x38`) indexes `ShadowTexture<N>` in the stage `.ini`
  (`NumShadowTextures=1` in track-4_E and track-22_N, two in track-34_E).
- Shadow uv: texcoord2 at vertex offset 32 on slot 7, next to texcoord0 at 16 and texcoord1 at 24. Pixel shader 7
  (`RLDoubleTextureShadow`) reads it as TEXCOORD2 (verified). The table's "f32 shadow strength" is not read; the
  light map applies at full strength (not verified).
- Chunks with the flag on view 8 (track-20_N 110, track-22_N 2) use the same light map. Their shadow uv is TEXCOORD4 at
  vertex offset 56 (`RLDoubleTextureSpecularShadow.vso`, `dcl_texcoord4 v13`, `mov oT3.xy, v13`). Milestone 5 draws them;
  the specular term that the same shader adds before the multiply is not drawn (see Material roles).
- Texture: `<track>_textures\shadow_N.dat` in the archive. Eight-byte header (little-endian u32 width, u32 height),
  then `width * height` `L8` texels. Retail sizes are 2048 and 1024.
- Pass: the fragment shader multiplies the colour by the light map's red channel after the texture blend.
  A missing light map is white (no shadow), not the magenta placeholder.
- Effect check: track-34_E dry has mean red 38.8 with the multiply off and 35.2 with it on. There is no reference
  frame to compare against.

## Tools

- `scripts/d3d9dis.py`: disassembles the `.vso`/`.pso` files (shader model 1.x).
- `harness/apitrace.sh <name> <dir>`: a playback run with apitrace's `d3d9.dll` in front of DXVK; the trace and
  its text dump stay in `~/rbr-apitrace` on cachy, the dump is copied to `<dir>`. About 27 s.
- `scripts/d3d9states.py <dump> <install shader dir> <patch shader dir> <first> <last>`: render, sampler and
  texture states per shader pair over a frame range. For texture identity, dump the mip 0 `memcpy` blobs
  (`apitrace dump --blobs --calls=N,...`) and compare them with the DDS files after their 128-byte header.

- Ground truth from a capture: `d3dretrace.exe` (apitrace 14.0 win32, `~/rbr-apitrace/apitrace-14.0-win32/bin` on
  cachy) under Proton via `umu-run`, in a scratch `WINEPREFIX` (never the game prefix):
  `d3dretrace.exe --snapshot-prefix=Z:\tmp\x\snap- --snapshot=<call>,... Z:\home\thomas\rbr-apitrace\<name>.trace`
  writes the frame at each `Present` call. `--snapshot=<first>-<last>/draw` writes one image per draw, which shows
  what a single draw puts on screen. The camera of a snapshot is the eye and forward axis of the view matrix
  uploaded before it. `viewer <stage> --camera ex,ey,ez,fx,fy,fz --size 800x600 --screenshot <png>` draws the same
  view.

## Camera

Projection is D3D left-handed (`_34 = 1`): near 0.15, far about 10,000, 4:3, vertical field of view about
56 degrees in the chase view. The exporter reads positions as left-handed with y up (it swaps y and z for
Blender).

## Texture coordinates in the vertex shaders (checked for milestone 2)

Every `.vso` in `VertexShaders/` and `VertexShaders/fog/` (48 files, `scripts/d3d9dis.py`) was checked for what
it writes to each `oT<n>`. Stage geometry and object shaders copy the input texcoords unchanged (`mov oT0.xy, vK.xyyy`),
with the input registers the shader declares (`dcl_texcoord*`):

| Shaders (checked) | oT0 | oT1 | oT2 | oT3 |
|---|---|---|---|---|
| `RLSingleTexture`, `RLSingleTextureSway`, `SkyDome`, `Particle`, `IOSingleTexture`, `CloudsSingleTexture` | v7 | - | - | - |
| `RLDoubleTexture`, `CloudsDoubleTexture`, `IODoubleTexture`, `RLDoubleTextureSway` | v7 | v8 | - | - |
| `RLSingleTextureShadow` | v7 | v8 (shadow uv) | - | - |
| `RLDoubleTextureShadow` | v7 | v8 | v9 (shadow uv) | - |
| `RLDoubleTextureSpecular` | v9 | v10 | v11 (specular uv) | `r0.z * v13.x` (specular term, not a uv) |
| `RLDoubleTextureSpecularShadow` | v9 | v10 | v11 (specular uv) | v13 (shadow uv, TEXCOORD4) |
| `RLSingleTextureSpecularShadow` | v9 | v10 | v12 (shadow uv, `texcoord3`) | - |

The declared offsets match `lbs.md`: slot 5 `TEXCOORD0` @16 and `TEXCOORD1` @24; slot 7 `TEXCOORD0` @16, `TEXCOORD1`
@24, `TEXCOORD2` @32; slots 6 and 8 `TEXCOORD0` @28, `TEXCOORD1` @36, `TEXCOORD2` @44.

Only the `*Anim` variants and the water shaders move the texcoords: `oT = c85 * time + v` (`mad oT0.xy, c85.x,
c85.yzzz, v7.xyyy`, and c86/c87 for the other sets). The car shaders write a reflection coordinate into `oT1` from the
normal (`mul oT1, r0, c80.z`); car work is M3.

Retail stage geometry never selects an `*Anim` variant: the vertex-shader-constant flag (render chunk 0x50) is 0 in all
70,292 chunks (`all_retail_lbs_parse` asserts it), and the constants that flag would replace are never used. Object
lanes use `RLSingleTextureSway` (flags 0x15), which moves the position only (`pos += ...sin(...)`), not the texcoords.
Milestone 2 therefore draws the texcoords as stored.

## Object lane render states (decoded from the kind byte)

`kind` is the flag byte of an object item (`HH_TM2_0D10_FlagDecode`, `src/hedgehog3d/HH_TextureManagerPart2.cpp:5003`):

| Bit | Meaning | Source |
|---|---|---|
| 0x01 | no culling (`st[15] = 1` in the factory) | exporter; every retail kind byte has it set (49, 13, 37, 25), so all lanes are double-sided |
| 0x10 / 0x40 | U address clamp / mirror | `FlagDecode` slot 0 modes 3 / 2; the capture's clamp samplers match (0x10) |
| 0x20 / 0x80 | V address clamp / mirror | `FlagDecode` slot 1 modes 3 / 2 |

Slot modes 1, 2 and 3 are the D3D9 `D3DTADDRESS_WRAP`, `MIRROR` and `CLAMP` values (inferred: the source does not call
`SetSamplerState`, and the mapping to the sampler is through the state table `HH_ApplyState_5A060`, not read). A mode
of 1 is wrap. Alpha test `alpha > 60/255`, no blend, z write on (object lanes, from the capture; see above).

Open: the blend and decal modes (the exporter's "decal" lanes), and whether the kind byte's other bits select the
pixel shader. Not decoded from the source.

## Clouds and sky items

The `(1,2)` clouds use the sky dome item layout but without the 12-byte position before each item: `HH22_FillSkyItem`
skips the position only for kind 0 (the sky dome, `(1,3)`). Clouds parse as `lbs::Chunk::Clouds`.

- `Maps/Clouds.lbs`: 12 cloud faces, one item each (flags 0x15, one texture). The face names are
  `MAPS/GLOBAL/SKYBOXES NEW TECH/<NAME>.KSH` with `NAME` one of `US_CLEAR`, `US_CLEAR02`, `US_CLOUDED`,
  `US_CLOUDED02`, `US_OVERCAST`, `US_OVERCAST02`, `US_PARTLY_CLOUDED`, `US_PARTLY_CLOUDED02`, `US_PRECIPITATION`,
  `US_PRECIPITATION02`, `US_PRECIPITATION_PC`, `US_PRECIPITATION02_PC`. Their textures are in `Clouds.ini` (7 names),
  and `Clouds_textures.rbz` holds them under `dry`, `damp` and `wet` (no wear folder).
- `Maps/skyboxes.lbs`: 5 chunks named `MAPS/GLOBAL/SKYBOXES PS2/<TIME>_CLEAR[_AR].KSH` (`EVENING`, `MORNING`, `NOON`).
  Each has a dome item (flags 0x05, kind 0, no texture, 193 vertices, about 89 m out). `EVENING_CLEAR` also has one
  textured item (flags 0x15, texture `WHITE.tga` in `skyboxes.ini`), not drawn.
- Which face shows: `Maps/TrackSettings.ini` has 405 sections named `NN[MNEO]_[E_]preset_cloud` (for example
  `[04E_crisp_partcloud]`). The exe builds the section name with `"%u%s_%s%s%s_%s"` (`RichardBurnsRally_SSE.exe.c`
  190018) from the track number, country letter, a preset from the weather state, and a cloud part. Its `CloudName`
  (for example `Us_clouded02`) names the face; `LoadSkySettings` copies it to `m_settings + 0x18`, over
  `SunOffset`, `Superbowl_Scale` and `Cloud_Scale`. The lookup of the face by that name is not traced. The renderer
  draws the face the section names; the section is its own pick per weather (`rbr-render` `atmosphere.rs`), and
  without a section it falls back to a guess from the names (`US_CLEAR` dry, `US_CLOUDED` and `US_PARTLY_CLOUDED`
  damp, `US_OVERCAST` and `US_PRECIPITATION` wet).
- Fog on the clouds and the superbowl: under `UseFog` both `CHH_KClouds::Render` and the superbowl render
  (`HH_KObj_Unk2f0c0`) set each item's fog state on with the fog colour. `CloudsSingleTexture.vso` writes
  `oFog = c49.w`, and `Render` loads `c49` with the fog colour and `SkyboxSaturation` (`m_settings + 0x8c`), so the
  clouds keep `SkyboxSaturation` of their colour (mostly 0 to 0.16 in the rain and snow sections). `RLSingleTexture.vso`
  writes `oFog = min((c45.y - z) * c45.z, 1)` with `z` the view depth; for the superbowl the fade block is
  `SuperbowlFogStart`, `SuperbowlFogEnd` and their reciprocal range (which register file it reaches is approximate in
  the reconstruction).
- The dome is not drawn. `fog/SkyDome.vso` declares no colour (`dcl_position` only) and computes the sky colour from the
  scattering constants (`c41` sun direction, `c50` camera, `c55`-`c66`). The stored vertex colour is cyan in every
  retail dome and is never read. The scattering is not reproduced; the clear colour stands in for the sky.
- Draw order (capture, frames 175 and 250): `SkyDome`, then `CloudsSingleTexture` (one face: `US_CLEAR02` in the damp
  run; alpha blend, no z test or write, clamp, point min and mag filter, linear mip), then the superbowl
  `RLSingleTexture` items, then the stage geometry. The clouds are part of the background, and the ground draws over
  them. The renderer draws them in the sky layer before the stage. Drawing them last, over the geometry, washed out
  the ground (render-m10).
- The clouds' render transform is not reproduced: the faces sit at their stored positions relative to the camera, which
  is wrong (they show as a ceiling at camera height). `CHH_KClouds::Render` (`src/hedgehog3d/HH_KClouds.cpp:114`) is
  marked approximate in the source, with unresolved D3DX callees. Decoded from the asm: `RotationX(pi/2)` and the
  inverse view; constants uploaded to `c0`, `c8`, `c45`, `c49`, `c66`. Not decoded: how they compose into the final
  matrix, so no transform is applied.
- Sky dome scattering (`SkyDome.vso`): the slots and formulas are known (`c41` sun direction from the car-shader sun
  vector, `c50` camera from the `m_unk228` tint, `c55`-`c66` scattering). Not resolved: the rig angles
  (`g_pHH->m_unk14`), the sun vector source, and `c50`. The dome stays undrawn until those inputs are proven.

## Material roles (milestone 5)

Texture slots and vertex attributes, from the shader files and the code that binds them. Addresses are the
`FUNCTION: HEDGEHOG3D` annotations in `src/hedgehog3d/ShaderCar.cpp` and `ShaderManager.cpp`, `HH_GeomCar.cpp`,
`HH_CarShaderManager.cpp`.

### Which car shader files load

The car shaders are the 1.02 patch files, `original/patch-1.02/files/Main_group/{PixelShaders,VertexShaders}/Fog/Car*`,
which are the reference. The retail-1.0 install (`RBR_DATA`) has different copies of the files the game loads:

- `CarGlass.pso`: 1.02 adds the diffuse term (`add r1.xyz, r1, v1` then `add r0.xyz, r1, r0`); retail overwrites it.
- `CarPaint1.vso`, `CarPaintInternal.vso`: 1.02 has `mul oD1.w, r9.w, c79.y` (Fresnel times Kf); retail has it for
  paint only, with `mov` for the internal one.
- `CarTexture.vso`: 1.02 adds a hemisphere ambient, `r2 = c76.x * n.y + c76.x`, then `r0 = c93 * r2`; retail uses `c93` alone.
- `CarPaint1.pso`: `mov_sat r0.w, c4.w` (c4.w is 1, no effect).
- `CarColorNoDirt.pso`: retail ends with `mov r0, c0` (a red output); 1.02 does not. No class loads it (below).

The stage shaders are the same in both installs. The target is 1.02: the patch replaces 18 car shader files (the
nine `Fog/` pixel and nine `Fog/` vertex shaders above) and the executables; everything else is the retail install.

Data lookup: `RBR_DATA` is the retail install, and `RBR_PATCH` (optional) is the patch's `files/Main_group`. A file the
patch has wins over the install's, per file, matched ignoring case (`rbr_formats::data::Data`). The renderer reads the
car's shader pair for each drawn class through it (`car::SHADER_FILES`, `CarReport::shader_files`), checks the `vs_1_1` or
`ps_1_1` version token, and the viewer prints the files it used. `CarIntGlass` is in `General`, which the patch does not
replace, so it comes from the install. `tests/car_data.rs` checks that, with `RBR_PATCH` set, the Fog files are the patch's
bytes; without it, the install's.

Which file each class loads. `ShaderManager.cpp` has id tables (`g_vsFogDescs` at 0x101d8340, `g_psFogDescs`, and the
`General` tables). Each class asks for its ids in `RefreshShaders`. `CarColor` and `CarColorNoDirt` ask for VS 0x2e and PS
0x11, which the tables map to `CarTextureNoDirt`, so the `CarColor*` files are never loaded. `CarTire` shares `CarTexture`'s
shaders, `CarLight` shares `CarRim`'s, and `CarIntGlass` loads `General/CarGlassInternal`.

### Car classes: shaders, texture stages and blend

`t0`-`t3` are the stages the class's `Load` binds (`CShaderCarBase::Load`, 0x1007b3e0, binds the material diffuse to stage 0).
"env" is `m_envMap`, or the fallback `m_unkC0` (`Load` at the address shown). "unread" means the pixel shader does not
sample the stage. Friendly names are `GetFriendlyTexture`, a texture-manager lookup; their files are not traced (below).

| Class | VS (id), PS (id) | Load | t0 | t1 | t2 | t3 | Blend (`IsTransparent`) |
|---|---|---|---|---|---|---|---|
| `CarColor` | `CarTextureNoDirt` (0x2e), `CarTextureNoDirt` (0x11) | 0x1007c230 | diffuse | - | - | - | opaque (0x1007c2d0) |
| `CarColorNoDirt` | `CarTextureNoDirt` (0x2e), `CarTextureNoDirt` (0x11) | 0x1007c4c0 | diffuse | - | - | - | opaque (0x1007c560) |
| `CarTextureNoDirt` | `CarTextureNoDirt` (0x2e), (0x11) | 0x1007e690 | diffuse | - | - | - | opaque (0x1007e730); with an opacity map drawn blended in the opaque list (below) |
| `CarTexture` | `CarTexture.vso` (0x2d), `CarTexture.pso` (0x10) | 0x1007e300 | diffuse | `Dirt`, used (mask `t1.a`) | - | - | opaque (0x1007e4a0) |
| `CarTire` | `CarTexture` (0x2d), `CarTexture` (0x10) | 0x1007e920 | diffuse | `TireDirt`, used | - | - | opaque (0x1007eac0) |
| `CarPaint` | `CarPaint1.vso` (0x2a), `CarPaint1.pso` (0x0d) | 0x1007d1f0 | diffuse | env, used | `Dirt`, used | `Scrape`, used | opaque (0x1007d490) |
| `CarPaintInt` | `CarPaintInternal.vso` (0x2b), `.pso` (0x0e) | 0x1007d680 | diffuse | env, used | `InternalScrape`, used | - | opaque (0x1007d8c0) |
| `CarPlastic` | `CarPlastic.vso` (0x32), `.pso` (0x15) | 0x1007dab0 | diffuse | env, unread | `Dirt`, unread | `Scrape`, unread | `1 > ambient.a` or opacity map (0x1007dcf0); its `Load` (0x1007dab0) then sets z write on and culling off |
| `CarRim` | `CarRim.vso` (0x31), `CarRim.pso` (0x14) | 0x1007df00 | diffuse | env, unread | `Gloss`, used | `Scrape`, unread | same test (0x1007e0f0); its `Load` sets no state, so `SetupRenderState`'s blend applies |
| `CarLight` | as `CarRim` (0x31, 0x14) | 0x1007cdf0 | as `CarRim` | as `CarRim` | as `CarRim` | as `CarRim` | same test (0x1007cfe0); lights are `Switch = true` |
| `CarGlass` | `CarGlass.vso` (0x2f), `CarGlass.pso` (0x12) | 0x1007c750 | window diffuse | env, used (`t1 * t1`) | `Gloss`, unread | `Scrape`, unread | `1 > ambient.a` or opacity map (0x1007c940) |
| `CarIntGlass` (`CarGlassInternal`) | `General/CarGlassInternal.vso` (0x30), `.pso` (0x13) | 0x1007cb50 | diffuse | - | - | - | always (0x1007cc00); `m_unk3c = 1` |

Retail materials: `ambient.a` is `1 - Transparency` and no retail car sets `Transparency` above 0, so `IsTransparent` is the
opacity channel. Opacity-mapped faces by class (a scan of the car part and material files, approximate): `CarGlass` 79,
`CarIntGlass` 18, `CarPlastic` 35, `CarRim` 3. `CarTextureNoDirt`, `CarTexture` and `CarTire` return false, so their opacity
maps do not put them in the transparent list; they are still blended (below).

Render state (render-m11, asm). Every car class's `Load` runs `CShaderCarBase::Load` (0x1007b3e0), which calls
`SShaderMaterial::SetupRenderState` (0x1003f4b0) on the material. When the material's diffuse alpha is below 1, its
`Transparency` is above 0 or its opacity map is on (`+0x48`), that sets z write off, alpha test on with `GREATER` (5) and
reference 0, and alpha blend on with `SRCALPHA`/`INVSRCALPHA`, whatever `IsTransparent` says. Culling stays the default
(`CCW`); `Requirements & 1` would turn it off, and no retail rim or tyre material sets it. `CarPlastic`'s `Load`, when
transparent, then sets z write on, alpha test `GREATER` and culling off; the blend stays on. State offsets
(`NLB_SyncRenderStateEH`): `+0x14`/`+0x18` source and destination blend, `+0x2c` alpha blend, `+0x30` z write, `+0x34` z
enable, `+0x3c` cull mode, `+0x94` alpha test, `+0x98` alpha function, `+0x9c` alpha reference. **Capture** (frame 175,
`scripts/d3d9states.py`): the `CarTexture` draw of `internal transparent::tire` has alpha blend on, `GREATER 0`, `CCW`,
z write off; `CarGlass` with an opacity map the same; `CarGlass` without one is opaque.

Draw order (`FUN_1001e430`): `CarRim` and `CarTire` shaders go to the wheel list (`+0xf4`), which the world render
(0x1006ada0) draws before `CHH_Car::Render(1)`; the others go to the transparent list (`+0xc4`) when `IsTransparent` is
true and to the opaque list (`+0xb4`) otherwise. A list is drawn by shader, and the shaders are created once per (class,
material) in the order the parts first use them. So COROLLA's `pc_e_window_*` panes (`CarTextureNoDirt`, `alpha = t0.a`,
blended) draw in the opaque list after `e_bonnetvent` and before `e_sparewheel_tarmac`, and the glass after them.

The back buffer is `D3DFMT_X8R8G8B8` (the capture's `CreateDevice`), so the drawn alpha is never shown. The number plate
texels of `corolla_external.dds` have alpha 0: a screenshot written with that alpha looks black or white over them in an
image viewer. `rbr-render`'s viewer writes RGB.

### Car pixel constants and vertex constants

Vertex constants are set in `CShaderCarBase::SetShaderConstants` (0x1007b930) and `SetObjectWorker` (0x1007bc60). Pixel
constants are set in `SetObjectWorker` and in each class's `Begin`.

| Register | Value and source | Used by |
|---|---|---|
| VS c0-c3 | world-view-projection, `SetObjectWorker` | all |
| VS c8-c11 | world, `SetObjectWorker` | position for the view vector |
| VS c12-c14 | inverse transpose of world, `SetObjectWorker` | normals (`m3x3 r7, v3, c12`) |
| VS c41 | sun direction (`r9 = -c41`, `n . -c41` is the diffuse term); not uploaded in `ShaderCar.cpp`, source not traced | lighting, specular half vector |
| VS c50 | camera position (`c50 - pos`); not uploaded in `ShaderCar.cpp` | view vector, Fresnel |
| VS c67, c68 | `mov oD1, c67`, `mul r0, r0, c68`: the two light colours. `SetObjectWorker` uploads pixel 0x43 and 0x44 (`m_lightA`, `m_lightB`); no VS upload found | lighting (open) |
| VS c76 | `(0.5, 1, 4, 1)`, `SetShaderConstants` (upload 0x4c) | hemisphere (`c76.x`), Fresnel and specular (`c76.y = 1`) |
| VS c77, c78 | `1/CookM^2` (from `CookTorranceM`), `CookTorranceB` | specular terms of the paint, glass, plastic and rim VS (`mul ... c77.x`, `mul ... c78.x`) |
| VS c79 | `(CookKs * ratio^2, Kf)`; `c79.y` = Kf | `oD1.w = F * Kf` |
| VS c80 | `(Ka * max(ratio, 0.7) * scale, Kd * div * scale * ratio, -1, 0)` | lighting; `oT1` reflection sign (`c80.z`) |
| VS c90 | `g_pHH->m_unk14->m_unk20` (not a settings value; source open) | stage and car specular terms |
| VS c91 | `(F0, 1 - F0, 0, 1)` with `F0 = 2^-0.5 / 2^2.5 = 0.125`: Schlick Fresnel | `F = F0 + (1 - F0)(1 - n.v)^5` |
| VS c92 | `(FresnelMin, FresnelMax, FresnelPower)` from the class settings | clamp of F (`c92.xy`); `c92.z` is the `lit` exponent in `CarPaint` and `CarPaintInt` only |
| VS c93 | `(0.5, 0.5, 0.5, 1)` ambient, `SetShaderConstants` | ambient |
| PS c3 | `(m_unkC0.xyz, m_unkBC)`, uploaded in `Begin` of `CarPaint` (0x1007d380), `CarPaintInt` (0x1007d7b0, unread by its shader), `CarTexture` (0x1007e3c0) and `CarTire` (0x1007e9e0). `w` is the dirt level | dirt colour and mask |
| PS c4 | `(fade.xyz, 1)`: `fade` is settings `m_unk98` when fog is on, else 0; `w = 1` | the final `lrp` with c4 is the identity (`w = 1`) |

The dirt level `m_unkBC` is zero when the manager is built (0x10001230) and on `Reset` (0x100013d0). It rises by
`m_unk88 * 1e-5` for each tyre-particle signal (`OnTireParticles`, 0x10001420), capped at 0.9. The ring average `m_unkB4`
(`AddDirtSample`, 0x10001510) is the `ratio` in the constants above. Whether the level resets between stages is not traced.

### Car vertex attributes

| Stream and attribute | Source | Read by |
|---|---|---|
| position `v0`, normal `v3` (stream 0) | `sgc` vertices (`sgc.md`) | all car VS |
| colour (stream 3) | `SetStreamSource(1, 3)`: the always-white aux stream (`sgc.md`); no `dcl_color` in the car VS | not read |
| `TEXCOORD0` `v7` (uv of the single UV set) | `sgc` UVs | `oT0` (diffuse); `oT1` in `CarTexture`, `CarTire`, `CarTextureNoDirt`; `oT2`, `oT3` in `CarPaint` and `CarPaintInt` (dirt, scrape, internal scrape) |
| `TEXCOORD1.x` `v8.x` (stream 2 = stream 7) | damage weights: `CGeomObjectCar::UpdateDeformation` (0x10003b80) writes `w * 0.01` per vertex | `CarPaint` and `CarPaintInt`: `oD0.w`, which is the scrape amount |
| `oT1` reflection | `r0 = reflect(view, n) * c80.z` (`mul oT1, r0, c80.z`) | `CarPaint`, `CarPaintInt`, `CarPlastic`, `CarRim`, `CarGlass` (env) |

`CarPaint`'s `oD0.w` is the damage weight and `oD1.w = F * Kf` (1.02). Paint shader: diffuse `lerp(t0, c3.xyz, dirt)` with
`dirt = t2.a * c3.w`; reflection `t1 * v1.w + v1` weighted by `(1 - t2.z) * t0.a`; scrape `lerp(paint, t3.rgb * v0, t3.a * v0.w)`
with the damage weight in `v0.w`. `CarTexture` and `CarTire` use `lerp(t0, c3.xyz, t1.a)`: the mask is not scaled by `c3.w`.

### Stage slots 6-8: texture and vertex roles

| Slot, shader | Vertex (offset) and its register | Pixel shader use |
|---|---|---|
| 5 `RLDoubleTexture` | TEXCOORD0 @16 (`v7`), TEXCOORD1 @24 (`v8`) | t0, t1 (existing) |
| 6 `RLDoubleTextureSpecular` | TEXCOORD0 @28 (`v9`), TEXCOORD1 @36 (`v10`), TEXCOORD2 @44 (`v11`, spec uv), TEXCOORD3 float @52 (`v13.x`) | t0, t1 base; `oT2` spec map `t2`; `oT3 = S * v13.x` (broadcast) times `t2`, times 4 |
| 7 `RLDoubleTextureShadow` | TEXCOORD0 @16, TEXCOORD1 @24, TEXCOORD2 @32 (shadow uv), TEXCOORD3 float @40 (`v10.x`) | shadow `t2`; the float goes to `oD1.w`, which the PS does not read (strength unused) |
| 8 `RLDoubleTextureSpecularShadow` | TEXCOORD0 @28, TEXCOORD1 @36, TEXCOORD2 @44 (spec uv), TEXCOORD3 float @52 (`v12.x`), TEXCOORD4 @56 (`v13`, shadow uv), TEXCOORD5 float @64 (`v14`, declared, never read) | spec `t2`, weighted by `v1.w = S * v12.x`; shadow `t3` from `oT3 = v13` |

Pixel formulas (1.02 files are the stage's own; no patch change). Base: `B = lerp(v0 * t0, v0 * t1, t1.a * v0.a)`.

- Slot 6: `sat(sat(B + 4 * sat(t2.rgb * S * v13.x)) + v1)`, then fog to `c4` (identity).
- Slot 7: `sat(sat(B) * t2 + v1)`, then fog to `c4` (identity). (The PS adds `v1` after the multiply.)
- Slot 8: `sat(sat(B + 4 * sat(t2.rgb * S * v12.x)) * t3 + v1)`, then fog.

`v1` is the in-scatter from `c55`-`c66` (not drawn, see Clouds and sky). `S` is the specular term the VS computes from the stored
normal `v2` (not normalised) and `h = normalize(c41 + normalize(c50 - pos))`:
`S = c90.y * 2^(c90.x * (1 - 1 / (n.h)^2)) / (n.h)^4`. Its inputs `c41`, `c50` and `c90` are not proven for stages (below), so
the renderer does not draw specular.

### Open (not drawn, not proven)

- Reflection cube (car `t1`, slot-6/8 specular). The game draws a 128-pixel `X8R8G8B8` target each frame (see the car notes
  above). Choose a stand-in or the render target in a later milestone; the car's look is not a goal (`rust-plan.md`).
- Car lighting: `c41` (sun), `c67` and `c68` (VS light colours; no VS upload found), and `c90` have no proven source, so the car
  lighting of the game is not reproduced. It would be `c93 * ka * ... + kd * max(-n.c41, 0)` times the light colours, not verified.
  `rbr-render` lights the car with its own model from the class settings and `TrackSettings.ini` (render-m12, its README).
- Dirt colour `c3.xyz` is `m_unkC0`. Only `SetC0` writes it, and `SetC0` has no callers (0x10001830). The preset in
  `m_dirtColor` (0.27, 0.22, 0.14, at 0x10001230) never reaches a shader. Until this is traced, `CarTexture` and `CarTire`
  blend to an unknown colour by their mask.
- Friendly names (`Dirt`, `TireDirt`, `Scrape`, `InternalScrape`, `Gloss`) to files: not traced. The capture matched
  `<car>_external_mud.dds` to `Dirt` (`CarPaint1`), `<car>_external_scrape.dds` to `Scrape` and `<car>_internal_scrape.dds` to
  `InternalScrape`. Those are the files the materials name (`dirtmap::dirtmap_diffuse_bitmap`, `scrapemap`), but the material
  sections are only used by the overlay parts.
- Overlay parts `texture_dirtmap` and `texture_scrapemap` have `Predicate = NotUsed` and `Switch = false` in every car. They are
  not drawn. Whether the loader ignores `Predicate` is not traced.
- Dirt and scrape at the stage start: zero (the dirt level is zero when the manager is built, and the damage weights are zero
  without damage). Drawn with the base texture only, which is the start state.
- Glass alpha in 1.02: `CarGlass` outputs `alpha = t0.a + 0.5 * v1.w`, with `v1.w = F * Kf` and `F` the Schlick term clamped to
  `[FresnelMin, FresnelMax]` from the view angle. Needs per-vertex normals and the camera in the shader. Not drawn.
- `CarPlastic` and `CarRim` with opacity: drawn from the asm (`SetupRenderState`, above). No `CarPlastic` or `CarRim` draw
  with an opacity map was checked in the capture.
- Specular (slots 6 and 8): the specular map is not drawn, because `c41`, `c50` and `c90` for stages have no proven source. Slot 8
  is drawn with its shadow and without the specular term; slot 6 is drawn as its base.
- Stage brightness and tint (render-m10): next to the capture at the same camera, the stage is brighter and less
  brown-grey. The stage vertex shaders scale the colour by an extinction term and the pixel shaders add in-scatter
  (`c55`, `c56`, `c65`), and neither is reproduced. Texture placement is checked against the capture (`lbs.md`, render
  chunk 0x6c, and Mip LOD bias above); the colour is not.
