// The volumetric clouds (renderer addition, `renderer::cloud_volume`, ours), appended to `shader.wgsl`: a layer of
// cumulus between `VOLUME_BASE` and `VOLUME_BASE + VOLUME_THICKNESS` above the eye, on the earth's curve, raymarched at
// half the frame's size before the stage is drawn, and laid over the sky by `fs_sky` (`cloud_volume_over`). Where the
// clouds are comes from the moving layer's field (`cloud_noise` in `clouds.wgsl`, whose shadows fall on the stage),
// drawn each frame into a map around the eye (`fs_cloud_weather`); their shapes from tiling 3D noise
// (`cloud_noise.wgsl`), after Schneider's Horizon Zero Dawn clouds (2015). Lit by the day's sun through the cloud toward
// it (Beer, with Wrenninge's octaves for the light scattered more than once, as Hillaire's Frostbite clouds, 2016) and a
// two-lobed Henyey-Greenstein phase, and by the day's sky above and the lit ground below, in the renderer's linear light
// (`frame.day_sun`, `frame.day_sky`); under an overcast sky a closed deck, lit from above through it
// (`volume_overcast`); then the haze between them and the eye (`sky_aerial`). Each frame marches from a
// jittered start and blends into the last frame's clouds along the same direction (`cloud_params.previous`), so the
// noise of the few steps settles over about ten frames. Only by day with the sky's tables; else the moving layer.

const VOLUME_BASE: f32 = 1400.0;
const VOLUME_THICKNESS: f32 = 1800.0;
const VOLUME_EARTH: f32 = 6360000.0;
// The farthest cloud marched, and where the clouds start fading into the sky's haze, metres.
const VOLUME_REACH: f32 = 60000.0;
const VOLUME_FADE: f32 = 20000.0;
// The shape and detail textures' tiles, metres.
const VOLUME_SHAPE_TILE: f32 = 2500.0;
const VOLUME_DETAIL_TILE: f32 = 300.0;
// Extinction of the densest cloud, per metre (cumulus about 0.05).
const VOLUME_EXTINCTION: f32 = 0.06;
const VOLUME_STEPS: i32 = 32;
// The steps toward the sun, metres: each this far past the last, doubling.
const VOLUME_LIGHT_STEP: f32 = 40.0;
const VOLUME_LIGHT_STEPS: i32 = 5;
// The share of the new frame in the blend.
const VOLUME_BLEND: f32 = 0.12;

struct CloudParams {
    // The last frame's rotation-only clip matrix (`Renderer::sky_view_projection`).
    previous: mat4x4<f32>,
    // x the frame's number, y 1 when the history holds a frame, z 1 when the depth is this frame's prepass's.
    state: vec4<f32>,
    // The weather map's corner (stage x and z) and its side, metres.
    weather: vec4<f32>,
}

@group(2) @binding(5) var cloud_shape_noise: texture_3d<f32>;
@group(2) @binding(6) var cloud_detail_noise: texture_3d<f32>;
@group(2) @binding(7) var cloud_weather: texture_2d<f32>;
@group(2) @binding(8) var cloud_history: texture_2d<f32>;
@group(2) @binding(9) var<uniform> cloud_params: CloudParams;
@group(2) @binding(10) var cloud_sampler: sampler;
// The clouds over the sky (`fs_sky`): their light (display colour, times their cover) and how much of the sky shows.
@group(1) @binding(7) var cloud_image: texture_2d<f32>;

// The weather map: how cloudy the layer's field is about each point, softer than its shadows' edge (`cloud_shape`).
@fragment
fn fs_cloud_weather(v: SkyOut) -> @location(0) vec4<f32> {
    let uv = vec2<f32>(v.ndc.x, -v.ndc.y) * 0.5 + 0.5;
    let xz = cloud_params.weather.xy + uv * cloud_params.weather.z;
    let p = cloud_at(xz);
    let n = cloud_noise(p + cloud_warp(p) * 0.45, 3);
    // Under an overcast sky a closed deck, thinner and thicker.
    if (frame.sun.w == 0.0) {
        return vec4<f32>(mix(0.5, 1.0, smoothstep(0.25, 0.75, n)), 0.0, 0.0, 1.0);
    }
    let threshold = mix(0.66, 0.2, frame.clouds.x);
    return vec4<f32>(smoothstep(threshold - 0.12, threshold + 0.3, n), 0.0, 0.0, 1.0);
}

// The distance along `dir` (its `y` the sine of its elevation) from the eye to the shell `height` above it.
fn volume_shell(dir_y: f32, height: f32) -> f32 {
    let k = height * (2.0 * VOLUME_EARTH + height);
    let b = VOLUME_EARTH * dir_y;
    let root = sqrt(max(b * b + k, 0.0));
    return select(k / (b + root), root - b, b < 0.0);
}

