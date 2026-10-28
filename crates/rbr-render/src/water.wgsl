// Flowing water and puddles (renderer additions, `renderer::Enhance::water` and `wet`; ours, not the game's look).

// The screen-space reflections with the additions (`renderer::pipelines::reflect_entry`): `reflect_ray` with the
// flowing water's normal and the puddles' normal and reflectance; puddles spread their rays as little as water does.
// A copy, so that the frame without the additions keeps `fs_reflect` as it was (a shared version moved a few pixels of
// water by a level).
@fragment
fn fs_reflect_plus(v: VertexOut) -> Traced {
    if (hidden(v)) {
        discard;
    }
    return Traced(reflect_plus(v), v.clip.z);
}

fn reflect_plus(v: VertexOut) -> vec4<f32> {
    let jitter = fract(52.9829189 * fract(dot(v.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    let view = normalize(frame.eye.xyz - v.world);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    var weight = 0.0;
    var still = f32(mode.water != 0u);
    if (mode.water != 0u && frame.enhance.y != 0.0) {
        let flowing = flowing_normal(v);
        n = flowing.xyz;
        weight = water_fresnel(n, view) * flowing.w;
    } else if (mode.water != 0u) {
        n = water_normal(v.world);
        weight = water_fresnel(n, view);
    } else if (frame.enhance.z != 0.0 && dot(v.normal, v.normal) > 1e-8) {
        let film = puddled(v);
        n = film.normal;
        weight = film.reflectance;
        still = film.puddle;
    }
    let ray = reflect(-view, n);
    if (weight <= 0.0 || ray.y <= 0.0) {
        return vec4<f32>(0.0);
    }
    var miss = vec4<f32>(0.0);
    if (frame.reflection.x != 0.0 && mode.water != 0u) {
        let near = 1.0 - smoothstep(SSR_CUBE_REACH.x, SSR_CUBE_REACH.y, distance(v.world, frame.eye.xyz));
        miss = vec4<f32>(surrounding(ray, 1.0, v.world), weight * near);
    }
    // Puddles on a road: a sharper, less stretched cone than water's (`PUDDLE_CONE`: gloss, roughness, stretch).
    let road_puddle = mode.water == 0u && still >= 0.5;
    // The film on tarmac takes its own cone (`TARMAC_CONE`): sharper and less stretched, so the car reads in it.
    let film_tarmac = mode.water == 0u && still < 0.5 && v.wet.x > TARMAC_SHARE.x;
    let film_cone = select(vec3<f32>(GLOSS.x, ROUGHNESS.x, STRETCH.x), TARMAC_CONE, film_tarmac);
    let roughness = select(select(ROUGHNESS.y, film_cone.y, still < 0.5), PUDDLE_CONE.y, road_puddle);
    let gloss = select(select(GLOSS.y, film_cone.x, still < 0.5), PUDDLE_CONE.x, road_puddle);
    let stretch = select(select(STRETCH.y, film_cone.z, still < 0.5), PUDDLE_CONE.z, road_puddle);
    let side = normalize(cross(ray, n + vec3<f32>(1e-3, 0.0, 0.0)));
    let up = cross(side, ray);
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < SSR_RAYS; k++) {
        let angle = 6.2831853 * (f32(k) + jitter) / f32(SSR_RAYS);
        let radius = roughness * sqrt((f32(k) + 0.5) / f32(SSR_RAYS));
        let spread = normalize(ray + radius * (cos(angle) * side + stretch * sin(angle) * up));
        let found = trace(v, spread, weight, miss, fract(jitter + f32(k) * 0.618), still, gloss);
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
    let n = rain_rings(normalize(vec3<f32>(-slope.x, 1.0, -slope.y)), v.world, 1.0);
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

// The wet road and its puddles (`Enhance::wet`): the film (`FILM`, its patches) and standing water in it, in wet
// weather; a damp road is drying: no standing water, a thinner film (`DAMP_FILM`) gone where the patches are lowest,
// the ground between them soaked as beside the road. Where: the road's share (`VertexOut::wet`), and puddles
// by `puddle_shape` on ground no steeper than `PUDDLE_GRADE`. A puddle follows the road (tilting it level
// swept the reflections along its rim), rippled by the rain's rings, is darker (`PUDDLE_DARK`: the road under it barely
// shows), and reflects sharply, from `PUDDLE_REFLECT` head on to `PUDDLE_MOST` at a grazing view. A road that takes
// half the film (gravel) still takes whole puddles (`PUDDLE_ROAD`); hollows hold water without a shape (`PUDDLE_LOW`).
const PUDDLE_CELL: f32 = 4.0;
const PUDDLE_SHARE: f32 = 0.7;
const PUDDLE_SIZE: vec2<f32> = vec2<f32>(0.18, 0.3);
const PUDDLE_GRADE: vec2<f32> = vec2<f32>(0.08, 0.15);
const PUDDLE_DARK: f32 = 0.15;
const PUDDLE_REFLECT: f32 = 0.55;
const PUDDLE_CONE: vec3<f32> = vec3<f32>(0.012, 0.006, 2.0);
const PUDDLE_MOST: f32 = 0.9;
const PUDDLE_ROAD: f32 = 2.0;
const PUDDLE_RIM: f32 = 0.7;
const PUDDLE_LOW: vec2<f32> = vec2<f32>(0.45, 0.8);
// Tarmac (the road's film share 1, `TARMAC_SHARE`) is one even film with no random puddles: only its hollows hold
// water (`PUDDLE_LOW`, at `TARMAC_HOLLOW` of a puddle's strength), and the film's patches no longer thin it.
const TARMAC_SHARE: vec2<f32> = vec2<f32>(0.6, 0.9);
const TARMAC_HOLLOW: f32 = 0.4;
const TARMAC_SKY: f32 = 0.7;
const TARMAC_NEAR: f32 = 0.6;
const TARMAC_CONE: vec3<f32> = vec3<f32>(0.012, 0.008, 10.0);
const TARMAC_MOST: f32 = 0.55;
const TARMAC_DARK: f32 = 0.45;
const DAMP_FILM: f32 = 0.4;
const FILM_MOST: f32 = 0.95;
const FILM_BEND: f32 = 3.0;
const FILM_SHEET: f32 = 0.5;
const FILM_LOOSE: f32 = 2.0;
// Wet asphalt's film is not a mirror (the film only, damp and puddles keep theirs): the water darkens it by a further
// `FILM_DARK`; its grain (3 and 9 cm of noise, `FILM_GRAIN` of the reflection) shows through; its microrelief tilts the
// normal by up to `FILM_TILT.x` along the view (the reflected lights and sky streak down the road) and `.y` across it;
// and rain's impacts (`FILM_POCK_CELL` metres, `FILM_POCK_RATE` a second, as many as the rain) dimple it by
// `FILM_POCK_TILT` and brighten the reflection by `FILM_POCK_FLASH`; all fading to the mean from `FILM_FINE_FADE.x` to `.y` metres, where a pixel
// spans a grain.
const FILM_DARK: f32 = 0.7;
const FILM_SKY: f32 = 0.6;
const FILM_GRAIN: f32 = 0.6;
const FILM_TILT: vec2<f32> = vec2<f32>(0.05, 0.01);
const FILM_POCK_CELL: f32 = 0.12;
const FILM_POCK_RATE: f32 = 2.5;
const FILM_POCK_REACH: f32 = 0.035;
const FILM_POCK_TILT: f32 = 0.15;
const FILM_POCK_FLASH: f32 = 0.2;
// Wet tarmac's film shows rain as fine splash specks, not rings (`TARMAC_POCK_*` in place of `FILM_POCK_*`: shorter
// lived, `TARMAC_POCK_RATE` a second, smaller and brighter) and keeps rings only in its hollows' water, at
// `TARMAC_RINGS` of a puddle's.
const TARMAC_POCK_RATE: f32 = 8.0;
const TARMAC_POCK_REACH: f32 = 0.03;
const TARMAC_POCK_FLASH: f32 = 0.8;
const TARMAC_RINGS: f32 = 0.2;
// Wet tarmac's normal is roughened at the asphalt's grain, not waved: noise of `TARMAC_TILT_CELL` metres (along the view
// and across it) tilting by `TARMAC_TILT`, its grain `TARMAC_GRAIN` of the reflection, fading from `TARMAC_FINE_FADE.x`
// to `.y` metres. Impacts are dots of `TARMAC_DOT` metres.
const TARMAC_TILT_CELL: vec2<f32> = vec2<f32>(0.012, 0.004);
const TARMAC_TILT: vec2<f32> = vec2<f32>(0.05, 0.025);
const TARMAC_GRAIN: f32 = 1.0;
const TARMAC_FINE_FADE: vec2<f32> = vec2<f32>(2.5, 12.0);
// The albedo's contrast on wet tarmac: its luminance to the power `TARMAC_CONTRAST` about `TARMAC_PIVOT`.
const TARMAC_CONTRAST: f32 = 0.5;
const TARMAC_PIVOT: f32 = 0.056;
const TARMAC_DOT: f32 = 0.012;
const FILM_FINE_FADE: vec2<f32> = vec2<f32>(4.0, 25.0);
// In wet weather the film's reflection and glint fade out over the road's last `EDGE_FADE` metres (`Vertex::edge`), from
// `EDGE_BARE` metres in from its edge, the boundary frayed by up to `EDGE_FRAY` metres of noise: a dark, rough shoulder
// blending into the soaked verge, not a straight mirror line. Puddles end nearer it (`PUDDLE_EDGE`); the road's darkness stays.
const EDGE_BARE: f32 = 0.1;
const EDGE_FADE: f32 = 0.7;
const EDGE_FRAY: f32 = 0.6;
// Puddles end `PUDDLE_EDGE.x` metres from the road's edge, over a further `.y`, frayed as the film is: a puddle the
// mesh's border cut showed as a straight pale line along the verge.
const PUDDLE_EDGE: vec2<f32> = vec2<f32>(0.05, 0.15);

// The film's fine relief at `world` (`FILM_GRAIN`): x the grain's brightness around 1, yz the normal's tilt (stage x, z)
// from the microrelief seen from `toward` (unit, to the eye) and the rain's impacts, w the impacts' flash; 0 tilt, 1
// brightness beyond `FILM_FINE_FADE`.
fn film_relief(world: vec3<f32>, toward: vec3<f32>, tarmac: f32) -> vec4<f32> {
    let fade = mix(FILM_FINE_FADE, TARMAC_FINE_FADE, tarmac);
    let near = 1.0 - smoothstep(fade.x, fade.y, length(frame.eye.xyz - world));
    if (near <= 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    let grain_cell = mix(vec2<f32>(0.03, 0.09), vec2<f32>(0.008, 0.03), tarmac);
    let grain = 0.6 * value_noise(world / grain_cell.x + vec3<f32>(41.0)) + 0.4 * value_noise(world / grain_cell.y + vec3<f32>(13.0));
    let along = normalize(toward.xz + vec2<f32>(1e-4));
    let across = vec2<f32>(-along.y, along.x);
    let cell = mix(vec2<f32>(0.05, 0.02), TARMAC_TILT_CELL, tarmac);
    let amount = mix(FILM_TILT, TARMAC_TILT, tarmac);
    let slope = vec2<f32>(value_noise(world / cell.x + vec3<f32>(3.0)) - 0.5, value_noise(world / cell.y + vec3<f32>(77.0)) - 0.5);
    var tilt = (along * slope.x * amount.x + across * slope.y * amount.y) * 2.0;
    var flash = 0.0;
    let rain = frame.wind.z;
    if (rain > 0.0) {
        let p = world.xz / FILM_POCK_CELL;
        let cell = floor(p);
        for (var k = 0; k < 9; k++) {
            let c = cell + vec2<f32>(f32(k % 3 - 1), f32(k / 3 - 1));
            let h = landing_hash(c + vec2<f32>(11.0));
            if (h.z > rain) {
                continue;
            }
            let age = fract(frame.wind.x * mix(FILM_POCK_RATE, TARMAC_POCK_RATE, tarmac) * mix(0.7, 1.3, h.y) + h.x);
            let d = (p - c - h.xy) * FILM_POCK_CELL;
            let r = length(d);
            let reach = mix(FILM_POCK_REACH, TARMAC_POCK_REACH, tarmac);
            let x = (r - age * reach) / (0.35 * reach + 0.004);
            if (tarmac > 0.5) {
                // A dot that shrinks and fades, no ring.
                flash += (1.0 - smoothstep(0.0, TARMAC_DOT * (1.0 - age), r)) * (1.0 - age) * (TARMAC_POCK_FLASH / FILM_POCK_FLASH);
            } else if (abs(x) < 1.0) {
                tilt += d / max(r, 1e-4) * sin(x * 3.14159265) * (1.0 - age) * FILM_POCK_TILT;
                flash += (1.0 - abs(x)) * (1.0 - age) * mix(1.0, TARMAC_POCK_FLASH / FILM_POCK_FLASH, tarmac);
            }
        }
    }
    return vec4<f32>(mix(1.0, 0.4 + 1.2 * grain, near), tilt * near, flash * near);
}

struct Puddled {
    normal: vec3<f32>,
    reflectance: f32,
    // 0 on the film, 1 in standing water.
    puddle: f32,
    // How wet: the film's up to 1 in a puddle.
    wet: f32,
}

// Three values in [0, 1) for `n` (the rain's `hash3`, which `rain.wgsl` keeps to itself).
fn puddle_hash(n: u32) -> vec3<f32> {
    let x = n * 747796405u + 2891336453u;
    var v = vec3<u32>(x, x * 1664525u + 1013904223u, x * 22695477u + 1u);
    v = (v ^ (v >> vec3<u32>(16u))) * 2246822519u;
    v = (v ^ (v >> vec3<u32>(13u))) * 3266489917u;
    v = v ^ (v >> vec3<u32>(16u));
    return vec3<f32>(v & vec3<u32>(0xffffffu)) / 16777216.0;
}

// How much of a puddle covers `p` (stage metres): one in `PUDDLE_SHARE` of the `PUDDLE_CELL` squares, centred anywhere
// in it, an ellipse up to 1.6 times as long as wide with a radius of `PUDDLE_SIZE` of the square (up to 30% more where
// the ground is low, `puddles.rs`), turned at random and its rim broken by the film's patches and a finer noise (`rim`). A puddle reaches at most
// half a square from its centre, so only the four squares about `p` can hold one over it.
fn puddle_shape(p: vec2<f32>, low: f32, patches: f32, rim: f32) -> f32 {
    let q = p / PUDDLE_CELL;
    let first = floor(q - 0.5);
    var cover = 0.0;
    for (var k = 0; k < 4; k++) {
        let cell = first + vec2<f32>(f32(k % 2), f32(k / 2));
        let n = bitcast<u32>(i32(cell.x)) * 73856093u ^ bitcast<u32>(i32(cell.y)) * 19349663u;
        let h = puddle_hash(n);
        if (h.x > PUDDLE_SHARE) {
            continue;
        }
        let g = puddle_hash(n ^ 0x9e3779b9u);
        let d = q - cell - g.xy;
        let c = 2.0 * g.z - 1.0;
        let turn = vec2<f32>(c, sqrt(1.0 - c * c));
        let across = vec2<f32>(dot(turn, d), (turn.x * d.y - turn.y * d.x) * mix(1.0, 1.6, h.z));
        let radius = mix(PUDDLE_SIZE.x, PUDDLE_SIZE.y, h.y) * (1.0 + 0.3 * low);
        cover = max(cover, 1.0 - smoothstep(0.7, 1.0, length(across) / radius + patches - 0.5 + PUDDLE_RIM * (rim - 0.5)));
    }
    return cover;
}

// How much of the film's reflection the road keeps at `v` (`puddled`, so the screen-space rays fade too): all inland and on
// a drying road, less toward its edge (`EDGE_FADE`).
fn road_edge(v: VertexOut) -> f32 {
    return edge_keep(v, EDGE_BARE, EDGE_FADE);
}

// 0 within `bare` metres of the road's edge (`Vertex::edge`) to 1 a further `fade` in, the boundary frayed by
// `EDGE_FRAY`; 1 on a drying road.
fn edge_keep(v: VertexOut, bare: f32, fade: f32) -> f32 {
    if (frame.wetness.x < 1.0) {
        return 1.0;
    }
    let fray = 0.6 * value_noise(v.world / 0.5 + vec3<f32>(5.0)) + 0.4 * value_noise(v.world / 0.15 + vec3<f32>(29.0));
    return smoothstep(bare, bare + fade, v.wet.y + EDGE_FRAY * (fray - 0.5));
}

fn puddled(v: VertexOut) -> Puddled {
    let patches = 0.65 * value_noise(v.world / PUDDLES) + 0.35 * value_noise(v.world / 0.7 + vec3<f32>(17.0));
    // Fine noise (0.4 and 0.13 m) that frays the outlines.
    let rim = 0.6 * value_noise(v.world / 0.4 + vec3<f32>(5.0)) + 0.4 * value_noise(v.world / 0.13 + vec3<f32>(9.0));
    let shines = v.wet.x;
    let drying = frame.wetness.x < 1.0;
    let tarmac = smoothstep(TARMAC_SHARE.x, TARMAC_SHARE.y, shines);
    let wet_film = mix(mix(FILM, 1.0, smoothstep(0.45, 0.7, patches)), 1.0, tarmac);
    let film = select(wet_film, DAMP_FILM * smoothstep(0.25, 0.6, patches), drying) * shines;
    let low = clamp(length(v.normal) - 1.0, 0.0, 1.0);
    let road = normalize(v.normal);
    let level = 1.0 - smoothstep(PUDDLE_GRADE.x, PUDDLE_GRADE.y, length(road.xz) / max(road.y, 1e-3));
    let pooled = clamp(shines * PUDDLE_ROAD, 0.0, 1.0);
    var puddle = select(pooled * level, 0.0, drying);
    if (puddle > 0.0) {
        let shape = puddle_shape(v.world.xz, low, patches, rim) * (1.0 - tarmac);
        puddle *= mix(1.0, TARMAC_HOLLOW, tarmac) * max(shape, smoothstep(PUDDLE_LOW.x, PUDDLE_LOW.y, low + 0.3 * (patches - 0.5) + 2.0 * PUDDLE_RIM * (rim - 0.5)));
        puddle *= edge_keep(v, PUDDLE_EDGE.x, PUDDLE_EDGE.y);
    }
    let wet = mix(film, 1.0, puddle);
    // The puddle follows the road as the film does (tilting it level swept the reflections along its rim), rippled by
    // the rain's rings.
    var n = rain_rings(road, v.world, mix(max(puddle, standing(film)), puddle * TARMAC_RINGS, tarmac));
    let view = normalize(frame.eye.xyz - v.world);
    var relief = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    if (!drying) {
        relief = film_relief(v.world, view, tarmac);
        n = normalize(n + vec3<f32>(relief.y, 0.0, relief.z) * (1.0 - puddle));
    }
    let base = mix(mix(WET_REFLECT.x, WET_REFLECT.y, wet), PUDDLE_REFLECT, puddle);
    let grazing = 1.0 - clamp(dot(n, view), 0.0, 1.0);
    var f: f32;
    if (drying) {
        f = (base + (mix(WET_MOST, PUDDLE_MOST, puddle) - base) * pow(grazing, 5.0)) * wet;
    } else {
        // Standing water in a wet film reflects as water does toward a grazing view (`FILM_MOST`, `FILM_BEND`) and
        // covers the road more evenly (`FILM_SHEET`); puddles as before.
        let most = mix(mix(FILM_MOST, TARMAC_MOST, tarmac), PUDDLE_MOST, puddle);
        f = (base + (most - base) * pow(grazing, mix(FILM_BEND, 5.0, puddle))) * mix(wet, 1.0, FILM_SHEET * shines);
        // A loose road's film is a sheen over the stones: its reflection falls with the film's share (`FILM_LOOSE`).
        f *= mix(pow(shines, FILM_LOOSE) * mix(FILM_SKY, TARMAC_SKY, tarmac), 1.0, puddle);
        f *= 1.0 + TARMAC_NEAR * tarmac * (1.0 - smoothstep(2.0, 14.0, length(frame.eye.xyz - v.world)));
        f *= mix(mix(1.0, relief.x, mix(FILM_GRAIN, TARMAC_GRAIN, tarmac)) + FILM_POCK_FLASH * relief.w, 1.0, puddle);
    }
    return Puddled(n, f * max(road_edge(v), puddle), puddle, wet);
}

// The wet road over `colour`: soaked, reflecting by `puddled`, with the sun's or the lamps' glint.
fn wet_plus(colour: vec3<f32>, v: VertexOut) -> vec3<f32> {
    if (dot(v.normal, v.normal) < 1e-8) {
        return colour;
    }
    let film = puddled(v);
    let soaking = mix(GROUND_SOAK * frame.wetness.x, 1.0, clamp(film.wet * 2.0, 0.0, 1.0));
    var soaked = soak(colour, soaking) * mix(1.0, PUDDLE_DARK, film.puddle);
    if (frame.wetness.x >= 1.0) {
        soaked *= mix(1.0, mix(FILM_DARK, TARMAC_DARK, smoothstep(TARMAC_SHARE.x, TARMAC_SHARE.y, v.wet.x)), clamp(film.wet * 2.0, 0.0, 1.0) * (1.0 - film.puddle));
    }
    // The asphalt's own texture shows through its film: its contrast raised about `TARMAC_PIVOT`.
    let tarmac = step(1.0, frame.wetness.x) * smoothstep(TARMAC_SHARE.x, TARMAC_SHARE.y, v.wet.x) * clamp(film.wet * 2.0, 0.0, 1.0) * (1.0 - film.puddle);
    soaked *= pow(max(dot(soaked, vec3<f32>(0.299, 0.587, 0.114)), 1e-4) / TARMAC_PIVOT, TARMAC_CONTRAST * tarmac);
    let view = normalize(frame.eye.xyz - v.world);
    let mirrored = reflect(-view, film.normal);
    let daylight = frame.ambient.rgb + frame.diffuse.rgb * max(-frame.sun.y, 0.0);
    var around = mirrored_light(mirrored, daylight);
    if (frame.reflection.x != 0.0) {
        around = ungraded(surrounding(mirrored, mix(2.0, 0.5, film.puddle), v.world));
    }
    var shine = glint(mirrored);
    if (frame.night.x != 0.0) {
        shine = lamp_glint(v.world, mirrored, film.reflectance);
    }
    return mix(soaked, around, film.reflectance) + shine * film.wet * max(road_edge(v), film.puddle);
}
