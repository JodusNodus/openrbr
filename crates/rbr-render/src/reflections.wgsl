// The screen-space reflections (`renderer::reflection::ScreenReflection`, ours): the wet roads' and the water's draws
// drawn again, each ray stepped through the frame's depth; in the stage's module.

const SSR_REACH: f32 = 120.0;

const SSR_CUBE_REACH: vec2<f32> = vec2<f32>(20.0, 60.0);

// Glossy reflections: the frame is read from a blurrier level (`renderer::SCENE_MIPS`) the further the ray travels on
// screen, as a cone of `GLOSS` (its width over the distance, in pixels): wider on roads (rough asphalt under the film)
// than on water. A hit is trusted less the further behind the frame's surface the ray ends (`thickness`), fading
// instead of cutting off, and a ray that ends further behind than that keeps stepping (it passed behind something in
// front, a trunk or a foliage card, rather than hitting it).
const GLOSS: vec2<f32> = vec2<f32>(0.12, 0.04);

const ROUGHNESS: vec2<f32> = vec2<f32>(0.02, 0.01);

const STRETCH: f32 = 4.0;

// The frame's depth at `uv` (the first sample).
fn scene_depth_at(uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(scene_depth));
    let pixel = vec2<i32>(clamp(uv * size, vec2<f32>(0.0), size - 1.0));
    if (textureDimensions(scene_depth_copy).x > 1u) {
        return textureLoad(scene_depth_copy, pixel, 0).x;
    }
    return textureLoad(scene_depth, pixel, 0);
}

// The eye's distance to the stage at `uv`, where the frame's depth is `depth`.
fn scene_distance(uv: vec2<f32>, depth: f32) -> f32 {
    let p = frame.inverse_view_projection * vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
    return distance(p.xyz / p.w, frame.eye.xyz);
}

// Where `p` falls on the screen (uv, and z its depth), w 0 when it is off screen or behind the eye.
fn on_screen(p: vec3<f32>) -> vec4<f32> {
    let clip = frame.view_projection * vec4<f32>(p, 1.0);
    if (clip.w <= 1e-4) {
        return vec4<f32>(0.0);
    }
    let ndc = clip.xyz / clip.w;
    let inside = f32(all(abs(ndc.xy) <= vec2<f32>(1.0)));
    return vec4<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, ndc.z, inside);
}

// The screen-space reflection at `v` and its weight: the mean of `SSR_RAYS` rays spread over the gloss cone
// (`ROUGHNESS`, wider on roads than on water), so hits and misses average out instead of showing per pixel.
fn reflect_ray(v: VertexOut) -> vec4<f32> {
    // Interleaved gradient noise (Jimenez 2014) by pixel.
    let jitter = fract(52.9829189 * fract(dot(v.clip.xy, vec2<f32>(0.06711056, 0.00583715))));
    let view = normalize(frame.eye.xyz - v.world);
    var n = vec3<f32>(0.0, 1.0, 0.0);
    var weight = 0.0;
    if (reflect_water()) {
        n = water_normal(v.world);
        weight = water_fresnel(n, view);
    } else if (dot(v.normal, v.normal) > 1e-8) {
        n = rain_rings(normalize(v.normal), v.world, standing(wet_mask(v)));
        weight = wet_reflectance(n, view, v);
    }
    let ray = reflect(-view, n);
    if (weight <= 0.0 || ray.y <= 0.0) {
        return vec4<f32>(0.0);
    }
    // What a missed ray reflects: on water the cube near the eye, else nothing (what the surface was drawn with: the
    // sky on water, the cube on roads).
    var miss = vec4<f32>(0.0);
    if (frame.reflection.x != 0.0 && reflect_water()) {
        let near = 1.0 - smoothstep(SSR_CUBE_REACH.x, SSR_CUBE_REACH.y, distance(v.world, frame.eye.xyz));
        miss = vec4<f32>(textureSampleLevel(surroundings_cube, reflection_sampler, ray, select(3.0, 1.0, reflect_water())).rgb, weight * near);
    }
    let roughness = select(ROUGHNESS.x, ROUGHNESS.y, reflect_water());
    let side = normalize(cross(ray, n + vec3<f32>(1e-3, 0.0, 0.0)));
    let up = cross(side, ray);
    var sum = vec4<f32>(0.0);
    for (var k = 0; k < SSR_RAYS; k++) {
        // Points on a disc, turned by the pixel's noise.
        let angle = 6.2831853 * (f32(k) + jitter) / f32(SSR_RAYS);
        let radius = roughness * sqrt((f32(k) + 0.5) / f32(SSR_RAYS));
        // Stretched toward and away from the eye (`STRETCH`), as wet roads smear reflections down the screen.
        let spread = normalize(ray + radius * (cos(angle) * side + STRETCH * sin(angle) * up));
        let found = trace(v, spread, weight, miss, fract(jitter + f32(k) * 0.618));
        // Colour weighted by how much each ray contributes.
        sum += vec4<f32>(found.rgb * found.a, found.a);
    }
    sum /= f32(SSR_RAYS);
    return vec4<f32>(sum.rgb / max(sum.a, 1e-4), sum.a);
}

