// Our tyre marks (`tyre_marks.rs`, `SkidPass::tyres`; renderer addition `marks`): procedural, no texture. The blend
// keeps `ground * alpha + rgb`, so a mark darkens the lit ground under it by its alpha and adds the light dust or snow
// it pushed up as its colour, lit as the ground is. Per corner (`skidmarks_pass.rs` `tyre_vertices`): `uv.x` across in
// half tread widths (the tyre's edges at 1, the berms beyond), `uv.y` along in metres; `cell` the kind (0 loose, 1
// tarmac, 2 snow, 3 grass), wetness, slide and swept width in metres; `rgba` the strength, fade and looseness.

// Marks fade out between these distances from the eye, m (the stage's fog lies over the ground they darken).
const TYRE_FAR: vec2<f32> = vec2<f32>(90.0, 220.0);

// What `add` over ground of a usual brightness (`TYRE_GROUND`) adds in the target's colour: in HDR, linear colour.
const TYRE_GROUND: f32 = 0.45;

fn brighter(add: vec3<f32>) -> vec3<f32> {
    return linear(vec3<f32>(TYRE_GROUND) + add) - linear(vec3<f32>(TYRE_GROUND));
}

fn tyre_hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

// Value noise, 0 to 1.
fn tyre_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = tyre_hash(i);
    let b = tyre_hash(i + vec2<f32>(1.0, 0.0));
    let c = tyre_hash(i + vec2<f32>(0.0, 1.0));
    let d = tyre_hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// A rolling tyre's tread printed in the ground, 1 where a block pressed, 0 in the grooves: chevrons of blocks in rows
// across the tread (`across`, `along` in metres), `pitch` long, staggered row by row, each block pressed a little
// differently.
fn tyre_tread(across: f32, along: f32, pitch: f32, rows: f32, width: f32) -> f32 {
    let row = (across / width + 0.5) * rows;
    let r = floor(row);
    let t = along / pitch + 0.5 * r + 0.9 * abs(across) / width;
    let p = fract(t);
    let q = fract(row);
    let block_along = smoothstep(0.0, 0.14, p) * (1.0 - smoothstep(0.52, 0.66, p));
    let block_across = smoothstep(0.0, 0.14, q) * (1.0 - smoothstep(0.8, 0.94, q));
    return block_along * block_across * (0.55 + 0.45 * tyre_hash(vec2<f32>(r, floor(t))));
}

