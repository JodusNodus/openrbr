// Temporal anti-aliasing (`taa.rs`; ours, after gp4's): one full-screen pass at the frame's size, between the scene
// and the post pass. The scene (`scene`) was drawn with its projection shifted by a sub-pixel jitter, so the history
// gathers samples spread over each pixel. For each pixel:
//   * the current colour and its four edge neighbours, in a tone-mapped space (c / (1 + luma)) so that a single bright
//     sample cannot dominate;
//   * motion: where something that moves of itself was drawn, the motion the renderer's motion pass wrote
//     (`motion.wgsl`), else the point from the depth (reversed) carried by the unjittered view-projections, current to
//     previous;
//   * history: the previous output there (Catmull-Rom, 5 bilinear taps), clipped toward the neighbourhood's mean in
//     YCoCg (variance clipping, Salvi 2016);
//   * blend: a tenth of the new frame, more where the history was clipped hard or the pixel moves fast; all of it
//     after a reset (a cut, a resize).
// The car's glass (its blended draws, the panes' mud) moves with the car and what shows through it with the stage: its
// opacity in the frame's alpha (`pipelines::alpha_writes`) is a mask, kept in the history's alpha, and where the
// glass is opaque now or was there the frame is taken new; not where the car is behind it (the dash below the
// windscreen, the cabin behind the rear window, a lamp's housing), which moves with the glass and what it reflects.
// The result goes to the post pass's scene and to the next frame's history.
// Drawn below the output's size, `fs_upscale` resolves at the output's size instead (after Qualcomm's Snapdragon Game
// Super Resolution 2, BSD-3): see there.

struct Taa {
    // The current frame's clip space (unjittered) to the previous frame's, for a point that stays put.
    view_to_prev: mat4x4<f32>,
    // The frame's size: w, h, 1/w, 1/h.
    size: vec4<f32>,
    // The output's (the history's): w, h, 1/w, 1/h; the frame's at its size.
    output: vec4<f32>,
    // xy the frame's jitter, its pixels (x right, y down); z the output's pixels a frame pixel; w the upscale's new
    // frame's share per unit of its samples' weight (`taa.rs`).
    jitter: vec4<f32>,
    // x the new frame's base share, y the clip box's half size (sigmas), z 1 = reset, w 1 = show the motion, 2 = the
    // mask.
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> t: Taa;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_2d;
@group(0) @binding(3) var history: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;
// The motion pass's (`renderer/motion.rs`): xy the motion in uv, z 0 where nothing moving was drawn, 1 where something
// was, 2 the car's meshes.
@group(0) @binding(5) var motion: texture_2d<f32>;
// Upscaling, the frame with the rain and the particles (`scene` is it before them, `Taa::keep`); else the frame.
@group(0) @binding(6) var over: texture_2d<f32>;

struct Out {
    @builtin(position) position: vec4<f32>,
};

struct Resolved {
    // The post pass's scene, and the next frame's history (the same).
    @location(0) frame: vec4<f32>,
    @location(1) history: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0));
}

// The glass mask's alpha from which the frame is taken more, and from which wholly new.
const GLASS: vec2<f32> = vec2<f32>(0.5, 0.9);

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

fn to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}

fn from_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}

// Reversible tone map (Karis): the HDR highlights' single bright samples do not flicker the resolve.
fn tone(c: vec3<f32>) -> vec3<f32> {
    return c / (1.0 + luma(c));
}

fn untone(c: vec3<f32>) -> vec3<f32> {
    return c / max(1.0 - luma(c), 1.0e-4);
}

fn load_scene(pixel: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(textureDimensions(scene)) - 1;
    // The float target holds what the passes wrote, NaN included: kept to 0..64.
    let c = textureLoad(scene, clamp(pixel, vec2<i32>(0), last), 0).rgb;
    return clamp(select(c, vec3<f32>(0.0), c != c), vec3<f32>(0.0), vec3<f32>(64.0));
}

// `image` (`texels`: w, h, 1/w, 1/h) at `uv`, Catmull-Rom from 5 bilinear taps (Jimenez 2016).
fn catmull_rom(image: texture_2d<f32>, uv: vec2<f32>, texels: vec4<f32>) -> vec4<f32> {
    let px = uv * texels.xy;
    let c = floor(px - 0.5) + 0.5;
    let f = px - c;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let p0 = (c - 1.0) * texels.zw;
    let p3 = (c + 2.0) * texels.zw;
    let p12 = (c + w2 / w12) * texels.zw;
    var r = textureSampleLevel(image, linear_sampler, vec2<f32>(p12.x, p0.y), 0.0) * (w12.x * w0.y);
    r += textureSampleLevel(image, linear_sampler, vec2<f32>(p0.x, p12.y), 0.0) * (w0.x * w12.y);
    r += textureSampleLevel(image, linear_sampler, p12, 0.0) * (w12.x * w12.y);
    r += textureSampleLevel(image, linear_sampler, vec2<f32>(p3.x, p12.y), 0.0) * (w3.x * w12.y);
    r += textureSampleLevel(image, linear_sampler, vec2<f32>(p12.x, p3.y), 0.0) * (w12.x * w3.y);
    let sum = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(r / max(sum, 1.0e-4), vec4<f32>(0.0));
}

