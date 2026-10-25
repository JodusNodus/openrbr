// The ground (ours): fine detail where the near ground's textures blur, the soak of wet ground, and the backdrop's
// detail; in the stage's module (`shade_stage`).

const GROUND_SOAK: f32 = 0.6;

// `colour` darkened and saturated as wet ground is, by `amount` of the road's (`WET_DARKEN`, `WET_SATURATION`).
fn soak(colour: vec3<f32>, amount: f32) -> vec3<f32> {
    let grey = dot(colour, vec3<f32>(0.299, 0.587, 0.114));
    let saturated = max(mix(vec3<f32>(grey), colour, mix(1.0, WET_SATURATION, amount)), vec3<f32>(0.0));
    return saturated * mix(1.0, WET_DARKEN, amount);
}

// Ground detail (renderer choice): two octaves of 3D value noise in stage space, at 9 and 33 cm, that brighten or
// darken the ground by up to `DETAIL` near the eye and fade out between `DETAIL_FADE` metres, so the low-resolution
// ground textures do not look smeared at the chase camera's distance. Two more octaves, at 8 and 30 m, brighten or
// darken it by up to `MACRO` at every distance, so the repeats of a ground texture do not look alike.
const DETAIL: f32 = 0.08;

const DETAIL_FADE: vec2<f32> = vec2<f32>(4.0, 30.0);

const MACRO: f32 = 0.22;

fn ground_detail(world: vec3<f32>) -> f32 {
    let near = 1.0 - smoothstep(DETAIL_FADE.x, DETAIL_FADE.y, distance(world, frame.eye.xyz));
    // Beyond the fade the fine octaves count for nothing, so they are not computed.
    var fine = 0.5;
    if (near > 0.0) {
        fine = 0.65 * value_noise(world * 11.0) + 0.35 * value_noise(world * 3.0);
    }
    // The sum of the octaves stays near its middle; stretched so the patches reach `MACRO`.
    let coarse = smoothstep(0.3, 0.7, 0.6 * value_noise(world / 8.0) + 0.4 * value_noise(world / 30.0));
    return (1.0 + DETAIL * near * (2.0 * fine - 1.0)) * (1.0 + MACRO * (2.0 * coarse - 1.0));
}

// Backdrop detail (renderer choice): the backdrop's texture is stretched over kilometres, a few metres a texel, so
// next to the stage it reads as a blur. Its own texture, tiled flat in stage space at `BACKDROP_TILES` metres, adds
// the fine structure it lacks: each tiling's brightness over the texture's mean (its last mip) scales the colour by up
// to `BACKDROP_DETAIL`, and the tilings fade out where a pixel spans an eighth of a tile (the mips already blur them).
const BACKDROP_DETAIL: f32 = 0.6;

const BACKDROP_TILES: vec2<f32> = vec2<f32>(12.0, 47.0);

fn backdrop_detail(world: vec3<f32>) -> f32 {
    let footprint = max(length(dpdx(world)), length(dpdy(world)));
    let mean = max(dot(textureSampleLevel(texture0, texture_sampler, vec2<f32>(0.5), 16.0).rgb, vec3<f32>(0.333)), 1e-3);
    var detail = 0.0;
    for (var i = 0; i < 2; i++) {
        let tile = BACKDROP_TILES[i];
        let shown = 1.0 - smoothstep(0.06 * tile, 0.125 * tile, footprint);
        let texel = textureSample(texture0, texture_sampler, world.xz / tile + vec2<f32>(0.37 * f32(i))).rgb;
        detail += 0.5 * shown * (dot(texel, vec3<f32>(0.333)) / mean - 1.0);
    }
    return max(1.0 + BACKDROP_DETAIL * detail, 0.2);
}
