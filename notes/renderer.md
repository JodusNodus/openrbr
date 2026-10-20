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
- Inputs: the recording frame (FRM1: car block, wheels, controls); in live mode also `PTC1` (tyre payloads), `PEV1`
  (damage, damping, engine fire and noise, exhaust, lights), `DMG1` (zones, last damage code), `DET1` (detached
  bodies), `CLK1` and the HUD records (`rbr_recording`). Playback takes `PTC1`/`PEV1` from a sidecar made once per
  recording by `rbr-standalone --boot --recording X --events-out FILE`, cached in `RBR_EVENTS_CACHE`
  (`particles/sidecar.rs`; `RBR_PARTICLE_EVENTS` overrides, empty disables).
- Presentation clocks step 7 ms per payload period; a live answer of several periods is one game frame.

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
  its last pose asleep. Scenery physics: [physics.md](physics.md).
- Sky faces `(1,3)`: no culling or depth write, cut at `> 60/255`. Clouds (`Clouds.lbs`): the face named by the
  section's `CloudName`, camera-anchored, before the stage.
- Water `(1,8)`: our water surface (retail texture keys unresolved). Reflection items `(1,7)` withheld. The sky dome
  (`skyboxes.lbs`) is not drawn.
- Textures: `normal`/`worn` fall back to `new` by the filename map; a missing `wet` texture reads `damp`, then `dry`
  (ours; reaches only maps the season table keeps dry, `tests/stage_data.rs`).
- Ours: ground detail noise, the backdrop's own texture tiled at 12 and 47 m as its detail (`backdrop_detail`), road
  shine on specular slots 6 and 8 (the game's formula, our `c90`), a wet film with screen-space reflections.

## Atmosphere and sky

`atmosphere.rs` reads one `Maps/TrackSettings.ini` section (sun, ambient, diffuse, car light scales, fog). The
game's name choice is undecoded, so we take the stage's sections (`54E_`) with the weather's preset word, else the
first. Haze is linear fog `FogStart..FogEnd` (backdrop `SuperbowlFog*`); without `UseFog` the game's scattering
shaders are not reproduced, so the fog is capped (`CLEAR_HAZE`, `SUPERBOWL_HAZE`). The sky gradient, the doubled
clouds (`CLOUD_LIGHT`) and the tone curve (`graded`) are ours.

- Backdrop `(1,3)` (the superbowl, 0x1002f0c0): every item drawn, world identity, the stage's matrix (shader manager
  `+0x4c0`), no cell pick. Its scattering depth is scaled by `Superbowl_Scale` (`c66.x` = settings `+0x20`; the stage
  sets 1); `LoadSkySettings` 0x100526f0 copies `CloudName` to `+0x60`, so the scale survives. Without `UseFog` its
  haze is the stage's range over the scale (`Atmosphere::superbowl_fog`), so the backdrop next to the road stays clear.

## Time of day and night

`--clock HH:MM` (`relight.rs`). Each variant letter is its own bake, so the bake's sun is fitted from the vertex
light (`relight::fit`), and the shader swaps its `b * facing` for the drawn sun's. A solar model per rally moves the
sun. Below -7 degrees: night (`night.wgsl`, `lamps.wgsl`): moonlight, the lamps as main or dipped beams with a shadow map,
stars, wet-road lamp glints. Shared bakes with M names (73, 83-89) probably fit the wrong hour.

- Night albedo (`night_albedo`): textures times vertex luminance over the bake's open-ground light
  (`Relight::open`, 90th percentile of luminance less `b * facing`; 0.14 on 45_O to 0.39 on 10_O), clamped 0.3-1, at
  0.25. Removing the bake's sun alone left 45_O's tarmac nearly black (its road vertices carry the trees' sun shadow).
- Headlight beam (`beam`, ours after the ECE patterns): dipped, cut-off 0.7 degrees down, reach about 60 m; main
  (the default, as rally cars run stages), dipped plus an axis lobe of 5 times the hot spot, 1/e at 4 degrees across
  and 2 up, reach about 130 m. `LampUniform` forward w: 1 dipped, 2 main.
- The lamps' light in the air (`fs_beams` in `light_shafts.wgsl`): quarter-size ray march at night and dusk in the
  light shafts' map, scattering 1 / fog range under `UseFog` (a tenth in rain sections: their fog stands for spray, and a
  full share hid the road behind the main beam), a fifth of that otherwise; the composite upsamples it by
  depth (`shafts_at`). About 1.6 ms at 1080p on the M-series test machine.
