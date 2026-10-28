//! The settings file, `launcher-settings.ini` in the player's folder (`home`): the form read from it and written to it.

use std::path::{Path, PathBuf};

use rbr_formats::data::Data;
use rbr_formats::ini;
use rbr_gameplay::crt::CrtRand;
use rbr_gameplay::markers::MarkerSettings;
use rbr_gameplay::race_options::{ChampionshipOptions, DamageLevel, DriverAids, RaceOptions, Skill};
use rbr_gameplay::weather::{self, Preset, Season};
use rbr_render::bindings::Bindings;
use rbr_render::camera_prefs::CameraPrefs;
use rbr_render::coords;
use rbr_render::ffb;
use rbr_render::glass::model::Switch;
use rbr_render::hud::Parts;
use rbr_render::mirrors::Rate;
use rbr_render::present::Present;
use rbr_render::renderer::{AntiAliasing, Detail, Enhance, Reflections};
use rbr_render::shifter::{CarBox, Engagement};
use rbr_sound::custom_codriver;

use crate::catalog::{self, Car, Catalog, TyreChoice, WEARS};
use crate::controls_page::ControlsPage;
use crate::ghosts::GhostChoice;
use crate::graphics_page::{HudChoice, DEFAULT_GRAPHICS, DEFAULT_MIRRORS, MAX_GROUP_VOLUME, MAX_VOLUME, RENDER_SCALES};
use crate::pages::Page;
use crate::rally_page::RallyPage;
use crate::setup_page::SetupPage;
use crate::{install, preview, Form, Paths, Start};

const DEFAULT_SIZE: (u32, u32) = (1280, 720);

/// `minutes` past midnight as `HH:MM`.
pub fn clock_text(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// `HH:MM` as minutes past midnight.
pub fn parse_clock(text: &str) -> Option<u32> {
    let (h, m) = text.split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Parses `WxH`.
fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// The Drive page's weather: a game preset, or one season row of the map (its `TrackSettings.ini` section name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeatherChoice {
    Preset(Preset),
    Row(String),
}

impl WeatherChoice {
    fn parse(text: &str) -> Option<WeatherChoice> {
        match text {
            "good" => Some(WeatherChoice::Preset(Preset::Good)),
            "random" => Some(WeatherChoice::Preset(Preset::Random)),
            "bad" => Some(WeatherChoice::Preset(Preset::Bad)),
            section if !section.is_empty() => Some(WeatherChoice::Row(section.to_owned())),
            _ => None,
        }
    }

    fn to_text(&self) -> String {
        match self {
            WeatherChoice::Preset(preset) => weather::name(*preset).to_owned(),
            WeatherChoice::Row(section) => section.clone(),
        }
    }
}

/// Our fog over the stage (`viewer --fog`), not the game's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fog {
    Off,
    Light,
    Heavy,
}

impl Fog {
    pub fn name(self) -> &'static str {
        match self {
            Fog::Off => "off",
            Fog::Light => "light",
            Fog::Heavy => "heavy",
        }
    }

    /// The Drive page's word for the fog.
    pub fn label(self) -> &'static str {
        match self {
            Fog::Off => "Off",
            Fog::Light => "Light",
            Fog::Heavy => "Heavy",
        }
    }

    pub fn parse(text: &str) -> Option<Fog> {
        match text {
            "off" => Some(Fog::Off),
            "light" => Some(Fog::Light),
            "heavy" => Some(Fog::Heavy),
            _ => None,
        }
    }
}

/// Our cloud cover over the stage (`viewer --clouds`), not the game's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clouds {
    Clear,
    Partly,
    Light,
    Heavy,
}

impl Clouds {
    pub const ALL: [Clouds; 4] = [Clouds::Clear, Clouds::Partly, Clouds::Light, Clouds::Heavy];

    pub fn name(self) -> &'static str {
        ["clear", "partly", "light", "heavy"][self as usize]
    }

    /// The Drive page's word for the cover.
    pub fn label(self) -> &'static str {
        ["Clear", "Partly cloudy", "Light clouds", "Overcast"][self as usize]
    }

    pub fn parse(text: &str) -> Option<Clouds> {
        Clouds::ALL.into_iter().find(|clouds| clouds.name() == text)
    }
}

/// Our rain or snow over the stage (`viewer --precipitation`), not the game's; the road's wetness stays separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precipitation {
    None,
    Light,
    Moderate,
    Heavy,
}

impl Precipitation {
    pub const ALL: [Precipitation; 4] =
        [Precipitation::None, Precipitation::Light, Precipitation::Moderate, Precipitation::Heavy];

    pub fn name(self) -> &'static str {
        ["none", "light", "moderate", "heavy"][self as usize]
    }

    /// The Drive page's word for the amount.
    pub fn label(self) -> &'static str {
        ["None", "Light", "Moderate", "Heavy"][self as usize]
    }

    pub fn parse(text: &str) -> Option<Precipitation> {
        Precipitation::ALL.into_iter().find(|rain| rain.name() == text)
    }
}

/// The road wetness word of `wetness` (0 dry, 1 damp, 2 wet).
pub fn wetness_name(wetness: u32) -> &'static str {
    ["dry", "damp", "wet"][wetness.min(2) as usize]
}

/// Parses a road wetness word.
pub fn parse_wetness(text: &str) -> Option<u32> {
    match text {
        "dry" => Some(0),
        "damp" => Some(1),
        "wet" => Some(2),
        _ => None,
    }
}

