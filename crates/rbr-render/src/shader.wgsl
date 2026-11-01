// Unlit stage drawing. Texture 0 times vertex colour; with `second_texture` the second texture blends in by
// its alpha and the vertex alpha (`RLDoubleTexture`, `notes/formats.md` "Shaders and vertex layouts"). On the lit car draws the second
// texture is the scrape texture, blended in after the light by the vertex alpha, the scrape amount (`CarPaint1.pso`). The alpha test drops texels
// with alpha at or below `alpha_ref`. Textures 0 and 1 are sampled with the mip LOD bias `mip_bias` (D3D9
// `D3DSAMP_MIPMAPLODBIAS`: 0 or `stage::MIP_MAP_BIAS`). The matrix is the view-projection from `coords`
// (left-handed, depth 0 to 1); no axis is flipped here.

// The haze and the sky are renderer choices from the stage's `TrackSettings.ini` section (`atmosphere.rs`): linear
// fog by the distance to the eye, and a sky from the fog colour at the horizon to `zenith` overhead.
struct Frame {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    // rgb, and w the most the fog covers (0: not fogged).
    fog_color: vec4<f32>,
    // x start, y end (metres); z 1 for the camera-anchored draws (the clouds); w the depth scale of the physically
    // based sky's haze (`sky_seen`: `Superbowl_Scale` for the backdrop, else 1).
    fog_range: vec4<f32>,
    // x 1 when the sky's draw lays the volumetric clouds' image over the sky (`cloud_volume_over`, the main view), 2
    // their dome (`cloud_dome_over`, the reflections and mirrors); y how far the
    // camera-anchored draws (the clouds) fade into the sky (`Atmosphere::cloud_haze`); z, w unused.
    haze: vec4<f32>,
    zenith: vec4<f32>,
    // The direction the sunlight travels; w = 1 to draw the sun's glow.
    sun: vec4<f32>,
    // The lit draws' ambient and sun colours (`Ambient*` times `Car_Ambient_Lighting`, `Diffuse*` times
    // `Car_Diffuse_Lighting`).
    ambient: vec4<f32>,
    diffuse: vec4<f32>,
    // The car's sun shadow map (renderer choice, `renderer::SHADOW_SIZE`): stage space to its clip space; x how much
    // the shadow darkens the stage (`Car_Shadow_Alpha`), y 1 when the map holds the car, z one filter step in map uv.
    shadow_matrix: mat4x4<f32>,
    shadow: vec4<f32>,
    // The contact shadow layer (`renderer::GROUND_REACH`): stage space to its clip space; x how much it darkens the
    // stage, y one filter step in map uv, z and w the near and far reach in map depth.
    ground_matrix: mat4x4<f32>,
    ground: vec4<f32>,
    // The uv rectangles (low xy, high xy) of the shadow map and the contact layer outside which their filters read no
    // car (`renderer::map_box`).
    shadow_box: vec4<f32>,
    ground_box: vec4<f32>,
    // x 1 when the reflection cube holds the stage around the car (`renderer::REFLECTION_SIZE`), else the sky is
    // reflected; y 1 in the in-car view (`renderer::Renderer::set_outside_view`); z 1 when the reflection probes are
    // baked (`surrounding`); w 1 when the surroundings cube holds the stage around the car (`paint_mirrored`).
    reflection: vec4<f32>,
    // x how wet the stage is: 0 dry, 0.5 damp, 1 wet (`road_shine`); y the car's dirt level (`dirt.rs`); w 1 when the
    // dirt maps' alpha is raised to its square root (ours, with the surfaces' dirt: weak maps show).
    wetness: vec4<f32>,
    // Renderer additions (`renderer::Enhance`): x 1 for the aerial perspective (`fogged`), y 1 for the flowing water
    // (`water_flowing`), z 1 for the puddles (`wet_plus`), w 1 on a snow stage's ground (`ground_far`).
    enhance: vec4<f32>,
    // The sun's shadow cascades (renderer addition, `sun.rs`): stage space to each cascade's clip space; `sun_shadow`
    // x how much the shadow darkens (0: off), y the filter step in cascade uv, z 1 when they hold the overcast sky's
    // light from overhead instead (`renderer::Renderer::shadow_light`), w 1 when the sun's shadows are on at all (the
    // baked light maps are then not used, `sun.rs`); `sun_texels` a texel of each and of the far layer (w) in metres;
    // `sun_splits` the far distance of each, and w the far layer's reach, metres.
    sun_matrices: array<mat4x4<f32>, 4>,
    sun_shadow: vec4<f32>,
    sun_texels: vec4<f32>,
    sun_splits: vec4<f32>,
    // The world layer (`sun::world`, `world_lit`): stage space to its clip space, and x 1 with it, y its texel, metres.
    world_matrix: mat4x4<f32>,
    world_shadow: vec4<f32>,
    // x the time in seconds; z how hard rain lands on the wet roads and the water (`rain_rings`, `Enhance::particles`).
    wind: vec4<f32>,
    // Night (renderer addition, `night.wgsl`): x 1 at night, y 1 when the headlights' shadow map (`lamp_matrix`, stage
    // space to its clip space) is drawn into its layer of `sun_map` (`LAMP_LAYER`); the car's lamps. z 1 in a mirror's pass
    // (`mirrors.wgsl`). w the bake's open-ground vertex light (`night_albedo` in `night.wgsl`; 0 without relighting).
    night: vec4<f32>,
    lamp_matrix: mat4x4<f32>,
    lamps: array<Lamp, 8>,
    // Toward the sun the stage was baked with, and w how much vertex light it gave (0: off; `relight.rs`, ours).
    relight: vec4<f32>,
    // The drawn sun's colour times its strength, and w the share of the bake's ambient that stays.
    relight_light: vec4<f32>,
    // The ambient light's colour, and w how much the car's lamps light the stage by day (dusk; `relight.rs`).
    dusk: vec4<f32>,
    // The clouds (`cloud_volume.wgsl`, ours): x their cover (0 to 1), y unused, zw their drift, metres a second.
    clouds: vec4<f32>,
    // The physically based sky (`sky_lut.wgsl`, ours): x 1 when its tables are drawn, y the sine of the sun's true
    // elevation, z 1 in the stage's frame, w 1 when the height fog is the whole fog (`height_fog::is_volume`).
    skylight: vec4<f32>,
    // The height fog (`height_fogged` in `sky_lut.wgsl`, ours): x its density at the base, per metre (0: none), y its
    // scale height and z the base's height, metres, w the density at the eye over the base's.
    height_fog: vec4<f32>,
    // The car body's model space to stage space, for its parts kept in model space (`placed`).
    car_body: mat4x4<f32>,
    // The moon's light on the stage at night (`Atmosphere::moonlight`), and w the night's level (1 under a clear sky,
    // less under cloud; `Atmosphere::night_level`), which the night sky takes.
    moon: vec4<f32>,
    // x unused, y the height over the eye the clouds' shadows are cast from (0: no volume), z the car's lowest height
    // and w 1 with a car (`ground_bounce`).
    occlusion: vec4<f32>,
    // The ground's maps' grid (`renderer::bounce::Ground`): its corner's x and z, columns and rows; 0 without them.
    ground_grid: vec4<f32>,
    // The day's light from the sky's tables (`skylight::DayLight`, `day_light` in `skylight.wgsl`), renderer units: the
    // sun's square on, w the pre-exposure (renderer units per cd/m2); the sky's above the horizon on up-facing ground, w
    // the gain from the tables' units (0 without the tables: at night, or without the sky).
    day_sun: vec4<f32>,
    day_sky: vec4<f32>,
    // Toward the sun, for the moon's lit side (`moon_shown`; opposite the moon for a full moon), and w 1 when the moon
    // is up (`Atmosphere::moon`).
    moon_disc: vec4<f32>,
    // The volumetric clouds' shadow map (`cloud_volume.wgsl`, ours): its corner's stage x and z, its side in metres,
    // and the height it was worked out at (the eye's); 0 without the volumetric clouds.
    cloud_shadow: vec4<f32>,
    // The car's dirt colour, and w the share of the paint's reflection it keeps (`dirt::Look`).
    car_dirt: vec4<f32>,
    // How light gets into the cabin (`car::cabin::SunIn`): rgb its faces' light bounced back against what comes in, w
    // their mean sky share; and where they see it, stage space, w the share of the sun the glass lets in
    // (`cabin_day_lit`).
    cabin: vec4<f32>,
    cabin_seen: vec4<f32>,
}

struct Mode {
    second_texture: u32,
    alpha_test: u32,
    alpha_ref: f32,
    mip_bias: f32,
    // 1: an opaque alpha-tested draw (foliage, fences, spectators; `pipelines::cutout`).
    cutout: u32,
    // 1: lit with `light` and `fresnel` (the car classes' settings), 2: lit glass, whose Fresnel term adds alpha.
    lit: u32,
    // 1: the water surface (`water_surface`).
    water: u32,
    // 1: a specular map in `specular_texture` (`road_shine`).
    specular: u32,
    // Ka, Kd, CookTorranceKs, CookTorranceM.
    light: vec4<f32>,
    // FresnelMin, FresnelMax, Kf; w 1 when `specular_texture` holds the car's dirt map (`stage::Shade::dirt`).
    fresnel: vec4<f32>,
    // x 1: an emissive mask in `glow_texture` (`backlit`: a dial's, the start board's); y 1 when its alpha counts (not
    // DXT1); z 1 when it glows by day too (the start board's lamps).
    glow: vec4<f32>,
    // x a glass pane's share of the mud film (`stage::Shade::grime`), 0 for none; y 1: interior glass
    // (`stage::Shade::tint`); z 1 for a wheel's or mud flap's dirt spatter (`stage::Shade::spatter`).
    grime: vec4<f32>,
    // The near ground's detail window in texture 0's uv (low u and v, width and height; z 0 for none;
    // `terrain_detail`).
    detail: vec4<f32>,
    // A lit lamp's part's glow colour (`stage::Shade::glow`), times `LAMP_GLOW`; w the share of it kept by day.
    lamp: vec4<f32>,
    // x and y texture 0's and texture 1's bank shares (`terrain_detail::bank_shares`; `terrain_bank`).
    bank: vec4<f32>,
}

// False for a pipeline whose draws have no alpha test (`renderer::PipelineKey::alpha_test`): its shader then has no
// `discard`, so a tiled GPU can drop the fragments it hides before shading them.
override ALPHA_TEST: bool = true;
override PERF_SHADE: f32 = 0.0;
// The pipeline's lit class (`renderer::textures::lit_class`, `mode.lit`): 0 the stage's, 1 to 4 the car's. Each
// pipeline is built with only its class's code; the code of the others held registers (on an M1 Pro every pixel ran
// several times slower for it). Past 4, `mode.lit` says at run time.
override LIT: u32 = 5u;

fn lit_class() -> u32 {
    if (LIT > 4u) {
        return mode.lit;
    }
    return LIT;
}

// The draw's features as its pipeline knows them (`renderer::textures::DrawFeatures`): the water stand-in, a specular
// map (the roads), a second texture, the near ground's mip bias (its detail). Without `KNOWN` the draw's mode says at
// run time. As with `LIT`, the code of the features a pipeline never draws held registers.
override KNOWN: bool = false;
override DRAW_WATER: bool = false;
override DRAW_SPECULAR: bool = false;
override DRAW_SECOND: bool = false;
override DRAW_DETAIL: bool = false;

fn water_draw() -> bool {
    return select(mode.water != 0u, DRAW_WATER, KNOWN);
}

fn specular_draw() -> bool {
    return select(mode.specular != 0u, DRAW_SPECULAR, KNOWN);
}

fn second_draw() -> bool {
    return select(mode.second_texture != 0u, DRAW_SECOND, KNOWN);
}

fn detail_draw() -> bool {
    return select(mode.mip_bias != 0.0, DRAW_DETAIL, KNOWN);
}

// The draw's frame uniform (`renderer::uniforms::frame_index`): 0 the world's (the stage and the car), 1 the
// camera-anchored clouds', 2 the sky faces'; past 2 the frame says at run time (`fog_range.z`, `skylight.z`). As with
// `LIT`, the code of the other frames held registers.
override FRAME_KIND: u32 = 3u;

// The camera-anchored draws (the clouds), which fade toward the horizon and into the sky.
fn camera_anchored() -> bool {
    if (FRAME_KIND > 2u) {
        return frame.fog_range.z != 0.0;
    }
    return FRAME_KIND == 1u;
}

// The stage's own frame, not the sky faces' or the clouds'.
fn stage_frame() -> bool {
    if (FRAME_KIND > 2u) {
        return frame.skylight.z != 0.0;
    }
    return FRAME_KIND == 0u;
}

// The lit pipelines' view (`renderer::pipelines::PipelineKey::inside`): 1 in the car (the cockpit; `frame.reflection.y`
// 1), 0 outside it, past 1 `frame.reflection.y` says at run time. As with `LIT`, the code of the other view held
// registers.
override INSIDE: u32 = 2u;

fn in_car() -> bool {
    if (INSIDE > 1u) {
        return frame.reflection.y != 0.0;
    }
    return INSIDE == 1u;
}
// True with the flowing water or the puddles (`renderer::Enhance::water`, `wet`; `water.wgsl`): without them the shader
// is built as before they were added (their branches, even untaken, moved a few pixels of water by a level).
override WATER_PLUS: bool = false;
// The car's lamps light the stage's draws (`renderer::PipelineKey::lamps`).
override LAMPS: bool = false;
// The loose and the paved roads and the snow take the road scans (`renderer::Enhance::gravel`, `asphalt`, `snow`;
// `road_scan`).
override GRAVEL: bool = false;
override ASPHALT: bool = false;
override SNOW: bool = false;
// The off-road ground takes the ground scan and eases the stage's baked tint (`renderer::Enhance::ground`; `ground_scan`).
override GROUND: bool = false;
// The stage has our grass field (`renderer::Enhance::grass`, `grass.rs`): the ground under it is thatched (`thatch`).
override GRASS: bool = false;
// The main view's draws after the depth prepass read the sun's shadow from its mask (`sun_mask.wgsl`), at the pixel
// `fs_main` or `fs_clouded` shades (`renderer::sun_mask`).
override SUN_MASK: bool = false;
// How many levels finer the surroundings cube is than the 128-texel cube the levels below are counted in (1 at
// `Detail::Ultra`, 256 texels).
override CUBE_SHIFT: f32 = 0.0;
var<private> pixel: vec2<i32>;

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var shadow_map: texture_depth_2d_array;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;
@group(0) @binding(3) var reflection_cube: texture_cube<f32>;
@group(0) @binding(4) var reflection_sampler: sampler;
// The surroundings cube (`Reflection`, ours): the stage around the car to the renderer's reach. The reflection cube
// holds the game's cells near the car, for the car's paint (`reflected`); what reads the car's surroundings reads this
// one.
@group(0) @binding(11) var surroundings_cube: texture_cube<f32>;
@group(0) @binding(5) var sun_map: texture_depth_2d_array;
// The ground's light seen from above (`bounce.rs`): per tile the sun's layer, then the sky's; and each grid cell's
// tile, or -1.
@group(0) @binding(14) var ground_light: texture_2d_array<f32>;
@group(0) @binding(15) var<storage, read> ground_tiles: array<i32>;
// The reflection probes (`renderer::probes`, ours): six layers each, and their grid.
@group(0) @binding(16) var probe_cubes: texture_cube_array<f32>;
@group(0) @binding(17) var<storage, read> probes: Probes;

struct Probes {
    // x and z of the grid's corner, its cell's side, metres.
    grid: vec4<f32>,
    // The grid's columns and rows, the probes' count.
    counts: vec4<u32>,
    points: array<vec4<f32>, 256>,
    // Each cell's nearest probe, row by row.
    cells: array<u32>,
}
@group(0) @binding(18) var shadow_ranges: texture_2d_array<f32>;
// The volumetric clouds' shadow map (`renderer::cloud_volume`, ours): each texel how much of the sun a cloud takes
// at the ground under it (`volume_shadow`), read by `cloud_shadow` when the volume is shown. In the frame's group so
// that every draw reads it; the vertex stage samples it (`vs_clouded`).
@group(0) @binding(19) var cloud_shadow_map: texture_2d<f32>;
@group(0) @binding(20) var cloud_shadow_sampler: sampler;
// The road scans (`renderer::road_scan`), 0 gravel, 1 asphalt and 2 snow: red the brightness, green and blue the normal's x
// and y, alpha the height; 3 the ground's colour and height, 4 its normal's x and y (`ground_scan`).
@group(0) @binding(21) var road_scans: texture_2d_array<f32>;
@group(0) @binding(22) var road_scan_sampler: sampler;
@group(1) @binding(0) var texture0: texture_2d<f32>;
@group(1) @binding(1) var texture1: texture_2d<f32>;
@group(1) @binding(2) var texture_sampler: sampler;
@group(1) @binding(3) var<uniform> mode: Mode;
@group(1) @binding(4) var shadow_texture: texture_2d<f32>;
@group(1) @binding(5) var specular_texture: texture_2d<f32>;
@group(1) @binding(6) var glow_texture: texture_2d<f32>;
// The screen-space reflections' pass (`fs_reflect`): the resolved frame and its depth.
@group(2) @binding(0) var scene_colour: texture_2d<f32>;
@group(2) @binding(1) var scene_depth: texture_depth_2d;
// The sun's first cascade's depth range per cell (`renderer::shadows::ShadowRange`), for the mask's pass.
@group(2) @binding(5) var sun_ranges: texture_2d_array<f32>;
// The stage's draws': the sun's mask (`sun_mask.wgsl`; 1 texel outside the main view, unread).
@group(2) @binding(3) var sun_mask: texture_2d<f32>;
@group(2) @binding(4) var sun_contact: texture_2d<f32>;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) uv2: vec2<f32>,
    @location(5) normal: vec3<f32>,
    @location(6) specular: vec3<f32>,
    // How little of the sky a stage vertex sees (`sky_occlusion.rs`), 0 elsewhere, and the mean direction it sees it in
    // (packed, `sky_direction`; 0 none).
    @location(7) occlusion: f32,
    @location(8) sky_dir: u32,
    // The light it gets off the stage per unit of the sun's and of the sky's (`bounce.rs`, packed, `bounce_light`).
    @location(9) bounce: vec2<u32>,
    // How much of the wet film the ground takes (`stage::Vertex::wet`) and how far the road's edge is (`Vertex::edge`).
    @location(10) wet: vec2<f32>,
    // 1 for the car's parts kept in its model space (`placed`), else 0.
    @builtin(instance_index) instance: u32,
}

struct VertexOut {
    // The depth prepass (`renderer::DrawPass`) needs the same depth from every pipeline that runs `vs_main`: Metal
    // gives it as is, Vulkan only with `@invariant`.
    @invariant @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv0: vec2<f32>,
    @location(2) uv1: vec2<f32>,
    @location(3) uv2: vec2<f32>,
    @location(4) world: vec3<f32>,
    @location(5) normal: vec3<f32>,
    @location(6) specular: vec3<f32>,
    // The mean direction the vertex sees the sky in, as its octahedral pair (`sky_direction`; none at 0, 0), and z how
    // little of the sky it sees.
    @location(7) sky: vec3<f32>,
    // The light it gets off the stage per unit of the sun's and of the sky's (`bounce_light`); a cabin vertex's
    // `bounce_sun` is the stage's shade about the car (`cabin_shade`).
    @location(8) bounce_sun: vec3<f32>,
    @location(9) bounce_sky: vec3<f32>,
    // How much of the wet film the ground takes and how far the road's edge is (`stage::Vertex::wet`, `edge`; `wet_plus`).
    @location(10) wet: vec2<f32>,
}

// `VertexOut` and how much of the sun a cloud takes there (`cloud_shadow`), for the volumetric clouds
// (`Enhance::lighting`, `vs_clouded`): the clouds are kilometres across, so per vertex. A separate entry point, so the
// frame without them is the same as before they were added (an extra output moved a few edge pixels).
struct CloudedOut {
    @invariant @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv0: vec2<f32>,
    @location(2) uv1: vec2<f32>,
    @location(3) uv2: vec2<f32>,
    @location(4) world: vec3<f32>,
    @location(5) normal: vec3<f32>,
    @location(6) specular: vec3<f32>,
    @location(7) sky: vec3<f32>,
    @location(8) bounce_sun: vec3<f32>,
    @location(9) bounce_sky: vec3<f32>,
    @location(10) wet: vec2<f32>,
    @location(11) cloud: f32,
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    return vertex(v);
}

@vertex
fn vs_clouded(v: VertexIn) -> CloudedOut {
    var moved = v;
    if (camera_anchored()) {
        moved.position = cloud_turn(v.position);
    }
    let o = vertex(moved);
    return CloudedOut(
        o.clip, o.color, o.uv0, o.uv1, o.uv2, o.world, o.normal, o.specular, o.sky, o.bounce_sun, o.bounce_sky, o.wet,
        cloud_shadow(o.world),
    );
}

// `v` in stage space. The car's parts that move only with its body are kept in its model space and drawn as instance
// 1 (`Renderer::set_car_vertices`): `frame.car_body` places them as `Playback::apply` places the others.
fn placed(v: VertexIn) -> VertexIn {
    if (v.instance == 0u) {
        return v;
    }
    var out = v;
    let turn = mat3x3<f32>(frame.car_body[0].xyz, frame.car_body[1].xyz, frame.car_body[2].xyz);
    out.position = (frame.car_body * vec4<f32>(v.position, 1.0)).xyz;
    out.normal = turn * v.normal;
    // Where a cabin vertex sees the sky turns with it (`car::cabin::turned`).
    if (v.specular.y >= CABIN_INTERIOR) {
        let seen = turn * vec3<f32>(v.uv2, (fract(v.specular.y) - CABIN_SEEN_Z.x) / CABIN_SEEN_Z.y);
        out.uv2 = seen.xy;
        out.specular.y = floor(v.specular.y) + CABIN_SEEN_Z.x + CABIN_SEEN_Z.y * clamp(seen.z, -1.0, 1.0);
    }
    return out;
}

// The direction `sky_occlusion.wgsl` packs as the octahedral pair `p` (about y), or `normal` for none (0, 0; the
// pair interpolates across a face, which holds over the upper half where the directions lie).
fn sky_direction(p: vec2<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (all(p == vec2<f32>(0.0))) {
        return normal;
    }
    var d = vec3<f32>(p.x, 1.0 - abs(p.x) - abs(p.y), p.y);
    if (d.y < 0.0) {
        d = vec3<f32>((1.0 - abs(p.y)) * sign(p.x), d.y, (1.0 - abs(p.x)) * sign(p.y));
    }
    return normalize(d);
}

fn vertex(input: VertexIn) -> VertexOut {
    let v = placed(input);
    var out: VertexOut;
    out.clip = frame.view_projection * vec4<f32>(v.position, 1.0);
    out.color = v.color;
    out.uv0 = v.uv0;
    out.uv1 = v.uv1;
    out.uv2 = v.uv2;
    out.world = v.position;
    if (is_grass_top(v)) {
        // Shrunk into the ground past the density where it stands and at the far edge of the grass.
        let height = v.uv2.x * grass_grown(distance(v.position, frame.eye.xyz), v.uv2.y);
        out.world = v.position + vec3<f32>(0.0, height - v.uv2.x, 0.0) + grass_sway(v.position, max(height, 1e-3));
        out.clip = frame.view_projection * vec4<f32>(out.world, 1.0);
    }
    out.normal = v.normal;
    out.specular = v.specular;
    out.sky = vec3<f32>(unpack2x16snorm(v.sky_dir), v.occlusion);
    out.bounce_sun = bounce_light(v.bounce.x);
    out.bounce_sky = bounce_light(v.bounce.y);
    out.wet = v.wet;
    // The cabin's vertices carry the stage's shade about the car there instead (`cabin_shade`; the stage's bounce is
    // not the car's): the same few points for every vertex, so not per pixel.
    if (lit_class() == 1u && v.specular.y >= CABIN_INTERIOR && in_car()) {
        out.bounce_sun = cabin_shade(cabin_seen(out));
    }
    return out;
}

// The top of one of our grass cards (`grass.rs`: `specular.x` past `GRASS_MARK` by its brightness, marked as a tuft,
// `uv2.x` its height; no shadow map on them).
fn is_grass_top(v: VertexIn) -> bool {
    return v.specular.x > GRASS_MARK && v.specular.x < GRASS_MARK + 3.0 && v.specular.y > 0.5 && v.specular.y < 1.5
        && v.specular.z > 0.5 && v.uv2.x > 0.0;
}

