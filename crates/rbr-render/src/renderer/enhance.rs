//! The renderer's additions beyond the game's look ([`Enhance`]).

/// Renderer additions beyond the game's look, off by default, for comparing against it (`viewer --enhance`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Enhance {
    /// 16x anisotropic filtering on the stage textures (the ground at grazing angles stays sharp further out).
    pub anisotropy: bool,
    /// The cabin lit by the sky it sees and by the sun through the glass, in the in-car view (`cabin.wgsl`,
    /// [`crate::car::cabin`]; ours).
    pub cabin: bool,
    /// Screen-space ambient occlusion ([`AmbientOcclusion`](crate::ambient_occlusion::AmbientOcclusion)).
    pub ambient_occlusion: bool,
    /// Moving clouds as much as the section's cloud word says, in place of the game's cloud faces, and their shadows on
    /// the stage under clear skies (`clouds.wgsl`,
    /// [`Atmosphere::cloud_cover`](crate::atmosphere::Atmosphere::cloud_cover); ours).
    pub clouds: bool,
    /// The car's soft contact shadow, its own shape seen from above ([`GROUND_REACH`](super::shadows::GROUND_REACH)),
    /// on the ground and the fences under it, in place of the game's shadow decal
    /// ([`Renderer::set_car_shadow`](super::Renderer::set_car_shadow)).
    pub contact: bool,
    /// The frame in linear float colour, with bloom and exposure before the tone curve ([`Post`](crate::post::Post)).
    pub hdr: bool,
    /// 4x multisampling under [`Enhance::taa`] too (`renderer::samples`); without it the temporal anti-aliasing
    /// smooths the edges alone, at one sample a pixel.
    pub msaa: bool,
    /// Aerial perspective: exponential, bluer haze in the sky's colour, brighter toward the sun (`fogged` in
    /// `shader.wgsl`).
    pub sky: bool,
    /// Real-time sun shadows on the stage near the eye, under clear skies ([`SunShadow`](crate::sun::SunShadow)), and
    /// sunlit foliage glowing against the sun.
    pub sun: bool,
    /// Wind swaying the foliage ([`crate::foliage`]; [`Renderer::set_time`](super::Renderer::set_time) moves it).
    pub wind: bool,
    /// Temporal anti-aliasing over a still or slowly moving view ([`Taa`](crate::taa::Taa)); needs [`Enhance::hdr`].
    pub taa: bool,
    /// Rain and snow drawn on the GPU about the eye, rings where rain lands on wet roads, and particles lit by the sun
    /// and soft where they meet the stage ([`Effects`](crate::effects::Effects);
    /// [`Renderer::set_precipitation`](super::Renderer::set_precipitation) sets the weather).
    pub particles: bool,
    /// The stage's rivers and lakes flowing along their texture scroll, rippling with time, shaded by their vertex
    /// light and fading into their banks by their vertex alpha (`water_flowing` in `water.wgsl`; ours).
    pub water: bool,
    /// Wet roads with standing water in their low spots ([`crate::puddles`]) and a thinner film in damp weather
    /// (`wet_plus` in `water.wgsl`; ours).
    pub wet: bool,
}

/// Names of additions taken out (`lens`, the lens dirt, 2026-10-18), still accepted from old settings and commands.
pub const REMOVED: [&str; 1] = ["lens"];

impl Enhance {
    pub const NAMES: [&str; 14] = [
        "anisotropy",
        "ao",
        "cabin",
        "clouds",
        "contact",
        "hdr",
        "msaa",
        "particles",
        "sky",
        "sun",
        "taa",
        "water",
        "wet",
        "wind",
    ];

    /// A comma-separated list of [`Enhance::NAMES`], or `all`. [`REMOVED`] names are ignored.
    pub fn parse(list: &str) -> Option<Enhance> {
        let mut enhance = Enhance::default();
        for name in list.split(',') {
            match name {
                "all" => {
                    enhance = Enhance {
                        anisotropy: true,
                        ambient_occlusion: true,
                        cabin: true,
                        clouds: true,
                        contact: true,
                        hdr: true,
                        msaa: true,
                        sky: true,
                        sun: true,
                        taa: true,
                        wind: true,
                        particles: true,
                        water: true,
                        wet: true,
                    }
                }
                "anisotropy" => enhance.anisotropy = true,
                "ao" => enhance.ambient_occlusion = true,
                "cabin" => enhance.cabin = true,
                "clouds" => enhance.clouds = true,
                "contact" => enhance.contact = true,
                "hdr" => enhance.hdr = true,
                "msaa" => enhance.msaa = true,
                name if REMOVED.contains(&name) => {}
                "particles" => enhance.particles = true,
                "sky" => enhance.sky = true,
                "sun" => enhance.sun = true,
                "taa" => enhance.taa = true,
                "water" => enhance.water = true,
                "wet" => enhance.wet = true,
                "wind" => enhance.wind = true,
                _ => return None,
            }
        }
        Some(enhance)
    }
}
