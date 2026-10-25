// The lit draws' shading (`shade_car`: the car's paint, glass, lamps and cracks, the characters): their pipelines take
// this module (`renderer::pipelines::CAR_SHADER`). The shared code is in `shader.wgsl`.

@fragment
fn fs_main(v: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    pixel = vec2<i32>(v.clip.xy);
    return shade_car(v, front, 0.0);
}

@fragment
fn fs_clouded(c: CloudedOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    pixel = vec2<i32>(c.clip.xy);
    let v = VertexOut(c.clip, c.color, c.uv0, c.uv1, c.uv2, c.world, c.normal, c.specular, c.sky, c.bounce_sun, c.bounce_sky);
    return shade_car(v, front, c.cloud);
}

fn in_car() -> bool {
    if (INSIDE > 1u) {
        return frame.reflection.y != 0.0;
    }
    return INSIDE == 1u;
}

// A lit draw's colour (`lit_class` 1 to 4: the car's paint, glass, lit lamps and cracks, the characters), with
// `under_cloud` of the sun taken by a cloud. Never camera-anchored nor a sky face.
fn shade_car(v: VertexOut, front: bool, under_cloud: f32) -> vec4<f32> {
    let first = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias);
    if (PERF_SHADE == 1.0) {
        var a = first.a;
        if (ALPHA_TEST && mode.alpha_test != 0u && a <= mode.alpha_ref) { discard; }
        if (mode.coverage != 0u) { a = clamp((a - mode.alpha_ref) / max(fwidth(a), 1e-4) + 0.5, 0.0, 1.0); }
        return vec4<f32>(v.color.rgb * first.rgb, a);
    }
    // The textures' colour, then lit by the vertex light.
    var surface = first.rgb;
    var alpha = first.a;
    var second = vec4<f32>(0.0);
    if (second_draw()) {
        second = textureSampleBias(texture1, texture_sampler, v.uv1, mode.mip_bias);
    }
    let night = is_night();
    // The game's vertex light (`relit` keeps a lit draw's own).
    var colour = relit(v) * surface;
    // The shadow light map multiplies the colour (`color = (base + spec) * shadow.rgb`, `notes/formats.md` "Shaders and vertex layouts"). With the
    // sun's shadows on (`sun_shadow.w`) it is not used: their cascades and far layer cast for any sun, and the moon.
    let baked = select(textureSample(shadow_texture, texture_sampler, v.uv2).r, 1.0, frame.sun_shadow.w != 0.0);
    colour = colour * baked;
    var n = normalize(v.normal) * select(-1.0, 1.0, front);
    // The car's shadow takes only the sun's term (and a crack's glint): faces turned from the sun skip it.
    var shadowed = 0.0;
    if (lit_class() == 4u || dot(n, -frame.sun.xyz) > 0.0) {
        shadowed = car_shadow(v.world);
    }
    let view = normalize(frame.eye.xyz - v.world);
    // A window's crack (ours): its normal from the crack texture's alpha, and the facets' glint in the sun.
    // The facets also mirror the surroundings, more where the crack is steep (`CRACK_MIRROR`).
    var glint = vec3<f32>(0.0);
    if (lit_class() == 4u) {
        let bumped = crack_normal(n, v.world, v.uv0);
        n = bumped.xyz;
        let sun = crack_glint(n, view) * (1.0 - shadowed) * frame.diffuse.rgb;
        glint = (sun + reflected(reflect(-view, n), 0.0) * CRACK_MIRROR) * bumped.w;
        alpha = max(alpha, clamp(max(glint.r, max(glint.g, glint.b)), 0.0, 1.0));
    }
    // In the in-car view the interior has no shadow of the car's own body and no light from the surroundings, as
    // in the game (`notes/formats.md` "Shaders and vertex layouts", car constants), and [`INTERIOR_AMBIENT`] times the ambient.
    let inside = in_car();
    let outside = surroundings(n);
    var around = select(outside, vec4<f32>(vec3<f32>(INTERIOR_AMBIENT), 1.0), inside);
    var sunlit = 1.0;
    if (!inside) {
        sunlit = (1.0 - shadowed) * stage_sun(v.world, n, outside.a) * (1.0 - CLOUD_CAR_SHADE * under_cloud);
    }
    // With the cabin light (`cabin.wgsl`, ours) the interior set takes the sky it sees and the sun through the glass,
    // its dial faces no darker than in the game, and the body's paint seen from inside (the bonnet) the outside
    // view's light. The rest (the glass the outside is seen through, the lamps, the cracks) is lit as in the game.
    let cabin_face = CABIN && inside && lit_class() == 1u && cabin_set(v);
    if (cabin_face) {
        let cabin = cabin_light(v, n, shadowed, under_cloud, outside);
        around = vec4<f32>(cabin.rgb, 1.0);
        sunlit = cabin.a;
        colour = v.color.rgb * cabin_texel(first.rgb);
    } else if (CABIN && inside && lit_class() == 1u && mode.fresnel.z != 0.0) {
        around = outside;
        sunlit = (1.0 - shadowed) * stage_sun(v.world, n, outside.a) * (1.0 - CLOUD_CAR_SHADE * under_cloud);
    }
    // The dirt (`CarPaint1.pso`): the diffuse lerped to the dirt colour by the dirt map's alpha times the level,
    // and the reflection reduced where it covers.
    var clean = 1.0;
    if (mode.fresnel.w != 0.0) {
        let mud = textureSampleBias(specular_texture, texture_sampler, v.uv0, mode.mip_bias);
        let covered = clamp(mud.a * frame.wetness.y, 0.0, 1.0);
        colour = mix(colour, DIRT_COLOUR, covered);
        clean = 1.0 - covered;
    }
    if (mode.grime.x != 0.0 && frame.wetness.y > 0.0) {
        // A pane's mud film by the dirt level times its share (ours, `dirt::grime`), in patches of a value noise
        // on the pane's uv with clearer glass between them; the wet mud keeps `GRIME_SHEEN` of the reflection.
        let film = frame.wetness.y * mode.grime.x;
        let patches = 0.65 * value_noise(vec3<f32>(v.uv0 * GRIME_SCALE, 0.0))
            + 0.35 * value_noise(vec3<f32>(v.uv0 * GRIME_SCALE * 4.0, 1.7));
        let covered = GRIME_MOST * film * smoothstep(0.3, 0.7, patches);
        colour = mix(colour, DIRT_COLOUR, covered);
        alpha = mix(alpha, 1.0, covered);
        clean = 1.0 - covered * (1.0 - GRIME_SHEEN);
    }
    var shaded: vec3<f32>;
    if (cabin_face && day_on()) {
        shaded = cabin_day_lit(colour, v, n, shadowed, under_cloud, outside);
    } else {
        shaded = lit(colour, n, view, sunlit, around.rgb, v.world);
    }
    // The Fresnel weight of the sky reflection: Schlick with F0 = 0.125 (`c91`), clamped to
    // [FresnelMin, FresnelMax] (`c92`), times Kf. Glass adds half of it to the alpha (`CarGlass`: `t0.a + 0.5 * F * Kf`);
    // other classes reflect where the diffuse alpha is (`CarPaint`: the reflection times `t0.a`).
    let nv = clamp(dot(n, view), 0.0, 1.0);
    var f = clamp(0.125 + 0.875 * pow(1.0 - nv, 5.0), mode.fresnel.x, mode.fresnel.y) * mode.fresnel.z;
    // With the day's light a dielectric's (Schlick, F0 0.04) where the class reflects at all.
    let physical = day_lit();
    if (physical) {
        f = select(0.0, 0.04 + 0.96 * pow(1.0 - nv, 5.0), mode.fresnel.y * mode.fresnel.z > 0.0);
    }
    if (lit_class() == 2u) {
        colour = shaded + reflected(reflect(-view, n), 0.0) * f * clean;
        alpha = clamp(alpha + 0.5 * f * clean, 0.0, 1.0);
        // The cabin's opaque glass is its mirrors (`cabin_mirror`).
        if (CABIN && inside && cabin_set(v)) {
            colour = cabin_mirror(first.rgb, reflect(-view, n));
        }
    } else {
        // With the HDR post pass, paint reflects less and a little blurred (`PAINT_GLOSS`).
        let gloss = select(vec2<f32>(1.0, 0.0), PAINT_GLOSS, frame.enhance.x != 0.0);
        var shine = reflected(reflect(-view, n), gloss.y) * f * gloss.x * alpha * clean;
        // The cabin's paint reflects only what it sees out of the windows (`cabin_open`).
        if (cabin_face) {
            shine = shine * cabin_open(cabin_seen(v), 1.0 - v.specular.z, reflect(-view, n));
        }
        colour = shaded + shine;
        // With the day's light the coat's reflection adds in linear light over the paint, which gets what it does
        // not reflect.
        if (physical) {
            let coat = f * alpha * clean;
            let mirrored = reflected(reflect(-view, n), gloss.y);
            colour = pow(pow(shaded, vec3<f32>(2.2)) * (1.0 - coat) + pow(mirrored, vec3<f32>(2.2)) * coat, vec3<f32>(1.0 / 2.2));
        }
    }
    if (CABIN && inside && v.specular.y >= CABIN_MIRROR_GLASS) {
        colour = cabin_mirror(first.rgb, reflect(-view, n));
    }
    // A mirror's glass with its image (`mirrors.wgsl`, ours), opaque.
    let mirror = mirror_of(v);
    if (mirror < MIRROR_COUNT) {
        colour = mirror_image(mirror, v.world);
        alpha = 1.0;
    }
    if (cabin_face && mode.fresnel.z == 0.0) {
        colour = colour + cabin_sheen(v, n, view);
    }
    colour = colour + glint;
    // A lit lamp's part (`stage::Shade::glow`) glows (ours).
    if (lit_class() == 3u) {
        colour = max(colour, first.rgb * LAMP_GLOW);
    }
    // A dial's backlight (`rbr_render::dials`, ours): by night, and at dusk as much as the lamps light the stage,
    // the texel glows in the light of its emissive mask (the RBRHUD gauges' `emissive_mask`): the mask's colour
    // (the bulbs' pools on analog faces) plus its alpha in white (displays, warning lamps).
    if (mode.glow.x != 0.0 && (night || frame.dusk.w > 0.0)) {
        let mask = textureSampleLevel(glow_texture, texture_sampler, v.uv0, 0.0);
        let backlight = mask.rgb + vec3<f32>(mask.a * mode.glow.y);
        colour = max(colour, first.rgb * backlight * DIAL_GLOW * select(frame.dusk.w, 1.0, night));
    }
    // The scrape (`CarPaint`, `CarPaintInt`): the scrape texel times the diffuse light (`t3 * v0`, no highlight or
    // reflection) replaces the paint by `sat(v0.w * t3.a)`, `v0.w` being the vertex's damage weight / 100.
    if (second_draw()) {
        let scraped = LIGHT_SCALE * second.rgb * light(n, sunlit, around.rgb, v.world);
        colour = mix(colour, scraped, clamp(v.color.a * second.a, 0.0, 1.0));
    }
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    if (mode.coverage != 0u) {
        alpha = clamp((alpha - mode.alpha_ref) / max(fwidth(alpha), 1e-4) + 0.5, 0.0, 1.0);
    }
    // With the cabin light the air inside the car is clear: the stage's fog, whose range can start behind the eye,
    // does not grey the interior set; the glass and what is seen through it keep theirs.
    if (CABIN && in_car() && lit_class() != 0u && cabin_set(v)) {
        return vec4<f32>(graded(colour), alpha);
    }
    return vec4<f32>(graded(fogged(colour, v.world)), alpha);
}