// One ray from `v` along `ray`: what it reflects and its weight (`weight` on a hit, `miss` when it leaves the screen
// or hits nothing), its steps starting `jitter` of a step early.
fn trace(v: VertexOut, ray: vec3<f32>, weight: f32, miss: vec4<f32>, jitter: f32) -> vec4<f32> {
    if (ray.y <= 0.0) {
        return miss;
    }
    let thickness = 0.15 + 0.01 * distance(v.world, frame.eye.xyz);
    var before = 0.0;
    for (var i = 1; i <= SSR_STEPS; i++) {
        let t = SSR_REACH * pow((f32(i) - jitter) / f32(SSR_STEPS), 2.0);
        let at = on_screen(v.world + ray * t);
        if (at.w == 0.0) {
            return miss;
        }
        // The step's distance behind the frame's surface only once it is behind it.
        let depth = scene_depth_at(at.xy);
        if (at.z < depth && depth > 0.0
            && distance(v.world + ray * t, frame.eye.xyz) - scene_distance(at.xy, depth) < thickness + 0.02 * t) {
            // Halve back to the surface.
            var low = before;
            var high = t;
            for (var k = 0; k < 5; k++) {
                let mid = 0.5 * (low + high);
                let m = on_screen(v.world + ray * mid);
                if (m.z < scene_depth_at(m.xy)) {
                    high = mid;
                } else {
                    low = mid;
                }
            }
            let hit = on_screen(v.world + ray * high);
            let behind = distance(v.world + ray * high, frame.eye.xyz) - scene_distance(hit.xy, scene_depth_at(hit.xy));
            let reach = thickness + 0.02 * high;
            if (behind <= reach) {
                let size = vec2<f32>(textureDimensions(scene_colour));
                let pixels = length((hit.xy - v.clip.xy / size) * size);
                let gloss = select(GLOSS.x, GLOSS.y, reflect_water());
                let lod = clamp(log2(max(pixels * gloss, 1.0)), 0.0, f32(textureNumLevels(scene_colour) - 1u));
                let colour = textureSampleLevel(scene_colour, reflection_sampler, hit.xy, lod).rgb;
                let edge = smoothstep(0.0, 0.15, min(min(hit.x, 1.0 - hit.x), min(hit.y, 1.0 - hit.y)));
                let fade = 1.0 - smoothstep(0.6, 1.0, high / SSR_REACH);
                let trust = edge * fade * (1.0 - smoothstep(0.3 * reach, reach, max(behind, 0.0)));
                return vec4<f32>(mix(miss.rgb, colour, trust), mix(miss.a, weight, trust));
            }
        }
        before = t;
    }
    return miss;
}

@fragment
fn fs_reflect(v: VertexOut) -> @location(0) vec4<f32> {
    return reflect_ray(v);
}
