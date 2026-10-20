//! A stage's light and haze from `Maps/TrackSettings.ini`: the sun direction, the ambient and diffuse colours, the
//! car lighting scales and the fog. The file has sections named `NN[MNEO]_[E_]preset_cloud` (for example
//! `[54E_crisp_clear]`, `[54O_E_lightrain_heavycloud]`). The game builds the name from the weather daemon's state,
//! which is not decoded (`notes/formats.md` "Shaders and vertex layouts", Clouds and sky), so the section is picked here by the stage
//! name and a preference per weather ([`section_name`]): a renderer choice, not the game's rule.
//!
//! How the values are drawn is also ours. The game's stage colour comes from its scattering shaders (`c55`-`c66`,
//! not reproduced); the renderer uses the section's linear fog (`FogStart`, `FogEnd`, `Fog*`) as the haze, also when
//! `UseFog` is false (then the values are a far, sky-blue haze that stands in for the scattering).
//!
//! `Superbowl_Scale` (settings `+0x20`) scales the backdrop's view depth in the scattering shader (`c66.x`, set by
//! the superbowl draw 0x1002f0c0; the stage's draw sets 1): [`Atmosphere::superbowl_fog`]. `LoadSkySettings`
//! (0x100526f0) copies `CloudName` to `+0x60`, past the scales. `Cloud_Scale` is not used here.

use glam::Vec3;
use rbr_formats::ini::{self, Section};
use rbr_formats::{rsf, stage_files};
use std::path::Path;

/// The light and haze a stage is drawn with.
#[derive(Debug, Clone, PartialEq)]
pub struct Atmosphere {
    /// The `TrackSettings.ini` section the values come from, `None` for the defaults.
    pub section: Option<String>,
    /// The direction the sunlight travels (`SunDir`, stage space, pointing down).
    pub sun_direction: Vec3,
    /// `Ambient{Red,Green,Blue}` and `Diffuse{Red,Green,Blue}`.
    pub ambient: Vec3,
    pub diffuse: Vec3,
    /// `Car_Ambient_Lighting` and `Car_Diffuse_Lighting`: scales of the two terms on the car.
    pub car_ambient: f32,
    pub car_diffuse: f32,
    /// `Car_Shadow_Alpha`: how much the car's sun shadow darkens the stage (0 in the overcast sections).
    pub car_shadow: f32,
    /// `Car_Deep_Shadow_Alpha`: how much the car's contact shadow darkens the ground under it (in 22 sections; the
    /// game's default, 1, elsewhere).
    pub car_deep_shadow: f32,
    /// `Fog{Red,Green,Blue}`, `FogStart` and `FogEnd`, metres from the eye. `UseFog` picks the sky colour
    /// ([`Atmosphere::zenith`]): a fogged section's sky is its fog.
    pub fog_color: Vec3,
    pub fog_start: f32,
    pub fog_end: f32,
    pub use_fog: bool,
    /// `SuperbowlFogStart` and `SuperbowlFogEnd`: the fog range of the far backdrop (the stage's sky faces, `(1,3)`,
    /// named `... SUPERBOWL.KSH`).
    pub superbowl_fog_start: f32,
    pub superbowl_fog_end: f32,
    /// `Superbowl_Scale`: the backdrop's view depth scale in the game's scattering ([`Atmosphere::superbowl_fog`]).
    pub superbowl_scale: f32,
    /// `CloudName`: the global cloud face the section shows (`stage::cloud_shows`), `None` without a section.
    pub cloud_name: Option<String>,
    /// `SkyboxSaturation`: under `UseFog`, how much of the clouds shows through the fog ([`Atmosphere::cloud_haze`]).
    pub skybox_saturation: f32,
    /// Drawn at night ([`Atmosphere::night`]).
    pub night: bool,
    /// The sun's true elevation, degrees, when a clock sets it (`relight::Clock`; `sun_direction` stays at least 2
    /// degrees up): the physically based sky's twilight (`skylight.rs`).
    pub sun_elevation: Option<f32>,
    /// The local solar hour when a clock sets it (`relight::Clock`): the height fog's morning mist lasts longer than
    /// the evening's (`height_fog.rs`).
    pub solar_hour: Option<f32>,
    /// The baked vertex light relit from its bake's sun to `sun_direction` (`relight`; ours).
    pub relight: Option<crate::relight::Relight>,
}