// The height over the eye of the point `t` along `dir`, on the earth's curve.
fn volume_height(t: f32, dir_y: f32) -> f32 {
    let q = t * t + 2.0 * VOLUME_EARTH * t * dir_y;
    return q / (sqrt(VOLUME_EARTH * VOLUME_EARTH + q) + VOLUME_EARTH);
}

fn volume_remap(x: f32, low: f32, high: f32) -> f32 {
    return clamp((x - low) / max(high - low, 1e-4), 0.0, 1.0);
}

// The cloud's density (0 to 1) at stage point `p` whose height in the layer is `h` (0 base to 1 top); without
// `detail` the shape alone (the light's steps).
fn volume_density(p: vec3<f32>, h: f32, detail: bool) -> f32 {
    let w = textureSampleLevel(cloud_weather, cloud_sampler, (p.xz - cloud_params.weather.xy) / cloud_params.weather.z, 0.0).r;
    if (w <= 0.01 || h <= 0.0 || h >= 1.0) {
        return 0.0;
    }
    // Flat bases, and tops that rise with the cover and round off over their upper half (domes over the thickest).
    let top = mix(0.3, 1.0, w);
    let profile = volume_remap(h, 0.0, 0.07) * (1.0 - volume_remap(h, top * 0.45, top));
    let drift = vec3<f32>(frame.clouds.z, 0.0, frame.clouds.w) * frame.wind.x;
    let q = p - drift + vec3<f32>(0.0, frame.wind.x * 0.5, 0.0);
    let s = textureSampleLevel(cloud_shape_noise, cloud_sampler, q / VOLUME_SHAPE_TILE, 0.0);
    let cells = s.g * 0.625 + s.b * 0.25 + s.a * 0.125;
    let shape = volume_remap(s.r, cells - 1.0, 1.0) * profile;
    var density = volume_remap(shape, 1.0 - w, 1.0) * w;
    if (detail && density > 0.0) {
        let d = textureSampleLevel(cloud_detail_noise, cloud_sampler, q / VOLUME_DETAIL_TILE, 0.0);
        let fine = d.r * 0.625 + d.g * 0.25 + d.b * 0.125;
        // Wispy at the base, billowing above.
        let erode = mix(fine, 1.0 - fine, clamp(h * 4.0, 0.0, 1.0)) * 0.55;
        density = volume_remap(density, erode, 1.0);
    }
    // A cloud's edge is sharp: the density rises to its body's within a few tens of metres.
    return smoothstep(0.0, 0.3, density);
}

fn volume_phase(mu: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * SKY_PI * pow(max(1.0 + g2 - 2.0 * g * mu, 1e-4), 1.5));
}

// The sun's light scattered toward the eye at a point whose path to the sun has optical depth `depth`, with angle
// cosine `mu` between the view and the sun: once, by the phase, and many times, which goes deeper and leaves evenly
// (`VOLUME_DIFFUSE` of the sun's light where it enters, falling by `VOLUME_DIFFUSE_FALL` of the depth): a cloud's
// sunlit side about as bright as a white face square on to the sun, its far side a few times darker.
const VOLUME_DIFFUSE: f32 = 0.25;
const VOLUME_DIFFUSE_FALL: f32 = 0.08;

fn volume_sun(depth: f32, mu: f32) -> f32 {
    let phase = mix(volume_phase(mu, 0.8), volume_phase(mu, -0.3), 0.3);
    return phase * exp(-depth) + VOLUME_DIFFUSE * exp(-depth * VOLUME_DIFFUSE_FALL);
}

// Under an overcast deck the light comes down through it, diffused: at optical depth `depth` below its top about
// `1 / (1 + 0.75 (1 - g) depth)` of it (the two-stream diffusion, g 0.85), scaled so that the deck's base reads about
// as bright as the overcast sky it stands for (`frame.day_sky`).
const VOLUME_OVERCAST: f32 = 2.5;
const VOLUME_DIFFUSION: f32 = 0.11;

fn volume_overcast(depth: f32) -> f32 {
    return VOLUME_OVERCAST / (1.0 + VOLUME_DIFFUSION * depth);
}

// Interleaved gradient noise (Jimenez 2014), for the march's start.
fn volume_jitter(p: vec2<f32>, frame_number: f32) -> f32 {
    let q = p + 5.588238 * (frame_number % 64.0);
    return fract(52.9829189 * fract(dot(q, vec2<f32>(0.06711056, 0.00583715))));
}