/// The launcher's last selection, the viewer's bindings and its force feedback (the `[ffb]` section).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub car: Option<String>,
    pub map: Option<u32>,
    /// The last cars (their names) and maps picked, newest first (`recent_cars = A|B`, `recent_maps = 54,129`).
    pub recent_cars: Vec<String>,
    pub recent_maps: Vec<u32>,
    /// The cars (their names) and stages starred, in the order starred (`favourite_cars = A|B`, `favourite_maps =
    /// 54,129`).
    pub favourite_cars: Vec<String>,
    pub favourite_maps: Vec<u32>,
    pub weather: Option<WeatherChoice>,
    /// The road wetness over the weather row's (0 dry, 1 damp, 2 wet); none takes it from the weather.
    pub wetness: Option<u32>,
    /// Our fog over the stage; none takes it from the weather.
    pub fog: Option<Fog>,
    /// Our cloud cover over the stage; none takes it from the weather.
    pub clouds: Option<Clouds>,
    /// Our rain or snow over the stage; none takes it from the weather.
    pub precipitation: Option<Precipitation>,
    pub wear: Option<u32>,
    pub tyre: Option<TyreChoice>,
    pub size: Option<(u32, u32)>,
    /// Every view's horizontal field of view, degrees (`viewer --fov`).
    pub fov: Option<f32>,
    /// The sound volume, linear, 0 to [`MAX_VOLUME`] (`viewer --volume`).
    pub volume: Option<f32>,
    /// The sound group volumes, linear, 0 to [`MAX_GROUP_VOLUME`] (`viewer --volumes E,C,F,T`).
    pub engine_volume: Option<f32>,
    pub codriver_volume: Option<f32>,
    pub effects_volume: Option<f32>,
    pub tyres_volume: Option<f32>,
    /// The co-driver calls the pace notes (`viewer --no-codriver` when off).
    pub codriver: Option<bool>,
    /// The co-driver mod's voice (`1.02` the original co-driver; none: the plugin's), its call style and range set
    /// (`viewer --codriver-voice`, `--codriver-style`, `--codriver-ranges`).
    pub codriver_voice: Option<String>,
    pub codriver_style: Option<String>,
    pub codriver_ranges: Option<String>,
    /// The locally owned voice pack (`codriver_packs::VOICES_DIR`) and stage-note pack
    /// (`codriver_packs::NOTES_DIR`); none each for the RSF install's.
    pub codriver_pack: Option<String>,
    pub notes_pack: Option<String>,
    /// The stage's NGPCarMenu MyPacenotes notes are called (`viewer --pacenotes`); on when unset.
    pub my_pacenotes: Option<bool>,
    /// The HUD shown (`viewer --no-hud` when off).
    pub hud: Option<bool>,
    /// The speed in mph (`viewer --mph`), else km/h.
    pub mph: Option<bool>,
    /// The FPS counter (`viewer --fps`).
    pub fps: Option<bool>,
    /// The pace stack's notes (`viewer --pace-stack`, 0 to 4).
    pub pace_stack: Option<u8>,
    /// The working dials (`viewer --no-dials` when off).
    pub dials: Option<bool>,
    /// The cockpit's steering wheel hidden (`viewer --hide-wheel`).
    pub hide_wheel: Option<bool>,
    /// The HUD's parts shown (`viewer --hud-hide`), kept as the hidden ones' list.
    pub hud_hide: Option<Parts>,
    /// The wiper switch at the start (`viewer --wipers`).
    pub wipers: Option<Switch>,
    /// Each renderer addition on or off (`viewer --enhance`), by [`Enhance::NAMES`], each kept under its name.
    pub graphics: [Option<bool>; Enhance::NAMES.len()],
    /// One of [`RENDER_SCALES`].
    pub render_scale: Option<u32>,
    /// The frame's anti-aliasing (`viewer --anti-aliasing`).
    pub anti_aliasing: Option<AntiAliasing>,
    /// The screen-space reflections (`viewer --reflections`).
    pub reflections: Option<Reflections>,
    /// The additions' detail (`viewer --detail`).
    pub detail: Option<Detail>,
    /// The car's and the stage's textures upscaled (`viewer --upscale-textures`).
    pub upscale_textures: Option<bool>,
    /// The driving window's frame presentation (`viewer --present`).
    pub present: Option<Present>,
    /// How often the cockpit's mirrors are drawn (`viewer --mirrors`).
    pub mirrors: Option<Rate>,
    /// The driving window fills the monitor (`viewer --fullscreen`).
    pub fullscreen: Option<bool>,
    /// The viewer writes a performance log (`viewer --perf-log`, in the player's `logs` folder).
    pub perf_log: Option<bool>,
    /// The start time, minutes past midnight (`viewer --clock`); none for the stage's own light.
    pub start_time: Option<u32>,
    pub callout_distance: Option<f32>,
    pub marker_threshold: Option<f32>,
    /// The Car setup page's named setup (`Physics/<car>/Setups/<name>.lsp`); none for the default.
    pub setup: Option<String>,
    /// The Drive page's race options (`rbr_gameplay::race_options`).
    pub damage: Option<DamageLevel>,
    pub auto_gears: Option<bool>,
    pub gear_guard: Option<bool>,
    pub neutral_lock: Option<bool>,
    pub clutch_help: Option<bool>,
    /// The quick race's ghost (`off`, `best` or a `GhostCars` file name).
    pub ghost: Option<GhostChoice>,
    /// The Championship page's options for a new season (`rbr_gameplay::race_options::ChampionshipOptions`; ours: the
    /// game starts every session on `InitSEA`'s Rookie and Realistic).
    pub championship_skill: Option<Skill>,
    pub championship_damage: Option<DamageLevel>,
    /// The team a new season takes (`TEAM_OFFERS`, the team's id 0-6).
    pub championship_team: Option<u32>,
    /// The RBR install folder (`crate::install`).
    pub install_folder: Option<String>,
    /// NGP's `physics.rbz` (`rbr-standalone --ngp-physics`) when not the install folder's.
    pub ngp_physics: Option<String>,
    /// Selected NGP DLL; empty follows the installed plugin.
    pub ngp_dll: Option<String>,
    /// The RallySimFans install (`Maps/<id>-<name>/` stage folders, `Cars/` and `rsfdata/` NGP cars) when not the
    /// install folder.
    pub rsf_root: Option<String>,
    /// NGP's `headGasketDurability` (`rbr-standalone --head-gasket-durability`; 1, NGP's default, leaves the engine
    /// as it is).
    pub head_gasket: Option<i32>,
    pub bindings: Bindings,
    pub ffb: ffb::Options,
    /// The viewer's camera choices (`[camera]`, `rbr_render::camera_prefs`): the viewer writes them, the launcher
    /// carries them over and resets a car's.
    pub camera: CameraPrefs,
    /// The championship's picked cars (`[season]`).
    pub season_cars: SeasonCars,
    /// Each car's gearbox for the advanced H-shifter (`[gearbox]`).
    pub gearboxes: Gearboxes,
}

/// The championship's picked cars (for a team whose car NGP doesn't drive), by name, kept out of the game's save
/// (`pf<profile>.rbr` stays the game's): each profile's season car (`profile <name> = <car>`) and the last one picked
/// (`last = <car>`), ours.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeasonCars {
    pub last: Option<String>,
    pub profiles: Vec<(String, String)>,
}

const SEASON: &str = "season";

impl SeasonCars {
    fn parse(sections: &[ini::Section]) -> SeasonCars {
        let Some(section) = sections.iter().find(|s| s.name == SEASON) else { return SeasonCars::default() };
        let profiles = section.entries.iter().filter_map(|(key, car)| {
            let profile = key.strip_prefix("profile ")?.trim();
            (!profile.is_empty() && !car.is_empty()).then(|| (profile.to_owned(), car.clone()))
        });
        SeasonCars {
            last: section.get("last").filter(|c| !c.is_empty()).map(str::to_owned),
            profiles: profiles.collect(),
        }
    }

    fn to_text(&self) -> String {
        if *self == SeasonCars::default() {
            return String::new();
        }
        let last = self.last.iter().map(|car| format!("last = {car}\n"));
        let profiles = self.profiles.iter().map(|(profile, car)| format!("profile {profile} = {car}\n"));
        format!("\n[{SEASON}]\n{}", last.chain(profiles).collect::<String>())
    }

