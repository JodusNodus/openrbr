// Ground-truth ambient occlusion (renderer addition, `ambient_occlusion.rs`; not the game's look; Jimenez et al.,
// "Practical Real-Time Strategies for Accurate Indirect Occlusion", 2016). `fs_occlusion` measures, at half the
// frame's resolution (each 2x2 block's first pixel), how much of the cosine-weighted hemisphere over the frame's
// surface the stage nearby hides: in `SLICES` planes through the view direction it walks the depth both ways for the
// highest horizon, and integrates the arc above them against the surface's normal. The slices turn over a 4x4
// pattern of pixels and, under the temporal anti-aliasing, a little more each frame, so the history gathers more of
// them. It also stores the pixel's distance from the eye; `fs_apply` blurs the occlusion over 4x4 of those (the
// pattern), keeping to pixels at about the same distance as the frame's pixel, and multiplies the resolved frame by it.

// The matrices take positions about the eye (`AmbientOcclusion::draw`), so `eye` is the origin.
struct Occlusion {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    // x the radius in metres, y the strength, z and w the distances over which it fades out.
    params: vec4<f32>,
    // The view's right and up in stage space; `right.w` the frame's pixels a metre spans one metre along the view,
    // `up.w` this frame's turn of the slices (0 to 1).
    right: vec4<f32>,
    up: vec4<f32>,
    // The view's axis.
    forward: vec4<f32>,
}

@group(0) @binding(0) var<uniform> occlusion: Occlusion;
// The frame's depth (`depth_copy.rs`).
@group(0) @binding(1) var scene_depth: texture_2d<f32>;
@group(1) @binding(0) var occlusion_map: texture_2d<f32>;
// Each pixel's distance from the eye, -1 where only sky is drawn.
@group(1) @binding(1) var reach_map: texture_2d<f32>;

const PI: f32 = 3.14159265;
// Planes through the view direction per pixel, and depth steps each way along each.
const SLICES: i32 = 2;
const STEPS: i32 = 6;
// Within this distance of the eye, metres, the radius shrinks with the distance, to `NEAR_LEAST` of it at the least:
// the cockpit around the eye, all within the full radius of itself, would go dark as a whole; at its own scale it gets
// the shade of its folds and corners.
const NEAR: f32 = 3.0;
const NEAR_LEAST: f32 = 0.05;
// The most frame pixels the radius spans: nearer, the steps would spread too thin to find the folds.
const MOST_PIXELS: f32 = 160.0;
// From this share of the radius out a stage point counts less as a horizon, none at the radius: a trunk far in front
// of the surface does not shade it, so its outline leaves no halo on what lies behind.
const FALLOFF: f32 = 0.6;
// Horizons this little (in cosine) above the surface's plane do not count: the depth's precision on far, grazing
// ground otherwise rose into a faint shade.
const HORIZON_BIAS: f32 = 0.03;

struct Out {
    @builtin(position) clip: vec4<f32>,
}

// One triangle over the target.
@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0));
}

fn size() -> vec2<i32> {
    return vec2<i32>(textureDimensions(scene_depth));
}

// The stage point at `pixel`, w 0 where only sky is drawn.
fn world_at(pixel: vec2<i32>) -> vec4<f32> {
    let p = clamp(pixel, vec2<i32>(0), size() - 1);
    let depth = textureLoad(scene_depth, p, 0).x;
    let uv = (vec2<f32>(p) + 0.5) / vec2<f32>(size());
    let w = occlusion.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return vec4<f32>(w.xyz / w.w, f32(depth > 0.0));
}

struct Measured {
    @location(0) occlusion: vec4<f32>,
    @location(1) reach: f32,
}

// The highest horizon (its cosine from the view direction `v`) seen from `centre` toward `omega` in the frame, out to
// `pixels`, at the least `low`: stage points within the radius, the farther the less.
fn horizon(pixel: vec2<f32>, omega: vec2<f32>, pixels: f32, centre: vec3<f32>, v: vec3<f32>, radius: f32, jitter: f32, low: f32) -> f32 {
    var highest = low;
    for (var j = 0; j < STEPS; j = j + 1) {
        let s = (f32(j) + jitter) / f32(STEPS);
        // Denser near the pixel, and at least a pixel a step.
        let at = pixel + omega * max(s * s * pixels, f32(j + 1));
        if (any(at < vec2<f32>(0.0)) || any(at >= vec2<f32>(size()))) {
            break;
        }
        let hit = world_at(vec2<i32>(at));
        if (hit.w == 0.0) {
            continue;
        }
        let to_hit = hit.xyz - centre;
        let length = max(sqrt(dot(to_hit, to_hit)), 1e-4);
        let weight = clamp((radius - length) / ((1.0 - FALLOFF) * radius), 0.0, 1.0);
        highest = max(highest, mix(low, dot(to_hit / length, v) - HORIZON_BIAS, weight));
    }
    return highest;
}

