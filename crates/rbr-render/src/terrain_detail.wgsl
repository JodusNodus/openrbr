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

// On a steep bank the top-down texture stretches down the fall line, so the bank takes the draw's own window in
// world space instead (`terrain_bank` below): the window tiled mirrored and triplanar as above, over the textures'
// directional mean (`bank_low`): the fall-line bands are wide across and endless along, so only a wide kernel across
// the footprint's major axis (`TERRAIN_BANK_WIDE` texels) averages them out, while the minor axis stays sharp for
// the along-slope patches. The window's brightness over its mean (`TERRAIN_BANK_MEAN` levels coarser) scales that
// mean, within `TERRAIN_BANK_CLAMP`: structure without the window's colour, whose draped blades read as streaks on
// a bank. Weighted by the steepness alone (`shade_stage`): the mapping stretches with the slope whatever the
// distance (measured: the bank's footprint is minified 2 to 4 times, yet streaked), while the flat reads 0
// steepness and the paved road keeps its own treatment.
const TERRAIN_BANK_WIDE: f32 = 256.0;
const TERRAIN_BANK_MEAN: f32 = 3.0;
const TERRAIN_BANK_CLAMP: vec2<f32> = vec2<f32>(0.7, 1.4);

// `t`'s mean at `uv` with `gx` and `gy` (uv per pixel) widened along the footprint's major axis to
// `TERRAIN_BANK_WIDE` texels: the bands across average out, the patches along stay. No derivative calls (the widened
// gradients are explicit), so it may run outside uniform control flow.
fn bank_low(t: texture_2d<f32>, uv: vec2<f32>, gx: vec2<f32>, gy: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(t));
    let px = gx * size;
    let py = gy * size;
    let a = dot(px, px);
    let b = dot(px, py);
    let c = dot(py, py);
    let disc = sqrt(max((a - c) * (a - c) + 4.0 * b * b, 0.0));
    let lambda = (a + c - disc) * 0.5;
    let major = sqrt(max((a + c + disc) * 0.5, 1e-6));
    // The minor axis: `(b, lambda - a)`, the axes themselves where the footprint is axis-aligned.
    var e = vec2<f32>(1.0, 0.0);
    if (abs(b) > 1e-6) {
        e = normalize(vec2<f32>(b, lambda - a));
    } else if (c < a) {
        e = vec2<f32>(0.0, 1.0);
    }
    let f = vec2<f32>(-e.y, e.x);
    let k = clamp(TERRAIN_BANK_WIDE / major, 1.0, 8.0);
    let wx = e * dot(px, e) + f * (dot(px, f) * k);
    let wy = e * dot(py, e) + f * (dot(py, f) * k);
    return textureSampleGrad(t, texture_sampler, uv, wx / size, wy / size).rgb;
}

// The bank's colour at `v`: the textures' directional mean (`blend` the second texture's share) structured by the
// window's triplanar brightness. `dx` and `dy` the world's screen gradients, `ax` and `ay` texture 0's uv gradients
// and `bx` and `by` texture 1's (all taken in uniform control flow by the caller). No derivative calls inside, so it
// may run in the steep branch alone.
fn terrain_bank(
    v: VertexOut, blend: f32, dx: vec3<f32>, dy: vec3<f32>, ax: vec2<f32>, ay: vec2<f32>, bx: vec2<f32>, by: vec2<f32>
) -> vec3<f32> {
    let window = mode.detail;
    var low = bank_low(texture0, v.uv0, ax, ay);
    if (second_draw()) {
        low = mix(low, bank_low(texture1, v.uv1, bx, by), blend);
    }
    let face = cross(dx, dy);
    let n = abs(face) / max(length(face), 1e-12);
    var w = n * n * n * n;
    w = w / (w.x + w.y + w.z);
    let size = vec2<f32>(textureDimensions(texture0));
    let tile = window.z * size.x / TERRAIN_DENSITY;
    let coarse = exp2(TERRAIN_BANK_MEAN);
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    var fine = 0.0;
    var mean = 0.0;
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
            let uv = window.xy + (1.0 - abs(1.0 - 2.0 * fract(0.5 * q))) * window.zw;
            gx *= window.zw;
            gy *= window.zw;
            fine += w[i] * dot(textureSampleGrad(texture0, texture_sampler, uv, gx, gy).rgb, luma);
            mean += w[i] * dot(textureSampleGrad(texture0, texture_sampler, uv, gx * coarse, gy * coarse).rgb, luma);
            weight += w[i];
        }
    }
    fine /= max(weight, 1e-3);
    mean /= max(weight, 1e-3);
    let ratio = clamp(fine / max(mean, 0.02), TERRAIN_BANK_CLAMP.x, TERRAIN_BANK_CLAMP.y);
    return low * ratio;
}

// The ground texture's colour without its streaks: the atlases' banks were painted by a projection from above, so on a
// steep face the texture holds streaks down the slope. There it is read `TERRAIN_EVEN` levels blurrier, an even
// colour that `terrain_detail` fills in.
const TERRAIN_EVEN: f32 = 1.0;

// `blur` levels blurrier than the footprint says.
fn terrain_even(t: texture_2d<f32>, uv: vec2<f32>, level: f32, blur: f32) -> vec4<f32> {
    return textureSampleLevel(t, texture_sampler, uv, level + blur);
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
