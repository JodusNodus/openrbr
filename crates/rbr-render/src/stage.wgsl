// The stage's shading (`shade_stage`, ours and the game's): the pipelines of the stage's draws, which `lit_class` 0
// marks, take this module (`renderer::pipelines::STAGE_SHADER`). The shared code is in `shader.wgsl`.

@fragment
fn fs_main(v: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    pixel = vec2<i32>(v.clip.xy);
    return shade_stage(v, front, 0.0);
}

@fragment
fn fs_clouded(c: CloudedOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    pixel = vec2<i32>(c.clip.xy);
    let v = VertexOut(c.clip, c.color, c.uv0, c.uv1, c.uv2, c.world, c.normal, c.specular, c.sky, c.bounce_sun, c.bounce_sky);
    return shade_stage(v, front, c.cloud);
}

fn water_draw() -> bool {
    return select(mode.water != 0u, DRAW_WATER, KNOWN);
}

fn specular_draw() -> bool {
    return select(mode.specular != 0u, DRAW_SPECULAR, KNOWN);
}

fn detail_draw() -> bool {
    return select(mode.mip_bias != 0.0, DRAW_DETAIL, KNOWN);
}

fn reflect_water() -> bool {
    if (REFLECT_WATER > 1u) {
        return mode.water != 0u;
    }
    return REFLECT_WATER == 1u;
}

fn is_wet() -> bool {
    return select(frame.wetness.x >= 1.0, WET, WEATHER_KNOWN);
}

