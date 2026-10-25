// Moving clouds and their shadows (renderer addition, `Enhance::clouds`, ours). The game's cloud faces (`Clouds.lbs`,
// the section's `CloudName`) stay and set the section's mood: they turn slowly about the vertical (`cloud_turn`) and
// take light from the sun's side of the sky (`cloud_face_lit`). Behind them, a layer of clouds `CLOUD_HEIGHT` above
// the eye (`clouded`): two-dimensional noise over the stage, as much cover as the face shows (`frame.clouds.x`,
// `Atmosphere::cloud_layer`), drifting with the wind (`frame.clouds.zw`, metres a second) and changing shape slowly.
// It is strongest overhead, where the faces leave the sky open, and gives way to them toward the horizon, where a flat
// layer would streak. Lit by the sun through its thickness (a second, shifted lookup toward the sun), brighter at the
// thin edges and toward the sun, and grey-blue underneath from the sky; under fog a grey deck; at dusk it takes the
// sunlight's colour and dims with the sky; at night it is dark against the stars. Under clear sections
// (`frame.sun.w`) the same layer, looked up along the sun from a lit surface, shades the stage and the car where a
// cloud is between them and the sun.
const CLOUD_HEIGHT: f32 = 1400.0;
// The size of the largest cloud shapes, metres.
const CLOUD_SCALE: f32 = 1700.0;
// How far the light lookup reaches toward the sun, metres along the layer.
const CLOUD_LIGHT_REACH: f32 = 200.0;
// How much a cloud's shadow takes from the sunlit stage's light (its baked light holds sun and sky together).
const CLOUD_SHADE: f32 = 0.38;
// How much of the sun on the car a cloud takes.
const CLOUD_CAR_SHADE: f32 = 0.8;
// How fast the game's cloud faces turn about the vertical, radians a second: about the layer's angular drift 30
// degrees up, a turn in 25 minutes.
const CLOUD_TURN: f32 = 0.0042;

// A lattice point's value, 0 to 1: Hoskins' hash without sine (all float arithmetic, cheaper than `hash`), good to
// lattice points in the tens of thousands, beyond any the layer reaches.
fn cloud_hash(i: vec2<f32>) -> f32 {
    var p = fract(vec3<f32>(i.x, i.y, i.x) * 0.1031);
    p += dot(p, p.yzx + 33.33);
    return fract((p.x + p.y) * p.z);
}

// Two-dimensional value noise at `p`.
fn cloud_value(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let low = mix(cloud_hash(i), cloud_hash(i + vec2<f32>(1.0, 0.0)), u.x);
    let high = mix(cloud_hash(i + vec2<f32>(0.0, 1.0)), cloud_hash(i + vec2<f32>(1.0, 1.0)), u.x);
    return mix(low, high, u.y);
}

// Fractal value noise over the layer at `p` (in units of `CLOUD_SCALE`), `octaves` of them. Each octave drifts a
// little against the others, so the shapes change slowly as they go.
fn cloud_noise(p: vec2<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var amplitude = 0.5;
    var q = p;
    let t = frame.wind.x * 0.004;
    for (var k = 0; k < octaves; k++) {
        sum += amplitude * cloud_value(q + vec2<f32>(t, -0.6 * t) * f32(k));
        q = q * 2.03 + vec2<f32>(1.7, 9.2);
        amplitude *= 0.5;
    }
    return sum / (1.0 - pow(0.5, f32(octaves)));
}

// The layer's coordinates (units of `CLOUD_SCALE`, drifting with the wind, from the sky's own origin) of stage
// position `xz`.
fn cloud_at(xz: vec2<f32>) -> vec2<f32> {
    return (xz - frame.clouds.zw * frame.wind.x) / CLOUD_SCALE + frame.wind.yw;
}

// The slow warp that bends the shapes at layer position `p`, so they billow rather than repeat. It changes little
// over a light lookup's reach, so that lookup takes the same.
fn cloud_warp(p: vec2<f32>) -> vec2<f32> {
    let t = frame.wind.x * 0.002;
    return vec2<f32>(cloud_value(p * 0.6 + t), cloud_value(p * 0.6 + vec2<f32>(17.3, -t))) - 0.5;
}

// The cloud's density (0 clear sky to 1 thick) at layer position `p` with warp `warp`: shapes from the low octaves,
// their edges eaten into puffs by finer ones by `detail` (0 leaves them out: far out, where they would only shimmer,
// and for the shadows).
fn cloud_shape(p: vec2<f32>, warp: vec2<f32>, octaves: i32, detail: f32) -> f32 {
    var n = cloud_noise(p + warp * 0.45, octaves);
    if (detail > 0.0) {
        let puffs = cloud_noise(p * 7.0 + warp * 2.0 + vec2<f32>(5.3, 1.1), 2);
        n = n - 0.5 * detail * (1.0 - puffs) + 0.18 * detail;
    }
    // Cover 0 leaves no cloud, 1 a closed layer.
    let threshold = mix(0.66, 0.2, frame.clouds.x);
    return smoothstep(threshold, threshold + 0.26, n);
}

