// The car's shadow map's depth range about each cell (renderer addition, ours; `renderer::shadows::ShadowRange`): the
// least and greatest depth of each layer over every texel a filter centred in a `RANGE_CELL` x `RANGE_CELL` texel cell
// can read, so that the filters (`car_shadow`, `car_contact` in `shader.wgsl`) tell from one texel when all their
// taps agree. Each cell's own range first (`cs_cells`), then spread over the filter's reach along x and along y.

@group(0) @binding(0) var shadow_map: texture_depth_2d_array;
@group(0) @binding(1) var cells: texture_storage_2d_array<rg32float, write>;
@group(0) @binding(2) var spread_from: texture_2d_array<f32>;
@group(0) @binding(3) var spread_to: texture_storage_2d_array<rg32float, write>;
// Per layer, the cells the reach spans each side.
@group(0) @binding(4) var<uniform> reach: vec4<u32>;

// Texels a cell's side (`renderer::shadows::RANGE_CELL`).
const RANGE_CELL: u32 = 4u;

@compute @workgroup_size(8, 8)
fn cs_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(cells);
    if (any(id.xy >= size)) {
        return;
    }
    var range = vec2<f32>(3.0e38, -3.0e38);
    for (var y = 0u; y < RANGE_CELL; y++) {
        for (var x = 0u; x < RANGE_CELL; x++) {
            let depth = textureLoad(shadow_map, id.xy * RANGE_CELL + vec2<u32>(x, y), id.z, 0);
            range = vec2<f32>(min(range.x, depth), max(range.y, depth));
        }
    }
    textureStore(cells, id.xy, id.z, vec4<f32>(range, 0.0, 0.0));
}

fn spread(id: vec3<u32>, along: vec2<i32>) {
    let size = vec2<i32>(textureDimensions(spread_to));
    if (any(vec2<i32>(id.xy) >= size)) {
        return;
    }
    let span = i32(reach[id.z]);
    var range = vec2<f32>(3.0e38, -3.0e38);
    for (var k = -span; k <= span; k++) {
        let at = clamp(vec2<i32>(id.xy) + along * k, vec2<i32>(0), size - 1);
        let cell = textureLoad(spread_from, at, id.z, 0).xy;
        range = vec2<f32>(min(range.x, cell.x), max(range.y, cell.y));
    }
    textureStore(spread_to, id.xy, id.z, vec4<f32>(range, 0.0, 0.0));
}

@compute @workgroup_size(8, 8)
fn cs_spread_x(@builtin(global_invocation_id) id: vec3<u32>) {
    spread(id, vec2<i32>(1, 0));
}

@compute @workgroup_size(8, 8)
fn cs_spread_y(@builtin(global_invocation_id) id: vec3<u32>) {
    spread(id, vec2<i32>(0, 1));
}
