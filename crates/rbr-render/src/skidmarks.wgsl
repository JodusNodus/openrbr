// The skid marks (`skidmarks_pass.rs`): textured, alpha-blended ground strips. The texture is the atlas cell of the
// record's texture index; v tiles along the trail inside the cell, u runs across the mark.

struct View {
    view_projection: mat4x4<f32>,
    // The light on the marks, whose colours are the game's for the lit ground (`SkidPass::draw`): 1 by day, the moon's
    // at night; and the car's lamps, which add theirs (`lamps.wgsl`, appended; none by day).
    light: vec4<f32>,
    lamps: array<Lamp, 8>,
}

@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var skid_texture: texture_2d<f32>;
@group(1) @binding(1) var skid_sampler: sampler;

// Set for the HDR scene (`Enhance::hdr`, renderer addition): the target holds linear colour, which the post pass
// tone-maps, so display colour is linearised as `graded` in `shader.wgsl` does.
override HDR: bool = false;

fn linear(colour: vec3<f32>) -> vec3<f32> {
    return select(colour, pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2)), HDR);
}

struct VertexIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) cell: vec4<f32>,
    @location(3) rgba: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) cell: vec4<f32>,
    @location(2) rgba: vec4<f32>,
    @location(3) world: vec3<f32>,
}

@vertex
fn vs_skid(v: VertexIn) -> VertexOut {
    return VertexOut(view.view_projection * vec4<f32>(v.pos, 1.0), v.uv, v.cell, v.rgba, v.pos);
}

// The light on the ground at `world` (facing up; the lamps without their shadow, the marks lying by the car).
fn lit(world: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let lamp = view.lamps[i];
        sum += lamp_toward(lamp, world) * lamp_facing(vec3<f32>(0.0, 1.0, 0.0), normalize(lamp.position.xyz - world), false);
    }
    return view.light.rgb + lamp_knee(sum);
}

@fragment
fn fs_skid(v: VertexOut) -> @location(0) vec4<f32> {
    let across = clamp(v.uv.x, 0.0, 1.0);
    let along = fract(v.uv.y);
    let texel = textureSampleLevel(skid_texture, skid_sampler, v.cell.xy + vec2<f32>(across, along) * v.cell.zw, 0.0);
    return vec4<f32>(linear(v.rgba.rgb * texel.rgb) * lit(v.world), v.rgba.a * texel.a);
}

// The car's shadow decal (`car_shadow.rs`): the whole texture, which the sampler clamps at its clear edges, so a
// ground face larger than the decal's rectangle shows it once.
@fragment
fn fs_decal(v: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSampleLevel(skid_texture, skid_sampler, v.uv, 0.0);
    return vec4<f32>(linear(v.rgba.rgb * texel.rgb) * lit(v.world), v.rgba.a * texel.a);
}