// A stage draw's colour (`lit_class` 0: the stage, its props and characters' cards), with `under_cloud` of the sun taken
// by a cloud.
fn shade_stage(v: VertexOut, front: bool, under_cloud: f32) -> vec4<f32> {
    let first = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias);
    if (PERF_SHADE == 1.0) {
        var a = first.a;
        if (ALPHA_TEST && mode.alpha_test != 0u && a <= mode.alpha_ref) { discard; }
        if (mode.coverage != 0u) { a = clamp((a - mode.alpha_ref) / max(fwidth(a), 1e-4) + 0.5, 0.0, 1.0); }
        return vec4<f32>(v.color.rgb * first.rgb, a);
    }
    if (WATER_PLUS && water_draw() && frame.enhance.z != 0.0) {
        let surface = water_flowing(v);
        return vec4<f32>(graded(fogged(surface.rgb, v.world)), surface.a);
    }
    if (water_draw()) {
        let surface = water_surface(v.world);
        return vec4<f32>(graded(fogged(surface.rgb, v.world)), surface.a);
    }
    // The textures' colour, then lit by the vertex light.
    var surface = first.rgb;
    var alpha = first.a;
    var second = vec4<f32>(0.0);
    if (second_draw()) {
        second = textureSampleBias(texture1, texture_sampler, v.uv1, mode.mip_bias);
        surface = mix(first.rgb, second.rgb, second.a * v.color.a);
    }
    // The backdrop (the world-anchored sky faces' frame) gets detail its stretched texture lacks.
    if (!stage_frame() && !camera_anchored()) {
        surface = surface * backdrop_detail(v.world);
    }
    let night = is_night();
    // With the computed occlusion the stage's light is ours (`own_light`), its sun shadowed by what hides the sun.
    let own = computed_occlusion() && day_on() && !camera_anchored();
    var sun_seen = 1.0;
    var colour: vec3<f32>;
    if (own) {
        // The sun's or, once it is down, the moon's share past the shadows.
        sun_seen = own_sun_seen(v, under_cloud);
        // Lit in linear light (`own_light`), then back to the display colour the rest of the frame works in.
        colour = pow(own_light(v, sun_seen, under_cloud) * pow(surface, vec3<f32>(2.2)), vec3<f32>(1.0 / 2.2));
    } else {
        colour = relit(v) * surface;
    }
    if (own && specular_draw() && !(WATER_PLUS && frame.enhance.w != 0.0 && is_wet())) {
        colour = with_specular(colour, v, front, sun_seen);
    }
    // The game's highlight off the road's specular map: the stones' detail on gravel, so kept beside our reflection.
    // With our light, as strong as the direct light against noon's: none once the sun has set and the moon is down.
    if (specular_draw() && !night) {
        var shine = road_shine(v);
        if (own) {
            shine *= sun_seen * min(dot(frame.day_sun.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)) / NOON_DIRECT, 1.0);
        }
        colour = colour + shine;
    }
    // The shadow light map multiplies the colour (`color = (base + spec) * shadow.rgb`, `notes/formats.md` "Shaders and vertex layouts"). With the
    // sun's shadows on (`sun_shadow.w`) it is not used: their cascades and far layer cast for any sun, and the moon.
    let baked = select(textureSample(shadow_texture, texture_sampler, v.uv2).r, 1.0, frame.sun_shadow.w != 0.0);
    // How much the sun's shadows reach here (`sun_light` below fades out with it).
    let near_sun = sun_reach(v.world);
    // The car's lamps (`lamp_sum`) at night and at dusk (`frame.dusk.w`, ours), as at night over the fading daylight:
    // with our light in lux (`lamp_lux`: over pi a white face's luminance, pre-exposed), added in linear light.
    var lamps = vec3<f32>(0.0);
    if (LAMPS && !camera_anchored() && (night || frame.dusk.w > 0.01)) {
        let has_normal = dot(v.normal, v.normal) > 1e-8;
        let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
        lamps = lamp_sum(v.world, n, mode.coverage != 0u);
    }
    if (own) {
        let lux = lamp_lux(lamps) * frame.day_sun.w / SKY_PI;
        colour = pow(pow(colour, vec3<f32>(2.2)) + pow(surface, vec3<f32>(2.2)) * lux, vec3<f32>(1.0 / 2.2));
    } else if (night) {
        colour = night_lit(surface, baked, v, front, lamps);
    } else {
        colour = colour * baked + lamp_albedo(surface, v) * lamp_knee(lamps) * frame.dusk.w;
    }
    if (WATER_PLUS && specular_draw() && frame.enhance.w != 0.0 && is_wet()) {
        colour = wet_plus(colour, v);
    } else if (specular_draw() && is_wet()) {
        colour = wet_film(colour, v);
    }
    // The near ground (the unlit draws with the mip bias) gets fine detail where its textures are blurred.
    if (detail_draw()) {
        colour = colour * ground_detail(v.world);
        // In wet weather the ground around the roads is soaked too, a little less (`GROUND_SOAK` of the road's).
        if (is_wet() && !specular_draw()) {
            colour = soak(colour, GROUND_SOAK);
        }
    }
    if (own && !is_overcast()) {
        // Our light holds the sun's, the car's and the clouds' shadows (`own_sun_seen`); the contact shadow and the
        // leaves' glow stay.
        if (frame.ground.x != 0.0) {
            colour = colour * (1.0 - frame.ground.x * car_contact(v.world));
        }
        if (near_sun > 0.0 && frame.skylight.y > 0.0) {
            colour = colour * (1.0 + leaf_glow(v, sun_seen, first.rgb) * near_sun);
        }
    } else {
        // The sun shadow fades where the point sees little sky (in the shade of trees or banks).
        let open_sky = smoothstep(0.3, 0.8, sky_share(v));
        if (frame.shadow.x * open_sky != 0.0) {
            colour = colour * (1.0 - frame.shadow.x * car_shadow(v.world) * open_sky);
        }
        if (frame.ground.x != 0.0) {
            colour = colour * (1.0 - frame.ground.x * car_contact(v.world));
        }
        // Under a cloud's shadow (`clouds.wgsl`) the sun's own shadows fade with the sunlight they take away.
        colour = colour * (1.0 - CLOUD_SHADE * under_cloud * mix(0.4, 1.0, open_sky));
        // At night the moon's shadows are in `night_lit`, off the lamps' light.
        if (near_sun > 0.0 && !night) {
            colour = colour * mix(vec3<f32>(1.0), sun_light(v, front, open_sky, first.rgb), near_sun * (1.0 - under_cloud));
        }
    }
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    if (mode.coverage != 0u) {
        alpha = clamp((alpha - mode.alpha_ref) / max(fwidth(alpha), 1e-4) + 0.5, 0.0, 1.0);
    }
    // Camera-anchored draws (the clouds, `frame.fog_range.z` 1) fade out toward the horizon and take on the sky's
    // colour there, so the cloud domes have no visible rim (renderer choice). Their texture and vertex colour are
    // lit by `CLOUD_LIGHT`, dimmed at dusk as the sky is (`sky_light`; 1 without `--clock`), so that they and the water
    // and wet roads that reflect them do not stay bright over a darkened stage (ours). Under fog they fade into the sky by `frame.haze.y` (the game fogs them by
    // `SkyboxSaturation`, `Atmosphere::cloud_haze`).
    if (camera_anchored()) {
        let dir = normalize(v.world);
        // With the physically based sky the domes' brightest clouds stay below white (`SKY_CLOUD_LIGHT`), so that
        // the sky through the canopy is not a white hole.
        colour = colour * select(CLOUD_LIGHT, SKY_CLOUD_LIGHT, sky_on()) * day_level();
        if (frame.clouds.y != 0.0) {
            colour = cloud_face_lit(colour, alpha, dir);
        }
        alpha = alpha * smoothstep(0.03, 0.2, dir.y);
        colour = mix(colour, sky(dir, 0.0), 0.5 * (1.0 - smoothstep(0.0, 0.35, dir.y)));
        colour = mix(colour, sky(dir, 0.0), frame.haze.y);
    }
    return vec4<f32>(graded(fogged(colour, v.world)), alpha);
}

