// Particle billboards (`particles/gpu.rs`): one instance per particle, a quad from its centre and two half axes,
// textured from the sprite array, lit by the stage's ambient and sun, hazed like the stage, blended premultiplied so
// that alpha-blended and additive particles share one sorted draw. The water waves (`vs_wave`) are triangles with
// their own texture coordinates, drawn flat before the billboards.

struct Frame {
    view_projection: mat4x4<f32>,
    eye: vec4<f32>,
    // rgb, and the most the haze covers.
    fog_color: vec4<f32>,
    // start, end.
    fog_range: vec4<f32>,
    // The light on a particle: rgb.
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;

// Set for the HDR scene (`Enhance::hdr`, renderer addition): the target holds linear colour, which the post pass
// tone-maps, so display colour is linearised as `graded` in `shader.wgsl` does.
override HDR: bool = false;

fn linear(colour: vec3<f32>) -> vec3<f32> {
    return select(colour, pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2)), HDR);
}

// The display brightness of the background `blended_alpha` assumes.
const BACKGROUND: f32 = 0.25;

// The particles' alpha was made for blending in display colour; blended in linear colour a pale streak over a darker
// background shows about three times as strongly, and its soft edges grow. The alpha that changes a `BACKGROUND`
// pixel as blending `rgb` (display) at `alpha` in display colour would, for small alpha.
fn blended_alpha(rgb: vec3<f32>, alpha: f32) -> f32 {
    if (!HDR) {
        return alpha;
    }
    let y = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
    let b = BACKGROUND;
    if (abs(y - b) < 1e-3) {
        return alpha;
    }
    let k = 2.2 * pow(b, 1.2) * (y - b) / (pow(max(y, 0.0), 2.2) - pow(b, 2.2));
    return min(alpha * clamp(k, 0.0, 2.0), 1.0);
}
@group(0) @binding(1) var sprites: texture_2d_array<f32>;
@group(0) @binding(2) var sprite_sampler: sampler;

struct Instance {
    @location(0) centre: vec3<f32>,
    @location(1) layer: f32,
    @location(2) axis_u: vec3<f32>,
    @location(3) additive: f32,
    @location(4) axis_v: vec3<f32>,
    @location(5) color: vec4<f32>,
};

struct WaveVertex {
    @location(0) position: vec3<f32>,
    @location(1) colour_uv: vec2<f32>,
    @location(2) alpha_uv: vec2<f32>,
    @location(3) colour: vec4<f32>,
    @location(4) layer: f32,
};

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: i32,
    @location(2) color: vec4<f32>,
    @location(3) additive: f32,
    @location(4) haze: f32,
    // For `fs_soft`: the stage point, the quad's unit axes, its larger half size and its centre.
    @location(5) world: vec3<f32>,
    @location(6) @interpolate(flat) across: vec3<f32>,
    @location(7) @interpolate(flat) up: vec3<f32>,
    @location(8) @interpolate(flat) radius: f32,
    @location(9) @interpolate(flat) centre: vec3<f32>,
    // The water waves' alpha coordinates (`vs_wave`).
    @location(10) alpha_uv: vec2<f32>,
};

const TONE_CURVE: f32 = 1.5;
const TOE: f32 = 1.1;
const SATURATION: f32 = 1.08;

// The renderer's tone curve (`graded` in `shader.wgsl`), so particles match the frame around them.
fn graded(colour: vec3<f32>) -> vec3<f32> {
    let c = pow((1.0 - exp(-TONE_CURVE * max(colour, vec3<f32>(0.0)))) / (1.0 - exp(-TONE_CURVE)), vec3<f32>(TOE));
    let grey = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return clamp(mix(vec3<f32>(grey), c, SATURATION), vec3<f32>(0.0), vec3<f32>(1.0));
}