// The crack's normal (ours, `crack_decal.rs`): the alpha is a height, its slope across a texel tilts `n` in the
// cotangent frame of the face's uv; `w` is how steep it is (0 on clear glass). `CRACK_BUMP` scales the slope.
const CRACK_BUMP: f32 = 1.5;

const CRACK_MIRROR: f32 = 0.6;

fn crack_normal(n: vec3<f32>, p: vec3<f32>, uv: vec2<f32>) -> vec4<f32> {
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let texel = max(1.0 / vec2<f32>(textureDimensions(texture0)), max(abs(duv1), abs(duv2)));
    let h = vec4<f32>(
        textureSampleBias(texture0, texture_sampler, uv + vec2<f32>(texel.x, 0.0), mode.mip_bias).a,
        textureSampleBias(texture0, texture_sampler, uv - vec2<f32>(texel.x, 0.0), mode.mip_bias).a,
        textureSampleBias(texture0, texture_sampler, uv + vec2<f32>(0.0, texel.y), mode.mip_bias).a,
        textureSampleBias(texture0, texture_sampler, uv - vec2<f32>(0.0, texel.y), mode.mip_bias).a,
    );
    let slope = vec2<f32>(h.x - h.y, h.z - h.w) * CRACK_BUMP;
    let side2 = cross(dp2, n);
    let side1 = cross(n, dp1);
    var t = side2 * duv1.x + side1 * duv2.x;
    var b = side2 * duv1.y + side1 * duv2.y;
    let scale = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-20));
    t = t * scale;
    b = b * scale;
    return vec4<f32>(normalize(n - slope.x * t - slope.y * b), clamp(length(slope), 0.0, 1.0));
}

