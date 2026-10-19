// Temporal anti-aliasing (renderer addition, `post.rs`; not the game's look). Each frame is drawn with the projection
// moved by a different fraction of a pixel; this pass finds where each pixel's stage point was in the last frame (by
// the nearest depth about it and the last frame's matrix; on the car, which moves with the camera, through the car's
// motion too), takes the accumulated colour there through a Catmull-Rom filter, clips it to the spread of the colours
// around the pixel now (so moving things do not smear), and blends the new frame in by `BLEND`, more the further the
// view moved (`MOVING_BLEND`), in brightness-compressed colour (so a lone bright sample does not flicker). Over a few
// frames still edges (foliage cut-outs, thin wires, shadow edges) are resolved as if supersampled. TAA-2 measured each
// step against 4K references in motion (`lead/reports/taa2.md`).

struct Taa {
    // This frame's clip space to the last frame's, both without their jitter.
    reprojection: mat4x4<f32>,
    // The same for the car's pixels, with the car's motion since; this frame's clip space to the car's model space;
    // the car's bounds there (low above high without a car).
    car_reprojection: mat4x4<f32>,
    to_car: mat4x4<f32>,
    car_low: vec4<f32>,
    car_high: vec4<f32>,
    // x 1 when the history holds a last frame; zw this frame's jitter in its pixels (`fs_taau`).
    params: vec4<f32>,
    // `fs_taau`: the frame's width and height, and the target's.
    sizes: vec4<f32>,
    // The last frame's view-projection, and the w row of this frame's clip space to stage space (`VELOCITY`).
    previous: mat4x4<f32>,
    unproject_w: vec4<f32>,
}

@group(0) @binding(0) var current: texture_2d<f32>;
@group(0) @binding(1) var history: texture_2d<f32>;
@group(0) @binding(2) var scene_depth: texture_depth_multisampled_2d;
@group(0) @binding(3) var linear_sampler: sampler;
@group(0) @binding(4) var<uniform> taa: Taa;
// With the wind (`taa.rs`): the swaying plants' motion since the last frame along x and y, metres in stage space, zero
// where none (`vs_velocity` in `shader.wgsl`); everything else reprojects by its depth alone. The sway's z is its x
// times `WIND_Z`, as `WIND` in `shader.wgsl`.
@group(0) @binding(5) var velocity: texture_multisampled_2d<f32>;
override VELOCITY: bool = false;
const WIND_Z: f32 = 0.39 / 0.92;

// The plants' motion at frame pixel `p`: xyz, and w 1 where there is any.
fn motion_at(p: vec2<i32>) -> vec4<f32> {
    let m = textureLoad(velocity, p, 0).xy;
    return vec4<f32>(m.x, m.y, m.x * WIND_Z, f32(any(m != vec2<f32>(0.0))));
}

// The new frame's share; in motion it grows toward `MOVING_BLEND` (upsampling: `UPSAMPLE_MOVING_BLEND`) as the history
// moved up to `MOVING` pixels, where the history's resampling has blurred and lags more.
const BLEND: f32 = 0.12;
const MOVING_BLEND: f32 = 0.25;
const MOVING: f32 = 8.0;
// The clipping box's half-width in standard deviations of the colours about the pixel.
const CLIP_SIGMA: f32 = 1.0;

struct Out {
    @builtin(position) clip: vec4<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0));
}

// The history and the scene the post pass reads: the frame was drawn into a target of its own (`Taa::input`), so the
// pass writes both, with no copy.
struct Resolved {
    @location(0) history: vec4<f32>,
    @location(1) scene: vec4<f32>,
}

fn resolved(c: vec3<f32>) -> Resolved {
    return Resolved(vec4<f32>(c, 1.0), vec4<f32>(c, 1.0));
}

// Linear colour compressed by its brightness (so the sun's disc does not dominate), and back.
fn compress(c: vec3<f32>) -> vec3<f32> {
    return c / (1.0 + max(max(c.r, c.g), c.b));
}

