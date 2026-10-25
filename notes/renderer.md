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
- Highest quality always: the full `PS_*.ini`, the high renderer's LOD and visibility distances.

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
  4x MSAA with alpha to coverage on alpha-tested draws; `--anti-aliasing off` (`AntiAliasing::Off`, launcher
  `anti_aliasing = off`): one sample, the alpha test alone, no dither (2-5 ms less a frame at 1440p,
  M-series, 2026-10-19).
- Pipelines are specialised by override constants (`pipelines.rs`): the lit class (`LIT`, 0 stage, 1-4 the car's
  classes), the car's draws in or outside the car (`INSIDE`, picked per frame in `draw_scene`), its draws all of the
  cabin's interior set or none (`CABIN_SET`; the interior's carry no fog), the draw's frame uniform (`FRAME_KIND`:
  the stage's, the clouds' or the sky faces') and the draw's features (`KNOWN`,
  `DRAW_*`: water, specular map, second texture, mip bias); each carries only the code it runs.
  Code a pipeline never runs still held registers: Metal System Trace's `graphics-compiler-spill-events` showed
  200-470 byte spills per thread in the stage and car pipelines on an M1 Pro (no Dynamic Caching, unlike the M3).
  Native x 0.75 on the M1 Pro, every enhancement, the RSF Porsche's real textures: road and chase views 51-57 to
  15-20 ms a frame, the cockpit 188 to 20 ms; frames as before to within a few edge pixels. Live, the launcher's
  settings (1280x720, 0.75): 107 to 156 frames a second; full screen, cockpit: 12 to 54 (2026-10-25). The fog
  (`fogged`), the sun's cascades and the car's shadow still spill (about 150-310 bytes) in the stage pipelines.
- Small arrays indexed in loops (`harmonics`, `texel_filter`) are written out: Metal kept them in memory.
- The sky (gradient, clouds, sky faces) draws after the opaque meshes at depth 0 (the viewport's depth range) with an
  `Equal` test, so the samples they cover are not shaded; the frame is the same as with the sky first (13 scenes), but
  with `--anti-aliasing off`, where 10 pixels of a spectator's cut-out edge on track-54_E differ by up to 3/255 (a tie
  the order decides; the clear colour does not show there). 4K cockpit on Semetin at dusk in heavy rain, every
  enhancement: 36.7 to 31.3 ms a frame (M3 Pro, 2026-10-20).
- Run to run (Metal, M-series, 2026-10-20): the pass after the depth prepass keeps the alpha test. Without its
  `discard`, which of two cut-out fragments tied at the laid depth a sample kept changed between runs: 66_E's
  spectators and twigs differed in about 1 run in 3. With the test, 18 of 18 runs match, and so does the frame
  without the prepass; the stage pass's GPU time is unchanged. Still open: with 4x MSAA only, the ground's triangle
  edges differ by up to 35/255 (33_N at 2000, about half the runs, even with only `anisotropy`). This happens on the
  first frames and now and then mid-run, while the vertex uploads, the draw list and the matrices are bit-identical.
  From the fourth frame on, 24 of 24 runs matched; captures that compare frames should render a few frames before
  the shot (`--frames 3`).
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

