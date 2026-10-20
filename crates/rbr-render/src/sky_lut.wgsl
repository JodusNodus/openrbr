// The physically based sky in the stage shader (renderer addition, `Enhance::sky`, ours), appended to `shader.wgsl`:
// the tables `skylight.wgsl` fills (`skylight.rs`) read back. The sky's light, the haze between the eye and the stage,
// the sunlight's colour, and the sky's irradiance and blurred reflection for the lit draws. By day under clear
// sections only (`frame.skylight.x`); under an overcast sky (`UseFog`) the sky keeps the fog's colour, and the
// irradiance and the cube follow it.

// True with `Enhance::sky` (a pipeline override, so that the frame without it is built as before).
override SKY_LUT: bool = false;

struct SkyTables {
    // The irradiance as nine spherical harmonics, relative to an up-facing surface's (luminance 1).
    sh: array<vec4<f32>, 9>,
    // x the scale from the tables to the renderer's linear light.
    scale: vec4<f32>,
}

@group(0) @binding(6) var sky_view_table: texture_2d<f32>;
@group(0) @binding(7) var sky_transmittance_table: texture_2d<f32>;
@group(0) @binding(8) var sky_aerial_table: texture_3d<f32>;
@group(0) @binding(9) var sky_cube: texture_cube<f32>;
@group(0) @binding(10) var<storage, read> sky_tables: SkyTables;

const SKY_PI: f32 = 3.14159265;
// The planet and the eye's height (`skylight.rs`, `skylight.wgsl`), km.
const SKY_BOTTOM: f32 = 6360.0;
const SKY_TOP: f32 = 6460.0;
const SKY_EYE: f32 = 0.05;
// The aerial table's reach, km (`skylight::AERIAL_REACH`).
const SKY_AERIAL_REACH: f32 = 12.0;
// The sky cube's blurriest level (`skylight::CUBE_LEVELS` - 1).
const SKY_CUBE_TOP: f32 = 5.0;
// The game's cloud faces' light under the physically based sky (`CLOUD_LIGHT` in `shader.wgsl` otherwise).
const SKY_CLOUD_LIGHT: f32 = 1.7;
// How far behind the stage the backdrop's foot lies, metres, whose haze the stage takes at the cells' reach.
const SKY_BACKDROP: f32 = 3000.0;

fn sky_on() -> bool {
    return SKY_LUT && frame.skylight.x != 0.0;
}

// The tables' coordinates of a stage-space direction: azimuth from the sun's, and the elevation squeezed toward the
// horizon (`view_params` in `skylight.wgsl`).
fn sky_table_uv(dir: vec3<f32>) -> vec2<f32> {
    let to_sun = -frame.sun.xyz;
    let flat_sun = normalize(to_sun.xz + vec2<f32>(1e-6, 0.0));
    let azimuth = acos(clamp(dot(normalize(dir.xz + vec2<f32>(1e-6, 0.0)), flat_sun), -1.0, 1.0));
    let elevation = asin(clamp(dir.y, -1.0, 1.0));
    return vec2<f32>(azimuth / SKY_PI, 0.5 + 0.5 * sign(elevation) * sqrt(abs(elevation) / (0.5 * SKY_PI)));
}

// How much of its colour the twilight sky keeps once the sun has set (`sky_display`): the Belt of Venus and the haze's
// pink and violet read garish next to a dark forest at full strength.
const SKY_TWILIGHT_SATURATION: f32 = 0.5;

// The renderer's colours before `graded` are display colours; the tables hold linear light. From the sun's setting
// (its true elevation 3 degrees up to 1 down) the colour goes to `SKY_TWILIGHT_SATURATION` of its saturation.
fn sky_display(linear: vec3<f32>) -> vec3<f32> {
    let c = pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    let twilight = 1.0 - smoothstep(-0.017, 0.052, frame.skylight.y);
    let grey = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(c, vec3<f32>(grey), twilight * (1.0 - SKY_TWILIGHT_SATURATION));
}

// The physically based sky's horizon and sun side are brighter than white, which the HDR pass rolls off; without it
// `graded` clips them, and the sky seen through the leaves shows as white holes. Without HDR the backdrop's colour
// rolls off above `SKY_KNEE` toward `SKY_WHITE` (its brightest channel, the hue kept), so it stays below white.
const SKY_KNEE: f32 = 0.8;
const SKY_WHITE: f32 = 0.95;

