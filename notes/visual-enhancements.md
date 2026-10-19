# Visual enhancements (exploration)

Opt-in additions to the Rust renderer beyond the game's look, for comparing against it. None is the default. Two
parts: renderer additions (`viewer --enhance`) and upscaled texture packs (`viewer --textures`).

## Renderer additions (`--enhance anisotropy,ao,cabin,clouds,contact,hdr,msaa,particles,sky,sun,taa,water,wet,wind` or `all`)

`renderer::Enhance`, passed to `Renderer::new`.

- `clouds` (visuals-4, `clouds.wgsl`, `Atmosphere::cloud_cover`; launcher "Moving clouds", off by default): the game's
  cloud domes (`Clouds.lbs`, camera-anchored, static textures that cannot follow the time of day) are hidden and the sky
  pass draws a layer of clouds 1800 m above the eye instead:
  - Cover from the `TrackSettings.ini` section's last word: `clear` 0.12, `partcloud` 0.42, `lightcloud` 0.58,
    `heavycloud` 0.9, 0.35 without a section (RSF sections use the same words).
  - Shapes: 3 octaves of value noise at 2 km, their edges eaten by 3 finer octaves (6 times smaller), fading near the
    horizon; the noise also changes slowly with time. The layer drifts at 14 m/s along the foliage's wind direction.
  - Light: a second lookup 220 m toward the sun (2 octaves) gives how thick the cloud is that way (Beer's law), plus a powder term
    so thin edges glow and a forward lobe toward the sun; the base is the overhead sky's light, greyed. Sunlight colour
    from `sunlight()`, dimmed with the time of day's sky (`relight_light.w * dusk`): pink and orange edges at dawn and
    dusk. Under `UseFog` the clouds are the fog's grey, darker where thick. At night they are lit like the night sky
    and hide the stars.
  - Cirrus: streaks 8 km up (3 octaves), stretched 4:1 along the wind, twice the drift, under clear sections whose cover is under
    about 0.85 (55% at most); white by day, sun-coloured at dawn and dusk.
  - Shadows: under clear sections by day, each vertex looks up along the sun to the layer (3 octaves, no edge detail)
    and passes the cover to the fragment (clouds are kilometres across). A separate entry point pair (`vs_clouded`,
    `fs_clouded`, `CloudedOut`), used by every stage pipeline when on: an extra vertex output on `vs_main` moved 79
    edge pixels of the plain wet frame by up to 20 levels. Without `clouds` the frame is pixel-identical to the base
    (plain wet 41_O, and a wet 46_E drive with hdr, sky, sun, wind; the TAA and particle runs are not deterministic
    run to run, so they were not compared). Unlit stage draws lose up to
    38% of their light there (less where the baked light is already dark), the sun cascades' shadows fade with it, and
    the car loses up to 80% of its sun. The water's and wet roads' sun glint goes out while the eye is under a cloud.
  - Reflections: water, the wet roads' sky fallback and the car's sky reflection before the cube is drawn see the
    clouds; the reflection cube draws them with the sky pass.
  - Cost (M3 Pro, 1920x1080, track-46_E chase, `viewer --frames 300` drawn only, three interleaved runs, the
    defaults on): 7.14 to 7.67 ms, about +0.5 ms. Nearly all of it is the sky pass, which is a full-screen triangle
    drawn first, so the clouds are computed under every pixel, not only where the sky shows (with the shadows' noise
    left out the cost stayed +0.47 ms). A sky view costs the same with or without (the domes' blended faces are gone).
    Open: draw the sky where nothing covers it (a depth-tested pass after the opaque draws; the backdrop faces write no
    depth), or a noise texture in place of the hashed value noise.
  - Captures: `lead/reports/visuals-4/`.

- `water` (visuals-5, `water_flowing` and `fs_reflect_plus` in `water.wgsl`; launcher "Flowing water", proposed on by
  default): the stage's water items, drawn by the default shader as a level sheet with hard polygon edges, keep what
  their vertices hold (`stage::water_mesh`, 48-byte layout: position, normal (up), D3DCOLOR, two uv layers, a float):
  - The vertex alpha is 0 at the banks and up to 135 to 255 inside (retail): times 2.5, it fades the surface out at the
    banks, and the body is clearer (down to half) and lighter (`SHALLOW`) toward them, so the bed shows in the shallows.
  - The vertex grey (33 to 128) is the bake's shade: over 128 (`WATER_SHADE`) it scales the body's light and the
    sky's reflection and glint (banks and trees hide the sky from the water).
  - The scroll is traced (visuals-6): the loader (`HH21_Block18` 0x10057080) stores the `key10`, `key20` and `key40`
    float pairs at the water object's +0x48, +0x50 and +0x58; `HH_KWaterRenderItems` (0x10030170) uploads
    `(t, +0x48 pair)` as vertex constant c85 and `(t, +0x50 pair)` as c86 (`SetVertexShaderConstantF`, vtable +0x178;
    the C++ reconstruction calls it the pixel-shader setter), `t` being the float the holder passes
    (`CHH_WorldUnk1C4::RenderWater` 0x10030ee0, its first word; read as seconds, not traced further). The shaders add it:
    `WaterSingleTexture.vso` (48-byte items) `oT0 = v9 + c85.x * c85.yz`, `oT1 = v10`; `WaterDoubleTexture.vso`
    (56-byte) also `oT1 = v10 + c86.x * c86.yz`. So `key10`'s pair (0.1/-0.1 on the rivers, 0.02-0.04 on the lakes)
    scrolls the first layer, `key20`'s the second layer of the double-texture items (0/0 on 71_M, the only ones), and
    `key40`'s pair is never used. Through the screen derivatives of each layer's coordinates and of the stage position
    the scroll becomes metres a second (at most 3) and moves one octave of ripples (2.2 and 0.55 m) downstream; where
    the second layer does not scroll its octave follows the first. The ripples also churn slowly (the noise's third
    axis).
  - Survey: `examples/water_survey.rs` (every retail stage's water items, flags, keys, pairs, bounds, colours and a deep
    vertex for a camera). The readable RSF samples have no water items.
  - The 71_M marsh pools stay hard-edged (visuals-6). Their polygons (`LAKE WATER`, about y -0.2) carry alpha 153 up
    to their edges, and the ground under them lies 0.1 to 0.3 m lower everywhere, edges included: the ground does not
    cut them, so neither the vertex alpha nor a depth fade against the ground softens them. A depth fade (the surface
    drawn after the stage, reading the ground's depth, `SHORE_DEPTH` 0.3 m) was tried: no softer edges, and it dimmed
    the shallow pools, so it was dropped. Softening them would need the distance to the polygon's boundary edges per
    pixel (split vertices with per-triangle edge distances).
  - Dusk (visuals-6): the bright reflection at dusk was the game's cloud domes, which stayed at full light; they now
    dim with the time of day (see Time of day), and so does their reflection.
- `wet` (visuals-5, `wet_plus` and `puddled` in `water.wgsl`, `puddles.rs`; launcher "Puddles on wet roads", proposed
  off): on the draws with the wet film (a specular map), water stands where the road is low: each road vertex's height
  against the mean of the road vertices in the 3 x 3 cells about it, at 1 m and 4 m cells, measured from its tangent
  plane (zero on any plane, positive in a hollow; 8 cm below counts fully), passed in the normal's length (1 + lowness)
  since the vertex has no free field on road draws. Against all ground (terrain too) the whole road read as low; against
  the road alone the puddles fall in the wheel ruts on 54_E. A puddle is level (with a faint wind ripple near the eye),
  45% as bright beneath, and reflects as water from 0.3 head on, its screen-space rays as narrow as the water's. Damp
  weather gets half the film, only the deeper hollows hold water, and the ground is half as soaked; it has no
  screen-space reflections (they cost about 8 ms at 1440p under load here), only the reflection cube.
  Under the forest stages the puddles mirror dark trees and the change is small. On the tarmac stages (visuals-6:
  43_N, 45_N, the open 73_M test area; wet and damp, day and dusk; `lead/reports/visuals-6/wet-*`) the option makes
  the road read less wet than the base film: one or two puddles in hollows reflect the sky, but between them the road
  is darker and more matte, and the open 73_M area loses most of its sheen. It stays off. Cost at 2560x1440 (M3 Pro,
  loaded, three interleaved runs): wet 45_N within the noise; damp 73_M +1.2 ms a frame drawn.
- Both go through a pipeline override (`WATER_PLUS` in `shader.wgsl`) and their own reflection entry point: with the
  branches in the shared `shade` and `fs_reflect`, even untaken, one to four water pixels moved by a level. With both
  off, ten views (dry, damp, wet, chase, cockpit, dusk, rivers, stream, lake, RSF 129, with and without the other
  additions) are byte-identical to the base viewer (a dusk view differs run to run on the base too: the clock runs).
- Cost (`--profile`, 2560x1440, M3 Pro shared with other workers' builds, three interleaved runs each; scene and
  screen reflections only): wet 54_E cockpit and 53_O river within the run-to-run noise (about ±1 ms when loaded;
  the scene pass read 18 to 26 ms against about 8 ms unloaded). No separate figure under 0.7 ms could be measured.
  Visuals-6, the 53_O river filling most of the view at 2560x1440: scene pass 22.55-23.19 ms without `water` against
  22.63-22.77 ms with it, screen reflections equal; under 0.1 ms.
- Captures: `lead/reports/visuals-5/`.
- `anisotropy`: 16x anisotropic filtering on the stage samplers. Sharpens the road and ground at grazing angles
  further out; free on Apple Silicon.
- `contact`: the car's soft contact shadow (render-m13, `renderer::GROUND_REACH`): the car seen from straight above
  in the shadow map's second layer darkens the ground and the fences under it, in place of the game's shadow decal
  (`car_shadow.rs`, `RenderShadows` 0x605670, drawn without it). The launcher's "Soft car shadow", on by default.
- `ao`: ground-truth ambient occlusion (visuals-9, GTAO after Jimenez et al. 2016; `ambient_occlusion.rs`, `ao.wgsl`),
  after the frame is resolved, at half resolution, over the surface rebuilt from the depth (`depth_copy.rs`): in two
  planes through the view direction per pixel it walks the depth six steps each way (denser near the pixel) for the
  highest horizon within the radius, and integrates the cosine-weighted arc above both horizons against the normal
  projected into the plane. Points farther than 60% of the radius count less as horizons, none at the radius, so a
  trunk far in front of a surface leaves no halo on it; sky never occludes; horizons less than 0.03 (in cosine) above
  the surface's plane do not count, so far grazing ground stays clean. The planes turn over a 4x4 pattern of pixels and,
  under `taa`, by the golden ratio each frame, so the history gathers more of them (without TAA a still frame stays the
  same). A 4x4 depth-aware blur (the pattern) multiplies it into the frame; 1.2 m radius, at most 160 frame pixels,
  shrinking within 3 m of the eye with the distance (to 5 cm at the least), so the cockpit gets the shade of its folds
  and corners (visuals-interior), and fading out between 60 and 150 m. Positions are rebuilt about the eye (in stage
  coordinates, thousands of metres out, f32 gave the cockpit's centimetre radii a visible grain).
  Against the SSAO before it (16 samples over the hemisphere, a point counting by how steeply it rose over the
  surface): bushes, verges and the foot of the banks sit in their own contact shade, the car's underside and the
  road under it darken, the trunks' shade keeps to the trunks instead of a soft band around them, and the dash's
  crease under the windscreen gets its shade (`lead/reports/visuals9/ao-*`). Cost: see visuals-9 in Frame time below.
  Visuals-interior's notes on the SSAO: against the pass before it a chase view changed on 1.5% of its pixels by 1.3
  levels on average (the precision), a bonnet view on 5% by 4.7 (the bonnet within 3 m); no dark halos where the dash
  meets the bright windscreen (`lead/reports/visuals-interior/ao-dash-edge-*`).
- `cabin` (visuals-interior, ours; `cabin.wgsl`, `car/cabin.rs`; launcher "Cabin light", proposed on by default): the
  in-car view's interior set. The game lights it with one ambient (`INTERIOR_AMBIENT`) and the full sun on every face
  turned to it, so the cabin read flat: no roof or pillar shade, no sun patches, the same light in the footwells as on
  the dash top. With `cabin`:
  - Sky share: at load each interior vertex casts 64 cosine-weighted rays through a 2.5 cm grid of both sets' opaque
    faces (the glass and blended faces let light through), up to 2.5 m; the open share goes in `Vertex.specular[2]` as
    `1 - sky` (the car's draws have no specular map). Under a second at load, nothing a frame. The ambient is
    `INTERIOR_AMBIENT * (0.6 + 1.2 * sky)`.
  - Sun only through the glass: the car's shadow map (it holds both sets whichever is shown, so the roof, pillars,
    cage and dash shade each other), the sun's first cascade (one filtered compare, only on faces the sun can reach;
    the 3 x 3 filter cost 0.9 ms), else the reflection cube toward the sun, and the clouds' shade; twice the game's
    interior sun (`CABIN_SUN`), so the patches stand out of the shade.
  - Only the interior set's opaque faces (marked at the bake, `specular[1]`; `mode.lit` 1): the interior glass, lamps
    and cracks and the exterior set stay lit as in the game, so the outside seen through the glass is unchanged (with
    the HDR pass the exposure meters the cabin at a fifth of the outside since visuals-7; before, the outside moved
    with the cabin's mean light by about 4% of linear light either way; `lead/reports/visuals-interior/outside-*`). A first version lit the interior set's
    windscreen too, which veiled the outside (whiter marshals, paler bushes).
  - Paint seen from inside: the interior set's paint open to the sky (`Kf` over 0, sky share 0.6 to 0.9) and the
    exterior set's paint (the bonnet through the windscreen) take the outside view's light; under the interior ambient
    the bonnet blew out white at midday.
  - Dial faces keep at least the game's light (ambient and sun each no less than the game's in-car ones): under the
    dash's hood they went darker and harder to read, and the exposure does not adapt to the cabin. A face is the circle
    about each RBRHUD `Rotating` needle's pivot in its texture, a quarter more than the needle reaches (the faces are
    not in the data, the needles are; `car::cabin::gauge_faces`); a triangle whose uv middle is on one is marked. Cars
    without RBRHUD gauges (the retail ones) have no floor.
  - Sheen: faces with `Kf` 0 (plastics, cloth, dials) reflect the reflection cube's mip 3 with a dielectric's Fresnel
    (F0 0.04), times the sky share squared.
  - Grey texels: DXT1 stores a block's colours in 5:6:5 bits, so the dark, nearly grey dashboards (Fiesta Rally3:
    48/50/54 on average) shift to magenta in one block and green in the next, which the lighting and the tone curve
    made a rainbow. Texels with a chroma under 0.03 (fading out to 0.06) are taken as grey at their brightness. The
    coarser mip levels carry the same noise, so they cannot give the hue back.
  - Night and dusk: the lit headlights' colour times `CABIN_BOUNCE` (0.00004) comes back in as ambient from the road
    ahead and below, times the sky share; read from the lamps uniform, the beam code untouched.
  - Off, the in-car frame is byte-identical to the base (retail and RSF cockpits, dry, wet, night; chase): `CABIN` is
    a pipeline override, like `WATER_PLUS`.
  - Cost (M3 Pro, 1920x1080, 45_N M3 E30 cockpit, `--enhance all` against all but `cabin`, 15 interleaved pairs on a
    loaded machine): scene pass medians 10.5 and 10.6 ms, within the noise.
  - Through the windows (interior2): the bake also keeps where each vertex sees the sky, the mean direction of its
    open rays (model space: x, y in `uv2`, z in the fraction of `specular[1]`; the playback turns it with the car).
    Its length over the open share says how gathered the open directions are (2/3 the whole hemisphere, toward 1 a
    narrow window). The sky share's light is the surroundings cube (the sky before it is drawn) that way, blurred by
    that width (cube levels 5.5 to 3), against the cube overhead, in linear light, 30% of its colour, held to 0.25 to
    2 (`cabin_window`, replacing the sky's irradiance by the normal). A face that sees out toward the trees is darker
    than one under the windscreen; one toward a sunset window warmer.
  - The plastics' sheen and the interior paint's reflection keep only the directions inside that cone
    (`cabin_open`): the dash reflects the sky through the windscreen, not where the roof is.
  - Sun bounce (interior2): at load the sun on the interior's opaque faces (mean `n.l` past the car, by area, 3000
    faces) is measured for 128 directions (`car::cabin::SunIn`); each frame the renderer looks up the sun's direction
    in the car's space (`frame.wetness.z`), and the cabin takes `CABIN_SUN_BOUNCE` (0.6) of it back as ambient, more
    on faces turned down, times the stage's and clouds' share of the sun. A sunlit dash lifts the roof lining.
  - Mirrors (interior2): the interior set's opaque glass-class faces (the mirrors) mirror the reflection cube at 85%,
    tinted by the texel, instead of the game's lit white glass.
  - Bake time (M3 Pro, release): Fiesta Rally3 82 ms, M3 E30 29 ms, COROLLA 17 ms, both tables together.
  - Captures: `lead/reports/visuals-interior/`, `lead/reports/interior2/`.
- `hdr` (`post.rs`, `post.wgsl`): every target before the post pass is `Rgba16Float` (scene, reflection cube, SSR
  levels); `graded` writes linear colour (`pow 2.2`) and `ungraded` inverts it, so the cube and SSR stay consistent.
  The post pass builds a 6-level bloom chain (13-tap down, tent up, 6% blended in), meters the frame and applies
  AgX (visuals-7, below; before it, the exposure was the smallest bloom level's mean light each frame, `KEY` 0.22, and
  the curve the ACES filmic fit scaled so display mid grey landed where the plain curve puts it).
  - Metering (visuals-9: `cs_histogram`, 32 workgroups of 256 threads, one grid point each, each binning its own in
    workgroup memory before adding them into a 64-bin buffer; then `cs_meter`, one workgroup, meters the bins and
    empties them; before, one workgroup did both, on one core; the exposure is the same to the bit): a 128x64 grid of the frame binned by log2 light (64 bins, -10
    to +6 stops); the mean log light between the 10th and 90th percentiles of the weight is the frame's, so the sun's
    disc, a lamp or a few sky holes do not set it; it runs from about 0.14 (open snow by day) through 0.01 (dense
    forest) to 0.001 (night). The exposure goes 35% of the way (in log) to `KEY` 0.124 over that light (the stages
    are lit game-bright: a dark forest stays darker than a snowfield) within 0.6 to 2.6, and eases there in
    log space at 3 per second toward a darker frame and 1.2 toward a brighter one (`Post::draw` gives the frames'
    time step; the first frame, a step back in time or one over a second sets it outright, so a screenshot is the
    adapted frame). In the in-car view the cabin (nearer than 2 m along the view, from the depth) counts a fifth of
    the outside, so the exposure follows what is seen through the glass and a dark cabin lifts it a little (the
    windscreen's mean on 10_O by day: 110 of 255 metering the whole frame, 95 with the cabin at a fifth). The HUD and overlays are drawn after the post pass and are not exposed.
  - AgX (Sobotka's, Wrensch's polynomial fit): the exposed light times 2 (`AGX_SCALE`), the contrast curve raised to
    1.25 (`AGX_PUNCH`) so mid grey and the shadows stay near the ACES look, then saturation 1.1. Bright skies,
    snow and sunlit gravel roll off toward white instead of clipping per channel: the 98th percentile of the day
    frames drops from 222 to 197 of 255 on RSF 201 (the sky through the canopy keeps its colour; visuals-sweep's
    white holes), 172 to 138 on 10_O (snow keeps its shading). Mean frame brightness over the 84 captures stays within
    about 8% of the ACES pass's; forest stages are a little brighter, open snow a little darker. The headlit verges
    at night are no more saturated than under ACES (bright greens' mean saturation 0.51 to 0.40 on RSF 127, 0.35 to
    0.36 on 54_E), so they do not go lime.
  - Cost (M3 Pro, 3 interleaved runs, 54_E chase and RSF 201 cockpit): the post pass's span 0.1 to 0.5 ms more at
    1920x1080 (the metering is one workgroup, so it runs on one core), within the noise at 3840x2160; the frame
    drawn only within the noise at both.
  - Captures: `lead/reports/visuals7/tone-*.png` (rows day, 10 degrees, -3 degrees, night, damp, wet; columns chase
    ACES and AgX, cockpit ACES and AgX; all with `sky`). The sun's disc is 4x brighter under HDR, for the bloom. The
  particles, windscreen drops and skid marks (`particles.wgsl`, `glass.wgsl`, `skidmarks.wgsl`) linearise their
  display colour through an `HDR` pipeline override (`post::scene_options`); the drops' view through the glass is
  already linear. Blended particles also scale their alpha (`blended_alpha`): their alpha was made for blending in
  display colour, and blended in linear colour a pale rain streak over dark trees showed about three times as
  strongly, its soft edges widening it into a blob. The scale matches display blending over a 0.25 background.
  Car paint reflects 75% of the game's Fresnel weight from the reflection cube's mip 1 (`PAINT_GLOSS`): the
  game's paint FresnelMin (0.29 on the Impreza) made it a sharp mirror even head on, reading as chrome.
- `sky`: aerial perspective in `fogged` (world-anchored draws only): exponential from the eye, 90% of the linear
  fog's cover at its end, blue extinguished 1.4x faster than red, and never less than the linear fog (visuals-sweep:
  the superbowl's range starts thousands of metres behind the eye, so the exponential alone left its tree-line strips
  unfogged in fogged sections, and left 10% of what lies past the fog's end); the haze takes the sky's colour toward
  the view (`sky_light`, the sky without the disc) plus a forward-scattering lobe toward the sun under clear presets.
  Since visuals-7, the physically based sky (ours; `skylight.rs`, `skylight.wgsl`, `sky_lut.wgsl`):
  - Tables (Hillaire, EGSR 2020), drawn by compute passes only when their inputs change (the sun by a tenth of a
    degree, the levels by half a percent, the section): transmittance 256x64, multiple scattering 32x32, sky view
    192x108 (azimuth from the sun, elevation squeezed toward the horizon), aerial perspective 32x32x32 (the same
    directions by the square root of the distance to 12 km: the air is the same all round, so one table serves the
    main view and the reflection cube's faces), a 32-texel sky cube with six levels blurred in widening cones, and the
    sky's irradiance as nine spherical harmonics with the scale to the renderer's light.
  - The air: Rayleigh, ozone, an aerosol by the section's word (`crisp` 1x, `hazy` 2.5x, else 1.6x Hillaire's), and a
    ground haze (scale height 0.15 km crisp, 0.3 hazy, else 0.2) in the fog colour's hue, dense enough to hide
    `FOG_COVER` (0.7) of the stage at `FogEnd`. The game's `Rayleigh_Multiplier`, `Mie_Multiplier` and `Turbidity`
    drive shaders we do not reproduce (most are 0.001 or 0), so they are not used. Ground albedo 0.2, 0.6 on snow.
  - The level: the sky's mean light (an up-facing irradiance over pi) is `SKY_LEVEL` (0.3, linear) times the stage's
    ambient level at that time of day, so the sky dims with the game-bright stage at dusk rather than physically. The
    sun's true elevation (`Atmosphere::sun_elevation`, set by the clock; `sun_direction` stays 2 degrees up) gives the
    twilight: pink and violet opposite a set sun, an orange glow toward it, no disc once it has set. From 3 degrees up
    to 1 down the sky, haze and sky cube go to half their saturation (`SKY_TWILIGHT_SATURATION`): at full strength
    the Belt of Venus read garish next to a dark forest. A gain of ten per
    2.5 degrees below the horizon keeps the tables in half floats.
  - By day under clear sections, the tables give the sky (`sky_light`), the sunlight's colour (`sunlight`: the
    transmittance toward the sun at the eye) and the haze in `fogged` (cover `max(1 - T, linear)`, colour the aerial
    light over its cover). In the stage's frame the haze goes to the backdrop's at its foot (3 km) between three
    quarters of the cells' reach and the reach (`CELL_REACH`, 315 m), so the stage's edge meets the backdrop or the
    sky in the same haze; under fog the stage is all fog by the reach, as the backdrop's foot is (visuals-sweep). The
    game's cloud faces are lit by 1.7 instead of 2 (`SKY_CLOUD_LIGHT`), so the sky through the canopy is less of a
    white hole (RSF 201). Without `hdr` the backdrop's horizon and sun side are brighter than white, which `graded`
    clipped (white holes in the leaves on 54_E); its colour rolls off above 0.8 toward 0.95 by its brightest channel
    (`sky_shoulder`, leftovers-5), which the HDR pass does with its own curve.
  - Image-based light: the lit draws' hemisphere term (`light`, outside the car) becomes the sky's irradiance at the
    normal (`sky_ambient`): the game's ambient keeps its level and colour, the sky gives the change with direction
    (half of its colour against the mean over all directions, brightness 0.55 to 1.05 of the up-facing). The cabin's
    sky share takes the same (`cabin.wgsl`). The car's surroundings measure against the blurred sky cube; before the
    reflection cube is drawn, reflections take the sky cube at the level's blur. The cloud cover greys and flattens
    the cube and the irradiance toward a CIE overcast (75% of the cover).
  - Overcast (`UseFog`) sections keep the fog's sky and haze; their irradiance and cube are the fog's sky. Night keeps
    the night sky (no tables).
  - Off, the frame is pixel-identical (a `SKY_LUT` pipeline override; 54_E chase and wet cockpit, 71_M at 16:35, the
    user's set without `sky` with and without clouds and wet).
  - Cost (M3 Pro): the scene pass within the run-to-run noise at 1080p and 4K; a redraw of the tables 0.1 to 1.6 ms
    (the GPU's clock state), the same at both sizes, about every 25 s under a real-time clock.
  - Captures: `lead/reports/visuals7/sky-ibl-*.png` (rows day, 10 degrees, -3 degrees, night, damp, wet; columns
    chase before and after, cockpit before and after).
  - Height fog (visuals-9, ours; `height_fog.rs`, `height_fogged` in `sky_lut.wgsl`): a mist whose density falls off
    exponentially with the height above a base, integrated along the view ray in closed form, over the haze
    (`fogged`, world-anchored draws) and the sky (`fs_sky`). The base is the low ground about the eye: at load the
    lowest world-anchored solid vertex in each 32 m square, and each frame their mean within 400 m of the eye
    (weighted to fade out there, so it moves smoothly) less 15 m; valley floors below it lie in the mist, ridges above
    it out of it. A global base did not work: 54_E's stage geometry spans 45 to 700 m. The game has no height fog (the
    sections give only the linear fog's range and colour), so the density is ours: the visibility at the base (95%
    cover) is half the section's `FogEnd` under `heavyfog`, `FogEnd` under `lightfog`, twice it in the other `UseFog`
    sections (rain, snow), with scale heights of 70, 55 and 45 m; under clear sections a dawn and dusk mist (900 m,
    22 m) blended by the sun's true elevation into a faint ground haze by day (9000 m, 60 m; half as far under
    `hazy`): thickest from 6 degrees below the horizon, burning off between 10 and 30 degrees in the morning (the
    clock's hour before noon) and forming below 14 degrees (all of it below 3) in the evening or without a clock. None
    at night. The mist's light is the haze's colour there (`fogged`'s; the sky's light toward the horizon for the
    sky) times 0.55, plus a forward Henyey-Greenstein lobe (g 0.6, 0.03 of it, capped at 4x) toward a risen sun, in
    the haze's colour (warm toward a low sun; no lookups of its own). In the stage's frame the ray's length goes from the point's distance to 3 km (`SKY_BACKDROP`) between
    three quarters of the cells' reach and the reach, the backdrop and the sky take the mist 3 km out along their
    direction, so the stage's edge meets them in the same mist. Particles, rain and the glass are not height-fogged.
    `viewer --sun-from STAGE:fog` picks a stage's fog section (`heavyfog`, else `lightfog`; 04, 20-24 O).
  - Light shafts (visuals-9, ours, with `hdr`; `light_shafts.rs`, `light_shafts.wgsl`): by day under clear sections
    with the sun in front of the eye and in or within 0.35 frame heights of the frame (fading out there, and in over
    its first 4 degrees above the horizon), two quarter-size passes before the post pass: a mask of the sky's light
    near the sun (the share of each 4x4 depth block that is sky, so the foliage's dithered cut-outs average out; within
    0.16 frame heights of the sun; only light from 0.8 to 2.5 up, so the glow and the disc rather than the open sky), a
    48-step blur along the line toward the sun (85% of the way, each step 0.965 of the last; Mitchell, GPU Gems 3), and
    0.2 of it, which the post pass's composite adds to the scene before the exposure (`SHAFTS`, a pipeline
    override; not into the TAA history, the meter or the bloom: added to the scene first, the shafts lowered the
    exposure, and a full-size apply pass cost more).
    Shafts show through the canopy and cloud gaps; the open sky around the sun brightens a little. Over 24 frames in
    motion with the sun ahead (RSF 127 chase) the frame-to-frame change was 4.29 levels with the shafts and 4.49
    without, the shafts' own part changed by 0.4 levels a frame on a mean of 1.7: they do not flicker with the foliage.
- `sun` (`sun.rs`, `sun.wgsl`, `sun_lit`/`sun_light` in `shader.wgsl`): real-time sun shadows under clear presets
  (`UseFog = false`). Three 2048 cascades to 14, 45 and 150 m, each fitted to its view slice's bounding sphere and
  moved in whole texels; reach 250 m toward the sun with near-plane pancaking. Casters: world-anchored stage draws
  that write depth and do not blend, at the view's level of detail, not the car (its own shadow) or water; alpha-test
  and alpha-to-coverage draws cut at the alpha reference, so leaves cast. Grass (plants under 1.5 m, `foliage.rs`)
  does not cast: the crisp shadows of its flat cards made them read as cards instead of blending into the ground. Received by the unlit stage draws: darken
  by 0.45 in full shadow (about the baked maps' shadow), on terrain also by the facing (smooth normals computed at
  upload where the stored normal is zero), half as much on foliage and less where the vertex light is already dark.
  Within the cascades the baked light map is faded out (it fades back in over the last 30 m), so shadows are not
  doubled. Sunlit plants (`foliage.rs`'s plants, marked by `specular[2]`; before visuals-8 every alpha-to-coverage draw,
  rocks and fences too) seen against the sun let it through their leaves (visuals-8, ours): the sunlight's colour
  (`sunlight()`) times the leaf texel's hue at full strength, `TRANSLUCENCY` 1.2 times `pow 4` of the view-to-sun
  angle, only where the cascades say it is lit, faded out as the sun drops from 14 to 2 degrees and at most `GLOW_MOST`
  0.8, so a low sun in the view does not wash the canopy out yellow (it did at 8 degrees).
  Under overcast presets (`UseFog`) the cascades hold the sky's light from overhead instead, leaning 0.35 toward the
  sun (`Renderer::shadow_light`), filtered 6 texels a step and darkening by 0.35, with no facing term or glow: soft
  shadows under trees, signs, barriers and timing boards, which otherwise float on the flat overcast light.
- `wind` (`foliage.rs`, `sway` in `shader.wgsl` and `sun.wgsl`): foliage sways. At upload each connected piece of an
  alpha-to-coverage draw's triangles is one plant, when the draw's texture is cut out (over 5% of the texels at or
  below the alpha reference, tested at the first level no wider than 256). Rocks, fences, trunks and chairs are
  alpha-to-coverage draws too, with no cut-out texels; the foliage atlases (`ObjectTexturePlateTransparent`,
  `HO_LD_DECIDUOUS_WOOD_1`) are 30 to 75% cut out. Plant heights on three stages dip between 1.5 and 2 m (tufts and
  low plants below, bushes and trees above), hence the grass limit; each vertex gets an amplitude (2 cm per metre of the plant's
  height, at most 25 cm, times the square of its height within the plant) in `Vertex.specular[0]`, and the grass
  flag in `specular[1]`, which foliage does not use. The vertex shaders displace along a fixed wind direction by a slow gust, a swing and a fast flutter,
  phased by position; the cascades use the same function, so shadows move with the leaves. `Renderer::set_time`
  drives it (the window's clock; `viewer --time S` for screenshots).
- `taa` (`taa.rs`, `taa.wgsl`, needs `hdr`): temporal anti-aliasing on top of the MSAA. Each frame's projection is
  moved by a Halton (2, 3) point within the pixel (8 positions; the sun cascades are not jittered). After AO the
  resolved frame is blended into a history reprojected through the nearest depth of the 3x3 and read through a
  5-tap Catmull-Rom, with the history clamped to the 3x3 neighbourhood's box in brightness-compressed YCoCg narrowed
  to mean ± 1 σ, and written back before the post pass (TAA-2 below).
  The reprojection matrix (`previous * inverse(current)`, both unjittered) is built in f64: through f32 matrices
  far points land a fraction of a pixel off and the bilinear history read blurs the frame within a few frames.
  Pixels inside the car body's model-space box (its parts at rest plus 0.15 m, `Renderer::set_car_body`) reproject
  through the car's motion too (`previous * car_before * inverse(car_now) * inverse(current)`): as still stage the
  car, which moves with the chase or cockpit camera, smeared and its decals doubled (visuals-fix,
  `lead/reports/visuals-fix/taa-chase-54E-2400-{before,after}.png`). The TAA pass's cost is unchanged within noise
  (3.9 ms at 3840x2160 on the M-series test Mac, both builds).
  `viewer --frames 16` lets a screenshot settle.
  - Motion (TAA-2): measured with Visuals-8's flicker metric (24-frame 1080p sequences against 4K references of the
    same drive frames, mean |Δt| of the luma error over foliage and fence crops) on 54_E, RSF 127 and 201, chase and
    cockpit. The 4K reference with TAA (Visuals-8's) is itself blurred by the old resolve and favours it; the 4K 4x
    reference without TAA is noisier but unbiased, and decided where the two disagree. Kept, in order of gain: the
    Catmull-Rom history read (-8 to -19%; the bilinear read blurred the history every moving frame), variance
    clipping at 1 σ (-2 to -4%), the nearest depth of the 3x3 (-6 to -9%), the new frame's share growing from 0.12 to
    0.25 as the history moved up to 8 pixels (0.12 to 0.4 times the nearest weight upsampling), the blend in
    compressed colour (neutral, kept: a lone bright sample no longer flickers). Tried and not kept: a Blackman-Harris
    reconstruction of the jittered 3x3 as the new frame (+7%), clipping toward the box's centre instead of clamping
    (neutral), 4 or 16 jitter positions instead of 8 (neutral), sharpening (the Catmull-Rom read is already as sharp
    as the reference). Native: -25 to -37% against the unbiased reference, -4 to -30% against the TAA one; TAAU 67%:
    -7 to -18% / -9 to -22%. Cost at 3840x2160 native: the pass +1.5 ms (3.1 to 4.6 ms); TAAU unchanged.
    `lead/reports/taa2.md`.
  - Upsampling (perf-render): with a render scale below 100% (`--render-scale`, `Renderer::set_render_scale`) the
    frame is drawn into its own target (`Taa::input`) and `fs_taau` writes the history and the post pass's scene at
    the output size, so the post pass and the HUD stay at full resolution. Per output pixel: the frame's 3x3 samples
    about it, each placed at its jittered position (pixel `i` saw the unjittered point `i + 0.5 - jitter`) and weighted
    by its distance in output pixels (Blackman-Harris fit `exp(-2.29 d²)`); the history through a 5-tap Catmull-Rom;
    the history clipped to the samples' min/max box narrowed to mean ± 1.25 σ in YCoCg; the new samples blended in by
    0.12 times the nearest sample's weight, so each output pixel is built from the frames that put a sample on it;
    reprojection by the nearest depth of the 3x3 (edges follow the near surface). The jitter takes as many Halton
    positions as output pixels per frame pixel times 8 (18 at 67%, 32 at 50%). The main view's textures 0 and 1 get a
    mip bias of log2(scale) (`texture_bias` in `shader.wgsl`, `Frame.reflection.z`), so they keep the output's detail.
    At 100% the TAA is unchanged (pixel-identical).
    Captures: `lead/reports/perf-render/`.
  - One sample (perf-render): under `taa` the frame is drawn with one sample a pixel (`renderer::samples`) unless
    `msaa` (launcher "Multisampling with TAA", off by default) keeps the 4x. The passes then draw straight into their
    targets (`Renderer::over_attachment`), the depth readers bind a single-sampled depth (`renderer::depth_source`),
    and the cut-outs, with no alpha to coverage, keep each pixel with their coverage's chance by interleaved gradient
    noise shifted per TAA frame (`dither_coverage`, `DITHER` set in the stage shader only at one sample, so the 4x
    code is unchanged). Thin geometry (fences, tape, wires, foliage cards, distant trees) at 100%, 67% and 50%: the
    crops match the 4x ones; frame-to-frame flicker on foliage is 2-9% above 4x at 100%, the same at 67%. Without
    `taa`, and with `msaa`, the frames are pixel-identical to before. Captures: `lead/reports/perf-render/`.
  - Cost (TAA-3): the frame is drawn into `Taa::input` at full size too, so `fs_taa` writes the history and the scene
    as two targets and the copy back goes (+1 target of the frame's size); the 3x3's loads are written out rather than
    looped over, which let them overlap. The frames are byte-identical. GPU pass timestamps at 3840x2160 on the Mac:
    3.7 ms pass + 0.5-0.7 ms copy before, 3.2 ms after (3.4 with the wind's motion). The old profile spans ("waiting
    at each mark") come in steps of about 1.5 ms on Metal, too coarse for this. At 1080p about level (1.0-1.1 ms).
    TAAU 67% at 4K is unchanged (about 3.9 ms); written out its loop was slower, so it stays a loop.
  - Cost (TAAU-perf): `fs_taau` redid the 3x3 (spread, nearest depth) for every output pixel, 2.25 per frame pixel
    at 67%, in a loop that kept too many values live. A pass at the frame's size, `fs_taau_box`, now works out each
    frame pixel's clipping box once (two Rgba16Float targets; the high corner's w says which of the 3x3 is nearest),
    written out (in a loop its arrays spilled, 60% of that pass). `fs_taau` reads the box at its nearest frame pixel,
    the depth and the plants' motion where the nearest is, and takes the separable weights' sum in 3 bilinear fetches
    and a load. Measured by repeating the passes 20 times in a command buffer of their own (wall time, 54_E chase,
    user set with wind): 4K 67% 4.03 to 1.92 ms (native 3.35), 4K 50% 3.96 to 1.69, 1440p 67% 1.75 to 0.93 (native
    1.52), 1440p 50% 1.75 to 0.78; the drawn frame at 4K 67% 16.2-16.8 to 14.2-14.4 ms. Output within 2 levels
    inside the frame and flicker on TAA-2's crops equal to 3 decimals; the outer row and column differ more (the box
    and the weights there are taken at the clamped pixel rather than per clamped tap). `fs_taa`'s spread is written out the same
    way (lead's choice): at 4K 3.35 to 1.98 ms (1.83 without wind), at 1440p 1.52 to 0.92 (0.84); its output moves by
    1-3 levels in 0.1-0.6% of the pixels (the compiler fuses the sums differently), flicker equal to 3 decimals. Metal's render-pass timestamps read the end a frame late here.
    `lead/reports/taau-perf.md`.
  - The wind's motion (TAA-3): with `wind`, the swaying plants (`foliage.rs`; opaque cut-outs that write depth) are
    drawn again after the frame where the depth is theirs (`DrawPass::Velocity`, `vs_velocity`/`fs_velocity` in
    `shader.wgsl`, same `vertex` position as the frame), writing the sway's offset at the last frame's time less now
    (`Frame.wind.w`, x and y in an Rg16Float target, z following x along `WIND`). The resolve adds that motion,
    projected through the last frame's matrix, where it is set (nearest-depth pixel of the 3x3); everything else
    reprojects from the depth as before. A still camera's tree crops: error against the 4K reference -6 to -15% at the
    sequences' step (7 ms), -23% at 16 ms (60 fps), -40% at 35 ms; moving chase crops -1 to -3%. The motion pass costs
    about 0.45 ms at 4K and 0.3 ms at 1080p (it redraws the plants), the resolve +0.2 ms (TAAU +0.4 ms). Without
    `wind` nothing of it is built, and the frames are byte-identical. `lead/reports/taa3.md`.
- `particles` (`effects.rs`, `effects.wgsl`, `rain.wgsl`, `fs_soft` in `particles.wgsl`, `rain_rings` in
  `shader.wgsl`): rain, snow and particles redone. The game's precipitation particles (fat soft sprites that fall
  straight down whatever the eye does, plus 6 m haze cards that showed as light bands) are dropped
  (`Particles::drop_weather`); the renderer draws the weather itself (`Renderer::set_precipitation`):
  - Rain and snow on the GPU: up to 16000 drops in a 24 m box and 40000 in an 80 m box about the eye, placed by a hash
    of the instance and wrapped, so they keep their stage positions as the eye moves. Each drop is the streak it
    leaves over 1/60 s against the eye's motion (`Renderer::set_eye_velocity` for screenshots, measured from frame to
    frame in the window), 3 mm wide: thinner than a pixel it is drawn a pixel wide and fainter by the same share, and
    a long streak is fainter where the drop spent less of the exposure. Rain takes the sky's colour, snow the stage's
    light, with wobbling 1.2 cm flakes. Depth-tested; under TAA drawn after it with the depth tested in the shader,
    because the TAA, without motion vectors, would clamp streaks that move every frame back to the background.
  - Rings where rain lands on wet roads and water: tilt the water's normal in the film, the puddles and the SSR rays
    (about 1.2 rings a second per 0.3 m square at full rain, gone past 20 m).
  - Particles (dust, spray, smoke, snow spray) drawn in a pass that reads the depth (`draw_over`, as the windscreen
    glass is), with the effects' bind group: they fade into the stage behind them, are shaded as soft balls (sky light
    from above, the ground's darker bounce below, the sun wrapped round and dimmed through the dense middle, shaded by
    the sun's cascades, scattered forward against the sun) and have their edges eaten by noise fixed to each particle,
    so puffs from one sprite differ.
  - Chimney smoke (fx-2, `particles/track.rs`): the stages' track emitters (`.dls` table 4, the chimneys of 10, 14,
    41 and 46) never emit in 1.02 (`notes/particles.md`, Track emitters). Here each row whose dummy is active smokes
    while the eye is within the row's distance, from the file's emitter of the row's name when a modded file has one,
    else from a pale, slow, rising clone of `g_firesmoke_emitter` (`chimney_smoke`: 5 a second, 9 s life, 0.7 to 7 m,
    drifting with 0.15 of the wind); a seek makes 9 s of it at once so the plumes stand.
