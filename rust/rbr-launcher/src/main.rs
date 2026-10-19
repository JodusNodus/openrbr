//! The launcher (ReRBR's): one window to pick a car, map, original weather, tyre wear, resolution and field of view, and Start.
//! It derives the stage variant and render wetness from the 1.02 season table, then runs the viewer with
//! `rbr-standalone --live --boot`. The viewer and `rbr-standalone` run as child processes.
//!
//! ```text
//! launcher [--data DIR] [--viewer PATH] [--standalone PATH] [--list] [--command]
//!     [--screenshot DIR [--screenshot-damage FILE]]
//! ```
//!
//! The game is the RBR install folder the Settings page sets (`install_folder` in `launcher-settings.ini`, `install`):
//! RBR 1.02 with NGP (RallySimFans or not), read in place. Without one the window asks for it, offering the first
//! usual install place with NGP (`install::detect`). `--data` (or `RBR_DATA`) and
//! `RBR_PATCH` override its game data and 1.02 patch files, the Settings page's Advanced fields its NGP archive and RSF
//! folder; the viewer and `rbr-standalone` get the game as `RBR_DATA` and `RBR_PATCH`.
//!
//! `--command` prints the viewer command Start runs with the saved selection (`RBR_DATA=` and `RBR_PATCH=`, the
//! program, then one argument a line) and exits, for starting it unattended. `--list`, `--command` and `--screenshot`
//! never write the settings file.
//!
//! Good, random and bad are the game's weather presets. Random draws the season row and the tyre wear as the game's
//! `E74_FindBest` does, from the CRT's `rand()` stream seeded with the time when the launcher starts (the game seeds it
//! at its start too; the menus' own `rand()` calls between are not modelled, ours); the Drive page shows the next
//! draw, and each Start takes it. A preset chooses the stage variant and dry, damp or wet render
//! folder; map 68 has no good or bad row and the original stops at its int3. Tyre wear defaults to new, and the tyre
//! defaults to `E74_WeatherKind` for the map surface and derived wetness. `--list` prints the startable map count and
//! any unavailable-map reasons without opening a window. The field of view is the horizontal one of the in-car views and
//! the free-fly camera (`viewer --fov`), 30 to 120 degrees, default 50; the chase view keeps its own. The viewer keeps
//! the view last chosen and each car's camera nudges in the settings file's `[camera]` section (`viewer --cameras`,
//! `rbr_render::camera_prefs`); the launcher carries that section over when it writes the file, and the Cameras row's
//! "Reset this car's cameras" drops the selected car's nudges. The sound volume
//! (`viewer --volume`) is linear, 0 to 4, default 1 (the game's). The Co-driver box turns the pace notes off
//! (`viewer --no-codriver`), and the season row's rain goes to the sound (`viewer --rain`). The launcher plays the
//! service park's ambience, at the sound volume, while a championship is at a service stop (its page and the Setup
//! page; `rbr_sound::menu`), and nothing while the viewer runs. The game's music is not played (user decision).
//! The Ghost field races a ghost car in the quick race (`viewer --ghost`): off, the best one (`--best-ghost
//! GhostCars/Best_<map:02>.ghc`, replaced when a finished drive beats it, as `BeginWrite` keeps the faster run; ours:
//! the game keeps it in memory and saves on request), or a saved `GhostCars/<name>_<map:02>.ghc` of the stage's map.
//! The Graphics & sound page turns the renderer's additions beyond the game's look on and off (`viewer --enhance`, one
//! `launcher-settings.ini` key each, named as `rbr_render::renderer::Enhance::NAMES`). The game's own quality settings
//! are not offered: the stage always draws at the highest (user decision).
//! The Time of day box draws a quick race's stage at a start time on its rally's date (`viewer --clock`, `start_time`
//! in the settings): the sun, its light and shadows, dusk and night lit by the car's lamps; the start board shows it
//! (`rbr-standalone --start-time`). The stage's own light, the default, is the game's, and a championship stage
//! always has it, with the season's start clock.
//! The Wipers box is the wiper switch at the start (`viewer --wipers`: off, intermittent, continuous, or auto by the
//! weather and the mud on the glass); the wiper key (I by default, in the Controls section) steps it while driving.
//! The headlight key (L by default) switches the headlights and tail lights off and on.
//! The physics is NGP's (`rbr-standalone --ngp-physics`, from the install's `physics.rbz`; user decision: the players'
//! game requires NGP, and 1.02's physics stays in `rbr-standalone` and the viewer's flags for the checks). The cars are
//! those NGP drives (`catalog::ngp_offered`): the retail slots its archive has NGP cars in (`catalog::ngp_cars`), and
//! with a RallySimFans folder (the install folder, or the RSF field) the cars of its cache (`rsfdata/cache/cars.json`,
//! with their group), raced from their packs as RSF installs them in slot 5 (`--rsf-car`, `rbr_formats::rsf`); the
//! retail slot 5 is then left out (it is the RSF car last picked). The cars added from gvrc.de's NGP car packs ("Add
//! cars…", `--add-cars DIR`: `car_packs`, in `cars/` next to the launcher) are raced the same way; a car RSF also has
//! is listed once, RSF's. Cars without a
//! pack are listed as unavailable with the reason (also by `--list`); cars without their model folder use the retail
//! model RSF names as their fallback. The stage records, personal bests and ghosts are kept in the game's files
//! under `NGP/` next to the launcher (`NGP/SavedGames/pfresult.rst`, `NGP/SavedGames/pf<name>.rbr`,
//! `NGP/GhostCars/`; ours: the game has no NGP convention and RSF keeps its records online). In an RSF race the Ghost
//! field lists RSF's kinds, and Best plays RSF's overall
//! best for the map and road condition (`GhostCars/<map>/<condition>/<map>.gch`).
//! The Car and Map fields open searchable lists (`picker`): grouped, filtered by whether they can start, the surface
//! and the length, the recent picks first and the ones that cannot start last with the reason; the previews follow the
//! highlighted entry. The last selection and the recent picks (`recent_cars`, `recent_maps`) are kept in
//! `launcher-settings.ini` next to the launcher; an older file's car NGP doesn't drive gives way to the most recent
//! one offered, and its `ngp` physics choice is ignored.
//! With a RallySimFans folder the maps add its community stages (`rbr_formats::rsf`); one the launcher cannot start
//! (an encrypted one) stays listed with the reason. Until saved, the NGP and RSF fields default to `$RBR_NGP_PHYSICS`
//! and `$RBR_RSF`.
//!
//! The tabs follow what the player wants to do (ours, after Content Manager): Drive (car and stage, conditions, race
//! options, the previews and Start), Championship, Car setup, Controls, Graphics & sound (window, camera, HUD and pace
//! notes, sound, the renderer's additions) and Settings (install folder, advanced paths, about). The status line under
//! every page says what the install lacks (the programs, NGP's archive, a car NGP drives), with its fix
//! (`Form::readiness`); the Drive page
//! says what else blocks Start (`Form::blocker`). The pages change only where a field is shown: the settings file and
//! the viewer command are those of the earlier pages (`command::tests::the_viewer_command_follows_the_saved_settings`).
//!
//! The Controls page (`controls_page`) sets the live mode's bindings (`rbr_render::bindings`), the wheel, pedals and
//! shifter first with a guided set-up, the keyboard's keys collapsed below: a gamepad input (gilrs) and a key for
//! each driving action. Click a binding, then move a wheel axis or press a button, or press a key; Escape cancels.
//! The launcher opens on it when a gamepad is connected and nothing drives from one yet. They are kept in the same
//! file's `[controls]` section, saved on every change, and the viewer reads that file (`viewer --bindings`); the
//! viewer, not the launcher, reads the inputs while driving. The advanced H-shifter
//! (`rbr_render::shifter`) is set there too; with it on, the launcher turns clutch help off and passes the RSF car's
//! gearbox and the grind sample from RBRControls' files in the RSF folder (`command::shifter_args`).
//!
//! The Drive page's Race section holds the menu's race options (`rbr_gameplay::race_options`): the damage level, which also sets the
//! outside-assistance penalty, and the four driver aids, passed as `rbr-standalone --damage`, `--auto-gears`,
//! `--gear-guard`, `--neutral-lock` and `--clutch-help`.
//!
//! The Car setup page edits the car's setup for the map's surface (`setup_page`): the values the game's setup pages edit,
//! within the car's ranges and by its steps, with named setups in the game's format. A changed setup goes to
//! `rbr-standalone --setup`. `--screenshot DIR` draws the pages, and the Car setup page's areas, offscreen
//! to PNGs and exits.
//!
//! The Championship page (`rally_page`) runs the season: new season or continue (the profile's save in
//! `SavedGames/`), each rally stage by stage with the service park between stages, the AI drivers' times, the rally
//! and season standings, the bulletins and the trials between rallies. New season takes the championship's own
//! options (the team, skill level and car damage, kept in `launcher-settings.ini` as `championship_team`,
//! `championship_skill` and `championship_damage`); the season drives its team's car when NGP drives it, else a car
//! picked for it (ours; each profile's in the settings file's `[season]` section, so the game's save stays the
//! game's), and its stages
//! run as the game's championship on the season's (`rbr-standalone --championship`) with the Drive page's aids.
//! `--screenshot` also draws it, and with `--screenshot-damage FILE` (`rbr-standalone --damage-out`) its service park on that car.

