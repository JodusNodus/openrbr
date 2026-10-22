// The light the stage bounces onto itself (`bounce.rs`, ours). `clear` zeroes each vertex's two bounce words; `sun`
// writes into the first, for the surface draws of `bounce_surface.wgsl`, the vertex's share of the sun: its facing
// times whether the sun's tile map sees it. Along each direction below the horizon `gather` looks the vertex up in the
// map of its tile, drawn from below: where a surface lies between, it adds the surface's light (the map's two colours,
// per unit of the sun's and of the sky's) weighed by how squarely the vertex faces it. `finish` packs the sums into
// the two words (`bounce_light` in `shader.wgsl`).

struct Params {
    // The direction the vertex looks along (toward the sun for `sun`), and w how far off the surface it is looked up,
    // metres.
    toward: vec4<f32>,
    // The tiles' grid: x and y its corner (stage x and z), z the tile's side, w its columns.
    grid: vec4<f32>,
    // x the grid's rows of tiles.
    counts: vec4<u32>,
    // x each direction's weight, its share of the lower hemisphere's solid angle over pi.
    weight: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<uniform> matrices: array<mat4x4<f32>, 8>;
@group(0) @binding(2) var<storage, read> maps: array<i32>;
@group(0) @binding(3) var depth: texture_depth_2d_array;
@group(0) @binding(4) var depth_sampler: sampler_comparison;
// The vertex buffer, as words (`stage::Vertex`: 20 a vertex, the normal from 10, the bounce at 18 and 19).
@group(0) @binding(5) var<storage, read_write> vertices: array<u32>;
// Each vertex's sums of the light it sees, per unit of the sun's and of the sky's.
@group(0) @binding(6) var<storage, read_write> sun_sums: array<vec4<f32>>;
@group(0) @binding(7) var<storage, read_write> sky_sums: array<vec4<f32>>;
@group(0) @binding(8) var sun_light: texture_2d_array<f32>;
@group(0) @binding(9) var sky_light: texture_2d_array<f32>;
@group(0) @binding(10) var light_sampler: sampler;

const STRIDE: u32 = 20u;
const NORMAL: u32 = 10u;
const BOUNCE: u32 = 18u;
// The packed shares' range (`bounce_light` in `shader.wgsl`).
const RANGE: f32 = 2.0;

fn word(at: u32) -> f32 {
    return bitcast<f32>(vertices[at]);
}

fn normal_of(base: u32) -> vec3<f32> {
    let n = vec3<f32>(word(base + NORMAL), word(base + NORMAL + 1u), word(base + NORMAL + 2u));
    return select(vec3<f32>(0.0, 1.0, 0.0), normalize(n), dot(n, n) > 1e-8);
}

// The map of vertex `base`'s tile in this batch, or -1.
fn map_of(position: vec3<f32>) -> i32 {
    let tile = floor((position.xz - params.grid.xy) / params.grid.z);
    let columns = u32(params.grid.w);
    if (any(tile < vec2<f32>(0.0)) || u32(tile.x) >= columns || u32(tile.y) >= params.counts.x) {
        return -1;
    }
    return maps[u32(tile.y) * columns + u32(tile.x)];
}

fn position_of(base: u32) -> vec3<f32> {
    return vec3<f32>(word(base), word(base + 1u), word(base + 2u));
}

// Where vertex `base` (normal `n`) is looked up in `map` along `toward`: the map's uv and depth, z above 1 outside.
fn looked_up(base: u32, n: vec3<f32>, map: i32) -> vec3<f32> {
    let toward = params.toward.xyz;
    let p = matrices[map] * vec4<f32>(position_of(base) + (n + toward) * params.toward.w, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return vec3<f32>(0.0, 0.0, 2.0);
    }
    return vec3<f32>(uv, p.z);
}

@compute @workgroup_size(64)
fn clear(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&sun_sums)) {
        return;
    }
    vertices[id.x * STRIDE + BOUNCE] = 0u;
    vertices[id.x * STRIDE + BOUNCE + 1u] = 0u;
}

@compute @workgroup_size(64)
fn sun(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= arrayLength(&sun_sums)) {
        return;
    }
    let base = id.x * STRIDE;
    let map = map_of(position_of(base));
    if (map < 0) {
        return;
    }
    let n = normal_of(base);
    let facing = max(dot(n, params.toward.xyz), 0.0);
    var seen = 0.0;
    if (facing > 0.0) {
        let at = looked_up(base, n, map);
        seen = 1.0;
        if (at.z <= 1.0) {
            seen = textureSampleCompareLevel(depth, depth_sampler, at.xy, map, at.z);
        }
    }
    vertices[base + BOUNCE] = bitcast<u32>(facing * seen);
}

@compute @workgroup_size(64)
fn gather(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= arrayLength(&sun_sums)) {
        return;
    }
    let base = i * STRIDE;
    let map = map_of(position_of(base));
    if (map < 0) {
        return;
    }
    let n = normal_of(base);
    let facing = dot(n, params.toward.xyz);
    if (facing <= 0.0) {
        return;
    }
    let at = looked_up(base, n, map);
    if (at.z > 1.0) {
        return;
    }
    // 1 where a surface lies between the map's eye (below) and the vertex: the one it sees.
    let hit = 1.0 - textureSampleCompareLevel(depth, depth_sampler, at.xy, map, at.z);
    if (hit == 0.0) {
        return;
    }
    let weight = hit * facing * params.weight.x;
    sun_sums[i] += textureSampleLevel(sun_light, light_sampler, at.xy, map, 0.0) * weight;
    sky_sums[i] += textureSampleLevel(sky_light, light_sampler, at.xy, map, 0.0) * weight;
}

// `c` (each 0 to `RANGE`) in three 10-bit shares.
fn packed(c: vec3<f32>) -> u32 {
    let q = vec3<u32>(round(clamp(c / RANGE, vec3<f32>(0.0), vec3<f32>(1.0)) * 1023.0));
    return q.x | (q.y << 10u) | (q.z << 20u);
}

@compute @workgroup_size(64)
fn finish(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= arrayLength(&sun_sums)) {
        return;
    }
    vertices[i * STRIDE + BOUNCE] = packed(sun_sums[i].rgb);
    vertices[i * STRIDE + BOUNCE + 1u] = packed(sky_sums[i].rgb);
}
