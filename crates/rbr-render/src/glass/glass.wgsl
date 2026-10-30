// The windscreen's drops, flakes and bugs (`glass/gpu.rs`): one instance each, a quad on the glass. Water drops are
// lenses: they show the frame behind them upside down and squeezed (sampled from the resolved frame about the drop's
// centre, mirrored), darker at the rim, with a highlight. Flakes and bugs are opaque splats with ragged edges, lit by
// the stage's light. Kind 4 is a patch of the wet film: the frame behind it blurred, streaked along the airflow. Kind 5
// is mud seen from inside: its shaded side with the outside's light through its thin rim. Kind 6 is a cell of the pane
// itself seen from inside, drawn first: its reflection loss, dust, specks and wiper scratches (`pane`). Ours.

struct Frame {
    view_projection: mat4x4<f32>,
    // width, height, 1 / width, 1 / height.
    screen: vec4<f32>,
    // The light on the glass: rgb.
    light: vec4<f32>,
    // At night (renderer addition, `Atmosphere::night`): rgb the headlights' glow on the glass, w 1 (`night_light`).
    night: vec4<f32>,
    // x 1 seen from outside: the film's wet layer (`wet_layer`).
    outside: vec4<f32>,
    // The eye, stage space.
    eye: vec4<f32>,
    // The way to the sun, w 1 by day; its light on the glass (display level, rgb).
    sun: vec4<f32>,
    sun_light: vec4<f32>,
    // The windscreen's wipers (`gpu::Pass::set_wipers`): two rows each, the pivot (chart metres) and reach, then the
    // park angle, the signed sweep and 1 for a wiper.
    wipers: array<vec4<f32>, 4>,
    // The sun's first cascade, and x 1 with the cascades.
    sun_matrix: mat4x4<f32>,
    shadow: vec4<f32>,
};

@group(0) @binding(0) var<uniform> frame: Frame;

// The target holds linear colour, which the post pass tone-maps, so the light's display colour is linearised (the frame
// seen through a drop already is linear) as `graded` in `shader.wgsl` does.
fn linear(colour: vec3<f32>) -> vec3<f32> {
    return pow(max(colour, vec3<f32>(0.0)), vec3<f32>(2.2));
}
@group(0) @binding(1) var scene: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;
@group(0) @binding(3) var sun_map: texture_depth_2d_array;
@group(0) @binding(4) var sun_sampler: sampler_comparison;
// The mirrors' images and views (`mirrors.wgsl`); the last is the windscreen's, the cabin reflected in it
// (`renderer::mirrors`).
struct MirrorView {
    view_projection: mat4x4<f32>,
    scale: vec4<f32>,
}

struct MirrorViews {
    views: array<MirrorView, 5>,
}

@group(0) @binding(5) var mirror_images: texture_2d_array<f32>;
@group(0) @binding(6) var<uniform> mirror_views: MirrorViews;
const WINDSCREEN_MIRROR: u32 = 4u;

