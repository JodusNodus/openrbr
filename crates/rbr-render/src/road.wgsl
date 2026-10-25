// The roads (renderer additions over the game's specular map, ours): the shine and the specular light off the road's
// surface, the wet film and the rain's rings, in the stage's module (`shade_stage`).

// The roads' specular reflection with the day's light (`own_light`, ours): a dielectric (F0 0.04) of the road's
// roughness, the sun's highlight (GGX, Smith's height-correlated visibility, Schlick's Fresnel) past its
// shadows and the sky's reflection (the sky's cube at that roughness, Karis's analytic environment BRDF), as much as
// the point sees the sky (Lagarde's specular occlusion); the diffuse light keeps what is not reflected. Only the roads
// (the draws with a specular map): `ROAD_ROUGHNESS`, smoother where the map is bright (polished stones) and when
// damp. The rest of the ground, rock and grass (roughness about 0.95) reflect a few percent, close to diffuse, and
// cost 0.5 ms a frame for it; the cut-out draws' normals are the crown's or the ground's, not the leaves'.
const ROAD_ROUGHNESS: f32 = 0.75;

const ROAD_POLISH: f32 = 0.15;

const DAMP_ROUGHNESS: f32 = 0.15;

fn with_specular(colour: vec3<f32>, v: VertexOut, front: bool, sun_seen: f32) -> vec3<f32> {
    if (dot(v.normal, v.normal) < 1e-8) {
        return colour;
    }
    let n = normalize(v.normal) * select(-1.0, 1.0, front);
    // The maps' texels run about 0 to 0.25.
    let map = dot(textureSample(specular_texture, texture_sampler, v.specular.xy).rgb, vec3<f32>(0.333));
    let polish = clamp(map * 4.0 * v.specular.z, 0.0, 1.0);
    let roughness = ROAD_ROUGHNESS - ROAD_POLISH * polish - DAMP_ROUGHNESS * min(frame.wetness.x * 2.0, 1.0);
    let view = normalize(frame.eye.xyz - v.world);
    let nv = max(dot(n, view), 1e-3);
    // Karis's fit of the split-sum environment BRDF for F0 0.04.
    let r = roughness * vec4<f32>(-1.0, -0.0275, -0.572, 0.022) + vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    let reflectance = 0.04 * ab.x + ab.y;
    let seen = 1.0 - v.sky.z;
    let occlusion = clamp(pow(nv + seen, exp2(-16.0 * roughness - 1.0)) - 1.0 + seen, 0.0, 1.0);
    let sky = pow(sky_reflected(reflect(-view, n), roughness), vec3<f32>(2.2)) * occlusion;
    // The sun's highlight.
    let to_sun = -frame.sun.xyz;
    let nl = dot(n, to_sun);
    var sun = vec3<f32>(0.0);
    if (nl > 0.0 && sun_seen > 0.0) {
        let h = normalize(to_sun + view);
        let nh = max(dot(n, h), 0.0);
        let a2 = pow(roughness, 4.0);
        let d = a2 / (SKY_PI * pow(nh * nh * (a2 - 1.0) + 1.0, 2.0));
        let a = roughness * roughness;
        let vis = 0.5 / (nl * (nv * (1.0 - a) + a) + nv * (nl * (1.0 - a) + a));
        let f = 0.04 + 0.96 * pow(1.0 - max(dot(view, h), 0.0), 5.0);
        sun = frame.day_sun.rgb * SKY_PI * d * vis * f * nl * sun_seen;
    }
    let linear = pow(colour, vec3<f32>(2.2)) * (1.0 - reflectance) + sky * reflectance + sun;
    return pow(linear, vec3<f32>(1.0 / 2.2));
}