mod capture;
mod car_packs;
mod catalog;
mod command;
mod controls_page;
mod disc_install;
mod drive_page;
mod ghosts;
mod graphics_page;
mod install;
mod pages;
mod picker;
mod preview;
mod rally_page;
mod settings;
mod settings_page;
mod setup_page;
mod window;

use std::path::{Path, PathBuf};
use std::process::{Child, ExitCode};

use catalog::{Catalog, TyreChoice};
use controls_page::ControlsPage;
use ghosts::GhostChoice;
use pages::Page;
use rally_page::RallyPage;
use rbr_formats::data::Data;
use rbr_gameplay::crt::CrtRand;
use rbr_gameplay::markers::MarkerSettings;
use rbr_gameplay::race_options::RaceOptions;
use rbr_gameplay::weather::Season;
use rbr_render::bindings::Bindings;
use rbr_render::glass::model::Switch;
use rbr_sound::menu::MenuPlayer;
use settings::{load, or_folder, read_settings, SeasonCars};
use setup_page::SetupPage;
use window::App;
use winit::event_loop::EventLoop;

const SETTINGS_FILE: &str = "launcher-settings.ini";
/// The folder next to the launcher with NGP's own `SavedGames/` and `GhostCars/` (ours).
const NGP_RECORDS: &str = "NGP";
/// `--help`'s text.
const USAGE: &str = "usage: launcher [OPTIONS]
Set your RBR install folder (1.02 with NGP; RallySimFans has both) on the Settings page, or at the first start; it is
only read. From it: the game data and the 1.02 exe at its top, NGP's physics.rbz and, when there, the RallySimFans
content (rsfdata/, Maps/<id>-<name>/, Cars/).
  --data DIR                the game data instead of the install folder's (default $RBR_DATA; $RBR_PATCH: the 1.02
                            patch files)
  --viewer PATH             the viewer (default: next to the launcher)
  --standalone PATH         rbr-standalone (default: next to the launcher)
  --list                    print the startable maps, the unavailable maps and cars, and what the install lacks, and exit
  --command                 print the viewer command Start runs with the saved selection (RBR_DATA= and RBR_PATCH=,
                            the program, one argument a line), and exit
  --screenshot DIR          draw the pages to PNGs in DIR, and exit
  --screenshot-damage FILE  with --screenshot: also the service park on this damage file's car
  --install-from-disc DISC PATCH NGP OUT
                            set up a game folder OUT (new or empty) from the disc (.iso or folder), the 1.02 patch
                            (RBR_v102.zip or its .exe) and the NGP package (.7z or folder), and exit
  --extract-test-data DISC PATCH OUT
                            write the test data (docs/development.md) to OUT (new or empty) from the disc and the
                            1.02 patch: OUT/retail-install for RBR_DATA and OUT/Main_group for RBR_PATCH, and exit
  --add-cars DIR            add the NGP car packs downloaded from gvrc.de into DIR (<Car> - NGP Physics.zip and
                            <Model> - Model.7z) to the cars folder next to the launcher, and exit
  -h, --help                print this text
