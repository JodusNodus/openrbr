// The windscreen's drops, flakes and bugs (`glass/gpu.rs`): one instance each, a quad on the glass. Water drops are
// lenses: they show the frame behind them upside down and squeezed (sampled from the resolved frame about the drop's
// centre, mirrored), darker at the rim, with a highlight. Flakes and bugs are opaque splats with ragged edges, lit by
// the stage's light. Kind 4 is a patch of the wet film: the frame behind it blurred, streaked along the airflow. Ours.

struct Frame {
    view_projection: mat4x4<f32>,
    // width, height, 1 / width, 1 / height.
    screen: vec4<f32>,
    // The light on the glass: rgb.
    light: vec4<f32>,
    // At night (renderer addition, `Atmosphere::night`): rgb the headlights' glow on the glass, w 1 (`night_light`).
    night: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;

// Set for the HDR scene (`Enhance::hdr`, renderer addition): the target holds linear colour, which the post pass
// tone-maps, so the light's display colour is linearised (the frame seen through a drop already is linear) as `graded` in `shader.wgsl` does.
override HDR: bool = false;

fn linear(colour: vec3<f32>) -> vec3<f32> {
    return select(colour, pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2)), HDR);
}
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;

struct Instance {
    @location(0) centre: vec3<f32>,
    @location(1) kind: f32,
    @location(2) axis_u: vec3<f32>,
    @location(3) seed: f32,
    @location(4) axis_v: vec3<f32>,
    @location(5) alpha: f32,
    @location(6) color: vec4<f32>,
};

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    // The drop's centre on screen (0 to 1, y down) and its radius there, in the same units.
    @location(1) @interpolate(flat) centre: vec3<f32>,
    @location(2) @interpolate(flat) kind: i32,
    @location(3) @interpolate(flat) seed: f32,
    @location(4) @interpolate(flat) alpha: f32,
    @location(5) @interpolate(flat) color: vec4<f32>,
    // The quad's axes on screen (aspect-corrected, unit): across (xy) and up the glass (zw).
    @location(6) @interpolate(flat) axes: vec4<f32>,
};

