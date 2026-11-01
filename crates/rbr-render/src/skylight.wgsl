// The physically based sky's tables (renderer addition, `Enhance::lighting`, ours; `skylight.rs`), after Hillaire, "A
// Scalable and Production Ready Sky and Atmosphere Rendering Technique" (EGSR 2020): Rayleigh and Mie scattering and
// ozone absorption over a spherical planet, in kilometres, for a sun of unit illuminance. The tables:
// - transmittance: from a height toward a direction to the top of the air (`TRANSMITTANCE`, Bruneton's mapping);
// - multiple scattering: the light scattered twice or more, as an isotropic source at each height and sun angle;
// - sky view: the sky's light by elevation and azimuth from the sun, seen from the eye;
// - aerial perspective: the light the air between the eye and a point adds, and how much of the point it lets
//   through, by direction (as the sky view) and distance; the air is the same all round, so one table serves every
//   view, the reflection cube's too;
// - the sky cube and the sky's light (`fs`, `cs_light`): the sky view on a cube, blurred for rougher reflections
//   level by level, and its irradiance as nine spherical harmonics, and the day's light on the stage, with the gain
//   that brings it and the sky to the renderer's light (`day_light`).
// Besides the clear air there is a ground haze (`Sky::haze`), as dense as the stage's fog says (`skylight.rs`).

struct Sky {
    // Toward the sun (stage space, y up, the sun's true elevation).
    to_sun: vec4<f32>,
    // Scattering per km at the ground, and w the scale height, km: air (Rayleigh), aerosol (Mie), ground haze (Mie).
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    haze: vec4<f32>,
    // The ground's albedo, and w the eye's height, km.
    ground: vec4<f32>,
    // x Mie's anisotropy g, y the aerial table's reach, km, z the tables' gain (they stay in half floats after dusk),
    // w 1 under an overcast sky (`Atmosphere::overcast`): then the sky is the cloud deck's (`overcast_light`).
    shape: vec4<f32>,
    // y the cloud cover (0 to 1).
    level: vec4<f32>,
    // Once the sun is down, the moon's light square on, lux, and w the sine of its elevation (`skylight::moon_light`);
    // the night sky's light on up-facing ground, lux (`moonlit`).
    moon: vec4<f32>,
    night_sky: vec4<f32>,
}

struct SkyLight {
    // The irradiance, as nine spherical harmonics of colour, relative to an up-facing surface's (luminance 1).
    sh: array<vec4<f32>, 9>,
    // x the scale from the tables to the renderer's linear light, y the clear sky's up-facing irradiance in table
    // units, z the sky's (`cs_light`), w the clear sky's with the sun's (the overcast's, `overcast_light`).
    scale: vec4<f32>,
    // The sunlight's colour at the eye (`sky_table_sunlight`).
    sun: vec4<f32>,
    // `sh` of the sky above the horizon alone (the ground's light below it is the stage's own, `bounce.rs`).
    sky_sh: array<vec4<f32>, 9>,
    // The eye's white balance (`day_light`), by which the sky is drawn as the stage's light.
    balance: vec4<f32>,
    // The day's light on the stage (`day_light`), renderer units: the sun's square on, w the pre-exposure (renderer
    // units per cd/m2); the sky's above the horizon on up-facing ground, w the gain from the tables' units.
    day_sun: vec4<f32>,
    day_sky: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sky: Sky;
@group(0) @binding(1) var lut_sampler: sampler;
@group(0) @binding(2) var transmittance_lut: texture_2d<f32>;
@group(0) @binding(3) var multiscatter_lut: texture_2d<f32>;
@group(0) @binding(4) var skyview_lut: texture_2d<f32>;
@group(0) @binding(5) var transmittance_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(6) var multiscatter_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(7) var skyview_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(8) var aerial_out: texture_storage_3d<rgba16float, write>;
@group(0) @binding(9) var<storage, read_write> light_out: SkyLight;
@group(0) @binding(10) var shown_out: texture_storage_2d<rgba16float, write>;
@group(1) @binding(0) var cube_out: texture_storage_2d_array<rgba16float, write>;
// x the level's blur (0 sharp, 1 the roughest), y its size in texels.
@group(1) @binding(1) var<uniform> cube_level: vec4<f32>;

const PI: f32 = 3.14159265;
const BOTTOM: f32 = 6360.0;
const TOP: f32 = 6460.0;
const OZONE: vec3<f32> = vec3<f32>(0.650e-3, 1.881e-3, 0.085e-3);
// Mie absorption over scattering (Hillaire's 4.40 / 3.996).
const MIE_ABSORPTION: f32 = 0.1;

// Distance along `mu` from radius `r` to the sphere of radius `radius`: the far hit, or -1 when there is none.
fn to_sphere(r: f32, mu: f32, radius: f32) -> f32 {
    let d = r * r * (mu * mu - 1.0) + radius * radius;
    if (d < 0.0) {
        return -1.0;
    }
    return -r * mu + sqrt(d);
}

// Distance to the ground along `mu` from `r`, or -1 when the ray misses it.
fn to_ground(r: f32, mu: f32) -> f32 {
    let d = r * r * (mu * mu - 1.0) + BOTTOM * BOTTOM;
    if (d < 0.0 || mu > 0.0) {
        return -1.0;
    }
    return -r * mu - sqrt(d);
}

struct Medium {
    rayleigh: vec3<f32>,
    mie: vec3<f32>,
    extinction: vec3<f32>,
}

fn medium(r: f32) -> Medium {
    let h = max(r - BOTTOM, 0.0);
    var m: Medium;
    m.rayleigh = sky.rayleigh.rgb * exp(-h / sky.rayleigh.w);
    let aerosol = sky.mie.rgb * exp(-h / sky.mie.w);
    let haze = sky.haze.rgb * exp(-h / sky.haze.w);
    m.mie = aerosol + haze;
    let ozone = OZONE * max(0.0, 1.0 - abs(h - 25.0) / 15.0);
    m.extinction = m.rayleigh + aerosol * (1.0 + MIE_ABSORPTION) + haze + ozone;
    return m;
}

fn rayleigh_phase(c: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + c * c);
}

