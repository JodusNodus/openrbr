// The sun's shadow cascades (renderer addition, `sun.rs`): stage draws drawn from the sun, depth only. The alpha-tested draws
// drop their cut-out texels at the alpha reference, so foliage casts its leaves, not its cards.

struct Cascade {
    matrix: mat4x4<f32>,
    // How far the backdrop's draws (instance 1) are moved, world metres (`sun::backdrop_drop`).
    drop: vec4<f32>,
}

// The head of `Mode` in `shader.wgsl`.
struct Mode {
    second_texture: u32,
    alpha_test: u32,
    alpha_ref: f32,
    mip_bias: f32,
    cutout: u32,
}

@group(0) @binding(0) var<uniform> cascade: Cascade;
@group(1) @binding(0) var texture0: texture_2d<f32>;
@group(1) @binding(2) var texture_sampler: sampler;
@group(1) @binding(3) var<uniform> mode: Mode;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(2) uv0: vec2<f32>,
    // y 1 on grass (`foliage.rs`).
    @location(6) specular: vec3<f32>,
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv0: vec2<f32>,
}

@vertex
fn vs_sun(v: VertexIn, @builtin(instance_index) instance: u32) -> Out {
    var out: Out;
    // Grass casts no shadow: outside the clip volume.
    if (mode.cutout != 0u && v.specular.y != 0.0) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0);
        return out;
    }
    let backdrop = instance == 1u;
    out.clip = cascade.matrix * vec4<f32>(v.position + select(vec3<f32>(0.0), cascade.drop.xyz, backdrop), 1.0);
    // Casters further toward the sun than the near plane stay on it rather than being clipped (pancaking), and the
    // backdrop's, moved deeper, further than the far plane on it.
    out.clip.z = max(out.clip.z, 0.0);
    if (backdrop) {
        out.clip.z = min(out.clip.z, out.clip.w);
    }
    out.uv0 = v.uv0;
    return out;
}

@fragment
fn fs_sun(v: Out) {
    let cut = mode.alpha_test != 0u || mode.cutout != 0u;
    if (cut && textureSampleLevel(texture0, texture_sampler, v.uv0, 0.0).a <= mode.alpha_ref) {
        discard;
    }
}
