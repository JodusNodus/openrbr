// The cabin's light in the in-car view (renderer addition, `Enhance::cabin`, ours), appended to `shader.wgsl`. The
// game lights the interior set with one ambient and the full sun wherever a face turns to it (`INTERIOR_AMBIENT`).
// Here the interior's ambient follows the share of sky each vertex sees past the car's opaque faces (`car/cabin.rs`,
// `specular.z` = 1 - that share) and takes the colour and brightness of what lies outside the way it sees out
// (`cabin_window`): a face turned to a sunlit or a sunset window is lit by that sky, one that sees out only toward the
// forest is darker. The sun reaches it only through the glass: past the car's own shadow map (the roof, pillars and
// dash shade each other), the stage's shadows (the sun's cascades, else the reflection cube toward the sun) and the
// clouds, and stronger than the game's, so the patches it lights stand out; they light the rest of the cabin back
// (`car::cabin::SunIn`). The plastics take a faint sheen of what they reflect out of the windows (`cabin_sheen`). By
// night and dusk the headlights' light off the road comes in through the glass. The dial faces keep at least the
// game's light, so they read as before until the exposure follows the cabin.

// True with `Enhance::cabin` (a pipeline override, so that the frame without it is built as before).
override CABIN: bool = false;
// The whole part of `specular.y` of the interior set's vertices and of its dial faces (`car::cabin::INTERIOR`,
// `GAUGE`). Its fraction holds the z of where the vertex sees the sky (`cabin_seen`).
const CABIN_INTERIOR: f32 = 1.0;
const CABIN_GAUGE: f32 = 2.0;
// And of a mirror's glass (`car::cabin::MIRROR`), which mirrors the surroundings (`cabin_mirror`).
const CABIN_MIRROR_GLASS: f32 = 3.0;
// `car::cabin::SEEN_Z`.
const CABIN_SEEN_Z: vec2<f32> = vec2<f32>(0.5, 0.45);

// The ambient's share of `INTERIOR_AMBIENT` with no sky seen (light bounced inside the cabin), and the share more with
// all of it.
const CABIN_FLOOR: f32 = 0.6;
const CABIN_SKY: f32 = 1.2;
// The sky share over which paint goes from the interior's light to the outside's.
const CABIN_OUT: vec2<f32> = vec2<f32>(0.6, 0.9);
// The sun through the glass against the game's interior sun.
const CABIN_SUN: f32 = 2.0;
// The share of the sun on the cabin's faces (`frame.wetness.z`, `car::cabin::SunIn`) that they give back to the rest
// of it: about their albedo times `CABIN_SUN`.
const CABIN_SUN_BOUNCE: f32 = 0.6;
// The headlights' light back off the lit road, per unit of their colour times intensity (`lights::HEADLIGHT`).
const CABIN_BOUNCE: f32 = 0.00004;