// Cornette-Shanks.
fn mie_phase(c: f32) -> f32 {
    let g = sky.shape.x;
    let k = 3.0 / (8.0 * PI) * (1.0 - g * g) / (2.0 + g * g);
    return k * (1.0 + c * c) / pow(max(1.0 + g * g - 2.0 * g * c, 1e-4), 1.5);
}

// Bruneton's transmittance table mapping: uv to (r, mu) and back.
fn transmittance_params(uv: vec2<f32>) -> vec2<f32> {
    let h = sqrt(TOP * TOP - BOTTOM * BOTTOM);
    let rho = h * uv.y;
    let r = sqrt(rho * rho + BOTTOM * BOTTOM);
    let d_min = TOP - r;
    let d_max = rho + h;
    let d = d_min + uv.x * (d_max - d_min);
    var mu = 1.0;
    if (d > 0.0) {
        mu = clamp((h * h - rho * rho - d * d) / (2.0 * r * d), -1.0, 1.0);
    }
    return vec2<f32>(r, mu);
}

fn transmittance_uv(r: f32, mu: f32) -> vec2<f32> {
    let h = sqrt(TOP * TOP - BOTTOM * BOTTOM);
    let rho = sqrt(max(r * r - BOTTOM * BOTTOM, 0.0));
    let d = max(to_sphere(r, mu, TOP), 0.0);
    let d_min = TOP - r;
    let d_max = rho + h;
    return vec2<f32>((d - d_min) / max(d_max - d_min, 1e-4), rho / h);
}

fn transmittance_to_top(r: f32, mu: f32) -> vec3<f32> {
    return textureSampleLevel(transmittance_lut, lut_sampler, transmittance_uv(r, mu), 0.0).rgb;
}

// The sun's light reaching radius `r` with the sun at cosine `mu_sun` from the zenith: none in the planet's shadow.
fn sunlight_at(r: f32, mu_sun: f32) -> vec3<f32> {
    if (to_ground(r, mu_sun) > 0.0) {
        return vec3<f32>(0.0);
    }
    return transmittance_to_top(r, mu_sun);
}

fn multiscatter(r: f32, mu_sun: f32) -> vec3<f32> {
    let uv = vec2<f32>(mu_sun * 0.5 + 0.5, (r - BOTTOM) / (TOP - BOTTOM));
    return textureSampleLevel(multiscatter_lut, lut_sampler, uv, 0.0).rgb;
}

@compute @workgroup_size(8, 8)
fn cs_transmittance(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(transmittance_out);
    if (any(id.xy >= size)) {
        return;
    }
    let p = transmittance_params((vec2<f32>(id.xy) + 0.5) / vec2<f32>(size));
    let length = to_sphere(p.x, p.y, TOP);
    let steps = 40;
    var depth = vec3<f32>(0.0);
    for (var i = 0; i < steps; i++) {
        let t = (f32(i) + 0.5) / f32(steps) * length;
        let r = sqrt(p.x * p.x + t * t + 2.0 * p.x * p.y * t);
        depth += medium(r).extinction * (length / f32(steps));
    }
    textureStore(transmittance_out, id.xy, vec4<f32>(exp(-depth), 1.0));
}