/// The sky's colour overhead when the section has no fog (ours; the game's sky dome colour is not reproduced).
const CLEAR_ZENITH: Vec3 = Vec3::new(0.30, 0.46, 0.74);

/// The night sky overhead (`NIGHT_ZENITH` in `night.wgsl`).
const NIGHT_ZENITH: Vec3 = Vec3::new(0.006, 0.01, 0.024);

/// The most the haze covers when the section has no fog (`UseFog = false`; ours).
const CLEAR_HAZE: f32 = 0.6;

/// The most the superbowl fog covers the backdrop's peaks under `UseFog` (ours). The game's range covers it almost
/// entirely: `SuperbowlFogStart` -6880 and `SuperbowlFogEnd` 2200 in `41O_M_heavyrain_heavycloud` leave 15 % of a
/// face at 800 m, none past 2200 m.
const SUPERBOWL_HAZE: f32 = 0.8;

/// The most the clouds fade into the sky under `UseFog` (ours). The game's `1 - SkyboxSaturation` is mostly 0.84 to 1
/// in the rain and snow sections, which leaves an almost plain sky; capped, the overcast keeps a soft structure.
const CLOUD_HAZE: f32 = 0.7;

impl Default for Atmosphere {
    /// The values most sections share (`[54E_crisp_clear]`), with a sun from the front left. Used for the car shown
    /// alone and when the file or the section is missing.
    fn default() -> Self {
        Atmosphere {
            section: None,
            sun_direction: Vec3::new(0.6, -0.6, 0.5).normalize(),
            ambient: Vec3::splat(0.5),
            diffuse: Vec3::splat(0.5),
            car_ambient: 1.74,
            car_diffuse: 1.9,
            car_shadow: 0.24,
            car_deep_shadow: 1.0,
            fog_color: Vec3::new(0.49, 0.62, 0.79),
            fog_start: 259.0,
            fog_end: 975.0,
            use_fog: false,
            superbowl_fog_start: -3760.0,
            superbowl_fog_end: 7960.0,
            superbowl_scale: 0.6,
            cloud_name: None,
            skybox_saturation: 0.45,
            night: false,
            sun_elevation: None,
            solar_hour: None,
            relight: None,
        }
    }
}

impl Atmosphere {
    /// The section for stage `name` (for example `track-54_E`) and `weather` (`dry`, `damp` or `wet`) in
    /// `maps/TrackSettings.ini`, or the defaults when the file or a matching section is missing. A stage in a folder of
    /// its own (`rbr_formats::stage_files`) looks in its `TrackSettings<id>.ini` first (RallySimFans).
    pub fn load(maps: &Path, name: &str, weather: &str) -> Atmosphere {
        let own = stage_files::folder(name)
            .zip(stage_files::parts(name))
            .and_then(|(folder, (id, _))| rsf::settings_text(folder, id));
        let retail =
            || std::fs::read(maps.join("TrackSettings.ini")).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
        let track = stage_files::name(name);
        [own, retail()]
            .into_iter()
            .flatten()
            .filter_map(|text| ini::parse(&text).ok())
            .find_map(|sections| {
                let section = section_name(&sections, track, weather)?;
                sections.iter().find(|s| s.name == section).map(Atmosphere::from_section)
            })
            .unwrap_or_default()
    }

