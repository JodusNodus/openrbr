// Rain and snow about the eye (renderer addition, `effects.rs`, `Enhance::particles`; not the game's look). Each
// instance is one drop or flake, placed by a hash of its index in a box that wraps about the eye, so drops keep
// their stage positions as the eye moves and the view is full at any time. A drop is drawn as the streak it leaves
// over `SHUTTER` seconds of its motion against the eye's, as a camera sees rain; drops thinner than a pixel are drawn
// a pixel wide and fainter by the same share, so far rain thins out instead of thickening. Depth-tested against the
// frame. Rain takes the sky's colour (a drop mostly refracts the sky), snow the stage's light.

// Set for the HDR scene: linear colour (`linear` in `particles.wgsl`).
override HDR: bool = false;
// Set when drawn over the resolved frame after the temporal anti-aliasing (which would fade streaks that move every
// frame): no depth attachment, so each fragment tests the frame's depth itself.
override RESOLVED: bool = false;

fn linear(colour: vec3<f32>) -> vec3<f32> {
    return select(colour, pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2)), HDR);
}

// The two layers of drops about the eye, metres: a near box and a sparser far one, with how far below the eye each
// starts and the distances its drops fade in and out over.
const NEAR_BOX: vec3<f32> = vec3<f32>(24.0, 16.0, 24.0);
const FAR_BOX: vec3<f32> = vec3<f32>(80.0, 36.0, 80.0);
const NEAR_BELOW: f32 = 6.0;
const FAR_BELOW: f32 = 12.0;
const NEAR_FADE: vec4<f32> = vec4<f32>(0.6, 1.5, 8.0, 11.5);
const FAR_FADE: vec4<f32> = vec4<f32>(8.0, 11.5, 26.0, 38.0);
// Instances in the near layer at full rain or snow (`effects.rs` draws the rest as the far layer).
const NEAR_COUNT: u32 = 16000u;

// Rain: fall speed range (m/s), drop diameter (m), how opaque a drop's streak is where it covers a pixel.
const RAIN_SPEED: vec2<f32> = vec2<f32>(7.5, 9.5);
const RAIN_SIZE: f32 = 0.003;
const RAIN_OPACITY: f32 = 0.75;
// Snow: fall speed range, flake diameter, opacity, and how far flakes flutter about their path (m).
const SNOW_SPEED: vec2<f32> = vec2<f32>(0.8, 1.5);
const SNOW_SIZE: f32 = 0.012;
const SNOW_OPACITY: f32 = 0.85;
const SNOW_FLUTTER: f32 = 0.25;
// The streak's exposure, seconds.
const SHUTTER: f32 = 1.0 / 60.0;
// The narrowest a streak is drawn, pixels.
const LEAST_WIDTH: f32 = 1.2;
// The drift the wind gives falling drops (the foliage's `WIND`), m/s.
const DRIFT: vec2<f32> = vec2<f32>(0.92, 0.39) * 1.2;

fn hash3(n: u32) -> vec3<f32> {
    var x = n * 747796405u + 2891336453u;
    var v = vec3<u32>(x, x * 1664525u + 1013904223u, x * 22695477u + 1u);
    v = (v ^ (v >> vec3<u32>(16u))) * 2246822519u;
    v = (v ^ (v >> vec3<u32>(13u))) * 3266489917u;
    v = v ^ (v >> vec3<u32>(16u));
    return vec3<f32>(v & vec3<u32>(0xffffffu)) / 16777216.0;
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    // x across the streak (-1 to 1), y along it (0 head to 1 tail).
    @location(0) at: vec2<f32>,
    @location(1) @interpolate(flat) colour: vec4<f32>,
}

fn hidden() -> Out {
    var out: Out;
    out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
    return out;
}

// The pixel position of clip point `c`.
fn to_pixels(c: vec4<f32>) -> vec2<f32> {
    return vec2<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5) * effects.screen.xy;
}

