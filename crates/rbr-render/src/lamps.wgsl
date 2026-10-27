// The car's lamps at night (renderer addition, `Atmosphere::night` and dusk, `lights::Lamp`), appended to `shader.wgsl` (with
// `night.wgsl`), to the effects (`effects.wgsl`), the light shafts and the skid marks: a lamp, the shape of a
// headlight's beam, and the light it gives a surface.

// One lamp (`LampUniform` in `renderer/uniforms.rs`): xyz its position, w 1 when lit; the way it points, w 1 for a headlight's
// dipped beam, 2 its main beam; the car's up; its colour times its intensity.
struct Lamp {
    position: vec4<f32>,
    forward: vec4<f32>,
    up: vec4<f32>,
    colour: vec4<f32>,
}

// The headlights' beam, main beam on top of the dipped beam as on a rally car at stage speed (tangents of angles from
// the lamp's axis, ours after the ECE patterns). The dipped beam: a sharp horizontal cut-off `BEAM_AIM` below the axis
// (0.7 degrees: lamps are aimed about 1% down), softened over `BEAM_CUT_SOFT`, with only `BEAM_GLARE` of the light
// above it; under it the hot spot, `BEAM_SPOT` high, then weakening with the square of the angle, so that the road it
// meets at a grazing angle is lit about evenly from the bumper to the cut-off's reach (about 60 m from lamps 0.7 m up).
// Across, the hot spot's half width and the spill's, a faint fringe (near the car, at the lamps' height, it lights a
// bank as brightly as the hot spot lights the road far ahead). The main beam: a lobe on the axis, `BEAM_MAIN_PEAK`
// times the hot spot, falling to 1/e at `BEAM_MAIN` across and up (4 and 2 degrees, half its peak at 3 and 1.5), which
// lights the road to about 130 m and the trees in front of the car to their crowns.
const BEAM_AIM: f32 = 0.012;
const BEAM_CUT_SOFT: f32 = 0.008;
const BEAM_GLARE: f32 = 0.01;
const BEAM_SPOT: vec2<f32> = vec2<f32>(0.02, 0.35);
const BEAM_SPILL: f32 = 1.0;
const BEAM_SPILL_SHARE: f32 = 0.015;
const BEAM_MAIN: vec2<f32> = vec2<f32>(0.07, 0.035);
const BEAM_MAIN_PEAK: f32 = 5.0;
// The share of a headlight's axis intensity along `from_lamp` (unit, from the lamp).
fn beam(lamp: Lamp, from_lamp: vec3<f32>) -> f32 {
    let f = dot(from_lamp, lamp.forward.xyz);
    if (f <= 0.05) {
        return 0.0;
    }
    let u = dot(from_lamp, lamp.up.xyz) / f;
    let side = length(from_lamp - lamp.forward.xyz * dot(from_lamp, lamp.forward.xyz) - lamp.up.xyz * dot(from_lamp, lamp.up.xyz)) / f;
    let across = exp(-side * side / (BEAM_SPOT.y * BEAM_SPOT.y));
    let spill = exp(-side * side / (BEAM_SPILL * BEAM_SPILL));
    // The elevation over the cut-off.
    let over = u + BEAM_AIM;
    var vertical = 0.0;
    if (over > 0.0) {
        let cut = over / BEAM_CUT_SOFT;
        vertical = mix(BEAM_GLARE * (1.0 - smoothstep(0.1, 0.4, u)), 1.0, exp(-cut * cut));
    } else {
        let below = min(BEAM_SPOT.x / -over, 1.0);
        vertical = below * below;
    }
    let lobe = side * side / (BEAM_MAIN.x * BEAM_MAIN.x) + u * u / (BEAM_MAIN.y * BEAM_MAIN.y);
    let main = select(0.0, BEAM_MAIN_PEAK * exp(-lobe), lamp.forward.w > 1.5);
    return vertical * (across + BEAM_SPILL_SHARE * spill) + main;
}

// The most the lamps light a drop or a particle in the air, a soft knee (`lamps_at` in `effects.wgsl`).
const AIR_KNEE: f32 = 1.5;

fn lamp_glow(light: vec3<f32>) -> vec3<f32> {
    return AIR_KNEE * light / (AIR_KNEE + light);
}

// A rear lamp's share of its axis intensity along `from_lamp` (unit, from the lamp; ours after the ECE R7 and R23
// patterns): a Gaussian of the tangents of the angles across and up or down (`SIGNAL_SPREAD`: half at about 40 and 10
// degrees), so that it shines back along the road and lights little of the ground under it.
const SIGNAL_SPREAD: vec2<f32> = vec2<f32>(1.0, 0.21);

fn signal(lamp: Lamp, from_lamp: vec3<f32>) -> f32 {
    let f = dot(from_lamp, lamp.forward.xyz);
    if (f <= 0.05) {
        return 0.0;
    }
    let u = dot(from_lamp, lamp.up.xyz) / f;
    let side = length(from_lamp - lamp.forward.xyz * f - lamp.up.xyz * dot(from_lamp, lamp.up.xyz)) / f;
    return exp(-side * side / (SIGNAL_SPREAD.x * SIGNAL_SPREAD.x) - u * u / (SIGNAL_SPREAD.y * SIGNAL_SPREAD.y));
}

// The light `lamp` gives a point at `world` facing it, before any shadow (0 when it is not lit).
fn lamp_toward(lamp: Lamp, world: vec3<f32>) -> vec3<f32> {
    if (lamp.position.w == 0.0) {
        return vec3<f32>(0.0);
    }
    let to = lamp.position.xyz - world;
    let d2 = dot(to, to);
    let from_lamp = -to * inverseSqrt(max(d2, 1e-6));
    let share = select(signal(lamp, from_lamp), beam(lamp, from_lamp), lamp.forward.w != 0.0);
    return lamp.colour.rgb * share / (d2 + 0.25);
}

