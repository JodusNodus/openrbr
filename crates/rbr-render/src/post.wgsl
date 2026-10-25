// HDR post pass (renderer addition, `post.rs`; not the game's look). The scene arrives as linear colour in a float
// target (`graded` in `shader.wgsl`). A bloom chain halves it (`fs_down`, the 13-tap filter of Jimenez 2014) and adds
// the levels back up (`fs_up`, a 3x3 tent); `cs_histogram` bins the scene's light and `cs_meter` sets the exposure
// from the histogram, eased over time as an eye adapts; `fs_composite` blends a little of the bloom in, exposes, and applies AgX in place of
// the renderer's tone curve.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;
@group(1) @binding(0) var bloom: texture_2d<f32>;

struct Exposure {
    // The exposure the composite applies.
    value: f32,
    // The luminance the eye is adapted to, cd/m2, eased as the exposure.
    adapting: f32,
}

// `cs_meter`'s settings (`post::Meter`).
struct Meter {
    // Seconds since the last frame's metering; 0 to set the exposure outright (the first frame, a jump in time).
    step: f32,
    snap: f32,
    // In the in-car view the depth (reversed) nearer than which the frame is the cabin; 0 outside the car.
    cabin_depth: f32,
    // The renderer's units per cd/m2 (`post::Conditions::pre_exposure`).
    pre_exposure: f32,
    // The weather's grade (`post::Conditions`): `x` the metered exposure's scale, `y` the saturation's.
    grade: vec4<f32>,
}

@group(1) @binding(1) var<storage, read> exposure: Exposure;
// The light shafts (`light_shafts.wgsl`), added with `SHAFTS` (a pipeline override, so the composite without them is
// built as before).
@group(1) @binding(2) var shafts_map: texture_2d<f32>;
// The frame's depth (reversed), which the shafts' depth (their alpha) is matched against (`shafts_at`).
@group(1) @binding(3) var composite_depth: texture_depth_multisampled_2d;
override SHAFTS: bool = false;

// `cs_histogram`'s and `cs_meter`'s, with the scene in `source`. `cs_meter` empties the histogram for the next frame.
// `meter` is the composite's too.
@group(1) @binding(2) var meter_depth: texture_depth_multisampled_2d;
@group(1) @binding(3) var<storage, read_write> meter_exposure: Exposure;
@group(1) @binding(4) var<uniform> meter: Meter;
@group(1) @binding(5) var<storage, read_write> meter_histogram: array<atomic<u32>, BINS>;

