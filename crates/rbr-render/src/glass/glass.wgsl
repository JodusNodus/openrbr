// The windscreen's drops, flakes and bugs (`glass/gpu.rs`): one instance each, a quad on the glass. Water drops are
// lenses: they show the frame behind them upside down and squeezed (sampled from the resolved frame about the drop's
// centre, mirrored), darker at the rim, with a highlight. Flakes and bugs are opaque splats with ragged edges, lit by
// the stage's light. Kind 4 is a blotch of the wet film: the frame behind it blurred, streaked along the airflow. Kind 5
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
    // x 1 seen from outside: the film's wet layer (`wet_layer`); y how wet the glass has been, 0 to 1 (its dried
    // spots).
    outside: vec4<f32>,
    // The eye, stage space.
    eye: vec4<f32>,
    // The way to the sun, w 1 by day; its light on the glass (display level, rgb).
    sun: vec4<f32>,
    sun_light: vec4<f32>,
    // The windscreen's wipers (`gpu::Wipers`): two rows each, the pivot (chart metres) and reach, then the park angle,
    // the signed sweep and 1 for a wiper.
    wipers: array<vec4<f32>, 4>,
    // Their last strokes, newest first: the angle fraction each went from and to, the seconds since it began and
    // since it ended (0 while under way); none where x == y.
    strokes: array<vec4<f32>, 4>,
    // x the newest stroke's number, y how fast the film builds per second, z the most it holds.
    wiping: vec4<f32>,
    // The windscreen's film cells: the first's low corner and a cell's size, chart metres.
    film: vec4<f32>,
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
    @location(8) far: vec3<f32>,
    @location(9) tint: vec4<u32>,
    @location(10) edge: vec4<f32>,
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
    // What a pane cell's glass lets through of the light outside (its texture's sun strip), across the cell
    // (`pane_tint`).
    @location(11) @interpolate(flat) tint: vec4<u32>,
    // How far a pane cell's corners lie inside the glass's outline, over `PANE_EDGE` (low, right, top, far).
    @location(12) @interpolate(flat) edge: vec4<f32>,
    // The quad's two half axes on screen, pixels (y down): a pixel's offset from the centre is `u * local.x + v * local.y`.
    @location(13) @interpolate(flat) pixels: vec4<f32>,
    // Its half sizes in stage space, metres (a trail's ribbon, kind 7).
    @location(14) @interpolate(flat) metres: vec2<f32>,
};

