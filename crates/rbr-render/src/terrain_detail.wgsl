// Terrain detail (ours, renderer choice): the near ground's textures are spread thin (RSF Luceram's 4096 atlases, a
// few texels a centimetre on the flat and stretched down the banks), so close up they read as a smear. The draw's own
// texture's most detailed window (`mode.detail`, `terrain_detail.rs`), tiled mirrored at `TERRAIN_DENSITY` texels a
// metre and projected on the three axis planes by the face's normal (triplanar, so a steep bank does not streak it),
// adds the fine structure: each projection's brightness over its own blur `TERRAIN_BLUR` levels coarser (a high pass,
// so only the structure and not the window's colour comes through) scales the albedo by up to `TERRAIN_DETAIL`, in
// two tilings `TERRAIN_SCALE` apart, on the flat `TERRAIN_FLAT` of it. It fades out where the ground's own texture is
// sharper than a pixel.
const TERRAIN_DETAIL: f32 = 0.6;
const TERRAIN_DENSITY: f32 = 160.0;
const TERRAIN_BLUR: f32 = 3.0;
const TERRAIN_SCALE: f32 = 2.3;
const TERRAIN_FLAT: f32 = 0.25;

fn terrain_detail(v: VertexOut, steep: f32) -> f32 {
    let dx = dpdx(v.world);
    let dy = dpdy(v.world);
    let face = cross(dx, dy);
    let n = abs(face) / max(length(face), 1e-12);
    var w = n * n * n * n;
    w = w / (w.x + w.y + w.z);
    let size = vec2<f32>(textureDimensions(texture0));
    // How many of the ground texture's texels a pixel spans along its finer axis: under 1 it is magnified (a smear).
    let native = min(length(dpdx(v.uv0) * size), length(dpdy(v.uv0) * size));
    // The flat (roads and verges) only a little: the window may be another material than the ground here (580's
    // tarmac took its atlas's gravel).
    let strength = TERRAIN_DETAIL * mix(TERRAIN_FLAT, 1.0, steep) * (1.0 - smoothstep(0.5, 1.5, native));
    let window = mode.detail;
    if (strength <= 0.0 || window.z == 0.0) {
        return 1.0;
    }
    // The window's side in metres.
    let tile = window.z * size.x / TERRAIN_DENSITY;
    let coarse = exp2(TERRAIN_BLUR);
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    var ratio = 0.0;
    var weight = 0.0;
    for (var k = 0; k < 2; k++) {
        let scale = tile * select(1.0, TERRAIN_SCALE, k == 1);
        let p = v.world / scale;
        let px = dx / scale;
        let py = dy / scale;
        for (var i = 0; i < 3; i++) {
            if (w[i] < 0.02) {
                continue;
            }
            // The plane across axis i, each offset so the projections do not repeat each other's texels.
            var q: vec2<f32>;
            var gx: vec2<f32>;
            var gy: vec2<f32>;
            if (i == 0) {
                q = p.zy; gx = px.zy; gy = py.zy;
            } else if (i == 1) {
                q = p.xz; gx = px.xz; gy = py.xz;
            } else {
                q = p.xy; gx = px.xy; gy = py.xy;
            }
            q += vec2<f32>(0.31, 0.57) * f32(i + 3 * k);
            // Mirrored, so the window tiles without seams.
            let uv = window.xy + (1.0 - abs(1.0 - 2.0 * fract(0.5 * q))) * window.zw;
            gx *= window.zw;
            gy *= window.zw;
            let fine = textureSampleGrad(texture0, texture_sampler, uv, gx, gy).rgb;
            let mean = textureSampleGrad(texture0, texture_sampler, uv, gx * coarse, gy * coarse).rgb;
            ratio += w[i] * (dot(fine, luma) / max(dot(mean, luma), 0.02) - 1.0);
            weight += w[i];
        }
    }
    return clamp(1.0 + strength * 2.0 * ratio / max(weight, 1e-3), 0.3, 1.8);
}

// The ground texture's colour without its streaks: the atlases' banks were painted by a projection from above, so on a
// steep face the texture holds streaks down the slope. There it is read `TERRAIN_EVEN` levels blurrier, an even
// colour that `terrain_detail` fills in.
const TERRAIN_EVEN: f32 = 1.0;

fn terrain_even(t: texture_2d<f32>, uv: vec2<f32>, level: f32, steep: f32) -> vec4<f32> {
    return textureSampleLevel(t, texture_sampler, uv, level + TERRAIN_EVEN * steep);
}

// `t`'s isotropic mip level at `uv` (by the longer footprint), outside the branches that read it (derivatives).
fn terrain_level(t: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    let size = vec2<f32>(textureDimensions(t));
    return max(log2(max(max(length(dpdx(uv) * size), length(dpdy(uv) * size)), 1e-6)), 0.0);
}

// How steep the face is, 0 flat to 1 past about 50 degrees.
fn terrain_steep(world: vec3<f32>) -> f32 {
    let face = cross(dpdx(world), dpdy(world));
    return 1.0 - smoothstep(0.6, 0.85, abs(face.y) / max(length(face), 1e-12));
}
