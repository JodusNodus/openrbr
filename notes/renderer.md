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
  surface while another thread submits (the loading thread: `present::configure` reports it and the windows try
  again next frame), and reports a hidden macOS window as occluded instead of waiting up to a second for a drawable
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
  One sample a pixel: cut-outs (`cutout`) anti-aliased by TAA (below), the alpha test alone with
  `--anti-aliasing off`. 4x MSAA, alpha to coverage and `depth_copy.rs` are gone (2026-10-26); reflections and
  light passes read the single-sample depth directly. `--reflections high|medium|low|off` (`Reflections`, `enhance.rs`; `on` = high); low halves the steps (wet 41M 14.2 to
  12.8 ms, 2.6% of pixels over 8; 45O similar, 2026-10-26). Off: wet roads and water take the cube and sky alone
  (wet 1080p 394M 26.4 to 10.5 ms; 41M similar, 2026-10-25).
  Reflections trace at half size (`reflection_composite.wgsl`): 41M
  chase 12.8 to 6.5 ms (45O similar); thin RSF reflections softer. Additions off: only water differs (44E max 17/255;
  71M 52 pixels over 8) (2026-10-26).
- `--anti-aliasing taa|off` (`AntiAliasing`, default `Taa`, ours after gp4's `taa.rs`; `4x` parses as `taa`): one
  sample, the main view's projection jittered and resolved into the post pass's scene, sharpened under TAA
  (`taa.rs`, `taa.wgsl`, `post.rs`: Halton 2,3, 8 phases, 16 below full size, so 0.75 and 0.5 render scales resolve
  cleanly; the reprojection, history, glass mask and reset rules are there).
  Measured 2026-10-26 (RX 9060 XT radv, `--enhance all`, 1440p, Laplacian variance): 4x / TAA / TAA sharpened
  forest chase 251 / 136 / 230, cockpit 174 / 95 / 120 (TAA roughly halves the aliasing, sharpening restores about
  half; dry, wet, rain and night scenes similar). `--anti-aliasing off` frames are byte-identical to before in
  13 scenes. Tried and removed: a negative texture mip bias (0 to -1; 201 to 205, no change).
  The steering wheel's pixels move by its turn since the last frame (`taa::Wheel`, `Playback::set_wheel_node`): its
  room is measured from its meshes in its node's frame, radius and depth along the column per 8 bands of radius (rims
  sit 0-13 cm up the column from the node, some meshes take the column down to -0.9 m). Before, a fast turn left
  translucent spokes and dark smears behind the rim's mark (Delta HF, M3 E30). Stills unchanged (34 shots
  byte-identical; 4 vary run to run, each matched a baseline render), TAA 0.72 ms either way, the wheel's test forced
  on too (Delta cockpit 1080p, M3 Pro, 2026-10-31). The turn is undone on the model-space point before the car's
  motion: a separate wheel-to-previous matrix changed Metal's fast-math code for the car's pixels (a chase still by
  up to 2/255).
  Glass over the car (the dash below the windscreen, the cabin behind the windows, lamp lenses) gathers as the rest:
  the mask takes the frame new only where the stage is behind the glass (2026-10-31). The lower windscreen (alpha about
  0.9 at its grazing angle) was taken new every frame, its dash edges and reflected cabin never smoothed; its
  frame-to-frame change halves (Fiesta Rally3 cockpit), rain drops and wipers as before. All 34 shots change: cockpits'
  lower windscreens, chase views' rear windows and tail lights smoother, the view through them slightly softer.
  Dark specks on the car's silhouette in a still chase view (3 pixels over 20 luma below their neighbours; 34 with
  off) are the thin dark sliver's real coverage, not a history lock: the raw jittered frames alternate dark and
  bright at those pixels.
- Pipelines are specialised by override constants (`pipelines.rs` `PipelineKey`: `LIT`, `INSIDE`, `CABIN_SET`,
  `FRAME_KIND`, `KNOWN`, `DRAW_*`; each carries only the code it runs).
  Dead code still held registers: 200-470 byte spills per thread in the stage and car pipelines on an M1 Pro (Metal System Trace, no Dynamic Caching). Native x 0.75 on the M1 Pro, every enhancement: road and chase
  51-57 to 15-20 ms, cockpit 188 to 20 ms; frames as before within a few edge pixels. Live at 1280x720: 107 to 156
  fps; full-screen cockpit 12 to 54 (2026-10-25). The fog, the sun's cascades and the car's shadow still spill
  (~150-310 bytes) in the stage pipelines.
- Small arrays indexed in loops (`harmonics`, `texel_filter`) are written out: Metal kept them in memory.
- Reflections medium, low and off differ from high by up to 35, 58 and 79 in the wet 45_O scene (1280x720, Xsara,
  every enhancement; 2026-10-26, RX 9060 XT, radv).
- Tried and removed (no gain on an RX 9060 XT, 2026-10-26): the car's shadow filter taps (`Quality`, `--shadows`,
  `SHADOW_TAPS`, `CONTACT_TAPS`; medium saved ~0.2 ms but differed from high in the cockpit with sun through the
  windows by mean 0.3-0.7, 1% of pixels over 5-10/255, so the filter stays high's 4 and 6; `--shadows X` and
  `shadows =` are read and ignored), the stage's weather in the pipelines (`WEATHER_KNOWN`, `Conditions`), the
  day-only pipelines (`DAY_ONLY`, `night_probes`), the split reflection pipelines (`REFLECT_WATER`; one pipeline
  draws water and wet road, the draw's mode says which).
- Passes over the kept frame draw into it (`Renderer::over_attachment` in `renderer/frame.rs`);
  `viewer track-20_N --anti-aliasing off` failed validation before.
- The car's shadow map's depth range about each 4x4 texel cell (`ShadowRange`, `shadow_range.wgsl`,
  `renderer/shadows.rs`): the cockpit gains little, its own faces vary within the filter.
- The car's shadow layers draw only the parts the view or the outside views show (`GpuMesh::present`, 2026-10-30):
  outside, the hidden interior and the other state of a lamp or window cast no more. 580 208 chase 1080p 1.03 to
  0.71 ms; cockpits byte-identical, outside views within 0.4 % RMSE (shadow edges by a few pixels, the interior
  through the rear window).
- The sky draws after the opaque meshes at depth 0 (`PipelineKey::sky`): the frame matches the sky-first order
  (13 scenes) but for `--anti-aliasing off`, where 10 cut-out edge pixels on track-54_E differ by up to 3/255. 4K
  cockpit on Semetin at dusk in heavy rain, every enhancement: 36.7 to 31.3 ms (M3 Pro, 2026-10-20).
- Run to run (Metal, M-series, 2026-10-20): 18 of 18 runs match (without the depth prepass's `discard`, 66_E's
  spectators and twigs differed in ~1 run in 3). Still open: with 4x MSAA only,
  ground triangle edges differ by up to 35/255 (33_N at 2000, ~half the runs) while uploads, draw lists and matrices
  are bit-identical; from the fourth frame on, 24 of 24 runs match, so captures comparing frames render a few frames
  first (`--frames 3`).
- Inputs: the recording frame and the live records (`rbr_recording` documents the FRM1/PTC1/PEV1/DMG1/DET1/CLK1
  layouts); playback's `PTC1`/`PEV1` from `RBR_PARTICLE_EVENTS` (`particles/sidecar.rs`), else none.
- Presentation clocks step 7 ms per payload period; a live answer of several periods is one game frame.

## Live frame loop and input latency

`bin/viewer/window.rs`, `live/mod.rs`; measured with `RBR_LATENCY=script` (`bin/viewer/latency.rs`: a ±0.5 steering
flip every 500 ms, ms from the flip being due to each stage, summary at exit). Ours, per frame, in this order:

- acquire the surface's texture (the loop's wait: a free image, the refresh under fifo; `desired_maximum_frame_latency`
  2, `window.rs`);