// The light scattered once along a ray from radius `r` (the view's zenith cosine `mu`, the cosine to the sun `nu`,
// the sun's zenith cosine `mu_sun`) over `length` km, with the multiple scattering term, and the transmittance
// through it. `ground` adds the light the ground reflects where the ray ends on it.
struct March {
    light: vec3<f32>,
    transmittance: vec3<f32>,
}

fn march(r0: f32, mu: f32, nu: f32, mu_sun: f32, length: f32, steps: i32, ground: bool, multiple: bool) -> March {
    var out: March;
    out.light = vec3<f32>(0.0);
    out.transmittance = vec3<f32>(1.0);
    let phase_r = rayleigh_phase(nu);
    let phase_m = mie_phase(nu);
    var t = 0.0;
    for (var i = 0; i < steps; i++) {
        let t_next = (f32(i) + 1.0) / f32(steps) * length;
        let dt = t_next - t;
        let tm = t + 0.5 * dt;
        t = t_next;
        let r = sqrt(r0 * r0 + tm * tm + 2.0 * r0 * mu * tm);
        // The sun's and the view's angles at the sample, on the sphere.
        let mu_sun_here = clamp((r0 * mu_sun + tm * nu) / r, -1.0, 1.0);
        let m = medium(r);
        let sun = sunlight_at(r, mu_sun_here);
        var s = sun * (m.rayleigh * phase_r + m.mie * phase_m);
        if (multiple) {
            s += multiscatter(r, mu_sun_here) * (m.rayleigh + m.mie);
        }
        let step_t = exp(-m.extinction * dt);
        let integral = (s - s * step_t) / max(m.extinction, vec3<f32>(1e-6));
        out.light += out.transmittance * integral;
        out.transmittance *= step_t;
    }
    if (ground) {
        let r = sqrt(r0 * r0 + length * length + 2.0 * r0 * mu * length);
        let mu_sun_here = clamp((r0 * mu_sun + length * nu) / r, -1.0, 1.0);
        out.light += out.transmittance * sunlight_at(r, mu_sun_here) * max(mu_sun_here, 0.0) * sky.ground.rgb / PI;
    }
    return out;
}

// The multiple scattering table (Hillaire's section 5.5): from each height and sun angle, 64 directions, the light
// scattered once more and the share that scatters again (f_ms); their geometric sum is the source.
@compute @workgroup_size(8, 8)
fn cs_multiscatter(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(multiscatter_out);
    if (any(id.xy >= size)) {
        return;
    }
    let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(size);
    let mu_sun = uv.x * 2.0 - 1.0;
    let r = BOTTOM + uv.y * (TOP - BOTTOM) + 0.01;
    var second = vec3<f32>(0.0);
    var share = vec3<f32>(0.0);
    let n = 8;
    for (var a = 0; a < n; a++) {
        for (var b = 0; b < n; b++) {
            let mu = 1.0 - 2.0 * (f32(a) + 0.5) / f32(n);
            let phi = 2.0 * PI * (f32(b) + 0.5) / f32(n);
            let s = sqrt(1.0 - mu * mu);
            let dir = vec3<f32>(s * cos(phi), mu, s * sin(phi));
            let sun = vec3<f32>(sqrt(1.0 - mu_sun * mu_sun), mu_sun, 0.0);
            let nu = dot(dir, sun);
            let ground = to_ground(r, mu);
            let length = select(to_sphere(r, mu, TOP), ground, ground > 0.0);
            // Isotropic phase for both: the light is spread evenly.
            var light = vec3<f32>(0.0);
            var scattered = vec3<f32>(0.0);
            var transmittance = vec3<f32>(1.0);
            var t = 0.0;
            let steps = 20;
            for (var i = 0; i < steps; i++) {
                let t_next = (f32(i) + 1.0) / f32(steps) * length;
                let dt = t_next - t;
                let tm = t + 0.5 * dt;
                t = t_next;
                let rr = sqrt(r * r + tm * tm + 2.0 * r * mu * tm);
                let mu_sun_here = clamp((r * mu_sun + tm * nu) / rr, -1.0, 1.0);
                let m = medium(rr);
                let scattering = m.rayleigh + m.mie;
                let step_t = exp(-m.extinction * dt);
                let integral = (vec3<f32>(1.0) - step_t) / max(m.extinction, vec3<f32>(1e-6));
                light += transmittance * sunlight_at(rr, mu_sun_here) * scattering / (4.0 * PI) * integral;
                scattered += transmittance * scattering * integral;
                transmittance *= step_t;
            }
            if (ground > 0.0) {
                let rr = sqrt(r * r + length * length + 2.0 * r * mu * length);
                let mu_sun_here = clamp((r * mu_sun + length * nu) / rr, -1.0, 1.0);
                light += transmittance * sunlight_at(rr, mu_sun_here) * max(mu_sun_here, 0.0) * sky.ground.rgb / PI;
            }
            second += light / f32(n * n);
            share += scattered / (4.0 * PI) * (4.0 * PI) / f32(n * n);
        }
    }
    let psi = second / max(vec3<f32>(1.0) - share, vec3<f32>(1e-3));
    textureStore(multiscatter_out, id.xy, vec4<f32>(psi, 1.0));
}

