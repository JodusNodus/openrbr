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
    // reflected; y 1 in the in-car view (`renderer::Renderer::set_outside_view`).
    reflection: vec4<f32>,
    // x how wet the stage is: 0 dry, 0.5 damp, 1 wet (`road_shine`); y the car's dirt level (`dirt.rs`).
    wetness: vec4<f32>,
    // Renderer additions (`renderer::Enhance`): x 1 to write linear colour for the HDR post pass (`graded`), y 1 for
    // the aerial perspective (`fogged`), z 1 for the flowing water (`water_flowing`), w 1 for the puddles (`wet_plus`).
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
    // The clouds (`clouds.wgsl`, ours): x their cover (0 to 1), y 1 when on, zw their drift, metres a second.
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
    // x 1 with the computed sky occlusion (`Enhance::occlusion`, `sky_occlusion.rs`) in the stage vertices, y unused, z
    // the car's lowest height and w 1 with a car (`ground_bounce`).
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
}

struct Mode {
    second_texture: u32,
    alpha_test: u32,
    alpha_ref: f32,
    mip_bias: f32,
    // Alpha to coverage: the alpha leaves as a sharpened edge, a coverage of 0 to 1 over about a pixel around
    // `alpha_ref` (renderer choice; `renderer::SAMPLE_COUNT`).
    coverage: u32,
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
    // x 1: a dial's emissive mask in `glow_texture` (`DIAL_GLOW`); y 1 when its alpha counts (not DXT1).
    glow: vec4<f32>,
    // x a glass pane's share of the mud film (`stage::Shade::grime`), 0 for none.
    grime: vec4<f32>,
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
// The main view's draws after the depth prepass read the sun's shadow from its mask (`sun_mask.wgsl`), at the pixel
// `fs_main` or `fs_clouded` shades (`renderer::sun_mask`).
override SUN_MASK: bool = false;
// With it, the contact shading at half the size (`fs_contact`, `Enhance::occlusion`).
override CONTACT: bool = false;
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
@group(1) @binding(0) var texture0: texture_2d<f32>;
@group(1) @binding(1) var texture1: texture_2d<f32>;
@group(1) @binding(2) var texture_sampler: sampler;
@group(1) @binding(3) var<uniform> mode: Mode;
@group(1) @binding(4) var shadow_texture: texture_2d<f32>;
@group(1) @binding(5) var specular_texture: texture_2d<f32>;
@group(1) @binding(6) var glow_texture: texture_2d<f32>;
// The screen-space reflections' pass (`fs_reflect`): the resolved frame and its depth.
@group(2) @binding(0) var scene_colour: texture_2d<f32>;
@group(2) @binding(1) var scene_depth: texture_depth_multisampled_2d;
// Its copy (`depth_copy.rs`), cheaper to read scattered, where the renderer makes one; else 1 texel.
@group(2) @binding(2) var scene_depth_copy: texture_2d<f32>;
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
    // How much of the wet film the ground takes (`stage::Vertex::wet`).
    @location(10) wet: f32,
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
    // The light it gets off the stage per unit of the sun's and of the sky's (`bounce_light`).
    @location(8) bounce_sun: vec3<f32>,
    @location(9) bounce_sky: vec3<f32>,
    // How much of the wet film the ground takes (`stage::Vertex::wet`, `wet_plus`).
    @location(10) wet: f32,
}

// `VertexOut` and how much of the sun a cloud takes there (`cloud_shadow`), for the moving clouds (`Enhance::clouds`,
// `vs_clouded`): the clouds are kilometres across, so per vertex. A separate entry point, so the frame without them is
// the same as before they were added (an extra output moved a few edge pixels).
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
    @location(10) wet: f32,
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
    out.normal = v.normal;
    out.specular = v.specular;
    out.sky = vec3<f32>(unpack2x16snorm(v.sky_dir), v.occlusion);
    out.bounce_sun = bounce_light(v.bounce.x);
    out.bounce_sky = bounce_light(v.bounce.y);
    out.wet = v.wet;
    return out;
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