The settings are kept in launcher-settings.ini next to the launcher; --list, --command and --screenshot do not write it.
The Settings page's Advanced fields override the install folder's NGP physics.rbz and RSF folder; until saved they come
from RBR_NGP_PHYSICS and RBR_RSF.";

/// The game's data and 1.02 patch files (`rbr_formats::data`), the programs and the settings file.
struct Paths {
    data: PathBuf,
    patch: Option<PathBuf>,
    viewer: PathBuf,
    standalone: PathBuf,
    settings: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("launcher: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--install-from-disc") {
        return disc_install::cli(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "--extract-test-data") {
        return disc_install::test_data_cli(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "--add-cars") {
        let here = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
        return car_packs::cli(&here.join(car_packs::DIR), &args[1..]);
    }
    let (start, list, command, screenshot, rally_damage) = parse_args(std::env::args().skip(1))?;
    let settings = read_settings(&start.settings);
    if !list && !command && screenshot.is_none() {
        // The window opens without a game too, to choose its folder.
        let form = load(&start, settings.clone()).map_err(|e| eprintln!("launcher: {e}")).ok();
        let folder = settings.install_folder;
        let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
        let mut app = App::new(start, form, folder);
        return event_loop.run_app(&mut app).map_err(|e| e.to_string());
    }
    let mut form = load(&start, settings)?;
    // Only the window writes the settings file.
    form.read_only = true;
    if list {
        let (catalog, season) = (&form.catalog, &form.season);
        println!("Startable maps: {}/{}", catalog.startable_maps(season).len(), catalog.maps.len());
        for map in &catalog.maps {
            if let Err(reason) = catalog.availability(season, map) {
                println!("Unavailable: {} — {reason}", catalog::map_label(map));
            }
        }
        if let Some(issue) = form.no_ngp() {
            println!("{issue}");
        }
        for car in &catalog.cars {
            if let Some(reason) = car.unavailable() {
                println!("Unavailable car: {} — {reason}", catalog::car_label(car));
            } else if let Some(rsf) = car.rsf.as_ref().filter(|r| r.added) {
                let model = match &rsf.model {
                    Ok(Some(stand_in)) => format!(" — model: {stand_in}"),
                    _ => String::new(),
                };
                let pack = rsf.pack.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                println!("Added car: {} — {pack}{model}", catalog::car_label(car));
            } else if let Some(Ok(Some(stand_in))) = car.rsf.as_ref().map(|r| &r.model) {
                println!("RSF car: {} — model: {stand_in}", catalog::car_label(car));
            }
        }
        return Ok(());
    }
    if command {
        // What Start runs with the saved selection, without starting it: the game's paths, the program, then one
        // argument a line.
        let mut rand = form.rand;
        let args = form.start_args(&mut rand).ok_or_else(|| format!("cannot start: {}", form.status))?;
        form.child_env().iter().for_each(|(key, value)| println!("{key}={}", value.display()));
        println!("{}", form.paths.viewer.display());
        args.iter().for_each(|a| println!("{a}"));
        return Ok(());
    }
    if let Some(dir) = screenshot {
        capture::pages(&mut form, &dir, rally_damage)?;
    }
    Ok(())
}

/// The programs, the settings file and the game's overrides of the install folder: `--data` (or `RBR_DATA`) and
/// `RBR_PATCH`.
#[derive(Clone)]
struct Start {
    viewer: PathBuf,
    standalone: PathBuf,
    settings: PathBuf,
    data: Option<PathBuf>,
    patch: Option<PathBuf>,
}

/// The start, `--list`, `--command`, `--screenshot` and `--screenshot-damage`.
type Parsed = (Start, bool, bool, Option<PathBuf>, Option<PathBuf>);

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let here = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let mut data = env("RBR_DATA");
    let mut viewer = here.join("viewer");
    let mut standalone = here.join("rbr-standalone");
    let (mut list, mut command) = (false, false);
    let mut screenshot = None;
    let mut rally_damage = None;
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            println!("{USAGE}");
            std::process::exit(0);
        }
        if flag == "--list" || flag == "--command" {
            list |= flag == "--list";
            command |= flag == "--command";
            continue;
        }
        let value = PathBuf::from(args.next().ok_or(format!("{flag} needs a value"))?);
        match flag.as_str() {
            "--data" => data = Some(value),
            "--viewer" => viewer = value,
            "--standalone" => standalone = value,
            "--screenshot" => screenshot = Some(value),
            "--screenshot-damage" => rally_damage = Some(value),
            _ => return Err(format!("unknown argument {flag}\n{USAGE}")),
        }
    }
    let start = Start { viewer, standalone, settings: here.join(SETTINGS_FILE), data, patch: env("RBR_PATCH") };
    Ok((start, list, command, screenshot, rally_damage))
}