// The haze of a point at `world`, as the stage's fog hazes it.
fn haze_at(world: vec3<f32>) -> f32 {
    let distance = length(world - frame.eye.xyz);
    let range = max(frame.fog_range.y - frame.fog_range.x, 1.0);
    return clamp((distance - frame.fog_range.x) / range, 0.0, 1.0) * frame.fog_color.w;
}

@vertex
fn vs_main(@builtin(vertex_index) corner: u32, instance: Instance) -> Out {
    let x = f32(corner & 1u) * 2.0 - 1.0;
    let y = f32((corner >> 1u) & 1u) * 2.0 - 1.0;
    let world = instance.centre + instance.axis_u * x + instance.axis_v * y;
    var out: Out;
    out.position = frame.view_projection * vec4<f32>(world, 1.0);
    out.uv = vec2<f32>(x * 0.5 + 0.5, 0.5 - y * 0.5);
    out.layer = i32(instance.layer);
    out.color = instance.color;
    out.additive = instance.additive;
    out.haze = haze_at(world);
    out.world = world;
    out.across = normalize(instance.axis_u);
    out.up = normalize(instance.axis_v);
    out.radius = max(length(instance.axis_u), length(instance.axis_v));
    out.centre = instance.centre;
    return out;
}

@vertex
fn vs_wave(v: WaveVertex) -> Out {
    var out: Out;
    out.position = frame.view_projection * vec4<f32>(v.position, 1.0);
    out.uv = v.colour_uv;
    out.alpha_uv = v.alpha_uv;
    out.layer = i32(v.layer);
    out.color = v.colour;
    out.haze = haze_at(v.position);
    out.world = v.position;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return flat_colour(in, textureSample(sprites, sprite_sampler, in.uv, in.layer));
}

// The wave's colour and alpha are two coordinates into one layer (`gpu::Sprites::add`): the colour's rgb, the alpha's
// alpha. Both wrap, as the texture does; a sheet's length rarely runs past one.
@fragment
fn fs_wave(in: Out) -> @location(0) vec4<f32> {
    let colour = textureSample(sprites, sprite_sampler, fract(in.uv), in.layer).rgb;
    let alpha = textureSample(sprites, sprite_sampler, fract(in.alpha_uv), in.layer).a;
    return flat_colour(in, vec4<f32>(colour, alpha));
}

fn flat_colour(in: Out, texel: vec4<f32>) -> vec4<f32> {
    let alpha = texel.a * in.color.a;
    var rgb = graded(texel.rgb * in.color.rgb * frame.light.rgb);
    if (in.additive > 0.5) {
        // Additive light fades with the haze rather than turning fog-coloured.
        return vec4<f32>(linear(rgb) * alpha * (1.0 - in.haze), 0.0);
    }
    rgb = mix(rgb, frame.fog_color.rgb, in.haze);
    let blended = blended_alpha(rgb, alpha);
    return vec4<f32>(linear(rgb) * blended, blended);
}

// Particles lit and soft (renderer addition, `effects.rs`, `Enhance::particles`; not the game's look). Each sprite
// is shaded as a soft ball: a normal from its uv; the sky's light from above and the ground's darker bounce from
// below (`BOUNCE`); the sun wrapped round (`WRAP`), shaded by its cascades (dust in a tree's shadow is darker), dimmed
// through the ball's dense middle (`SELF_SHADOW`) and scattered forward toward the eye where it is thin (`SCATTER`,
// dust and spray glowing against the sun). Its edge is eaten by noise fixed to the particle (`EROSION`), so sprites
// from one texture differ, and it fades out over `SOFT` of its size where it nears the stage behind it, instead of
// cutting a hard line into the road.
const WRAP: f32 = 0.25;
const DIFFUSE: f32 = 0.75;
const SCATTER: f32 = 0.2;
const FORWARD: f32 = 0.5;
const BOUNCE: f32 = 0.45;
const SELF_SHADOW: f32 = 0.45;
const EROSION: f32 = 0.55;
const SOFT: f32 = 0.6;
const WISP_EROSION: f32 = 0.08;
const NEGLIGIBLE: f32 = 0.001;
// The share of the lamps' glow a wisp takes: its great size would hold the tail lamps' red as a blot.
const WISP_LAMPS: f32 = 0.2;