// How much of the frame the bloom replaces.
const BLOOM: f32 = 0.06;
// The bloom chain's levels (`post::LEVELS`); `fs_up` sums them.
const LEVELS: f32 = 6.0;
// The metering (`cs_histogram`, `cs_meter`): the scene's luminance (cd/m2, its light over the pre-exposure) on a grid
// of `METER_GRID` points binned by its log2 in `BINS` bins from `EV_LOW` over `EV_SPAN` stops (starlit ground to the
// sunlit sky); the mean of the bins between the `PERCENTILES` (the darkest shadows and the
// brightest highlights dropped) is the frame's light. In the in-car view the cabin counts `CABIN_WEIGHT` of the
// outside, so the exposure follows what is seen through the glass and a dark cabin lifts it only a little.
const METER_GRID: vec2<u32> = vec2<u32>(128u, 64u);
const BINS: u32 = 128u;
const EV_LOW: f32 = -14.0;
const EV_SPAN: f32 = 32.0;
const PERCENTILES: vec2<f32> = vec2<f32>(0.1, 0.9);
const WEIGHT: u32 = 5u;
const CABIN_WEIGHT: u32 = 1u;
// The exposure, as Unreal's and BeamNG's auto exposure: the metered light, back in luminance (over the pre-exposure),
// as EV100 (ISO 100, the calibration constant `CALIBRATION`), held within `EV100_RANGE`, is exposed to `KEY`, middle
// grey (18 percent before `AGX_SCALE`), shifted by the exposure compensation curve (`compensation`). The curve is Krawczyk, Myszkowski and Seidel's (2005) key value
// against the adapting luminance, relative to a sunny scene's (EV100 15): dim scenes are drawn darker, as a dim scene
// looks darker to the adapted eye (twilight 1 to 3 EV down, a moonlit night about 4.5).
const KEY: f32 = 0.18 / AGX_SCALE;
const CALIBRATION: f32 = 12.5;
const EV100_RANGE: vec2<f32> = vec2<f32>(-8.0, 18.0);
const SUNNY_LUMINANCE: f32 = 4096.0;
// Rod vision in the mesopic range: Cao, Pokorny, Smith and Zele's (2008) model of the rods' signal added to the cones'
// opponent channels, as Kirk and O'Brien (2011) tone map with it and Filament and Ghost of Tsushima (Patry 2021) apply
// it. `LMSR` takes linear Rec.709 light to the L, M and S cones' and the rods' responses (each receptor's curve over
// D65 and the Rec.709 primaries' reflectance spectra); `CONE_MAX` is the cones' maximal sensitivity (Cao et al.,
// Table 1); `ROD_INPUT` the rods' input strength to each (Kirk and O'Brien's k1, k1, k2); `ROD_SHIFT` the rods' signal
// through the KC pathway's opponent channels (scale 45, static saturation 10, surround 0.6, white ratio 0.139, L
// weight 0.6189) back to RGB. `ROD_RETINA` takes cd/m2 to the model's units: Filament's 380 (Patry's +11.4 EV) over
// exposed light, which at a moonlit night's exposure (EV100 about -3.5) is about 3000 over cd/m2.
const LMSR: mat4x3<f32> = mat4x3<f32>(
    vec3<f32>(7.696847, 18.424824, 2.068096),
    vec3<f32>(2.431137, 18.697937, 3.012463),
    vec3<f32>(0.289117, 1.401833, 13.792292),
    vec3<f32>(0.466386, 15.564362, 10.059963),
);
const CONE_MAX: vec3<f32> = vec3<f32>(0.63721, 0.39242, 1.6064);
const ROD_INPUT: vec3<f32> = vec3<f32>(0.2, 0.2, 0.3);
const ROD_SHIFT: mat3x3<f32> = mat3x3<f32>(
    vec3<f32>(0.319458, 0.058098, 0.659214),
    vec3<f32>(-0.342068, 0.239183, 0.654598),
    vec3<f32>(0.010507, -0.011333, 0.061863),
);
const ROD_RETINA: f32 = 3000.0;
// Local exposure (`fs_composite`; as Unreal's local exposure lowers the highlights' contrast): the eye adapts to each
// part of what it sees, a single exposure cannot, and the lamps' beam on the road at night is several stops over the
// dark the meter exposes for. Where the frame's broad light, the bloom chain's sum of blurs, is exposed more than
// `LOCAL_FROM` stops over `KEY`, `LOCAL_GIVE` of the excess is taken off the exposure there; the detail within it stays.
// A day's sky seldom reaches it.
const LOCAL_FROM: f32 = 2.0;
const LOCAL_GIVE: f32 = 0.6;
// How fast the exposure follows the frame, per second: faster down into a bright frame than up into a dark one.
const ADAPT_SPEED: vec2<f32> = vec2<f32>(3.0, 1.2);

// AgX (Sobotka; the polynomial fit of Wrensch 2023) on the exposed linear light, pre-scaled by `AGX_SCALE`, over
// `AGX_RANGE` stops about middle grey and given `AGX_PUNCH` contrast so that the display middle grey stays near where
// the plain renderer's curve puts it, then `SATURATION`. Highlights desaturate toward white rather than clipping per
// channel. The contrast follows the adapting light, as contrast looks higher in brighter light (the Stevens effect):
// a dim scene keeps Blender's range (-10 to +6.5), a sunny one's (`compensation` 1) tops out a stop lower, with more
// punch, so a day's brightest reach about 85 percent of white rather than 80 while the night's shadows hold.
const AGX_SCALE: f32 = 2.0;
const AGX_RANGE: vec3<f32> = vec3<f32>(-10.0, 6.5, 5.5);
const AGX_PUNCH: vec2<f32> = vec2<f32>(1.25, 1.6);
const SATURATION: f32 = 1.1;