    /// The season car of `profile`.
    pub fn of(&self, profile: &str) -> Option<&str> {
        self.profiles.iter().find(|(p, _)| p == profile).map(|(_, car)| car.as_str())
    }

    /// Forgets the season car of `profile` (its season drives the team's car).
    pub fn remove(&mut self, profile: &str) {
        self.profiles.retain(|(p, _)| p != profile);
    }

    /// Makes `car` the season car of `profile`, and the last one.
    pub fn set(&mut self, profile: &str, car: &str) {
        self.profiles.retain(|(p, _)| p != profile);
        self.profiles.push((profile.to_owned(), car.to_owned()));
        self.last = Some(car.to_owned());
    }
}

/// Each car's gearbox for the advanced H-shifter, the Drive page's choice by car name (`<car> = synchro|dog`,
/// ours). A car without one is sequential, which leaves the shifter off and the gearbox aids on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gearboxes(pub Vec<(String, CarBox)>);

const GEARBOX: &str = "gearbox";

/// A sequential gearbox, every car's until one is chosen.
pub const SEQUENTIAL: CarBox = CarBox { engagement: Engagement::Synchro, sequential: true };

impl Gearboxes {
    fn parse(sections: &[ini::Section]) -> Gearboxes {
        let Some(section) = sections.iter().find(|s| s.name == GEARBOX) else { return Gearboxes::default() };
        let boxes = section
            .entries
            .iter()
            .filter_map(|(car, value)| CarBox::parse_arg(value).filter(|b| !b.sequential).map(|b| (car.clone(), b)));
        Gearboxes(boxes.collect())
    }

    fn to_text(&self) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        let lines = self.0.iter().map(|(car, gearbox)| format!("{car} = {}\n", gearbox.arg()));
        format!("\n[{GEARBOX}]\n{}", lines.collect::<String>())
    }

    /// The gearbox of `car`.
    pub fn of(&self, car: &str) -> CarBox {
        self.0.iter().find(|(c, _)| c == car).map_or(SEQUENTIAL, |(_, b)| *b)
    }

    pub fn set(&mut self, car: &str, gearbox: CarBox) {
        self.0.retain(|(c, _)| c != car);
        if !gearbox.sequential {
            self.0.push((car.to_owned(), gearbox));
        }
    }
}

/// The car of `cars` the first of `names` names, else the first that can start, else the first.
pub fn pick_car<'a>(cars: &[Car], names: impl IntoIterator<Item = &'a String>) -> usize {
    names
        .into_iter()
        .find_map(|name| cars.iter().position(|c| c.name == *name))
        .or_else(|| cars.iter().position(|c| c.unavailable().is_none()))
        .unwrap_or(0)
}