cachy, 4K 60 Hz Hyprland without tearing for our window, user's settings (cockpit, 4x AA, immediate; mailbox the
same), scripted flip, 3 runs each, p50 (p95) ms from the flip: before (input read before the acquire, latency 1, the
previous pump's answers drawn) input read 7.5 (16), answer taken 25 (33-45), submitted 41 (49-62), GPU done 51
(58-71); after: input read 9 (16), taken 11 (16-32), submitted 18 (21-49), GPU done 28 (30-58). On screen
(`wp_presentation` feedback, `latency/shown.rs`) with the `rerbr` window rule: 41 (46), direct scan-out, vsync, no
tearing; without the rule 61 (61), composited. Hyprland 0.56.2 never tears (`isTearingBlocked` returns "next frame is
not torn" before the only code that sets it; upstream #16102), whatever the rule or cursor setting. The MangoHud layer
(the user's global `MANGOHUD=1`) added p95 tail (49.5 against 45.9 on screen); `play.sh` sets `MANGOHUD=0` for ours. A patched
0.56.2 (Arch 0.56.2-4.2) tears: `activelyTearing` true, not vsync, but `directScanoutBlockedBy: software
renders/cursors` with hardware cursors on too (`no_hardware_cursors` 0 set at runtime with `hyprctl eval`, the cursor
buffer in KMS): on screen 33.8 (40.3) against 33.9 (39.8) with the setting's auto, so the cursor is not what blocks it.

Live (`rbr-standalone --live`, `live/realtime.rs`, the only live mode; the viewer has no pacing of its own): the
stepping process runs a frame per physics step on its own clock (sleep to the step grid; a late wake's frame takes the
missed steps, the frame clock cuts it at 0.1 s), each with the newest input; the viewer's input thread
(`window/feed.rs`) polls the devices (`rbr_render::devices::Input`, the game's `Tic` over our bindings) every 1 ms and sends on change, with a
50 ms keepalive, one-shot flags ORed until a frame takes them; a 52-byte STP1 record after each stepped frame
carries the force, the car's control and the input number to the reader thread, which sets `Wheel::set_magnitude`
and the crew's wheel; answers stay one per 7 ms of game time (the frames' events and last tyre payloads folded,
`Live::picture`). `--frames-out` logs each frame's payload, periods, steps and FRM1-body digest; `--frames FILE|-`
runs such frames instead of the clock and answers each, compared where they carry a digest: a log replays, and the
tools that step a drive themselves (`Live::lockstep`: the captures, a live `--screenshot --at`, `rbr-sound`) write
one unchecked frame a step. A frame a step equals the old 7 ms payload under 1.02 (its step is 7 ms); under NGP it is
one 1/720 s step. cachy, 35 s scripted: flip to the first step p50 1.64 (p95 1.84) ms (sent to first
step 1.14 (1.56)); 720 steps/s, one per frame (3 of 25175 frames two); the clock's wake p50 0.075 ms late (p99 0.28);
on screen 42 (51), since the frame still draws a 60 Hz frame of an answer up to 7 ms old. Wheel (`harness/ffb_trace.sh`
preload, autopilot, 25 s): 526 level changes/s against 43/s paced (RSF about 510). Replays: scripted 17999 of 17999
frames, autopilot 17994 of 17994 frames equal.

RSF (drive2 sessions, RSS1): NGP's input thread polls the wheel every 1.1 ms (POL1); the game thread runs one
StepForward of one 1/720 s step per frame at about 580 frames a second (`vSyncActive=0`), the car's control (CTL1)
takes the newest poll, a poll change reaches the car's control in p50 1.0 ms (p95 1.6) and its step's end in 1.2
(1.7); the user's Hyprland rule gives the game window `immediate` (tearing), so the frame scans out as it is
presented. Ours stays a frame of 60 Hz rendering behind that for the picture: the wait for the read, the build and
the GPU; the input and the force are RSF's in real time.

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
| Ghost | mesh, look, pose, files | race-time origin, best ghost, RSF ghosts |
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
  `CELL_REACH` 315 m (300 m far plane times high's 1.05; `HH_H24_VisSelect` 0x10062dd0). Ours draws to `DRAW_REACH`
  3 km, so from a crest or above the stage shows rather than the coarse backdrop under it (61_M from 80 m up: 27k
  to 70k triangles, the same as with no reach at all). The reflection cube takes
  cells within `CUBE_REACH` 10 m (`HH_H24_VisFill5` 0x10062bc0), no props.
- Object lanes `(1,1)`: alpha test `> 60/255`, double-sided, address modes from the kind byte.
- Interactive props `(1,6)`: every retail record drawn at its placement while its cell shows. Live mode moves a
  woken prop by its solid's pose (handle `0x9c40` + the record's global index; `CSolid::Vf28` 0x52c620) and keeps
  its last pose asleep. Drawn after the solid stage and before the blended meshes, so the car's glass, which writes no
  depth, goes over them: drawn last, a sign behind the car showed through its windows untinted (2026-10-20). Scenery
  physics: [physics.md](physics.md).
- Sky faces `(1,3)`: no culling or depth write, cut at `> 60/255`. Clouds (`Clouds.lbs`): the face named by the
  section's `CloudName`, camera-anchored, before the stage.
- Water `(1,8)`: our water surface (retail texture keys unresolved). Reflection items `(1,7)` withheld. The sky dome
  (`skyboxes.lbs`) is not drawn.
- Textures: `normal`/`worn` fall back to `new` by the filename map; a missing `wet` texture reads `damp`, then `dry`
  (ours; reaches only maps the season table keeps dry, `tests/stage_data.rs`).
- Ours: ground detail noise, the backdrop's own texture tiled at 12 and 47 m as its detail (`backdrop_detail`), road
  shine on specular slots 6 and 8 (the game's formula, our `c90`), a wet film with screen-space reflections.

## Atmosphere and sky

`atmosphere.rs` reads one `Maps/TrackSettings.ini` section (sun, ambient, diffuse, car light scales, fog): the season
row's own, `<map><letter>_[<cloud letter>_]<weather>_<cloud>` (`viewer --sky`, `Weather::section`; every retail row of
an installed stage but `69S`'s clear ones names one, `rbr-gameplay` `tests/weather.rs`), else the stage's sections
(`54E_`) with the wetness's preset word, else the first. `viewer --fog off|light|heavy` (ours, `Atmosphere::fog`) puts
our height fog over any section: 95% at 600 m or 150 m, or with none a 12 km haze under a fogged section and the clear
day's haze without its dawn mist under a clear one; off also leaves the sky's tables the clear day's haze (a fogged row's `FogEnd` greyed the sky and hid the clouds). With `--clouds` and `--precipitation` any row can be any sky (394O's heavy fog row as a partly cloudy dry day). Haze is linear fog `FogStart..FogEnd` (backdrop `SuperbowlFog*`); without `UseFog` the game's scattering
shaders are not reproduced, so the fog is capped (`CLEAR_HAZE`). The sky gradient, the doubled
clouds (`CLOUD_LIGHT`) and the tone curve (`graded`) are ours.

- Backdrop `(1,3)` (the superbowl, 0x1002f0c0): every item drawn, world identity, the stage's matrix (shader manager
  `+0x4c0`), no cell pick. Its scattering depth is scaled by `Superbowl_Scale` (`c66.x` = settings `+0x20`; the stage
  sets 1); `LoadSkySettings` 0x100526f0 copies `CloudName` to `+0x60`, so the scale survives. Without `UseFog` its
  haze is the stage's range over the scale (`Atmosphere::superbowl_fog`), so the backdrop next to the road stays clear.
- Fog with `Enhance::sky` (ours, `fogged`, `height_fog.rs`): the height fog alone, over the stage, the backdrop, the sky
  and the cloud faces alike, 95% at half `FogEnd` (`heavyfog`), three quarters (`lightfog`) or all of it (rain and snow)
  at the low ground and thinning with the height (scale height 150, 250, 600 m); the sky takes it to 100 km, so the
  farthest land fades into the sky rather than to an edge. The backdrop is fogged as no nearer than 3 km
  (`SKY_BACKDROP`, at its own height): its faces 600 m off with nothing under them showed their lower edges as lines
  (Peyregrosse). The earlier rules (backdrop all fog below the eye with clear peaks, the stage all fog at the cells'
  reach, a thin mist over a linear fog) left ridges and tree lines floating over a grey band under a cloud edge
  (Peyregrosse from above, 2026-10-22). Without ground about the eye or at night the linear fog stays, the backdrop at
  the stage's range.

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
- Baked shading lives in three places. The shadow light maps (`.ini` `NumShadowTextures`: every retail M, N, E but
  the British M clones 73, 78, 81, 83-86, 88, 89, and all of 4 and 69) paint the bake's cast shadows: with
  `Enhance::sun` they are neither loaded nor used (`sun_shadow.w`; Semetin 176_M: 165 maps, 350 MB); the cascades
  and the far layer cast for any sun, and at night for the moon (`moon_lit`: 40% of the moonlight in its shadow). The
  backdrop (world-anchored sky layer: 460M's 56k-vertex mountains, 45O's tree-line panels) casts into every sun map
  like the stage. Past the far layer's reach the world layer (`sun::world`, `world_lit`) shades the stage and the
  backdrop: the whole world's outline from the sun, drawn only when the light changes (4 m a texel on 460M). 460M at
  07:00 now has its lower slopes in the mountains' shade; 41M's low morning sun sets behind the backdrop hills; no
  measurable frame cost (460M, interleaved, 2026-10-25). Before, with the backdrop neither casting nor receiving, a
  5 km layer changed nothing (2026-10-23). Left: the backdrop's sky light (no occlusion, so at 17:00 its shaded
  slopes stay brighter than the stage's).
  The vertex light, less the bake's sun (`relit`, `sky_share`), is how much sky a point sees: kept as the ambient
  occlusion, or with `Enhance::occlusion` the computed one in its place (`sky_occlusion.rs`, below; the bake then
  gives only the hue); it also softens the sun's shadows where little sky is seen, and is not on the lamps' light
  (`lamp_albedo`; under trees it left the road dark in the beams). No stage paints shadows
  into its track textures (2026-10-23, every installed variant: retail ground textures all tile; the unique-mapped
  ones are satellite photos on far terrain, 369, 370, 460, 468, and atlases on 129, 176, 396).
- The stage's own sun is the fitted bake sun where the fit holds (`relight::own_sun`, r from 0.35), so the drawn sun,
  its shadows and the sky's sun match the bake: 40 retail variants' `SunDir` lie over 20 degrees off it (69M/N, 73,
  78, 81, 83-89 over 100). Weak fits (127N, 51N, 63N, 65N, 20N, the unbaked) keep `SunDir`. The fit samples 100k
  vertices, 15 degrees over the sky then 5 about the best (the full 5-degree grid's answer within a step).

- Night albedo (`night_albedo`): textures times vertex luminance over the bake's open-ground light
  (`Relight::open`, 90th percentile of luminance less `b * facing`; 0.14 on 45_O to 0.39 on 10_O), clamped 0.3-1, at
  0.25. Removing the bake's sun alone left 45_O's tarmac nearly black (its road vertices carry the trees' sun shadow).
- Headlight beam (`beam`, ours after the ECE patterns): dipped, cut-off 0.7 degrees down, reach about 60 m; main
  (the default, as rally cars run stages), dipped plus an axis lobe of 5 times the hot spot, 1/e at 4 degrees across
  and 2 up, reach about 130 m. `LampUniform` forward w: 1 dipped, 2 main.
- The lamps' light in the air (`fs_beams` in `light_shafts.wgsl`): quarter-size ray march at night and dusk in the
  light shafts' map, scattering 1 / fog range under `UseFog` (a tenth in rain sections: their fog stands for spray, and a
  full share hid the road behind the main beam), a fifth of that otherwise, and at least the height fog's density at
  the eye (`height_fog::uniform`: our light and heavy fog and the dusk mist, which the section's range left nearly
  unlit; 2026-10-24); the composite upsamples it by depth (`shafts_at`). About 1.6 ms at 1080p on the M-series test
  machine. With the physical light the mist's forward lobe is the moon's once the sun has set (while it is up), and the
  haze's sun lobe (`haze_colour`) fades out with the sun's true elevation (it shone toward the moon at dusk).
- The night's light (`Atmosphere::night_level`, `frame.moon`) is a moonlit night as the adapted eye sees it: clear,
  the frame's mean about 11 of 255 from 40 m up on 44E, the land and the tree lines readable (the earlier moonlight,
  2.5 times darker, gave 2); it falls with the cloud cover, squared, to 0.4 under heavy cloud, as dark as before
  (2026-10-23: 44E, 45O, 21O, 41M).
- Skid marks and the car's shadow decal are unlit records, so they take the moon (`mark_light`) and the lamps.
- With the physical light the moon is the real one for the rally's date in 2004 (`relight::lunar`, decided
  2026-10-24): the Astronomical Almanac's low-precision series (Meeus 47.a within 0.5 degrees, full at the 2004-10-28
  eclipse), its hour angle the sun's plus the right ascensions' difference, turned from the bake's anchor as the sun;
  its light by phase (Krisciunas-Schaefer: a half moon 0.09 of full), none below the horizon (`toward` then held 2
  degrees up). 2004's nights: arctic 0.35 of full, Mont Blanc 0.07 (rising about 22:30), USA 0.78, British, Australia
  and Japan moonless (airglow only). The disc shows its phase (`frame.moon_disc`, toward the sun); its aureole is drawn
  over the sky (`moon_aureole`), since the tables are folded about the sun's azimuth. The game-bright path keeps its
  full moon opposite the sun. The game's road highlight (`road_shine`) takes the direct light's share of noon's with
  our light (it lit the road at full strength through dusk).
- With the physical light (`Enhance::sky`, `Clock::physical`) twilight and night are the day's path: the sky's tables
  run at every sun elevation, the moon (`skylight::moon_light`: 0.27 lux full above the air, Kasten-Young air mass,
  optical depth 0.25) is the direct light from 1 degree down (`relight::MOON_FROM`), its sky (0.17 of it, as the day
  tables' sky against the sun's at 10 to 38 degrees) and 0.002 lux of airglow go into the sky view (`moonlit` in
  `skylight.wgsl`), so the sky, the haze, the irradiance and the clouds carry them; night (`Atmosphere::night`) from
  -18 (`PHYSICAL_NIGHT`) only turns on the night-only effects. The pre-exposure stops at the night's
  (`NIGHT_PRE_EXPOSURE`); the game's light (in-car view, water, marks, overcast fog) follows the day's
  (`DayLight::level`, `parts`). The lamps light in lux (`lamp_lux`: a dipped hot spot 20 000 cd), only in pipelines
  built with them (`PipelineKey::lamps`: the lamps' code cost about 1 ms by day at 1080p unused). Measured on 45O
  (2026-10-23): the tables' twilight at -6.4 degrees 5.0 lux, -9.9 0.07, -13.3 0.006 (AMS: civil end 2 to 3.5,
  nautical end 0.008); the frame's mean 92 at 17:30 falls smoothly to 14 by 18:25 and holds to 23:00. The rod vision
  (`post.wgsl::rod_vision`): Cao et al. 2008's rod signal in the cones' opponent channels, as Kirk and O'Brien 2011,
  Filament and Ghost of Tsushima, divided by what it adds to white at the adapting luminance so the exposure holds;
  `ROD_RETINA` 3000 per cd/m2 is Filament's 380 at a moonlit EV100 of -3.5 (380 went deep navy, 1500 still blue). Moonlit
  ground turns blue with dim greens and reds kept, reds darkest; the headlit road stays neutral; by day at most 1/255
  (17:50 6/255 in blue). The earlier grey blend (Krawczyk 2005 `0.04 / (0.04 + Y)`) took the colour out. The meter bins luminance (cd/m2) from -14 to 18 EV in 128
  bins: in pre-exposed units its range missed twilight, which went black.
- With the day's light the lit draws outside the car (the car, the characters, parked cars; `day_lit`) shade as the
  stage: the texture times the vertex colour is the linear albedo (at most 0.9), Lambert under the day's sky, sun and
  ground maps, plus the sun's Cook-Torrance highlight (Beckmann with the class's `CookTorranceM` as roughness, 0.6
  where `CookTorranceKs` is 0, at least 0.1; V-cavity, Schlick F0 0.04); paint and glass reflect by Schlick's F0 0.04
  where the class reflects, the coat in linear over the paint. The game's Ka, Kd, Fresnel limits and `LIGHT_SCALE`
  are off that path (they put the car at about a fifth of the stage's light: the white Xsara grey, the 206 black at
  night; FresnelMin 0.29 to 0.44 made glass milky and paint a mirror head on). The windows now show the interior. The
  in-car view keeps the game's or the cabin's light. Frame time unchanged (2026-10-24: 176M, 41M, 45O chase).
