// Volumetric sunlight in the air (renderer addition, `light_shafts.rs`, ours): a ray march per texel of the
// quarter-size map, from the eye along the view ray to the scene's depth, sampling the sun's shadow cascades and the
// volumetric clouds' shadow map. The scene's haze and mist (`fogged` in `shader.wgsl`) already hold the sun's light
// scattered by the air as if all of it were sunlit, so the march takes the share of the frame's light the shaded air
// leaves out, as negative light: the post pass's composite adds the map to the scene (`post.wgsl`, `SHAFTS`),
// darkening the air in shadow, so the sun shows in shafts through the canopy and the cloud gaps between darker shaded
// air while open air stays as it was. At night `fs_beams` draws there instead the car's lamps' light scattered by the
// air (the headlights' beams in the fog). `lamps.wgsl` and `volume_shadow.wgsl` are appended.

struct Shafts {
    // `fs_march`'s and `fs_beams`': the unjittered main view's clip space to stage space; `fs_beams`' eye and in w the
    // air's scattering per metre, and the lamps (`LampUniform`, at the share they light the stage).
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    // `fs_march`'s: xyz the direction the sunlight travels.
    march_dir: vec4<f32>,
    // `fs_march`'s: x how strongly the sun shows (0 to 1), y the share of the sky's light the mist scatters
    // (`MIST_SKY` in `sky_lut.wgsl`), z how much the haze's own lobe adds to the mist's sun term (`MARCH_LOBE` in
    // `light_shafts.rs`), w the frame's index for the jitter.
    march_light: vec4<f32>,
    // `fs_march`'s: the glow's sun colour per metre of optical depth (pre-exposed linear, `MARCH_GLOW` and the strength
    // folded in), w the least air's density it takes, per metre (`GLOW_AIR` in `light_shafts.rs`).
    march_glow: vec4<f32>,
    // `fs_march`'s: the height fog (`height_fog.rs`' uniform: density at the base per metre, scale height, base height)
    // and in w the base air's density per metre (`air` in `light_shafts.rs`).
    march_fog: vec4<f32>,
    // `fs_march`'s: each cascade's far distance from the eye, metres, w how much the cascades shade (0: none).
    march_splits: vec4<f32>,
    // `fs_march`'s: stage space to each cascade's clip space.
    march_matrices: array<mat4x4<f32>, 3>,
    // `fs_march`'s: the volumetric clouds' shadow map's parameters (`cloud_volume::shadow_params`; zero without them).
    march_clouds: vec4<f32>,
    lamps: array<Lamp, 8>,
    // `fs_beams`': the headlights' shadow map (`lamp_shaft` in `lamps.wgsl`): stage space to its clip space, zero
    // with no map.
    lamp_matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> shafts: Shafts;
// `fs_march`'s: the frame (the post pass's scene, linear light), read through `shafts_sampler`.
@group(0) @binding(2) var scene: texture_2d<f32>;
// `fs_march`'s and `fs_beams`': the frame's depth (reversed: 0 where only sky is drawn).
@group(0) @binding(3) var scene_depth: texture_depth_2d;
// `fs_blur`'s input: the march's map.
@group(0) @binding(4) var source: texture_2d<f32>;
// `fs_march`'s and `fs_beams`': the sun's map (the cascades' layers for the march, the headlights' layer for the
// beams) and its comparison sampler.
@group(0) @binding(5) var shafts_sun_map: texture_depth_2d_array;
@group(0) @binding(6) var shafts_sun_sampler: sampler_comparison;
// `fs_march`'s: the volumetric clouds' shadow map (`volume_shadow`), and the linear sampler reading it and the frame.
@group(0) @binding(7) var shafts_cloud_map: texture_2d<f32>;
@group(0) @binding(8) var shafts_sampler: sampler;

// Steps along the view ray, spread by the square of their index (close together near the eye, where the canopy's
// shafts are, and far apart out to the cloud gaps'), the farthest the march reaches (metres; sky texels march the
// whole way), and the sun's term in the scene's mist (`MIST_SUN`, `MIST_G` and the lobe's cap in `sky_lut.wgsl`).
const MARCH_STEPS: i32 = 24;
const MARCH_REACH: f32 = 2000.0;
const MARCH_SUN: f32 = 0.03;
const MARCH_FORWARD: f32 = 0.6;
const MARCH_PHASE_CAP: f32 = 4.0;
// The glow's Henyey-Greenstein lobe, narrow about the sun, how far from the eye the air glows, metres, and the share of
// that way in shade from which it glows in full: a ray all through sunlit air (open air, the valley from a hill) has
// its light in the haze already, so the glow only shows the sun's rays among shaded air.
const GLOW_FORWARD: f32 = 0.85;
const GLOW_REACH: f32 = 350.0;
const GLOW_SHADE: f32 = 0.25;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}

