// The car's lamps at night (renderer addition, `Atmosphere::night` and dusk, `lights::Lamp`), appended to `shader.wgsl` (with
// `night.wgsl`) and to the effects (`effects.wgsl`): a lamp, and the shape of a headlight's beam.

// One lamp (`LampUniform` in `renderer/uniforms.rs`): xyz its position, w 1 when lit; the way it points, w 1 for a headlight's
// beam; the car's up; its colour times its intensity.
struct Lamp {
    position: vec4<f32>,
    forward: vec4<f32>,
    up: vec4<f32>,
    colour: vec4<f32>,
}

// The headlights' beam, a dipped beam (tangents of angles from the lamp's axis, ours after the ECE low-beam pattern): a
// sharp horizontal cut-off `BEAM_AIM` below the axis (0.7 degrees: lamps are aimed about 1% down), softened over
// `BEAM_CUT_SOFT`, with only `BEAM_GLARE` of the light above it; under it the hot spot, `BEAM_SPOT` high, then
// weakening with the square of the angle, so that the road it meets at a grazing angle is lit about evenly from the
// bumper to the cut-off's reach (about 60 m from lamps 0.7 m up). Across, the hot spot's half width and the spill's,
// a faint fringe (near the car, at the lamps' height, it lights a bank as brightly as the hot spot lights the road far
// ahead). Trees are lit on their lower part only, and less with distance, as the cut-off rises up them.
const BEAM_AIM: f32 = 0.012;
const BEAM_CUT_SOFT: f32 = 0.008;
const BEAM_GLARE: f32 = 0.01;
const BEAM_SPOT: vec2<f32> = vec2<f32>(0.02, 0.35);
const BEAM_SPILL: f32 = 1.0;
const BEAM_SPILL_SHARE: f32 = 0.015;
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
    return vertical * (across + BEAM_SPILL_SHARE * spill);
}

// The most the lamps light a drop or a particle in the air, a soft knee (`lamps_at` in `effects.wgsl`).
const AIR_KNEE: f32 = 1.5;

fn lamp_glow(light: vec3<f32>) -> vec3<f32> {
    return AIR_KNEE * light / (AIR_KNEE + light);
}

// The light `lamp` gives a point at `world` facing it, before any shadow (0 when it is not lit).
fn lamp_toward(lamp: Lamp, world: vec3<f32>) -> vec3<f32> {
    if (lamp.position.w == 0.0) {
        return vec3<f32>(0.0);
    }
    let to = lamp.position.xyz - world;
    let d2 = dot(to, to);
    let from_lamp = -to * inverseSqrt(max(d2, 1e-6));
    let share = select(max(dot(from_lamp, lamp.forward.xyz), 0.0), beam(lamp, from_lamp), lamp.forward.w != 0.0);
    return lamp.colour.rgb * share / (d2 + 0.25);
}
