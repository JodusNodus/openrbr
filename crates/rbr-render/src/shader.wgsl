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

fn second_draw() -> bool {
    return select(mode.second_texture != 0u, DRAW_SECOND, KNOWN);
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

// The screen-space reflections' pipeline (`renderer::reflection::ScreenReflection`): 1 for the water's draws, 0 for the
// wet roads'; past 1 the draw's mode says at run time.
override REFLECT_WATER: u32 = 2u;

// The stage's weather, which holds for the renderer's life (`renderer::pipelines::Conditions`): with `WEATHER_KNOWN`
// the pipeline is built for it, else the frame says at run time. As with `LIT`, the code of the other weathers held
// registers (the hazy 23_E chase view 17.8 to 12.9 ms a frame at 2268x1473 on an M1 Pro).
override WEATHER_KNOWN: bool = false;
override OVERCAST: bool = false;
override WET: bool = false;
override VOLUME_FOG: bool = false;

// The stage's draws take the pipelines with the lamps' code at night and at dusk (`Renderer::lamps_light`), so those
// without it (`DAY_ONLY`) are never drawn at night.
override DAY_ONLY: bool = false;

fn is_night() -> bool {
    return !DAY_ONLY && frame.night.x != 0.0;
}

// `UseFog`: no sun (`frame.sun.w` 0) and the overcast's shadow filter (`frame.sun_shadow.z` 1).
fn is_overcast() -> bool {
    return select(frame.sun_shadow.z != 0.0, OVERCAST, WEATHER_KNOWN);
}

fn sun_out() -> bool {
    return select(frame.sun.w != 0.0, !OVERCAST, WEATHER_KNOWN);
}

// The height fog as a volume (`height_fog::is_volume`, `frame.skylight.w`).
fn volume_fog() -> bool {
    return select(frame.skylight.w != 0.0, VOLUME_FOG, WEATHER_KNOWN);
}

// The lit pipelines' view (`renderer::pipelines::PipelineKey::inside`): 1 in the car (the cockpit; `frame.reflection.y`
// 1), 0 outside it, past 1 `frame.reflection.y` says at run time. As with `LIT`, the code of the other view held
// registers.
override INSIDE: u32 = 2u;

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
@group(0) @binding(16) var shadow_ranges: texture_2d_array<f32>;
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
    @location(10) cloud: f32,
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
        o.clip, o.color, o.uv0, o.uv1, o.uv2, o.world, o.normal, o.specular, o.sky, o.bounce_sun, o.bounce_sky,
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
    return out;
}