// How far the wind moves a grass card's top at `p`, `height` above its foot (ours): a bow along `GRASS_WIND` by
// `GRASS_SWAY` of its height, in gusts that cross the field as a slow wave (`GRASS_GUST`: its metres per radian and
// radians a second), with a quicker flutter of its own (`GRASS_FLUTTER`), dipping the top a little as it bows.
const GRASS_WIND: vec2<f32> = vec2<f32>(0.8, 0.6);
// The distances from the eye, metres, over which our grass's cards flatten into the ground and drawing ends
// (`grass::REACH`), and the ground under it fades its thatch (`thatch`).
const GRASS_FADE: vec2<f32> = vec2<f32>(100.0, 110.0);
// The grass is full within `GRASS_THIN.x` metres of the eye, then halves every `.y` (`grass::density`); a card of rank
// `keep` (`grass::Card::keep`) is full height under the density and shrinks into the ground up to `.z` times it
// (`grass::GROW_MARGIN`).
const GRASS_THIN: vec3<f32> = vec3<f32>(50.0, 25.0, 1.5);

fn grass_density(d: f32) -> f32 {
    return min(exp2((GRASS_THIN.x - d) / GRASS_THIN.y), 1.0) * (1.0 - smoothstep(GRASS_FADE.x, GRASS_FADE.y, d));
}

// How grown a card of rank `keep` is `d` metres from the eye, 0 to 1.
fn grass_grown(d: f32, keep: f32) -> f32 {
    let density = grass_density(d);
    return 1.0 - smoothstep(density, density * GRASS_THIN.z, keep);
}
const GRASS_SWAY: f32 = 0.22;
const GRASS_GUST: vec2<f32> = vec2<f32>(2.5, 1.4);
const GRASS_FLUTTER: vec2<f32> = vec2<f32>(0.3, 4.3);

fn grass_sway(p: vec3<f32>, height: f32) -> vec3<f32> {
    let t = frame.wind.x;
    let gust = 0.5 + 0.5 * sin(dot(p.xz, GRASS_WIND) / GRASS_GUST.x - t * GRASS_GUST.y);
    let flutter = sin(dot(p.xz, vec2<f32>(1.0, 1.3)) / GRASS_FLUTTER.x + t * GRASS_FLUTTER.y);
    let bow = (0.25 + 0.75 * gust * gust + 0.15 * flutter) * GRASS_SWAY * height;
    return vec3<f32>(GRASS_WIND.x * bow, -0.4 * bow * bow / height, GRASS_WIND.y * bow);
}

// A colour packed as `bounce.wgsl` packs it: three 10-bit shares of 0 to 2.
fn bounce_light(w: u32) -> vec3<f32> {
    return vec3<f32>(vec3<u32>(w, w >> 10u, w >> 20u) & vec3<u32>(1023u)) * (2.0 / 1023.0);
}

@fragment
fn fs_main(v: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    pixel = vec2<i32>(v.clip.xy);
    return shade(v, front, 0.0);
}

@fragment
fn fs_clouded(c: CloudedOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    pixel = vec2<i32>(c.clip.xy);
    let v = VertexOut(
        c.clip, c.color, c.uv0, c.uv1, c.uv2, c.world, c.normal, c.specular, c.sky, c.bounce_sun, c.bounce_sky, c.wet,
    );
    return shade(v, front, c.cloud);
}

// A stage or lit draw's colour (`shade_stage`, `shade_car`) by its lit class. A fence (`fences::FENCE`) takes its
// normals under our own light (`own_light`), a span's (two-sided) turned to the eye; else it is lit as before, as
// facing up.
fn shade(v: VertexOut, front: bool, under_cloud: f32) -> vec4<f32> {
    if (lit_class() != 0u) {
        return shade_car(v, front, under_cloud);
    }
    var seen = v;
    var facing = front;
    if (is_fence(v)) {
        let own = computed_occlusion() && day_on();
        let turned = select(1.0, -1.0, !front && v.specular.z > 0.0);
        seen.normal = select(vec3<f32>(0.0), v.normal * turned, own);
        facing = true;
    }
    return shade_stage(seen, facing, under_cloud);
}

// Whether `v` is a fence's (`fences::FENCE`). Its `specular.z` is the share of the light on a span's other side that
// it lets through (`fences::THROUGH`, 0 on the posts).
const FENCE: f32 = 0.1;

fn is_fence(v: VertexOut) -> bool {
    return mode.cutout != 0u && !specular_draw() && abs(v.specular.x - FENCE) < 0.025;
}

// A stage draw's colour (`lit_class` 0: the stage, its props and characters' cards), with `under_cloud` of the sun taken
// by a cloud.
fn shade_stage(v: VertexOut, front: bool, under_cloud: f32) -> vec4<f32> {
    let first = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias);
    if (PERF_SHADE == 1.0) {
        var a = first.a;
        if (ALPHA_TEST && mode.alpha_test != 0u && a <= mode.alpha_ref) { discard; }
        return vec4<f32>(v.color.rgb * first.rgb, a);
    }
    if (WATER_PLUS && water_draw() && frame.enhance.y != 0.0) {
        let surface = water_flowing(v);
        return vec4<f32>(graded(fogged(surface.rgb, v.world)), surface.a);
    }
    if (water_draw()) {
        let surface = water_surface(v.world);
        return vec4<f32>(graded(fogged(surface.rgb, v.world)), surface.a);
    }
    // The textures' colour, then lit by the vertex light.
    var surface = first.rgb;
    var alpha = first.a;
    var second = vec4<f32>(0.0);
    if (second_draw()) {
        second = textureSampleBias(texture1, texture_sampler, v.uv1, mode.mip_bias);
        surface = mix(first.rgb, second.rgb, second.a * v.color.a);
    }
    // The backdrop (the world-anchored sky faces' frame) gets detail its stretched texture lacks.
    if (!stage_frame() && !camera_anchored()) {
        surface = surface * backdrop_detail(v.world);
    }
    // With the asphalt scan the paved road's texture colour over its local mean (`ASPHALT_MEAN`), see `albedo_remap` below.
    var paved_mean = vec3<f32>(1.0);
    var paved_flat = 0.0;
    var paved_grey = 1.0;
    var paved_line = 0.0;
    // The factor easing the off-road ground's pale texels (`ground_pale_ease`).
    var ground_pale = 1.0;
    // How much of the pixel is ground (`ground_share`), with the paved mesh's grass and earth shoulders (`shoulder`, found by
    // the asphalt's grey test).
    var ground = ground_share(v);
    var shoulder = 0.0;
    // The near ground gets the fine structure its thinly spread texture lacks (`terrain_detail`).
    if (detail_draw() && stage_frame() && mode.cutout == 0u && mode.detail.z != 0.0) {
        // On the banks the texture's streaks are blurred out first (`terrain_even`); the flat keeps its filtering.
        let steep = terrain_steep(v.world);
        let levels = vec2<f32>(terrain_level(texture0, v.uv0), terrain_level(texture1, v.uv1));
        // With the ground scan the off-road ground's texture is read `GROUND_BLUR` levels blurrier on the flat too.
        // Only where the texture is magnified past `GROUND_MAGNIFIED` (its smear): a crisp one keeps its own detail.
        let texels = vec2<f32>(textureDimensions(texture0));
        let native = min(length(dpdx(v.uv0) * texels), length(dpdy(v.uv0) * texels));
        let magnified = 1.0 - smoothstep(GROUND_MAGNIFIED.x, GROUND_MAGNIFIED.y, native);
        // The paved road's texture is read no finer than `ASPHALT_FINEST` metres a pixel (`asphalt_texel`), as the
        // hardware's filter would at the draw's mip bias where it is coarser.
        let paved_reach = max(exp2(mode.mip_bias), ASPHALT_FINEST / max(min(length(dpdx(v.world)), length(dpdy(v.world))), 1e-6));
        let paved_grads = array<vec2<f32>, 4>(dpdx(v.uv0), dpdy(v.uv0), dpdx(v.uv1), dpdy(v.uv1));
        // The paved mesh's shoulders carry grass textures: only a grey texture (the road's) takes the asphalt's treatment.
        var paved_share = select(0.0, smoothstep(0.75, 0.95, v.wet.x), ASPHALT);
        if (paved_share > 0.0) {
            paved_mean = terrain_even(texture0, v.uv0, 0.0, ASPHALT_MEAN).rgb;
            if (second_draw()) {
                paved_mean = mix(paved_mean, terrain_even(texture1, v.uv1, 0.0, ASPHALT_MEAN).rgb, second.a * v.color.a);
            }
            var probe = terrain_even(texture0, v.uv0, 0.0, ASPHALT_BLUR).rgb;
            if (second_draw()) {
                probe = mix(probe, terrain_even(texture1, v.uv1, 0.0, ASPHALT_BLUR).rgb, second.a * v.color.a);
            }
            let top = max(probe.r, max(probe.g, probe.b));
            paved_grey = 1.0 - smoothstep(ASPHALT_GREY.x, ASPHALT_GREY.y, (top - min(probe.r, min(probe.g, probe.b))) / max(top, 1e-3));
            paved_share = paved_share * paved_grey;
            // The paved mesh's grass and earth shoulders meet the off-road ground: they take its treatment, so no seam
            // shows. Not a coloured road or wall: only a warm texture (`ground_rock`'s test) on a face under `SHOULDER_WALL`.
            let warm = smoothstep(GROUND_ROCK_AT.x, GROUND_ROCK_AT.y, (max(probe.r, probe.g) - probe.b) / max(top, 0.05));
            let up = select(1.0, abs(v.normal.y) * inverseSqrt(dot(v.normal, v.normal)), dot(v.normal, v.normal) > 1e-8);
            shoulder = (1.0 - paved_grey) * warm * smoothstep(SHOULDER_WALL.x, SHOULDER_WALL.y, up);
            ground = max(ground, smoothstep(0.75, 0.95, v.wet.x) * shoulder);
        }
        paved_flat = paved_share;
        let blur = steep * TERRAIN_EVEN
            + select(0.0, (GROUND_BLUR + GROUND_FAR_BLUR * ground_far(v.world)) * ground * magnified, GROUND);
        var even = surface;
        if (blur > 0.0) {
            even = terrain_even(texture0, v.uv0, levels.x, blur).rgb;
            if (second_draw()) {
                even = mix(even, terrain_even(texture1, v.uv1, levels.y, blur).rgb, second.a * v.color.a);
            }
        }
        if (paved_share > 0.0) {
            var paved = asphalt_texel(texture0, v.uv0, paved_grads[0], paved_grads[1], paved_reach);
            if (second_draw()) {
                paved = mix(paved, asphalt_texel(texture1, v.uv1, paved_grads[2], paved_grads[3], paved_reach), second.a * v.color.a);
            }
            even = mix(even, paved, paved_share);
            // The painted lines stay crisp: where the blurred road is brighter than its mean, it is paint, with its own texels.
            let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
            paved_line = smoothstep(ASPHALT_LINE.x, ASPHALT_LINE.y, dot(even, luma) / max(dot(paved_mean, luma), 1e-3));
            even = mix(even, surface, paved_line);
        }
        surface = even;
        // On the steep banks the texture painted from above streaks down the fall line: a triplanar sampling of the
        // draw's own window takes its place (`terrain_bank`), weighted by the steepness and by how far the textures'
        // steep faces are banks (`mode.bank`: not a facade's, a wall's or a fence's). The flat reads 0 steepness,
        // the crisp and the paved road keep theirs bit for bit (the branch is skipped).
        let bank_share = select(mode.bank.x, mix(mode.bank.x, mode.bank.y, second.a * v.color.a), second_draw());
        let bank_weight = steep * (1.0 - paved_share) * bank_share;
        let bank_dx = dpdx(v.world);
        let bank_dy = dpdy(v.world);
        let bank_ax = dpdx(v.uv0);
        let bank_ay = dpdy(v.uv0);
        let bank_bx = dpdx(v.uv1);
        let bank_by = dpdy(v.uv1);
        if (bank_weight > 0.0) {
            surface = mix(
                surface,
                terrain_bank(v, second.a * v.color.a, bank_dx, bank_dy, bank_ax, bank_ay, bank_bx, bank_by),
                bank_weight
            );
        }
        if (GROUND) {
            var mean = terrain_even(texture0, v.uv0, levels.x, GROUND_MEAN).rgb;
            if (second_draw()) {
                mean = mix(mean, terrain_even(texture1, v.uv1, levels.y, GROUND_MEAN).rgb, second.a * v.color.a);
            }
            ground_pale = mix(1.0, ground_pale_ease(surface, mean), ground * max(ground_near(v.world), ground_far(v.world)));
        }
        // With the ground scan the stage's own high pass is a share of what it was, on the ground the scan covers, and
        // on the paved road with the asphalt scan.
        let keep = (1.0 - select(0.0, ground * (1.0 - GROUND_KEEP), GROUND)) * (1.0 - paved_share * (1.0 - ASPHALT_KEEP));
        surface = surface * mix(1.0, terrain_detail(v, steep), keep);
    }
    let night = frame.night.x != 0.0;
    // With the computed occlusion the stage's light is ours (`own_light`), its sun shadowed by what hides the sun.
    let own = computed_occlusion() && day_on() && !camera_anchored();
    // The texture's linear albedo, which under our physical light is brought to the measured ranges (`albedo_remap`).
    var albedo = pow(surface, vec3<f32>(2.2));
    if (own) {
        let remapped = albedo_remap(albedo);
        albedo = remapped;
        // `albedo_remap`'s cut steepens through the road's brightness (up to 2.6 times its contrast, the flecks of the
        // texture's fine variation): the paved road takes the cut of its local mean, with its own variation over it, and
        // its painted lines (brighter than the mean) the plain cut.
        if (paved_flat > 0.0) {
            let mean = pow(paved_mean, vec3<f32>(2.2));
            let evened = albedo_remap(mean) * albedo / max(mean, vec3<f32>(1e-4));
            albedo = mix(remapped, mix(evened, remapped, paved_line), paved_flat);
        }
        if (is_tuft(v) && v.specular.x > GRASS_MARK) {
            // Our grass cards (`grass.rs`) take the ground's brightness under them.
            albedo = albedo * (v.specular.x - GRASS_MARK);
        } else if (is_tuft(v)) {
            // Pale tufts take the straw tint, and green leaves are no brighter than `TUFT_GREEN_MOST` (leaves measure
            // about 0.08 to 0.15): brighter, 580's lime ferns read as lit card beside our grass. Straw and snow keep
            // theirs.
            let luma = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
            let pale = smoothstep(TUFT_PALE.x, TUFT_PALE.y, luma) * (1.0 - smoothstep(TUFT_SNOW.x, TUFT_SNOW.y, luma));
            albedo = albedo * mix(vec3<f32>(1.0), TUFT_STRAW, pale);
            let green = albedo.g - max(albedo.r, albedo.b);
            let y = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
            albedo = albedo * mix(1.0, min(TUFT_GREEN_MOST / max(y, 1e-4), 1.0), smoothstep(0.02, 0.06, green));
        }
    }
    // A shrub's dark leaves are lifted, a tall tree's are not (`CROWN_SHRUB`, by the crown's half height).
    if (own && mode.cutout != 0u && !second_draw()) {
        let half = crown_radii(v).y;
        if (is_crown(v)) {
            albedo = albedo * mix(CROWN_LIFT, 1.0, smoothstep(CROWN_SHRUB.x, CROWN_SHRUB.y, half));
        }
    }
    var sun_seen = 1.0;
    var colour: vec3<f32>;
    if (own) {
        // The sun's or, once it is down, the moon's share past the shadows.
        sun_seen = own_sun_seen(v, under_cloud);
        // A crown's own leaves shade it softly, rather than its cards each other (`crown_sun_seen`).
        if (mode.cutout != 0u && !second_draw()) {
            let crown = crown_sun_seen(v, under_cloud, crown_radii(v));
            if (is_crown(v)) {
                sun_seen = crown;
            }
        }
        // Lit in linear light (`own_light`), then back to the display colour the rest of the frame works in.
        colour = pow(own_light(v, sun_seen, under_cloud) * albedo, vec3<f32>(1.0 / 2.2));
    } else {
        colour = relit(v) * surface;
    }
    // Our wet road (`wet_plus`), with the puddles' addition in wet and damp weather where the surface takes it.
    let wet = WATER_PLUS && frame.enhance.z != 0.0 && frame.wetness.x > 0.0;
    let wet_road = wet && detail_draw() && v.wet.x > 0.0;
    if (own && specular_draw() && !wet_road) {
        colour = with_specular(colour, v, front, sun_seen);
    }
    // The game's highlight off the road's specular map: the stones' detail on gravel, so kept beside our reflection.
    // With our light, as strong as the direct light against noon's: none once the sun has set and the moon is down.
    if (specular_draw() && !night) {
        var shine = road_shine(v);
        if (own) {
            shine *= sun_seen * min(dot(frame.day_sun.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) / NOON_DIRECT, 1.0);
        }
        colour = colour + shine;
    }
    // The shadow light map multiplies the colour (`color = (base + spec) * shadow.rgb`, `notes/formats.md` "Shaders and vertex layouts"). With the
    // sun's shadows on (`enhance.x`) it is not used: their cascades and far layer cast for any sun, and the moon.
    let baked = select(textureSample(shadow_texture, texture_sampler, v.uv2).r, 1.0, frame.enhance.x != 0.0);
    // How much the sun's shadows reach here (`sun_light` below fades out with it).
    let near_sun = sun_reach(v.world);
    // The car's lamps (`lamp_sum`) at night and at dusk (`frame.dusk.w`, ours), as at night over the fading daylight:
    // with our light in lux (`lamp_lux`: over pi a white face's luminance, pre-exposed), added in linear light.
    var lamps = vec3<f32>(0.0);
    if (LAMPS && !camera_anchored() && (night || frame.dusk.w > 0.01)) {
        let has_normal = dot(v.normal, v.normal) > 1e-8;
        let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
        lamps = lamp_sum(v.world, n, mode.cutout != 0u);
    }
    if (own) {
        let lux = lamp_lux(lamps) * frame.day_sun.w / SKY_PI;
        colour = pow(pow(colour, vec3<f32>(2.2)) + albedo * lux, vec3<f32>(1.0 / 2.2));
    } else if (night) {
        colour = night_lit(surface, baked, v, front, lamps);
    } else {
        colour = colour * baked + lamp_albedo(surface, v) * lamp_knee(lamps) * frame.dusk.w;
    }
    if (wet_road) {
        colour = wet_plus(colour, v);
    }
    // The near ground (the unlit draws with the mip bias) gets fine detail where its textures are blurred.
    if (detail_draw()) {
        colour = colour * ground_detail(v.world);
        // The loose roads (gravel and dirt, half the film's share, `stage::Vertex::wet`) take the gravel scan, the paved
        // ones (tarmac, all of it) the asphalt's, the snow (`puddles::SNOW_MARK`) the snow's.
        if (GROUND) {
            let share = ground;
            let face_dx = dpdx(v.world);
            let face_dy = dpdy(v.world);
            if (share > 0.0) {
                let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
                let seen = select(1.0, sun_seen, own);
                colour = mix(colour, ground_scan(colour * ground_pale, v.world, n, face_dx, face_dy, seen, surface), share);
            }
        }
        if (GRAVEL || ASPHALT || SNOW) {
            let seen = select(1.0, sun_seen, own);
            let loose_uv = v.world.xz / SCAN_TILE[0];
            let loose_dx = dpdx(loose_uv);
            let loose_dy = dpdy(loose_uv);
            let loose = smoothstep(0.05, 0.3, v.wet.x) * (1.0 - smoothstep(0.6, 0.9, v.wet.x));
            if (GRAVEL && loose > 0.0) {
                colour = colour * mix(1.0, road_scan(0, loose_uv, loose_dx, loose_dy, seen, 1.0), loose);
            }
            let paved_uv = v.world.xz / SCAN_TILE[1];
            let paved_dx = dpdx(paved_uv);
            let paved_dy = dpdy(paved_uv);
            // With the ground scan, the shoulders take its grain instead (`shoulder`).
            let paved = smoothstep(0.75, 0.95, v.wet.x) * (1.0 - select(0.0, shoulder, GROUND));
            if (ASPHALT && paved > 0.0) {
                colour = colour * mix(1.0, road_scan(1, paved_uv, paved_dx, paved_dy, seen, paved_grey), paved);
            }
            let snow_uv = v.world.xz / SCAN_TILE[2];
            let snow_dx = dpdx(snow_uv);
            let snow_dy = dpdy(snow_uv);
            let snowy = 1.0 - smoothstep(-0.9, -0.3, v.wet.x);
            if (SNOW && snowy > 0.0) {
                colour = colour * mix(1.0, road_scan(2, snow_uv, snow_dx, snow_dy, seen, 1.0), snowy);
            }
        }
        if (GRASS && mode.cutout == 0u) {
            colour = thatch(colour, v, cross(dpdx(v.world), dpdy(v.world)));
        }
        // The ground around the roads is soaked too, a little less (`GROUND_SOAK` of the road's, by the wetness).
        if (wet && !wet_road) {
            colour = soak(colour, GROUND_SOAK * frame.wetness.x);
        }
    }
    if (own && frame.sun_shadow.z == 0.0) {
        // Our light holds the sun's, the car's and the clouds' shadows (`own_sun_seen`); the contact shadow and the
        // leaves' glow stay.
        if (frame.ground.x != 0.0) {
            colour = colour * (1.0 - frame.ground.x * car_contact(v.world));
        }
        if (near_sun > 0.0 && frame.skylight.y > 0.0) {
            colour = colour * (1.0 + leaf_glow(v, sun_seen, first.rgb) * near_sun);
        }
    } else {
        // The sun shadow fades where the point sees little sky (in the shade of trees or banks).
        let open_sky = smoothstep(0.3, 0.8, sky_share(v));
        if (frame.shadow.x * open_sky != 0.0) {
            colour = colour * (1.0 - frame.shadow.x * car_shadow(v.world) * open_sky);
        }
        if (frame.ground.x != 0.0) {
            colour = colour * (1.0 - frame.ground.x * car_contact(v.world));
        }
        // Under a cloud's shadow (`cloud_shadow`) the sun's own shadows fade with the sunlight they take away.
        colour = colour * (1.0 - CLOUD_SHADE * under_cloud * mix(0.4, 1.0, open_sky));
        // At night the moon's shadows are in `night_lit`, off the lamps' light.
        if (near_sun > 0.0 && !night) {
            colour = colour * mix(vec3<f32>(1.0), sun_light(v, front, open_sky, first.rgb), near_sun * (1.0 - under_cloud));
        }
    }
    colour = backlit(colour, first.rgb, v.uv0, night, 1.0);
    alpha = plant_faded(v, alpha);
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    // Camera-anchored draws (the clouds, `frame.fog_range.z` 1) fade out toward the horizon and take on the sky's
    // colour there, so the cloud domes have no visible rim (renderer choice). Their texture and vertex colour are
    // lit by `CLOUD_LIGHT`, dimmed at dusk as the sky is (`sky_light`; 1 without `--clock`), so that they and the water
    // and wet roads that reflect them do not stay bright over a darkened stage (ours). Under fog they fade into the sky by `frame.haze.y` (the game fogs them by
    // `SkyboxSaturation`, `Atmosphere::cloud_haze`).
    if (camera_anchored()) {
        let dir = normalize(v.world);
        // With the physically based sky the domes' brightest clouds stay below white (`SKY_CLOUD_LIGHT`), so that
        // the sky through the canopy is not a white hole.
        colour = colour * select(CLOUD_LIGHT, SKY_CLOUD_LIGHT, sky_on()) * day_level();
        alpha = alpha * smoothstep(0.03, 0.2, dir.y);
        colour = mix(colour, sky(dir, 0.0), 0.5 * (1.0 - smoothstep(0.0, 0.35, dir.y)));
        colour = mix(colour, sky(dir, 0.0), frame.haze.y);
    }
    return vec4<f32>(graded(fogged(colour, v.world)), alpha);
}

// A texel's backlight (ours): by night, and at dusk as much as the lamps light the stage (always with `mode.glow.z`),
// the texel glows in the light of its emissive mask (the RBRHUD gauges' `emissive_mask`, `rbr_render::dials`; the
// start board's digits and lamps, `rbr_render::start_board`): the mask's colour (the bulbs' pools on analog faces)
// plus its alpha in white (displays, warning lamps). In a mirror's pass (`frame.night.z`) the light is what a
// recessed face shows past its bezel at `facing`, the cosine to the eye (`DIAL_RECESS`): the dials are flat in the
// meshes, and the windscreen, seeing them from some 75 degrees off, mirrored every face whole at night (Audi quattro
// A2, Helette). The main view sees the dials about head on, and the lamps standing out of the dash (the A2's orange
// dome) glow on their sides.
fn backlit(colour: vec3<f32>, texel: vec3<f32>, uv: vec2<f32>, night: bool, facing: f32) -> vec3<f32> {
    let level = select(select(frame.dusk.w, 1.0, night), 1.0, mode.glow.z != 0.0);
    if (mode.glow.x == 0.0 || level <= 0.0) {
        return colour;
    }
    let mask = textureSampleLevel(glow_texture, texture_sampler, uv, 0.0);
    let backlight = mask.rgb + vec3<f32>(mask.a * mode.glow.y);
    let c = clamp(facing, 1e-3, 1.0);
    let shown = select(1.0, clamp(1.0 - DIAL_RECESS * sqrt(1.0 - c * c) / c, 0.0, 1.0), frame.night.z != 0.0);
    return max(colour, texel * backlight * DIAL_GLOW * level * shown);
}

