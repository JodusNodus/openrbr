// HDR post pass (renderer addition, `post.rs`; not the game's look). The scene arrives as linear colour in a float
// target (`graded` in `shader.wgsl`). A bloom chain halves it (`fs_down`, the 13-tap filter of Jimenez 2014) and adds
// the levels back up (`fs_up`, a 3x3 tent); `cs_histogram` bins the scene's light and `cs_meter` sets the exposure
// from the histogram, eased over time as an eye adapts; `fs_composite` blends a little of the bloom in, exposes, and applies AgX in place of
// the renderer's tone curve.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(1) @binding(0) var bloom: texture_2d<f32>;

struct Exposure {
    // The exposure the composite applies.
    value: f32,
}

// `cs_meter`'s settings (`post::Meter`).
struct Meter {
    // Seconds since the last frame's metering; 0 to set the exposure outright (the first frame, a jump in time).
    step: f32,
    snap: f32,
    // In the in-car view the depth (reversed) nearer than which the frame is the cabin; 0 outside the car.
    cabin_depth: f32,
    pad: f32,
}

@group(1) @binding(1) var<storage, read> exposure: Exposure;
// The light shafts (`light_shafts.wgsl`), added with `SHAFTS` (a pipeline override, so the composite without them is
// built as before).
@group(1) @binding(2) var shafts_map: texture_2d<f32>;
// The frame's depth (reversed), which the shafts' depth (their alpha) is matched against (`shafts_at`).
@group(1) @binding(3) var composite_depth: texture_depth_multisampled_2d;
override SHAFTS: bool = false;

// `cs_histogram`'s and `cs_meter`'s, with the scene in `source`. `cs_meter` empties the histogram for the next frame.
@group(1) @binding(2) var meter_depth: texture_depth_multisampled_2d;
@group(1) @binding(3) var<storage, read_write> meter_exposure: Exposure;
@group(1) @binding(4) var<uniform> meter: Meter;
@group(1) @binding(5) var<storage, read_write> meter_histogram: array<atomic<u32>, BINS>;

// How much of the frame the bloom replaces.
const BLOOM: f32 = 0.06;
// The bloom chain's levels (`post::LEVELS`); `fs_up` sums them.
const LEVELS: f32 = 6.0;
// The metering (`cs_histogram`, `cs_meter`): the scene's light on a grid of `METER_GRID` points binned by its log2 in `BINS` bins
// from `EV_LOW` over `EV_SPAN` stops; the mean of the bins between the `PERCENTILES` (the darkest shadows and the
// brightest highlights dropped) is the frame's light. In the in-car view the cabin counts `CABIN_WEIGHT` of the
// outside, so the exposure follows what is seen through the glass and a dark cabin lifts it only a little.
const METER_GRID: vec2<u32> = vec2<u32>(128u, 64u);
const BINS: u32 = 64u;
const EV_LOW: f32 = -10.0;
const EV_SPAN: f32 = 16.0;
const PERCENTILES: vec2<f32> = vec2<f32>(0.1, 0.9);
const WEIGHT: u32 = 5u;
const CABIN_WEIGHT: u32 = 1u;
// The light the exposure aims for, how far toward it it goes (0 none, 1 all; the stages are lit game-bright, so a
// dark forest stays darker than an open snowfield), and its limits. The metered light runs from about 0.14 (open
// snow by day) through 0.01 (dense forest) to 0.001 (night).
const KEY: f32 = 0.124;
const ADAPT: f32 = 0.35;
const EXPOSURE: vec2<f32> = vec2<f32>(0.6, 2.6);
// How fast the exposure follows the frame, per second: faster down into a bright frame than up into a dark one.
const ADAPT_SPEED: vec2<f32> = vec2<f32>(3.0, 1.2);

// AgX (Sobotka; the polynomial fit of Wrensch 2023) on the exposed linear light, pre-scaled by `AGX_SCALE` and given
// `AGX_PUNCH` contrast so that the display middle grey stays near where the plain renderer's curve puts it, then
// `SATURATION`. Highlights desaturate toward white rather than clipping per channel.
const AGX_SCALE: f32 = 2.0;
const AGX_PUNCH: f32 = 1.25;
const SATURATION: f32 = 1.1;