fn expand(t: vec3<f32>) -> vec3<f32> {
    return t / max(1.0 - max(max(t.r, t.g), t.b), 1e-4);
}

// Compressed colour in YCoCg, and back.
fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    let t = compress(c);
    return vec3<f32>(
        0.25 * t.r + 0.5 * t.g + 0.25 * t.b,
        0.5 * t.r - 0.5 * t.b,
        -0.25 * t.r + 0.5 * t.g - 0.25 * t.b,
    );
}

fn from_ycocg(y: vec3<f32>) -> vec3<f32> {
    return expand(vec3<f32>(y.x + y.y - y.z, y.x + y.z, y.x - y.y - y.z));
}

// Where the point at target `uv` and `depth` was in the last frame's target uv (the car's pixels through its motion, a
// swaying plant's through its `motion`); outside 0 to 1 when it was off screen.
fn previous_uv(uv: vec2<f32>, depth: f32, motion: vec4<f32>) -> vec2<f32> {
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    let model = taa.to_car * ndc;
    let point = model.xyz / model.w;
    var before = taa.reprojection * ndc;
    if (all(point >= taa.car_low.xyz) && all(point <= taa.car_high.xyz)) {
        before = taa.car_reprojection * ndc;
    }
    if (motion.w != 0.0) {
        // `reprojection` is `previous` times this frame's clip space to stage space, whose w scales the motion too.
        before += dot(taa.unproject_w, ndc) * (taa.previous * vec4<f32>(motion.xyz, 0.0));
    }
    if (before.w <= 0.0) {
        return vec2<f32>(-1.0);
    }
    return vec2<f32>(before.x / before.w * 0.5 + 0.5, 0.5 - before.y / before.w * 0.5);
}

// The colours' box in YCoCg narrowed to their mean ± `width` standard deviations.
fn clip_low(low: vec3<f32>, mean: vec3<f32>, sigma: vec3<f32>, width: f32) -> vec3<f32> {
    return max(low, mean - width * sigma);
}

fn clip_high(high: vec3<f32>, mean: vec3<f32>, sigma: vec3<f32>, width: f32) -> vec3<f32> {
    return min(high, mean + width * sigma);
}

// The history kept (clipped, in YCoCg) blended with the colour `now` by `blend`, grown in motion by how far, in target
// pixels, the history moved, to at most `moving`; in compressed colour.
fn resolve(kept: vec3<f32>, now: vec3<f32>, blend: f32, moving: f32, moved: f32) -> vec3<f32> {
    let share = mix(blend, max(blend, moving), clamp(moved / MOVING, 0.0, 1.0));
    return expand(mix(compress(from_ycocg(kept)), compress(now), share));
}

// The 3x3 pixels about `pixel` (clamped to the frame's `size`), row by row, and the frame's colours and depths there.
// The loads are written out rather than looped over, so that they overlap: the loop cost a fifth of the pass at 4K on
// the Mac (TAA-3).
fn around(pixel: vec2<i32>, size: vec2<i32>) -> array<vec2<i32>, 9> {
    let x0 = max(pixel.x - 1, 0);
    let x2 = min(pixel.x + 1, size.x - 1);
    let y0 = max(pixel.y - 1, 0);
    let y2 = min(pixel.y + 1, size.y - 1);
    let x = pixel.x;
    let y = pixel.y;
    return array<vec2<i32>, 9>(
        vec2<i32>(x0, y0), vec2<i32>(x, y0), vec2<i32>(x2, y0),
        vec2<i32>(x0, y), vec2<i32>(x, y), vec2<i32>(x2, y),
        vec2<i32>(x0, y2), vec2<i32>(x, y2), vec2<i32>(x2, y2),
    );
}

fn colours(p: array<vec2<i32>, 9>) -> array<vec3<f32>, 9> {
    return array<vec3<f32>, 9>(
        textureLoad(current, p[0], 0).rgb, textureLoad(current, p[1], 0).rgb, textureLoad(current, p[2], 0).rgb,
        textureLoad(current, p[3], 0).rgb, textureLoad(current, p[4], 0).rgb, textureLoad(current, p[5], 0).rgb,
        textureLoad(current, p[6], 0).rgb, textureLoad(current, p[7], 0).rgb, textureLoad(current, p[8], 0).rgb,
    );
}

