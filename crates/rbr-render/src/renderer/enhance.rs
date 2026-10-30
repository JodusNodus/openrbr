//! The renderer's additions beyond the game's look ([`Enhance`]).

/// Renderer additions beyond the game's look, off by default, for comparing against it (`viewer --enhance`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Enhance {
    /// 16x anisotropic filtering on the stage textures (the ground at grazing angles stays sharp further out).
    pub anisotropy: bool,
    /// The paved roads (the ground whose collision surface is tarmac, [`crate::puddles::mark_roads`]) take the fine grain
    /// and relief of a scanned asphalt surface under the sun, over their own colour (`road_scan` in `shader.wgsl`,
    /// [`super::road_scan`]; ours).
    pub asphalt: bool,
    /// The cabin lit by the sky it sees and by the sun through the glass, in the in-car view (`cabin.wgsl`,
    /// [`crate::car::cabin`]; ours), and the car's glass drawn from both sides ([`crate::car::Car::add_inner_glass`]).
    pub cabin: bool,
    /// The car's soft contact shadow, its own shape seen from above ([`GROUND_REACH`](super::shadows::GROUND_REACH)),
    /// on the ground and the fences under it, in place of the game's shadow decal
    /// ([`Renderer::set_car_shadow`](super::Renderer::set_car_shadow)).
    pub contact: bool,
    /// The stage's soft ground decals (the RSF stages' painted skid marks) alpha blended, as the game's object blend
    /// state, in place of the 60/255 cut that turns them solid ([`crate::decals`]).
    pub decals: bool,
    /// The loose roads (the ground whose collision surface is gravel or dirt, [`crate::puddles::mark_roads`]) take the
    /// fine grain and relief of a scanned gravel surface under the sun, over their own colour (`road_scan` in
    /// `shader.wgsl`, [`super::road_scan`]; ours).
    pub gravel: bool,
    /// The off-road ground (the ground whose collision surface is neither road nor snow, [`crate::puddles::mark_ground`])
    /// takes the grain, colour and relief of a scanned grass and earth surface, triplanar and lit by the sun, over its
    /// own mean colour, and the stage's baked vertex tint is eased (`ground_scan` in `shader.wgsl`,
    /// [`super::road_scan`]; ours).
    pub ground: bool,
    /// A grass field over the ground near the road, streamed about the eye ([`crate::grass`]; ours, a proof of concept:
    /// not in `all`).
    pub grass: bool,
    /// Our lighting in place of the game's look: the physically based sky, its haze and fog, sun shafts and volumetric
    /// clouds with their shadows ([`crate::skylight`], [`crate::renderer::cloud_volume`]), real-time sun shadows near
    /// the eye and sunlit foliage ([`SunShadow`](crate::sun::SunShadow)), the stage's ambient occlusion computed from
    /// its geometry at load ([`crate::sky_occlusion`]) with the light the ground bounces, and the reflection probes
    /// drawn at load along the driveline.
    pub lighting: bool,
    /// Our tyre marks in place of the game's skid marks: the tread's width swept along the travel, a rut, rubber,
    /// a groove or flattened grass by the surface, longer trails ([`crate::tyre_marks`]).
    pub marks: bool,
    /// Rain and snow drawn on the GPU about the eye, rings where rain lands on wet roads, and particles lit by the sun
    /// and soft where they meet the stage ([`Effects`](crate::effects::Effects);
    /// [`Renderer::set_precipitation`](super::Renderer::set_precipitation) sets the weather), and the car's dirt by
    /// what the surfaces threw ([`CarDirt::look`](crate::dirt::CarDirt::look)).
    pub particles: bool,
    /// The snow (the ground whose collision surface is snow, [`crate::puddles::mark_roads`]) takes the fine grain and
    /// relief of a scanned snow surface under the sun, over its own colour (`road_scan` in `shader.wgsl`,
    /// [`super::road_scan`]; ours).
    pub snow: bool,
    /// The stage's rivers and lakes flowing along their texture scroll, rippling with time, shaded by their vertex
    /// light and fading into their banks by their vertex alpha (`water_flowing` in `water.wgsl`; ours).
    pub water: bool,
    /// The wet roads in wet weather: a film of water and puddles, larger in the ground's low spots, where the
    /// collision surface is road, and the ground about them soaked ([`crate::puddles`]; `wet_plus` in `water.wgsl`;
    /// ours); in damp weather a drying road, a thinner film without puddles. Off, the wet stages keep 1.02's look.
    pub wet: bool,
    /// The windscreen seen from inside as glass (ours): its Fresnel loss, a thin dust film and specks lit by the sun,
    /// wiper scratches and dried water spots (`pane` in `glass.wgsl`), and the cabin reflected in it, a mirror pass of
    /// its own every frame ([`Renderer::set_windscreen_mirror`](super::Renderer::set_windscreen_mirror)).
    pub windscreen: bool,
    /// The cockpit's mirrors drawn as mirrors, and how often ([`crate::mirrors`]; ours, `viewer --mirrors`, not in
    /// [`Enhance::NAMES`]).
    pub mirrors: crate::mirrors::Rate,
    /// The frame's anti-aliasing (`viewer --anti-aliasing`, not in [`Enhance::NAMES`]).
    pub anti_aliasing: AntiAliasing,
    /// The screen-space reflections on the wet roads and the water (`viewer --reflections`, not in
    /// [`Enhance::NAMES`]).
    pub reflections: Reflections,
    /// How much detail the additions draw at (`viewer --detail`, not in [`Enhance::NAMES`]).
    pub detail: Detail,
}

