// The retail textures were painted for the game's dim baked light, and read too bright and too saturated under the
// physical sun (`notes/renderer.md` "Enhancements"): neutral mid-bright surfaces (tarmac, rock, concrete: mean 0.33
// against 0.05 to 0.2 quoted) lose up to `ALBEDO_NEUTRAL_CUT` of their luminance, snow and white paint (over 0.7) keep
// it, and the chroma of strongly saturated colours (grass, foliage: 0.74 against 0.6 to 0.7 quoted) is eased by up to
// `ALBEDO_CHROMA_CUT`. Linear in and out, one rule for every surface. Shared by the stage shader and the bounce's
// surfaces (`bounce_surface.wgsl`), so that the light bounced between surfaces has the albedo they are drawn with.
const ALBEDO_NEUTRAL_CUT: f32 = 0.4;
const ALBEDO_CHROMA_CUT: f32 = 0.15;

fn albedo_remap(c: vec3<f32>) -> vec3<f32> {
    let y = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    let top = max(c.r, max(c.g, c.b));
    let saturation = (top - min(c.r, min(c.g, c.b))) / max(top, 1e-4);
    let neutral = 1.0 - smoothstep(0.15, 0.45, saturation);
    let band = smoothstep(0.18, 0.30, y) * (1.0 - smoothstep(0.5, 0.7, y));
    let eased = vec3<f32>(y) + (c - y) * (1.0 - ALBEDO_CHROMA_CUT * smoothstep(0.5, 0.8, saturation));
    return eased * (1.0 - ALBEDO_NEUTRAL_CUT * neutral * band);
}
