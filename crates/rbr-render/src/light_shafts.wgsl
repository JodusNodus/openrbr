// Volumetric sunlight in the air (renderer addition, `light_shafts.rs`, ours): a ray march per texel of the
// quarter-size map, from the eye along the view ray to the scene's depth, sampling the sun's shadow cascades, so the
// sun shines through the canopy and the cloud gaps in shafts looking toward and across it, in haze, fog and dust. The
// post pass's composite adds the map to the scene (`post.wgsl`, `SHAFTS`). At night `fs_beams` draws there instead the
// car's lamps' light scattered by the air (the headlights' beams in the fog), with `lamps.wgsl` appended.

struct Shafts {
    // Unused by the march (kept for the beams' uniform layout); the march's strength folds into `march_colour`.
    sun: vec4<f32>,
    // `fs_march`'s and `fs_beams`': the unjittered main view's clip space to stage space; `fs_beams`' eye and in w the
    // air's scattering per metre, and the lamps (`LampUniform`, at the share they light the stage).
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    // `fs_march`'s: xyz the direction the sunlight travels, w unused (0 when the map is cleared).
    march_dir: vec4<f32>,
    // `fs_march`'s: the sun's in-scatter colour (pre-exposed linear, strength folded in), w the frame's index for the
    // jitter.
    march_colour: vec4<f32>,
    // `fs_march`'s: the height fog (`height_fog.rs`' uniform: density at the base per metre, scale height, base height)
    // and in w the base air's density per metre (`air` in `light_shafts.rs`).
    march_fog: vec4<f32>,
    // `fs_march`'s: each cascade's far distance from the eye, metres, w how much the cascades shade (0: none).
    march_splits: vec4<f32>,
    // `fs_march`'s: stage space to each cascade's clip space.
    march_matrices: array<mat4x4<f32>, 3>,
    // `fs_march`'s: the dust grid's corner in stage space, xyz, and its cell in metres, w.
    march_grid: vec4<f32>,
    lamps: array<Lamp, 8>,
    // `fs_beams`': the headlights' shadow map (`lamp_shaft` in `lamps.wgsl`): stage space to its clip space, zero
    // with no map.
    lamp_matrix: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> shafts: Shafts;
// `fs_march`'s and `fs_beams`': the frame's depth (reversed: 0 where only sky is drawn).
@group(0) @binding(3) var scene_depth: texture_depth_2d;
// `fs_blur`'s input.
@group(0) @binding(4) var source: texture_2d<f32>;
// `fs_march`'s and `fs_beams`': the sun's map (the cascades' layers for the march, the headlights' layer for the
// beams) and its comparison sampler.
@group(0) @binding(5) var shafts_sun_map: texture_depth_2d_array;
@group(0) @binding(6) var shafts_sun_sampler: sampler_comparison;
// `fs_march`'s: the wheel spray's dust density about the eye (`dust_density`).
@group(0) @binding(7) var dust_map: texture_3d<f32>;

// Steps along the view ray, the farthest the march reaches (metres; sky texels march the whole way), the
// Henyey-Greenstein forward lobe and the isotropic share mixed into it, and the most a texel gathers (a share of the
// sun's colour, so fog in full sun stays bounded).
const MARCH_STEPS: i32 = 16;
const MARCH_REACH: f32 = 350.0;
const MARCH_FORWARD: f32 = 0.55;
const MARCH_ISOTROPIC: f32 = 0.2;
const MARCH_BRIGHTEST: f32 = 0.6;

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
    let p = vec2<f32>(pixel) + 5.588238 * f32(u32(shafts.march_colour.w) % 64u);
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// How much of the sun reaches the air at stage point `world` (1 lit, 0 shaded): the cascades as `sun_reach` in
// `effects.wgsl` takes them (1 past them or without them), times the clouds' shadow (1 until the cloud shadow map
// lands; another worker builds it, the integrator wires it in).
fn air_sun_seen(world: vec3<f32>) -> f32 {
    if (shafts.march_splits.w == 0.0) {
        return 1.0;
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
        return 1.0;
    }
    let clip = shafts.march_matrices[cascade] * vec4<f32>(world, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return 1.0;
    }
    let lit = textureSampleCompareLevel(shafts_sun_map, shafts_sun_sampler, uv, cascade, clip.z - 0.002);
    return lit * cloud_shadow(world);
}

fn cloud_shadow(world: vec3<f32>) -> f32 {
    return 1.0;
}

// The air's extinction at stage point `world`, per metre (ours): the height fog's density there, at least the base
// air's (the section's haze, the rain's mist, a faint base in clear air). The dust grid adds its share (the wheel
// spray's, `dust_density`).
fn air_density(world: vec3<f32>) -> f32 {
    let scale = max(shafts.march_fog.y, 1.0);
    let fogged = shafts.march_fog.x * exp(-(world.y - shafts.march_fog.z) / scale);
    return max(max(fogged, 0.0), shafts.march_fog.w) + dust_density(world);
}

// The dust grid about the eye, cells (`DUST_CELLS`) of `march_grid.w` metres: the wheel spray's density, per metre,
// splatted from the particle billboards each frame (`LightShafts::set_dust`). How much it counts (tuned so sunlit dust
// shows shafts without glowing brighter overall).
const DUST_CELLS: vec3<i32> = vec3<i32>(64, 32, 64);
const DUST_GAIN: f32 = 0.05;

fn dust_density(world: vec3<f32>) -> f32 {
    let half = vec3<f32>(DUST_CELLS) * 0.5 * shafts.march_grid.w;
    let centred = (world - (shafts.march_grid.xyz + half)) / (2.0 * half) + 0.5;
    if (any(centred < vec3<f32>(0.0)) || any(centred > vec3<f32>(1.0))) {
        return 0.0;
    }
    // Trilinear (`R32Float` filters nowhere): the eight corners out.
    let size = vec3<i32>(DUST_CELLS);
    let g = centred * vec3<f32>(size) - 0.5;
    let base = vec3<i32>(floor(g));
    let f = fract(g);
    let c000 = textureLoad(dust_map, clamp(base, vec3<i32>(0), size - 1), 0).r;
    let c100 = textureLoad(dust_map, clamp(base + vec3<i32>(1, 0, 0), vec3<i32>(0), size - 1), 0).r;
    let c010 = textureLoad(dust_map, clamp(base + vec3<i32>(0, 1, 0), vec3<i32>(0), size - 1), 0).r;
    let c110 = textureLoad(dust_map, clamp(base + vec3<i32>(1, 1, 0), vec3<i32>(0), size - 1), 0).r;
    let c001 = textureLoad(dust_map, clamp(base + vec3<i32>(0, 0, 1), vec3<i32>(0), size - 1), 0).r;
    let c101 = textureLoad(dust_map, clamp(base + vec3<i32>(1, 0, 1), vec3<i32>(0), size - 1), 0).r;
    let c011 = textureLoad(dust_map, clamp(base + vec3<i32>(0, 1, 1), vec3<i32>(0), size - 1), 0).r;
    let c111 = textureLoad(dust_map, clamp(base + vec3<i32>(1, 1, 1), vec3<i32>(0), size - 1), 0).r;
    return mix(
        mix(mix(c000, c100, f.x), mix(c010, c110, f.x), f.y),
        mix(mix(c001, c101, f.x), mix(c011, c111, f.x), f.y),
        f.z
    ) * DUST_GAIN;
}

// The march's scattering phase at cosine `c` between the view ray and the way to the sun: Henyey-Greenstein,
// forward-heavy as haze and fog droplets scatter, with a little isotropic mixed in (normalised so isotropic is 1, as
// `phase` in `effects.wgsl`).
fn march_phase(c: f32) -> f32 {
    let g = MARCH_FORWARD;
    let g2 = g * g;
    let lobe = (1.0 - g2) / pow(max(1.0 + g2 - 2.0 * g * c, 1e-4), 1.5);
    return mix(1.0, lobe, 1.0 - MARCH_ISOTROPIC);
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
    let phase = march_phase(dot(dir, -shafts.march_dir.xyz));
    let sun = shafts.march_colour.rgb;
    let step = reach / f32(MARCH_STEPS);
    let start = march_jitter(vec2<i32>(v.uv * vec2<f32>(textureDimensions(source)))) * step;
    var transmit = 1.0;
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < MARCH_STEPS; i++) {
        let world = eye + dir * (start + (f32(i) + 0.5) * step);
        let density = air_density(world);
        let lit = air_sun_seen(world);
        sum += transmit * sun * (density * lit) * step;
        transmit *= exp(-density * step);
    }
    let brightest = max(dot(sun, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-4) * MARCH_BRIGHTEST;
    sum *= phase;
    let luma = dot(sum, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Alpha is the texel's depth, for the composite's upsampling (`shafts_at` in `post.wgsl`).
    return vec4<f32>(sum * min(luma, brightest) / max(luma, 1e-6), depth);
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
        sum += light * swept / (wide * f32(BEAM_STEPS));
    }
    return vec4<f32>(sum * air * BEAM_AIR * 0.25, depth);
}