- Overcast (`Atmosphere::overcast`) is the section's cloud word, `heavycloud` or `lightcloud` under `UseFog`, not
  `UseFog` alone, which also marks fog under a clear or broken sky (`32N`, `54M`, `64E` crisp clear, the hazy
  partclouds): those now have their sun (`Renderer::sunless`; without the sky's tables `UseFog` as before). Under it
  the tables hold the CIE overcast sky (zenith three times the horizon), grey, giving up-facing ground a quarter of
  the clear sky's light with the sun's (Kasten and Czeplak 1980; `overcast_light`, `light_out.scale.w`), which the
  shown sky (`cs_shown`), the cube, the irradiance and the haze read; the white balance takes it as by day. Before,
  the sky and its light were the section's fog colour (blue to purple grey on 45O). 45O wet mean 92 to 95; dusk to
  night smooth (2026-10-24).
- With the day's light the roads (the draws with a specular map) reflect as a rough dielectric (`with_specular`):
  F0 0.04, roughness 0.75 less 0.15 on the map's polished stones and 0.15 damp, the sun by GGX past its shadows and
  the sky's cube by Karis's environment BRDF under Lagarde's specular occlusion, the diffuse keeping the rest; the
  game's `road_shine` stays beside it (it is the gravel's stone detail: without it 42O's gravel went flat). Retail
  only (the RSF stages' roads have no specular map); frame time ~0 (2026-10-24: 176M, 41M, 45O chase, 5 runs). On
  all ground it cost 0.5 ms for a few percent; on leaves the crown normals turned the canopy white at grazing angles.
- The clock's ambient share and tint scale the stage's display colours (`relit`, before `graded`), so its light is
  them to the 2.2 (`Atmosphere::ambient_light`); the sky's level and the exposure's limits take that (2026-10-20:
  the sky took the share as it was, 2956:1 to the ground at -4.4 degrees against 29:1 at noon, now 554:1; the
  exposure sat at its 2.6 limit from sunset on, now 3.2 to 4.9; day frames byte-identical).
- The headlights' shadow map has its own layer of the sun's map (`sun::LAMP`), so the lamps cast at dusk too, beside the
  sun's cascades.

## Car

`car/` reads `Cars/Cars.ini`, the `.sgc`, `<car>.ini`, `<car>_shaders.ini` and textures.

- State per `SShaderMaterial::SetupRenderState` 0x1003f4b0: an opacity channel means blend, test `> 0`, no depth
  write; `CarPlastic` writes depth, two-sided; `CarIntGlass` two-sided. Order (`FUN_1001e430`): wheels, opaque,
  transparent.
- Not drawn: `ghost_*`, `texture_*`, `Plane_*`, switched parts not in their state, gravel/snow/spare wheels, `k_` and
  `d_` parts. The interior `i_*` only in the cockpit (`CHH_Car::Render`); the bonnet view draws the exterior (user).
- Lighting (ours; `c41`, `c67`, `c68`, `c90` unproven): per class, `<car>_shader_settings_clear.ini` (`Ka`, `Kd`,
  `CookTorranceKs/M`, `FresnelMin/Max`, `Kf`) with the stage's light; Schlick `F0 = 0.125` (`c91`).
- Reflection: a 128-texel cube of the sky and the cells within 10 m, as the game's; a second cube to 315 m (ours) for
  the car's ambient and sun occlusion, wet roads and water.
- Shadow decal (faithful, `car_shadow.rs`): `car_dpshad2.tga` on the `.col` triangles under a 2.0 x 4.2 m rectangle
  centred `(0, 2.3, -0.3)` in the body frame; alpha `Car_Deep_Shadow_Alpha` x `asin(m[8]) * 0.63662` x the height
  fade; none on an RSF-encrypted `.col`. `--enhance contact` replaces it.
- Playback: body and tarmac tyres from the recorded poses; steering is not recorded.

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
- Ours: each kind's light on the stage and in the air at a luminous intensity (`lights::CANDELA`, the record's colour
  at unit luminance): headlight hot spot 20000 cd, tail 8, brake 100, reverse 150 (ECE R7, R23 typical); a lamp in
  its state since a drive's first frame is settled, not fading (`since_switch`; the sheet's car views at 0.28 s had the
  headlights at 28% and the never-lit reverse lamps fading out, 2026-10-25). The rear lamps shine in a flat band
  (`signal` in `lamps.wgsl`, half at about 10 degrees up or down and 40 across, after ECE R7): as a cosine lobe at
  real candela they lit the road behind neon red and the night fog dark red.