fn depths(p: array<vec2<i32>, 9>) -> array<f32, 9> {
    return array<f32, 9>(
        textureLoad(scene_depth, p[0], 0), textureLoad(scene_depth, p[1], 0), textureLoad(scene_depth, p[2], 0),
        textureLoad(scene_depth, p[3], 0), textureLoad(scene_depth, p[4], 0), textureLoad(scene_depth, p[5], 0),
        textureLoad(scene_depth, p[6], 0), textureLoad(scene_depth, p[7], 0), textureLoad(scene_depth, p[8], 0),
    );
}

// The colours' spread in YCoCg: their box, mean and standard deviation. Written out, as the loads are: in a loop the
// arrays spilled, which cost 60% of the upsampling's box pass and 40% of `fs_taa` at 4K on the Mac (TAAU-perf; `fs_taa`'s
// result moved by a level or two in 0.2% of the pixels, as the compiler fuses the sums differently).
struct Spread {
    low: vec3<f32>,
    high: vec3<f32>,
    mean: vec3<f32>,
    sigma: vec3<f32>,
}

fn spread(colours: array<vec3<f32>, 9>) -> Spread {
    let c0 = to_ycocg(colours[0]);
    let c1 = to_ycocg(colours[1]);
    let c2 = to_ycocg(colours[2]);
    let c3 = to_ycocg(colours[3]);
    let c4 = to_ycocg(colours[4]);
    let c5 = to_ycocg(colours[5]);
    let c6 = to_ycocg(colours[6]);
    let c7 = to_ycocg(colours[7]);
    let c8 = to_ycocg(colours[8]);
    let low = min(min(min(min(c0, c1), min(c2, c3)), min(min(c4, c5), min(c6, c7))), c8);
    let high = max(max(max(max(c0, c1), max(c2, c3)), max(max(c4, c5), max(c6, c7))), c8);
    let mean = (c0 + c1 + c2 + c3 + c4 + c5 + c6 + c7 + c8) / 9.0;
    let square = c0 * c0 + c1 * c1 + c2 * c2 + c3 * c3 + c4 * c4 + c5 * c5 + c6 * c6 + c7 * c7 + c8 * c8;
    return Spread(low, high, mean, sqrt(max(square / 9.0 - mean * mean, vec3<f32>(0.0))));
}

// The nearest of the depths (reversed depth: the largest).
fn nearest_depth(d: array<f32, 9>) -> f32 {
    return max(max(max(max(max(max(max(max(d[0], d[1]), d[2]), d[3]), d[4]), d[5]), d[6]), d[7]), d[8]);
}

// The plants' motion at the nearest of the 3x3 `around` (whose depth is `depth`), or none. Written out: a loop here
// cost a third of the pass at 4K on the Mac.
fn motion(around: array<vec2<i32>, 9>, depths: array<f32, 9>, depth: f32) -> vec4<f32> {
    if (!VELOCITY) {
        return vec4<f32>(0.0);
    }
    var at = around[8];
    at = select(at, around[7], depths[7] == depth);
    at = select(at, around[6], depths[6] == depth);
    at = select(at, around[5], depths[5] == depth);
    at = select(at, around[4], depths[4] == depth);
    at = select(at, around[3], depths[3] == depth);
    at = select(at, around[2], depths[2] == depth);
    at = select(at, around[1], depths[1] == depth);
    at = select(at, around[0], depths[0] == depth);
    return motion_at(at);
}