fn own_light(v: VertexOut, sun_seen: f32, under_cloud: f32) -> vec3<f32> {
    let c = v.color.rgb;
    let most = max(c.r, max(c.g, c.b));
    let hue = mix(vec3<f32>(1.0), c / max(most, 1e-3), smoothstep(0.02, 0.08, most));
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    let ground_sky = open_sky_light(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0));
    let sky_unit = frame.day_sky.rgb;
    let sun_unit = frame.day_sun.rgb;
    let sky = sky_unit * (1.0 - v.sky.z) * open_sky_light(n, sky_direction(v.sky.xy, n)) / max(ground_sky, vec3<f32>(1e-4));
    let sun = sun_unit * max(dot(n, -frame.sun.xyz), 0.0) * sun_seen;
    let bounce = BOUNCE * (sky_unit * v.bounce_sky + sun_unit * v.bounce_sun * (1.0 - under_cloud));
    // The contact shading on the sky's and the bounce's light, never the sun's (`contact_light`).
    var contact = 1.0;
    if (SUN_MASK && CONTACT) {
        contact = textureLoad(sun_contact, pixel / 2, 0).r;
    }
    return pow(hue, vec3<f32>(2.2)) * ((sky + bounce) * contact + sun);
}

// The share of the sun `v` gets past the stage (the cascades where they reach, past them the world layer, when the sky
// is not overcast), the car (its shadow map) and the clouds (`under_cloud`).
fn own_sun_seen(v: VertexOut, under_cloud: f32) -> f32 {
    var seen = 1.0 - under_cloud;
    if (frame.shadow.x != 0.0) {
        seen *= 1.0 - car_shadow(v.world);
    }
    let near = sun_reach(v.world);
    if (seen > 0.0 && !is_overcast()) {
        let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
        if (near > 0.0) {
            seen *= mix(world_lit(v.world, n), sun_lit(v.world, n), near);
        } else {
            seen *= world_lit(v.world, n);
        }
    }
    return seen;
}

// What the sun does to an unlit stage draw's colour (renderer addition), as a factor: its shadow darkens by
// `frame.sun_shadow.x` in full shadow and on faces turned from the sun (terrain only; foliage cards take light from
// both sides), less where the point sees little sky (`open_sky`) and on foliage, whose cards shadow each
// other. Sunlit plants (`foliage.rs`, `specular.z` 1) seen against the sun let it through their leaves (thin-surface
// transmission, ours): the sunlight's colour filtered by the leaf's (`albedo`, its hue at full strength), in a
// lobe about the sun (`TRANSLUCENCY`, `pow 4` of the view-to-sun angle), fading as the sun nears the horizon and at
// most `GLOW_MOST`, so that a low sun in the view does not wash the canopy out.
const TRANSLUCENCY: f32 = 1.2;

const GLOW_MOST: f32 = 0.8;