- gilrs events (the pause menu's navigation) and the input thread's last poll (`window/feed.rs`): the input read;
- `Live::pump`: the payloads due at 7 ms per period (at 60 fps 2-3 a frame, all with this read), then up to 6 ms
  waiting for their answers (one frame of 5-6 NGP steps: p50 1.5 ms, p95 2.2 ms with the pipes), so the frame draws
  the step of the input it read;
- the playback shows the newest answer; the crew's wheel and hands take the car's control the last step took (CTL1,
  the filtered value, not the device value);
- CPU build and encode (6-8 ms at 4K cockpit), submit, present; GPU 10 ms (4K, 4x AA, cachy's RTX 2080 Super Max-Q).

cachy, 4K 60 Hz, scripted flip, 3 runs each, p50 (p95) ms from the flip: before, input read 7.5 (16), answer
taken 25 (33-45), submitted 41 (49-62), GPU done 51 (58-71); after: 9 (16), 11 (16-32), 18 (21-49), 28 (30-58).
On screen 41 (46) with direct scan-out, 61 (61) composited; `play.sh` sets `MANGOHUD=0` (it added p95 tail).

Live (`rbr-standalone --live`, the only live mode; the viewer has no pacing of its own): a frame per physics step on
its own clock (a late wake's frame takes the missed steps, cut at 0.1 s), each with the newest input; the input thread
(`window/feed.rs`) polls the devices every 1 ms (50 ms keepalive, one-shot flags ORed until a frame takes them); an
STP1 record (`rbr_recording::realtime`) after each stepped frame carries force, control and input number.
`--frames-out` logs each
frame's digest; `--frames FILE|-` replays such a log; the tools that step a drive themselves (`Live::lockstep`:
captures, `--screenshot --at`, `rbr-sound`) write one unchecked frame a step (1/720 s under NGP). cachy, 35 s
scripted: flip to first step p50 1.64 (p95 1.84) ms; 720 steps/s, one per frame (3 of 25175 two); wake p50 0.075 ms
late (p99 0.28); on screen 42 (51). Wheel: 526 level changes/s against 43/s paced (RSF about 510). Replays: scripted
17999 of 17999, autopilot 17994 of 17994 frames equal.

RSF (drive2 sessions, RSS1): NGP's input thread polls the wheel every 1.1 ms (POL1), one 1/720 s step per frame at
about 580 fps; a poll change reaches the car's control in p50 1.0 ms (p95 1.6), its step's end in 1.2 (1.7). Ours stays
a 60 Hz render frame behind that for the picture; the input and the force are RSF's in real time.

Video: `viewer --replay DRIVE.rrp --video FILE` (`bin/viewer/video.rs`; `--video-fps`, default 60 at 1920x1080;
ffmpeg `libx264 medium crf 18`, `aac 192k`). The sound renders offline (`Player::offline`); window and video share `draw::draw_car` and `draw::draw_particles`.
Start-orbit frames vary between repeated exports. Kormoran I + Fiesta
Rally3 Evo, 1280x720: 63.9 fps render, replay 16920 of 16920 frames equal, video and audio durations match.

## Faithful and ours

| System | Faithful | Ours |
| --- | --- | --- |
| Stage | slots, render states, LOD lists, visibility cells, lanes, props, light maps | LOD 600 m, texture fallback, water surface |
| Sky | `TrackSettings.ini` values, linear fog, cloud face | section pick, sky, haze caps, tone curve |
| Car | parts, switch states, classes, draw lists | lighting, reflection cubes, sun and contact shadows |
| Car shadow decal | `RenderShadows` 0x605670 | body pose as its frame |
| Lights | switching, part predicates | headlight key, night light, no flares |
| Damage | deformation, scrape, crack projection, dirt level | crack texture, glass grime, detached motion |
| Particles | emitters, classes, handlers, rain instances | step, pools, lighting, sparks, chimney smoke |
| Glass | wiper angles and timing, bug data | the water-on-glass model |
| Skid marks | emission, rings, fade, strips | frame-time accumulator, depth bias |
| Ghost | mesh, look, pose, files | race-time origin, best ghost |
| Characters | placement, Lua, triggers, clips, crew IK | shading, CPU skinning |
| Fences | `CFenceHandler` cloth | inside test, body placement |
| Dials | RBRHUD data | all drawing |

## Stage

- Geometry `(1,0)` slots 0, 5-8, its light map, mip bias and culling as stored (`stage/mesh.rs` `DrawState`).
- LOD (chunk 0x6c): list 1 within 600 m, list 3 beyond 550 m, list 2 always (`stage/mesh.rs` `LOD_DISTANCE`;
  reference installs run low's 200 m).
- Visibility cells `(2,10)` (`visibility.rs` `CELL_REACH`/`DRAW_REACH`/`CUBE_REACH`): ours draws to 3 km, so from
  a crest the stage shows rather than the coarse backdrop (61_M from 80 m up: 27k to 70k triangles).
- The backdrop's pieces have no bounds (no reach, no levels); each view culls them by their vertices' box
  (`GpuMesh::backdrop_bounds`, 2026-10-31): 580 chase 2.04M to 1.85M triangles, surroundings cube face 0.51 to
  0.35 ms, frames byte-identical.
- Load (`Renderer::with_progress`): the meshes' drawn vertices (`sun::with_normals`, `foliage::with_plants`, ...) are
  made over the load's threads 1024 meshes at a time, the uploads in order (2026-10-31): 580 renderer creation to the
  sky occlusion bake 5.0 to 4.1 s, byte-identical. The cockpit eye's fit (`views::cockpit_back`, twice a load) casts
  its lines only at the interior triangles whose boxes come within reach of the eye: the 208's car load 1.9 to 1.0 s,
  the 42 RSF cars' mounts identical. The glass's film samples (`film_corners`, `Tints::film_tints`) go over the
  threads too: `glass::Drawn::load` 0.56 to 0.25 s. The road and ground marks (`Stage::mark_surfaces`) share one load
  of the collision ground and one lookup per ground triangle: 580's stage load 2.99 to 2.14 s. The road's edges
  (`puddles::mark_roads`) map only the film triangles' edges, the others' looked up and the edge distances found over
  the threads: 2.14 to 1.66 s. The grass's ground texels (`grass::add`, decoded and blurred per ground texture) are
  made over the threads before the ground triangles number them: 1.14 to 0.39 s. The bounce bake's surface maps leave
  out the grass's unfilled tile meshes, as the shadow casters do: 580's bounce 1.6 to 0.8 s.
- Object lanes `(1,1)` (`stage::OBJECT_ALPHA_REF`, kind-byte address modes).
- Interactive props `(1,6)` (`stage/mod.rs` `InteractivePose`): every retail record drawn at its placement while its
  cell shows; live moves a woken prop by its solid's pose. The car's glass (no depth write) goes over them.
  Scenery physics: [physics.md](physics.md).
- Sky faces `(1,3)` and `Clouds.lbs` (`stage/mod.rs` `global_clouds`): the face the section's `CloudName` names,
  camera-anchored, before the stage. Water `(1,8)` is our surface (`water.wgsl`); reflection items `(1,7)`
  withheld; the sky dome (`skyboxes.lbs`) is not drawn.
- Textures: weather/wear fallback as requested (`stage/mod.rs` `Weather::fallbacks`; `tests/stage_data.rs`).
- Texture size: from the first mip that fits the adapter's 2D limit (`renderer/textures.rs` `dds_texture`; RSF
  484's backdrop is 16384 x 8192, RX 9060 XT allows 32768; checked capped at 8192, 2026-10-27).
- Ours: ground detail noise, the backdrop's own texture as its detail (`backdrop_detail` in `shader.wgsl`), road
  shine on specular slots 6 and 8 (the game's formula, our `c90`); the wet film is the `wet` addition.
- Terrain detail (ours, always on; `terrain_detail.wgsl`, `terrain_detail.rs`, 2026-10-28): the draw's most
  detailed texture window tiled mirrored and triplanar adds its high pass on the near ground (RSF Luceram's 4096
  `z*` atlases, past the upscaler's cap). Load 63-72 ms (52M, 417O); scene pass 1080p 417O rain fly-by 2.74 to 2.84
  ms (golden fly-by, 580 chase and 1440p similar; `RERBR_NO_TERRAIN_DETAIL` A/B). The windows are found over the
  load's threads (2026-10-31): 580's 0.51 s to about 0.05.
- Steep banks (ours; `terrain_bank`, `bank_low` in `terrain_detail.wgsl`, 2026-10-31): past about 50 degrees
  (`terrain_steep`) the stretched base colour is the textures' mean widened along the footprint's major axis to 256
  texels (`TERRAIN_BANK_WIDE`; the fall-line bands average out, the along-slope patches stay), structured by the
  window's triplanar luma high pass, the paved road kept out (`paved_share`). The footprint there is minified 2 to
  4 times yet streaked, so no magnification gate, and the ground mark covers only half of 135's bank, so no ground
  gate (both tried). Weighted per texture by its bank share (`terrain_detail::bank_shares`, `mode.bank`, 2026-11-01):
  of its steep near-ground faces, how few stand within about 12 degrees of vertical. Ground atlases under 1 %
  (135, Luceram, Sturec), Sardian's facades and Sturec's walls 99 to 100 %: ungated they turned windows plaid and
  stone walls to mush. A uv test (projected from above or not) failed: the banks' uvs follow their faces, the
  streaks are painted in. 135's cockpit bank loses its smears for mottled grass (Clio, 19:30, frame 4900; the gate
  keeps 92 to 100 % of it); scene pass 1080p 3.66 to 3.75 ms there; Sardian byte-identical, banks elsewhere calmer.

## Atmosphere and sky

`atmosphere.rs` picks one `Maps/TrackSettings.ini` section by stage and weather preset (`viewer --sky` overrides;
every retail row of an installed stage but `69S`'s clear ones names one). `viewer --fog off|light|heavy` puts our
height fog over any section; `--clouds` and `--precipitation` put any sky over any row. Haze is the linear fog's
range, at least 600 m without `UseFog` (retail 69's light fog, 71 and 45 RSF rows at 25..215 m hid the stage in a
white wall; other frames byte-identical, 2026-10-27). The sky gradient, doubled clouds and tone curve are ours.

- Backdrop `(1,3)` (the superbowl): every item drawn whole with the stage's matrix, its scattering depth by
  `Superbowl_Scale`; without `UseFog` its haze is the stage's range over the scale (`Atmosphere::superbowl_fog`), so
  the backdrop next to the road stays clear.
- Height fog with `Enhance::lighting` (`height_fog.rs`): the mist at half `FogEnd` (`heavyfog`), three quarters
  (`lightfog`) or all of it at the low ground, thinning with height; the sky to 100 km. Fogged at its own distance,
  as the stage (an earlier 3 km floor left 122 Bruchsal's tree lines as ghosts and its flat backdrop a pale slab from
  above, 2026-10-27); without ground about the eye or at night the linear fog stays, the backdrop at the stage's
  range.
- The sun's light in the air with `Enhance::lighting` (`light_shafts.wgsl`, ours, 2026-10-27, in place of the
  screen-space shafts), by day under a sky not overcast: a quarter-size march to the depth, the cascades times the
  volumetric clouds' shadow map, jittered and blurred; negative light, so only shaded air darkens (adding
  in-scatter instead laid a pale veil over the far land). Plus a glow within 350 m, so the sun shines through the
  canopy in rays. Shows in fog and low sun through trees; in clear air faint (13:00 mean under 0.4/255). 0.16-0.18
  ms at 2560x1440 (RX 9060 XT; 0.01-0.14 before). Additions off, overcast, night, night fog and dusk byte-identical
  to before. Tried and dropped: a CPU dust-grid splat for the wheel spray (no visible difference).
- The backdrop's sky occlusion is weighed against its own meshes alone, not the strip it continues under (with the
  strip as cover its vertices went near black, 41M raised camera, 2026-10-27; `renderer/occlusion.rs`). The strip's
  own occlusion is unchanged; load +0.0 to 0.1 s (Semetin 0.2 to 0.3 s).
- The backdrop's sun lookups sit 80 m higher along the ray (`lifted` in `shader.wgsl`), so the stage's edge and
  trees tens of metres over the backdrop do not shade it while farther ridges still do (41M raised camera, 12:00 and
  17:30, 2026-10-27). The other way, its casters sit 200 m lower (`sun::backdrop_drop`), so the backdrop casts
  nothing on the stage ground it lies over (the Capo di Feno patch: a sea sheet 120-200 m over the ground; 200 m
  clears it, Valstagna's gorge still shades the strip). Frame and load time within noise (41M 0.94 to 0.97 ms, Capo
  2.50 to 2.53 ms at 1440p).
- Panorama backdrops: on 60 of 356 installed RSF stage folders the backdrop is at least half upright faces, on some a
  ring of photo strips with alpha sky (462-467 Odry: 4 items, 22 triangles, 2.5-2.8 km out, white vertex colour,
  `HOR_2`/`Hor_11` 4096x512; also 138-140, 149, 166-169, 436). Where the eye is above a strip's top the sky dome
  shows below the horizon: with `Enhance::lighting` it takes the horizon's light there (`SKY_HORIZON` in
  `shader.wgsl`), not the tables' grey ground (also seen from crests on retail 41M, 45O, 61M) (2026-10-27).

## Time of day and night

`--clock HH:MM` (`relight.rs`; each variant letter is its own bake, so the bake's sun is fitted from the vertex
light). A solar model per rally moves the sun. Below -7 degrees: night (`night.wgsl`, `lamps.wgsl`): moonlight, the
lamps as main or dipped beams with a shadow map, stars, wet-road lamp glints. Shared bakes with M names (73, 83-89)
probably fit the wrong hour.

- Variants are one stage under another light: the same geometry (99-100% of vertex positions) and textures but the
  sky, the vertex light, the shadow light maps (`shadow_0.dat`) and the section (2026-10-23, every installed variant).
- Every stage is relit (`--no-relight` keeps the bake): at `--clock`, else under the bake's own sun at its hour
  (where the rally's path never climbs as high, as on the British stages, the path is lifted to it), and the viewer
  gives the start board that hour (`--start-time`, not on a championship stage).
- The start board (ours; `start_board.rs`): red during the countdown, green for 5 s from the go (none on a jump
  start's go), unlit lamps darkened to a quarter. Lit lamps glow at any hour, the digits by night and dusk, still
  fogged (2026-10-27, track-54_E).
- Baked shading lives in three places. The shadow light maps paint the bake's cast shadows: with `Enhance::lighting`
  they are neither loaded nor used (Semetin 176_M: 165 maps, 350 MB); the cascades, the far layer and the world layer
  (`sun::world`) cast for any sun, and at night for the moon (no measurable frame cost, 2026-10-25). The backdrop
  takes no sky occlusion (baking it left it near 0 for +0.1-0.3 s load). The vertex light less the bake's sun is the
  ambient occlusion, or the computed one with `Enhance::lighting` (the bake then gives only the hue, so the crowds'
  saturated clothes are not lifted past their texture). No stage paints shadows into its track textures (every
  installed variant, 2026-10-23).
- The stage's own sun is the fitted bake sun where the fit holds (r from 0.35, `relight::own_sun`): 40 variants'
  `SunDir` lie over 20 degrees off it; weak fits keep `SunDir`.

- Night albedo (`night_albedo` in `night.wgsl`): textures times vertex luminance over the bake's open-ground light,
  clamped 0.3-1 at 0.25 (0.14 on 45_O to 0.39 on 10_O).
- Headlight beam (`beam` in `lamps.wgsl`, ours after the ECE patterns): dipped, cut-off 0.7 degrees down, reach about
  60 m; main (the default, as rally cars run stages), dipped plus an axis lobe of 5 times the hot spot, 1/e at 4
  degrees across and 2 up, reach about 130 m.
- The lamps' light in the air (`light_shafts.wgsl` `fs_beams`): quarter-size ray march at night and dusk (Kulla and
  Fajardo 2012): 1.36 ms at 1080p. The main pass keeps its depth whenever the post pass runs (else the beams marched
  120 m of fog over the cabin).
- The night's light is a moonlit night as the adapted eye sees it: clear, the frame's mean about 11 of 255 from
  40 m up on 44E, falling with cloud cover squared to 0.4 under heavy cloud (2026-10-23: 44E, 45O, 21O, 41M)
  (`night.wgsl`).
- Skid marks and the car's shadow decal are unlit records, so they take the moon and the lamps (`mark_light`).
- With the physical light the moon is the real one for the rally's date in 2004 (`relight::lunar`, decided
  2026-10-24): light by phase (half moon 0.09 of full), none below the horizon.
  2004's nights: arctic 0.35 of full, Mont Blanc 0.07, USA 0.78, British, Australia and Japan moonless. The disc shows
  its phase; the game-bright path keeps its full moon opposite the sun.
- With the physical light twilight and night are the day's path: the sky's tables run at every sun elevation, the moon
  (0.27 lux full) is the direct light from 1 degree down with 0.002 lux of airglow, and night from -18 only turns
  on the night-only effects. The lamps light in lux (dipped hot spot 20 000 cd), only in pipelines built with them
  (45O, 2026-10-23: -6.4 degrees 5.0 lux, -9.9 0.07, -13.3 0.006; frame mean 92 at 17:30 to 14 by 18:25, holding to
  23:00). Rod vision (`rod_vision`): moonlit ground turns blue with dim greens and reds kept, the headlit road stays
  neutral, by day at most 1/255; the meter bins luminance from -14 to 18 EV (in pre-exposed units twilight went
  black).
- With the day's light the lit draws outside the car shade as the stage (`day_lit` in `shader.wgsl`): linear albedo,
  Lambert under the day's sky, sun and ground maps, plus the sun's highlight; paint and glass reflect by Schlick's
  F0 0.04. The game's Ka, Kd, Fresnel limits and `LIGHT_SCALE` are off that path (they put the car at a fifth of the
  stage's light). The windows now show the interior. Frame time unchanged (2026-10-24: 176M, 41M, 45O chase).
- On that path paint and glass are a clear coat (`shader.wgsl` `COAT_ROUGHNESS` etc.), mirroring the 128-texel
  surroundings cube, still drawn beside the probes when the car is seen from outside (one face every 3 frames, 0.10
  ms). Fiesta R5 and GR Yaris on 103O at noon: 5 to 16% of pixels over 8 of 255; cockpit and additions off byte for
  byte (2026-10-26). Tried and dropped: bonnet SSR over the cube (little change); per-vertex car occlusion (needs
  the car in the prepass).
- Car look (2026-10-28, `car_look` in `shader.wgsl`; day's light, not in a cockpit's mirrors): the body's ambient
  and bounce light times an analytic occlusion in model space (sills, arches, underbody); a second sun lobe, pigment
  saturation 1.25; rubber by texel luminance, the rim a rough mirror; contact shadow 0.4 to 0.7 (0.4 in the cockpit
  view). gab-lit scene pass 3.01 to 3.06 ms; cockpits and additions off byte for byte.
  Round 2: the car's opaque reflecting draws are in the main view's depth prepass and shaded by the outside pipeline
  (`DrawPass::CarMain` in `renderer/pipelines.rs`), so the stage's contact shading sees the car; `Equal` depth
  there turned
  the Yaris's LED strips black. The coat mirrors half of a procedural far field (`horizon_mirrored`), so panels show
  a band over a dark one.
- Car look round 4 (`lens` in `shader.wgsl`): a lamp's lens is a reflecting lit draw with no dirt map (the M3's
  headlights are class-1 textured discs, no glass mesh): a sharp Fresnel mirror with the bright texels as chrome;
  the Yaris's LED strips are not affected. Tyre dirt falls off above 0.08..0.3 m over the ground, rubber only.