@fragment
fn fs_tyre(v: VertexOut) -> @location(0) vec4<f32> {
    let kind = u32(round(v.cell.x));
    let wet = v.cell.y;
    let slide = clamp(v.cell.z, 0.0, 1.0);
    let width = max(v.cell.w, 0.01);
    let strength = v.rgba.x;
    let loose = v.rgba.z;
    let along = v.uv.y;
    let across = v.uv.x * width * 0.5;
    let ground = v.world.xz;
    let far = 1.0 - smoothstep(TYRE_FAR.x, TYRE_FAR.y, distance(view.eye.xyz, v.world));
    let fade = v.rgba.y * far;
    if fade <= 0.001 {
        discard;
    }
    // The fine patterns fade where a pixel spans them, against shimmer.
    let fine = 1.0 - smoothstep(0.004, 0.012, max(fwidth(along), fwidth(across)));
    let rolling = (1.0 - slide) * fine;
    // Broken edges: the edge wanders a few centimetres along the trail.
    let ragged = (tyre_noise(ground * 3.0) - 0.5) * 0.3 + (tyre_noise(ground * 14.0) - 0.5) * 0.25
        + (tyre_noise(ground * 60.0) - 0.5) * 0.12;
    let x = abs(v.uv.x) + ragged * mix(0.25, 1.0, f32(kind != 1u));
    let inside = 1.0 - smoothstep(0.75, 1.02, x);
    // Streaks along the trail where the tyre slid, and the ground's grain.
    let streak = tyre_noise(vec2<f32>(across * 140.0, along * 0.9)) * 0.6 + tyre_noise(vec2<f32>(across * 420.0, along * 2.0)) * 0.4;
    let grain = tyre_noise(ground * 70.0);
    // How hard the tyre pressed, varying along the trail.
    let press = 0.75 + 0.5 * tyre_noise(vec2<f32>(along * 0.7, v.uv.x * 0.3));
    let tread = tyre_tread(across, along, 0.034, 5.0, width);

    var keep = 1.0;
    var add = vec3<f32>(0.0);
    switch kind {
        // Tarmac: rubber laid by a sliding, locked or spinning tyre; gravel swept off the line where there is any.
        case 1u: {
            let rubber = inside * (0.7 + 0.3 * streak * fine);
            keep = 1.0 - 0.62 * strength * rubber;
        }
        // Snow: a packed groove, its walls in shade, the blocks' print in its floor, the snow thrown up bright beside
        // it.
        case 2u: {
            let wall = smoothstep(0.6, 0.95, x) * (1.0 - smoothstep(0.95, 1.12, x));
            let floor_ = inside * ((0.1 + 0.06 * loose) * press + 0.12 * tread * rolling + 0.08 * streak * slide);
            keep = 1.0 - strength * (0.22 * wall + floor_);
            let clumps = smoothstep(0.25, 0.8, tyre_noise(ground * 5.0 + 31.0)) * (0.5 + 0.5 * grain);
            let berm = smoothstep(0.98, 1.15, x) * (1.0 - smoothstep(1.15, 1.5, x)) * clumps;
            add = vec3<f32>(0.92, 0.95, 1.0) * 0.025 * strength * berm;
        }
        // Grass: flattened, darker blades along the track.
        case 3u: {
            let blades = 0.7 + 0.3 * tyre_noise(vec2<f32>(across * 90.0, along * 0.6));
            keep = 1.0 - (0.2 + 0.1 * loose + 0.1 * wet) * strength * inside * blades;
        }
        // Gravel and dirt: a pressed rut, darker in its floor with the tread blocks' print, the displaced gravel in
        // lighter berms along both edges, wetter and darker in the wet.
        default: {
            let floor_ = inside * ((0.26 + 0.1 * loose) * press + 0.16 * tread * rolling + 0.12 * (streak - 0.5) * slide
                + 0.08 * (grain - 0.5));
            let wall = smoothstep(0.7, 0.95, x) * (1.0 - smoothstep(0.95, 1.1, x));
            keep = 1.0 - strength * (floor_ * (1.0 + 0.2 * wet) + 0.1 * wall);
            let stones = smoothstep(0.45, 0.8, tyre_noise(ground * 55.0)) * (0.4 + 0.6 * grain);
            let clumps = smoothstep(0.3, 0.8, tyre_noise(ground * 6.0 + 17.0));
            let berm = smoothstep(0.92, 1.12, x) * (1.0 - smoothstep(1.12, 1.5, x)) * clumps * (0.25 + 0.75 * stones);
            add = vec3<f32>(0.62, 0.57, 0.5) * (0.035 + 0.025 * loose) * strength * berm * (1.0 - 0.6 * wet) * (1.0 + slide);
        }
    }
    // Wet: water pooled in the pressed floor catches the sky at a grazing view.
    let view_down = abs(normalize(view.eye.xyz - v.world).y);
    let pooled = smoothstep(0.45, 0.85, tyre_noise(vec2<f32>(across * 25.0, along * 0.8) + 7.0));
    let sheen = wet * inside * pooled * pow(1.0 - view_down, 4.0) * 0.06 * f32(kind == 0u || kind == 3u);
    add += vec3<f32>(0.8, 0.85, 0.9) * sheen * strength;
    let light = lit(v.world);
    return vec4<f32>(brighter(add) * light * fade, mix(1.0, clamp(keep, 0.0, 1.0), fade));
}
