// The screen-space reflections traced at half the frame's size (`fs_reflect` in `shader.wgsl`,
// `renderer::ScreenReflection`) laid over the frame: each pixel takes the 3x3 half-size texels about it by a tent
// `TENT` texels wide (wider than bilinear, so that the rays' per-texel noise does not show as blocks), each times how
// near its depth is to the pixel's (`DEPTH_MATCH` on the relative difference), not renormalised, so that a pixel whose
// texels lie on another surface (the car over the road, the verge beside it) takes none of their reflection. The
// texels hold the reflection premultiplied by its weight. Built with `renderer::depth_source`.

@group(0) @binding(0) var traced: texture_2d<f32>;
@group(0) @binding(1) var traced_depth: texture_2d<f32>;
@group(0) @binding(2) var frame_depth: texture_depth_2d;

const DEPTH_MATCH: f32 = 100.0;
const TENT: f32 = 1.5;

struct Out {
    @builtin(position) clip: vec4<f32>,
}

// One triangle over the frame.
@vertex
fn vs_composite(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0));
}

@fragment
fn fs_composite(v: Out) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(traced));
    let depth = textureLoad(frame_depth, vec2<i32>(v.clip.xy), 0);
    let p = v.clip.xy * 0.5 - 0.5;
    let centre = vec2<i32>(round(p));
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var k = 0; k < 9; k++) {
        let at = centre + vec2<i32>(k % 3 - 1, k / 3 - 1);
        let far = abs(vec2<f32>(at) - p);
        let tent = max(TENT - far.x, 0.0) * max(TENT - far.y, 0.0);
        let texel = clamp(at, vec2<i32>(0), size - 1);
        let d = textureLoad(traced_depth, texel, 0).r;
        let off = abs(d - depth) / max(max(d, depth), 1e-6);
        sum += textureLoad(traced, texel, 0) * tent * exp(-off * off * DEPTH_MATCH);
        total += tent;
    }
    return vec4<f32>(sum.rgb / max(sum.a, 1e-6), sum.a / total);
}