// The sky view's and the aerial table's mapping: x the azimuth from the sun's, 0 to pi (the sky is symmetric about
// the sun's vertical plane), y the elevation, squeezed toward the horizon where the sky changes fastest.
fn view_params(uv: vec2<f32>) -> vec2<f32> {
    let azimuth = uv.x * PI;
    let side = uv.y * 2.0 - 1.0;
    let elevation = sign(side) * side * side * 0.5 * PI;
    return vec2<f32>(azimuth, elevation);
}

// In a frame where the sun lies in the x-y plane: the view direction for an azimuth from the sun and an elevation.
fn view_dir(p: vec2<f32>) -> vec3<f32> {
    return vec3<f32>(cos(p.y) * cos(p.x), sin(p.y), cos(p.y) * sin(p.x));
}

fn sun_in_frame() -> vec3<f32> {
    let s = normalize(sky.to_sun.xyz);
    return vec3<f32>(length(s.xz), s.y, 0.0);
}

fn eye_radius() -> f32 {
    return BOTTOM + sky.ground.w;
}

// The sky's light along `dir` (sun frame) from the eye, without the sun's disc.
fn sky_along(dir: vec3<f32>, steps: i32) -> vec3<f32> {
    let r = eye_radius();
    let sun = sun_in_frame();
    let ground = to_ground(r, dir.y);
    let length = select(to_sphere(r, dir.y, TOP), ground, ground > 0.0);
    return march(r, dir.y, dot(dir, sun), sun.y, length, steps, ground > 0.0, true).light;
}

@compute @workgroup_size(8, 8)
fn cs_skyview(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(skyview_out);
    if (any(id.xy >= size)) {
        return;
    }
    let dir = view_dir(view_params((vec2<f32>(id.xy) + 0.5) / vec2<f32>(size)));
    textureStore(skyview_out, id.xy, vec4<f32>((sky_along(dir, 30) + moonlit(dir)) * sky.shape.z, 1.0));
}

// The night sky's light along `dir` (sun frame), in the tables' units (the sun's light above the air 1): the moonlit
// sky's and the night sky's own (`sky.night_sky`, `skylight::moon_light`), `NIGHT_SKY_HORIZON` of its mean luminance at
// the horizon and `NIGHT_SKY_ZENITH` overhead. The tables' twilight leaves it out (the sun's light alone), so the sky
// view, the haze and the irradiance take it in here. The glow around the moon is drawn with it (`moon_aureole` in
// `night.wgsl`): the tables are folded about the sun's azimuth, and the moon is not opposite the sun.
const NIGHT_SKY_HORIZON: f32 = 1.6;
const NIGHT_SKY_ZENITH: f32 = 0.8;

fn moonlit(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y, 0.0, 1.0);
    return sky.night_sky.rgb / (SUN_LUX * PI) * mix(NIGHT_SKY_HORIZON, NIGHT_SKY_ZENITH, sqrt(up));
}

// The sky view as the stage shader shows it (`sky_table_light`): at the renderer's scale, in its display colour.
@compute @workgroup_size(8, 8)
fn cs_shown(@builtin(global_invocation_id) id: vec3<u32>) {
    if (any(id.xy >= textureDimensions(shown_out))) {
        return;
    }
    var l = textureLoad(skyview_lut, id.xy, 0).rgb;
    // Under an overcast sky, the cloud deck (its light depends on the elevation alone, the same in the sun's frame).
    if (sky.shape.w != 0.0) {
        let size = vec2<f32>(textureDimensions(shown_out));
        l = overcast_light(view_dir(view_params((vec2<f32>(id.xy) + 0.5) / size)));
    }
    textureStore(shown_out, id.xy, vec4<f32>(sky_display(l * shown_scale()), 1.0));
}

