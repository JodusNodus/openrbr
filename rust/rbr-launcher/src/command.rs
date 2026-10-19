//! The viewer command Start runs: the selection resolved to a stage and weather, the viewer's and
//! `rbr-standalone`'s arguments, and the viewer as a child process.

use std::path::Path;
use std::process::Command;

use rbr_formats::saves::SaveDir;
use rbr_gameplay::crt::CrtRand;
use rbr_gameplay::markers::MarkerSettings;
use rbr_gameplay::race_options::RaceOptions;
use rbr_gameplay::weather::{self, Draws, Preset, Season, Weather};
use rbr_render::glass::model::Switch;
use rbr_render::renderer::Enhance;

use crate::catalog::{self, Car, Stage, TyreChoice};
use crate::graphics_page::{Graphics, HudChoice, AUTO_SCALE};
use crate::settings::clock_text;
use crate::{ghosts, rally_page, Form};

/// The viewer's `--clock` arguments for a start time, none for the stage's own light.
pub fn clock_args(start: Option<u32>) -> Vec<String> {
    start.map_or_else(Vec::new, |minutes| vec!["--clock".into(), clock_text(minutes)])
}

/// The viewer's `--render-scale` arguments for `percent` ([`AUTO_SCALE`]: `auto`), none at full size.
pub fn render_scale_args(percent: u32) -> Vec<String> {
    match percent {
        100 => Vec::new(),
        AUTO_SCALE => vec!["--render-scale".into(), "auto".into()],
        percent => vec!["--render-scale".into(), (f64::from(percent) / 100.0).to_string()],
    }
}

/// The `viewer --enhance` list of the additions on, or `None` with none on.
pub fn enhance_list(graphics: &Graphics) -> Option<String> {
    let on: Vec<&str> = Enhance::NAMES.iter().zip(graphics).filter(|(_, on)| **on).map(|(name, _)| *name).collect();
    (!on.is_empty()).then(|| on.join(","))
}

/// The viewer and `rbr-standalone` arguments for a boot launch.
#[allow(clippy::too_many_arguments)]
pub fn viewer_args(
    stage: &Stage,
    car: &Car,
    options: &RaceOptions,
    weather: &Weather,
    tyre: u32,
    size: (u32, u32),
    fov: f32,
    volume: f32,
    rain: &str,
    codriver: bool,
    hud: HudChoice,
    graphics: &Graphics,
    wipers: Switch,
    standalone: &Path,
    data: &Path,
    marker: MarkerSettings,
    bindings: Option<&Path>,
    setup: Option<&Path>,
) -> Vec<String> {
    let pack = car.rsf.as_ref().and_then(|r| r.pack.as_ref().ok()).map(|p| p.display().to_string());
    let car_arg = match &pack {
        Some(pack) => ["--rsf-car".into(), pack.clone()],
        None => ["--car".into(), car.name.clone()],
    };
    let mut args: Vec<String> = vec![
        stage.name.clone(),
        car_arg[0].clone(),
        car_arg[1].clone(),
        "--weather".into(),
        weather.mat_words().1.into(),
        "--size".into(),
        format!("{}x{}", size.0, size.1),
        "--fov".into(),
        fov.to_string(),
        "--volume".into(),
        volume.to_string(),
        "--rain".into(),
        rain.into(),
    ];
    if !codriver {
        args.push("--no-codriver".into());
    }
    if !hud.show {
        args.push("--no-hud".into());
    }
    if hud.mph {
        args.push("--mph".into());
    }
    args.extend(["--wipers".into(), wipers.name().into()]);
    if hud.fps {
        args.push("--fps".into());
    }
    if hud.stack > 0 {
        args.extend(["--pace-stack".into(), hud.stack.to_string()]);
    }
    if hud.pace_3d {
        args.push("--pace-3d".into());
    }
    if !hud.dials {
        args.push("--no-dials".into());
    }
    if let Some(path) = bindings {
        let path = path.display().to_string();
        args.extend(["--bindings".into(), path.clone(), "--cameras".into(), path]);
    }
    if let Some(list) = enhance_list(graphics) {
        args.extend(["--enhance".into(), list]);
    }
    args.extend([
        "--live".into(),
        standalone.display().to_string(),
        "--live".into(),
        "--boot".into(),
        "--weather".into(),
        options.weather.id().to_string(),
        "--wear".into(),
        catalog::WEARS[weather.wear.min(2) as usize].into(),
        "--tyre".into(),
        tyre.to_string(),
        "--stage".into(),
        stage.name.clone(),
        "--car".into(),
        car.index.to_string(),
        "--data".into(),
        data.display().to_string(),
        "--callout-distance".into(),
        marker.callout_distance.to_string(),
        "--marker-threshold".into(),
        marker.marker_threshold.to_string(),
    ]);
    if let (Preset::Random, Some(row)) = (options.weather, weather.row) {
        args.extend(["--season-row".into(), row.to_string()]);
    }
    let on_off = |on: bool| if on { "on" } else { "off" }.to_owned();
    let aids = options.aids;
    args.extend([
        "--damage".into(),
        options.damage.name().into(),
        "--auto-gears".into(),
        on_off(aids.auto_gears),
        "--gear-guard".into(),
        on_off(aids.gear_guard),
        "--neutral-lock".into(),
        on_off(aids.neutral_lock),
        "--clutch-help".into(),
        on_off(aids.clutch_help),
    ]);
    if let Some(path) = setup {
        args.extend(["--setup".into(), path.display().to_string()]);
    }
    if let Some(pack) = pack {
        args.extend(["--rsf-car".into(), pack]);
    }
    args
}

