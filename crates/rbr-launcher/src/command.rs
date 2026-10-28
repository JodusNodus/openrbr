//! The viewer command Start runs: the selection resolved to a stage and weather, the viewer's and
//! `rbr-standalone`'s arguments, and the viewer as a child process.

use std::path::{Path, PathBuf};
use std::process::Command;

use rbr_formats::rsf::{codriver, pacenotes, stage_list};
use rbr_formats::saves::SaveDir;
use rbr_gameplay::crt::CrtRand;
use rbr_gameplay::markers::MarkerSettings;
use rbr_gameplay::race_options::RaceOptions;
use rbr_gameplay::weather::{self, Draws, Preset, Weather};
use rbr_render::glass::model::Switch;
use rbr_render::renderer::{AntiAliasing, Detail, Enhance, Reflections};
use rbr_render::shifter::CarBox;
use rbr_sound::mixer::Volumes;

use crate::catalog::{self, Car, Stage, TyreChoice};
use crate::graphics_page::{Graphics, HudChoice};
use crate::settings::{clock_text, wetness_name, Clouds, Fog, Precipitation};
use crate::{rally_page, Form};

/// The viewer's `--clock` arguments for a start time, none for the stage's own light.
pub fn clock_args(start: Option<u32>) -> Vec<String> {
    start.map_or_else(Vec::new, |minutes| vec!["--clock".into(), clock_text(minutes)])
}

/// The Environment's extra arguments: the viewer's `--sky` (the row's exact `TrackSettings.ini` section) and
/// `--fog`, `--clouds` and `--precipitation` (ours, still editable with a ghost), and `rbr-standalone`'s `--wetness` (the override; a ghost keeps its
/// own wetness). The viewer's `--weather` word and the auto tyre already follow the override through the weather.
///
/// The first list goes to the viewer, the second to `rbr-standalone`.
pub fn environment_args(
    section: Option<String>,
    fog: Option<Fog>,
    clouds: Option<Clouds>,
    precipitation: Option<Precipitation>,
    wetness: Option<u32>,
    ghosted: bool,
) -> (Vec<String>, Vec<String>) {
    let mut viewer = Vec::new();
    if let Some(section) = section {
        viewer.extend(["--sky".to_owned(), section]);
    }
    if let Some(fog) = fog {
        viewer.extend(["--fog".to_owned(), fog.name().to_owned()]);
    }
    if let Some(clouds) = clouds {
        viewer.extend(["--clouds".to_owned(), clouds.name().to_owned()]);
    }
    if let Some(precipitation) = precipitation {
        viewer.extend(["--precipitation".to_owned(), precipitation.name().to_owned()]);
    }
    let mut standalone = Vec::new();
    if !ghosted {
        if let Some(wetness) = wetness {
            standalone.extend(["--wetness".to_owned(), wetness_name(wetness).to_owned()]);
        }
    }
    (viewer, standalone)
}

/// The viewer's `--perf-log` into `dir`, when the form has the performance log on.
fn perf_log_args(on: bool, dir: &Path) -> Vec<String> {
    if on {
        vec!["--perf-log".to_owned(), dir.display().to_string()]
    } else {
        Vec::new()
    }
}

/// The viewer's `--render-scale` arguments for `percent`, none at full size.
pub fn render_scale_args(percent: u32) -> Vec<String> {
    match percent {
        100 => Vec::new(),
        percent => vec!["--render-scale".into(), (f64::from(percent) / 100.0).to_string()],
    }
}

/// The `viewer --enhance` list of the additions on, or `None` with none on.
pub fn enhance_list(graphics: &Graphics) -> Option<String> {
    let on: Vec<&str> = Enhance::NAMES.iter().zip(graphics).filter(|(_, on)| **on).map(|(name, _)| *name).collect();
    (!on.is_empty()).then(|| on.join(","))
}

/// The RSF install's Pacenote plugin: its folder and the voices, styles and range sets it offers.
#[derive(Debug, Clone, PartialEq)]
pub struct CodriverMods {
    pub dir: PathBuf,
    pub voices: Vec<String>,
    pub styles: Vec<String>,
    pub ranges: Vec<String>,
    /// A locally owned pack's default voice (`PaceNote.ini`'s, else its first discovered voice): what an unset
    /// choice plays, so packs without a `PaceNote.ini` still speak. None for the RSF install, whose unset choice
    /// stays `PaceNote.ini`'s.
    pub default_voice: Option<String>,
}

/// Whether the installed voice `have` is the configured one `wanted`: case-insensitively, with `\` and `/`
/// meaning the same folder, so a stale-cased `PaceNote.ini` still resolves to the installed spelling.
fn voices_equal(have: &str, wanted: &str) -> bool {
    let normal = |v: &str| v.replace('\\', "/").to_ascii_lowercase();
    normal(have) == normal(wanted)
}

impl CodriverMods {
    /// The plugin of the RSF install `rsf_root`, or `None` without one.
    pub fn scan(rsf_root: &str) -> Option<Self> {
        let dir = codriver::plugin_dir(Path::new(rsf_root.trim()))?;
        let (voices, styles, ranges) = (codriver::voices(&dir), codriver::styles(&dir), codriver::range_sets(&dir));
        Some(Self { dir, voices, styles, ranges, default_voice: None })
    }

    /// The plugin in the folder `dir` itself (a locally owned pack): its voices, styles and range sets. The
    /// default voice is the installed voice matching `PaceNote.ini`'s (case-insensitively, `\` or `/`), else
    /// the first installed voice: a stale or foreign setting never names a voice that is not there.
    pub fn from_dir(dir: PathBuf) -> Option<Self> {
        let (voices, styles, ranges) = (codriver::voices(&dir), codriver::styles(&dir), codriver::range_sets(&dir));
        let configured = codriver::settings(&dir).voice;
        let default_voice = match configured {
            Some(wanted) => {
                voices.iter().find(|v| voices_equal(v, &wanted)).cloned().or_else(|| voices.first().cloned())
            }
            None => voices.first().cloned(),
        };
        Some(Self { dir, voices, styles, ranges, default_voice })
    }