fn sky_shoulder(colour: vec3<f32>) -> vec3<f32> {
    let m = max(max(colour.r, colour.g), colour.b);
    if (m <= SKY_KNEE) {
        return colour;
    }
    let span = SKY_WHITE - SKY_KNEE;
    return colour * (SKY_KNEE + span * (1.0 - exp(-(m - SKY_KNEE) / span))) / m;
}

// The sky's light along `dir`, without the sun's disc (`sky_light` under `sky_on`).
fn sky_table_light(dir: vec3<f32>) -> vec3<f32> {
    let l = textureSampleLevel(sky_view_table, reflection_sampler, sky_table_uv(dir), 0.0).rgb;
    return sky_display(l * sky_tables.scale.x);
}

// The sunlight's colour at the eye (`sunlight` under `sky_on`): the transmittance toward the sun, at its true
// elevation, held at the horizon's once it sets.
fn sky_table_sunlight() -> vec3<f32> {
    let r = SKY_BOTTOM + SKY_EYE;
    let mu = max(frame.skylight.y, 0.0);
    let h = sqrt(SKY_TOP * SKY_TOP - SKY_BOTTOM * SKY_BOTTOM);
    let rho = sqrt(r * r - SKY_BOTTOM * SKY_BOTTOM);
    let d = -r * mu + sqrt(r * r * (mu * mu - 1.0) + SKY_TOP * SKY_TOP);
    let d_min = SKY_TOP - r;
    let uv = vec2<f32>((d - d_min) / (rho + h - d_min), rho / h);
    return max(textureSampleLevel(sky_transmittance_table, reflection_sampler, uv, 0.0).rgb, vec3<f32>(1e-4));
}

// The haze between the eye and a point `distance` metres along `dir`: its colour (display) times its cover, and how
// much of the point shows through.
fn sky_aerial(dir: vec3<f32>, distance: f32) -> vec4<f32> {
    let w = sqrt(clamp(distance * 0.001 / SKY_AERIAL_REACH, 0.0, 1.0));
    return textureSampleLevel(sky_aerial_table, reflection_sampler, vec3<f32>(sky_table_uv(dir), w), 0.0);
}

// `fogged`'s haze under `sky_on`: the cover `f` (at least `linear`, the section's) and the haze's colour (display),
// for the point at `world`. In the stage's frame (`frame.skylight.z`), past three quarters of the cells' reach
// (`frame.skylight.w`, `visibility::CELL_REACH`) the haze goes to the backdrop's at its foot, so the stage's edge
// meets the backdrop (or the sky) in the same haze.
fn sky_haze(world: vec3<f32>, linear: vec3<f32>) -> array<vec3<f32>, 2> {
    let d = distance(world, frame.eye.xyz);
    let dir = normalize(world - frame.eye.xyz);
    var a = sky_aerial(dir, d);
    if (frame.skylight.z != 0.0 && frame.skylight.w > 0.0) {
        let edge = smoothstep(0.75 * frame.skylight.w, frame.skylight.w, d);
        if (edge > 0.0) {
            a = mix(a, sky_aerial(normalize(vec3<f32>(dir.x, min(dir.y, 0.0), dir.z)), SKY_BACKDROP), edge);
        }
    }
    let cover = 1.0 - a.a;
    return array<vec3<f32>, 2>(max(vec3<f32>(cover), linear), a.rgb / max(cover, 1e-3));
}

// The sky's irradiance at normal `n` relative to an up-facing surface's (`sky_tables.sh`; luminance 1 facing up).
fn sky_irradiance(n: vec3<f32>) -> vec3<f32> {
    var b = array<f32, 9>(
        0.282095,
        0.488603 * n.y,
        0.488603 * n.z,
        0.488603 * n.x,
        1.092548 * n.x * n.y,
        1.092548 * n.y * n.z,
        0.315392 * (3.0 * n.z * n.z - 1.0),
        1.092548 * n.x * n.z,
        0.546274 * (n.x * n.x - n.y * n.y),
    );
    var e = vec3<f32>(0.0);
    for (var c = 0; c < 9; c++) {
        e += sky_tables.sh[c].rgb * b[c];
    }
    return max(e, vec3<f32>(0.0));
}

// The ambient's scale from the sky (`light`'s hemisphere term): the game's ambient keeps its level and colour (the
// section's, dimmed and tinted at dusk by the clock), and the sky gives how it changes with the direction: brighter
// facing up, warmer toward a low sun, bluer away from it, flatter under clouds. The colour is the irradiance's
// against the mean over all directions (the first harmonic; `IBL_TINT` of it), the brightness held between
// `IBL_RANGE` of the up-facing one.
const IBL_TINT: f32 = 0.5;
const IBL_RANGE: vec2<f32> = vec2<f32>(0.55, 1.05);