// The clouds along `dir`: their light (display colour, times their cover) and how much shows through.
fn volume_march(dir: vec3<f32>, pixel: vec2<f32>) -> vec4<f32> {
    if (dir.y < -0.02) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let start = volume_shell(dir.y, VOLUME_BASE);
    let end = min(volume_shell(dir.y, VOLUME_BASE + VOLUME_THICKNESS), VOLUME_REACH);
    if (start >= end) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let to_sun = -frame.sun.xyz;
    let mu = dot(dir, to_sun);
    let sunlit = frame.sun.w != 0.0;
    // The light's steps go toward the sun, or under an overcast sky up through the deck.
    let toward = select(vec3<f32>(0.0, 1.0, 0.0), to_sun, sunlit);
    let sun = SKY_PI * frame.day_sun.rgb;
    let sky_above = frame.day_sky.rgb;
    // The ground's light under the clouds: its albedo (about 0.15) times the sun's and the sky's on it.
    let ground = 0.15 * (frame.day_sun.rgb * max(to_sun.y, 0.0) + frame.day_sky.rgb);
    let step = (end - start) / f32(VOLUME_STEPS);
    var t = start + step * volume_jitter(pixel, cloud_params.state.x);
    var light = vec3<f32>(0.0);
    var through = 1.0;
    var distance_sum = 0.0;
    var weight_sum = 0.0;
    for (var i = 0; i < VOLUME_STEPS; i++) {
        let h = volume_height(t, dir.y) - VOLUME_BASE;
        let p = frame.eye.xyz + dir * t;
        let density = volume_density(p, h / VOLUME_THICKNESS, true);
        if (density > 0.0) {
            let sigma = density * VOLUME_EXTINCTION;
            var depth = 0.0;
            var reach = 0.0;
            var length = VOLUME_LIGHT_STEP;
            for (var k = 0; k < VOLUME_LIGHT_STEPS; k++) {
                let q = p + toward * (reach + 0.5 * length);
                let hq = (h + toward.y * (reach + 0.5 * length)) / VOLUME_THICKNESS;
                depth += volume_density(q, hq, false) * VOLUME_EXTINCTION * length;
                reach += length;
                length *= 2.0;
            }
            var source = sky_above * volume_overcast(depth);
            if (sunlit) {
                let height = clamp(h / VOLUME_THICKNESS, 0.0, 1.0);
                let ambient = mix(ground, sky_above, height) * mix(0.5, 1.0, height);
                source = sun * volume_sun(depth, mu) + ambient;
            }
            let lost = exp(-sigma * step);
            light += through * source * (1.0 - lost);
            distance_sum += t * through * (1.0 - lost);
            weight_sum += through * (1.0 - lost);
            through *= lost;
            if (through < 0.01) {
                break;
            }
        }
        t += step;
    }
    let cover = 1.0 - through;
    if (cover <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    // The haze between the eye and the clouds, and far off their fading into the sky's.
    let d = distance_sum / max(weight_sum, 1e-6);
    let haze = sky_aerial(dir, d);
    let seen = sky_display(light / cover) * haze.a + haze.rgb;
    let faded = cover * (1.0 - smoothstep(VOLUME_FADE, VOLUME_REACH, d));
    return vec4<f32>(seen * faded, 1.0 - faded);
}

// Linear light to the display colours `sky_display` gives the sky's tables.
fn sky_display(linear: vec3<f32>) -> vec3<f32> {
    return pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}

// The march at half the frame's size, blended into the last frame's along the same direction. Where the stage covers
// the texel (the prepass's depth) it keeps the last frame's, so that the clouds are there when it moves off them.
@fragment
fn fs_cloud_march(v: SkyOut) -> @location(0) vec4<f32> {
    let far = frame.inverse_view_projection * vec4<f32>(v.ndc, 0.0, 1.0);
    let near = frame.inverse_view_projection * vec4<f32>(v.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - near.xyz / near.w);
    var history = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    var known = false;
    if (cloud_params.state.y != 0.0) {
        let last = cloud_params.previous * vec4<f32>(dir, 1.0);
        if (last.w > 0.0) {
            let uv = vec2<f32>(last.x, -last.y) / last.w * 0.5 + 0.5;
            if (all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0))) {
                history = textureSampleLevel(cloud_history, reflection_sampler, uv, 0.0);
                known = true;
            }
        }
    }
    let full = vec2<i32>(v.clip.xy * 2.0);
    if (known && cloud_params.state.z != 0.0 && mask_depth(min(full, vec2<i32>(textureDimensions(scene_depth)) - 1)) > 0.0) {
        return history;
    }
    let now = volume_march(dir, v.clip.xy);
    return select(now, mix(history, now, VOLUME_BLEND), known);
}

// The sky `colour` (display) along screen point `ndc` with the clouds over it.
fn cloud_volume_over(colour: vec3<f32>, ndc: vec2<f32>) -> vec3<f32> {
    let uv = vec2<f32>(ndc.x, -ndc.y) * 0.5 + 0.5;
    let c = textureSampleLevel(cloud_image, reflection_sampler, uv, 0.0);
    return colour * c.a + c.rgb;
}
