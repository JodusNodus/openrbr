// The effects' shared light and depth (renderer addition, `effects.rs`, `Enhance::particles`; not the game's look),
// appended to `rain.wgsl` and `particles.wgsl`: the camera, the sun, its shadow cascades and the frame's depth, for
// rain and snow drawn about the eye and for particles that are lit and fade into what they meet.

struct Effects {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    // xyz the eye, w the time in seconds.
    eye: vec4<f32>,
    // xyz the eye's velocity, metres per second; w how hard it rains or snows (0: not).
    motion: vec4<f32>,
    // xyz the direction the sunlight travels; w how much the cascades shade (0: no cascades).
    sun: vec4<f32>,
    // The stage's sun colour; w 1 when it snows.
    sun_colour: vec4<f32>,
    // The stage's ambient colour; w how far from the eye the cabin reaches in the in-car views, metres (0: outside).
    ambient: vec4<f32>,
    // The sky toward the horizon (what a drop mostly refracts), display colour; w the fog's end, metres.
    sky: vec4<f32>,
    // The fog's colour, w the most it covers.
    fog: vec4<f32>,
    // width, height, 1 / width, 1 / height.
    screen: vec4<f32>,
    sun_matrices: array<mat4x4<f32>, 3>,
    // Each cascade's far distance from the eye, metres.
    sun_splits: vec4<f32>,
    // The car's lamps at night (`lamps.wgsl`).
    lamps: array<Lamp, 8>,
}

// The light the car's lamps give a point in the air at `world` (no shadows; 0 by day). The lamps other than the
// headlights' beams (tail, brake, reverse) light it as a soft tinted glow (ours): as if from `AIR_NEAR` metres away at
// the least, at `AIR_REAR` of their light, so that the dust and the snow right behind the car do not burn into blobs.
const AIR_NEAR: f32 = 1.0;
const AIR_REAR: f32 = 0.3;

// Whether the lamps' code is in the pipeline (`LAMPS` as in `shader.wgsl`): it costs registers, so a fragment shader
// that holds it runs slower even where no lamp is lit. Only the frames with a lamp lit take the pipelines with it.
override LAMPS: bool = true;

fn lamps_at(world: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    if (!LAMPS) {
        return sum;
    }
    for (var i = 0; i < 8; i++) {
        let lamp = effects.lamps[i];
        if (lamp.forward.w != 0.0) {
            sum += lamp_toward(lamp, world);
        } else {
            let d2 = dot(lamp.position.xyz - world, lamp.position.xyz - world);
            sum += lamp_toward(lamp, world) * (d2 + 0.25) / (d2 + AIR_NEAR) * AIR_REAR;
        }
    }
    return sum;
}

@group(1) @binding(0) var<uniform> effects: Effects;
@group(1) @binding(1) var effects_depth: texture_depth_2d;
@group(1) @binding(2) var effects_sun_map: texture_depth_2d_array;
@group(1) @binding(3) var effects_sun_sampler: sampler_comparison;

// The distance from the eye to what the frame shows at fragment `clip` (the first sample), metres; very far where
// only sky is drawn.
fn scene_distance(clip: vec4<f32>) -> f32 {
    let pixel = vec2<i32>(clip.xy);
    let depth = textureLoad(effects_depth, pixel, 0);
    if (depth <= 0.0) {
        return 1e6;
    }
    let uv = clip.xy * effects.screen.zw;
    let w = effects.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return distance(w.xyz / w.w, effects.eye.xyz);
}

// How much sunlight reaches stage point `world` past the cascades' casters (1 lit, 0 shaded); 1 beyond the cascades
// or without them.
fn sun_reach(world: vec3<f32>) -> f32 {
    if (effects.sun.w == 0.0) {
        return 1.0;
    }
    let reach = distance(world, effects.eye.xyz);
    var cascade = 0;
    if (reach > effects.sun_splits.x) {
        cascade = 1;
    }
    if (reach > effects.sun_splits.y) {
        cascade = 2;
    }
    if (reach > effects.sun_splits.z) {
        return 1.0;
    }
    let clip = effects.sun_matrices[cascade] * vec4<f32>(world, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return 1.0;
    }
    let texel = 1.0 / f32(textureDimensions(effects_sun_map).x);
    var lit = 0.0;
    for (var k = 0; k < 4; k++) {
        let offset = (vec2<f32>(f32(k & 1), f32(k >> 1)) - 0.5) * 2.0 * texel;
        lit += textureSampleCompareLevel(effects_sun_map, effects_sun_sampler, uv + offset, cascade, clip.z - 0.002);
    }
    return lit / 4.0;
}

// Henyey-Greenstein phase for scattering by `g` at cosine `c` between the light's travel and the view (toward the
// eye), normalised so isotropic scattering is 1.
fn phase(c: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / pow(1.0 + g2 - 2.0 * g * c, 1.5);
}
