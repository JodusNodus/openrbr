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
- [x] **Surface classes (roads).** The retail stages draw nearly all their ground from two atlases
  (`TRACKTEXTUREPLATEOPAQUE`, `OBJECTTEXTUREPLATETRANSPARENT`), so texture names do not tell surfaces apart; the
  draw's slot does for the roads. Roads reflect as a rough dielectric (`with_specular`: GGX sun, the sky's cube by
  Karis's environment BRDF, Lagarde's specular occlusion; roughness 0.75, less on polished stones and when damp),
  beside the game's `road_shine`, which carries the gravel's stone detail. ~0 ms. Tried and dropped: the same on all
  ground (roughness 0.95 reflects a few percent, +0.5 ms); on the leaves (crown normals are not the leaves', the
  canopy went white at grazing angles); the specular map read as a height for a stone bump (noisy on asphalt, flat on
  gravel). Left: detail normal maps per surface, which need the atlases' regions classified.
- [ ] **Vertex colour hue.** Keep the vertex colour where it varies within a surface (material: dirt, grass patches),
  drop the smooth gradients (lighting). Compare per stage. Cost: load.
- [x] **Wet and snow albedo.** Not needed (2026-10-24): wet roads and the ground beside them are soaked already
  (`soak`, `wet_film`); darkening the bounce and the sky's ground by the same soak changed wet 41M and 45O by under
  0.2/255 (the exposure takes it back). The arctic ground sheet is already 0.68 linear albedo at its median, 0.85 at
  the 75th percentile (fresh snow about 0.8): snow reads grey because the exposure meters it to the mean, as a
  camera does. A brighter key for snow is an exposure choice, not albedo.

## 3. Global illumination per texel