// The aerial table: for each direction, the haze's colour (the light added over the cover, at the renderer's scale, as
// `sky_display` shows it) and the mean transmittance at each distance slice (`(k + 0.5) / N` squared of the reach).
// The colour is the table's, not each pixel's: per pixel its powers were most of the haze's cost (`sky_haze`).
@compute @workgroup_size(8, 8)
fn cs_aerial(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(aerial_out);
    if (any(id.xy >= size.xy)) {
        return;
    }
    let dir = view_dir(view_params((vec2<f32>(id.xy) + 0.5) / vec2<f32>(size.xy)));
    let r = eye_radius();
    let sun = sun_in_frame();
    let nu = dot(dir, sun);
    let phase_r = rayleigh_phase(nu);
    let phase_m = mie_phase(nu);
    var light = vec3<f32>(0.0);
    var transmittance = vec3<f32>(1.0);
    var t = 0.0;
    for (var k = 0u; k < size.z; k++) {
        let w = (f32(k) + 0.5) / f32(size.z);
        let end = w * w * sky.shape.y;
        let sub = 4;
        let span = (end - t) / f32(sub);
        for (var i = 0; i < sub; i++) {
            let tm = t + (f32(i) + 0.5) * span;
            let rr = sqrt(r * r + tm * tm + 2.0 * r * dir.y * tm);
            let mu_sun_here = clamp((r * sun.y + tm * nu) / rr, -1.0, 1.0);
            let m = medium(rr);
            let s = sunlight_at(rr, mu_sun_here) * (m.rayleigh * phase_r + m.mie * phase_m)
                + multiscatter(rr, mu_sun_here) * (m.rayleigh + m.mie);
            let step_t = exp(-m.extinction * span);
            light += transmittance * (s - s * step_t) / max(m.extinction, vec3<f32>(1e-6));
            transmittance *= step_t;
        }
        t = end;
        // The colour times the cover, which `sky_haze` divides by the cover again: so the slices blend as the light
        // did.
        let shown = dot(transmittance, vec3<f32>(1.0 / 3.0));
        let cover = max(1.0 - shown, 1e-3);
        let lit = light + moonlit(dir) * cover;
        textureStore(aerial_out, vec3<u32>(id.xy, k), vec4<f32>(sky_display(lit * sky.shape.z * shown_scale() / cover) * cover, shown));
    }
}

// The renderer's colours before `graded` are display colours; the tables hold linear light, and the ones the stage
// shader reads (the sky view shown, the aerial table, the sky cube) are brought to display colours here, once, rather
// than per pixel. (The twilight's colours fade by the rods' vision, `post.wgsl`.)
fn sky_display(linear: vec3<f32>) -> vec3<f32> {
    return pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
}

// The sky view's light along a stage-space direction (no disc), in table units, or the overcast sky's.
fn sky_light_table(dir: vec3<f32>) -> vec3<f32> {
    if (sky.shape.w != 0.0) {
        return overcast_light(dir);
    }
    let s = normalize(sky.to_sun.xyz);
    let flat_sun = normalize(vec2<f32>(s.x, s.z) + vec2<f32>(1e-6, 0.0));
    let flat = vec2<f32>(dir.x, dir.z);
    let azimuth = acos(clamp(dot(normalize(flat + vec2<f32>(1e-6, 0.0)), flat_sun), -1.0, 1.0));
    let elevation = asin(clamp(dir.y, -1.0, 1.0));
    let v = 0.5 + 0.5 * sign(elevation) * sqrt(abs(elevation) / (0.5 * PI));
    let clear = textureSampleLevel(skyview_lut, lut_sampler, vec2<f32>(azimuth / PI, v), 0.0).rgb;
    // Cloud cover greys and flattens the sky toward an overcast of the same light (CIE overcast: (1 + 2 up) / 3 of the
    // zenith); `light_out.scale.y` is the clear sky's up-facing irradiance.
    let cover = sky.level.y * 0.75;
    if (cover <= 0.0) {
        return clear;
    }
    return mix(clear, cie_overcast(dir, light_out.scale.y), cover);
}

// The CIE standard overcast sky (Moon and Spencer 1942) giving up-facing ground `e` (table units): `(1 + 2 up) / 3` of
// its zenith's luminance, grey, and below the horizon the ground it lights.
fn cie_overcast(dir: vec3<f32>, e: f32) -> vec3<f32> {
    let zenith = 9.0 * e / (7.0 * PI);
    return select(zenith * sky.ground.rgb * 7.0 / 9.0, vec3<f32>(zenith * (1.0 + 2.0 * max(dir.y, 0.0)) / 3.0), dir.y >= 0.0);
}

// The overcast sky (`Atmosphere::overcast`): the cloud deck lets through `OVERCAST_SHARE` of the clear sky's light on
// the ground (`light_out.scale.w`, `cs_light`), spread as the CIE overcast sky, grey; the eye's white balance
// (`day_light`) then takes it as it takes the clear sky's.
fn overcast_light(dir: vec3<f32>) -> vec3<f32> {
    return cie_overcast(dir, OVERCAST_SHARE * light_out.scale.w);
}

