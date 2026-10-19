//! The renderer's additions beyond the game's look ([`Enhance`]).

/// Renderer additions beyond the game's look, off by default, for comparing against it (`viewer --enhance`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Enhance {
    /// 16x anisotropic filtering on the stage textures (the ground at grazing angles stays sharp further out).
    pub anisotropy: bool,
    /// The cabin lit by the sky it sees and by the sun through the glass, in the in-car view (`cabin.wgsl`,
    /// [`crate::car::cabin`]; ours).
    pub cabin: bool,
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
    /// Aerial perspective: exponential, bluer haze in the sky's colour, brighter toward the sun (`fogged` in
    /// `shader.wgsl`).
    pub sky: bool,
    /// Real-time sun shadows on the stage near the eye, under clear skies ([`SunShadow`](crate::sun::SunShadow)), and
    /// sunlit foliage glowing against the sun.
    pub sun: bool,
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
    /// The cockpit's mirrors drawn as mirrors, and how often ([`crate::mirrors`]; ours, `viewer --mirrors`, not in
    /// [`Enhance::NAMES`]).
    pub mirrors: crate::mirrors::Rate,
}

/// Names of additions taken out, still accepted from old settings and commands: `lens`, the lens dirt (2026-10-18);
/// `ao`, the ambient occlusion, `msaa`, multisampling under the temporal anti-aliasing, `taa`, the temporal
/// anti-aliasing, and `wind`, the wind in the foliage (2026-10-19).
pub const REMOVED: [&str; 5] = ["lens", "ao", "msaa", "taa", "wind"];

impl Enhance {
    pub const NAMES: [&str; 10] =
        ["anisotropy", "cabin", "clouds", "contact", "hdr", "particles", "sky", "sun", "water", "wet"];

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
                        hdr: true,
                        sky: true,
                        sun: true,
                        particles: true,
                        water: true,
                        wet: true,
                        mirrors: enhance.mirrors,
                    }
                }
                "anisotropy" => enhance.anisotropy = true,
                "cabin" => enhance.cabin = true,
                "clouds" => enhance.clouds = true,
                "contact" => enhance.contact = true,
                "hdr" => enhance.hdr = true,
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