// The frame-animated interleaved gradient noise (Jimenez 2014) at march texel `pixel`, 0 to 1.
fn march_jitter(pixel: vec2<i32>) -> f32 {
    let p = vec2<f32>(pixel) + 5.588238 * f32(u32(shafts.march_light.w) % 64u);
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// How much of the sun reaches the air at stage point `world` (1 lit, 0 shaded): the cascades as `sun_reach` in
// `effects.wgsl` takes them (1 past them or without them), times what the volumetric clouds let through.
fn air_sun_seen(world: vec3<f32>) -> f32 {
    let to_sun = -shafts.march_dir.xyz;
    var clouds = 1.0;
    if (to_sun.y > 0.05) {
        clouds = 1.0 - volume_shadow(shafts_cloud_map, shafts_sampler, shafts.march_clouds, world, to_sun);
    }
    if (shafts.march_splits.w == 0.0) {
        return clouds;
    }
    let reach = distance(world, shafts.eye.xyz);
    var cascade = 0;
    if (reach > shafts.march_splits.x) {
        cascade = 1;
    }
    if (reach > shafts.march_splits.y) {
        cascade = 2;
    }
    if (reach > shafts.march_splits.z) {
        return clouds;
    }
    let clip = shafts.march_matrices[cascade] * vec4<f32>(world, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return clouds;
    }
    return clouds * textureSampleCompareLevel(shafts_sun_map, shafts_sun_sampler, uv, cascade, clip.z - 0.002);
}

// The air's extinction at stage point `world`, per metre (ours): the height fog's density there, at least the base
// air's (the section's haze, the rain's mist, a faint base in clear air).
fn air_density(world: vec3<f32>) -> f32 {
    let scale = max(shafts.march_fog.y, 1.0);
    let fogged = shafts.march_fog.x * exp(-max(world.y - shafts.march_fog.z, 0.0) / scale);
    return max(fogged, shafts.march_fog.w);
}

@fragment
fn fs_march(v: Out) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(scene_depth));
    let texel = clamp(vec2<i32>(v.uv * vec2<f32>(size)), vec2<i32>(0), size - 1);
    let depth = textureLoad(scene_depth, texel, 0);
    let ndc = vec2<f32>(v.uv.x * 2.0 - 1.0, 1.0 - v.uv.y * 2.0);
    let eye = shafts.eye.xyz;
    let point = shafts.inverse_view_projection * vec4<f32>(ndc, max(depth, 1e-6), 1.0);
    let to = point.xyz / point.w - eye;
    let dist = length(to);
    let dir = to / max(dist, 1e-6);
    let reach = select(min(dist, MARCH_REACH), MARCH_REACH, depth <= 0.0);
    // The sun's share of the mist's light along `dir` (`height_fog_light`, display colour, the haze's own lobe added),
    // taken to linear light.
    let g = MARCH_FORWARD;
    let b = 1.0 + g * g - 2.0 * g * dot(dir, -shafts.march_dir.xyz);
    let sun = MARCH_SUN * (1.0 + shafts.march_light.z) * min((1.0 - g * g) / (b * sqrt(b)), MARCH_PHASE_CAP);
    let share = 1.0 - pow(shafts.march_light.y / (shafts.march_light.y + sun), 2.2);
    let jitter = march_jitter(vec2<i32>(v.uv * vec2<f32>(size / 4)));
    // How much of the view the shaded air covers, and the sunlit air's optical depth for the glow, each step's dimmed
    // by the air before it.
    var transmit = 1.0;
    var shaded = 0.0;
    var lit = 0.0;
    var near = 0.0;
    var near_shade = 0.0;
    for (var i = 0; i < MARCH_STEPS; i++) {
        let u = (f32(i) + jitter) / f32(MARCH_STEPS);
        let t = reach * u * u;
        let step = 2.0 * reach * u / f32(MARCH_STEPS);
        let world = eye + dir * t;
        let density = air_density(world);
        let seen = air_sun_seen(world);
        shaded += transmit * density * (1.0 - seen) * step;
        let glows = f32(t < GLOW_REACH);
        lit += transmit * max(density, shafts.march_glow.w) * seen * step * glows;
        near += step * glows;
        near_shade += (1.0 - seen) * step * glows;
        transmit *= exp(-density * step);
    }
    let k = 1.0 + GLOW_FORWARD * GLOW_FORWARD - 2.0 * GLOW_FORWARD * dot(dir, -shafts.march_dir.xyz);
    let among_shade = smoothstep(0.0, GLOW_SHADE, near_shade / max(near, 1e-6));
    let glow = shafts.march_glow.rgb * lit * among_shade * (1.0 - GLOW_FORWARD * GLOW_FORWARD) / (k * sqrt(k));
    // The frame's light stands for the mist's (where the mist covers little, so does the shaded air): the least of
    // four bilinear reads over the texel's 4x4 pixels, so that a dark edge in front of a bright mist is not taken
    // below black.
    let d = 1.0 / vec2<f32>(size);
    var light = textureSampleLevel(scene, shafts_sampler, v.uv + vec2<f32>(-d.x, -d.y), 0.0).rgb;
    light = min(light, textureSampleLevel(scene, shafts_sampler, v.uv + vec2<f32>(d.x, -d.y), 0.0).rgb);
    light = min(light, textureSampleLevel(scene, shafts_sampler, v.uv + vec2<f32>(-d.x, d.y), 0.0).rgb);
    light = min(light, textureSampleLevel(scene, shafts_sampler, v.uv + vec2<f32>(d.x, d.y), 0.0).rgb);
    // Alpha is the texel's depth, for the composite's upsampling (`shafts_at` in `post.wgsl`).
    return vec4<f32>(glow - max(light, vec3<f32>(0.0)) * share * shafts.march_light.x * min(shaded, 1.0), depth);
}