- Rain on the paint (`wet_paint`/`wet_base` in `shader.wgsl`; only with rain on wet roads, so damp, dry and
  additions off are byte for byte (dry overcast 3 pixels of 2M one level off, the codegen)): clear-coat roughness
  halved, base 15% darker and 40% more saturated (negative channels clamped: they made yellow decals lavender
  through `pow`); beads and streaks tilt the reflection's normal in the car's model space, faded out over 6 to 18
  m; the body takes grime low down (it has no ambient occlusion). Scene pass on the Luceram fly-by (1080p, ultra)
  1.73 to 1.74 ms (2026-10-28); the cockpit view's exterior paint past the A-pillar changes too, the cabin does
  not.
- Overcast is the section's cloud word, not `UseFog` alone (which also marks fog under a clear or broken sky: those
  now have their sun). Under it the tables hold the CIE overcast sky, giving up-facing ground a quarter of the clear
  sky's light (Kasten and Czeplak 1980); 45O wet mean 92 to 95; dusk to night smooth (2026-10-24).
- With the day's light the roads with a specular map reflect as a rough dielectric (`with_specular` in `shader.wgsl`,
  retail only): the game's `road_shine` stays beside it (the gravel's stone detail). Frame
  time ~0 (2026-10-24). On all ground it cost 0.5 ms for a few percent; on leaves the crown normals turned the canopy
  white.
- The clock's ambient share and tint scale the stage's display colours, so its light is them to the 2.2; day frames
  byte-identical (2026-10-20).
- The headlights' shadow map has its own layer of the sun's map (`sun::LAMP`), so the lamps cast at dusk too, beside the
  sun's cascades.

## Car

`car/` reads `Cars/Cars.ini`, the `.sgc`, `<car>.ini`, `<car>_shaders.ini` and textures.

- State per `SShaderMaterial::SetupRenderState`, order wheels, opaque, transparent (`car/shading.rs`): an opacity
  channel means blend, test `> 0`, no depth write; `CarPlastic` writes depth, two-sided; `CarIntGlass` two-sided.
- Not drawn (`car/`, `CHH_Car::Render`): `ghost_*`, `texture_*`, `Plane_*`, switched parts not in their state,
  gravel/snow/spare wheels, `k_` and `d_` parts; the interior `i_*` only in the cockpit, the bonnet view draws the
  exterior (user). `--hide-wheel` drops the interior parts the steering turns (one mesh on the 911 SC Grp4 and the
  i20 WRC 2020, dash and gauges kept).
- The steering wheel is the interior meshes whose node is `i_steeringwheel` (`MeshDamage::steering`), not the meshes
  centred near the node: rim and column in one mesh have their centre off the hub (Delta HF 4WD GrpA 0.17 m; the old
  0.12 m match missed it, the Escort Cosworth, Focus 2006, Impreza 95 and Polo R5 too; `tests/steering_data.rs`, 42
  installed RSF cars, all with the node). Delta frames: wheel static before, turning after; Escort, Clio R3 and M3
  cockpit shots byte-identical.
- Lighting per class from `<car>_shader_settings_clear.ini` with the stage's light (`car/shading.rs`); Schlick
  `F0 = 0.125` (`c91`). `c41`, `c67`, `c68`, `c90` unproven.
- Reflection: a 128-texel surroundings cube, as the game's; the load-time probes (`renderer/probes.rs`) for the
  car's ambient and sun occlusion, wet roads and water.
- Shadow decal (faithful, `car_shadow.rs`): `car_dpshad2.tga` on the `.col` triangles under the car, alpha by
  `Car_Deep_Shadow_Alpha`, pitch and height; none on an RSF-encrypted `.col`. `--enhance contact` replaces it.
- Playback: body and tarmac tyres from the recorded poses; steering is not recorded.
- Tyre place (2026-10-28, `rbr_recording::hub`, `WheelPose::centre`): a tyre goes at `anchor + HubOffset * axle`,
  the live process sending the offsets (HUB1; recordings carry none and keep the anchor). The 208 Rally4's and Clio
  Rally3's (-0.045 tarmac, -0.04 loose) sit 4.5 cm further in: centre 0.750 m against the model's 0.744 m, no longer
  10 cm outboard. Particles still take the anchor.
- Not drawn: `pc_e_premove` (2026-10-28, `car/mod.rs` 0x1001e090): the classifier returns kind 10, which
  `FUN_1001e430` adds to no draw list (`cmp eax, 9; ja` at 0x1001e59d, `cmp eax, 8; ja` at 0x1001e61e).
  RSF models give it a `Default Material` FaceBlock (40 of 583 installed `.ini`s; a 2.3 m white cube about the
  cabin), which showed as a pale panel behind the rear window in chase views (GC8 555, Clio Rally3, Fiat 131
  Grp4; Fiesta Rally2, M3 E30, Escort Mk II byte for byte).
- Block-compressed car textures with no mip levels get a chain at load (ours, 2026-10-31, `upscale::with_block_mips`:
  top level kept, the rest box-filtered and re-encoded in the file's format). The RSF Fiat 131 Abarth Grp4's 4096x4096
  livery, its 2048x2048 dynamic and its glass texture are DXT3 with one level and aliased in the chase view; its paint,
  dark green (0, 56, 8), reads grey under the overcast sky by the shared clear coat, its rear window's top strip is
  opaque green (0, 132, 51, alpha 255) and the beige dash panel is in its texture: the car's own art. Escort, GC8
  golden chase and the Escort cockpit byte-identical, Clio 0.001 % RMSE.
- Glass seen from outside with the day's light (ours, 2026-10-28; `GLASS_*` in `shader.wgsl`): the reflection takes
  its own weight, the pane absorbs at least half the view, so the crew dims and the sky and trees mirror (0.8 and
  0.6 read as milky fog). Only the window parts; the lamps' glass and the in-car view keep the old look. Dry day
  shots change on every car (glass only); scene pass unchanged (2.93 ms, rain fly-by). Tried and dropped: 1.7 px
  enlarged drops (aliasing speckle); lighting the cabin from outside (no visible change).
  The water film's cells (48x24, `wet_layer` in `glass.wgsl`) add a soft layer, wiper-swept areas clearer; outside
  views only, cockpit byte-identical.
- `CarIntGlass` in the in-car view with the day's light (ours, 2026-10-26; `shader.wgsl`): translucent texels darken
  what is seen through them, near-opaque ones (sun strips, visors) stay lit. A light texel (RSF 52's `i_window_f`,
  grey 134 at alpha 85) greyed the view in overcast and wet. 12 RSF cars checked; additions off byte-identical.
  Coloured texels stay lit too (chroma 0.15 to 0.3, `INT_GLASS_PRINTED_CHROMA`, 2026-10-30): the RSF M3 E30's yellow
  banner is drawn at alpha 0.47 and went dark against the sky; its clear glass is grey at 0.22.

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
- Flares (`InitLights` 0x10016fd0, `RenderLights` 0x100158b0; `lights.rs`): `<car>_settings.ini` `LightN`, key
  `kind * 4 + i`; additive quad 0.2 m toward the eye, alpha `facing * fade * 114.75`. Only the first record of a key
  with a `Switch = true` part switches. Ours: the headlight key (L) steps main beam, dipped, off (with the tail
  lights). Ours: no flare is drawn (user, 2026-10-31: the Clio Rally3's sat over and outboard of its lamps).
- Ours: the lamps show as their lit parts, which glow (`LAMP_GLOW`) day and night, a tail lamp's by day at 0.3 of it
  (`lights::TAIL_DAY`, about 8 cd against a brake lamp's 100) so braking shows (2026-10-31; golden Gabiria Escort chase
  0.6 % RMSE, night shots byte-identical). The game's rear sprites read as blobs by day.
- Ours: a lamp's lit part the game never draws (on the `light` or `brake` box, or `Predicate = NotUsed`), named
  `e_light_lf_lit`, `e_light_lb_lit`, `e_light_brake_lb_lit` and so on, follows its lamp by name, with its unlit pair
  `e_light_lb` when that one's box does not switch it (`lights::Switch::of_part`, 2026-10-31; user: "whatever is
  actually realistic"). Headlamps of 42 installed cars now glow from the front, by day too (M3 E30's dipped outer
  lamps, Fiesta Rally2's units; day Gabiria M3 and golden Luceram Fiesta fly-bys 1.9 and 1.6 % RMSE, night front
  views a white glare). Tail and brake ones glow in the records'
  red (`LAMP_RED`), not their texture's: the GC8's brake parts are white reflectors, the M3 E30's and 911 SC's lit
  textures light the indicator. 43 installed cars had dark tail lamps (night Sardian M3 chase); the GC8 brakes red
  (rain Sipirkakim chase 0.6 % RMSE), other reference shots byte-identical. A lit part the game always draws stays
  as drawn, no glow: the GC8's `e_light_lb_lit` (`Switch = false`) is its reverse lamp's reflector, which glowed white.
  Still dark: the GC8's and MAZDA 323's tail lamps (that case), the Fiesta Rally2 23 as installed (its `<car>.ini`
  lacks the `_lit` sections; both car-parts options have them, always drawn), and cars whose model has no `_lit` tail
  mesh (Focus WRC 2006: its lamp is not seen from behind at all; Delta HF 4WD, Lancer Evo II, Fiesta WRC 2014).
- Ours: each kind's light at a luminous intensity (headlight hot spot 20000 cd, tail 8, brake 100, reverse 150); a lamp
  in its state since the drive's first frame is settled, not fading (2026-10-25). The rear lamps shine in a flat band
  after ECE R7.
- Ours: the headlights' light back off the road (`lamp_bounce` in `lamps.wgsl`): two Lambertian patches facing up, 15
  and 55 m ahead; scene +0.15 ms at night (45O chase and cockpit).
- Tried for the beam's white look (2026-10-25) and dropped: pulling the meter toward the 97th percentile, and local
  exposure from 1.5 stops (halos, day highlights down). The road under the beam at 1 to 5 lux against ~50x dimmer
  moonlight is physical.
- Ours: a car with no kind 1 record that ever lights (an RSF car installed with headlights off: none, or all
  `Active = false` without their switch box, as 19 installed cars incl. the Escort Mk II, 2026-10-30) gets a beam per
  `Point_light_lf`/`rf` for the night light, in the colour of its "on" option's headlight record
  (`rsf::headlights_on`), else the retail 255, 255, 233. Points the car's `<car>.ini` lacks come from the "on"
  option's (`lights::with_headlight_points`, 2026-10-30): 9 of 108 installed cars had neither records nor points and lit
  nothing at night (GR Yaris Rally2, Fabia RS Rally2, Polo GTI R5, DS3 R5, Fiesta S2000 and R2, Grande Punto S2000,
  Kalina RC2, Mini WRC). The BMW M3 E36's option is named `Lights` (taken too; not rendered, its RSF pack is not
  installed here). The Fiesta Rally3 Evo has no `carparts/`: its `light_on/` beside the model is taken.
- Open: the writer of the lamp words on damage; `RenderLights`' second visibility test.
- Ours: the headlights' shadow map (`sun::LAMP`, `lamps.wgsl`) shades the beams beyond the stage (2026-10-27): rain,
  snow, dust and mist (`lamps_at`), each step of the beams' march and the wet-road glints (`lamp_glint`). Only with
  `Enhance::lighting` and a drawn map (else a zero matrix reads lit): day and shadows-off frames byte-identical.
  41M start chase in fog cuts the air into shafts (mean diff 0.32, max 147/255); scene/particles/shafts within noise
  at 1440p (+0.06 ms at most). The retail skid marks keep their unshadowed lamp light.
- Foliage cards in the headlights' shadow keep half their light (`LAMP_THROUGH` in `night.wgsl`, as `sun_light` halves
  the sun's shadow on cut-outs; 2026-10-30): a tuft's opaque foot shadowed the next tuft's whole, a black bar under each
  tuft along the verges (41M 23:00 chase, Gabiria night chase). Night shots 0.7 to 2.4 % RMSE, day byte-identical.
- Rear lamps are capped by a soft knee on their summed light (2026-10-27): a brake lamp's 100 cd saturated red within
  a few metres under the night's exposure. Maps 450/369 fly-by 22:00-22:15 and 466 chase 23:00 show small lamps and
  a faint wash; headlights unchanged (map 126 night chase MAE 0.0004 to 0.0005).

## Damage visuals

`damage.rs`, `damage_parts.rs`, `dirt.rs`, `crack_decal.rs`. Inputs: `DMG1` live, `--damage`/`--crack` by hand.

- Deformation (`damage.rs`, bit for bit with the original, `tests/damage_data.rs`): per subtype-3 panel over the 64
  zone lists; normals kept; vertex alpha is the scrape amount. Interior panels without a morph follow their twin
  (ours).
- Cracks (`damage.rs` `Cracks`): code 3 the front window, code 4 a side window or (index 4) both rear halves; slot
  limits 2, 1, 1, 1, 1, 2. The texture is ours, a generated decal at `WS_Broken.dds`'s star positions.
- Detached parts: moved rigidly by the `DET1` body pose (ours; the game's motion after `OnDetachFromParent` is
  untraced).
- Dirt (`dirt.rs`): each tyre payload adds `mass * 1e-5`, capped 0.9; `CarPaint` lerps by the `Dirt` map. Ours: the
  colour, tyre dirt by `TireDirt` (never made in 1.02), side and rear glass grime (at most 55 %, the wet mud keeping
  40 % of the reflection).
