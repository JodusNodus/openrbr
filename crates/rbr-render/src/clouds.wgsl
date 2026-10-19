
// Moving clouds and their shadows (renderer addition, `Enhance::clouds`, ours; `clouds.rs`). In place of the game's
// cloud faces, a layer of clouds `CLOUD_HEIGHT` above the eye: two-dimensional noise over the stage, as much cover as
// the section's cloud word says (`frame.clouds.x`), drifting with the wind (`frame.clouds.zw`, metres a second) and
// changing shape slowly. Lit by the sun through their thickness (a second, shifted lookup toward the sun), brighter
// at their thin edges and toward the sun, and grey-blue underneath from the sky; at dusk they take the sunlight's
// colour and dim with the sky; at night they are dark against the stars. Toward the horizon they thin into the haze.
// Under clear sections (`frame.sun.w`) the same layer, looked up along the sun from a lit surface, shades the stage
// and the car where a cloud is between them and the sun.
const CLOUD_HEIGHT: f32 = 1800.0;
// The size of the largest cloud shapes, metres.
const CLOUD_SCALE: f32 = 2000.0;
// How far the light lookup reaches toward the sun, metres along the layer.
const CLOUD_LIGHT_REACH: f32 = 220.0;
// How much a cloud's shadow takes from the sunlit stage's light (its baked light holds sun and sky together).
const CLOUD_SHADE: f32 = 0.38;
// How much of the sun on the car a cloud takes.
const CLOUD_CAR_SHADE: f32 = 0.8;

// Fractal value noise over the layer at `p` (in units of `CLOUD_SCALE`), `octaves` of them, slowly evolving.
fn cloud_noise(p: vec2<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var amplitude = 0.5;
    var q = p;
    let t = frame.wind.x * 0.004;
    for (var k = 0; k < octaves; k++) {
        sum += amplitude * value_noise(vec3<f32>(q, t + f32(k) * 3.7));
        q = q * 2.03 + vec2<f32>(1.7, 9.2);
        amplitude *= 0.5;
    }
    return sum / (1.0 - pow(0.5, f32(octaves)));
}

// The cloud's density (0 clear sky to 1 thick) at stage position `xz` on the layer: the shapes from the low octaves,
// their edges eaten into puffs by finer ones, by `detail` (0 leaves them out: far out, where they would
// only shimmer, and for the light and the shadows).
fn cloud_density(xz: vec2<f32>, octaves: i32, detail: f32) -> f32 {
    let cover = frame.clouds.x;
    let p = (xz - frame.clouds.zw * frame.wind.x) / CLOUD_SCALE;
    var n = cloud_noise(p, octaves);
    if (detail > 0.0) {
        let puffs = cloud_noise(p * 6.0 + vec2<f32>(5.3, 1.1), 3);
        n = n - 0.3 * detail * (1.0 - puffs);
    }
    // Cover 0 leaves no cloud, 1 a closed layer.
    let threshold = mix(0.6, 0.12, cover);
    return clamp((n - threshold) / 0.16, 0.0, 1.0);
}

// The sky along `dir` with the clouds over it: the thin, high cirrus, then the clouds of the layer.
fn clouded(colour: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    if (frame.clouds.y == 0.0 || dir.y <= 0.0) {
        return colour;
    }
    return cumulus(cirrus(colour, dir), dir);
}

// Cirrus (ours): streaks of ice cloud `CIRRUS_HEIGHT` up, stretched along the wind, in clear and partly cloudy skies
// (`CIRRUS` of the sky at most, none under a closed layer), lit white by the sun and coloured by it at dusk.
const CIRRUS_HEIGHT: f32 = 8000.0;
const CIRRUS: f32 = 0.55;

