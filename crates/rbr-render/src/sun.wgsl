// The sun's shadow cascades (renderer addition, `sun.rs`): stage draws drawn from the sun, depth only. Alpha-tested
// and alpha-to-coverage draws drop their cut-out texels at the alpha reference, so foliage casts its leaves, not its
// cards.

struct Cascade {
    matrix: mat4x4<f32>,
    // The wind (`Frame::wind` in `shader.wgsl`): x the time, y 1 when on.
    wind: vec4<f32>,
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
    @location(2) uv0: vec2<f32>,
    // x the plant's sway amplitude, y 1 on grass (`foliage.rs`).
    @location(6) specular: vec3<f32>,
}

// `sway` in `shader.wgsl`, kept in step, so that the shadows move with the leaves.
const WIND: vec2<f32> = vec2<f32>(0.92, 0.39);

fn sway(p: vec3<f32>, amplitude: f32, t: f32) -> vec3<f32> {
    let along = dot(p.xz, WIND);
    let gust = 0.5 + 0.5 * sin(0.7 * t - 0.08 * along);
    let swing = 0.55 * sin(1.6 * t + 0.31 * along) + 0.25 * sin(2.9 * t + 0.53 * p.x - 0.4 * p.z);
    let flutter = 0.12 * sin(9.0 * t + 1.7 * p.y + 0.9 * p.x);
    let push = amplitude * (0.35 + gust * (0.65 + swing) + flutter);
    return vec3<f32>(WIND.x * push, -0.15 * abs(push), WIND.y * push);
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv0: vec2<f32>,
}

@vertex
fn vs_sun(v: VertexIn) -> Out {
    var out: Out;
    // Grass casts no shadow: outside the clip volume.
    if (mode.coverage != 0u && v.specular.y != 0.0) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    var position = v.position;
    if (cascade.wind.y != 0.0 && mode.coverage != 0u) {
        position = position + sway(position, v.specular.x, cascade.wind.x);
    }
    out.clip = cascade.matrix * vec4<f32>(position, 1.0);
    // Casters further toward the sun than the near plane stay on it rather than being clipped (pancaking).
    out.clip.z = max(out.clip.z, 0.0);
    out.uv0 = v.uv0;
    return out;
}

@fragment
fn fs_sun(v: Out) {
    let cut = mode.alpha_test != 0u || mode.coverage != 0u;
    if (cut && textureSampleLevel(texture0, texture_sampler, v.uv0, 0.0).a <= mode.alpha_ref) {
        discard;
    }
}
