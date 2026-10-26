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
without the `sky` addition, the textures' painted light, the vertex colour's hue, the default hour ("As baked"
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
  unchanged. The in-car view's interior set on the same light (`cabin_day_lit`, 2026-10-25). Left: the stage's own
  specular (surface classes, below).
- [x] **Stage level without the bake.** `own_light` takes the day's light alone; `Relight::open` only gates the
  computed occlusion and lights the non-physical night. A per-stage albedo scale only if a stage's textures turn out
  authored too dark or bright.
- [x] **The car and characters on the same light.** Lit by the day's sun, sky and ground maps as the stage; their sky
  occlusion and the sun past trees still come from the car's reflection cube (`surroundings`), see section 3.

## 2. Albedo and materials

The textures carry painted and photographed light (shade, cracks, highlights): physical light on top of them looks lit
twice.

- [ ] **De-lit albedo.** Dropped for the ground (2026-10-25): 41M's atlases are evenly lit photos whose large-scale
  light and dark is material (dirt patches, gravel strips, walls), and the tree cards' dark cores are the self-shadow
  cards need; dividing by a blurred luminance flattens both. Only building fronts carry painted shade.
- [x] **Surface classes (roads).** The retail stages draw nearly all their ground from two atlases
  (`TRACKTEXTUREPLATEOPAQUE`, `OBJECTTEXTUREPLATETRANSPARENT`), so texture names do not tell surfaces apart; the
  draw's slot does for the roads. Roads reflect as a rough dielectric (`with_specular`: GGX sun, the sky's cube by
  Karis's environment BRDF, Lagarde's specular occlusion; roughness 0.75, less on polished stones and when damp),
  beside the game's `road_shine`, which carries the gravel's stone detail. ~0 ms. Tried and dropped: the same on all
  ground (roughness 0.95 reflects a few percent, +0.5 ms); on the leaves (crown normals are not the leaves', the
  canopy went white at grazing angles); the specular map read as a height for a stone bump (noisy on asphalt, flat on
  gravel). Left: detail normal maps per surface, which need the atlases' regions classified.
- [x] **Vertex colour hue.** (2026-10-25) The stages' vertex colours are near grey (mean chroma at most 0.08 on 45O,
  41M, 176M; 2% of the retail vertices over 0.15, the crowds' clothes): the hue is now a filter, the colour over its
  brightest channel (`own_light`, `bounce_surface.wgsl`), not over its luminance, which lifted a saturated tint's
  main channel past the texture (salmon and pastel crowds); the clothes are deep red, blue and green. Splitting
  material from light by how the colour varies was not needed: the ground's tint is the bake's grey. Cost: none.
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
- [x] **Backdrop panels and far terrain.** (2026-10-25) The backdrop casts into the sun's maps, and past the far layer
  the world layer shades the stage and the backdrop (`sun::world`); its normals were already real. Cost: none
  measured. Its sky occlusion, baked, came out near 0 and changed nothing: not kept.
- [x] **Characters far from the car.** Dropped (2026-10-25): the spectators are stage draws, lit by the occlusion
  where they stand; only the scripted crew (marshals, photographers) take the car's cube, and they stand by the car.
  Tried: the ambient times the ground maps' sky share there over under the car (45O sheet: no visible change).
- [x] **The car's cube against the open sky.** (2026-10-25) With the ground's maps the cube is taken against the open
  sky halfway between the face's normal and up (`surroundings`): the ground below is `ground_bounce`'s. The sides of the
  car a little brighter in the open. Found with it: the cubes' sky was black until the clouds' dome was first drawn
  (`CloudVolume::marched`). Cost: none.

## 4. Fine shading and shadows

- [x] **Contact shading.** Screen-space obscurance at half size on the sky and bounce terms only, never the sun
  (`contact_light`): grass roots, the verge under rails, post and people's feet. Left: the car's wheels (the car is
  not in the prepass's depth). Cost: about 0.4 to 0.8 ms.
- [x] **Soft shadows by distance (PCSS).** On the nearest cascade, in the sun's mask pass (`renderer/sun_mask.rs`).
  Cost with the mask pass about 0.7 to 0.9 ms over main (M3 Pro); in the stage's shader it measured near 0 now that
  the pipelines are specialised. Left: decide between the two on the M1 Pro, where the cascades spill in the stage.
- [x] **Sun moving during a drive.** Dropped (2026-10-25, user decision): the time of day is fixed for the drive, so
  the bounce is drawn once at load.

## 5. Sky, clouds and air

- [x] **Volumetric clouds.** Raymarched cumulus by day in place of the game's cloud faces (`cloud_volume.wgsl`), where
  the moving layer's field puts them (so its shadows on the stage match), at half size, 32 steps and 5 toward the sun,
  blended over about ten frames; warm at dusk; under an overcast sky a closed deck lit from above. +0.6 ms on an open
  1080p sky (Verkiai, 100 degrees), within noise on the stages' views. Kinds by the cover (fair-weather cumulus under
  cirrus, cumulus, broken stratocumulus, the deck lower and darker in rain), each section its own field, and the
  launcher's Clouds over any row: +0.7 to 0.8 ms on that sky. The reflections and mirrors take a dome of the same
  clouds, the shadows fall from the layer's height. At night they are lit by the moon and the night sky in the tables (since the physical night; dark shapes over the
  stars, 394M at 21:00 to 05:00, 2026-10-25). Left: the clouds overhead are soft
  (not the half size: a full-size march, +0.5 to 0.8 ms on 394M, only crisped the far edges a little, 2026-10-25).