fn agx_contrast(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

fn agx(linear: vec3<f32>) -> vec3<f32> {
    let inset = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104),
    );
    let outset = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),
    );
    let ev = vec2<f32>(-12.47393, 4.026069);
    let encoded = (clamp(log2(max(inset * linear, vec3<f32>(1e-10))), vec3<f32>(ev.x), vec3<f32>(ev.y)) - ev.x)
        / (ev.y - ev.x);
    let curved = pow(clamp(agx_contrast(encoded), vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(AGX_PUNCH));
    return clamp(outset * curved, vec3<f32>(0.0), vec3<f32>(1.0));
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}

fn tap(uv: vec2<f32>, offset: vec2<f32>) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    // Clamped, so that the sun's disc does not flood the chain.
    return min(textureSampleLevel(source, linear_sampler, uv + offset * texel, 0.0).rgb, vec3<f32>(32.0));
}

@fragment
fn fs_down(v: Out) -> @location(0) vec4<f32> {
    let a = tap(v.uv, vec2<f32>(-2.0, -2.0));
    let b = tap(v.uv, vec2<f32>(0.0, -2.0));
    let c = tap(v.uv, vec2<f32>(2.0, -2.0));
    let d = tap(v.uv, vec2<f32>(-2.0, 0.0));
    let e = tap(v.uv, vec2<f32>(0.0, 0.0));
    let f = tap(v.uv, vec2<f32>(2.0, 0.0));
    let g = tap(v.uv, vec2<f32>(-2.0, 2.0));
    let h = tap(v.uv, vec2<f32>(0.0, 2.0));
    let i = tap(v.uv, vec2<f32>(2.0, 2.0));
    let j = tap(v.uv, vec2<f32>(-1.0, -1.0));
    let k = tap(v.uv, vec2<f32>(1.0, -1.0));
    let l = tap(v.uv, vec2<f32>(-1.0, 1.0));
    let m = tap(v.uv, vec2<f32>(1.0, 1.0));
    let sum = e * 0.125 + (a + c + g + i) * 0.03125 + (b + d + f + h) * 0.0625 + (j + k + l + m) * 0.125;
    return vec4<f32>(sum, 1.0);
}

@fragment
fn fs_up(v: Out) -> @location(0) vec4<f32> {
    var sum = tap(v.uv, vec2<f32>(0.0, 0.0)) * 4.0;
    sum += (tap(v.uv, vec2<f32>(-1.0, 0.0)) + tap(v.uv, vec2<f32>(1.0, 0.0))) * 2.0;
    sum += (tap(v.uv, vec2<f32>(0.0, -1.0)) + tap(v.uv, vec2<f32>(0.0, 1.0))) * 2.0;
    sum += tap(v.uv, vec2<f32>(-1.0, -1.0)) + tap(v.uv, vec2<f32>(1.0, -1.0));
    sum += tap(v.uv, vec2<f32>(-1.0, 1.0)) + tap(v.uv, vec2<f32>(1.0, 1.0));
    return vec4<f32>(sum / 16.0, 1.0);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

var<workgroup> histogram: array<atomic<u32>, BINS>;

// Bins the scene's light (`source`) into `meter_histogram`: a workgroup for each 16x16 of the grid's points, each
// binning its own before adding them in.
@compute @workgroup_size(16, 16)
fn cs_histogram(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) index: u32) {
    if (index < BINS) {
        atomicStore(&histogram[index], 0u);
    }
    workgroupBarrier();
    if (all(id.xy < METER_GRID)) {
        let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(METER_GRID);
        let l = luminance(textureSampleLevel(source, linear_sampler, uv, 0.0).rgb);
        let bin = u32(clamp((log2(max(l, 1e-6)) - EV_LOW) / EV_SPAN * f32(BINS), 0.0, f32(BINS - 1u)));
        let depth = textureLoad(meter_depth, vec2<i32>(uv * vec2<f32>(textureDimensions(meter_depth))), 0);
        let cabin = meter.cabin_depth > 0.0 && depth > meter.cabin_depth;
        atomicAdd(&histogram[bin], select(WEIGHT, CABIN_WEIGHT, cabin));
    }
    workgroupBarrier();
    if (index < BINS) {
        let count = atomicLoad(&histogram[index]);
        if (count != 0u) {
            atomicAdd(&meter_histogram[index], count);
        }
    }
}