@vertex
fn vs_rain(@builtin(vertex_index) corner: u32, @builtin(instance_index) index: u32) -> Out {
    let snow = effects.sun_colour.w != 0.0;
    let t = effects.eye.w;
    let near = index < u32(f32(NEAR_COUNT) * effects.motion.w);
    let size = select(FAR_BOX, NEAR_BOX, near);
    let below = select(FAR_BELOW, NEAR_BELOW, near);
    let fade = select(FAR_FADE, NEAR_FADE, near);
    let seed = hash3(index);
    let more = hash3(index ^ 0x9e3779b9u);
    let speed = select(mix(RAIN_SPEED.x, RAIN_SPEED.y, more.x), mix(SNOW_SPEED.x, SNOW_SPEED.y, more.x), snow);
    var velocity = vec3<f32>(DRIFT.x, -speed, DRIFT.y) * select(1.0, 0.5, snow);
    var position = seed * size + velocity * t;
    if (snow) {
        let wobble = vec2<f32>(sin(1.3 * t + 6.3 * more.y), cos(1.1 * t + 6.3 * more.z));
        position += vec3<f32>(wobble.x, 0.0, wobble.y) * SNOW_FLUTTER;
        velocity += vec3<f32>(1.3 * cos(1.3 * t + 6.3 * more.y), 0.0, -1.1 * sin(1.1 * t + 6.3 * more.z)) * SNOW_FLUTTER;
    }
    // Wrapped into the box about the eye.
    let origin = effects.eye.xyz - vec3<f32>(size.x * 0.5, below, size.z * 0.5);
    let local = position - origin - floor((position - origin) / size) * size;
    let world = origin + local;
    let reach = distance(world, effects.eye.xyz);
    // Fading toward the box's top and bottom, where drops wrap.
    let edge = min(local.y, size.y - local.y);
    let alpha_fade = smoothstep(fade.x, fade.y, reach) * (1.0 - smoothstep(fade.z, fade.w, reach))
        * smoothstep(0.0, 2.0, edge);
    if (alpha_fade <= 0.0) {
        return hidden();
    }
    let diameter = select(RAIN_SIZE, SNOW_SIZE * mix(0.6, 1.4, more.z), snow);
    // The streak over the exposure: from where the drop is to where it was, against the eye.
    let head = effects.view_projection * vec4<f32>(world, 1.0);
    let tail = effects.view_projection * vec4<f32>(world - (velocity - effects.motion.xyz) * SHUTTER, 1.0);
    if (head.w <= 0.05 || tail.w <= 0.05) {
        return hidden();
    }
    let a = to_pixels(head);
    let b = to_pixels(tail);
    var direction = b - a;
    let span = max(sqrt(dot(direction, direction)), 1e-3);
    direction = select(vec2<f32>(0.0, 1.0), direction / span, span > 1e-2);
    let side = vec2<f32>(-direction.y, direction.x);
    // The drop's width in pixels and the share of a drawn pixel it covers.
    let focal = 0.5 * effects.screen.y * effects.view_projection[1][1];
    let width = diameter * focal / head.w;
    let drawn = max(width, LEAST_WIDTH);
    let coverage = width / drawn;
    // A moving drop spends a share of the exposure on each pixel of its streak; a streak shorter than the drop is
    // the drop itself.
    let dwell = min(drawn / max(span, drawn), 1.0);
    let opacity = select(RAIN_OPACITY * sqrt(dwell), SNOW_OPACITY * dwell, snow);
    // Corners: two triangles from the head (y 0) to the tail (y 1), half a width out at each end and side.
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[corner];
    let end = select(head, tail, c.y > 0.5);
    let pixel = mix(a, b, c.y) + side * c.x * drawn * 0.5 + direction * (c.y * 2.0 - 1.0) * drawn * 0.5;
    let ndc = vec2<f32>(pixel.x * effects.screen.z * 2.0 - 1.0, 1.0 - pixel.y * effects.screen.w * 2.0);
    var out: Out;
    out.clip = vec4<f32>(ndc * end.w, end.z, end.w);
    out.at = c;
    // Rain shows the sky it refracts, a little brighter where the sun lights it; snow is lit like the stage.
    var colour = mix(effects.sky.rgb, vec3<f32>(dot(effects.sky.rgb, vec3<f32>(0.333))), 0.5) * 0.9;
    if (snow) {
        colour = min(effects.ambient.rgb + effects.sun_colour.rgb * 0.6, vec3<f32>(1.0));
    }
    // Lit by the car's lamps at night.
    colour += 0.6 * lamp_glow(lamps_at(world));
    let haze = clamp(reach / max(effects.sky.w, 1.0), 0.0, 1.0) * effects.fog.w;
    colour = mix(colour, effects.fog.rgb, haze);
    out.colour = vec4<f32>(colour, opacity * coverage * alpha_fade);
    return out;
}

@fragment
fn fs_rain(v: Out) -> @location(0) vec4<f32> {
    if (RESOLVED && v.clip.z < textureLoad(effects_depth, vec2<i32>(v.clip.xy / effects.ambient.w), 0)) {
        discard;
    }
    let snow = effects.sun_colour.w != 0.0;
    // Soft across, and toward the ends; a flake is a round soft dot.
    let across = 1.0 - v.at.x * v.at.x;
    let along = smoothstep(0.0, 0.15, v.at.y) * (1.0 - smoothstep(0.7, 1.0, v.at.y));
    let shape = select(across * mix(0.4, 1.0, along), across * smoothstep(0.0, 0.5, v.at.y) * (1.0 - smoothstep(0.5, 1.0, v.at.y)), snow);
    let alpha = clamp(v.colour.a * shape, 0.0, 1.0);
    return vec4<f32>(linear(v.colour.rgb) * alpha, alpha);
}
