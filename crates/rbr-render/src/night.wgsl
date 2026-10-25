// Night (renderer addition, `Atmosphere::night` from the time of day; the game has no night), appended to `shader.wgsl`: the stage under
// moonlight, lit by the car's lamps (`lights::Lamp`), under a night sky with stars and the moon where the sun would be.
// The stage's baked light is daylight, so the stage's albedo is its textures times the bake's occlusion
// (`night_albedo`): moonlight (`frame.moon`) and the lamps light that.
// A lit lamp's own part glows by this times its texture.
const LAMP_GLOW: f32 = 1.6;
// A dial's backlit texel against its texture (`shade`, ours).
const DIAL_GLOW: f32 = 1.6;

// 1 where the headlights reach `world` past the stage, 0 in a shadow (`Renderer::lamp_shadow_matrix` in `renderer/shadows.rs`,
// layer `LAMP_LAYER` of `sun_map`, `sun::LAMP`): a 3x3 filter, looked up off the surface toward the lamp `toward` and along `n`, further with the
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
            lit += textureSampleCompareLevel(sun_map, shadow_sampler, uv + vec2<f32>(x, y) * frame.sun_shadow.y, LAMP_LAYER, c.z - 0.00002);
        }
    }
    return lit / 9.0;
}

// Past this distance from every lit lamp, metres, the lamps give next to nothing (a headlight's hot spot there is a
// ten-thousandth of what it is at 1.5 m) and `lamp_sum` returns at once (ours, for speed).
const LAMP_REACH: f32 = 150.0;

// The light the lit lamps give a surface at `world` with normal `n` (`two_sided` for foliage cards), in the lamps'
// units (`lights::HEADLIGHT` a headlight's hot spot), before the knee (`lamp_knee`). `fs_main` works it out once for
// all its paths.
fn lamp_sum(world: vec3<f32>, n: vec3<f32>, two_sided: bool) -> vec3<f32> {
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
        sum += light * lamp_facing(n, normalize(lamp.position.xyz - world), two_sided);
    }
    return sum;
}

// `lamp_sum`'s light `sum` in lux (`own_light`'s, physical): the headlights' dipped hot spot `HEADLIGHT_CD` (an ECE
// dipped beam's) for `lights::HEADLIGHT` (`HEADLIGHT_UNIT`).
const HEADLIGHT_CD: f32 = 20000.0;
const HEADLIGHT_UNIT: f32 = 3000.0;

fn lamp_lux(sum: vec3<f32>) -> vec3<f32> {
    return sum * (HEADLIGHT_CD / HEADLIGHT_UNIT);
}

// A stage draw's albedo at night, from its textures `surface` (ours): the textures times how much sky the point sees
// (`sky_share`), at least `NIGHT_SHADE`, so that the bake's occlusion stays and its sun goes.
// The bakes' vertex light differs from stage to stage (open ground 0.14 to 0.39); `NIGHT_ALBEDO` gives every stage
// their typical one.
const NIGHT_SHADE: f32 = 0.3;
const NIGHT_ALBEDO: f32 = 0.25;

