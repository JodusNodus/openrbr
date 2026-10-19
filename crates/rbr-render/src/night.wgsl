// Night (renderer addition, `Atmosphere::night` from the time of day; the game has no night), appended to `shader.wgsl`: the stage under
// moonlight, lit by the car's lamps (`lights::Lamp`), under a night sky with stars and the moon where the sun would be.
// The stage's baked light is daylight, so the stage keeps its colour times the baked light as its albedo: moonlight
// (`MOONLIGHT`) and the lamps light that.

// The moon's light on the stage, as a share of the baked daylight.
const MOONLIGHT: vec3<f32> = vec3<f32>(0.11, 0.13, 0.19);
// A lamp's light falling on a face turned from it: none for terrain (`WRAP` of it for the grazing road), half for
// foliage cards lit through.
const WRAP: f32 = 0.4;
// The most light a lamp gives a surface, a soft knee (the beam's foot is very bright on a tree in front of the car).
const LAMP_KNEE: f32 = 2.5;
// A lit lamp's own part glows by this times its texture.
const LAMP_GLOW: f32 = 1.6;
// A dial's backlit texel against its texture (`shade`, ours).
const DIAL_GLOW: f32 = 1.6;

// 1 where the headlights reach `world` past the stage, 0 in a shadow (`Renderer::lamp_shadow_matrix` in `renderer/shadows.rs`,
// layer 0 of `sun_map`): a 3x3 filter, looked up off the surface toward the lamp `toward` and along `n`, further with the
// distance (trunk cards have no normal to step along).
fn lamp_reach(world: vec3<f32>, n: vec3<f32>, toward: vec3<f32>) -> f32 {
    if (frame.night.y == 0.0) {
        return 1.0;
    }
    let offset = 0.03 + 0.015 * distance(world, frame.eye.xyz);
    let p = frame.lamp_matrix * vec4<f32>(world + (toward + 0.5 * n) * offset, 1.0);
    if (p.w <= 0.0) {
        return 1.0;
    }
    let c = p.xyz / p.w;
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || c.z > 1.0) {
        return 1.0;
    }
    var lit = 0.0;
    for (var y = -1.0; y < 1.5; y += 1.0) {
        for (var x = -1.0; x < 1.5; x += 1.0) {
            lit += textureSampleCompareLevel(sun_map, shadow_sampler, uv + vec2<f32>(x, y) * frame.sun_shadow.y, 0, c.z - 0.00002);
        }
    }
    return lit / 9.0;
}

// Past this distance from every lit lamp, metres, the lamps give next to nothing (a headlight's hot spot there is a
// ten-thousandth of what it is at 1.5 m) and `lamp_light` returns at once (ours, for speed).
const LAMP_REACH: f32 = 150.0;

// The light the lit lamps give a surface at `world` with normal `n` (`two_sided` for foliage cards).
fn lamp_light(world: vec3<f32>, n: vec3<f32>, two_sided: bool) -> vec3<f32> {
    var near = false;
    for (var i = 0; i < 8; i++) {
        let lamp = frame.lamps[i];
        if (lamp.position.w != 0.0 && distance(world, lamp.position.xyz) < LAMP_REACH) {
            near = true;
            break;
        }
    }
    if (!near) {
        return vec3<f32>(0.0);
    }
    var sum = vec3<f32>(0.0);
    var shadow = -1.0;
    for (var i = 0; i < 8; i++) {
        let lamp = frame.lamps[i];
        var light = lamp_toward(lamp, world);
        if (all(light == vec3<f32>(0.0))) {
            continue;
        }
        if (lamp.forward.w != 0.0) {
            if (shadow < 0.0) {
                shadow = lamp_reach(world, n, normalize(lamp.position.xyz - world));
            }
            light *= shadow;
        }
        let ndl = dot(n, normalize(lamp.position.xyz - world));
        let facing = select(max(ndl, 0.0) * (1.0 - WRAP) + WRAP * smoothstep(-0.2, 0.2, ndl), mix(abs(ndl), 1.0, 0.5), two_sided);
        sum += light * facing;
    }
    return LAMP_KNEE * sum / (LAMP_KNEE + sum);
}

// An unlit stage draw's colour at night: `unbaked` its colour before the baked light map `baked` (1 where the map is
// sunlit), lit by the moon and the lamps.
fn night_lit(unbaked: vec3<f32>, baked: f32, v: VertexOut, front: bool) -> vec3<f32> {
    let moon = unbaked * baked * MOONLIGHT;
    // The clouds and the backdrop: moonlit only.
    if (frame.fog_range.z != 0.0 || distance(v.world, frame.eye.xyz) > 400.0) {
        return moon;
    }
    let has_normal = dot(v.normal, v.normal) > 1e-8;
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
    return moon + unbaked * lamp_light(v.world, n, mode.coverage != 0u);
}