// A dial's or lamp's depth in its housing over its radius (`backlit`): at 25 degrees off its axis 88 % of the face
// shows, at 75 degrees 7 %.
const DIAL_RECESS: f32 = 0.25;

// The interior glass's alpha over which a texel is printed on the glass (lit), not a tint: from x, wholly at y (ours);
// and its colour's chroma (max - min) over which it is printed whatever its alpha (the BMW M3 E30's yellow banner at
// alpha 0.47 over a grey glass at 0.22).
const INT_GLASS_PRINTED: vec2<f32> = vec2<f32>(0.5, 0.75);
const INT_GLASS_PRINTED_CHROMA: vec2<f32> = vec2<f32>(0.15, 0.3);

// The glass seen from outside (ours): the least share of the view it absorbs, and the share of the day's light its own
// colour keeps (dry, rain).
const GLASS_ABSORB: vec2<f32> = vec2<f32>(0.5, 0.55);
const GLASS_DARK: vec2<f32> = vec2<f32>(0.6, 0.35);
// The share more it mirrors with rain: the water film over the glass (the sheet's own Fresnel, a rougher face).
const GLASS_WET: f32 = 0.5;

// A lit draw's colour (`lit_class` 1 to 4: the car's paint, glass, lit lamps and cracks, the characters), with
// `under_cloud` of the sun taken by a cloud. Never water, a sky face nor camera-anchored, with no specular map, and no
// lamps' light on it (`lamp_sum` is the stage's).
fn shade_car(v: VertexOut, front: bool, under_cloud: f32) -> vec4<f32> {
    let first = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias);
    if (PERF_SHADE == 1.0) {
        var a = first.a;
        if (ALPHA_TEST && mode.alpha_test != 0u && a <= mode.alpha_ref) { discard; }
        return vec4<f32>(v.color.rgb * first.rgb, a);
    }
    // The textures' colour, then lit by the vertex light.
    let surface = first.rgb;
    var alpha = first.a;
    var second = vec4<f32>(0.0);
    if (second_draw()) {
        second = textureSampleBias(texture1, texture_sampler, v.uv1, mode.mip_bias);
    }
    let night = frame.night.x != 0.0;
    // The game's vertex light (the relight is the stage's, `relit`).
    var colour = v.color.rgb * surface;
    // The shadow light map multiplies the colour (`color = (base + spec) * shadow.rgb`, `notes/formats.md` "Shaders and vertex layouts"). With the
    // sun's shadows on (`enhance.x`) it is not used: their cascades and far layer cast for any sun, and the moon.
    let baked = select(textureSample(shadow_texture, texture_sampler, v.uv2).r, 1.0, frame.enhance.x != 0.0);
    colour = colour * baked;
    var n = normalize(v.normal) * select(-1.0, 1.0, front);
    // The car's shadow takes only the sun's term (and a crack's glint): faces turned from the sun skip it.
    var shadowed = 0.0;
    if (lit_class() == 4u || dot(n, -frame.sun.xyz) > 0.0) {
        shadowed = car_shadow(v.world);
    }
    let view = normalize(frame.eye.xyz - v.world);
    // A window's crack (ours): its normal from the crack texture's alpha, and the facets' glint in the sun.
    // The facets also mirror the surroundings, more where the crack is steep (`CRACK_MIRROR`).
    var glint = vec3<f32>(0.0);
    if (lit_class() == 4u) {
        let bumped = crack_normal(n, v.world, v.uv0);
        n = bumped.xyz;
        let sun = crack_glint(n, view) * (1.0 - shadowed) * frame.diffuse.rgb;
        glint = (sun + reflected(reflect(-view, n), 0.0) * CRACK_MIRROR) * bumped.w;
        alpha = max(alpha, clamp(max(glint.r, max(glint.g, glint.b)), 0.0, 1.0));
    }
    // In the in-car view the interior has no shadow of the car's own body and no light from the surroundings, as
    // in the game (`notes/formats.md` "Shaders and vertex layouts", car constants), and [`INTERIOR_AMBIENT`] times the ambient.
    let inside = in_car();
    let outside = surroundings(n, v.world);
    var around = select(outside, vec4<f32>(vec3<f32>(INTERIOR_AMBIENT), 1.0), inside);
    var sunlit = 1.0;
    if (!inside) {
        sunlit = (1.0 - shadowed) * stage_sun(v.world, n, outside.a) * (1.0 - CLOUD_CAR_SHADE * under_cloud);
    }
    // With the cabin light (`cabin.wgsl`, ours) the interior set takes the sky it sees and the sun through the glass,
    // its dial faces no darker than in the game, and the body's paint seen from inside (the bonnet) the outside
    // view's light. The rest (the glass the outside is seen through, the lamps, the cracks) is lit as in the game.
    let cabin_face = CABIN && inside && lit_class() == 1u && cabin_set(v);
    if (cabin_face) {
        let cabin = cabin_light(v, n, shadowed, under_cloud, outside);
        around = vec4<f32>(cabin.rgb, 1.0);
        sunlit = cabin.a;
        colour = v.color.rgb * cabin_texel(first.rgb);
    } else if (CABIN && inside && lit_class() == 1u && mode.fresnel.z != 0.0) {
        around = outside;
        sunlit = (1.0 - shadowed) * stage_sun(v.world, n, outside.a) * (1.0 - CLOUD_CAR_SHADE * under_cloud);
    }
    // The dirt (`CarPaint1.pso`): the diffuse lerped to the dirt colour by the dirt map's alpha times the level,
    // and the reflection reduced where it covers (ours: but for the share the dirt keeps, `dirt::Look`; with the
    // surfaces' dirt none on the cabin's interior set).
    var clean = 1.0;
    // A wheel's rubber and metal by the texel's luminance (`WHEEL_LUMA`); 0 for the rest of the car.
    var metal = 0.0;
    var dirt_share = 1.0;
    if (mode.grime.z != 0.0 && day_lit() && car_look()) {
        metal = smoothstep(WHEEL_LUMA.x, WHEEL_LUMA.y, dot(surface, vec3<f32>(0.2126, 0.7152, 0.0722)));
        colour = colour * mix(RUBBER_ALBEDO, 1.0, metal);
        // The dust sits low on the tyre: the rubber higher up keeps `RUBBER_DIRT_HIGH` of its dirt.
        let low = 1.0 - smoothstep(TYRE_BAND.x, TYRE_BAND.y, world_height(v.world));
        dirt_share = mix(mix(RUBBER_DIRT_HIGH, 1.0, low), 1.0, metal);
    }
    if (mode.fresnel.w != 0.0 && !(cabin_face && frame.wetness.w != 0.0)) {
        let mud = textureSampleBias(specular_texture, texture_sampler, v.uv0, mode.mip_bias);
        let mask = select(mud.a, sqrt(mud.a), frame.wetness.w != 0.0);
        let covered = clamp(mask * frame.wetness.y, 0.0, 1.0) * dirt_share;
        colour = mix(colour, frame.car_dirt.rgb, covered);
        clean = 1.0 - covered * (1.0 - frame.car_dirt.w);
    }
    if (mode.grime.z != 0.0 && frame.wetness.w != 0.0) {
        // A wheel's or mud flap's spatter by the dirt level (ours, `dirt::spatter`, with the surfaces' dirt), in spots
        // of a value noise over its texels (so of about one size on every car) over a film, on what its dirt map gave.
        let texels = v.uv0 * vec2<f32>(textureDimensions(texture0)) / SPATTER_TEXELS;
        let spots = 0.6 * value_noise(vec3<f32>(texels, 2.3)) + 0.4 * value_noise(vec3<f32>(texels * 4.0, 5.1));
        var covered = frame.wetness.y * (SPATTER_FILM + SPATTER_SPOTS * smoothstep(0.45, 0.8, spots));
        if (day_lit() && car_look()) {
            covered = covered * mix(RUBBER_SPATTER, 1.0, metal) * dirt_share;
        }
        colour = mix(colour, frame.car_dirt.rgb, covered);
        clean = clean * (1.0 - covered * (1.0 - frame.car_dirt.w));
    }
    if (mode.grime.x != 0.0 && frame.wetness.y > 0.0) {
        // A pane's mud film by the dirt level times its share (ours, `dirt::grime`), in patches of a value noise
        // on the pane's uv with clearer glass between them; the wet mud keeps `GRIME_SHEEN` of the reflection.
        let film = frame.wetness.y * mode.grime.x;
        let patches = 0.65 * value_noise(vec3<f32>(v.uv0 * GRIME_SCALE, 0.0))
            + 0.35 * value_noise(vec3<f32>(v.uv0 * GRIME_SCALE * 4.0, 1.7));
        let covered = GRIME_MOST * film * smoothstep(0.3, 0.7, patches);
        colour = mix(colour, frame.car_dirt.rgb, covered);
        alpha = mix(alpha, 1.0, covered);
        clean = 1.0 - covered * (1.0 - GRIME_SHEEN);
    }
    // Rain on the paint (ours, `wet_paint`): a film darkens and saturates the base, the beads and streaks break the
    // reflection's normal, and the lower body takes wet grime in the dirt's colour.
    var nr = n;
    var film = 0.0;
    if (day_lit() && lit_class() == 1u && !cabin_face && frame.wind.z > 0.0 && mode.fresnel.y * mode.fresnel.z > 0.0) {
        let wp = wet_paint(n, v.world);
        nr = wp.normal;
        film = WET_FILM;
        colour = wet_base(colour, v.world, wp.up) * (1.0 - WET_DROP * wp.drops);
    }
    // A lamp's lens (ours): a reflecting lit draw with no dirt map, not a wheel's (the lit lamps' parts glow, `lit_class` 3).
    let lens = lit_class() == 1u && mode.fresnel.w == 0.0 && mode.grime.z == 0.0 && mode.fresnel.y * mode.fresnel.z > 0.0
        && day_lit() && car_look() && !cabin_face;
    var shaded: vec3<f32>;
    var visible = 1.0;
    if (day_lit() && !cabin_face && car_look()) {
        visible = body_occlusion(n, v.world);
        if (SUN_MASK) {
            let contact = textureLoad(sun_contact, pixel / 2, 0).r;
            visible = visible * contact * contact;
        }
        body_visible = visible;
    }
    if (cabin_face && day_on()) {
        // The cabin debug view (`viewer --debug-view`, `cabin_debug` in `cabin.wgsl`): the term alone as a
        // display grey, passed through unexposed and untonemapped by `post.wgsl`, with no reflection, sheen,
        // glow or fog on it.
        if (DEBUG_VIEW != 0u) {
            return vec4<f32>(vec3<f32>(cabin_debug(v, n, shadowed, under_cloud, outside)), 1.0);
        }
        shaded = cabin_day_lit(colour, v, n, shadowed, under_cloud, outside);
    } else {
        shaded = lit(colour, n, view, sunlit, around.rgb, v.world);
    }
    // The Fresnel weight of the sky reflection: Schlick with F0 = 0.125 (`c91`), clamped to
    // [FresnelMin, FresnelMax] (`c92`), times Kf. Glass adds half of it to the alpha (`CarGlass`: `t0.a + 0.5 * F * Kf`);
    // other classes reflect where the diffuse alpha is (`CarPaint`: the reflection times `t0.a`).
    let nv = clamp(dot(n, view), 0.0, 1.0);
    var f = clamp(0.125 + 0.875 * pow(1.0 - nv, 5.0), mode.fresnel.x, mode.fresnel.y) * mode.fresnel.z;
    // With the day's light a dielectric's (Schlick, F0 0.04) where the class reflects at all.
    let physical = day_lit();
    if (physical) {
        f = select(0.0, 0.04 + 0.96 * pow(1.0 - nv, 5.0), mode.fresnel.y * mode.fresnel.z > 0.0);
    }
    if (lit_class() == 2u) {
        // With the day's light the glass mirrors what the paint does (`paint_mirrored`).
        let mirrored = select(reflected(reflect(-view, n), 0.0), paint_mirrored(reflect(-view, n), COAT_ROUGHNESS), physical);
        colour = shaded + mirrored * f * clean;
        alpha = clamp(alpha + 0.5 * f * clean, 0.0, 1.0);
        if (physical && !inside && mode.grime.y == 0.0 && mode.grime.w != 0.0) {
            // Seen from outside the pane mirrors the surroundings by their own weight F, not the blend's alpha times F
            // (a tenth of it), and absorbs at least `GLASS_ABSORB` (dry, rain) of the view through it.
            let rain = select(0.0, 1.0, frame.wind.z > 0.0);
            let reflect_share = min(f * clean * (1.0 + GLASS_WET * rain), 1.0);
            let body = max(max(first.a, mix(GLASS_ABSORB.x, GLASS_ABSORB.y, rain)), alpha - 0.5 * reflect_share);
            let seen = 1.0 - (1.0 - reflect_share) * (1.0 - body);
            colour = (mirrored * reflect_share + shaded * mix(GLASS_DARK.x, GLASS_DARK.y, rain) * body * (1.0 - reflect_share)) / max(seen, 1e-4);
            alpha = seen;
        }
        // The cabin's opaque glass is its mirrors (`cabin_mirror`).
        if (CABIN && inside && cabin_set(v)) {
            colour = cabin_mirror(first.rgb, reflect(-view, n));
        }
    } else {
        // Paint reflects less and a little blurred (`PAINT_GLOSS`).
        var shine = reflected(reflect(-view, n), PAINT_GLOSS.y) * f * PAINT_GLOSS.x * alpha * clean;
        // In the cabin only paint open to the sky (the bonnet seen through the glass, as `cabin_light` takes it) keeps
        // its reflection, of what it sees out of the windows (`cabin_open`); the rest reflects as the cabin's plastics
        // (`cabin_sheen`), whatever its class's Fresnel (the Clio Rally3's dash, `Kf` 1 from 0, read as glass).
        if (cabin_face) {
            let paint = select(0.0, smoothstep(CABIN_OUT.x, CABIN_OUT.y, 1.0 - v.specular.z), mode.fresnel.z != 0.0);
            let open = shine * cabin_open(cabin_seen(v), 1.0 - v.specular.z, reflect(-view, n));
            shine = mix(cabin_sheen(v, n, view), open, paint);
        }
        colour = shaded + shine;
        // With the day's light the clear coat (`COAT_ROUGHNESS`).
        if (physical) {
            let sky_dim = mix(1.0, WET_SKY, film);
            let roughness = COAT_ROUGHNESS * (1.0 - film * (1.0 - WET_ROUGHNESS));
            var coat = select(0.0, environment_reflectance(max(nv, 1e-3), roughness), f > 0.0) * alpha * clean;
            if (lit_class() == 1u && car_look()) {
                coat = min(coat * COAT_STRENGTH, 1.0);
            }
            var mirrored = paint_mirrored(reflect(-view, nr), roughness);
            if (lens) {
                // A lamp's lens over its reflector bowl: a sharp, Fresnel-weighted mirror over the texture, whose
                // bright texels (the bowl) are chrome, a rough mirror of the surroundings.
                let lum = dot(surface, vec3<f32>(0.2126, 0.7152, 0.0722));
                let bowl = smoothstep(LENS_LUMA.x, LENS_LUMA.y, lum);
                let glass = environment_reflectance(max(nv, 1e-3), LENS_ROUGHNESS) * LENS_COAT;
                let chrome = RIM_METAL * bowl;
                let shine = paint_mirrored(reflect(-view, nr), LENS_BOWL_ROUGHNESS);
                let sharp = paint_mirrored(reflect(-view, nr), LENS_ROUGHNESS);
                mirrored = mix(sharp, vec3<f32>(dot(shine, vec3<f32>(0.333))), chrome / max(chrome + glass, 1e-4));
                coat = min(max(coat, glass) + chrome * (1.0 - glass), 1.0);
            }
            if (metal > 0.0) {
                let rim = RIM_METAL * metal * clean;
                let shine = paint_mirrored(reflect(-view, nr), RIM_ROUGHNESS);
                mirrored = mix(mirrored, vec3<f32>(dot(shine, vec3<f32>(0.333))), rim / max(rim + coat, 1e-4));
                coat = max(coat, rim);
            }
            colour = pow(pow(shaded, vec3<f32>(2.2)) * (1.0 - coat) + pow(mirrored, vec3<f32>(2.2)) * coat * sky_dim * visible, vec3<f32>(1.0 / 2.2));
        }
    }
    if (CABIN && inside && v.specular.y >= CABIN_MIRROR_GLASS) {
        colour = cabin_mirror(first.rgb, reflect(-view, n));
    }
    // A mirror's glass with its image (`mirrors.wgsl`, ours), opaque.
    let mirror = mirror_of(v);
    if (mirror < MIRROR_COUNT) {
        colour = mirror_image(mirror, v.world);
        alpha = 1.0;
    }
    // With the day's light the interior glass's translucent texels only darken what is seen through it, by their
    // alpha and darkness: lit as in the game under the day's exposure, a light one (RSF 52's `i_window_f`) greyed the
    // view. Near-opaque texels (sun strips, visors printed on the glass) and coloured ones (banners) stay lit.
    if (inside && day_on() && mode.grime.y != 0.0 && mirror >= MIRROR_COUNT) {
        let chroma = max(first.r, max(first.g, first.b)) - min(first.r, min(first.g, first.b));
        let printed = max(smoothstep(INT_GLASS_PRINTED.x, INT_GLASS_PRINTED.y, alpha),
            smoothstep(INT_GLASS_PRINTED_CHROMA.x, INT_GLASS_PRINTED_CHROMA.y, chroma));
        let tint = alpha * (1.0 - dot(first.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)));
        let blended = mix(tint, alpha, printed);
        colour = colour * printed * alpha / max(blended, 1e-4);
        alpha = blended;
    }
    colour = colour + glint;
    // A lit lamp's part (`stage::Shade::glow`) glows (ours): by day `mode.lamp.w` of it, by night and at dusk, as the
    // lamps light the stage, all of it.
    if (lit_class() == 3u) {
        let level = mix(mode.lamp.w, 1.0, select(frame.dusk.w, 1.0, night));
        colour = max(colour, first.rgb * LAMP_GLOW * mode.lamp.rgb * level);
    }
    colour = backlit(colour, first.rgb, v.uv0, night, dot(n, view));
    // The scrape (`CarPaint`, `CarPaintInt`): the scrape texel times the diffuse light (`t3 * v0`, no highlight or
    // reflection) replaces the paint by `sat(v0.w * t3.a)`, `v0.w` being the vertex's damage weight / 100.
    if (second_draw()) {
        let scraped = LIGHT_SCALE * second.rgb * light(n, sunlit, around.rgb, v.world);
        colour = mix(colour, scraped, clamp(v.color.a * second.a, 0.0, 1.0));
    }
    alpha = plant_faded(v, alpha);
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    // With the cabin light the air inside the car is clear: the stage's fog, whose range can start behind the eye,
    // does not grey the interior set; the glass and what is seen through it keep theirs.
    if (CABIN && in_car() && lit_class() != 0u && cabin_set(v)) {
        return vec4<f32>(graded(colour), alpha);
    }
    return vec4<f32>(graded(fogged(colour, v.world)), alpha);
}

// The baked vertex light of an unlit world draw moved from the bake's sun to the drawn one (`relight.rs`, ours): the
// bake's `w * facing` comes out of the luminance and the drawn sun's goes in, keeping the hue. At least
// `RELIGHT_FLOOR` of the baked light stays (its ambient and occlusion). Foliage carries the ground's normal under it
// (`relight::Ground`), so it shifts with the ground.
const RELIGHT_FLOOR: f32 = 0.35;

// How much of the sky a stage vertex sees, 0 to 1 (ours): its baked light less the bake's sun (`relit`), over open
// ground's (`relight::Relight::open`, `frame.night.w`), so it holds for any sun. Without the relight, the vertex light
// over a typical open ground's.
fn sky_share(v: VertexOut) -> f32 {
    if (computed_occlusion()) {
        return 1.0 - v.sky.z;
    }
    let luminance = dot(v.color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (frame.relight.w == 0.0 || frame.night.w == 0.0) {
        return clamp(luminance / 0.5, 0.0, 1.0);
    }
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    let before = max(dot(n, frame.relight.xyz), 0.0);
    return clamp((luminance - frame.relight.w * before) / frame.night.w, 0.0, 1.0);
}

// Whether the stage's ambient occlusion is the computed one (`sky_occlusion.rs`): with the relight, which gives the
// open ground's light it scales (`frame.night.w`).
fn computed_occlusion() -> bool {
    return frame.enhance.x != 0.0 && frame.night.w > 0.0;
}

fn relit(v: VertexOut) -> vec3<f32> {
    let c = v.color.rgb;
    let computed = computed_occlusion();
    if ((frame.relight.w == 0.0 && !computed) || camera_anchored()) {
        return c;
    }
    // Draws without a normal (the interactive props: signs, boards) are taken as facing up.
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    let before = max(dot(n, frame.relight.xyz), 0.0);
    let after = max(dot(n, -frame.sun.xyz), 0.0);
    let luminance = max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-3);
    // The bake's ambient, or with the computed occlusion the open ground's times the sky the vertex sees, in the light
    // of the sky along the mean direction it sees it in under the physically based sky (`sky_ambient`, as the lit
    // draws take it); the bake keeps only its hue.
    var ambient = vec3<f32>(max(luminance - frame.relight.w * before, RELIGHT_FLOOR * luminance));
    if (computed) {
        ambient = vec3<f32>(frame.night.w * (1.0 - v.sky.z));
        if (sky_on()) {
            ambient = ambient * sky_ambient(sky_direction(v.sky.xy, n));
        }
    }
    let lit = ambient * frame.relight_light.w * frame.dusk.rgb + frame.relight.w * after * frame.relight_light.rgb;
    return c / luminance * lit;
}

// The stage's light with the computed occlusion (ours), linear (irradiance; the texture's linear colour times it is the
// linear colour): the day's light from the sky's tables (`frame.day_sky`, `frame.day_sun`). The sky's above the
// horizon, times the sky the vertex sees, as much as a face turned `n` gets of it, in the colour of the sky it sees
// (along its bent normal); the sun's by its facing and `sun_seen`, its share of the sun past the stage's, the car's
// and the clouds' shadows, so a shadow is the sky's light alone; the stage's light below the horizon (`bounce.rs`),
// its sunlit share dimmed under a cloud's shadow with the ground's. The bake gives only the hue, as a filter (over its
// brightest channel: a tint never brightens the texture, so the crowds' coloured clothes stay as dark as they are
// saturated), white where it is black.
// The bounce's strength (`bounce.rs`): 1 is a Lambertian ground's, its linear albedo times its light.
const BOUNCE: f32 = 1.0;

// The bake's hue of vertex colour `c`, as `own_light` takes it.
fn baked_hue(c: vec3<f32>) -> vec3<f32> {
    let most = max(c.r, max(c.g, c.b));
    return mix(vec3<f32>(1.0), c / max(most, 1e-3), smoothstep(0.02, 0.08, most));
}