fn agx_contrast(x: vec3<f32>) -> vec3<f32> {
    let x2 = x * x;
    let x4 = x2 * x2;
    return 15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.00232;
}

// `bright`: 0 for a dim scene's contrast, 1 for a sunny one's.
fn agx(linear: vec3<f32>, bright: f32) -> vec3<f32> {
    let inset = mat3x3<f32>(
        vec3<f32>(0.842479062253094, 0.0423282422610123, 0.0423756549057051),
        vec3<f32>(0.0784335999999992, 0.878468636469772, 0.0784336),
        vec3<f32>(0.0792237451477643, 0.0791661274605434, 0.879142973793104),
    );
    let outset = mat3x3<f32>(
        vec3<f32>(1.19687900512017, -0.0528968517574562, -0.0529716355144438),
        vec3<f32>(-0.0980208811401368, 1.15190312990417, -0.0980434501171241),
        vec3<f32>(-0.0990297440797205, -0.0989611768448433, 1.15107367264116),
    );
    let top = mix(AGX_RANGE.y, AGX_RANGE.z, bright);
    let stops = clamp(log2(max(inset * linear, vec3<f32>(1e-10)) / 0.18), vec3<f32>(AGX_RANGE.x), vec3<f32>(top));
    let encoded = (stops - AGX_RANGE.x) / (top - AGX_RANGE.x);
    let punch = mix(AGX_PUNCH.x, AGX_PUNCH.y, bright);
    let curved = pow(clamp(agx_contrast(encoded), vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(punch));
    return clamp(outset * curved, vec3<f32>(0.0), vec3<f32>(1.0));
}

struct Out {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0), uv);
}

fn tap(uv: vec2<f32>, offset: vec2<f32>) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    // Clamped, so that the sun's disc does not flood the chain.
    return min(textureSampleLevel(source, linear_sampler, uv + offset * texel, 0.0).rgb, vec3<f32>(32.0));
}

@fragment
fn fs_down(v: Out) -> @location(0) vec4<f32> {
    let a = tap(v.uv, vec2<f32>(-2.0, -2.0));
    let b = tap(v.uv, vec2<f32>(0.0, -2.0));
    let c = tap(v.uv, vec2<f32>(2.0, -2.0));
    let d = tap(v.uv, vec2<f32>(-2.0, 0.0));
    let e = tap(v.uv, vec2<f32>(0.0, 0.0));
    let f = tap(v.uv, vec2<f32>(2.0, 0.0));
    let g = tap(v.uv, vec2<f32>(-2.0, 2.0));
    let h = tap(v.uv, vec2<f32>(0.0, 2.0));
    let i = tap(v.uv, vec2<f32>(2.0, 2.0));
    let j = tap(v.uv, vec2<f32>(-1.0, -1.0));
    let k = tap(v.uv, vec2<f32>(1.0, -1.0));
    let l = tap(v.uv, vec2<f32>(-1.0, 1.0));
    let m = tap(v.uv, vec2<f32>(1.0, 1.0));
    let sum = e * 0.125 + (a + c + g + i) * 0.03125 + (b + d + f + h) * 0.0625 + (j + k + l + m) * 0.125;
    return vec4<f32>(sum, 1.0);
}