// The sun's glint on the crack's facets (ours): a tight highlight of the bumped normal.
fn crack_glint(n: vec3<f32>, view: vec3<f32>) -> f32 {
    let to_sun = -frame.sun.xyz;
    let h = normalize(to_sun + view);
    return 2.0 * pow(max(dot(n, h), 0.0), 90.0) * step(0.0, dot(n, to_sun));
}

// The dirt's colour (ours): `c3.xyz` comes from `m_unkC0`, which nothing writes (`notes/formats.md` "Shaders and vertex layouts"); this is the manager's
// unused preset `m_dirtColor` (0.27, 0.22, 0.14).
const DIRT_COLOUR: vec3<f32> = vec3<f32>(0.27, 0.22, 0.14);

// The panes' mud film (ours, `dirt::grime`): its patches per unit of uv, the most it covers, and the share of the
// reflection the wet mud keeps.
const GRIME_SCALE: f32 = 12.0;

const GRIME_MOST: f32 = 0.55;

const GRIME_SHEEN: f32 = 0.4;

// The renderer's car lighting (the game's needs constants with unproven sources, `notes/formats.md` "Shaders and vertex layouts"): ambient Ka, with a
// little more from above than below, plus Kd by the sun's angle, and a Cook-Torrance-shaped highlight
// (`CookTorranceKs`, `CookTorranceM`) in the sun's colour. `LIGHT_SCALE` maps the settings' 0.5 colours and 1.7 to 1.9
// scales to about 1 in full sun.
const LIGHT_SCALE: f32 = 0.5;