// The march blurred where the depth holds still (a 3x3 bilateral by the texels' depths, which the march keeps in
// alpha), so the jitter dithers out without smearing past edges.
@fragment
fn fs_blur(v: Out) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(source));
    let p = clamp(vec2<i32>(v.uv * vec2<f32>(size)), vec2<i32>(0), size - 1);
    let centre = textureLoad(source, p, 0);
    var sum = vec3<f32>(0.0);
    var depth = 0.0;
    var weight = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let t = textureLoad(source, clamp(p + vec2<i32>(x, y), vec2<i32>(0), size - 1), 0);
            let off = abs(t.a - centre.a) / max(max(t.a, centre.a), 1e-6);
            let w = exp(-off * off * 50.0) * select(0.5, 1.0, x == 0 && y == 0);
            sum += t.rgb * w;
            depth += t.a * w;
            weight += w;
        }
    }
    return vec4<f32>(sum / max(weight, 1e-6), depth / max(weight, 1e-6));
}

// The car's lamps in the air (ours): along the view ray to the stage (or `BEAM_REACH` metres into the sky), each lamp's
// light at each point (as if from `BEAM_NEAR` square metres away at the least: the light leaves a lens, not a point),
// scattered toward the eye forward-heavily (Henyey-Greenstein, `BEAM_FORWARD`, as fog droplets do, so that a beam seen
// from behind its lamp shows most; a quarter of it, as the stage's light leaves out Lambert's 1 / pi and the phase's
// 1 / (4 pi)), dimmed by the air on the way, at `BEAM_AIR` (the stage's `NIGHT_ALBEDO`; more veiled the road from the
// bonnet on main beam in fog). The headlights' beams test the shadow map at each step (`lamp_shaft` in `lamps.wgsl`),
// so things in front of the car cut shafts through them; the rear lamps stay unshadowed. Each lamp's light is taken
// in `BEAM_STEPS` steps spread evenly in the angle the ray sweeps as seen from the lamp (equi-angular sampling, Kulla
// and Fajardo 2012), so that the metres about the lamp, where its light peaks, get most of them: steps spread along
// the ray left too few there, which changed with each pixel's depth (rings about the view's centre, bands at the
// stage's edges). Alpha is the texel's depth, for the composite's upsampling (`shafts_at` in `post.wgsl`).
const BEAM_REACH: f32 = 120.0;
const BEAM_STEPS: i32 = 12;
const BEAM_FORWARD: f32 = 0.3;
const BEAM_AIR: f32 = 0.045;
const BEAM_NEAR: f32 = 9.0;
// The most the rear lamps' light in the air adds to a texel (a soft knee on their sum, after the air's scattering): a
// brake lamp's 100 cd seen from a metre or two behind would otherwise fill the frame with saturated red.
const BEAM_REAR_KNEE: f32 = 0.00005;

