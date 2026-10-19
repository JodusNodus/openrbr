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
use rbr_render::mirrors::Rate;
use rbr_render::present::Present;
use rbr_render::renderer::Enhance;

use crate::catalog::{self, Car, Catalog, TyreChoice, WEARS};
use crate::controls_page::ControlsPage;
use crate::ghosts::GhostChoice;
use crate::graphics_page::{HudChoice, AUTO_SCALE, DEFAULT_GRAPHICS, DEFAULT_MIRRORS, MAX_VOLUME, RENDER_SCALES};
use crate::pages::Page;
use crate::rally_page::RallyPage;
use crate::setup_page::SetupPage;
use crate::{install, picker, preview, Form, Paths, Start};

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

/// The launcher's last selection, the viewer's bindings and its force feedback (the `[ffb]` section).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub car: Option<String>,
    pub map: Option<u32>,
    /// The last cars (their names) and maps picked, newest first (`recent_cars = A|B`, `recent_maps = 54,129`).
    pub recent_cars: Vec<String>,
    pub recent_maps: Vec<u32>,
    pub weather: Option<Preset>,
    pub wear: Option<u32>,
    pub tyre: Option<TyreChoice>,
    pub size: Option<(u32, u32)>,
    /// Every view's horizontal field of view, degrees (`viewer --fov`).
    pub fov: Option<f32>,
    /// The sound volume, linear, 0 to [`MAX_VOLUME`] (`viewer --volume`).
    pub volume: Option<f32>,
    /// The co-driver calls the pace notes (`viewer --no-codriver` when off).
    pub codriver: Option<bool>,
    /// The HUD shown (`viewer --no-hud` when off).
    pub hud: Option<bool>,
    /// The speed in mph (`viewer --mph`), else km/h.
    pub mph: Option<bool>,
    /// The FPS counter (`viewer --fps`).
    pub fps: Option<bool>,
    /// The pace stack's notes (`viewer --pace-stack`, 0 to 4).
    pub pace_stack: Option<u8>,
    /// The 3D pace notes (`viewer --pace-3d`).
    pub pace_3d: Option<bool>,
    /// The working dials (`viewer --no-dials` when off).
    pub dials: Option<bool>,
    /// The wiper switch at the start (`viewer --wipers`).
    pub wipers: Option<Switch>,
    /// Each renderer addition on or off (`viewer --enhance`), by [`Enhance::NAMES`], each kept under its name.
    pub graphics: [Option<bool>; Enhance::NAMES.len()],
    /// One of [`RENDER_SCALES`].
    pub render_scale: Option<u32>,
    /// The driving window's frame presentation (`viewer --present`).
    pub present: Option<Present>,
    /// How often the cockpit's mirrors are drawn (`viewer --mirrors`).
    pub mirrors: Option<Rate>,
    /// The driving window fills the monitor (`viewer --fullscreen`).
    pub fullscreen: Option<bool>,
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
    /// NGP's `physics.rbz` (`rbr-standalone --ngp-physics`) when not the install folder's. An older file's `ngp`
    /// (the physics choice) is ignored: the launcher drives NGP only.
    pub ngp_physics: Option<String>,
    /// The RallySimFans install (`Maps/<id>-<name>/` stage folders, `Cars/` and `rsfdata/` NGP cars) when not the
    /// install folder.
    pub rsf_root: Option<String>,
    pub bindings: Bindings,
    pub ffb: ffb::Options,
    /// The viewer's camera choices (`[camera]`, `rbr_render::camera_prefs`): the viewer writes them, the launcher
    /// carries them over and resets a car's.
    pub camera: CameraPrefs,
    /// The championship's picked cars (`[season]`).
    pub season_cars: SeasonCars,
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
        Settings {
            car: get("car"),
            map: get("map").and_then(|v| v.parse().ok()),
            recent_cars: get("recent_cars")
                .map(|v| v.split('|').map(str::trim).filter(|c| !c.is_empty()).map(str::to_owned).collect())
                .unwrap_or_default(),
            recent_maps: get("recent_maps")
                .map(|v| v.split(',').filter_map(|m| m.trim().parse().ok()).collect())
                .unwrap_or_default(),
            weather: get("weather").and_then(|v| match v.as_str() {
                "good" => Some(Preset::Good),
                "random" => Some(Preset::Random),
                "bad" => Some(Preset::Bad),
                _ => None,
            }),
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
            codriver: on_off(get("codriver")),
            hud: on_off(get("hud")),
            mph: get("units").and_then(|v| match v.as_str() {
                "mph" => Some(true),
                "kmh" => Some(false),
                _ => None,
            }),
            fps: on_off(get("fps")),
            pace_stack: get("pace_stack").and_then(|v| v.parse().ok()).filter(|n| *n <= 4),
            pace_3d: on_off(get("pace_3d")),
            dials: on_off(get("dials")),
            wipers: get("wipers").as_deref().and_then(Switch::parse),
            graphics: Enhance::NAMES.map(|name| on_off(get(name))),
            render_scale: get("render_scale").and_then(|v| match v.as_str() {
                "auto" => Some(AUTO_SCALE),
                v => v.parse().ok().filter(|v| *v != AUTO_SCALE && RENDER_SCALES.contains(v)),
            }),
            present: get("present").as_deref().and_then(Present::parse),
            mirrors: get("mirrors").as_deref().and_then(Rate::parse),
            fullscreen: on_off(get("fullscreen")),
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
            rsf_root: get("rsf_root").filter(|v| !v.is_empty()),
            bindings: Bindings::parse(text),
            ffb: ffb::Options::parse(text),
            camera: CameraPrefs::parse(text),
            season_cars: SeasonCars::parse(&sections),
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
            ("weather", self.weather.map(|v| weather::name(v).to_owned())),
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
            ("codriver", on_off_text(self.codriver)),
            ("hud", on_off_text(self.hud)),
            ("units", self.mph.map(|v| if v { "mph" } else { "kmh" }.to_owned())),
            ("fps", on_off_text(self.fps)),
            ("pace_stack", self.pace_stack.map(|v| v.to_string())),
            ("pace_3d", on_off_text(self.pace_3d)),
            ("wipers", self.wipers.map(|v| v.name().to_owned())),
            ("dials", on_off_text(self.dials)),
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
            ("rsf_root", self.rsf_root.clone()),
            (
                "render_scale",
                self.render_scale.map(|v| if v == AUTO_SCALE { "auto".to_owned() } else { v.to_string() }),
            ),
            ("present", self.present.map(|v| v.name().to_owned())),
            ("mirrors", self.mirrors.map(Rate::name)),
            ("fullscreen", on_off_text(self.fullscreen)),
            ("start_time", self.start_time.map(clock_text)),
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
        // An older file's retail car is not in the list: the most recent RSF car then, else the first that starts.
        let car = pick_car(&catalog.cars, settings.car.iter().chain(&settings.recent_cars));
        let season_pick = pick_car(&catalog.cars, settings.season_cars.last.iter().chain(&settings.car));
        let default_map = catalog.maps.iter().position(|map| catalog.availability(&season, map).is_ok()).unwrap_or(0);
        let map = settings
            .map
            .and_then(|number| catalog.maps.iter().position(|map| map.number == number))
            .unwrap_or(default_map);
        let aids = DriverAids::default();
        let options = RaceOptions {
            weather: settings.weather.unwrap_or(Preset::Good),
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
            car_picker: picker::Picker::new(picker::Kind::Car),
            season_pick,
            season_picker: picker::Picker::new(picker::Kind::Car),
            season_cars: settings.season_cars.clone(),
            // Filled by `list_maps`.
            map_picker: picker::Picker::new(picker::Kind::Map),
            recent_cars: settings.recent_cars.clone(),
            recent_maps: settings.recent_maps.clone(),
            options,
            rand: CrtRand::srand(now.as_secs() as u32),
            wear,
            tyre,
            size,
            sizes: vec![size],
            fov,
            volume,
            codriver: settings.codriver.unwrap_or(true),
            ghost: settings.ghost.clone().unwrap_or_default(),
            menu_sound: None,
            hud: HudChoice {
                show: settings.hud.unwrap_or(true),
                mph: settings.mph.unwrap_or(false),
                fps: settings.fps.unwrap_or(false),
                stack: settings.pace_stack.unwrap_or(0),
                pace_3d: settings.pace_3d.unwrap_or(false),
                dials: settings.dials.unwrap_or(true),
            },
            graphics: std::array::from_fn(|k| settings.graphics[k].unwrap_or(DEFAULT_GRAPHICS[k])),
            render_scale: settings.render_scale.unwrap_or(AUTO_SCALE),
            present: settings.present.unwrap_or_default(),
            mirrors: settings.mirrors.unwrap_or(DEFAULT_MIRRORS),
            fullscreen: settings.fullscreen.unwrap_or(true),
            start_time: settings.start_time,
            wipers: settings.wipers.unwrap_or_default(),
            install_folder: settings.install_folder.clone().unwrap_or_default(),
            folder,
            disc_setup: Default::default(),
            adding,
            reload: false,
            ngp_physics: settings.ngp_physics.clone().unwrap_or_default(),
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
            controls: ControlsPage::new(),
            read_only: false,
        };
        form.list_cars();
        form.list_maps();
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
            weather: Some(self.options.weather),
            wear: Some(self.wear),
            tyre: Some(self.tyre),
            size: Some(self.size),
            fov: Some(self.fov),
            volume: Some(self.volume),
            codriver: Some(self.codriver),
            hud: Some(self.hud.show),
            mph: Some(self.hud.mph),
            fps: Some(self.hud.fps),
            pace_stack: Some(self.hud.stack),
            pace_3d: Some(self.hud.pace_3d),
            dials: Some(self.hud.dials),
            graphics: self.graphics.map(Some),
            render_scale: Some(self.render_scale),
            present: Some(self.present),
            mirrors: Some(self.mirrors),
            fullscreen: Some(self.fullscreen),
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
            rsf_root: Some(self.rsf_root.trim().to_owned()).filter(|p| !p.is_empty()),
            bindings: self.bindings.clone(),
            ffb: self.ffb.clone(),
            // The viewer owns these: carried over from the file as it is now.
            camera: read_settings(&self.paths.settings).camera,
            season_cars: self.season_cars.clone(),
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
    fn settings_round_trip_and_omit_obsolete_keys() {
        let settings = Settings {
            car: Some("COROLLA".into()),
            map: Some(54),
            recent_cars: vec!["COROLLA".into(), "Mitsubishi Lancer Evo IX N4".into()],
            recent_maps: vec![54, 129],
            weather: Some(Preset::Bad),
            wear: Some(2),
            tyre: Some(TyreChoice::Fixed(5)),
            size: Some((1920, 1080)),
            fov: Some(72.5),
            volume: Some(0.75),
            codriver: Some(false),
            hud: Some(false),
            mph: Some(true),
            fps: Some(true),
            pace_stack: Some(2),
            pace_3d: Some(true),
            dials: Some(false),
            wipers: Some(Switch::Intermittent),
            graphics: [
                Some(true),
                None,
                Some(false),
                None,
                Some(false),
                Some(false),
                Some(true),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(true),
            ],
            render_scale: Some(75),
            present: Some(Present::Immediate),
            mirrors: Some(Rate::Every(3)),
            fullscreen: Some(false),
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
            rsf_root: Some("/games/rsf".into()),
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
        };
        let text = settings.to_text();
        assert!(
            text.ends_with(
                "\n[camera]\nview = bonnet\nrsf-24 chase = 0 0.5 -1\n\n[season]\nlast = Skoda Fabia R5\n\
                 profile player = Mitsubishi Lancer Evo IX N4\nprofile me = Skoda Fabia R5\n"
            ),
            "{text}"
        );
        assert_eq!(settings.season_cars.of("me"), Some("Skoda Fabia R5"));
        let mut seasons = settings.season_cars.clone();
        seasons.set("player", "Skoda Fabia R5");
        assert_eq!(seasons.profiles.len(), 2);
        assert_eq!(seasons.of("player"), Some("Skoda Fabia R5"));
        // The 1.02 physics choice of an older file is ignored.
        assert_eq!(Settings::parse("[launcher]\nngp = off\n"), Settings::default());
        assert!(!text.contains("ngp ="));
        assert_eq!(Settings::parse(&text), settings);
        assert!(text.contains("map = 54\nrecent_cars = COROLLA|Mitsubishi Lancer Evo IX N4\nrecent_maps = 54,129\n"));
        assert!(text.contains("fov = 72.5\nvolume = 0.75\n"), "the game chase's camera page is gone");
        assert!(!text.contains("stage ="));
        assert_eq!(Settings::parse("[launcher]\nstage = track-54_E\nweather = dry\n"), Settings::default());
        assert_eq!(Settings::parse(""), Settings::default());
        assert_eq!(Settings::parse("[launcher]\nfov = 121\n").fov, None);
        assert_eq!(Settings::parse("[launcher]\nfov = 30\n").fov, Some(30.0));
        assert_eq!(Settings::parse("[launcher]\nvolume = 5\n").volume, None);
        assert!(text.contains("volume = 0.75\n"));
        assert!(text.contains("codriver = off\nhud = off\n"));
        // The dropped music keys of an older file are ignored.
        assert_eq!(Settings::parse("[launcher]\nmusic = on\nmusic_volume = 0.5\n"), Settings::default());
        assert!(
            text.contains("hud = off\nunits = mph\nfps = on\npace_stack = 2\npace_3d = on\nwipers = intermittent\n")
        );
        assert_eq!(Settings::parse("[launcher]\npace_stack = 5\n").pace_stack, None);
        assert!(text.contains("wipers = intermittent\ndials = off\n"), "{text}");
        assert!(text.contains("anisotropy = on\ncabin = off\ncontact = off\nhdr = off\nmsaa = on\nwind = on\n"));
        // Night is a start time now: an older file's `night` key is ignored.
        assert_eq!(Settings::parse("[launcher]\nnight = on\n"), Settings::default());
        // The lens dirt is gone: an older file's `lens` key is ignored.
        assert_eq!(Settings::parse("[launcher]\nlens = on\n"), Settings::default());
        assert!(text.contains("start_time = 16:30\n"));
        assert_eq!(Settings::parse("[launcher]\nstart_time = 24:00\n").start_time, None);
        assert_eq!(clock_args(Some(5 * 60)), ["--clock", "05:00"]);
        assert!(clock_args(None).is_empty());
        assert_eq!(Settings::parse("[launcher]\nwipers = fast\n").wipers, None);
        assert!(text.contains("render_scale = 75\n"));
        assert!(text.contains("present = immediate\nmirrors = 3\n"), "{text}");
        assert_eq!(Settings::parse("[launcher]\nmirrors = 4\n").mirrors, None);
        assert_eq!(Settings::parse("[launcher]\nmirrors = off\n").mirrors, Some(Rate::Off));
        assert_eq!(Settings::parse("[launcher]\nrender_scale = 80\n").render_scale, None);
        assert_eq!(render_scale_args(67), ["--render-scale", "0.67"]);
        assert!(render_scale_args(100).is_empty());
        assert_eq!(Settings::parse("[launcher]\nrender_scale = auto\n").render_scale, Some(AUTO_SCALE));
        assert_eq!(Settings::parse("[launcher]\nrender_scale = 0\n").render_scale, None);
        assert_eq!(render_scale_args(AUTO_SCALE), ["--render-scale", "auto"]);
        assert!(text.contains("damage = realistic\nauto_gears = on\ngear_guard = off\n"));
        assert_eq!(Settings::parse("[launcher]\nweather = random\n").weather, Some(Preset::Random));
        assert_eq!(Settings::parse("[launcher]\nghost = best\n").ghost, Some(GhostChoice::Best));
        let args = GhostChoice::Best.viewer_args(Path::new("/l"), 54, &Default::default());
        assert_eq!(args[..2], ["--best-ghost".to_owned(), "/l/GhostCars/Best_54.ghc".to_owned()]);
        assert!(GhostChoice::File("gone_54.ghc".into())
            .viewer_args(Path::new("/l"), 54, &Default::default())
            .is_empty());
        assert_eq!(Settings::parse("[launcher]\ndamage = hard\nclutch_help = yes\n"), Settings::default());
        assert!(text.contains("championship_skill = champion\nchampionship_damage = reduced\nchampionship_team = 5\n"));
        assert_eq!(Settings::parse("[launcher]\nchampionship_skill = legend\n"), Settings::default());
        assert_eq!(Settings::parse("[launcher]\nchampionship_team = 7\n"), Settings::default());
    }
}