// The car paint's reflection with the HDR post pass (renderer addition, `Enhance::hdr`): its share of the game's
// Fresnel weight (whose FresnelMin of about 0.3 makes the paint a mirror head on), and the reflection cube's mip level,
// so it reads as a clear coat rather than chrome.
const PAINT_GLOSS: vec2<f32> = vec2<f32>(0.75, 1.0);

fn lit(colour: vec3<f32>, n: vec3<f32>, view: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let to_sun = -frame.sun.xyz;
    let ndl = max(dot(n, to_sun), 0.0) * sunlit;
    if (day_lit()) {
        // Lambert on the texture's albedo plus the sun's Cook-Torrance highlight, in linear light.
        let e = light_sum(n, sunlit, ambient, world, vec2<f32>(1.0));
        let albedo = min(pow(colour * CAR_ALBEDO, vec3<f32>(2.2)), vec3<f32>(ALBEDO_MOST));
        return pow(albedo * e + sun_highlight(n, view, to_sun) * frame.day_sun.rgb * ndl, vec3<f32>(1.0 / 2.2));
    }
    let h = normalize(to_sun + view);
    let nh = max(dot(n, h), 1e-3);
    let m2 = max(mode.light.w * mode.light.w, 1e-3);
    let highlight = mode.light.z * exp((1.0 - 1.0 / (nh * nh)) / m2) * step(0.0, ndl) * min(ndl * 4.0, 1.0);
    return LIGHT_SCALE * (colour * light(n, sunlit, ambient, world) + 4.0 * highlight * frame.diffuse.rgb);
}

// The texture's colour's scale to the albedo the day's light takes (ours).
const CAR_ALBEDO: f32 = 1.0;

// The highest albedo (fresh snow's; white paint is about 0.8).
const ALBEDO_MOST: f32 = 0.9;

// The highlight's roughness (Beckmann's m) where the game's class has none (`CookTorranceKs` 0), and its least.
const MATTE: f32 = 0.6;

const LEAST_ROUGHNESS: f32 = 0.1;

// Whether the lit draws take the day's light (`own_light`'s): with it, outside the car (the in-car view's interior keeps
// the game's light, or the cabin's own).
fn day_lit() -> bool {
    return day_on() && !in_car();
}