// The scale and the irradiance: one group of 64 threads, each projecting 16 directions (a Fibonacci sphere) of the sky
// onto the first nine spherical harmonics; summed, convolved with the cosine lobe.
var<workgroup> partial: array<array<vec4<f32>, 9>, 64>;
var<workgroup> clear_up: array<f32, 64>;
var<workgroup> whole: array<vec4<f32>, 9>;

// Thread `index`'s share of the sky's harmonics, above the horizon alone with `above`, into `partial`.
fn project(index: u32, above: bool) {
    var coefficients: array<vec4<f32>, 9>;
    for (var c = 0; c < 9; c++) {
        coefficients[c] = vec4<f32>(0.0);
    }
    for (var k = 0u; k < SAMPLES / 64u; k++) {
        let d = fibonacci(index * (SAMPLES / 64u) + k, SAMPLES);
        if (above && d.y <= 0.0) {
            continue;
        }
        let l = sky_light_table(d);
        var basis = sh_basis(d);
        for (var c = 0; c < 9; c++) {
            coefficients[c] += vec4<f32>(l * basis[c], 0.0);
        }
    }
    partial[index] = coefficients;
}

// The sum of `partial`, convolved with the cosine lobe (`cs_light`).
fn reduced() -> array<vec4<f32>, 9> {
    var sh: array<vec4<f32>, 9>;
    for (var c = 0; c < 9; c++) {
        sh[c] = vec4<f32>(0.0);
    }
    for (var i = 0u; i < 64u; i++) {
        for (var c = 0; c < 9; c++) {
            sh[c] += partial[i][c];
        }
    }
    // Radiance to irradiance: the cosine lobe's bands (pi, 2 pi / 3, pi / 4), over the samples' solid angle.
    var bands = array<f32, 9>(PI, 2.0 * PI / 3.0, 2.0 * PI / 3.0, 2.0 * PI / 3.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0, PI / 4.0);
    for (var c = 0; c < 9; c++) {
        sh[c] = sh[c] * bands[c] * 4.0 * PI / f32(SAMPLES);
    }
    return sh;
}

fn sh_basis(d: vec3<f32>) -> array<f32, 9> {
    return array<f32, 9>(
        0.282095,
        0.488603 * d.y,
        0.488603 * d.z,
        0.488603 * d.x,
        1.092548 * d.x * d.y,
        1.092548 * d.y * d.z,
        0.315392 * (3.0 * d.z * d.z - 1.0),
        1.092548 * d.x * d.z,
        0.546274 * (d.x * d.x - d.y * d.y),
    );
}

fn fibonacci(i: u32, n: u32) -> vec3<f32> {
    let y = 1.0 - 2.0 * (f32(i) + 0.5) / f32(n);
    let r = sqrt(max(1.0 - y * y, 0.0));
    let phi = f32(i) * 2.39996323;
    return vec3<f32>(r * cos(phi), y, r * sin(phi));
}

const SAMPLES: u32 = 1024u;