fn to_screen(clip: vec4<f32>) -> vec2<f32> {
    let ndc = clip.xy / clip.w;
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

@vertex
fn vs_main(@builtin(vertex_index) corner: u32, instance: Instance) -> Out {
    let x = f32(corner & 1u) * 2.0 - 1.0;
    let y = f32((corner >> 1u) & 1u) * 2.0 - 1.0;
    var world = instance.centre + instance.axis_u * x + instance.axis_v * y;
    if (abs(instance.kind - 6.0) < 0.5) {
        // A pane cell: its low corner and the others' offsets from it, so each corner lands exactly where its
        // neighbours' does and the cells meet without cracks or pinholes.
        let low = select(vec3<f32>(0.0), instance.axis_u, corner == 1u);
        let high = select(instance.axis_v, instance.far, corner == 3u);
        world = instance.centre + select(low, high, corner >= 2u);
    }
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
    let su = (to_screen(edge) - c) * frame.screen.xy;
    let sv = (up - c) * frame.screen.xy;
    out.pixels = vec4<f32>(su, sv);
    out.metres = vec2<f32>(length(instance.axis_u), length(instance.axis_v));
    out.kind = i32(instance.kind + 0.5);
    out.seed = instance.seed;
    out.alpha = instance.alpha;
    out.color = instance.color;
    out.world = world;
    out.axis_u = normalize(instance.axis_u);
    out.axis_v = normalize(instance.axis_v);
    out.normal = instance.normal.xyz;
    out.tint = instance.tint;
    out.edge = instance.edge;
    return out;
}

// A ragged edge: the radius at an angle, 1 give or take `amount`.
fn ragged(local: vec2<f32>, seed: f32, amount: f32) -> f32 {
    let a = atan2(local.y, local.x);
    let k = seed * 37.0;
    return 1.0 - amount * (0.5 + 0.25 * sin(a * 3.0 + k) + 0.15 * sin(a * 7.0 + k * 1.7) + 0.1 * sin(a * 13.0 + k * 2.3));
}

// A drop's or bead's outline (ours: no water on glass lies round): how far out to its edge the point `local` of its
// unit disc lies, 1 at the edge. Its edge holds where it touched the glass (`ragged` by `amount`), and it is up to
// `DROP_STRETCH` longer one way than across, its own way by `seed`.
const DROP_STRETCH: f32 = 0.35;
// A drop's raggedness, from the small ones to those 6 pixels across and more, and a bead's.
const DROP_RAGGED: vec2<f32> = vec2<f32>(0.12, 0.28);
const BEAD_RAGGED: f32 = 0.15;

fn outline(local: vec2<f32>, seed: f32, amount: f32) -> f32 {
    let a = seed * 19.0;
    let way = vec2<f32>(cos(a), sin(a));
    let p = vec2<f32>(dot(local, way), (local.y * way.x - local.x * way.y) * (1.0 + DROP_STRETCH * fract(seed * 5.77)));
    return length(p) / ragged(p, seed, amount);
}

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

// The water lens (ours): what a drop, bead or ribbon of water on the glass shows. The frame behind it is seen
// mirrored through its centre, `mag` times as far out as the pixel's offset from it (a drop sees a wide field upside
// down), the more toward its rim where the surface turns away; the rim bends the view out of the eye, a dark outline
// that narrows to a line on the small ones; a tiny highlight where its crown faces the light. `q` is the pixel in the
// lens's unit disc (y up), `o` its offset from the centre in pixels (y down), `size` the lens's radius in pixels,
// `centre` its middle on the screen (0 to 1). Linear colour.
const LENS_MAG: f32 = 5.0;
const DROP_RIM: f32 = 0.55;
// How much of itself a lens lays over what is already drawn behind it (the glass's own haze included) at its middle:
// clear water, it lets the rest show through; at its rim, where it bends the view hardest, all of it.
const LENS_BODY: f32 = 0.85;
// The sky's light in a lens's lower part (`Frame::light`, linear, times `LENS_SKY`) at most `LENS_SKY_SHARE` of it,
// and its crescent's.
const LENS_SKY: f32 = 0.7;
const LENS_SKY_SHARE: f32 = 0.45;
const LENS_CRESCENT: f32 = 0.2;
// No two drops alike (ours): each catches `LENS_VARY` to all of that sky and crescent, its outline as dark or a quarter
// lighter, its highlight a little to either side, by its own number.
const LENS_VARY: f32 = 0.3;

// `weight` is how much of a lens a thing of water is: 1 a drop, less a thin ribbon (more of the view straight behind
// it, a fainter outline); `vary` its own number, 0 to 1. Premultiplied light over what lies behind it.
fn lens(q: vec2<f32>, o: vec2<f32>, look: vec2<f32>, size: f32, centre: vec2<f32>, mag: f32, spec: f32, weight: f32, vary: f32) -> vec4<f32> {
    let r = min(length(q), 1.0);
    let bend = mag * (1.0 + 0.6 * r * r);
    let uv = clamp(centre - look * bend * frame.screen.zw, vec2<f32>(0.0), vec2<f32>(1.0));
    // What it sees is softened over its field: four more taps spread over it.
    var through = textureSampleLevel(scene, scene_sampler, uv, 0.0).rgb;
    if (size > 2.0) {
        let spread = size * mag * 0.3 * frame.screen.zw;
        through = through * 0.4
            + 0.15 * (textureSampleLevel(scene, scene_sampler, clamp(uv + vec2<f32>(spread.x, spread.y), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb
            + textureSampleLevel(scene, scene_sampler, clamp(uv + vec2<f32>(-spread.x, spread.y), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb
            + textureSampleLevel(scene, scene_sampler, clamp(uv + vec2<f32>(spread.x, -spread.y), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb
            + textureSampleLevel(scene, scene_sampler, clamp(uv + vec2<f32>(-spread.x, -spread.y), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb);
    }
    // A drop of a pixel or two shows no outline or highlight (`DETAIL`).
    let detail = smoothstep(DETAIL.x * 0.5, DETAIL.y, size);
    // Upside down, its lower part looks up at the sky over the stage (in a cockpit the frame above it is the roof, so
    // the sky's light stands in): lighter below the sky's line, which curves up to its sides, with a bright crescent
    // inside its lower rim where that light gathers. The rim bends the view out of the eye: a dark outline, darker
    // along the top, which looks down.
    let sky = linear(frame.light.rgb) * LENS_SKY;
    let own = vec4<f32>(vary, fract(vary * 7.31), fract(vary * 13.7), fract(vary * 3.37));
    let below = (1.0 - smoothstep(-0.35, 0.0, q.y - 0.4 * q.x * q.x)) * weight * smoothstep(0.5, 1.5, size);
    through = mix(through, max(through, sky), below * LENS_SKY_SHARE * mix(LENS_VARY, 1.0, own.x));
    let rim = smoothstep(0.62, 1.0, r);
    let dark = min(mix(0.25, DROP_RIM, smoothstep(1.0, 3.0, size)) * weight * rim * (1.0 + 0.35 * clamp(q.y, -1.0, 1.0)), 0.7)
        * mix(0.75, 1.0, own.w);
    let crescent = smoothstep(0.45, 0.8, r) * (1.0 - smoothstep(0.8, 0.97, r)) * (1.0 - smoothstep(-0.8, -0.2, q.y));
    var colour = through * (1.0 - dark) + sky * (crescent * LENS_CRESCENT * mix(LENS_VARY, 1.0, own.y) * weight * detail);
    let nz = sqrt(max(1.0 - r * r, 0.0));
    let n = normalize(vec3<f32>(q, nz * 1.3 + 0.05));
    let h = normalize(vec3<f32>(-0.4 + 0.5 * (own.z - 0.5), 0.6, 1.0));
    var glint = linear(frame.light.rgb) * pow(max(dot(n, h), 0.0), 60.0) * spec * smoothstep(0.7, 2.5, size);
    if (frame.night.w != 0.0) {
        // At night the rim also catches the lamps the lens sees, and a little of the headlights' glow.
        glint += (through * 0.9 + linear(frame.night.rgb) * 0.15) * rim * detail * weight;
    }
    let alpha = mix(LENS_BODY, 1.0, rim) * mix(0.5, 1.0, weight) * mix(0.75, 1.0, detail);
    return vec4<f32>(colour * alpha + glint, alpha);
}

// The wipers' strokes (ours, `model::Stroke`): when a blade last passed over a point of the windscreen. Behind it the
// glass is clear, the film builds again from nothing at the rain's rate (`Frame::wiping`) and its beads land in a
// layout of their own; a thin smear dries off; ahead of it the water it gathers rides along its edge.
const NEVER: f32 = 1e6;

// Where chart point `at` lies about wiper `i`: its angle from the park as a share of the sweep, its radius, and 1
// within the blade's reach (0 off it or without the wiper).
fn about_wiper(at: vec2<f32>, i: i32) -> vec3<f32> {
    let w = frame.wipers[i * 2];
    let s = frame.wipers[i * 2 + 1];
    let d = at - w.xy;
    let r = length(d);
    let a = atan2(d.y, d.x) - s.x;
    let share = (a - 6.2831853 * round(a / 6.2831853)) * sign(s.y) / max(abs(s.y), 1e-3);
    return vec3<f32>(share, r, select(0.0, 1.0, s.z != 0.0 && r >= w.z && r <= w.w));
}

// When a blade last passed over chart point `at`: the seconds since (`NEVER` when none of the last strokes did), the
// stroke's number, the radius from its wiper's pivot and the place along its arc, metres.
fn last_wipe(at: vec2<f32>) -> vec4<f32> {
    var wipe = vec4<f32>(NEVER, -1.0, 0.0, 0.0);
    for (var i = 0; i < 2; i++) {
        let w = about_wiper(at, i);
        if (w.z == 0.0 || w.x < 0.0 || w.x > 1.0) {
            continue;
        }
        for (var k = 0; k < 4; k++) {
            let s = frame.strokes[k];
            if (s.x != s.y && w.x >= min(s.x, s.y) && w.x <= max(s.x, s.y)) {
                let since = mix(s.z, s.w, (w.x - s.x) / (s.y - s.x));
                if (since < wipe.x) {
                    wipe = vec4<f32>(since, frame.wiping.x - f32(k), w.y, w.x * abs(frame.wipers[i * 2 + 1].y) * w.y);
                }
                break;
            }
        }
    }
    return wipe;
}

// A film cell's chart point at `t` across it (0 to 1).
fn cell_point(in: Out, t: vec2<f32>) -> vec2<f32> {
    let index = u32(in.alpha);
    let cell = vec2<f32>(f32(index % u32(FILM_COLUMNS)), f32(index / u32(FILM_COLUMNS)));
    return frame.film.xy + (cell + t) * frame.film.zw;
}

// `last_wipe` at `t` across a film cell; none on a side window (no wipers; its streak, `seed`, is negative).
fn wipe_at(in: Out, t: vec2<f32>) -> vec4<f32> {
    if (in.seed < 0.0) {
        return vec4<f32>(NEVER, -1.0, 0.0, 0.0);
    }
    return last_wipe(cell_point(in, t));
}

// The film where the cells hold `film`: behind a blade, what the rain has built since (`wipe`).
fn film_after(film: f32, wipe: vec4<f32>) -> f32 {
    return select(film, min(frame.wiping.z, frame.wiping.y * wipe.x), wipe.x < NEVER);
}

// A chart vector (metres) in a film cell's screen pixels (y down).
fn chart_pixels(in: Out, c: vec2<f32>) -> vec2<f32> {
    return in.pixels.xy * (c.x * 2.0 / frame.film.z) + in.pixels.zw * (c.y * 2.0 / frame.film.w);
}

// The beads (ours): the small drops the film holds, too many to be drops of their own. Each film cell is `squares` a side
// in two layers, a fine one and a coarser one over it, each square with a bead at a hashed place when a hash is under
// the layer's share of the film's thickness (`bead_share`), of a hashed size from the layer's range (a share of the
// square); drawn as small lenses, like the drops. Behind a blade, each stroke lays its own (the hash takes the
// stroke's number). They gather in patches about `BEAD_PATCH` metres across, from `BEAD_PATCHY.x` to `.y` of that
// share (ours: the glass wets unevenly).
const BEADS_FINE: f32 = 5.0;
const BEAD_PATCH: f32 = 0.04;
const BEAD_PATCHY: vec2<f32> = vec2<f32>(0.25, 1.5);
const BEADS_COARSE: f32 = 2.0;
const BEAD_SIZE_FINE: vec2<f32> = vec2<f32>(0.18, 0.45);
const BEAD_SIZE_COARSE: vec2<f32> = vec2<f32>(0.2, 0.42);
const BEAD_FILL_FINE: f32 = 0.95;
const BEAD_FILL_COARSE: f32 = 0.45;
// The beads' share of their squares as the film builds, `1 - exp(-film BEAD_BUILD)`; behind a blade no more than
// land in the time since, `BEAD_LANDING` of the squares per unit of the film's rate a second (0.35 a second in heavy
// rain), so that continuous wipers keep the glass mostly clear.
const BEAD_BUILD: f32 = 2.5;
const BEAD_LANDING: f32 = 0.6;

fn bead_share(film: f32, wipe: vec4<f32>) -> f32 {
    let share = 1.0 - exp(-film_after(film, wipe) * BEAD_BUILD);
    return select(share, min(share, frame.wiping.y * wipe.x * BEAD_LANDING), wipe.x < NEVER);
}

// Four numbers in [0, 1) from `n` (PCG steps).
fn hash4(n: u32) -> vec4<f32> {
    var v = vec4<u32>(n, n ^ 0x9e3779b9u, n ^ 0x85ebca6bu, n ^ 0xc2b2ae35u) * 747796405u + 2891336453u;
    v = ((v >> ((v >> vec4<u32>(28u)) + 4u)) ^ v) * 277803737u;
    v = (v >> vec4<u32>(22u)) ^ v;
    return vec4<f32>(v >> vec4<u32>(8u)) / 16777216.0;
}

// The bead of one layer over this pixel of a film cell, premultiplied; `film` the cells' film here, `per_pixel` the
// cell's local units per pixel (x and y).
fn bead_layer(in: Out, film: f32, per_pixel: mat2x2<f32>, squares: f32, sizes: vec2<f32>, fill: f32, salt: u32) -> vec4<f32> {
    let grid = (in.local * 0.5 + 0.5) * squares;
    let square = floor(grid);
    let pane = select(0u, 1u, in.seed < 0.0);
    // Whole beads: the square's middle says when a blade last passed.
    let wipe = wipe_at(in, (square + 0.5) / squares);
    let key = ((u32(in.alpha) * 2u + pane) * 64u + u32(square.y) * 8u + u32(square.x)) * 3u + salt;
    let h = hash4(key ^ (u32(wipe.y + 1.0) * 0x9e3779b9u));
    let patchy = mix(BEAD_PATCHY.x, BEAD_PATCHY.y, value_noise(cell_point(in, (square + 0.5) / squares) / BEAD_PATCH, 52u));
    if (h.x >= bead_share(film, wipe) * fill * patchy) {
        return vec4<f32>(0.0);
    }
    let radius = mix(sizes.x, sizes.y, h.y * h.y);
    let centre = square + radius + (1.0 - 2.0 * radius) * h.zw;
    // The pixel's offset from the bead's centre, in pixels (the inverse of the local units per pixel).
    let to_pixels = inverse2(per_pixel);
    let local = (grid - centre) * (2.0 / squares);
    let offset = to_pixels * local;
    let size = length(to_pixels * vec2<f32>(radius * 2.0 / squares, 0.0));
    let vary = fract(h.z * 5.3 + h.w);
    let r = outline((grid - centre) / radius, vary, BEAD_RAGGED);
    let cover = 1.0 - smoothstep(1.0 - 0.7 / max(size, 0.7), 1.0, r);
    if (cover <= 0.0) {
        return vec4<f32>(0.0);
    }
    let q = vec2<f32>(offset.x, -offset.y) * (min(r, 1.0) / max(length(offset), 1e-3));
    return lens(q, offset, offset, size, (in.position.xy - offset) * frame.screen.zw, LENS_MAG, 0.9, 1.0, vary) * cover;
}

fn beads(in: Out, film: f32, per_pixel: mat2x2<f32>) -> vec4<f32> {
    let fine = bead_layer(in, film, per_pixel, BEADS_FINE, BEAD_SIZE_FINE, BEAD_FILL_FINE, 0u);
    let coarse = bead_layer(in, film, per_pixel, BEADS_COARSE, BEAD_SIZE_COARSE, BEAD_FILL_COARSE, 1u);
    return vec4<f32>(coarse.rgb + fine.rgb * (1.0 - coarse.a), coarse.a + fine.a * (1.0 - coarse.a));
}

// What a blade leaves behind it (ours): a thin smear in fine arcs about its pivot, broken along them, which dries in
// `SMEAR_LIFE` seconds; `SMEAR_PITCH` apart (metres). Its share of a pixel.
const SMEAR_LIFE: f32 = 0.5;
const SMEAR_PITCH: f32 = 0.002;

fn smear(wipe: vec4<f32>) -> f32 {
    if (wipe.x >= SMEAR_LIFE) {
        return 0.0;
    }
    let arcs = value_noise(vec2<f32>(wipe.z / SMEAR_PITCH, wipe.y * 3.1), 33u);
    let runs = value_noise(vec2<f32>(wipe.w / 0.05, wipe.z / 0.012), 34u);
    let left = 1.0 - wipe.x / SMEAR_LIFE;
    return smoothstep(0.55, 0.85, arcs) * smoothstep(0.25, 0.6, runs) * left * left;
}

// The water a blade pushes (ours): a ridge along its leading edge, `RIDGE_GAP` metres ahead of its line, as wide as
// `RIDGE_GROW` times the root of the water it has gathered over the stroke so far (the film times the arc swept, at
// this radius), within `RIDGE_WIDTH`, lumpy along the blade (`RIDGE_LUMP` metres): clear water, `RIDGE_WEIGHT` of a
// drop's lens. Its churned surface catches the sky's light in patches `RIDGE_CHURN` metres apart, up to `RIDGE_SHEEN`
// of it. Where a stroke ends it is left lying and breaks up over `RIDGE_LINGER` seconds, into the drops the blade
// leaves (`model::Glass::leave`). At 3 to 12 mm, a third of the sky's light and over 0.25 s it read as a bank of white
// foam where the blades turned.
const RIDGE_GAP: f32 = 0.001;
const RIDGE_GROW: f32 = 0.025;
const RIDGE_WIDTH: vec2<f32> = vec2<f32>(0.002, 0.007);
const RIDGE_LUMP: f32 = 0.02;
const RIDGE_LINGER: f32 = 0.1;
const RIDGE_MAG: f32 = 2.0;
const RIDGE_WEIGHT: f32 = 0.6;
const RIDGE_SHEEN: f32 = 0.08;
const RIDGE_CHURN: f32 = 0.006;

struct Ridge {
    // The place across it, -1 (the blade's side) to 1, along it (metres from the pivot), its width (metres) and how
    // much of it there is (0 none).
    across: f32,
    along: f32,
    width: f32,
    amount: f32,
    // The way ahead, chart.
    ahead: vec2<f32>,
}

fn ridge(at: vec2<f32>, film: f32) -> Ridge {
    var best = Ridge(0.0, 0.0, 0.0, 0.0, vec2<f32>(0.0));
    for (var i = 0; i < 2; i++) {
        let w = about_wiper(at, i);
        if (w.z == 0.0) {
            continue;
        }
        let sweep = frame.wipers[i * 2 + 1].y;
        for (var k = 0; k < 2; k++) {
            let s = frame.strokes[k];
            if (s.x == s.y || s.w > RIDGE_LINGER * 4.0) {
                continue;
            }
            let way = sign(s.y - s.x);
            let ahead = (w.x - s.y) * way * abs(sweep) * w.y;
            let gathered = film * abs(s.y - s.x) * abs(sweep) * w.y;
            let lumps = value_noise(vec2<f32>(w.y / RIDGE_LUMP, frame.wiping.x - f32(k)), 41u);
            let width = clamp(RIDGE_GROW * sqrt(gathered), RIDGE_WIDTH.x, RIDGE_WIDTH.y) * (0.5 + lumps);
            let x = (ahead - RIDGE_GAP) / width;
            // Left lying, it breaks into lumps as it runs off.
            let left = exp(-s.w / RIDGE_LINGER);
            let amount = smoothstep(0.0003, 0.003, gathered) * left * smoothstep(0.6 * (1.0 - left), 0.6 * (1.0 - left) + 0.15, lumps);
            if (x >= 0.0 && x <= 1.0 && amount > best.amount) {
                let d = (at - frame.wipers[i * 2].xy) / w.y;
                best = Ridge(x * 2.0 - 1.0, w.y, width, amount, vec2<f32>(-d.y, d.x) * sign(sweep) * way);
            }
        }
    }
    return best;
}

// The ridge over this pixel of a film cell at `t` across it, where the film ahead of the blade is `film`: a
// cylinder of water lying along the blade, premultiplied.
fn blade_water(in: Out, t: vec2<f32>, film: f32) -> vec4<f32> {
    if (in.seed < 0.0) {
        return vec4<f32>(0.0);
    }
    let r = ridge(cell_point(in, t), film);
    if (r.amount <= 0.0) {
        return vec4<f32>(0.0);
    }
    let half_width = chart_pixels(in, r.ahead * r.width * 0.5);
    let size = max(length(half_width), 0.5);
    let o = half_width * r.across;
    let q = vec2<f32>(o.x, -o.y) / size;
    let edge = 1.0 - smoothstep(1.0 - 0.8 / size, 1.0, abs(r.across));
    let churn = smoothstep(0.4, 0.85, value_noise(vec2<f32>(r.along / RIDGE_CHURN, r.across * 1.5), 43u));
    let sheen = linear(frame.light.rgb) * (RIDGE_SHEEN * churn * (1.0 - r.across * r.across));
    let water = lens(q, o, o, size, (in.position.xy - o) * frame.screen.zw, RIDGE_MAG, 0.5, RIDGE_WEIGHT, churn);
    return (water + vec4<f32>(sheen, 0.0)) * (edge * r.amount);
}

// A cell of wet film (kind 4): the frame behind it seen through a wavy sheet of water, as thick as its corners' film
// (`color`) blended across it, or behind a blade as the rain has built it since; patchy, the thin film breaking up.
// The sheet's waves shift the view a few pixels and streak it along the airflow (`seed`: the streak's length over the
// blur's width, up the pane, or across it when negative). No sheen: added light lifted the dark trim behind the
// glass's edges to grey. A blade's smear is a thin sheet too, and its ridge lies over the rest.
const FILM_WARP: f32 = 2.0;
const FILM_BLUR: f32 = 0.0022;
const FILM_COVER: f32 = 0.7;
const SMEAR_COVER: f32 = 0.5;

fn film_cell(in: Out, per_pixel: mat2x2<f32>) -> vec4<f32> {
    let t = in.local * 0.5 + 0.5;
    let cells = mix(mix(in.color.x, in.color.y, t.x), mix(in.color.z, in.color.w, t.x), t.y);
    let wipe = wipe_at(in, t);
    let film = film_after(cells, wipe);
    let smeared = smear(wipe);
    let index = u32(in.alpha);
    let pane = select(0.0, 1.0, in.seed < 0.0);
    let cell = vec2<f32>(f32(index % u32(FILM_COLUMNS)), f32(index / u32(FILM_COLUMNS))) + t;
    // The waves: a noise over the film, its slope the shift.
    let p = vec2<f32>(cell.x * 2.0, cell.y * 2.0);
    let n0 = wet_noise(p, pane + 4.0);
    let slope = vec2<f32>(wet_noise(p + vec2<f32>(0.25, 0.0), pane + 4.0) - n0, (wet_noise(p + vec2<f32>(0.0, 0.25), pane + 4.0) - n0)) * 4.0;
    let blotch = wet_noise(vec2<f32>(cell.x * 1.3, cell.y * 1.3 / WET_STREAK), pane + 8.0);
    let cover = film * FILM_COVER * smoothstep(0.25, 0.6, blotch * 0.8 + film * 0.55) + smeared * SMEAR_COVER;
    let at = (in.position.xy + slope * (film + smeared) * FILM_WARP) * frame.screen.zw;
    let to_uv = vec2<f32>(frame.screen.y * frame.screen.z, 1.0) * FILM_BLUR;
    let stretch = select(vec2<f32>(1.0, in.seed), vec2<f32>(-in.seed, 1.0), in.seed < 0.0);
    var sum = vec3<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        let a = f32(k) * 2.3999632 + 0.7;
        let d = sqrt((f32(k) + 0.5) / 4.0);
        let offset = (in.axes.xy * cos(a) * stretch.x + in.axes.zw * sin(a) * stretch.y) * d * to_uv;
        sum += textureSampleLevel(scene, scene_sampler, clamp(at + offset, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
    }
    let colour = sum * 0.25;
    let bead = beads(in, cells, per_pixel);
    let layer = wet_layer(in, film);
    var under = vec4<f32>(bead.rgb + colour * cover * (1.0 - bead.a), bead.a + cover * (1.0 - bead.a));
    let ridge = blade_water(in, t, film);
    under = vec4<f32>(ridge.rgb + under.rgb * (1.0 - ridge.a), ridge.a + under.a * (1.0 - ridge.a));
    return vec4<f32>(layer.rgb + under.rgb * (1.0 - layer.a), layer.a + under.a * (1.0 - layer.a));
}

// A ribbon of water a running drop left (kind 7): a thin capsule of water along its quad's second axis, as wide as
// the first, `alpha` its wetness. It bends the view like a cylinder of water, with a dark line along each edge.
fn ribbon(in: Out) -> vec4<f32> {
    let half_width = in.metres.x;
    let spine = max(in.metres.y - half_width, 0.0);
    let p = vec2<f32>(in.local.x * half_width, in.local.y * in.metres.y);
    let q = vec2<f32>(p.x, p.y - clamp(p.y, -spine, spine)) / half_width;
    let r = length(q);
    if (r > 1.0) {
        discard;
    }
    let size = max(length(in.pixels.xy), 0.5);
    let o = in.pixels.xy * q.x + in.pixels.zw * q.y * (half_width / max(in.metres.y, 1e-6));
    return lens(screen_up(o, r), o, o, size, (in.position.xy - o) * frame.screen.zw, LENS_MAG * 0.4, 0.0, 0.3, in.seed)
        * ((1.0 - smoothstep(1.0 - 0.8 / size, 1.0, r)) * in.alpha);
}

// A lens's `q` for a pixel `o` pixels from its centre (y down), `r` out in its unit disc: the lens's up is the screen's,
// whichever way its quad lies (a running drop's runs along its path, down the glass or up it).
fn screen_up(o: vec2<f32>, r: f32) -> vec2<f32> {
    return vec2<f32>(o.x, -o.y) * (r / max(length(o), 1e-6));
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
        return film_cell(in, per_pixel);
    }
    if (in.kind == 7) {
        return ribbon(in);
    }
    if (in.kind == 0) {
        // A drop: its outline (`outline`, more ragged on the larger ones), the lens inside it.
        let size = max(length(in.pixels.xy), 0.5);
        let rr = outline(in.local, in.seed, mix(DROP_RAGGED.x, DROP_RAGGED.y, smoothstep(2.0, 6.0, size)));
        if (rr > 1.0) {
            discard;
        }
        let q = in.local * (rr / max(length(in.local), 1e-6));
        // A running drop is longer than it is wide, but its lens looks about as far out each way.
        let run = max(length(in.pixels.zw) / size, 1.0);
        let o = in.pixels.xy * q.x + in.pixels.zw * q.y;
        let look = in.pixels.xy * q.x + in.pixels.zw * (q.y / run);
        return lens(screen_up(o, rr), o, look, size, in.centre.xy, LENS_MAG, 0.6, 1.0, in.seed)
            * ((1.0 - smoothstep(1.0 - 0.8 / size, 1.0, rr)) * in.alpha);
    }
    var soft: f32;
    var rim: f32;
    if (in.kind == 3 || in.kind == 5) {
        let s = splat(in, per_pixel);
        soft = s.x;
        rim = s.y;
        if (soft <= 0.0) {
            discard;
        }
    } else {
        let edge = ragged(in.local, in.seed, select(0.35, 0.25, in.kind == 1));
        if (r > edge) {
            discard;
        }
        soft = 1.0 - smoothstep(edge * 0.75, edge, r);
        rim = r / edge;
    }
    if (in.kind == 5) {
        return mud_inside(in, rim, soft);
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

// A speck of mud or slush (kinds 3 and 5, ours): the drop that hit the glass, its outline ragged and drawn out its own
// way (`outline`), thick at its core and thinner to its edge (`SPLAT_THIN`), and up to `SPLAT_DROPLETS` fine droplets
// it threw around it, most up the glass with the airflow. The quad spans `SPLAT_REACH` of the drop's radius
// (`glass::SPLAT_REACH`). Returns its cover (0 to 1, its edge a pixel soft, so the specks a pixel across stay steady)
// and how far out to its edge the point lies (0 the core, 1 the edge and the droplets), for the light through it.
const SPLAT_REACH: f32 = 2.6;
const SPLAT_RAGGED: f32 = 0.3;
const SPLAT_THIN: f32 = 0.7;
const SPLAT_DROPLETS: u32 = 4u;
// A droplet's chance, its distance out from the drop's centre and its radius, in the drop's radii; its cover.
const DROPLET_CHANCE: f32 = 0.6;
const DROPLET_OUT: vec2<f32> = vec2<f32>(1.3, 2.4);
const DROPLET_SIZE: vec2<f32> = vec2<f32>(0.1, 0.3);
const DROPLET_COVER: f32 = 0.75;

fn splat(in: Out, per_pixel: mat2x2<f32>) -> vec2<f32> {
    let p = in.local * SPLAT_REACH;
    // A pixel, in the drop's radii.
    let pixel = max(max(length(per_pixel[0]), length(per_pixel[1])) * SPLAT_REACH, 1e-4);
    let rr = outline(p, in.seed, SPLAT_RAGGED);
    var cover = (1.0 - smoothstep(1.0 - pixel, 1.0 + pixel, rr)) * mix(1.0, SPLAT_THIN, smoothstep(0.2, 1.0, rr));
    var rim = min(rr, 1.0);
    let salt = u32(in.seed * 16777216.0);
    for (var k = 0u; k < SPLAT_DROPLETS; k++) {
        let h = hash4(salt + k * 7919u);
        if (h.w > DROPLET_CHANCE) {
            continue;
        }
        // Up the glass, within 80 degrees either side.
        let a = 1.5708 + (h.x * 2.0 - 1.0) * 1.4;
        let at = vec2<f32>(cos(a), sin(a)) * mix(DROPLET_OUT.x, DROPLET_OUT.y, h.y);
        let size = mix(DROPLET_SIZE.x, DROPLET_SIZE.y, h.z * h.z);
        let d = length(p - at);
        let dot = (1.0 - smoothstep(size - pixel, size + pixel, d)) * DROPLET_COVER;
        if (dot > cover) {
            cover = dot;
            rim = 1.0;
        }
    }
    return vec2<f32>(cover, rim);
}

// Mud seen from inside (kind 5, ours): the side stuck to the glass in its shade (`color`, `glass::MUD_INSIDE` of the
// speck's), and the outside behind it through the mud, filtered by its hue to the power 1.5: `MUD_THROUGH.x` through
// the thick core, `MUD_THROUGH.y` through the thin rim (`rim` 0 at the centre to 1 at the ragged edge). So a speck is a
// dark silhouette that glows warm at its edge against the sunlit road or the sky, as backlit mud does, and stays dark
// against the dark trees. Wet mud (`frame.outside.z`, the stage's wetness) lets `MUD_WET_THROUGH` of that through.
const MUD_THROUGH: vec2<f32> = vec2<f32>(0.03, 0.45);
const MUD_WET_THROUGH: f32 = 0.4;

fn mud_inside(in: Out, rim: f32, soft: f32) -> vec4<f32> {
    let hue = in.color.rgb / max(max(in.color.r, max(in.color.g, in.color.b)), 1e-3);
    let behind = textureSampleLevel(scene, scene_sampler, in.position.xy * frame.screen.zw, 0.0).rgb;
    let through = behind * hue * sqrt(hue) * mix(MUD_THROUGH.x, MUD_THROUGH.y, smoothstep(0.3, 1.0, rim))
        * mix(1.0, MUD_WET_THROUGH, frame.outside.z);
    var face = linear(in.color.rgb * frame.light.rgb);
    if (frame.night.w != 0.0) {
        face += in.color.rgb * (night_light(in.centre.xy) * 0.8 + linear(frame.night.rgb) * 0.6);
    }
    let alpha = soft * in.alpha * in.color.a;
    return vec4<f32>((face + through) * alpha, alpha);
}

// The pane seen from inside (kind 6, ours). A cell of the glass in `color` (its middle and size in the chart, metres),
// `edge` (how far inside the glass's outline its corners lie, cut at the outline over a pixel) and `seed` (0 the
// windscreen, 1 a side window). What it adds to the frame behind it and what it lets through, as premultiplied light
// over it (alpha the loss):
// - the light the two faces reflect away, by Schlick's Fresnel, so a steep pane passes less of the view;
// - a fine dust film that scatters the sunlight falling on the pane toward the eye, mostly forward (Henyey-Greenstein,
//   `DUST_G`), a veil about the sun when the car faces it, and the sky's light a little; patchy, thinner where the
//   wipers sweep;
// - dirt specks `SPECK_CELL` apart at most, a few tenths of a millimetre, dark against the view behind, bright when the
//   sun lights them from ahead;
// - dried water spots, `SPOT_CELL` apart at most, as many as the glass has been wet, rings of what the water left,
//   faint unless the sun lights them;
// - the wipers' fine scratches, arcs about their pivots in bands: a scratch lights where the sun's light, bent about
//   it, reaches the eye (the cone `dot(t, sun - view) = 0` about its tangent `t`), so the lit arcs line up through the
//   pivot toward the sun.
const PANE_F0: f32 = 0.04;
// The share of the faces' reflection of the cabin drawn (ours, the user's call: a less reflective interior; in full
// the light dash and the wheel veiled the view).
const CABIN_REFLECTED: f32 = 0.5;
// A pane cell's `edge` at 1, metres (`glass::gpu::PANE_EDGE`).
const PANE_EDGE: f32 = 0.05;
// The dust: the share of the sun's light on the pane it scatters toward the eye (a forward lobe, `DUST_G`, and a broad
// one, `DUST_BROAD` of it), and of the sky's; patches `DUST_PATCH` across, the swept areas at `DUST_SWEPT` of it. A
// light film: at 1% the whole view went milky driving toward a low sun. Onboard footage shows the haze toward the sun
// and next to none away from it, so the forward lobe carries it.
const DUST: f32 = 0.006;
const DUST_G: f32 = 0.8;
const DUST_BROAD: f32 = 0.1;
const DUST_SKY: f32 = 0.001;
const DUST_PATCH: f32 = 0.12;
const DUST_SWEPT: f32 = 0.55;
// The side windows' dust against the windscreen's, and the most the slant lengthens the light's way through it.
const DUST_SIDE: f32 = 0.5;
const DUST_SLANT: f32 = 2.5;
const SPECK_CELL: f32 = 0.008;
// A cell's chance of a speck and its radius, metres (road dust: at 1.2 mm and one cell in eight they read as orange
// flecks a few pixels wide on a side window), and the light it scatters toward the eye besides its forward glow, of
// the sun's and of the sky's (seen from inside, its lit side faces away).
const SPECK_CHANCE: f32 = 0.03;
const SPECK_RADIUS: vec2<f32> = vec2<f32>(0.0001, 0.0004);
const SPECK_COLOUR: vec3<f32> = vec3<f32>(0.62, 0.56, 0.47);
const SPECK_SHADE: f32 = 0.7;
const SPECK_DIFFUSE: vec2<f32> = vec2<f32>(0.04, 0.08);
// The scratches: rings `SCRATCH_PITCH` apart about the pivot, each there by `SCRATCH_CHANCE` with a strength of its
// own, in bands `SCRATCH_BAND` across; a scratch's light spread over the pixel it falls in (`SCRATCH_LINE` wide).
// Toward a low sun they read as a few faint streaks in the footage, not a pattern of arcs.
const SCRATCH: f32 = 3.0;
// A scratch's runs along its arc: their scale, metres, the noise above which there is one, and the share of the arc
// they cover.
const SCRATCH_RUN: vec3<f32> = vec3<f32>(0.06, 0.6, 0.3);
const SPOT_CELL: f32 = 0.014;
const SPOT_CHANCE: f32 = 0.06;
// A spot's radius, metres, its rim's width over the radius and how much the rim wanders in and out (a dried drop's
// edge is not a circle: at 4 mm and a sharp rim they read as drawn rings).
const SPOT_RADIUS: vec2<f32> = vec2<f32>(0.0008, 0.0025);
const SPOT_RIM: f32 = 0.3;
const SPOT_WOBBLE: f32 = 0.25;
const SPOT_LIGHT: f32 = 0.25;
const SCRATCH_WIDTH: f32 = 0.1;
const SCRATCH_PITCH: f32 = 0.0012;
const SCRATCH_CHANCE: f32 = 0.15;
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

// A pane cell's `tint` (`glass::gpu::Drop`) at `t` across the cell (0 to 1): its `PANE_TINTS` + 1 samples a side, 4
// bits each, bilinearly.
const PANE_TINTS: u32 = 4u;

fn pane_tint(tint: vec4<u32>, t: vec2<f32>) -> f32 {
    let p = clamp(t, vec2<f32>(0.0), vec2<f32>(1.0)) * f32(PANE_TINTS);
    let i = min(vec2<u32>(p), vec2<u32>(PANE_TINTS - 1u));
    let f = p - vec2<f32>(i);
    let k = i.y * (PANE_TINTS + 1u) + i.x;
    let row = PANE_TINTS + 1u;
    return mix(mix(tint_sample(tint, k), tint_sample(tint, k + 1u), f.x),
        mix(tint_sample(tint, k + row), tint_sample(tint, k + row + 1u), f.x), f.y);
}

fn tint_sample(tint: vec4<u32>, k: u32) -> f32 {
    return f32((tint[k / 8u] >> (4u * (k % 8u))) & 15u) / 15.0;
}

fn pane(in: Out, per_pixel: mat2x2<f32>) -> vec4<f32> {
    let at = in.color.xy + in.local * in.color.zw * 0.5;
    // A pixel's size on the glass, metres.
    let pixel = max(length(per_pixel[0] * in.color.zw * 0.5), length(per_pixel[1] * in.color.zw * 0.5));
    // Up to the glass's outline, over a pixel.
    let t = in.local * 0.5 + 0.5;
    let inside = mix(mix(in.edge.x, in.edge.y, t.x), mix(in.edge.z, in.edge.w, t.x), t.y) * PANE_EDGE;
    let glass = clamp(inside / max(pixel, 1e-5) + 0.5, 0.0, 1.0);
    if (glass <= 0.0) {
        discard;
    }
    var n = in.normal;
    let view = normalize(in.world - frame.eye.xyz);
    n = select(n, -n, dot(n, view) < 0.0);
    let cos_v = max(dot(view, n), 0.05);
    let fresnel = PANE_F0 + (1.0 - PANE_F0) * pow(1.0 - cos_v, 5.0);
    var through = (1.0 - fresnel) * (1.0 - fresnel);
    let sun = frame.sun.xyz;
    // The outside's light on the pane, behind what its texture lets through (a sun strip keeps it off the dust).
    let tint = pane_tint(in.tint, t);
    let sun_on = max(dot(sun, n), 0.0) * frame.sun.w * sun_reach(in.world + n * 0.05) * tint;
    let sun_light = linear(frame.sun_light.rgb) * sun_on;
    let sky = linear(frame.light.rgb) * tint;
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
                // Grit under the blade cuts a run of the arc, not all of it.
                let pivot = at - frame.wipers[i * 2].xy;
                let along = atan2(pivot.y, pivot.x) * w.y / SCRATCH_RUN.x;
                let run = smoothstep(SCRATCH_RUN.y, SCRATCH_RUN.y + 0.1, value_noise(vec2<f32>(along, floor(ring)), 12u));
                let line = select(0.0, h.y, h.x < SCRATCH_CHANCE) * run * min(SCRATCH_LINE / pixel, 1.0)
                    * exp(-d * d / max(pixel * pixel * 0.5, 1e-12));
                // Finer than a pixel the rings average out.
                let mean = SCRATCH_CHANCE * 0.5 * SCRATCH_LINE / SCRATCH_PITCH * SCRATCH_RUN.z;
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
        let lit = linear(SPECK_COLOUR)
            * (sun_light * (SPECK_DIFFUSE.x + henyey_greenstein(cos_t, 0.5) * 2.0) + sky * SPECK_DIFFUSE.y);
        light += lit * cover;
        through *= 1.0 - cover * SPECK_SHADE;
    }
    // The water spots: a ring, a thin film inside it.
    let spot_cell = floor(at / SPOT_CELL);
    let spot_key = vec2<u32>(vec2<i32>(spot_cell) + vec2<i32>(8192));
    let s = hash4(spot_key.x * 83492791u ^ spot_key.y * 2971215073u ^ u32(in.seed) * 19349663u);
    if (s.x < SPOT_CHANCE * frame.outside.y * mix(1.0, 0.3, swept)) {
        let radius = mix(SPOT_RADIUS.x, SPOT_RADIUS.y, s.y);
        let centre = (spot_cell + 0.3 + 0.4 * s.zw) * SPOT_CELL;
        let d = at - centre;
        let turn = atan2(d.y, d.x) * 0.9549297;
        let wobble = value_noise(vec2<f32>(turn, s.y * 64.0), 23u) - 0.5;
        let r = length(d) / (radius * (1.0 + SPOT_WOBBLE * wobble));
        let width = max(SPOT_RIM, pixel / radius);
        let ring = exp(-(r - 1.0) * (r - 1.0) / (width * width)) * min(SPOT_RIM / width, 1.0) * 0.5;
        let film = (1.0 - smoothstep(0.8, 1.0, r)) * 0.15;
        let residue = ring + film;
        light += sun_light * henyey_greenstein(cos_t, 0.7) * SPOT_LIGHT * residue + sky * 0.02 * residue;
        through *= 1.0 - 0.08 * residue;
    }
    // The cabin reflected in the inner face, and in the outer seen through it and its texture twice (a sun strip
    // keeps only the inner face's), `CABIN_REFLECTED` of it.
    if (in.seed == 0.0) {
        light += fresnel * (1.0 + through * tint * tint) * reflected(in.world) * CABIN_REFLECTED;
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