// The history at `uv`, with its glass mask in alpha.
fn history_at(uv: vec2<f32>) -> vec4<f32> {
    return catmull_rom(history, uv, t.output);
}

// `h` clipped toward the box's centre `c` +- `e`, along the line to the centre.
fn clip_box(h: vec3<f32>, c: vec3<f32>, e: vec3<f32>) -> vec3<f32> {
    let d = h - c;
    let u = abs(d) / max(e, vec3<f32>(1.0e-5));
    let m = max(u.x, max(u.y, u.z));
    return select(h, c + d / m, m > 1.0);
}

// Where the point seen at pixel `pixel`, `uv` (clip `here`), was in the previous frame, uv, and z 1 if it is the
// car's.
fn previous_uv(pixel: vec2<i32>, uv: vec2<f32>, here: vec4<f32>) -> vec3<f32> {
    let moved = textureLoad(motion, pixel, 0).xyz;
    if (moved.z > 0.5) {
        return vec3<f32>(uv + moved.xy, select(0.0, 1.0, moved.z > 1.5));
    }
    let clip = t.view_to_prev * here;
    let ndc = clip.xy / max(clip.w, 1.0e-6);
    return vec3<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, 0.0);
}

@fragment
fn fs_taa(in: Out) -> Resolved {
    let pixel = vec2<i32>(in.position.xy);
    let uv = in.position.xy * t.size.zw;
    // The pixel and its four edge neighbours: the clip's colour box.
    let s0 = tone(load_scene(pixel));
    let mask = clamp(textureLoad(scene, pixel, 0).a, 0.0, 1.0);
    if (t.params.w > 1.5) {
        let c = vec4<f32>(vec3<f32>(mask), mask);
        return Resolved(c, c);
    }
    let fresh = vec4<f32>(untone(s0), mask);
    if (t.params.z > 0.5) {
        return Resolved(fresh, fresh);
    }
    let y0 = to_ycocg(s0);
    let y1 = to_ycocg(tone(load_scene(pixel + vec2<i32>(1, 0))));
    let y2 = to_ycocg(tone(load_scene(pixel - vec2<i32>(1, 0))));
    let y3 = to_ycocg(tone(load_scene(pixel + vec2<i32>(0, 1))));
    let y4 = to_ycocg(tone(load_scene(pixel - vec2<i32>(0, 1))));
    let m1 = (y0 + y1 + y2 + y3 + y4) * 0.2;
    let m2 = (y0 * y0 + y1 * y1 + y2 * y2 + y3 * y3 + y4 * y4) * 0.2;
    // The pixel's centre, unjittered: the history has no jitter, so a still view keeps it in place.
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let here = vec4<f32>(ndc, textureLoad(depth, pixel, 0), 1.0);
    let previous = previous_uv(pixel, uv, here);
    let prev = previous.xy;
    let motion = prev - uv;
    if (t.params.w > 0.5) {
        let c = vec4<f32>(abs(motion) * 50.0, 0.0, mask);
        return Resolved(c, c);
    }
    if (any(prev < vec2<f32>(0.0)) || any(prev > vec2<f32>(1.0))) {
        return Resolved(fresh, fresh);
    }
    let kept = history_at(prev);
    let held = to_ycocg(tone(kept.rgb));
    let sigma = sqrt(max(m2 - m1 * m1, vec3<f32>(0.0)));
    let clipped = clip_box(held, m1, sigma * t.params.y + vec3<f32>(1.0e-3));
    // The new frame's share: the base, more where the history moved a lot (uncovered) or the pixel moves fast
    // (resampling blurs).
    let moved = length((clipped - held) / max(m1.x, 0.02));
    let speed = length(motion * t.size.xy);
    var share = clamp(t.params.x + 0.25 * clamp(moved, 0.0, 1.0) + 0.1 * clamp(speed / 16.0, 0.0, 1.0), 0.03, 1.0);
    // Clean glass (alpha below GLASS.x) gathers as the rest; mud and drops (GLASS.y) are taken new, but not over the car,
    // which moves with them.
    share = max(share, smoothstep(GLASS.x, GLASS.y, max(mask, kept.a)) * (1.0 - previous.z));
    let c = vec4<f32>(untone(mix(from_ycocg(clipped), s0, share)), mask);
    return Resolved(c, c);
}