    fn from_section(section: &Section) -> Atmosphere {
        let base = Atmosphere::default();
        let number = |key: &str, default: f32| section.get(key).and_then(|v| v.parse::<f32>().ok()).unwrap_or(default);
        let colour = |prefix: &str, default: Vec3| {
            Vec3::new(
                number(&format!("{prefix}Red"), default.x),
                number(&format!("{prefix}Green"), default.y),
                number(&format!("{prefix}Blue"), default.z),
            )
        };
        let sun_direction = section
            .get("SunDir")
            .map(|v| v.split_whitespace().filter_map(|n| n.parse().ok()))
            .map(|n| n.collect::<Vec<f32>>())
            .filter(|n| n.len() == 3)
            .map(|n| Vec3::new(n[0], n[1], n[2]).normalize_or_zero())
            .filter(|d| *d != Vec3::ZERO)
            .unwrap_or(base.sun_direction);
        Atmosphere {
            section: Some(section.name.clone()),
            sun_direction,
            ambient: colour("Ambient", base.ambient),
            diffuse: colour("Diffuse", base.diffuse),
            car_ambient: number("Car_Ambient_Lighting", base.car_ambient),
            car_diffuse: number("Car_Diffuse_Lighting", base.car_diffuse),
            car_shadow: number("Car_Shadow_Alpha", base.car_shadow),
            car_deep_shadow: number("Car_Deep_Shadow_Alpha", base.car_deep_shadow),
            fog_color: colour("Fog", base.fog_color),
            fog_start: number("FogStart", base.fog_start),
            fog_end: number("FogEnd", base.fog_end),
            use_fog: section.get("UseFog").is_some_and(|v| v.eq_ignore_ascii_case("true")),
            superbowl_fog_start: number("SuperbowlFogStart", base.superbowl_fog_start),
            superbowl_fog_end: number("SuperbowlFogEnd", base.superbowl_fog_end),
            superbowl_scale: number("Superbowl_Scale", base.superbowl_scale),
            cloud_name: section.get("CloudName").map(str::to_owned),
            skybox_saturation: number("SkyboxSaturation", base.skybox_saturation),
            night: false,
            sun_elevation: None,
            solar_hour: None,
            relight: None,
        }
    }

    /// The most the fog covers: all under `UseFog`, else [`CLEAR_HAZE`], so that a far backdrop keeps its shape.
    pub fn haze(&self) -> f32 {
        if self.use_fog {
            1.0
        } else {
            CLEAR_HAZE
        }
    }

    /// The fog over the backdrop (the stage's sky faces): the most at the eye's height, the most well above it
    /// (`MIST_HEIGHT` in `shader.wgsl`) and the least at the eye's height. Under `UseFog` its foot is all fog, as
    /// the fogged stage in front of it, and its peaks keep [`SUPERBOWL_HAZE`], so that the mountains keep their
    /// shape and some texture in the rain. Else [`CLEAR_HAZE`] by the distance, as the stage.
    pub fn superbowl_haze(&self) -> [f32; 3] {
        if self.use_fog {
            [1.0, SUPERBOWL_HAZE, 1.0]
        } else {
            [CLEAR_HAZE, CLEAR_HAZE, 0.0]
        }
    }

    /// The backdrop's fog range, metres from the eye. Under `UseFog` the superbowl's own (the game's linear fog,
    /// `SuperbowlFogStart` and `SuperbowlFogEnd`). Else the stage's haze, which stands in for the scattering, at the
    /// view depth times `Superbowl_Scale` as the game's scattering takes it: the stage's range over the scale, so
    /// that the backdrop next to the road is hazed as little as the stage there.
    pub fn superbowl_fog(&self) -> [f32; 2] {
        if self.use_fog {
            [self.superbowl_fog_start, self.superbowl_fog_end]
        } else {
            let scale = self.superbowl_scale.max(0.01);
            [self.fog_start / scale, self.fog_end / scale]
        }
    }

    /// How far the clouds fade into the sky: under `UseFog`, `1 - SkyboxSaturation`, at most [`CLOUD_HAZE`], else
    /// none. The game draws the clouds with D3D fog on under `UseFog` (`CHH_KClouds::Render`,
    /// `src/hedgehog3d/HH_KClouds.cpp`) and `CloudsSingleTexture.vso` writes `oFog = c49.w`, which `Render` loads
    /// with `SkyboxSaturation` (0.12 in `41O_M_heavyrain_heavycloud`; mostly 0 to 0.16 in the rain and snow sections,
    /// 0.33 to 0.51 in the unfogged ones).
    pub fn cloud_haze(&self) -> f32 {
        if self.use_fog {
            (1.0 - self.skybox_saturation.clamp(0.0, 1.0)).min(CLOUD_HAZE)
        } else {
            0.0
        }
    }