- [x] **Overcast, fog and rain rows.** Overcast is the cloud word (`Atmosphere::overcast`: `heavycloud`, or `lightcloud`
  under `UseFog`), not `UseFog`, which also marks fog under clear and broken skies (`54M_crisp_clear` now has its
  sun). The overcast sky is the CIE overcast sky, grey, letting through a quarter of the clear sky's light (Kasten and
  Czeplak), in the tables (`overcast_light`): sky, cube, irradiance and the haze take it, and the white balance as by
  day; the fog's colour no longer tints it. 45O wet, heavy rain, 42O light fog, 10O and 21O: neutral grey, the sky
  brighter overhead; dusk to night smooth. ~0. Left: the deck's thickness by the row (light rain against heavy fog),
  or the volumetric clouds.
- [ ] **Rain.** (2026-10-25) Done: wet weather falls from an overcast sky (the section's sun stayed out over the
  rain); rings only in standing water, faded by the ground's footprint; RSF roads (no specular map) take the film,
  puddles and reflections by their collision surface, and so do the retail roads now (one wet path; the film and
  the soaked ground are the `wet` addition, off is 1.02's wet look); the screen-space reflections are a setting (12 to
  17 ms in the wet). Reflection probes baked at load along the driveline replace the per-frame cubes (with
  `occlusion`; -1 to -2 ms). Left: the screen-space reflections stay for the car and near things on wet roads; the streaks are sparse bright needles (want many faint ones and a mist); spray behind the car; cheaper
  screen-space reflections (4 rays of 64 steps a pixel; half size or fewer rays).
- [x] **Light in the fog.** The mist takes the sky's light and the sun's forward lobe, the moon's once the sun has set;
  the lamps' beams scatter at least as the height fog drawn around the eye (`fs_beams`). Left: the mist's density along
  the beams is the eye's (not by height); the section's fog at physical night keeps its range's density.
- [x] **Twilight.** The sky's tables run at every sun elevation; the moon lights the stage from 1 degree down
  (`relight::MOON_FROM`), night from -18 (`PHYSICAL_NIGHT`); stars come out as the sky darkens; the exposure meters
  luminance (cd/m2) over 32 stops. 45O from 17:30 to 23:00 changes smoothly (mean 92 to 14). Cost: ~0.
- [x] **The moon.** Its real place and phase for the rally's date in 2004 (`relight::lunar`), its light by phase,
  the disc's phase and the aureole where it is. Left: the moon by day (drawn only once the sun is down).
- [x] **Night.** Lamps in candela (`lights::CANDELA`), the rear lamps' flat band, the headlights' bounce off the road
  (`lamp_bounce`), 2026-10-25. The bounce on the car's body: tried, 0.001% of the night chase and cockpit pixels
  (the patch lies 15 to 55 m ahead, edge-on to the body), not kept.
  The beam clipping to white at night: local exposure (`post.wgsl`, 2026-10-25), ~0 ms. Cost: bounce +0.15 ms.
- [x] **The sun's path.** It follows each rally's latitude and date (`relight::solar`, `rally_of`), its compass
  anchored to the bake's sun and its elevation lifted to the bake's (`Clock::lift`). Decided (2026-10-24): kept, so
  our shadows agree with the shading painted in the stage; the track's authored hour ("As baked") stays the default.

## 6. Checking it

- [x] A fixed set of views (forest, open, cliff, crowd, chase, cockpit) on retail, RSF and unbaked stages, at 07:00,
  10:00, 13:00, 17:00, 18:00, 23:00, dry, wet and fog, rendered to one sheet per change (`scripts/light-sheet.py`).
- [ ] Reference photos of real stages (same latitude, hour, weather) to judge against, not the old look.
- [ ] The performance rule above for every change, with the frame time and memory in the change's report.
- [ ] Additions off stays pixel-identical to the original renderer.

## Limits

The assets cap what lighting can do: low-poly terrain, card trees, 2004 texture resolution. Lighting can make them look
like a well-lit old game; for a photo, the geometry and materials need work too (denser terrain, modelled trees, new
textures), which is outside this list.
