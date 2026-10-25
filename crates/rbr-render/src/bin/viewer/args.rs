//! The command line (`--help`'s [`USAGE`]).

use glam::Vec3;
use rbr_render::atmosphere::Fog;
use rbr_render::bindings::Bindings;
use rbr_render::camera::FreeFly;
use rbr_render::coords;
use rbr_render::damage::{self, Zones};
use rbr_render::ffb;
use rbr_render::ghost;
use rbr_render::glass;
use rbr_render::hud::pause::Nav;
use rbr_render::hud::Parts;
use rbr_render::present::Present;
use rbr_render::renderer::{AntiAliasing, Enhance};
use rbr_render::shifter::CarBox;
use rbr_render::stage::{Selection, Sky, Wear, Weather};
use rbr_render::views::CarView;
use std::path::{Path, PathBuf};

pub struct Args {
    pub stage: Option<String>,
    pub car: Option<String>,
    /// `--rsf-car`: an RSF car in slot 5, read in place (`rbr_formats::rsf::select`).
    pub rsf_car: Option<String>,
    pub drive: Option<PathBuf>,
    pub at: Option<u32>,
    /// `--at-drive`: the recording whose controls the `--at` payloads send, each the control of the step it feeds.
    pub at_drive: Option<PathBuf>,
    pub live: Option<Vec<String>>,
    pub bindings: Bindings,
    /// The wheel's force feedback in live mode (the `--bindings` file's `[ffb]` section).
    pub ffb: ffb::Options,
    pub screenshot: Option<PathBuf>,
    pub camera: Option<FreeFly>,
    /// `--size`: the screenshot size, and the window's in pixels.
    pub size: Option<(u32, u32)>,
    /// `--fov`: the horizontal field of view of the cameras but the chase, radians.
    pub fov: f32,
    /// `--cameras`: the file with the player's camera choices (`camera_prefs`).
    pub cameras: Option<PathBuf>,
    pub selection: Selection,
    /// `--view`: the camera view a drive starts in (else the one last chosen).
    pub view: Option<CarView>,
    /// `--damage`: the damage zones the car is dented for.
    pub damage: Option<Zones>,
    /// `--no-sound` and `--volume`: the live drive's sound, and its volume (linear, 1 = the game's).
    pub sound: bool,
    pub volume: f32,
    /// `--rain none|light|heavy` and `--no-codriver`: the rain the sound plays, and the pace notes off.
    pub rain: f32,
    pub codriver: bool,
    /// `--codriver-mod`: a Pacenote plugin's co-driver in place of 1.02's (`rbr_sound::custom_codriver`).
    pub codriver_mod: Option<CodriverMod>,
    /// `--crack`: damage events (code, functor index) that crack the windows.
    pub cracks: Vec<(u32, u32)>,
    /// `--textures`: a texture pack replacing stage textures.
    pub textures: Option<PathBuf>,
    /// `--enhance`: renderer additions beyond the game's look; `--mirrors` its mirrors' rate; `--anti-aliasing` the
    /// frame's anti-aliasing.
    pub enhance: Enhance,
    /// `--clock HH:MM`: the sun, its light and the ambient at that local solar time on the stage's rally
    /// (`relight::daylight`); the stage at night (`Atmosphere::night`) once the sun is 6 degrees down. Without it the
    /// stage starts at its bake's sun and hour (`relight::Clock::new`).
    pub clock: Option<f32>,
    /// `--clock-rate R`: clock seconds per drive second (1: real time).
    pub clock_rate: f32,
    pub relight: bool,
    /// `--fog off|light|heavy`: our fog over the section's (`Atmosphere::fog`; with `--enhance sky`).
    pub fog: Option<Fog>,
    /// `--render-scale`: the share of the window's width and height the frame is drawn at (`Renderer::set_render_scale`;
    /// `auto`, from older settings: 1).
    pub render_scale: f32,
    /// `--present`: how the window presents its frames; `--max-fps`: the drawn frames per second, at most.
    pub present: Present,
    pub max_fps: Option<f32>,
    /// `--fullscreen`: the window fills its monitor, borderless (`--size` is then the size it leaves fullscreen at).
    pub fullscreen: bool,
    /// `--title`: what the loading screen names (the launcher's stage and car), else the stage's file name.
    pub title: Option<String>,
    /// `--no-particles` clears it.
    pub particles: bool,
    /// `--frames`: frames to time before the screenshot; `--profile`: per stage too.
    pub frames: u32,
    pub profile: bool,
    /// `--sequence N` (with `--screenshot` and a drive): N frames, each one drive frame on, as `<stem>-NNN.png`.
    pub sequence: u32,
    /// `--time`: the screenshot's time in seconds.
    pub time: f32,
    /// `--birdstrike`: every bird-strike item loads; `--rescue-at`: the step the rescue helicopter is called at.
    pub birdstrike: bool,
    pub rescue_at: Option<u32>,
    /// `--seed`: the race start's seed of `rand()` and `LuaRandom`.
    pub seed: u32,
    /// `--quit-after S`: the window closes itself after S seconds, as on Escape (unattended runs); counted from the
    /// race's end with `end+S` (the flag).
    pub quit_after: Option<(f32, bool)>,
    /// `--no-fences` clears it.
    pub fences: bool,
    /// The working dials of an RSF car with RBRHUD gauges (`rbr_render::dials`); `--no-dials` clears it. `--gauge-mode N`:
    /// the gauge mode they start in (1 on; the gauge mode key cycles it).
    pub dials: bool,
    pub gauge_mode: u32,
    /// `--wipers`: the wiper switch at the start (`glass::model::Switch`).
    pub wipers: glass::model::Switch,
    /// `--look-back`: the screenshot's chases look behind the car.
    pub look_back: bool,
    /// `--pause-menu MOVES`: the screenshot shows the pause menu, opened and then moved through `MOVES` (`u` up, `d`
    /// down, `l` left, `r` right, `e` select, `b` back; `-` none).
    pub pause_menu: Option<Vec<Nav>>,
    /// `--no-hud`, `--mph`, `--fps`, `--pace-stack`, `--pace-3d` and `--hud-hide`.
    pub hud: HudOptions,
    /// `--ghost`, `--save-ghost`, `--best-ghost` and `--ghost-weather`.
    pub ghost: Option<PathBuf>,
    pub save_ghost: Option<PathBuf>,
    pub best_ghost: Option<PathBuf>,
    pub ghost_weather: rbr_formats::ghost::Weather,
    /// `--rsf-ghosts DIR`: keep the live drive in RSF's ghost layout under DIR; the RSF install and the `--rsf-car`'s
    /// id and group (an RSF ghost's car, the kept drive's car).
    pub rsf_ghosts: Option<PathBuf>,
    pub rsf_install: Option<PathBuf>,
    pub rsf_ghost_car: Option<(u32, Option<String>)>,
    /// `--car-gearbox`: the car's gearbox for the advanced H-shifter's `auto` settings; `--grind-sound`: its grind.
    pub car_gearbox: Option<CarBox>,
    pub grind_sound: Option<PathBuf>,
    /// `--car-lock DEG`: the car's lock-to-lock rotation (`rbr_render::wheel_rotation::car_degrees` of its setup), the
    /// wheel's when the rotation is per car.
    pub car_lock: Option<u16>,
}