/// The selection and the started viewer.
struct Form {
    catalog: Catalog,
    paths: Paths,
    data: Data,
    season: Season,
    page: Page,
    setup: SetupPage,
    rally: RallyPage,
    /// The Drive page's stage and car previews.
    previews: preview::Previews,
    car: usize,
    map: usize,
    /// The Drive page's car and map lists, and their recent picks (car names, map numbers).
    car_picker: picker::Picker,
    map_picker: picker::Picker,
    recent_cars: Vec<String>,
    recent_maps: Vec<u32>,
    /// The car a new season takes, its list, and each profile's season car (`[season]`).
    season_pick: usize,
    season_picker: picker::Picker,
    season_cars: SeasonCars,
    /// The weather preset, the damage level and the driver aids.
    options: RaceOptions,
    /// The random weather's `rand()` stream (`srand(time)` at the launcher's start).
    rand: CrtRand,
    wear: u32,
    tyre: TyreChoice,
    size: (u32, u32),
    sizes: Vec<(u32, u32)>,
    /// Every view's horizontal field of view, degrees.
    fov: f32,
    /// The sound volume, linear (`viewer --volume`).
    volume: f32,
    /// The co-driver calls the pace notes.
    codriver: bool,
    /// The quick race's ghost car.
    ghost: GhostChoice,
    /// The menus' sound, once something has played (`None` without an audio device).
    menu_sound: Option<Option<MenuPlayer>>,
    /// The HUD shown, its speed unit, and the FPS counter.
    hud: graphics_page::HudChoice,
    /// The renderer additions on (`viewer --enhance`).
    graphics: graphics_page::Graphics,
    /// The render scale, percent ([`graphics_page::RENDER_SCALES`]).
    render_scale: u32,
    /// How the driving window presents its frames (`viewer --present`).
    present: rbr_render::present::Present,
    /// How often the cockpit's mirrors are drawn (`viewer --mirrors`).
    mirrors: rbr_render::mirrors::Rate,
    /// The driving window fills the monitor (`viewer --fullscreen`); `size` applies when off.
    fullscreen: bool,
    /// The start time, minutes past midnight (`viewer --clock`); none for the stage's own light.
    start_time: Option<u32>,
    /// The wiper switch at the start (`viewer --wipers`).
    wipers: Switch,
    /// The RBR install folder's field, and what the form was loaded from (`install`).
    install_folder: String,
    folder: install::Folder,
    /// The install folder's "Set up from the disc…" panel.
    disc_setup: disc_install::DiscSetup,
    /// "Add cars…" and the added cars (`car_packs`).
    adding: car_packs::Adding,
    /// Set when the install folder changed: the window loads the form again.
    reload: bool,
    /// NGP's `physics.rbz` field (empty: the install folder's).
    ngp_physics: String,
    /// The archive's NGP revision (none: not NGP's), its name (`install::ngp_label`), and the car numbers it has NGP
    /// cars in (`catalog::ngp_cars`).
    ngp_revision: Option<u32>,
    ngp_label: String,
    ngp_cars: Vec<u32>,
    /// The RSF game folder field (empty: the install folder), whose cars and stages the lists add.
    rsf_root: String,
    /// The folder the lists hold the RSF cars and stages of: the field's once it loses focus, as a scan of a full
    /// install takes 0.2-1.7 s (ours).
    rsf_scanned: String,
    marker: MarkerSettings,
    child: Option<Child>,
    status: String,
    bindings: Bindings,
    /// The wheel's force feedback in live mode (the settings file's `[ffb]` section).
    ffb: rbr_render::ffb::Options,
    controls: ControlsPage,
    /// Never writes the settings file (`--command`, `--screenshot`).
    read_only: bool,
}