// A stage, car or sky-face draw's colour, with `under_cloud` of the sun taken by a cloud.
fn shade(v: VertexOut, front: bool, under_cloud: f32) -> vec4<f32> {
    let first = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias);
    if (PERF_SHADE == 1.0) {
        var a = first.a;
        if (ALPHA_TEST && mode.alpha_test != 0u && a <= mode.alpha_ref) { discard; }
        if (mode.coverage != 0u) { a = clamp((a - mode.alpha_ref) / max(fwidth(a), 1e-4) + 0.5, 0.0, 1.0); }
        return vec4<f32>(v.color.rgb * first.rgb, a);
    }
    if (WATER_PLUS && water_draw() && frame.enhance.z != 0.0) {
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
        if (lit_class() == 0u) {
            surface = mix(first.rgb, second.rgb, second.a * v.color.a);
        }
    }
    // The backdrop (the world-anchored sky faces' frame) gets detail its stretched texture lacks.
    if (!stage_frame() && !camera_anchored()) {
        surface = surface * backdrop_detail(v.world);
    }
    let night = frame.night.x != 0.0;
    // With the computed occlusion the stage's light is ours (`own_light`), its sun shadowed by what hides the sun.
    let own = computed_occlusion() && day_on() && lit_class() == 0u && !camera_anchored();
    var sun_seen = 1.0;
    var colour: vec3<f32>;
    if (own) {
        // The sun's or, once it is down, the moon's share past the shadows.
        sun_seen = own_sun_seen(v, under_cloud);
        // Lit in linear light (`own_light`), then back to the display colour the rest of the frame works in.
        colour = pow(own_light(v, sun_seen, under_cloud) * pow(surface, vec3<f32>(2.2)), vec3<f32>(1.0 / 2.2));
    } else {
        colour = relit(v) * surface;
    }
    // Our wet road (`wet_plus`), with the puddles' addition in wet weather where the surface takes it.
    let wet = WATER_PLUS && frame.enhance.w != 0.0 && frame.wetness.x >= 1.0;
    let wet_road = wet && detail_draw() && v.wet > 0.0;
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
    // sun's shadows on (`sun_shadow.w`) it is not used: their cascades and far layer cast for any sun, and the moon.
    let baked = select(textureSample(shadow_texture, texture_sampler, v.uv2).r, 1.0, frame.sun_shadow.w != 0.0);
    // How much the sun's shadows reach here (`sun_light` below fades out with it).
    let near_sun = sun_reach(v.world);
    // The car's lamps (`lamp_sum`) at night and at dusk (`frame.dusk.w`, ours), as at night over the fading daylight:
    // with our light in lux (`lamp_lux`: over pi a white face's luminance, pre-exposed), added in linear light.
    var lamps = vec3<f32>(0.0);
    if (LAMPS && lit_class() == 0u && !camera_anchored() && (night || frame.dusk.w > 0.01)) {
        let has_normal = dot(v.normal, v.normal) > 1e-8;
        let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
        lamps = lamp_sum(v.world, n, mode.coverage != 0u);
    }
    if (own) {
        let lux = lamp_lux(lamps) * frame.day_sun.w / SKY_PI;
        colour = pow(pow(colour, vec3<f32>(2.2)) + pow(surface, vec3<f32>(2.2)) * lux, vec3<f32>(1.0 / 2.2));
    } else if (night && lit_class() == 0u) {
        colour = night_lit(surface, baked, v, front, lamps);
    } else {
        colour = colour * baked + lamp_albedo(surface, v) * lamp_knee(lamps) * frame.dusk.w;
    }
    if (wet_road) {
        colour = wet_plus(colour, v);
    }
    // The near ground (the unlit draws with the mip bias) gets fine detail where its textures are blurred.
    if (lit_class() == 0u && detail_draw()) {
        colour = colour * ground_detail(v.world);
        // In wet weather the ground around the roads is soaked too, a little less (`GROUND_SOAK` of the road's).
        if (wet && !wet_road) {
            colour = soak(colour, GROUND_SOAK);
        }
    }
    if (lit_class() != 0u) {
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
        let outside = surroundings(n);
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
        // and the reflection reduced where it covers.
        var clean = 1.0;
        if (mode.fresnel.w != 0.0) {
            let mud = textureSampleBias(specular_texture, texture_sampler, v.uv0, mode.mip_bias);
            let covered = clamp(mud.a * frame.wetness.y, 0.0, 1.0);
            colour = mix(colour, DIRT_COLOUR, covered);
            clean = 1.0 - covered;
        }
        if (mode.grime.x != 0.0 && frame.wetness.y > 0.0) {
            // A pane's mud film by the dirt level times its share (ours, `dirt::grime`), in patches of a value noise
            // on the pane's uv with clearer glass between them; the wet mud keeps `GRIME_SHEEN` of the reflection.
            let film = frame.wetness.y * mode.grime.x;
            let patches = 0.65 * value_noise(vec3<f32>(v.uv0 * GRIME_SCALE, 0.0))
                + 0.35 * value_noise(vec3<f32>(v.uv0 * GRIME_SCALE * 4.0, 1.7));
            let covered = GRIME_MOST * film * smoothstep(0.3, 0.7, patches);
            colour = mix(colour, DIRT_COLOUR, covered);
            alpha = mix(alpha, 1.0, covered);
            clean = 1.0 - covered * (1.0 - GRIME_SHEEN);
        }
        var shaded: vec3<f32>;
        if (cabin_face && day_on()) {
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
            colour = shaded + reflected(reflect(-view, n), 0.0) * f * clean;
            alpha = clamp(alpha + 0.5 * f * clean, 0.0, 1.0);
            // The cabin's opaque glass is its mirrors (`cabin_mirror`).
            if (CABIN && inside && cabin_set(v)) {
                colour = cabin_mirror(first.rgb, reflect(-view, n));
            }
        } else {
            // With the HDR post pass, paint reflects less and a little blurred (`PAINT_GLOSS`).
            let gloss = select(vec2<f32>(1.0, 0.0), PAINT_GLOSS, frame.enhance.x != 0.0);
            var shine = reflected(reflect(-view, n), gloss.y) * f * gloss.x * alpha * clean;
            // The cabin's paint reflects only what it sees out of the windows (`cabin_open`).
            if (cabin_face) {
                shine = shine * cabin_open(cabin_seen(v), 1.0 - v.specular.z, reflect(-view, n));
            }
            colour = shaded + shine;
            // With the day's light the coat's reflection adds in linear light over the paint, which gets what it does
            // not reflect.
            if (physical) {
                let coat = f * alpha * clean;
                let mirrored = reflected(reflect(-view, n), gloss.y);
                colour = pow(pow(shaded, vec3<f32>(2.2)) * (1.0 - coat) + pow(mirrored, vec3<f32>(2.2)) * coat, vec3<f32>(1.0 / 2.2));
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
        if (cabin_face && mode.fresnel.z == 0.0) {
            colour = colour + cabin_sheen(v, n, view);
        }
        colour = colour + glint;
        // A lit lamp's part (`stage::Shade::glow`) glows (ours).
        if (lit_class() == 3u) {
            colour = max(colour, first.rgb * LAMP_GLOW);
        }
        // A dial's backlight (`rbr_render::dials`, ours): by night, and at dusk as much as the lamps light the stage,
        // the texel glows in the light of its emissive mask (the RBRHUD gauges' `emissive_mask`): the mask's colour
        // (the bulbs' pools on analog faces) plus its alpha in white (displays, warning lamps).
        if (mode.glow.x != 0.0 && (night || frame.dusk.w > 0.0)) {
            let mask = textureSampleLevel(glow_texture, texture_sampler, v.uv0, 0.0);
            let backlight = mask.rgb + vec3<f32>(mask.a * mode.glow.y);
            colour = max(colour, first.rgb * backlight * DIAL_GLOW * select(frame.dusk.w, 1.0, night));
        }
        // The scrape (`CarPaint`, `CarPaintInt`): the scrape texel times the diffuse light (`t3 * v0`, no highlight or
        // reflection) replaces the paint by `sat(v0.w * t3.a)`, `v0.w` being the vertex's damage weight / 100.
        if (second_draw()) {
            let scraped = LIGHT_SCALE * second.rgb * light(n, sunlit, around.rgb, v.world);
            colour = mix(colour, scraped, clamp(v.color.a * second.a, 0.0, 1.0));
        }
    } else if (own && frame.sun_shadow.z == 0.0) {
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
        // Under a cloud's shadow (`clouds.wgsl`) the sun's own shadows fade with the sunlight they take away.
        colour = colour * (1.0 - CLOUD_SHADE * under_cloud * mix(0.4, 1.0, open_sky));
        // At night the moon's shadows are in `night_lit`, off the lamps' light.
        if (near_sun > 0.0 && !night) {
            colour = colour * mix(vec3<f32>(1.0), sun_light(v, front, open_sky, first.rgb), near_sun * (1.0 - under_cloud));
        }
    }
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    if (mode.coverage != 0u) {
        alpha = clamp((alpha - mode.alpha_ref) / max(fwidth(alpha), 1e-4) + 0.5, 0.0, 1.0);
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
        if (frame.clouds.y != 0.0) {
            colour = cloud_face_lit(colour, alpha, dir);
        }
        alpha = alpha * smoothstep(0.03, 0.2, dir.y);
        colour = mix(colour, sky(dir, 0.0), 0.5 * (1.0 - smoothstep(0.0, 0.35, dir.y)));
        colour = mix(colour, sky(dir, 0.0), frame.haze.y);
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
    return frame.occlusion.x != 0.0 && frame.night.w > 0.0;
}

fn relit(v: VertexOut) -> vec3<f32> {
    let c = v.color.rgb;
    let computed = computed_occlusion();
    if ((frame.relight.w == 0.0 && !computed) || lit_class() != 0u || camera_anchored()) {
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

fn own_light(v: VertexOut, sun_seen: f32, under_cloud: f32) -> vec3<f32> {
    let c = v.color.rgb;
    let most = max(c.r, max(c.g, c.b));
    let hue = mix(vec3<f32>(1.0), c / max(most, 1e-3), smoothstep(0.02, 0.08, most));
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    let ground_sky = open_sky_light(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0));
    let sky_unit = frame.day_sky.rgb;
    let sun_unit = frame.day_sun.rgb;
    let sky = sky_unit * (1.0 - v.sky.z) * open_sky_light(n, sky_direction(v.sky.xy, n)) / max(ground_sky, vec3<f32>(1e-4));
    let sun = sun_unit * max(dot(n, -frame.sun.xyz), 0.0) * sun_seen;
    let bounce = BOUNCE * (sky_unit * v.bounce_sky + sun_unit * v.bounce_sun * (1.0 - under_cloud));
    // The contact shading on the sky's and the bounce's light, never the sun's (`contact_light`).
    var contact = 1.0;
    if (SUN_MASK && CONTACT) {
        contact = textureLoad(sun_contact, pixel / 2, 0).r;
    }
    return pow(hue, vec3<f32>(2.2)) * ((sky + bounce) * contact + sun);
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

// The depth prepass of the alpha-tested draws (`renderer::DrawPass`): `fs_main`'s alpha test and alpha to coverage on
// the texture's alpha, in the same steps, so that the samples it covers are the ones `fs_main` would. The colour, the
// normal `own_sun_seen` takes, is written only for the sun's mask (`sun_mask.wgsl`).
@fragment
fn fs_prepass(v: VertexOut) -> @location(0) vec4<f32> {
    var alpha = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias).a;
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    if (mode.coverage != 0u) {
        alpha = clamp((alpha - mode.alpha_ref) / max(fwidth(alpha), 1e-4) + 0.5, 0.0, 1.0);
    }
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    return vec4<f32>(n * 0.5 + 0.5, alpha);
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

// The dirt's colour (ours): `c3.xyz` comes from `m_unkC0`, which nothing writes (`notes/formats.md` "Shaders and vertex layouts"); this is the manager's
// unused preset `m_dirtColor` (0.27, 0.22, 0.14).
const DIRT_COLOUR: vec3<f32> = vec3<f32>(0.27, 0.22, 0.14);
// The panes' mud film (ours, `dirt::grime`): its patches per unit of uv, the most it covers, and the share of the
// reflection the wet mud keeps.
const GRIME_SCALE: f32 = 12.0;
const GRIME_MOST: f32 = 0.55;
const GRIME_SHEEN: f32 = 0.4;

// The clouds' light (ours): the vertex colours of the cloud faces peak at 128 (0.5) and darken toward the
// undersides; the game's extinction and in-scatter terms that scale and add to them (`CloudsSingleTexture.vso`,
// `c55`-`c66`) are not reproduced. Doubled, a face's lit side is the texture's white.
const CLOUD_LIGHT: f32 = 2.0;

// The renderer's car lighting (the game's needs constants with unproven sources, `notes/formats.md` "Shaders and vertex layouts"): ambient Ka, with a
// little more from above than below, plus Kd by the sun's angle, and a Cook-Torrance-shaped highlight
// (`CookTorranceKs`, `CookTorranceM`) in the sun's colour. `LIGHT_SCALE` maps the settings' 0.5 colours and 1.7 to 1.9
// scales to about 1 in full sun.
const LIGHT_SCALE: f32 = 0.5;
// The car paint's reflection with the HDR post pass (renderer addition, `Enhance::hdr`): its share of the game's
// Fresnel weight (whose FresnelMin of about 0.3 makes the paint a mirror head on), and the reflection cube's mip level,
// so it reads as a clear coat rather than chrome.
const PAINT_GLOSS: vec2<f32> = vec2<f32>(0.75, 1.0);

fn lit(colour: vec3<f32>, n: vec3<f32>, view: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let to_sun = -frame.sun.xyz;
    let ndl = max(dot(n, to_sun), 0.0) * sunlit;
    if (day_lit()) {
        // Lambert on the texture's albedo plus the sun's Cook-Torrance highlight, in linear light.
        let e = light_sum(n, sunlit, ambient, world, vec2<f32>(1.0));
        let albedo = min(pow(colour * CAR_ALBEDO, vec3<f32>(2.2)), vec3<f32>(ALBEDO_MOST));
        return pow(albedo * e + sun_highlight(n, view, to_sun) * frame.day_sun.rgb * ndl, vec3<f32>(1.0 / 2.2));
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

// The sun's specular reflection off a dielectric (F0 0.04) of the class's roughness (`CookTorranceM`), Beckmann's
// distribution with the V-cavity shadowing and Schlick's Fresnel, times pi (the day's sun is its illuminance over pi).
fn sun_highlight(n: vec3<f32>, view: vec3<f32>, to_sun: vec3<f32>) -> f32 {
    let m = select(MATTE, max(mode.light.w, LEAST_ROUGHNESS), mode.light.z > 0.0);
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
    return k.x * sky * hemisphere * ambient + k.y * sun * ndl + bounce;
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

fn surroundings(n: vec3<f32>) -> vec4<f32> {
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
    let around = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, look, 5.0).rgb);
    // With the physically based sky, the open sky as blurred as the cube's level (`sky_reflected`).
    let open = open_sky(look, 1.0);
    let ambient = mix(vec3<f32>(dot(around, luma) / max(dot(open, luma), 1e-3)), around / max(open, vec3<f32>(1e-3)), AMBIENT_TINT);
    let to_sun = -frame.sun.xyz;
    let toward = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, to_sun, 3.0).rgb);
    let seen = dot(toward, luma) / max(dot(open_sky(to_sun, 0.6), luma), 1e-3);
    return vec4<f32>(clamp(ambient, vec3<f32>(0.3), vec3<f32>(1.2)), mix(SUN_LEAST, 1.0, smoothstep(SUN_SEEN.x, SUN_SEEN.y, seen)));
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
    // Karis's fit of the split-sum environment BRDF for F0 0.04.
    let r = roughness * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    let reflectance = 0.04 * ab.x + ab.y;
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
    return mix(BANK * daylight, clouded(sky(mirrored, 1.0), mirrored), smoothstep(0.0, BANKS, mirrored.y));
}

// The sun's glint on water along `mirrored`, in clear weather: a small bright core (a calm surface mirrors the disc)
// and a soft halo.
fn glint(mirrored: vec3<f32>) -> vec3<f32> {
    let light = sunlight();
    let toward = max(dot(mirrored, -frame.sun.xyz), 0.0);
    let shine = frame.sun.w * (1.0 - frame.night.x) * (2.5 * pow(toward, 12000.0) + 0.15 * pow(toward, 600.0)) * light / max(max(light.r, light.g), light.b);
    return shine * (1.0 - cloud_shadow(frame.eye.xyz));
}

// A wet road (renderer addition, `Enhance::wet`, `wet_plus` in `water.wgsl`): a film of water in wet weather over the
// ground whose collision surface takes it (`VertexOut::wet`, `puddles::mark_roads`), reflecting the stage around the
// car (its reflection cube; the sky like the water surface, `mirrored_light`, before the cube is drawn) and the sun's
// glint, over the darkened road. Patches of about 3 m (`PUDDLES`) hold more water.
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

fn landing_hash(c: vec2<f32>) -> vec3<f32> {
    let q = vec3<f32>(dot(c, vec2<f32>(127.1, 311.7)), dot(c, vec2<f32>(269.5, 183.3)), dot(c, vec2<f32>(419.2, 371.9)));
    return fract(sin(q) * 43758.5453);
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
// again over the resolved frame. The reflected ray is stepped through the stage (`SSR_STEPS` steps, growing, out to
// `SSR_REACH` metres); the first step behind the frame's depth, by less than a thickness that grows with the distance,
// (each pixel's steps start at a different fraction of a step, so the step size does not show as bands) is refined
// by halving and its frame colour is blended in by the surface's Fresnel weight, fading out toward the
// screen's edges and with the distance stepped. Rays that leave the screen or hit nothing take the car's reflection
// cube instead (the stage around the car, `renderer::Reflection`), near the eye only (`SSR_CUBE_REACH` metres), since
// the cube is drawn at the car; elsewhere, or with no car, the sky reflection drawn with the surface shows.
const SSR_STEPS: i32 = 64;
const SSR_RAYS: i32 = 4;
const SSR_REACH: f32 = 120.0;
const SSR_CUBE_REACH: vec2<f32> = vec2<f32>(20.0, 60.0);
// Glossy reflections: the frame is read from a blurrier level (`renderer::SCENE_MIPS`) the further the ray travels on
// screen, as a cone of `GLOSS` (its width over the distance, in pixels): wider on roads (rough asphalt under the film)
// than on water. A hit is trusted less the further behind the frame's surface the ray ends (`thickness`), fading
// instead of cutting off, and a ray that ends further behind than that keeps stepping (it passed behind something in
// front, a trunk or a foliage card, rather than hitting it).
const GLOSS: vec2<f32> = vec2<f32>(0.12, 0.04);
const ROUGHNESS: vec2<f32> = vec2<f32>(0.02, 0.01);
const STRETCH: f32 = 4.0;

// The frame's depth at `uv` (the first sample).
fn scene_depth_at(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let pixel = vec2<i32>(clamp(uv * size, vec2<f32>(0.0), size - 1.0));
    if (textureDimensions(scene_depth_copy).x > 1u) {
        return textureLoad(scene_depth_copy, pixel, 0).x;
    }
    return textureLoad(scene_depth, pixel, 0);
}

// The eye's distance to the stage at `uv`, where the frame's depth is `depth`.
fn scene_distance(uv: vec2<f32>, depth: f32) -> f32 {
    let p = frame.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return distance(p.xyz / p.w, frame.eye.xyz);
}

// Where `p` falls on the screen (uv, and z its depth), w 0 when it is off screen or behind the eye.
fn on_screen(p: vec3<f32>) -> vec4<f32> {
    let clip = frame.view_projection * vec4<f32>(p, 1.0);
    if (clip.w <= 1e-4) {
        return vec4<f32>(0.0);
    }
    let ndc = clip.xyz / clip.w;
    let inside = f32(all(abs(ndc.xy) <= vec2<f32>(1.0)));
    return vec4<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, ndc.z, inside);
}

@fragment
fn fs_reflect(v: VertexOut) -> @location(0) vec4<f32> {
    return reflect_ray(v);
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
        miss = vec4<f32>(textureSampleLevel(surroundings_cube, reflection_sampler, ray, select(3.0, 1.0, mode.water != 0u)).rgb, weight * near);
    }
    let roughness = select(ROUGHNESS.x, ROUGHNESS.y, mode.water != 0u);
    let side = normalize(cross(ray, n + vec3<f32>(1e-3, 0.0, 0.0)));
    let up = cross(side, ray);
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < SSR_RAYS; k++) {
        // Points on a disc, turned by the pixel's noise.
        let angle = 6.2831853 * (f32(k) + jitter) / f32(SSR_RAYS);
        let radius = roughness * sqrt((f32(k) + 0.5) / f32(SSR_RAYS));
        // Stretched toward and away from the eye (`STRETCH`), as wet roads smear reflections down the screen.
        let spread = normalize(ray + radius * (cos(angle) * side + STRETCH * sin(angle) * up));
        let found = trace(v, spread, weight, miss, fract(jitter + f32(k) * 0.618));
        // Colour weighted by how much each ray contributes.
        sum += vec4<f32>(found.rgb * found.a, found.a);
    }
    sum /= f32(SSR_RAYS);
    return vec4<f32>(sum.rgb / max(sum.a, 1e-4), sum.a);
}

// One ray from `v` along `ray`: what it reflects and its weight (`weight` on a hit, `miss` when it leaves the screen
// or hits nothing), its steps starting `jitter` of a step early.
fn trace(v: VertexOut, ray: vec3<f32>, weight: f32, miss: vec4<f32>, jitter: f32) -> vec4<f32> {
    if (ray.y <= 0.0) {
        return miss;
    }
    let thickness = 0.15 + 0.01 * distance(v.world, frame.eye.xyz);
    var before = 0.0;
    for (var i = 1; i <= SSR_STEPS; i++) {
        let t = SSR_REACH * pow((f32(i) - jitter) / f32(SSR_STEPS), 2.0);
        let at = on_screen(v.world + ray * t);
        if (at.w == 0.0) {
            return miss;
        }
        // The step's distance behind the frame's surface only once it is behind it.
        let depth = scene_depth_at(at.xy);
        if (at.z < depth && depth > 0.0
            && distance(v.world + ray * t, frame.eye.xyz) - scene_distance(at.xy, depth) < thickness + 0.02 * t) {
            // Halve back to the surface.
            var low = before;
            var high = t;
            for (var k = 0; k < 5; k++) {
                let mid = 0.5 * (low + high);
                let m = on_screen(v.world + ray * mid);
                if (m.z < scene_depth_at(m.xy)) {
                    high = mid;
                } else {
                    low = mid;
                }
            }
            let hit = on_screen(v.world + ray * high);
            let behind = distance(v.world + ray * high, frame.eye.xyz) - scene_distance(hit.xy, scene_depth_at(hit.xy));
            let reach = thickness + 0.02 * high;
            if (behind <= reach) {
                let size = vec2<f32>(textureDimensions(scene_colour));
                let pixels = length((hit.xy - v.clip.xy / size) * size);
                let gloss = select(GLOSS.x, GLOSS.y, mode.water != 0u);
                let lod = clamp(log2(max(pixels * gloss, 1.0)), 0.0, f32(textureNumLevels(scene_colour) - 1u));
                let colour = textureSampleLevel(scene_colour, reflection_sampler, hit.xy, lod).rgb;
                let edge = smoothstep(0.0, 0.15, min(min(hit.x, 1.0 - hit.x), min(hit.y, 1.0 - hit.y)));
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

// A lattice point's value; the lattice repeats every 289 points, which keeps the hash precise far from the origin.
fn hash(p: vec3<f32>) -> f32 {
    let q = p - floor(p / 289.0) * 289.0;
    return fract(sin(dot(q, vec3<f32>(127.1, 311.7, 74.7))) * 43758.5453);
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

// Backdrop detail (renderer choice): the backdrop's texture is stretched over kilometres, a few metres a texel, so
// next to the stage it reads as a blur. Its own texture, tiled flat in stage space at `BACKDROP_TILES` metres, adds
// the fine structure it lacks: each tiling's brightness over the texture's mean (its last mip) scales the colour by up
// to `BACKDROP_DETAIL`, and the tilings fade out where a pixel spans an eighth of a tile (the mips already blur them).
const BACKDROP_DETAIL: f32 = 0.6;
const BACKDROP_TILES: vec2<f32> = vec2<f32>(12.0, 47.0);

fn backdrop_detail(world: vec3<f32>) -> f32 {
    let footprint = max(length(dpdx(world)), length(dpdy(world)));
    let mean = max(dot(textureSampleLevel(texture0, texture_sampler, vec2<f32>(0.5), 16.0).rgb, vec3<f32>(0.333)), 1e-3);
    var detail = 0.0;
    for (var i = 0; i < 2; i++) {
        let tile = BACKDROP_TILES[i];
        let shown = 1.0 - smoothstep(0.06 * tile, 0.125 * tile, footprint);
        let texel = textureSample(texture0, texture_sampler, world.xz / tile + vec2<f32>(0.37 * f32(i))).rgb;
        detail += 0.5 * shown * (dot(texel, vec3<f32>(0.333)) / mean - 1.0);
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
    let p = frame.sun_matrices[i] * vec4<f32>(world + n * frame.sun_texels[i] * 1.5, 1.0);
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
    let p = frame.world_matrix * vec4<f32>(world + n * frame.world_shadow.y * 1.5, 1.0);
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
    if (has_normal && mode.coverage == 0u && !overcast && frame.relight.w == 0.0) {
        facing = smoothstep(-0.05, 0.25, dot(n, -frame.sun.xyz));
    }
    var lit = 0.0;
    if (facing > 0.0) {
        lit = sun_lit(v.world, n) * facing;
    }
    let receive = select(1.0, 0.5, mode.coverage != 0u) * mix(0.4, 1.0, open_sky);
    var glow = vec3<f32>(0.0);
    if (!overcast) {
        glow = leaf_glow(v, lit, albedo);
    }
    return (1.0 - frame.sun_shadow.x * (1.0 - lit) * receive) * (1.0 + glow);
}

// A sunlit plant's glow against the sun (`sun_light`), `lit` its share of the sun.
fn leaf_glow(v: VertexOut, lit: f32, albedo: vec3<f32>) -> vec3<f32> {
    if (mode.coverage == 0u || v.specular.z <= 0.5) {
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

// 0 in the sun to 1 in the car's shadow: a 4x4 filter over the shadow map. Points past the map's far side (the
// ground beyond the car) compare at its far plane, so they are shadowed wherever the car is in the map.
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
    var sunlit = 0.0;
    for (var row = 0; row < 4; row++) {
        for (var column = 0; column < 4; column++) {
            let x = f32(column) - 1.5;
            let y = f32(row) - 1.5;
            let at = uv + vec2<f32>(x, y) * frame.shadow.z;
            sunlit += textureSampleCompareLevel(shadow_map, shadow_sampler, at, 0, reference);
        }
    }
    return 1.0 - sunlit / 16.0;
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
            return clouded(sky_reflected(dir, level / 7.0), dir);
        }
        return clouded(sky(dir, 1.0), dir);
    }
    return ungraded(textureSampleLevel(reflection_cube, reflection_sampler, dir, level).rgb);
}

// Linear fog by the distance to the eye, covering at most `frame.fog_color.w`. Under fog (the section's or ours) with
// `Enhance::sky` the height fog alone (`height_fog.rs`), the same over the stage, the backdrop and the sky, so the land fades into the sky as
// into a volume of fog, thinning with the height, rather than to an edge. The backdrop stands for the land beyond the
// stage, so the fog takes it as no nearer than `SKY_BACKDROP` (at its own height): its faces that hang a few hundred
// metres off, with nothing under them, showed their lower edges as lines through the fog.
fn fogged(colour: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let volume = SKY_LUT && frame.enhance.y != 0.0 && frame.skylight.w != 0.0 && frame.height_fog.x > 0.0;
    if (volume) {
        if (camera_anchored()) {
            return sky_misted(colour, normalize(world));
        }
        let d = max(distance(world, frame.eye.xyz), 1e-3);
        let dir = (world - frame.eye.xyz) / d;
        let reach = select(d, max(d, SKY_BACKDROP), !stage_frame());
        return mix(colour, height_fog_light(dir, haze_colour(dir)), height_fog_cover_at(world.y, reach));
    }
    let range = max(frame.fog_range.y - frame.fog_range.x, 1.0);
    var f = vec3<f32>(clamp((distance(world, frame.eye.xyz) - frame.fog_range.x) / range, 0.0, 1.0));
    var haze = frame.fog_color.rgb;
    var seen = world;
    if (frame.enhance.y != 0.0 && !camera_anchored()) {
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
    if (SKY_LUT && frame.height_fog.x > 0.0 && !camera_anchored()) {
        return height_fogged(hazed, seen, haze);
    }
    return hazed;
}

// Aerial perspective (renderer addition, `Enhance::sky`): the haze thickens exponentially from the eye rather than
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

// The renderer's tone curve (ours, not the game's: D3D9 wrote the colour as it was). The stages' baked vertex light is
// dim, and the game's in-scatter, which brightens it, is not reproduced. `1 - exp(-k c)`, scaled so that 1 stays 1,
// with a power for the dark end, lifts the dark and middle tones gently (0.1 to 0.15, 0.5 to 0.65) and rolls off toward
// white; a little saturation gets back what the curve takes from coloured pixels. Kept mild, for a natural look.
const TONE_CURVE: f32 = 1.5;
const TOE: f32 = 1.1;
const SATURATION: f32 = 1.08;

fn graded(colour: vec3<f32>) -> vec3<f32> {
    // For the HDR post pass (`post.wgsl`), which applies the curve: linear colour, unclipped.
    if (frame.enhance.x != 0.0) {
        return pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2));
    }
    let c = pow((1.0 - exp(-TONE_CURVE * max(colour, vec3<f32>(0.0)))) / (1.0 - exp(-TONE_CURVE)), vec3<f32>(TOE));
    let grey = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return clamp(mix(vec3<f32>(grey), c, SATURATION), vec3<f32>(0.0), vec3<f32>(1.0));
}

// The inverse of `graded`'s curve (not of its saturation), per channel.
fn ungraded(colour: vec3<f32>) -> vec3<f32> {
    if (frame.enhance.x != 0.0) {
        return pow(max(colour, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    }
    let c = pow(clamp(colour, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / TOE));
    return -log(max(1.0 - c * (1.0 - exp(-TONE_CURVE)), vec3<f32>(1e-4))) / TONE_CURVE;
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
    var colour: vec3<f32>;
    if (frame.haze.x == 1.0) {
        colour = cloud_volume_over(sky(dir, 0.0), v.ndc);
    } else if (frame.haze.x == 2.0) {
        colour = cloud_dome_over(sky(dir, 0.0), dir);
    } else {
        colour = clouded(sky(dir, 0.0), dir);
    }
    if (SKY_LUT && frame.height_fog.x > 0.0) {
        colour = sky_misted(colour, dir);
    }
    if (sky_on() && frame.enhance.x == 0.0) {
        colour = sky_shoulder(colour);
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
        // Under HDR the disc is brighter than white, for the bloom (`post.wgsl`).
        let brightness = select(1.5, 1.5 * SUN_HDR, frame.enhance.x != 0.0);
        colour = mix(colour + glow * sun_colour, mix(vec3<f32>(1.0), sun_colour, 0.25) * brightness, disc);
    }
    let below = clamp(-dir.y * 4.0, 0.0, 1.0) * ground;
    return mix(colour, frame.fog_color.rgb * 0.25, below);
}

// How much brighter the sun's disc is than white under HDR.
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
