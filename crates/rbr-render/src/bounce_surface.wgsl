// The stage's surfaces as the light they send on (`bounce.rs`, ours), drawn into a tile's maps from below: the
// texture's linear colour (a coarse mip) times the vertex's hue, per unit of the sun's light (its share of the sun,
// `sun` in `bounce.wgsl`) and of the sky's (the sky it sees), as `own_light` in `shader.wgsl` lights it. Cut-out
// texels drop as in `sun.wgsl`.

struct Cascade {
    matrix: mat4x4<f32>,
}

// The head of `Mode` in `shader.wgsl`.
struct Mode {
    second_texture: u32,
    alpha_test: u32,
    alpha_ref: f32,
    mip_bias: f32,
    coverage: u32,
}

@group(0) @binding(0) var<uniform> cascade: Cascade;
@group(1) @binding(0) var texture0: texture_2d<f32>;
@group(1) @binding(2) var texture_sampler: sampler;
@group(1) @binding(3) var<uniform> mode: Mode;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv0: vec2<f32>,
    // y 1 on grass (`foliage.rs`).
    @location(6) specular: vec3<f32>,
    @location(7) occlusion: f32,
    // x the vertex's share of the sun, as an f32's bits, while the bounce is drawn.
    @location(9) bounce: vec2<u32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv0: vec2<f32>,
    @location(1) sun: vec3<f32>,
    @location(2) sky: vec3<f32>,
}

struct Light {
    @location(0) sun: vec4<f32>,
    @location(1) sky: vec4<f32>,
}

// The mip the colour is read at: the texture's mean over a few texels' patch, as a metre of ground.
const COARSE_MIP: f32 = 3.0;

@vertex
fn vs_surface(v: VertexIn) -> Out {
    var out: Out;
    // Grass, outside the clip volume: the ground under it stands in.
    if (mode.coverage != 0u && v.specular.y != 0.0) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    out.clip = cascade.matrix * vec4<f32>(v.position, 1.0);
    out.clip.z = max(out.clip.z, 0.0);
    out.uv0 = v.uv0;
    // The hue of `own_light`, linear: the vertex colour over its brightest channel, white where it is black.
    let c = v.color.rgb;
    let most = max(c.r, max(c.g, c.b));
    let hue = pow(mix(vec3<f32>(1.0), c / max(most, 1e-3), smoothstep(0.02, 0.08, most)), vec3<f32>(2.2));
    out.sun = hue * bitcast<f32>(v.bounce.x);
    out.sky = hue * (1.0 - v.occlusion);
    return out;
}

@fragment
fn fs_surface(v: Out) -> Light {
    let cut = mode.alpha_test != 0u || mode.coverage != 0u;
    if (cut && textureSampleLevel(texture0, texture_sampler, v.uv0, 0.0).a <= mode.alpha_ref) {
        discard;
    }
    let albedo = pow(textureSampleLevel(texture0, texture_sampler, v.uv0, COARSE_MIP).rgb, vec3<f32>(2.2));
    return Light(vec4<f32>(albedo * v.sun, 1.0), vec4<f32>(albedo * v.sky, 1.0));
}