fn to_screen(clip: vec4<f32>) -> vec2<f32> {
    let ndc = clip.xy / clip.w;
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

@vertex
fn vs_main(@builtin(vertex_index) corner: u32, instance: Instance) -> Out {
    let x = f32(corner & 1u) * 2.0 - 1.0;
    let y = f32((corner >> 1u) & 1u) * 2.0 - 1.0;
    let world = instance.centre + instance.axis_u * x + instance.axis_v * y;
    var out: Out;
    out.position = frame.view_projection * vec4<f32>(world, 1.0);
    out.local = vec2<f32>(x, y);
    let centre = frame.view_projection * vec4<f32>(instance.centre, 1.0);
    let edge = frame.view_projection * vec4<f32>(instance.centre + instance.axis_u, 1.0);
    let c = to_screen(centre);
    out.centre = vec3<f32>(c, length((to_screen(edge) - c) * frame.screen.xy) * frame.screen.w);
    let aspect = vec2<f32>(frame.screen.x * frame.screen.w, 1.0);
    let up = to_screen(frame.view_projection * vec4<f32>(instance.centre + instance.axis_v, 1.0));
    let along = normalize((up - c) * aspect + vec2<f32>(0.0, 1e-6));
    out.axes = vec4<f32>(along.y, -along.x, along);
    out.kind = i32(instance.kind + 0.5);
    out.seed = instance.seed;
    out.alpha = instance.alpha;
    out.color = instance.color;
    return out;
}

// A ragged edge: the radius at an angle, 1 give or take `amount`.
fn ragged(local: vec2<f32>, seed: f32, amount: f32) -> f32 {
    let a = atan2(local.y, local.x);
    let k = seed * 37.0;
    return 1.0 - amount * (0.5 + 0.25 * sin(a * 3.0 + k) + 0.15 * sin(a * 7.0 + k * 1.7) + 0.1 * sin(a * 13.0 + k * 2.3));
}

// How far the lens looks out, in drop radii: the scene seen through a drop spans this much more than the drop.
const LENS: f32 = 6.0;

// At night a drop or flake is lit by what lies in front of the car rather than by the dark sky: the frame's light
// around it (the mean of a ring of samples `NIGHT_REACH` of the screen out, so drops over the beam-lit road glow and
// those against the dark trees stay faint), and the headlights' glow scattered back (`Frame::night`).
const NIGHT_REACH: f32 = 0.12;

fn night_light(centre: vec2<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var k = 0; k < 8; k++) {
        let a = f32(k) * 0.7853982;
        let at = centre + vec2<f32>(cos(a), sin(a)) * NIGHT_REACH * vec2<f32>(frame.screen.y * frame.screen.z, 1.0);
        sum += textureSampleLevel(scene, scene_sampler, clamp(at, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
    }
    return sum / 8.0;
}

// The wet film (ours): how far its blur reaches, in screen heights, its taps (a sunflower disc, so that the image
// is softened rather than doubled) and its most cover, on the thickest film.
const FILM_BLUR: f32 = 0.005;
const FILM_TAPS: i32 = 12;
const FILM_COVER: f32 = 0.45;

// A drop this many pixels in radius and more shows its rim and highlight in full; below `DETAIL.x` none (a drop a pixel
// or two across reads as a speck of light, not a lens).
const DETAIL: vec2<f32> = vec2<f32>(1.5, 5.0);

// The most of a drop's lower rim that shows the sky, and the sky's light over the light on the glass.
const DROP_SKY: f32 = 0.5;
const DROP_SKY_LIGHT: f32 = 1.2;

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let r = length(in.local);
    if (in.kind == 4) {
        // A cell of wet film: the frame behind it softened, as thick as its corners' film (`color`) blended across
        // it. No sheen: added light lifted the dark trim behind the glass's edges to grey.
        let t = in.local * 0.5 + 0.5;
        let film = mix(mix(in.color.x, in.color.y, t.x), mix(in.color.z, in.color.w, t.x), t.y);
        let at = in.position.xy * frame.screen.zw;
        let to_uv = vec2<f32>(frame.screen.y * frame.screen.z, 1.0) * FILM_BLUR;
        var sum = vec3<f32>(0.0);
        for (var k = 0; k < FILM_TAPS; k++) {
            let a = f32(k) * 2.3999632;
            let d = sqrt((f32(k) + 0.5) / f32(FILM_TAPS));
            // Streaked along the airflow (`seed`: the streak's length over the blur's width, up the pane, or across it
            // when negative).
            let stretch = select(vec2<f32>(1.0, in.seed), vec2<f32>(-in.seed, 1.0), in.seed < 0.0);
            let offset = (in.axes.xy * cos(a) * stretch.x + in.axes.zw * sin(a) * stretch.y) * d * to_uv;
            sum += textureSampleLevel(scene, scene_sampler, clamp(at + offset, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
        }
        let colour = sum / f32(FILM_TAPS);
        let alpha = film * FILM_COVER;
        return vec4<f32>(colour * alpha, alpha);
    }
    if (in.kind == 0) {
        if (r > 1.0) {
            discard;
        }
        let nz = sqrt(max(1.0 - r * r, 0.0));
        // Through the lens: the frame about the drop, mirrored through its centre.
        let aspect = vec2<f32>(frame.screen.y * frame.screen.z, 1.0);
        let uv = in.centre.xy - in.local * vec2<f32>(1.0, -1.0) * in.centre.z * LENS * aspect;
        var through = textureSampleLevel(scene, scene_sampler, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
        // Darker toward the rim, where the lens bends the light away; a sky highlight up and to the left. A drop of a
        // pixel or two shows neither (`DETAIL`).
        let detail = smoothstep(DETAIL.x, DETAIL.y, in.centre.z * frame.screen.y);
        through = through * mix(1.0, (0.2 + 0.8 * sqrt(nz)) * 1.15, detail);
        // A drop's lens takes in a wide field, upside down: a crescent along its lower rim shows the sky above the car.
        let crescent = smoothstep(-0.2, -0.85, in.local.y) * smoothstep(0.4, 0.85, r);
        through = mix(through, linear(frame.light.rgb) * DROP_SKY_LIGHT, crescent * DROP_SKY * detail * (1.0 - frame.night.w));
        let n = normalize(vec3<f32>(in.local.x, in.local.y, nz * 1.2));
        let h = normalize(vec3<f32>(-0.35, 0.55, 1.0));
        let shine = (pow(max(dot(n, h), 0.0), 60.0) * 0.7 + pow(max(dot(n, h * vec3<f32>(-1.0, -1.0, 1.0)), 0.0), 8.0) * 0.08) * frame.light.rgb * detail;
        let rim = smoothstep(0.7, 1.0, r) * 0.45 * detail;
        var colour = through * (1.0 - rim) + linear(shine);
        if (frame.night.w != 0.0) {
            // The rim and the highlight catch what the drop sees (its lens's light: a drop over the lit road glows, one
            // against the dark trees stays dark) and a little of the headlights' glow.
            let caught = (smoothstep(0.5, 0.95, r) + pow(max(dot(n, h), 0.0), 60.0)) * detail;
            colour += (through * 1.5 + linear(frame.night.rgb) * 0.3) * caught;
        }
        let alpha = (1.0 - smoothstep(0.85, 1.0, r)) * in.alpha;
        return vec4<f32>(colour * alpha, alpha);
    }
    let edge = ragged(in.local, in.seed, select(0.35, 0.25, in.kind == 1));
    if (r > edge) {
        discard;
    }
    let soft = 1.0 - smoothstep(edge * 0.75, edge, r);
    var colour = in.color.rgb * frame.light.rgb;
    if (in.kind == 2) {
        // A bug: a darker core.
        colour = colour * (0.6 + 0.4 * smoothstep(0.0, 0.6, r));
    }
    var lit = linear(colour);
    if (frame.night.w != 0.0) {
        lit += in.color.rgb * (night_light(in.centre.xy) * 0.8 + linear(frame.night.rgb) * 0.6);
    }
    let alpha = soft * in.alpha * in.color.a;
    return vec4<f32>(lit * alpha, alpha);
}
