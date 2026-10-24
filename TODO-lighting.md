# Lighting: what is left for realistic lighting

Goal: every stage lit realistically at any time of day and in any weather, by our own physically based model; nothing
taken from the original bake. State and measurements live in `notes/renderer.md` ("occlusion", relight, bounce); this
file lists only what is open, in the suggested order.

## Performance rule

- Measure every change's in-game cost: `viewer --frames N --profile` (median per stage) before and after, at 1920x1080
  on the slowest stage (Semetin 176_M) and a forest one (41_M), chase and cockpit views, with the time of day and
  weather the change touches.
- Load time (bakes at load, the loading screen) may grow; frame time is what counts. Report load time too.
- A change that costs more than about 0.5 ms a frame, or 5% of the frame, or holds memory over 100 MB, goes to Thomas
  before it lands, with the numbers and what it buys.
- Each item below gives the expected in-game cost: **load** (load time only), **~0** (negligible in game), or an
  estimate in ms.

## Where it stands

Computed per vertex at load: sky occlusion and bent normal (`sky_occlusion.rs`), bounce off the ground below the
horizon and the ground's light maps (`bounce.rs`). Drawn each frame: our sun and sky (`own_light`), sun cascades plus a
1 km far layer, moon shadows, the headlights' shadow layer, cloud shadows, the open sky's light apart from the
ground's, AgX tone mapping and an EV100 exposure (`post.wgsl`). The light is physical and pre-exposed from noon to
night: sun, sky, twilight and moon in lux from the sky's tables (`skylight.wgsl::day_light`, `skylight::moon_light`),
the lamps in candela, rod vision in the tone pass. The car and characters outside the car shade as the stage
(linear albedo, Cook-Torrance sun, Schlick reflection). Still taken from the original: the in-car view's game light
(scaled by `DayLight::parts`), the textures' painted light, the vertex colour's hue, the default hour ("As baked"
fit), the flat cloud layers.

## 1. A physical core

Everything after builds on this; without it the rest stays tuned rather than real.

- [x] **Linear light, step A: the stage's light adds in linear.** `own_light` sums the sky, the sun and the bounce as
  linear irradiance (each tuned display level raised to 2.2), so open sunlit and shaded ground keep their brightness
  and faces half turned, penumbrae, occlusion and bounce take their physical share; back to display colour after. Partly
  shaded places brighten (42O forest +38% mean, 460M +19%, 176M +13%, 41M at 16:30 +53%); night unchanged. Frame time
  within noise (176M, 41M, 45O chase, 3 runs each, +-0.2 ms).
- [x] **Linear light, step B: real units.** The sky tables' irradiances in lux, pre-exposed (`SUN_LUX`, `pre`),
  EV100 auto exposure with Krawczyk's key, CIECAM02 white balance. Twilight and night on the same path (below).
  Frame time: day +0.0 to +0.4 ms (176M, 41M, 45O chase); a night now costs a day's plus the lamps (176M 10.4 -> 16.3
  ms, 45O chase 14.3 -> 20.5 ms), since it is drawn with the day's light in place of the old cheap night shader.