@fragment
fn fs_up(v: Out) -> @location(0) vec4<f32> {
    var sum = tap(v.uv, vec2<f32>(0.0, 0.0)) * 4.0;
    sum += (tap(v.uv, vec2<f32>(-1.0, 0.0)) + tap(v.uv, vec2<f32>(1.0, 0.0))) * 2.0;
    sum += (tap(v.uv, vec2<f32>(0.0, -1.0)) + tap(v.uv, vec2<f32>(0.0, 1.0))) * 2.0;
    sum += tap(v.uv, vec2<f32>(-1.0, -1.0)) + tap(v.uv, vec2<f32>(1.0, -1.0));
    sum += tap(v.uv, vec2<f32>(-1.0, 1.0)) + tap(v.uv, vec2<f32>(1.0, 1.0));
    return vec4<f32>(sum / 16.0, 1.0);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

var<workgroup> histogram: array<atomic<u32>, BINS>;

// Bins the scene's light (`source`) into `meter_histogram`: a workgroup for each 16x16 of the grid's points, each
// binning its own before adding them in.
@compute @workgroup_size(16, 16)
fn cs_histogram(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) index: u32) {
    if (index < BINS) {
        atomicStore(&histogram[index], 0u);
    }
    workgroupBarrier();
    if (all(id.xy < METER_GRID)) {
        let uv = (vec2<f32>(id.xy) + 0.5) / vec2<f32>(METER_GRID);
        let l = luminance(textureSampleLevel(source, linear_sampler, uv, 0.0).rgb) / max(meter.pre_exposure, 1e-12);
        let bin = u32(clamp((log2(max(l, 1e-6)) - EV_LOW) / EV_SPAN * f32(BINS), 0.0, f32(BINS - 1u)));
        let depth = textureLoad(meter_depth, vec2<i32>(uv * vec2<f32>(textureDimensions(meter_depth))), 0);
        let cabin = meter.cabin_depth > 0.0 && depth > meter.cabin_depth;
        atomicAdd(&histogram[bin], select(WEIGHT, CABIN_WEIGHT, cabin));
    }
    workgroupBarrier();
    if (index < BINS) {
        let count = atomicLoad(&histogram[index]);
        if (count != 0u) {
            atomicAdd(&meter_histogram[index], count);
        }
    }
}

var<workgroup> counts: array<u32, BINS>;

// The light `c` (cd/m2) with the rods' signal added (`LMSR`).
fn rod_vision(c: vec3<f32>) -> vec3<f32> {
    let q = max(c * ROD_RETINA, vec3<f32>(0.0)) * LMSR;
    let gain = inverseSqrt(1.0 + 0.33 / CONE_MAX * (q.xyz + ROD_INPUT * q.w));
    return c + ROD_SHIFT * (gain * q.w) / ROD_RETINA;
}

// The exposure compensation curve (`KEY`'s scale) for an adapting luminance `l`, cd/m2.
fn compensation(l: f32) -> f32 {
    return krawczyk_key(l) / krawczyk_key(SUNNY_LUMINANCE);
}

fn krawczyk_key(l: f32) -> f32 {
    return 1.03 - 2.0 / (2.0 + log2(l + 1.0) * 0.30103);
}

// The exposure from `cs_histogram`'s histogram, eased from the last frame's (`meter_exposure`).
@compute @workgroup_size(128)
fn cs_meter(@builtin(local_invocation_index) index: u32) {
    if (index < BINS) {
        counts[index] = atomicExchange(&meter_histogram[index], 0u);
    }
    workgroupBarrier();
    if (index != 0u) {
        return;
    }
    var total = 0.0;
    for (var bin = 0u; bin < BINS; bin++) {
        total += f32(counts[bin]);
    }
    // The mean log2 light of the weight between the percentiles.
    let range = PERCENTILES * total;
    var below = 0.0;
    var sum = 0.0;
    var counted = 0.0;
    for (var bin = 0u; bin < BINS; bin++) {
        let w = f32(counts[bin]);
        let kept = max(min(below + w, range.y) - max(below, range.x), 0.0);
        sum += kept * (EV_LOW + (f32(bin) + 0.5) / f32(BINS) * EV_SPAN);
        counted += kept;
        below += w;
    }
    let luminance = exp2(sum / max(counted, 1.0));
    let ev100 = clamp(log2(luminance * 100.0 / CALIBRATION), EV100_RANGE.x, EV100_RANGE.y);
    let held = exp2(ev100) * CALIBRATION / 100.0;
    let aim = KEY * compensation(held) / (held * meter.pre_exposure) * meter.grade.x;
    let last = meter_exposure.value;
    if (meter.snap != 0.0 || !(last > 0.0) || !(meter_exposure.adapting > 0.0)) {
        meter_exposure.value = aim;
        meter_exposure.adapting = held;
        return;
    }
    let speed = select(ADAPT_SPEED.y, ADAPT_SPEED.x, aim < last);
    let eased = 1.0 - exp(-meter.step * speed);
    meter_exposure.value = exp2(mix(log2(last), log2(aim), eased));
    meter_exposure.adapting = exp2(mix(log2(meter_exposure.adapting), log2(held), eased));
}