fn sun_light(v: VertexOut, front: bool, open_sky: f32, albedo: vec3<f32>) -> vec3<f32> {
    let has_normal = dot(v.normal, v.normal) > 1e-8;
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal) * select(-1.0, 1.0, front), has_normal);
    let overcast = is_overcast();
    // Under the relight (`relit`) the vertex light already turns from the sun. Faces turned from it skip the cascades.
    var facing = 1.0;
    if (has_normal && mode.coverage == 0u && !overcast && frame.relight.w == 0.0) {
        facing = smoothstep(-0.05, 0.25, dot(n, -frame.sun.xyz));
    }
    var lit = 0.0;
    if (facing > 0.0) {
        lit = sun_lit(v.world, n) * facing;
    }
    let receive = select(1.0, 0.5, mode.coverage != 0u) * mix(0.4, 1.0, open_sky);
    var glow = vec3<f32>(0.0);
    if (!overcast) {
        glow = leaf_glow(v, lit, albedo);
    }
    return (1.0 - frame.sun_shadow.x * (1.0 - lit) * receive) * (1.0 + glow);
}

// A sunlit plant's glow against the sun (`sun_light`), `lit` its share of the sun.
fn leaf_glow(v: VertexOut, lit: f32, albedo: vec3<f32>) -> vec3<f32> {
    if (mode.coverage == 0u || v.specular.z <= 0.5) {
        return vec3<f32>(0.0);
    }
    let against = max(dot(normalize(v.world - frame.eye.xyz), -frame.sun.xyz), 0.0);
    let high = smoothstep(0.03, 0.25, -frame.sun.y);
    let hue = albedo / max(max(albedo.r, max(albedo.g, albedo.b)), 1e-3);
    return min(TRANSLUCENCY * lit * pow(against, 4.0) * high, GLOW_MOST) * sunlight() * hue;
}

// The depth prepass of the alpha-tested draws (`renderer::DrawPass`): `fs_main`'s alpha test and alpha to coverage on
// the texture's alpha, in the same steps, so that the samples it covers are the ones `fs_main` would. The colour, the
// normal `own_sun_seen` takes, is written only for the sun's mask (`sun_mask.wgsl`).
@fragment
fn fs_prepass(v: VertexOut) -> @location(0) vec4<f32> {
    var alpha = textureSampleBias(texture0, texture_sampler, v.uv0, mode.mip_bias).a;
    if (ALPHA_TEST && mode.alpha_test != 0u && alpha <= mode.alpha_ref) {
        discard;
    }
    if (mode.coverage != 0u) {
        alpha = clamp((alpha - mode.alpha_ref) / max(fwidth(alpha), 1e-4) + 0.5, 0.0, 1.0);
    }
    let n = select(vec3<f32>(0.0, 1.0, 0.0), normalize(v.normal), dot(v.normal, v.normal) > 1e-8);
    return vec4<f32>(n * 0.5 + 0.5, alpha);
}

// One triangle covering the screen.
@vertex
fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
    let ndc = vec2<f32>(f32(i == 1u) * 4.0 - 1.0, f32(i == 2u) * 4.0 - 1.0);
    var out: SkyOut;
    // At the far plane (depth 0: the depth is reversed, `coords::perspective`).
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

// The view direction from the rotation-only (camera-anchored) clip matrix: the fog colour at and below the horizon,
// the zenith colour overhead, and a soft glow toward the sun under a clear sky.
@fragment
fn fs_sky(v: SkyOut) -> @location(0) vec4<f32> {
    let far = frame.inverse_view_projection * vec4<f32>(v.ndc, 0.0, 1.0);
    let near = frame.inverse_view_projection * vec4<f32>(v.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w - near.xyz / near.w);
    var colour: vec3<f32>;
    if (frame.haze.x == 1.0) {
        colour = cloud_volume_over(sky(dir, 0.0), v.ndc);
    } else if (frame.haze.x == 2.0) {
        colour = cloud_dome_over(sky(dir, 0.0), dir);
    } else {
        colour = clouded(sky(dir, 0.0), dir);
    }
    if (SKY_LUT && frame.height_fog.x > 0.0) {
        colour = sky_misted(colour, dir);
    }
    if (sky_on() && frame.enhance.x == 0.0) {
        colour = sky_shoulder(colour);
    }
    return vec4<f32>(graded(colour), 1.0);
}
