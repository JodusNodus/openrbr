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
    // The sunlight's colour at the eye (`sky_table_sunlight`).
    sun: vec4<f32>,
    // `sh` of the sky above the horizon alone.
    sky_sh: array<vec4<f32>, 9>,
}

@group(0) @binding(6) var sky_view_table: texture_2d<f32>;
@group(0) @binding(8) var sky_aerial_table: texture_3d<f32>;
@group(0) @binding(9) var sky_cube: texture_cube<f32>;
@group(0) @binding(10) var<storage, read> sky_tables: SkyTables;

const SKY_PI: f32 = 3.14159265;
// The aerial table's reach, km (`skylight::AERIAL_REACH`).
const SKY_AERIAL_REACH: f32 = 12.0;
// The sky cube's blurriest level (`skylight::CUBE_LEVELS` - 1).
const SKY_CUBE_TOP: f32 = 5.0;
// The game's cloud faces' light under the physically based sky (`CLOUD_LIGHT` in `shader.wgsl` otherwise).
const SKY_CLOUD_LIGHT: f32 = 1.7;
// How far the mist the sky takes reaches, metres: about the backdrop's foot under a clear sky; under fog all the way, so
// that the sky is what the farthest land fades into (`sky_mist_reach`).
const SKY_BACKDROP: f32 = 3000.0;
const SKY_FOG_REACH: f32 = 100000.0;

fn sky_mist_reach() -> f32 {
    return select(SKY_BACKDROP, SKY_FOG_REACH, frame.skylight.w != 0.0);
}

// The mist over the sky along `dir` (the sky dome, its clouds and the game's cloud faces), lit as the haze there.
fn sky_misted(colour: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    return height_mist(colour, dir, sky_mist_reach(), sky_light(normalize(vec3<f32>(dir.x, clamp(dir.y, 0.02, 0.3), dir.z))));
}

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

// The sky's light along `dir`, without the sun's disc (`sky_light` under `sky_on`).
fn sky_table_light(dir: vec3<f32>) -> vec3<f32> {
    return textureSampleLevel(sky_view_table, reflection_sampler, sky_table_uv(dir), 0.0).rgb;
}

// The sunlight's colour at the eye (`sunlight` under `sky_on`): the transmittance toward the sun, at its true
// elevation, held at the horizon's once it sets (`cs_light`, with the tables).
fn sky_table_sunlight() -> vec3<f32> {
    return sky_tables.sun.rgb;
}

// The haze between the eye and a point `distance` metres along `dir`: its colour (display) times its cover, and how
// much of the point shows through.
fn sky_aerial(dir: vec3<f32>, distance: f32) -> vec4<f32> {
    let w = sqrt(clamp(distance * 0.001 / SKY_AERIAL_REACH, 0.0, 1.0));
    return textureSampleLevel(sky_aerial_table, reflection_sampler, vec3<f32>(sky_table_uv(dir), w), 0.0);
}

// `fogged`'s haze under `sky_on`: the cover and the haze's colour (display) for the point at `world`, by its distance,
// the stage's and the backdrop's alike.
fn sky_haze(world: vec3<f32>) -> array<vec3<f32>, 2> {
    let a = sky_aerial(normalize(world - frame.eye.xyz), distance(world, frame.eye.xyz));
    let cover = 1.0 - a.a;
    return array<vec3<f32>, 2>(vec3<f32>(cover), a.rgb / max(cover, 1e-3));
}

// Where the haze takes a point at `world` to be. The backdrop lies farther and lower than the land it stands for (next
// to a strip on a ridge, its valley floor is a kilometre off), so the game scales its scattering depth by
// `Superbowl_Scale` (`frame.fog_range.w`, 0.2 in most sections; 1 for the stage). Here the scale holds next to the
// eye and eases to 1 by `SKY_BACKDROP_NEAR` metres, so that the backdrop is hazed next to the road as the stage there,
// and from there on more with the distance, as far as it is.
const SKY_BACKDROP_NEAR: f32 = 3000.0;

fn sky_seen(world: vec3<f32>) -> vec3<f32> {
    if (frame.fog_range.w == 1.0) {
        return world;
    }
    let d = distance(world, frame.eye.xyz);
    let seen = d * mix(frame.fog_range.w, 1.0, smoothstep(0.0, SKY_BACKDROP_NEAR, d));
    return frame.eye.xyz + (world - frame.eye.xyz) * (seen / max(d, 1e-3));
}