The occlusion and bounce are per vertex, metres apart on the terrain: the large soft shading (under one tree, along a
wall's foot) is blurred or missing, and the bounce sees only below the horizon.

- [ ] **Own light maps.** Shelved (2026-10-25): a planar map over the ground within 30 m of the driveline, 25 cm
  texels, the sky's share and bent normal baked with the tiled depth maps (0.5 s on 41M, 1.2 s on Semetin; 0.02 to
  0.1 s from a disk cache), matched the per-vertex values on the road and the grass (the stages' meshes are dense
  there and the sky's occlusion is smooth), changing 0.4 to 1.6% of the pixels by more than 8 of 255, for 49 MB
  (41M) to 129 MB (Semetin) and +0.3 to 1.5 ms. The fine shade comes from the sun's shadows and the contact
  shading. Worth another look only for the bounce per texel or radiance transfer below.
- [ ] **Radiance transfer.** Store per texel how each part of the sky lights it, with every bounce (precomputed
  radiance transfer: a few spherical harmonics per texel, the sun's visibility from the cascades as now). Any hour and
  any weather is then a lookup, with light from above, canopy undersides and several bounces included. Replaces the
  per-vertex bounce, the ground's maps and the rebake as the sun moves. Cost: load (minutes on the big stages;
  offline per stage and cached), memory to measure, frame about 0.1 ms.
- [x] **Tree crown normals.** Each plant piece taller than grass takes normals out of the ellipsoid it spans, half
  blended with the ground's (`foliage::with_plants`); crossed cards share an axis, so no grouping is needed. Load
  only. Left: light through the leaves from the far side beyond `leaf_glow`.
- [x] **Static crowds and props with up normals.** Spectator cards are structures by name (`foliage::STRUCTURES`:
  the ground's normal, no leaf light, they cast); backdrops, tree-line panels and wires are not crowns by size. Left:
  RSF house and prop atlases with cut-out gables (176M `d1`, `m_4`) still count as plants. Cost: load.
- [ ] **Backdrop panels and far terrain.** Outside the tiles (sky layer, beyond the reach): lit with an up normal and no
  occlusion. Light them to match the stage's edge at every hour. Cost: ~0.
- [ ] **Characters far from the car.** Their surroundings (`surroundings`, the reflection cube) are captured at the
  car. Light them from the transfer or the ground's maps and the occlusion where they stand. Cost: ~0.
- [ ] **The car's cube against the open sky.** With the sky and the ground apart, compare the cube with the open sky
  only and let the ground carry what lies below. Cost: ~0.

## 4. Fine shading and shadows

- [x] **Contact shading.** Screen-space obscurance at half size on the sky and bounce terms only, never the sun
  (`contact_light`): grass roots, the verge under rails, post and people's feet. Left: the car's wheels (the car is
  not in the prepass's depth). Cost: about 0.4 to 0.8 ms.
- [x] **Soft shadows by distance (PCSS).** On the nearest cascade, in the sun's mask pass (`renderer/sun_mask.rs`).
  Cost with the mask pass about 0.7 to 0.9 ms over main (M3 Pro); in the stage's shader it measured near 0 now that
  the pipelines are specialised. Left: decide between the two on the M1 Pro, where the cascades spill in the stage.
- [ ] **Sun moving during a drive.** While the per-vertex bounce stays, spread its rebake (up to 0.4 s at once on
  Semetin) over frames and blend. Cost: ~0 per frame, no hitch.

## 5. Sky, clouds and air

- [x] **Volumetric clouds.** Raymarched cumulus by day in place of the game's cloud faces (`cloud_volume.wgsl`), where
  the moving layer's field puts them (so its shadows on the stage match), at half size, 32 steps and 5 toward the sun,
  blended over about ten frames; warm at dusk; under an overcast sky a closed deck lit from above. +0.6 ms on an open
  1080p sky (Verkiai, 100 degrees), within noise on the stages' views. Kinds by the cover (fair-weather cumulus under
  cirrus, cumulus, broken stratocumulus, the deck lower and darker in rain), each section its own field, and the
  launcher's Clouds over any row: +0.7 to 0.8 ms on that sky. Left: the reflections and mirrors keep the flat layer,
  the night keeps it, the clouds overhead are soft at half size, the stage's cloud shadows stay 1.4 km up.
- [x] **Overcast, fog and rain rows.** Overcast is the cloud word (`Atmosphere::overcast`: `heavycloud`, or `lightcloud`
  under `UseFog`), not `UseFog`, which also marks fog under clear and broken skies (`54M_crisp_clear` now has its
  sun). The overcast sky is the CIE overcast sky, grey, letting through a quarter of the clear sky's light (Kasten and
  Czeplak), in the tables (`overcast_light`): sky, cube, irradiance and the haze take it, and the white balance as by
  day; the fog's colour no longer tints it. 45O wet, heavy rain, 42O light fog, 10O and 21O: neutral grey, the sky
  brighter overhead; dusk to night smooth. ~0. Left: the deck's thickness by the row (light rain against heavy fog),
  or the volumetric clouds.
- [x] **Light in the fog.** The mist takes the sky's light and the sun's forward lobe, the moon's once the sun has set;
  the lamps' beams scatter at least as the height fog drawn around the eye (`fs_beams`). Left: the mist's density along
  the beams is the eye's (not by height); the section's fog at physical night keeps its range's density.
- [x] **Twilight.** The sky's tables run at every sun elevation; the moon lights the stage from 1 degree down
  (`relight::MOON_FROM`), night from -18 (`PHYSICAL_NIGHT`); stars come out as the sky darkens; the exposure meters
  luminance (cd/m2) over 32 stops. 45O from 17:30 to 23:00 changes smoothly (mean 92 to 14). Cost: ~0.
- [x] **The moon.** Its real place and phase for the rally's date in 2004 (`relight::lunar`), its light by phase,
  the disc's phase and the aureole where it is. Left: the moon by day (drawn only once the sun is down).
- [ ] **Night.** Lamps beyond the headlights in candela (tail, brake,
  reverse are scaled from the headlight's unit); the lamps' bounce off the road onto the verges and the car's nose.
  Chase views at night: the beam is metered with the rest (10 to 90 percent), so the car's back is a silhouette;
  decide on a centre-weighted meter. Cost: ~0 (the lamps' bounce to measure).
- [x] **The sun's path.** It follows each rally's latitude and date (`relight::solar`, `rally_of`), its compass
  anchored to the bake's sun and its elevation lifted to the bake's (`Clock::lift`). Decided (2026-10-24): kept, so
  our shadows agree with the shading painted in the stage; the track's authored hour ("As baked") stays the default.

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