struct Instance {
    @location(0) centre: vec3<f32>,
    @location(1) kind: f32,
    @location(2) axis_u: vec3<f32>,
    @location(3) seed: f32,
    @location(4) axis_v: vec3<f32>,
    @location(5) alpha: f32,
    @location(6) color: vec4<f32>,
    @location(7) normal: vec4<f32>,
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
    // Stage space: the point, and the quad's axes (unit).
    @location(7) world: vec3<f32>,
    @location(8) @interpolate(flat) axis_u: vec3<f32>,
    @location(9) @interpolate(flat) axis_v: vec3<f32>,
    @location(10) @interpolate(flat) normal: vec3<f32>,
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
    out.world = world;
    out.axis_u = normalize(instance.axis_u);
    out.axis_v = normalize(instance.axis_v);
    out.normal = instance.normal.xyz;
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

// The film's wet layer seen from outside (ours): at a distance its drops and beads are under a pixel, and what the
// eye sees of a wet pane is a soft mottled sheen, streaked up the glass. A value noise over the pane's cells (the
// cell's index and place in it), 3 cells and 1 cell across, stretched `WET_STREAK` along the pane's height, brightens
// the pane by up to `WET_LIGHT` of the glass's light, by the film; it fades out as a cell grows from `WET_CELL.x` to
// `WET_CELL.y` pixels, where the drops resolve and draw themselves.
const WET_LIGHT: f32 = 0.4;
const WET_STREAK: f32 = 3.0;
const WET_CELL: vec2<f32> = vec2<f32>(14.0, 30.0);
// The film's cells across a pane (`model::FILM_CELLS`).
const FILM_COLUMNS: f32 = 48.0;

fn wet_noise(p: vec2<f32>, pane: f32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash4(u32(i.x + 64.0) * 131u + u32(i.y + 64.0) * 7919u + u32(pane) * 104729u).x;
    let b = hash4(u32(i.x + 65.0) * 131u + u32(i.y + 64.0) * 7919u + u32(pane) * 104729u).x;
    let c = hash4(u32(i.x + 64.0) * 131u + u32(i.y + 65.0) * 7919u + u32(pane) * 104729u).x;
    let d = hash4(u32(i.x + 65.0) * 131u + u32(i.y + 65.0) * 7919u + u32(pane) * 104729u).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// The layer over this pixel of a film cell: premultiplied light.
fn wet_layer(in: Out, film: f32) -> vec4<f32> {
    let cell_px = in.centre.z * 2.0 * frame.screen.y;
    let fade = 1.0 - smoothstep(WET_CELL.x, WET_CELL.y, cell_px);
    if (frame.outside.x == 0.0 || fade <= 0.0) {
        return vec4<f32>(0.0);
    }
    let index = u32(in.alpha);
    let t = in.local * 0.5 + 0.5;
    let cell = vec2<f32>(f32(index % u32(FILM_COLUMNS)), f32(index / u32(FILM_COLUMNS))) + t;
    let pane = select(0.0, 1.0, in.seed < 0.0);
    let p = vec2<f32>(cell.x, cell.y / WET_STREAK);
    let mottle = 0.65 * wet_noise(p / 3.0, pane) + 0.35 * wet_noise(vec2<f32>(cell.x, cell.y / 2.0), pane + 2.0);
    let a = WET_LIGHT * film * smoothstep(0.4, 0.8, mottle) * fade;
    return vec4<f32>(linear(frame.light.rgb) * a, a);
}

// A drop this many pixels in radius and more shows its rim and highlight in full; below `DETAIL.x` none (a drop a pixel
// or two across reads as a speck of light, not a lens).
const DETAIL: vec2<f32> = vec2<f32>(1.5, 5.0);

// The most of a drop's lower rim that shows the sky, and the sky's light over the light on the glass.
const DROP_SKY: f32 = 0.5;
const DROP_SKY_LIGHT: f32 = 1.2;

// The beads (ours): the small drops the film holds, too many to be drops of their own. Each film cell is `BEADS`
// squares a side, each with a bead at a hashed place when a hash is under the film's thickness, of a hashed size up to
// `BEAD_SIZE` of the square; drawn as small lenses, like the drops.
const BEADS: f32 = 5.0;
const BEAD_SIZE: vec2<f32> = vec2<f32>(0.18, 0.45);

// Four numbers in [0, 1) from `n` (PCG steps).
fn hash4(n: u32) -> vec4<f32> {
    var v = vec4<u32>(n, n ^ 0x9e3779b9u, n ^ 0x85ebca6bu, n ^ 0xc2b2ae35u) * 747796405u + 2891336453u;
    v = ((v >> ((v >> vec4<u32>(28u)) + 4u)) ^ v) * 277803737u;
    v = (v >> vec4<u32>(22u)) ^ v;
    return vec4<f32>(v >> vec4<u32>(8u)) / 16777216.0;
}

// The bead over this pixel of a film cell, premultiplied; `per_pixel` the cell's local units per pixel (x and y).
fn beads(in: Out, film: f32, per_pixel: mat2x2<f32>) -> vec4<f32> {
    let grid = (in.local * 0.5 + 0.5) * BEADS;
    let square = floor(grid);
    let pane = select(0u, 1u, in.seed < 0.0);
    let h = hash4(((u32(in.alpha) * 2u + pane) * 64u + u32(square.y) * 8u + u32(square.x)));
    if (h.x >= film) {
        return vec4<f32>(0.0);
    }
    let radius = mix(BEAD_SIZE.x, BEAD_SIZE.y, h.y);
    let centre = square + radius + (1.0 - 2.0 * radius) * h.zw;
    // The pixel's offset from the bead's centre, in pixels (the inverse of the local units per pixel).
    let local = (grid - centre) * (2.0 / BEADS);
    let to_pixels = inverse2(per_pixel);
    let offset = to_pixels * local;
    let size = length(to_pixels * vec2<f32>(radius * 2.0 / BEADS, 0.0));
    let r = length(grid - centre) / radius;
    let cover = 1.0 - smoothstep(1.0 - 0.7 / max(size, 0.7), 1.0, r);
    if (cover <= 0.0) {
        return vec4<f32>(0.0);
    }
    let nz = sqrt(max(1.0 - r * r, 0.0));
    let at = (in.position.xy - offset * (1.0 + LENS)) * frame.screen.zw;
    var through = textureSampleLevel(scene, scene_sampler, clamp(at, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
    let detail = smoothstep(DETAIL.x * 0.5, DETAIL.y, size);
    through = through * mix(1.0, (0.2 + 0.8 * sqrt(nz)) * 1.15, detail);
    let crescent = smoothstep(0.2, 0.85, -local.y / max(radius * 2.0 / BEADS, 1e-6)) * smoothstep(0.4, 0.85, r);
    through = mix(through, linear(frame.light.rgb) * DROP_SKY_LIGHT, crescent * DROP_SKY * detail * (1.0 - frame.night.w));
    let n = normalize(vec3<f32>(local / max(radius * 2.0 / BEADS, 1e-6), nz * 1.2));
    let hl = normalize(vec3<f32>(-0.35, 0.55, 1.0));
    let shine = pow(max(dot(n, hl), 0.0), 60.0) * 0.7 * frame.light.rgb * detail;
    let colour = through * (1.0 - smoothstep(0.7, 1.0, r) * 0.45 * detail) + linear(shine);
    return vec4<f32>(colour * cover, cover);
}

fn inverse2(m: mat2x2<f32>) -> mat2x2<f32> {
    let d = m[0][0] * m[1][1] - m[1][0] * m[0][1];
    return mat2x2<f32>(m[1][1], -m[0][1], -m[1][0], m[0][0]) * (1.0 / select(d, 1e-9, abs(d) < 1e-12));
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    // The local units per pixel, before any branch (derivatives need uniform control flow).
    let per_pixel = mat2x2<f32>(dpdx(in.local), dpdy(in.local));
    let r = length(in.local);
    if (in.kind == 6) {
        return pane(in, per_pixel);
    }
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
        let bead = beads(in, film, per_pixel);
        let layer = wet_layer(in, film);
        let under = vec4<f32>(bead.rgb + colour * alpha * (1.0 - bead.a), bead.a + alpha * (1.0 - bead.a));
        return vec4<f32>(layer.rgb + under.rgb * (1.0 - layer.a), layer.a + under.a * (1.0 - layer.a));
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
    if (in.kind == 5) {
        return mud_inside(in, r / edge, soft);
    }
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

// Mud seen from inside (kind 5, ours): the side stuck to the glass in its shade (`color`, `glass::MUD_INSIDE` of the
// speck's), and the outside behind it through the mud, filtered by its hue to the power 1.5: `MUD_THROUGH.x` through
// the thick core, `MUD_THROUGH.y` through the thin rim (`rim` 0 at the centre to 1 at the ragged edge). So a speck is a
// dark silhouette that glows warm at its edge against the sunlit road or the sky, as backlit mud does, and stays dark
// against the dark trees.
const MUD_THROUGH: vec2<f32> = vec2<f32>(0.03, 0.45);

fn mud_inside(in: Out, rim: f32, soft: f32) -> vec4<f32> {
    let hue = in.color.rgb / max(max(in.color.r, max(in.color.g, in.color.b)), 1e-3);
    let behind = textureSampleLevel(scene, scene_sampler, in.position.xy * frame.screen.zw, 0.0).rgb;
    let through = behind * hue * sqrt(hue) * mix(MUD_THROUGH.x, MUD_THROUGH.y, smoothstep(0.3, 1.0, rim));
    var face = linear(in.color.rgb * frame.light.rgb);
    if (frame.night.w != 0.0) {
        face += in.color.rgb * (night_light(in.centre.xy) * 0.8 + linear(frame.night.rgb) * 0.6);
    }
    let alpha = soft * in.alpha * in.color.a;
    return vec4<f32>((face + through) * alpha, alpha);
}

// The pane seen from inside (kind 6, ours). A cell of the glass in `color` (its middle and size in the chart, metres),
// `alpha` (which corners are on the glass, faded between) and `seed` (0 the windscreen, 1 a side window). What it adds to the frame behind it and what it lets through, as
// premultiplied light over it (alpha the loss):
// - the light the two faces reflect away, by Schlick's Fresnel, so a steep pane passes less of the view;
// - a fine dust film that scatters the sunlight falling on the pane toward the eye, mostly forward (Henyey-Greenstein,
//   `DUST_G`), a veil about the sun when the car faces it, and the sky's light a little; patchy, thinner where the
//   wipers sweep;
// - dirt specks `SPECK_CELL` apart at most, a few tenths of a millimetre, dark against the view behind, bright when the
//   sun lights them from ahead;
// - dried water spots, `SPOT_CELL` apart at most, rings of what the water left, faint unless the sun lights them;
// - the wipers' fine scratches, arcs about their pivots in bands: a scratch lights where the sun's light, bent about
//   it, reaches the eye (the cone `dot(t, sun - view) = 0` about its tangent `t`), so the lit arcs line up through the
//   pivot toward the sun.
const PANE_F0: f32 = 0.04;
// The dust: the share of the sun's light on the pane it scatters toward the eye (a forward lobe, `DUST_G`, and a broad
// one, `DUST_BROAD` of it), and of the sky's; patches `DUST_PATCH` across, the swept areas at `DUST_SWEPT` of it.
const DUST: f32 = 0.01;
const DUST_G: f32 = 0.8;
const DUST_BROAD: f32 = 0.3;
const DUST_SKY: f32 = 0.008;
const DUST_PATCH: f32 = 0.12;
const DUST_SWEPT: f32 = 0.55;
// The side windows' dust against the windscreen's, and the most the slant lengthens the light's way through it.
const DUST_SIDE: f32 = 0.5;
const DUST_SLANT: f32 = 2.5;
const SPECK_CELL: f32 = 0.008;
const SPECK_CHANCE: f32 = 0.12;
const SPECK_RADIUS: vec2<f32> = vec2<f32>(0.0002, 0.0012);
const SPECK_COLOUR: vec3<f32> = vec3<f32>(0.62, 0.56, 0.47);
const SPECK_SHADE: f32 = 0.7;
// The scratches: rings `SCRATCH_PITCH` apart about the pivot, each there by `SCRATCH_CHANCE` with a strength of its
// own, in bands `SCRATCH_BAND` across; a scratch's light spread over the pixel it falls in (`SCRATCH_LINE` wide).
const SCRATCH: f32 = 8.0;
const SPOT_CELL: f32 = 0.014;
const SPOT_CHANCE: f32 = 0.06;
const SPOT_RADIUS: vec2<f32> = vec2<f32>(0.0015, 0.004);
const SPOT_LIGHT: f32 = 0.25;
const SCRATCH_WIDTH: f32 = 0.1;
const SCRATCH_PITCH: f32 = 0.0012;
const SCRATCH_CHANCE: f32 = 0.3;
const SCRATCH_LINE: f32 = 0.0002;
const SCRATCH_BAND: f32 = 0.02;

fn henyey_greenstein(cos_t: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (12.566371 * pow(max(1.0 + g2 - 2.0 * g * cos_t, 1e-4), 1.5));
}

// A value noise over the plane, `seed` apart per use.
fn value_noise(p: vec2<f32>, seed: u32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let u = f * f * (3.0 - 2.0 * f);
    let k = vec2<u32>(vec2<i32>(i) + vec2<i32>(4096));
    let a = hash4(k.x * 131u + k.y * 7919u + seed * 104729u).x;
    let b = hash4((k.x + 1u) * 131u + k.y * 7919u + seed * 104729u).x;
    let c = hash4(k.x * 131u + (k.y + 1u) * 7919u + seed * 104729u).x;
    let d = hash4((k.x + 1u) * 131u + (k.y + 1u) * 7919u + seed * 104729u).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// How far `at` (chart metres) lies inside wiper `i`'s swept area, 0 to 1 over `soft` metres; its radius from the
// pivot; and its tangent in the chart.
fn wiped(at: vec2<f32>, i: i32, soft: f32) -> vec4<f32> {
    let w = frame.wipers[i * 2];
    let s = frame.wipers[i * 2 + 1];
    if (s.z == 0.0) {
        return vec4<f32>(0.0);
    }
    let d = at - w.xy;
    let r = length(d);
    let a = atan2(d.y, d.x) - s.x;
    let angle = (a - 6.2831853 * round(a / 6.2831853)) * sign(s.y);
    let reach = min(r - w.z, w.w - r);
    let turn = min(angle, abs(s.y) - angle) * r;
    let inside = smoothstep(0.0, soft, reach) * smoothstep(0.0, soft, turn);
    return vec4<f32>(inside, r, -d.y / max(r, 1e-4), d.x / max(r, 1e-4));
}

// How much of the sun reaches the glass at `world` past the stage (the car is not in the cascades): the first cascade,
// 3 x 3 taps `SUN_SPREAD` texels apart, so the trees' shade moves softly across the pane; 1 off the cascade or
// without it.
const SUN_SPREAD: f32 = 1.5;

fn sun_reach(world: vec3<f32>) -> f32 {
    if (frame.shadow.x == 0.0) {
        return 1.0;
    }
    let clip = frame.sun_matrix * vec4<f32>(world, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || clip.z > 1.0) {
        return 1.0;
    }
    let texel = SUN_SPREAD / f32(textureDimensions(sun_map).x);
    var lit = 0.0;
    for (var k = 0; k < 9; k++) {
        let offset = vec2<f32>(f32(k % 3) - 1.0, f32(k / 3) - 1.0) * texel;
        lit += textureSampleCompareLevel(sun_map, sun_sampler, uv + offset, 0, clip.z - 0.002);
    }
    return lit / 9.0;
}

fn pane(in: Out, per_pixel: mat2x2<f32>) -> vec4<f32> {
    let at = in.color.xy + in.local * in.color.zw * 0.5;
    let on = vec4<f32>(vec4<u32>(u32(in.alpha)) & vec4<u32>(1u, 2u, 4u, 8u)) / vec4<f32>(1.0, 2.0, 4.0, 8.0);
    let t = in.local * 0.5 + 0.5;
    let glass = smoothstep(0.6, 1.0, mix(mix(on.x, on.y, t.x), mix(on.z, on.w, t.x), t.y));
    if (glass <= 0.0) {
        discard;
    }
    // A pixel's size on the glass, metres.
    let pixel = max(length(per_pixel[0] * in.color.zw * 0.5), length(per_pixel[1] * in.color.zw * 0.5));
    var n = in.normal;
    let view = normalize(in.world - frame.eye.xyz);
    n = select(n, -n, dot(n, view) < 0.0);
    let cos_v = max(dot(view, n), 0.05);
    let fresnel = PANE_F0 + (1.0 - PANE_F0) * pow(1.0 - cos_v, 5.0);
    var through = (1.0 - fresnel) * (1.0 - fresnel);
    let sun = frame.sun.xyz;
    let sun_on = max(dot(sun, n), 0.0) * frame.sun.w * sun_reach(in.world + n * 0.05);
    let sun_light = linear(frame.sun_light.rgb) * sun_on;
    let sky = linear(frame.light.rgb);
    let cos_t = dot(sun, view);

    // The wipers' areas: cleaner, and scratched.
    var swept = 0.0;
    var scratch = vec3<f32>(0.0);
    if (in.seed == 0.0) {
        for (var i = 0; i < 2; i++) {
            let w = wiped(at, i, 0.1);
            swept = max(swept, w.x);
            if (w.x > 0.0) {
                // The tangent in stage space, and how near the cone the sun's light through it takes.
                let t = normalize(in.axis_u * w.z + in.axis_v * w.w);
                let off = dot(t, sun - view) / SCRATCH_WIDTH;
                let band = value_noise(vec2<f32>(w.y / SCRATCH_BAND, f32(i) * 7.0), 11u);
                let ring = w.y / SCRATCH_PITCH;
                let h = hash4(u32(ring) * 2654435761u + u32(i) * 97u);
                let d = abs(fract(ring) - 0.5) * SCRATCH_PITCH;
                let line = select(0.0, h.y, h.x < SCRATCH_CHANCE) * min(SCRATCH_LINE / pixel, 1.0)
                    * exp(-d * d / max(pixel * pixel * 0.5, 1e-12));
                // Finer than a pixel the rings average out.
                let mean = SCRATCH_CHANCE * 0.5 * SCRATCH_LINE / SCRATCH_PITCH;
                let fine = mix(line, mean, smoothstep(SCRATCH_PITCH * 0.5, SCRATCH_PITCH * 2.0, pixel));
                let ahead = (0.5 + 0.5 * cos_t) * (0.5 + 0.5 * cos_t);
                scratch += sun_light * SCRATCH * w.x * band * fine * exp(-off * off) * ahead / cos_v;
            }
        }
    }
    let patchy = 0.65 * value_noise(at / DUST_PATCH, 21u) + 0.35 * value_noise(at / (DUST_PATCH * 0.3), 22u);
    let dust = DUST * mix(0.4, 1.6, patchy) * mix(1.0, DUST_SWEPT, swept) * select(1.0, DUST_SIDE, in.seed != 0.0);
    let phase = mix(henyey_greenstein(cos_t, DUST_G), henyey_greenstein(cos_t, 0.1), DUST_BROAD) * 12.566371;
    var light = (sun_light * phase + sky * DUST_SKY / DUST) * dust * min(1.0 / cos_v, DUST_SLANT) + scratch;
    through *= 1.0 - dust * 0.5;

    // The specks: one at most per cell of `SPECK_CELL`, its cover of the pixel spread over the pixel's footprint.
    let cell = floor(at / SPECK_CELL);
    let key = vec2<u32>(vec2<i32>(cell) + vec2<i32>(8192));
    let h = hash4(key.x * 73856093u ^ key.y * 19349663u ^ u32(in.seed) * 83492791u);
    if (h.x < SPECK_CHANCE * mix(1.0, 0.4, swept)) {
        let radius = mix(SPECK_RADIUS.x, SPECK_RADIUS.y, h.y * h.y);
        let centre = (cell + 0.2 + 0.6 * h.zw) * SPECK_CELL;
        let d2 = dot(at - centre, at - centre);
        let spread = radius * radius + 0.36 * pixel * pixel;
        let cover = min(radius * radius / spread, 1.0) * exp(-d2 / spread);
        let lit = linear(SPECK_COLOUR) * (sun_light * (0.15 + henyey_greenstein(cos_t, 0.5) * 2.0) + sky * 0.25);
        light += lit * cover;
        through *= 1.0 - cover * SPECK_SHADE;
    }
    // The water spots: a ring, a thin film inside it.
    let spot_cell = floor(at / SPOT_CELL);
    let spot_key = vec2<u32>(vec2<i32>(spot_cell) + vec2<i32>(8192));
    let s = hash4(spot_key.x * 83492791u ^ spot_key.y * 2971215073u ^ u32(in.seed) * 19349663u);
    if (s.x < SPOT_CHANCE * mix(1.0, 0.3, swept)) {
        let radius = mix(SPOT_RADIUS.x, SPOT_RADIUS.y, s.y);
        let centre = (spot_cell + 0.3 + 0.4 * s.zw) * SPOT_CELL;
        let r = length(at - centre) / radius;
        let width = max(0.12, pixel / radius);
        let ring = exp(-(r - 1.0) * (r - 1.0) / (width * width)) * min(0.12 / width, 1.0);
        let film = (1.0 - smoothstep(0.8, 1.0, r)) * 0.15;
        let residue = ring + film;
        light += sun_light * henyey_greenstein(cos_t, 0.7) * SPOT_LIGHT * residue + sky * 0.02 * residue;
        through *= 1.0 - 0.08 * residue;
    }
    // The cabin reflected in the inner face, and in the outer seen through it.
    if (in.seed == 0.0) {
        light += fresnel * (1.0 + through) * reflected(in.world);
    }
    return vec4<f32>(light, 1.0 - through) * glass;
}

// The windscreen's mirror image at `world` on its pane (the image drawn reflected across the pane's mean plane, which
// the pane's cells lie in), linear; nothing without one.
fn reflected(world: vec3<f32>) -> vec3<f32> {
    let view = mirror_views.views[WINDSCREEN_MIRROR];
    if (view.scale.z == 0.0) {
        return vec3<f32>(0.0);
    }
    let clip = view.view_projection * vec4<f32>(world, 1.0);
    let ndc = clip.xy / max(clip.w, 1e-6);
    let uv = clamp(vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5), vec2<f32>(0.0), vec2<f32>(1.0)) * view.scale.xy;
    return textureSampleLevel(mirror_images, scene_sampler, uv, WINDSCREEN_MIRROR, 0.0).rgb;
}