var<workgroup> counts: array<u32, BINS>;

// The exposure from `cs_histogram`'s histogram, eased from the last frame's (`meter_exposure`).
@compute @workgroup_size(64)
fn cs_meter(@builtin(local_invocation_index) index: u32) {
    if (index < BINS) {
        counts[index] = atomicExchange(&meter_histogram[index], 0u);
    }
    workgroupBarrier();
    if (index != 0u) {
        return;
    }
    var total = 0.0;
    for (var bin = 0u; bin < BINS; bin++) {
        total += f32(counts[bin]);
    }
    // The mean log2 light of the weight between the percentiles.
    let range = PERCENTILES * total;
    var below = 0.0;
    var sum = 0.0;
    var counted = 0.0;
    for (var bin = 0u; bin < BINS; bin++) {
        let w = f32(counts[bin]);
        let kept = max(min(below + w, range.y) - max(below, range.x), 0.0);
        sum += kept * (EV_LOW + (f32(bin) + 0.5) / f32(BINS) * EV_SPAN);
        counted += kept;
        below += w;
    }
    let light = exp2(sum / max(counted, 1.0));
    let aim = clamp(pow(KEY / max(light, 1e-3), ADAPT), EXPOSURE.x, EXPOSURE.y);
    let last = meter_exposure.value;
    if (meter.snap != 0.0 || !(last > 0.0)) {
        meter_exposure.value = aim;
        return;
    }
    let speed = select(ADAPT_SPEED.y, ADAPT_SPEED.x, aim < last);
    meter_exposure.value = exp2(mix(log2(last), log2(aim), 1.0 - exp(-meter.step * speed)));
}

// The shafts' map (a quarter of the frame's size) at `uv`: its four texels around `uv` weighted bilinearly and by how
// near their depth (alpha: `fs_beams` keeps its texel's, the sun's shafts all the same) is to the frame's here, so that
// the lamps' beams stop at the edges of what stands in front of them.
fn shafts_at(uv: vec2<f32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(shafts_map));
    let p = uv * vec2<f32>(size) - 0.5;
    let base = vec2<i32>(floor(p));
    let f = fract(p);
    let a = textureLoad(shafts_map, clamp(base, vec2<i32>(0), size - 1), 0);
    let b = textureLoad(shafts_map, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), size - 1), 0);
    let c = textureLoad(shafts_map, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), size - 1), 0);
    let d = textureLoad(shafts_map, clamp(base + vec2<i32>(1, 1), vec2<i32>(0), size - 1), 0);
    var w = vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    // Four texels of one depth (the sun's shafts, a cleared map) share the depth weight, which cancels: the plain
    // bilinear sum, without the frame's depth, the costly read.
    let texel_depth = vec4<f32>(a.a, b.a, c.a, d.a);
    if (any(texel_depth != vec4<f32>(a.a))) {
        let depth_size = vec2<i32>(textureDimensions(composite_depth));
        let depth = textureLoad(composite_depth, clamp(vec2<i32>(uv * vec2<f32>(depth_size)), vec2<i32>(0), depth_size - 1), 0);
        let off = abs(texel_depth - depth) / max(max(texel_depth, vec4<f32>(depth)), vec4<f32>(1e-6));
        w *= exp(-off * off * 100.0) + 1e-3;
    }
    let sum = a.rgb * w.x + b.rgb * w.y + c.rgb * w.z + d.rgb * w.w;
    return sum / max(w.x + w.y + w.z + w.w, 1e-6);
}

@fragment
fn fs_composite(v: Out) -> @location(0) vec4<f32> {
    let scene = textureSampleLevel(source, linear_sampler, v.uv, 0.0).rgb;
    let glow = textureSampleLevel(bloom, linear_sampler, v.uv, 0.0).rgb / LEVELS;
    var lit = mix(scene, glow, BLOOM);
    if (SHAFTS) {
        lit += shafts_at(v.uv);
    }
    let linear = lit * exposure.value;
    let c = agx(max(linear, vec3<f32>(0.0)) * AGX_SCALE);
    let grey = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(clamp(mix(vec3<f32>(grey), c, SATURATION), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
