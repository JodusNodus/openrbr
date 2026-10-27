// The volumetric clouds' shadow map read back (renderer addition, `renderer::cloud_volume`, ours), appended to
// `shader.wgsl` and `light_shafts.wgsl`.

// How much of the sun a cloud takes at stage point `world` (0 none, 1 all) from the shadow map: `map` holds the
// sun's transmittance through the volume along the sun from the ground at `params` (its corner's stage x and z, its
// side in metres, the height it was worked out at), `samp` its sampler. Another pass (the light shafts') binds the
// same map and calls this. Outside the map, none.
fn volume_shadow(map: texture_2d<f32>, samp: sampler, params: vec4<f32>, world: vec3<f32>, to_sun: vec3<f32>) -> f32 {
    if (params.z <= 0.0) {
        return 0.0;
    }
    let xz = world.xz + to_sun.xz / to_sun.y * (params.w - world.y);
    let uv = (xz - params.xy) / params.z;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        return 0.0;
    }
    return textureSampleLevel(map, samp, uv, 0.0).r;
}