fn sky_ambient(n: vec3<f32>) -> vec3<f32> {
    let mean = max(sky_tables.sh[0].rgb, vec3<f32>(1e-6));
    let e = sky_irradiance(n) / mean * dot(mean, vec3<f32>(0.2126, 0.7152, 0.0722));
    let grey = dot(e, vec3<f32>(0.2126, 0.7152, 0.0722));
    let tinted = mix(vec3<f32>(grey), e, IBL_TINT);
    return tinted * clamp(grey, IBL_RANGE.x, IBL_RANGE.y) / max(grey, 1e-4);
}

// The sky reflected along `dir`, blurred by `roughness` (0 sharp to 1), from the sky cube (display colour).
fn sky_reflected(dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    let l = textureSampleLevel(sky_cube, reflection_sampler, dir, roughness * SKY_CUBE_TOP).rgb;
    return sky_display(l * sky_tables.scale.x);
}

// The height fog (ours, `height_fog.rs`): a mist whose density falls off exponentially with the height above the
// stage's low ground (`frame.height_fog`), integrated along the view ray, lit by the haze's sky light and the sun's
// in-scatter. In the stage's frame it thickens to the mist 3 km out (`SKY_BACKDROP`) between three quarters of the
// cells' reach and the reach, and the backdrop and the sky take that mist, so the stage's edge meets them in it.
// How much of the sky's light along the haze the mist takes, and how strongly it scatters the sun forward (the
// Henyey-Greenstein lobe's `MIST_G`, times the sky's brightness, so it dims with the stage at dusk).
const MIST_SKY: f32 = 0.55;
const MIST_SUN: f32 = 0.03;
const MIST_G: f32 = 0.6;

// How much of what lies `distance` metres along `dir` from the eye the mist covers.
fn height_fog_cover(dir: vec3<f32>, distance: f32) -> f32 {
    let f = frame.height_fog;
    // Heights over the scale height above the base at both ends; below the base the mist is no thicker than a little
    // under it.
    let h0 = max((frame.eye.y - f.z) / f.y, -2.0);
    let h1 = max((frame.eye.y + dir.y * distance - f.z) / f.y, -2.0);
    let mean = select((exp(-h0) - exp(-h1)) / (h1 - h0), exp(-0.5 * (h0 + h1)), abs(h1 - h0) < 1e-3);
    return 1.0 - exp(-f.x * distance * mean);
}

// The mist's light along `dir` (display colour), from the haze's light `sky` there: the mist scatters the sun forward
// in the haze's colour (warm toward a low sun), so the lobe needs no lookup of its own.
fn height_fog_light(dir: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    var scatter = MIST_SKY;
    if (frame.sun.w != 0.0 && frame.night.x == 0.0) {
        let mu = dot(dir, -frame.sun.xyz);
        let phase = (1.0 - MIST_G * MIST_G) / pow(1.0 + MIST_G * MIST_G - 2.0 * MIST_G * mu, 1.5);
        // Once the sun has set (its true elevation) it no longer lights the mist.
        let up = select(1.0, smoothstep(-0.02, 0.03, frame.skylight.y), sky_on());
        scatter = scatter + MIST_SUN * min(phase, 4.0) * up;
    }
    return sky * scatter;
}

fn height_mist(colour: vec3<f32>, dir: vec3<f32>, distance: f32, sky: vec3<f32>) -> vec3<f32> {
    return mix(colour, height_fog_light(dir, sky), height_fog_cover(dir, distance));
}

// `fogged`'s height fog over the hazed colour at `world`, lit by the haze's light `haze`.
fn height_fogged(colour: vec3<f32>, world: vec3<f32>, haze: vec3<f32>) -> vec3<f32> {
    let d = distance(world, frame.eye.xyz);
    let dir = (world - frame.eye.xyz) / max(d, 1e-3);
    var reach = d;
    if (frame.skylight.z == 0.0) {
        reach = SKY_BACKDROP;
    } else if (frame.skylight.w > 0.0) {
        reach = mix(d, SKY_BACKDROP, smoothstep(0.75 * frame.skylight.w, frame.skylight.w, d));
    }
    return height_mist(colour, dir, reach, haze);
}