- Dirt by surface (ours, `dirt::CarDirt::look`): the level from the mud and snow thrown, 0.3 of the water and 0.01 of
  the rest, so dry tarmac stays near clean (0.1 still filled a car to 0.79 in 98 s of clean tarmac, 2026-10-28);
  colour from the stage's dust (dry) to the game's mud tinted by it (wet, keeping 35% of the reflection) to packed
  snow by the snow's share; dirt maps' alpha raised to its square root; none on the cabin's interior set (with
  `cabin`). Cost within noise (9.06 vs 9.05 ms); additions off byte for byte (2026-10-27).
- Wheel and mud flap spatter (ours, `dirt::spatter`): parts named `e_wheel*` and `e_mudflap*` take the dirt colour as
  a film plus spots by the level. No brake dust: the rim is in the wheel's mesh.

## Particles

`particles/` throws the rally's `PS_*.ini` (`rbr_formats::pss`); sprites from `particles.dat`. `--no-particles`.

Each thrower follows its 1.02 source (`particles/*.rs` cite them; `E82F_BlkD0::Emit` 0x5ddba0 emits `rate * frame
dt` a payload, we use 1/60 s). The payload layout is in `rbr_recording::particles` (wheel, edges, velocity, rubber,
smoke, mass, depth, dirt, splash, surfaces, blend, contact). Ours: the step, pools 4x the file's, sparks (1.02
throws none), no 3D stones (never loaded in 1.02). Open: camera-speed modulation, the wave's vertex alpha and
scroll.

Wheel spray (`particles/wheel_spray/`, ours, `--enhance particles`): replaces the tyre handlers, fake smoke, splash
and wave with dust and water only (no stones, user decision 2026-10-20); dust, powder, mist, drops and splash by
surface, wetness, slip, speed and standing water. The splash payload +0x98 stayed 0 in every drive, so 1.02's own
splash never shows on a wet stage.
- Dust light and count (2026-10-27, `fs_soft` in `effects.wgsl`): by day the soft particles take the stage's light,
  not the game's ambient through their tone curve, which left dust darker than the road it came off (64_E heavy fog
  06:45: dust 61,49,35 against road 77,60,52); half the dust (60/s a wheel) looks the same and costs less (64_O fog
  fly-by 1080p particles 5.76 to 4.11 ms). Night unchanged; additions off byte for byte.
- Dust settles, mud does not mist (2026-10-27): dust no longer rises; shallow gravel and sprinkled tarmac throw
  less; wet gravel, dirt and grass throw a quarter of a hard road's mist (e.g. 450's `Dirt_Hard_Wet` corner cut
  beside dry tarmac threw a wet road's plume).
- Snow powder (2026-10-27): was alive but unseen (white on snow); now 70/s a wheel, 0.8 to 7 m, 3.5 s (45/s too
  thin). Particles pass +0.9 ms at 1080p (208 Sturec chase; Escort similar).
- Wet spray (2026-10-28): fully wet ground has pools of its own (the old `Mist` stays the damp haze, so dry
  weather's damp gravel is byte-identical). Effects pass 1080p: Arbroz chase 0.87 to 1.13 ms (Sipirkakim within
  noise).
- Wet spray sheet (2026-10-28): the road's mist was an opaque white wall; alphas and streaks thinned. 450's
  wet-surface corner in dry weather changes too.
- Mud plume (2026-10-28): `MudMist` lumpy and low, born at the contact patch. Effects pass Arbroz 1.09, gc8
  0.62 ms.
- Wet spray round 2 (2026-10-28; test frames stage 21 chase/fly-by `--at 5600`/`5700`): `MudDrops` are dark clods
  thrown back and out keeping most of the car's speed; `Splash` is a sheet with a white `SplashMist`; `WetMist`
  hangs. Dry, damp, dust and powder byte-identical. Effects pass 1080p: 41O chase 1.18 to 0.98 ms (52O within
  noise).
- Wet spray look (2026-10-28): the road's plume uses a holed sprite (`sprites::WISP`), fewer and denser, starting at
  the tyre. Effects pass 1080p: Luceram fly-by 2.28 to 1.63 ms (cockpit similar). Dry, dust, powder, snow and night
  byte-identical; damp shots change where wheels cross wet patches (spray keys on the surface's wetness, not the
  weather), by decision the same as in the rain (Gabiria damp chase: 129 of 2247 wheel frames on wet tarmac).
- Dust plume (2026-10-28, `sprites::DUST`, `fs_soft`): dry dust is its own sprite with a sunlit rim and a
  self-shadowed core, young dust dark tan thinning with age; rate 120 (was 60), damp ground an eighth of dry. New
  `Debris` pool: streaked dark stones off the tread. Rain, fog, snow shots and 580's chase RMSE 0; dry gravel shots
  change by design.
