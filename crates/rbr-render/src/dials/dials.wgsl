// The dials' drawing (`gpu.rs`): the restore copies the base's texels at the level drawn, the sprites sample the atlas
// times their colour.

struct In {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) colour: vec4<f32>,
    @location(3) level: u32,
}

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) @interpolate(flat) level: u32,
}

@group(0) @binding(0) var base: texture_2d<f32>;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

@vertex
fn vs(v: In) -> Out {
    return Out(vec4<f32>(v.position, 0.0, 1.0), v.uv, v.colour, v.level);
}

@fragment
fn fs_restore(o: Out) -> @location(0) vec4<f32> {
    return textureLoad(base, vec2<i32>(o.position.xy), i32(o.level));
}

@fragment
fn fs_sprite(o: Out) -> @location(0) vec4<f32> {
    return textureSample(atlas, atlas_sampler, o.uv) * o.colour;
}