    /// How cloudy the section is (ours), 0 to 1, by the section name's cloud word: `clear`, `partcloud`, `lightcloud`,
    /// `heavycloud`. The physically based sky greys by it (`skylight.rs`).
    pub fn cloud_cover(&self) -> f32 {
        let word = self.section.as_deref().and_then(|s| s.rsplit('_').next()).unwrap_or("");
        match word.to_ascii_lowercase().as_str() {
            "clear" => 0.12,
            "partcloud" => 0.42,
            "lightcloud" => 0.58,
            "heavycloud" => 0.9,
            _ => 0.35,
        }
    }

    /// How hard it rains or snows (ours), 0 to 1, by the section name's preset word: `heavyrain` and `heavysnow` 1,
    /// `lightrain` and `lightsnow` a half, else (`norain`, `nosnow`, the fogs, the clear presets) none.
    pub fn precipitation(&self) -> f32 {
        let words = self.section.as_deref().unwrap_or("").to_ascii_lowercase();
        if words.contains("heavyrain") || words.contains("heavysnow") {
            1.0
        } else if words.contains("lightrain") || words.contains("lightsnow") {
            0.5
        } else {
            0.0
        }
    }

    /// How much of the sky the moving clouds' layer covers (renderer addition, `Enhance::clouds`; ours), 0 to 1: as
    /// much as the section's cloud face shows (`CloudName`; the faces stay drawn in front of the layer), else by the
    /// cloud word ([`Atmosphere::cloud_cover`]).
    pub fn cloud_layer(&self) -> f32 {
        let face = self.cloud_name.as_deref().unwrap_or("").to_ascii_lowercase();
        let covers = [
            ("us_partly_clouded", 0.35),
            ("us_clouded", 0.5),
            ("us_clear", 0.15),
            ("us_overcast", 0.72),
            ("us_precipitation", 0.92),
        ];
        covers.iter().find(|(name, _)| face.starts_with(name)).map_or_else(|| self.cloud_cover(), |&(_, cover)| cover)
    }

    /// The same stage at night (renderer addition, from the time of day: `relight::Daylight::night`; the game has no night): moonlight from where the
    /// sun was for the lit draws (the car), no sun shadow from the car, and a dark haze the colour of the night sky's
    /// horizon (`NIGHT_HORIZON` in `night.wgsl`), drawn in a little nearer.
    pub fn night(&self) -> Atmosphere {
        Atmosphere {
            ambient: Vec3::new(0.08, 0.09, 0.13),
            diffuse: Vec3::new(0.07, 0.085, 0.12),
            car_shadow: 0.0,
            fog_color: Vec3::new(0.035, 0.042, 0.06),
            fog_start: self.fog_start.min(0.0),
            fog_end: self.fog_end * 0.7,
            night: true,
            ..self.clone()
        }
    }

    /// The stage's ambient light relative to the day's, linear (ours): 1 without a clock. The relight's share and tint
    /// scale the stage's display colours (`relit` in `shader.wgsl`, before `graded`), so its light is them to the 2.2.
    /// At night the share at [`relight::NIGHT`](crate::relight::NIGHT), where the dusk's light meets the moonlight.
    pub fn ambient_light(&self) -> f32 {
        let (ambient, tint) = match (self.night, self.relight) {
            (true, _) => crate::relight::ambient_at(crate::relight::NIGHT),
            (false, Some(r)) => (r.ambient, r.tint),
            (false, None) => return 1.0,
        };
        (ambient * tint).powf(2.2).dot(Vec3::new(0.2126, 0.7152, 0.0722))
    }