- Dust plume round 2 (2026-10-28): fewer, crisper puffs (`DUST_RATE` 120 to 40). Wet shots RMSE 0.
- Dust close up (2026-10-29; chase views read as opaque white cotton balls in shade): within 3 to 9 m of the eye a
  puff gets no outline of its own (radial falloff times the blurred sprite's lumps, peak 0.5), cut into wisps at the
  edge (golden Odry chase dust wholly in shade: luma 135-185 against the lit road's 100-120, now about 125-145).
  gab-lit RMSE 0.003, day Gabiria 0.0004; additions off byte-identical. The tan blocks in the fog Odry chase are the
  game's own stones, as on main.
- Dust sun shafts (2026-10-29, `dust_sun_reach` in `effects.wgsl`): a puff takes the sun's visibility averaged over
  its depth, so tree shadows print as soft shafts (fog 532 fly-by RMSE 0.022, fog Odry chase 0.028, Sipirkakim
  0.007, day Gabiria and 621 fly-by 0); other particles keep `sun_reach`. Effects pass in the 532 fly-by 2.6 to
  3.7 ms. Tried and dropped: peak alpha 0.9/0.55 to 0.7/0.4 (thinned gab-lit's and Odry's); `DUST_SUN` 2.2 to 1.6
  and a 0.75 far veil (dimmed gab-lit's far more than the 532 plume, so it stays a bright tan body).
Soft-particle cost (2026-10-26, RX 9060 XT): the effects pass is fill-bound (Sipirkakim II `--at 15000`: 926
quads, ~400 screens of coverage, 35 ms at 1440p, nearly all `fs_soft`; GPU clocks bimodal). Fixes, frames within
2/255 of before (lamps' code out of the day's pipeline, fragments under 1/1000 alpha discarded, depth after the
erosion test). Medians of 3 interleaved runs, 1440p: dusty chase 59.7 to 16.1 ms (4K 137 to 39); wet heavy rain
cockpit 68 to 15, Falstone bumper 18.7 to 3.7. Rain streaks and mist cost under 1 ms of that.
`--frames` is steady state: particles built once, not per frame (the drawn-only frame's long 'encoding' is the CPU
waiting for the GPU).
Wet mist (2026-10-26): it had never shown (edge erosion ate the 0.28-alpha mist). Now a wisp (grey-white 0.32
alpha, 2.5 s, up to 3.4 m wide, 120/s a wheel at 28 m/s). Damp: a `Haze` pool at a third of the rate, 1.2 s, no
drops. Additions off unchanged; wet chase within noise of main.

## Windscreen and wipers

`glass/`, ours (`glass/model.rs` documents the chart, inputs and wiper schedule: 1.3823/1.3195 rad, 0.4 s each way,
`--wipers`, key I). Drops are lenses over the resolved frame; a wet film blurs it. Camera dirt on outside views is
not ported.
- Look (ours): mud specks are opaque (the HDR sky through a 5 % gap read as grey); with `cabin`, seen from inside, a speck's thin rim lets
  3 to 45 % of the light behind through in the mud's hue, its core dark (2026-10-27; glass pass 0.14 ms before and
  after, off frames byte-identical). Tried and dropped: a dashboard reflection in the windscreen (user's call).
- Water redone (ours, 2026-10-31; `lens`, `film_cell`, `ribbon` in `glass.wgsl`; `flow`, `merge` in `model.rs`): a drop, bead or
  ribbon is clear water, not a white cap: it lays 0.75 (0.55 for a ribbon's weight) of a lens over what is drawn behind it
  (the pane's haze included; replacing the pixel left drops darker than the hazy glass), the frame seen mirrored through
  its centre 3x its offset, a thin dark outline (darker below), a lit upper-left side and a small highlight. Pixel offsets
  come from the quad's screen axes (`Out::pixels`). Beads: two layers a film cell (5 and 2 squares a side), shown for
  `film^0.3`; under two pixels a bead has no outline. Film: the frame through a wavy sheet (noise slope, 2 px), 4 taps
  streaked along the airflow, patchy; no flat veil. Sim: rain radius 1.2 to 7 mm (cubed), 800/m2/s still; each drop grips its
  own (0.7 to 1.5), runs down to 0.6 of its pin, slides faster as r^2 (at most 0.6 m/s); drops that touch merge (volume kept,
  every 1/60 s, swept by x); drops from 2.2 mm lay a ribbon (kind 7, 2.5 s) every four radii from their own volume and a
  bead beside it (5 s), so they shrink and stop; a blade leaves 0.1 of the film and clears water within `BLADE_PAD` (2 cm)
  of the angles it swept, so none shows over the moving blade. Wiper schedule and drizzle mapping unchanged.
  Cost at 1080p heavy rain (GC8 Luceram, `--profile --frames 120`): "particles, rain" 1.12 to 1.18 ms, drawn-only frame 7.88 to
  7.98 ms. Dry and snow cockpits and chases byte-identical. Open: heavy rain at a stop reads less speckled than before (fewer
  tiny white specks, by design); the 131 Abarth shows no windscreen water at night (its glass is not charted, as before);
  `--sequence` with `--live` now ends on `--at` (the car moves between frames; before it started there and stood still).
- Beads (ours, 2026-10-26): the film's cells draw the small drops too many to be drops of their own. The film builds to the rain's amount, less with the airflow, and the wipers clear
  the beads with it (drops alone covered 1.3-2.1% after 12 s; more drops cost ~5 ms a step at 100 km/h). Shows with
  additions off too (user-approved); cockpit heavy 13.1 ms before and after.
- Drizzle (ours, 2026-10-31; launcher "Drizzle" = `Precipitation::Light`, intensity 0.5): `Precipitation::shower()` is
  the intensity squared for rain (0.25 for drizzle, 1 for heavy, snow unchanged); the falling streaks' count and the
  glass's drop and film rates use it, so drizzle is a sparse faint shower with fine drops that build slowly. The wipers
  read the raw intensity: at 0.6 or below intermittent (`DRIZZLE_MOST`), heavier rain continuous. Heavy rain shots
  byte-identical.
- Side windows (ours): each side window its own glass (drops, trails, film, mud; no wipers, no bugs), drawn on its
  exterior pane; 0.3 of the windscreen's rain and 0.1 of its spray, landing low.
- The pane itself from inside (ours, 2026-10-30, with `windscreen`; kind 6, `pane` in `glass.wgsl`): a cell per film
  cell, drawn under the drops: Schlick loss through both faces, a patchy dust film scattering the sun (forward and broad
  lobes, the stage's shade from sun cascade 0) and the sky, thinner in the wipers' sweep, specks, dried water spots, and
  arc scratches that glint where `dot(t, sun - view) = 0`; side windows half the dust. Dropped: dust in arcs along the
  blade (noise stretched along the arc read as even concentric rings). The cabin reflected in it (2026-10-30): the
  windscreen's mean plane is a mirror pass of its own (`set_windscreen_mirror`, the mirrors' last image layer, drawn as
  the centre mirror's, every frame), weighed by `F (1 + (1 - F)^2)` for the inner face and the outer seen through it; it
  shows over dark backgrounds and the tinted band, barely over the lit road. +1.7 ms at 1080p (208 cockpit: mirrors span
  1.2 to 2.9 ms), most of it the stage to 300 m behind the rear window. Tried and dropped: a screen-space march over the
  frame's depth (the dash under the glass, what the glass mostly reflects, is hidden from the eye behind the dash's
  crest). Normal: the chart's mean plane (the grid's stepped heights striped per cell). A cell is its four corners
  exactly (the low one and the others' offsets, `Drop::far`), each from its integer film corner: drawn as a centre and
  two axes the non-planar cells cracked into dark lines along every third cell edge (where the 48 x 24 film corners
  meet the 64 x 32 height grid's edges) and pinholes; a hair of overlap instead showed a bright grid (Clio Rally3
  Wythop, 2026-10-30). The film corners lie on the glass's triangles (off the glass at the nearest point's height;
  the grid's stepped heights cut the film in stairs where it meets the door trim, and the nearest triangle's plane
  threw cells off the glass), and each cell is cut at the glass's outline over a pixel by its corners' signed distance
  to it (`Windscreen::edge`, `Drop::edge` over `PANE_EDGE` 5 cm): kept or dropped by corner, the film ran up to half a
  2 cm cell past the glass in 50 px stairs (Clio Rally3 A-pillar fringe; the Escort's pillar edge). The outside's
  light on the pane (its dust, specks, spots and scratches) is weighed by what the
  window part's texture lets through, `1 - alpha` of its blended faces (`glass::Tints`, read at load from the
  interior set's window parts, `MeshDamage::window`), at 4 steps across each cell (`gpu::PANE_TINTS`, 5 x 5 samples
  of 4 bits a cell; at the corners alone the sky's light lay on the strip's lower 2 cm, a pale band over the black,
  Clio Rally3 (42, 43, 35) against the strip's (9, 17, 8), now the strip's): the Clio Rally3's printed sun strip glowed brown
  (0.37 against 0.05 with `windscreen` off) and now only shows the cabin reflected in it. Dust 0.4% (was 1%: the view
  went milky toward a low sun; dark trees +0.15 in display value, now +0.08). Against onboard footage and matched
  frames (2026-10-30; M3 E30 621 13:00 heavy cloud, Fiesta Rally2 Luceram 08:30): the glass hazes toward a low sun and
  reads clear away from it, with a few faint streaks and no visible dots or rings. Dust 0.6% with the broad lobe a
  tenth (was 0.4% and 0.3); scratches a third as strong, half as many, each a run of its arc (`SCRATCH_RUN`, about
  0.3 of it: whole rings read as a regular pattern of arcs); specks a quarter as many and at most 0.4 mm (orange flecks
  on the side windows); dried spots only as the glass has been wet (`Renderer::wetness` or the precipitation, so none on
  a dry stage), 0.8 to 2.5 mm with a soft wavering rim (drawn rings). The dust's sky light a third (`DUST_SKY`
  0.001, was 0.0032: an even milky veil away from the sun, the golden Clio's view's deviation 34 against 46 without
  it, now 40), and the cabin reflected in the outer face weighed by the texture twice (the strip keeps only the inner
  face's: the Clio Rally3's switch panel over its strip 56 to 35 of 255). Left: the display reflected at the strip's
  edge (lit texels at the Fresnel of 60 degrees, physical; a viewing cone on the backlight changed nothing, its
  display is not backlit at 18:30).
- Dials in the reflection at night (2026-10-30, `backlit` in `shader.wgsl`): the windscreen sees the dash some 75
  degrees off its faces, and the flat dials of the meshes showed whole and at their full backlight (Audi quattro A2,
  Helette 22:30: the gauge cluster mirrored over the road). In a mirror's pass a backlit texel shows what a recessed
  face does past its bezel, `1 - DIAL_RECESS tan` of the angle off its face (0.25: 7 % at 75 degrees); the main view
  is unchanged (the A2's dash byte-identical; a recess there hollowed the orange dome lamp that stands out of the
  dash). The dials are 25 cd/m2 (`DIAL_GLOW` over `NIGHT_PRE_EXPOSURE`), their 8 % reflection as bright as the lit
  road, and local exposure takes the dials themselves down: the pair read alike. Left: the dome's reflection (a lit
  lamp), the Clio Rally3's display at the top of the glass (seen 45 degrees off from above, physical). The bulkhead (`glass::bulkhead`, with
  `windscreen`): a dark strip hanging 0.6 m from the glass's foot in the interior set, drawn as its first opaque lit
  face, added after the cabin light's bake (seeing no sky) and drawn only in the in-car mirrors' passes, casting no
  shadow (`Renderer::set_reflected_only`). The sets leave gaps between the dash and the glass that the driver never
  sees past; the reflection showed the road under the car and the sky through them (Clio Rally3: a ragged white patch
  over the dash; with the stage left out of the pass it turned to sky). Drawn in the main view it hid the Escort Mk
  II's footwell (upright glass), and baked it took the cabin's sky share from 0.280 to 0.267; now the Escort is
  unchanged and the Clio changes only in the gap; a few dim lines stay at the dash's edge. Tried and dropped: sloping
  it back under the dash at 45 degrees (lit grey facets, a larger gap). Outside views byte-identical.
  Cars without `i_window_f` (RSF M3 E30) have no glass at all.

## Skid marks

`skidmarks.rs` (pure, documents the emission, rings and fade), `skidmarks_pass.rs`. Records from the `PS_*.ini`
"Skidmarks" section and its aliases; `_low` files have none.

Measured (ours): the payload edges are 0.20 m apart; records scale them by 2.0-2.2 and the strips fill ~85 %, so 1.02
marks show ~37 cm, laid along the axle. The payload's velocity +0x50 is not the patch's; rubber is 0-1 in the game's
payloads, a slip speed in m/s in playback's from poses. Snow drives run mostly on `Snowwall_Bottom` (0x58).

`--enhance marks` (ours) replaces them: width 0.21 m loose/snow, 0.235 m tarmac, swept across the travel. Gravel/dirt:
rut with tread print, light berms; tarmac only when sliding; snow groove; grass flattened; wet darker, a sheen at
grazing views. Sections every 1 m, 800 m a wheel, the last 250 m fading; marks fade 90-220 m from the eye. A seek
replays only the 4 s warm-up, so captures show short trails.

## Ghost

`ghost.rs` (documents the mesh, look, pose and placement), `rbr_gameplay::ghost`, `rbr_formats::ghost`. Ours:
`--best-ghost`. RSF ghosts (`.gch`) are not supported (user decision).

## Characters

`characters/` (documents the spawn, triggers, visibility, crew and scripted events): `.lbs` `(2,14)` items in
`(2,15)` random groups, Lua 4.0 scripts, `.sga` clips, CPU skinning. Generators: the exe's LCG and msvcr71 `rand()`,
`--seed`. In-car crew: chase views only, as the High renderer. Tests: `tests/characters_data.rs`,
`scripted_data.rs`, `crew_data.rs`.

## Fences and dials

`fences/` (documents the cloth, spans, posts and textures): `.fnc` lines as `CFenceHandler` cloth, pushed by the
car's `c_e_body`. With the own light (ours) the lines carry their normals; fence meshes skip `on_foliage` and
`with_plants` (the 45_O net was a "plant", glowing in patches). Without the own light they stay facing up;
additions off equal on 64_O, 45_O, 61_M but 1 to 2 pixels by 1 (shader codegen). Cost: +0.17 ms scene at 1440p with
a net filling the view. A torn line's rewrite has occlusion 0 (no bake), as before. `dials/` (ours): an RSF car's
needles and displays from RBRHUD's gauges, drawn into the dial textures from the HUD records.

## Enhancements

`renderer::Enhance`, `--enhance a,b` or `all`; each off (but for the always-on HDR and TAA) leaves the frame as the
game draws it. The launcher's Quality section (`graphics_page::Graphics`, user decisions 2026-10-29) sets them by
preset (Low, Medium, High by default, Ultra) or one by one: `anisotropy`, `decals` and `water` always on (free), the
four scans as one "Surfaces" switch, `wet` with `--reflections` as "Wet roads" (off; low: no screen-space rays;
medium; high), `grass` as "Dense grass" on High and Ultra (user decision 2026-10-30: a half-density, shorter
field cost more frame time than the full one at 4K, so no middle setting), `windscreen` as "Windscreen" on High and
Ultra (user decision 2026-10-30: one switch for the pane and its reflection). Removed: `lens`; `ao`, `msaa`, `taa`,
`wind` (user decision 2026-10-19); `hdr` (2026-10-26: float targets, bloom, exposure and AgX are always on). Older settings and names are
not read (user decision). The launcher's render scale is 100 % by default. The default frame was
byte-identical before and after (chase, cockpit, heavy rain, 1920x1080).

| Name | What |
| --- | --- |
| `anisotropy` | 16x anisotropic stage samplers |
| `cabin` | cockpit lit by its baked sky share, sun only through glass, interior set clear of the stage fog; with the day's light the interior set takes it as the lit draws outside do (45O Xsara and RSF Evo IX, dry, wet, fog; scene pass within noise; additions off byte-identical, 2026-10-25); the car's glass drawn from both sides, so the far window seen through the near one tints too (the game culls it) |
| `contact` | soft contact shadow in place of the decal |
| `decals` | soft ground decals blended after the stage instead of cut at 60/255 (`decals.rs` documents the rule: RSF 129, 369, 370 skid marks and 129 sand patches; no retail draw; the factory's blend switch untraced). Upright soft cut-outs keep the cut: blended, the wire mesh vanishes |
| `asphalt` | the paved roads (`Vertex::wet` 1, `puddles.rs`) take the asphalt scan (Poly Haven Asphalt Track, CC0, 1024 packed, layer 1, every 2 m; the same strengths). Launcher "Surfaces", default on |
| `gravel` | the loose roads (`Vertex::wet` 0.5, `puddles::mark_roads`, dry too) take a scanned gravel surface (Poly Haven Gravel Ground 01, CC0, 1024 packed, embedded; `renderer/road_scan.rs` layer 0) tiled flat every 3 m; the road's mean brightness and colour stay (below). Launcher "Surfaces", default on |
| `lighting` | our lighting in place of the game's look (2026-10-27: one flag for the former `sky`, `sun`, `occlusion` and `clouds`; the half-on looks, the 2D cloud layer and its cloud-face lighting went): the physically based sky with aerial perspective, height fog, light shafts and volumetric clouds with their shadows; sun shadow cascades and sunlit foliage in place of the baked light maps; ambient occlusion and bounced light computed at load; reflection probes; the day's exposure. Details below |
| ~~`hdr`~~ (always on) | float targets, bloom, histogram exposure, AgX (`post.rs`); the exposure's limits follow the clock's light, heavy rain or snow a fifth darker, heavy cloud 15% more saturated. Local exposure: where the bloom blur is over 2 stops above middle grey, 0.6 off, so the night beam keeps the road's texture (45O cockpit 23:00: 24% clipped, now none); post pass 2.91-2.94 ms; additions off byte-identical (2026-10-25). Tried and dropped: a 97th-percentile cap (blacked out everything around the beam). Daytime exposure under our sun lighting (`Enhance::lighting`, 2026-10-27; `post::SUN_EXPOSURE` 1.5, `DayLight::sun_share`): 41M chase noon lit grass (188 207 141) to (166 184 124), road 190 to 137; 45O noon grass green 165 to 121, road 178 to 115. 1.5 keeps sunlit grass above a photo's 120-140 green; 2 crushes the shadows (31 to 24). AgX contrast follows the adapting light (Stevens effect): day p95/p99 up ~10 levels, p5 down ~6; post pass 3.34 to 3.35 ms; additions off byte-identical |
| `marks` | our tyre marks in place of the skid marks (Skid marks; launcher default off) |
| `particles` | GPU rain and snow, rain rings, lit soft particles, chimney smoke, wheel spray. The rain or snow is decided once (`Scene::precipitation` in the viewer; `Rain::of_weather`, `Precipitation::of`): `--precipitation`, else with `particles` ours by the road, else the game's by the preset. Rings only in standing water, gone past 60 m over the view's cosine (2026-10-25). Rain looks (2026-10-26): 196000 faint streaks at full rain in place of 56000 bright needles, plus a fullscreen rain mist (a fifth of the lamps' light at night); no drop falls inside the cabin; zero without rain or `particles`, so additions off stay byte-identical. Streak look (2026-10-28): `--flyby`/`--camera` eyes take zero eye velocity (they took the car's ~25 m/s, slanting streaks ~40 degrees); `SHUTTER` 1/30 to 1/60 s; dry and damp shots byte-identical. About +0.5 to 1 ms on 45O chase wet, the denser drops another +1 ms on heavy; 30 chase frames without flicker. Drizzle (2026-10-31): `Precipitation::shower()`, the intensity squared, sets the streak count and the glass rates (light rain 0.25, heavy 1) |
| `snow` | the snow (`Vertex::wet` -1, `puddles::SNOW_MARK`, takes no film) takes the snow scan the same way (Poly Haven Snow 02, CC0, 1024 packed, layer 2, every 2 m; the factor no darker than 0.88). Launcher "Surfaces", default on |
| `ground` | the off-road ground (`puddles::mark_ground`, `Vertex::edge` -1) takes a scanned grass-and-earth ground (Poly Haven Grass Ground, CC0, 1024 packed, layers 3 and 4 of the road scans; `ground_scan` in `shader.wgsl`) triplanar at 1.7 m and a 5.1 m tiling turned 30 degrees; bright grey ground takes only brightness, no darker than 0.88. Round 2 (2026-10-29): `ground_patches` adds earth and grass in patches 2 to 6 m across and stones of 0.3 and 0.9 m cells. Launcher "Surfaces", default on |
| `grass` | not in `all` (launcher "Dense grass", default off; the reference shots take it): crossed cards of drawn grass blades over the green and straw ground within 20 m of the driveline (`grass.rs`), streamed by 8 m tile about the eye into 550 fixed tile meshes in 9 pools by card count (a tile takes the smallest free one holding its cards, bare ground none), drawn within 118 m (`Lod::Close`), full within 50 m then halving every 25 m, flattened into the ground from 100 to 110 m; each tile holds 1.5x its density (by its nearest point) on 24 steps and the vertex shader shrinks each card by its rank against the density where it stands (`GRASS_THIN`), so the field glides instead of popping; swayed by the wind in the vertex shader (`grass_sway`). Tiles are made on a worker thread (the near 24 m on the frame itself, uploads capped; tiles ahead of the motion built early). Lit as a dense canopy (`GRASS_*` in `shader.wgsl`): tighter wrap, sky and sun shut out down the blades, a low sun more; the ground's computed sky occlusion, sky direction and bounce read back once at load (`Renderer::grass_light`). 580 chase 1080p scene 5.05 to 5.14 ms (+39k triangles), Gabiria fly-by 3.76 to 3.62 ms (-77k triangles); ~280 MB video memory (2026-10-30; ~540 MB since the reach below). Look (2026-10-30): cards 0.07 to 0.14 m (were 0.1 to 0.2), their foot lighter (`GRASS_BASE` 0.6, `GRASS_SUN_BASE` 0.4 to 0.6), so the field reads as a low meadow, not dark fur; the retail tufts toned to sit with it (`TUFT_TRANSMIT` 0.35). Snow shots unchanged (RMSE under 0.005). Retail tufts lit as our grass (2026-10-30): the stage's tufts and ferns take our grass's light (`GRASS_*`: wrap 0.3, sun and sky shut out down them, no warm tips), keep 0.4 of the sky's colour (`TUFT_SKY_TINT`: their grey-green textures read bluish), no dark-leaf lift, green leaves at most 0.09 linear (`TUFT_GREEN_MOST`, was 0.15; 580's lime ferns read as lit card). Their luma over our grass's in the same 48-pixel tiles: 580 Evo chase 2.4-2.5 to 1.2-1.3, 580 shot 3.7 to 1.5, 532 1.34 to 0.89, Gabiria's pale straw 2.1 to 1.9 (its texels sit in the snow test's range: a straw cap missed them, and moving the range changed Pian's snow tufts). Snow shots RMSE under 0.0035, night Sardian identical. The tile meshes not yet given a tile (no bounds, zeroed cards: 306 of 560 at the Luceram shot) drew their whole capacity in the main view and the mirrors until skipped (2026-10-31): scene 0.5-0.7 ms less (580 chase 5.09 to 4.59, Luceram rain fly-by 3.83 to 3.18), mirrors 0.45-1.0 ms less in the cockpit, the 34 shots byte-identical. Their zeroed vertices no longer go through the load's vertex prep (2026-10-31): all in one place, they made `foliage::with_plants`'s tree grouping quadratic; 580 load 28 to 22 s, the 34 shots 594 to 441 s, byte-identical. Tried and dropped: lighting our blades as upright on banks (no visible change; 580's dark bank is the hill's own shadow). Thatch (2026-10-30): with a field (`GRASS`) the ground under the grass is darker (0.82) and fuller in colour (1.1) by the grass's own test, slope and density by the eye's distance (`thatch` in `shader.wgsl`; not the 20 m band, so the near ground past it is thatched too), so its pale and pink texels no longer show between the blades; the blades brighter (`SHADE` 0.62 to 0.9), less saturated (`SATURATION` 1.15 to 0.85, `LUSH_SHARE` 0.25 to 0.15: lit, 580's came out at blue over green 0.26 against the ground's 0.63, now 0.37 against 0.57) and fuller (240, 180, 180, 140 blades a picture, were 170, 130, 130, 100; the same cards). Blades over the ground between them (median of 32 px tiles): 580 Evo chase 0.51 to 1.12 and 0.31 to 0.86, Gabiria fly-by 0.79 to 1.29, 532 0.48 to 0.90; the field's mean brightness 580 Evo +0.5% and -14%, 580 shot +8%, Gabiria +4%, 532 -8%. Snow shots unchanged; scene pass 580 19.8 ms and 532 11.8 ms both before and after. Reach (2026-10-31): full to 50 m (was 14), halving every 25 m (was 10), to 110 m (was 100); the tile meshes sized by their cards (a full 6400-card mesh for every tile in reach would be ~1.2 GB; sized on 580, 621, 532, 450, 417, 176 and 52, a few of the farthest at Odry 462's densest spot go without). Meshes never filled drew their whole capacity of zero-area cards (no bounds: never culled), about 0.5 to 2.4 M triangles a frame before: now they draw nothing until filled. Full density to 100 m measured 580 chase 29.4 to 39.0 ms a frame, 621 cockpit 12.5 to 29.4 ms (with the empty meshes still drawn); this reach, after the fix: scene pass 580 chase 20.1 to 21.0 ms, 621 cockpit 7.8 to 8.4, Odry chase 10.8 to 8.7, Gabiria fly-by 11.4 both |
| `water` | water flowing by its scroll (`water.wgsl`), fading into banks |
| `windscreen` | the windscreen seen from inside as glass: the pane (Fresnel loss, dust, specks, wiper scratches, water spots) and the cabin reflected in it, a mirror pass of its own every 2nd frame whatever `--mirrors` says (held with the car between) ("Windscreen and wipers" above). Off: the frame as before |
| `wet` | the wet road, in wet and damp weather: a film and puddles over the ground whose `.col` surface takes them (tarmac 1, gravel and dirt 0.5), retail and RSF alike (user decision 2026-10-25; `puddles.rs`). Screen-space reflections on the wet roads: +12 to 17 ms at 1080p; film alone +0.9 to 1.7 ms; load +0.2 s. Wet film (2026-10-28): the rays missed the car, so the film halves back to any crossing and `reach` judges it (`film` in `trace`); the far road takes the sky, the near road stays dark. 41O chase screen reflections 1.32 to 1.50 ms at 1080p. Loose roads (2026-10-28): the half-film road's film was a pale even sheet; its reflection now falls with the film's share, so the stones show through and puddles stand out (tarmac and dry byte-identical; 52O/21O chase screen reflections 1.46/1.67 to 1.14/1.37 ms). The rings were never on the film: they were in the puddles. Damp is a drying road: no puddles, rings or screen-space reflections, the film 0.4 of the wet one's, the ground soaked by half (41M dry 101, damp 68, wet 59 at 13:00; 460M 147 to 131; 45O 140 to 118). 41M +0.9 ms, 45O within noise; wet and additions off unchanged byte for byte (2026-10-26). Wet road edge (2026-10-28): the reflection and glint fade over the outer 0.1 to 0.8 m, frayed by noise (puddles and the road's darkness stay); mean distance 1.2 m on 41M, 0.3 m on RSF Gabiria; damp and dry byte-identical. By texture (2026-10-28): a film triangle takes the share most film triangles of its texture have (RSF Bois de Bie tags stretches of tarmac as gravel: 46000 `Gravel_Fine_Shallow_Wet` against 139000 `Tarmac_Coarse_Clean_Wet`); retail Fraizer Wells byte for byte, damp Gabiria RMSE 0.0003. Wet road look (2026-10-28, tarmac first): the film alone is darker and its reflection takes the asphalt's grain, its normal tilted by microrelief and rain's impacts, fading to the mean between 4 and 25 m. Tried and dropped: a 0.12 tilt (white sky blobs over the near road). Gravel nearly unchanged; dry, snow, night byte-identical. Flyby 1080p scene 1.72 to 1.80 ms, screen reflections 1.58 to 1.85 ms; cockpit within noise. Tarmac's even film (2026-10-28, user decision): where the film share is 1 there are no random puddles and only the hollows hold water, so the bumper and plate read in the road near the car. Rain rings on tarmac only in the hollows' water; the film shows fine splash specks instead. Round 2 roughens the normal at the grain in place of the 5 and 2 cm waves. The near road's olive cast is the green bank's reflection, kept. Gravel and dirt keep the film and puddles (52O chase RMSE 0.0000022); dry, damp, snow and night-dry byte-identical; Luceram fly-by within compiler noise (RMSE 0.00015). Puddles at the edge (2026-10-28): puddles now end 0.05 to 0.2 m from the edge, frayed as the film is; 52O chase RMSE 0.017, dry byte-identical. Wheel tracks (2026-10-28, loose roads only): the water lies in two tracks 1.5 m in from the road's edge, rims frayed by 7 cm stone noise; damp keeps its old spray. 52O chase scene 2.54 to 2.65 ms, screen reflections 1.69 to 1.74 ms; dry, damp, snow, night byte-identical, Luceram within compiler noise (RMSE 0.00015) |

Gravel scan (2026-10-28): procedural stones read as cobbles or coins, so a scanned surface's detail over the
stage's own colour reads as gravel (`renderer/road_scan.rs`). `pack-scan.py` decodes the sRGB-encoded normal/height
PNGs by the normal's mean. Parallax looked the same in stills (off 5.04 ms, +1.39/+0.65/+0.43 at 1440p); none kept
(+0.08). Off, frames byte-identical to before (Clio damp, 621 dry, Sipirkakim wet, Gabiria tarmac); on, a dry stage
loads about 1 s longer. 1024 against 2048: the same grain at 4K (road std 19.48 against 19.45). Open: some RSF stages
tag stretches of tarmac as gravel (Gabiria's road edge takes the scan, faintly); Semetin and Sardian show no change.

Asphalt scan (2026-10-28): Poly Haven Asphalt 01, 06 and Track tried on Gabiria, Luceram and Helette; Track (fine
aggregate, brightness varying 29% about a dark mean) gave Luceram's smooth tarmac crisp aggregate. Road mean within
1.3%; on the wet road it shows faintly under the film. Scene pass at 1440p, Luceram fly-by: off 4.57 ms, on 4.66.
Both off, and gravel alone, frames byte-identical to before (Clio damp, Gabiria, Luceram wet, 621 dry); the
derivatives stay `dpdx(world / tile)` per layer (`dpdx(world) / tile` moved up to 8/255 on 1754 pixels of the Clio
fly-by).

Asphalt look (2026-10-29; `albedo_remap` and `ground_scan` in `shader.wgsl`): the scratchy tarmac at Gabiria was
`albedo_remap`, not the stage texture's flecks (road std 18 of 119 on a flat patch, scan off too). With `asphalt`, a
paved road takes the cut of its local mean with its own variation over it, reads its texture blurrier where magnified
(painted lines stay crisp), and gives up the stage's high pass; the scan carries the grain. Road patch mean 119 to
97, lines, patches and shadows kept; gravel and snow shots byte-identical. Tried and dropped: a noise-broken road
edge (only frayed the shoulder's flattening). The local mean is the texture's mip itself, read 6 levels past the
footprint: quads stretching the texture differently read different mips and 45's road showed light and dark blocks
(gone; Gabiria 79.2 to 79.4, 45 and 417 within 0.4; the whole texture's mean also removed them but lightened
Gabiria's road 79 to 91). Blur by distance (2026-10-30): the texture is read no finer than 4 cm a pixel
(`ASPHALT_FINEST`, widened gradients) in place of the blur by magnification, which was decided per quad and left a
blotchy band ahead of the car that snapped sharp (RSF Fintele 523); the road is even, a little blander mid-distance.

Snow scan (2026-10-28; `SCAN_FLOOR`): Poly Haven Snow 02 on Komarov, Peklo, Pian and Sturec. Its dark marks read as
dirt streaks on Peklo's banks, so the factor is held at 0.88 or brighter (0 for gravel and asphalt, which stay
byte-identical). Road mean within 0.3%. Scene pass at 1440p, Peklo fly-by: off 3.26 ms, on 3.34. Off, the snow
stages' frames byte-identical to before (Komarov, Peklo, Pian).

Ground scan (2026-10-28; `ground_scan` in `shader.wgsl`): the hillside's blue sheen and the verge's blue-grey band
were the stage's baked vertex tint, not specular, sky ambient, fog or the film (each off alone: RMSE 0.004 to 0.02,
the sheen stayed). It keeps 0.3 of itself on every near ground draw. Hill mean colour within 1 of 255 per channel
(96,100,76 to 97,98,79), as the scan keeps the stage's mean. Cost: scene pass at 1080p, Gabiria fly-by, 2.7 to 3.5
ms on, 3.0 ms off (GPU noise larger than the effect); memory +11 MB, load unchanged. Ground off, every other
addition on: byte-identical (Gabiria, Komarov snow, Sipirkakim). Tried and dropped: the scan at 2048 (no visible
difference at a 1.7 m tile). Open: at grazing views the scan's detail stretches along the view (physically right),
and the target's brightness (146,131,98 against ours 97,98,79) is the light's, not the ground's.

Rock keeps its hue (2026-10-29; `ground_rock`, `ground_scan`): rock, stone walls and concrete take the ground
scan's and the earth patches' brightness only, so cuttings and parapets keep their own colour; a flat grey verge
(gravel) keeps the tint. Rock to grey: rock gives up 0.85 of its lit colour for grey on softer steepness.

Calmer verge (2026-10-29; `ground_pale_ease`): the verge's pale texels past 1.1 of the texture's local mean are
eased, within 12 to 30 m of the eye. Tried and dropped: a dirt-and-gravel strip on the paved mesh's non-grey band (on
some stages that band is the whole grass verge and bank).
Shoulders as ground (2026-10-29; `SHOULDER_WALL` in `shader.wgsl`): the paved mesh's grass and earth shoulders take the
ground's treatment, so no seam shows beside the terrain (580's straight dark strip; its bank turns mossy earth). RMSE 580
3.6%, Luceram 2.1%, Gabiria 1.2%, Semetin 0.6%, gravel and snow stages and additions off identical; 580 4K scene pass
8.40 to 8.92 ms.
Grey test at a fixed mip (2026-10-30; `ASPHALT_BLUR`): read 3 levels past the footprint, the test reached 580's verge
(painted beside the road in its texture) from about 8 m out (footprint mip 1), so the far road took the ground scan as a
shoulder: green and pink patches moving with the car (Evo chase replay; gone with asphalt or ground off). It reads mip 3
at any distance, the near road's level before. 580 road patch chroma std 6.4 to 2.9; shots RMSE 580 0.0034, Gabiria up
to 0.0042 (its dirt strip on the paved mesh reads as road further out), Luceram up to 0.0033, Sardian up to 0.0044
(calmer curb streaks), Sturec snow 0.0052 (the wall top takes the shoulder's earth, as near the car), the rest under
0.001 or identical.
Far hillside (2026-10-29; `ground_far`, `ground_clumps` in `shader.wgsl`): from 8 to 18 m out the ground texture
is read blurrier, its pale texels eased, the scan's chroma and grain eased, and grass clumps added (value noise at
1.5, 0.7 and 0.32 m; snow stages left alone). The speckle on the hill is mostly the stage texture's own (there with
the scan off).

Lighting, in pieces (all with `lighting`; each measured on its own before the merge):

- Sky: Hillaire sky, aerial perspective, height fog, light shafts; the tables are stored in display colours once per sky
  change (same frame within 1/255, 2026-10-20); under a clear sky the ground haze hides 70% no nearer than 7.7, 4.6 or
  2.3 km (Koschmieder visibility 25, 15, 7.5 km; 2026-10-27), the day's mist 95% at 33 km.
- Probes, occlusion, bounce, contact: reflection probes (`renderer/probes.rs`) along the driveline replace both
  per-frame cubes (headless 117 to 256 probes in 0.44 to 1.43 s, up to 67 MB). 41M 1080p drawn only 21.4 to 20.3 ms
  wet, 10.3 to 8.2 ms dry; additions off unchanged byte for byte (2026-10-25). Ambient occlusion from the stage's
  geometry at load (`sky_occlusion.rs`; 0.1 s on 41M, 0.5 s on Semetin, 2026-10-23): unlike the bake's `sky_share`,
  the computed one holds no sun shadows. With it the stage's light is ours (`own_light`): the sky 0.56 of it times
  the sky seen, the sun 0.79 by its facing past the cascades, so a shadow is the sky's light alone; the bake gives
  only the hue. Every stage within about 10% of the bake-driven frame's brightness; the weak fits and the unbaked
  gain the sun's modelling, dusk at -3 degrees the twilight sky's light (45O mean 15 to 24 of 255). The light off the
  stage (`bounce.rs`): drawn once at load (no running time of day, 2026-10-25); 0.1 to 0.4 s (Semetin), the bake
  0.1-0.5 to 0.2-0.9 s. Below the horizon, then also above (2026-10-26): Valstagna's shaded cliff +20% (41.6 to 49.9
  of 255). Tried and dropped: light through the leaves (nothing visible at 3x sun). Partly shaded places brighter
  (42O chase +38% mean, 460M +19%, 176M +13%), frame time within noise. With additions off the frame moved by up to
  14 of 255 on 88 pixels of 41M's two characters, unexplained. Contact shading: half size, on the sky's and bounce's
  light only; +0.4 to 0.8 ms over the mask alone (2026-10-25; full size +2 to 3 ms). `Detail::Ultra` reflections
  (2026-10-27): the surroundings cube at 256 texels, a face every frame, the probes baked at 128 texels, and the wet
  screen-space reflections traced at full size. 41M 1440p: dry +0.05 ms, wet screen reflections 1.75 to 3.4 ms; High
  byte for byte (6 scenes). Sky domes (`sky_dome.rs`): a world sky face over the stage's middle casts into neither
  the sky's occlusion nor the cascades (39 of 659 readable RSF variants, e.g. 519, 580); as a caster it left 519 and
  580 black. Under `lighting` the dome is not drawn at all (user decision; 2026-10-27).
