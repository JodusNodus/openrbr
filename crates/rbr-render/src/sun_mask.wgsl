// The sun's shadow over the main view (renderer addition, `renderer::sun_mask`, ours), appended to `shader.wgsl`: once
// per pixel from the depth and the normals the prepass laid, before the stage is shaded, which reads it back (`sun_lit` with
// `SUN_MASK`). The stage's shader keeps no filter of its own, so the soft shadows' search and taps cost it no
// registers (with them in it, the stage's shading took 1 to 2 ms longer whether they ran or not).

// The depth's samples (`renderer::AntiAliasing::samples`).
override MASK_SAMPLES: i32 = 4;

// The nearest of the depth's samples at `p` (the depth is reversed: the largest), 0 where only the sky is.
fn mask_depth(p: vec2<i32>) -> f32 {
    var depth = 0.0;
    for (var s = 0; s < MASK_SAMPLES; s++) {
        depth = max(depth, textureLoad(scene_depth, p, s));
    }
    return depth;
}

fn mask_world(p: vec2<i32>, depth: f32) -> vec3<f32> {
    let uv = (vec2<f32>(p) + 0.5) / vec2<f32>(textureDimensions(scene_depth));
    let w = frame.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return w.xyz / w.w;
}

@fragment
fn fs_sun_mask(v: SkyOut) -> @location(0) vec4<f32> {
    let p = vec2<i32>(v.clip.xy);
    let depth = mask_depth(p);
    if (depth == 0.0 || frame.sun_shadow.x == 0.0) {
        return vec4<f32>(1.0);
    }
    let world = mask_world(p, depth);
    // The draws' normal as `own_sun_seen` takes it, the prepass's (`fs_prepass`), its samples' mean.
    let n = textureLoad(scene_colour, p, 0).xyz * 2.0 - 1.0;
    let c = sun_point(world, select(vec3<f32>(0.0, 1.0, 0.0), normalize(n), dot(n, n) > 1e-4));
    if (c.layer < 0) {
        return vec4<f32>(1.0);
    }
    if (frame.sun_shadow.z != 0.0) {
        return vec4<f32>(overcast_filter(c));
    }
    // Soft only on the nearest cascade, where the penumbra spans texels (`soft_filter`).
    if (c.layer == 0) {
        return vec4<f32>(soft_filter(c.uv, c.layer, c.depth));
    }
    return vec4<f32>(texel_filter(c.uv, c.layer, c.depth));
}

// Soft shadows (ours, PCSS after Fernando 2005): the sun's disc, 0.53 degrees across, blurs a shadow by its caster's
// distance, sharp where it touches. The casters' mean depth from a 3x3 search within `SHADOW_SEARCH` metres of the
// point (at most `SEARCH_TEXELS`), their distance along the sun (the cascade's depth per metre from its matrix), and a
// grid of compared taps over the penumbra, `SUN_RADIUS` of that distance about the point, at most `SOFT_TEXELS` (casters
// to about 12 m), the taps at most 2 texels apart (further, their gaps showed as a grid; up to 5x5). On the nearest
// cascade only: the others' texels are as wide as the penumbrae (1.3 to 2 ms with all of them, 16 search taps and 6
// texels at most). Within `texel_filter`'s
// reach, or with no caster among the search's taps (a twig between them), `texel_filter` as before.
const SUN_RADIUS: f32 = 0.00465;
const SHADOW_SEARCH: f32 = 0.3;
const SEARCH_TEXELS: f32 = 16.0;
const SOFT_TEXELS: f32 = 4.0;

fn soft_filter(uv: vec2<f32>, layer: i32, depth: f32) -> f32 {
    let size = vec2<f32>(textureDimensions(sun_map));
    let texel = frame.sun_texels[layer];
    let search = min(SHADOW_SEARCH / texel, SEARCH_TEXELS);
    var sum = 0.0;
    var casters = 0.0;
    for (var y = 0; y < 3; y++) {
        for (var x = 0; x < 3; x++) {
            let offset = (vec2<f32>(f32(x), f32(y)) - 1.0) * search;
            let at = clamp(vec2<i32>(uv * size + offset), vec2<i32>(0), vec2<i32>(size) - 1);
            let d = textureLoad(sun_map, at, layer, 0);
            if (d < depth) {
                sum += d;
                casters += 1.0;
            }
        }
    }
    let m = frame.sun_matrices[layer];
    let metres = select(0.0, (depth - sum / max(casters, 1.0)) / length(vec3<f32>(m[0].z, m[1].z, m[2].z)), casters > 0.0);
    let radius = min(metres * SUN_RADIUS / texel, SOFT_TEXELS);
    if (radius <= 1.5) {
        return texel_filter(uv, layer, depth);
    }
    let taps = i32(ceil(2.0 * radius)) + 1;
    let half = 0.5 * f32(taps - 1);
    var lit = 0.0;
    for (var y = 0; y < taps; y++) {
        for (var x = 0; x < taps; x++) {
            let at = uv + (vec2<f32>(f32(x), f32(y)) - half) / half * radius / size;
            lit += textureSampleCompareLevel(sun_map, shadow_sampler, at, layer, depth);
        }
    }
    return lit / f32(taps * taps);
}