- Ours: the headlights' light back off the road (`lamp_bounce`): two Lambertian patches facing up, 15 and 55 m ahead,
  at the beam's flux on the road times albedo 0.2 over pi (numerically over `beam`: road 1 to 5 lux from 5 to 20 m),
  on the stage and the cabin (it replaces the cabin's road term, which took the road at 200 lux, about 100 times
  too much). Spectators, fences and house fronts by the beam show; scene +0.15 ms at night (45O chase and cockpit).
- Tried for the beam's white look (2026-10-25) and dropped: a soft pull of the meter toward the 97th percentile (the
  dark around the beam goes black, the beam barely dims); local exposure from 1.5 stops at 0.8 (greys the beam with
  halos, day highlights 10 to 20 levels down). The road under the beam at 1 to 5 lux against moonlight about 50
  times dimmer is physical.
- Ours: a car with no kind 1 record (an RSF car installed with headlights off) gets a beam per
  `Point_light_lf`/`rf` for the night light, no flare, in the colour of its "on" option's headlight record
  (`rsf::headlights_on`), else the retail 255, 255, 233.
- Open: the writer of the lamp words on damage; `RenderLights`' second visibility test.

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
and wave with dust and water only (user decision 2026-10-20: no stones). Inputs: the surface name's kind, wetness
(`_Wet` 1, `_Damp` 0.5) and looseness, slip (+0x80), car speed, standing water. Dry loose ground trails dust in the
region's colour (by the `PS_*` file), a plume lingering 6.5 s that dies 4 m under the ground, not at it (wind gusts
blow down); snow trails powder; wet ground (damp throws none) trails mist and streaked drops released off the tread's
back (a low rooster tail rising with speed); standing water splashes, with a burst on entering it: the renderer's
puddles (`puddle_shape` placed on the CPU without its rim noise, with `--enhance wet`), `Water_Surface`, or +0x98.
Sprites drawn in code. The splash payload +0x98 stayed 0 in every drive,
so 1.02's own splash never shows on a wet stage.