    /// The sky's colour overhead: the fog colour, a little darker, under fog, else a clear blue.
    pub fn zenith(&self) -> Vec3 {
        if self.night && !self.use_fog {
            NIGHT_ZENITH
        } else if self.use_fog {
            self.fog_color * 0.9
        } else {
            CLEAR_ZENITH
        }
    }
}

/// The section for stage `name` and `weather`: the sections of the stage's track number and letter (`54E_` for
/// `track-54_E`, `54O_` for `track-54_O`), the first whose name holds one of the weather's presets, else the first of
/// the track's. Ours (see the module docs).
fn section_name(sections: &[Section], name: &str, weather: &str) -> Option<String> {
    let (number, letter) = rbr_formats::stage_files::parts(name)?;
    let prefix = format!("{number}{letter}_");
    let presets: &[&str] = match weather {
        "damp" => &["lightrain", "lightsnow", "lightfog", "crisp_partcloud"],
        "wet" => &["heavyrain", "heavysnow", "hazy_partcloud", "hazy_clear"],
        // Only through the viewer's `--sun-from STAGE:fog`: the fog sections are no weather of their own here.
        "fog" => &["heavyfog", "lightfog"],
        _ => &["norain_lightcloud", "nosnow_lightcloud", "crisp_clear"],
    };
    let ours: Vec<&Section> = sections.iter().filter(|s| s.name.starts_with(&prefix)).collect();
    presets
        .iter()
        .find_map(|preset| ours.iter().find(|s| s.name.contains(preset)))
        .or(ours.first())
        .map(|s| s.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sections(text: &str) -> Vec<Section> {
        ini::parse(text).unwrap()
    }

    #[test]
    fn the_section_follows_the_stage_and_weather() {
        let s = sections(
            "[54E_crisp_clear]\nUseFog = false\n[54E_crisp_partcloud]\n[54O_E_lightrain_heavycloud]\n\
             [54O_E_norain_lightcloud]\n[541E_crisp_clear]\n[21O_E_lightfog_heavycloud]\n\
             [21O_E_heavyfog_heavycloud]\n",
        );
        assert_eq!(section_name(&s, "track-54_E", "dry").as_deref(), Some("54E_crisp_clear"));
        assert_eq!(section_name(&s, "track-54_E", "damp").as_deref(), Some("54E_crisp_partcloud"));
        assert_eq!(section_name(&s, "track-54_O", "damp").as_deref(), Some("54O_E_lightrain_heavycloud"));
        assert_eq!(section_name(&s, "track-54_O", "dry").as_deref(), Some("54O_E_norain_lightcloud"));
        assert_eq!(section_name(&s, "track-55_O", "dry"), None);
        assert_eq!(section_name(&s, "track-21_O", "fog").as_deref(), Some("21O_E_heavyfog_heavycloud"));
    }

    #[test]
    fn values_are_read_from_the_section() {
        let s = sections(
            "[10O_N_x]\nSunDir = 0 -2 0\nFogStart = -40.5\nFogEnd = 165\nFogRed = 0.72\nUseFog = true\n\
             Car_Diffuse_Lighting = 0.5\nCar_Shadow_Alpha = 0.11\n",
        );
        let a = Atmosphere::from_section(&s[0]);
        assert_eq!(a.sun_direction, Vec3::NEG_Y);
        assert_eq!((a.fog_start, a.fog_end, a.fog_color.x), (-40.5, 165.0, 0.72));
        assert!(a.use_fog);
        assert_eq!(a.car_diffuse, 0.5);
        assert_eq!(a.car_ambient, Atmosphere::default().car_ambient);
        assert_eq!((a.car_shadow, a.car_deep_shadow), (0.11, 1.0));
        assert_eq!(a.zenith(), a.fog_color * 0.9);
    }

    #[test]
    fn night_keeps_the_stage_and_darkens_its_light() {
        let day = Atmosphere::default();
        let night = day.night();
        assert!(night.night && !day.night);
        assert_eq!((night.sun_direction, night.use_fog), (day.sun_direction, day.use_fog));
        assert_eq!(night.zenith(), NIGHT_ZENITH);
        assert_eq!(night.car_shadow, 0.0);
        assert!(night.ambient.max_element() < 0.2 && night.fog_color.max_element() < 0.1);
        let fogged = Atmosphere { use_fog: true, ..day }.night();
        assert_eq!(fogged.zenith(), fogged.fog_color * 0.9);
    }

    #[test]
    fn the_ambient_light_falls_through_dusk_and_holds_at_night() {
        let at = |elevation: f32| {
            let (ambient, tint) = crate::relight::ambient_at(elevation);
            let relight = crate::relight::Relight {
                bake_sun: Vec3::Y,
                strength: 0.3,
                r: 1.0,
                open: 0.3,
                sun: Vec3::ZERO,
                ambient,
                tint,
                lamps: 0.0,
            };
            Atmosphere { relight: Some(relight), ..Atmosphere::default() }
        };
        assert_eq!(Atmosphere::default().ambient_light(), 1.0);
        assert!((at(20.0).ambient_light() - 1.0).abs() < 1e-6);
        // The share at 4 degrees down is 0.27 of the display colour: about a twentieth of the light.
        let dusk = at(-4.0).ambient_light();
        assert!(dusk > 0.03 && dusk < 0.06, "{dusk}");
        let last = at(crate::relight::NIGHT).ambient_light();
        assert!((at(-20.0).night().ambient_light() - last).abs() < 1e-6);
    }

    #[test]
    fn fog_fades_the_clouds_and_the_backdrop() {
        let s = sections(
            "[41O_M_heavyrain_heavycloud]\nUseFog = true\nCloudName = Us_precipitation02_pc\nSkyboxSaturation = 0.12\n\
             [41O_M_norain_lightcloud]\nUseFog = false\nCloudName = Us_overcast02\nSkyboxSaturation = 0.42\n\
             [10O_N_heavysnow_heavycloud]\nUseFog = true\nSkyboxSaturation = 0.0\n",
        );
        let [rain, clear, snow] = [0, 1, 2].map(|i| Atmosphere::from_section(&s[i]));
        assert_eq!(rain.cloud_name.as_deref(), Some("Us_precipitation02_pc"));
        assert_eq!(snow.cloud_name, None);
        assert_eq!(Atmosphere::default().cloud_name, None);
        // Under fog the clouds fade by `1 - SkyboxSaturation`, at most `CLOUD_HAZE`; clear, not at all.
        assert!((rain.cloud_haze() - CLOUD_HAZE).abs() < 1e-6);
        assert_eq!(snow.cloud_haze(), CLOUD_HAZE);
        assert_eq!(clear.cloud_haze(), 0.0);
        let partly = Atmosphere { skybox_saturation: 0.5, ..rain.clone() };
        assert!((partly.cloud_haze() - 0.5).abs() < 1e-6);
        // The moving clouds' cover follows the cloud word.
        assert!(rain.cloud_cover() > clear.cloud_cover());
        assert_eq!(Atmosphere::default().cloud_cover(), 0.35);
        // The moving clouds' layer follows the cloud face, else the word.
        assert_eq!((rain.cloud_layer(), clear.cloud_layer()), (0.92, 0.72));
        assert_eq!(snow.cloud_layer(), snow.cloud_cover());
        // The backdrop: all fog at its foot and `SUPERBOWL_HAZE` on its peaks under fog, else the clear haze.
        assert_eq!(rain.superbowl_haze(), [1.0, SUPERBOWL_HAZE, 1.0]);
        assert_eq!(clear.superbowl_haze(), [CLEAR_HAZE, CLEAR_HAZE, 0.0]);
        // Its range: the superbowl's under fog, else the stage's at the depth times `Superbowl_Scale`.
        let rain = Atmosphere { superbowl_fog_start: -6880.0, superbowl_fog_end: 2200.0, ..rain };
        assert_eq!(rain.superbowl_fog(), [-6880.0, 2200.0]);
        let clear = Atmosphere { fog_start: 240.0, fog_end: 960.0, superbowl_scale: 0.8, ..clear };
        assert_eq!(clear.superbowl_fog(), [300.0, 1200.0]);
    }
}