fn own_light(v: VertexOut, sun_seen: f32, under_cloud: f32) -> vec3<f32> {
    var hue = baked_hue(v.color.rgb);
    if (GROUND) {
        hue = mix(hue, mix(vec3<f32>(1.0), hue, GROUND_HUE), select(0.0, 1.0, detail_draw()));
    }
    var n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    let tuft = is_tuft(v);
    let crown = is_crown(v) && !second_draw();
    var height = 1.0;
    if (tuft) {
        height = v.wet.x;
        n = tuft_normal(v, n);
    }
    if (crown) {
        height = clamp(0.5 + 0.5 * crown_at(v).y, 0.0, 1.0);
    }
    let ground_sky = open_sky_light(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0));
    let sky_unit = frame.day_sky.rgb;
    let sun_unit = frame.day_sun.rgb;
    var sky = sky_unit * (1.0 - v.sky.z) * open_sky_light(n, sky_direction(v.sky.xy, n)) / max(ground_sky, vec3<f32>(1e-4));
    // A fence span lets `through` of the light on its other side through (`fences::THROUGH`): the sun's on that side,
    // and its sky and bounce taken as this side's (an upright film sees much the same either way).
    let through = select(0.0, v.specular.z, is_fence(v));
    let facing = dot(n, -frame.sun.xyz);
    var sun = sun_unit * (max(facing, 0.0) + through * max(-facing, 0.0)) * sun_seen;
    var bounce = BOUNCE * (sky_unit * v.bounce_sky + sun_unit * v.bounce_sun * (1.0 - under_cloud));
    var occlusion = 1.0;
    var ambient_occlusion = 1.0;
    if (tuft || crown) {
        // Blades lit from both sides: a wrapped facing (a crown's tighter, to keep its lit and shaded halves) and the
        // light through them behind.
        // Tufts, our grass cards (`grass.rs`) and the stage's alike, are lit as a dense field: a tight wrap, so each
        // clump has a lit and a shaded side, the sun and the sky shut out deeper down it, the sun the more the lower it
        // is (`GRASS_SUN_BASE` from a low sun to a high one), no straw in their tips, and the light through the blades
        // tinted by their vertex colour. Lit as lone tufts (a wide wrap, warm tips), 580's ferns were 2.4 to 3.7 times
        // as bright as our grass beside them.
        let wrap = select(GRASS_WRAP, CROWN_WRAP, crown);
        let wrapped = max(facing + wrap, 0.0) / (1.0 + wrap);
        let behind = max(-facing, 0.0);
        let base = mix(GRASS_SUN_BASE.x, GRASS_SUN_BASE.y, smoothstep(0.1, 0.8, -frame.sun.y));
        var shade = mix(base, 1.0, smoothstep(0.0, GRASS_RISE, height));
        let leaf = pow(hue, vec3<f32>(2.2));
        var through_tint = leaf / max(max(leaf.r, max(leaf.g, leaf.b)), 1e-3);
        var tip = mix(vec3<f32>(1.0), GRASS_TIP, smoothstep(0.5, 1.0, height));
        if (crown) {
            // Only the crown's inside is shaded from the sun, not its lit surface; the light through its leaves is
            // straw-tinted.
            shade = mix(CROWN_INNER, 1.0, smoothstep(0.2, 0.9, length(crown_at(v))));
            tip = vec3<f32>(1.0);
            through_tint = TUFT_STRAW;
        }
        sun = sun_unit * (wrapped + TUFT_TRANSMIT * behind * through_tint) * sun_seen * shade * tip;
        // The sky's light is near neutral on the stage's plants (their grey-green textures read bluish under a blue
        // sky; our grass has the ground's hue), and the sunlit ground around a plant sends its warm light up from below.
        let ours = tuft && v.specular.x > GRASS_MARK;
        let grey = dot(sky, vec3<f32>(0.2126, 0.7152, 0.0722));
        sky = mix(vec3<f32>(grey), sky, select(TUFT_SKY_TINT, 1.0, ours));
        bounce = bounce + TUFT_GROUND * (sky_unit * 0.5 + sun_unit * max(-frame.sun.y, 0.0) * sun_seen) * (1.0 - height);
        occlusion = mix(GRASS_BASE, 1.0, smoothstep(0.0, GRASS_RISE, height));
        if (crown) {
            // The sky's and the ground's light, not the sun's, is shut out of a crown's lower and inner parts.
            ambient_occlusion = mix(CROWN_BASE, 1.0, smoothstep(0.0, 0.6, height)) * mix(CROWN_INNER, 1.0, smoothstep(0.2, 1.0, length(crown_at(v))));
            occlusion = 1.0;
        }
    }
    // A fence's from the ground's maps where it looks, as the car's (`light_sum`): its cloth moves, and the bake's
    // stage meshes (`bounce.rs`) give its thin faces little.
    if (is_fence(v) && frame.ground_grid.z != 0.0) {
        let g = ground_bounce(v.world, n);
        bounce = BOUNCE * (sky_unit * g.sky + sun_unit * g.sun * (1.0 - under_cloud));
    }
    // The contact shading on the sky's and the bounce's light, never the sun's (`contact_light`).
    var contact = 1.0;
    if (SUN_MASK) {
        contact = textureLoad(sun_contact, pixel / 2, 0).r;
    }
    return pow(hue, vec3<f32>(2.2)) * (((sky + bounce) * ambient_occlusion * contact * (1.0 + through) + sun) * occlusion);
}

// A grass tuft's or low bush's pixel (`foliage::with_plants`: `specular.y` 1, `wet` its share of height, `uv1` the way
// out across), lit as vegetation rather than as a flat card (ours), as our grass is (`GRASS_*`). The straw tint on its
// pale texels (the retail textures are a grey-green with no straw in them; not the snow's), its green leaves no brighter
// than `TUFT_GREEN_MOST`, a normal that turns out of the tuft by `TUFT_BEND` at the tip, its transmission behind
// `TUFT_TRANSMIT`, the sky's colour kept `TUFT_SKY_TINT` (a crown's too; our grass keeps all of it), and the ground's
// `TUFT_GROUND` returned from below.
const TUFT_STRAW: vec3<f32> = vec3<f32>(1.07, 1.03, 0.9);
// `specular.x` of our grass cards past this is their brightness (`grass::GRASS_MARK`); a tuft's sunlit tips' lift,
// wrap, occlusion and sun at the foot, and the share of height both reach 1 at.
const GRASS_MARK: f32 = 4.0;
const GRASS_TIP: vec3<f32> = vec3<f32>(1.12);
const GRASS_WRAP: f32 = 0.3;
const GRASS_BASE: f32 = 0.6;
const GRASS_SUN_BASE: vec2<f32> = vec2<f32>(0.4, 0.6);
const GRASS_RISE: f32 = 0.9;
const TUFT_PALE: vec2<f32> = vec2<f32>(0.06, 0.16);
const TUFT_SNOW: vec2<f32> = vec2<f32>(0.3, 0.5);
const TUFT_GREEN_MOST: f32 = 0.09;
const TUFT_BEND: f32 = 0.8;
const TUFT_TRANSMIT: f32 = 0.35;
const TUFT_SKY_TINT: f32 = 0.4;
const CROWN_LIFT: f32 = 1.5;
const CROWN_SHRUB: vec2<f32> = vec2<f32>(1.0, 3.0);
const CROWN_BASE: f32 = 0.75;
const CROWN_WRAP: f32 = 0.25;
const CROWN_INNER: f32 = 0.7;
const TUFT_GROUND: vec3<f32> = vec3<f32>(0.3, 0.24, 0.14);

fn is_tuft(v: VertexOut) -> bool {
    return mode.cutout != 0u && !second_draw() && v.specular.y > 0.5 && v.specular.y < 1.5 && v.specular.z > 0.5
        && !is_crown(v);
}

fn tuft_normal(v: VertexOut, up: vec3<f32>) -> vec3<f32> {
    let out = vec3<f32>(v.uv1.x, 0.5, v.uv1.y);
    return normalize(mix(up, normalize(out), TUFT_BEND * smoothstep(0.0, 0.8, v.wet.x)));
}

// Whether the frame has the day's light from the sky's tables (`own_light`).
fn day_on() -> bool {
    return frame.day_sky.w > 0.0;
}

// The day's level for the clouds, in display level: with the day's light, its mean on open faces (as `day_light` in
// `skylight.wgsl` takes it) over noon's (`DAY_MEAN`, 1 until the pre-exposure stops at the night's in twilight;
// `skylight::DayLight::level`), else the clock's ambient (`relight::Daylight`).
const DAY_MEAN: f32 = 1.158;

fn day_level() -> vec3<f32> {
    if (day_on()) {
        let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
        let up = abs(frame.sun.y);
        let mean = dot(frame.day_sky.rgb, luma) + dot(frame.day_sun.rgb, luma) * (1.0 + up) * 0.25;
        return vec3<f32>(pow(mean / DAY_MEAN, 1.0 / 2.2));
    }
    return frame.relight_light.w * frame.dusk.rgb;
}

// The share of the sun `v` gets past the stage (the cascades where they reach, past them the world layer, when the sky
// is not overcast), the car (its shadow map) and the clouds (`under_cloud`).
fn own_sun_seen(v: VertexOut, under_cloud: f32) -> f32 {
    var seen = 1.0 - under_cloud;
    if (frame.shadow.x != 0.0) {
        seen *= 1.0 - car_shadow(v.world);
    }
    let near = sun_reach(v.world);
    if (seen > 0.0 && frame.sun_shadow.z == 0.0) {
        let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
        if (near > 0.0) {
            seen *= mix(world_lit(v.world, n), sun_lit(v.world, n), near);
        } else {
            seen *= world_lit(v.world, n);
        }
    }
    return seen;
}

// Whether `v` is on a crown's card (`foliage::TREE_CARD`, `specular.x` 0.3).
fn is_crown(v: VertexOut) -> bool {
    return mode.cutout != 0u && v.specular.x > 0.15 && v.specular.x < 0.45;
}

// Where a crown's pixel is in its ellipsoid (`foliage::with_plants`): centre 0, surface 1. Not on draws with a second
// texture, whose `uv1` it would take.
fn crown_at(v: VertexOut) -> vec3<f32> {
    return vec3<f32>(v.uv1.x, v.wet.x, v.uv1.y);
}

// The crown's half extents at the pixel, metres: the world position's slopes over the crown position's, which are
// affine in each other; a card along an axis gives the other's from its own.
fn crown_radii(v: VertexOut) -> vec3<f32> {
    let q1 = dpdx(crown_at(v));
    let q2 = dpdy(crown_at(v));
    let p1 = dpdx(v.world);
    let p2 = dpdy(v.world);
    let first = abs(q1) > abs(q2);
    let q = select(q2, q1, first);
    let p = select(p2, p1, first);
    var r = abs(p / select(q, vec3<f32>(1.0), abs(q) < vec3<f32>(1e-6)));
    let has = abs(q) >= vec3<f32>(1e-6);
    r.x = select(r.z, r.x, has.x);
    r.z = select(r.x, r.z, has.z);
    return max(r, vec3<f32>(0.1));
}

// The share of the sun a crown's pixel gets (ours), in place of `own_sun_seen`: its own leaves by how deep it lies
// toward the sun, Beer-Lambert over the way out of its ellipsoid (`CROWN_DENSITY` per mean radius, at least
// `CROWN_SELF_LEAST`), instead of its cards' hard shadows on each other; the rest of the stage from where the sun's ray
// leaves the crown, so that other trees and the ground still cast.
const CROWN_DENSITY: f32 = 1.0;
const CROWN_SELF_LEAST: f32 = 0.15;

fn crown_sun_seen(v: VertexOut, under_cloud: f32, r: vec3<f32>) -> f32 {
    var seen = 1.0 - under_cloud;
    if (frame.shadow.x != 0.0) {
        seen *= 1.0 - car_shadow(v.world);
    }
    if (frame.sun_shadow.z != 0.0) {
        return seen;
    }
    let s = -frame.sun.xyz;
    let q = crown_at(v);
    let sq = s / r;
    let a = dot(sq, sq);
    let b = dot(q, sq);
    let c = dot(q, q) - 1.0;
    let disc = b * b - a * c;
    let t = select(0.0, max((-b + sqrt(max(disc, 0.0))) / a, 0.0), disc > 0.0);
    let own = mix(CROWN_SELF_LEAST, 1.0, exp(-CROWN_DENSITY * t * 3.0 / (r.x + r.y + r.z)));
    let out = v.world + s * (t + 0.5);
    let near = sun_reach(out);
    var lit = 1.0;
    if (near > 0.0) {
        let p = sun_point(out, vec3<f32>(0.0));
        if (p.layer >= 0) {
            lit = texel_filter(p.uv, p.layer, p.depth);
        }
    }
    return seen * own * mix(world_lit(out, vec3<f32>(0.0)), lit, near);
}

// A plant card's alpha faded as it turns edge-on to the eye (ours): seen at a grazing angle a card is a stretched
// stripe, the giveaway of crossed cards. Below `EDGE_ON.x` of the facing it is gone, from `EDGE_ON.y` whole; crossed
// cards are never both under 0.7, so one of them always shows. The face's own normal, from the position's slopes.
// A crown's cards are faded whole (`foliage::TREE_CARD`), their strips that are no plant's too.
const EDGE_ON: vec2<f32> = vec2<f32>(0.2, 0.5);

fn plant_faded(v: VertexOut, alpha: f32) -> f32 {
    let face = cross(dpdx(v.world), dpdy(v.world));
    if (mode.cutout == 0u || (v.specular.z <= 0.5 && v.specular.x <= 0.15)) {
        return alpha;
    }
    let to_eye = frame.eye.xyz - v.world;
    let facing = abs(dot(normalize(face), normalize(to_eye)));
    return alpha * smoothstep(EDGE_ON.x, EDGE_ON.y, facing);
}

// The depth prepass of the alpha-tested draws (`renderer::DrawPass`): `fs_main`'s alpha test on the texture's
// alpha, so that the pixels it keeps are the ones `fs_main` would. The colour, the normal
// `own_sun_seen` takes, is written only for the sun's mask (`sun_mask.wgsl`).
@fragment
fn fs_prepass(v: VertexOut) -> @location(0) vec4<f32> {
    let alpha = plant_faded(v, textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias).a);
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    return vec4<f32>(n * 0.5 + 0.5, 1.0);
}

// The crack's normal (ours, `crack_decal.rs`): the alpha is a height, its slope across a texel tilts `n` in the
// cotangent frame of the face's uv; `w` is how steep it is (0 on clear glass). `CRACK_BUMP` scales the slope.
const CRACK_BUMP: f32 = 1.5;
const CRACK_MIRROR: f32 = 0.6;

fn crack_normal(n: vec3<f32>, p: vec3<f32>, uv: vec2<f32>) -> vec4<f32> {
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let texel = max(1.0 / vec2<f32>(textureDimensions(texture0)), max(abs(duv1), abs(duv2)));
    let h = vec4<f32>(
        textureSampleBias(texture0, texture_sampler, uv + vec2<f32>(texel.x, 0.0), mode.mip_bias).a,
        textureSampleBias(texture0, texture_sampler, uv - vec2<f32>(texel.x, 0.0), mode.mip_bias).a,
        textureSampleBias(texture0, texture_sampler, uv + vec2<f32>(0.0, texel.y), mode.mip_bias).a,
        textureSampleBias(texture0, texture_sampler, uv - vec2<f32>(0.0, texel.y), mode.mip_bias).a,
    );
    let slope = vec2<f32>(h.x - h.y, h.z - h.w) * CRACK_BUMP;
    let side2 = cross(dp2, n);
    let side1 = cross(n, dp1);
    var t = side2 * duv1.x + side1 * duv2.x;
    var b = side2 * duv1.y + side1 * duv2.y;
    let scale = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-20));
    t = t * scale;
    b = b * scale;
    return vec4<f32>(normalize(n - slope.x * t - slope.y * b), clamp(length(slope), 0.0, 1.0));
}

// The sun's glint on the crack's facets (ours): a tight highlight of the bumped normal.
fn crack_glint(n: vec3<f32>, view: vec3<f32>) -> f32 {
    let to_sun = -frame.sun.xyz;
    let h = normalize(to_sun + view);
    return 2.0 * pow(max(dot(n, h), 0.0), 90.0) * step(0.0, dot(n, to_sun));
}

// The panes' mud film (ours, `dirt::grime`): its patches per unit of uv, the most it covers, and the share of the
// reflection the wet mud keeps.
const GRIME_SCALE: f32 = 12.0;
const GRIME_MOST: f32 = 0.55;
const GRIME_SHEEN: f32 = 0.4;
// The wheels' and mud flaps' spatter (ours, `dirt::spatter`): its spots' size in texels, and how much its film and
// its spots cover at the full level.
const SPATTER_TEXELS: f32 = 8.0;
const SPATTER_FILM: f32 = 0.3;
const SPATTER_SPOTS: f32 = 0.35;
// A wheel's texels by their luminance (ours, with the day's light): below the first the tyre's rubber, whose albedo is
// `RUBBER_ALBEDO` of the texture's and whose spatter `RUBBER_SPATTER` of the rim's; above the second the rim's metal,
// which mirrors the surroundings by `RIM_METAL` (a bright metal's reflectance, rough by `RIM_ROUGHNESS`) in the
// cube's grey, between them the blend.
const WHEEL_LUMA: vec2<f32> = vec2<f32>(0.3, 0.55);
const RUBBER_ALBEDO: f32 = 0.6;
const RUBBER_SPATTER: f32 = 0.5;
const RIM_METAL: f32 = 0.55;
// The tyre's dust band (ours): heights over the ground, metres, where the rubber's dirt falls from all to
// `RUBBER_DIRT_HIGH` of it.
const TYRE_BAND: vec2<f32> = vec2<f32>(0.08, 0.3);
const RUBBER_DIRT_HIGH: f32 = 0.15;
// A lamp's lens (ours, `lens`): its glass's roughness and how much more than a dielectric's reflection it takes, the
// luminance of the texels that are the reflector's chrome, and that chrome's roughness.
const LENS_ROUGHNESS: f32 = 0.02;
const LENS_COAT: f32 = 3.0;
const LENS_LUMA: vec2<f32> = vec2<f32>(0.3, 0.6);
const LENS_BOWL_ROUGHNESS: f32 = 0.15;
const RIM_ROUGHNESS: f32 = 0.3;

// The clouds' light (ours): the vertex colours of the cloud faces peak at 128 (0.5) and darken toward the
// undersides; the game's extinction and in-scatter terms that scale and add to them (`CloudsSingleTexture.vso`,
// `c55`-`c66`) are not reproduced. Doubled, a face's lit side is the texture's white.
const CLOUD_LIGHT: f32 = 2.0;

// The car paint's clear coat with the day's light (ours): a smooth dielectric (F0 0.04) of `COAT_ROUGHNESS` (GGX's),
// reflecting `paint_mirrored` by Karis's environment BRDF over the paint, which gets what it does not reflect. Its
// sun highlight is a little wider (`COAT_HIGHLIGHT`, Beckmann's m) than the coat, so that it does not flicker on
// curved panels a few pixels across, and it replaces the class's `CookTorranceM` (`sun_highlight`).
const COAT_ROUGHNESS: f32 = 0.07;
const COAT_HIGHLIGHT: f32 = 0.04;
// The coat's wider sheen (the share of a second highlight and its Beckmann m, a lacquer's orange-peel waviness), and the
// paint's base pigment (the saturation and the albedo's scale), at paint only.
// The paint's reflection scale over a dielectric's: the lacquer over a metallic base mirrors more than bare F0 0.04.
const COAT_STRENGTH: f32 = 1.5;
const COAT_SHEEN: vec2<f32> = vec2<f32>(0.6, 0.3);
const PAINT_PIGMENT: vec2<f32> = vec2<f32>(1.25, 1.0);

// Rain on the car's paint (ours, `frame.wind.z`, the wet weather): a water film, `WET_FILM` of it on the whole body, with
// the clear coat at `WET_ROUGHNESS` of its roughness, the base `WET_DARK` darker and `WET_SAT` more saturated, and the
// sky it mirrors at `WET_SKY` of its light (the overcast sky it sees is a dull grey, not the bright fog of the horizon;
// the Fresnel weight still makes it strongest at a grazing view). Beads on the up-facing panels (cells of `WET_BEADS` metres) and streaks down the standing
// ones (`WET_STREAK` across a metre, `WET_RUN` a metre long) tilt the reflection's normal by `WET_TILT`; they are placed
// in the car's model space, so they stay on the body, and fade out with the distance (`WET_FAR`). The lower body (below
// `WET_LOW` metres in model space) takes `WET_GRIME` of the dirt's colour (`dirt::Look`), and is in the shade of the
// body and the ground: `WET_UNDER` of the light at the sills, rising to all of it at `WET_UNDER_TOP` metres.
const WET_FILM: f32 = 1.0;
const WET_ROUGHNESS: f32 = 0.5;
const WET_SKY: f32 = 0.35;
const WET_UNDER: f32 = 0.7;
const WET_UNDER_TOP: f32 = 0.8;
const WET_DARK: f32 = 0.15;
const WET_SAT: f32 = 0.4;
const WET_BEADS: vec2<f32> = vec2<f32>(0.034, 0.019);
const WET_STREAK: f32 = 55.0;
const WET_RUN: f32 = 0.35;
const WET_TILT: vec2<f32> = vec2<f32>(0.35, 0.3);
const WET_FAR: vec2<f32> = vec2<f32>(6.0, 18.0);
const WET_LOW: vec2<f32> = vec2<f32>(0.1, 0.55);
const WET_GRIME: f32 = 0.35;
const WET_DROP: f32 = 0.45;

// `world` in the car's model space.
fn car_local(world: vec3<f32>) -> vec3<f32> {
    return (world - frame.car_body[3].xyz) * mat3x3<f32>(frame.car_body[0].xyz, frame.car_body[1].xyz, frame.car_body[2].xyz);
}

// The beads of one layer at `p` (model space, a plane): the tilt of a dome in the cell's jittered spot, none where
// the cell holds no drop.
fn wet_bead(p: vec2<f32>, cell: f32, seed: f32) -> vec2<f32> {
    let q = p / cell;
    let c = floor(q);
    let h = vec3<f32>(hash(vec3<f32>(c, seed)), hash(vec3<f32>(c, seed + 1.7)), hash(vec3<f32>(c, seed + 3.1)));
    let d = (fract(q) - (0.3 + 0.4 * h.xy)) / (0.14 + 0.2 * h.z);
    return d * (1.0 - smoothstep(0.6, 1.0, length(d))) * select(0.0, 1.0, hash(vec3<f32>(c, seed + 5.9)) > 0.3);
}

// The paint's normal `n` (stage space) with the beads and streaks at `world`, the up-facing share (the grime keeps
// off the roof) and how much the drops tilt it (they darken the base by `WET_DROP`).
struct WetPaint {
    normal: vec3<f32>,
    up: f32,
    drops: f32,
}

fn wet_paint(n: vec3<f32>, world: vec3<f32>) -> WetPaint {
    let turn = mat3x3<f32>(frame.car_body[0].xyz, frame.car_body[1].xyz, frame.car_body[2].xyz);
    let p = car_local(world);
    let nl = n * turn;
    let up = smoothstep(0.45, 0.85, n.y);
    let near = 1.0 - smoothstep(WET_FAR.x, WET_FAR.y, distance(world, frame.eye.xyz));
    var tilt = vec3<f32>(0.0);
    if (near > 0.0) {
        let beads = wet_bead(p.xz, WET_BEADS.x, 3.0) + 0.7 * wet_bead(p.xz + 0.37, WET_BEADS.y, 11.0);
        tilt = vec3<f32>(beads.x, 0.0, beads.y) * WET_TILT.x * up;
        // Streaks run down the standing panels along the panel's horizontal, in patches.
        let side = smoothstep(0.7, 0.3, abs(nl.y)) * smoothstep(-0.2, 0.0, nl.y + 0.2);
        let across = normalize(vec2<f32>(-nl.z, nl.x) + vec2<f32>(1e-4));
        let u = dot(p.xz, across);
        let y = p.y + 0.4 * u;
        let a = value_noise(vec3<f32>(u * WET_STREAK, y / WET_RUN, 4.0));
        let b = value_noise(vec3<f32>(u * WET_STREAK + 0.5, y / WET_RUN, 4.0));
        let patches = smoothstep(0.35, 0.65, value_noise(vec3<f32>(u * 5.0, y * 0.8, 9.0)));
        tilt = tilt + vec3<f32>(across.x, 0.0, across.y) * (b - a) * 2.0 * WET_TILT.y * side * patches;
    }
    return WetPaint(normalize(n + turn * tilt * near), up, min(length(tilt) * near, 1.0));
}

// The paint's base `colour` with the film (darker and more saturated) and the wet grime of the lower body at
// `world`, off the up-facing panels (`up`).
fn wet_base(colour: vec3<f32>, world: vec3<f32>, up: f32) -> vec3<f32> {
    let grey = dot(colour, vec3<f32>(0.2126, 0.7152, 0.0722));
    var wet = max(mix(vec3<f32>(grey), colour, 1.0 + WET_SAT), vec3<f32>(0.0)) * (1.0 - WET_DARK);
    let p = car_local(world);
    let patches = 0.5 + 0.5 * value_noise(p * 6.0);
    wet = wet * mix(WET_UNDER, 1.0, smoothstep(WET_LOW.x, WET_UNDER_TOP, p.y));
    wet = mix(wet, frame.car_dirt.rgb, WET_GRIME * (1.0 - smoothstep(WET_LOW.x, WET_LOW.y, p.y)) * (1.0 - up) * patches);
    return wet;
}

// The car body's own occlusion of the ambient light and the reflections (ours; the car's vertices are posed on the CPU
// each frame, so no computed one): the faces that look down or aside near the ground, from the sills to the arches
// and the underbody, see the road and the body, not the sky. Faces turned up see all of it. In the car's model space:
// `CAR_AO_SIDE` the share a face turned aside takes off at the ground, `CAR_AO_DOWN` the share one turned down, `CAR_AO_LOW`
// the heights (metres) it fades out between.
const CAR_AO_SIDE: f32 = 0.12;
const CAR_AO_DOWN: f32 = 0.85;
const CAR_AO_LOW: vec2<f32> = vec2<f32>(0.08, 0.45);

