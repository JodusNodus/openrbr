// Light shafts (renderer addition, `light_shafts.rs`, ours): screen-space shafts from the sun through the canopy and the
// clouds. `fs_mask` keeps, at a quarter of the frame's size, the light of the sky seen near the sun (the share of each
// 4x4 block's depth that is sky, so the foliage's dithered cut-outs average out); `fs_blur` gathers it along the line
// toward the sun, fading with each step (Mitchell, GPU Gems 3, chapter 13), into the map the post pass's composite adds
// to the scene (`post.wgsl`, `SHAFTS`).

struct Shafts {
    // The sun's place in the frame (uv), how strongly the shafts show (0: none), and the frame's width over its height.
    sun: vec4<f32>,
}

@group(0) @binding(0) var<uniform> shafts: Shafts;
@group(0) @binding(1) var linear_sampler: sampler;
// `fs_mask`'s: the scene (linear light) and its depth (reversed: 0 where only sky is drawn).
@group(0) @binding(2) var scene: texture_2d<f32>;
@group(0) @binding(3) var scene_depth: texture_depth_multisampled_2d;
// `fs_blur`'s input.
@group(0) @binding(4) var source: texture_2d<f32>;

// How far from the sun (in frame heights) the sky's light counts, the light (linear) from which it counts, all of it
// from `KNEE.y` (only the sun's glow, not the open sky, so the shafts do not veil the frame), and the most a texel's
// light counts (the sun's disc is far brighter than white under HDR).
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