- [x] **Linear light in real units.** The stage (`own_light`) and the lit draws outside the car (`day_lit`): textures
  as linear albedo, Lambert under the day's sky, sun and bounce in pre-exposed lux, the sun's Cook-Torrance highlight
  (the class's `CookTorranceM` as roughness), paint and glass reflecting by Schlick's F0 0.04. The game's Ka, Kd,
  Fresnel limits and `LIGHT_SCALE` are off that path (the car was at about a fifth of the stage's light). Frame time
  unchanged. Left: the in-car view's interior (the game's or the cabin's light), the stage's own specular (surface
  classes, below).
- [x] **Stage level without the bake.** `own_light` takes the day's light alone; `Relight::open` only gates the
  computed occlusion and lights the non-physical night. A per-stage albedo scale only if a stage's textures turn out
  authored too dark or bright.
- [x] **The car and characters on the same light.** Lit by the day's sun, sky and ground maps as the stage; their sky
  occlusion and the sun past trees still come from the car's reflection cube (`surroundings`), see section 3.

## 2. Albedo and materials

The textures carry painted and photographed light (shade, cracks, highlights): physical light on top of them looks lit
twice.

- [ ] **De-lit albedo.** A load-time filter per texture: take out the low-frequency shading (divide by a blurred
  luminance), keep the colour and the fine detail. Compare per surface class; keep the original where it looks worse.
  Cost: load.
- [ ] **Surface classes.** Asphalt, gravel, dirt, grass, rock, bark, leaves, snow, painted wood and metal, by texture
  name and the draw's slot: each a roughness, a specular level, and a detail normal map (generated or authored). Gives
  every surface its own sun highlight and sky reflection in place of `road_shine` alone. Cost: about 0.2 to 0.5 ms
  (a normal map sample and the specular), to be measured.
- [ ] **Vertex colour hue.** Keep the vertex colour where it varies within a surface (material: dirt, grass patches),
  drop the smooth gradients (lighting). Compare per stage. Cost: load.
- [ ] **Wet and snow albedo.** Wet ground darker and glossier, snow far brighter and rougher, by the weather, so the
  bounce, the sky's ground colour and the exposure follow. Cost: ~0.

## 3. Global illumination per texel

The occlusion and bounce are per vertex, metres apart on the terrain: the large soft shading (under one tree, along a
wall's foot) is blurred or missing, and the bounce sees only below the horizon.

- [ ] **Own light maps.** A second uv set per stage mesh (generated at load or cached on disk), and per texel (25 to
  50 cm) the sky's occlusion and bent normal and the bounce, in place of the vertex ones. Computed with the same tiled
  depth maps or by ray queries where the device has them. Cost: load (seconds to tens of seconds; cache per stage),
  memory to measure (about 50 to 200 MB for Semetin), frame ~0 (one texture sample).
- [ ] **Radiance transfer.** Store per texel how each part of the sky lights it, with every bounce (precomputed
  radiance transfer: a few spherical harmonics per texel, the sun's visibility from the cascades as now). Any hour and
  any weather is then a lookup, with light from above, canopy undersides and several bounces included. Replaces the
  per-vertex bounce, the ground's maps and the rebake as the sun moves. Cost: load (minutes on the big stages;
  offline per stage and cached), memory to measure, frame about 0.1 ms.
- [x] **Tree crown normals.** Each plant piece taller than grass takes normals out of the ellipsoid it spans, half
  blended with the ground's (`foliage::with_plants`); crossed cards share an axis, so no grouping is needed. Load
  only. Left: light through the leaves from the far side beyond `leaf_glow`; cut-out photo spectators (176M) are
  taken as crowns.
- [ ] **Static crowds and props with up normals.** The stage's own spectator cards and props: the same, or a normal
  toward the road. Cost: load.
- [ ] **Backdrop panels and far terrain.** Outside the tiles (sky layer, beyond the reach): lit with an up normal and no
  occlusion. Light them to match the stage's edge at every hour. Cost: ~0.
- [ ] **Characters far from the car.** Their surroundings (`surroundings`, the reflection cube) are captured at the
  car. Light them from the transfer or the ground's maps and the occlusion where they stand. Cost: ~0.
- [ ] **The car's cube against the open sky.** With the sky and the ground apart, compare the cube with the open sky
  only and let the ground carry what lies below. Cost: ~0.

## 4. Fine shading and shadows

- [ ] **Contact shading.** Screen-space AO (GTAO) on the sky and bounce terms only, never the sun: grass roots, post
  and barrier feet, rocks on the verge, the car's wheels. Cost: about 0.5 to 1 ms, goes to Thomas.
- [ ] **Soft shadows by distance (PCSS).** Sharp at contact, softening with distance by the sun's 0.53 degree disc: a
  blocker search in the cascades. Cost: about 0.3 ms.
- [ ] **Sun moving during a drive.** While the per-vertex bounce stays, spread its rebake (up to 0.4 s at once on
  Semetin) over frames and blend. Cost: ~0 per frame, no hitch.

## 5. Sky, clouds and air

- [ ] **Volumetric clouds.** The game's flat cloud layers dominate the image at dusk and in overcast. Raymarched
  clouds lit by the sun and the sky (a low-resolution pass, reprojected over frames), casting their shadows and
  setting the sky's light and the overcast. Cost: about 1 to 2 ms, goes to Thomas; cheaper variants (a cloud dome
  rendered at intervals) to compare.
- [ ] **Overcast, fog and rain rows.** The sky's tables are off under them (`sky_on`), so the light is flat and has no
  direction. Run the tables with the overcast sky so the light is soft, from above, dimmer by the cloud's thickness
  (or from the volumetric clouds). Check 45O wet, 176O heavy fog, the British N and O rows. Cost: ~0.
- [ ] **Light in the fog.** The height fog's colour is the section's, eased by the hour. Take the sun's in-scatter
  (bright toward the sun; the shafts exist), the sky's, and at night the moon's and the lamps'. Cost: about 0.1 to
  0.3 ms.
- [x] **Twilight.** The sky's tables run at every sun elevation; the moon lights the stage from 1 degree down
  (`relight::MOON_FROM`), night from -18 (`PHYSICAL_NIGHT`); stars come out as the sky darkens; the exposure meters
  luminance (cd/m2) over 32 stops. 45O from 17:30 to 23:00 changes smoothly (mean 92 to 14). Cost: ~0.
- [ ] **Night.** The moon sits opposite the sun, as far up as the sun is down (10 to 60 degrees), always full: its
  real position and phase by date and hour, its light by phase. Lamps beyond the headlights in candela (tail, brake,
  reverse are scaled from the headlight's unit); the lamps' bounce off the road onto the verges and the car's nose.
  Chase views at night: the beam is metered with the rest (10 to 90 percent), so the car's back is a silhouette;
  decide on a centre-weighted meter. Cost: ~0 (the lamps' bounce to measure).
- [ ] **The sun's path.** It follows each rally's latitude and date (`relight::solar`, `rally_of`), but its compass is
  anchored to the bake's sun and its elevation lifted to the bake's (`Clock::lift`). Decide between the stage's real
  north (where known) and the bake's anchor, drop the lift, and give each rally a default hour (its real start times)
  in place of "As baked", which stays as an option. Cost: ~0.

## 6. Checking it

- [ ] A fixed set of views (forest, open, cliff, crowd, chase, cockpit) on retail, RSF and unbaked stages, at 07:00,
  10:00, 13:00, 17:00, 18:00, 23:00, dry, wet and fog, rendered to one sheet per change.
- [ ] Reference photos of real stages (same latitude, hour, weather) to judge against, not the old look.
- [ ] The performance rule above for every change, with the frame time and memory in the change's report.
- [ ] Additions off stays pixel-identical to the original renderer.

## Limits

The assets cap what lighting can do: low-poly terrain, card trees, 2004 texture resolution. Lighting can make them look
like a well-lit old game; for a photo, the geometry and materials need work too (denser terrain, modelled trees, new
textures), which is outside this list.