// Whether the car takes its look (the body's occlusion, the coat's strength and sheen, the pigment, the rims): in the
// views of the car's outside, not in a cockpit's mirrors, which keep the game's car (`frame.night.z`).
fn car_look() -> bool {
    return frame.night.z == 0.0;
}

// `world`'s height over the ground under the car (`frame.occlusion.z`).
fn world_height(world: vec3<f32>) -> f32 {
    return world.y - frame.occlusion.z;
}

fn body_occlusion(n: vec3<f32>, world: vec3<f32>) -> f32 {
    let turn = mat3x3<f32>(frame.car_body[0].xyz, frame.car_body[1].xyz, frame.car_body[2].xyz);
    let p = car_local(world);
    let nl = n * turn;
    let low = 1.0 - smoothstep(CAR_AO_LOW.x, CAR_AO_LOW.y, p.y);
    let aside = 1.0 - smoothstep(0.2, 0.8, nl.y);
    let down = smoothstep(0.1, -0.5, nl.y) * (1.0 - smoothstep(0.5, 1.1, p.y));
    return (1.0 - CAR_AO_SIDE * low * aside) * (1.0 - CAR_AO_DOWN * down);
}

// What the car's paint mirrors: the surroundings cube drawn at the car (`renderer::Reflection`), at the coat's
// roughness by the sky's rule (`sky_reflected`: the level over 7), so the coat mirrors the stage around it; with no
// cube drawn, `reflected`.
fn paint_mirrored(dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    let level = roughness * 7.0;
    if (car_look() && lit_class() == 1u) {
        return mix(paint_surroundings(dir, level), horizon_mirrored(dir), HORIZON_SHARE);
    }
    return paint_surroundings(dir, level);
}

// The far field a glossy panel mirrors (ours): the stage's blurred upper band above the horizon and its ground below it, split
// at the reflected direction's horizon over `HORIZON_SOFT` of its height, so that a panel shows the horizon's line bent
// by its curvature, not the stage's blurred skyline (the hills and trees stand in the probes' cube). `HORIZON_SHARE`
// of the coat's mirror is this.
const HORIZON_SHARE: f32 = 0.5;
// The blur (cube level) of the far field's sky and ground: the stage the probe sees there, a forest's trees or the open sky.
const HORIZON_BLUR: f32 = 4.0;
const HORIZON_SOFT: vec2<f32> = vec2<f32>(-0.3, 0.0);

fn horizon_mirrored(dir: vec3<f32>) -> vec3<f32> {
    let up = normalize(vec3<f32>(dir.x, max(dir.y, 0.03), dir.z));
    let down = normalize(vec3<f32>(dir.x, -0.7, dir.z));
    return mix(paint_surroundings(down, 5.0), paint_surroundings(up, HORIZON_BLUR), smoothstep(HORIZON_SOFT.x, HORIZON_SOFT.y, dir.y));
}

fn paint_surroundings(dir: vec3<f32>, level: f32) -> vec3<f32> {
    if (frame.reflection.w == 0.0) {
        return reflected(dir, level);
    }
    // At `Detail::Ultra` the cube is finer: the coat's level is one of its own, a mip level sharper.
    return ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, dir, level).rgb);
}

// Whether the draw is the car's paint or glass with a clear coat (a class that reflects, `shade`).
fn coated() -> bool {
    return (lit_class() == 1u || lit_class() == 2u) && mode.fresnel.y * mode.fresnel.z > 0.0;
}

// The renderer's car lighting (the game's needs constants with unproven sources, `notes/formats.md` "Shaders and vertex layouts"): ambient Ka, with a
// little more from above than below, plus Kd by the sun's angle, and a Cook-Torrance-shaped highlight
// (`CookTorranceKs`, `CookTorranceM`) in the sun's colour. `LIGHT_SCALE` maps the settings' 0.5 colours and 1.7 to 1.9
// scales to about 1 in full sun.
const LIGHT_SCALE: f32 = 0.5;
// The car paint's reflection (renderer addition): its share of the game's
// Fresnel weight (whose FresnelMin of about 0.3 makes the paint a mirror head on), and the reflection cube's mip level,
// so it reads as a clear coat rather than chrome.
const PAINT_GLOSS: vec2<f32> = vec2<f32>(0.75, 1.0);

fn lit(colour: vec3<f32>, n: vec3<f32>, view: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let to_sun = -frame.sun.xyz;
    let ndl = max(dot(n, to_sun), 0.0) * sunlit;
    if (day_lit()) {
        // Lambert on the texture's albedo plus the sun's Cook-Torrance highlight, in linear light.
        let e = light_sum(n, sunlit, ambient, world, vec2<f32>(1.0));
        var albedo = min(pow(colour * CAR_ALBEDO, vec3<f32>(2.2)), vec3<f32>(ALBEDO_MOST));
        var highlight = sun_highlight(n, view, to_sun);
        if (lit_class() == 1u && coated() && car_look()) {
            // The clear coat's wider sheen (`COAT_SHEEN`) over the sharp highlight, and the base's pigment deeper.
            highlight += COAT_SHEEN.x * sun_lobe(n, view, to_sun, COAT_SHEEN.y);
            let grey = dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722));
            albedo = max(mix(vec3<f32>(grey), albedo, PAINT_PIGMENT.x), vec3<f32>(0.0)) * PAINT_PIGMENT.y;
        }
        return pow(albedo * e + highlight * frame.day_sun.rgb * ndl, vec3<f32>(1.0 / 2.2));
    }
    let h = normalize(to_sun + view);
    let nh = max(dot(n, h), 1e-3);
    let m2 = max(mode.light.w * mode.light.w, 1e-3);
    let highlight = mode.light.z * exp((1.0 - 1.0 / (nh * nh)) / m2) * step(0.0, ndl) * min(ndl * 4.0, 1.0);
    return LIGHT_SCALE * (colour * light(n, sunlit, ambient, world) + 4.0 * highlight * frame.diffuse.rgb);
}

// The texture's colour's scale to the albedo the day's light takes (ours).
const CAR_ALBEDO: f32 = 1.0;
// The highest albedo (fresh snow's; white paint is about 0.8).
const ALBEDO_MOST: f32 = 0.9;
// The highlight's roughness (Beckmann's m) where the game's class has none (`CookTorranceKs` 0), and its least.
const MATTE: f32 = 0.6;
const LEAST_ROUGHNESS: f32 = 0.1;

// Whether the lit draws take the day's light (`own_light`'s): with it, outside the car (the in-car view's interior keeps
// the game's light, or the cabin's own).
fn day_lit() -> bool {
    return day_on() && !in_car();
}

// The sun's specular reflection off a dielectric (F0 0.04) of the class's roughness (`CookTorranceM`, the clear coat's
// `COAT_HIGHLIGHT` where it has one), Beckmann's distribution with the V-cavity shadowing and Schlick's Fresnel, times
// pi (the day's sun is its illuminance over pi).
fn sun_highlight(n: vec3<f32>, view: vec3<f32>, to_sun: vec3<f32>) -> f32 {
    var m = select(MATTE, max(mode.light.w, LEAST_ROUGHNESS), mode.light.z > 0.0);
    if (coated()) {
        m = COAT_HIGHLIGHT;
    }
    return sun_lobe(n, view, to_sun, m);
}

// One of the sun's highlights, of Beckmann roughness `m`.
fn sun_lobe(n: vec3<f32>, view: vec3<f32>, to_sun: vec3<f32>, m: f32) -> f32 {
    let h = normalize(to_sun + view);
    let nh = max(dot(n, h), 1e-3);
    let nv = max(dot(n, view), 1e-3);
    let nl = max(dot(n, to_sun), 1e-3);
    let vh = max(dot(view, h), 1e-3);
    let m2 = m * m;
    let nh2 = nh * nh;
    let d = exp((nh2 - 1.0) / (m2 * nh2)) / (SKY_PI * m2 * nh2 * nh2);
    let g = min(1.0, 2.0 * nh * min(nv, nl) / vh);
    let f = 0.04 + 0.96 * pow(1.0 - vh, 5.0);
    return SKY_PI * d * g * f / (4.0 * nv * nl);
}

// The ambient and sun light of `lit` at `world`, without the highlight, in display level.
fn light(n: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (day_lit()) {
        return pow(light_sum(n, sunlit, ambient, world, vec2<f32>(1.0)), vec3<f32>(1.0 / 2.2)) * CAR_ALBEDO / LIGHT_SCALE;
    }
    return light_sum(n, sunlit, ambient, world, mode.light.xy);
}

// How much of the ambient and bounce light the car's body lets reach the face being shaded (`body_occlusion`), 1 for
// everything else.
var<private> body_visible: f32 = 1.0;

// The ambient and sun light at `world` scaled by `k` (the game's Ka and Kd): the game's, or with the day's light
// (`day_lit`) linear irradiance as the stage's (`own_light`).
fn light_sum(n: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>, k: vec2<f32>) -> vec3<f32> {
    let ndl = max(dot(n, -frame.sun.xyz), 0.0) * sunlit;
    let day = day_lit();
    let sky = select(frame.ambient.rgb, frame.day_sky.rgb, day);
    let sun = select(frame.diffuse.rgb, frame.day_sun.rgb, day);
    var hemisphere = vec3<f32>(0.8 + 0.2 * n.y);
    // Outside the car (the in-car view's interior keeps the game's light, or the cabin's own).
    if (sky_on() && !in_car()) {
        hemisphere = sky_ambient(n);
    }
    // With the ground's maps (`bounce.rs`) the sky above the horizon and the ground below it apart: the ground's light
    // where the face looks, by its sun and sky.
    var bounce = vec3<f32>(0.0);
    if (!in_car() && frame.ground_grid.z != 0.0) {
        hemisphere = open_sky_light(n, n);
        let ground_sky = open_sky_light(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0));
        let g = ground_bounce(world, n);
        bounce = BOUNCE * (k.y * sun * g.sun + k.x * sky * ground_sky * g.sky);
    }
    return k.x * sky * hemisphere * ambient * body_visible + k.y * sun * ndl + bounce * body_visible;
}

// The light of the sky above the horizon alone on a face turned `n`, relative to an up-facing one's, in the colour
// of the sky along `toward` (the sky the face sees): with the physically based sky as `sky_ambient` makes it from
// `open_sky_irradiance` (up-facing the same), without its floor, which stood for the ground below; else an even
// overcast's, (1 + n.y) / 2.
fn open_sky_light(n: vec3<f32>, toward: vec3<f32>) -> vec3<f32> {
    if (!sky_on()) {
        return vec3<f32>(0.5 + 0.5 * n.y);
    }
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let mean = max(sky_tables.sh[0].rgb, vec3<f32>(1e-6));
    let e = open_sky_irradiance(toward) / mean * dot(mean, luma);
    let grey = dot(e, luma);
    let tinted = mix(vec3<f32>(grey), e, IBL_TINT) / max(grey, 1e-4);
    let level = dot(open_sky_irradiance(n) / mean * dot(mean, luma), luma);
    return tinted * min(level, IBL_RANGE.y);
}

// The ground's light (`ground_at`) a face at `world` turned `n` gets, per unit of the sun's and of the sky's on the
// ground: the ground it looks toward, 1.5 and 4 metres out, by how much of it the face sees (an open plane below:
// all facing down, half facing sideways). Near the car the ground there loses its sun to the car's shadow and its sky
// to the car over it (`car_contact`), taken at the car's lowest height.
struct GroundLight {
    sun: vec3<f32>,
    sky: vec3<f32>,
}

fn ground_bounce(world: vec3<f32>, n: vec3<f32>) -> GroundLight {
    var light = GroundLight(vec3<f32>(0.0), vec3<f32>(0.0));
    let seen = 0.25 * (1.0 - n.y);
    for (var k = 0u; k < 2u; k++) {
        let xz = world.xz + n.xz * select(1.5, 4.0, k == 1u);
        var g = ground_at(xz);
        if (frame.occlusion.w != 0.0) {
            let q = vec3<f32>(xz.x, frame.occlusion.z, xz.y);
            g.sun *= 1.0 - car_shadow(q);
            g.sky *= 1.0 - car_contact(q);
        }
        light.sun += g.sun * seen;
        light.sky += g.sky * seen;
    }
    return light;
}

// The tiles' side, metres (`sky_occlusion::TILE`), and the range of the ground maps' shares (`RANGE` in `bounce.wgsl`).
const GROUND_TILE: f32 = 1000.0;
const GROUND_RANGE: f32 = 2.0;