    /// The voice `choice` names: the plugin's default when unset, none for the original 1.02 co-driver
    /// ([`ORIGINAL_VOICE`]).
    pub fn voice(&self, choice: Option<&str>) -> Option<String> {
        match choice {
            Some(ORIGINAL_VOICE) => None,
            Some(voice) => Some(voice.to_owned()),
            None => self.default_voice.clone().or_else(|| codriver::settings(&self.dir).voice),
        }
    }
}

/// The voice choice for the original 1.02 co-driver.
pub const ORIGINAL_VOICE: &str = "1.02";

/// The viewer's co-driver mod arguments without the stage notes: the plugin with the voice, style and range set.
pub fn codriver_base_args(mods: &CodriverMods, voice: Option<&str>, style: &str, ranges: &str) -> Vec<String> {
    let Some(voice) = mods.voice(voice) else { return Vec::new() };
    ["--codriver-mod", &mods.dir.display().to_string()]
        .into_iter()
        .chain(["--codriver-voice", &voice, "--codriver-style", style, "--codriver-ranges", ranges])
        .map(str::to_owned)
        .collect()
}

/// The viewer's co-driver mod arguments: the plugin with the voice, style and range set, and the stage's MyPacenotes
/// file when `my_pacenotes` is on and there is one for the stage `map` (RSF's name for it, and the community stage's).
pub fn codriver_args(
    mods: &CodriverMods,
    voice: Option<&str>,
    style: &str,
    ranges: &str,
    rsf_root: &Path,
    my_pacenotes: bool,
    stage_names: &[&str],
) -> Vec<String> {
    let mut args = codriver_base_args(mods, voice, style, ranges);
    if args.is_empty() {
        return args;
    }
    let notes = my_pacenotes
        .then(|| pacenotes::my_pacenotes_dir(rsf_root))
        .flatten()
        .and_then(|dir| pacenotes::my_pacenotes(&dir, stage_names));
    if let Some(file) = notes {
        args.extend(["--pacenotes".into(), file.display().to_string()]);
    }
    args
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
    volumes: Volumes,
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
        "--volumes".into(),
        format!("{},{},{},{}", volumes.engine, volumes.codriver, volumes.effects, volumes.tyres),
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
    if !hud.dials {
        args.push("--no-dials".into());
    }
    if hud.hide_wheel {
        args.push("--hide-wheel".into());
    }
    if let Some(list) = hud.parts.hidden() {
        args.extend(["--hud-hide".into(), list]);
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

/// The viewer's arguments for the advanced H-shifter: the car's gearbox (`--car-gearbox`) and the grind from
/// RBRControls' folder in the RSF install `rsf_root`, `Shifter_Error.wav` (or the `.wav.sample` RBRControls ships it
/// as; `--grind-sound`).
pub fn shifter_args(gearbox: CarBox, rsf_root: &Path) -> Vec<String> {
    let mut args = vec!["--car-gearbox".into(), gearbox.arg().into()];
    if rsf_root.as_os_str().is_empty() {
        return args;
    }
    let dir = rsf_root.join("Plugins").join("RBRControls");
    let grind =
        ["Shifter_Error.wav", "Shifter_Error.wav.sample"].map(|n| dir.join(n)).into_iter().find(|p| p.is_file());
    if let Some(wav) = grind {
        args.extend(["--grind-sound".into(), wav.display().to_string()]);
    }
    args
}

impl Form {
    /// Resolves the current preset into its stage variant and weather row: the random weather's from the next draw of
    /// the stream (`rand`), without taking it.
    pub fn selection(&self) -> Result<(catalog::Stage, Weather), String> {
        self.selection_from(&mut self.rand.clone()).map(|(stage, weather, _)| (stage, weather))
    }

    /// Resolves the Environment choices into the stage, the weather and the preset the launch runs as: a specific
    /// row runs like Random with that row (the chosen wear set after), and the Custom weather's wetness replaces its
    /// base's. A saved ghost brings its own weather, and the launch keeps the user's preset.
    fn selection_from(&self, rand: &mut CrtRand) -> Result<(catalog::Stage, Weather, Preset), String> {
        let map = self.catalog.maps[self.map].number;
        if let Some((stage, weather)) = self.ghost_weather() {
            return Ok((stage, weather, self.options.weather));
        }
        let season = self.catalog.maps[self.map].season(&self.season);
        let row = self.weather_row.as_deref().and_then(|s| catalog::row_by_section(&season, map, s));
        let (preset, draws, specific) = match row.filter(|_| self.clouds.is_none()) {
            // The Custom weather: our sky over the stage's good weather, or its bad one on a damp or wet road
            // (its light and textures made for the wet), with the chosen wear.
            None if self.clouds.is_some() => (self.custom_base(), Draws { row: None, wear: self.wear }, true),
            Some(k) => (Preset::Random, Draws { row: Some(k), wear: 1 }, true),
            None if self.options.weather == Preset::Random => (
                Preset::Random,
                weather::random_draws(&season, map, rand)
                    .ok_or(format!("map {map} has no season row (the original stops at its int3)"))?,
                false,
            ),
            None => (self.options.weather, Draws { row: None, wear: self.wear }, false),
        };
        let (stage, mut weather) = self.catalog.stage_weather(&self.season, map, preset, draws)?;
        if specific {
            // A specific row takes the chosen wear (`Draws` takes the random weather's 1 or 2).
            weather.wear = self.wear;
        }
        if self.custom() {
            if let Some(wetness) = self.wetness {
                weather.wetness = wetness.min(2);
            }
        }
        Ok((stage, weather, preset))
    }

    /// Whether the weather is Custom (a cloud cover chosen): ours over the stage, its fog, rain, wetness and time of
    /// day too; else the game's variant as authored, at the hour it was baked at.
    pub(crate) fn custom(&self) -> bool {
        self.clouds.is_some()
    }

    /// The Custom weather's base: the stage's good weather on a dry road, its bad weather on a damp or wet one.
    pub(crate) fn custom_base(&self) -> Preset {
        if self.wetness.unwrap_or(0) >= 1 {
            Preset::Bad
        } else {
            Preset::Good
        }
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

    /// The viewer's arguments for the quick race Start runs, drawing the random weather from `rand`, with the
    /// Environment's sky, fog and wetness.
    pub fn start_args(&mut self, rand: &mut CrtRand) -> Option<Vec<String>> {
        let (stage, weather, preset) = self.selection_from(rand).ok()?;
        // Only the quick race races and keeps a ghost (`rbr_gameplay::ghost::clears`: the championship clears it).
        let words = weather.ghost_words(&self.season);
        let ghost = self.ghost.viewer_args(&self.records(), stage.map, &words);
        // The quick race keeps the stage record and the personal best in the game's save files of the player's folder.
        let saves = ["--saves".to_owned(), SaveDir::new(&self.records()).path.display().to_string()];
        let mut extra = [saves.as_slice(), &["--profile".to_owned(), self.rally.profile.clone()]].concat();
        // The Custom weather's Time of day is the quick race's: the stage is drawn at it and the start board shows
        // it. The Stage weather and a championship stage keep the stage's own light and the season's start clock.
        let custom = self.custom();
        let start_time = self.start_time.filter(|_| custom);
        if let Some(m) = start_time {
            extra.extend(["--start-time".to_owned(), format!("{}:{:02}", m / 60, m % 60)]);
        }
        let viewer_extra = [ghost, clock_args(start_time)].concat();
        // The Environment: the exact sky of the row or the Custom weather's base, and the Custom weather's fog, clouds,
        // rain and wetness (a ghost keeps its own wetness).
        let (sky_fog, wetness_args) = environment_args(
            weather.section(),
            self.fog.filter(|_| custom),
            self.clouds,
            self.precipitation.filter(|_| custom),
            self.wetness.filter(|_| custom),
            self.ghost_weather().is_some(),
        );
        let viewer_extra = [viewer_extra, sky_fog].concat();
        extra.extend(wetness_args);
        let options = RaceOptions { weather: preset, ..self.options };
        self.launch_args(stage, options, weather, extra, viewer_extra)
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
                self.validate_weather_row();
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
        if let Some(problem) = self.ngp_dll.problem(&self.data, &self.rsf()) {
            self.status = problem;
            return None;
        }
        let tyre = self.tyre_id(&weather).ok()?;
        let Some(car) = self.car().cloned() else {
            self.status = "no car: the Settings page's RallySimFans folder has none".into();
            return None;
        };
        let car = &car;
        if self.bindings.shifter.advanced {
            let gearbox = self.gearboxes.of(&car.name);
            // Clutch help ignores the clutch pedal the advanced H-shifter needs; the lever, not the game, picks the
            // gear. A sequential car keeps them: the viewer leaves its shifter off.
            if !gearbox.sequential {
                (options.aids.clutch_help, options.aids.auto_gears) = (false, false);
            }
            viewer_extra.extend(shifter_args(gearbox, Path::new(self.rsf_root.trim())));
        }
        if let Some(mods) = self.codriver_mods.as_ref().filter(|_| self.codriver) {
            // The stage's names for the note lookup: RSF's aliases where there are any, then the launcher's own
            // catalogue names, so the lookup works without an RSF stage list.
            let names = self.stage_note_names(&stage);
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            match self.notes_pack.as_deref() {
                // The RSF install's MyPacenotes, as before.
                None => {
                    let rsf_root = self.rsf();
                    viewer_extra.extend(codriver_args(
                        mods,
                        self.codriver_voice.as_deref(),
                        &self.codriver_style,
                        &self.codriver_ranges,
                        Path::new(rsf_root.trim()),
                        self.my_pacenotes,
                        &refs,
                    ));
                }
                // A locally owned note pack: searched by the launcher's own stage names.
                Some(_) => {
                    let mut args = codriver_base_args(
                        mods,
                        self.codriver_voice.as_deref(),
                        &self.codriver_style,
                        &self.codriver_ranges,
                    );
                    if !args.is_empty() {
                        if let Some(file) = self.resolve_notes(&refs) {
                            args.extend(["--pacenotes".into(), file.display().to_string()]);
                        }
                        viewer_extra.extend(args);
                    }
                }
            }
        }
        self.select_setup();
        let setup = match self.setup.current_file(&self.here()) {
            Ok(setup) => setup,
            Err(e) => {
                self.status = format!("setup: {e}");
                return None;
            }
        };
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
            Volumes {
                engine: self.engine_volume,
                codriver: self.codriver_volume,
                effects: self.effects_volume,
                tyres: self.tyres_volume,
            },
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
        // The viewer's `--live` command is the rest of the line.
        let live = args.iter().position(|a| a == "--live").unwrap_or(args.len());
        let scale = render_scale_args(self.render_scale);
        let anti_aliasing = (self.anti_aliasing != AntiAliasing::default())
            .then(|| ["--anti-aliasing".to_owned(), self.anti_aliasing.name().to_owned()])
            .into_iter()
            .flatten();
        let reflections = (self.reflections != Reflections::default())
            .then(|| ["--reflections".to_owned(), self.reflections.name().to_owned()])
            .into_iter()
            .flatten();
        let detail = (self.detail != Detail::default())
            .then(|| ["--detail".to_owned(), self.detail.name().to_owned()])
            .into_iter()
            .flatten();
        let present = ["--present".to_owned(), self.present.name().to_owned()];
        let mirrors = ["--mirrors".to_owned(), self.mirrors.name()];
        let fullscreen = self.fullscreen.then(|| "--fullscreen".to_owned());
        let perf_log = perf_log_args(self.perf_log, &self.logs_dir());
        let upscale = (self.upscale_textures && self.upscaler)
            .then(|| ["--upscale-textures".to_owned(), self.upscaler_dir().display().to_string()]);
        // The loading screen names the stage and the car.
        let map =
            self.catalog.maps.iter().find(|m| m.number == stage.map).map_or(stage.name.clone(), catalog::Map::name);
        let title = ["--title".to_owned(), format!("{map} · {}", car.name)];
        // The car's lock, the wheel's rotation when it is per car.
        let lock = self.setup.steering_lock().map(rbr_render::wheel_rotation::car_degrees);
        let car_lock = lock.into_iter().flat_map(|d| ["--car-lock".to_owned(), d.to_string()]);
        let viewer_extra = viewer_extra
            .into_iter()
            .chain(scale)
            .chain(anti_aliasing)
            .chain(reflections)
            .chain(detail)
            .chain(present)
            .chain(mirrors)
            .chain(fullscreen)
            .chain(perf_log)
            .chain(upscale.into_iter().flatten())
            .chain(title)
            .chain(car_lock);
        args.splice(live..live, viewer_extra);
        args.extend(extra);
        args.extend(["--ngp-physics".to_owned(), self.ngp_archive()]);
        let sensitivity = self.ffb.sensitivity_for(car.rsf.as_ref().map(|r| r.id));
        if sensitivity != [100; 3] {
            let [gravel, tarmac, snow] = sensitivity;
            args.extend(["--ffb-sensitivity".to_owned(), format!("{gravel},{tarmac},{snow}")]);
        }
        if self.head_gasket > 1 {
            args.extend(["--head-gasket-durability".to_owned(), self.head_gasket.to_string()]);
        }
        Some(args)
    }

    /// The stage's names for the stage-note lookup: RSF's aliases where there are any (its stage list's name and
    /// a community stage's own), then the launcher's own catalogue names (the map's name, the game's title and the
    /// stage variant), so the lookup works without an RSF stage list.
    pub fn stage_note_names(&self, stage: &catalog::Stage) -> Vec<String> {
        let rsf_root = self.rsf();
        let listed = stage_list(Path::new(rsf_root.trim())).remove(&stage.map).and_then(|mut e| e.remove("name"));
        let map = self.catalog.maps.iter().find(|m| m.number == stage.map);
        listed
            .into_iter()
            .chain(map.and_then(|m| m.rsf.as_ref()).map(|s| s.name.clone()))
            .chain(map.map(catalog::Map::name))
            .chain(catalog::title(stage.map).map(str::to_owned))
            .chain([stage.name.clone()])
            .collect::<Vec<_>>()
            .into_iter()
            .fold(Vec::new(), |mut seen: Vec<String>, name| {
                if !name.is_empty() && !seen.contains(&name) {
                    seen.push(name);
                }
                seen
            })
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
    use crate::graphics_page::{graphics_index, DEFAULT_GRAPHICS, GRAPHICS_LABELS};
    use crate::settings::{load, Settings, SEQUENTIAL};
    use crate::{install, Paths, Start, SETTINGS_FILE};
    use rbr_formats::data::Data;
    use rbr_gameplay::race_options::{DamageLevel, DriverAids};
    use rbr_gameplay::season_events::Turn;
    use rbr_gameplay::service::ParkState;
    use rbr_gameplay::weather::Season;
    use rbr_render::hud::Parts;
    use rbr_render::shifter::Engagement;
    use std::path::PathBuf;

    /// Saved settings and the viewer command Start runs for them (`testdata/commands.txt`): the pages may move,
    /// the command stays. Needs `RBR_DATA`, `RBR_PATCH`, `RBR_NGP_PHYSICS` and `RBR_RSF` (unsaved, the last two are
    /// the defaults); `{ngp}` and `{rsf}` are theirs. `RBR_LAUNCHER_BLESS=1` writes the file.
    const COMMAND_CASES: [(&str, &str); 7] = [
        ("defaults", ""),
        (
            "everything changed",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nweather = bad\nwear = worn\ntyre = 5\n\
             size = 1920x1080\nfov = 72.5\nvolume = 0.75\ncodriver = off\nhud = off\nunits = mph\nfps = on\n\
             pace_stack = 2\npace_3d = on\nwipers = intermittent\ncallout_distance = 0.5\nmarker_threshold = 12.5\n\
             damage = realistic\nauto_gears = on\ngear_guard = off\nneutral_lock = on\nclutch_help = off\n\
             ghost = best\nrender_scale = 75\nanti_aliasing = off\nreflections = off\npresent = immediate\nfullscreen = off\nstart_time = 16:30\n",
        ),
        (
            "random weather, advanced shifter",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 71\nweather = random\nsize = 1280x720\n\
             [controls]\nh-shifter = advanced\n",
        ),
        ("NGP", "[launcher]\ncar = COROLLA\nmap = 33\nngp_physics = {ngp}\nweather = good\n"),
        (
            "RSF car on a retail stage",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nngp_physics = {ngp}\nrsf_root = {rsf}\n\
             ghost = best\n",
        ),
        (
            "RSF car and stage",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 129\nngp_physics = {ngp}\nrsf_root = {rsf}\n\
             ghost = best\n",
        ),
        (
            "force feedback sensitivity",
            "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nhead_gasket = 10\n[ffb]\n\
             sensitivity_gravel = 70\nsensitivity_snow = 130\nsensitivity_car24 = 90,100,110\n",
        ),
    ];

    /// An RSF car goes to the viewer and to `rbr-standalone` as its pack, in its slot.
    #[test]
    fn viewer_args_name_an_rsf_cars_pack() {
        let stage = Stage { name: "track-54_E".into(), map: 54, letter: 'E' };
        let rsf = RsfCar {
            id: 24,
            added: false,
            group: None,
            pack: Ok("/rsf/rsfdata/cars/evo".into()),
            model: Ok(None),
            drive_train: None,
        };
        let car = Car { index: 5, name: "Mitsubishi Lancer Evo IX N4".into(), rsf: Some(rsf) };
        assert_eq!(car.unavailable(), None);
        let weather =
            Weather { map: 54, row: Some(162), letter: 4, cloud: 1, cloud_letter: 0, preset: 0, wetness: 1, wear: 0 };
        let args = viewer_args(
            &stage,
            &car,
            &RaceOptions::default(),
            &weather,
            4,
            (1280, 720),
            65.0,
            1.0,
            Volumes::default(),
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
        let weather =
            Weather { map: 54, row: Some(167), letter: 3, cloud: 3, cloud_letter: 2, preset: 4, wetness: 2, wear: 2 };
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
            Volumes { engine: 0.5, codriver: 1.0, effects: 1.5, tyres: 2.0 },
            false,
            HudChoice {
                show: false,
                mph: true,
                fps: true,
                stack: 3,
                dials: false,
                hide_wheel: true,
                parts: Parts::hiding("inputs,view").unwrap(),
            },
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
            "track-54_O --car COROLLA --weather wet --size 1280x720 --fov 65 --volume 0.5 --volumes 0.5,1,1.5,2 --no-codriver \
             --no-hud --mph --wipers continuous --fps --pace-stack 3 --no-dials --hide-wheel --hud-hide inputs,view --bindings /l/launcher-settings.ini \
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
            Volumes::default(),
            true,
            HudChoice::default(),
            &["marks", "lighting"].iter().fold(Graphics::default(), |mut g, name| {
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
        assert!(args.contains("--enhance lighting,marks --live "), "{args}");
        assert!(
            args.ends_with(
                "--season-row 167 --damage realistic --auto-gears on --gear-guard off --neutral-lock on --clutch-help off"
            ),
            "{args}"
        );
    }

    /// The Environment's extra arguments: the row's exact sky, our fog, clouds and rain for the viewer, the wetness
    /// override for `rbr-standalone` (a ghost keeps its own wetness, while the fog, clouds and rain stay editable).
    #[test]
    fn environment_args_name_the_sky_fog_clouds_rain_and_wetness() {
        let (viewer, standalone) = environment_args(
            Some("54O_E_heavyfog_heavycloud".into()),
            Some(Fog::Heavy),
            Some(Clouds::Partly),
            Some(Precipitation::None),
            Some(2),
            false,
        );
        assert_eq!(
            viewer,
            ["--sky", "54O_E_heavyfog_heavycloud", "--fog", "heavy", "--clouds", "partly", "--precipitation", "none"]
        );
        assert_eq!(standalone, ["--wetness", "wet"]);
        let (viewer, standalone) = environment_args(None, None, None, None, None, false);
        assert!(viewer.is_empty() && standalone.is_empty());
        let (viewer, standalone) = environment_args(
            Some("54M_norain_partcloud".into()),
            Some(Fog::Off),
            Some(Clouds::Heavy),
            Some(Precipitation::Heavy),
            Some(0),
            true,
        );
        assert_eq!(
            viewer,
            ["--sky", "54M_norain_partcloud", "--fog", "off", "--clouds", "heavy", "--precipitation", "heavy"]
        );
        assert!(standalone.is_empty());
    }

    /// The wetness override changes the viewer's `--weather` word (it replaces the weather's wetness after
    /// selection; the auto tyre and `rbr-standalone --wetness` follow the same weather).
    #[test]
    fn the_wetness_override_changes_the_viewer_weather_word() {
        let stage = Stage { name: "track-54_O".into(), map: 54, letter: 'O' };
        let car = Car { index: 6, name: "COROLLA".into(), rsf: None };
        let weather = |wetness| Weather {
            map: 54,
            row: Some(7),
            letter: 3,
            cloud: 3,
            cloud_letter: 1,
            preset: 4,
            wetness,
            wear: 1,
        };
        let word = |wetness| {
            let args = viewer_args(
                &stage,
                &car,
                &RaceOptions::default(),
                &weather(wetness),
                3,
                (1280, 720),
                65.0,
                1.0,
                Volumes::default(),
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
            let at = args.iter().position(|a| a == "--weather").unwrap();
            args[at + 1].clone()
        };
        assert_eq!(word(0), "dry");
        assert_eq!(word(2), "wet");
    }

    /// A specific row resolves like Random with that row: the chosen wear is set after, the wetness override
    /// replaces the row's, and the launch runs as Random. A section that is not the map's falls back to Good at
    /// load. Needs no game data.
    #[test]
    fn a_specific_row_resolves_like_random_with_the_chosen_wear_and_override() {
        use crate::catalog::{Map, Stage};
        use crate::settings::WeatherChoice;
        use rbr_gameplay::weather::Row;
        let season = || Season {
            rows: vec![
                Row { map: 54, letter: 3, cloud: 3, cloud_letter: 1, preset: 4, wetness: 2 },
                Row { map: 54, letter: 3, cloud: 1, cloud_letter: 0, preset: 2, wetness: 1 },
                Row { map: 54, letter: 3, cloud: 3, cloud_letter: 1, preset: 4, wetness: 0 },
            ],
            nice: vec![0],
            bad: vec![1],
        };
        let catalog = || Catalog {
            cars: Vec::new(),
            all_cars: Vec::new(),
            maps: vec![Map {
                number: 54,
                stages: vec![Stage { name: "track-54_O".into(), map: 54, letter: 'O' }],
                rsf: None,
            }],
        };
        let section = catalog::environment_rows(&season(), 54)[1].1.clone();
        assert_eq!(section, "54O_M_norain_partcloud");
        let here = std::env::temp_dir().join(format!("rbr-launcher-environment-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&here);
        std::fs::create_dir_all(&here).unwrap();
        let paths = || Paths {
            data: here.join("data"),
            patch: None,
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: here.join(SETTINGS_FILE),
        };
        let settings = Settings {
            map: Some(54),
            weather: Some(WeatherChoice::Row(section.clone())),
            wetness: Some(2),
            fog: Some(Fog::Heavy),
            wear: Some(2),
            ..Settings::default()
        };
        let form = Form::new(
            catalog(),
            &settings,
            paths(),
            Data::new(here.join("data"), None),
            season(),
            install::Folder::default(),
        );
        assert_eq!(form.weather_row.as_deref(), Some(section.as_str()));
        assert_eq!(form.options.weather, Preset::Random);
        let (stage, weather, preset) = form.selection_from(&mut CrtRand::srand(1)).unwrap();
        assert_eq!((stage.name.as_str(), preset), ("track-54_O", Preset::Random));
        // The stage's weather keeps the row's wetness; the saved one is the Custom weather's.
        assert_eq!((weather.row, weather.wear, weather.wetness), (Some(1), 2, 1));
        assert_eq!(weather.section().as_deref(), Some(section.as_str()));
        assert_eq!(weather.mat_words(), ("worn", "damp"));
        // A section that is not the map's falls back to Good at load.
        let settings = Settings { weather: Some(WeatherChoice::Row("21O_E_heavyfog_heavycloud".into())), ..settings };
        let form = Form::new(
            catalog(),
            &settings,
            paths(),
            Data::new(here.join("data"), None),
            season(),
            install::Folder::default(),
        );
        assert_eq!(form.weather_row, None);
        assert_eq!(form.options.weather, Preset::Good);
        // The Custom weather (a cloud cover) leaves the row: the good weather on a dry road, the bad on a wet one,
        // both with the chosen wear.
        for (wetness, preset) in [(0, Preset::Good), (2, Preset::Bad)] {
            let custom = Settings {
                weather: Some(WeatherChoice::Row(section.clone())),
                wetness: Some(wetness),
                clouds: Some(Clouds::Light),
                precipitation: Some(Precipitation::Heavy),
                ..settings.clone()
            };
            let form = Form::new(
                catalog(),
                &custom,
                paths(),
                Data::new(here.join("data"), None),
                season(),
                install::Folder::default(),
            );
            let (_, weather, chosen) = form.selection_from(&mut CrtRand::srand(1)).unwrap();
            assert_eq!((chosen, weather.wear, weather.wetness), (preset, 2, wetness));
        }
        std::fs::remove_dir_all(&here).unwrap();
    }

    /// A specific row's start carries its sky, fog, wetness and season row, and the wetness override changes the
    /// viewer's weather word. Needs `RBR_DATA`, `RBR_PATCH`, `RBR_NGP_PHYSICS` and `RBR_RSF`.
    #[test]
    fn the_performance_log_goes_to_the_logs_folder_when_on() {
        let dir = Path::new("/l/logs");
        assert_eq!(perf_log_args(true, dir), ["--perf-log", "/l/logs"]);
        assert!(perf_log_args(false, dir).is_empty());
    }

    #[test]
    fn a_specific_row_starts_with_its_sky_and_the_overrides() {
        let env = |key| std::env::var(key).ok().filter(|v| !v.is_empty());
        let (Some(data), Some(patch), Some(_), Some(_)) =
            (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS"), env("RBR_RSF"))
        else {
            return;
        };
        let here = std::env::temp_dir().join(format!("rbr-launcher-row-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&here);
        std::fs::create_dir_all(&here).unwrap();
        let start = Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: here.join(SETTINGS_FILE),
            data: Some(PathBuf::from(&data)),
            patch: Some(PathBuf::from(&patch)),
        };
        let mut form = load(&start, Settings::parse("[launcher]\nmap = 54\n")).unwrap();
        form.read_only = true;
        form.map = form.catalog.maps.iter().position(|m| m.number == 54).unwrap();
        let rows = {
            let map = &form.catalog.maps[form.map];
            catalog::environment_rows(&map.season(&form.season), map.number)
        };
        assert!(rows.len() > 1);
        form.weather_row = Some(rows[1].1.clone());
        form.wetness = Some(2);
        form.fog = Some(Fog::Heavy);
        form.precipitation = Some(Precipitation::Heavy);
        form.start_time = Some(16 * 60 + 30);
        form.wear = 1;
        let split = |args: Vec<String>| {
            let live = args.iter().position(|a| a == "--live").unwrap();
            let standalone = args[live..].to_vec();
            (args[..live].to_vec(), standalone)
        };
        let pair = |args: &[String], flag: &str| args.iter().position(|a| a == flag).map(|at| args[at + 1].clone());
        // The stage's weather is the row as authored: the Custom fog, rain, wetness and time stay out.
        let (_, weather) = form.selection().unwrap();
        let (viewer, standalone) = split(form.start_args(&mut CrtRand::srand(1)).unwrap());
        assert_eq!(pair(&viewer, "--sky"), weather.section());
        for flag in ["--fog", "--clouds", "--precipitation", "--clock"] {
            assert_eq!(pair(&viewer, flag), None, "{flag}");
        }
        assert_eq!(weather.wetness, 1);
        assert_eq!(pair(&viewer, "--weather").as_deref(), Some("damp"));
        assert_eq!(pair(&standalone, "--wetness"), None);
        assert_eq!(pair(&standalone, "--wear").as_deref(), Some("normal"));
        assert_eq!(pair(&standalone, "--season-row"), weather.row.map(|row| row.to_string()));
        // The Custom weather takes them all over the bad weather (a wet road).
        form.clouds = Some(Clouds::Light);
        let (_, weather) = form.selection().unwrap();
        let (viewer, standalone) = split(form.start_args(&mut CrtRand::srand(1)).unwrap());
        assert_eq!(pair(&viewer, "--sky"), weather.section());
        assert_eq!(pair(&viewer, "--fog").as_deref(), Some("heavy"));
        assert_eq!(pair(&viewer, "--clouds").as_deref(), Some("light"));
        assert_eq!(pair(&viewer, "--precipitation").as_deref(), Some("heavy"));
        form.precipitation = Some(Precipitation::Moderate);
        let (viewer, _) = split(form.start_args(&mut CrtRand::srand(1)).unwrap());
        assert_eq!(pair(&viewer, "--precipitation").as_deref(), Some("moderate"));
        form.precipitation = Some(Precipitation::Heavy);
        assert_eq!(pair(&viewer, "--clock").as_deref(), Some("16:30"));
        assert_eq!(pair(&viewer, "--weather").as_deref(), Some("wet"));
        assert_eq!(pair(&standalone, "--wetness").as_deref(), Some("wet"));
        assert_eq!(pair(&standalone, "--weather").as_deref(), Some("2"));
        assert_eq!(pair(&standalone, "--wear").as_deref(), Some("normal"));
        let lock = viewer.iter().position(|a| a == "--car-lock").map(|at| viewer[at + 1].clone());
        assert_eq!(lock, form.setup.steering_lock().map(|l| rbr_render::wheel_rotation::car_degrees(l).to_string()));
        std::fs::remove_dir_all(&here).unwrap();
    }

    #[test]
    fn codriver_args_name_the_mod_voice_and_the_stage_notes() {
        let tmp = std::env::temp_dir().join(format!("rbr-launcher-codriver-{}", std::process::id()));
        let notes = tmp.join("Plugins").join("NGPCarMenu").join("MyPacenotes").join("Tanner");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("tanner.ini"), "").unwrap();
        let plugin = tmp.join("Plugins/Pacenote");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n").unwrap();
        let mods = CodriverMods::scan(&tmp.display().to_string()).unwrap();
        let args = |voice, on| codriver_args(&mods, voice, "Rbr.ini", "Rbr.ini", &tmp, on, &["Tanner"]).join(" ");
        let head = format!("--codriver-mod {} --codriver-voice", mods.dir.display());
        let tail = "--codriver-style Rbr.ini --codriver-ranges Rbr.ini";
        let found = format!("--pacenotes {}", notes.join("tanner.ini").display());
        assert_eq!(args(None, true), format!("{head} english/steve {tail} {found}"));
        assert_eq!(args(Some("Janne"), false), format!("{head} Janne {tail}"));
        assert_eq!(args(Some(ORIGINAL_VOICE), true), "");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn the_enhance_list_names_what_the_viewer_takes() {
        assert_eq!(enhance_list(&Graphics::default()), None);
        let all = enhance_list(&[true; Enhance::NAMES.len()]).unwrap();
        assert_eq!(all, "anisotropy,asphalt,cabin,contact,decals,gravel,lighting,marks,particles,snow,water,wet");
        assert_eq!(Enhance::parse(&all), Enhance::parse("all"));
        let defaults = enhance_list(&DEFAULT_GRAPHICS).unwrap();
        assert_eq!(defaults, "anisotropy,asphalt,cabin,contact,decals,gravel,lighting,particles,snow,water,wet");
        for name in ["asphalt", "gravel", "snow"] {
            assert!(GRAPHICS_LABELS[graphics_index(name)].0.starts_with(&format!("Enhanced {name}")));
        }
    }

    /// The advanced H-shifter takes the car's gearbox and the grind from RBRControls' folder.
    #[test]
    fn shifter_args_read_the_grind() {
        let root = std::env::temp_dir().join(format!("rbr-launcher-shifter-{}", std::process::id()));
        let dir = root.join("Plugins").join("RBRControls");
        std::fs::create_dir_all(&dir).unwrap();
        let dog = CarBox { engagement: Engagement::Dog, sequential: false };
        assert_eq!(shifter_args(dog, &root), ["--car-gearbox", "dog"]);
        assert_eq!(shifter_args(SEQUENTIAL, Path::new("")), ["--car-gearbox", "sequential"]);
        std::fs::write(dir.join("Shifter_Error.wav.sample"), []).unwrap();
        let grind = dir.join("Shifter_Error.wav.sample").display().to_string();
        assert_eq!(shifter_args(dog, &root), ["--car-gearbox".to_owned(), "dog".into(), "--grind-sound".into(), grind]);
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
        let text = "[launcher]\ncar = Mitsubishi Lancer Evo IX N4\nmap = 54\nclouds = partly\nstart_time = 20:30\n";
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
            if let Some(dll) = form.ngp_dll.resolved(&form.data, &form.rsf()) {
                case = case.replace(&dll.display().to_string(), "<ngp-dll>");
            }
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

    /// The installed voice and Tanner notes reach the viewer; choosing the original voice or disabling calls
    /// removes the mod without changing the stepping process's arguments.
    #[test]
    fn the_installed_codriver_reaches_the_viewer_on_tanner() {
        let env = |key| std::env::var_os(key).map(PathBuf::from);
        let (Some(data), Some(patch), Some(ngp), Some(rsf)) =
            (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS"), env("RBR_RSF"))
        else {
            return;
        };
        let Some(mods) = CodriverMods::scan(&rsf.to_string_lossy()) else { return };
        let Some(voice) = mods.voice(None) else { return };
        let here = std::env::temp_dir().join(format!("rbr-launcher-tanner-codriver-{}", std::process::id()));
        std::fs::create_dir_all(&here).unwrap();
        let start = Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: here.join(SETTINGS_FILE),
            data: Some(data),
            patch: Some(patch),
        };
        let settings = Settings {
            map: Some(54),
            ngp_physics: Some(ngp.to_string_lossy().into_owned()),
            rsf_root: Some(rsf.to_string_lossy().into_owned()),
            ..Settings::default()
        };
        let mut form = load(&start, settings).unwrap();
        form.read_only = true;
        let launch = |form: &mut Form| form.start_args(&mut CrtRand::srand(1234)).unwrap();
        let value = |args: &[String], flag| args.iter().position(|a| a == flag).map(|i| args[i + 1].clone());
        let defaults = launch(&mut form);
        assert_eq!(value(&defaults, "--codriver-mod"), Some(mods.dir.display().to_string()));
        assert_eq!(value(&defaults, "--codriver-voice"), Some(voice.clone()));
        let notes = pacenotes::my_pacenotes_dir(&rsf).and_then(|dir| pacenotes::my_pacenotes(&dir, &["Tanner"]));
        assert_eq!(value(&defaults, "--pacenotes"), notes.map(|p| p.display().to_string()));
        let stepping = defaults[defaults.iter().position(|a| a == "--live").unwrap()..].to_vec();

        form.codriver_voice = Some(voice.clone());
        form.codriver_style = "Numeric.ini".into();
        form.codriver_ranges = "Rbr.ini".into();
        form.my_pacenotes = false;
        let custom = launch(&mut form);
        assert_eq!(value(&custom, "--codriver-voice"), Some(voice));
        assert_eq!(value(&custom, "--codriver-style").as_deref(), Some("Numeric.ini"));
        assert_eq!(value(&custom, "--codriver-ranges").as_deref(), Some("Rbr.ini"));
        assert!(value(&custom, "--pacenotes").is_none());

        form.codriver_voice = Some(ORIGINAL_VOICE.into());
        let original = launch(&mut form);
        assert!(value(&original, "--codriver-mod").is_none());
        assert_eq!(&original[original.iter().position(|a| a == "--live").unwrap()..], stepping);
        form.codriver_voice = None;
        form.codriver = false;
        let muted = launch(&mut form);
        assert!(muted.iter().any(|a| a == "--no-codriver"));
        assert!(value(&muted, "--codriver-mod").is_none());
        std::fs::remove_dir_all(here).unwrap();
    }

    /// Without an RSF install, a locally owned voice pack and Tanner notes reach the viewer on the same flags;
    /// the imported choices persist in the settings and work after the sources are removed. The stepping
    /// process's arguments never change.
    #[test]
    fn local_codriver_packs_drive_without_rsf() {
        use crate::codriver_packs;
        use rbr_formats::rsf::codriver;
        let env = |key| std::env::var_os(key).map(PathBuf::from);
        let (Some(data), Some(patch), Some(ngp)) = (env("RBR_DATA"), env("RBR_PATCH"), env("RBR_NGP_PHYSICS")) else {
            return;
        };
        let root = std::env::temp_dir().join(format!("rbr-launcher-local-codriver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let write = |path: PathBuf, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
        };
        // A voice download: an extracted folder with a wrapper directory.
        let download = root.join("download").join("wrapper");
        write(download.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        write(download.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(download.join("config/pacenotes/calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(download.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(download.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        // A notes-only download: a MyPacenotes folder.
        let notes_src = root.join("notes-download");
        write(
            notes_src.join("Tanner/tanner.ini"),
            "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n",
        );
        // The player's folder owns the copies; the RSF field stays empty (no RSF install).
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        codriver_packs::import(&home, &root.join("download"), &std::sync::atomic::AtomicBool::new(false)).unwrap();
        codriver_packs::import(&home, &notes_src, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        let start = Start {
            viewer: PathBuf::from("/bin/viewer"),
            standalone: PathBuf::from("/bin/rbr-standalone"),
            settings: home.join(SETTINGS_FILE),
            data: Some(data),
            patch: Some(patch),
        };
        let settings = Settings {
            map: Some(54),
            ngp_physics: Some(ngp.to_string_lossy().into_owned()),
            rsf_root: Some(String::new()),
            codriver_pack: Some("download".into()),
            notes_pack: Some("notes-download".into()),
            ..Settings::default()
        };
        // The choices persist: written and read back.
        let text = settings.to_text();
        assert!(text.contains("codriver_pack = download\nnotes_pack = notes-download\n"), "{text}");
        assert_eq!(Settings::parse(&text).codriver_pack.as_deref(), Some("download"));
        let mut form = load(&start, settings).unwrap();
        form.read_only = true;
        assert!(form.codriver_mods.is_some());
        let launch = |form: &mut Form| form.start_args(&mut CrtRand::srand(1234)).unwrap();
        let value = |args: &[String], flag| args.iter().position(|a| a == flag).map(|i| args[i + 1].clone());
        let args = launch(&mut form);
        let pack = home.join(codriver_packs::VOICES_DIR).join("download");
        assert_eq!(value(&args, "--codriver-mod"), Some(pack.display().to_string()));
        assert_eq!(value(&args, "--codriver-voice").as_deref(), Some("english/steve"));
        let notes =
            codriver_packs::find_note(&home.join(codriver_packs::NOTES_DIR).join("notes-download"), &["Tanner"]);
        assert_eq!(value(&args, "--pacenotes"), notes.map(|p| p.display().to_string()));
        assert!(value(&args, "--pacenotes").is_some());
        let stepping = args[args.iter().position(|a| a == "--live").unwrap()..].to_vec();
        assert!(!stepping.iter().any(|a| a == "--codriver-mod" || a == "--pacenotes"));
        // Imported choices work after the sources are removed.
        std::fs::remove_dir_all(root.join("download")).unwrap();
        std::fs::remove_dir_all(&notes_src).unwrap();
        form.rescan_codrivers();
        assert!(codriver::load(&pack, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&0));
        let again = launch(&mut form);
        assert_eq!(value(&again, "--codriver-mod"), Some(pack.display().to_string()));
        assert_eq!(&again[again.iter().position(|a| a == "--live").unwrap()..], stepping);
        // The original voice and mute keep working with local packs chosen.
        form.codriver_voice = Some(ORIGINAL_VOICE.into());
        let original = launch(&mut form);
        assert!(value(&original, "--codriver-mod").is_none());
        assert_eq!(&original[original.iter().position(|a| a == "--live").unwrap()..], stepping);
        form.codriver_voice = None;
        form.codriver = false;
        let muted = launch(&mut form);
        assert!(muted.iter().any(|a| a == "--no-codriver"));
        assert!(value(&muted, "--codriver-mod").is_none());
        // Switching packs normalizes stale choices: the old voice and style do not travel along.
        let second = root.join("second");
        write(second.join("config/pacenotes/Voice.ini"), "[PACKAGE::V]\nfile=z-calls.ini\n");
        write(second.join("config/pacenotes/z-calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(second.join("config/ranges/Voice.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(second.join("sounds/other/ann/one_left.ogg"), "OggS fake");
        let no_cancel = std::sync::atomic::AtomicBool::new(false);
        codriver_packs::import(&home, &second, &no_cancel).unwrap();
        form.codriver = true;
        form.codriver_voice = Some("english/steve".into());
        form.codriver_style = "Rbr.ini".into();
        form.codriver_pack = Some("second".into());
        form.rescan_codrivers();
        form.normalize_codriver();
        assert_eq!(form.codriver_voice, None);
        assert_eq!(form.codriver_style, "Voice.ini");
        assert_eq!(form.codriver_ranges, "Voice.ini");
        // The pack without a PaceNote.ini speaks its discovered voice, and the normalized choices save.
        let switched = launch(&mut form);
        assert_eq!(value(&switched, "--codriver-voice").as_deref(), Some("other/ann"));
        form.read_only = false;
        assert!(form.save());
        let saved = std::fs::read_to_string(home.join(SETTINGS_FILE)).unwrap();
        assert!(saved.contains("codriver_pack = second\n"), "{saved}");
        assert!(!saved.contains("codriver_voice"), "{saved}");
        assert!(saved.contains("codriver_style = Voice.ini\n"), "{saved}");
        // The explicit 1.02 choice survives a pack switch.
        form.codriver_voice = Some(ORIGINAL_VOICE.into());
        form.normalize_codriver();
        assert_eq!(form.codriver_voice.as_deref(), Some(ORIGINAL_VOICE));
        std::fs::remove_dir_all(&root).unwrap();
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