// The stage's specular highlight (slots 6 and 8, `notes/formats.md` "Shaders and vertex layouts"): `4 * sat(t2.rgb * S * strength)` with the specular
// map `t2`, the vertex's strength, and the game's highlight `S = scale * 2^(sharpness * (1 - 1 / (n.h)^2)) / (n.h)^4`
// from the stored normal and the half vector between the sun and the eye. The game's `sharpness` and `scale` (`c90`)
// have no traced source: `SHINE_SHARPNESS` and `SHINE_SCALE` are ours. So is `WEATHER_SHINE`, how much the highlight
// is scaled when dry, damp and wet (the game's specular maps are the same in every weather; on wet roads the water
// film, `wet_film`, smooths the glitter out and does the shining).
const SHINE_SHARPNESS: f32 = 8.0;

const SHINE_SCALE: f32 = 0.5;

const WEATHER_SHINE: vec3<f32> = vec3<f32>(1.125, 1.5, 0.3);

// The direct light square on with the sun 37.5 degrees up, renderer units (`skylight::NOON_SUN`).
const NOON_DIRECT: f32 = 2.532;

fn road_shine(v: VertexOut) -> vec3<f32> {
    let map = textureSample(specular_texture, texture_sampler, v.specular.xy).rgb;
    if (dot(v.normal, v.normal) < 1e-8) {
        return vec3<f32>(0.0);
    }
    let n = normalize(v.normal);
    let h = normalize(normalize(frame.eye.xyz - v.world) - frame.sun.xyz);
    let nh = max(dot(n, h), 0.05);
    let nh2 = nh * nh;
    let weather = select(select(WEATHER_SHINE.x, WEATHER_SHINE.y, frame.wetness.x >= 0.5), WEATHER_SHINE.z, is_wet());
    let s = SHINE_SCALE * exp2(SHINE_SHARPNESS * (1.0 - 1.0 / nh2)) / (nh2 * nh2) * weather;
    return 4.0 * clamp(map * s * v.specular.z, vec3<f32>(0.0), vec3<f32>(1.0));
}

// How much a wet road reflects (stylised, well above water's Fresnel, so wet roads read as in games and photos of
// rain): `WET_REFLECT` at a steep view between puddles and in them, rising toward `WET_MOST` at a grazing view, so
// the road's texture still shows. Wet asphalt is
// also darker and deeper in colour (`WET_DARKEN`, `WET_SATURATION`): water fills its pores.
const WET_REFLECT: vec2<f32> = vec2<f32>(0.06, 0.35);

const WET_MOST: f32 = 0.75;

const WET_DARKEN: f32 = 0.7;

const WET_SATURATION: f32 = 1.2;

// Where water stands (0 to 1), by stage position so it does not follow the texture's stones: a thin film everywhere
// (`FILM`) and smooth patches of standing water from about 0.7 to 3 m across (`puddle` and a finer octave). Scaled by
// the game's specular map (`SPEC_WET`, its brightness, blurred `SPEC_BLUR` mip levels; not the vertex strength, which
// is low on whole stages), so the film fades out where the map does, toward the road's borders, instead of stopping at the edge of the road's faces.
const FILM: f32 = 0.35;

const SPEC_WET: vec2<f32> = vec2<f32>(0.02, 0.12);

const SPEC_BLUR: f32 = 3.0;

fn wet_mask(v: VertexOut) -> f32 {
    let patches = 0.65 * value_noise(v.world / PUDDLES) + 0.35 * value_noise(v.world / 0.7 + vec3<f32>(17.0));
    let map = textureSampleBias(specular_texture, texture_sampler, v.specular.xy, SPEC_BLUR).rgb;
    let shines = smoothstep(SPEC_WET.x, SPEC_WET.y, dot(map, vec3<f32>(0.299, 0.587, 0.114)));
    return mix(FILM, 1.0, smoothstep(0.45, 0.7, patches)) * shines;
}

fn wet_reflectance(n: vec3<f32>, view: vec3<f32>, v: VertexOut) -> f32 {
    let wet = wet_mask(v);
    let base = mix(WET_REFLECT.x, WET_REFLECT.y, wet);
    return (base + (WET_MOST - base) * pow(1.0 - clamp(dot(n, view), 0.0, 1.0), 5.0)) * wet;
}