- `lens` (fx-2, removed 2026-10-18 at the user's request: its mud splats read as dark stars): lens grime and mud on
  the outside views. `--enhance lens` and an old `lens` key in `launcher-settings.ini` are ignored
  (`Enhance::REMOVED`). The game's camera dirt (`NParticleSystem2D::CParticleEffectCameraDirt`, vf08 0x5f23f0) is a 2D
  effect and not ported (`notes/particles.md`).
- Night (`night.wgsl`, `lamps.wgsl`, `lights::Lamp`, `Atmosphere::night`): a time of day, not an addition of its
  own: `--clock HH:MM` once the sun is 7 degrees down (see Time of day). The
  baked vertex light and light map are daylight, so the stage keeps colour times baked light as its albedo, lit by
  moonlight (`MOONLIGHT`, a blue-grey 0.11 to 0.19 of the baked light) and the car's lit lamps. The playback hands the
  lamps over each frame (`Renderer::set_lamps`, position, direction, colour times intensity, up to 8):
  - Headlights are dipped beams along the car (`beam`, visuals-fix): a sharp horizontal cut-off 0.7 degrees below
    the lamps' axis (aimed 1% down), softened over 0.46 degrees, with 1% of the light above it; under it a hot spot
    1.1 degrees high and about 19 degrees wide whose intensity falls with the square of the angle below, so the
    grazing road is lit about evenly from the bumper to the cut-off's reach (about 60 m); a faint, wide spill (1.5% of
    the hot spot). Trees and banks are lit on their lower part only. The earlier beam was centred on the axis with a
    soft top: it lit trees at and above lamp height brightly (lime green under HDR) and left the road dim (the user's
    cachy captures, `lead/reports/visuals-fix/cachy-127N-2030-*`, and `night-127N-2030-fiesta-*-before-after.png`). Light on a surface is
    `colour * share * facing / (d^2 + 0.25)`, facing wrapped 0.4 for the grazing road and two-sided on foliage, through
    a soft knee at 2.5. They cast shadows: a 2 x 1 radian perspective map from between the headlights (0.3 to 160 m)
    drawn into the sun's first cascade (the casters are the sun's: not the car), 3x3 filtered, looked up off the
    surface toward the lamp.
  - Tail, brake and reverse lights light the half-space they face (0.15, 0.8, 3). The lit lamp parts glow
    (`Shade::glow`, `mode.lit` 3).
  - The lit draws (the car) take moonlight as their sun and ambient; no car sun shadow; a dark blue haze nearer in.
  - The sky (`night_sky`): a dark blue gradient, stars hashed on a grid of directions (fading toward the horizon,
    twinkling with the time), the moon where the sun would be. Under overcast presets the dark fog only. Clouds and the
    backdrop are moonlit only.
  - Rain, snow and particles (`particles`) are lit by the lamps (`lamps_at`), so rain streaks show in the beams and
    dust and spray glow red behind the tail lights.
  - Wet roads mirror the lit lamps (`lamp_glint`): each lamp's face as a streak narrow across (0.03 rad, wider near
    the lamp) and long in elevation (0.3), as bright as the lamp shines toward that point (headlights 12, tail lights
    2.5, brake lights up to 8), times the film's reflectance; the rain rings break the streaks up. Seen from behind
    the car (red streaks) or ahead of it (white); the chase camera has them at the bottom edge, under the spray.
    Only roads with the wet film (a specular map: tarmac, not the gravel stages) reflect, day or night.
  - The windscreen (`glass.wgsl`, `Frame::night`): drops' rims and highlights and the flakes and bugs take the
    frame's light around them (a ring of samples 0.12 of the screen out) and a faint headlight glow, so drops over the
    beam-lit road catch the light and those against the dark trees stay faint; by day the glass is unchanged.
  - Without a drive (car at the start, or no car) no lamps are lit: the stage is moonlit only.

  The baked light maps (`shadow_N.dat`, one 1024x2048 or 2048 atlas per stage, 56 in retail) already hold the
  sun's static shadows on the ground chunks; the cascades add resolution near the eye and shadows on and from
  everything else (props, barriers, grass tufts, trees, terrain).

