# Renderer and visual systems

`crates/rbr-render`: the wgpu renderer, the driving window (`viewer`) and every visual system. Cameras, HUD, live
mode and the launcher: [cameras-hud-launcher.md](cameras-hud-launcher.md); sound: [sound.md](sound.md). Module
map: [crates/rbr-render/README.md](../crates/rbr-render/README.md). Dropped detail (surveys, tuning and timing runs):
rbr-recomp `rust/rbr-render/README.md`, `notes/visual-enhancements.md`.

## Decisions

- Presentation never changes the physics: it reads frames and payloads after a step and writes nothing back.
- Visuals and sound may be better than the original. The base look follows 1.02's data and render states;
  `--enhance` additions are ours.
- Particles aim to look good, not to match 1.02: no bit-exact state, no harness comparison.
- The windscreen gets a modern water-on-glass system in place of 1.02's screen-fixed `NParticleSystem2D` sprites.
- macOS (Metal) and Linux (Vulkan), portable code. `shader_check.rs` validates every WGSL module and override set
  through naga to SPIR-V as wgpu's Vulkan backend does. Reversed-edge `smoothstep` is undefined on Metal.
- wgpu 30, naga 30, egui 0.37 (2026-10-25, from 27 and 0.33). wgpu 30 checks the fragment stage's sampled textures at
  the pipeline layout (the reflections read 17; `request_gpu` asks the adapter's limit), refuses to reconfigure a
  surface while another thread submits (the loading thread: `present::configure` reports it and the windows try again
  next frame), and reports a hidden macOS window as occluded instead of waiting up to a second for a drawable
  (`present::Acquired::Hidden`: the viewer runs the frame undrawn, the live process stepping, at most every 16 ms).
  egui 0.37 has no default features (`default_fonts` named) and draws glyphs a few levels apart from 0.33.
- Highest quality always: the full `PS_*.ini`, the high renderer's LOD and visibility distances.
- The backdrop `(1,3)` comes in two kinds, and both must look right: low terrain with baked vertex light (retail,
  most RSF stages) and upright photo panorama strips with alpha sky and white vertex colour (many RSF stages, 462
  Odry; "Panorama backdrops" below). Backdrop code may not take it as flat, facing up or reaching the horizon: the eye
  can see over a strip's top. Check changes on both (41M and 462M, 138M).

## Architecture

- Data only through `rbr-formats`. `RBR_DATA` is the retail 1.0 install; `RBR_PATCH` (1.02's `Main_group`)
  overrides it per file (`rbr_formats::data::Data`), e.g. the car's `Fog/` shaders.
- Conventions (`coords.rs`, the one place): stage positions as stored, left-handed, y up, matching wgpu clip space;
  reversed depth (stage positions `@invariant` for the depth prepass on Vulkan); front faces clockwise; vertex colour
  D3DCOLOR (B, G, R, A); no sRGB conversion, non-sRGB 8-bit surface as D3D9.
- Physics is z-up: `stage = (x, z, y)`. The car model's front is `-z`, `+x` its left; `model_to_body` is a mirror so
  model to stage is a proper rotation. A part sits at `F * P * v`: `F` the node's chained world, `P` the 14-word
  transform `T * R1 * R2 * S * R2^-1` (`HH_H7ReadTransform` 0x1000c7a0, `coords::part_to_car`).
- `renderer/`: pipelines per render state, BC uploads, shadows, reflection cubes, the frame's passes (`frame.rs`).
  One sample a pixel, the alpha test alone (cut-outs `cutout`), anti-aliased by TAA (below) or, `--anti-aliasing off`,
  not at all. 4x MSAA, its alpha to coverage and `depth_copy.rs` are gone (2026-10-26); the reflections and the
  light passes read the single-sample depth directly. `--reflections high|medium|low|off` (`Reflections`; `on`
  reads as high): rays a pixel and steps a ray 4 and 64, 2 and 48, 1 and 32 (`SSR_RAYS`, `SSR_STEPS`; low, half the
  steps: wet 41M 14.2 to 12.8 ms, 45O chase 9.9 to 9.3 ms, 3 runs; 2.6% of 41M's pixels over 8, the puddles'
  reflections a little sparser; 2026-10-26). Off: no screen-space reflections, wet roads and water take the cube and sky
  alone (wet 1080p: 394M 26.4 to 10.5 ms, 41M 22.4 to 10.9 ms, 2026-10-25).
  Reflections trace at half size and composite by a depth-weighted 3x3 tent (`reflection_composite.wgsl`): 41M 12.8
  to 6.5 ms, 45O chase 9.1 to 5.3 ms; thin RSF reflections softer. Additions off: only water differs (44E max 17/255;
  71M 52 pixels over 8) (2026-10-26).
- `--anti-aliasing taa|off` (`AntiAliasing`, default `Taa`, ours after gp4's `taa.rs`; `4x` parses as `taa`): one
  sample, the main view's projection jittered (Halton 2,3, 8 phases, 16 below full size) and resolved into the post
  pass's scene (`taa.rs`, `taa.wgsl`): reprojection from the depth by the unjittered view-projections (f64 on the CPU),
  the car's own motion inside its body parts' box, Catmull-Rom history, YCoCg variance clipping. The car's blended
  draws alone write the frame's alpha under it (`pipelines::alpha_writes`, the clear's alpha 0): a mask of its glass,
  kept in the history's alpha, that takes mud and drops (alpha 0.5 to 0.9 and over) new. The rain and the particles,
  which write no depth, are drawn over the resolve in the unjittered view (`Renderer::late_effects`). The composite
  sharpens under TAA (`post.wgsl` `sharpened`, `TAA_SHARPEN` 1.0: unsharp mask in Karis's tone-mapped space, held
  within the four neighbours' range).
  History dropped (`Renderer::reset_history`) on a resize, an in-car/outside switch, an eye jump over 8 m, and from
  the viewer on a restart (`R`, live restart), a seek and any camera change that is not the same camera
  (`Shown::same_camera`).
  Measured 2026-10-26 (RX 9060 XT radv, `--enhance all`, 1440p, Laplacian variance of the frame): 4x / TAA / TAA
  sharpened: forest chase 251 / 136 / 230, forest cockpit 174 / 95 / 120, dry 541 / 330 / 578, wet 452 / 324 / 543, rain
  cockpit 702 / 483 / 674, night chase 145 / 87 / 142, a static forest still 422 / 201 / 355. A negative texture mip
  bias (0 to -1) changed nothing (201 to 205), so it was dropped. Render scale 0.75 and 0.5 resolve cleanly (16
  phases). `--anti-aliasing off` frames are byte-identical to before in 13 scenes.
  Dark specks on the car's silhouette in a still chase view (3 pixels over 20 luma below their neighbours; 34 with
  off) are not a history lock: the raw jittered frames alternate between dark and bright at those pixels, so they
  are the thin dark sliver's real coverage; a 3x3 neighbourhood or a looser clip left them.
- Pipelines are specialised by override constants (`pipelines.rs`): the lit class (`LIT`, 0 stage, 1-4 the car's
  classes), the car's draws in or outside the car (`INSIDE`, picked per frame in `draw_scene`), its draws all of the
  cabin's interior set or none (`CABIN_SET`; the interior's carry no fog), the draw's frame uniform (`FRAME_KIND`:
  the stage's, the clouds' or the sky faces') and the draw's features (`KNOWN`,
  `DRAW_*`: water, specular map, second texture, mip bias); each carries only the code it runs.
  Code a pipeline never runs still held registers: 200-470 byte spills per thread in the stage and car pipelines on
  an M1 Pro (Metal System Trace, no Dynamic Caching). Native x 0.75 on the M1 Pro, every enhancement: road and chase
  51-57 to 15-20 ms, cockpit 188 to 20 ms; frames as before within a few edge pixels. Live at 1280x720: 107 to 156
  fps; full-screen cockpit 12 to 54 (2026-10-25). The fog, the sun's cascades and the car's shadow still spill
  (~150-310 bytes) in the stage pipelines.
- Small arrays indexed in loops (`harmonics`, `texel_filter`) are written out: Metal kept them in memory.
- Reflections medium, low and off differ from high by up to 35, 58 and 79 in the wet 45_O scene (1280x720, Xsara,
  every enhancement; 2026-10-26, RX 9060 XT, radv).
- The car's shadow filters' taps were once a setting (`Quality`, `--shadows`, `SHADOW_TAPS`, `CONTACT_TAPS`): medium
  (3 and 4 taps) saved ~0.2 ms with at most 1/255 difference in the chase view, but in the cockpit with the sun through
  the windows it differs from high by a mean 0.3-0.7 and 1 percent of pixels over 5-10/255 (Fiesta R5, falstone and
  sipirkakim, 1920x1080), so the fixed filter stays high's (4 and 6). Measured no gain on an RX 9060 XT (2026-10-26),
  removed; `--shadows X` and `shadows =` in the settings are read and ignored.
- The stage's weather (the sun hidden, wet, volume fog) was once built into the stage's pipelines (`WEATHER_KNOWN`,
  `Conditions`): measured no gain above noise at 1440p and 4K in haze, fog, wet and dusk on an RX 9060 XT
  (2026-10-26), removed; the frame uniform says at run time.
- The stage's day pipelines once left the night's code out (`DAY_ONLY`, `night_probes`): measured no gain on an RX 9060
  XT (2026-10-26), removed.
- The reflections' pipelines were once split by water and wet road (`REFLECT_WATER`): measured no gain on an RX 9060 XT
  (2026-10-26), removed; one pipeline draws both, the draw's mode says which.
- A pass over the kept frame whose target takes no copies (a window's surface, without the post pass) draws the frame
  into it with the mip blit (`over_attachment`); `viewer track-20_N --anti-aliasing off` failed validation before.
- The car's shadow map's depth range about each 4 x 4 texel cell (`ShadowRange`, `shadow_range.wgsl`), spread over
  each layer's filter reach after the maps are drawn: `car_shadow` and `car_contact` return the filter's answer from
  one texel where every tap's texels lie on one side of the compare (the same frames; the interior's own faces vary
  within the filter, so the cockpit gains little).
- The sky (gradient, clouds, sky faces) draws after the opaque meshes at depth 0 with an `Equal` test; the frame
  matches the sky-first order (13 scenes) but for `--anti-aliasing off`, where 10 cut-out edge pixels on track-54_E
  differ by up to 3/255. 4K cockpit on Semetin at dusk in heavy rain, every enhancement: 36.7 to 31.3 ms (M3 Pro,
  2026-10-20).
- Run to run (Metal, M-series, 2026-10-20): the pass after the depth prepass keeps the alpha test, so 18 of 18 runs
  match (without its `discard`, 66_E's spectators and twigs differed in ~1 run in 3). Still open: with 4x MSAA only,
  ground triangle edges differ by up to 35/255 (33_N at 2000, ~half the runs) while uploads, draw lists and matrices
  are bit-identical; from the fourth frame on, 24 of 24 runs match, so captures comparing frames render a few frames
  first (`--frames 3`).
- Inputs: the recording frame (FRM1: car block, wheels, controls); in live mode also `PTC1` (tyre payloads), `PEV1`
  (damage, damping, engine fire and noise, exhaust, lights), `DMG1` (zones, last damage code), `DET1` (detached
  bodies), `CLK1` and the HUD records (`rbr_recording`). Playback takes `PTC1`/`PEV1` from `RBR_PARTICLE_EVENTS` (a file
  `rbr-standalone --boot --recording X --events-out FILE` writes; `particles/sidecar.rs`), else none.
- Presentation clocks step 7 ms per payload period; a live answer of several periods is one game frame.

## Live frame loop and input latency

`bin/viewer/window.rs`, `live/mod.rs`; measured with `RBR_LATENCY=script` (`bin/viewer/latency.rs`: a ±0.5 steering
flip every 500 ms, ms from the flip being due to each stage, summary at exit). Ours, per frame, in this order:

- acquire the surface's texture (the loop's wait: a free image, the refresh under fifo; `desired_maximum_frame_latency`
  2, since with 1 and the wait first the frame missed every other refresh, 27 fps at 4K on cachy);
- gilrs events (the pause menu's navigation) and the input thread's last poll (`window/feed.rs`): the input read;
- `Live::pump`: the payloads due at 7 ms per period (at 60 fps 2-3 a frame, all with this read), then up to 6 ms
  waiting for their answers (one frame of 5-6 NGP steps: p50 1.5 ms, p95 2.2 ms with the pipes), so the frame draws
  the step of the input it read;
- the playback shows the newest answer; the crew's wheel and hands take the car's control the last step took (CTL1,
  the filtered value: LocoMotion `Unk18` 0x10020cf0 turns `i_steeringwheel` by `CSigCarControl` +0x10, the
  filter's and NGP's payload, not the device value);
- CPU build and encode (6-8 ms at 4K cockpit), submit, present; GPU 10 ms (4K, 4x AA, cachy's RTX 2080 Super Max-Q).

cachy, 4K 60 Hz, scripted flip, 3 runs each, p50 (p95) ms from the flip: before, input read 7.5 (16), answer
taken 25 (33-45), submitted 41 (49-62), GPU done 51 (58-71); after: 9 (16), 11 (16-32), 18 (21-49), 28 (30-58).
On screen 41 (46) with direct scan-out, 61 (61) composited; `play.sh` sets `MANGOHUD=0` (it added p95 tail).

Live (`rbr-standalone --live`, the only live mode; the viewer has no pacing of its own): a frame per physics step on
its own clock (a late wake's frame takes the missed steps, cut at 0.1 s), each with the newest input; the input thread
(`window/feed.rs`) polls the devices every 1 ms (50 ms keepalive, one-shot flags ORed until a frame takes them); a
52-byte STP1 record after each stepped frame carries force, control and input number. `--frames-out` logs each
frame's digest; `--frames FILE|-` replays such a log; the tools that step a drive themselves (`Live::lockstep`:
captures, `--screenshot --at`, `rbr-sound`) write one unchecked frame a step (1/720 s under NGP). cachy, 35 s
scripted: flip to first step p50 1.64 (p95 1.84) ms; 720 steps/s, one per frame (3 of 25175 two); wake p50 0.075 ms
late (p99 0.28); on screen 42 (51). Wheel: 526 level changes/s against 43/s paced (RSF about 510). Replays: scripted
17999 of 17999, autopilot 17994 of 17994 frames equal.

RSF (drive2 sessions, RSS1): NGP's input thread polls the wheel every 1.1 ms (POL1), one 1/720 s step per frame at
about 580 fps; a poll change reaches the car's control in p50 1.0 ms (p95 1.6), its step's end in 1.2 (1.7). Ours stays
a 60 Hz render frame behind that for the picture; the input and the force are RSF's in real time.

## Faithful and ours

| System | Faithful | Ours |
| --- | --- | --- |
| Stage | slots, render states, LOD lists, visibility cells, lanes, props, light maps | LOD 600 m, texture fallback, water surface |
| Sky | `TrackSettings.ini` values, linear fog, cloud face | section pick, sky, haze caps, tone curve |
| Car | parts, switch states, classes, draw lists | lighting, reflection cubes, sun and contact shadows |
| Car shadow decal | `RenderShadows` 0x605670 | body pose as its frame |
| Lights | switching, part predicates, flares | headlight key, night light |
| Damage | deformation, scrape, crack projection, dirt level | crack texture, glass grime, detached motion |
| Particles | emitters, classes, handlers, rain instances | step, pools, lighting, sparks, chimney smoke |
| Glass | wiper angles and timing, bug data | the water-on-glass model |
| Skid marks | emission, rings, fade, strips | frame-time accumulator, depth bias |
| Ghost | mesh, look, pose, files | race-time origin, best ghost |
| Characters | placement, Lua, triggers, clips, crew IK | shading, CPU skinning |
| Fences | `CFenceHandler` cloth | inside test, body placement |
| Dials | RBRHUD data | all drawing |

## Stage

- Geometry `(1,0)` slots 0, 5-8: texture 0 times vertex colour, texture 1 blended by vertex alpha, back faces
  culled, wrap. Slots 7 and 8 with the shadow flag multiply the light map (TEXCOORD2 / TEXCOORD4). Mip bias -3 on
  near chunks.
- LOD (chunk 0x6c): list 1 within 600 m of the block's centre, list 3 beyond 550 m, list 2 always
  (`stage::LOD_DISTANCE`; reference installs run low's 200 m).
- Visibility cells `(2,10)` (`visibility.rs`): a cell's block, lane and props draw while its box begins within
  `CELL_REACH` 315 m. Ours draws to `DRAW_REACH` 3 km, so from a crest the stage shows rather than the coarse backdrop
  (61_M from 80 m up: 27k to 70k triangles); the reflection cube takes cells within `CUBE_REACH` 10 m, no props.
- Object lanes `(1,1)`: alpha test `> 60/255`, double-sided, address modes from the kind byte.
- Interactive props `(1,6)`: every retail record drawn at its placement while its cell shows. Live moves a woken prop
  by its solid's pose and keeps its last pose asleep. Drawn after the solid stage and before the blended meshes, so the
  car's glass (no depth write) goes over them. Scenery physics: [physics.md](physics.md).
- Sky faces `(1,3)`: no culling or depth write, cut at `> 60/255`. Clouds (`Clouds.lbs`): the face named by the
  section's `CloudName`, camera-anchored, before the stage.
- Water `(1,8)`: our water surface (retail texture keys unresolved). Reflection items `(1,7)` withheld. The sky dome
  (`skyboxes.lbs`) is not drawn.
- Textures: `normal`/`worn` fall back to `new` by the filename map; a missing `wet` texture reads `damp`, then `dry`
  (ours; reaches only maps the season table keeps dry, `tests/stage_data.rs`).
- Texture size: the device takes the adapter's 2D limit (8192 by default; RSF 484's backdrop is 16384 x 8192, RX 9060
  XT allows 32768); a texture past it starts at its first mip that fits (checked capped at 8192, 2026-10-27).
- Ours: ground detail noise, the backdrop's own texture tiled at 12 and 47 m as its detail (`backdrop_detail`, faded out
  on faces steeper than about 55 degrees, where the flat tiling streaked), road
  shine on specular slots 6 and 8 (the game's formula, our `c90`); the wet film is the `wet` addition.
- Terrain detail (ours, always on; `terrain_detail.rs`, `terrain_detail.wgsl`, 2026-10-28): RSF Luceram's ground is
  4096 atlases (`z*`, past the upscaler's cap) spread a few texels a centimetre, its banks painted by a projection from
  above, so the streaks are in the texels. At load each texture's most detailed 512-texel window (an eighth, at least
  256; scored at its mip of 512 or less) goes in `mode.detail`; on the near ground (mip bias draws, not cut-outs) where
  the texture is magnified, the window tiled mirrored at 160 and 368 texels a metre, triplanar, adds its high pass
  (brightness over its blur 3 levels coarser) to the albedo, a quarter as much on the flat; steep faces read their
  texture one level blurrier. Tiling the whole atlas left the detail patchy (padding, road strips); full strength on
  the flat put 580's atlas gravel on its tarmac. Load 63-72 ms (52M, 417O); scene pass 1080p 417O rain fly-by 2.74 to
  2.84 ms, golden fly-by 3.02 to 3.15, 580 chase 4.21 to 4.25; 1440p 4.13 to 4.30 (`RERBR_NO_TERRAIN_DETAIL` A/B,
  same binary, GPU otherwise idle).

## Atmosphere and sky

`atmosphere.rs` reads one `Maps/TrackSettings.ini` section: the season row's own, else the stage's sections with
the wetness's preset word, else the first (`viewer --sky`; every retail row of an installed stage but `69S`'s clear
ones names one). `viewer --fog off|light|heavy` (ours) puts our height fog over any section; `--clouds` and
`--precipitation` put any sky over any row. Haze is linear fog `FogStart..FogEnd`; without `UseFog` the game's
scattering shaders are not reproduced, so the fog is capped (`CLEAR_HAZE`) and its `FogEnd` is at least 600 m
(`CLEAR_FOG_END`; retail 69's light fog and 71, and 45 RSF rows such as `580M_crisp_clear` at 25..215 m, left a
white wall; other frames byte-identical, 2026-10-27). The sky gradient, doubled clouds and tone
curve are ours.

- Backdrop `(1,3)` (the superbowl, 0x1002f0c0): every item drawn, world identity, the stage's matrix (shader manager
  `+0x4c0`), no cell pick. Its scattering depth is scaled by `Superbowl_Scale` (`c66.x` = settings `+0x20`; the stage
  sets 1); `LoadSkySettings` 0x100526f0 copies `CloudName` to `+0x60`, so the scale survives. Without `UseFog` its
  haze is the stage's range over the scale (`Atmosphere::superbowl_fog`), so the backdrop next to the road stays clear.
- Fog with `Enhance::lighting` (ours, `height_fog.rs`): height fog over stage, backdrop, sky and cloud faces, 95% at
  half `FogEnd` (`heavyfog`), three quarters (`lightfog`) or all of it at the low ground, thinning with height; the sky
  takes it to 100 km. The backdrop is fogged at its own distance, as the stage: the earlier floor of 3 km (for faces 600
  m off on 129 Peyregrosse, not reproduced on its fog sections) left tree lines 50-100 m from 122 Bruchsal's road as
  ghosts and its flat backdrop a pale slab from above (2026-10-27); without ground about the eye or at night the linear
  fog stays, the backdrop at the stage's range.
- The sun's light in the air with `Enhance::lighting` (`light_shafts.wgsl`, ours, 2026-10-27; in place of the
  screen-space shafts), by day under a sky not overcast, fading in over 0-4 degrees of elevation: a quarter-size march
  per texel, 24 steps spread by the square of their index to the depth (2 km at most), the cascades (one linear
  compare a step, the same sampler as the beams' headlight layer) times the volumetric clouds' shadow map
  (`volume_shadow.wgsl`), jittered and 3x3 depth-bilaterally blurred. The scene's mist already holds the sunlit air's
  light, so adding in-scatter laid a pale veil over the far land (the first version, sheets 2026-10-27): the map is
  the frame's light (least of four bilinear reads per texel) times the shaded air's cover (the height fog's density,
  at least the base haze) times the sun's share of the mist's light (`MIST_SUN` lobe with the haze's own toward-sun
  lobe, `MARCH_LOBE` 3.7, to linear), negative; the post composite adds it unchanged, so the beams' night frames stay
  byte-identical (a multiply in the composite moved 1-4 night pixels by 1/255). Plus a glow added within 350 m, HG g
  0.85, 0.06 of the sun's colour per metre of air at least 1/3000 thick, so the sun shines through the canopy in
  rays; it shows in full only where a quarter of that way is in shade (`GLOW_SHADE`): through open sunlit air it laid
  a warm wash over the land toward the sun (45O 16:30 from 290 m up), whose light the haze already holds. Shows in
  fog and toward a low sun through trees (41M 07:30, 25O 09:00); in clear air it is faint (the mist near the road is
  thin), at 13:00 nearly nothing (mean under 0.4/255). Cloud gaps only darken the far air slightly. Light shafts 0.16-0.18 ms at 2560x1440 (RX 9060 XT, shared GPU; 0.01-0.14 before). Additions
  off, overcast, night, night fog and dusk byte-identical to before. The wheel spray's dust grid (a CPU
  splat uploaded each frame) made no visible difference and was dropped.