// The ground's light seen from above at `xz` (`bounce.rs`): its linear albedo times its share of the sun and of the
// sky, 0 off the maps.
fn ground_at(xz: vec2<f32>) -> GroundLight {
    let grid = frame.ground_grid;
    let at = (xz - grid.xy) / GROUND_TILE;
    let cell = floor(at);
    if (any(cell < vec2<f32>(0.0)) || cell.x >= grid.z || cell.y >= grid.w) {
        return GroundLight(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let pair = ground_tiles[u32(cell.y * grid.z + cell.x)];
    if (pair < 0) {
        return GroundLight(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let uv = at - cell;
    let sun = textureSampleLevel(ground_light, reflection_sampler, uv, pair * 2, 0.0);
    let sky = textureSampleLevel(ground_light, reflection_sampler, uv, pair * 2 + 1, 0.0);
    return GroundLight(sun.rgb * sun.a * GROUND_RANGE, sky.rgb * sky.a * GROUND_RANGE);
}

// The car's light from its surroundings (renderer choice): the reflection cube blurred to 4 texels a face (mip 5)
// around the normal, relative to the open sky there, scales the ambient (`rgb`, `AMBIENT_TINT` of it by colour, so
// the ground and trees tint it); the cube toward the sun blurred to 16 texels a face (mip 3), relative to the sky
// there, gives how much of the sun reaches the car past trees and hills (`a`; the blur spreads the sun's disc, so an
// open sky measures about 0.4 and a forest about 0.1), at least `SUN_LEAST` (light through the leaves). All 1 before
// the cube is drawn.
const AMBIENT_TINT: f32 = 0.5;
// The interior's ambient in the in-car view (ours): the game's car constants are not traced (`notes/formats.md` "Shaders and vertex layouts"), and at 1
// the dash is about a third as bright as in a 1.02 capture.
const INTERIOR_AMBIENT: f32 = 2.5;
const SUN_SEEN: vec2<f32> = vec2<f32>(0.1, 0.35);
const SUN_LEAST: f32 = 0.35;

fn surroundings(n: vec3<f32>, at: vec3<f32>) -> vec4<f32> {
    if (frame.reflection.x == 0.0) {
        return vec4<f32>(1.0);
    }
    let luma = vec3<f32>(0.299, 0.587, 0.114);
    // With the ground's maps the ground below is their light (`ground_bounce`): the cube is then taken against the open
    // sky where the face sees the sky, halfway between its normal and up.
    var look = n;
    if (frame.ground_grid.z != 0.0) {
        let half = n + vec3<f32>(0.0, 1.0, 0.0);
        look = select(vec3<f32>(0.0, 1.0, 0.0), normalize(half), dot(half, half) > 1e-4);
    }
    let around = ungraded(surrounding(look, 5.0, at));
    // With the physically based sky, the open sky as blurred as the cube's level (`sky_reflected`).
    let open = open_sky(look, 1.0);
    let ambient = mix(vec3<f32>(dot(around, luma) / max(dot(open, luma), 1e-3)), around / max(open, vec3<f32>(1e-3)), AMBIENT_TINT);
    let to_sun = -frame.sun.xyz;
    let toward = ungraded(surrounding(to_sun, 3.0, at));
    let seen = dot(toward, luma) / max(dot(open_sky(to_sun, 0.6), luma), 1e-3);
    return vec4<f32>(clamp(ambient, vec3<f32>(0.3), vec3<f32>(1.2)), mix(SUN_LEAST, 1.0, smoothstep(SUN_SEEN.x, SUN_SEEN.y, seen)));
}

// The stage around `at` along `dir`, blurred to the surroundings cube's `level`, through the tone curve: once the
// probes are baked (`frame.reflection.z`) the two nearest `at` blended by their distances, of the cell's nearest and
// its neighbours along the driveline (the probes' first level is the cube's second); else the surroundings cube.
fn surrounding(dir: vec3<f32>, level: f32, at: vec3<f32>) -> vec3<f32> {
    if (frame.reflection.z == 0.0) {
        return textureSampleLevel(surroundings_cube, reflection_sampler, dir, level + CUBE_SHIFT).rgb;
    }
    let size = vec2<i32>(probes.counts.xy);
    let cell = clamp(vec2<i32>(floor((at.xz - probes.grid.xy) / probes.grid.z)), vec2<i32>(0), size - 1);
    let first = i32(probes.cells[cell.y * size.x + cell.x]);
    var near = vec2<i32>(first);
    var d = vec2<f32>(distance(at, probes.points[first].xyz), 1e30);
    for (var step = -1; step <= 1; step += 2) {
        let k = first + step;
        if (k < 0 || k >= i32(probes.counts.z)) {
            continue;
        }
        let dk = distance(at, probes.points[k].xyz);
        if (dk < d.x) {
            near = vec2<i32>(k, near.x);
            d = vec2<f32>(dk, d.x);
        } else if (dk < d.y) {
            near.y = k;
            d.y = dk;
        }
    }
    let lod = max(level + CUBE_SHIFT - 1.0, 0.0);
    let a = textureSampleLevel(probe_cubes, reflection_sampler, dir, near.x, lod).rgb;
    let b = textureSampleLevel(probe_cubes, reflection_sampler, dir, near.y, lod).rgb;
    return mix(a, b, d.x / max(d.x + d.y, 1e-3));
}

// The open sky along `dir`: `sky_reflected` by `roughness` under the physically based sky, else `sky`. A branch, not a
// `select`, which would work out both.
fn open_sky(dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    if (sky_on()) {
        return sky_reflected(dir, roughness);
    }
    return sky(dir, 1.0);
}

// The roads' specular reflection with the day's light (`own_light`, ours): a dielectric (F0 0.04) of the road's
// roughness, the sun's highlight (GGX, Smith's height-correlated visibility, Schlick's Fresnel) past its
// shadows and the sky's reflection (the sky's cube at that roughness, Karis's analytic environment BRDF), as much as
// the point sees the sky (Lagarde's specular occlusion); the diffuse light keeps what is not reflected. Only the roads
// (the draws with a specular map): `ROAD_ROUGHNESS`, smoother where the map is bright (polished stones) and when
// damp. The rest of the ground, rock and grass (roughness about 0.95) reflect a few percent, close to diffuse, and
// cost 0.5 ms a frame for it; the cut-out draws' normals are the crown's or the ground's, not the leaves'.
const ROAD_ROUGHNESS: f32 = 0.75;
const ROAD_POLISH: f32 = 0.15;
const DAMP_ROUGHNESS: f32 = 0.15;

// How much of its surroundings a dielectric (F0 0.04) of `roughness` reflects at `nv`: Karis's fit of the split-sum
// environment BRDF, at most 1.
fn environment_reflectance(nv: f32, roughness: f32) -> f32 {
    let r = roughness * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return min(0.04 * ab.x + ab.y, 1.0);
}

fn with_specular(colour: vec3<f32>, v: VertexOut, front: bool, sun_seen: f32) -> vec3<f32> {
    if (dot(v.normal, v.normal) < 1e-8) {
        return colour;
    }
    let n = normalize(v.normal) * select(-1.0, 1.0, front);
    // The maps' texels run about 0 to 0.25.
    let map = dot(textureSample(specular_texture, texture_sampler, v.specular.xy).rgb, vec3<f32>(0.333));
    let polish = clamp(map * 4.0 * v.specular.z, 0.0, 1.0);
    let roughness = ROAD_ROUGHNESS - ROAD_POLISH * polish - DAMP_ROUGHNESS * min(frame.wetness.x * 2.0, 1.0);
    let view = normalize(frame.eye.xyz - v.world);
    let nv = max(dot(n, view), 1e-3);
    let reflectance = environment_reflectance(nv, roughness);
    let seen = 1.0 - v.sky.z;
    let occlusion = clamp(pow(nv + seen, exp2(-16.0 * roughness - 1.0)) - 1.0 + seen, 0.0, 1.0);
    let sky = pow(sky_reflected(reflect(-view, n), roughness), vec3<f32>(2.2)) * occlusion;
    // The sun's highlight.
    let to_sun = -frame.sun.xyz;
    let nl = dot(n, to_sun);
    var sun = vec3<f32>(0.0);
    if (nl > 0.0 && sun_seen > 0.0) {
        let h = normalize(to_sun + view);
        let nh = max(dot(n, h), 0.0);
        let a2 = pow(roughness, 4.0);
        let d = a2 / (SKY_PI * pow(nh * nh * (a2 - 1.0) + 1.0, 2.0));
        let a = roughness * roughness;
        let vis = 0.5 / (nl * (nv * (1.0 - a) + a) + nv * (nl * (1.0 - a) + a));
        let f = 0.04 + 0.96 * pow(1.0 - max(dot(view, h), 0.0), 5.0);
        sun = frame.day_sun.rgb * SKY_PI * d * vis * f * nl * sun_seen;
    }
    let linear = pow(colour, vec3<f32>(2.2)) * (1.0 - reflectance) + sky * reflectance + sun;
    return pow(linear, vec3<f32>(1.0 / 2.2));
}

// The stage's specular highlight (slots 6 and 8, `notes/formats.md` "Shaders and vertex layouts"): `4 * sat(t2.rgb * S * strength)` with the specular
// map `t2`, the vertex's strength, and the game's highlight `S = scale * 2^(sharpness * (1 - 1 / (n.h)^2)) / (n.h)^4`
// from the stored normal and the half vector between the sun and the eye. The game's `sharpness` and `scale` (`c90`)
// have no traced source: `SHINE_SHARPNESS` and `SHINE_SCALE` are ours. So is `WEATHER_SHINE`, how much the highlight
// is scaled when dry, damp and wet (the game's specular maps are the same in every weather; with `Enhance::wet` the
// water film, `wet_plus`, smooths the glitter out and does the shining).
const SHINE_SHARPNESS: f32 = 8.0;
const SHINE_SCALE: f32 = 0.5;
const WEATHER_SHINE: vec3<f32> = vec3<f32>(1.125, 1.5, 0.3);

// The direct light square on with the sun 37.5 degrees up, renderer units (`skylight::NOON_SUN`).
const NOON_DIRECT: f32 = 2.532;

fn road_shine(v: VertexOut) -> vec3<f32> {
    let map = textureSample(specular_texture, texture_sampler, v.specular.xy).rgb;
    if (dot(v.normal, v.normal) < 1e-8) {
        return vec3<f32>(0.0);
    }
    let n = normalize(v.normal);
    let h = normalize(normalize(frame.eye.xyz - v.world) - frame.sun.xyz);
    let nh = max(dot(n, h), 0.05);
    let nh2 = nh * nh;
    let weather = select(select(WEATHER_SHINE.x, WEATHER_SHINE.y, frame.wetness.x >= 0.5), WEATHER_SHINE.z, frame.wetness.x >= 1.0);
    let s = SHINE_SCALE * exp2(SHINE_SHARPNESS * (1.0 - 1.0 / nh2)) / (nh2 * nh2) * weather;
    return 4.0 * clamp(map * s * v.specular.z, vec3<f32>(0.0), vec3<f32>(1.0));
}

// Water (renderer choice: the game's water texture keys are unresolved, `rbr-render/README.md` (git 9c28c0b8)): a level surface rippled by two
// octaves of value noise (about 1.4 and 0.4 m, `RIPPLE` the slope, fading with distance so far water does not
// shimmer), reflecting the sky by water's Fresnel (F0 0.02), over a dark body (`WATER_BODY`, lit by the stage's
// ambient and sun) that lets the bed show through when looked at from above, and in clear weather a glint of the sun.
// Only the sky is drawn for reflection, so reflections within `BANKS` of the horizon, where the banks and trees would
// be, turn to a dull bank colour (`BANK`, lit like the body).
const RIPPLE: f32 = 0.12;
const WATER_BODY: vec3<f32> = vec3<f32>(0.08, 0.10, 0.09);
const BANKS: f32 = 0.7;
const BANK: vec3<f32> = vec3<f32>(0.22, 0.22, 0.17);

fn ripple_height(p: vec2<f32>) -> f32 {
    return 0.6 * value_noise(vec3<f32>(p * 0.7, 0.0)) + 0.4 * value_noise(vec3<f32>(p * 2.3, 0.5));
}

// What water reflects along `mirrored` (`BANKS`, `BANK`), lit by `daylight`.
fn mirrored_light(mirrored: vec3<f32>, daylight: vec3<f32>) -> vec3<f32> {
    return mix(BANK * daylight, sky(mirrored, 1.0), smoothstep(0.0, BANKS, mirrored.y));
}

// The sun's glint on water along `mirrored`, in clear weather: a small bright core (a calm surface mirrors the disc)
// and a soft halo.
fn glint(mirrored: vec3<f32>) -> vec3<f32> {
    let light = sunlight();
    let toward = max(dot(mirrored, -frame.sun.xyz), 0.0);
    let shine = frame.sun.w * (1.0 - frame.night.x) * (2.5 * pow(toward, 12000.0) + 0.15 * pow(toward, 600.0)) * light / max(max(light.r, light.g), light.b);
    return shine * (1.0 - cloud_shadow(frame.eye.xyz));
}

// A wet road (renderer addition, `Enhance::wet`, `wet_plus` in `water.wgsl`): a film of water (thinner when damp)
// over the ground whose collision surface takes it (`VertexOut::wet`, `puddles::mark_roads`), reflecting the stage
// around the car (its reflection cube; the sky like the water surface, `mirrored_light`, before the cube is drawn) and
// the sun's glint, over the darkened road. Patches of about 3 m (`PUDDLES`) hold more water.
const PUDDLES: f32 = 3.0;

// Water's Fresnel (F0 0.02) for normal `n` seen along `view` (toward the eye).
fn water_fresnel(n: vec3<f32>, view: vec3<f32>) -> f32 {
    return 0.02 + 0.98 * pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 5.0);
}

// How much a wet road reflects (stylised, well above water's Fresnel, so wet roads read as in games and photos of
// rain): `WET_REFLECT` at a steep view between the patches and in them, rising toward `WET_MOST` at a grazing view, so
// the road's texture still shows. Wet asphalt is also darker and deeper in colour (`WET_DARKEN`, `WET_SATURATION`):
// water fills its pores.
const WET_REFLECT: vec2<f32> = vec2<f32>(0.06, 0.35);
const WET_MOST: f32 = 0.75;
const WET_DARKEN: f32 = 0.7;
const WET_SATURATION: f32 = 1.2;

// Where water stands (0 to 1), by stage position so it does not follow the texture's stones: a thin film everywhere
// (`FILM`) and smooth patches of standing water from about 0.7 to 3 m across (`PUDDLES` and a finer octave), times the
// road's share (`VertexOut::wet`), which fades it out toward the road's borders (`puddled` in `water.wgsl`).
const FILM: f32 = 0.35;

// How much water stands where the film is `wet`: none on the thin film, all in its patches.
fn standing(wet: f32) -> f32 {
    return smoothstep(0.6, 1.0, wet);
}

const GROUND_SOAK: f32 = 0.6;

// `colour` darkened and saturated as wet ground is, by `amount` of the road's (`WET_DARKEN`, `WET_SATURATION`).
fn soak(colour: vec3<f32>, amount: f32) -> vec3<f32> {
    let grey = dot(colour, vec3<f32>(0.299, 0.587, 0.114));
    let saturated = max(mix(vec3<f32>(grey), colour, mix(1.0, WET_SATURATION, amount)), vec3<f32>(0.0));
    return saturated * mix(1.0, WET_DARKEN, amount);
}

// Rain landing on standing water (renderer addition, `Enhance::particles`; not the game's look): rings spreading from
// where drops land, about `LANDING_RATE` a second in each `LANDING_CELL` square at full rain (fewer in lighter rain),
// tilting the water's normal by `standing` (0 on the thin film, where drops leave no rings; 1 on puddles and water) so
// its reflections shiver. Two offset grids, each ring tested in its cell's neighbours; gone where the distance over
// the view's cosine passes `LANDING_FAR`, as a pixel then spans a ring and they would only flicker.
const LANDING_CELL: f32 = 0.3;
const LANDING_RATE: f32 = 1.2;
const LANDING_REACH: f32 = 0.16;
const LANDING_WIDTH: f32 = 0.02;
const LANDING_TILT: f32 = 0.25;
const LANDING_FAR: f32 = 60.0;

// Three values of the cell `c`: "Hash without Sine" (`hash32`), Copyright (c) 2014 David Hoskins, MIT (as `hash`).
fn landing_hash(c: vec2<f32>) -> vec3<f32> {
    var p3 = fract(c.xyx * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yxz + 33.33);
    return fract((p3.xxy + p3.yzz) * p3.zyx);
}

fn rain_rings(n: vec3<f32>, world: vec3<f32>, standing: f32) -> vec3<f32> {
    let rain = frame.wind.z;
    let toward = frame.eye.xyz - world;
    let stretched = length(toward) / max(dot(n, normalize(toward)), 0.05);
    let fade = (1.0 - smoothstep(0.4 * LANDING_FAR, LANDING_FAR, stretched)) * standing;
    if (rain <= 0.0 || fade <= 0.0) {
        return n;
    }
    var tilt = vec2<f32>(0.0);
    for (var layer = 0; layer < 2; layer++) {
        let p = world.xz / LANDING_CELL + vec2<f32>(0.5 * f32(layer));
        let cell = floor(p);
        for (var k = 0; k < 9; k++) {
            let c = cell + vec2<f32>(f32(k % 3 - 1), f32(k / 3 - 1));
            let h = landing_hash(c + vec2<f32>(37.0 * f32(layer)));
            if (h.z > rain) {
                continue;
            }
            let age = fract(frame.wind.x * LANDING_RATE * mix(0.7, 1.3, h.y) + h.x);
            let d = (p - c - h.xy) * LANDING_CELL;
            let r = length(d);
            let x = (r - age * LANDING_REACH) / LANDING_WIDTH;
            if (abs(x) < 1.0) {
                tilt += d / max(r, 1e-4) * sin(x * 3.14159265) * (1.0 - age) * (1.0 - age);
            }
        }
    }
    return normalize(n + vec3<f32>(tilt.x, 0.0, tilt.y) * LANDING_TILT * fade);
}

// The rippled water surface's normal at `world`.
fn water_normal(world: vec3<f32>) -> vec3<f32> {
    let p = world.xz;
    let step = 0.05;
    let h = ripple_height(p);
    let strength = RIPPLE / (1.0 + distance(world, frame.eye.xyz) / 40.0) / step;
    let slope = strength * vec2<f32>(ripple_height(p + vec2<f32>(step, 0.0)) - h, ripple_height(p + vec2<f32>(0.0, step)) - h);
    return rain_rings(normalize(vec3<f32>(-slope.x, 1.0, -slope.y)), world, 1.0);
}

fn water_surface(world: vec3<f32>) -> vec4<f32> {
    let view = normalize(frame.eye.xyz - world);
    let n = water_normal(world);
    let nv = clamp(dot(n, view), 0.0, 1.0);
    let f = water_fresnel(n, view);
    let mirrored = reflect(-view, n);
    let to_sun = -frame.sun.xyz;
    let daylight = frame.ambient.rgb + frame.diffuse.rgb * max(to_sun.y, 0.0);
    let body = WATER_BODY * daylight;
    // The body hides the bed more at a grazing view; the reflection covers what it reflects.
    let body_alpha = mix(0.6, 0.95, 1.0 - nv);
    let alpha = body_alpha * (1.0 - f) + f;
    let colour = body * body_alpha * (1.0 - f) + mirrored_light(mirrored, daylight) * f + glint(mirrored);
    return vec4<f32>(colour / alpha, clamp(alpha, 0.0, 1.0));
}

// Screen-space reflections (renderer choice, `renderer::ScreenReflection`): the water, and the wet roads' film, drawn
// again at half the frame's size over the resolved frame (`Traced`, laid over it by `reflection_composite.wgsl`). The
// reflected ray is stepped through the stage (`SSR_STEPS` steps, the setting's `Reflections::rays`, growing, out to `SSR_REACH` metres); the first step
// behind the frame's depth, by less than a thickness that grows with the distance, (each pixel's steps start at a
// different fraction of a step, so the step size does not show as bands) is refined by halving and its frame colour is
// blended in by the surface's Fresnel weight, fading out toward the screen's edges and with the distance stepped. Rays that leave the screen or hit nothing take the car's reflection
// cube instead (the stage around the car, `renderer::Reflection`), near the eye only (`SSR_CUBE_REACH` metres), since
// the cube is drawn at the car; elsewhere, or with no car, the sky reflection drawn with the surface shows.
override SSR_STEPS: i32 = 64;
override SSR_RAYS: i32 = 4;
const SSR_REACH: f32 = 120.0;
const SSR_CUBE_REACH: vec2<f32> = vec2<f32>(20.0, 60.0);
// Glossy reflections: the frame is read from a blurrier level (`renderer::SCENE_MIPS`) the further the ray travels on
// screen, as a cone of `GLOSS` (its width over the distance, in pixels), and its rays spread by `ROUGHNESS`, stretched
// toward and away from the eye by `STRETCH`; (x the wet road's film, y water and puddles). The film reflects almost as
// sharply as water: wet asphalt smears a mirror image a little, and the car must stay recognisable. A hit is trusted less the further behind the frame's surface the ray ends (`thickness`), fading
// instead of cutting off, and a ray that ends further behind than that keeps stepping (it passed behind something in
// front, a trunk or a foliage card, rather than hitting it).
const GLOSS: vec2<f32> = vec2<f32>(0.01, 0.04);
const ROUGHNESS: vec2<f32> = vec2<f32>(0.012, 0.01);
const STRETCH: vec2<f32> = vec2<f32>(12.0, 4.0);
// The film's hits fade out over this share of the screen's edge (a car at the bottom of the frame reflects in the road
// behind it); water and puddles over 0.15.
const FILM_EDGE: f32 = 0.03;

// The frame's depth at `uv`.
fn scene_depth_at(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let pixel = vec2<i32>(clamp(uv * size, vec2<f32>(0.0), size - 1.0));
    return textureLoad(scene_depth, pixel, 0);
}

// The eye's distance to the stage at `uv`, where the frame's depth is `depth`.
fn scene_distance(uv: vec2<f32>, depth: f32) -> f32 {
    let p = frame.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return distance(p.xyz / p.w, frame.eye.xyz);
}

// Where `p` falls on the screen (uv, and z its depth), w 0 when it is off screen or behind the eye.
fn on_screen(p: vec3<f32>) -> vec4<f32> {
    return on_screen_clip(frame.view_projection * vec4<f32>(p, 1.0));
}

// `on_screen` from the point's clip position.
fn on_screen_clip(clip: vec4<f32>) -> vec4<f32> {
    if (clip.w <= 1e-4) {
        return vec4<f32>(0.0);
    }
    let ndc = clip.xyz / clip.w;
    let inside = f32(all(abs(ndc.xy) <= vec2<f32>(1.0)));
    return vec4<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, ndc.z, inside);
}

// What the half-size trace writes (`renderer::ScreenReflection`): the reflection and its weight, and the surface's
// depth, by which `reflection_composite.wgsl` lays it over the frame.
struct Traced {
    @location(0) colour: vec4<f32>,
    @location(1) depth: f32,
}

// The trace has no depth attachment at half size: a surface behind the frame's by more than `HIDDEN` of its depth (a
// share of the distance) is hidden.
const HIDDEN: f32 = 0.02;

fn hidden(v: VertexOut) -> bool {
    return v.clip.z < scene_depth_at(on_screen(v.world).xy) * (1.0 - HIDDEN);
}

@fragment
fn fs_reflect(v: VertexOut) -> Traced {
    if (hidden(v)) {
        discard;
    }
    return Traced(reflect_ray(v), v.clip.z);
}

// The screen-space reflection at `v` and its weight: the mean of `SSR_RAYS` rays spread over the gloss cone
// (`ROUGHNESS`, wider on roads than on water), so hits and misses average out instead of showing per pixel.
fn reflect_ray(v: VertexOut) -> vec4<f32> {
    // Interleaved gradient noise (Jimenez 2014) by pixel.
    let jitter = fract(52.9829189 * fract(dot(v.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    let view = normalize(frame.eye.xyz - v.world);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    var weight = 0.0;
    if (mode.water != 0u) {
        n = water_normal(v.world);
        weight = water_fresnel(n, view);
    }
    let ray = reflect(-view, n);
    if (weight <= 0.0 || ray.y <= 0.0) {
        return vec4<f32>(0.0);
    }
    // What a missed ray reflects: on water the cube near the eye, else nothing (what the surface was drawn with: the
    // sky on water, the cube on roads).
    var miss = vec4<f32>(0.0);
    if (frame.reflection.x != 0.0 && mode.water != 0u) {
        let near = 1.0 - smoothstep(SSR_CUBE_REACH.x, SSR_CUBE_REACH.y, distance(v.world, frame.eye.xyz));
        miss = vec4<f32>(surrounding(ray, select(3.0, 1.0, mode.water != 0u), v.world), weight * near);
    }
    let roughness = select(ROUGHNESS.x, ROUGHNESS.y, mode.water != 0u);
    let side = normalize(cross(ray, n + vec3<f32>(1e-3, 0.0, 0.0)));
    let up = cross(side, ray);
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < SSR_RAYS; k++) {
        // Points on a disc, turned by the pixel's noise.
        let angle = 6.2831853 * (f32(k) + jitter) / f32(SSR_RAYS);
        let radius = roughness * sqrt((f32(k) + 0.5) / f32(SSR_RAYS));
        // Stretched toward and away from the eye (`STRETCH.y`), as wet roads smear reflections down the screen.
        let spread = normalize(ray + radius * (cos(angle) * side + STRETCH.y * sin(angle) * up));
        let found = trace(v, spread, weight, miss, fract(jitter + f32(k) * 0.618), f32(mode.water != 0u), select(GLOSS.x, GLOSS.y, mode.water != 0u));
        // Colour weighted by how much each ray contributes.
        sum += vec4<f32>(found.rgb * found.a, found.a);
    }
    sum /= f32(SSR_RAYS);
    return vec4<f32>(sum.rgb / max(sum.a, 1e-4), sum.a);
}

// One ray from `v` along `ray` (`still` 1 on water and puddles: a sharp reflection, `gloss` its cone, `GLOSS`): what it reflects and its weight (`weight` on a hit, `miss` when it leaves the screen
// or hits nothing), its steps starting `jitter` of a step early.
fn trace(v: VertexOut, ray: vec3<f32>, weight: f32, miss: vec4<f32>, jitter: f32, still: f32, gloss: f32) -> vec4<f32> {
    if (ray.y <= 0.0) {
        return miss;
    }
    let thickness = 0.15 + 0.01 * distance(v.world, frame.eye.xyz);
    // The clip position runs linearly along the ray.
    let start = frame.view_projection * vec4<f32>(v.world, 1.0);
    let along = frame.view_projection * vec4<f32>(ray, 0.0);
    // A film's step may end far behind the surface it crossed (a car's rear, seen along the ray): it halves back to
    // the crossing whatever its depth and `reach` checks that; water's and puddles' must end within the thickness.
    let film = still < 0.5;
    var before = 0.0;
    for (var i = 1; i <= SSR_STEPS; i++) {
        let t = SSR_REACH * pow((f32(i) - jitter) / f32(SSR_STEPS), 2.0);
        let at = on_screen_clip(start + along * t);
        if (at.w == 0.0) {
            return miss;
        }
        // The step's distance behind the frame's surface only once it is behind it.
        let depth = scene_depth_at(at.xy);
        if (at.z < depth && depth > 0.0
            && (film || distance(v.world + ray * t, frame.eye.xyz) - scene_distance(at.xy, depth) < thickness + 0.02 * t)) {
            // Halve back to the surface.
            var low = before;
            var high = t;
            for (var k = 0; k < 5; k++) {
                let mid = 0.5 * (low + high);
                let m = on_screen_clip(start + along * mid);
                if (m.z < scene_depth_at(m.xy)) {
                    high = mid;
                } else {
                    low = mid;
                }
            }
            let hit = on_screen_clip(start + along * high);
            let behind = distance(v.world + ray * high, frame.eye.xyz) - scene_distance(hit.xy, scene_depth_at(hit.xy));
            let reach = thickness + 0.02 * high;
            if (behind <= reach) {
                let size = vec2<f32>(textureDimensions(scene_colour));
                let pixels = length((hit.xy - on_screen_clip(start).xy) * size);
                let lod = clamp(log2(max(pixels * gloss, 1.0)), 0.0, f32(textureNumLevels(scene_colour) - 1u));
                let colour = textureSampleLevel(scene_colour, reflection_sampler, hit.xy, lod).rgb;
                let edge = smoothstep(0.0, select(0.15, FILM_EDGE, film), min(min(hit.x, 1.0 - hit.x), min(hit.y, 1.0 - hit.y)));
                let fade = 1.0 - smoothstep(0.6, 1.0, high / SSR_REACH);
                let trust = edge * fade * (1.0 - smoothstep(0.3 * reach, reach, max(behind, 0.0)));
                return vec4<f32>(mix(miss.rgb, colour, trust), mix(miss.a, weight, trust));
            }
        }
        before = t;
    }
    return miss;
}

// Ground detail (renderer choice): two octaves of 3D value noise in stage space, at 9 and 33 cm, that brighten or
// darken the ground by up to `DETAIL` near the eye and fade out between `DETAIL_FADE` metres, so the low-resolution
// ground textures do not look smeared at the chase camera's distance. Two more octaves, at 8 and 30 m, brighten or
// darken it by up to `MACRO` at every distance, so the repeats of a ground texture do not look alike.
const DETAIL: f32 = 0.08;
const DETAIL_FADE: vec2<f32> = vec2<f32>(4.0, 30.0);
const MACRO: f32 = 0.22;

// A point's value, 0 to 1: "Hash without Sine" (`hash13`), Copyright (c) 2014 David Hoskins, MIT
// (`notes/references/hash-without-sine/LicenseText.txt`). Multiplies and adds of small values, which every GPU rounds
// alike (a fused multiply-add moves it by under 0.5 %). A sine hash (`fract(sin(x) * 43758.5)`) does not: GPUs reduce
// its large arguments differently, so the ground's patches fell elsewhere on each and a shot differed between machines.
fn hash(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.zyx + 31.32);
    return fract((p3.x + p3.y) * p3.z);
}

fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let x00 = mix(hash(i), hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x);
    let x10 = mix(hash(i + vec3<f32>(0.0, 1.0, 0.0)), hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x);
    let x01 = mix(hash(i + vec3<f32>(0.0, 0.0, 1.0)), hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x);
    let x11 = mix(hash(i + vec3<f32>(0.0, 1.0, 1.0)), hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

fn ground_detail(world: vec3<f32>) -> f32 {
    let near = 1.0 - smoothstep(DETAIL_FADE.x, DETAIL_FADE.y, distance(world, frame.eye.xyz));
    // Beyond the fade the fine octaves count for nothing, so they are not computed.
    var fine = 0.5;
    if (near > 0.0) {
        fine = 0.65 * value_noise(world * 11.0) + 0.35 * value_noise(world * 3.0);
    }
    // The sum of the octaves stays near its middle; stretched so the patches reach `MACRO`.
    let coarse = smoothstep(0.3, 0.7, 0.6 * value_noise(world / 8.0) + 0.4 * value_noise(world / 30.0));
    return (1.0 + DETAIL * near * (2.0 * fine - 1.0)) * (1.0 + MACRO * (2.0 * coarse - 1.0));
}

// Road scans (renderer choice, `Enhance::gravel`, `asphalt`, `snow`): a scanned surface (`renderer::road_scan`, by
// layer: 0 gravel, 1 asphalt, 2 snow) tiled flat in stage space every `SCAN_TILE` metres. Only its brightness over its mean (its last
// mip) scales the road's colour, by `SCAN_DETAIL`, so the stage keeps its colour and its tyre tracks; its normals lit by
// the sun against its mean normal, by `SCAN_RELIEF` and faded out with a low sun, and its hollows darker by
// `SCAN_HOLLOWS` give the grain relief, leaving the road's mean brightness. The mips average it out with distance.
// Parallax (4 to 24 steps within 25 m) looked the same on gravel in stills and cost 0.35 to 1.3 ms of the scene at
// 1440p, so it was left out.
const SCAN_TILE: vec3<f32> = vec3<f32>(3.0, 2.0, 2.0);
const SCAN_DETAIL: vec3<f32> = vec3<f32>(0.5, 1.2, 0.5);
const SCAN_RELIEF: vec3<f32> = vec3<f32>(0.35, 0.6, 0.35);
const SCAN_HOLLOWS: vec3<f32> = vec3<f32>(0.24, 0.4, 0.24);
// The least a scan's factor darkens to: the snow scan's dark marks (footprints, debris) otherwise read as dirt.
const SCAN_FLOOR: vec3<f32> = vec3<f32>(0.0, 0.0, 0.88);

// Scan `layer`'s factor on the colour at `uv` (its tiles), with the derivatives `dx` and `dy` taken in uniform control
// flow, `sun_seen` of the sun reaching the ground there.
fn road_scan(layer: i32, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, sun_seen: f32, grey: f32) -> f32 {
    var strength = vec3<f32>(SCAN_DETAIL[layer], SCAN_RELIEF[layer], SCAN_HOLLOWS[layer]);
    if (layer == 1) {
        strength = mix(ASPHALT_SHOULDER, strength, grey);
    }
    let scan = textureSampleGrad(road_scans, road_scan_sampler, uv, layer, dx, dy);
    let mean = textureSampleLevel(road_scans, road_scan_sampler, uv, layer, 20.0);
    let detail = mix(1.0, scan.r / max(mean.r, 1e-3), strength.x);
    let to_sun = -frame.sun.xyz;
    let n = scan_normal(scan.gb);
    let lit = clamp((dot(n, to_sun) - dot(scan_normal(mean.gb), to_sun)) / max(to_sun.y, 0.05), -1.0, 1.0);
    let relief = 1.0 + lit * sun_seen * strength.y * smoothstep(0.1, 0.35, to_sun.y);
    let hollows = 1.0 - strength.z * (mean.a - scan.a);
    return max(detail * relief * hollows, SCAN_FLOOR[layer]);
}

// Ground scan (renderer choice, `Enhance::ground`): the off-road ground (`puddles::mark_ground`, `Vertex::edge` -1, `ground_share`)
// takes a scanned grass-and-earth surface (`renderer::road_scan` layers 3 and 4, 2.5 m across) tiled in world space and
// projected on the three axis planes by the face's normal (triplanar, as `terrain_detail`, so a bank does not streak
// it), at `GROUND_TILES` metres (the second tiling turned by `GROUND_TURN`, cosine and sine of 30 degrees, against repeats). The scan's colour over its
// mean (the last mip) scales the ground's, by `GROUND_DETAIL` per tiling: patches of grass and bare earth, stones and
// roots, in the stage's own mean colour. A surface that is bright and grey (snow, pale rock) takes only its brightness,
// no lighter than `GROUND_FLOOR`. The first tiling's normals lit by the sun against its mean normal, by `GROUND_RELIEF`
// and faded out with a low sun, and its hollows darker by `GROUND_HOLLOWS`, give the grain relief. The stage's baked
// vertex tint (`own_light`) keeps `GROUND_HUE` of itself on the near ground, roads too: it was baked for the original's sun and
// ambient and tints the shade blue and the lit slopes over-green under ours. The ground's own texture is read
// `GROUND_BLUR` levels blurrier (its streaks and smear are in its fine texels; its patches of colour stay), and its
// high pass (`terrain_detail`) keeps `GROUND_KEEP` of its strength: the scan brings the detail.
const GROUND_TILES: vec2<f32> = vec2<f32>(1.7, 5.1);
const GROUND_GRAIN: f32 = 2.2;
const GROUND_CHROMA: f32 = 0.6;
const GROUND_SHARP: f32 = 0.6;
const GROUND_SOFT: f32 = 4.0;
const GROUND_DETAIL: vec2<f32> = vec2<f32>(1.8, 1.8);
const GROUND_TURN: vec2<f32> = vec2<f32>(0.866, 0.5);
const GROUND_RELIEF: f32 = 0.5;
const GROUND_HOLLOWS: f32 = 0.3;
const GROUND_FLOOR: f32 = 0.88;
const GROUND_HUE: f32 = 0.3;
const GROUND_BLUR: f32 = 0.75;
const GROUND_MAGNIFIED: vec2<f32> = vec2<f32>(0.4, 1.0);
const GROUND_KEEP: f32 = 0.7;
// The mip the paved road's grey test reads its texture at (`paved_grey`), whatever the footprint: read that many levels
// past it, the far road read a mip where the verge painted beside it in the texture blends in, failed the test and took
// the ground scan as a shoulder, green and pink patches swimming ahead of the car (RSF Helette 580).
const ASPHALT_BLUR: f32 = 3.0;
// The finest the paved road's own texture is read, metres a pixel: the asphalt scan carries the grain under it, and the
// texture's flecks magnified read as scratches. Its gradients are widened to it (`paved_reach` in `shade_stage`), so the
// blur falls off with the distance alone, the same on every quad (blurring by its magnification, quads stretching
// the texture differently changed the road's look in steps, and a band ahead of the car read blotchier, RSF Fintele).
const ASPHALT_FINEST: f32 = 0.04;
// The paved road's local mean is its texture's mip `ASPHALT_MEAN`, whatever the footprint: read that many levels past the
// footprint, the quads beside each other that stretch the texture differently read different mips and showed as blocks.
const ASPHALT_MEAN: f32 = 6.0;
const ASPHALT_GREY: vec2<f32> = vec2<f32>(0.1, 0.18);
// The scan's strengths (detail, relief, hollows) on the paved mesh's grass shoulders: what it was before the road's.
const ASPHALT_SHOULDER: vec3<f32> = vec3<f32>(0.5, 0.35, 0.24);
// The face's up component under which a paved mesh's warm texture is a wall, not a shoulder (`shade_stage`).
const SHOULDER_WALL: vec2<f32> = vec2<f32>(0.3, 0.5);
const ASPHALT_LINE: vec2<f32> = vec2<f32>(1.1, 1.3);
const ASPHALT_KEEP: f32 = 0.0;

// The paved road's texel at `uv` of `t`, its gradients `dx` and `dy` (taken in uniform control flow) widened by `reach`.
fn asphalt_texel(t: texture_2d<f32>, uv: vec2<f32>, dx: vec2<f32>, dy: vec2<f32>, reach: f32) -> vec3<f32> {
    return textureSampleGrad(t, texture_sampler, uv, dx * reach, dy * reach).rgb;
}

// Under our grass (`grass.rs`) the ground between the blades is the sward's thatch in their shade: darker and fuller in
// colour (`THATCH`: brightness, saturation) by how much grass grows there. That is the grass's own test (`grass::green`,
// `THATCH_GRASSY` its ranges off the road and on the road mesh) on the ground's colour as it takes it: the texture's mip
// about 45 texels wide (`THATCH_MIP` levels under its width: the test's 256-texel level blurred over 7) in `GROUND_HUE`
// of the vertex tint; on faces no steeper than it grows on (`THATCH_UP`, about `grass::LEVEL`), at its density by the
// distance from the eye (`grass_density`). Not the band along the driveline it keeps to: the near ground past it is thatched too. `face` is the face's
// normal, any length.
const THATCH: vec2<f32> = vec2<f32>(0.82, 1.1);
const THATCH_GRASSY: array<vec2<f32>, 2> = array<vec2<f32>, 2>(vec2<f32>(0.02, 0.07), vec2<f32>(0.05, 0.11));
const THATCH_MIP: f32 = 5.5;
const THATCH_UP: vec2<f32> = vec2<f32>(0.4, 0.5);

fn thatch(colour: vec3<f32>, v: VertexOut, face: vec3<f32>) -> vec3<f32> {
    let road = v.wet.x > 0.0;
    let candidate = select(ground_share(v), 1.0, road);
    if (candidate == 0.0) {
        return colour;
    }
    var ground = thatch_texel(texture0, v.uv0).rgb;
    if (second_draw()) {
        let second = thatch_texel(texture1, v.uv1);
        ground = mix(ground, second.rgb, second.a * v.color.a);
    }
    let c = ground * mix(vec3<f32>(1.0), baked_hue(v.color.rgb), GROUND_HUE);
    let g = c / max(c.r + c.g + c.b, 1e-3);
    let range = THATCH_GRASSY[u32(road)];
    let green = smoothstep(range.x, range.y, g.g - g.b - 2.0 * max(g.r - g.g - 0.02, 0.0));
    let up = smoothstep(THATCH_UP.x, THATCH_UP.y, abs(face.y) * inverseSqrt(max(dot(face, face), 1e-12)));
    let dense = grass_density(distance(v.world, frame.eye.xyz));
    let grey = dot(colour, vec3<f32>(0.2126, 0.7152, 0.0722));
    let thatched = max(mix(vec3<f32>(grey), colour, THATCH.y), vec3<f32>(0.0)) * THATCH.x;
    return mix(colour, thatched, candidate * green * up * dense);
}

fn thatch_texel(t: texture_2d<f32>, uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(t, texture_sampler, uv, max(log2(f32(textureDimensions(t).x)) - THATCH_MIP, 0.0));
}

// How much of the pixel is the off-road ground: the vertices' mark against the road's share.
fn ground_share(v: VertexOut) -> f32 {
    return clamp(-v.wet.y, 0.0, 1.0) * (1.0 - clamp(v.wet.x * 4.0, 0.0, 1.0));
}

// The world direction of a scan normal `t` (OpenGL: x along u, y up the image, z out of it) laid on the plane across
// `axis`, whose uv are the other two axes in order (`ground_scan`), seen from the side `side` (1 or -1) of the plane.
fn plane_normal(axis: i32, t: vec3<f32>, side: f32) -> vec3<f32> {
    if (axis == 0) {
        return vec3<f32>(side * t.z, -t.y, t.x);
    } else if (axis == 1) {
        return vec3<f32>(t.x, side * t.z, -t.y);
    }
    return vec3<f32>(t.x, -t.y, side * t.z);
}

// Earth and grass in patches a few metres across, and stones (the scan holds neither at that scale): a mask from noise at
// `GROUND_PATCH` metres, more earth on a slope (`GROUND_SLOPE`), takes the colour toward `GROUND_EARTH` or `GROUND_GRASS`
// (their middle taken out, so the mean stays) by `GROUND_MIX`; stones of two sizes (`GROUND_STONES` metres across a cell, their
// share of the cells, more on the earth) are brighter on their top and darker round their foot, faded out where a pixel
// spans a quarter of a stone.
const GROUND_PATCH: vec2<f32> = vec2<f32>(2.2, 6.5);
const GROUND_SLOPE: f32 = 0.5;
const GROUND_EARTH: vec3<f32> = vec3<f32>(1.08, 1.03, 0.82);
const GROUND_GRASS: vec3<f32> = vec3<f32>(0.92, 1.05, 0.84);
const GROUND_MIX: f32 = 0.9;
const GROUND_STONES: vec2<f32> = vec2<f32>(0.32, 0.9);
const GROUND_STONE_SHARE: vec2<f32> = vec2<f32>(0.1, 0.05);

fn ground_patches(world: vec3<f32>, n: vec3<f32>, dx: vec3<f32>, dy: vec3<f32>) -> vec3<f32> {
    let noise = 0.55 * value_noise(world / GROUND_PATCH.x) + 0.45 * value_noise(world / GROUND_PATCH.y);
    let earth = smoothstep(0.38, 0.62, noise + GROUND_SLOPE * (1.0 - n.y) * 0.6);
    let middle = 0.5 * (GROUND_EARTH + GROUND_GRASS);
    var tint = mix(GROUND_GRASS, GROUND_EARTH, earth) / middle;
    let footprint = max(length(dx), length(dy));
    var stones = 1.0;
    for (var k = 0; k < 2; k++) {
        let size = GROUND_STONES[k];
        let shown = 1.0 - smoothstep(0.1 * size, 0.25 * size, footprint);
        if (shown <= 0.0) {
            continue;
        }
        let p = world / size;
        let cell = floor(p);
        let here = hash(cell);
        if (here < GROUND_STONE_SHARE[k] * (0.4 + 1.2 * earth)) {
            let centre = 0.3 + 0.4 * vec3<f32>(hash(cell + 1.7), hash(cell + 3.1), hash(cell + 5.3));
            let radius = mix(0.14, 0.3, hash(cell + 9.1));
            let at = (fract(p) - centre) / radius;
            let inside = dot(at, at);
            if (inside < 1.6) {
                let cap = clamp(1.0 - inside, 0.0, 1.0);
                let lit = 0.7 + 0.8 * cap + 0.25 * dot(at, -frame.sun.xyz);
                let foot = 1.0 - 0.45 * smoothstep(0.8, 1.6, inside) * (1.0 - smoothstep(1.0, 1.6, inside));
                stones *= mix(1.0, select(foot, lit, inside < 1.0), shown);
            }
        }
    }
    return mix(vec3<f32>(1.0), tint, GROUND_MIX) * stones;
}

// The off-road ground's texture colour `surface` over its local mean (the texture `GROUND_MEAN` levels blurrier): its pale
// texels (dried straw, white at a grazing view) past `GROUND_PALE` times the mean's brightness are eased to the power
// `GROUND_PALE_FLAT` of the excess, and the factor is returned. The calming works on the verge only: it fades out between
// `GROUND_CALM` metres from the eye, the hill beyond keeps its look.
const GROUND_MEAN: f32 = 5.0;
const GROUND_CALM: vec2<f32> = vec2<f32>(12.0, 30.0);
const GROUND_PALE: f32 = 1.1;
const GROUND_PALE_FLAT: f32 = 0.3;

fn ground_near(world: vec3<f32>) -> f32 {
    return 1.0 - smoothstep(GROUND_CALM.x, GROUND_CALM.y, distance(world, frame.eye.xyz));
}

// The hillside (`GROUND_CLUMP_AT` metres from the eye and beyond) is calmed too: its texture is read `GROUND_FAR_BLUR` levels
// blurrier, its pale texels eased, the scan's colour ratio's chroma and fine grain eased by the shares `GROUND_FAR_CALM`
// (pixel-scale pink and green speckle), and `ground_clumps` brings the variation back at the scale of grass clumps.
const GROUND_CLUMP_AT: vec2<f32> = vec2<f32>(8.0, 18.0);
const GROUND_FAR_BLUR: f32 = 1.5;
const GROUND_FAR_CALM: vec2<f32> = vec2<f32>(0.6, 0.6);

fn ground_far(world: vec3<f32>) -> f32 {
    // Snow stages' ground (`Stage::snow_ground`, `frame.enhance.w`) is left as it was.
    return (1.0 - frame.enhance.w) * smoothstep(GROUND_CLUMP_AT.x, GROUND_CLUMP_AT.y, distance(world, frame.eye.xyz));
}

fn ground_pale_ease(surface: vec3<f32>, mean: vec3<f32>) -> f32 {
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let ratio = dot(surface, luma) / max(dot(mean, luma), 1e-3);
    let eased = GROUND_PALE * pow(max(ratio / GROUND_PALE, 1.0), GROUND_PALE_FLAT);
    return select(1.0, eased / ratio, ratio > GROUND_PALE);
}

// Whether the face is rock, stone wall or concrete, 0 to 1: its `surface` colour (the stage's own texture) is grey or cool
// (its red and green over its blue, against its brightest channel, under `GROUND_ROCK_AT`; soil and grass are warm or
// green) and the face is steep (its facet's up component, under `GROUND_STEEP`: cuttings, rock walls, parapets). A flat
// grey verge is gravel, and keeps its tint.
const GROUND_ROCK_AT: vec2<f32> = vec2<f32>(0.06, 0.14);
const GROUND_STEEP: vec2<f32> = vec2<f32>(0.75, 0.97);
// How much of its colour rock gives up for grey: its stage texture's cast and the lit colour's tint are mostly not its own.
const GROUND_ROCK_GREY: f32 = 0.85;

fn ground_rock(surface: vec3<f32>, facet: f32) -> f32 {
    let top = max(max(surface.r, surface.g), surface.b);
    let cool = 1.0 - smoothstep(GROUND_ROCK_AT.x, GROUND_ROCK_AT.y, (max(surface.r, surface.g) - surface.b) / max(top, 0.05));
    return cool * (1.0 - smoothstep(GROUND_STEEP.x, GROUND_STEEP.y, facet));
}

// Grass clumps on the far ground (`ground_far`): value noise at `GROUND_CLUMP_SCALES` metres (each octave fades out where a
// pixel spans a third of it) decides where the tufts stand (`t`) and, at 2.6 m, which are green and which dry. Tufts take the
// moss green `GROUND_CLUMP_GREEN` or grey-green `GROUND_CLUMP_DRY` (luma-neutral factors, by `GROUND_CLUMP_TINT`), their
// gaps are `GROUND_CLUMP_GAP` darker (`GROUND_CLUMP_GAIN` keeps the mean), and the lumps' height is lit by the sun: the
// slope toward it, by a step of `GROUND_CLUMP_STEP` metres, by `GROUND_CLUMP_RELIEF`. Returns the factor on the colour.
const GROUND_CLUMP_SCALES: vec3<f32> = vec3<f32>(1.5, 0.7, 0.32);
const GROUND_CLUMP_GREEN: vec3<f32> = vec3<f32>(0.84, 1.06, 0.76);
const GROUND_CLUMP_DRY: vec3<f32> = vec3<f32>(1.0, 1.0, 0.94);
const GROUND_CLUMP_TINT: f32 = 0.6;
const GROUND_CLUMP_GAP: f32 = 0.3;
const GROUND_CLUMP_GAIN: f32 = 1.13;
const GROUND_CLUMP_STEP: f32 = 0.12;
const GROUND_CLUMP_RELIEF: f32 = 0.3;

fn ground_clump_field(world: vec3<f32>, foot: f32) -> f32 {
    var sum = 0.0;
    for (var k = 0; k < 3; k++) {
        let scale = GROUND_CLUMP_SCALES[k];
        let weight = (1.0 - smoothstep(0.15, 0.4, foot / scale)) * (0.6 - 0.15 * f32(k));
        sum += weight * (value_noise(world / scale + f32(k) * 17.3) - 0.5);
    }
    return sum;
}

fn ground_clumps(world: vec3<f32>, n: vec3<f32>, dx: vec3<f32>, dy: vec3<f32>, sun_seen: f32, far: f32) -> vec3<f32> {
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let foot = max(length(dx), length(dy));
    let here = ground_clump_field(world, foot);
    let t = smoothstep(0.25, 0.75, 0.5 + 1.6 * here);
    let green_at = smoothstep(0.3, 0.7, value_noise(world / 2.6 + 5.1));
    let green = mix(GROUND_CLUMP_DRY, GROUND_CLUMP_GREEN, green_at);
    var tint = mix(GROUND_CLUMP_DRY, green, t);
    tint = mix(vec3<f32>(1.0), tint / dot(tint, luma), GROUND_CLUMP_TINT);
    let to_sun = -frame.sun.xyz;
    var across = to_sun - n * dot(to_sun, n);
    across = across / max(length(across), 1e-3);
    let toward = ground_clump_field(world + across * GROUND_CLUMP_STEP, foot);
    let lit = clamp((here - toward) / GROUND_CLUMP_STEP * GROUND_CLUMP_RELIEF * 0.4, -0.5, 0.5);
    let shade = GROUND_CLUMP_GAIN * (1.0 - GROUND_CLUMP_GAP * (1.0 - t)) * (1.0 + lit * sun_seen);
    return mix(vec3<f32>(1.0), tint * shade, far);
}

// The colour `colour_in` at `world` of a face with unit normal `n` and derivatives `dx` and `dy` (taken in uniform
// control flow) under the ground scan, `sun_seen` of the sun reaching it, over the stage's own `surface` colour.
fn ground_scan(colour_in: vec3<f32>, world: vec3<f32>, n: vec3<f32>, dx: vec3<f32>, dy: vec3<f32>, sun_seen: f32, surface: vec3<f32>) -> vec3<f32> {
    var w = n * n;
    w = w * w;
    w = w * w;
    w = w / (w.x + w.y + w.z);
    let mean = textureSampleLevel(road_scans, road_scan_sampler, vec2<f32>(0.5), 3, 20.0);
    let mean_normal = textureSampleLevel(road_scans, road_scan_sampler, vec2<f32>(0.5), 4, 20.0);
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let far = ground_far(world);
    var ratio = vec3<f32>(1.0);
    var hollows = 0.0;
    var tilt = vec3<f32>(0.0);
    for (var k = 0; k < 2; k++) {
        let scale = GROUND_TILES[k];
        let p = world / scale;
        let px = dx * GROUND_SHARP / scale;
        let py = dy * GROUND_SHARP / scale;
        var colour = vec3<f32>(0.0);
        var softer = vec3<f32>(0.0);
        var height = 0.0;
        var kept = 0.0;
        for (var i = 0; i < 3; i++) {
            if (w[i] < 0.02) {
                continue;
            }
            // The plane across axis i, each offset so the projections do not repeat each other's texels.
            var q: vec2<f32>;
            var gx: vec2<f32>;
            var gy: vec2<f32>;
            if (i == 0) {
                q = p.zy; gx = px.zy; gy = py.zy;
            } else if (i == 1) {
                q = p.xz; gx = px.xz; gy = py.xz;
            } else {
                q = p.xy; gx = px.xy; gy = py.xy;
            }
            q += vec2<f32>(0.31, 0.57) * f32(i + 3 * k);
            if (k == 1) {
                let r = mat2x2<f32>(GROUND_TURN.x, GROUND_TURN.y, -GROUND_TURN.y, GROUND_TURN.x);
                q = r * q;
                gx = r * gx;
                gy = r * gy;
            }
            let scan = textureSampleGrad(road_scans, road_scan_sampler, q, 3, gx, gy);
            colour += w[i] * scan.rgb;
            height += w[i] * scan.a;
            kept += w[i];
            if (k == 0) {
                softer += w[i] * textureSampleGrad(road_scans, road_scan_sampler, q, 3, gx * GROUND_SOFT, gy * GROUND_SOFT).rgb;
                let nor = textureSampleGrad(road_scans, road_scan_sampler, q, 4, gx, gy);
                let side = select(-1.0, 1.0, n[i] >= 0.0);
                tilt += w[i] * (plane_normal(i, scan_normal(nor.xy), side) - plane_normal(i, scan_normal(mean_normal.xy), side));
            }
        }
        kept = max(kept, 1e-3);
        let each = select(colour, softer, k == 0) / kept / max(mean.rgb, vec3<f32>(1e-3));
        let bright = dot(each, luma);
        ratio *= pow(vec3<f32>(bright), vec3<f32>(GROUND_DETAIL[k])) * pow(each / max(bright, 1e-3), vec3<f32>(select(GROUND_CHROMA, GROUND_CHROMA * (1.0 - GROUND_FAR_CALM.x * far), far > 0.0)));
        if (k == 0) {
            // The fine grain over its softer self, its contrast raised: the scan's blades are faint at 1024 texels.
            ratio *= pow(colour / max(softer, vec3<f32>(1e-3)), vec3<f32>(select(GROUND_GRAIN, GROUND_GRAIN * (1.0 - GROUND_FAR_CALM.y * far), far > 0.0)));
            hollows = (mean.a - height / kept) * GROUND_HOLLOWS;
            tilt = tilt / kept;
        }
    }
    ratio *= ground_patches(world, n, dx, dy);
    // A bright, grey surface takes only the scan's brightness, and not much darker than itself.
    let top = max(surface.r, max(surface.g, surface.b));
    let grey = 1.0 - smoothstep(0.1, 0.25, (top - min(surface.r, min(surface.g, surface.b))) / max(top, 1e-3));
    let bright = smoothstep(0.55, 0.75, dot(surface, luma));
    let level = dot(ratio, luma);
    // Rock, stone and concrete (`ground_rock`) take the scan's and the earth patches' brightness only, with no hue of their own.
    let facet = abs(normalize(cross(dx, dy)).y);
    let rock = ground_rock(surface, facet);
    let tint = mix(mix(ratio, vec3<f32>(level), rock), vec3<f32>(max(level, GROUND_FLOOR)), grey * bright);
    let to_sun = -frame.sun.xyz;
    let lit = clamp(dot(tilt, to_sun) / max(dot(n, to_sun), 0.15), -1.0, 1.0);
    let relief = 1.0 + lit * sun_seen * GROUND_RELIEF * smoothstep(0.02, 0.15, to_sun.y);
    var scanned = colour_in * clamp(tint * relief * (1.0 - hollows), vec3<f32>(0.3), vec3<f32>(2.0));
    scanned = mix(scanned, vec3<f32>(dot(scanned, luma)), rock * GROUND_ROCK_GREY);
    // The far ground's grass clumps, not on bright grey ground or rock.
    let shown = far * (1.0 - grey * bright) * (1.0 - rock);
    if (shown > 0.0) {
        scanned = scanned * ground_clumps(world, n, dx, dy, sun_seen, shown);
    }
    return scanned;
}

// A scan normal's stage direction from its stored x and y (OpenGL: y up the image, which is the stage's -z).
fn scan_normal(xy: vec2<f32>) -> vec3<f32> {
    let t = xy * 2.0 - 1.0;
    return normalize(vec3<f32>(t.x, sqrt(max(1.0 - dot(t, t), 0.0)), -t.y));
}

// Backdrop detail (renderer choice): the backdrop's texture is stretched over kilometres, a few metres a texel, so
// next to the stage it reads as a blur. Its own texture, tiled flat in stage space at `BACKDROP_TILES` metres, adds
// the fine structure it lacks: each tiling's brightness over the texture's mean (its last mip) scales the colour by up
// to `BACKDROP_DETAIL`, and the tilings fade out where a pixel spans an eighth of a tile (the mips already blur them).
// The flat tiling stretches down a steep face into streaks, so it fades out there too: none on the photo panoramas
// some stages stand upright round the stage in place of terrain (462 Odry, 138 RSI, 166 Hradek).
const BACKDROP_DETAIL: f32 = 0.6;
const BACKDROP_TILES: vec2<f32> = vec2<f32>(12.0, 47.0);

fn backdrop_detail(world: vec3<f32>) -> f32 {
    let dx = dpdx(world);
    let dy = dpdy(world);
    let footprint = max(length(dx), length(dy));
    let n = cross(dx, dy);
    let flat = smoothstep(0.35, 0.6, abs(n.y) / max(length(n), 1e-12));
    let mean = max(dot(textureSampleLevel(texture0, texture_sampler, vec2<f32>(0.5), 16.0).rgb, vec3<f32>(0.333)), 1e-3);
    var detail = 0.0;
    for (var i = 0; i < 2; i++) {
        let tile = BACKDROP_TILES[i];
        let shown = 1.0 - smoothstep(0.06 * tile, 0.125 * tile, footprint);
        let texel = textureSample(texture0, texture_sampler, world.xz / tile + vec2<f32>(0.37 * f32(i))).rgb;
        detail += 0.5 * shown * flat * (dot(texel, vec3<f32>(0.333)) / mean - 1.0);
    }
    return max(1.0 + BACKDROP_DETAIL * detail, 0.2);
}

// How much the sun's cascades and far layer cover `world`: 1 near the eye, fading out over the far layer's outer fifth
// (`sun_splits.w`), 0 when the sun is off.
fn sun_reach(world: vec3<f32>) -> f32 {
    if (frame.sun_shadow.x == 0.0) {
        return 0.0;
    }
    let far = frame.sun_splits.w;
    return 1.0 - smoothstep(0.8 * far, far, distance(world, frame.eye.xyz));
}

// 1 in the sun to 0 in the shadow of the stage, from the cascade that holds `world` (`sun_point`): a 4x4 filter. With
// the main view's mask (`SUN_MASK`) the mask's at the pixel, worked out from the depth (`fs_sun_mask`).
fn sun_lit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (SUN_MASK) {
        return textureLoad(sun_mask, pixel, 0).r;
    }
    let c = sun_point(world, n);
    if (c.layer < 0) {
        return 1.0;
    }
    if (frame.sun_shadow.z == 0.0) {
        return texel_filter(c.uv, c.layer, c.depth);
    }
    return overcast_filter(c);
}

// The backdrop is the coarse terrain the stage lies on, and the stage rises tens of metres over it at its edge: its
// lookups sit `BACKDROP_LIFT` metres higher along the sun's ray (at most `BACKDROP_REACH` metres along it, under a low
// sun), so that the stage's edge and trees do not shade it, and hills and ridges farther off still do. The sun's maps
// hold the backdrop lower for the stage (`sun::backdrop_drop`).
const BACKDROP_LIFT: f32 = 80.0;
const BACKDROP_REACH: f32 = 400.0;

fn lifted(world: vec3<f32>) -> vec3<f32> {
    if (stage_frame() || camera_anchored()) {
        return world;
    }
    let along = min(BACKDROP_LIFT / max(-frame.sun.y, 0.01), BACKDROP_REACH);
    return world - frame.sun.xyz * along;
}

// Where `world` falls in the sun's maps: the cascade that holds it (past the last, the far layer), looked up a little
// off the surface along `n` so that it does not shadow itself; layer -1 outside them.
struct SunPoint {
    uv: vec2<f32>,
    layer: i32,
    depth: f32,
}

fn sun_point(world: vec3<f32>, n: vec3<f32>) -> SunPoint {
    let reach = distance(world, frame.eye.xyz);
    var i = select(select(2, 1, reach <= frame.sun_splits.y), 0, reach <= frame.sun_splits.x);
    if (reach > frame.sun_splits.z) {
        i = 3;
    }
    let p = frame.sun_matrices[i] * vec4<f32>(lifted(world) + n * frame.sun_texels[i] * 1.5, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0) {
        return SunPoint(uv, -1, p.z);
    }
    return SunPoint(uv, i, p.z);
}

// Under an overcast sky (`sun_shadow.z`): taps `sun_shadow.y` apart, a wide soft shadow.
fn overcast_filter(c: SunPoint) -> f32 {
    var lit = 0.0;
    for (var row = 0; row < 4; row++) {
        for (var column = 0; column < 4; column++) {
            let x = f32(column) - 1.5;
            let y = f32(row) - 1.5;
            let at = c.uv + vec2<f32>(x, y) * frame.sun_shadow.y;
            lit += textureSampleCompareLevel(sun_map, shadow_sampler, at, c.layer, c.depth);
        }
    }
    return lit / 16.0;
}

// The stage's shadow on a lit draw seen from outside (the car's body, the characters), 1 in the sun: the sun's cascades
// where they reach and the sun can reach the face (trees and banks cast, so the light dapples over the paint), fading to
// the surroundings cube's sun occlusion `occlusion` (`surroundings`) beyond them, there in the world layer's shadow, and
// under an overcast sky.
fn stage_sun(world: vec3<f32>, n: vec3<f32>, occlusion: f32) -> f32 {
    let near = sun_reach(world);
    if (frame.sun_shadow.z != 0.0 || dot(n, -frame.sun.xyz) <= 0.0) {
        return occlusion;
    }
    let far = world_lit(world, n) * occlusion;
    if (near == 0.0) {
        return far;
    }
    return mix(far, sun_lit(world, n), near);
}

// 1 in the sun to 0 in the shadow of the stage and its backdrop, past the cascades' reach (`sun_reach`): from the
// world layer (`sun::world`, 1 without it), a few metres a texel.
const WORLD_LAYER: i32 = 5;

fn world_lit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (frame.world_shadow.x == 0.0) {
        return 1.0;
    }
    let p = frame.world_matrix * vec4<f32>(lifted(world) + n * frame.world_shadow.y * 1.5, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0) {
        return 1.0;
    }
    return texel_filter(uv, WORLD_LAYER, p.z);
}

// `sun_lit`'s 4x4 filter for steps of one texel (not overcast) in 3x3 taps. Its 16 bilinear taps weigh the 5 texels
// along each axis from the second before `uv`'s nearest texel corner by 1 - g, 1, 1, 1 and g (g the fraction past
// that corner); three bilinear taps reproduce them: the first pair weighted 2 - g with its second texel's share
// 1 / (2 - g), the middle pair 2 at its centre, and the last texel g.
fn texel_filter(uv: vec2<f32>, layer: i32, depth: f32) -> f32 {
    let size = vec2<f32>(textureDimensions(sun_map));
    let corner = floor(uv * size);
    let g = uv * size - corner;
    let w0 = 2.0 - g;
    let a0 = (corner - 1.5 + 1.0 / w0) / size;
    let a1 = (corner + 1.0) / size;
    let a2 = (corner + 2.5) / size;
    let r0 = w0.x * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a0.x, a0.y), layer, depth)
        + 2.0 * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a1.x, a0.y), layer, depth)
        + g.x * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a2.x, a0.y), layer, depth);
    let r1 = w0.x * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a0.x, a1.y), layer, depth)
        + 2.0 * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a1.x, a1.y), layer, depth)
        + g.x * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a2.x, a1.y), layer, depth);
    let r2 = w0.x * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a0.x, a2.y), layer, depth)
        + 2.0 * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a1.x, a2.y), layer, depth)
        + g.x * textureSampleCompareLevel(sun_map, shadow_sampler, vec2<f32>(a2.x, a2.y), layer, depth);
    return (w0.y * r0 + 2.0 * r1 + g.y * r2) / 16.0;
}