- Sun: cascades and a far layer to 1 km in place of the baked light maps, leaf translucency (`sun.rs`,
  `foliage.rs`); the characters cast into the cascades, and the car's body and the characters take them (the car
  stays out, its own map shadows it). A plant taller than grass is a crown (`trees`: the retail trees are three wings
  about 120 degrees apart); its cards fade out edge-on and take their sun from the crown (Beer-Lambert out of its
  ellipsoid). 41M 1080p drawn only 3.93 to 4.11 ms. Tried and dropped (2026-10-26): per-tree tint (snow firs turned
  brown on 10O), leaf-gradient normals, per-pixel crown normals (slight), turned card copies (square tops, darker),
  interior transmission (lost the backlit glow). Vegetation look (2026-10-28; `tuft_*` in `shader.wgsl`, `foliage::with_plants`):
  grass, low bushes and shrub crowns get bent normals, wrapped sun with transmission, foot occlusion and a straw tint
  on pale textures (retail tufts read bluish under the blue sky); gab-lit 1.9% RMSE, additions off identical. Each piece's cut-out test runs once a piece over a summed-area
  table: Semetin's load 7.4 to 2.3 s, 41M 1.08 to 0.67 s, frames identical (2026-10-25). Left: RSF house and prop
  atlases with cut-out gables and fences still count as plants. The main view's sun shadow is worked out once per
  pixel before the stage is shaded (PCSS, 3x3 blocker search). Semetin 5.39 to 6.06 ms at 1920x1080 (41M similar,
  2026-10-25). The prepass's normals go to an `Rgb10a2Unorm` target of their own: the prepass 1.0 to 0.52 ms at 1440p
  (4K similar); with TAA, frames differ on 0.3-1% of pixels, mostly by 1/255, dusk and night byte-identical. The first
  cascade's depth range per 4x4 texels (`ShadowRange::sun`, 0.035 ms per frame) lets the mask skip where every texel
  is before or behind the point: frames byte-identical, the mask up to 4x faster (dusk 1.7 to 0.4 ms; 25O, 41M
  similar, 45O unchanged). The main view draws the lit opaque draws before the other stage draws: frames
  byte-identical on 45O and 41M, scene 1440p ~0.2 ms faster (45O chase 2.23 to 2.02 ms; 2026-10-26). `Detail::Ultra`
  (with `lighting`): the maps at 4096 texels instead of 2048 (400 MB of depth instead of 100), so foliage and trunk
  shadows on the road are cleaner; the far layer's texel 0.5 m. 1440p 25O, cascades 0.55-0.6 to 1.4-2.5 ms (+0.9 to +1.9;
  3072: +0.5), scene within the GPU's noise; `High` byte-identical to before (25O, 41M, 45O dusk and night). Tried and dropped: no alpha sample in the prepass for opaque draws and no `world_lit` where the
  cascades reach in full (2026-10-26; the whole sun term, `own_sun_seen` apart from the car, under 0.03 ms); the same reordering for every opaque
  non-prepassed world draw (no gain); single shading terms (each 0.1 ms or less, 48-72 VGPRs, no spills); far layer
  to 2.5 km (2026-10-27; +0.1 ms, nothing more seen); contact shading at full size (+0.1 to 0.2 ms, edge pixels only).
  Fewer casters (2026-10-30, `--profile` times each layer): the far layer is drawn again only after 50 m of travel
  or a change of light or size (`sun::FarDrawn`); the grass's tiles (`Lod::Close`) go into no layer and no cube; the
  backdrop's meshes are culled by each map's sides. 1080p Ultra, 580 chase: sun maps 7.36 to 3.92 ms, surroundings
  cube 0.82 to 0.52, CPU drawn-only 15.4 to 12.2; Gabiria fly-by 3.83 to 1.64. What is left is cut-out fill at 4096
  (cascades 0.5/1.5/1.7 ms; 0.2/0.3/0.3 without cut-outs). Frames differ only in the paint's cube (no grass) and the
  asphalt. Tried and dropped: alpha from a coarser mip in `fs_sun` (-0.5 ms on cascade 3, changes coverage), cut-out
  casters sorted front to back (no gain). The mirrors draw only the rain's near layer (the far one's drops are under a
  pixel there): 4K Luceram rain cockpit mirrors 0.91 to 0.72 ms, the mirror images unchanged to the eye (2026-10-30).