- The backdrop's computed sky occlusion and bent normal are weighed against the backdrop's own meshes alone (a second
  run over its vertices, `MeshBuffers::backdrop`, `occlusion.rs`), not the strip, which the backdrop continues under:
  with the strip as cover its vertices went near black and the large triangles carried it past the strip's edge (41M
  raised camera, `--enhance all`, 2026-10-27). The strip's own occlusion is unchanged (before/after diff on 41M is
  backdrop pixels only), load +0.0 to 0.1 s (Semetin 0.2 to 0.3 s). What remains is the sun's: the cascades and world
  layer shadow the backdrop below the strip's edge and trees (fixed below), and in Capo di Feno 369 a hard-edged patch
  goes black with `lighting` (fixed below: a sea sheet of the backdrop lies 120 to 200 m over the
  stage's ground tiles there).
- The backdrop's sun lookups (`lifted` in `shader.wgsl`, cascades and world layer) sit `BACKDROP_LIFT` 80 m higher
  along the sun's ray (at most 400 m along it under a low sun), so that the stage's edge and trees, tens of metres over
  the backdrop, do not shade it; ridges farther off still do. Lifts of 8, 20 and 40 m left the 41M wedges, 80 m
  removed them (41M raised camera, 12:00 and 17:30, 2026-10-27). The stage's lookups are unchanged. The other way, the
  backdrop's casters sit `BACKDROP_DROP` 200 m lower in the sun's cascades, far and world layers (instance 1 of the
  backdrop's draws, along the sun's ray, at most twice that; `sun::backdrop_drop`) and in the stage's run of the sky
  occlusion (straight down), so that the backdrop casts nothing on the stage's ground it lies over: 80 m left the Capo
  patch black, 140 a blob, 200 clears it; Valstagna's low-sun gorge changes 0.7 of 255 in the mean between 80 and 200,
  so ridges still shade the strip. Backdrop on backdrop then sees casters 280 m lower than its lookups: only ridges
  over that shade it. Frame and load time within noise (41M 0.94 to 0.97 ms, Capo 2.50 to 2.53 ms profiled at 1440p).
- Panorama backdrops: on 60 of 356 installed RSF stage folders the backdrop is at least half upright faces, on some a
  ring of photo strips with alpha sky (462-467 Odry: 4 items, 22 triangles, 2.5 to 2.8 km out, white vertex colour,
  `HOR_2`/`Hor_11` 4096x512; also 138-140 RSI, 149 Pribram, 166-169 Hradek and Liptakov, 436 Barak). Where the eye
  is above a strip's top, the sky dome shows below the horizon: with `Enhance::lighting` it takes the horizon's light
  there (`SKY_HORIZON`), not the tables' grey ground (also seen from crests on retail 41M, 45O, 61M) (2026-10-27).

## Time of day and night

`--clock HH:MM` (`relight.rs`). Each variant letter is its own bake, so the bake's sun is fitted from the vertex
light (`relight::fit`), and the shader swaps its `b * facing` for the drawn sun's. A solar model per rally moves the
sun. Below -7 degrees: night (`night.wgsl`, `lamps.wgsl`): moonlight (`Atmosphere::moonlight`, at half
saturation), the lamps as main or dipped beams with a shadow map, stars, wet-road lamp glints. Shared bakes with M names (73, 83-89) probably fit the wrong hour.

- Variants are one stage under another light: the same geometry (99-100% of vertex positions) and textures but the
  sky, the vertex light, the shadow light maps (`shadow_0.dat`) and the section (2026-10-23, every installed variant).
- Every stage is relit (`--no-relight` keeps the bake): at `--clock`, else under the bake's own sun at its hour
  (`Clock::new`; where the rally's path never climbs as high, as on the British stages, the path is lifted to it),
  and the viewer gives the start board that hour (`--start-time`, not on a championship stage).
- The start board (ours, the game's lamp switching not traced; `start_board.rs`): its red and green lamp textures
  (`START_CLOCK_*LIGHT_ON.tga`) are `Dynamic` with filler texels in retail, so the game writes them. Ours follows
  the race clock: red while the countdown runs, green for 5 s from the go, none on a jump start's go, an unlit lamp
  darkened to a quarter. Lit lamps glow at any hour, the clock's digits by night and at dusk (`backlit`, the dials'
  emissive mask path), still fogged (2026-10-27, track-54_E captures).
- Baked shading lives in three places. The shadow light maps paint the bake's cast shadows: with `Enhance::lighting`
  they are neither loaded nor used (Semetin 176_M: 165 maps, 350 MB); the cascades and the far layer cast for any sun,
  and at night for the moon. The backdrop casts into every sun map like the stage; past the far layer the world layer
  (`sun::world`) shades stage and backdrop when the light changes. No measurable frame cost (2026-10-25). The backdrop
  takes no sky occlusion (baking it left it near 0, +0.1-0.3 s load for nothing). The vertex light less the bake's sun
  (`relit`, `sky_share`) is the ambient occlusion, or the computed one with `Enhance::lighting` (the bake then gives
  only the hue, so the crowds' saturated clothes are not lifted past their texture); it also softens the sun's shadows
  where little sky is seen, and is not on the lamps' light. No stage paints shadows into its track textures (every
  installed variant, 2026-10-23).
- The stage's own sun is the fitted bake sun where the fit holds (`relight::own_sun`, r from 0.35): 40 variants'
  `SunDir` lie over 20 degrees off it; weak fits keep `SunDir`. The fit samples 100k vertices, 15 degrees over the sky
  then 5 about the best.

- Night albedo (`night_albedo`): textures times vertex luminance over the bake's open-ground light, clamped 0.3-1 at
  0.25 (0.14 on 45_O to 0.39 on 10_O).
- Headlight beam (`beam`, ours after the ECE patterns): dipped, cut-off 0.7 degrees down, reach about 60 m; main
  (the default, as rally cars run stages), dipped plus an axis lobe of 5 times the hot spot, 1/e at 4 degrees across
  and 2 up, reach about 130 m. `LampUniform` forward w: 1 dipped, 2 main.
- The lamps' light in the air (`light_shafts.wgsl`): quarter-size ray march at night and dusk, scattering 1 / fog
  range under `UseFog` (a tenth in rain sections, whose fog stands for spray), a fifth otherwise, at least the height
  fog's density at the eye; 12 equi-angular steps a lamp (Kulla and Fajardo 2012): 1.36 ms at 1080p. The main pass keeps
  its depth whenever the post pass runs (else the beams marched 120 m of fog over the cabin). The mist's forward lobe
  is the moon's once the sun has set; the haze's sun lobe fades with the sun's true elevation.
- The night's light is a moonlit night as the adapted eye sees it: clear, the frame's mean about 11 of 255 from
  40 m up on 44E, falling with cloud cover squared to 0.4 under heavy cloud (2026-10-23: 44E, 45O, 21O, 41M).
- Skid marks and the car's shadow decal are unlit records, so they take the moon (`mark_light`) and the lamps.
- With the physical light the moon is the real one for the rally's date in 2004 (`relight::lunar`, decided
  2026-10-24): Meeus 47.a within 0.5 degrees, light by phase (half moon 0.09 of full), none below the horizon.
  2004's nights: arctic 0.35 of full, Mont Blanc 0.07, USA 0.78, British, Australia and Japan moonless. The disc shows
  its phase; the game-bright path keeps its full moon opposite the sun.
- With the physical light twilight and night are the day's path: the sky's tables run at every sun elevation, the moon
  (0.27 lux full) is the direct light from 1 degree down with 0.002 lux of airglow, and night from -18 only turns on
  the night-only effects. The lamps light in lux (dipped hot spot 20 000 cd), only in pipelines built with them.
  Measured on 45O (2026-10-23): twilight at -6.4 degrees 5.0 lux, -9.9 0.07, -13.3 0.006; the frame's mean 92 at 17:30
  falls smoothly to 14 by 18:25 and holds to 23:00. Rod vision (`rod_vision`): moonlit ground turns blue with dim
  greens and reds kept, the headlit road stays neutral, by day at most 1/255; the meter bins luminance from -14 to
  18 EV (in pre-exposed units twilight went black).
- With the day's light the lit draws outside the car (`day_lit`) shade as the stage: linear albedo (at most 0.9),
  Lambert under the day's sky, sun and ground maps, plus the sun's Cook-Torrance highlight; paint and glass reflect
  by Schlick's F0 0.04. The game's Ka, Kd, Fresnel limits and `LIGHT_SCALE` are off that path (they put the car at
  a fifth of the stage's light). The windows now show the interior. Frame time unchanged (2026-10-24: 176M, 41M, 45O
  chase).
- On that path paint and glass are a clear coat (`COAT_ROUGHNESS` 0.07, Karis's environment BRDF, sun highlight
  Beckmann m 0.04 in place of `CookTorranceM`), mirroring the 128-texel surroundings cube, still drawn beside the
  probes when the car is seen from outside (one face every 3 frames, 0.10 ms; the probes' 64 texels read blurred on
  the bonnet). Fiesta R5 and GR Yaris on 103O at noon: 5 to 16% of pixels over 8 of 255; cockpit and additions off
  byte for byte (2026-10-26). Bonnet SSR changed little over the cube and was dropped; the car's own ambient occlusion
  is not computed (car vertices are posed on the CPU each frame).
- Car look (2026-10-28, `body_occlusion`, `car_look`, `COAT_*`, `PAINT_PIGMENT`, `WHEEL_LUMA`..`RIM_*`; day's light, not
  in a cockpit's mirrors): the body's ambient and bounce light times an analytic occlusion in model space (aside faces
  at the ground `CAR_AO_SIDE` 0.2, down faces to 1.1 m `CAR_AO_DOWN` 0.85: sills, arches, underbody; the coat's reflection
  too); paint reflection x`COAT_STRENGTH` 1.5, a second sun lobe (`COAT_SHEEN` 0.25 at m 0.2), pigment saturation 1.25; wheels:
  rubber by texel luminance (albedo 0.6, spatter half, `RUBBER_*`) and the rim a rough mirror (`RIM_METAL` 0.55); the
  contact shadow `GROUND_SCALE` 0.4 to 0.7 (kept 0.4 in the cockpit view). gab-lit: scene pass 3.01 to 3.06 ms; cockpits
  and additions off byte for byte, rain flyby keeps its wet look. Per-vertex computed occlusion and the arches' exact
  shape need the car in the prepass (`prepassed` leaves lit draws out), not done.
  Round 2: the car's opaque reflecting draws are in the main view's depth prepass (`prepassed`, `fresnel_max > 0`: not
  the characters; outside views only, cockpit unchanged) and shaded by the outside pipeline (`DrawPass::CarMain`, Greater over a prepass depth `PREPASS_BIAS` behind it, so coincident faces keep their order: Equal turned the Yaris's LED strips black), so the stage's
  contact shading (`contact_light`) sees the car: the body takes its square on the ambient, and the road near the car its
  occlusion. The coat mirrors `HORIZON_SHARE` 0.5 of a procedural far field (`horizon_mirrored`: the stage above, the cube's
  blurred ground below, horizon at -0.3..0 of the reflected height, as the stage's skyline stands in the probes) so
  panels show a band over a dark one (its upper band the cube's own, blurred to `HORIZON_BLUR` 4, so a forest's is dark); `COAT_SHEEN` 0.25/0.2 to 0.6/0.3, `CAR_AO_SIDE` 0.2 to 0.12.
- Car look round 4: a lamp's lens is a reflecting lit draw with no dirt map (the M3's headlights are class-1 textured discs, no
  glass mesh, `lens`): a sharp Fresnel mirror (`LENS_ROUGHNESS` 0.02, `LENS_COAT` 3) with the bright texels (`LENS_LUMA`) as
  chrome (`RIM_METAL`, `LENS_BOWL_ROUGHNESS` 0.15); the Yaris's LED strips are not affected. Tyre dirt (dirt map and spatter)
  falls to `RUBBER_DIRT_HIGH` 0.15 above `TYRE_BAND` 0.08..0.3 m over the ground (`frame.occlusion.z`), rubber only.
- Rain on the paint (`wet_paint`, `wet_base`, `WET_*`; only with rain on wet roads, `frame.wind.z`, so damp, dry and
  additions off are byte for byte; dry overcast 3 pixels of 2 M one level off, the codegen): clear-coat roughness
  halved, base 15% darker and 40% more saturated (negative channels clamped: they made the yellow decals lavender
  through `pow`), the sky it mirrors at `WET_SKY` 0.35 of its light, beads on up-facing and streaks on standing panels
  tilt the reflection's normal (`WET_TILT` 0.35) and darken the base, placed in the car's model space (stick to the
  body), faded out over 6 to 18 m; lower body (model y under 0.55 m) takes `WET_GRIME` of `car_dirt`'s colour, and the
  body under 0.8 m darkens to `WET_UNDER` 0.7 of its colour (the car has no ambient occlusion). Cause of the pale
  look, by isolating terms (Luceram fly-by): the diffuse base matched the AI target's blue, the Fresnel reflection of the
  bright overcast sky added about +30 on R and G over the whole bonnet, roof and nose (the old `WET_COAT` +20% and the
  beads' tilted mirror made it worse, the white sparkles). Scene pass on the fly-by (1080p, ultra, 60 frames) 1.73
  -> 1.74 ms (2026-10-28). The cockpit view's exterior paint seen past the A-pillar changes too, the cabin does not.
- Overcast is the section's cloud word, not `UseFog` alone (which also marks fog under a clear or broken sky: those
  now have their sun). Under it the tables hold the CIE overcast sky, giving up-facing ground a quarter of the clear
  sky's light (Kasten and Czeplak 1980); 45O wet mean 92 to 95; dusk to night smooth (2026-10-24).
- With the day's light the roads with a specular map reflect as a rough dielectric (`with_specular`, retail only):
  F0 0.04, roughness 0.75; the game's `road_shine` stays beside it (the gravel's stone detail). Frame time ~0
  (2026-10-24). On all ground it cost 0.5 ms for a few percent; on leaves the crown normals turned the canopy white.
- The clock's ambient share and tint scale the stage's display colours, so its light is them to the 2.2; day frames
  byte-identical (2026-10-20).
- The headlights' shadow map has its own layer of the sun's map (`sun::LAMP`), so the lamps cast at dusk too, beside the
  sun's cascades.

## Car

`car/` reads `Cars/Cars.ini`, the `.sgc`, `<car>.ini`, `<car>_shaders.ini` and textures.

- State per `SShaderMaterial::SetupRenderState` 0x1003f4b0: an opacity channel means blend, test `> 0`, no depth
  write; `CarPlastic` writes depth, two-sided; `CarIntGlass` two-sided. Order (`FUN_1001e430`): wheels, opaque,
  transparent.
- Not drawn: `ghost_*`, `texture_*`, `Plane_*`, switched parts not in their state, gravel/snow/spare wheels, `k_` and
  `d_` parts. The interior `i_*` only in the cockpit (`CHH_Car::Render`); the bonnet view draws the exterior (user).
  `--hide-wheel` (ours) drops the interior parts the steering turns (centre within 0.12 m of `i_steeringwheel`):
  one mesh on the 911 SC Grp4 and the i20 WRC 2020, dash and gauges kept.
- Lighting (ours; `c41`, `c67`, `c68`, `c90` unproven): per class, `<car>_shader_settings_clear.ini` (`Ka`, `Kd`,
  `CookTorranceKs/M`, `FresnelMin/Max`, `Kf`) with the stage's light; Schlick `F0 = 0.125` (`c91`).
- Reflection: a 128-texel cube of the sky and the cells within 10 m, as the game's; a second cube to 315 m (ours) for
  the car's ambient and sun occlusion, wet roads and water.
- Shadow decal (faithful, `car_shadow.rs`): `car_dpshad2.tga` on the `.col` triangles under a 2.0 x 4.2 m rectangle
  centred `(0, 2.3, -0.3)` in the body frame; alpha `Car_Deep_Shadow_Alpha` x `asin(m[8]) * 0.63662` x the height
  fade; none on an RSF-encrypted `.col`. `--enhance contact` replaces it.
- Playback: body and tarmac tyres from the recorded poses; steering is not recorded.
- Tyre place (2026-10-28): the wheel block's hub (`+0x140`, the recorded anchor) is the hub moved along the axle (`+0x9e0`, outward
  on both sides) by `-HubOffset` (`place_hub`); `Copy100` moves it back by the tyre's `HubOffset` (tail `+0x6c`) for the wheel's pose.
  A tyre goes at `anchor + HubOffset * axle` (`WheelPose::centre`), the live process sending the offsets (HUB1, `rbr_recording::hub`;
  recordings carry none and keep the anchor). The 208 Rally4's and Clio Rally3's (-0.045 tarmac, -0.04 loose) sit 4.5 cm further in: tyre
  centre 0.750 m against the model's own 0.744 m, no longer 10 cm outboard of the body. Particles still take the anchor.
- Not drawn: `pc_e_premove` (2026-10-28): HedgeHog3D's part classifier 0x1001e090 compares the name to it first and returns
  kind 10, which `FUN_1001e430` (`cmp eax, 9; ja` at 0x1001e59d and `cmp eax, 8; ja` at 0x1001e61e) adds to no draw list.
  RSF models give it a `Default Material` FaceBlock (40 of 583 installed `.ini`s, retail cars have none): a 2.3 m
  cube about the cabin, white without a texture, which showed as a pale panel behind the rear window in chase views
  (GC8 555, Clio Rally3, Fiat 131 Abarth Grp4; Fiesta Rally2, M3 E30, Escort Mk II unchanged byte for byte).
- Glass seen from outside with the day's light (ours, 2026-10-28; `GLASS_ABSORB`, `GLASS_DARK`, `GLASS_WET`): the blend
  weighted the reflection by alpha times F (a tenth of F on a 0.1-alpha pane), so the panes read clear over a cabin
  lit as the outside. Now the reflection takes its own weight F (rain: 1.5x, `GLASS_WET` 0.5, the water film), the pane absorbs at
  least 0.5 of the view (0.55 in rain) and its own colour keeps 0.6 (0.35 in rain; a first 0.8 and 0.6 read as milky fog), so the crew dims and the sky and trees mirror.
  Water seen from outside (`wet_layer` in `glass.wgsl`): a drop is 1 to 4 mm, under a pixel at fly-by distance, so the
  glass read dry; enlarging them to 1.7 px read as aliasing speckle and was dropped. The film's cells (48 x 24) now
  add a soft layer: value noise 3 and 2 cells across, stretched 3x up the pane, brightening by up to `WET_LIGHT` 0.4 of
  the glass's light times the film (so wiper-swept areas stay clearer), fading out as a cell grows from 14 to 30 px
  (drops resolve then). Outside views only (`outside_view`); cockpit byte-identical (an earlier 1.6e-5 difference was
  a noisy render, not the code: three later renders equal main).
  Only the window parts (`Shade::window`, `damage::window_slot`); the lamps' glass (body meshes of the same class,
  the GC8's tail lamps) and the in-car view keep the old look. Dry day shots change on every car (glass only);
  scene pass unchanged (2.93 ms before and after, rain flyby). The cabin's own light from outside (the interior set
  is lit as paint: `cabin_face` needs `inside`) was tried: no visible change, left.