// What the sun does to an unlit stage draw's colour (renderer addition), as a factor: its shadow darkens by
// `frame.sun_shadow.x` in full shadow and on faces turned from the sun (terrain only; foliage cards take light from
// both sides), less where the point sees little sky (`open_sky`) and on foliage, whose cards shadow each
// other. Sunlit plants (`foliage.rs`, `specular.z` 1) seen against the sun let it through their leaves (thin-surface
// transmission, ours): the sunlight's colour filtered by the leaf's (`albedo`, its hue at full strength), in a
// lobe about the sun (`TRANSLUCENCY`, `pow 4` of the view-to-sun angle), fading as the sun nears the horizon and at
// most `GLOW_MOST`, so that a low sun in the view does not wash the canopy out.
const TRANSLUCENCY: f32 = 1.2;
const GLOW_MOST: f32 = 0.8;

fn sun_light(v: VertexOut, front: bool, open_sky: f32, albedo: vec3<f32>) -> vec3<f32> {
    let has_normal = dot(v.normal, v.normal) > 1e-8;
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
    let overcast = frame.sun_shadow.z != 0.0;
    // Under the relight (`relit`) the vertex light already turns from the sun. Faces turned from it skip the cascades.
    var facing = 1.0;
    if (has_normal && mode.cutout == 0u && !overcast && frame.relight.w == 0.0) {
        facing = smoothstep(-0.05, 0.25, dot(n, -frame.sun.xyz));
    }
    var lit = 0.0;
    if (facing > 0.0) {
        lit = sun_lit(v.world, n) * facing;
    }
    let receive = select(1.0, 0.5, mode.cutout != 0u) * mix(0.4, 1.0, open_sky);
    var glow = vec3<f32>(0.0);
    if (!overcast) {
        glow = leaf_glow(v, lit, albedo);
    }
    return (1.0 - frame.sun_shadow.x * (1.0 - lit) * receive) * (1.0 + glow);
}