// The sun's specular reflection off a dielectric (F0 0.04) of the class's roughness (`CookTorranceM`), Beckmann's
// distribution with the V-cavity shadowing and Schlick's Fresnel, times pi (the day's sun is its illuminance over pi).
fn sun_highlight(n: vec3<f32>, view: vec3<f32>, to_sun: vec3<f32>) -> f32 {
    let m = select(MATTE, max(mode.light.w, LEAST_ROUGHNESS), mode.light.z > 0.0);
    let h = normalize(to_sun + view);
    let nh = max(dot(n, h), 1e-3);
    let nv = max(dot(n, view), 1e-3);
    let nl = max(dot(n, to_sun), 1e-3);
    let vh = max(dot(view, h), 1e-3);
    let m2 = m * m;
    let nh2 = nh * nh;
    let d = exp((nh2 - 1.0) / (m2 * nh2)) / (SKY_PI * m2 * nh2 * nh2);
    let g = min(1.0, 2.0 * nh * min(nv, nl) / vh);
    let f = 0.04 + 0.96 * pow(1.0 - vh, 5.0);
    return SKY_PI * d * g * f / (4.0 * nv * nl);
}

// The ambient and sun light of `lit` at `world`, without the highlight, in display level.
fn light(n: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    if (day_lit()) {
        return pow(light_sum(n, sunlit, ambient, world, vec2<f32>(1.0)), vec3<f32>(1.0 / 2.2)) * CAR_ALBEDO / LIGHT_SCALE;
    }
    return light_sum(n, sunlit, ambient, world, mode.light.xy);
}

// The ambient and sun light at `world` scaled by `k` (the game's Ka and Kd): the game's, or with the day's light
// (`day_lit`) linear irradiance as the stage's (`own_light`).
fn light_sum(n: vec3<f32>, sunlit: f32, ambient: vec3<f32>, world: vec3<f32>, k: vec2<f32>) -> vec3<f32> {
    let ndl = max(dot(n, -frame.sun.xyz), 0.0) * sunlit;
    let day = day_lit();
    let sky = select(frame.ambient.rgb, frame.day_sky.rgb, day);
    let sun = select(frame.diffuse.rgb, frame.day_sun.rgb, day);
    var hemisphere = vec3<f32>(0.8 + 0.2 * n.y);
    // Outside the car (the in-car view's interior keeps the game's light, or the cabin's own).
    if (sky_on() && !in_car()) {
        hemisphere = sky_ambient(n);
    }
    // With the ground's maps (`bounce.rs`) the sky above the horizon and the ground below it apart: the ground's light
    // where the face looks, by its sun and sky.
    var bounce = vec3<f32>(0.0);
    if (!in_car() && frame.ground_grid.z != 0.0) {
        hemisphere = open_sky_light(n, n);
        let ground_sky = open_sky_light(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0));
        let g = ground_bounce(world, n);
        bounce = BOUNCE * (k.y * sun * g.sun + k.x * sky * ground_sky * g.sky);
    }
    return k.x * sky * hemisphere * ambient + k.y * sun * ndl + bounce;
}

fn ground_bounce(world: vec3<f32>, n: vec3<f32>) -> GroundLight {
    var light = GroundLight(vec3<f32>(0.0), vec3<f32>(0.0));
    let seen = 0.25 * (1.0 - n.y);
    for (var k = 0u; k < 2u; k++) {
        let xz = world.xz + n.xz * select(1.5, 4.0, k == 1u);
        var g = ground_at(xz);
        if (frame.occlusion.w != 0.0) {
            let q = vec3<f32>(xz.x, frame.occlusion.z, xz.y);
            g.sun *= 1.0 - car_shadow(q);
            g.sky *= 1.0 - car_contact(q);
        }
        light.sun += g.sun * seen;
        light.sky += g.sky * seen;
    }
    return light;
}

// The tiles' side, metres (`sky_occlusion::TILE`), and the range of the ground maps' shares (`RANGE` in `bounce.wgsl`).
const GROUND_TILE: f32 = 1000.0;

const GROUND_RANGE: f32 = 2.0;

