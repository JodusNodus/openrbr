// The cockpit's mirrors (renderer addition, ours: `crate::mirrors`, `renderer/mirrors.rs`), appended to `shader.wgsl`.
// A mirror's glass shows the image of its own pass: the scene drawn from the eye reflected across the glass, cropped
// to the glass's box on screen. A glass vertex holds 1 plus its mirror's number in `specular.x`
// (`mirrors::mark`); the glass samples the image where its own point lies in the image's view.

// True with mirror passes (`Enhance::mirrors` not off; a pipeline override, so that the frame without them is built as
// before).
override MIRRORS: bool = false;

// `mirrors::MOST_MIRRORS`.
const MIRROR_COUNT: u32 = 4u;
// The share of the light a silvered glass gives back.
const MIRROR_SILVER: f32 = 0.85;

struct MirrorView {
    // Stage space to the image's clip space, as drawn (moved with the car since).
    view_projection: mat4x4<f32>,
    // xy the share of the image layer it fills, z 1 when it holds an image.
    scale: vec4<f32>,
}

struct MirrorViews {
    views: array<MirrorView, 4>,
}

@group(0) @binding(12) var mirror_images: texture_2d_array<f32>;
@group(0) @binding(13) var<uniform> mirror_views: MirrorViews;

// The number of the mirror whose glass `v` is, when it holds an image (`frame.night.z` is 1 in the mirrors' own
// passes, whose glass keeps the look it has without them), else `MIRROR_COUNT`.
fn mirror_of(v: VertexOut) -> u32 {
    if (!MIRRORS || frame.night.z != 0.0 || lit_class() == 0u || v.specular.x < 1.0) {
        return MIRROR_COUNT;
    }
    let k = min(u32(v.specular.x + 0.5) - 1u, MIRROR_COUNT);
    if (k == MIRROR_COUNT || mirror_views.views[k].scale.z == 0.0) {
        return MIRROR_COUNT;
    }
    return k;
}

// Mirror `k`'s image at `world` on its glass, in the frame's colours before grading. The glass's texel is not used:
// the retail cars' glass textures are dark throughout.
fn mirror_image(k: u32, world: vec3<f32>) -> vec3<f32> {
    let view = mirror_views.views[k];
    let clip = view.view_projection * vec4<f32>(world, 1.0);
    let ndc = clip.xy / max(clip.w, 1e-6);
    let uv = clamp(vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5), vec2<f32>(0.0), vec2<f32>(1.0)) * view.scale.xy;
    let image = textureSampleLevel(mirror_images, reflection_sampler, uv, k, 0.0).rgb;
    return ungraded(image) * MIRROR_SILVER;
}