impl Form {
    /// Fills the map list with the maps and their availability.
    fn list_maps(&mut self) {
        let catalog = &self.catalog;
        let entries = catalog
            .maps
            .iter()
            .enumerate()
            .map(|(i, map)| picker::map_entry(i, map, catalog.availability(&self.season, map).err()))
            .collect();
        self.map_picker.set_entries(entries);
    }

    /// Offers the cars NGP's archive drives, keeping the selected ones when they are still there, and fills the car
    /// lists (the Drive page's and the season's) and NGP's name.
    fn list_cars(&mut self) {
        let archive = self.ngp_archive();
        self.ngp_revision = install::ngp_revision(Path::new(&archive));
        self.ngp_label = install::ngp_label(self.ngp_revision);
        self.ngp_cars = catalog::ngp_cars(Path::new(&archive));
        let name = self.car().map(|c| c.name.clone());
        let season = self.catalog.cars.get(self.season_pick).map(|c| c.name.clone());
        self.catalog.offer(&self.ngp_cars);
        self.car = settings::pick_car(&self.catalog.cars, name.iter().chain(&self.recent_cars));
        self.season_pick = settings::pick_car(&self.catalog.cars, season.iter().chain(&self.recent_cars));
        let entries: Vec<_> = self.catalog.cars.iter().enumerate().map(|(i, car)| picker::car_entry(i, car)).collect();
        self.season_picker.set_entries(entries.clone());
        self.car_picker.set_entries(entries);
    }