// The ground's light seen from above at `xz` (`bounce.rs`): its linear albedo times its share of the sun and of the
// sky, 0 off the maps.
fn ground_at(xz: vec2<f32>) -> GroundLight {
    let grid = frame.ground_grid;
    let at = (xz - grid.xy) / GROUND_TILE;
    let cell = floor(at);
    if (any(cell < vec2<f32>(0.0)) || cell.x >= grid.z || cell.y >= grid.w) {
        return GroundLight(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let pair = ground_tiles[u32(cell.y * grid.z + cell.x)];
    if (pair < 0) {
        return GroundLight(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let uv = at - cell;
    let sun = textureSampleLevel(ground_light, reflection_sampler, uv, pair * 2, 0.0);
    let sky = textureSampleLevel(ground_light, reflection_sampler, uv, pair * 2 + 1, 0.0);
    return GroundLight(sun.rgb * sun.a * GROUND_RANGE, sky.rgb * sky.a * GROUND_RANGE);
}

// The car's light from its surroundings (renderer choice): the reflection cube blurred to 4 texels a face (mip 5)
// around the normal, relative to the open sky there, scales the ambient (`rgb`, `AMBIENT_TINT` of it by colour, so
// the ground and trees tint it); the cube toward the sun blurred to 16 texels a face (mip 3), relative to the sky
// there, gives how much of the sun reaches the car past trees and hills (`a`; the blur spreads the sun's disc, so an
// open sky measures about 0.4 and a forest about 0.1), at least `SUN_LEAST` (light through the leaves). All 1 before
// the cube is drawn.
const AMBIENT_TINT: f32 = 0.5;

// The interior's ambient in the in-car view (ours): the game's car constants are not traced (`notes/formats.md` "Shaders and vertex layouts"), and at 1
// the dash is about a third as bright as in a 1.02 capture.
const INTERIOR_AMBIENT: f32 = 2.5;

const SUN_SEEN: vec2<f32> = vec2<f32>(0.1, 0.35);

const SUN_LEAST: f32 = 0.35;

fn surroundings(n: vec3<f32>) -> vec4<f32> {
    if (frame.reflection.x == 0.0) {
        return vec4<f32>(1.0);
    }
    let luma = vec3<f32>(0.299, 0.587, 0.114);
    // With the ground's maps the ground below is their light (`ground_bounce`): the cube is then taken against the open
    // sky where the face sees the sky, halfway between its normal and up.
    var look = n;
    if (frame.ground_grid.z != 0.0) {
        let half = n + vec3<f32>(0.0, 1.0, 0.0);
        look = select(vec3<f32>(0.0, 1.0, 0.0), normalize(half), dot(half, half) > 1e-4);
    }
    let around = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, look, 5.0).rgb);
    // With the physically based sky, the open sky as blurred as the cube's level (`sky_reflected`).
    let open = open_sky(look, 1.0);
    let ambient = mix(vec3<f32>(dot(around, luma) / max(dot(open, luma), 1e-3)), around / max(open, vec3<f32>(1e-3)), AMBIENT_TINT);
    let to_sun = -frame.sun.xyz;
    let toward = ungraded(textureSampleLevel(surroundings_cube, reflection_sampler, to_sun, 3.0).rgb);
    let seen = dot(toward, luma) / max(dot(open_sky(to_sun, 0.6), luma), 1e-3);
    return vec4<f32>(clamp(ambient, vec3<f32>(0.3), vec3<f32>(1.2)), mix(SUN_LEAST, 1.0, smoothstep(SUN_SEEN.x, SUN_SEEN.y, seen)));
}

// The open sky along `dir`: `sky_reflected` by `roughness` under the physically based sky, else `sky`. A branch, not a
// `select`, which would work out both.
fn open_sky(dir: vec3<f32>, roughness: f32) -> vec3<f32> {
    if (sky_on()) {
        return sky_reflected(dir, roughness);
    }
    return sky(dir, 1.0);
}

// The stage's shadow on a lit draw seen from outside (the car's body, the characters), 1 in the sun: the sun's cascades
// where they reach and the sun can reach the face (trees and banks cast, so the light dapples over the paint), fading to
// the surroundings cube's sun occlusion `occlusion` (`surroundings`) beyond them, there in the world layer's shadow, and
// under an overcast sky.
fn stage_sun(world: vec3<f32>, n: vec3<f32>, occlusion: f32) -> f32 {
    let near = sun_reach(world);
    if (is_overcast() || dot(n, -frame.sun.xyz) <= 0.0) {
        return occlusion;
    }
    let far = world_lit(world, n) * occlusion;
    if (near == 0.0) {
        return far;
    }
    return mix(far, sun_lit(world, n), near);
}

// What the car reflects along `dir`: the stage around it from the reflection cube, whose texels are already through
// the tone curve (so they are taken back through its inverse), or the sky before the cube is drawn.
fn reflected(dir: vec3<f32>, level: f32) -> vec3<f32> {
    if (frame.reflection.x == 0.0) {
        if (sky_on()) {
            return clouded(sky_reflected(dir, level / 7.0), dir);
        }
        return clouded(sky(dir, 1.0), dir);
    }
    return ungraded(textureSampleLevel(reflection_cube, reflection_sampler, dir, level).rgb);
}