/// The viewer's arguments for the advanced H-shifter from RBRControls' files in the RSF install `rsf_root`: an RSF
/// car's gearbox from `Plugins/RBRControls/CarInfo.ini` (`--car-gearbox`; not an added car's, whose id is ours) and
/// the grind, `Shifter_Error.wav` (or the `.wav.sample` RBRControls ships it as; `--grind-sound`). None without the
/// install or the files.
pub fn shifter_args(car: &Car, rsf_root: &Path) -> Vec<String> {
    if rsf_root.as_os_str().is_empty() {
        return Vec::new();
    }
    let dir = rsf_root.join("Plugins").join("RBRControls");
    let mut args = Vec::new();
    let text = std::fs::read(dir.join("CarInfo.ini")).map(|b| String::from_utf8_lossy(&b).into_owned());
    if let (Some(rsf), Ok(text)) = (car.rsf.as_ref().filter(|r| !r.added), text) {
        if let Some(gearbox) = rbr_render::shifter::CarBox::from_car_info(&text, rsf.id) {
            args.extend(["--car-gearbox".into(), gearbox.arg()]);
        }
    }
    let grind =
        ["Shifter_Error.wav", "Shifter_Error.wav.sample"].map(|n| dir.join(n)).into_iter().find(|p| p.is_file());
    if let Some(wav) = grind {
        args.extend(["--grind-sound".into(), wav.display().to_string()]);
    }
    args
}

/// The rain of `weather`'s season row (`viewer --rain`): its weather id 3 is light rain, 4 heavy (`notes/gameplay.md` "Weather").
pub fn rain(season: &Season, weather: &Weather) -> &'static str {
    match weather.row.and_then(|k| season.rows.get(k)).map(|r| r.preset) {
        Some(3) => "light",
        Some(4) => "heavy",
        _ => "none",
    }
}

impl Form {
    /// Resolves the current preset into its stage variant and weather row: the random weather's from the next draw of
    /// the stream (`rand`), without taking it.
    pub fn selection(&self) -> Result<(catalog::Stage, Weather), String> {
        self.selection_from(&mut self.rand.clone())
    }

    fn selection_from(&self, rand: &mut CrtRand) -> Result<(catalog::Stage, Weather), String> {
        let map = self.catalog.maps[self.map].number;
        if let Some(chosen) = self.ghost_weather() {
            return Ok(chosen);
        }
        let draws = match self.options.weather {
            Preset::Random => weather::random_draws(&self.catalog.maps[self.map].season(&self.season), map, rand)
                .ok_or(format!("map {map} has no season row (the original stops at its int3)"))?,
            _ => Draws { row: None, wear: self.wear },
        };
        self.catalog.stage_weather(&self.season, map, self.options.weather, draws)
    }

    pub fn tyre_id(&self, weather: &Weather) -> Result<u32, String> {
        match self.tyre {
            TyreChoice::Auto => weather::tyre_kind(self.catalog.maps[self.map].surface(), weather.wetness)
                .ok_or_else(|| "the map surface and wetness have no tyre mapping".into()),
            TyreChoice::Fixed(id) if id <= 6 => Ok(id),
            TyreChoice::Fixed(id) => Err(format!("tyre {id}: expected 0 through 6")),
        }
    }