@fragment
fn fs_beams(v: Out) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(scene_depth));
    let depth = textureLoad(scene_depth, clamp(vec2<i32>(v.uv * vec2<f32>(size)), vec2<i32>(0), size - 1), 0);
    let ndc = vec2<f32>(v.uv.x * 2.0 - 1.0, 1.0 - v.uv.y * 2.0);
    let eye = shafts.eye.xyz;
    let point = shafts.inverse_view_projection * vec4<f32>(ndc, max(depth, 1e-6), 1.0);
    let to = point.xyz / point.w - eye;
    let dir = normalize(to);
    let reach = select(min(length(to), BEAM_REACH), BEAM_REACH, depth <= 0.0);
    let air = shafts.eye.w;
    let g = BEAM_FORWARD;
    var sum = vec3<f32>(0.0);
    var rear = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let lamp = shafts.lamps[i];
        if (lamp.position.w == 0.0) {
            continue;
        }
        // The ray's point nearest the lamp, `along` from the eye, `wide` from the lamp with the lens's spread; the
        // light falls as 1 / ((t - along)^2 + wide^2), whose integral the angles measure.
        let along = dot(lamp.position.xyz - eye, dir);
        let off = eye + dir * along - lamp.position.xyz;
        let wide = sqrt(dot(off, off) + BEAM_NEAR);
        let first = atan(-along / wide);
        let swept = atan((reach - along) / wide) - first;
        let beam_lamp = lamp.forward.w != 0.0;
        var light = vec3<f32>(0.0);
        for (var s = 0; s < BEAM_STEPS; s++) {
            let t = along + wide * tan(first + swept * (f32(s) + 0.5) / f32(BEAM_STEPS));
            let p = eye + dir * t;
            let from_lamp = p - lamp.position.xyz;
            let d2 = dot(from_lamp, from_lamp);
            let c = dot(dir, from_lamp * inverseSqrt(max(d2, 1e-6)));
            let k = 1.0 + g * g - 2.0 * g * c;
            let phase = (1.0 - g * g) / (k * sqrt(k));
            var shadow = 1.0;
            if (beam_lamp) {
                shadow = lamp_shaft(shafts_sun_map, shafts_sun_sampler, shafts.lamp_matrix, p);
            }
            light += lamp_toward(lamp, p) * (d2 + 0.25) * phase * exp(-air * (sqrt(d2) + t)) * shadow;
        }
        let lit = light * swept / (wide * f32(BEAM_STEPS)) * air * BEAM_AIR * 0.25;
        if (beam_lamp) {
            sum += lit;
        } else {
            rear += lit;
        }
    }
    return vec4<f32>(sum + BEAM_REAR_KNEE * rear / (BEAM_REAR_KNEE + rear), depth);
}
