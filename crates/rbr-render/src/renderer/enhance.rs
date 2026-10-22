//! The renderer's additions beyond the game's look ([`Enhance`]).

/// Renderer additions beyond the game's look, off by default, for comparing against it (`viewer --enhance`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Enhance {
    /// 16x anisotropic filtering on the stage textures (the ground at grazing angles stays sharp further out).
    pub anisotropy: bool,
    /// The cabin lit by the sky it sees and by the sun through the glass, in the in-car view (`cabin.wgsl`,
    /// [`crate::car::cabin`]; ours), and the car's glass drawn from both sides ([`crate::car::Car::add_inner_glass`]).
    pub cabin: bool,
    /// Moving clouds as much as the section's cloud word says, in place of the game's cloud faces, and their shadows on
    /// the stage under clear skies (`clouds.wgsl`,
    /// [`Atmosphere::cloud_cover`](crate::atmosphere::Atmosphere::cloud_cover); ours).
    pub clouds: bool,
    /// The car's soft contact shadow, its own shape seen from above ([`GROUND_REACH`](super::shadows::GROUND_REACH)),
    /// on the ground and the fences under it, in place of the game's shadow decal
    /// ([`Renderer::set_car_shadow`](super::Renderer::set_car_shadow)).
    pub contact: bool,
    /// The stage's soft ground decals (the RSF stages' painted skid marks) alpha blended, as the game's object blend
    /// state, in place of the 60/255 cut that turns them solid ([`crate::decals`]).
    pub decals: bool,
    /// The frame in linear float colour, with bloom and exposure before the tone curve ([`Post`](crate::post::Post)).
    pub hdr: bool,
    /// Our tyre marks in place of the game's skid marks: the tread's width swept along the travel, a rut, rubber,
    /// a groove or flattened grass by the surface, longer trails ([`crate::tyre_marks`]).
    pub marks: bool,
    /// Aerial perspective: exponential, bluer haze in the sky's colour, brighter toward the sun (`fogged` in
    /// `shader.wgsl`).
    pub sky: bool,
    /// Real-time sun shadows on the stage near the eye, under clear skies ([`SunShadow`](crate::sun::SunShadow)), and
    /// sunlit foliage glowing against the sun.
    pub sun: bool,
    /// The stage's ambient occlusion computed from its geometry at load, in place of the bake's vertex light
    /// ([`crate::sky_occlusion`]; ours).
    pub occlusion: bool,
    /// Rain and snow drawn on the GPU about the eye, rings where rain lands on wet roads, and particles lit by the sun
    /// and soft where they meet the stage ([`Effects`](crate::effects::Effects);
    /// [`Renderer::set_precipitation`](super::Renderer::set_precipitation) sets the weather).
    pub particles: bool,
    /// The stage's rivers and lakes flowing along their texture scroll, rippling with time, shaded by their vertex
    /// light and fading into their banks by their vertex alpha (`water_flowing` in `water.wgsl`; ours).
    pub water: bool,
    /// Puddles on the wet film of the roads in wet weather, larger in the ground's low spots ([`crate::puddles`];
    /// `wet_plus` in `water.wgsl`; ours).
    pub wet: bool,
    /// The cockpit's mirrors drawn as mirrors, and how often ([`crate::mirrors`]; ours, `viewer --mirrors`, not in
    /// [`Enhance::NAMES`]).
    pub mirrors: crate::mirrors::Rate,
    /// The frame's anti-aliasing (`viewer --anti-aliasing`, not in [`Enhance::NAMES`]).
    pub anti_aliasing: AntiAliasing,
}

/// The frame's anti-aliasing: 4x multisampling ([`SAMPLE_COUNT`](super::SAMPLE_COUNT)) or one sample a pixel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AntiAliasing {
    /// 4x multisampling, the cut-outs' edges smoothed by alpha to coverage.
    #[default]
    Msaa4,
    /// One sample a pixel, the cut-outs alpha-tested as in 1.02: cheaper, with jagged edges.
    Off,
}

impl AntiAliasing {
    /// The choices offered, in the launcher's order.
    pub const ALL: [AntiAliasing; 2] = [AntiAliasing::Msaa4, AntiAliasing::Off];

    /// `4x` or `off`.
    pub fn parse(text: &str) -> Option<AntiAliasing> {
        AntiAliasing::ALL.into_iter().find(|a| a.name() == text)
    }

    /// Its [`AntiAliasing::parse`] name.
    pub fn name(self) -> &'static str {
        match self {
            AntiAliasing::Msaa4 => "4x",
            AntiAliasing::Off => "off",
        }
    }

    /// The colour and depth targets' samples.
    pub fn samples(self) -> u32 {
        match self {
            AntiAliasing::Msaa4 => super::SAMPLE_COUNT,
            AntiAliasing::Off => 1,
        }
    }
}

/// Names of additions taken out, still accepted from old settings and commands: `lens`, the lens dirt (2026-10-18);
/// `ao`, the ambient occlusion, `msaa`, multisampling under the temporal anti-aliasing, `taa`, the temporal
/// anti-aliasing, and `wind`, the wind in the foliage (2026-10-19).
pub const REMOVED: [&str; 5] = ["lens", "ao", "msaa", "taa", "wind"];

impl Enhance {
    pub const NAMES: [&str; 13] = [
        "anisotropy",
        "cabin",
        "clouds",
        "contact",
        "decals",
        "hdr",
        "marks",
        "occlusion",
        "particles",
        "sky",
        "sun",
        "water",
        "wet",
    ];

    /// A comma-separated list of [`Enhance::NAMES`], or `all`. [`REMOVED`] names are ignored.
    pub fn parse(list: &str) -> Option<Enhance> {
        let mut enhance = Enhance::default();
        for name in list.split(',') {
            match name {
                "all" => {
                    enhance = Enhance {
                        anisotropy: true,
                        cabin: true,
                        clouds: true,
                        contact: true,
                        decals: true,
                        hdr: true,
                        marks: true,
                        occlusion: true,
                        sky: true,
                        sun: true,
                        particles: true,
                        water: true,
                        wet: true,
                        mirrors: enhance.mirrors,
                        anti_aliasing: enhance.anti_aliasing,
                    }
                }
                "anisotropy" => enhance.anisotropy = true,
                "cabin" => enhance.cabin = true,
                "clouds" => enhance.clouds = true,
                "contact" => enhance.contact = true,
                "decals" => enhance.decals = true,
                "hdr" => enhance.hdr = true,
                "marks" => enhance.marks = true,
                "occlusion" => enhance.occlusion = true,
                name if REMOVED.contains(&name) => {}
                "particles" => enhance.particles = true,
                "sky" => enhance.sky = true,
                "sun" => enhance.sun = true,
                "water" => enhance.water = true,
                "wet" => enhance.wet = true,
                _ => return None,
            }
        }
        Some(enhance)
    }
}
