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

// The mask: the sun's share.
@fragment
fn fs_sun_mask(v: SkyOut) -> @location(0) vec4<f32> {
    let p = vec2<i32>(v.clip.xy);
    let depth = mask_depth(p);
    if (depth == 0.0) {
        return vec4<f32>(1.0);
    }
    let world = mask_world(p, depth);
    // The draws' normal as `own_sun_seen` takes it, the prepass's (`fs_prepass`), its samples' mean.
    var n = textureLoad(scene_colour, p, 0).xyz * 2.0 - 1.0;
    n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(n), dot(n, n) > 1e-4);
    return vec4<f32>(mask_sun(world, n));
}

// The contact shading (`contact_light`) at half the frame's size, from the depth's copy: each texel the pixel at its
// top left corner. The stage reads it at its pixel's (`CONTACT`).
@fragment
fn fs_contact(v: SkyOut) -> @location(0) vec4<f32> {
    let p = vec2<i32>(v.clip.xy) * 2;
    let depth = textureLoad(scene_depth_copy, p, 0).x;
    if (depth == 0.0) {
        return vec4<f32>(1.0);
    }
    return vec4<f32>(contact_light(p, mask_world(p, depth)));
}

fn mask_sun(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (frame.sun_shadow.x == 0.0) {
        return 1.0;
    }
    let c = sun_point(world, n);
    if (c.layer < 0) {
        return 1.0;
    }
    if (is_overcast()) {
        return overcast_filter(c);
    }
    // Soft only on the nearest cascade, where the penumbra spans texels (`soft_filter`).
    if (c.layer == 0) {
        return soft_filter(c.uv, c.layer, c.depth);
    }
    return texel_filter(c.uv, c.layer, c.depth);
}

// Contact shading (ours, screen-space ambient occlusion after McGuire et al. 2011, "The Alchemy Screen-Space Ambient
// Obscurance"): the sky's and the bounce's share at a point that the stage within `CONTACT_RADIUS` of it hides, from
// the depth about the pixel along `CONTACT_DIRECTIONS` directions in `CONTACT_STEPS` steps; each neighbour above the
// face hides by its angle over it, fading out to the radius. The per-vertex occlusion (`sky_occlusion.rs`) holds what
// lies metres away; this, what it is too coarse for: grass roots, posts' and barriers' feet, rocks on the verge,
// creases. Never the sun's (its cascades hold that). Faded out where the radius spans under 2 pixels.
const CONTACT_RADIUS: f32 = 1.0;
const CONTACT_DIRECTIONS: i32 = 4;
const CONTACT_STEPS: i32 = 3;
const CONTACT_STRENGTH: f32 = 1.5;
// The angle over the face below which a neighbour hides nothing (against the depth's flat steps).
const CONTACT_BIAS: f32 = 0.1;

fn contact_light(p: vec2<i32>, world: vec3<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let m = frame.view_projection;
    let w = (m * vec4<f32>(world, 1.0)).w;
    // The radius in pixels: the projection's vertical scale over the distance along the view.
    let pixels = CONTACT_RADIUS * 0.5 * size.y * length(vec3<f32>(m[0].y, m[1].y, m[2].y)) / max(w, 1e-3);
    if (pixels < 2.0) {
        return 1.0;
    }
    let reach = min(pixels, 64.0);
    // The face's own normal from the depth: the draws' (up on the crowds' and boards' cards, the crowns' on foliage)
    // would have a face hide itself.
    var n = cross(depth_step(world, p, vec2<i32>(0, 1)), depth_step(world, p, vec2<i32>(1, 0)));
    n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(n), dot(n, n) > 1e-12);
    n *= select(-1.0, 1.0, dot(n, frame.eye.xyz - world) >= 0.0);
    var hidden = 0.0;
    for (var d = 0; d < CONTACT_DIRECTIONS; d++) {
        let angle = (f32(d) + 0.5) * 6.2831853 / f32(CONTACT_DIRECTIONS);
        let along = vec2<f32>(cos(angle), sin(angle));
        for (var s = 0; s < CONTACT_STEPS; s++) {
            // Steps spread further out, the first a pixel or two away; staggered per direction.
            let t = (f32(s) + 0.5 + 0.5 * f32(d & 1)) / f32(CONTACT_STEPS);
            let q = clamp(p + vec2<i32>(along * max(t * t * reach, 1.5)), vec2<i32>(0), vec2<i32>(size) - 1);
            let depth = textureLoad(scene_depth_copy, q, 0).x;
            if (depth == 0.0) {
                continue;
            }
            let h = mask_world(q, depth) - world;
            let d2 = dot(h, h);
            let r2 = CONTACT_RADIUS * CONTACT_RADIUS;
            if (d2 < r2 && d2 > 1e-6) {
                hidden += max(dot(n, h) * inverseSqrt(d2) - CONTACT_BIAS, 0.0) * (1.0 - d2 / r2);
            }
        }
    }
    let fade = smoothstep(2.0, 4.0, pixels);
    let open = max(1.0 - CONTACT_STRENGTH * hidden / f32(CONTACT_DIRECTIONS * CONTACT_STEPS), 0.0);
    return mix(1.0, open, fade);
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

// Along `axis` from the pixel `p` at `world`, the nearer of the two neighbours' steps, so that a silhouette's far side
// does not tilt the face.
fn depth_step(world: vec3<f32>, p: vec2<i32>, axis: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(textureDimensions(scene_depth)) - 1;
    let after = clamp(p + axis, vec2<i32>(0), last);
    let before = clamp(p - axis, vec2<i32>(0), last);
    let ahead = mask_world(after, textureLoad(scene_depth_copy, after, 0).x) - world;
    let behind = world - mask_world(before, textureLoad(scene_depth_copy, before, 0).x);
    return select(behind, ahead, dot(ahead, ahead) < dot(behind, behind));
}