## Windscreen and wipers

`glass/`, ours. Drops, flakes, bugs and mud live in the windscreen's chart (`i_window_f`: mean plane plus a height
grid), drawn in the cockpit and on `e_window_f` outside. Inputs: precipitation, speed, tyre spray and surface
(`dirt::thrown`), `misc/Part2D.ini` bug data. Drops are lenses over the resolved frame (`Renderer::draw_over`); a wet
film blurs it. Wipers sweep 1.3823 rad (left) and 1.3195 rad (right), 0.4 s each way (`E88Wiper::InitTable`
0x5f8a50); switch off, intermittent, continuous, auto (`--wipers`, key I). Camera dirt on outside views is not
ported.
- Look (ours): rain radii 1 to 4 mm, cubed for mostly small drops; a drop's rim and highlight scale in from 1.5 to 5
  px radius; a sky crescent on its lower rim by day; at night its rim catches only its own lens's light. The film
  blurs streaked up the glass, longer with speed. Mud specks are opaque (the HDR sky through a 5 % gap read as grey).
  Cost at 1080p in heavy rain: about 0.8 ms. A reflection of the dashboard in the windscreen was tried and dropped
  (user's call).
- Side windows (ours): each `i_window_lf/lb/rf/rb` its own glass (drops, trails, film, mud; no wipers, no bugs),
  charted level across and up, drawn on its `e_window_*` outside. 0.3 of the windscreen's rain and 0.1 of its spray,
  landing low; the airflow runs back along the pane, the film streaks with it; pins 3 times harder (drops under about
  2.5 mm hold on the vertical pane).

## Skid marks

`skidmarks.rs` (pure), `skidmarks_pass.rs`. Records from the `PS_*.ini` "Skidmarks" section (material, strip 0-7 of
`skidmarks.dds`, RGB, size, flush threshold, add, rubber/depth/dirt terms) and its aliases; `_low` files have none.
`EmitTire` 0x5e3050: with contact and a record, `k = add + sum((term + reduction) * multiplier)`, clamped 1, is the
alpha; per wheel `acc += frame dt`, within the flush threshold the open mark extends, else a new one starts; edges
lifted 0.03. Four rings of 256 marks (`0x7d6d48`); the 32 oldest fade by `(i + 1) / 32`.

Measured (ours): the payload edges are 0.20 m apart (12k payloads of a recording; playback `TYRE_HALF_WIDTH` too);
records scale them by 2.0-2.2 and the strips fill ~85 %, so 1.02 marks show ~37 cm, laid along the axle (thinner in a
slide). The payload's velocity +0x50 is not the patch's (~1 m/s, mostly along the axle, at 19 m/s travel); rubber is
0-1 in the game's payloads (q99 0.65 on a tarmac drive, 0 on gravel), a slip speed in m/s (median 4.8) in playback's
from poses. Steps can lack a wheel's payload (snow drive). Snow drives run mostly on `Snowwall_Bottom` (0x58).

`--enhance marks` (ours, `tyre_marks.rs` pure, drawn by `SkidPass::tyres`, `tyre_marks.wgsl`) replaces them: width
0.21 m loose/snow, 0.235 m tarmac (constants: the viewer has the car's model folder, not its physics folder, tyre type
or NGP data; retail `TreadWidth` is 0.20-0.225, 0.135 on snow tyres), swept `w|cos a| + 0.16|sin a|`, across the
travel (patch centre motion between payloads). Gravel/dirt: rut with tread print, light berms; tarmac only when sliding
(slip angle, game rubber); snow groove; grass flattened; wet darker, a sheen at grazing views. The blend keeps
`ground * alpha + rgb`, so marks take the ground's own colour and light. Sections every 1 m (sooner in turns), 800 m a
wheel, the last 250 m fading; marks fade out 90-220 m from the eye. A seek replays only the 4 s warm-up, so captures
show short trails. Surfaces by `particles/wheel_spray/ground.rs`, shared with the spray.

## Ghost

`ghost.rs`, `rbr_gameplay::ghost`, `rbr_formats::ghost`. The car's `ghost_*` mesh in one colour, `Ambient` of
`Cars/GhostCarSettings.ini`, alpha `min(d^2 / FadeDistance, MaxOpacity)` (`CShaderGhostCar::SetObject` HH
0x1007f000). Pose: the model root, at race time = step counter less the 160 settle steps, plus the live drive's
outside-assistance penalties (HUD2) from the step each was seen on, as the game's ghost clock takes them. Ours: `--best-ghost`,
`--rsf-ghosts`, RSF `.gch` ghosts on their car.

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
0.2 m), pushed by the car's `c_e_body`. `dials/` (ours): an RSF car's needles and displays from RBRHUD's gauges,
drawn into the dial textures from the HUD records.

