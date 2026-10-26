// Light shafts (renderer addition, `light_shafts.rs`, ours): screen-space shafts from the sun through the canopy and the
// clouds. `fs_mask` keeps, at a quarter of the frame's size, the light of the sky seen near the sun (the share of each
// 4x4 block's depth that is sky, so the foliage's cut-outs average out); `fs_blur` gathers it along the line
// toward the sun, fading with each step (Mitchell, GPU Gems 3, chapter 13), into the map the post pass's composite adds
// to the scene (`post.wgsl`, `SHAFTS`). At night `fs_beams` draws there instead the car's lamps' light scattered by the
// air (the headlights' beams in the fog), with `lamps.wgsl` appended.

struct Shafts {
    // The sun's place in the frame (uv), how strongly the shafts show (0: none), and the frame's width over its height.
    sun: vec4<f32>,
    // `fs_beams`': the frame's clip space to stage space, the eye and in w the air's scattering per metre, and the lamps
    // (`LampUniform`, at the share they light the stage).
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    lamps: array<Lamp, 8>,
}

@group(0) @binding(0) var<uniform> shafts: Shafts;
@group(0) @binding(1) var linear_sampler: sampler;
// `fs_mask`'s: the scene (linear light) and its depth (reversed: 0 where only sky is drawn).
@group(0) @binding(2) var scene: texture_2d<f32>;
@group(0) @binding(3) var scene_depth: texture_depth_2d;
// `fs_blur`'s input.
@group(0) @binding(4) var source: texture_2d<f32>;

// How far from the sun (in frame heights) the sky's light counts, the light (linear) from which it counts, all of it
// from `KNEE.y` (only the sun's glow, not the open sky, so the shafts do not veil the frame), and the most a texel's
// light counts (the sun's disc is far brighter than white).
const GLOW: f32 = 0.16;
const KNEE: vec2<f32> = vec2<f32>(0.8, 2.5);
const BRIGHTEST: f32 = 4.0;
// Steps toward the sun, the share of the way to it they cover, and how much each step's light fades.
const STEPS: i32 = 48;
const LENGTH: f32 = 0.85;
const DECAY: f32 = 0.965;

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}

@fragment
fn fs_mask(v: Out) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let corner = vec2<i32>(v.uv * size - 2.0);
    let last = vec2<i32>(size) - 1;
    var sky = 0.0;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let depth = textureLoad(scene_depth, clamp(corner + vec2<i32>(x, y), vec2<i32>(0), last), 0);
            sky += select(0.0, 1.0, depth <= 0.0);
        }
    }
    let away = (v.uv - shafts.sun.xy) * vec2<f32>(shafts.sun.w, 1.0);
    let glow = exp(-dot(away, away) / (GLOW * GLOW));
    let light = min(textureSampleLevel(scene, linear_sampler, v.uv, 0.0).rgb, vec3<f32>(BRIGHTEST));
    let bright = smoothstep(KNEE.x, KNEE.y, dot(light, vec3<f32>(0.2126, 0.7152, 0.0722)));
    return vec4<f32>(light * bright * glow * sky / 16.0, 1.0);
}

@fragment
fn fs_blur(v: Out) -> @location(0) vec4<f32> {
    let step = (shafts.sun.xy - v.uv) * LENGTH / f32(STEPS);
    var uv = v.uv;
    var fade = 1.0;
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    for (var i = 0; i < STEPS; i++) {
        sum += textureSampleLevel(source, linear_sampler, uv, 0.0).rgb * fade;
        weight += fade;
        fade *= DECAY;
        uv += step;
    }
    return vec4<f32>(sum / weight * shafts.sun.z, 1.0);
}

// The car's lamps in the air (ours): along the view ray to the stage (or `BEAM_REACH` metres into the sky), each lamp's
// light at each point (as if from `BEAM_NEAR` square metres away at the least: the light leaves a lens, not a point),
// scattered toward the eye forward-heavily (Henyey-Greenstein, `BEAM_FORWARD`, as fog droplets do, so that a beam seen
// from behind its lamp shows most; a quarter of it, as the stage's light leaves out Lambert's 1 / pi and the phase's
// 1 / (4 pi)), dimmed by the air on the way, at `BEAM_AIR` (the stage's `NIGHT_ALBEDO`; more veiled the road from the
// bonnet on main beam in fog). Each lamp's light is taken in `BEAM_STEPS` steps spread evenly in the angle the ray
// sweeps as seen from the lamp (equi-angular sampling, Kulla and Fajardo 2012), so that the metres about the lamp,
// where its light peaks, get most of them: steps spread along the ray left too few there, which changed with each
// pixel's depth (rings about the view's centre, bands at the stage's edges). Alpha is the texel's depth, for the
// composite's upsampling (`shafts_at` in `post.wgsl`).
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
        var light = vec3<f32>(0.0);
        for (var s = 0; s < BEAM_STEPS; s++) {
            let t = along + wide * tan(first + swept * (f32(s) + 0.5) / f32(BEAM_STEPS));
            let p = eye + dir * t;
            let from_lamp = p - lamp.position.xyz;
            let d2 = dot(from_lamp, from_lamp);
            let c = dot(dir, from_lamp * inverseSqrt(max(d2, 1e-6)));
            let k = 1.0 + g * g - 2.0 * g * c;
            let phase = (1.0 - g * g) / (k * sqrt(k));
            light += lamp_toward(lamp, p) * (d2 + 0.25) * phase * exp(-air * (sqrt(d2) + t));
        }
        sum += light * swept / (wide * f32(BEAM_STEPS));
    }
    return vec4<f32>(sum * air * BEAM_AIR * 0.25, depth);
}