    pub fn start(&mut self) {
        let mut rand = self.rand;
        if let Some(args) = self.start_args(&mut rand) {
            if self.spawn(args) {
                self.rand = rand;
            }
        }
    }

    /// The viewer's arguments for the quick race Start runs, drawing the random weather from `rand`.
    pub fn start_args(&mut self, rand: &mut CrtRand) -> Option<Vec<String>> {
        let (stage, weather) = self.selection_from(rand).ok()?;
        // Only the quick race races and keeps a ghost (`rbr_gameplay::ghost::clears`: the championship clears it).
        let words = weather.ghost_words(&self.season);
        let ghost = match self.rsf_ghost_key(&weather) {
            Some(key) => {
                ghosts::rsf_ghost_args(&self.ghost, &self.records(), Path::new(self.rsf().trim()), &key, &words)
            }
            None => self.ghost.viewer_args(&self.records(), stage.map, &words),
        };
        // The quick race keeps the stage record and the personal best in the game's save files next to the launcher.
        let saves = ["--saves".to_owned(), SaveDir::new(&self.records()).path.display().to_string()];
        let mut extra = [saves.as_slice(), &["--profile".to_owned(), self.rally.profile.clone()]].concat();
        // The start board shows the time of day the stage is drawn at (a championship stage keeps the season's).
        // The Time of day is the quick race's: the stage is drawn at it and the start board shows it. A championship
        // stage keeps the stage's own light and the season's start clock, as the game's.
        if let Some(m) = self.start_time {
            extra.extend(["--start-time".to_owned(), format!("{}:{:02}", m / 60, m % 60)]);
        }
        let viewer_extra = [ghost, clock_args(self.start_time)].concat();
        self.launch_args(stage, self.options, weather, extra, viewer_extra)
    }

    /// Starts a championship stage on its season-table row (the random preset's row) with the stage's tyres, road
    /// wear and the season's options.
    pub fn start_rally_stage(&mut self, start: rally_page::StageStart) {
        let rally_page::StageStart { stage, args: mut extra, tyre, wear, setup } = start;
        // A finished championship stage is offered to the stage record and the personal best too (`M00480810` calls
        // `M00567330` in every mode but 6, 7 and 9); the trials are practice.
        if extra.iter().any(|a| a == "--championship") {
            let saves = SaveDir::new(&self.records()).path.display().to_string();
            extra.extend(["--saves".to_owned(), saves, "--profile".to_owned(), self.rally.profile.clone()]);
        }
        let draws = weather::Draws { row: Some(stage.row), wear: wear.max(1) };
        let found = weather::quick_race(&self.season, stage.map, Preset::Random, draws).and_then(|weather| {
            let name = stage.track();
            let map = self.catalog.maps.iter().find(|m| m.number == stage.map);
            let s =
                map.and_then(|m| m.stages.iter().find(|s| s.name == name)).ok_or(format!("{name}: not installed"))?;
            Ok((s.clone(), weather))
        });
        match found {
            Ok((s, weather)) => {
                self.map = self.catalog.maps.iter().position(|m| m.number == stage.map).unwrap_or(self.map);
                // The setup row picked for the stage, on the stage's surface (`CSeasonChooseSetupUI`).
                self.select_setup();
                if let Err(e) = self.setup.choose(&self.here(), setup) {
                    eprintln!("launcher: setup {}: {e}; the stage keeps the current setup", setup.label());
                }
                // The quick race's options keep their menu values; the stage takes the season's from `extra`.
                let options = RaceOptions { weather: Preset::Random, ..self.options };
                // The road's wear is the season's (`Draws` takes the random weather's 1 or 2; 0 is set after).
                let weather = Weather { wear, ..weather };
                let chosen = std::mem::replace(&mut self.tyre, TyreChoice::Fixed(tyre));
                if let Some(args) = self.launch_args(s, options, weather, extra, Vec::new()) {
                    self.spawn(args);
                }
                self.tyre = chosen;
                self.rally.setup_text = self.setup.current_text().into_bytes();
            }
            Err(e) => self.status = e,
        }
        if self.child.is_none() {
            // Not started: the page takes it as a stage left at once.
            self.rally.stage_closed(&self.here());
        }
    }