- Clouds: the main view's clouds are volumetric (`renderer/cloud_volume.rs`, `cloud_volume.wgsl`; by night lit by the
  moon) and the game's cloud faces are not drawn (2026-10-25). The layer's kind by the cover: clear cumulus 1.2 km up
  under cirrus, partly 2.2 km thick, light stratocumulus 1.1 km up, overcast a closed deck 0.9 km up. A sunlit layer's
  shape is cut at most at 0.65 of its weather, the weather's threshold lowered to keep the cover (cut at the full
  weather they stood as flat-topped columns, 2026-10-27). Each section has its own field start and drift. About 0.7
  to 0.8 ms on a 1080p open sky; additions off and all but the clouds unchanged byte for byte. With the volume shown
  the ground's shadows come from the volume itself (a 512² R8 map over 8 km about the eye, redrawn each frame;
  `cloud_volume::shadow`, 2026-10-27). Overcast, night, and additions off byte-identical; 1440p scene medians within
  noise of before.
- Merge (2026-10-27): the four names are no longer accepted (`--enhance sky` is an unknown name); `LIGHTING` is the one
  pipeline override (was `SKY_LUT`, `CONTACT`, `PROBES`; `SUN_MASK` stays apart, it needs the pass). The 2D cloud layer
  is gone (`clouded`, `cloud_shape`, `cloud_face_lit`, `clouds.wgsl`): the cloud field's noise, `cloud_turn` and
  `cloud_shadow` live in `cloud_volume.wgsl`. `--enhance all` is byte-identical to before
  in 13 scenes. The sky in the baked reflection probes, wet roads, water and the car's paint no longer holds the 2D
  layer (up to 1/255 on a few thousand pixels, 3/255 at Valstagna's sea). `--no-relight` with lighting on stays a debug state.

Albedo and shadow colour (2026-10-27, `lighting`; `examples/albedo_survey.rs`): terrain mean linear albedo (Y,
saturation): 41M/45O grass 0.12-0.13 (0.73), 61M dirt 0.18 (0.66), 10O snow 0.77, grey road/rock 0.33-0.36 (0.41 on
32M's limestone). Commonly quoted ranges (from memory; not checked): grass visible 0.08-0.15 (sat 0.6-0.7), dry soil
0.15-0.25, fresh snow 0.8-0.9, worn asphalt 0.05-0.2, concrete 0.3-0.5. So grass, dirt and snow are within range; only
neutral mid-bright surfaces run 2-3x high and grass is a little over-saturated. `albedo_remap` (`shader.wgsl`):
neutral surfaces with Y 0.2-0.6 lose up to 40% (snow over 0.7 untouched), saturation over 0.5 eased by up to 15%.
Shadows are not a light fault (41M 15:30 tree shadow on grass: lit sat 0.71, shadow sat 0.56, cooler and greyer as
expected); the shadowed grass reads more saturated on screen because AgX desaturates the bright lit grass, so the
shared tone curve was left alone. Haze: `CLEAR_REACH` 15, 9, 4.5 to 7.7, 4.6, 2.3 km (crisp, plain, hazy) and
`DAY_HAZE` 18 to 33 km. 41M 2560x1440 scene 2.49 to 2.51 ms; additions off byte-identical on nine scenes.

Shadow lookups skip only taps that cannot change the pixel (2026-10-20: cockpit scene pass 13.9 to 11.5 ms at
2560x1440, frames equal). Tried and dropped: a min/max depth map of the cascades, corner taps first, a 1024 car map.

Cabin light: only the car's lit pipelines carry `CABIN`, and the open sky is a branch, not a `select` (2026-10-20:
cockpit scene pass 13.6 to 11.5 ms at 2560x1440, frames equal but 2 pixels by 1). Tried and dropped: an RG11B10
bloom chain. Shade about the car (2026-10-30, `cabin_shade`): the probes read a forest overhead about as bright as
the open sky (1.1 to 1.27 of it on 450, 523, 532) and their sun reach stays 1 under most trees, so the cabin kept its
light in shade. The first cascade at 5 x 5 points about the cabin now dims the window light to 0.7 where all is shade
(weighed by where each face looks out) and scales the sun bounced inside; per vertex (the sun map is visible to the
vertex stage), +0.05 ms at 4K (per pixel it was +0.6). Wythop 532 cockpit frames in shade darker, open and
overcast frames unchanged.
Window colour (2026-10-30, `cabin_window`): what lies outside the seen way is taken against the overhead per channel,
as `open_sky_light` against the mean; against the overhead's luma the sky's blue came in twice (in `day_sky` and in the
cube), and the Clio Rally3's white cage on Wythop 532 lit blue (blue minus red 78 to 33 of 255, the dash 40 to 18).
Day Gabiria Escort, night Sardian Clio, fog Alpes GC8 cockpits unchanged (20 pixels past 8 of 255).

Cabin bounce by the day's light (ours, 2026-10-30, `cabin_day_lit`): the constant inside floor and the sun's back share
are gone. A closed room's interreflection gives the bounce: E = (1 - sky) x rho / (1 - rho) x the mean light the
interior takes in, the sky's through the cabin's mean seen direction and the sun's mean n.l (`SunIn`). rho is the
area-weighted interior albedo (texel at each face's uv centroid at 64 px or less, times the vertex colour, per channel,
at most 0.9; the bounce per channel, so a tan cabin's is warm), over the faces that see the sky at most 0.6 to 0.9 of
the time (`cabin::OUTSIDE`): the interior set holds outside body panels, and the RSF M3 E30's green livery (13.7 of its
28.7 m^2) lit its headliner teal (albedo 0.099 / 0.183 / 0.146, now 0.095 / 0.112 / 0.106; roof 19 / 44 / 34 of 255,
now 23 / 29 / 28). The light in (sky share, seen direction, the sun's n.l) is weighed so too (2026-10-31, with the
cabin's own exposure below): the body seen through the glass takes in light the cabin does not, and counted it made
the bounce 1.5 to 2.7 stops too bright (before, under one exposure, it took the M3's roof to 7 of 255 and was left
out). The day's window light and sky inside take `IBL_TINT` as the outside's ambient does (no visible change).
Measured (luma, the viewer's `cabin:` line) sky share (was) / albedo / bounce: Escort 0.090 (0.280) / 0.104 / 0.116,
GC8 0.068 (0.343) / 0.056 / 0.060, Clio Rally3 0.055 (0.361) / 0.084 / 0.092, 131 0.087 (0.222) / 0.152 / 0.179, 208
0.057 (0.296) / 0.087 / 0.095, Fiesta 0.059 (0.236) / 0.043 / 0.045, Yaris 0.059 (0.198) / 0.467 / 0.877, M3 E30 0.092
(0.380) / 0.108 / 0.122. Dark cabins fall off to the roof and
footwells, the white Yaris shell is lighter; the window term is untinted (`CABIN_DAY_RANGE` 0.03 to 4). The glass: the
sky's share through two clear faces, `(1 - F)^2` over the cosine-weighed hemisphere, 0.848 (`CABIN_SKY_THROUGH`; the
former 0.85 was this); the sun's from the blended panes facing it, weighed by area toward it, per measured direction
(`SunIn::through`; 208 at 19:30 about 0.8). Left heuristic: `CABIN_SHADE`, `CABIN_GAUGE_LEAST`.

Exposure in the cockpit (user's call, 2026-10-31; was one exposure, the cabin metered at 1/2 of the outside, before that
2/5): the outside and the cabin (nearer than `post::CABIN_REACH`) are metered apart (`cs_meter`, 128 bins each), as the
eye adapts to the dash when it looks there. The outside's meter counts the cabin at 1/5 (`IN_CAR_WEIGHT`): at 1/2 the
dark cabin held the outside 1.5 to 2 stops over (Escort road 168 of 255; metered alone 91). The cabin takes its own
exposure, eased as the outside's, at least the outside's and at most 4 stops over it (`CABIN_STOPS`), its darkest
values lifted by a toe (`CABIN_TOE`, 1 stop at black, a sixty-fifth at middle grey), in a composite pipeline of its
own (`CABIN`). The glow and the light shafts keep the outside's exposure (at the cabin's they lit the pillars with a
green haze); a pixel takes the cabin's only when its 3 x 3 are all cabin (a softer blend drew a dark band inside the
pillars, or a bright halo outside them). Chosen against more cabin light (the light is right, above), a shadow-side
local exposure and the curve's toe alone, then from cabin weights 0, 1/5, 1/2 by 3, 4, 5 stops. Eight day cockpit
shots, means: outside 149 to 125, cabin 35 to 62, pixels at 8 or below 24.9 to 14.0 %, cabin detail (mean |luma - its
5 px blur|) 4.3 to 7.5; the two nights within a few levels. Post pass 0.29 to 0.36 ms (Escort cockpit, 1080p); the 24
outside shots byte-identical; 4 s of driving (Escort, Clio) hold the cabin within 2 levels, no pumping. Left: glass
that writes depth (the Clio Rally3's `CarPlastic` side windows) and the rear-view mirror count as cabin, so what is
seen in them takes the cabin's exposure (about 1.5 stops over on the Clio); the forest shots' door cards stay dark (5
to 7 stops under the road, which their light is).

Interior sheen (2026-10-31, `cabin_sheen`): every cabin face but paint open to the sky (the bonnet through the glass,
`CABIN_OUT`) reflects as a low-gloss plastic, `environment_reflectance` at roughness 0.8 (2 to 3 %) of the cube's
level 3, where the reflected ray leaves the car (`cabin_open`). A class with a Fresnel kept the paint's reflection: the
Clio Rally3's dash is `CarPaintInt` with `Kf` 1 from `FresnelMin` 0 (9 installed cars; 70 have 0.21 / 0.44) and read
as glass; the rest took a smooth F0 0.04, a third of the sky at a grazing angle. Mlynky 135 Clio cockpit 19:30: dash
top 108 to 85 of 255 (no sheen 86). Cockpit reference shots 1 to 2.6 % RMSE by day (Fiesta's grey dash 6.5 %), nights
0.02 to 0.04 %, outside shots byte-identical. The dash still reads blue-grey: near-black texels under the cabin's own
exposure and the sky through the windscreen.

The car's sun shadow (2026-10-30, `car_shadow`): a penumbra by the sun's disc and the casters' mean distance (3x3
search, PCSS as the cascades), taps a texel apart, at most 9 a side and the range's reach. The fixed 4x4 taps 4 cm
apart over 3 mm texels drew a 1 cm grid of steps where the sun came through the glass (208 binnacle, 19:30). 580 208
chase RMSE under 1 %; cost not measured.

## Mirrors

Ours: `mirrors/` (detection, views), `renderer/mirrors.rs` (passes), `mirrors.wgsl`; `viewer --mirrors off|1|2|3`,
launcher "Mirrors" (default every frame). Off: no passes, the frame byte-identical (6 views, 2026-10-19).

- Found at load from the cockpit eye (`Mounts`, `mirrors/mod.rs`): the `*mirrorglass*` parts (else the `*mirror*`
  housings' faces) and the opaque glass-class faces (retail cars), in panes of welded, coplanar faces. A pane is a
  mirror when the eye's line reflects back along the car, it is 3-45 cm, 0.2-1.5 m ahead, at most 45 cm below the
  eye, flat to 4 % of its size, and the eye sees it past opaque non-glass faces; one per side and one centre. No
  named centre mirror (RSF): the largest opaque pane hanging there. Not flat: the cube as before.
- Survey (`examples/mirror_survey.rs`): the retail cars 2 door mirrors each, Lancer Evo VII 1 door + centre, Impreza
  2000 2 + centre; the 42 RSF samples 1-3 each but the Fiesta Rally2 (none); curved glass in 12 keeps the cube.
- View: `crop * VP * R`, R the reflection across the glass, crop the glass's screen box with x flipped, z row for an
  oblique near plane 1 cm past the glass. Image about 1:1 with the glass, 32 px steps, at most 1024 x 512. Rates take
  turns between mirrors.
- Under TAA each image is drawn with its own sub-texel jitter (Halton by the layer's draws), which the glass samples it
  by, so the main view's resolve gathers its edges (2026-10-31, no cost). The windscreen's curve shears its image low on
  the pane: an edge along a texel row showed as fins about 10 texels long, halved by a 2048 image (tried for Ultra,
  +1 ms) and only softened by a blur or FXAA; the jitter with TAA's glass-over-the-car rule removes them at 1024.
- Passes draw the stage to 300 m, sky, fog, the car, ghosts, the game's particles, rain and lamps, with the frame's
  sun cascades and exposure; no prepass, light shafts, cubes or bloom. Only the glass's pipelines have `MIRRORS`: on
  in the shared shader it cost 1.1 ms a frame at 1440p (M-series).
- Cost over Off, Evo X R4 cockpit, min of 5 (2026-10-19): every frame +0.66-0.82 ms at 1080p, +0.71-0.75 at 1440p,
  +0.71-1.13 at 4K; every 2nd +0.29-0.45; every 3rd +0.18-0.34.
- `--profile` times each layer (`mirror 1` to `4`, `windscreen reflection`). RX 9060 XT, 1080p ultra cockpits
  (2026-10-31): the windscreen's 1.2-1.5 ms every frame, the mirrors' 0.4-1.0 each; the stage in the windscreen's
  only 0.3 (the cabin hides it, a reach of 10 m costs as 300), the rain in it 0.07. Since drawn every 2nd frame (user's
  call, 2026-10-31): 532 Clio cockpit GPU frame 12.02 to 11.08 ms, mirrors 2.01 to 1.34.

### Cost on an M3 Pro (2026-10-29)

GPU frame time, `viewer --screenshot --frames 60` mid-drive (`render-shots.py`'s placement), 1080p, M3 Pro 14-core
GPU; each choice off from the launcher's High. Seven shots: 580 day chase, 52 gravel, 450 cockpit, 417 heavy rain
chase and cockpit, 126 night, 413 snow. Repeats within 0.5 %.

| Choice | Cost |
| --- | --- |
| `lighting` | 3.6-13.9 ms (18-53 %; 580 day most); load +1.1 to 6.2 s (occlusion, bounce, probes) |
| `particles` | 4.7-6.2 ms heavy rain (rain pass ~4.8, more in the mirrors), 1.5 snow, 0.2 dry |
| `wet` | 3.1-5.6 ms in rain, 0 dry; `--reflections` high to off 2.5-3.5 rain, 1.2 dry 580 (water); medium 1.0-1.6, low 1.6-2.2 |
| `--detail ultra` | +3.6-6.1 ms (+14-38 %; cascades 6.1 to 11.8 ms on 450) |
| `grass` | +3.5-4.2 ms; most in the cascades (+2.3) and the surroundings cube (+0.8), waiting profile |
| `--mirrors 1` | 0.1-0.3 ms dry cockpit, 2.9 heavy rain (the rain drawn in them ~2.2); every 2nd/3rd by the rate |
| `cabin` | 1.0-1.4 ms in the cockpit |
| `asphalt`, `ground`, `contact` | 0.5-1.0, 0.4-0.7, 0.1-0.8 ms |
| `--anti-aliasing off` | -0.5 to -0.75 ms |
| `gravel`, `snow` | 0.1-0.2 ms |
| `anisotropy`, `decals`, `water`, `marks` | within noise |

Presets 1080p (native 3024x1964 on 580, 450, 417 x2): Low 5.7-8.8 ms (8.0-13.3), Medium 7.2-13.6 (13.1-32.1, measured
with low reflections), High 11.4-26.2 (23.7-47.6), Ultra 15.9-33.9 (33.1-63.7). Native at render scale 67 %: 15.9-27.5.
Lighting on with the rest cut saves only 1.5-2 ms dry. Tried: sun maps at 1024 saved 0.4-1.9 ms (the cascades are
geometry-bound), so no cheaper lighting tier by size.

### Performance log (2026-10-26)

Launcher Graphics page "Performance log" (`perf_log = on`, default off) passes `viewer --perf-log <files folder>/logs`
(`perf_log.rs`, `profile.rs`, `bin/viewer/perf.rs`; both document the files, spans and timestamp modes). Two files
per session, named by the start in UTC: `perf-<YYYYMMDD-HHMMSS>.log` and `.trace.json` (last 600 frames, written at
exit, so a killed viewer leaves only the `.log`).
- `viewer --frames N --perf-log DIR` logs the offscreen frames too (header `present mode offscreen`). Frames are
  byte-identical with it on (1080p `--enhance all` screenshot `cmp`). Overhead median +0.08 ms a frame (45O, 7 runs,
  640x360 and 1440p). Untested without a display: the window path (`App::frame` spans, `Renderer::end_frame`
  after the present's submit).

`--frames N --profile` prints per-pass medians. Texture packs (`bin/texture-pack.rs`, never committed):
`viewer --textures <dir>/pack` reads them, the car's after its dials are made (dial faces keep theirs). The compact
`realesr-animevideov3` beats `realesrgan-x4plus` on retail liveries and signs (cleaner flat edges, 1024 livery 4.0 vs
20.7 s; RX 9060 XT, 2026-10-26); both garble non-Latin sign text. Cap 4096 (the default): Fiesta R5 28 s, 257 MB
against 79 s, 833 MB at 8192, its livery text as sharp but the smallest line.

Load-time upscaling (`upscale.rs`, ours): `viewer --upscale-textures DIR` (the launcher's "Textures: upscaled",
`upscale_textures`; DIR `<player folder>/Upscaler`, filled by the launcher's "Download upscaler…") upscales the
car's and the stage's textures with Real-ESRGAN General WDN v3 4x (halved for 2048 sources, 4096 at most),
after the car's dials are made, cached by the source file's SHA-256. Model choice (10 tried): WDN v3 (~4.4 s for a
1024 livery) crisper on lettering than UltraSharp and Nomos8kSC (both ~14 s) with the grain kept; animevideov3 and
digital-art flatten grain, digital-art and x4plus invent letters. ACCENT 7 textures 17.2 s first, 0.2 s cached;
stage objects ~25 s first; the ground's atlases 8 to 14 s more. Cached, a stage loads in under 1 s more. The
backdrop and the course tape showed no difference. Missing textures go through one run of the binary over a folder:
track-41_M with ACCENT 73.6 s before, 27.0 s (2026-10-26). Tried and dropped: shrinking 2048 sources to 1024
before the 4x (4.9 s; smears them).
Dial textures (the cockpit of a car with an RBRHUD gauge pack, `Image::Dial`) are upscaled too (user's call,
2026-11-01), from the car's own texture's size: RBRHUD's 13 bases larger than their car's (the Clio Rally3's 4096
against 2048) are smooth resizes of it, their finest octave 0.45 to 0.71 of the car's detail. The needles and
displays keep the pack's texels, scaled to the target's. Clio 4K cockpit: edges crisper, the dash's grain smoothed,
the small switch labels garbled. Reference cockpits 0.35 to 2.86 % RMSE, upscale off byte-identical; the target
gets the base's mip chain (+21 MB at 4096, the dials redrawn into every level: a Clio redraw 0.92 ms GPU against
0.60, `--profile` with the dials redrawn each frame).
Not packaged: the download takes Real-ESRGAN v0.2.5.0's release zip (macOS universal, Linux and Windows x86_64; none for ARM Linux) and the
model's ncnn files from upscayl/custom-models at 500e6a0, each checked by SHA-256.

## Lighting sheet

`scripts/light-sheet.py --a VIEWER [--b VIEWER] --out DIR [--quick]` (documents its views, hours, weathers and
summary) renders seven fixed views at 1920x1080 with `--enhance all` and writes one sheet per view and `summary.txt`
(mean, maximum and share over 8 and 32 of 255 of the a/b difference). The same binary as a and b differs on under
0.01% of pixels (a few up to 50 of 255: shadow and particle noise); judge by the mean and the shares, not the
maximum. Quick with two binaries: about 2.5 to 4 minutes.

## Open

- Sky dome and cloud transforms and inputs (`SkyDome.vso` `c41`-`c66`); stage in-scatter not reproduced.
- Cut-out backdrop strips with black transparent texels (Odry's panoramas) show a dark rim along the ridge: the
  alpha cut falls where the filtered colour is half black (462 `HOR_1`, `HOR_2`: transparent texels (0, 0, 0)). A fix
  bleeds the colour into them at load (the textures upload as BCn, so a decode and re-encode) or reads such textures
  as premultiplied; either changes every cut-out with black transparent texels, trees included.
- Stage `c90`; kind-byte blend bits; reflection items `(1,7)`; water texture; mip levels beyond the 4 stored.
- Wheel set shown; `Plane_*` shadows; detached node motion; `pc_e_window_*` panels.