fn on_off(value: Option<String>) -> Option<bool> {
    match value?.as_str() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

fn on_off_text(value: Option<bool>) -> Option<String> {
    value.map(|v| if v { "on" } else { "off" }.to_owned())
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let sections = ini::parse(text).unwrap_or_default();
        let launcher = sections.iter().find(|s| s.name == "launcher");
        let get = |key: &str| launcher.and_then(|s| s.get(key)).map(str::to_owned);
        let group = |key: &str| get(key).and_then(|v| v.parse().ok()).filter(|v| (0.0..=MAX_GROUP_VOLUME).contains(v));
        Settings {
            car: get("car"),
            map: get("map").and_then(|v| v.parse().ok()),
            recent_cars: get("recent_cars")
                .map(|v| v.split('|').map(str::trim).filter(|c| !c.is_empty()).map(str::to_owned).collect())
                .unwrap_or_default(),
            recent_maps: get("recent_maps")
                .map(|v| v.split(',').filter_map(|m| m.trim().parse().ok()).collect())
                .unwrap_or_default(),
            favourite_cars: get("favourite_cars")
                .map(|v| v.split('|').map(str::trim).filter(|c| !c.is_empty()).map(str::to_owned).collect())
                .unwrap_or_default(),
            favourite_maps: get("favourite_maps")
                .map(|v| v.split(',').filter_map(|m| m.trim().parse().ok()).collect())
                .unwrap_or_default(),
            weather: get("weather").as_deref().and_then(WeatherChoice::parse),
            wetness: get("wetness").as_deref().and_then(parse_wetness),
            fog: get("fog").as_deref().and_then(Fog::parse),
            clouds: get("clouds").as_deref().and_then(Clouds::parse),
            precipitation: get("precipitation").as_deref().and_then(Precipitation::parse),
            wear: get("wear").and_then(|v| WEARS.iter().position(|wear| *wear == v.as_str())).map(|wear| wear as u32),
            tyre: get("tyre").and_then(|v| {
                if v == "auto" {
                    Some(TyreChoice::Auto)
                } else {
                    v.parse::<u32>().ok().filter(|n| *n <= 6).map(TyreChoice::Fixed)
                }
            }),
            size: get("size").as_deref().and_then(parse_size),
            fov: get("fov").and_then(|v| v.parse().ok()).filter(|v| coords::FOV_RANGE_DEGREES.contains(v)),
            volume: get("volume").and_then(|v| v.parse().ok()).filter(|v| (0.0..=MAX_VOLUME).contains(v)),
            engine_volume: group("engine_volume"),
            codriver_volume: group("codriver_volume"),
            effects_volume: group("effects_volume"),
            tyres_volume: group("tyres_volume"),
            codriver: on_off(get("codriver")),
            codriver_voice: get("codriver_voice").filter(|v| !v.is_empty()),
            codriver_style: get("codriver_style").filter(|v| !v.is_empty()),
            codriver_ranges: get("codriver_ranges").filter(|v| !v.is_empty()),
            codriver_pack: get("codriver_pack").filter(|v| !v.is_empty()),
            notes_pack: get("notes_pack").filter(|v| !v.is_empty()),
            my_pacenotes: on_off(get("my_pacenotes")),
            hud: on_off(get("hud")),
            mph: get("units").and_then(|v| match v.as_str() {
                "mph" => Some(true),
                "kmh" => Some(false),
                _ => None,
            }),
            fps: on_off(get("fps")),
            pace_stack: get("pace_stack").and_then(|v| v.parse().ok()).filter(|n| *n <= 4),
            dials: on_off(get("dials")),
            hide_wheel: on_off(get("hide_wheel")),
            hud_hide: get("hud_hide").and_then(|v| Parts::hiding(&v).ok()),
            wipers: get("wipers").as_deref().and_then(Switch::parse),
            graphics: Enhance::NAMES.map(|name| on_off(get(name))),
            render_scale: get("render_scale").and_then(|v| v.parse().ok()).filter(|v| RENDER_SCALES.contains(v)),
            anti_aliasing: get("anti_aliasing").as_deref().and_then(AntiAliasing::parse),
            reflections: get("reflections").as_deref().and_then(Reflections::parse),
            detail: get("detail").as_deref().and_then(Detail::parse),
            upscale_textures: on_off(get("upscale_textures")),
            present: get("present").as_deref().and_then(Present::parse),
            mirrors: get("mirrors").as_deref().and_then(Rate::parse),
            fullscreen: on_off(get("fullscreen")),
            perf_log: on_off(get("perf_log")),
            start_time: get("start_time").as_deref().and_then(parse_clock),
            callout_distance: get("callout_distance").and_then(|v| v.parse().ok()),
            marker_threshold: get("marker_threshold").and_then(|v| v.parse().ok()),
            setup: get("setup").filter(|v| !v.is_empty()),
            damage: get("damage").as_deref().and_then(DamageLevel::from_name),
            auto_gears: on_off(get("auto_gears")),
            gear_guard: on_off(get("gear_guard")),
            neutral_lock: on_off(get("neutral_lock")),
            clutch_help: on_off(get("clutch_help")),
            ghost: get("ghost").map(|v| GhostChoice::parse(&v)),
            championship_skill: get("championship_skill").as_deref().and_then(Skill::from_name),
            championship_damage: get("championship_damage").as_deref().and_then(DamageLevel::from_name),
            championship_team: get("championship_team").and_then(|v| v.parse().ok()).filter(|&t: &u32| t < 7),
            install_folder: get("install_folder").filter(|v| !v.is_empty()),
            ngp_physics: get("ngp_physics").filter(|v| !v.is_empty()),
            ngp_dll: get("ngp_dll"),
            rsf_root: get("rsf_root").filter(|v| !v.is_empty()),
            head_gasket: get("head_gasket").and_then(|v| v.parse().ok()).filter(|v| *v >= 1),
            bindings: Bindings::parse(text),
            ffb: ffb::Options::parse(text),
            camera: CameraPrefs::parse(text),
            season_cars: SeasonCars::parse(&sections),
            gearboxes: Gearboxes::parse(&sections),
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("[launcher]\n");
        for (key, value) in [
            ("car", self.car.clone()),
            ("map", self.map.map(|v| v.to_string())),
            ("recent_cars", (!self.recent_cars.is_empty()).then(|| self.recent_cars.join("|"))),
            (
                "recent_maps",
                (!self.recent_maps.is_empty())
                    .then(|| self.recent_maps.iter().map(u32::to_string).collect::<Vec<_>>().join(",")),
            ),
            ("favourite_cars", (!self.favourite_cars.is_empty()).then(|| self.favourite_cars.join("|"))),
            (
                "favourite_maps",
                (!self.favourite_maps.is_empty())
                    .then(|| self.favourite_maps.iter().map(u32::to_string).collect::<Vec<_>>().join(",")),
            ),
            ("weather", self.weather.as_ref().map(WeatherChoice::to_text)),
            ("wetness", self.wetness.map(wetness_name).map(str::to_owned)),
            ("fog", self.fog.map(Fog::name).map(str::to_owned)),
            ("clouds", self.clouds.map(Clouds::name).map(str::to_owned)),
            ("precipitation", self.precipitation.map(Precipitation::name).map(str::to_owned)),
            ("wear", self.wear.map(|v| WEARS[v.min(2) as usize].to_owned())),
            (
                "tyre",
                self.tyre.map(|v| match v {
                    TyreChoice::Auto => "auto".to_owned(),
                    TyreChoice::Fixed(n) => n.to_string(),
                }),
            ),
            ("size", self.size.map(|(w, h)| format!("{w}x{h}"))),
            ("fov", self.fov.map(|v| v.to_string())),
            ("volume", self.volume.map(|v| v.to_string())),
            ("engine_volume", self.engine_volume.map(|v| v.to_string())),
            ("codriver_volume", self.codriver_volume.map(|v| v.to_string())),
            ("effects_volume", self.effects_volume.map(|v| v.to_string())),
            ("tyres_volume", self.tyres_volume.map(|v| v.to_string())),
            ("codriver", on_off_text(self.codriver)),
            ("codriver_voice", self.codriver_voice.clone()),
            ("codriver_style", self.codriver_style.clone()),
            ("codriver_ranges", self.codriver_ranges.clone()),
            ("codriver_pack", self.codriver_pack.clone()),
            ("notes_pack", self.notes_pack.clone()),
            ("my_pacenotes", on_off_text(self.my_pacenotes)),
            ("hud", on_off_text(self.hud)),
            ("units", self.mph.map(|v| if v { "mph" } else { "kmh" }.to_owned())),
            ("fps", on_off_text(self.fps)),
            ("pace_stack", self.pace_stack.map(|v| v.to_string())),
            ("wipers", self.wipers.map(|v| v.name().to_owned())),
            ("dials", on_off_text(self.dials)),
            ("hide_wheel", on_off_text(self.hide_wheel)),
            ("hud_hide", self.hud_hide.and_then(|p| p.hidden())),
            ("callout_distance", self.callout_distance.map(|v| v.to_string())),
            ("marker_threshold", self.marker_threshold.map(|v| v.to_string())),
            ("setup", self.setup.clone()),
            ("damage", self.damage.map(|v| v.name().to_owned())),
            ("auto_gears", on_off_text(self.auto_gears)),
            ("gear_guard", on_off_text(self.gear_guard)),
            ("neutral_lock", on_off_text(self.neutral_lock)),
            ("clutch_help", on_off_text(self.clutch_help)),
            ("ghost", self.ghost.as_ref().map(GhostChoice::to_text)),
            ("championship_skill", self.championship_skill.map(|v| v.name().to_owned())),
            ("championship_damage", self.championship_damage.map(|v| v.name().to_owned())),
            ("championship_team", self.championship_team.map(|v| v.to_string())),
            ("install_folder", self.install_folder.clone()),
            ("ngp_physics", self.ngp_physics.clone()),
            ("ngp_dll", self.ngp_dll.clone()),
            ("rsf_root", self.rsf_root.clone()),
            ("head_gasket", self.head_gasket.map(|v| v.to_string())),
            ("render_scale", self.render_scale.map(|v| v.to_string())),
            ("anti_aliasing", self.anti_aliasing.map(|v| v.name().to_owned())),
            ("reflections", self.reflections.map(|v| v.name().to_owned())),
            ("detail", self.detail.map(|v| v.name().to_owned())),
            ("upscale_textures", on_off_text(self.upscale_textures)),
            ("present", self.present.map(|v| v.name().to_owned())),
            ("mirrors", self.mirrors.map(Rate::name)),
            ("fullscreen", on_off_text(self.fullscreen)),
            ("start_time", self.start_time.map(clock_text)),
            ("perf_log", on_off_text(self.perf_log)),
        ]
        .into_iter()
        .chain(Enhance::NAMES.into_iter().zip(self.graphics.map(on_off_text)))
        {
            if let Some(v) = value {
                out.push_str(&format!("{key} = {v}\n"));
            }
        }
        out.push('\n');
        out.push_str(&self.bindings.to_text());
        out.push('\n');
        out.push_str(&self.ffb.to_text());
        out.push('\n');
        out.push_str(&self.camera.to_text());
        out.push_str(&self.season_cars.to_text());
        out.push_str(&self.gearboxes.to_text());
        out
    }
}

/// The settings file's settings (the defaults without one).
pub fn read_settings(path: &Path) -> Settings {
    std::fs::read_to_string(path).map(|t| Settings::parse(&t)).unwrap_or_default()
}

/// The form for the game of `settings` (their install folder, or `start`'s overrides), or why there is none.
pub fn load(start: &Start, mut settings: Settings) -> Result<Form, String> {
    let folder = install::Folder::read(settings.install_folder.as_deref().unwrap_or_default());
    let data = start.data.clone().or_else(|| folder.game.clone());
    let paths = Paths {
        data: data.ok_or("no game: set the RBR install folder on the Settings page (or --data)")?,
        patch: start.patch.clone().or_else(|| folder.game.clone()),
        viewer: start.viewer.clone(),
        standalone: start.standalone.clone(),
        settings: start.settings.clone(),
    };
    // Unsaved, the NGP and RSF fields come from the environment; and NGP physics is on by default when its archive
    // is there.
    let env = |key| std::env::var(key).ok().filter(|v| !v.is_empty());
    settings.ngp_physics = settings.ngp_physics.or_else(|| env("RBR_NGP_PHYSICS"));
    settings.rsf_root = settings.rsf_root.or_else(|| env("RBR_RSF"));
    settings.ngp_dll = settings.ngp_dll.or_else(|| env("RBR_NGP_DLL"));
    let mut catalog = Catalog::scan(&paths.data)?;
    catalog.maps.retain(catalog::is_listed);
    let rsf = or_folder(settings.rsf_root.as_deref(), &folder.rsf_root);
    if !rsf.is_empty() {
        catalog.set_rsf(&rsf);
    }
    let here = paths.settings.parent().unwrap_or(Path::new(""));
    catalog.set_rsf_cars(&paths.data, &rsf, &here.join(crate::car_packs::DIR));
    catalog.offer(&catalog::ngp_cars(Path::new(&or_folder(settings.ngp_physics.as_deref(), &folder.ngp_physics))));
    let data = Data::new(&paths.data, paths.patch.clone());
    let season = Season::load(&data)?;
    // Without RSF cars the form still loads: the Settings page and the status line say what is missing.
    if catalog.maps.is_empty() {
        return Err(format!("no maps in {}", paths.data.display()));
    }
    let startable = catalog.startable_maps(&season);
    eprintln!("launcher: {} cars, {}/{} maps startable", catalog.cars.len(), startable.len(), catalog.maps.len());
    Ok(Form::new(catalog, &settings, paths, data, season, folder))
}

/// A path field's text, or the install folder's path when it is empty.
pub fn or_folder(field: Option<&str>, folder: &Option<PathBuf>) -> String {
    match field.map(str::trim).filter(|f| !f.is_empty()) {
        Some(field) => field.to_owned(),
        None => folder.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
    }
}

impl Form {
    pub fn new(
        catalog: Catalog,
        settings: &Settings,
        paths: Paths,
        data: Data,
        season: Season,
        folder: install::Folder,
    ) -> Form {
        // A saved car not in the list gives way to the most recent one that is, else the first that starts.
        let car = pick_car(&catalog.cars, settings.car.iter().chain(&settings.recent_cars));
        let season_pick = pick_car(&catalog.cars, settings.season_cars.last.iter().chain(&settings.car));
        let default_map = catalog.maps.iter().position(|map| catalog.availability(&season, map).is_ok()).unwrap_or(0);
        let map = settings
            .map
            .and_then(|number| catalog.maps.iter().position(|map| map.number == number))
            .unwrap_or(default_map);
        let aids = DriverAids::default();
        // A specific row launches like Random with that row; a section that is not this map's falls back to Good.
        let (mut preset, mut weather_row) =
            match settings.weather.clone().unwrap_or(WeatherChoice::Preset(Preset::Good)) {
                WeatherChoice::Preset(preset) => (preset, None),
                WeatherChoice::Row(section) => (Preset::Random, Some(section)),
            };
        let map_number = catalog.maps[map].number;
        if weather_row
            .as_deref()
            .is_some_and(|s| catalog::row_by_section(&catalog.maps[map].season(&season), map_number, s).is_none())
        {
            (preset, weather_row) = (Preset::Good, None);
        }
        let options = RaceOptions {
            weather: preset,
            damage: settings.damage.unwrap_or_default(),
            aids: DriverAids {
                auto_gears: settings.auto_gears.unwrap_or(aids.auto_gears),
                gear_guard: settings.gear_guard.unwrap_or(aids.gear_guard),
                neutral_lock: settings.neutral_lock.unwrap_or(aids.neutral_lock),
                clutch_help: settings.clutch_help.unwrap_or(aids.clutch_help),
            },
        };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let wear = settings.wear.unwrap_or(0).min(2);
        let tyre = settings.tyre.unwrap_or(TyreChoice::Auto);
        let size = settings.size.unwrap_or(DEFAULT_SIZE);
        let fov = settings.fov.unwrap_or(coords::FOV_DEGREES);
        let volume = settings.volume.unwrap_or(1.0);
        let engine_volume = settings.engine_volume.unwrap_or(1.0);
        let codriver_volume = settings.codriver_volume.unwrap_or(1.0);
        let effects_volume = settings.effects_volume.unwrap_or(1.0);
        let tyres_volume = settings.tyres_volume.unwrap_or(1.0);
        let defaults = MarkerSettings::default();
        let marker = MarkerSettings {
            callout_distance: settings.callout_distance.unwrap_or(defaults.callout_distance),
            marker_threshold: settings.marker_threshold.unwrap_or(defaults.marker_threshold),
        };
        let mut rally = RallyPage::new(&data);
        let championship = ChampionshipOptions::default();
        rally.options = ChampionshipOptions {
            skill: settings.championship_skill.unwrap_or(championship.skill),
            damage: settings.championship_damage.unwrap_or(championship.damage),
        };
        rally.team = settings.championship_team.unwrap_or(rally.team);
        let adding = crate::car_packs::Adding::new(&paths.settings.with_file_name(crate::car_packs::DIR));
        let home = paths.settings.parent().unwrap_or(Path::new(".")).to_path_buf();
        let mut form = Form {
            catalog,
            paths,
            previews: preview::Previews::new(data.clone()),
            data,
            season,
            page: Page::Drive,
            setup: SetupPage::new(settings.setup.clone().unwrap_or_default()),
            rally,
            car,
            map,
            // Filled by `list_cars`.
            car_browser: Default::default(),
            season_pick,
            season_cars: settings.season_cars.clone(),
            gearboxes: settings.gearboxes.clone(),
            // Filled by `list_maps`.
            stage_browser: Default::default(),
            recent_cars: settings.recent_cars.clone(),
            recent_maps: settings.recent_maps.clone(),
            favourite_cars: settings.favourite_cars.clone(),
            favourite_maps: settings.favourite_maps.clone(),
            options,
            rand: CrtRand::srand(now.as_secs() as u32),
            weather_row,
            wetness: settings.wetness,
            fog: settings.fog,
            clouds: settings.clouds,
            precipitation: settings.precipitation,
            weather_adjust: false,
            wear,
            tyre,
            size,
            sizes: vec![size],
            fov,
            volume,
            engine_volume,
            codriver_volume,
            effects_volume,
            tyres_volume,
            codriver: settings.codriver.unwrap_or(true),
            codriver_mods: crate::command::CodriverMods::scan(&or_folder(
                settings.rsf_root.as_deref(),
                &folder.rsf_root,
            )),
            codriver_pack: settings.codriver_pack.clone(),
            notes_pack: settings.notes_pack.clone(),
            rsf_plugin: false,
            rsf_voices: Vec::new(),
            codriver_styles_differ: false,
            codriver_ranges_differ: false,
            cached_notes_map: u32::MAX,
            cached_notes_short: String::new(),
            cached_notes_full: String::new(),
            cached_notes_dirty: true,
            owned_voices: crate::codriver_packs::voice_packs(&home),
            owned_notes: crate::codriver_packs::note_packs(&home),
            codriver_job: None,
            codriver_status: None,
            codriver_voice: settings.codriver_voice.clone(),
            codriver_style: settings.codriver_style.clone().unwrap_or_else(|| custom_codriver::DEFAULT_STYLE.into()),
            codriver_ranges: settings.codriver_ranges.clone().unwrap_or_else(|| custom_codriver::DEFAULT_RANGES.into()),
            my_pacenotes: settings.my_pacenotes.unwrap_or(true),
            ghost: settings.ghost.clone().unwrap_or_default(),
            menu_sound: None,
            hud: HudChoice {
                show: settings.hud.unwrap_or(true),
                mph: settings.mph.unwrap_or(false),
                fps: settings.fps.unwrap_or(false),
                stack: settings.pace_stack.unwrap_or(0),
                dials: settings.dials.unwrap_or(true),
                hide_wheel: settings.hide_wheel.unwrap_or(false),
                parts: settings.hud_hide.unwrap_or_default(),
            },
            graphics: std::array::from_fn(|k| settings.graphics[k].unwrap_or(DEFAULT_GRAPHICS[k])),
            render_scale: settings.render_scale.unwrap_or(100),
            anti_aliasing: settings.anti_aliasing.unwrap_or_default(),
            reflections: settings.reflections.unwrap_or_default(),
            detail: settings.detail.unwrap_or_default(),
            upscale_textures: settings.upscale_textures.unwrap_or(false),
            upscaler: rbr_render::upscale::Upscaler::find(&home.join(rbr_render::upscale::DIR)).is_some(),
            upscaler_download: Default::default(),
            present: settings.present.unwrap_or_default(),
            mirrors: settings.mirrors.unwrap_or(DEFAULT_MIRRORS),
            fullscreen: settings.fullscreen.unwrap_or(true),
            perf_log: settings.perf_log.unwrap_or(false),
            start_time: settings.start_time,
            wipers: settings.wipers.unwrap_or_default(),
            install_folder: settings.install_folder.clone().unwrap_or_default(),
            folder,
            disc_setup: Default::default(),
            adding,
            downloads: Default::default(),
            reload: false,
            ngp_physics: settings.ngp_physics.clone().unwrap_or_default(),
            ngp_dll: crate::ngp_dll::Choice::new(settings.ngp_dll.clone().unwrap_or_default()),
            // Filled by `list_cars`.
            ngp_revision: None,
            ngp_label: String::new(),
            ngp_cars: Vec::new(),
            rsf_root: settings.rsf_root.clone().unwrap_or_default(),
            rsf_scanned: settings.rsf_root.clone().unwrap_or_default(),
            marker,
            child: None,
            status: String::new(),
            bindings: settings.bindings.clone(),
            ffb: settings.ffb.clone(),
            head_gasket: settings.head_gasket.unwrap_or(1),
            controls: ControlsPage::new(),
            read_only: false,
        };
        form.list_cars();
        form.list_maps();
        form.rescan_codrivers();
        form
    }

    /// Writes the selection and the bindings to the settings file; whether it worked.
    pub fn save(&self) -> bool {
        if self.read_only {
            return false;
        }
        let settings = Settings {
            car: self.car().map(|c| c.name.clone()),
            map: Some(self.catalog.maps[self.map].number),
            recent_cars: self.recent_cars.clone(),
            recent_maps: self.recent_maps.clone(),
            favourite_cars: self.favourite_cars.clone(),
            favourite_maps: self.favourite_maps.clone(),
            weather: Some(match &self.weather_row {
                Some(section) => WeatherChoice::Row(section.clone()),
                None => WeatherChoice::Preset(self.options.weather),
            }),
            wetness: self.wetness,
            fog: self.fog,
            clouds: self.clouds,
            precipitation: self.precipitation,
            wear: Some(self.wear),
            tyre: Some(self.tyre),
            size: Some(self.size),
            fov: Some(self.fov),
            volume: Some(self.volume),
            engine_volume: Some(self.engine_volume),
            codriver_volume: Some(self.codriver_volume),
            effects_volume: Some(self.effects_volume),
            tyres_volume: Some(self.tyres_volume),
            codriver: Some(self.codriver),
            codriver_voice: self.codriver_voice.clone(),
            codriver_style: Some(self.codriver_style.clone()),
            codriver_ranges: Some(self.codriver_ranges.clone()),
            codriver_pack: self.codriver_pack.clone(),
            notes_pack: self.notes_pack.clone(),
            my_pacenotes: Some(self.my_pacenotes),
            hud: Some(self.hud.show),
            mph: Some(self.hud.mph),
            fps: Some(self.hud.fps),
            pace_stack: Some(self.hud.stack),
            dials: Some(self.hud.dials),
            hide_wheel: Some(self.hud.hide_wheel),
            hud_hide: Some(self.hud.parts),
            graphics: self.graphics.map(Some),
            render_scale: Some(self.render_scale),
            anti_aliasing: Some(self.anti_aliasing),
            reflections: Some(self.reflections),
            detail: Some(self.detail),
            upscale_textures: Some(self.upscale_textures),
            present: Some(self.present),
            mirrors: Some(self.mirrors),
            fullscreen: Some(self.fullscreen),
            perf_log: Some(self.perf_log),
            start_time: self.start_time,
            wipers: Some(self.wipers),
            callout_distance: Some(self.marker.callout_distance),
            marker_threshold: Some(self.marker.marker_threshold),
            setup: Some(self.setup.name.trim().to_owned()).filter(|n| !n.is_empty()),
            damage: Some(self.options.damage),
            auto_gears: Some(self.options.aids.auto_gears),
            gear_guard: Some(self.options.aids.gear_guard),
            neutral_lock: Some(self.options.aids.neutral_lock),
            clutch_help: Some(self.options.aids.clutch_help),
            ghost: Some(self.ghost.clone()),
            championship_skill: Some(self.rally.options.skill),
            championship_damage: Some(self.rally.options.damage),
            championship_team: Some(self.rally.team),
            install_folder: Some(self.install_folder.trim().to_owned()).filter(|p| !p.is_empty()),
            ngp_physics: Some(self.ngp_physics.trim().to_owned()).filter(|p| !p.is_empty()),
            ngp_dll: Some(self.ngp_dll.path.trim().to_owned()),
            rsf_root: Some(self.rsf_root.trim().to_owned()).filter(|p| !p.is_empty()),
            head_gasket: Some(self.head_gasket),
            bindings: self.bindings.clone(),
            ffb: self.ffb.clone(),
            // The viewer owns these: carried over from the file as it is now.
            camera: read_settings(&self.paths.settings).camera,
            season_cars: self.season_cars.clone(),
            gearboxes: self.gearboxes.clone(),
        };
        std::fs::write(&self.paths.settings, settings.to_text())
            .map_err(|e| eprintln!("launcher: {}: {e}", self.paths.settings.display()))
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{clock_args, render_scale_args};

    #[test]
    fn an_empty_dll_choice_preserves_automatic_discovery() {
        let settings = Settings::parse("[launcher]\nngp_dll = \n");
        assert_eq!(settings.ngp_dll.as_deref(), Some(""));
        assert_eq!(Settings::parse(&settings.to_text()).ngp_dll.as_deref(), Some(""));
        assert_eq!(Settings::default().ngp_dll, None);
    }

    #[test]
    fn settings_round_trip() {
        let settings = Settings {
            car: Some("COROLLA".into()),
            map: Some(54),
            recent_cars: vec!["COROLLA".into(), "Mitsubishi Lancer Evo IX N4".into()],
            recent_maps: vec![54, 129],
            favourite_cars: vec!["XSARA".into(), "Mitsubishi Lancer Evo IX N4".into()],
            favourite_maps: vec![129, 54, 300],
            weather: Some(WeatherChoice::Preset(Preset::Bad)),
            wetness: Some(2),
            fog: Some(Fog::Heavy),
            clouds: Some(Clouds::Partly),
            precipitation: Some(Precipitation::Moderate),
            wear: Some(2),
            tyre: Some(TyreChoice::Fixed(5)),
            size: Some((1920, 1080)),
            fov: Some(72.5),
            volume: Some(0.75),
            engine_volume: Some(0.5),
            codriver_volume: Some(1.5),
            effects_volume: Some(0.0),
            tyres_volume: Some(2.0),
            codriver: Some(false),
            codriver_voice: Some("Janne Laahanen EnNu A".into()),
            codriver_style: Some("Numeric.ini".into()),
            codriver_ranges: Some("Rbr.ini".into()),
            codriver_pack: Some("luppis".into()),
            notes_pack: Some("tanner-notes".into()),
            my_pacenotes: Some(false),
            hud: Some(false),
            mph: Some(true),
            fps: Some(true),
            pace_stack: Some(2),
            dials: Some(false),
            hide_wheel: Some(true),
            hud_hide: Some(Parts::hiding("progress,car").unwrap()),
            wipers: Some(Switch::Intermittent),
            graphics: [
                Some(true),
                None,
                Some(false),
                Some(false),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(true),
                None,
            ],
            render_scale: Some(75),
            anti_aliasing: Some(AntiAliasing::Off),
            reflections: Some(Reflections::Off),
            detail: Some(Detail::Ultra),
            upscale_textures: Some(true),
            present: Some(Present::Immediate),
            mirrors: Some(Rate::Every(3)),
            fullscreen: Some(false),
            perf_log: Some(true),
            start_time: Some(16 * 60 + 30),
            callout_distance: Some(0.5),
            marker_threshold: Some(12.5),
            setup: Some("gravel fast".into()),
            damage: Some(DamageLevel::Realistic),
            auto_gears: Some(true),
            gear_guard: Some(false),
            neutral_lock: Some(true),
            clutch_help: Some(false),
            ghost: Some(GhostChoice::File("me_54.ghc".into())),
            championship_skill: Some(Skill::Champion),
            championship_damage: Some(DamageLevel::Reduced),
            championship_team: Some(5),
            install_folder: Some("/games/Richard Burns Rally".into()),
            ngp_physics: Some("/games/rbr-ngp/physics.rbz".into()),
            ngp_dll: Some("/games/ngp/Plugins/NGP.dll".into()),
            rsf_root: Some("/games/rsf".into()),
            head_gasket: Some(10),
            bindings: Bindings::default(),
            ffb: ffb::Options { rate: 500, ..Default::default() },
            camera: CameraPrefs::parse("[camera]\nview = bonnet\nrsf-24 chase = 0 0.5 -1\n"),
            season_cars: SeasonCars {
                last: Some("Skoda Fabia R5".into()),
                profiles: vec![
                    ("player".into(), "Mitsubishi Lancer Evo IX N4".into()),
                    ("me".into(), "Skoda Fabia R5".into()),
                ],
            },
            gearboxes: Gearboxes(vec![(
                "Ford Escort Mk II".into(),
                CarBox { engagement: Engagement::Dog, sequential: false },
            )]),
        };
        let text = settings.to_text();
        assert!(
            text.ends_with(
                "\n[camera]\nview = bonnet\nrsf-24 chase = 0 0.5 -1\n\n[season]\nlast = Skoda Fabia R5\n\
                 profile player = Mitsubishi Lancer Evo IX N4\nprofile me = Skoda Fabia R5\n\n\
                 [gearbox]\nFord Escort Mk II = dog\n"
            ),
            "{text}"
        );
        assert_eq!(settings.season_cars.of("me"), Some("Skoda Fabia R5"));
        let mut seasons = settings.season_cars.clone();
        seasons.set("player", "Skoda Fabia R5");
        assert_eq!(seasons.profiles.len(), 2);
        assert_eq!(seasons.of("player"), Some("Skoda Fabia R5"));
        // A car without a gearbox is sequential; picking sequential forgets it.
        let mut gearboxes = settings.gearboxes.clone();
        assert_eq!(gearboxes.of("Skoda Fabia R5"), SEQUENTIAL);
        assert!(!gearboxes.of("Ford Escort Mk II").sequential);
        gearboxes.set("Ford Escort Mk II", SEQUENTIAL);
        assert_eq!(gearboxes, Gearboxes::default());
        assert_eq!(Settings::parse(&text), settings);
        assert!(text.contains("map = 54\nrecent_cars = COROLLA|Mitsubishi Lancer Evo IX N4\nrecent_maps = 54,129\nfavourite_cars = XSARA|Mitsubishi Lancer Evo IX N4\nfavourite_maps = 129,54,300\n"));
        assert!(text.contains("fov = 72.5\nvolume = 0.75\n"), "the game chase's camera page is gone");
        // Any other word is a season row's section, not a preset.
        assert_eq!(Settings::parse("[launcher]\nweather = dry\n").weather, Some(WeatherChoice::Row("dry".into())));
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::parse("[launcher]\nfov = 121\n").fov, None);
        assert_eq!(Settings::parse("[launcher]\nfov = 30\n").fov, Some(30.0));
        assert_eq!(Settings::parse("[launcher]\nvolume = 5\n").volume, None);
        assert!(text.contains("volume = 0.75\n"));
        assert!(
            text.contains("engine_volume = 0.5\ncodriver_volume = 1.5\neffects_volume = 0\ntyres_volume = 2\n"),
            "{text}"
        );
        assert_eq!(Settings::parse("[launcher]\nengine_volume = 3\n").engine_volume, None);
        assert_eq!(Settings::parse("[launcher]\ncodriver_volume = loud\n").codriver_volume, None);
        assert_eq!(Settings::parse("[launcher]\neffects_volume = -1\n").effects_volume, None);
        assert_eq!(Settings::parse("[launcher]\ntyres_volume = \n").tyres_volume, None);
        assert!(text.contains("codriver = off\ncodriver_voice = Janne Laahanen EnNu A\ncodriver_style = Numeric.ini\n"));
        assert!(
            text.contains("codriver_ranges = Rbr.ini\ncodriver_pack = luppis\nnotes_pack = tanner-notes\n"),
            "{text}"
        );
        assert!(text.contains("my_pacenotes = off\nhud = off\n"), "{text}");
        let unset = Settings::parse("[launcher]\ncodriver_voice = \nmy_pacenotes = maybe\n");
        assert_eq!((unset.codriver_voice, unset.my_pacenotes), (None, None));
        assert!(text.contains("hud = off\nunits = mph\nfps = on\npace_stack = 2\nwipers = intermittent\n"));
        assert_eq!(Settings::parse("[launcher]\npace_stack = 5\n").pace_stack, None);
        assert!(
            text.contains("wipers = intermittent\ndials = off\nhide_wheel = on\nhud_hide = progress,car\n"),
            "{text}"
        );
        assert!(text.contains("anisotropy = on\ncabin = off\ncontact = off\nwater = on\n"), "{text}");
        assert!(text.contains("start_time = 16:30\nperf_log = on\n"), "{text}");
        assert_eq!(Settings::parse("[launcher]\nperf_log = maybe\n").perf_log, None);
        assert_eq!(Settings::parse("").perf_log, None);
        assert_eq!(Settings::parse("[launcher]\nstart_time = 24:00\n").start_time, None);
        assert_eq!(clock_args(Some(5 * 60)), ["--clock", "05:00"]);
        assert!(clock_args(None).is_empty());
        assert_eq!(Settings::parse("[launcher]\nwipers = fast\n").wipers, None);
        assert!(text.contains("render_scale = 75\nanti_aliasing = off\nreflections = off\ndetail = ultra\n"), "{text}");
        assert_eq!(Settings::parse("[launcher]\ndetail = high\n").detail, Some(Detail::High));
        assert_eq!(Settings::parse("[launcher]\ndetail = max\n").detail, None);
        assert_eq!(Settings::parse("[launcher]\nreflections = medium\n").reflections, Some(Reflections::Medium));
        assert_eq!(Settings::parse("[launcher]\nreflections = on\n").reflections, None);
        assert_eq!(Settings::parse("[launcher]\nanti_aliasing = 2x\n").anti_aliasing, None);
        assert!(text.contains("present = immediate\nmirrors = 3\n"), "{text}");
        assert_eq!(Settings::parse("[launcher]\nmirrors = 4\n").mirrors, None);
        assert_eq!(Settings::parse("[launcher]\nmirrors = off\n").mirrors, Some(Rate::Off));
        assert_eq!(Settings::parse("[launcher]\nrender_scale = 80\n").render_scale, None);
        assert_eq!(render_scale_args(67), ["--render-scale", "0.67"]);
        assert!(render_scale_args(100).is_empty());
        assert_eq!(Settings::parse("[launcher]\nrender_scale = 0\n").render_scale, None);
        assert!(text.contains("damage = realistic\nauto_gears = on\ngear_guard = off\n"));
        assert_eq!(
            Settings::parse("[launcher]\nweather = random\n").weather,
            Some(WeatherChoice::Preset(Preset::Random))
        );
        // A specific row is its section name, and the wetness and fog overrides round-trip (absent: from weather).
        assert_eq!(
            Settings::parse("[launcher]\nweather = 21O_E_heavyfog_heavycloud\n").weather,
            Some(WeatherChoice::Row("21O_E_heavyfog_heavycloud".into()))
        );
        assert!(
            text.contains("weather = bad\nwetness = wet\nfog = heavy\nclouds = partly\nprecipitation = moderate\n"),
            "{text}"
        );
        assert_eq!(Settings::parse(&text).weather, Some(WeatherChoice::Preset(Preset::Bad)), "{text}");
        assert_eq!((Settings::parse(&text).wetness, Settings::parse(&text).fog), (Some(2), Some(Fog::Heavy)));
        assert_eq!(Settings::parse("[launcher]\nweather = \nwetness = soaked\nfog = mist\n").weather, None);
        assert_eq!(Settings::parse("[launcher]\nwetness = damp\n").wetness, Some(1));
        assert_eq!(Settings::parse("[launcher]\nfog = light\n").fog, Some(Fog::Light));
        assert_eq!(
            (Settings::parse(&text).clouds, Settings::parse(&text).precipitation),
            (Some(Clouds::Partly), Some(Precipitation::Moderate))
        );
        assert_eq!(Settings::parse("[launcher]\nclouds = storm\nprecipitation = hail\n").clouds, None);
        assert_eq!(Settings::parse("[launcher]\nprecipitation = none\n").precipitation, Some(Precipitation::None));
        assert_eq!(Settings::parse("[launcher]\nghost = best\n").ghost, Some(GhostChoice::Best));
        let args = GhostChoice::Best.viewer_args(Path::new("/l"), 54, &Default::default());
        let best = Path::new("/l").join("GhostCars").join("Best_54.ghc");
        assert_eq!(args[..2], ["--best-ghost".to_owned(), best.display().to_string()]);
        assert!(GhostChoice::File("gone_54.ghc".into())
            .viewer_args(Path::new("/l"), 54, &Default::default())
            .is_empty());
        assert_eq!(Settings::parse("[launcher]\ndamage = hard\nclutch_help = yes\n"), Settings::default());
        assert!(text.contains("championship_skill = champion\nchampionship_damage = reduced\nchampionship_team = 5\n"));
        assert_eq!(Settings::parse("[launcher]\nchampionship_skill = legend\n"), Settings::default());
        assert_eq!(Settings::parse("[launcher]\nchampionship_team = 7\n"), Settings::default());
    }
}