## Enhancements

`renderer::Enhance`, `--enhance a,b` or `all`; each off leaves the frame pixel-identical. The launcher enables all
of them. Removed, still accepted and ignored (`REMOVED`): `lens`; `ao`, `msaa`, `taa`, `wind` (user decision
2026-10-19). The launcher's render scale is 100 % by default; `auto` from older settings is 100 %. The default frame was
byte-identical before and after (chase, cockpit, heavy rain, 1920x1080).

| Name | What |
| --- | --- |
| `anisotropy` | 16x anisotropic stage samplers |
| `cabin` | cockpit lit by its baked sky share, sun only through glass, interior set clear of the stage fog (`car/cabin.rs`); with the day's light (`sky`) the interior set takes it as the lit draws outside do (`cabin_day_lit`: linear albedo, the open sky's light times the sky each vertex sees through the windows plus 5% bounced inside, the sun through the glass at 0.85, no sun highlight, dial faces at least a quarter of the open sky's light), so the cabin sits stops below the sunlit outside and loses the game's ambient's green cast (45O Xsara and RSF Evo IX, 07:00 to 23:00, dry, wet, fog; scene pass within noise; chase and additions off byte-identical, 2026-10-25); the car's glass drawn from both sides, its inner side before any pane, so the far window seen through the near one tints too (`Car::add_inner_glass`; the game culls it) |
| `clouds` | the game's cloud face turning slowly and lit from the sun's side, a drifting layer behind it as cloudy as the face (`Atmosphere::cloud_layer`), its shadows (`clouds.wgsl`). With `sky`, by day the main view's clouds are volumetric (`cloud_volume.wgsl`, `renderer/cloud_volume.rs`) and the faces are not drawn: tiling Perlin-Worley and Worley noise made at load (128³ and 32³), a 512² weather map over 120 km drawn each frame from the layer's field (the shadows' threshold; closed under an overcast sky), a march at half size over the layer's shell (32 steps, 5 toward the sun or up through an overcast deck), blended into the last frame's along the same direction, laid over the sky in `fs_sky`; the reflection cubes and the mirrors take a 256² dome of the same march over the upper hemisphere (octahedral, `fs_cloud_dome`, blended in place), and the stage's cloud shadows fall from low in the layer (`cloud_volume::shadow_height`). The layer's kind by the cover (`cloud_volume::layer`): clear, small fair-weather cumulus 1.2 km up under cirrus (8 km, streaked along the wind, in the weather map's second channel); partly, cumulus 2.2 km thick, the thickest towering; light, flat broken stratocumulus 1.1 km up; overcast, a closed deck 0.9 km up, lower, thicker and darker with the rain. Each section (and `--clouds` cover, which also hides the faces) has its own field start and drift direction (`cloud_volume::drift`, a hash; the moving layer and its shadows share it). About 0.7 to 0.8 ms on a 1080p open sky, every kind (march run 31 times: +21 to 23 ms); additions off and all but `clouds` unchanged byte for byte (2026-10-25) |
| `contact` | soft contact shadow in place of the decal |
| `decals` | soft ground decals (alpha < 230/255, triangles within 32° of level: the skid marks of RSF 129, 369, 370 and 129's sand patches; no retail draw) alpha blended after the stage instead of cut at 60/255 (`decals.rs`). The factory gives object items (flags `0x15`) alpha blend `SRCALPHA`/`INVSRCALPHA`, test > 0 (`HH_TM2_12E0_Factory` 0x100512e0); the capture at default quality shows only the cut, its switch untraced. Upright soft cut-outs (wire fences, phone lines) keep the cut: blended, the wire mesh vanishes |
| `hdr` | float targets, bloom, histogram exposure, AgX (`post.rs`); the exposure's limits follow the clock's light, and heavy rain or snow draws a fifth darker and a heavy cloud 15% more saturated (`post::conditions`, as the Assetto Corsa filters grade weather). Measured over every stage and weather (2026-10-20): on dry forest stages the exposure mostly sits at its 2.6 limit (54_E 15/25 chase frames, 21/25 cockpit), and the frame stays within 0.81-1.36x of the plain renderer's brightness. Local exposure (`fs_composite`, `LOCAL_FROM`, `LOCAL_GIVE`): where the bloom chain's blur is exposed over 2 stops above middle grey, 0.6 of the excess is taken off, so the lamps' beam at night keeps the road's texture (45O cockpit 23:00: 24% of the near road clipped, now none) and the cockpit's windscreen by day is less washed out; open days within a few levels; post pass 2.91-2.94 ms either way; additions off byte-identical (2026-10-25). A cap on the meter by the 97th percentile was tried and dropped: it blacked out everything around the beam. AgX contrast follows the adapting light (Stevens effect, `fs_composite`: `compensation` 0 to 1 blends the range top +6.5 to +5.5 stops and punch 1.25 to 1.6, middle grey held): over 84 day sheet frames p95/p99 up about 10 levels, p5 down about 6, saturation about 10% up; the 18 night frames within a level; post pass 3.34 to 3.35 ms; additions off byte-identical (2026-10-25) |
| `marks` | our tyre marks in place of the skid marks (Skid marks; launcher default off) |
| `particles` | GPU rain and snow, rain rings, lit soft particles, chimney smoke (`effects.rs`), wheel spray (`particles/wheel_spray/`) |
| `sky` | Hillaire sky, aerial perspective, height fog, light shafts (`skylight.rs`); the tables the stage shader reads (sky view, aerial, cube, sunlight) are stored in display colours, once per sky change, not per pixel (same frame within 1/255, 2026-10-20); the stage takes the haze and the mist by its distance, the backdrop by its distance times `Superbowl_Scale` next to the eye, easing to its full distance by 3 km (`sky_seen`, as the game scales its scattering depth: next to a ridge its valley floor is a kilometre off and went blue-grey beside the stage, 2026-10-20), and under a clear sky the ground haze hides 70% no nearer than 15, 9 or 4.5 km (`crisp`, plain, `hazy`), the day's mist 95% at 18 km and no thicker below its base, rather than at the linear fog's `FogEnd`: the earlier 3 km mist on the backdrop and the stage past 236 m showed as a white wall from above (2026-10-20) |
| `occlusion` | ambient occlusion from the stage's geometry at load (`sky_occlusion.rs`): the stage drawn as depth along 32 directions over the sky, in 1 km tiles at 2048 texels (0.5 to 1 m), each vertex weighing the directions it sees by its facing into `Vertex::occlusion`, and the mean seen direction (its bent normal) into `Vertex::sky_dir` (octahedral, two 16-bit snorms), along which the physically based sky's irradiance lights its ambient (`sky_ambient`, as the lit draws): shade under a low sun takes the blue of the sky it sees (44E at 07:30), walls and banks the dimmer horizon; 0.1 s on 41M, 0.5 s on Semetin's 3.4 million vertices (2026-10-23). The bake's `sky_share` held its sun shadows too (black on 42O's road, by the 45O building); the computed one does not, is about 0.9 on open ground, darkens inside the canopies and under Semetin's start tent, and gives the unbaked stages their shade. With it the stage's light is ours (`own_light`): the stage's level (`Relight::level`, its sunlit open ground's vertex light, 1 on the unbaked; no fit) split as the retail bakes' medians split it, the sky 0.56 of it times the sky seen, the sun 0.79 by its facing and its share past the cascades, the car's shadow and the clouds (`own_sun_seen`), so a shadow is the sky's light alone; the bake gives only the hue. Every stage within about 10% of the bake-driven frame's brightness (45O, 41M, 44E, 61O, 129M, 138M, 176M, 369M, 460M); the weak fits and the unbaked gain the sun's modelling, dusk at -3 degrees the twilight sky's light (45O mean 15 to 24 of 255). The light off the stage below the horizon (`bounce.rs`): the tiles drawn from below along 24 directions as each surface's linear albedo (mip 3) times its share of the sun (facing, past a 2048 tile map along the sun) and of the sky, 1024 texels a tile; each vertex sums what it sees by its facing into `Vertex::bounce` (per unit of the sun's and of the sky's light, two 10-10-10 words), which `own_light` times by the frame's; drawn once at load, the clock being fixed for the drive (user decision 2026-10-25: no running time of day); 0.1 to 0.4 s (Semetin). Only faces turned below the horizon get it: the terrain and the foliage (whose normals are up) none, the shaded cliff base at Valstagna 460M +12% (33.5 to 37.6 of 255), sunlit cuttings +2%, the dark asphalt sending little. What is seen above the horizon counts as dark (a map from above holds the occluder's far side). The tiles are also drawn from above without the cut-out draws (the floor under the trees) into the ground's maps (`bounce::Ground`, 256 texels a tile, rgba8, the sun's and the sky's layer per tile), from which the lit draws (the car, the marshals) take the ground they face 1.5 and 4 m out, by (1 - n.y) / 2, the car's shadow and contact at its lowest height taken off. The sky's tables keep a second set of harmonics over the sky above the horizon alone (`SkyLight::sky_sh`): with the bounce, the stage's sky light and the lit draws' ambient read it (`open_sky_light`, as `sky_ambient` without its 0.55 floor; (1 + n.y) / 2 without the physically based sky), since the whole sphere's held a generic sunlit ground that the bounce would count twice (the car's rear +29%, its lower bumper +52%). Up-facing faces as before; the car's lower body takes the sunlit road (45O rear 93 to 95 of 255), shaded walls the real ground (Valstagna's cliff 36 to 30, 369's cutting 101 to 96). Additions off unchanged; night and dusk within 0.5 of 255. The stage's light adds in linear (`own_light`): the open sky's level on up-facing ground and the sun's square on, each the tuned display level raised to 2.2, then the occlusion, the facing, the sun's share and the bounce scale them as irradiance; open sunlit and shaded ground as before, partly shaded places brighter (42O chase +38% mean, 460M +19%, 176M +13%), frame time within noise. With the additions off the frame moved by up to 14 of 255 on 88 pixels of 41M's two characters, unexplained (not the shader's `relit`) Contact shading (`contact_light` in `sun_mask.wgsl`, after McGuire's Alchemy obscurance): at half size from a single-sample copy of the prepass's depth, 4 directions of 3 steps within 1 m, the face's normal from the depth (the draws' up normal on the crowds' and boards' cards made them hide themselves), on the sky's and the bounce's light only; grass roots, the verge under rails, posts' and people's feet. Full size, from the multisampled depth, 6 by 4 taps: +2 to 3 ms; half size about +0.4 to 0.8 ms over the mask alone (2026-10-25). |
| `sun` | sun shadow cascades and a far layer to 1 km (in place of the baked light maps), leaf translucency (`sun.rs`); the characters cast into the cascades, and the car's body and the characters take them (`stage_sun`; the car stays out, its own map shadows it); a plant is a cut-out piece by its own part of the texture (`foliage.rs`), so the object atlases' spectators, rocks and houses cast and do not glow: as cut-out atlases they were all plants, their small pieces grass (2026-10-20). A plant taller than grass is a crown (`with_plants`): its normals point out of the ellipsoid its piece spans (centre 0.55 up it), half blended with the ground's, so trees have a sunlit and a shaded side and look down underneath; crossed cards share an axis, so each card's ellipsoid is the tree's. 41M at 08:00 and 17:00 and 45N at 10:00 show the crowns' volume; overcast 42O nearly unchanged; load only (2026-10-24). Each piece's cut-out test runs once a piece, over a summed-area table of the texture: it had run once a vertex, over every texel the piece's uv spans, 5 of Semetin's 7.4 s load (now 2.3 s; 41M 1.08 to 0.67 s; frames identical, 2026-10-25). Spectator cards are structures by name (`divaci`, `lidi`, `Spectators`, `people`, `Person`; their cut-out share, 0.4 to 0.5, is a tree's), and pieces over 40 m or wider than 1.5 times their height (backdrops, tree-line panels, wires) are not crowns (2026-10-24). Left: RSF house and prop atlases with cut-out gables and fences (176M `d1`, `m_4`) still count as plants The main view's sun shadow is worked out once per pixel before the stage is shaded (`renderer/sun_mask.rs`, `sun_mask.wgsl`): every opaque world draw lays its depth and its normal (`fs_prepass`, resolved), then `fs_sun_mask` filters the cascades and the stage reads the result (`SUN_MASK`); the depth's normal offset there gave the spectators' cards the wrong one (lit when the bake shades them), hence the draws' normal. Soft by the caster's distance on the nearest cascade (PCSS: a 3x3 blocker search, the sun's 0.53 degree disc, taps one texel apart, the half-width at most 4 texels); taps two texels apart showed a crosshatch. Against main at 1920x1080, M3 Pro, interleaved medians (noise about 0.5 ms): Semetin 5.39 to 6.06 ms, 41M 7.68 to 8.57; the same filter in the stage's shader measured close to main (2026-10-25). |
| `water` | water flowing by its scroll `c85`/`c86`, fading into banks |
| `wet` | puddles added to the wet film, wet weather only: random ellipses per 4 m square, larger in hollows (`puddles.rs`), bounded by grade and spec map; they keep the road's normal (a level normal swept the SSR along the rim: the orange rim was the marker pole). `.col` list A puddle quads: only maps 20, 22, 24, 25, 51, 54-56, 61, 62, so unused |

