// How much of the sky each stage vertex sees (`sky_occlusion.rs`, ours). For one direction toward the sky,
// `accumulate` looks each vertex up in the depth map of its tile, a little off the surface, and adds the direction's
// weight (how squarely the vertex faces it) to the vertex's sums when the sky is seen; `finish` writes 1 - seen / all
// into the vertex's `occlusion`.

struct Params {
    // Toward the sky, and w how far off the surface a vertex is looked up, metres.
    toward: vec4<f32>,
    // The tiles' grid: x and y its corner (stage x and z), z the tile's side, w its columns.
    grid: vec4<f32>,
    // x the grid's rows of tiles.
    counts: vec4<u32>,
}

@group(0) @binding(0) var<uniform> params: Params;
// Each map's clip matrix in this batch.
@group(0) @binding(1) var<uniform> matrices: array<mat4x4<f32>, 8>;
// Each tile's map in this batch, or -1.
@group(0) @binding(2) var<storage, read> maps: array<i32>;
@group(0) @binding(3) var depth: texture_depth_2d_array;
@group(0) @binding(4) var depth_sampler: sampler_comparison;
// The vertex buffer, as words (`stage::Vertex`: 17 a vertex, the normal from 10, the occlusion at 16).
@group(0) @binding(5) var<storage, read_write> vertices: array<f32>;
// Each vertex's seen and total weight.
@group(0) @binding(6) var<storage, read_write> sums: array<vec2<f32>>;

const STRIDE: u32 = 17u;
const NORMAL: u32 = 10u;
const OCCLUSION: u32 = 16u;

fn normal_of(base: u32) -> vec3<f32> {
    let n = vec3<f32>(vertices[base + NORMAL], vertices[base + NORMAL + 1u], vertices[base + NORMAL + 2u]);
    return select(vec3<f32>(0.0, 1.0, 0.0), normalize(n), dot(n, n) > 1e-8);
}

@compute @workgroup_size(64)
fn accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= arrayLength(&sums)) {
        return;
    }
    let base = i * STRIDE;
    let position = vec3<f32>(vertices[base], vertices[base + 1u], vertices[base + 2u]);
    let tile = floor((position.xz - params.grid.xy) / params.grid.z);
    let columns = u32(params.grid.w);
    if (any(tile < vec2<f32>(0.0)) || u32(tile.x) >= columns || u32(tile.y) >= params.counts.x) {
        return;
    }
    let map = maps[u32(tile.y) * columns + u32(tile.x)];
    if (map < 0) {
        return;
    }
    let n = normal_of(base);
    let toward = params.toward.xyz;
    let weight = max(dot(n, toward), 0.0);
    if (weight == 0.0) {
        return;
    }
    let p = matrices[map] * vec4<f32>(position + (n + toward) * params.toward.w, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    var seen = 1.0;
    if (all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0)) && p.z <= 1.0) {
        seen = textureSampleCompareLevel(depth, depth_sampler, uv, map, p.z);
    }
    sums[i] += vec2<f32>(seen * weight, weight);
}

@compute @workgroup_size(64)
fn finish(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= arrayLength(&sums)) {
        return;
    }
    let sum = sums[i];
    if (sum.y > 0.0) {
        vertices[i * STRIDE + OCCLUSION] = 1.0 - sum.x / sum.y;
    }
}