// The interior's ambient scale (rgb, `around` in `shade`) and its sunlit share (a), with the car's shadow `shadowed`,
// the cloud's `under_cloud` and the outside view's light `outside` (`surroundings`) there.
fn cabin_light(v: VertexOut, n: vec3<f32>, shadowed: f32, under_cloud: f32, outside: vec4<f32>) -> vec4<f32> {
    let sky = 1.0 - v.specular.z;
    var stage = outside.a;
    // The cascades only where the sun can reach the face.
    if (frame.sun_shadow.x != 0.0 && frame.sun_shadow.z == 0.0 && dot(n, -frame.sun.xyz) > 0.0 && shadowed < 1.0) {
        stage = cabin_stage_lit(v.world, n);
    }
    let sunlit = CABIN_SUN * (1.0 - shadowed) * stage * (1.0 - CLOUD_CAR_SHADE * under_cloud);
    var ambient = INTERIOR_AMBIENT * (CABIN_FLOOR + CABIN_SKY * sky * cabin_window(cabin_seen(v), sky));
    // Paint (the classes that reflect, `Kf` over 0) open to the sky is the body seen through the glass (the bonnet):
    // lit as the outside view lights it, not by the interior's ambient, which blew it out white.
    if (mode.fresnel.z != 0.0) {
        ambient = mix(ambient, outside.rgb, smoothstep(CABIN_OUT.x, CABIN_OUT.y, sky));
    }
    let held = max(mode.light.x * frame.ambient.rgb, vec3<f32>(1e-3));
    // The sun back off the cabin's sunlit faces, onto all of it and more onto the faces turned down (the roof lining
    // over a sunlit dash or seat), as much as the sun reaches the car past the trees and the clouds.
    if (frame.wetness.z > 0.0) {
        let reach = outside.a * (1.0 - CLOUD_CAR_SHADE * under_cloud);
        let facing = 0.75 - 0.25 * n.y;
        ambient += CABIN_SUN_BOUNCE * frame.wetness.z * reach * facing * mode.light.y * frame.diffuse.rgb / held;
    }
    let level = select(frame.dusk.w, 1.0, frame.night.x != 0.0);
    if (level > 0.0) {
        var bounce = vec3<f32>(0.0);
        for (var i = 0; i < 8; i++) {
            let lamp = frame.lamps[i];
            if (lamp.position.w != 0.0 && lamp.forward.w != 0.0) {
                // The road it lights lies ahead and below.
                let road = normalize(lamp.forward.xyz - 0.4 * lamp.up.xyz);
                bounce += lamp.colour.rgb * (0.5 + 0.5 * dot(n, road));
            }
        }
        ambient += CABIN_BOUNCE * level * sky * bounce / held;
    }
    // A dial face keeps at least the game's light.
    if (v.specular.y >= CABIN_GAUGE) {
        return max(vec4<f32>(ambient, sunlit), vec4<f32>(vec3<f32>(INTERIOR_AMBIENT), 1.0));
    }
    return vec4<f32>(ambient, sunlit);
}

// `sun_lit` for the cabin, all within the first cascade: one filtered compare (2 x 2 texels, 1.4 cm each) in place of
// the 3 x 3 taps, which cost about 0.9 ms a 1080p cockpit frame.
fn cabin_stage_lit(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let p = frame.sun_matrices[0] * vec4<f32>(world + n * frame.sun_texels[0] * 1.5, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0) {
        return 1.0;
    }
    return textureSampleCompareLevel(sun_map, shadow_sampler, uv, 0, p.z);
}

// The sheen of the interior's faces that reflect nothing in the game's settings (`Kf` 0: plastics, cloth, dials): a
// dielectric's Fresnel (`CABIN_F0`) of the surroundings blurred (`CABIN_SHEEN_LEVEL` of the cube), where the reflected
// direction leaves the car (`cabin_open`): the dash reflects the sky through the windscreen, not where the roof is.
const CABIN_F0: f32 = 0.04;
const CABIN_SHEEN_LEVEL: f32 = 3.0;

fn cabin_sheen(v: VertexOut, n: vec3<f32>, view: vec3<f32>) -> vec3<f32> {
    let nv = clamp(dot(n, view), 0.0, 1.0);
    let f = CABIN_F0 + (1.0 - CABIN_F0) * pow(1.0 - nv, 5.0);
    let r = reflect(-view, n);
    return reflected(r, CABIN_SHEEN_LEVEL) * f * cabin_open(cabin_seen(v), 1.0 - v.specular.z, r);
}

// Where a baked vertex sees the sky (`car::cabin::seen`, turned with the car): the mean of its open rays' directions.
// Its length over the open share is 2/3 when the whole hemisphere is open and goes to 1 for a narrow window.
fn cabin_seen(v: VertexOut) -> vec3<f32> {
    return vec3<f32>(v.uv2, (fract(v.specular.y) - CABIN_SEEN_Z.x) / CABIN_SEEN_Z.y);
}

// How much of what lies outside along `dir` a vertex that sees the sky by `seen` with the open share `share` sees: the
// open directions taken as a cone about `seen`, as wide as the share (a cosine-weighted cone of half angle a holds
// sin(a)^2 of the hemisphere), with a soft edge.
fn cabin_open(seen: vec3<f32>, share: f32, dir: vec3<f32>) -> f32 {
    let length = length(seen);
    if (length < 1e-4) {
        return 0.0;
    }
    let edge = sqrt(clamp(1.0 - share, 0.0, 1.0));
    return smoothstep(edge - CABIN_OPEN_EDGE, edge + CABIN_OPEN_EDGE, dot(dir, seen / length));
}