// The shafts' map (a quarter of the frame's size) at `uv`: its four texels around `uv` weighted bilinearly and by how
// near their depth (alpha: `fs_beams` keeps its texel's, the sun's shafts all the same) is to the frame's here, so that
// the lamps' beams stop at the edges of what stands in front of them.
fn shafts_at(uv: vec2<f32>) -> vec3<f32> {
    let size = vec2<i32>(textureDimensions(shafts_map));
    let p = uv * vec2<f32>(size) - 0.5;
    let base = vec2<i32>(floor(p));
    let f = fract(p);
    let a = textureLoad(shafts_map, clamp(base, vec2<i32>(0), size - 1), 0);
    let b = textureLoad(shafts_map, clamp(base + vec2<i32>(1, 0), vec2<i32>(0), size - 1), 0);
    let c = textureLoad(shafts_map, clamp(base + vec2<i32>(0, 1), vec2<i32>(0), size - 1), 0);
    let d = textureLoad(shafts_map, clamp(base + vec2<i32>(1, 1), vec2<i32>(0), size - 1), 0);
    var w = vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y);
    // Four texels of one depth (the sun's shafts, a cleared map) share the depth weight, which cancels: the plain
    // bilinear sum, without the frame's depth, the costly read.
    let texel_depth = vec4<f32>(a.a, b.a, c.a, d.a);
    if (any(texel_depth != vec4<f32>(a.a))) {
        let depth_size = vec2<i32>(textureDimensions(composite_depth));
        let depth = textureLoad(composite_depth, clamp(vec2<i32>(uv * vec2<f32>(depth_size)), vec2<i32>(0), depth_size - 1), 0);
        let off = abs(texel_depth - depth) / max(max(texel_depth, vec4<f32>(depth)), vec4<f32>(1e-6));
        w *= exp(-off * off * 100.0) + 1e-3;
    }
    let sum = a.rgb * w.x + b.rgb * w.y + c.rgb * w.z + d.rgb * w.w;
    return sum / max(w.x + w.y + w.z + w.w, 1e-6);
}

@fragment
fn fs_composite(v: Out) -> @location(0) vec4<f32> {
    let scene = textureSampleLevel(source, linear_sampler, v.uv, 0.0).rgb;
    let glow = textureSampleLevel(bloom, linear_sampler, v.uv, 0.0).rgb / LEVELS;
    var lit = mix(scene, glow, BLOOM);
    if (SHAFTS) {
        lit += shafts_at(v.uv);
    }
    // Rod vision (`rod_vision`) on the light in cd/m2, over what it adds to white at the adapting luminance: the
    // exposure meters the cones' light, the rods shift the hues and the light between colours.
    let pre = max(meter.pre_exposure, 1e-12);
    let adapting = max(exposure.adapting, 1e-6);
    let white = luminance(rod_vision(vec3<f32>(adapting))) / adapting;
    lit = rod_vision(lit / pre) * pre / white;
    // Local exposure (`LOCAL_FROM`).
    let broad = log2(max(luminance(glow) * exposure.value, 1e-9) / KEY);
    let linear = lit * exposure.value * exp2(-LOCAL_GIVE * max(broad - LOCAL_FROM, 0.0));
    let c = agx(max(linear, vec3<f32>(0.0)) * AGX_SCALE, compensation(adapting));
    let grey = dot(c, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(clamp(mix(vec3<f32>(grey), c, SATURATION * meter.grade.y), vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