@fragment
fn fs_taa(v: Out) -> Resolved {
    let size = vec2<i32>(textureDimensions(current));
    let pixel = vec2<i32>(v.clip.xy);
    let now = textureLoad(current, pixel, 0).rgb;
    if (taa.params.x == 0.0) {
        return resolved(now);
    }
    // The colours around the pixel now, and the nearest surface among them (reversed depth: the largest), whose
    // motion an edge follows.
    let around = around(pixel, size);
    let colours = colours(around);
    let depths = depths(around);
    let depth = nearest_depth(depths);
    let uv = (vec2<f32>(pixel) + 0.5) / vec2<f32>(size);
    let at = previous_uv(uv, depth, motion(around, depths, depth));
    if (any(at < vec2<f32>(0.0)) || any(at > vec2<f32>(1.0))) {
        return resolved(now);
    }
    let s = spread(colours);
    let kept = clamp(
        to_ycocg(history_sharp(at)),
        clip_low(s.low, s.mean, s.sigma, CLIP_SIGMA),
        clip_high(s.high, s.mean, s.sigma, CLIP_SIGMA),
    );
    let moved = length((at - uv) * vec2<f32>(size));
    return resolved(resolve(kept, now, BLEND, MOVING_BLEND, moved));
}

// Upsampling (`taa.rs`, render scale below 1): the frame (`current`) is smaller than the target and the history. Each
// target pixel takes the frame's 3x3 samples about it by their jittered positions' distance, Gaussian weighted; the
// history comes through a Catmull-Rom fetch (sharp under motion) clipped to the samples' spread in YCoCg; and the
// samples blend in by how near the nearest one lies, so each target pixel is built from the frames whose jitter put a
// sample on it. The spread and the nearest depth depend only on the frame pixel the 3x3 is about, so `fs_taau_box`
// works them out once per frame pixel, and the weights are separable, so `fs_taau` takes the weighted sum in 3
// bilinear fetches and a load (TAAU-perf: the per-target-pixel 3x3 cost about 2.5 ms of 4 at 4K on the Mac).
const UPSAMPLE_BLEND: f32 = 0.12;
const UPSAMPLE_MOVING_BLEND: f32 = 0.4;
// Wider than at full size: fewer of the samples lie near the pixel.
const UPSAMPLE_CLIP_SIGMA: f32 = 1.25;

// Upsampling, per frame pixel (`fs_taau_box`): the clipping box's low corner, and its high corner and which of the 3x3
// (row by row) is the nearest.
@group(1) @binding(0) var box_low: texture_2d<f32>;
@group(1) @binding(1) var box_high: texture_2d<f32>;

struct Box {
    @location(0) low: vec4<f32>,
    @location(1) high: vec4<f32>,
}

// Bilinear sample of the history at target uv `at` (clamped at the edges by the sampler).
fn history_at(at: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(history, linear_sampler, at, 0.0).rgb;
}

// The history at `at` through a Catmull-Rom filter, in 5 bilinear fetches (the corners left out).
fn history_sharp(at: vec2<f32>) -> vec3<f32> {
    let size = taa.sizes.zw;
    let position = at * size;
    let centre = floor(position - 0.5) + 0.5;
    let f = position - centre;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let t0 = (centre - 1.0) / size;
    let t3 = (centre + 2.0) / size;
    let t12 = (centre + w2 / w12) / size;
    let sum = history_at(vec2<f32>(t12.x, t0.y)) * (w12.x * w0.y)
        + history_at(vec2<f32>(t0.x, t12.y)) * (w0.x * w12.y)
        + history_at(t12) * (w12.x * w12.y)
        + history_at(vec2<f32>(t3.x, t12.y)) * (w3.x * w12.y)
        + history_at(vec2<f32>(t12.x, t3.y)) * (w12.x * w3.y);
    let weight = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(sum / weight, vec3<f32>(0.0));
}

@fragment
fn fs_taau_box(v: Out) -> Box {
    let pixel = vec2<i32>(v.clip.xy);
    let around = around(pixel, vec2<i32>(textureDimensions(current)));
    let colours = colours(around);
    let depths = depths(around);
    let depth = nearest_depth(depths);
    // The first of the nearest, written out as `motion` is.
    var nearest = 8.0;
    nearest = select(nearest, 7.0, depths[7] == depth);
    nearest = select(nearest, 6.0, depths[6] == depth);
    nearest = select(nearest, 5.0, depths[5] == depth);
    nearest = select(nearest, 4.0, depths[4] == depth);
    nearest = select(nearest, 3.0, depths[3] == depth);
    nearest = select(nearest, 2.0, depths[2] == depth);
    nearest = select(nearest, 1.0, depths[1] == depth);
    nearest = select(nearest, 0.0, depths[0] == depth);
    let s = spread(colours);
    let width = UPSAMPLE_CLIP_SIGMA;
    return Box(
        vec4<f32>(clip_low(s.low, s.mean, s.sigma, width), 0.0),
        vec4<f32>(clip_high(s.high, s.mean, s.sigma, width), nearest),
    );
}