## Time of day (`viewer --clock HH:MM`, launcher "Time of day"; `relight.rs`, explore/time-of-day)

The game has no clock for the light: each variant letter (M, N, E, O) is a bake (`.lbs` vertex colours and the
`shadow_N.dat` light map) and a `TrackSettings.ini` section. Findings and what the renderer does:

- The opaque terrain's vertex light holds the bake's sun: luminance fits `a + b * facing` (`b` 0.26 to 0.34, r 0.5 to
  0.58 on 54 M and E, negative against the other variant's sun; `examples/baked_light.rs`). Foliage's is flat.
- Only 54 has two separately baked sunny variants. 69's M, E and N carry byte-identical vertex light baked with E's
  sun under three `SunDir`s; 73, 78, 81 and 83 to 89 share one bake about 110 degrees from their `SunDir`; 4's
  variants are identical. So the bake's sun is fitted from the data (`relight::fit`, best correlation over 5 degree
  directions), not taken from `SunDir`. Survey of all 86: `examples/relight_survey.rs`.
- Relight (`relit` in `shader.wgsl`): `b * facing(bake sun)` out of the luminance, `b * facing(drawn sun)` in, keeping
  35% (ambient). 54 M relit to E's sun against E's own bake: mean error 0.132 to 0.049, r 0.05 to 0.57. Normals are
  welded by position on opaque draws (`sun::with_normals`; seams made stripes), the cascades' facing term is skipped
  under the relight (it darkened twice), foliage takes the ground's normal under it (`relight::Ground`, 4 m cells),
  props without normals count as facing up.