fn soft_hash(p: vec3<f32>) -> f32 {
    return fract(sin(dot(p, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453);
}

fn soft_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(soft_hash(i), soft_hash(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
        mix(soft_hash(i + vec3<f32>(0.0, 1.0, 0.0)), soft_hash(i + vec3<f32>(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(soft_hash(i + vec3<f32>(0.0, 0.0, 1.0)), soft_hash(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
        mix(soft_hash(i + vec3<f32>(0.0, 1.0, 1.0)), soft_hash(i + vec3<f32>(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

@fragment
fn fs_soft(in: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(sprites, sprite_sampler, in.uv, in.layer);
    // Under a thousandth the fragment changes no pixel: its corners and the sprite's clear rim skip the rest.
    if (texel.a * in.color.a < NEGLIGIBLE) {
        discard;
    }
    let depth = clamp(SOFT * in.radius, 0.05, 1.5);
    let gap = scene_distance(in.position) - distance(in.world, effects.eye.xyz);
    // Noise in the sprite's plane, seeded by where the particle is, eating more toward the edge.
    let seed = floor(in.centre * 3.0);
    let grain = 0.65 * soft_noise(vec3<f32>(in.uv * 3.0, 0.0) + seed) + 0.35 * soft_noise(vec3<f32>(in.uv * 7.0, 5.0) + seed);
    let density = texel.a * in.color.a;
    // A wisp (the wheel spray's mist, `additive` -1) is faint by design: the erosion would eat all of it, so it keeps
    // a little, and it fades away from the eye by its size instead of filling a view from inside.
    let wisp = in.additive < -0.5;
    let erosion = select(EROSION, WISP_EROSION, wisp);
    let eaten = clamp((density - erosion * (1.0 - grain) * (1.0 - density)) / max(density, 1e-3), 0.0, 1.0);
    let near = select(1.0, smoothstep(in.radius, 3.0 * in.radius, distance(in.centre, effects.eye.xyz)), wisp);
    let alpha = density * eaten * near * clamp(gap / depth, 0.0, 1.0);
    if (alpha < NEGLIGIBLE) {
        discard;
    }
    if (in.additive > 0.5) {
        let rgb = graded(texel.rgb * in.color.rgb * frame.light.rgb);
        return vec4<f32>(linear(rgb) * alpha * (1.0 - in.haze), 0.0);
    }
    let p = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let facing = sqrt(max(1.0 - dot(p, p), 0.0));
    let toward = normalize(effects.eye.xyz - in.world);
    let n = normalize(in.across * p.x + in.up * p.y + toward * (facing + 0.3));
    let to_sun = -effects.sun.xyz;
    let wrapped = clamp((dot(n, to_sun) + WRAP) / (1.0 + WRAP), 0.0, 1.0);
    let thin = 1.0 - clamp(density, 0.0, 1.0);
    let scatter = SCATTER * phase(dot(effects.sun.xyz, toward), FORWARD) * mix(0.4, 1.0, thin);
    let through = mix(1.0, 1.0 - SELF_SHADOW, clamp(density * facing, 0.0, 1.0));
    let sky = mix(1.0 - BOUNCE, 1.0, n.y * 0.5 + 0.5);
    let light = effects.ambient.rgb * sky
        + effects.sun_colour.rgb * sun_reach(in.world) * (DIFFUSE * wrapped * through + scatter)
        + lamp_glow(lamps_at(in.world)) * select(1.0, WISP_LAMPS, wisp);
    var rgb = graded(texel.rgb * in.color.rgb * light);
    rgb = mix(rgb, frame.fog_color.rgb, in.haze);
    let blended = blended_alpha(rgb, alpha);
    return vec4<f32>(linear(rgb) * blended, blended);
}