// The weights of the frame pixels -1, 0 and 1 from the nearest along one axis, whose centres lie `d` target pixels
// from the target pixel's, `step` apart.
fn weights(d: f32, step: f32) -> vec3<f32> {
    let e = vec3<f32>(d - step, d, d + step);
    return exp(-2.29 * e * e);
}

@fragment
fn fs_taau(v: Out) -> Resolved {
    let input = vec2<i32>(taa.sizes.xy);
    let ratio = taa.sizes.xy / taa.sizes.zw;
    let jitter = taa.params.zw;
    // The target pixel's centre in the frame's pixels; the frame pixel `i` sampled the unjittered point `i + 0.5 -
    // jitter`.
    let centre = v.clip.xy * ratio;
    let nearest = vec2<i32>(floor(centre + jitter));
    // The 3x3's weights, x times y, by the distance in target pixels: the pixels -1 and 0 together in a bilinear fetch
    // between them (the sampler clamps at the edges as the loads do), and 1 alone.
    let d = (vec2<f32>(nearest) + 0.5 - jitter - centre) / ratio;
    let x = weights(d.x, 1.0 / ratio.x);
    let y = weights(d.y, 1.0 / ratio.y);
    let pairs = vec2<f32>(x.x + x.y, y.x + y.y);
    let between = (vec2<f32>(nearest) - 0.5 + vec2<f32>(x.y, y.y) / pairs) / taa.sizes.xy;
    let after = (vec2<f32>(nearest) + 1.5) / taa.sizes.xy;
    let sum = textureSampleLevel(current, linear_sampler, between, 0.0).rgb * (pairs.x * pairs.y)
        + textureSampleLevel(current, linear_sampler, vec2<f32>(after.x, between.y), 0.0).rgb * (x.z * pairs.y)
        + textureSampleLevel(current, linear_sampler, vec2<f32>(between.x, after.y), 0.0).rgb * (pairs.x * y.z)
        + textureLoad(current, min(nearest + 1, input - 1), 0).rgb * (x.z * y.z);
    let now = sum / ((pairs.x + x.z) * (pairs.y + y.z));
    let closest = max(max(x.x, x.y), x.z) * max(max(y.x, y.y), y.z);
    if (taa.params.x == 0.0) {
        return resolved(now);
    }
    let at_box = clamp(nearest, vec2<i32>(0), input - 1);
    let low = textureLoad(box_low, at_box, 0);
    let high = textureLoad(box_high, at_box, 0);
    let uv = v.clip.xy / taa.sizes.zw;
    // The nearest surface about the pixel (reversed depth: the largest), whose motion its edge follows.
    let i = i32(high.w);
    let surface = clamp(at_box + vec2<i32>(i % 3 - 1, i / 3 - 1), vec2<i32>(0), input - 1);
    var moving = vec4<f32>(0.0);
    if (VELOCITY) {
        moving = motion_at(surface);
    }
    let at = previous_uv(uv, textureLoad(scene_depth, surface, 0), moving);
    if (any(at < vec2<f32>(0.0)) || any(at > vec2<f32>(1.0))) {
        return resolved(now);
    }
    let kept = clamp(to_ycocg(history_sharp(at)), low.xyz, high.xyz);
    let moved = length((at - uv) * taa.sizes.zw);
    let blend = UPSAMPLE_BLEND * closest;
    return resolved(resolve(kept, now, blend, UPSAMPLE_MOVING_BLEND * closest, moved));
}