- Skid marks and the car's shadow decal are unlit records, so they take the moon (`mark_light`) and the lamps.
- The clock's ambient share and tint scale the stage's display colours (`relit`, before `graded`), so its light is
  them to the 2.2 (`Atmosphere::ambient_light`); the sky's level and the exposure's limits take that (2026-10-20:
  the sky took the share as it was, 2956:1 to the ground at -4.4 degrees against 29:1 at noon, now 554:1; the
  exposure sat at its 2.6 limit from sunset on, now 3.2 to 4.9; day frames byte-identical).
- Open: no lamp shadow at dusk (the sun's cascades hold layer 0 there).

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
0x1007f000). Pose: the model root, at race time = step counter less the 160 settle steps. Ours: `--best-ghost`,
`--rsf-ghosts`, RSF `.gch` ghosts on their car.

## Characters

`characters/`: `.lbs` `(2,14)` items in `(2,15)` random groups, Lua 4.0 scripts (`script.rs`), `.sga` clips by
element-wise lerp, CPU skinning; shown and scripted while their `(2,10)` cell meets the frustum. Driveline triggers
fire at `distance - lead * v < x` (`x` = `CCar+0x17c`). Generators: the exe's LCG and msvcr71 `rand()`, `--seed`.
Scripted events (flip spectators, rescue helicopter, bird strike): `characters/scripted.rs`. In-car crew (`crew.rs`,
`incar.rs`): seated at `D_driver_use`/`D_codriver_use`, steering scrubs the driver clip, gear and handbrake moves from
`OnCarControl`'s ring with `Misc/LM_Driver.ini`, arm IK (`CLM_IKChain::Solve`), `CSigIncarShake` lean; chase views
only, as the High renderer. Tests: `tests/characters_data.rs`, `scripted_data.rs`, `crew_data.rs`.

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
| `cabin` | cockpit lit by its baked sky share, sun only through glass, interior set clear of the stage fog (`car/cabin.rs`) |
| `clouds` | the game's cloud face turning slowly and lit from the sun's side, a drifting layer behind it as cloudy as the face (`Atmosphere::cloud_layer`), its shadows (`clouds.wgsl`) |
| `contact` | soft contact shadow in place of the decal |
| `decals` | soft ground decals (alpha < 230/255, triangles within 32° of level: the skid marks of RSF 129, 369, 370 and 129's sand patches; no retail draw) alpha blended after the stage instead of cut at 60/255 (`decals.rs`). The factory gives object items (flags `0x15`) alpha blend `SRCALPHA`/`INVSRCALPHA`, test > 0 (`HH_TM2_12E0_Factory` 0x100512e0); the capture at default quality shows only the cut, its switch untraced. Upright soft cut-outs (wire fences, phone lines) keep the cut: blended, the wire mesh vanishes |
| `hdr` | float targets, bloom, histogram exposure, AgX (`post.rs`) |
| `marks` | our tyre marks in place of the skid marks (Skid marks; launcher default off) |
| `particles` | GPU rain and snow, rain rings, lit soft particles, chimney smoke (`effects.rs`), wheel spray (`particles/wheel_spray/`) |
| `sky` | Hillaire sky, aerial perspective, height fog, light shafts (`skylight.rs`); the tables the stage shader reads (sky view, aerial, cube, sunlight) are stored in display colours, once per sky change, not per pixel (same frame within 1/255, 2026-10-20); the stage takes the haze and the mist by its distance, the backdrop by its distance times `Superbowl_Scale` next to the eye, easing to its full distance by 3 km (`sky_seen`, as the game scales its scattering depth: next to a ridge its valley floor is a kilometre off and went blue-grey beside the stage, 2026-10-20), and under a clear sky the ground haze hides 70% no nearer than 15, 9 or 4.5 km (`crisp`, plain, `hazy`), the day's mist 95% at 18 km and no thicker below its base, rather than at the linear fog's `FogEnd`: the earlier 3 km mist on the backdrop and the stage past 236 m showed as a white wall from above (2026-10-20) |
| `sun` | sun shadow cascades, leaf translucency (`sun.rs`); the characters cast into the cascades, and the car's body and the characters take them (`stage_sun`; the car stays out, its own map shadows it); a plant is a cut-out piece by its own part of the texture (`foliage.rs`), so the object atlases' spectators, rocks and houses cast and do not glow: as cut-out atlases they were all plants, their small pieces grass (2026-10-20) |
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

## Open

- Sky dome and cloud transforms and inputs (`SkyDome.vso` `c41`-`c66`); stage in-scatter not reproduced.
- Stage `c90`; kind-byte blend bits; reflection items `(1,7)`; water texture; mip levels beyond the 4 stored.
- Wheel set shown; `Plane_*` shadows; detached node motion; `pc_e_window_*` panels.