// The sky's irradiance at normal `n` relative to an up-facing surface's (`sky_tables.sh`; luminance 1 facing up).
fn sky_irradiance(n: vec3<f32>) -> vec3<f32> {
    return harmonics(sky_tables.sh, n);
}

// `sky_irradiance` from the sky above the horizon alone (`sky_tables.sky_sh`): the light below it is the ground's,
// which the stage's bounce gives (`bounce.rs`).
fn open_sky_irradiance(n: vec3<f32>) -> vec3<f32> {
    return harmonics(sky_tables.sky_sh, n);
}

fn harmonics(sh: array<vec4<f32>, 9>, n: vec3<f32>) -> vec3<f32> {
    let e = sh[0].rgb * 0.282095
        + sh[1].rgb * (0.488603 * n.y)
        + sh[2].rgb * (0.488603 * n.z)
        + sh[3].rgb * (0.488603 * n.x)
        + sh[4].rgb * (1.092548 * n.x * n.y)
        + sh[5].rgb * (1.092548 * n.y * n.z)
        + sh[6].rgb * (0.315392 * (3.0 * n.z * n.z - 1.0))
        + sh[7].rgb * (1.092548 * n.x * n.z)
        + sh[8].rgb * (0.546274 * (n.x * n.x - n.y * n.y));
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
    return textureSampleLevel(sky_cube, reflection_sampler, dir, roughness * SKY_CUBE_TOP).rgb;
}

// The height fog (ours, `height_fog.rs`): a mist whose density falls off exponentially with the height above the
// stage's low ground (`frame.height_fog`), integrated along the view ray, lit by the haze's sky light and the sun's
// in-scatter, by each point's distance; the sky takes the mist to `SKY_BACKDROP`.
// How much of the sky's light along the haze the mist takes: under a clear sky less than the bright horizon behind
// it, under fog (`skylight.w`) all of it, so that a deep mist is the fog's colour, as the sky (a darker mist showed as a
// grey band under the sky). And how strongly it scatters the sun forward (the Henyey-Greenstein lobe's `MIST_G`,
// times the sky's brightness, so it dims with the stage at dusk).
const MIST_SKY: vec2<f32> = vec2<f32>(0.55, 1.0);
const MIST_SUN: f32 = 0.03;
const MIST_G: f32 = 0.6;

// How much of what lies `distance` metres along `dir` from the eye the mist covers.
fn height_fog_cover(dir: vec3<f32>, distance: f32) -> f32 {
    return height_fog_cover_at(frame.eye.y + dir.y * distance, distance);
}

// How much the mist covers over a path `distance` metres long from the eye to the height `height`.
fn height_fog_cover_at(height: f32, distance: f32) -> f32 {
    let f = frame.height_fog;
    // Heights over the scale height above the base at both ends; below the base (a valley under the stage, seen from
    // above) the mist is as thick as at it: thicker, it turned the valley blue-grey right beside the stage.
    let h0 = max((frame.eye.y - f.z) / f.y, 0.0);
    let h1 = max((height - f.z) / f.y, 0.0);
    let e0 = f.w;
    let e1 = exp(-h1);
    let mean = select((e0 - e1) / (h1 - h0), 0.5 * (e0 + e1), abs(h1 - h0) < 1e-3);
    return 1.0 - exp(-f.x * distance * mean);
}

// The mist's light along `dir` (display colour), from the haze's light `sky` there: the mist scatters the sun forward
// in the haze's colour (warm toward a low sun), so the lobe needs no lookup of its own. Once the sun has set (its true
// elevation) the moon's lobe while it is up (`frame.sun` is then toward it, and the sky its moonlit sky).
fn height_fog_light(dir: vec3<f32>, sky: vec3<f32>) -> vec3<f32> {
    var scatter = select(MIST_SKY.x, MIST_SKY.y, frame.skylight.w != 0.0);
    if (frame.sun.w != 0.0 && (frame.night.x == 0.0 || sky_on())) {
        let mu = dot(dir, -frame.sun.xyz);
        let b = 1.0 + MIST_G * MIST_G - 2.0 * MIST_G * mu;
        let phase = (1.0 - MIST_G * MIST_G) / (b * sqrt(b));
        var up = 1.0;
        if (sky_on()) {
            up = select(smoothstep(-0.02, 0.03, frame.skylight.y), frame.moon_disc.w, moon_up());
        }
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
    return height_mist(colour, (world - frame.eye.xyz) / max(d, 1e-3), d, haze);
}
