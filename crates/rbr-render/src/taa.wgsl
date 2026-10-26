// Temporal anti-aliasing (`taa.rs`; ours, after gp4's): one full-screen pass at the frame's size, between the scene
// and the post pass. The scene (`scene`) was drawn with its projection shifted by a sub-pixel jitter, so the history
// gathers samples spread over each pixel. For each pixel:
//   * the current colour and its four edge neighbours, in a tone-mapped space (c / (1 + luma)) so that a single bright
//     sample cannot dominate;
//   * motion: the point from the depth (reversed) carried by the unjittered view-projections, current to previous, or
//     inside the car's box by the car's own motion (its previous pose);
//   * history: the previous output there (Catmull-Rom, 5 bilinear taps), clipped toward the neighbourhood's mean in
//     YCoCg (variance clipping, Salvi 2016);
//   * blend: a tenth of the new frame, more where the history was clipped hard or the pixel moves fast; all of it
//     after a reset (a cut, a resize).
// The car's glass (its blended draws, the panes' mud) moves with the car and what shows through it with the stage: its
// opacity in the frame's alpha (`pipelines::alpha_writes`) is a mask, kept in the history's alpha, and where the
// glass is opaque now or was there the frame is taken new.
// The result goes to the post pass's scene and to the next frame's history.

struct Taa {
    // The current frame's clip space (unjittered) to the previous frame's, for a point that stays put.
    view_to_prev: mat4x4<f32>,
    // The current clip space to the car's model space, and the car's model space to the previous clip space.
    car_to_local: mat4x4<f32>,
    car_to_prev: mat4x4<f32>,
    // The car's box in its model space (w of `car_low`: 1 with a car), and its rectangle on the screen (uv, y down:
    // min x, min y, max x, max y).
    car_low: vec4<f32>,
    car_high: vec4<f32>,
    car_rect: vec4<f32>,
    // The frame's size: w, h, 1/w, 1/h.
    size: vec4<f32>,
    // x the new frame's base share, y the clip box's half size (sigmas), z 1 = reset, w 1 = show the motion, 2 = the
    // mask.
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> t: Taa;
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var depth: texture_depth_multisampled_2d;
@group(0) @binding(3) var history: texture_2d<f32>;
@group(0) @binding(4) var linear_sampler: sampler;

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

// Catmull-Rom history sample from 5 bilinear taps (Jimenez 2016), with its glass mask in alpha.
fn history_at(uv: vec2<f32>) -> vec4<f32> {
    let px = uv * t.size.xy;
    let c = floor(px - 0.5) + 0.5;
    let f = px - c;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    let w12 = w1 + w2;
    let p0 = (c - 1.0) * t.size.zw;
    let p3 = (c + 2.0) * t.size.zw;
    let p12 = (c + w2 / w12) * t.size.zw;
    var r = textureSampleLevel(history, linear_sampler, vec2<f32>(p12.x, p0.y), 0.0) * (w12.x * w0.y);
    r += textureSampleLevel(history, linear_sampler, vec2<f32>(p0.x, p12.y), 0.0) * (w0.x * w12.y);
    r += textureSampleLevel(history, linear_sampler, p12, 0.0) * (w12.x * w12.y);
    r += textureSampleLevel(history, linear_sampler, vec2<f32>(p3.x, p12.y), 0.0) * (w3.x * w12.y);
    r += textureSampleLevel(history, linear_sampler, vec2<f32>(p12.x, p3.y), 0.0) * (w12.x * w3.y);
    let sum = w12.x * w0.y + w0.x * w12.y + w12.x * w12.y + w3.x * w12.y + w12.x * w3.y;
    return max(r / max(sum, 1.0e-4), vec4<f32>(0.0));
}

// `h` clipped toward the box's centre `c` +- `e`, along the line to the centre.
fn clip_box(h: vec3<f32>, c: vec3<f32>, e: vec3<f32>) -> vec3<f32> {
    let d = h - c;
    let u = abs(d) / max(e, vec3<f32>(1.0e-5));
    let m = max(u.x, max(u.y, u.z));
    return select(h, c + d / m, m > 1.0);
}

// Where the point seen at `uv` (clip `here`) was in the previous frame, uv.
fn previous_uv(uv: vec2<f32>, here: vec4<f32>) -> vec2<f32> {
    var clip = t.view_to_prev * here;
    if (t.car_low.w > 0.5 && all(uv >= t.car_rect.xy) && all(uv <= t.car_rect.zw)) {
        let local = t.car_to_local * here;
        // The sky (at infinity, w 0) is in no box.
        let p = local.xyz / local.w;
        if (local.w > 0.0 && all(p > t.car_low.xyz) && all(p < t.car_high.xyz)) {
            clip = t.car_to_prev * local;
        }
    }
    let ndc = clip.xy / max(clip.w, 1.0e-6);
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
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
    let prev = previous_uv(uv, here);
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
    // Clean glass (alpha below GLASS.x) gathers as the rest; mud and drops (GLASS.y) are taken new.
    share = max(share, smoothstep(GLASS.x, GLASS.y, max(mask, kept.a)));
    let c = vec4<f32>(untone(mix(from_ycocg(clipped), s0, share)), mask);
    return Resolved(c, c);
}