// The visible arc's cosine-weighted share between horizon angles `h` (from the view direction) about the projected
// normal at angle `n`.
fn arc(h: f32, n: f32) -> f32 {
    return 0.25 * (cos(n) + 2.0 * h * sin(n) - cos(2.0 * h - n));
}

@fragment
fn fs_occlusion(v: Out) -> Measured {
    let half = vec2<i32>(v.clip.xy);
    let pixel = half * 2;
    let centre = world_at(pixel);
    let reach = distance(centre.xyz, occlusion.eye.xyz);
    let stored = select(-1.0, reach, centre.w != 0.0);
    let fade = 1.0 - smoothstep(occlusion.params.z, occlusion.params.w, reach);
    if (centre.w == 0.0 || fade <= 0.0) {
        return Measured(vec4<f32>(1.0), stored);
    }
    // The normal from the neighbours on the side nearer the surface, so edges do not bend it.
    let l = world_at(pixel - vec2<i32>(1, 0)).xyz - centre.xyz;
    let r = world_at(pixel + vec2<i32>(1, 0)).xyz - centre.xyz;
    let u = world_at(pixel - vec2<i32>(0, 1)).xyz - centre.xyz;
    let d = world_at(pixel + vec2<i32>(0, 1)).xyz - centre.xyz;
    let dx = select(-l, r, dot(r, r) < dot(l, l));
    let dy = select(-u, d, dot(d, d) < dot(u, u));
    var n = normalize(cross(dx, dy));
    let to_eye = normalize(occlusion.eye.xyz - centre.xyz);
    n = select(n, -n, dot(n, to_eye) < 0.0);

    let radius = occlusion.params.x * clamp(reach / NEAR, NEAR_LEAST, 1.0);
    let pixels = min(radius * occlusion.right.w / max(dot(centre.xyz, occlusion.forward.xyz), 1e-3), MOST_PIXELS);
    if (pixels < 1.0) {
        return Measured(vec4<f32>(1.0), stored);
    }
    // The 4x4 pattern's turn and step offset (`fs_apply` averages them out), moved on each frame.
    let cell = (half.x & 3) * 4 + (half.y & 3);
    let order = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let turn = fract(f32(cell) / 16.0 + occlusion.up.w);
    let jitter = fract((order[cell] + 0.5) / 16.0 + occlusion.up.w * 2.618034);
    let centre_pixel = vec2<f32>(pixel) + 0.5;
    var seen = 0.0;
    for (var i = 0; i < SLICES; i = i + 1) {
        let phi = (f32(i) + turn) * PI / f32(SLICES);
        let omega = vec2<f32>(cos(phi), sin(phi));
        // The frame's y runs down, the view's up.
        let along = occlusion.right.xyz * omega.x - occlusion.up.xyz * omega.y;
        let ortho = normalize(along - dot(along, to_eye) * to_eye);
        let axis = cross(ortho, to_eye);
        let projected = n - axis * dot(n, axis);
        let projected_length = length(projected);
        if (projected_length < 1e-4) {
            continue;
        }
        let cos_n = clamp(dot(projected, to_eye) / projected_length, -1.0, 1.0);
        let angle = sign(dot(ortho, projected)) * acos(cos_n);
        let c0 = horizon(centre_pixel, omega, pixels, centre.xyz, to_eye, radius, jitter, cos(angle + 0.5 * PI));
        let c1 = horizon(centre_pixel, -omega, pixels, centre.xyz, to_eye, radius, jitter, cos(angle - 0.5 * PI));
        let h0 = angle + max(-acos(c1) - angle, -0.5 * PI);
        let h1 = angle + min(acos(c0) - angle, 0.5 * PI);
        seen = seen + projected_length * (arc(h0, angle) + arc(h1, angle));
    }
    let hidden = clamp(1.0 - seen / f32(SLICES), 0.0, 1.0);
    let ao = 1.0 - occlusion.params.y * fade * hidden;
    return Measured(vec4<f32>(clamp(ao, 0.0, 1.0)), stored);
}

@fragment
fn fs_apply(v: Out) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(v.clip.xy);
    let centre = world_at(pixel);
    if (centre.w == 0.0) {
        return vec4<f32>(1.0);
    }
    let reach = distance(centre.xyz, occlusion.eye.xyz);
    // The 4x4 measured pixels about this one.
    let around = (pixel - 1) / 2;
    let last = vec2<i32>(textureDimensions(reach_map)) - 1;
    var sum = 0.0;
    var weight = 0.0;
    for (var y = -1; y < 3; y = y + 1) {
        for (var x = -1; x < 3; x = x + 1) {
            let p = clamp(around + vec2<i32>(x, y), vec2<i32>(0), last);
            let other = textureLoad(reach_map, p, 0).r;
            let w = select(0.0, exp(-abs(other - reach) / (0.08 * reach + 0.1)), other >= 0.0);
            sum = sum + textureLoad(occlusion_map, p, 0).r * w;
            weight = weight + w;
        }
    }
    let ao = select(1.0, sum / weight, weight > 1e-4);
    return vec4<f32>(ao, ao, ao, 1.0);
}