// The lamps mirrored in a wet road at `world` along `mirrored` (the view reflected about the road), with the film's
// reflectance `f`: each lit lamp's face as wet roads show lights, a streak stretched toward and away from the eye
// (`REFLECTION_SPREAD`: narrow across, long in elevation), as bright as the lamp shines toward that point of the road.
// Headlights (`REFLECTION_BRIGHTNESS.x`) outshine the tail lights (`.y`, brake lights up to `.z`).
const REFLECTION_SPREAD: vec2<f32> = vec2<f32>(0.03, 0.3);
const REFLECTION_BRIGHTNESS: vec3<f32> = vec3<f32>(12.0, 2.5, 8.0);

fn lamp_glint(world: vec3<f32>, mirrored: vec3<f32>, f: f32) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let across = normalize(vec2<f32>(mirrored.x, mirrored.z) + vec2<f32>(1e-6, 0.0));
    for (var i = 0; i < 8; i++) {
        let lamp = frame.lamps[i];
        if (lamp.position.w == 0.0) {
            continue;
        }
        let to = lamp.position.xyz - world;
        let d = length(to);
        let l = to / d;
        let flat = normalize(vec2<f32>(l.x, l.z) + vec2<f32>(1e-6, 0.0));
        // The angle between the two directions across, and their difference in elevation; the lamp's own width widens
        // the streak near it.
        let side = abs(across.x * flat.y - across.y * flat.x) / (REFLECTION_SPREAD.x + 0.1 / d);
        let rise = (mirrored.y - l.y) / REFLECTION_SPREAD.y;
        let lobe = exp(-side * side - rise * rise) * f32(dot(across, flat) > 0.0);
        let beam_lamp = lamp.forward.w != 0.0;
        let toward = select(max(dot(-l, lamp.forward.xyz), 0.0), min(beam(lamp, -l) * 4.0, 1.0), beam_lamp);
        let tint = lamp.colour.rgb / max(max(lamp.colour.r, lamp.colour.g), max(lamp.colour.b, 1e-6));
        let tail = clamp(max(lamp.colour.r, max(lamp.colour.g, lamp.colour.b)) / 0.15, REFLECTION_BRIGHTNESS.y, REFLECTION_BRIGHTNESS.z);
        sum += tint * select(tail, REFLECTION_BRIGHTNESS.x, beam_lamp) * toward * lobe;
    }
    return sum * clamp(3.0 * f, 0.0, 1.0);
}

// The night sky along `dir`: a dark blue gradient, lighter toward the horizon, stars, and the moon (where the sun
// would be) with a halo. Under fog (`frame.sun.w` 0): the dark fog only.
const NIGHT_ZENITH: vec3<f32> = vec3<f32>(0.006, 0.01, 0.024);
const NIGHT_HORIZON: vec3<f32> = vec3<f32>(0.035, 0.042, 0.06);
// Star cells across a unit direction (about a third of a degree each), and the share of cells with a star.
const STAR_CELLS: f32 = 170.0;
const STAR_SHARE: f32 = 0.06;

fn night_sky_light(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y, 0.0, 1.0);
    if (frame.sun.w == 0.0) {
        return frame.fog_color.rgb * (1.0 + 0.1 * sqrt(up));
    }
    let moon = -frame.sun.xyz;
    let halo = 0.05 * pow(max(dot(dir, moon), 0.0), 8.0) + 0.02 * pow(max(dot(dir, moon), 0.0), 2.0);
    return mix(NIGHT_HORIZON, NIGHT_ZENITH, pow(up, 0.5)) + halo * vec3<f32>(0.6, 0.7, 1.0);
}

fn night_sky(dir: vec3<f32>) -> vec3<f32> {
    var colour = night_sky_light(dir);
    if (frame.sun.w == 0.0) {
        return colour;
    }
    let up = dir.y;
    // Stars: one at a hashed spot in some cells of a grid over the directions, fading toward the horizon, twinkling.
    let p = dir * STAR_CELLS;
    let cell = floor(p);
    let h = hash(cell);
    if (h < STAR_SHARE) {
        let spot = cell + 0.5 + 0.35 * (vec3<f32>(hash(cell + 17.0), hash(cell + 31.0), hash(cell + 53.0)) - 0.5);
        let d = distance(p, spot);
        let brightness = pow(h / STAR_SHARE, 6.0) * 1.6 + 0.08;
        let twinkle = 0.8 + 0.2 * sin(frame.wind.x * (3.0 + 5.0 * hash(cell + 7.0)) + 40.0 * h);
        let tint = mix(vec3<f32>(1.0, 0.85, 0.7), vec3<f32>(0.75, 0.85, 1.0), hash(cell + 91.0));
        colour += tint * brightness * twinkle * (1.0 - smoothstep(0.0, 0.32, d)) * smoothstep(0.0, 0.15, up);
    }
    // The moon: a disc about a degree across, with darker seas.
    let moon = -frame.sun.xyz;
    let mu = dot(dir, moon);
    let disc = smoothstep(0.99984, 0.99988, mu);
    if (disc > 0.0) {
        let seas = 0.8 + 0.2 * value_noise(dir * 3000.0);
        let brightness = select(0.9, 0.9 * SUN_HDR, frame.enhance.x != 0.0);
        colour = mix(colour, vec3<f32>(0.92, 0.94, 1.0) * seas * brightness, disc);
    }
    return colour;
}