// The sky along `dir` with the layer's clouds over it.
fn clouded(colour: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    // Low down the faces and the haze have the sky (`alpha` below).
    if (frame.clouds.y == 0.0 || dir.y <= 0.06) {
        return colour;
    }
    let p = cloud_at(frame.eye.xz + dir.xz * (CLOUD_HEIGHT / dir.y));
    // Overcast faces (`Atmosphere::cloud_layer` 0.72 and up) close the layer into a deck behind them that shows neither
    // the warp, the finest octave and the puffs nor the light through it, so they are left out there.
    let overcast = frame.clouds.x >= 0.7;
    var warp = vec2<f32>(0.0);
    var density: f32;
    if (overcast) {
        density = cloud_shape(p, warp, 2, 0.0);
    } else {
        warp = cloud_warp(p);
        density = cloud_shape(p, warp, 3, smoothstep(0.1, 0.3, dir.y));
    }
    if (density <= 0.0) {
        return colour;
    }
    let to_sun = -frame.sun.xyz;
    let toward = normalize(to_sun.xz + vec2<f32>(1e-4, 0.0));
    // Light through the cloud: the thicker the cloud toward the sun, the darker (Beer), never below what the light
    // scattered inside it gives; a soft powder term so the thin edges glow. Overcast, the cloud's own density.
    var near = density;
    if (!overcast) {
        near = cloud_shape(p + toward * (CLOUD_LIGHT_REACH * 0.5 / CLOUD_SCALE), warp, 2, 0.0);
    }
    let through = mix(0.3, 1.0, exp(-2.0 * near)) * (1.0 - 0.35 * exp(-6.0 * density));
    let mu = dot(dir, to_sun);
    let forward = 1.0 + 1.2 * pow(max(mu, 0.0), 8.0);
    let sun_colour = direct_colour();
    let sky_above = sky_light(vec3<f32>(0.0, 1.0, 0.0));
    var lit: vec3<f32>;
    if (is_night() && !day_on()) {
        // Moonlit: a little of the night sky's light, so they hide the stars.
        lit = night_sky_light(dir) * (0.6 + 0.6 * through);
    } else if (!sun_out()) {
        // Under fog the sun is not seen: a grey deck, the fog's colour (with the sky's tables the overcast sky's),
        // darker where thick, faded as the faces are.
        let deck = select(frame.fog_color.rgb, sky_above, sky_on());
        lit = mix(deck * mix(1.06, 0.8, density * near), colour, 0.5 * frame.haze.y);
    } else {
        let level = day_level();
        let base = mix(vec3<f32>(dot(sky_above, vec3<f32>(0.299, 0.587, 0.114))), sky_above, 0.4) * 0.7;
        lit = (base + sun_colour * 0.95 * through * forward * clamp(to_sun.y * 4.0 + 0.3, 0.0, 1.0)) * level;
    }
    // Toward the horizon the layer gives way to the faces and the haze.
    let alpha = density * smoothstep(0.06, 0.4, dir.y);
    return mix(colour, lit, alpha);
}

// The direct light's colour, brightest channel 1: the sun's at the eye (`sunlight`), or the moon's once it is the
// day's light (`moon_up`; white under an overcast sky, which has none).
fn direct_colour() -> vec3<f32> {
    var light = sunlight();
    if (moon_up()) {
        light = select(frame.day_sun.rgb, vec3<f32>(1.0), dot(frame.day_sun.rgb, frame.day_sun.rgb) <= 0.0);
    }
    return light / max(max(light.r, light.g), max(light.b, 1e-12));
}

// A game cloud face's vertex `position` (camera-anchored, a direction) turned about the vertical by the time.
fn cloud_turn(position: vec3<f32>) -> vec3<f32> {
    let a = CLOUD_TURN * frame.wind.x;
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * position.x - s * position.z, position.y, s * position.x + c * position.z);
}

// A game cloud face's `colour` (its texture, vertex colour and day light) of coverage `alpha` along `dir`, lit from
// the sun's side of the sky: away from the sun a little greyer, toward it brighter, with the thin edges glowing,
// and toward a low sun in its colour.
fn cloud_face_lit(colour: vec3<f32>, alpha: f32, dir: vec3<f32>) -> vec3<f32> {
    if (!sun_out() || (is_night() && !day_on())) {
        return colour;
    }
    let to_sun = -frame.sun.xyz;
    let mu = dot(dir, to_sun);
    let sun_colour = direct_colour();
    let low = 1.0 - smoothstep(0.05, 0.5, to_sun.y);
    let side = mix(0.82, 1.06, 0.5 + 0.5 * mu);
    let edge = 0.5 * pow(max(mu, 0.0), 6.0) * (1.0 - alpha);
    return colour * (side + edge) * mix(vec3<f32>(1.0), sun_colour, low * (0.2 + 0.7 * max(mu, 0.0) * max(mu, 0.0)));
}

// How much of the sun a cloud takes at `world` (0 none, 1 all), under clear sections by day.
fn cloud_shadow(world: vec3<f32>) -> f32 {
    if (frame.clouds.y == 0.0 || !sun_out() || (is_night() && !day_on()) || camera_anchored()) {
        return 0.0;
    }
    let to_sun = -frame.sun.xyz;
    if (to_sun.y <= 0.05) {
        return 0.0;
    }
    // At the volumetric clouds' height when they are drawn (`frame.occlusion.y`).
    let height = select(CLOUD_HEIGHT, frame.occlusion.y, frame.occlusion.y > 0.0);
    let p = cloud_at(world.xz + to_sun.xz / to_sun.y * (height + frame.eye.y - world.y));
    return cloud_shape(p, cloud_warp(p), 3, 0.0);
}