fn night_albedo(surface: vec3<f32>, v: VertexOut) -> vec3<f32> {
    if (frame.night.w == 0.0) {
        return surface * dot(v.color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    }
    return surface * NIGHT_ALBEDO * max(sky_share(v), NIGHT_SHADE);
}

// The albedo the car's lamps light (ours): `night_albedo` without the bake's occlusion, which is how much of the sky a
// point sees and no shade from a lamp (under trees it left the road dark in the beams).
fn lamp_albedo(surface: vec3<f32>, v: VertexOut) -> vec3<f32> {
    if (frame.night.w == 0.0) {
        return night_albedo(surface, v);
    }
    return surface * NIGHT_ALBEDO;
}

const LAMP_LAYER: i32 = 4;

// The share of the moonlight in the moon's shadow and on faces turned from it (the moonlit sky's), ours.
const MOON_SHADE: f32 = 0.4;

// The moonlit colours' saturation: by moonlight the eye sees little colour (rod vision, the Purkinje shift), by the
// lamps all of it.
const MOON_SATURATION: f32 = 0.5;

// An unlit stage draw's colour at night from its textures `surface`: lit by the moon, under the baked light map `baked`
// (1 where the map is sunlit), or with the sun's shadows on, in the moon's own shadows (the cascades, cast along
// `frame.sun`, the moon's), its colour faded (`MOON_SATURATION`); and by the lamps.
fn night_lit(surface: vec3<f32>, baked: f32, v: VertexOut, front: bool, lamps: vec3<f32>) -> vec3<f32> {
    let has_normal = dot(v.normal, v.normal) > 1e-8;
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
    let albedo = night_albedo(surface, v);
    let grey = vec3<f32>(dot(albedo, vec3<f32>(0.2126, 0.7152, 0.0722)));
    let moon = mix(grey, albedo, MOON_SATURATION) * baked * frame.moon.rgb * moon_lit(v.world, n, has_normal);
    // The clouds and the backdrop: moonlit only.
    if (camera_anchored() || distance(v.world, frame.eye.xyz) > 400.0) {
        return moon;
    }
    return moon + lamp_albedo(surface, v) * lamp_knee(lamps);
}

// The moonlight's share at `world` with normal `n`: 1 in the open, `MOON_SHADE` in the moon's shadow and on faces
// turned from it (foliage cards take light from both sides), fading to 1 where the cascades do not reach.
fn moon_lit(world: vec3<f32>, n: vec3<f32>, has_normal: bool) -> f32 {
    let near = sun_reach(world);
    if (near == 0.0 || camera_anchored()) {
        return 1.0;
    }
    var facing = 1.0;
    if (has_normal && mode.coverage == 0u) {
        facing = smoothstep(-0.05, 0.25, dot(n, -frame.sun.xyz));
    }
    var lit = 0.0;
    if (facing > 0.0) {
        lit = sun_lit(world, n) * facing;
    }
    return mix(1.0, mix(MOON_SHADE, 1.0, lit), near);
}

// The lamps mirrored in a wet road at `world` along `mirrored` (the view reflected about the road), with the film's
// reflectance `f`: each lit lamp's face as wet roads show lights, a streak stretched toward and away from the eye
// (`REFLECTION_SPREAD`: narrow across, long in elevation), as bright as the lamp shines toward that point of the road.
// Headlights (`REFLECTION_BRIGHTNESS.x`) outshine the tail lights (`.y`, brake lights up to `.z`), a rear lamp by its
// colour's strongest channel over a tail lamp's (`REFLECTION_TAIL`, `lights::CANDELA`).
const REFLECTION_SPREAD: vec2<f32> = vec2<f32>(0.03, 0.3);
const REFLECTION_BRIGHTNESS: vec3<f32> = vec3<f32>(12.0, 2.5, 8.0);
const REFLECTION_TAIL: f32 = 5.64;

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
        let tail = clamp(max(lamp.colour.r, max(lamp.colour.g, lamp.colour.b)) / REFLECTION_TAIL, REFLECTION_BRIGHTNESS.y, REFLECTION_BRIGHTNESS.z);
        sum += tint * select(tail, REFLECTION_BRIGHTNESS.x, beam_lamp) * toward * lobe;
    }
    return sum * clamp(3.0 * f, 0.0, 1.0);
}

// The night sky along `dir`: a dark blue gradient, lighter toward the horizon, stars, and the moon (where the sun
// would be) with a halo, as light as the night is (`frame.moon.w`). Under fog (`frame.sun.w` 0): the dark fog only.
const NIGHT_ZENITH: vec3<f32> = vec3<f32>(0.015, 0.025, 0.06);
const NIGHT_HORIZON: vec3<f32> = vec3<f32>(0.0875, 0.105, 0.15);
// Star cells across a unit direction (about a third of a degree each), and the share of cells with a star.
const STAR_CELLS: f32 = 170.0;
const STAR_SHARE: f32 = 0.06;

// With the day's light (`day_on`) the moon's disc is `MOON_LUMINANCE` cd/m2 (a full moon's).
const MOON_LUMINANCE: f32 = 2500.0;

// Whether the day's light is the moon's (`relight::MOON_FROM`, 1 degree down): `frame.sun` is then toward it.
fn moon_up() -> bool {
    return day_on() && frame.skylight.y < -0.0175;
}