/// How much detail the additions draw at: [`Detail::High`], the frame as tuned, or [`Detail::Ultra`], finer and
/// costlier for strong GPUs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Detail {
    #[default]
    High,
    Ultra,
}

impl Detail {
    /// The choices offered, in the launcher's order.
    pub const ALL: [Detail; 2] = [Detail::High, Detail::Ultra];

    /// `high` or `ultra`.
    pub fn parse(text: &str) -> Option<Detail> {
        Detail::ALL.into_iter().find(|d| d.name() == text)
    }

    /// Its [`Detail::parse`] name.
    pub fn name(self) -> &'static str {
        match self {
            Detail::High => "high",
            Detail::Ultra => "ultra",
        }
    }
}

/// The screen-space reflections ([`ScreenReflection`](super::reflection::ScreenReflection)): high (4 rays a pixel,
/// 64 steps a ray), medium (2 and 48), low (1 and 32, grainier), or off for the reflection probes and the sky alone.
/// The costliest pass in the wet: 5 to 6.5 ms at 1080p where a road fills the view at high (2026-10-26).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Reflections {
    #[default]
    High,
    Medium,
    Low,
    Off,
}

impl Reflections {
    /// The choices offered, in the launcher's order.
    pub const ALL: [Reflections; 4] = [Reflections::High, Reflections::Medium, Reflections::Low, Reflections::Off];

    /// `high`, `medium`, `low` or `off`.
    pub fn parse(text: &str) -> Option<Reflections> {
        Reflections::ALL.into_iter().find(|r| r.name() == text)
    }

    /// Its [`Reflections::parse`] name.
    pub fn name(self) -> &'static str {
        match self {
            Reflections::High => "high",
            Reflections::Medium => "medium",
            Reflections::Low => "low",
            Reflections::Off => "off",
        }
    }

    /// The rays a pixel and the steps each takes (`SSR_RAYS`, `SSR_STEPS` in `shader.wgsl`); none when off.
    pub fn rays(self) -> Option<[u32; 2]> {
        match self {
            Reflections::High => Some([4, 64]),
            Reflections::Medium => Some([2, 48]),
            Reflections::Low => Some([1, 32]),
            Reflections::Off => None,
        }
    }
}

/// The frame's anti-aliasing: temporal, or one sample a pixel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AntiAliasing {
    /// One sample a pixel, jittered from frame to frame and gathered over the frames ([`crate::taa`]; ours): soft
    /// edges, the cut-outs' included, at about the cost of [`AntiAliasing::Off`].
    #[default]
    Taa,
    /// One sample a pixel, the cut-outs alpha-tested as in 1.02: cheaper, with jagged edges.
    Off,
}

impl AntiAliasing {
    /// The choices offered, in the launcher's order.
    pub const ALL: [AntiAliasing; 2] = [AntiAliasing::Taa, AntiAliasing::Off];

    /// `taa` or `off`.
    pub fn parse(text: &str) -> Option<AntiAliasing> {
        AntiAliasing::ALL.into_iter().find(|a| a.name() == text)
    }

    /// Its [`AntiAliasing::parse`] name.
    pub fn name(self) -> &'static str {
        match self {
            AntiAliasing::Taa => "taa",
            AntiAliasing::Off => "off",
        }
    }
}

impl Enhance {
    pub const NAMES: [&str; 15] = [
        "anisotropy",
        "asphalt",
        "cabin",
        "contact",
        "decals",
        "grass",
        "gravel",
        "ground",
        "lighting",
        "marks",
        "particles",
        "snow",
        "water",
        "wet",
        "windscreen",
    ];

    /// A comma-separated list of [`Enhance::NAMES`], or `all` (every one but `grass`).
    pub fn parse(list: &str) -> Option<Enhance> {
        let mut enhance = Enhance::default();
        for name in list.split(',') {
            match name {
                "all" => {
                    enhance = Enhance {
                        anisotropy: true,
                        asphalt: true,
                        cabin: true,
                        contact: true,
                        decals: true,
                        gravel: true,
                        ground: true,
                        grass: enhance.grass,
                        lighting: true,
                        marks: true,
                        particles: true,
                        snow: true,
                        water: true,
                        wet: true,
                        windscreen: true,
                        mirrors: enhance.mirrors,
                        anti_aliasing: enhance.anti_aliasing,
                        reflections: enhance.reflections,
                        detail: enhance.detail,
                    }
                }
                "anisotropy" => enhance.anisotropy = true,
                "asphalt" => enhance.asphalt = true,
                "cabin" => enhance.cabin = true,
                "contact" => enhance.contact = true,
                "decals" => enhance.decals = true,
                "gravel" => enhance.gravel = true,
                "ground" => enhance.ground = true,
                "grass" => enhance.grass = true,
                "lighting" => enhance.lighting = true,
                "marks" => enhance.marks = true,
                "particles" => enhance.particles = true,
                "snow" => enhance.snow = true,
                "water" => enhance.water = true,
                "wet" => enhance.wet = true,
                "windscreen" => enhance.windscreen = true,
                _ => return None,
            }
        }
        Some(enhance)
    }
}