- The sun's path (`relight::Clock`, ours): a solar model at a latitude and date per rally (`rally_of`, the real
  rallies' regions and seasons), anchored to the fitted bake sun since stage north is unknown (bake hour on the
  morning side for M, afternoon for E, nearest noon otherwise; `54O_E` is E). The clock runs from the start time in
  real time (`--clock-rate R` speeds it up; the launcher passes the start time only), and the atmosphere is rebuilt
  each frame (`Renderer::set_atmosphere`).
- Light: the sun warms and fades below 25 degrees; ambient stays full to a sun 4 degrees up, then falls to a third
  by -7 in a bluer tint (game-bright: a measured curve made a 3 degree sun look like night); the sky dims with it,
  less toward the sun. The game's cloud domes (`Clouds.lbs`, the camera-anchored branch of `shade`) dim by the same
  level (`relight_light.w * dusk`; visuals-6, ours): before, they stayed at full light over a dark stage at dusk and
  the water and wet roads reflected them. The level is exactly 1 without a clock and by day, so those frames are
  unchanged; at night the domes are not drawn. Captures with water off, before and after: 53_O 18:30 and 20:00, 71_M
  (clear) 16:00, 16:30, 18:30 and 20:00 (`lead/reports/visuals-6/sky-*`, `sky-grid.png`). The car's lamps light the stage from 6 degrees up and their parts glow; 1.02's front and tail lights
  are always on (no switch, `notes/car-lights.md`); ours adds a headlight key (L, `[controls] headlights`) that
  switches the front and tail lights off and on (brake and reverse lamps unaffected). Below -7 degrees: night, with no visible step. Lamp light skips
  pixels with no lit lamp within 150 m (`LAMP_REACH`): dusk costs about 0.6 ms at 1080p.