// Rain landing on standing water (renderer addition, `Enhance::particles`; not the game's look): rings spreading from
// where drops land, about `LANDING_RATE` a second in each `LANDING_CELL` square at full rain (fewer in lighter rain),
// tilting the water's normal by `standing` (0 on the thin film, where drops leave no rings; 1 on puddles and water) so
// its reflections shiver. Two offset grids, each ring tested in its cell's neighbours; gone where the distance over
// the view's cosine passes `LANDING_FAR`, as a pixel then spans a ring and they would only flicker.
const LANDING_CELL: f32 = 0.3;

const LANDING_RATE: f32 = 1.2;

const LANDING_REACH: f32 = 0.16;

const LANDING_WIDTH: f32 = 0.02;

const LANDING_TILT: f32 = 0.25;

const LANDING_FAR: f32 = 60.0;

fn landing_hash(c: vec2<f32>) -> vec3<f32> {
    let q = vec3<f32>(dot(c, vec2<f32>(127.1, 311.7)), dot(c, vec2<f32>(269.5, 183.3)), dot(c, vec2<f32>(419.2, 371.9)));
    return fract(sin(q) * 43758.5453);
}

fn rain_rings(n: vec3<f32>, world: vec3<f32>, standing: f32) -> vec3<f32> {
    let rain = frame.wind.z;
    let toward = frame.eye.xyz - world;
    let stretched = length(toward) / max(dot(n, normalize(toward)), 0.05);
    let fade = (1.0 - smoothstep(0.4 * LANDING_FAR, LANDING_FAR, stretched)) * standing;
    if (rain <= 0.0 || fade <= 0.0) {
        return n;
    }
    var tilt = vec2<f32>(0.0);
    for (var layer = 0; layer < 2; layer++) {
        let p = world.xz / LANDING_CELL + vec2<f32>(0.5 * f32(layer));
        let cell = floor(p);
        for (var k = 0; k < 9; k++) {
            let c = cell + vec2<f32>(f32(k % 3 - 1), f32(k / 3 - 1));
            let h = landing_hash(c + vec2<f32>(37.0 * f32(layer)));
            if (h.z > rain) {
                continue;
            }
            let age = fract(frame.wind.x * LANDING_RATE * mix(0.7, 1.3, h.y) + h.x);
            let d = (p - c - h.xy) * LANDING_CELL;
            let r = length(d);
            let x = (r - age * LANDING_REACH) / LANDING_WIDTH;
            if (abs(x) < 1.0) {
                tilt += d / max(r, 1e-4) * sin(x * 3.14159265) * (1.0 - age) * (1.0 - age);
            }
        }
    }
    return normalize(n + vec3<f32>(tilt.x, 0.0, tilt.y) * LANDING_TILT * fade);
}

fn wet_film(colour: vec3<f32>, v: VertexOut) -> vec3<f32> {
    if (dot(v.normal, v.normal) < 1e-8) {
        return colour;
    }
    let n = rain_rings(normalize(v.normal), v.world, standing(wet_mask(v)));
    let view = normalize(frame.eye.xyz - v.world);
    // Soaked like the ground (`GROUND_SOAK`) where the map fades, fully where water stands.
    let soaked = soak(colour, mix(GROUND_SOAK, 1.0, clamp(wet_mask(v) * 2.0, 0.0, 1.0)));
    let mirrored = reflect(-view, n);
    let daylight = frame.ambient.rgb + frame.diffuse.rgb * max(-frame.sun.y, 0.0);
    let f = wet_reflectance(n, view, v);
    // The stage around the car (its reflection cube, a little blurred) where it is drawn, else the sky.
    var around = mirrored_light(mirrored, daylight);
    if (frame.reflection.x != 0.0) {
        around = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, mirrored, 2.0).rgb);
    }
    var shine = glint(mirrored);
    if (is_night()) {
        shine = lamp_glint(v.world, mirrored, f);
    }
    return mix(soaked, around, f) + shine * wet_mask(v);
}