// A colour packed as `bounce.wgsl` packs it: three 10-bit shares of 0 to 2.
fn bounce_light(w: u32) -> vec3<f32> {
    return vec3<f32>(vec3<u32>(w, w >> 10u, w >> 20u) & vec3<u32>(1023u)) * (2.0 / 1023.0);
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

// How much water stands where the film is `wet` (`wet_mask`): none on the thin film, all in its patches.
fn standing(wet: f32) -> f32 {
    return smoothstep(0.6, 1.0, wet);
}

// Screen-space reflections (renderer choice, `renderer::ScreenReflection`): the water, and the wet roads' film, drawn
// again over the resolved frame. The reflected ray is stepped through the stage (`SSR_STEPS` steps, growing, out to
// `SSR_REACH` metres); the first step behind the frame's depth, by less than a thickness that grows with the distance,
// (each pixel's steps start at a different fraction of a step, so the step size does not show as bands) is refined
// by halving and its frame colour is blended in by the surface's Fresnel weight, fading out toward the
// screen's edges and with the distance stepped. Rays that leave the screen or hit nothing take the car's reflection
// cube instead (the stage around the car, `renderer::Reflection`), near the eye only (`SSR_CUBE_REACH` metres), since
// the cube is drawn at the car; elsewhere, or with no car, the sky reflection drawn with the surface shows.
override SSR_STEPS: i32 = 64;
override SSR_RAYS: i32 = 4;

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
    if (!is_overcast()) {
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

@vertex
fn vs_shadow(v: VertexIn) -> @builtin(position) vec4<f32> {
    return frame.shadow_matrix * vec4<f32>(placed(v).position, 1.0);
}

// The taps along each axis of the car's sun shadow's filter and of its contact shadow's (`renderer::Quality`, `viewer
// --shadows`): 4 and 6 at high.
override SHADOW_TAPS: i32 = 4;
override CONTACT_TAPS: i32 = 6;
// True with `Enhance::cabin` (a pipeline override, so that the frame without it is built as before).
override CABIN: bool = false;
// The whole part of `specular.y` of the interior set's vertices and of its dial faces (`car::cabin::INTERIOR`,
// `GAUGE`). Its fraction holds the z of where the vertex sees the sky (`cabin_seen`).
const CABIN_INTERIOR: f32 = 1.0;
// Whether the draw's faces are all the interior set's (1) or none are (0), as its pipeline knows them
// (`renderer::pipelines::PipelineKey::cabin_set`); past 1 each fragment's `specular.y` says.
override CABIN_SET: u32 = 2u;
// `car::cabin::SEEN_Z`.
const CABIN_SEEN_Z: vec2<f32> = vec2<f32>(0.5, 0.45);
// True with mirror passes (`Enhance::mirrors` not off; a pipeline override, so that the frame without them is built as
// before).
override MIRRORS: bool = false;

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
    // Where every tap's texels lie behind the reference, or all in front of it, the filter's answer is known.
    let range = shadow_range(0, uv);
    if (reference <= range.x) {
        return 0.0;
    }
    if (reference > range.y) {
        return 1.0;
    }
    // `SHADOW_TAPS` along each axis, over 3 steps.
    let spacing = 3.0 / f32(SHADOW_TAPS - 1);
    let centre = 0.5 * f32(SHADOW_TAPS - 1);
    var sunlit = 0.0;
    for (var row = 0; row < SHADOW_TAPS; row++) {
        for (var column = 0; column < SHADOW_TAPS; column++) {
            let x = (f32(column) - centre) * spacing;
            let y = (f32(row) - centre) * spacing;
            let at = uv + vec2<f32>(x, y) * frame.shadow.z;
            sunlit += textureSampleCompareLevel(shadow_map, shadow_sampler, at, 0, reference);
        }
    }
    return 1.0 - sunlit / f32(SHADOW_TAPS * SHADOW_TAPS);
}

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
    // `CONTACT_TAPS` along each axis, over 5 steps.
    let spacing = 5.0 / f32(CONTACT_TAPS - 1);
    let centre = 0.5 * f32(CONTACT_TAPS - 1);
    var covered = 0.0;
    for (var row = 0; row < CONTACT_TAPS; row++) {
        for (var column = 0; column < CONTACT_TAPS; column++) {
            let x = (f32(column) - centre) * spacing;
            let y = (f32(row) - centre) * spacing;
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
    return covered / f32(CONTACT_TAPS * CONTACT_TAPS);
}

// Linear fog by the distance to the eye, covering at most `frame.fog_color.w`. Under fog (the section's or ours) with
// `Enhance::sky` the height fog alone (`height_fog.rs`), the same over the stage, the backdrop and the sky, so the land fades into the sky as
// into a volume of fog, thinning with the height, rather than to an edge. The backdrop stands for the land beyond the
// stage, so the fog takes it as no nearer than `SKY_BACKDROP` (at its own height): its faces that hang a few hundred
// metres off, with nothing under them, showed their lower edges as lines through the fog.
fn fogged(colour: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let volume = SKY_LUT && frame.enhance.y != 0.0 && volume_fog() && frame.height_fog.x > 0.0;
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
        if (sky_on() && sun_out()) {
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
    if (sun_out() && !is_night()) {
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
    if (is_night()) {
        colour = night_sky(dir);
    } else if (sun_out()) {
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
    if (is_night()) {
        return night_sky_light(dir);
    }
    // The tables hold the overcast sky too (`overcast_light` in `skylight.wgsl`).
    if (sky_on()) {
        return sky_table_light(dir);
    }
    let up = clamp(dir.y, 0.0, 1.0);
    let to_sun = -frame.sun.xyz;
    let mu = clamp(dot(dir, to_sun), -1.0, 1.0);
    if (!sun_out()) {
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