/// `--codriver-mod DIR` and its options: the plugin folder, `--codriver-voice` (else `PaceNote.ini`'s),
/// `--codriver-style` and `--codriver-ranges` (else `Rbr.ini`), and the stage's `--pacenotes` file (else its `.dls`).
#[derive(Debug, Clone, PartialEq)]
pub struct CodriverMod {
    pub plugin: PathBuf,
    pub voice: Option<String>,
    pub style: String,
    pub ranges: String,
    pub notes: Option<PathBuf>,
}

/// Whether the HUD is drawn, its speed unit, and whether it shows the frame rate.
#[derive(Debug, Clone, Copy)]
pub struct HudOptions {
    pub show: bool,
    pub mph: bool,
    pub fps: bool,
    /// `--pace-stack N`: the pace stack's notes (`PN_STACK`, 0 to 4; 0, the game's default, hides it).
    pub stack: u8,
    /// `--pace-3d`: the 3D pace notes (`AID_MARKER`, off by default as in the game).
    pub pace_3d: bool,
    /// `--hud-hide LIST`: the HUD's parts shown (`rbr_render::hud::Part`), all by default.
    pub parts: Parts,
}

/// `--help`'s text.
pub const USAGE: &str = "usage: RBR_DATA=<install> [RBR_PATCH=<patch>] viewer [STAGE] [OPTIONS] [--live COMMAND...]
  STAGE: track-NN_X (default track-10_O), a RallySimFans stage folder, or the path of its files without the extension
  car:        --car NAME  --rsf-car PACK|ID  --damage ZONE:VALUE,...  --crack CODE:INDEX,...
  drive:      --drive RECORDING  --at STEP  --at-drive RECORDING  --live COMMAND... (the rest of the line)
              --bindings INI  --ghost FILE  --save-ghost FILE  --best-ghost FILE  --ghost-weather WORDS  --rsf-ghosts DIR
              --car-gearbox synchro|dog[,dogleg]  --grind-sound WAV  --car-lock DEG
  view:       --view NAME  --cameras INI  --fov DEG  --camera EX,EY,EZ,FX,FY,FZ  --size WxH  --fullscreen  --look-back
              --title TEXT  --pause-menu MOVES
  look:       --weather dry|damp|wet  --wear new|normal|worn  --sky SECTION  --fog off|light|heavy  --wipers off|intermittent|continuous|auto
              --present mailbox|fifo|immediate  --max-fps N
              --enhance LIST  --mirrors off|1|2|3  --anti-aliasing 4x|off  --render-scale S  --textures DIR  --no-particles  --no-fences  --no-dials  --gauge-mode N
              --clock HH:MM  --clock-rate R  --no-relight
  sound, HUD: --no-sound  --volume V  --rain none|light|heavy  --no-codriver  --no-hud  --mph  --fps
              --codriver-mod DIR  --codriver-voice NAME  --codriver-style FILE  --codriver-ranges FILE  --pacenotes FILE
              --pace-stack N  --pace-3d  --hud-hide clock,progress,pace,splits,dash,inputs,car,messages,view
  events:     --birdstrike  --rescue-at STEP  --seed N
  runs:       --screenshot PNG  --frames N  --profile  --sequence N  --time S  --quit-after S|end+S
  -V, --version  print the version
  -h, --help  print this text";