// The soft edge of `cabin_open`'s cone, in the cosine.
const CABIN_OPEN_EDGE: f32 = 0.15;

// The light through the windows of a vertex that sees the sky by `seen` with the open share `share` (ours): what lies
// outside that way (`cabin_outside`), blurred over the cone the open rays span (`CABIN_LEVELS`: the cube's level for
// the whole hemisphere and for a narrow window), against what lies overhead as blurred as the widest, in linear light:
// a face that sees the open sky above keeps the light it had. `CABIN_TINT` of its colour, the brightness held to
// `CABIN_RANGE`. A face turned to a sunset window takes its warm light, one that sees out only toward the trees less.
const CABIN_LEVELS: vec2<f32> = vec2<f32>(5.5, 3.0);
const CABIN_TINT: f32 = 0.3;
const CABIN_RANGE: vec2<f32> = vec2<f32>(0.25, 2.0);

fn cabin_window(seen: vec3<f32>, share: f32) -> vec3<f32> {
    let length = length(seen);
    if (length < 1e-4) {
        return vec3<f32>(1.0);
    }
    let gathered = smoothstep(0.67, 0.95, length / max(share, 1e-3));
    let outside = cabin_outside(seen / length, mix(CABIN_LEVELS.x, CABIN_LEVELS.y, gathered));
    let luma = vec3<f32>(0.2126, 0.7152, 0.0722);
    let overhead = dot(cabin_outside(vec3<f32>(0.0, 1.0, 0.0), CABIN_LEVELS.x), luma);
    let ratio = outside / max(overhead, 1e-4);
    let grey = dot(ratio, luma);
    let tinted = mix(vec3<f32>(grey), ratio, CABIN_TINT);
    return tinted * clamp(grey, CABIN_RANGE.x, CABIN_RANGE.y) / max(grey, 1e-4);
}

// What lies outside the car along `dir`, blurred to the surroundings cube's `level` (the sky before the cube is drawn),
// in linear light (the colours are display colours, `sky_display`).
fn cabin_outside(dir: vec3<f32>, level: f32) -> vec3<f32> {
    var outside = select(sky(dir, 1.0), sky_reflected(dir, level / 7.0), sky_on());
    if (frame.reflection.x != 0.0) {
        outside = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, dir, level).rgb);
    }
    return pow(max(outside, vec3<f32>(0.0)), vec3<f32>(2.2));
}

// A mirror of the cabin (ours): the interior set's opaque faces of the glass class, which the game lights as glass, so
// that they show the texel lit white and only a Fresnel share of the surroundings, and a mirror's glass
// (`car::cabin::MIRROR`). Here they mirror them, behind the car through the rear window or beside it, as a silvered
// glass does (`CABIN_MIRROR` of the light), darkened where the texel is dark so that a frame painted in the texture
// stays. The texel's hue is left out: models paint the glass flat green or blue (the BMW M3 E30's `i_body_tukor`).
const CABIN_MIRROR: f32 = 0.85;

fn cabin_mirror(texel: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let grey = max(max(texel.r, max(texel.g, texel.b)), 1e-3);
    return reflected(dir, 0.0) * CABIN_MIRROR * smoothstep(0.1, 0.4, grey);
}

// The diffuse texel without DXT1's hue noise (ours): a block's two colours are stored in 5:6:5 bits, so a dark, nearly
// grey gradient (the dashboards) shifts toward magenta in one block and green in the next, and the lighting and the
// tone curve make that a rainbow. A texel that nearly grey (chroma under `CABIN_GREY`, faded out up to twice that) is
// taken as grey at its brightness; coloured texels (trim, decals, dials) stay as they are. The texture's own coarser
// mip levels carry the same noise, so they cannot give the hue back.
const CABIN_GREY: f32 = 0.03;

fn cabin_texel(texel: vec3<f32>) -> vec3<f32> {
    let chroma = max(texel.r, max(texel.g, texel.b)) - min(texel.r, min(texel.g, texel.b));
    let grey = 1.0 - smoothstep(CABIN_GREY, 2.0 * CABIN_GREY, chroma);
    return mix(texel, vec3<f32>(dot(texel, vec3<f32>(0.299, 0.587, 0.114))), grey);
}