@compute @workgroup_size(64)
fn cs_light(@builtin(local_invocation_index) index: u32) {
    // First the clear sky's up-facing irradiance, which the cover's overcast needs.
    var up = 0.0;
    for (var k = 0u; k < SAMPLES / 64u; k++) {
        let d = fibonacci(index * (SAMPLES / 64u) + k, SAMPLES);
        if (d.y > 0.0) {
            let s = normalize(sky.to_sun.xyz);
            let flat_sun = normalize(vec2<f32>(s.x, s.z) + vec2<f32>(1e-6, 0.0));
            let azimuth = acos(clamp(dot(normalize(d.xz + vec2<f32>(1e-6, 0.0)), flat_sun), -1.0, 1.0));
            let v = 0.5 + 0.5 * sqrt(asin(d.y) / (0.5 * PI));
            let l = textureSampleLevel(skyview_lut, lut_sampler, vec2<f32>(azimuth / PI, v), 0.0).rgb;
            up += dot(l, vec3<f32>(0.2126, 0.7152, 0.0722)) * d.y;
        }
    }
    clear_up[index] = up * 4.0 * PI / f32(SAMPLES);
    workgroupBarrier();
    if (index == 0u) {
        var sum = 0.0;
        for (var i = 0u; i < 64u; i++) {
            sum += clear_up[i];
        }
        light_out.scale.y = sum;
        // The clear sky's light on up-facing ground with the sun's (or the moon's) square on times its sine, for the
        // overcast sky.
        var direct = transmittance_to_top(BOTTOM + sky.ground.w, max(sky.to_sun.y, 0.0)) * smoothstep(-0.005, 0.005, sky.to_sun.y);
        var up = sky.to_sun.y;
        if (up < 0.0) {
            direct = sky.moon.rgb / SUN_LUX;
            up = sky.moon.w;
        }
        light_out.scale.w = sum + dot(direct, LUMA) * up * sky.shape.z;
    }
    storageBarrier();
    workgroupBarrier();
    project(index, false);
    workgroupBarrier();
    if (index == 0u) {
        whole = reduced();
    }
    workgroupBarrier();
    project(index, true);
    workgroupBarrier();
    if (index != 0u) {
        return;
    }
    let sh = whole;
    let sky_sh = reduced();
    // The up-facing irradiance, to which the harmonics are made relative, and the scale that brings the sky's mean
    // light (irradiance / pi) to the renderer's level.
    var b = sh_basis(vec3<f32>(0.0, 1.0, 0.0));
    var e_up = vec3<f32>(0.0);
    for (var c = 0; c < 9; c++) {
        e_up += sh[c].rgb * b[c];
    }
    let up_luminance = max(dot(e_up, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-9);
    for (var c = 0; c < 9; c++) {
        light_out.sh[c] = sh[c] / up_luminance;
        light_out.sky_sh[c] = sky_sh[c] / up_luminance;
    }
    // The transmittance toward the sun at its true elevation, held at the horizon's once it sets.
    let sun = transmittance_to_top(BOTTOM + sky.ground.w, max(sky.to_sun.y, 0.0));
    var e_sky = vec3<f32>(0.0);
    for (var c = 0; c < 9; c++) {
        e_sky += sky_sh[c].rgb * b[c];
    }
    let day = day_light(e_sky, sun);
    light_out.day_sun = day[0];
    light_out.day_sky = day[1];
    light_out.sun = vec4<f32>(max(sun * light_out.balance.rgb, vec3<f32>(1e-4)), 0.0);
    // The sky drawn at the stage's gain (its light over pi is a white face's under it).
    light_out.scale = vec4<f32>(day[1].w * PI / sky.shape.z, light_out.scale.y, up_luminance, light_out.scale.w);
}

// The day's light on the stage (ours), from the tables, in which the sun's light above the air is 1 (`SUN_LUX`): the
// sun's square on (its transmittance, `sun`, as much of its disc as is up; none under an overcast sky) and the sky's
// above the horizon on up-facing ground (`e_sky`, in the tables' gain; under an overcast sky the deck's,
// `overcast_light`).
//
// The renderer's units are the light's luminance pre-exposed (as Unreal's pre-exposure): times `pre`, which draws the
// mean light on open faces turned anywhere above the horizon (the sky's on up-facing ground and a quarter of the sun's
// square on times 1 plus the sine of its elevation) at noon's level in the renderer, the open sunlit ground at
// `OPEN_LIGHT`. So the frame keeps its range in half floats at every hour, and within it the light keeps its physical
// shares; the exposure (`post.wgsl`) takes the luminance back (dividing by `pre`) and meters it in EV100. Through
// twilight the pre-exposure stops at the night's (`NIGHT_PRE_EXPOSURE`), which the moonlit night keeps.
//
// The eye's white balance (`balance`, the stage's and the sky's alike): the noon light's colour (`WHITE`) is white, and
// the eye goes the CIECAM02 degree of adaptation `D` of the way to the open ground's light's colour as it changes (for
// an adapting luminance of a fifth of the mean light's, an average surround: 0.82 at night to 1 in daylight), less of
// the way to a warm light (`WARM_RATIO`).
const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
// The sun's illuminance above the air, lux (Janiczek and DeYoung 1987).
const SUN_LUX: f32 = 133775.0;
// Under a clear sky with the sun 55 degrees up, in the tables' units: up-facing ground's light, and the mean light.
const NOON_OPEN: f32 = 0.75;
const NOON_MEAN: f32 = 0.45;
// The open sunlit ground's light at noon, renderer units: that of the retail bakes' median open ground (the sky 0.56 of
// its vertex light, the sun 0.79), linear, so that the stages' textures keep the brightness they were made for.
const OPEN_LIGHT: f32 = 1.93;
// The renderer's units per cd/m2 at night (`skylight::NIGHT_PRE_EXPOSURE`).
const NIGHT_PRE_EXPOSURE: f32 = 0.004 * PI / 0.25;
// A fully overcast sky's light on the ground as a share of the clear sky's (Kasten and Czeplak 1980).
const OVERCAST_SHARE: f32 = 0.25;
// The open ground's light's colour at noon (as `NOON_OPEN`).
const WHITE: vec3<f32> = vec3<f32>(1.06, 0.99, 0.92);
// The eye adapts less to warm light than to white (a lamp-lit room still looks warm), and the player's eye is adapted
// to the screen, so a low sun keeps some of its gold, as in a photograph (user's call, 2026-11-01): as the open
// ground's light's red over blue grows from `WHITE`'s to `WARM_RATIO.y` times it, `D` falls to `WARM_D` of itself (the
// sun 13 degrees up: 1.29 times, `D` 0.45). Neutral and bluer light (noon, overcast, twilight, night) keeps the full
// `D`.
const WARM_RATIO: vec2<f32> = vec2<f32>(1.0, 1.35);
const WARM_D: f32 = 0.4;

fn day_light(e_sky: vec3<f32>, sun: vec3<f32>) -> array<vec4<f32>, 2> {
    var sky_light = e_sky / sky.shape.z;
    var sun_light = sun * smoothstep(-0.005, 0.005, sky.to_sun.y);
    // Once the sun is down the direct light is the moon's (`sky.moon.w` the sine of its elevation).
    var up = sky.to_sun.y;
    if (up < 0.0) {
        sun_light = sky.moon.rgb / SUN_LUX;
        up = sky.moon.w;
    }
    if (sky.shape.w != 0.0) {
        sun_light = vec3<f32>(0.0);
    }
    let open_light = sky_light + sun_light * up;
    let open = max(dot(open_light, LUMA), 1e-9);
    let mean = max(dot(sky_light, LUMA) + dot(sun_light, LUMA) * (1.0 + up) * 0.25, 1e-9);
    let adapting = 0.2 * mean * SUN_LUX / PI;
    let warm = smoothstep(WARM_RATIO.x, WARM_RATIO.y, open_light.r / max(open_light.b, 1e-9) / (WHITE.r / WHITE.b));
    let d = (1.0 - exp((-adapting - 42.0) / 92.0) / 3.6) * mix(1.0, WARM_D, warm);
    var balance = pow(WHITE, vec3<f32>(d - 1.0)) * pow(max(open_light / open, vec3<f32>(1e-3)), vec3<f32>(-d));
    balance /= dot(open_light / open * balance, LUMA);
    light_out.balance = vec4<f32>(balance, 0.0);
    // The gain from the tables' units (irradiance) to the renderer's (a white face's pre-exposed luminance), and the
    // pre-exposure (renderer units per cd/m2).
    let pre = min(OPEN_LIGHT * NOON_MEAN / NOON_OPEN / mean * PI / SUN_LUX, NIGHT_PRE_EXPOSURE);
    let gain = pre * SUN_LUX / PI;
    return array<vec4<f32>, 2>(vec4<f32>(sun_light * balance * gain, pre), vec4<f32>(sky_light * balance * gain, gain));
}

// The tables' light to the renderer's (before `sky_display`): the gain and the white balance.
fn shown_scale() -> vec3<f32> {
    return light_out.scale.x * light_out.balance.rgb;
}

// The sky cube's level `cube_level`: each texel's direction (the cube's faces as the GPU samples them) and the sky
// around it, blurred more on rougher levels (cosine-weighted samples in a cone), in the renderer's display colour as
// `sky_reflected` reads it.
fn cube_dir(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let u = uv.x * 2.0 - 1.0;
    let v = uv.y * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3<f32>(1.0, -v, -u)); }
        case 1u: { return normalize(vec3<f32>(-1.0, -v, u)); }
        case 2u: { return normalize(vec3<f32>(u, 1.0, v)); }
        case 3u: { return normalize(vec3<f32>(u, -1.0, -v)); }
        case 4u: { return normalize(vec3<f32>(u, -v, 1.0)); }
        default: { return normalize(vec3<f32>(-u, -v, -1.0)); }
    }
}

