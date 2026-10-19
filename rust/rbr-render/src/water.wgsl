// Flowing water and puddles (renderer additions, `renderer::Enhance::water` and `wet`; ours, not the game's look).

// The screen-space reflections with the additions (`renderer::pipelines::reflect_entry`): `reflect_ray` with the
// flowing water's normal and the puddles' normal and reflectance; puddles spread their rays as little as water does.
// A copy, so that the frame without the additions keeps `fs_reflect` as it was (a shared version moved a few pixels of
// water by a level).
@fragment
fn fs_reflect_plus(v: VertexOut) -> @location(0) vec4<f32> {
    let jitter = fract(52.9829189 * fract(dot(v.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    let view = normalize(frame.eye.xyz - v.world);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    var weight = 0.0;
    var still = f32(mode.water != 0u);
    if (mode.water != 0u && frame.enhance.z != 0.0) {
        let flowing = flowing_normal(v);
        n = flowing.xyz;
        weight = water_fresnel(n, view) * flowing.w;
    } else if (mode.water != 0u) {
        n = water_normal(v.world);
        weight = water_fresnel(n, view);
    } else if (frame.enhance.w != 0.0 && dot(v.normal, v.normal) > 1e-8) {
        let film = puddled(v);
        n = film.normal;
        weight = film.reflectance;
        still = film.puddle;
    } else if (dot(v.normal, v.normal) > 1e-8) {
        n = rain_rings(normalize(v.normal), v.world);
        weight = wet_reflectance(n, view, v);
    }
    let ray = reflect(-view, n);
    if (weight <= 0.0 || ray.y <= 0.0) {
        return vec4<f32>(0.0);
    }
    var miss = vec4<f32>(0.0);
    if (frame.reflection.x != 0.0 && mode.water != 0u) {
        let near = 1.0 - smoothstep(SSR_CUBE_REACH.x, SSR_CUBE_REACH.y, distance(v.world, frame.eye.xyz));
        miss = vec4<f32>(textureSampleLevel(surroundings_cube, reflection_sampler, ray, 1.0).rgb, weight * near);
    }
    let roughness = mix(ROUGHNESS.x, ROUGHNESS.y, still);
    let side = normalize(cross(ray, n + vec3<f32>(1e-3, 0.0, 0.0)));
    let up = cross(side, ray);
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < SSR_RAYS; k++) {
        let angle = 6.2831853 * (f32(k) + jitter) / f32(SSR_RAYS);
        let radius = roughness * sqrt((f32(k) + 0.5) / f32(SSR_RAYS));
        let spread = normalize(ray + radius * (cos(angle) * side + STRETCH * sin(angle) * up));
        let found = trace(v, spread, weight, miss, fract(jitter + f32(k) * 0.618));
        sum += vec4<f32>(found.rgb * found.a, found.a);
    }
    sum /= f32(SSR_RAYS);
    return vec4<f32>(sum.rgb / max(sum.a, 1e-4), sum.a);
}

// Flowing water (`Enhance::water`): the stage's water items keep their vertex colour (a grey shade, darker under the
// trees, `WATER_SHADE` the grey that leaves the light as it is) and alpha (0 at the banks), their two texture layers'
// coordinates (`uv0`, `uv1`) and the scroll of each layer (`uv2`, `specular.xy`, texture coordinates a second, as the
// game's water shaders move them; `stage::water_mesh`). Each layer's scroll is turned into metres a second through the
// screen derivatives of its coordinates and of the stage position, and moves one octave of ripples (`FLOW_WAVES`,
// metres) along it; where the second layer does not scroll (all but the double-texture items) its octave follows the
// first. The ripples also change shape slowly (`FLOW_CHURN`, the noise's third axis a second). The body is lighter and clearer
// toward the banks (`SHALLOW`), where the alpha fades the whole surface out.
const WATER_SHADE: f32 = 0.5;
const FLOW_WAVES: vec2<f32> = vec2<f32>(2.2, 0.55);
const FLOW_CHURN: f32 = 0.35;
const FLOW_RIPPLE: f32 = 0.16;
const FLOW_FASTEST: f32 = 3.0;
const SHALLOW: vec3<f32> = vec3<f32>(0.20, 0.19, 0.13);

// Metres a second on the stage for `scroll` texture coordinates a second of the layer with coordinates `uv` at `world`.
fn flow_velocity(uv: vec2<f32>, world: vec3<f32>, scroll: vec2<f32>) -> vec2<f32> {
    let du = mat2x2<f32>(dpdx(uv), dpdy(uv));
    let dw = mat2x2<f32>(dpdx(world.xz), dpdy(world.xz));
    let det = du[0].x * du[1].y - du[1].x * du[0].y;
    if (abs(det) < 1e-12) {
        return vec2<f32>(0.0);
    }
    let inverse = mat2x2<f32>(vec2<f32>(du[1].y, -du[0].y), vec2<f32>(-du[1].x, du[0].x)) * (1.0 / det);
    let velocity = dw * inverse * scroll;
    let speed = length(velocity);
    return select(velocity, velocity * (FLOW_FASTEST / speed), speed > FLOW_FASTEST);
}

fn flow_height(p: vec2<f32>, flows: vec4<f32>, t: f32) -> f32 {
    let a = value_noise(vec3<f32>((p - flows.xy * t) / FLOW_WAVES.x, t * FLOW_CHURN / FLOW_WAVES.x).xzy);
    let b = value_noise(vec3<f32>((p - flows.zw * t) / FLOW_WAVES.y + vec2<f32>(31.0), t * FLOW_CHURN / FLOW_WAVES.y).xzy);
    return 0.65 * a + 0.35 * b;
}

// The flowing surface's normal, and w how much of it there is (the banks' fade).
fn flowing_normal(v: VertexOut) -> vec4<f32> {
    let first = flow_velocity(v.uv0, v.world, v.uv2);
    let second = select(first, flow_velocity(v.uv1, v.world, v.specular.xy), any(v.specular.xy != vec2<f32>(0.0)));
    let flows = vec4<f32>(first, second);
    let t = frame.wind.x;
    let p = v.world.xz;
    let step = 0.04;
    let h = flow_height(p, flows, t);
    let strength = FLOW_RIPPLE / (1.0 + distance(v.world, frame.eye.xyz) / 30.0) / step;
    let slope = strength * vec2<f32>(flow_height(p + vec2<f32>(step, 0.0), flows, t) - h, flow_height(p + vec2<f32>(0.0, step), flows, t) - h);
    let n = rain_rings(normalize(vec3<f32>(-slope.x, 1.0, -slope.y)), v.world);
    return vec4<f32>(n, clamp(v.color.a * 2.5, 0.0, 1.0));
}

fn water_flowing(v: VertexOut) -> vec4<f32> {
    let flowing = flowing_normal(v);
    let n = flowing.xyz;
    let shore = flowing.w;
    let view = normalize(frame.eye.xyz - v.world);
    let nv = clamp(dot(n, view), 0.0, 1.0);
    let f = water_fresnel(n, view);
    let mirrored = reflect(-view, n);
    let shade = v.color.rgb / WATER_SHADE;
    let daylight = (frame.ambient.rgb + frame.diffuse.rgb * max(-frame.sun.y, 0.0)) * shade;
    // Clearer and lighter toward the banks, where the water is shallow.
    let body = mix(SHALLOW, WATER_BODY, shore) * daylight;
    let body_alpha = mix(0.35, 0.9, pow(1.0 - nv, 2.0)) * mix(0.5, 1.0, shore);
    let alpha = body_alpha * (1.0 - f) + f;
    // The banks and trees that darken the vertex light also hide the sky from the water.
    let open = min(shade, vec3<f32>(1.0));
    let colour = body * body_alpha * (1.0 - f) + mirrored_light(mirrored, daylight) * open * f + glint(mirrored) * open;
    return vec4<f32>(colour / alpha, clamp(alpha * smoothstep(0.0, 1.0, shore), 0.0, 1.0));
}

// Puddles (`Enhance::wet`): where water stands on a road with the wet film, in wet weather and, less, in damp weather
// (`DAMP` of the film, and only the deeper low spots hold water). The ground's lowness (`puddles.rs`, in the length of
// the normal) decides where, broken up by the road's 3 m patches (`PUDDLE_NOISE`); the game's specular map still
// bounds it, as it does the film. A puddle is level (rippled by the wind, `PUDDLE_RIPPLE`, and by the rain), darker
// (`PUDDLE_DARK`: the road under it barely shows), and reflects as water does, sharply, from `PUDDLE_REFLECT` head on.
const DAMP: f32 = 0.5;
const PUDDLE_DEPTH: vec2<f32> = vec2<f32>(0.3, 0.5);
const PUDDLE_NOISE: f32 = 0.5;
const PUDDLE_DAMP: f32 = 0.25;
const PUDDLE_DARK: f32 = 0.45;
const PUDDLE_REFLECT: f32 = 0.3;
const PUDDLE_RIPPLE: f32 = 0.03;

struct Puddled {
    normal: vec3<f32>,
    reflectance: f32,
    // 0 on the film, 1 in standing water.
    puddle: f32,
    // How wet (`wet_mask`'s measure): the film's share up to 1 in a puddle.
    wet: f32,
}

fn puddled(v: VertexOut) -> Puddled {
    let damp = frame.wetness.x < 1.0;
    let map = textureSampleBias(specular_texture, texture_sampler, v.specular.xy, SPEC_BLUR).rgb;
    let shines = smoothstep(SPEC_WET.x, SPEC_WET.y, dot(map, vec3<f32>(0.299, 0.587, 0.114)));
    let low = clamp(length(v.normal) - 1.0, 0.0, 1.0);
    let patches = 0.65 * value_noise(v.world / PUDDLES) + 0.35 * value_noise(v.world / 0.7 + vec3<f32>(17.0));
    let depth = low + PUDDLE_NOISE * (patches - 0.5) - select(0.0, PUDDLE_DAMP, damp);
    let puddle = smoothstep(PUDDLE_DEPTH.x, PUDDLE_DEPTH.y, depth) * shines;
    let wet = mix(FILM * select(1.0, DAMP, damp) * shines, 1.0, puddle);
    // The film follows the road; the puddle is level, rippled by the wind near the eye.
    let near = 1.0 - smoothstep(10.0, 40.0, distance(v.world, frame.eye.xyz));
    let t = frame.wind.x;
    let ripple = PUDDLE_RIPPLE * near * vec2<f32>(
        value_noise(vec3<f32>(v.world.xz * 2.5, t * 0.6)) - 0.5,
        value_noise(vec3<f32>(v.world.xz * 2.5 + vec2<f32>(9.0), t * 0.6)) - 0.5);
    let level = normalize(vec3<f32>(ripple.x, 1.0, ripple.y));
    let n = rain_rings(normalize(mix(normalize(v.normal), level, puddle)), v.world);
    let view = normalize(frame.eye.xyz - v.world);
    let base = mix(mix(WET_REFLECT.x, WET_REFLECT.y, wet), PUDDLE_REFLECT, puddle) * select(1.0, DAMP, damp && puddle < 0.5);
    let f = (base + (WET_MOST - base) * pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 5.0)) * wet;
    return Puddled(n, f, puddle, wet);
}

fn wet_plus(colour: vec3<f32>, v: VertexOut) -> vec3<f32> {
    if (dot(v.normal, v.normal) < 1e-8) {
        return colour;
    }
    let film = puddled(v);
    let amount = select(1.0, DAMP, frame.wetness.x < 1.0);
    let soaked = soak(colour, mix(GROUND_SOAK * amount, 1.0, clamp(film.wet * 2.0, 0.0, 1.0))) * mix(1.0, PUDDLE_DARK, film.puddle);
    let view = normalize(frame.eye.xyz - v.world);
    let mirrored = reflect(-view, film.normal);
    let daylight = frame.ambient.rgb + frame.diffuse.rgb * max(-frame.sun.y, 0.0);
    var around = mirrored_light(mirrored, daylight);
    if (frame.reflection.x != 0.0) {
        around = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, mirrored, mix(2.0, 0.5, film.puddle)).rgb);
    }
    var shine = glint(mirrored);
    if (frame.night.x != 0.0) {
        shine = lamp_glint(v.world, mirrored, film.reflectance);
    }
    return mix(soaked, around, film.reflectance) + shine * film.wet;
}