// The upscale (`fs_upscale`, the frame drawn below the output's size; after Qualcomm's Snapdragon Game Super Resolution
// 2): the history is the output's size, and each of its pixels gathers the frame's 3x3 samples about it, weighted by
// how near their jittered points fell (`gathered`, a Gaussian fit of Blackman-Harris about an output pixel wide), so
// over the jitter's phases it gathers its own detail. The new frame's share is the base over the scale per unit of that
// weight: a longer history, about as many samples a pixel as at the frame's size. Where the history was clipped hard
// (uncovered) the samples count five times, and the history gives way to the frame alone (`alone`: Catmull-Rom over
// its samples, held within the 3x3's range; the nearest sample's own drew blocks, a Gaussian over the frame's pixels
// smudged moving foliage), as where the pixel moves fast, through the glass's mud and drops, or with no history (a
// reset, off the previous frame). The clip box is the 3x3 samples' (weighted by their
// nearness it clipped the history to the nearest sample's side of an edge: stair-steps, speckled grass); the motion is
// the nearest of them's (the largest depth, reversed), so an edge moves with what is in front. The rain and the
// particles, drawn after `Taa::keep` (`over`), are added at the frame's resolution and kept out of the history.

// A sample's weight `d` output pixels from the pixel's centre.
fn kernel(d: vec2<f32>) -> f32 {
    return exp(-2.29 * dot(d, d));
}

@fragment
fn fs_upscale(in: Out) -> Resolved {
    let uv = in.position.xy * t.output.zw;
    // The pixel's centre in the frame's pixels, and the frame's pixel whose (jittered) sample is nearest it.
    let p = uv * t.size.xy;
    let base = vec2<i32>(floor(p + t.jitter.xy));
    let last = vec2<i32>(t.size.xy) - 1;
    var gathered = vec4<f32>(0.0);
    var low = vec3<f32>(1.0e9);
    var high = vec3<f32>(0.0);
    // The clip box's moments.
    var m1 = vec3<f32>(0.0);
    var m2 = vec3<f32>(0.0);
    var nearest = base;
    var near = -1.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = base + vec2<i32>(x, y);
            let c = tone(load_scene(q));
            let d = vec2<f32>(q) + 0.5 - t.jitter.xy - p;
            let w = kernel(d * t.jitter.z);
            gathered += vec4<f32>(c * w, w);
            low = min(low, c);
            high = max(high, c);
            let yc = to_ycocg(c);
            m1 += yc;
            m2 += yc * yc;
            let at = clamp(q, vec2<i32>(0), last);
            let z = textureLoad(depth, at, 0);
            if (z > near) {
                near = z;
                nearest = at;
            }
        }
    }
    let sampled = catmull_rom(scene, (p + t.jitter.xy) * t.size.zw, t.size).rgb;
    let alone = clamp(tone(select(sampled, vec3<f32>(0.0), sampled != sampled)), low, high);
    let mask = clamp(textureLoad(scene, clamp(base, vec2<i32>(0), last), 0).a, 0.0, 1.0);
    if (t.params.w > 1.5) {
        let c = vec4<f32>(vec3<f32>(mask), mask);
        return Resolved(c, c);
    }
    // What the rain and the particles changed, added to the frame and not to the history.
    let o = textureSampleLevel(over, linear_sampler, uv, 0.0).rgb - textureSampleLevel(scene, linear_sampler, uv, 0.0).rgb;
    let effects = select(o, vec3<f32>(0.0), o != o);
    let fresh = vec4<f32>(untone(alone), mask);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let previous = previous_uv(nearest, uv, vec4<f32>(ndc, near, 1.0));
    let prev = previous.xy;
    let motion = prev - uv;
    if (t.params.w > 0.5) {
        let c = vec4<f32>(abs(motion) * 50.0, 0.0, mask);
        return Resolved(c, c);
    }
    var c = fresh;
    if (t.params.z < 0.5 && all(prev >= vec2<f32>(0.0)) && all(prev <= vec2<f32>(1.0))) {
        let kept = history_at(prev);
        let held = to_ycocg(tone(kept.rgb));
        let mean = m1 / 9.0;
        let sigma = sqrt(max(m2 / 9.0 - mean * mean, vec3<f32>(0.0)));
        let clipped_y = clip_box(held, mean, sigma * t.params.y + vec3<f32>(1.0e-3));
        let clipped = from_ycocg(clipped_y);
        let moved = length((clipped_y - held) / max(mean.x, 0.02));
        // `mix(clipped, gathered / weight, share)`, without dividing by a weight near 0 (no sample near).
        let rate = t.jitter.w * (1.0 + 4.0 * clamp(moved, 0.0, 1.0));
        let share = min(gathered.w * rate, 1.0);
        let gathers = clipped * (1.0 - share) + gathered.rgb * select(rate, 1.0 / gathered.w, share == 1.0);
        let speed = length(motion * t.output.xy);
        var taken = 0.25 * clamp(moved, 0.0, 1.0) + 0.1 * clamp(speed / 16.0, 0.0, 1.0);
        // The glass's mud and drops, as in `fs_taa`.
        taken = max(taken, smoothstep(GLASS.x, GLASS.y, max(mask, kept.a)) * (1.0 - previous.z));
        c = vec4<f32>(untone(mix(gathers, alone, taken)), mask);
    }
    return Resolved(vec4<f32>(max(c.rgb + effects, vec3<f32>(0.0)), mask), c);
}