Shadow lookups (`shader.wgsl`) skip only taps that cannot change the pixel: the cascades and the car's shadow on faces
turned from the sun, the car's shadow where the stage takes none, the contact shadow with `contact` off and its reaches
where nothing is above (2026-10-20: cockpit scene pass 13.9 to 11.5 ms at 2560x1440 on the Mac, frames equal). Not
kept: a min/max depth map of the cascades never settles a pixel (a sloped receiver's own depth spans its block);
corner taps first lose thin shadows (tape poles); a 1024 car map changes the bonnet's self-shadow.

Cabin light: only the car's (lit) pipelines carry `CABIN`, and the open sky (`open_sky`) is a branch, not a `select`
that worked out both skies (2026-10-20: cockpit scene pass 13.6 to 11.5 ms at 2560x1440 on the Mac, 11.6 to 10.2 over
the shadow skips, frames equal but 2 pixels by 1). `shafts_at` reads the frame's depth only where the four shaft
texels' depths differ (the beams); the sun's shafts are plain bilinear. Not kept: an RG11B10 bloom chain (no gain on
the Mac, steps in the sky's bloom).

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
- View: `crop * VP * R`, R the reflection across the glass, crop the glass's screen box to the image with x flipped
  (winding kept), z row `w - k (plane . R p)` for an oblique near plane 1 cm past the glass, `k` 0.95 x the least
  cosine to the view's axis over the box (the far side stays past 300 m). Image about 1:1 with the glass at the
  render size, 32 px steps, at most 1024 x 512, a layer of one 2D array; the glass samples it by its point in the view
  it was drawn with, moved with the car since. Rates take turns between mirrors.
- Passes draw the stage to 300 m (far ground), sky, fog, the car (exterior in door mirrors, cabin in the centre),
  ghosts, the game's particles, rain (`Effects::mirror_group` over its depth) and lamps, with the frame's sun cascades
  and exposure; no prepass, light shafts, cubes or bloom. Only the glass's pipelines have `MIRRORS`: on in
  the shared shader it cost 1.1 ms a frame at 1440p (M-series).
- Cost, Evo X R4 cockpit, launcher graphics (auto render scale), frame time over Off, min of 5 (M-series, 2026-10-19):
  every frame +0.66-0.82 ms at 1080p, +0.71-0.75 at 1440p, +0.71-1.13 at 4K; every 2nd +0.29-0.45; every 3rd
  +0.18-0.34.

`--frames N --profile` prints per-pass medians. Texture packs: `src/bin/texture-pack.rs` upscales a stage's
textures 4x (Upscayl's Real-ESRGAN), re-encodes them with full mips under `out/` (never committed); `viewer
--textures <dir>/pack` reads them.

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
- Stage `c90`; kind-byte blend bits; reflection items `(1,7)`; water texture; mip levels beyond the 4 stored.
- Wheel set shown; `Plane_*` shadows; detached node motion; `pc_e_window_*` panels.
