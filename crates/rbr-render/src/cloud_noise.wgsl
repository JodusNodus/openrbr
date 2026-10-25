// The volumetric clouds' noise (`renderer::cloud_volume`, ours), made once at load: tiling 3D textures after
// Schneider's (Guerrilla, "The real-time volumetric cloudscapes of Horizon Zero Dawn", 2015). The shape texture holds
// Perlin-Worley noise (r: Perlin's billows cut by inverted Worley cells) and inverted Worley at three rising
// frequencies (gba); the detail texture inverted Worley at three more (rgb). Every octave's period divides the
// texture, so both repeat seamlessly.

struct NoiseParams {
    // x the texture's side, texels; y 1 for the detail texture.
    size: vec4<u32>,
}

@group(0) @binding(0) var<uniform> noise_params: NoiseParams;
@group(0) @binding(1) var noise_out: texture_storage_3d<rgba8unorm, write>;

fn noise_hash(p: vec3<u32>) -> vec3<f32> {
    var v = p * vec3<u32>(1664525u, 1013904223u, 22695477u) + vec3<u32>(1013904223u, 1664525u, 2654435761u);
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    v ^= v >> vec3<u32>(16u);
    v.x += v.y * v.z;
    v.y += v.z * v.x;
    v.z += v.x * v.y;
    return vec3<f32>(v & vec3<u32>(0xffffffu)) / f32(0xffffff);
}

fn wrap(c: vec3<i32>, period: i32) -> vec3<u32> {
    return vec3<u32>(((c % period) + period) % period);
}

// Worley noise at `p` (0 to 1 over the tile) with `period` cells across, inverted: 1 at a cell's point, 0 far from all.
fn worley(p: vec3<f32>, period: i32, seed: u32) -> f32 {
    let q = p * f32(period);
    let cell = vec3<i32>(floor(q));
    var nearest = 1e9;
    for (var z = -1; z <= 1; z++) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let c = cell + vec3<i32>(x, y, z);
                let point = vec3<f32>(c) + noise_hash(wrap(c, period) + vec3<u32>(seed * 131u));
                let d = q - point;
                nearest = min(nearest, dot(d, d));
            }
        }
    }
    return 1.0 - clamp(sqrt(nearest), 0.0, 1.0);
}

fn gradient(c: vec3<i32>, period: i32) -> vec3<f32> {
    return normalize(noise_hash(wrap(c, period) + vec3<u32>(7919u)) * 2.0 - 1.0 + vec3<f32>(1e-4));
}

// Perlin's gradient noise at `p` with `period` cells across, about -1 to 1.
fn perlin(p: vec3<f32>, period: i32) -> f32 {
    let q = p * f32(period);
    let cell = vec3<i32>(floor(q));
    let f = fract(q);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    var corners: array<f32, 8>;
    for (var k = 0; k < 8; k++) {
        let o = vec3<i32>(k & 1, (k >> 1u) & 1, (k >> 2u) & 1);
        corners[k] = dot(gradient(cell + o, period), f - vec3<f32>(o));
    }
    let x0 = mix(corners[0], corners[1], u.x);
    let x1 = mix(corners[2], corners[3], u.x);
    let x2 = mix(corners[4], corners[5], u.x);
    let x3 = mix(corners[6], corners[7], u.x);
    return mix(mix(x0, x1, u.y), mix(x2, x3, u.y), u.z);
}

// Three octaves of inverted Worley from `period` cells up.
fn worley_fbm(p: vec3<f32>, period: i32) -> f32 {
    return worley(p, period, 1u) * 0.625 + worley(p, period * 2, 2u) * 0.25 + worley(p, period * 4, 3u) * 0.125;
}

fn remap(x: f32, low: f32, high: f32, to_low: f32, to_high: f32) -> f32 {
    return to_low + (x - low) / (high - low) * (to_high - to_low);
}

@compute @workgroup_size(4, 4, 4)
fn cs_cloud_noise(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = noise_params.size.x;
    if (any(id >= vec3<u32>(size))) {
        return;
    }
    let p = (vec3<f32>(id) + 0.5) / f32(size);
    if (noise_params.size.y != 0u) {
        textureStore(noise_out, id, vec4<f32>(worley_fbm(p, 2), worley_fbm(p, 4), worley_fbm(p, 8), 1.0));
        return;
    }
    var billows = 0.0;
    var amplitude = 1.0;
    var total = 0.0;
    for (var k = 0; k < 4; k++) {
        billows += perlin(p, 4 << u32(k)) * amplitude;
        total += amplitude;
        amplitude *= 0.5;
    }
    billows = clamp(billows / total * 0.5 + 0.5, 0.0, 1.0);
    let cells = worley_fbm(p, 4);
    let perlin_worley = clamp(remap(billows, cells - 1.0, 1.0, 0.0, 1.0), 0.0, 1.0);
    textureStore(noise_out, id, vec4<f32>(perlin_worley, cells, worley_fbm(p, 8), worley_fbm(p, 16)));
}