pub fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut stage = None;
    let mut car = None;
    let mut rsf_car = None;
    let mut drive = None;
    let mut at = None;
    let mut live = None;
    let mut bindings = Bindings::default();
    let mut ffb_options = ffb::Options::default();
    let mut screenshot = None;
    let mut camera = None;
    let mut size = None;
    let mut fov = coords::FOV_DEGREES;
    let mut cameras = None;
    let mut selection = Selection::default();
    let mut view = None;
    let mut damage = None;
    let mut cracks = Vec::new();
    let mut textures = None;
    let mut enhance = Enhance::default();
    let mut mirrors = rbr_render::mirrors::Rate::Off;
    let mut anti_aliasing = AntiAliasing::default();
    let (mut clock, mut clock_rate, mut relight) = (None, 1.0, true);
    let mut fog = None;
    let mut render_scale = 1.0;
    let mut present = Present::default();
    let mut max_fps = None;
    let mut particles = true;
    let (mut ghost, mut save_ghost, mut best_ghost, mut ghost_weather) = (None, None, None, Default::default());
    let mut rsf_ghosts = None;
    let (mut sound, mut volume, mut rain, mut codriver) = (true, 1.0, 0.0, true);
    let (mut codriver_mod, mut codriver_voice, mut pacenotes) = (None, None, None);
    let mut codriver_style = rbr_sound::custom_codriver::DEFAULT_STYLE.to_owned();
    let mut codriver_ranges = rbr_sound::custom_codriver::DEFAULT_RANGES.to_owned();
    let mut frames = 0;
    let mut sequence = 0;
    let mut profile = false;
    let mut time = 0.0;
    let mut birdstrike = false;
    let mut rescue_at = None;
    let mut seed = 0;
    let mut quit_after = None;
    let mut fences = true;
    let mut dials = true;
    let mut gauge_mode = 1;
    let mut wipers = glass::model::Switch::default();
    let mut look_back = false;
    let mut fullscreen = false;
    let mut title = None;
    let mut pause_menu = None;
    let mut hud = HudOptions { show: true, mph: false, fps: false, stack: 0, pace_3d: false, parts: Parts::default() };
    let mut at_drive = None;
    let (mut car_gearbox, mut grind_sound, mut car_lock) = (None, None, None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("viewer {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--no-particles" => particles = false,
            "--no-fences" => fences = false,
            "--no-dials" => dials = false,
            "--gauge-mode" => {
                let value = args.next().ok_or("--gauge-mode needs a mode, 1 on")?;
                gauge_mode = value.parse().ok().filter(|&n| n >= 1).ok_or(format!("--gauge-mode {value}: 1 on"))?;
            }
            "--look-back" => look_back = true,
            "--fullscreen" => fullscreen = true,
            "--title" => title = Some(args.next().ok_or("--title needs a text")?),
            "--pause-menu" => {
                let value = args.next().ok_or("--pause-menu needs moves (u, d, l, r, e, b) or -")?;
                let nav = |c| match c {
                    'u' => Ok(Nav::Up),
                    'd' => Ok(Nav::Down),
                    'l' => Ok(Nav::Left),
                    'r' => Ok(Nav::Right),
                    'e' => Ok(Nav::Select),
                    'b' => Ok(Nav::Back),
                    c => Err(format!("--pause-menu: {c}?")),
                };
                pause_menu = Some(value.chars().filter(|&c| c != '-').map(nav).collect::<Result<_, _>>()?);
            }
            "--wipers" => {
                let value = args.next().ok_or("--wipers needs off, intermittent, continuous or auto")?;
                wipers = glass::model::Switch::parse(&value).ok_or(format!("--wipers: {value}?"))?;
            }
            "--ghost" => ghost = Some(PathBuf::from(args.next().ok_or("--ghost needs a ghost file")?)),
            "--save-ghost" => save_ghost = Some(PathBuf::from(args.next().ok_or("--save-ghost needs a file")?)),
            "--rsf-ghosts" => rsf_ghosts = Some(PathBuf::from(args.next().ok_or("--rsf-ghosts needs a folder")?)),
            "--best-ghost" => best_ghost = Some(PathBuf::from(args.next().ok_or("--best-ghost needs a file")?)),
            "--ghost-weather" => {
                ghost_weather = ghost::parse_weather(&args.next().ok_or("--ghost-weather needs eight words")?)?
            }
            "--screenshot" => {
                let path = args.next().ok_or("--screenshot needs a path")?;
                screenshot = Some(PathBuf::from(path));
            }
            "--camera" => {
                let value = args.next().ok_or("--camera needs ex,ey,ez,fx,fy,fz")?;
                let numbers = value
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<Vec<f32>, _>>()
                    .map_err(|e| format!("--camera {value}: {e}"))?;
                let [ex, ey, ez, fx, fy, fz] = numbers[..] else {
                    return Err(format!("--camera {value}: needs six numbers"));
                };
                camera = Some(FreeFly::looking_along(Vec3::new(ex, ey, ez), Vec3::new(fx, fy, fz)));
            }
            "--size" => {
                let value = args.next().ok_or("--size needs WxH")?;
                let parsed = value.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
                size = Some(parsed.ok_or_else(|| format!("--size {value}: needs WxH"))?);
            }
            "--fov" => {
                let value = args.next().ok_or("--fov needs degrees")?;
                let range = coords::FOV_RANGE_DEGREES;
                let degrees = value.parse().ok().filter(|degrees| range.contains(degrees));
                fov = degrees
                    .ok_or_else(|| format!("--fov {value}: needs {} to {} degrees", range.start(), range.end()))?;
            }
            "--cameras" => cameras = Some(PathBuf::from(args.next().ok_or("--cameras needs a file")?)),
            "--view" => {
                let value = args.next().ok_or("--view needs a view name")?;
                view = Some(CarView::parse(&value).ok_or_else(|| {
                    let names: Vec<&str> = CarView::ALL.iter().map(|v| v.name()).collect();
                    format!("unknown view {value}; use {}", names.join(", "))
                })?);
            }
            "--car" => {
                let name = args.next().ok_or("--car needs a car name")?;
                car = Some(name);
            }
            "--rsf-car" => rsf_car = Some(args.next().ok_or("--rsf-car needs a pack or an id")?),
            "--drive" => {
                let path = args.next().ok_or("--drive needs a recording file")?;
                drive = Some(PathBuf::from(path));
            }
            "--live" => {
                live = Some(args.by_ref().collect());
            }
            "--bindings" => {
                let path = args.next().ok_or("--bindings needs a file")?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                bindings = Bindings::parse(&text);
                ffb_options = ffb::Options::parse(&text);
            }
            "--car-gearbox" => {
                let value = args.next().ok_or("--car-gearbox needs synchro or dog")?;
                car_gearbox =
                    Some(CarBox::parse_arg(&value).ok_or(format!("--car-gearbox {value}: synchro|dog[,dogleg]"))?);
            }
            "--car-lock" => {
                let value = args.next().ok_or("--car-lock needs degrees")?;
                car_lock = Some(value.parse().map_err(|_| format!("--car-lock {value}: degrees"))?);
            }
            "--grind-sound" => grind_sound = Some(PathBuf::from(args.next().ok_or("--grind-sound needs a WAV")?)),
            "--damage" => {
                let value = args.next().ok_or("--damage needs zone:value pairs")?;
                damage = Some(damage::parse_zones(&value).map_err(|e| format!("--damage: {e}"))?);
            }
            "--crack" => {
                let value = args.next().ok_or("--crack needs code:index pairs")?;
                cracks = damage::parse_events(&value).map_err(|e| format!("--crack: {e}"))?;
            }
            "--mirrors" => {
                let value = args.next().ok_or("--mirrors needs off, 1, 2 or 3")?;
                mirrors =
                    rbr_render::mirrors::Rate::parse(&value).ok_or(format!("--mirrors {value}: off, 1, 2 or 3"))?;
            }
            "--anti-aliasing" => {
                let value = args.next().ok_or("--anti-aliasing needs 4x or off")?;
                anti_aliasing = AntiAliasing::parse(&value).ok_or(format!("--anti-aliasing {value}: 4x or off"))?;
            }
            "--enhance" => {
                let value = args.next().ok_or("--enhance needs a list")?;
                enhance = Enhance::parse(&value)
                    .ok_or_else(|| format!("--enhance {value}: use all or {}", Enhance::NAMES.join(", ")))?;
            }
            "--no-relight" => relight = false,
            "--clock" => {
                let value = args.next().ok_or("--clock needs HH:MM")?;
                let hour = value
                    .split_once(':')
                    .and_then(|(h, m)| Some(h.parse::<f32>().ok()? + m.parse::<f32>().ok()? / 60.0))
                    .ok_or(format!("--clock {value}: use HH:MM"))?;
                clock = Some(hour);
            }
            "--clock-rate" => {
                let value = args.next().ok_or("--clock-rate needs a rate")?;
                clock_rate = value.parse().map_err(|e| format!("--clock-rate {value}: {e}"))?;
            }
            "--time" => {
                let value = args.next().ok_or("--time needs seconds")?;
                time = value.parse().map_err(|e| format!("--time {value}: {e}"))?;
            }
            "--render-scale" => {
                let value = args.next().ok_or("--render-scale needs a share")?;
                render_scale = match value.as_str() {
                    "auto" => 1.0,
                    _ => value
                        .parse()
                        .ok()
                        .filter(|s| (0.25..=1.0).contains(s))
                        .ok_or_else(|| format!("--render-scale {value}: use 0.25 to 1"))?,
                };
            }
            "--present" => {
                let value = args.next().ok_or("--present needs a mode")?;
                present = Present::parse(&value).ok_or(format!("--present {value}: use mailbox, fifo or immediate"))?;
            }
            "--max-fps" => {
                let value = args.next().ok_or("--max-fps needs a rate")?;
                max_fps = Some(
                    value
                        .parse()
                        .ok()
                        .filter(|r: &f32| *r >= 10.0)
                        .ok_or_else(|| format!("--max-fps {value}: use 10 or more"))?,
                );
            }
            "--frames" => {
                let value = args.next().ok_or("--frames needs a count")?;
                frames = value.parse().map_err(|e| format!("--frames {value}: {e}"))?;
            }
            "--profile" => profile = true,
            "--sequence" => {
                let value = args.next().ok_or("--sequence needs a count")?;
                sequence = value.parse().map_err(|e| format!("--sequence {value}: {e}"))?;
            }
            "--textures" => {
                let path = args.next().ok_or("--textures needs a pack folder")?;
                textures = Some(PathBuf::from(path));
            }
            "--no-sound" => sound = false,
            "--volume" => {
                let value = args.next().ok_or("--volume needs a number")?;
                volume = value
                    .parse()
                    .ok()
                    .filter(|v: &f32| (0.0..=4.0).contains(v))
                    .ok_or_else(|| format!("--volume {value}: needs 0 to 4"))?;
            }
            "--rain" => {
                let value = args.next().ok_or("--rain needs none, light or heavy")?;
                rain = match value.as_str() {
                    "none" => 0.0,
                    "light" => 50.0,
                    "heavy" => 100.0,
                    _ => return Err(format!("--rain {value}: none, light or heavy")),
                };
            }
            "--no-codriver" => codriver = false,
            "--codriver-mod" => codriver_mod = Some(PathBuf::from(args.next().ok_or("--codriver-mod needs a folder")?)),
            "--codriver-voice" => codriver_voice = Some(args.next().ok_or("--codriver-voice needs a name")?),
            "--codriver-style" => codriver_style = args.next().ok_or("--codriver-style needs a file")?,
            "--codriver-ranges" => codriver_ranges = args.next().ok_or("--codriver-ranges needs a file")?,
            "--pacenotes" => pacenotes = Some(PathBuf::from(args.next().ok_or("--pacenotes needs a file")?)),
            "--no-hud" => hud.show = false,
            "--mph" => hud.mph = true,
            "--fps" => hud.fps = true,
            "--pace-stack" => {
                let value = args.next().ok_or("--pace-stack needs 0 to 4")?;
                hud.stack = value.parse().ok().filter(|n| *n <= 4).ok_or(format!("--pace-stack {value}: 0 to 4"))?;
            }
            "--pace-3d" => hud.pace_3d = true,
            "--hud-hide" => hud.parts = Parts::hiding(&args.next().ok_or("--hud-hide needs a list of HUD parts")?)?,
            "--at-drive" => at_drive = Some(PathBuf::from(args.next().ok_or("--at-drive needs a recording")?)),
            "--at" => {
                let value = args.next().ok_or("--at needs a step counter")?;
                at = Some(value.parse().map_err(|e| format!("--at {value}: {e}"))?);
            }
            "--sky" => {
                let value = args.next().ok_or("--sky needs a TrackSettings.ini section")?;
                selection.sky =
                    Some(Sky::parse(&value).ok_or_else(|| format!("--sky {value}: not a weather section"))?);
            }
            "--fog" => {
                let value = args.next().ok_or("--fog needs off, light or heavy")?;
                fog = Some(Fog::parse(&value).ok_or_else(|| format!("--fog {value}: use off, light or heavy"))?);
            }
            "--weather" => {
                let value = args.next().ok_or("--weather needs dry, damp or wet")?;
                selection.weather =
                    Weather::parse(&value).ok_or_else(|| format!("unknown weather {value}; use dry, damp or wet"))?;
            }
            "--wear" => {
                let value = args.next().ok_or("--wear needs new, normal or worn")?;
                selection.wear =
                    Wear::parse(&value).ok_or_else(|| format!("unknown wear {value}; use new, normal or worn"))?;
            }
            "--birdstrike" => birdstrike = true,
            "--quit-after" => {
                let value = args.next().ok_or("--quit-after needs seconds")?;
                let (seconds, end) = value.strip_prefix("end+").map_or((value.as_str(), false), |s| (s, true));
                quit_after = Some((seconds.parse().map_err(|e| format!("--quit-after {value}: {e}"))?, end));
            }
            "--seed" => {
                let value = args.next().ok_or("--seed needs a number")?;
                seed = value.parse().map_err(|e| format!("--seed {value}: {e}"))?;
            }
            "--rescue-at" => {
                let value = args.next().ok_or("--rescue-at needs a step counter")?;
                rescue_at = Some(value.parse().map_err(|e| format!("--rescue-at {value}: {e}"))?);
            }
            option if option.starts_with("--") => return Err(format!("unknown option {option}\n{USAGE}")),
            _ => {
                if stage.replace(arg).is_some() {
                    return Err("more than one stage named".to_owned());
                }
            }
        }
    }
    let fov = fov.to_radians();
    // A stage folder (RallySimFans, `rbr_formats::rsf`): its default variant. One it cannot load (encrypted) says why.
    let stage = match stage {
        Some(dir) if Path::new(&dir).is_dir() => {
            let rsf = rbr_formats::rsf::open(Path::new(&dir))?;
            if let Some(problem) = &rsf.problem {
                return Err(format!("RSF stage {}: {problem}", rsf.id));
            }
            let letter = rsf.default_variant().ok_or(format!("{dir}: no stage variant"))?;
            Some(rsf.stage(letter))
        }
        stage => stage,
    };
    Ok(Args {
        stage,
        car,
        rsf_car,
        drive,
        at,
        at_drive,
        live,
        bindings,
        ffb: ffb_options,
        screenshot,
        camera,
        size,
        fov,
        cameras,
        selection,
        view,
        damage,
        cracks,
        textures,
        enhance: Enhance { mirrors, anti_aliasing, ..enhance },
        clock,
        clock_rate,
        relight,
        fog,
        render_scale,
        present,
        max_fps,
        fullscreen,
        title,
        particles,
        sound,
        volume,
        rain,
        codriver,
        codriver_mod: codriver_mod.map(|plugin| CodriverMod {
            plugin,
            voice: codriver_voice,
            style: codriver_style,
            ranges: codriver_ranges,
            notes: pacenotes,
        }),
        frames,
        profile,
        sequence,
        time,
        birdstrike,
        rescue_at,
        seed,
        quit_after,
        fences,
        dials,
        gauge_mode,
        wipers,
        look_back,
        pause_menu,
        hud,
        ghost,
        save_ghost,
        best_ghost,
        ghost_weather,
        rsf_ghosts,
        rsf_install: std::env::var_os("RBR_RSF").map(PathBuf::from),
        rsf_ghost_car: None,
        car_gearbox,
        grind_sound,
        car_lock,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Args, String> {
        parse_args(line.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn fov_is_horizontal_degrees_within_the_range() {
        assert_eq!(parse("track-54_E").unwrap().fov, 50.0f32.to_radians());
        assert_eq!(parse("track-54_E --fov 75 --car COROLLA").unwrap().fov, 75.0f32.to_radians());
        assert_eq!(parse("--fov 30").unwrap().fov, 30.0f32.to_radians());
        assert_eq!(parse("--fov 120").unwrap().fov, 120.0f32.to_radians());
        for bad in ["--fov 29.5", "--fov 121", "--fov wide", "--fov NaN", "--fov"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn present_defaults_to_mailbox_uncapped_windowed() {
        let args = parse("track-54_E").unwrap();
        assert_eq!((args.present, args.max_fps, args.fullscreen), (Present::Mailbox, None, false));
        assert!(parse("--fullscreen").unwrap().fullscreen);
        let args = parse("--present immediate --max-fps 144").unwrap();
        assert_eq!((args.present, args.max_fps), (Present::Immediate, Some(144.0)));
        for bad in ["--present vsync", "--present", "--max-fps 5", "--max-fps NaN", "--max-fps"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn anti_aliasing_defaults_to_4x() {
        assert_eq!(parse("track-54_E").unwrap().enhance.anti_aliasing, AntiAliasing::Msaa4);
        let args = parse("--anti-aliasing off --enhance all").unwrap();
        assert_eq!(args.enhance.anti_aliasing, AntiAliasing::Off);
        for bad in ["--anti-aliasing msaa", "--anti-aliasing"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn render_scale_is_a_share_or_auto() {
        assert_eq!(parse("track-54_E").unwrap().render_scale, 1.0);
        assert_eq!(parse("--render-scale 0.67").unwrap().render_scale, 0.67);
        // `auto`, the removed choice by the window's size, from older settings.
        assert_eq!(parse("--render-scale auto").unwrap().render_scale, 1.0);
        for bad in ["--render-scale 0.2", "--render-scale 1.5", "--render-scale", "--render-scale half"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn codriver_mod_takes_its_options() {
        assert_eq!(parse("track-54_E").unwrap().codriver_mod, None);
        let args = parse("--codriver-mod /p --codriver-voice a/b --codriver-style N.ini --pacenotes /n.ini").unwrap();
        let m = args.codriver_mod.unwrap();
        assert_eq!((m.plugin, m.voice.as_deref(), m.style.as_str()), (PathBuf::from("/p"), Some("a/b"), "N.ini"));
        assert_eq!((m.ranges.as_str(), m.notes), ("Rbr.ini", Some(PathBuf::from("/n.ini"))));
        for bad in ["--codriver-mod", "--codriver-mod /p --codriver-voice"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