fn night_sky_light(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y, 0.0, 1.0);
    if (frame.sun.w == 0.0) {
        return frame.fog_color.rgb * (1.0 + 0.1 * sqrt(up));
    }
    // With the day's light the tables' night sky (`moonlit` in `skylight.wgsl`).
    if (day_on()) {
        return sky_table_light(dir);
    }
    let toward = max(dot(dir, -frame.sun.xyz), 0.0);
    let halo = 0.05 * pow(toward, 8.0) + 0.02 * pow(toward, 2.0);
    return (mix(NIGHT_HORIZON, NIGHT_ZENITH, pow(up, 0.5)) + halo * vec3<f32>(0.6, 0.7, 1.0)) * frame.moon.w;
}

fn night_sky(dir: vec3<f32>) -> vec3<f32> {
    var colour = night_sky_light(dir);
    if (frame.sun.w == 0.0) {
        return colour;
    }
    return moon_shown(dir, starry(dir, moon_aureole(dir, colour)));
}

// `colour` with the stars: one at a hashed spot in some cells of a grid over the directions, fading toward the
// horizon, twinkling. With the day's light they are `STAR_LUMINANCE` cd/m2 a unit of brightness, added in linear
// light, so that they come out as the twilight sky darkens.
const STAR_LUMINANCE: f32 = 4.0;

fn starry(dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    let p = dir * STAR_CELLS;
    let cell = floor(p);
    let h = hash(cell);
    if (h >= STAR_SHARE) {
        return colour;
    }
    let spot = cell + 0.5 + 0.35 * (vec3<f32>(hash(cell + 17.0), hash(cell + 31.0), hash(cell + 53.0)) - 0.5);
    let d = distance(p, spot);
    let brightness = pow(h / STAR_SHARE, 6.0) * 1.6 + 0.08;
    let twinkle = 0.8 + 0.2 * sin(frame.wind.x * (3.0 + 5.0 * hash(cell + 7.0)) + 40.0 * h);
    let tint = mix(vec3<f32>(1.0, 0.85, 0.7), vec3<f32>(0.75, 0.85, 1.0), hash(cell + 91.0));
    let star = tint * brightness * twinkle * (1.0 - smoothstep(0.0, 0.32, d)) * smoothstep(0.0, 0.15, dir.y);
    if (day_on()) {
        return pow(pow(colour, vec3<f32>(2.2)) + star * STAR_LUMINANCE * frame.day_sun.w, vec3<f32>(1.0 / 2.2));
    }
    return colour + star;
}

// With the day's light, `colour` (the tables' night sky) brighter around the moon while it is up: its aureole, the
// moonlight the air scatters near it.
fn moon_aureole(dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    if (!day_on()) {
        return colour;
    }
    let toward = max(dot(dir, -frame.sun.xyz), 0.0);
    let aureole = 1.0 + (4.0 * pow(toward, 8.0) + 0.6 * pow(toward, 2.0)) * frame.moon_disc.w;
    return pow(pow(colour, vec3<f32>(2.2)) * aureole, vec3<f32>(1.0 / 2.2));
}

// The moon's disc's outer radius, radians (its edge in `moon_shown` fades in between cosines 0.99984 and 0.99988), so
// that a full moon's disc is drawn as before its phase was.
const MOON_RADIUS: f32 = 0.0179;

// `colour` with the moon's disc (toward `-frame.sun` once the sun is down), about two degrees across, with darker seas,
// lit on the side toward the sun (`frame.moon_disc`): the phase.
fn moon_shown(dir: vec3<f32>, colour: vec3<f32>) -> vec3<f32> {
    let m = -frame.sun.xyz;
    let c = dot(dir, m);
    let edge = smoothstep(0.99984, 0.99988, c) * frame.moon_disc.w;
    // The moon's sphere under `dir`: its normal faces the eye, back along `m`.
    let q = (dir - m * c) / MOON_RADIUS;
    let n = q - m * sqrt(max(1.0 - dot(q, q), 0.0));
    let disc = edge * smoothstep(-0.03, 0.03, dot(n, frame.moon_disc.xyz));
    if (disc <= 0.0) {
        return colour;
    }
    let seas = 0.8 + 0.2 * value_noise(dir * 3000.0);
    var brightness = select(0.9, 0.9 * SUN_HDR, frame.enhance.x != 0.0);
    if (day_on()) {
        brightness = pow(MOON_LUMINANCE * frame.moon.w * frame.day_sun.w, 1.0 / 2.2);
    }
    return mix(colour, vec3<f32>(0.92, 0.94, 1.0) * seas * brightness, disc);
}