@compute @workgroup_size(8, 8)
fn cs_cube(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = u32(cube_level.y);
    if (any(id.xy >= vec2<u32>(size))) {
        return;
    }
    let dir = cube_dir(id.z, (vec2<f32>(id.xy) + 0.5) / f32(size));
    let blur = cube_level.x;
    if (blur <= 0.0) {
        textureStore(cube_out, id.xy, id.z, vec4<f32>(sky_display(sky_light_table(dir) * shown_scale()), 1.0));
        return;
    }
    let side = normalize(cross(dir, select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(dir.y) > 0.9)));
    let other = cross(dir, side);
    // The cone's half angle grows to a hemisphere at the roughest level.
    let spread = tan(min(blur * blur * 1.5, 1.4));
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    let n = 48u;
    for (var i = 0u; i < n; i++) {
        let radius = sqrt((f32(i) + 0.5) / f32(n));
        let angle = f32(i) * 2.39996323;
        let offset = radius * spread * (cos(angle) * side + sin(angle) * other);
        let d = normalize(dir + offset);
        let w = 1.0 - radius * radius * 0.5;
        sum += sky_light_table(d) * w;
        weight += w;
    }
    textureStore(cube_out, id.xy, id.z, vec4<f32>(sky_display(sum / weight * shown_scale()), 1.0));
}