fn cirrus(colour: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let amount = CIRRUS * (1.0 - smoothstep(0.45, 0.85, frame.clouds.x));
    if (amount <= 0.0 || frame.sun.w == 0.0) {
        return colour;
    }
    let at = frame.eye.xz + dir.xz * (CIRRUS_HEIGHT / max(dir.y, 0.02)) - frame.clouds.zw * 2.0 * frame.wind.x;
    // Along the wind and across it, stretched four to one.
    let wind = normalize(frame.clouds.zw + vec2<f32>(1e-4, 0.0));
    let p = vec2<f32>(dot(at, wind), dot(at, vec2<f32>(-wind.y, wind.x)) * 4.0) / 9000.0;
    let n = cloud_noise(p + vec2<f32>(31.0, 7.0), 3);
    let density = amount * smoothstep(0.5, 0.8, n) * smoothstep(0.0, 0.1, dir.y);
    var lit = vec3<f32>(1.0);
    if (frame.night.x != 0.0) {
        lit = night_sky_light(dir) * 1.3;
    } else {
        let light = sunlight();
        lit = mix(sky_light(dir), light / max(max(light.r, light.g), light.b), 0.75) * 1.1
            * frame.relight_light.w * frame.dusk.rgb;
    }
    return mix(colour, lit, density);
}

// The layer's clouds over `colour` along `dir`.
fn cumulus(colour: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let t = CLOUD_HEIGHT / max(dir.y, 0.02);
    let at = frame.eye.xz + dir.xz * t;
    let density = cloud_density(at, 3, smoothstep(0.04, 0.12, dir.y));
    if (density <= 0.0) {
        return colour;
    }
    let to_sun = -frame.sun.xyz;
    let toward = normalize(to_sun.xz + vec2<f32>(1e-4, 0.0));
    let shadowed = cloud_density(at + toward * CLOUD_LIGHT_REACH, 2, 0.0);
    // Light through the cloud: the thicker the cloud toward the sun, the darker (Beer), with a soft powder term so
    // the thin edges glow.
    let through = exp(-1.4 * shadowed) * (1.0 - 0.4 * exp(-5.0 * density));
    let mu = dot(dir, to_sun);
    let forward = 1.0 + 1.6 * pow(max(mu, 0.0), 8.0);
    let light = sunlight();
    let sun_colour = light / max(max(light.r, light.g), light.b);
    let sky_above = sky_light(vec3<f32>(0.0, 1.0, 0.0));
    let overcast = frame.sun.w == 0.0;
    var lit: vec3<f32>;
    if (frame.night.x != 0.0) {
        // Moonlit: a little of the night sky's light, so they hide the stars.
        lit = night_sky_light(dir) * (0.6 + 0.6 * through);
    } else if (overcast) {
        // Under a closed overcast the sun is not seen: the clouds are the fog's grey, darker where thick.
        lit = frame.fog_color.rgb * mix(1.08, 0.72, density * shadowed);
    } else {
        let level = frame.relight_light.w * frame.dusk.rgb;
        let base = mix(vec3<f32>(dot(sky_above, vec3<f32>(0.299, 0.587, 0.114))), sky_above, 0.4) * 0.85;
        lit = (base + sun_colour * 1.25 * through * forward * clamp(to_sun.y * 4.0 + 0.3, 0.0, 1.0)) * level;
    }
    // Toward the horizon the clouds go into the haze.
    let far = smoothstep(0.02, 0.25, dir.y);
    lit = mix(sky_light(dir), lit, 0.35 + 0.65 * far);
    let alpha = density * smoothstep(0.0, 0.06, dir.y);
    return mix(colour, lit, alpha);
}

// How much of the sun a cloud takes at `world` (0 none, 1 all), under clear sections by day.
fn cloud_shadow(world: vec3<f32>) -> f32 {
    if (frame.clouds.y == 0.0 || frame.sun.w == 0.0 || frame.night.x != 0.0 || frame.fog_range.z != 0.0) {
        return 0.0;
    }
    let to_sun = -frame.sun.xyz;
    if (to_sun.y <= 0.05) {
        return 0.0;
    }
    let at = world.xz + to_sun.xz / to_sun.y * (CLOUD_HEIGHT + frame.eye.y - world.y);
    return cloud_density(at, 3, 0.0);
}