- Limits: fogged sections (`UseFog`, 54 M) get no sun shadows; the light maps'
  shadows past the cascades are the bake's; shared bakes with M names (73, 83 to 89) fit an evening sun, so their
  bake hour is probably wrong; the 206's headlight beam is faint (its lamp settings, not checked).

Metal: `smoothstep` with its edges reversed is undefined there and returned 0 (a beam that went dark wherever the road
climbed above the lamps, and the rain streaks' tails); written as `1 - smoothstep(low, high, x)`.

Already in the renderer before this (not additions): 4x MSAA with alpha to coverage, fog and haze, sky gradient,
tone curve, car sun and contact shadows, near-ground procedural detail (`ground_detail`, render-m13).

Findings (track-10_O, track-54_E, track-20_N, start views and a wet chase view, 1600x900):

- AO gives foliage volume, grounds props (log piles, grass tufts, roadside plants) and shades eaves; anisotropic
  filtering sharpens distant gravel. Moderate on their own.
- `hdr` and `sky` together change the look most: depth from the haze (forest tunnels, treelines going blue-grey),
  filmic contrast (snow reads as snow, colder light), and the exposure keeps dark forest stages near their plain
  brightness. Calibrated to match the plain renderer first, both were nearly invisible; the values above are tuned
  by eye on three stages and may be too strong (blue) on far-fogged stages.
- `sun` gives the clearest single change on sunny stages: barriers, tables, chairs and grass tufts cast crisp
  shadows and the scene reads as sunlit (`car_all_sun.png` against `car_base.png`). Under dense canopy
  (track-54_E) it shadows the whole road, which is physically right but loses the bright patch the baked map had.
- `wind`: t = 0 against t = 1.5 changes about half the sampled bytes in the start views, no tearing or detached
  leaves seen.
- `taa` (16 frames, still view): foliage cut-outs, branches against the sky and thin twigs lose their staircase
  without losing texture detail. With the history read through the jittered matrices instead, the frame went
  visibly soft.
- Without `--enhance` the frame is pixel-identical to before (checked on three views, again after wind and TAA).

- In the cockpit under `sky` the cabin is darker than the game's: the game's linear fog greys even the dashboard at
  half a metre, the aerial perspective does not haze what is next to the eye. Left as is (`cabin` relights it).
- `particles`: rain reads as rain (thin streaks at many depths, slanted by the car's speed) instead of white
  pills; snow streaks past at speed; dust has volume (darker undersides, sunlit tops, broken edges)
  (`rain_chase_old.png`/`rain_chase_new.png`, `dust_old.png`/`dust_new.png`, `snow_new.png`, `rain_bonnet_rings.png`).
  Without TAA the rings' sun glints sparkle a little.
- Without `--enhance`, merged with main (particles, glass, skid marks): pixel-identical to main's viewer on the
  start views, the car view and two drives (dust, wet cockpit with rain and drops) but for 2 sky pixels off by one
  on track-10_O and track-21_O, from the sky refactor's float rounding.

Cut-out structures are not plants (TAA-2): textures whose file names name a structure (`FENCE`, `FNC`, `PYLON`,
`POWERLINE`, `POLE`, `GATE`, `WINDMILL`, `LAMP`, `BARRIER`, `GUARDRAIL`, `BRIDGE`, `BANNER`, `PANNEAU`, `CROIX`, `Net`;
`foliage::STRUCTURES`) neither sway nor let the sun through, and cast their sun shadow at any height. Over the retail
stages and the RSF samples these words match only fences, pylons, power lines, gates, lamps, signs and nets. Colour
does not separate them: per piece, snowy firs on 10_O and 12_O are as grey as a pylon, and wooden fences as brown as
autumn leaves. Structures inside a shared atlas (some RSF `otpo`/`otpt` atlases) still count as plants.

Coverage-preserving alpha mips (visuals-8, tried, not kept): every retail texture stores four mip levels (D3D9, like
us, samples the 1/8 level beyond them), and their stored levels already keep their coverage (the 54_E atlas: 0.34,
0.36, 0.37, 0.39 of the texels above the alpha reference); RSF stages ship full DXT5 chains. Completing the chains to
1x1 with each level's alpha scaled to level 0's coverage changed the flicker against 4K references by +0.2 to +4%
(worse, within noise) on 54_E, RSF 127 and 201, under TAA, TAAU 67% and 4x MSAA alone: at 1080p only thin plants,
trunks and poles sample past the stored levels. The patch and the numbers: `lead/reports/visuals8.md`.

## Frame time

`viewer --frames N --profile` draws N frames offscreen and prints each stage's median (`profile.rs`). Where the
device has timestamps inside encoders and is not on macOS (Vulkan), each mark writes the GPU clock and the frame runs
as usual. On Metal, whose timestamps are sampled without waiting for the passes before, it waits on the GPU at each
stage's end instead; subtract the `(nothing)` line, about 1.3 ms, from each (`RBR_PROFILE=wait|stamps` overrides).
Medians, because other work on the machine makes the means jump.

At 1920x1080 on an M3 Pro (track-21_O chase and track-46_E wet cockpit at step 1500, `--enhance all`), net of the
waits:

| Stage | Before | After |
| --- | --- | --- |
| scene (4x MSAA) | 8.4 / 7.5 ms | unchanged code (see below) |
| ambient occlusion | 6.5 / 3.9 ms | 1.7 / 1.4 ms |
| screen reflections (water / wet roads) | 1.4 / 8.1 ms | 1.4 / 6.5 ms |
| sun cascades, lamp shadow, cube face, car shadow, TAA, post | about 0.3 ms each | same |

- The depth copy (`depth_copy.rs`): AO and the wet roads' SSR read the multisampled depth per sample step, which on
  Apple GPUs pulls the other three samples into the cache too. A single-sample copy of the first sample is the same
  values and cheaper to read. The copy itself costs about 0.6 ms at 1080p, more than the water's reflections alone
  save, so it is only made for AO or wet roads; the SSR shader reads whichever it has (`scene_depth_at`). The SSR
  steps also unproject the depth only once a step is behind it.
- AO at half resolution: the occlusion was 3.5 ms of the pass. The occlusion pass also writes each pixel's distance,
  so the blur reads that instead of rebuilding 16 positions. Up to 20/255 different from full resolution at edges,
  not visible side by side.
- The default frame stays pixel-identical (six views, dry and wet, chase and cockpit). With `all`, the depth copy and
  the distance map alone changed a few thousand pixels by 1/255.
- The default scene pass is shading-bound (3.4 ms with the shader cut to its texture), but no single part of it
  accounts for much: with the car shadow, ground detail, night or sun code removed in turn, each change was within
  the run-to-run noise (about ±0.5 ms). Not changed, since it would change the default frame.
- The wet cockpit's scene pass read 0.4 ms slower after the change in three interleaved runs, though the pass is
  unchanged; the chase view's did not. Probably the GPU's clock state after a lighter frame; not chased.
- Wet SSR (4 rays of 64 steps) is still the largest addition. Fewer steps or rays would change the default wet frame.

Not yet checked: the interactive window (wind motion, TAA ghosting when the camera moves, swaying foliage under TAA
with no motion vectors), low sun angles and the sun
in view (bloom), and whether `SunDir` matches the baked maps on every stage.

Visuals-9 (M3 Pro, RSF 127 chase with the sun ahead, the user's set; `--profile` spans, min of 3 to 6 interleaved
runs, net of the 1.5 ms mark; the Mac was shared with another worker's GPU runs, so whole-frame times swung by 10 ms
and only spans are given): GTAO against the SSAO 1.6 / 1.6 ms at 1920x1080, 6.1 / 6.1 ms at 3840x2160, 3.5 to 4.4 /
1.8 to 3.1 ms at 3840x2160 with TAAU 67%; the light shafts 0.05 to 0.25 ms at all three (one 4K run read 1.5 ms);
the scene pass with the height fog within the noise at 67% (11.0 / 11.2 ms), +0.1 to +1.6 ms at 1080p and 4K (the
noise there is about as large); the post pass with the two-pass meter within the noise (0.2 to 0.8 ms either way at
1080p, 1.7 ms both at 4K).

## Upscaled texture packs

Packs derive from the retail data and stay under `out/`, never committed.

### Pipeline

`rust/rbr-render/src/bin/texture-pack.rs`, then `viewer --textures <dir>/pack` (`Stage::apply_texture_pack`):

```sh
RBR_DATA=<install> texture-pack track-10_O out/texture-packs/track-10_O --model ultrasharp-4x
RBR_DATA=<install> viewer track-10_O --textures out/texture-packs/track-10_O/pack
```

1. The stage's DDS textures for one weather and wear, level 0 decoded to `source/*.png` (BC1/BC2 via `texpresso`).
2. 4x upscale with Upscayl's bundled Real-ESRGAN ncnn binary (`/Applications/Upscayl.app`) into `upscaled/`.
   Alpha comes through (RGBA in, RGBA out).
3. Re-encoded in the source format (DXT1 or DXT3), full mip chain by 2x2 box filter, largest level capped at
   4096 px, written to `pack/<request path>`. The pack replaces textures by request path, so weather and wear
   folders map one to one.

track-10_O dry/new: 25 files (most of the stage is four atlases up to 2048 px), about 3 minutes on an M-series Mac,
231 MB of upscaled PNGs, 70 MB pack.

### Findings (track-10_O, 1600x900, start view)

- Signs, text, doors, building planks: clearly sharper, no BC block noise.
- Foliage atlases: crisper branches; slight halos where the transparent black bleeds into the edge.
- Ground (snow, ice, gravel grain) depends on the model. `upscayl-standard-4x` smooths the grain to a plastic
  look. `ultrasharp-4x` keeps it and is the best of the bundled models, but amplifies faint BC block
  patterns. `remacri-4x` invents crack patterns, `high-fidelity-4x` cross-hatching.
- From the start view the screen rarely magnifies the original textures, so the in-game gain is modest; it
  shows close up (cockpit and bonnet views, roadside objects) and at higher window resolutions.
- The road's streaks are painted into the texture; neither upscaling nor anisotropic filtering changes them.

### Open

- Per-texture model choice (ground vs signs vs foliage), or removing BC artefacts before upscaling.
- Dilate colour into transparent texels before upscaling to remove foliage halos; alpha-coverage-preserving mips.
- Atlas tiles bleed into neighbours at 4x only if the model's receptive field crosses the border; not seen yet.
- BC7 output (needs `rbr_formats::dds` and the uploader to take DX10 headers) for better quality than DXT1.
- Car textures (`car.rs` loads them separately) and all stages: a batch mode over `Maps/`.
- Whether any addition becomes a default is a separate decision.