    /// The viewer's arguments for `stage`; `extra` goes to `rbr-standalone`, `viewer_extra` to the viewer. `None`, with
    /// the status saying why, when it cannot start.
    fn launch_args(
        &mut self,
        stage: catalog::Stage,
        mut options: RaceOptions,
        weather: Weather,
        extra: Vec<String>,
        mut viewer_extra: Vec<String>,
    ) -> Option<Vec<String>> {
        let tyre = self.tyre_id(&weather).ok()?;
        let Some(car) = self.car() else {
            self.status = "no car: the Settings page's RallySimFans folder has none".into();
            return None;
        };
        if self.bindings.shifter.advanced {
            // Clutch help ignores the clutch pedal the advanced H-shifter needs.
            options.aids.clutch_help = false;
            viewer_extra.extend(shifter_args(car, Path::new(self.rsf_root.trim())));
        }
        self.select_setup();
        let setup = match self.setup.current_file(&self.here()) {
            Ok(setup) => setup,
            Err(e) => {
                self.status = format!("setup: {e}");
                return None;
            }
        };
        let car = &self.catalog.cars[self.car];
        // Read-only, the file already holds the selection and the bindings, when there is one.
        let saved = self.save() || self.read_only && self.paths.settings.is_file();
        let bindings = saved.then_some(self.paths.settings.as_path());
        let mut args = viewer_args(
            &stage,
            car,
            &options,
            &weather,
            tyre,
            self.size,
            self.fov,
            self.volume,
            rain(&self.season, &weather),
            self.codriver,
            self.hud,
            &self.graphics,
            self.wipers,
            &self.paths.standalone,
            &self.paths.data,
            self.marker,
            bindings,
            setup.as_deref(),
        );
        // The viewer's `--live` command is the rest of the line. The render scale needs HDR.
        let live = args.iter().position(|a| a == "--live").unwrap_or(args.len());
        let scale = render_scale_args(self.render_scale).into_iter().filter(|_| self.hdr());
        let present = ["--present".to_owned(), self.present.name().to_owned()];
        let mirrors = ["--mirrors".to_owned(), self.mirrors.name()];
        let fullscreen = self.fullscreen.then(|| "--fullscreen".to_owned());
        let viewer_extra = viewer_extra.into_iter().chain(scale).chain(present).chain(mirrors).chain(fullscreen);
        args.splice(live..live, viewer_extra);
        args.extend(extra);
        args.extend(["--ngp-physics".to_owned(), self.ngp_archive()]);
        if self.ffb.sensitivity != [100; 3] {
            let [gravel, tarmac, snow] = self.ffb.sensitivity;
            args.extend(["--ffb-sensitivity".to_owned(), format!("{gravel},{tarmac},{snow}")]);
        }
        Some(args)
    }

    /// Runs the viewer with `args`; whether it started.
    fn spawn(&mut self, args: Vec<String>) -> bool {
        eprintln!("launcher: {} {}", self.paths.viewer.display(), args.join(" "));
        let mut command = Command::new(&self.paths.viewer);
        command.args(&args).envs(self.child_env());
        if self.paths.patch.is_none() {
            command.env_remove("RBR_PATCH");
        }
        match command.spawn() {
            Ok(child) => {
                self.status = format!("viewer running (pid {})", child.id());
                self.child = Some(child);
                true
            }
            Err(e) => {
                self.status = format!("{}: {e}", self.paths.viewer.display());
                false
            }
        }
    }