// A headlight's light back off the road it lights (ours): the road as two Lambertian patches facing up on the ground
// `BOUNCE_HEIGHT` below the lamp, near and far (`BOUNCE_AT` metres ahead), each softened by its size (`BOUNCE_SIZE`:
// just over a patch about its albedo times the road's light, as under a lit plane) and as strong as its part of the
// beam's flux on the road times the road's albedo 0.2, over pi (`BOUNCE_SHARE` of the lamp's intensity, dipped then
// main beam; numerically over `beam` for lamps 0.7 m up: 272 and 260 lm dipped, 370 and 516 on main beam, the road lit
// at 1 to 5 lux from 5 to 20 m).
const BOUNCE_HEIGHT: f32 = 0.7;
const BOUNCE_AT: vec2<f32> = vec2<f32>(15.0, 55.0);
const BOUNCE_SIZE: vec2<f32> = vec2<f32>(9.0, 18.0);
const BOUNCE_SHARE: vec4<f32> = vec4<f32>(0.00087, 0.00083, 0.00118, 0.00164);

// The light `lamp`'s bounce gives a face at `world` with normal `n` (`two_sided` for foliage cards); none but a
// headlight's.
fn lamp_bounce(lamp: Lamp, world: vec3<f32>, n: vec3<f32>, two_sided: bool) -> vec3<f32> {
    if (lamp.position.w == 0.0 || lamp.forward.w == 0.0) {
        return vec3<f32>(0.0);
    }
    let up = lamp.up.xyz;
    let ahead = normalize(lamp.forward.xyz - up * dot(lamp.forward.xyz, up) + vec3<f32>(1e-6, 0.0, 0.0));
    let share = select(BOUNCE_SHARE.xy, BOUNCE_SHARE.zw, lamp.forward.w > 1.5);
    var light = 0.0;
    for (var k = 0; k < 2; k++) {
        let to = world - (lamp.position.xyz + ahead * BOUNCE_AT[k] - up * BOUNCE_HEIGHT);
        let d2 = dot(to, to);
        let away = to * inverseSqrt(max(d2, 1e-6));
        let size = BOUNCE_SIZE[k] * BOUNCE_SIZE[k];
        light += share[k] * max(dot(away, up), 0.0) / (d2 + size) * lamp_facing(n, -away, two_sided);
    }
    return lamp.colour.rgb * light;
}

// A lamp's light falling on a face turned from it: none for terrain (`LAMP_WRAP` of it for the grazing road), half for
// foliage cards lit through.
const LAMP_WRAP: f32 = 0.4;
// The most light the lamps give a surface, a soft knee (the beam's foot is very bright on a tree in front of the car).
const LAMP_KNEE: f32 = 4.0;

// The share of a lamp's light a face with normal `n` takes from `toward` (unit, toward the lamp); `two_sided` for
// foliage cards.
fn lamp_facing(n: vec3<f32>, toward: vec3<f32>, two_sided: bool) -> f32 {
    let ndl = dot(n, toward);
    return select(max(ndl, 0.0) * (1.0 - LAMP_WRAP) + LAMP_WRAP * smoothstep(-0.2, 0.2, ndl), mix(abs(ndl), 1.0, 0.5), two_sided);
}

// The headlights' shadow map (`sun::LAMP`, drawn by `renderer/shadows.rs` into its layer of the sun's map): of the
// depth `map` with comparison sampler `smp`, `matrix` stage space to its clip space, `texel` one texel in its uv
// and `LAMP_BIAS` the depth bias. A zero matrix holds no map (no headlight lit, or the shadows off) and samples lit.
// `lamp_shadow` filters 3x3 taps about `world` shifted by `offset` (the normal offset, against acne on the grazed
// road), for surfaces; `lamp_shaft` takes the one tap at `world`, for the air's march (`fs_beams`).
const LAMP_LAYER: i32 = 4;
const LAMP_BIAS: f32 = 0.00002;

fn lamp_project(matrix: mat4x4<f32>, world: vec3<f32>) -> vec4<f32> {
    let p = matrix * vec4<f32>(world, 1.0);
    if (p.w <= 0.0) {
        return vec4<f32>(0.0);
    }
    let c = p.xyz / p.w;
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || c.z > 1.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(uv, c.z, 1.0);
}

fn lamp_shadow(
    map: texture_depth_2d_array,
    smp: sampler_comparison,
    matrix: mat4x4<f32>,
    texel: f32,
    world: vec3<f32>,
    offset: vec3<f32>
) -> f32 {
    let at = lamp_project(matrix, world + offset);
    if (at.z == 0.0 && at.x == 0.0 && at.y == 0.0) {
        return 1.0;
    }
    var lit = 0.0;
    for (var y = -1.0; y < 1.5; y += 1.0) {
        for (var x = -1.0; x < 1.5; x += 1.0) {
            lit += textureSampleCompareLevel(map, smp, at.xy + vec2<f32>(x, y) * texel, LAMP_LAYER, at.z - LAMP_BIAS);
        }
    }
    return lit / 9.0;
}

fn lamp_shaft(map: texture_depth_2d_array, smp: sampler_comparison, matrix: mat4x4<f32>, world: vec3<f32>) -> f32 {
    let at = lamp_project(matrix, world);
    if (at.z == 0.0 && at.x == 0.0 && at.y == 0.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(map, smp, at.xy, LAMP_LAYER, at.z - LAMP_BIAS);
}

// The lamps' summed light `sum` on a surface, under `LAMP_KNEE`.
fn lamp_knee(sum: vec3<f32>) -> vec3<f32> {
    return LAMP_KNEE * sum / (LAMP_KNEE + sum);
}