    /// The selected car, none when NGP drives none.
    fn car(&self) -> Option<&catalog::Car> {
        self.catalog.cars.get(self.car)
    }

    /// Replaces the lists' RSF cars and stages by the RSF field's folder's, keeping the selected map when it is
    /// still there (the cars: `list_cars`).
    fn rescan_rsf(&mut self) {
        let root = self.rsf();
        self.catalog.set_rsf_cars(&self.paths.data, &root, &self.cars_dir());
        let number = self.catalog.maps[self.map].number;
        self.catalog.set_rsf(&root);
        self.rsf_scanned = self.rsf_root.clone();
        let (catalog, season) = (&self.catalog, &self.season);
        self.map = (catalog.maps.iter().position(|m| m.number == number))
            .or_else(|| catalog.maps.iter().position(|map| catalog.availability(season, map).is_ok()))
            .unwrap_or(0);
        self.list_maps();
    }

    /// NGP's `physics.rbz`: the field's, else the install folder's.
    fn ngp_archive(&self) -> String {
        or_folder(Some(&self.ngp_physics), &self.folder.ngp_physics)
    }

    /// The RSF folder: the field's, else the install folder when it is one.
    fn rsf(&self) -> String {
        or_folder(Some(&self.rsf_root), &self.folder.rsf_root)
    }

    /// The viewer's and `rbr-standalone`'s game (`rbr_formats::data::Data::from_env`), and the RSF install an RSF
    /// ghost's car model is read from.
    fn child_env(&self) -> Vec<(&'static str, PathBuf)> {
        let patch = self.paths.patch.clone().map(|p| ("RBR_PATCH", p));
        let rsf = self.rsf();
        let rsf = (!rsf.trim().is_empty()).then(|| ("RBR_RSF", PathBuf::from(rsf.trim())));
        [("RBR_DATA", self.paths.data.clone())].into_iter().chain(patch).chain(rsf).collect()
    }

    /// The launcher's folder, where the settings and the saved setups are.
    fn here(&self) -> PathBuf {
        self.paths.settings.parent().map(Path::to_path_buf).unwrap_or_default()
    }

    /// The added cars' folder (`car_packs`).
    fn cars_dir(&self) -> PathBuf {
        self.here().join(car_packs::DIR)
    }

    /// Where the stage records, personal bests and ghosts are: the launcher's `NGP/` (ours), apart from the 1.02 ones
    /// earlier versions kept in the launcher's folder.
    fn records(&self) -> PathBuf {
        self.here().join(NGP_RECORDS)
    }

    /// Points the Setup page at the selected car and the map's surface.
    fn select_setup(&mut self) {
        let surface = ["gravel", "tarmac", "snow"][self.catalog.maps[self.map].surface() as usize];
        let here = self.here();
        let archive = self.ngp_archive();
        if let Some(car) = self.catalog.cars.get(self.car) {
            self.setup.select(&self.data, &here, car, Path::new(&archive), surface);
        }
    }
}