    /// Notes the viewer's exit.
    pub fn poll(&mut self) {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.status = format!("viewer exited ({status})");
                    self.child = None;
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = format!("viewer: {e}");
                    self.child = None;
                }
            }
            if self.child.is_none() {
                self.rally.stage_closed(&self.here());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, RsfCar};
    use crate::graphics_page::{graphics_index, DEFAULT_GRAPHICS};
    use crate::settings::{load, Settings};
    use crate::{install, Paths, Start, SETTINGS_FILE};
    use rbr_formats::data::Data;
    use rbr_gameplay::race_options::{DamageLevel, DriverAids};
    use rbr_gameplay::season_events::Turn;
    use rbr_gameplay::service::ParkState;
    use std::path::PathBuf;

    /// Saved settings and the viewer command Start runs for them (`testdata/commands.txt`): the pages may move,
    /// the command stays. Needs `RBR_DATA`, `RBR_PATCH`, `RBR_NGP_PHYSICS` and `RBR_RSF` (unsaved, the last two are
    /// the defaults); `{ngp}` and `{rsf}` are theirs. `RBR_LAUNCHER_BLESS=1` writes the file.
    const COMMAND_CASES: [(&str, &str); 8] = [
        ("defaults", ""),
        (
            "everything changed",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nweather = bad\nwear = worn\ntyre = 5\n\
             size = 1920x1080\nfov = 72.5\nvolume = 0.75\ncodriver = off\nhud = off\nunits = mph\nfps = on\n\
             pace_stack = 2\npace_3d = on\nwipers = intermittent\ncallout_distance = 0.5\nmarker_threshold = 12.5\n\
             damage = realistic\nauto_gears = on\ngear_guard = off\nneutral_lock = on\nclutch_help = off\n\
             ghost = best\nrender_scale = 75\npresent = immediate\nfullscreen = off\nstart_time = 16:30\nhdr = on\n\
             taa = on\nsky = off\n",
        ),
        (
            "random weather, advanced shifter",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 71\nweather = random\nsize = 1280x720\n\
             [controls]\nh-shifter = advanced\n",
        ),
        ("NGP", "[launcher]\ncar = COROLLA\nmap = 33\nngp = on\nngp_physics = {ngp}\nweather = good\n"),
        // An older file's 1.02 physics and a car NGP doesn't drive: the most recent car offered, on NGP.
        (
            "an older file's 1.02 and retail car",
            "[launcher]\ncar = XSARA\nrecent_cars = XSARA|Mitsubishi Lancer Evo IX N4\nmap = 33\nngp = off\n\
             weather = good\n",
        ),
        (
            "RSF car on a retail stage",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nngp = on\nngp_physics = {ngp}\n\
             rsf_root = {rsf}\nghost = best\n",
        ),
        (
            "RSF car and stage",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 129\nngp = on\nngp_physics = {ngp}\n\
             rsf_root = {rsf}\nghost = best\n",
        ),
        (
            "force feedback sensitivity",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\n[ffb]\nsensitivity_gravel = 70\n\
             sensitivity_snow = 130\n",
        ),
    ];

    /// An RSF car goes to the viewer and to `rbr-standalone` as its pack, in its slot.
    #[test]
    fn viewer_args_name_an_rsf_cars_pack() {
        let stage = Stage { name: "track-54_E".into(), map: 54, letter: 'E' };
        let rsf =
            RsfCar { id: 24, added: false, group: None, pack: Ok("/rsf/rsfdata/cars/evo".into()), model: Ok(None) };
        let car = Car { index: 5, name: "Mitsubishi Lancer Evo IX N4".into(), rsf: Some(rsf) };
        assert_eq!(car.unavailable(), None);
        let weather = Weather { map: 54, row: Some(162), letter: 4, wetness: 1, wear: 0 };
        let args = viewer_args(
            &stage,
            &car,
            &RaceOptions::default(),
            &weather,
            4,
            (1280, 720),
            65.0,
            1.0,
            "none",
            true,
            HudChoice::default(),
            &Graphics::default(),
            Switch::Off,
            Path::new("/b/rbr-standalone"),
            Path::new("/d"),
            MarkerSettings::default(),
            None,
            None,
        );
        let live = args.iter().position(|a| a == "--live").unwrap();
        assert_eq!(args[1..3], ["--rsf-car", "/rsf/rsfdata/cars/evo"]);
        assert!(!args[..live].contains(&"--car".to_owned()));
        assert_eq!(args[args.len() - 2..], ["--rsf-car", "/rsf/rsfdata/cars/evo"]);
        let car_at = args.iter().rposition(|a| a == "--car").unwrap();
        assert_eq!(args[car_at + 1], "5");
    }

    #[test]
    fn viewer_args_include_game_weather_wear_and_tyre() {
        let stage = Stage { name: "track-54_O".into(), map: 54, letter: 'O' };
        let car = Car { index: 6, name: "COROLLA".into(), rsf: None };
        let weather = Weather { map: 54, row: Some(167), letter: 3, wetness: 2, wear: 2 };
        let options = RaceOptions { weather: Preset::Bad, ..RaceOptions::default() };
        let args = viewer_args(
            &stage,
            &car,
            &options,
            &weather,
            5,
            (1280, 720),
            65.0,
            0.5,
            "heavy",
            false,
            HudChoice { show: false, mph: true, fps: true, stack: 3, pace_3d: true, dials: false },
            &Graphics::default(),
            Switch::Continuous,
            Path::new("/b/rbr-standalone"),
            Path::new("/d"),
            MarkerSettings::default(),
            Some(Path::new("/l/launcher-settings.ini")),
            Some(Path::new("/l/setup-current.lsp")),
        );
        assert_eq!(
            args.join(" "),
            "track-54_O --car COROLLA --weather wet --size 1280x720 --fov 65 --volume 0.5 --rain heavy --no-codriver \
             --no-hud --mph --wipers continuous --fps --pace-stack 3 --pace-3d --no-dials --bindings /l/launcher-settings.ini \
             --cameras /l/launcher-settings.ini --live /b/rbr-standalone --live --boot --weather 2 --wear worn --tyre 5 \
             --stage track-54_O --car 6 --data /d --callout-distance 4 --marker-threshold 25 \
             --damage safe --auto-gears off --gear-guard on --neutral-lock off --clutch-help on --setup /l/setup-current.lsp"
        );
        let options = RaceOptions {
            weather: Preset::Random,
            damage: DamageLevel::Realistic,
            aids: DriverAids { auto_gears: true, gear_guard: false, neutral_lock: true, clutch_help: false },
        };
        let args = viewer_args(
            &stage,
            &car,
            &options,
            &weather,
            5,
            (1280, 720),
            65.0,
            0.5,
            "heavy",
            true,
            HudChoice::default(),
            &["hdr", "taa"].iter().fold(Graphics::default(), |mut g, name| {
                g[graphics_index(name)] = true;
                g
            }),
            Switch::Auto,
            Path::new("/b/rbr-standalone"),
            Path::new("/d"),
            MarkerSettings::default(),
            None,
            None,
        )
        .join(" ");
        assert!(args.contains("--boot --weather 1 --wear worn"), "{args}");
        assert!(args.contains("--enhance hdr,taa --live "), "{args}");
        assert!(
            args.ends_with(
                "--season-row 167 --damage realistic --auto-gears on --gear-guard off --neutral-lock on --clutch-help off"
            ),
            "{args}"
        );
    }

    #[test]
    fn the_enhance_list_names_what_the_viewer_takes() {
        assert_eq!(enhance_list(&Graphics::default()), None);
        let all = enhance_list(&[true; Enhance::NAMES.len()]).unwrap();
        assert_eq!(all, "anisotropy,ao,cabin,clouds,contact,hdr,msaa,particles,sky,sun,taa,water,wet,wind");
        assert!(Enhance::parse(&all).is_some_and(|e| e.taa && e.ambient_occlusion));
        // A removed addition in an older list is ignored.
        assert_eq!(Enhance::parse("hdr,lens,taa"), Enhance::parse("hdr,taa"));
        assert!(enhance_list(&DEFAULT_GRAPHICS).as_deref().and_then(Enhance::parse).is_some_and(|e| !e
            .ambient_occlusion
            && e.cabin
            && !e.clouds
            && e.water
            && !e.wet));
    }

    /// The advanced H-shifter takes an RSF car's gearbox and the grind from RBRControls' folder.
    #[test]
    fn shifter_args_read_rbrcontrols_files() {
        let root = std::env::temp_dir().join(format!("rbr-launcher-shifter-{}", std::process::id()));
        let dir = root.join("Plugins").join("RBRControls");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CarInfo.ini"), "[Car24]\nh_pattern = 1\nengagement = 1\ndogleg = 0\n").unwrap();
        let rsf = |id| RsfCar { id, added: false, group: None, pack: Ok("/pack".into()), model: Ok(None) };
        let car = |id| Car { index: 5, name: "car".into(), rsf: Some(rsf(id)) };
        assert_eq!(shifter_args(&car(24), &root), ["--car-gearbox", "dog"]);
        assert!(shifter_args(&car(25), &root).is_empty());
        std::fs::write(dir.join("Shifter_Error.wav.sample"), []).unwrap();
        let grind = dir.join("Shifter_Error.wav.sample").display().to_string();
        assert_eq!(shifter_args(&car(25), &root), ["--grind-sound".to_owned(), grind]);
        assert!(shifter_args(&car(24), Path::new("")).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The Time of day reaches a quick race's viewer and start board, and not a championship stage (`launch_args`,
    /// as `start_rally_stage` calls it), which keeps the stage's own light and the season's start clock.
    #[test]
    fn the_time_of_day_is_the_quick_races_only() {
        let env = |key| std::env::var(key).ok().filter(|v| !v.is_empty());
        let (Some(data), Some(patch), Some(_)) = (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_RSF")) else {
            return;
        };
        let here = std::env::temp_dir().join(format!("rbr-launcher-clock-{}", std::process::id()));
        std::fs::create_dir_all(&here).unwrap();
        let text = "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nstart_time = 20:30\n";
        let start = Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: here.join(SETTINGS_FILE),
            data: Some(PathBuf::from(&data)),
            patch: Some(PathBuf::from(&patch)),
        };
        let mut form = load(&start, Settings::parse(text)).unwrap();
        form.read_only = true;
        let after = |args: &[String], flag: &str| args.iter().position(|a| a == flag).map(|i| args[i + 1].clone());
        let quick = form.start_args(&mut CrtRand::srand(1234)).unwrap();
        assert_eq!(after(&quick, "--clock").as_deref(), Some("20:30"));
        assert_eq!(after(&quick, "--start-time").as_deref(), Some("20:30"));
        let (stage, weather) = form.selection().unwrap();
        let stage_args = vec!["--championship".to_owned(), "rookie".to_owned()];
        let rally = form.launch_args(stage, form.options, weather, stage_args, Vec::new()).unwrap();
        assert!(!rally.iter().any(|a| a == "--clock" || a == "--start-time"), "{rally:?}");
        std::fs::remove_dir_all(&here).unwrap();
    }

    #[test]
    fn the_viewer_command_follows_the_saved_settings() {
        let env = |key| std::env::var(key).ok().filter(|v| !v.is_empty());
        let (Some(data), Some(patch), Some(ngp), Some(rsf)) =
            (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS"), env("RBR_RSF"))
        else {
            return;
        };
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/testdata/commands.txt");
        let mut cases = Vec::new();
        for (n, (name, text)) in COMMAND_CASES.into_iter().enumerate() {
            let text = text.replace("{ngp}", &ngp).replace("{rsf}", &rsf);
            let here = std::env::temp_dir().join(format!("rbr-launcher-command-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&here);
            std::fs::create_dir_all(&here).unwrap();
            let settings = here.join(SETTINGS_FILE);
            std::fs::write(&settings, &text).unwrap();
            let start = Start {
                viewer: PathBuf::from("/bin/viewer"),
                standalone: PathBuf::from("/bin/rbr-standalone"),
                settings: settings.clone(),
                data: Some(PathBuf::from(&data)),
                patch: Some(PathBuf::from(&patch)),
            };
            let mut form = load(&start, Settings::parse(&text)).unwrap();
            form.read_only = true;
            let mut rand = CrtRand::srand(1234);
            let args = form.start_args(&mut rand).unwrap_or_else(|| panic!("{name}: {}", form.status));
            let env: Vec<String> = form.child_env().iter().map(|(k, v)| format!("{k}={}", v.display())).collect();
            let lines = env.into_iter().chain(args);
            let mut case: String = lines.map(|line| line + "\n").collect();
            case = case.replace(&here.display().to_string(), "<here>");
            for (path, tag) in [(&data, "<data>"), (&patch, "<patch>"), (&ngp, "<ngp>"), (&rsf, "<rsf>")] {
                case = case.replace(path.as_str(), tag);
            }
            cases.push((name, case));
            let _ = std::fs::remove_dir_all(&here);
        }
        if std::env::var_os("RBR_LAUNCHER_BLESS").is_some() {
            let text: String = cases.iter().map(|(name, case)| format!("== {name}\n{case}")).collect();
            std::fs::write(&golden, text).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(&golden).unwrap();
        let expected: Vec<(&str, &str)> =
            expected.split("== ").skip(1).filter_map(|block| block.split_once('\n')).collect();
        for (name, case) in &cases {
            let found = expected.iter().find(|(n, _)| n == name).map(|(_, c)| *c);
            assert_eq!(found, Some(case.as_str()), "{name}");
        }
    }

    /// Two championship stages and the service between them, live, through the launcher's turns: each stage starts
    /// as Drive SS<n> starts it (`start_rally_stage`), runs in the viewer to its end, and the page reads its result
    /// when the viewer exits; the park between closes with the manager's advice. `RBR_E2E_VIEWER` is the viewer to
    /// run (a wrapper adding `--quit-after` and `rbr-standalone --pilot autopilot` drives it unattended),
    /// `RBR_E2E_STANDALONE` the stepping program; `RBR_DATA` and `RBR_PATCH` the game, `RBR_NGP_PHYSICS` the physics
    /// and `RBR_RSF` the cars. `RBR_E2E_TEAM` is the team (default the first offered): its car when NGP drives it,
    /// else the Drive page's (as picked; `RBR_E2E_CAR` names it). Takes as long as the stages.
    #[test]
    #[ignore]
    fn a_live_two_stage_championship_runs_through_the_turns() {
        let var = std::env::var_os;
        let (Some(viewer), Some(standalone), Some(install), Some(rsf), Some(ngp)) =
            (var("RBR_E2E_VIEWER"), var("RBR_E2E_STANDALONE"), var("RBR_DATA"), var("RBR_RSF"), var("RBR_NGP_PHYSICS"))
        else {
            return;
        };
        let here = std::env::temp_dir().join(format!("rbr-e2e-championship-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&here);
        std::fs::create_dir_all(&here).unwrap();
        let paths = Paths {
            data: PathBuf::from(&install),
            patch: std::env::var_os("RBR_PATCH").map(PathBuf::from),
            viewer: PathBuf::from(viewer),
            standalone: PathBuf::from(standalone),
            settings: here.join(SETTINGS_FILE),
        };
        let data = Data::new(&paths.data, paths.patch.clone());
        let season = Season::load(&data).unwrap();
        let mut catalog = Catalog::scan(&paths.data).unwrap();
        catalog.set_rsf_cars(&paths.data, &rsf.to_string_lossy(), Path::new(""));
        catalog.offer(&crate::catalog::ngp_cars(Path::new(&ngp)));
        let car = std::env::var("RBR_E2E_CAR").ok();
        let settings = Settings { car, ngp_physics: Some(ngp.to_string_lossy().into()), ..Settings::default() };
        let mut form = Form::new(catalog, &settings, paths, data, season, install::Folder::default());
        form.volume = 0.02;
        if let Some(team) = std::env::var("RBR_E2E_TEAM").ok().and_then(|t| t.parse().ok()) {
            form.rally.team = team;
        }
        form.car = form.team_car(form.rally.team).unwrap_or(form.car);
        let store = form.rally.store(&here);
        form.rally.new_season(&form.season, Some(store), 1234);
        form.rally.start_rally();
        let car = form.car().unwrap().index;
        let stages =
            form.rally.championship.as_ref().unwrap().rally().stages.iter().map(|s| s.track()).collect::<Vec<_>>();
        eprintln!("car {}, stages {stages:?}", form.car().unwrap().name);
        for stage in 0..2 {
            assert_eq!(form.rally.run.as_ref().map(|r| r.turn), Some(Turn::Stage(stage)));
            let start = form.rally.stage_start(&here, stage as usize).unwrap();
            form.start_rally_stage(start);
            let status = form.child.take().expect("the viewer started").wait().unwrap();
            assert!(status.success(), "viewer: {status}");
            form.rally.stage_closed(&here);
            let c = form.rally.championship.as_ref().unwrap();
            let time = c.times().get(stage as usize).map(|t| t.time);
            eprintln!("SS{}: {:?}, phase {:?}", stage + 1, time, c.phase());
            assert!(time.is_some_and(|t| t > 0.0), "SS{} has no time", stage + 1);
            if let Some(Turn::Service { .. }) = form.rally.run.as_ref().map(|r| r.turn) {
                form.rally.open_park(&form.data, car, Some(Path::new(&ngp)));
                let park = form.rally.park.as_mut().unwrap();
                park.manager_advice();
                if !park.start() {
                    park.forfeit();
                }
                park.finish_now();
                let ParkState::Closed { penalty, .. } = park.state else { panic!("{:?}", park.state) };
                let damage = park.damage.clone();
                eprintln!("service: penalty {penalty} s");
                form.rally.service_done(penalty, damage);
            }
        }
        let save = &form.rally.run.as_ref().unwrap().save;
        let r = rally_page::player_rally(save, 0).unwrap();
        eprintln!("saved: SS1 {} SS2 {} penalty {}", r.stages[0].time, r.stages[1].time, r.f170);
        assert!(r.stages[0].time > 0.0 && r.stages[1].time > 0.0);
        // The profile file on disk holds the same season, as Continue loads it.
        let (loaded, _) = form.rally.store(&here).load().unwrap().expect("a saved season");
        assert_eq!(&loaded, save, "the saved season reads back");
        let _ = std::fs::remove_dir_all(&here);
    }
}