// A sunlit plant's glow against the sun (`sun_light`), `lit` its share of the sun.
fn leaf_glow(v: VertexOut, lit: f32, albedo: vec3<f32>) -> vec3<f32> {
    if (mode.cutout == 0u || v.specular.z <= 0.5) {
        return vec3<f32>(0.0);
    }
    let against = max(dot(normalize(v.world - frame.eye.xyz), -frame.sun.xyz), 0.0);
    let high = smoothstep(0.03, 0.25, -frame.sun.y);
    let hue = albedo / max(max(albedo.r, max(albedo.g, albedo.b)), 1e-3);
    return min(TRANSLUCENCY * lit * pow(against, 4.0) * high, GLOW_MOST) * sunlight() * hue;
}

@vertex
fn vs_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    return frame.shadow_matrix * vec4<f32>(placed(v).position, 1.0);
}

// 0 in the sun to 1 in the car's shadow: a filter over the shadow map as wide as the sun's penumbra. Points past the
// map's far side (the ground beyond the car) compare at its far plane, so they are shadowed wherever the car is in the
// map.
fn car_shadow(world: vec3<f32>) -> f32 {
    if (frame.shadow.y == 0.0) {
        return 0.0;
    }
    let p = frame.shadow_matrix * vec4<f32>(world, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z < 0.0) {
        return 0.0;
    }
    // Beyond the box every tap compares sunlit against the cleared map.
    if (outside(uv, frame.shadow_box)) {
        return 0.0;
    }
    let reference = min(p.z, 1.0) - 0.002;
    // Where every tap's texels lie behind the reference, or all in front of it, the filter's answer is known.
    let range = shadow_range(0, uv);
    if (reference <= range.x) {
        return 0.0;
    }
    if (reference > range.y) {
        return 1.0;
    }
    // The sun's disc blurs the shadow by its casters' distance (PCSS as `soft_filter`): their mean depth from a 3x3
    // search over the filter's reach. The map's depth and uv both span its width, so the penumbra's radius in uv is
    // `SUN_RADIUS` times the depth between. At least a texel, at most the reach the range holds; taps a texel apart, at
    // most `CAR_SOFT_TAPS` a side (a fixed 4x4 four centimetres apart showed as steps in the cabin).
    let size = vec2<f32>(textureDimensions(shadow_map));
    let reach = 1.5 * frame.shadow.z;
    var sum = 0.0;
    var casters = 0.0;
    for (var row = 0; row < 3; row++) {
        for (var column = 0; column < 3; column++) {
            let at = uv + (vec2<f32>(f32(column), f32(row)) - 1.0) * reach;
            let d = textureLoad(shadow_map, clamp(vec2<i32>(at * size), vec2<i32>(0), vec2<i32>(size) - 1), 0, 0);
            if (d < reference) {
                sum += d;
                casters += 1.0;
            }
        }
    }
    let between = select(0.0, reference - sum / max(casters, 1.0), casters > 0.0);
    let radius = clamp(between * SUN_RADIUS, 1.0 / size.x, reach);
    let taps = min(i32(ceil(2.0 * radius * size.x)) + 1, CAR_SOFT_TAPS);
    let half = 0.5 * f32(taps - 1);
    var sunlit = 0.0;
    for (var row = 0; row < taps; row++) {
        for (var column = 0; column < taps; column++) {
            let at = uv + (vec2<f32>(f32(column), f32(row)) - half) / half * radius;
            sunlit += textureSampleCompareLevel(shadow_map, shadow_sampler, at, 0, reference);
        }
    }
    return 1.0 - sunlit / f32(taps * taps);
}

const CAR_SOFT_TAPS: i32 = 9;

// The least and greatest depth of the car's shadow map's `layer` over every texel its filter centred at `uv` reads
// (`shadow_ranges`, `renderer::shadows::ShadowRange`): a compare at or before the least passes on every tap, past the
// greatest fails on all.
const RANGE_CELL: f32 = 4.0;

fn shadow_range(layer: i32, uv: vec2<f32>) -> vec2<f32> {
    let cells = vec2<i32>(textureDimensions(shadow_ranges));
    let cell = clamp(vec2<i32>(floor(uv * vec2<f32>(textureDimensions(shadow_map)) / RANGE_CELL)), vec2<i32>(0), cells - 1);
    return textureLoad(shadow_ranges, cell, layer, 0).xy;
}

// Whether `uv` lies outside the rectangle `uv_box` (low xy, high xy).
fn outside(uv: vec2<f32>, uv_box: vec4<f32>) -> bool {
    return any(uv < uv_box.xy) || any(uv > uv_box.zw);
}

@vertex
fn vs_ground(v: VertexIn) -> @builtin(position) vec4<f32> {
    return frame.ground_matrix * vec4<f32>(placed(v).position, 1.0);
}

// 0 to 1 under the car: the share of a 6x6 filter around the point where the car is above it, within the near
// reach, plus half the share between the near and far reach.
fn car_contact(world: vec3<f32>) -> f32 {
    if (frame.shadow.y == 0.0) {
        return 0.0;
    }
    let p = frame.ground_matrix * vec4<f32>(world, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z < 0.0 || p.z > 1.0) {
        return 0.0;
    }
    // Beyond the box every tap reads the cleared map: nothing above.
    if (outside(uv, frame.ground_box)) {
        return 0.0;
    }
    // Where nothing of the car is above any tap's texels, nothing is within the reaches either.
    if (p.z <= shadow_range(1, uv).x) {
        return 0.0;
    }
    var covered = 0.0;
    for (var row = 0; row < 6; row++) {
        for (var column = 0; column < 6; column++) {
            let x = f32(column) - 2.5;
            let y = f32(row) - 2.5;
            let at = uv + vec2<f32>(x, y) * frame.ground.y;
            // Each compare is 1 where nothing of the car is above that depth; with nothing above, nothing is within
            // the reaches either.
            let above = 1.0 - textureSampleCompareLevel(shadow_map, shadow_sampler, at, 1, p.z);
            if (above > 0.0) {
                let near = 1.0 - textureSampleCompareLevel(shadow_map, shadow_sampler, at, 1, p.z - frame.ground.z);
                let far = 1.0 - textureSampleCompareLevel(shadow_map, shadow_sampler, at, 1, p.z - frame.ground.w);
                covered += above - 0.5 * (near + far);
            }
        }
    }
    return covered / 36.0;
}

// What the car reflects along `dir`: the stage around it from the reflection cube, whose texels are already through
// the tone curve (so they are taken back through its inverse), or the sky before the cube is drawn.
fn reflected(dir: vec3<f32>, level: f32) -> vec3<f32> {
    if (frame.reflection.x == 0.0) {
        if (sky_on()) {
            return sky_reflected(dir, level / 7.0);
        }
        return sky(dir, 1.0);
    }
    if (LIGHTING && frame.reflection.z != 0.0) {
        return ungraded(surrounding(dir, level, frame.car_body[3].xyz));
    }
    return ungraded(textureSampleLevel(reflection_cube, reflection_sampler, dir, level).rgb);
}

// Linear fog by the distance to the eye, covering at most `frame.fog_color.w`. Under fog (the section's or ours) with
// `Enhance::lighting` the height fog alone (`height_fog.rs`), the same over the stage, the backdrop and the sky, so the
// land fades into the sky as into a volume of fog, thinning with the height, rather than to an edge. The backdrop takes
// it at its own distance, as the stage does: some of its tree lines stand 50 to 100 m from the road (122 Bruchsal).
fn fogged(colour: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let volume = LIGHTING && frame.enhance.x != 0.0 && frame.skylight.w != 0.0 && frame.height_fog.x > 0.0;
    if (volume) {
        if (camera_anchored()) {
            return sky_misted(colour, normalize(world));
        }
        let d = max(distance(world, frame.eye.xyz), 1e-3);
        let dir = (world - frame.eye.xyz) / d;
        return mix(colour, height_fog_light(dir, haze_colour(dir)), height_fog_cover_at(world.y, d));
    }
    let range = max(frame.fog_range.y - frame.fog_range.x, 1.0);
    var f = vec3<f32>(clamp((distance(world, frame.eye.xyz) - frame.fog_range.x) / range, 0.0, 1.0));
    var haze = frame.fog_color.rgb;
    var seen = world;
    if (frame.enhance.x != 0.0 && !camera_anchored()) {
        // The physically based sky's haze is the section's own (`skylight::FOG_COVER`), so it replaces the linear fog.
        // Otherwise never thinner than the linear fog, whose end hides what lies past it. A clear sky's haze takes the
        // backdrop nearer than it lies (`sky_share`); fog takes it where it lies, so that it fades out behind the stage.
        if (sky_on() && frame.sun.w != 0.0) {
            seen = sky_seen(world);
            let h = sky_haze(seen);
            f = h[0];
            haze = h[1];
        } else {
            f = max(aerial(distance(world, frame.eye.xyz) / frame.fog_range.y), f);
            haze = haze_colour(normalize(world - frame.eye.xyz));
        }
    }
    let hazed = mix(colour, haze, frame.fog_color.w * f);
    if (LIGHTING && frame.height_fog.x > 0.0 && !camera_anchored()) {
        return height_fogged(hazed, seen, haze);
    }
    return hazed;
}

// Aerial perspective (renderer addition, `Enhance::lighting`): the haze thickens exponentially from the eye rather than
// linearly from the fog's start, reaching 90% of the linear fog's cover at its end (`AERIAL_DENSITY`), and blue fades first
// (`EXTINCTION`), so the middle distance already softens and the far distance goes blue-grey. `fogged` keeps it at
// least the linear fog's cover.
const EXTINCTION: vec3<f32> = vec3<f32>(0.85, 1.0, 1.2);
// Optical depth at the fog's end: 2.3 covers 90% of the linear fog's cover there.
const AERIAL_DENSITY: f32 = 2.3;

fn aerial(depth: f32) -> vec3<f32> {
    return 1.0 - exp(-AERIAL_DENSITY * max(depth, 0.0) * EXTINCTION);
}

// The haze's colour along `dir`: the sky behind it at the horizon (so it matches the sky's gradient, warmer and
// brighter toward the sun), plus light the haze scatters forward from the sun (a Henyey-Greenstein lobe).
fn haze_colour(dir: vec3<f32>) -> vec3<f32> {
    let level = normalize(vec3<f32>(dir.x, clamp(dir.y, 0.02, 0.3), dir.z));
    var colour = sky_light(level);
    if (frame.sun.w != 0.0 && frame.night.x == 0.0) {
        let g = 0.7;
        let mu = dot(dir, -frame.sun.xyz);
        let phase = (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * mu, 1.5) / 12.0;
        let light = sunlight();
        // With the physically based sky, none once the sun has set: `frame.sun` is then toward the moon.
        let up = select(1.0, smoothstep(-0.02, 0.03, frame.skylight.y), sky_on());
        colour = colour + 0.25 * min(phase, 2.0) * up * light / max(max(light.r, light.g), light.b);
    }
    return colour;
}

// The scene is linear colour for the post pass (`post.wgsl`), which applies the tone curve: `graded` takes the display
// colours the stage works in there, unclipped, and `ungraded` back.
fn graded(colour: vec3<f32>) -> vec3<f32> {
    return pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2));
}

fn ungraded(colour: vec3<f32>) -> vec3<f32> {
    return pow(max(colour, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

// One triangle covering the screen.
@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let ndc = vec2<f32>(f32(i == 1u) * 4.0 - 1.0, f32(i == 2u) * 4.0 - 1.0);
    var out: SkyOut;
    // At the far plane (depth 0: the depth is reversed, `coords::perspective`).
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

// The view direction from the rotation-only (camera-anchored) clip matrix: the fog colour at and below the horizon,
// the zenith colour overhead, and a soft glow toward the sun under a clear sky.
@fragment
fn fs_sky(v: SkyOut) -> @location(0) vec4<f32> {
    let far = frame.inverse_view_projection * vec4<f32>(v.ndc, 0.0, 1.0);
    let near = frame.inverse_view_projection * vec4<f32>(v.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - near.xyz / near.w);
    // Below the horizon the physically based sky's tables hold the planet's ground, a grey band wherever the eye sees
    // past the backdrop (from a crest, or over the photo panoramas some stages stand round the stage, 462 Odry): the
    // horizon's light goes on there instead, as without them, and no stars, moon or sun.
    var base = sky(dir, 0.0);
    if (sky_on() && dir.y < SKY_HORIZON) {
        base = sky_light(normalize(vec3<f32>(dir.x, SKY_HORIZON, dir.z)));
    }
    var colour: vec3<f32>;
    if (frame.haze.x == 1.0) {
        colour = cloud_volume_over(base, v.ndc);
    } else if (frame.haze.x == 2.0) {
        colour = cloud_dome_over(base, dir);
    } else {
        colour = base;
    }
    if (LIGHTING && frame.height_fog.x > 0.0) {
        colour = sky_misted(colour, dir);
    }
    return vec4<f32>(graded(colour), 1.0);
}

// The sky colour along `dir` (renderer choice; the game's `SkyDome` scattering has unproven inputs). Clear sections
// (`frame.sun.w` 1): the hue goes from a pale horizon (the section's fog colour, halfway to white) to the blue zenith
// (`Atmosphere::zenith`) with the elevation, and the brightness follows the Preetham, Shirley and Smits (1999) daylight
// distribution relative to the zenith (`daylight`, turbidity `TURBIDITY`), capped at 3 so the sky next to the sun stays
// blue-white: brighter around the sun and toward the horizon, darkest about 90 degrees from the sun. Near a low sun the light warms to the colour of the sunlight that
// reaches the eye (`sunlight`), and the sun's disc is drawn in that colour. Fogged sections: the fog colour, a little
// brighter overhead, with a faint bright patch where the sun is. Below the horizon (for reflections, `ground` 1) it
// darkens toward the ground.
fn sky(dir: vec3<f32>, ground: f32) -> vec3<f32> {
    var colour = sky_light(dir);
    if (frame.night.x != 0.0) {
        colour = night_sky(dir);
    } else if (frame.sun.w != 0.0) {
        let mu = clamp(dot(dir, -frame.sun.xyz), -1.0, 1.0);
        let light = sunlight();
        let sun_colour = light / max(max(light.r, light.g), light.b);
        // The disc, about a degree across (twice the real one, so that it reads at screen sizes), and a tight glow
        // around it, as a camera sees the sun.
        let toward = max(mu, 0.0);
        var glow = 0.9 * pow(toward, 3000.0) + 0.3 * pow(toward, 200.0);
        var disc = smoothstep(0.99988, 0.99993, mu);
        // With the physically based sky, the sun's true elevation: no disc once it has set, and the light is the moon's.
        if (sky_on() && frame.skylight.y < -0.005) {
            glow = 0.0;
            disc = 0.0;
            if (moon_up()) {
                colour = moon_shown(dir, starry(dir, moon_aureole(dir, colour)));
            }
        }
        // The disc is brighter than white, for the bloom (`post.wgsl`).
        colour = mix(colour + glow * sun_colour, mix(vec3<f32>(1.0), sun_colour, 0.25) * 1.5 * SUN_HDR, disc);
    }
    let below = clamp(-dir.y * 4.0, 0.0, 1.0) * ground;
    return mix(colour, frame.fog_color.rgb * 0.25, below);
}

// How much brighter the sun's disc is than white.
const SUN_HDR: f32 = 4.0;

// `sky` without the sun's disc and glow (also the light the haze takes on, `haze_colour`).
fn sky_light(dir: vec3<f32>) -> vec3<f32> {
    if (frame.night.x != 0.0) {
        return night_sky_light(dir);
    }
    // The tables hold the overcast sky too (`overcast_light` in `skylight.wgsl`).
    if (sky_on()) {
        return sky_table_light(dir);
    }
    let up = clamp(dir.y, 0.0, 1.0);
    let to_sun = -frame.sun.xyz;
    let mu = clamp(dot(dir, to_sun), -1.0, 1.0);
    if (frame.sun.w == 0.0) {
        return frame.fog_color.rgb * (1.0 + 0.1 * sqrt(up) + 0.12 * pow(max(mu, 0.0), 6.0));
    }
    let horizon = mix(frame.fog_color.rgb, vec3<f32>(1.0), 0.5);
    let relative = daylight(up, mu, to_sun.y);
    var colour = mix(horizon, frame.zenith.rgb, pow(up, 0.45)) * (0.85 + 0.15 * min(relative, 3.0));
    // Below the horizon (the backdrop of the car shown alone) the haze stays the plain horizon colour.
    colour = mix(colour, mix(horizon, frame.fog_color.rgb, 0.5), clamp(-dir.y * 4.0, 0.0, 1.0));
    let light = sunlight();
    let sun_colour = light / max(max(light.r, light.g), light.b);
    let warmth = 0.6 * clamp((relative - 2.0) / 6.0, 0.0, 1.0) * (1.0 - clamp(to_sun.y, 0.0, 1.0));
    colour = mix(colour, colour * sun_colour, warmth);
    // At dusk (`relight.rs`, ours) the sky dims and turns bluer with the ambient light, less toward the sun.
    let level = frame.relight_light.w * frame.dusk.rgb;
    return colour * mix(level, vec3<f32>(1.0), 0.6 * pow(max(mu, 0.0), 4.0) * (1.0 - up));
}

const TURBIDITY: f32 = 2.5;

// The Perez sky distribution with coefficients A to E, for a view at zenith cosine `cos_theta` and angle `gamma` from
// the sun.
fn perez(cos_theta: f32, gamma: f32, a: f32, b: f32, c: f32, d: f32, e: f32) -> f32 {
    let cos_gamma = cos(gamma);
    return (1.0 + a * exp(b / max(cos_theta, 0.01))) * (1.0 + c * exp(d * gamma) + e * cos_gamma * cos_gamma);
}

// Preetham's sky luminance for a view with sine of elevation `up` and cosine `mu` to the sun, relative to the zenith's,
// for a sun at elevation sine `sun_up`: 1 overhead, about 2 at the horizon and 4 to 10 next to the sun.
fn daylight(up: f32, mu: f32, sun_up: f32) -> f32 {
    let t = TURBIDITY;
    let a = 0.1787 * t - 1.4630;
    let b = -0.3554 * t + 0.4275;
    let c = -0.0227 * t + 5.3251;
    let d = 0.1206 * t - 2.5771;
    let e = -0.0670 * t + 0.3703;
    let sun_zenith = min(acos(clamp(sun_up, 0.0, 1.0)), 1.55);
    return perez(up, acos(mu), a, b, c, d, e) / perez(1.0, sun_zenith, a, b, c, d, e);
}

// The colour of the sunlight after crossing the air toward the sun: Rayleigh optical depths for red, green and blue
// (5.8, 13.5 and 33.1 per 1000 km over an 8 km scale height) and a little grey haze, over the Kasten and Young air
// mass.
fn sunlight() -> vec3<f32> {
    if (sky_on()) {
        return sky_table_sunlight();
    }
    let y = clamp(-frame.sun.y, 0.0, 1.0);
    let elevation = degrees(asin(y));
    let mass = 1.0 / (y + 0.50572 * pow(elevation + 6.07995, -1.6364));
    return exp(-vec3<f32>(0.076, 0.138, 0.295) * mass);
}