- `CarIntGlass` in the in-car view with the day's light (ours, 2026-10-26): translucent texels are black at their
  alpha times one less their luminance, a tint over the outside; texels from alpha 0.5 to 0.75 turn back to lit (sun
  strips and visors printed on the glass, alpha 224 and over). Lit as in the game under the day's exposure, a light
  texel (RSF 52's `i_window_f`, grey 134 at alpha 85) greyed the view in overcast and wet. 12 RSF cars checked;
  additions off byte-identical.

## Lights

`lights.rs`. `CCarLightSystem` (`CCar+0x838`) emits `CSigCarLight` per lamp; `CHH_World::OnCarLight` 0x10068e30
swaps the `Predicate = on/off` parts and restarts the flare fade.

| Kind | Lamps | Switch box | On when |
| --- | --- | --- | --- |
| 1 | headlights | `light_lf/rf` | always (setter never called) |
| 3 | tail | `light_lb/rb` | always |
| 4 | brake | `brake_lb/rb/m` | brake to the drive `> 0.1` (`Set1e40` 0x4f1e40, pedal swap included) |
| 5 | reverse | `reverse_lb/rb` | gear index 0 (`Set1ed0` 0x4f1ed0) |

- Lit = kind switch, master word `+0xdc` and the lamp's works bit; the `lightglass_*` part and flare go with the glass
  bit. Live and sidecar: PEV1 kind 6. Otherwise brake from `CCar+0x4ec` past 0.1, no reverse or damage.
- Flares (`InitLights` 0x10016fd0, `RenderLights` 0x100158b0): `<car>_settings.ini` `LightN`, key `kind * 4 + i`;
  additive quad at `pos - 0.2 * toCamera`, alpha `facing * fade * 114.75`. Only the first record of a key with a
  `Switch = true` part switches. Ours: the headlight key (L) steps main beam, dipped, off (with the tail lights).
- Ours: only headlight records draw a flare; the rear lamps show as their lit parts, which glow (`LAMP_GLOW`) day
  and night. The game's rear sprites read as blobs by day.
- Ours: each kind's light at a luminous intensity (headlight hot spot 20000 cd, tail 8, brake 100, reverse 150); a lamp
  in its state since the drive's first frame is settled, not fading (2026-10-25). The rear lamps shine in a flat band
  after ECE R7.
- Ours: the headlights' light back off the road (`lamp_bounce`): two Lambertian patches facing up, 15 and 55 m ahead;
  scene +0.15 ms at night (45O chase and cockpit).
- Tried for the beam's white look (2026-10-25) and dropped: pulling the meter toward the 97th percentile, and local
  exposure from 1.5 stops (halos, day highlights down). The road under the beam at 1 to 5 lux against moonlight about
  50 times dimmer is physical.
- Ours: a car with no kind 1 record (an RSF car installed with headlights off) gets a beam per
  `Point_light_lf`/`rf` for the night light, no flare, in the colour of its "on" option's headlight record
  (`rsf::headlights_on`), else the retail 255, 255, 233.
- Open: the writer of the lamp words on damage; `RenderLights`' second visibility test.
- Ours: the headlights' shadow map (`sun::LAMP`) shades the beams beyond the stage (2026-10-27): the rain, snow, dust
  and mist through `lamps_at` (`effects.wgsl`, one tap 5 cm toward the lamp, headlights only), each step of the beams'
  march in `fs_beams` (one tap) and the wet-road lamp glints (`lamp_glint`, 3x3 taps); the sampling is one reusable pair
  in `lamps.wgsl` (`lamp_shadow`, `lamp_shaft`), which `lamp_reach` uses too. New sampling only with `Enhance::lighting`
  and a drawn map (else a zero matrix reads lit): day and shadows-off frames byte-identical. 41M start chase in fog: the
  air cut into shafts (mean diff 0.32, max 147 of 255); scene/particles/shafts within noise of before at 1440p (+0.06 ms
  at most). The retail skid marks keep their unshadowed lamp light.
- Rear lamps (tail, brake, reverse) are capped by a soft knee on their summed light (2026-10-27): a brake lamp's 100 cd within a few metres saturated red under the night's exposure, as blobs round the exhaust, red bushes, banks and spectators, and a red frame. Knees: surfaces `REAR_KNEE` 0.004 (`night.wgsl`, lamps' units), dust and snow `AIR_REAR_KNEE` 0.003 (`effects.wgsl`), air `BEAM_REAR_KNEE` 0.00005 (`light_shafts.wgsl`); far surfaces keep the 1/d^2 falloff. Maps 450 and 369 flyby 22:00 to 22:15 and 466 chase 23:00 now show small lamps and a faint wash; headlights unchanged, map 126 night chase frames 0 and 2 MAE 0.0004 to 0.0005 of full scale.

## Damage visuals

`damage.rs`, `damage_parts.rs`, `dirt.rs`, `crack_decal.rs`. Inputs: `DMG1` live, `--damage`/`--crack` by hand.

- Deformation (`UpdateDeformation` HH 0x10003b80): per subtype-3 panel `w_i = min(100, sum w * damage_j * 100)`
  over the 64 zone lists; `pos += delta_i * w_i`; normals kept; vertex alpha `w * 0.01` is the scrape amount. Bit
  for bit with the original (`tests/damage_data.rs`). Interior panels without a morph follow their twin (ours).
- Cracks (`CHH_Car::OnCarDamage` 0x10016d50): code 3 the front window, code 4 a side window or (index 4) both rear
  halves; slot limits 2, 1, 1, 1, 1, 2; UVs per `ProjectUVs` 0x100165d0 (`damage::crack_uv`). The texture is ours,
  a generated decal at `WS_Broken.dds`'s star positions.
- Detached parts: moved rigidly by the `DET1` body pose (ours; the game's motion after `OnDetachFromParent` is
  untraced).
- Dirt: each tyre payload adds `mass * 1e-5`, capped 0.9 (`CCarShaderManager` HH 0x10001420); `CarPaint` lerps by
  the `Dirt` map. Ours: the colour, tyre dirt by `TireDirt` (never made in 1.02), side and rear glass grime (patches
  of a value noise, at most 55 %, the wet mud keeping 40 % of the reflection).
- Dirt by surface (ours, `particles`, `dirt::CarDirt::look`): the level from the mud and snow thrown, 0.3 of the
  water and 0.01 of the rest (dry tarmac; 0.1 still filled a car to 0.79 in 98 s of clean tarmac, 580's 208 and
  417's Fiesta, 2026-10-28), so dry tarmac stays near clean; colour from 0.85 of the stage's dust (dry) to
  the game's mud tinted by that dust (wet, keeping 35 % of the reflection), to packed snow (0.8, 0.82, 0.85) by the
  snow's share; dirt maps' alpha raised to its square root (the Yaris's `Dirt` alpha is 0.2 at its 90th percentile,
  the Fiesta Rally2's 0.6); none on the cabin's interior set (with `cabin`). Snow shots were "clean" because the game's
  brown on a weak mask reads as shading, not because of a low level. Cost within noise (track-64_O chase, 9.06 vs 9.05
  ms); additions off byte for byte (2026-10-27).
- Wheel and mud flap spatter (ours, `particles`, `dirt::spatter`): parts named `e_wheel*` and `e_mudflap*` take the
  dirt colour as a film (0.3) plus value-noise spots (0.35) 8 texels across, by the level. By name, as the classes vary
  (208 Rally4 wheels are `CarPlastic`/`CarRim`, its body partly `CarTire`) and 35 of 104 RSF `TireDirt` maps are
  empty; spots on the uv alone came out camouflage-sized per car. No brake dust: the rim is in the wheel's mesh.

## Particles

`particles/` throws the rally's `PS_*.ini` (`rbr_formats::pss`); sprites from `particles.dat`. `--no-particles`.

| File | 1.02 source |
| --- | --- |
| `tyres.rs` | `E82F_BlkD0::Emit` 0x5ddba0 (`rate * frame dt` a payload; we use 1/60 s), splash tables of `E82F_Emit::Run` 0x5de140 |
| `wave.rs` | `E84Wave` sheets, `CPC_Wave::Vf04` 0x5f05e0 |
| `car.rs` | `EmitDebris` 0x5df5f0, `CSigEngineFire`, steam, `EmitExhaust` 0x5dea10 |
| `damping.rs` | bushes, `ScanEmit` 0x5dd460 |
| `rain.rs` | `CEmitterRain` (`Init` 0x5daa90, `Step` 0x5da450) |
| `weather.rs` | `Misc/track_w.ini` wind |
| `track.rs` | `CTrackEmittersManager` (`M5e9490` 0x5e9490): never switched on in 1.02 |
| `ground.rs` | playback's tyre values from poses, the fallback without a sidecar |

`CSigTireParticles` payload: wheel +0, contact edges +0x10/+0x20, velocity +0x50, rubber +0x80, smoke +0x84, mass
+0x88, depth +0x90, dirt +0x94, splash +0x98, surfaces +0xa0/+0xa4, blend +0xa8, contact +0xac. Ours: the step,
pools 4x the file's, sparks (1.02 throws none), no 3D stones (never loaded in 1.02). Open: camera-speed modulation,
the wave's vertex alpha and scroll.

Wheel spray (`particles/wheel_spray/`, ours, `--enhance particles`): replaces the tyre handlers, fake smoke, splash
and wave with dust and water only (no stones, user decision 2026-10-20). Inputs: surface kind, wetness and looseness,
slip, car speed, standing water. Dry loose ground trails dust in the region's colour, a plume lingering 6.5 s; snow
trails powder; wet ground (damp throws none) trails mist and streaked drops off the tread's back; standing water
splashes, with a burst on entering it. Sprites drawn in code. The splash payload +0x98 stayed 0 in every drive, so
1.02's own splash never shows on a wet stage.
- Dust light and count (2026-10-27): by day with the sky's tables the soft particles take the stage's light
  (`day_sky`, `day_sun`, linear albedo; `fs_soft`), not the game's ambient through their tone curve, which left dust
  darker than the road it came off (64_E heavy fog 06:45: dust 61,49,35 against road 77,60,52); the dust in the air is
  paler (0.35 of the way to its grey, x1.1); half the dust (60 a second a wheel) at 0.45/0.65 alpha in place of
  0.3/0.45 looks the same and costs less (64_O fog fly-by 1080p, particles 5.76 to 4.11 ms). Night unchanged; additions
  off byte for byte. The reference shots' dust stages (532, 462) are not on the Mac's install, so their walls were
  checked on 64 and 52 only.
- Dust settles, mud does not mist (2026-10-27): dust no longer rises (was 0.3 m/s), lives 4.5 s (was 6.5) and grows
  to 6.5 m (was 8): a bend's earlier dust hung as a ball above 462's road. Shallow gravel 0.15 loose (was 0.6),
  sprinkled tarmac 0.1 (was 0.25). Wet gravel, dirt and grass throw a quarter of a hard road's mist (`LOOSE_MIST`):
  450's corner cut is `Dirt_Hard_Wet` with puddles beside dry tarmac, and threw a wet road's white plume. In the shade
  the dust takes the sky's blue as the road does (462: road 116,139,196 without particles); left as is (no white
  balance).
- Snow powder (2026-10-27): was alive but unseen (white on snow, 0.5 peak alpha cut to about a third by
  `blended_alpha` for bright particles); now 70 a second a wheel, 0.4/1.0 alpha, 0.8 to 7 m, 3.5 s, rising 0.15: a
  plume that veils the ruts and trails off the wheels. Particles pass +0.9 ms at 1080p (208 Sturec chase 0.95 to 1.88,
  Escort fly-by 0.84 to 1.68); at 45 a second it was too thin.
- Wet spray (2026-10-28): fully wet ground has pools of its own (`WetMist`, `MudMist`, `MudDrops`; the old `Mist` stays
  the damp haze, so dry weather's damp gravel is byte-identical: any change to a pool it emits into moves the shared
  RNG and the dust). Road: grey (0.66) mist at 1.5x, drops 0.55 alpha, 14 mm wide streaked 0.04 s, colours in an
  overcast's grey (they were 0.9 white, the glitter). Loose ground: mist 0.4 of the road's and drops 25 mm streaked
  0.03 s, in the airborne dust colour x0.6 (`MUD_SHADE`), friction 2 so they fall near. The Clio fly-by's white slab
  (x1260-1540, y600-680) was dense bright wet mist of loose ground, now dim brown; no hard-edged box was found (its
  left edge is the car's wheel). Effects pass 1080p: Arbroz chase 0.87 to 1.13 ms, Sipirkakim chase 0.68 to 0.69.
- Wet spray sheet (2026-10-28, target an AI-enhanced still): the road's mist was an opaque white wall; `WetMist` alpha 0.3/0.4 to 0.16/0.22, `SplashMist` peak 0.6 to 0.35, so the guardrail and trees show through; road `Drops` 12 to 18 mm, alpha 0.4 to 0.6, streak 0.03 to 0.06 s, `Splash` alpha 0.3 to 0.4, streak 0.02 to 0.04 s: sheets of streaked drops fan off the tyre. Luceram fly-by, fog chase, cockpit change; dust, snow, night, gravel rain byte-identical; 450's wet-surface corner in dry weather changes too (its wet tarmac patch throws these pools).
- Mud plume (2026-10-28): `MudMist` is lumpy and low: size variance 2.5 (`WetMist` 1.5, was 0.5), born at the contact
  patch (release angle 0 to 0.2), up 0.2 to 0.9 m/s sideways and back, falling 0.4 m/s^2, alpha peak 0.4 at 12 % then
  fading, 0.5 to 2.2 m. The Clio fly-by's plume no longer hangs as a flat slab. Effects pass Arbroz 1.09, gc8 0.62 ms.
- Wet spray round 2 (2026-10-28): wet gravel/dirt test frames are stage 21 chase/fly-by (`--at 5600`/`5700`: a wheel in a puddle). `MudDrops` are dark clods (`CLOD_SHADE`, the ground colour squared; the airborne tint came out grey), 25 mm, 0.9 alpha, 0.35 s, streaked 0.015 s, the road's count (`MUD_DROP_GAIN` 1), thrown back and out (0.2 to 1.2 m/s, up to 0.8) from the whole tread keeping 0.85 of the car's speed: a fan behind each wheel (2.5x, 0.55 s and 2 m/s out covered the pale wet road behind the car in dark dashes and streaked across the chase camera). `Splash` is a sheet: 2500/s, 35 mm streaks (70 mm read as blobs), out 2 to 6 and up 1.5 to 5.5 m/s, plus a white `SplashMist` (300/s); the 2x splash term left `fine`. `WetMist` hangs: half the puffs (`WET_MIST_SHARE`), 1 to 3.8 m, 0.22 peak, 3.2 s, rising 0.3, launch height squared for lumps, size variance 2, darker grey (0.5): the Arbroz 131 fly-by's white slab is a translucent lumpy plume (at 5 m and 4 s it veiled the trees). Dry, damp, dust and powder byte-identical. Effects pass 1080p: 41O chase 1.18 to 0.98 ms, 52O chase 0.67 to 0.73 ms.
- Wet spray look (2026-10-28, vs an AI-enhanced still of the rain fly-by): the road's plume (`WetMist`, `SplashMist`) uses a holed sprite (`sprites::WISP`: noise cut to clear gaps, so overlapping puffs leave tendrils and the scenery shows through), fewer and denser: `WET_MIST_SHARE` 0.5 to 0.3, `SPLASH_MIST_RATE` 300 to 150 at 0.5 to 0.6 alpha. `WetMist` starts at the tyre: 0.3 alpha and paler (0.66,0.69,0.72) young, 0.4 at 10 % of its life, then grey (0.5) and gone (it was 0 young, peak at 20 %, the densest part about 5 m behind the car). Billboards of the holed sprite are marked `additive` -2 and fade in over twice the depth with a smoothstep against the ground (`WISP_SOFT` 2), so the base lifts off the road; other wisps unchanged (the same fade on all of them moved dry shots). The white feathers at the wheel were the water drops and splash streaks: water colour 0.72,0.76,0.8 to 0.5,0.53,0.57, `Drops` 0.55 to 0.4 alpha, 14 to 12 mm, streaked 0.04 to 0.03 s, `Splash` 0.6 to 0.3 alpha, 35 to 20 mm, streaked 0.03 to 0.02 s. Effects pass 1080p: Luceram fly-by 2.28 to 1.63 ms, cockpit 1.28 to 1.00, Sipirkakim chase and stage 21 chase within noise. Dry, dust, powder, snow and night shots byte-identical; damp shots change where the wheels cross wet patches (spray is keyed on the surface's wetness, not the weather: Gabiria damp chase, 129 of 2247 wheel frames on wet tarmac), by decision the same as in the rain.
- Dust plume (2026-10-28, target an AI-enhanced still of the Gabiria fly-by): dry dust is its own sprite (`sprites::DUST`, billboards marked -3: cauliflower ridge noise, darker in the folds and underneath) lit in `fs_soft` with a sunlit rim (`DUST_RIM` 2) and a self-shadowed core and underside (`DUST_SHADOW` 0.5, `DUST_BOUNCE` 0.6; wisps and puffs unchanged). Young dust is dark tan (`DUST_YOUNG` 0.95 of the airborne colour, 0.7 alpha) and thins and lightens with age; 3.6 s (was 4.5), 0.8 to 5.6 m (was 1 to 6.5), rate 120 (was 60), kicked up by a squared draw (0.3 to 2 m/s) so there is no flat top, rise 0.15. Damp ground throws an eighth of dry dust (`bound` = dry cubed, was squared). New `Debris` pool: streaked dark stones off the tread (110/s, 18 mm, 0.006 s streak). Rain, fog, snow shots and 580's chase RMSE 0; dry gravel shots change by design.
- Dust plume round 2 (2026-10-28): fewer, crisper puffs so structure survives overlap: `DUST_RATE` 120 to 40, sprite edge `smooth(0.38, 0.68)` over ridge noise, shade 0.6 to 1.2 (folds darker), sizes 0.8 to 1.7 to 4.2 m, alpha 0.7 young, 0.4 mid, `DUST_PALE` 1.1 tan; sun term x`DUST_SUN` 1.5, `DUST_SHADOW` 0.7, `DUST_BOUNCE` 0.8. `Debris` 150/s, 35 mm, 0.01 s streak, low (0.2 to 1.4 m/s up) and out (0.5 to 4.5) from the lower tread. Wet shots RMSE 0.
- Dust close up (2026-10-29, chase views read as opaque white cotton balls in shade): within `DUST_CLOSE` 3 to 9 m of the eye `fs_soft` gives a dust puff no outline of its own: opacity is a radial falloff (`DUST_CLOSE_FALL` 1.5 power of 1 - r^2, flat at the rim) times the blurred sprite's lumps (`DUST_CLOSE_LUMP` 0.35 without) at peak `DUST_CLOSE_PEAK` 0.5, erosion `DUST_CLOSE_EROSION` 0.5 cuts the thin outer part into wisps (was a thresholded sprite edge). Sky light on dust is x`DUST_SKY` 0.4 to 1 by `sun_reach` (a canopy shades the sky as well as the sun) and x`DUST_CLOSE_SKY` 0.6 close up; the sun term is unchanged. Golden Odry chase's dust is wholly in shade (`sun_reach` 0 on every puff, checked by painting it), and its luma was 135 to 185 against the lit road's 100 to 120; now about 125 to 145. gab-lit RMSE 0.003 and day Gabiria 0.0004 against before the round; additions off byte-identical. The tan blocks in the fog Odry chase are the game's own stones, as on main.
- Dust sun shafts (2026-10-29, fog 532 fly-by: trees' sun shadows printed hard blocky patches into the plume): a dust puff takes the sun's visibility averaged over its depth along the view ray (`DUST_TAPS` 7 taps over `DUST_DEPTH` 1 radius either side) with a wider filter (`DUST_SPREAD` 2.5 texels, 4 taps each; `dust_sun_reach` in `effects.wgsl`), so tree shadows are soft shafts; other particles keep `sun_reach`. Fog 532 fly-by RMSE 0.022, fog Odry chase 0.028 (its tan blocks on the road were this print: now dappled haze), Sipirkakim 0.007, fiesta chase 0.0016, gab-lit 0.003, Odry GC8 chase 0.0003, day Gabiria and 621 fly-by 0; effects pass in the 532 fly-by 2.6 to 3.7 ms. Tried and dropped: peak alpha 0.9/0.55 to 0.7/0.4 makes the 532 plume translucent but thins gab-lit's and Odry's; `DUST_SUN` 2.2 to 1.6 and a 0.75 far veil dim gab-lit's plume far more than the 532 one, so the 532 plume stays a bright tan body (its forest is dark, the sunlit dust is the brightest thing in frame).
Soft-particle cost (2026-10-26, RX 9060 XT): the effects pass is fill-bound. In a dust cloud (Sipirkakim II `--at 15000`: 926 quads of 2 to 3 m round the eye, about 400 screens of summed coverage) it cost 35 ms at 1440p, nearly all `fs_soft`. Fixes, frames within 2/255 of before: the lamps' code is out of the day's pipeline (`LAMPS` override in `effects.wgsl`, as `shader.wgsl`'s; its registers cost 11 ms with no lamp lit), fragments under 1/1000 alpha are discarded (the sprite's rim, eroded dust), and the depth is read only after the erosion test. Effects pass, median of 3 interleaved runs, 1440p / 4K: dusty cockpit 35.5 to 9.3 / 81 to 28 ms, chase 59.7 to 16.1 / 137 to 39, wet heavy rain cockpit 68 to 15 / 154 to 35, Falstone bumper 18.7 to 3.7 / 42 to 11 (GPU clocks bimodal; the slow mode adds up to 10 ms). Rain streaks and mist cost under 1 ms of that. `--frames` is steady state: the particles are built once, not per frame; the long 'encoding' of a drawn-only frame is the CPU waiting for the GPU (2 ms with the GPU idle).
Wet mist (2026-10-26): it had never shown (edge erosion ate the 0.28-alpha mist; the drops alone drew). Now a wisp:
erosion 0.08, lamps' glow at 0.2, faded by size from the eye. Grey-white 0.32 alpha, 2.5 s, up to 3.4 m wide, 120/s
a wheel at 28 m/s, growing from 5 m/s. Damp: a `Haze` pool at a third of the rate, 1.2 s, no drops. Additions off
unchanged; wet chase within noise of main.

## Windscreen and wipers

`glass/`, ours. Drops, flakes, bugs and mud live in the windscreen's chart (`i_window_f`: mean plane plus a height
grid), drawn in the cockpit and on `e_window_f` outside. Inputs: precipitation, speed, tyre spray and surface
(`dirt::thrown`), `misc/Part2D.ini` bug data. Drops are lenses over the resolved frame (`Renderer::draw_over`); a wet
film blurs it. Wipers sweep 1.3823 rad (left) and 1.3195 rad (right), 0.4 s each way (`E88Wiper::InitTable`
0x5f8a50); switch off, intermittent, continuous, auto (`--wipers`, key I). Camera dirt on outside views is not
ported.
- Look (ours): rain radii 1 to 4 mm, cubed for mostly small drops; a drop's rim and highlight scale in from 1.5 to 5
  px radius; a sky crescent on its lower rim by day; at night its rim catches only its own lens's light. The film
  blurs streaked up the glass, longer with speed. Mud specks are opaque (the HDR sky through a 5 % gap read as grey);
  with `cabin`, seen from inside (kind 5) a speck's thin rim lets 3 to 45 % of the light behind through in the mud's
  hue, its core dark (2026-10-27; glass pass 0.14 ms before and after, off frames byte-identical).
  Cost at 1080p in heavy rain: about 0.8 ms. A reflection of the dashboard in the windscreen was tried and dropped
  (user's call).
- Beads (ours, 2026-10-26): the film's cells draw the small drops too many to be drops of their own: 5x5 squares a
  cell, each a bead at a hashed place when a hash is under the film (about 1 to 3 mm), a lens like the drops'. The film
  builds to the rain's amount, less with the airflow, and the wipers clear the beads with it (drops alone covered 1.3%
  to 2.1% of the parked glass after 12 s; more drops cost about 5 ms a step at 100 km/h). Shows with additions off too
  (user-approved); cockpit heavy 13.1 ms before and after.
- Side windows (ours): each side window its own glass (drops, trails, film, mud; no wipers, no bugs), drawn on its
  exterior pane. 0.3 of the windscreen's rain and 0.1 of its spray, landing low; the film streaks back with the airflow;
  pins 3 times harder.

## Skid marks

`skidmarks.rs` (pure), `skidmarks_pass.rs`. Records from the `PS_*.ini` "Skidmarks" section (material, strip 0-7 of
`skidmarks.dds`, RGB, size, flush threshold, add, rubber/depth/dirt terms) and its aliases; `_low` files have none.
`EmitTire` 0x5e3050: with contact and a record, `k = add + sum((term + reduction) * multiplier)`, clamped 1, is the
alpha; per wheel `acc += frame dt`, within the flush threshold the open mark extends, else a new one starts; edges
lifted 0.03. Four rings of 256 marks (`0x7d6d48`); the 32 oldest fade by `(i + 1) / 32`.

Measured (ours): the payload edges are 0.20 m apart; records scale them by 2.0-2.2 and the strips fill ~85 %, so 1.02
marks show ~37 cm, laid along the axle. The payload's velocity +0x50 is not the patch's; rubber is 0-1 in the game's
payloads, a slip speed in m/s in playback's from poses. Snow drives run mostly on `Snowwall_Bottom` (0x58).

`--enhance marks` (ours) replaces them: width 0.21 m loose/snow, 0.235 m tarmac, swept across the travel. Gravel/dirt:
rut with tread print, light berms; tarmac only when sliding; snow groove; grass flattened; wet darker, a sheen at
grazing views. Sections every 1 m, 800 m a wheel, the last 250 m fading; marks fade 90-220 m from the eye. A seek
replays only the 4 s warm-up, so captures show short trails.

## Ghost

`ghost.rs`, `rbr_gameplay::ghost`, `rbr_formats::ghost`. The car's `ghost_*` mesh in one colour, `Ambient` of
`Cars/GhostCarSettings.ini`, alpha `min(d^2 / FadeDistance, MaxOpacity)` (`CShaderGhostCar::SetObject` HH
0x1007f000). Pose: the model root, at race time = step counter less the 160 settle steps, plus the live drive's
outside-assistance penalties (HUD2) from the step each was seen on, as the game's ghost clock takes them. Ours: `--best-ghost`.
RSF ghosts (`.gch`) are not supported (user decision).

## Characters

`characters/`: `.lbs` `(2,14)` items in `(2,15)` random groups, Lua 4.0 scripts (`script.rs`), `.sga` clips by
element-wise lerp, CPU skinning; shown and scripted while their `(2,10)` cell meets the frustum. Driveline triggers
fire at `distance - lead * v < x` (`x` = `CCar+0x17c`). Generators: the exe's LCG and msvcr71 `rand()`, `--seed`.
Scripted events (flip spectators, rescue helicopter, bird strike): `characters/scripted.rs`. In-car crew (`crew.rs`,
`incar.rs`): seated at `D_driver_use`/`D_codriver_use`, steering scrubs the driver clip, gear and handbrake moves from
`OnCarControl`'s ring with `Misc/LM_Driver.ini` (live: from the car's control, CTL1), arm IK (`CLM_IKChain::Solve`),
`CSigIncarShake` lean; chase views only, as the High renderer. Tests: `tests/characters_data.rs`, `scripted_data.rs`,
`crew_data.rs`.

## Fences and dials

`fences/`: `.fnc` lines as `CFenceHandler` cloth (Verlet, 0.96 damping, posts break past 0.5 m, tapes tear past
0.2 m), pushed by the car's `c_e_body`. With the own light (ours) the lines carry their normals (the cloth's smooth
per span, the posts round; `fences::FENCE` in `specular.x`): a span is lit on the side seen and lets 0.4 of the far
side's sun through, its ambient taken as both sides' (one `own_light`), its bounce from the ground maps
(`ground_bounce`; the bake gave 3 cm posts almost none). Fence meshes skip `on_foliage` and `with_plants` (the 45_O
net was a "plant", glowing in patches). Without the own light they stay facing up; additions off equal on 64_O,
45_O, 61_M but 1 to 2 pixels by 1 (shader codegen: gone with `is_fence` false; two `shade_stage` call sites gave
more). Cost: +0.17 ms scene at 1440p with a net filling the view. A torn line's rewrite has occlusion 0 (no bake),
as before. `dials/` (ours): an RSF car's needles and displays from RBRHUD's gauges, drawn into the dial textures
from the HUD records.

## Enhancements

`renderer::Enhance`, `--enhance a,b` or `all`; each off (but for the always-on HDR and TAA) leaves the frame as the
game draws it. The launcher enables all of them. Removed: `lens`; `ao`, `msaa`, `taa`, `wind` (user decision
2026-10-19); `hdr` (2026-10-26: float targets, bloom, exposure and AgX are always on). Older settings and names are
not read (user decision). The launcher's render scale is 100 % by default. The default frame was
byte-identical before and after (chase, cockpit, heavy rain, 1920x1080).

| Name | What |
| --- | --- |
| `anisotropy` | 16x anisotropic stage samplers |
| `cabin` | cockpit lit by its baked sky share, sun only through glass, interior set clear of the stage fog; with the day's light the interior set takes it as the lit draws outside do, so the cabin sits stops below the sunlit outside (45O Xsara and RSF Evo IX, dry, wet, fog; scene pass within noise; additions off byte-identical, 2026-10-25); the car's glass drawn from both sides, so the far window seen through the near one tints too (the game culls it) |
| `contact` | soft contact shadow in place of the decal |
| `decals` | soft ground decals (alpha < 230/255, triangles within 32° of level: the skid marks of RSF 129, 369, 370 and 129's sand patches; no retail draw) alpha blended after the stage instead of cut at 60/255 (`decals.rs`). The factory gives object items (flags `0x15`) alpha blend `SRCALPHA`/`INVSRCALPHA`, test > 0 (`HH_TM2_12E0_Factory` 0x100512e0); the capture at default quality shows only the cut, its switch untraced. Upright soft cut-outs (wire fences, phone lines) keep the cut: blended, the wire mesh vanishes |
| `asphalt` | the paved roads (`Vertex::wet` 1: tarmac by the `.col` surface, as `gravel`) take the asphalt scan the same way (Poly Haven Asphalt Track, CC0, 1024 packed, layer 1, every 2 m; the same strengths). Launcher "Enhanced asphalt", default on |
| `gravel` | the loose roads (`Vertex::wet` 0.5: gravel and dirt by the `.col` surface, `puddles::mark_roads`; dry too, `Stage::mark_dry_roads`) take a scanned gravel surface (Poly Haven Gravel Ground 01, CC0, 1024 packed, embedded; `renderer/road_scan.rs` layer 0) tiled flat every 3 m: its brightness over its mean scales the road's colour by 0.5, its normals lit by the sun against its mean normal by 0.35 (faded out below about 6 to 20 degrees of sun), its hollows darker by 0.24; the road's mean brightness and colour stay (below). Launcher "Enhanced gravel", default on |
| `lighting` | our lighting in place of the game's look (2026-10-27: one flag for the former `sky`, `sun`, `occlusion` and `clouds`; the half-on looks, the 2D cloud layer and its cloud-face lighting went): the physically based sky with aerial perspective, height fog, light shafts and volumetric clouds with their shadows; sun shadow cascades and sunlit foliage in place of the baked light maps; ambient occlusion and bounced light computed at load; reflection probes; the day's exposure. Details below |
| ~~`hdr`~~ (always on) | float targets, bloom, histogram exposure, AgX (`post.rs`); the exposure's limits follow the clock's light, heavy rain or snow a fifth darker, heavy cloud 15% more saturated. Local exposure: where the bloom blur is over 2 stops above middle grey, 0.6 off, so the night beam keeps the road's texture (45O cockpit 23:00: 24% clipped, now none); post pass 2.91-2.94 ms; additions off byte-identical (2026-10-25). A 97th-percentile cap was tried and dropped (blacked out everything around the beam). Daytime exposure under our sun lighting (`Enhance::lighting`, 2026-10-27): the exposure's scale is lowered by `post::SUN_EXPOSURE` (1.5) stops times the sun's share of the day's light on open ground (`DayLight::sun_share`: 0.89 at a clear noon, near 0 under a closed cloud, faded out for sun elevations of 6 to 20 degrees), so overcast, snow, twilight and night keep the meter's exposure (byte-identical against the previous commit on overcast, snow, dusk, night, wet 41M). The metered mean of a forest scene put sunlit grass at about 207 green of 255 (41M chase noon); AgX's shoulder takes only about 12 to 15 levels off per stop there (lit grass green 207 to 198 at 1 stop, 184 at 1.5, 169 at 2; road 190 to 154, 137, 121). At 1.5: 41M chase noon lit grass (188 207 141) to (166 184 124), road 190 to 137; 45O noon (partly cloudy) grass green 165 to 121, road 178 to 115. The sunlit 41M grass stays above a photo's 120 to 140 green; going further crushes the shadows (31 at 1.5 stops, 24 at 2). Additions off byte-identical. AgX contrast follows the adapting light (Stevens effect): day p95/p99 up ~10 levels, p5 down ~6; night frames within a level; post pass 3.34 to 3.35 ms; additions off byte-identical |
| `marks` | our tyre marks in place of the skid marks (Skid marks; launcher default off) |
| `particles` | GPU rain and snow, rain rings, lit soft particles, chimney smoke, wheel spray. The rain or snow is decided once, in the viewer's `scene::load` (2026-10-26): `--precipitation`, else with `particles` ours by the road (`Rain::of_weather`: wet by the section's preset, else moderate; none on damp or dry roads), else the game's by the preset (`Precipitation::of`, additions off); the sky (`Atmosphere::rain`), the particles, the glass and the live sound (`start_sound`, in place of the launcher's `--rain` from the season row) all take it (`Scene::precipitation`). The wet weather's rain follows the section's preset (0.5/1.0/0.75; `--precipitation` overrides) and falls from an overcast sky unless `--clouds` is given (2026-10-26). Rings only in standing water, gone past 60 m over the view's cosine (2026-10-25). Rain looks (2026-10-26): 196000 faint streaks at full rain in place of 56000 bright needles, plus a fullscreen rain mist (cover per metre of depth, density varying 30% in 90 m patches, a fifth of the lamps' light at night); streaks harder with the amount; no drop falls inside the cabin; zero without rain or `particles`, so additions off stay byte-identical. Streak look (2026-10-28): the screenshot's `--flyby`/`--camera` eye stood still but took the car's velocity for the eye's (~25 m/s), slanting streaks ~40 degrees; now zero for those cameras (car-riding views keep it). `SHUTTER` 1/30 to 1/60 s and `RAIN_OPACITY` 0.9 to 0.6: shorter, fainter streaks (snow's too); dry and damp shots byte-identical. About +0.5 to 1 ms on 45O chase wet, the denser drops another +1 ms on heavy; 30 chase frames without flicker |
| `snow` | the snow (the ground over a `Snow*` `.col` surface, `SnowOnGravel_*`, `SnowOnIce_*`, `Snowwall*`, that takes no film: `Vertex::wet` `puddles::SNOW_MARK` -1, below every film share, so the wet road never takes it) takes the snow scan the same way (Poly Haven Snow 02, CC0, 1024 packed, layer 2, every 2 m; the same strengths, the factor no darker than 0.88). Launcher "Enhanced snow", default on |
| `ground` | the off-road ground (the near-LOD ground draws' triangles lying on a collision surface that is neither road nor snow, within 0.6 m of it: `puddles::mark_ground`, `Vertex::edge` -1, so also dry; `Stage::mark_ground`) takes a scanned grass-and-earth ground (Poly Haven Grass Ground, CC0, 1024 packed, layers 3 (colour, height) and 4 (normal) of the road scans; `ground_scan` in `shader.wgsl`) triplanar by the face's normal at 1.7 m and a 5.1 m tiling turned 30 degrees: colour over its mean as patches of grass and bare earth (`GROUND_DETAIL` 1.8 luma, `GROUND_CHROMA` 0.8) and fine grain over its softer self (`GROUND_GRAIN` 2.2), sun-lit normals (`GROUND_RELIEF` 0.5) and darker hollows (0.3), gradients scaled `GROUND_SHARP` 0.6; bright grey ground (snow, pale rock) takes only brightness, no darker than 0.88. The stage's texture is read `GROUND_BLUR` 0.75 levels blurrier only where it is magnified (`GROUND_MAGNIFIED` 0.4 to 1 texel a pixel: its smear; a crisp texture, Yaris 621, keeps its own detail) and its own high pass (`terrain_detail`) keeps `GROUND_KEEP` 0.7. Round 2 (2026-10-29): `ground_patches` adds earth and grass in patches 2 to 6 m across (value noise, more earth on a slope; mean-preserving tints) and stones of 0.3 and 0.9 m cells; the triplanar blend is by the normal to the eighth power (the fourth averaged two projections on a slope); round 1's global 1.5-level blur had made the Yaris slope and the hill soft. Launcher "Enhanced ground", default on |
| `water` | water flowing by its scroll `c85`/`c86`, fading into banks |
| `wet` | the wet road, in wet and damp weather: a film and puddles over the ground whose `.col` surface takes them (tarmac 1, gravel and dirt 0.5), retail and RSF alike (user decision 2026-10-25). Puddles: random ellipses per 4 m square (`ground.rs` of the wheel spray mirrors the shape consts), larger in hollows, which also hold water of their own (`PUDDLE_LOW`), bounded by grade; they keep the road's normal, take the whole of a half-film road's share (`PUDDLE_ROAD`; the film keeps 0.5), reflect 0.4 to 0.9 as sharp mirrors (screen-space rays at the water's gloss, the cube at its sharp mip; a flat sky blend painted pale discs) and fray their outlines with fine noise (`PUDDLE_RIM`). Before (2026-10-28) heavy rain on 52O/41O showed them at 0.2 to 0.35 reflectance and under half strength on gravel, invisible against the grey road though the film and lowness were all there; scene pass +0.03 ms, screen reflections +0.07 ms at 1080p, 52O chase. Screen-space reflections on the wet roads: +12 to 17 ms at 1080p; film alone +0.9 to 1.7 ms; load +0.2 s. Wet film (2026-10-28): the car did not show in the wet road because the screen-space rays missed it, not for lack of reflectance: a step that crossed the car's rear ended too deep behind it to pass the thickness test (the ray travels along the view), the hit faded over the screen's last 15% where the car sits in a chase frame, and the film's cone (`GLOSS` 0.12, `STRETCH` 4) smeared what was left. The film now halves back to any crossing and `reach` judges it (`film` in `trace`), fades over 3% (`FILM_EDGE`), and has its own cone (`GLOSS.x` 0.03, `ROUGHNESS.x` 0.01, `STRETCH.x` 2; water and puddles keep theirs). Fresnel toward `FILM_MOST` 0.95 with `FILM_BEND` 3 (puddles keep 0.9 and 5) and `FILM_SHEET` 0.5 of the film's patchiness evened out: the far road takes the sky, the near road stays dark. 41O chase screen reflections 1.32 to 1.50 ms at 1080p. Loose roads (2026-10-28): the half-film road's film was a pale even sheet (stones gone, as concrete); its reflection now falls with the film's share (`FILM_LOOSE` 2: share squared, puddles keep theirs) and `FILM_SHEET` evens out only by that share, so the stones show through and puddles stand out as mirrors (r1, r3); tarmac and dry byte-identical, 52O/21O chase screen reflections 1.46/1.67 to 1.14/1.37 ms, scene unchanged. The rings were never on the film (`standing` is 0 below 0.6, the loose film tops at 0.5): they were in the puddles, which on gravel were lost in the pale sheet. Damp, dry and additions off byte-identical. Damp is a drying road: no puddles, rings or screen-space reflections, the film 0.4 of the wet one's, the ground soaked by half. Road at 13:00 (mean of 255): 41M dry 101, damp 68, wet 59; 460M 147 to 131 (wet 86); 45O 140 to 118 (wet 113). 41M +0.9 ms, 45O within noise; wet and additions off unchanged byte for byte (2026-10-26). Wet road edge (2026-10-28): `Vertex::edge`, each film vertex's distance (up to 8 m) to the nearest edge a film triangle shares with a ground triangle without film (by position, over all meshes); in wet weather the reflection (in `puddled`, so the screen-space rays too) and glint fade over the outer 0.1 to 0.8 m, frayed by noise (puddles and the road's darkness stay); mean distance 1.2 m on 41M, 0.3 m on RSF Gabiria; damp and dry byte-identical. By texture (2026-10-28): a film triangle takes the share most film triangles of its texture have, as a stage may tag stretches of its tarmac as gravel (RSF Bois de Bie: 46000 `Gravel_Fine_Shallow_Wet` against 139000 `Tarmac_Coarse_Clean_Wet`, its chase stretch matte under `FILM_LOOSE`); a stretch tagged gravel with a texture of its own keeps it (Bois de Bie's fly-by); retail Fraizer Wells unchanged byte for byte, damp Gabiria's road slightly (RMSE 0.0003). Wet road look (2026-10-28, tarmac first; target an AI-enhanced still): the film alone, not damp or puddles, is darker by `FILM_DARK` 0.7 (on top of the soak) and its sky reflection scaled by `FILM_SKY` 0.6, its reflection takes the asphalt's grain (`film_relief`: 3 and 9 cm noise, `FILM_GRAIN` 0.6, so the texture shows through), its normal is tilted by microrelief along the view (`FILM_TILT` 0.05 along, 0.01 across: streaked reflections) and by rain's impacts over the whole film (`FILM_POCK_*`: 12 cm cells, dimples and a flash of reflection), all fading to the mean between 4 and 25 m; the film's cone is sharper and stretched further down the screen (`GLOSS.x` 0.03 to 0.01, `ROUGHNESS.x` 0.01 to 0.012, `STRETCH.x` 2 to 12). Road puddles (not water) take their own cone `PUDDLE_CONE` (gloss 0.012, roughness 0.006, stretch 2), `PUDDLE_DARK` 0.3 to 0.15 and `PUDDLE_REFLECT` 0.4 to 0.55: the reflected car reads clearly. `FILM_POCK_FLASH` 0.2 (1.0 read as sparkle). A 0.12 tilt painted white sky blobs over the near road and was dropped. Gravel nearly unchanged; dry, snow, night byte-identical. Flyby 1080p scene 1.72 to 1.80 ms, screen reflections 1.58 to 1.85 ms; cockpit within noise. Tarmac's even film (2026-10-28, user decision: the target's evenly wet road reads better than puddles): where the road's film share is 1 (`TARMAC_SHARE`) there are no random puddles (`puddle_shape` off; the wheel spray's `puddle_cover` is gravel and dirt only), the film's patches no longer thin it (wet 1), and only the hollows hold water (`PUDDLE_LOW`, at `TARMAC_HOLLOW` 0.4 of a puddle's strength). The film is the main mirror there: `TARMAC_SKY` 0.7 (was `FILM_SKY` 0.6), `TARMAC_NEAR` 0.6 (reflection up to 1.6x within 2 to 14 m of the eye), `TARMAC_MOST` 0.55 at a grazing view (`FILM_MOST` 0.95 whitened the far road in fog), `TARMAC_DARK` 0.45 (`FILM_DARK` 0.7) and its own cone `TARMAC_CONE` (gloss 0.012, roughness 0.008, stretch 10; the film's 12 smeared the car to a blob), so the bumper and plate read in the road near the car. The wheel spray treats wet tarmac as shallow standing water (`FILM_WATER` 0.5 of a puddle's splash everywhere, `wheel_spray/mod.rs`, only in wet weather with `wet` on: damp byte-identical; the shader has no puddles there), so the splash and its mist stay. Rain rings on tarmac only in the hollows' water, at `TARMAC_RINGS` 0.4 of a puddle's; the film shows fine splash specks instead (`TARMAC_POCK_RATE` 8 a second, `REACH` 0.03 m, `FLASH` 1.2; gravel's `FILM_POCK_*` unchanged, 52O chase byte-identical). Round 2: impacts are dots (`TARMAC_DOT` 0.012 m), the normal is roughened at the grain (`TARMAC_TILT_CELL` 1.2 and 0.4 cm, `TARMAC_TILT` 0.05/0.025, fading 2.5 to 12 m) in place of the 5 and 2 cm waves, `TARMAC_GRAIN` 1.0, `TARMAC_RINGS` 0.2, and the albedo's contrast raised (`TARMAC_CONTRAST` 0.5 about `TARMAC_PIVOT` 0.056, wet weather only; mean road 68 to 71). The near road's olive cast is the green bank's reflection (reflection off: neutral 62/60/57; raw texture 123/123/120), kept. Gravel and dirt keep the film and puddles (52O chase RMSE 0.0000022), dry, damp, snow and night-dry byte-identical; Luceram flyby 1080p scene and screen reflections within the GPU's noise (2.0 to 2.9 ms bimodal both). Puddles at the edge (2026-10-28): the puddles (larger along the road's low edges) kept their whole strength up to the road mesh's border, a straight pale line along 52O's verges; they now end 0.05 to 0.2 m from the edge (`PUDDLE_EDGE`), frayed as the film is (`edge_keep`); 52O chase RMSE 0.017, dry byte-identical. Wheel tracks (2026-10-28, loose roads only, `loose` = film share about 0.5): the water lies in two tracks `RUT_AT` 1.5 m in from the road's edge (`Vertex::edge`; 52O is 4 to 5 m wide, so the tracks sit 1.5 m apart), centre wandering 1 m, half-width 0.18 to 0.5 m, broken by noise into pieces (`rut_shape`, `RUT_GAP` 0.42 to 0.5: under half of a track is water; no road direction needed), rims frayed by 7 cm stone noise (`RUT_STONES`, `RUT_FRAY` 0.9); stray puddles `PUDDLE_SHARE` 0.7 to 0.12, hollows need `LOOSE_LOW` 0.35 more depth (the left verge's pale blobs were these hollows: 0.6 removed them all), a wet margin (`RUT_MARGIN` 1.6 times wider, `PUDDLE_MARGIN` 0.7 darker), reflectance `PUDDLE_LOOSE` 0.65, and toward a rim stones break it (`PUDDLE_STONES` 0.8 where the water is shallow). Old puddles were not brighter than the target's (luma 90 to 113 against 133): they read as snow for their shape and no margin. The wheel spray takes `RUT_COVER` 0.08 in wet weather (the shader's gate noise is not mirrorable on the CPU); damp keeps its old spray. 52O chase scene 2.54 to 2.65 ms, screen reflections 1.69 to 1.74 ms; dry, damp, snow, night byte-identical, Luceram within compiler noise (RMSE 0.00015) |

Gravel scan (2026-10-28): upscaling the ground atlas further (8x, 8192) changed nothing on Sipirkakim's fly-by: the
gravel cell is 55 texels a metre unstretched (4.5 mm upscaled), and the streaks are a flat photo seen at a grazing
angle. TAA, anisotropy and the wet film off changed nothing either. Procedural stones (Voronoi, sun-lit domes) read as
cobbles or coins; a scanned surface's detail over the stage's own colour read as gravel. Poly Haven stores its normal
and height PNGs sRGB-encoded (normal x and y mean 0.72 raw), decoded to linear by the packer; with the raw values the
relief brightened the road by a sixth to a third at a low sun, so the relief is taken against the scan's mean normal
and fades out with a low sun (road mean 86.8 off, 86.5 on, Clio fly-by 4K). Parallax (24, 6 to 14, 4 to 8 steps
within 25 or 15 m) looked the same in stills; scene pass at 1440p, Clio fly-by, RX 9060 XT: off 5.04 ms, 24 steps
+1.39, 6 to 14 +0.65, 4 to 8 +0.43, none +0.08 (kept). Off, frames byte-identical to before (Clio damp, 621 dry,
Sipirkakim wet, Gabiria tarmac); on, a dry stage loads about 1 s longer (the roads marked, 621). 1024 against 2048:
the same grain at 4K (road standard deviation 19.48 against 19.45). Open: some RSF stages tag stretches of tarmac as
gravel (Gabiria's road edge takes the scan, faintly); Semetin and Sardian show no change.

Asphalt scan (2026-10-28): Poly Haven Asphalt 01, 06 and Track tried on Gabiria, Luceram and Helette; Track (fine
aggregate, its brightness varying 29% about a dark mean) gave Luceram's smooth tarmac crisp aggregate and Gabiria's
detailed texture a little grain, 01 a faint grain, 06 next to nothing. Road mean within 1.3%; on the wet road it shows
faintly under the film. Scene pass at 1440p, Luceram fly-by: off 4.57 ms, on 4.66. The asphalt scans' normals are
stored linear (mean 0.50), Gravel Ground 01's sRGB-encoded (0.72), whatever their gAMA chunk: `pack-scan.py` decodes
by the normal's mean. Both off, and gravel alone, frames byte-identical to before (Clio damp, Gabiria, Luceram wet,
621 dry); the derivatives stay `dpdx(world / tile)` per layer, as `dpdx(world) / tile` moved up to 8/255 on 1754
pixels of the Clio fly-by.

Asphalt look (2026-10-29, target an AI-enhanced still): the scratchy tarmac at Gabiria was not the stage texture's flecks
but `albedo_remap`: its cut steepens through the road's brightness (linear 0.5 to 0.7, up to 2.6 times its contrast), so
the texture's fine variation came out as bright elongated flecks (road std 18 of 119 on a flat patch, with the scan off
too). With `asphalt`, a paved road (`Vertex::wet` over 0.75 and a grey texture: `ASPHALT_GREY` saturation 0.1 to 0.18 of
the texture read `ASPHALT_BLUR` 3 levels blurrier, so the shoulders' grass and brown, which the paved mesh carries, keep
their own look and the old scan strengths `ASPHALT_SHOULDER`) takes the cut of its local mean (the texture `ASPHALT_MEAN`
6 levels blurrier) with its own variation over it, reads its texture `ASPHALT_BLUR` levels blurrier where magnified
(painted lines stay crisp by the blurred texture's brightness over its mean, `ASPHALT_LINE` 1.1 to 1.3) and gives up
the stage's high pass (`ASPHALT_KEEP` 0). The scan carries the grain: layer 1 `SCAN_DETAIL` 0.5 to 1.2, `SCAN_RELIEF`
0.35 to 0.6, `SCAN_HOLLOWS` 0.24 to 0.4. Road patch mean 119 to 97 (flat patch std 18.2 to 17.5 as fine grain), lines,
patches and shadows kept; gravel and snow shots byte-identical. A noise-broken road edge (grey threshold wandering) only
frayed the shoulder's flattening and was dropped: the crumbly edge needs the ground shader or the mesh.

Snow scan (2026-10-28): Poly Haven Snow 02 on Komarov, Peklo, Pian and Sturec. Its dark marks (footprints, debris)
read as dirt streaks on Peklo's banks, so the factor is held at 0.88 or brighter (`SCAN_FLOOR`; 0 for gravel and
asphalt, which stay byte-identical). Its normal averages 0.58 (linear, a slight tilt the mean-normal relief cancels).
Road mean within 0.3%. Scene pass at 1440p, Peklo fly-by: off 3.26 ms, on 3.34. Off, the snow stages' frames
byte-identical to before (Komarov, Peklo, Pian): the snow mark is below the film's test everywhere.

Ground scan (2026-10-28, target an AI-enhanced still of Gabiria): the hillside's blue sheen and the verge's blue-grey band were
not specular, sky ambient, fog or the film (each switched off alone: RMSE 0.004 to 0.02, the sheen stayed) but the stage's
baked vertex tint (`own_light`'s `hue`: the bake's colour over its brightest channel, `notes/formats.md`): a smooth map that is
mostly white with blue blotches (the bake's shade) and orange ones, painted for the original's sun, and the blue patches show
in our lit slopes and along the road's edge strip (with the hue at 1 the sheen and the band vanish; the strip turns tan, as the
target's dirt shoulder). With `Enhance::ground` it keeps `GROUND_HUE` 0.3 of itself on every near ground draw (road too:
the edge strip is road-marked); the road changes only by the lost blue blotches. Hill mean colour within 1 of 255 per channel
(96,100,76 to 97,98,79), as the scan keeps the stage's mean. Cost: scene pass at 1080p, Gabiria fly-by, 2.7 to 3.5 ms on, 3.0
ms off (GPU noise larger than the effect); memory +11 MB (two 1024 layers with mips), load unchanged. Ground off, every other
addition on: byte-identical (Gabiria, Komarov snow, Sipirkakim). Snow stages: banks keep reading as snow (bright grey takes
brightness only). Tried and dropped: the scan at 2048 in its own array (no visible difference from 1024 at a 1.7 m tile, 20 MB of PNG); contrast of the scan's own patches alone (0.9 detail) read as faint, hence the exponents. Open: at grazing
views the scan's detail stretches along the view (physically right; the target's crisp blades are the AI's), and the target's
brightness (146,131,98 against ours 97,98,79 on the hill) is the light's, not the ground's.

Rock keeps its hue (2026-10-29): rock, stone walls and concrete (`ground_rock`: a grey or cool stage texture, red and green
over blue under `GROUND_ROCK_AT` 0.06 to 0.14 of its brightest channel, on a steep face, facet up component under
`GROUND_STEEP` 0.6 to 0.85) take the ground scan's and the earth patches' brightness only, so cuttings and parapets keep
their own colour; a flat grey verge (gravel) keeps the tint.
Rock to grey: rock gives up `GROUND_ROCK_GREY` 0.85 of its lit colour for grey (its texture's cast and the light's tint are
mostly not its own), steepness `GROUND_STEEP` 0.75 to 0.97 (softer); `ground_scan` takes and returns the lit colour.

Calmer verge (2026-10-29): the verge's pale texels (straw, white at a grazing view) past `GROUND_PALE` 1.1 of the texture's
local mean (`GROUND_MEAN` 5 levels blurrier) are eased to the power `GROUND_PALE_FLAT` 0.3, within `GROUND_CALM` 12 to 30 m
of the eye. A dirt-and-gravel strip on the paved mesh's non-grey band was dropped: on some stages that band is the whole
grass verge and bank (580, Peugeot chase: both turned to dirt).
Far hillside (2026-10-29): from `GROUND_CLUMP_AT` 8 to 18 m out (`ground_far`) the ground texture is read
`GROUND_FAR_BLUR` 1.5 levels blurrier, its pale texels eased as the verge's, the scan's chroma and grain exponents eased by
`GROUND_FAR_CALM` 0.6 each, and `ground_clumps` adds grass clumps (value noise at 1.5, 0.7 and 0.32 m: moss green or
grey-green tufts by `GROUND_CLUMP_TINT` 0.6, gaps `GROUND_CLUMP_GAP` 0.3 darker, lumps lit along the sun by
`GROUND_CLUMP_RELIEF` 0.3). Snow stages (`puddles::mark_ground`: over one in `SNOW_STAGE` 20 ground triangles on snow,
`frame.enhance.w`) are left alone. The speckle on the hill is mostly the stage texture's own (there with the scan off).

Lighting, in pieces (all with `lighting`; each measured on its own before the merge):

- Sky: Hillaire sky, aerial perspective, height fog, light shafts; the tables are stored in display colours once per sky
  change, not per pixel (same frame within 1/255, 2026-10-20); the stage takes haze and mist by distance, the backdrop
  by its distance easing to full by 3 km; under a clear sky the ground haze hides 70% no nearer than 7.7, 4.6 or 2.3 km
  (Koschmieder visibility 25, 15, 7.5 km; 2026-10-27), the day's mist 95% at 33 km; the sun's light in the air
  (Atmosphere and sky).
- Probes, occlusion, bounce, contact: reflection probes (`renderer/probes.rs`): the stage drawn at load into 64-texel
  float cubes (7 levels) along the driveline, 1 m up, 40 m apart or up to 256, without the car, characters, props, lamps
  and cascades; a 32 m grid names each place's nearest probe. They replace both per-frame cubes, drawn after the first
  frame (headless 117 to 256 probes in 0.44 to 1.43 s, up to 67 MB). 41M 1080p drawn only 21.4 to 20.3 ms wet, 10.3 to
  8.2 ms dry; additions off unchanged byte for byte (2026-10-25). Ambient occlusion from the stage's geometry at load
  (`sky_occlusion.rs`): the stage drawn as depth along 32 directions over the sky, in 1 km tiles at 2048 texels (0.5 to
  1 m), each vertex weighing the directions it sees into `Vertex::occlusion` and its bent normal into `Vertex::sky_dir`:
  shade under a low sun takes the blue of the sky it sees, walls and banks the dimmer horizon; 0.1 s on 41M, 0.5 s on
  Semetin (2026-10-23). Unlike the bake's `sky_share`, the computed one holds no sun shadows; about 0.9 on open ground,
  darker in canopies. With it the stage's light is ours (`own_light`): the sky 0.56 of it times the sky seen, the sun
  0.79 by its facing and its share past the cascades, so a shadow is the sky's light alone; the bake gives only the hue.
  Every stage within about 10% of the bake-driven frame's brightness (45O, 41M, 44E, 61O, 129M, 138M, 176M, 369M, 460M);
  the weak fits and the unbaked gain the sun's modelling, dusk at -3 degrees the twilight sky's light (45O mean 15 to 24
  of 255). The light off the stage below the horizon (`bounce.rs`): each surface's linear albedo times its share of sun
  and sky, summed per vertex; drawn once at load (no running time of day, 2026-10-25); 0.1 to 0.4 s (Semetin). Only
  faces turned below the horizon get it: the terrain and the foliage (whose normals are up) none, the shaded cliff base
  at Valstagna 460M +12% (33.5 to 37.6 of 255), sunlit cuttings +2%, the dark asphalt sending little. Above the horizon
  counts too (2026-10-26): Valstagna's shaded cliff +20% (41.6 to 49.9 of 255), 41M, 176M, 369M, 394M and 45O within
  0.05% of pixels over 8; the bake 0.1-0.5 to 0.2-0.9 s. Light through the leaves changed nothing visible even at three
  times the sun, so it was dropped. The ground's maps feed the lit draws the ground they face 1.5 and 4 m out. The sky's
  tables keep a second harmonics set over the sky above the horizon alone, since the whole sphere's held a generic
  sunlit ground the bounce would count twice. The car's lower body takes the sunlit road, shaded walls the real ground.
  Additions off unchanged; night and dusk within 0.5 of 255. The stage's light adds in linear: open sunlit and shaded
  ground as before, partly shaded places brighter (42O chase +38% mean, 460M +19%, 176M +13%), frame time within noise.
  With additions off the frame moved by up to 14 of 255 on 88 pixels of 41M's two characters, unexplained. Contact
  shading: half size from a single-sample depth copy, 4 directions of 3 steps within 1 m, on the sky's and bounce's
  light only; about +0.4 to 0.8 ms over the mask alone (2026-10-25). Full size: +2 to 3 ms. `Detail::Ultra` reflections
  (2026-10-27): the surroundings cube at 256 texels (the car's paint and glass mirror it a mip level sharper, bonnet
  view the clearest), a face every frame instead of every third, the probes baked from it at 128 texels (up to 268 MB at
  256 probes; 124 probes 0.28-0.45 s, 256 probes +30 ms over High), and the wet roads' and water's screen-space
  reflections traced at full size with a wider composite tent (`TENT` 2). 41M 1440p: dry +0.05 ms (surroundings face
  0.09 to 0.15 ms), wet screen reflections 1.75 to 3.4 ms; High byte for byte (6 scenes).
  Sky domes (`sky_dome.rs`): a world sky face over the stage's middle and the corners of its central half, higher
  than the stage (39 of 659 readable RSF variants: 519, 580, 592, 450, 574 to 577 and others) casts into neither the
  sky's occlusion nor the cascades; as a caster it shut every vertex in and left 519 and 580 black. Under `lighting`
  the dome is not drawn at all, our sky and clouds take its place (user decision; 2026-10-27).
- Sun: sun shadow cascades and a far layer to 1 km (in place of the baked light maps), leaf translucency (`sun.rs`); the
  characters cast into the cascades, and the car's body and the characters take them (`stage_sun`; the car stays out,
  its own map shadows it); a plant is a cut-out piece by its own part of the texture (`foliage.rs`), so the object
  atlases' spectators, rocks and houses cast and do not glow. A plant taller than grass is a crown: its normals point
  out of the ellipsoid its piece spans, half blended with the ground's (2026-10-24). A crown is the whole tree
  (`trees`): the retail trees are three wings about 120 degrees apart, cut in strips, on a shared and sometimes leaning
  edge, so pieces sharing a vertex place or with centres within 0.3 m are one tree, one ellipsoid about that axis (per
  wing, the halves lit as two shapes). Its cards (`specular[0]` 0.3) fade out edge-on (`plant_faded`, facing 0.2 to 0.5)
  and take their sun from the crown, not the cascades (`crown_sun_seen`): Beer-Lambert over the way out of the ellipsoid
  toward the sun (density 1 per mean radius, at least 0.15), the cascades read where the ray leaves it; the crown
  position rides in `uv1`/`wet` (not on second-texture draws), the radii from its slopes. Removes the cards' hard
  shadows on each other; 41M 1080p drawn only 3.93 to 4.11 ms. Tried and dropped: per-tree tint (snow firs turned brown
  on 10O), leaf-gradient normals, per-pixel crown normals (slight), turned card copies (square tops, darker), interior
  transmission (lost the backlit glow) (2026-10-26). Each piece's cut-out test runs once a piece over a summed-area
  table: Semetin's load 7.4 to 2.3 s, 41M 1.08 to 0.67 s, frames identical (2026-10-25). Spectator cards are structures
  by name, and pieces over 40 m or wider than 1.5 times their height are not crowns (2026-10-24). Left: RSF house and
  prop atlases with cut-out gables and fences still count as plants. The main view's sun shadow is worked out once per
  pixel before the stage is shaded: every opaque world draw lays its depth and normal, then a filter pass reads the
  result. Soft by the caster's distance on the nearest cascade (PCSS, 3x3 blocker search, half-width at most 4 texels).
  Semetin 5.39 to 6.06 ms, 41M 7.68 to 8.57 at 1920x1080 (2026-10-25). The prepass's normals go to an `Rgb10a2Unorm`
  target of their own (not the frame's float colour): the prepass 1.0 to 0.52 ms at 1440p, 2.5 to 1.4-2.2 at 4K (under
  4x MSAA); with TAA, frames differ on 0.3-1% of pixels, mostly by 1/255 and a few shadow-edge pixels up to 14 (the
  offset normal's rounding flips a tap), dusk and night byte-identical. The first cascade's depth range per 4x4 texels
  (`ShadowRange::sun`, `sun_ranges`, 0.035 ms per frame) lets the mask skip `soft_filter` where every texel its search
  and taps reach is before or behind the point: frames byte-identical, the mask at dusk 1.7 to 0.4 ms, forest (25O) 1.6
  to 1.0, 41M 1.1 to 0.9, 45O unchanged. Measured and dropped (2026-10-26): no alpha sample in the prepass for opaque
  draws, no `world_lit` where the cascades reach in full (the stage shader's whole sun term, `own_sun_seen` apart from
  the car, is under 0.03 ms). The main view draws the lit opaque draws (the car, the characters) before the other stage
  draws, so the shaded ground's `Equal` test fails under them instead of shading it for the car to paint over (the
  contact shadow's 6x6 loop was the cost there): frames byte-identical on 45O and 41M, scene 1440p 45O 2.23 to 2.02 ms,
  41M 2.84 to 2.58, 25O 2.71 to 2.45, 45O dusk 2.11 to 1.90, night 2.37 to 2.16 (2026-10-26). Measured and dropped: the
  same for every opaque non-prepassed world draw (no gain); stage shading terms one at a time (specular, road shine,
  fog, sky light, sun seen, ground detail each 0.1 ms or less; no register pressure: 48-72 VGPRs, no spills). The GPU
  times of one binary fall in modes per process (main pass 1.3, 1.75 or 1.88 ms), so compare minima of interleaved runs;
  the prepass, mask and cascades are stable. `Detail::Ultra` (with `lighting`): the maps at 4096 texels instead of 2048
  (`sun::Quality`; cascades, far layer, world layer, headlights' layer: 400 MB of depth instead of 100), the soft
  shadows' search and filter in 32 and 8 texels at most (the same metres, with 9 taps a side at most); the shadows of
  foliage and trunks on the road are cleaner, the far layer's texel 0.5 m. 1440p 25O and 41M, cascades 0.55-0.6 to
  1.4-2.5 ms (+0.9 to +1.9; 3072: +0.5), scene within the GPU's noise; `High` byte-identical to before (25O, 41M, 45O
  dusk and night). Tried and dropped (2026-10-27): far layer to 2.5 km (+0.1 ms, nothing more seen from a high camera
  over 41M; at 2048 texels the 2.4 m texel loses the tree clumps' shadows), contact shading at full size (+0.1 to 0.2
  ms, edge pixels only, 100 pixels of 2 million differ by more than 2%).
- Clouds: the main view's clouds are volumetric (by night lit by the moon) and the game's cloud faces are not drawn:
  noise made at load, a 512² weather map over 120 km drawn each frame, a march at half size over the layer's shell (32
  steps, 5 toward the sun), blended into the last frame's; cubes and mirrors take a 256² dome of the same march
  (2026-10-25). The layer's kind by the cover: clear cumulus 1.2 km up under cirrus, partly 2.2 km thick, light
  stratocumulus 1.1 km up, overcast a closed deck 0.9 km up. A sunlit layer's shape is cut at most at 0.65 of its
  weather, the weather's threshold lowered to keep the cover: cut at the full weather, the clouds stood as flat-topped
  columns outlined by the weather map, boxes against a low sun (2026-10-27). Each section has its own field start and
  drift. About 0.7 to 0.8 ms on a 1080p open sky; additions off and all but the clouds unchanged byte for byte
  (2026-10-25). With the volume shown the ground's shadows come from the volume itself (`cloud_volume::shadow`,
  2026-10-27): a 512² R8 map over 8 km about the eye, redrawn each frame, each texel the sun's transmittance along the
  sun through the layer in 5 height samples of the drawn density (no detail), extinction 0.006 so bodies shadow fully
  and edges softly; `cloud_shadow` reads it (no shadows at night, where the map is not drawn), every caller with it,
  same shade factors. Overcast, night, and additions off byte-identical; 1440p scene medians within noise of before.
- Merge (2026-10-27): the four names are no longer accepted (`--enhance sky` is an unknown name); `LIGHTING` is the one
  pipeline override (was `SKY_LUT`, `CONTACT`, `PROBES`; `SUN_MASK` stays apart, it needs the pass). The 2D cloud layer
  is gone (`clouded`, `cloud_shape`, `cloud_face_lit`, `clouds.wgsl`): the cloud field's noise, `cloud_turn` and
  `cloud_shadow` live in `cloud_volume.wgsl`. `--enhance all` is byte-identical to before in 13 scenes (41M day,
  overcast, night with fog, wet, 45O, 10O snow, 61M, Valstagna, Capo di Feno, cockpit and chase); the sky in the baked
  reflection probes, wet roads, water and the car's paint no longer holds the 2D layer, up to 1/255 on a few thousand
  pixels of the wet and drive scenes (3/255 at Valstagna's sea). `--no-relight` with lighting on stays a debug state.

Albedo and shadow colour (2026-10-27, `lighting`; `examples/albedo_survey.rs`): terrain textures sampled by area, binned
by colour, mean linear albedo (Y, saturation): 41M/45O grass 0.12-0.13 (0.73), 61M dirt 0.18 (0.66), 10O snow 0.77, grey
road/rock 0.33-0.36 on 41M/45O (0.41 on 32M's limestone). Commonly quoted ranges (from memory of USGS/ASTER spectral
libraries, urban-heat albedo tables and PBR charts; not checked against the sources): green grass visible 0.08-0.15 (sat
0.6-0.7), dry soil 0.15-0.25, fresh snow 0.8-0.9, worn asphalt 0.05-0.2, concrete 0.3-0.5. So grass, dirt and snow are
within range; only neutral mid-bright surfaces run 2-3x high and grass is a little over-saturated. The vertex colours
are neutral (mean 0.28-0.38 rgb equal), so the hue filter adds no tint. `albedo_remap` (`shader.wgsl`, one rule in
linear space for every stage draw with `lighting` on): neutral surfaces with Y 0.2-0.6 lose up to 40% (snow over 0.7
untouched), saturation over 0.5 eased by up to 15%. Shadows: not a light fault. 41M 15:30 tree shadow on grass before
tone mapping: lit (0.16, 0.25, 0.07) sat 0.71, shadow (0.030, 0.069, 0.039) sat 0.56, cooler and greyer as expected (sky
light 0.17/0.25/0.44, sun 2.39/2.26/1.95; bounce on or off within 0.1%). The shadowed grass reads more saturated on
screen because AgX desaturates the bright lit grass and the metered exposure puts it at 0.55 linear (display 195 of
255); the tone curve is always on and shared with the off frames, so it was left alone. The bounce pass
(`bounce_surface.wgsl`) takes the same `albedo_remap` (shared `albedo.wgsl`) with `lighting` on. Haze: `CLEAR_REACH` 15,
9, 4.5 to 7.7, 4.6, 2.3 km (crisp, plain, hazy) and `DAY_HAZE` 18 to 33 km, which keeps the low-ground total extinction
(crisp 0.25 per km, V about 16 km) for the chase camera while the high camera's hills take the new 25 km visibility. 41M
2560x1440 scene 2.49 to 2.51 ms (3 interleaved runs, shared GPU); additions off byte-identical on nine scenes.

Shadow lookups skip only taps that cannot change the pixel (2026-10-20: cockpit scene pass 13.9 to 11.5 ms at
2560x1440, frames equal). Not kept: a min/max depth map of the cascades, corner taps first, a 1024 car map.

Cabin light: only the car's lit pipelines carry `CABIN`, and the open sky is a branch, not a `select` (2026-10-20:
cockpit scene pass 13.6 to 11.5 ms at 2560x1440, frames equal but 2 pixels by 1). Not kept: an RG11B10 bloom chain.

## Mirrors

Ours: `mirrors/` (detection, views), `renderer/mirrors.rs` (passes), `mirrors.wgsl`; `viewer --mirrors off|1|2|3`,
launcher "Mirrors" (default every frame). Off: no passes, the frame byte-identical (6 views, 2026-10-19).

- Found at load from the cockpit eye (`Mounts`): the `*mirrorglass*` parts (else the `*mirror*` housings' faces) and
  the opaque glass-class faces (retail cars), in panes of welded, coplanar faces (10°, 3 mm). A pane is a mirror when
  the eye's line reflects back along the car (`z` >= 0.5), it is 3-45 cm, 0.2-1.5 m ahead, at most 45 cm below the
  eye, flat to 4 % of its size, and the eye sees it past opaque non-glass faces at one of 15 points; one per side and
  one centre. No named centre mirror (RSF): the largest opaque pane hanging there. Not flat: the cube as before.
- Survey (`examples/mirror_survey.rs`): the retail cars 2 door mirrors each, Lancer Evo VII 1 door + centre, Impreza
  2000 2 + centre; the 42 RSF samples 1-3 each but the Fiesta Rally2 (none); curved glass in 12 keeps the cube.
- View: `crop * VP * R`, R the reflection across the glass, crop the glass's screen box with x flipped, z row for an
  oblique near plane 1 cm past the glass. Image about 1:1 with the glass, 32 px steps, at most 1024 x 512; the glass
  samples it by its point in the view it was drawn with. Rates take turns between mirrors.
- Passes draw the stage to 300 m (far ground), sky, fog, the car (exterior in door mirrors, cabin in the centre),
  ghosts, the game's particles, rain (`Effects::mirror_group` over its depth) and lamps, with the frame's sun cascades
  and exposure; no prepass, light shafts, cubes or bloom. Only the glass's pipelines have `MIRRORS`: on in
  the shared shader it cost 1.1 ms a frame at 1440p (M-series).
- Cost over Off, Evo X R4 cockpit, min of 5 (2026-10-19): every frame +0.66-0.82 ms at 1080p, +0.71-0.75 at 1440p,
  +0.71-1.13 at 4K; every 2nd +0.29-0.45; every 3rd +0.18-0.34.

### Performance log (2026-10-26)

Launcher Graphics page "Performance log" (`perf_log = on`, default off) passes `viewer --perf-log <files folder>/logs`;
`perf_log.rs` (the span recorder and the writers), `profile.rs` (GPU timestamps), `bin/viewer/perf.rs` (header). Two
files per session, named by the start in UTC: `perf-<YYYYMMDD-HHMMSS>.log` (header, a line a second with frame time
mean/p50/p99/max, hitches = frames over twice the second's median and 25 ms, CPU spans and GPU passes as mean/max ms,
then a summary with mean/p50/p99/max by name) and `.trace.json` (Chrome Trace Event format, ui.perfetto.dev: last
600 frames, written at exit, so a killed viewer leaves only the `.log`; tracks: main, shadow maps, loading, frames, GPU).
- CPU: `perf_log::span(name)` guards (`Span::next` for a function's phases) on the window's frame (acquire surface,
  input, live link, live state and sound, scene update, render, HUD, submit, GPU timing readback, present), window
  events, the shadow-map thread, the loading thread, and the renderer's own `mark`/`span` labels (the CPU between a
  mark and its span: the stage's encoding, nested in "render"). The live link never waits for a physics step (the
  pump takes what has come), so there is no span for it. Off, a span is one relaxed load and a branch.
- GPU: `Profiler` streaming mode: a ring of 8 query sets and readback buffers, `map_async` polled without waiting
  (`PollType::Poll`), a frame read back a few frames late (a frame whose slot is still being read has no GPU
  times: the unthrottled `--frames` loop loses most, a window held to 2 frames in flight few). The shadow thread
  writes its own two timestamps into the frame's slot. No timestamps (no `TIMESTAMP_QUERY_INSIDE_ENCODERS`, macOS,
  `RBR_PROFILE=wait`): no GPU passes, the header says so, the frame times and CPU spans stay.
- The GPU's clock is not the CPU's and wgpu has no correlation: a frame's passes are drawn ending at the CPU time its
  last submission returned (`Profiler::end_frame`). Durations are exact; a GPU behind the CPU shows earlier than it ran.
- `viewer --frames N --perf-log DIR` logs the offscreen frames too (header `present mode offscreen`). Frames are
  byte-identical with it on (1080p `--enhance all` screenshot `cmp`).
- Overhead (45O, xsara, `--enhance all`, release, radv; CPU per frame with the GPU idle, recording, encoding, submit and
  readback, least of 3 x 150 frames): 7 runs at 640x360 and 1440p: +0.23, +0.04, +0.18, +0.08, +0.14, +0.05, 0.00 ms,
  median +0.08 ms (run to run noise is ~0.1 ms; the GPU here is shared and stalls queue submits for 70 ms at 1440p, so
  the drawn-only frame time is not usable as a measure).
- Untested without a display: the window path (`App::frame` spans, `Renderer::end_frame` after the present's submit).

`--frames N --profile` prints per-pass medians. Texture packs: `src/bin/texture-pack.rs` upscales a stage's or
(`--car`, `--rsf-car`) a car's textures with a Real-ESRGAN ncnn binary, 4x or 2x to fit 4096, and re-encodes them
with full mips (never committed); `viewer --textures <dir>/pack` reads them, the car's after its dials are made
(dial faces keep theirs). The compact `realesr-animevideov3` beats `realesrgan-x4plus` on retail liveries and signs
(cleaner flat edges, 1024 livery 4.0 vs 20.7 s; RX 9060 XT, 2026-10-26); both garble non-Latin sign text. Close
cockpit and bonnet texels gain most: ACCENT's bonnet logo loses its stair steps, its console (a few dozen source
texels) barely changes; ACCENT 11 textures 16 s; GR Yaris Rally2 (2048 and 4096 sources) a crisper carbon weave,
19 textures 131 s, 982 MB at 8192. Cap 4096 (the default): Fiesta R5 (2048 sources) 28 s, 257 MB against 79 s,
833 MB at 8192, its livery text as sharp but the smallest line; both smooth the seat fabric's grain (the model, not
the size).

Load-time upscaling (`upscale.rs`, ours): `viewer --upscale-textures DIR` (the launcher's "Textures: upscaled",
`upscale_textures`; DIR `<player folder>/Upscaler`, the binary, `models/` and `cache/`, which the launcher's "Download
upscaler…" fills, `rbr-launcher/src/upscaler.rs`) upscales the car's and the stage's textures
(`upscale::stage_textures`: the stage set less the sky's and the start clock's placeholder; not the characters, whose
faces WDN distorts) at least 32 texels a side with Real-ESRGAN General WDN v3 4x (halved for 2048 sources, 4096 at most)
through `realesrgan-ncnn-vulkan` (`RBR_UPSCALER`, else DIR, else `upscaler/` beside the program), after the car's dials
are made, cached by the source file's SHA-256. Model choice (10 tried on livery, sign, switch panel and dash crops): WDN
v3 (BSD-3, compact, ~4.4 s for a 1024 livery) crisper on lettering than UltraSharp and Nomos8kSC (both ~14 s, keep the
source's stair steps) with the grain kept; animevideov3 and digital-art flatten grain, digital-art and x4plus invent
letters. ACCENT 7 textures: WDN 17.2 s first (UltraSharp 48.7 s), 0.2 s cached. Stage objects (track-41_M 21, track-10_O
19: signs, banners and fences 64 to 256, the object atlases 512x1024 and 2048) take ~25 s first, mostly the 2048 atlas;
plants and the guardrail and van sharpen cleanly. The ground's atlases (1024 to 2048, one set per weather and wear) take
8 to 14 s more: stone gravel (track-51_O) gains the most, blurred stones made crisp; dirt (track-61_O) a little; tarmac
(track-41_M, track-10_O) loses some grain. Cached, a stage loads in under 1 s more. The backdrop (superbowl, 128) and
the course tape (Fence set) showed no difference. The missing textures go through one run of the binary over a folder
(`-v` names each saved file), each encoded while it makes the next: track-41_M with ACCENT 73.6 s first before, 27.0 s;
track-51_O 35.4 s, 20.9 s; the cache byte for byte the same (2026-10-26). Per texture the binary's start cost more than
the work; a 2048 atlas is ~14.5 s on the GPU, 0.9 s reading its 8192 PNG, 1.4 s encoding (ClusterFit; RangeFit 0.1 s,
worse blocks). Shrinking 2048 sources to 1024 before the 4x (4.9 s) smears them; a 2x model of the same kind would cost
about the same, its work being at the input's size. The binary's `-s 2` scrambles a 4x-only model's tiles; `-t 1024`
breaks past one tile. Not packaged: the download takes Real-ESRGAN v0.2.5.0's release zip (macOS universal, Linux and
Windows x86_64; none for ARM Linux) and the model's ncnn files from upscayl/custom-models at 500e6a0, each checked by
SHA-256.

## Lighting sheet

`scripts/light-sheet.py --a VIEWER [--b VIEWER] --out DIR [--quick]` renders seven fixed views at 1920x1080 with
`--enhance all` (forest 41M, open ground 394M, cliff 460M, crowd 176M under the start tent, 45O chase and cockpit
with `--drive`, the unbaked 369M) at 07:00, 10:00, 13:00, 17:00, 18:00 and 23:00 in dry, wet and heavy fog
(`--quick`: 10:00, 18:00, 23:00, dry), three at a time, and writes one sheet per view (rows hours, columns weather,
a|b per cell) and `summary.txt` (mean, maximum and share over 8 and 32 of 255 of the a/b difference). The same binary
as a and b differs on under 0.01% of pixels (a few up to 50 of 255: shadow and particle noise); judge by the mean and
the shares, not the maximum. Quick with two binaries: about 2.5 to 4 minutes.

## Open

- Sky dome and cloud transforms and inputs (`SkyDome.vso` `c41`-`c66`); stage in-scatter not reproduced.
- With `Enhance::sky` at night (sun -15 degrees, 462M and 41M at 23:00) the aerial table's haze is red twilight, which
  tints the stage and backdrop red under a dark blue sky (on main too).
- Cut-out backdrop strips with black transparent texels (Odry's panoramas) show a dark rim along the ridge.
- Stage `c90`; kind-byte blend bits; reflection items `(1,7)`; water texture; mip levels beyond the 4 stored.
- Wheel set shown; `Plane_*` shadows; detached node motion; `pc_e_window_*` panels.
