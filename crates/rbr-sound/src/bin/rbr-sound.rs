//! Renders a recorded drive's sound to a WAV, offline: the recording's control payloads go through
//! `rbr-standalone --live --boot` (as the viewer's live drive does), and each step's SND1 and PCN1 records and the
//! HUD's countdown through the models and the mixer, one physics step (7 ms) of samples per step. At the end it
//! prints each one-shot kind's made and refused requests under the sound limit (`rbr_sound::mixer::SOUND_LIMIT`).
//!
//! Usage: rbr-sound --recording X [--stage NAME] --out WAV [--seconds S] [--camera inside|chase|trackside]
//!        [--no-codriver] [--volume V] [--volumes E,C,F,T] [--standalone PATH] [--log CSV] [--from S] [--events CSV] [--car N]
//!        --ngp-physics FILE [--rsf-car PACK|ID [--no-fmod]] [--solo LAYER]
//!        [--codriver-mod DIR [--codriver-voice V] [--codriver-style S] [--codriver-ranges R] [--pacenotes FILE]]
//!
//! The car, weather and tyre are the recording's `.auto` (`map car weather tyre`); the weather preset gives the
//! season row's rain, cloud and stage (the default `--stage`) as the standalone picks it (no `--season-row`). The WAV is stereo. `--camera` is where
//! the listener is (ours): `inside` the car (`--inside` is the same), `chase` 6 m behind and 2 m above it (the
//! default), or `trackside`, standing for the replay's TV cameras: a camera 8 m beside the road 50 m ahead of the
//! car, moved on once the car is 70 m past it, which plays the replay's birds, far wind and the `.dls` siren
//! spheres. `--no-codriver` turns the pace notes off. `--volumes` gives the player's volume options for the engine, the co-driver,
//! the effects and the tyres (`rbr_sound::mixer::Volumes`; default 1 each). `--log` writes each step's voice commands as CSV (step, rpm after the slew, load, then the gain and pitch
//! of the gearbox, diff, turbo, wind and siren loops, the summed tyre, rumble and wetness gains, the first wheel's surface id, and the one-shots, effects (`shot:gain@pitch`) and speech). `--from` starts the WAV at
//! second S of the drive (the steps before it still run). `--events` plays the scripted events of a CSV
//! (`frame,kind,x,y,z`; kind `heli`, `bird` or `call`, the frame the recording's, the point in the physics frame), as
//! `rbr-render`'s `scripted_events` example writes them. `--standalone` is the stepping program (default: `rbr-standalone` next to this one). `--ngp-physics` (required) drives
//! under NGP's physics (`rbr-standalone --ngp-physics`; the viewer's 7 ms frames, so a step of sound is still 7 ms)
//! and `--car` replaces the `.auto`'s car, for NGP's own cars. `--rsf-car` drives an RSF car under NGP
//! (`rbr-standalone --rsf-car`) in slot 5 with its FMOD bank's sound (`rbr_sound::fmod`; `RBR_RSF`'s
//! `AudioFMOD/`), or without one (or with `--no-fmod`) slot 5's retail sound, as the viewer plays it. `--codriver-mod`
//! calls the notes with a co-driver mod (`rbr_sound::custom_codriver`), its Pacenote plugin folder: the voice (default
//! `PaceNote.ini`'s), the style and range set when the notes name none (default `Rbr.ini`), and the stage's note file
//! (default the stage's `.dls` keys), at the profile's default callout distance. `RBR_DATA` and `RBR_PATCH` are
//! passed on.

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Stdio};

use rbr_formats::data::Data;
use rbr_gameplay::weather::{self, Draws, Preset, Season};
use rbr_recording::damage::Damage;
use rbr_recording::hud::Hud;
use rbr_recording::live::read_records;
use rbr_recording::pose::{car_pose, CarPose};
use rbr_recording::realtime;
use rbr_recording::sound::Inputs;
use rbr_recording::{Frames, CONTROL_BYTES, HEADER_BYTES, WHEELS, WHEEL_BYTES};
use rbr_sound::data::CarBank;
use rbr_sound::mixer::Mixer;
use rbr_sound::voice::{Command, Loop, WheelLoop};
use rbr_sound::{Events, Models, Scene, Sky, Sphere};
use std::collections::HashMap;

/// The boot's load settle: the recording's first frames, which `--live --boot` runs before its first payload
/// (`rbr_gameplay::tick::SETTLE_SUBSTEPS`).
const SETTLE_FRAMES: usize = 4;
const RATE: u32 = 44100;
/// Steps per second of the drive: 1 / `MaxTimeStep`.
const STEPS_PER_SECOND: f64 = 1.0 / rbr_recording::STEP_SECONDS as f64;

fn main() {
    if let Err(e) = run() {
        eprintln!("rbr-sound: {e}");
        process::exit(1);
    }
}

#[derive(Default)]
struct Args {
    recording: PathBuf,
    stage: Option<String>,
    out: PathBuf,
    seconds: f32,
    camera: Camera,
    codriver: bool,
    volume: f32,
    standalone: PathBuf,
    log: Option<PathBuf>,
    from: f32,
    events: Option<PathBuf>,
    car: Option<String>,
    ngp_physics: PathBuf,
    rsf_car: Option<String>,
    fmod: bool,
    solo: Option<usize>,
    volumes: rbr_sound::mixer::Volumes,
    codriver_mod: Option<CodriverMod>,
}

/// `--codriver-mod` and its options.
struct CodriverMod {
    plugin: PathBuf,
    voice: Option<String>,
    style: String,
    ranges: String,
    notes: Option<PathBuf>,
}

fn args() -> Result<Args, String> {
    parse(std::env::args().skip(1))
}

/// `args()` of an argument list.
fn parse(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    // The command line so far; the required paths, the standalone and the co-driver mod are computed below.
    let mut args =
        Args { seconds: 60.0, camera: Camera::Chase, codriver: true, volume: 1.0, fmod: true, ..Args::default() };
    let (mut recording, mut out, mut ngp_physics, mut standalone) = (None, None, None, None);
    let (mut plugin, mut voice, mut pacenotes) = (None, None, None);
    let mut style = rbr_sound::custom_codriver::DEFAULT_STYLE.to_owned();
    let mut ranges = rbr_sound::custom_codriver::DEFAULT_RANGES.to_owned();
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--recording" => recording = Some(PathBuf::from(value()?)),
            "--stage" => args.stage = Some(value()?),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--seconds" => args.seconds = value()?.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--volume" => args.volume = value()?.parse().map_err(|e| format!("--volume: {e}"))?,
            "--volumes" => args.volumes = rbr_sound::mixer::Volumes::parse(&value()?)?,
            "--standalone" => standalone = Some(PathBuf::from(value()?)),
            "--inside" => args.camera = Camera::Inside,
            "--camera" => {
                args.camera = match value()?.as_str() {
                    "inside" => Camera::Inside,
                    "chase" => Camera::Chase,
                    "trackside" => Camera::Trackside { at: None, id: 0 },
                    other => return Err(format!("--camera {other}: inside, chase or trackside")),
                }
            }
            "--no-codriver" => args.codriver = false,
            "--log" => args.log = Some(PathBuf::from(value()?)),
            "--events" => args.events = Some(PathBuf::from(value()?)),
            "--car" => args.car = Some(value()?),
            "--ngp-physics" => ngp_physics = Some(PathBuf::from(value()?)),
            "--rsf-car" => args.rsf_car = Some(value()?),
            "--no-fmod" => args.fmod = false,
            "--solo" => {
                let name = value()?;
                args.solo = Some(
                    rbr_sound::mixer::LAYERS
                        .iter()
                        .position(|&l| l == name)
                        .ok_or_else(|| format!("--solo {name}: one of {}", rbr_sound::mixer::LAYERS.join(", ")))?,
                )
            }
            "--from" => args.from = value()?.parse().map_err(|e| format!("--from: {e}"))?,
            "--codriver-mod" => plugin = Some(PathBuf::from(value()?)),
            "--codriver-voice" => voice = Some(value()?),
            "--codriver-style" => style = value()?,
            "--codriver-ranges" => ranges = value()?,
            "--pacenotes" => pacenotes = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    args.recording = recording.ok_or("--recording is required")?;
    args.out = out.ok_or("--out is required")?;
    args.ngp_physics = ngp_physics.ok_or("--ngp-physics is required (ReRBR steps NGP physics only)")?;
    args.standalone = match standalone {
        Some(path) => path,
        None => std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name(format!("rbr-standalone{}", std::env::consts::EXE_SUFFIX)),
    };
    if args.rsf_car.is_some() {
        args.car = Some(rbr_formats::rsf::SLOT.to_string());
    }
    args.codriver_mod = plugin.map(|plugin| CodriverMod { plugin, voice, style, ranges, notes: pacenotes });
    Ok(args)
}

/// The scripted events of an `--events` CSV, by recording frame.
fn read_events(path: &Path) -> Result<HashMap<usize, Events>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut events: HashMap<usize, Events> = HashMap::new();
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split(',').collect();
        let [frame, kind, x, y, z] = cells[..] else { return Err(format!("{}: bad line {line}", path.display())) };
        let number = |s: &str| s.parse::<f32>().map_err(|e| format!("{line}: {e}"));
        let at = [number(x)?, number(y)?, number(z)?];
        let entry = events.entry(frame.parse().map_err(|e| format!("{line}: {e}"))?).or_default();
        match kind {
            "heli" => entry.helicopter = Some(at),
            "bird" => entry.birdstrikes.push(at),
            "call" => entry.rescue_called = true,
            other => return Err(format!("{}: unknown event {other}", path.display())),
        }
    }
    Ok(events)
}

/// The `.auto`'s map, car, weather and tyre words.
fn auto(recording: &Path) -> Result<[String; 4], String> {
    let path = recording.with_extension("auto");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let words: Vec<&str> = text.split_whitespace().collect();
    match words[..] {
        [map, car, weather, tyre, ..] => Ok([map, car, weather, tyre].map(str::to_owned)),
        _ => Err(format!("{}: needs map car weather tyre", path.display())),
    }
}

/// The rain and cloud of the season row the standalone picks for weather preset `preset` on `map`, and its stage.
fn sky(data: &Data, map: u32, preset: u32) -> Result<(Sky, String), String> {
    let preset = Preset::from_id(preset).ok_or(format!("weather {preset}: 0 good, 1 random or 2 bad"))?;
    let season = Season::load(data)?;
    let draws = Draws { row: None, wear: u32::from(preset == Preset::Random) };
    let weather = weather::quick_race(&season, map, preset, draws)?;
    let sky = weather.row.map(|k| season.rows[k]).map_or(Sky::default(), |r| Sky::from_weather(r.preset, r.cloud));
    Ok((sky, weather.stage()))
}

/// The stage's `.dls` table-6 spheres (none when the file is missing).
fn spheres(data: &Data, stage: &str) -> Result<Vec<Sphere>, String> {
    let Some(path) = data.resolve(&format!("Maps/{stage}.dls")) else { return Ok(Vec::new()) };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dls = rbr_formats::dls::parse(&bytes).map_err(|e| format!("{stage}.dls: {e}"))?;
    Ok(dls.spheres.iter().map(|s| Sphere { center: s.center, radius: s.radius }).collect())
}

/// Where the listener is (ours; see the module docs).
#[derive(Debug, Clone, Copy, Default)]
enum Camera {
    Inside,
    #[default]
    Chase,
    /// The trackside camera's position and id.
    Trackside {
        at: Option<[f32; 3]>,
        id: u32,
    },
}

const UP: [f32; 3] = [0.0, 0.0, 1.0];
const TRACKSIDE_AHEAD: f32 = 50.0;
const TRACKSIDE_SIDE: f32 = 8.0;
const TRACKSIDE_PAST: f32 = 70.0;

fn add(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// `forward x up`, flattened and unit (the physics frame is right-handed, z up).
fn right_of(forward: [f32; 3]) -> [f32; 3] {
    let r = [forward[1] * UP[2] - forward[2] * UP[1], forward[2] * UP[0] - forward[0] * UP[2], 0.0];
    let n = (r[0] * r[0] + r[1] * r[1]).sqrt().max(1e-6);
    [r[0] / n, r[1] / n, 0.0]
}

impl Camera {
    /// The scene for the car at `pose`, moving the trackside camera on when the car has passed it.
    fn scene(&mut self, pose: &CarPose) -> Scene {
        let forward = pose.forward();
        let base = Scene { car: pose.position, velocity: pose.velocity, ..Scene::default() };
        match self {
            Camera::Inside => Scene { listener: pose.position, right: right_of(forward), ..base },
            Camera::Chase => Scene {
                listener: add(add(pose.position, forward, -6.0), UP, 2.0),
                right: right_of(forward),
                inside: false,
                ..base
            },
            Camera::Trackside { at, id } => {
                let far = |p: [f32; 3]| {
                    let d = add(pose.position, p, -1.0);
                    let behind = d[0] * forward[0] + d[1] * forward[1] + d[2] * forward[2] > 0.0;
                    behind && (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() > TRACKSIDE_PAST
                };
                if at.is_none_or(far) {
                    let ahead = add(pose.position, forward, TRACKSIDE_AHEAD);
                    *at = Some(add(add(ahead, right_of(forward), TRACKSIDE_SIDE), UP, 2.0));
                    *id += 1;
                }
                let listener = at.unwrap_or(pose.position);
                let look = add(pose.position, listener, -1.0);
                Scene {
                    listener,
                    right: right_of(look),
                    inside: false,
                    replay: true,
                    camera: *id,
                    tracking: false,
                    ..base
                }
            }
        }
    }
}

/// One answer's sound inputs, car pose and physics substeps (0 for a frame of the countdown).
type Answer = (Option<Inputs>, Option<CarPose>, u32);

/// Reads one answer of the live process and returns its sound inputs, the car's pose and its substeps, or `None` at
/// the end of the stream.
fn read_answer(reader: &mut impl Read) -> Result<Option<Answer>, String> {
    let mut header = [0u8; HEADER_BYTES];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.to_string()),
    }
    let substeps = u32::from_le_bytes(header[12..16].try_into().unwrap());
    let car_bytes = u32::from_le_bytes(header[16..20].try_into().unwrap()) as usize;
    let mut frame = vec![0u8; CONTROL_BYTES + car_bytes + WHEELS * WHEEL_BYTES];
    reader.read_exact(&mut frame).map_err(|e| e.to_string())?;
    let car = &frame[CONTROL_BYTES..CONTROL_BYTES + car_bytes];
    let hud = Hud::read(reader).map_err(|e| format!("HUD record: {e}"))?;
    Damage::read(reader).map_err(|e| format!("damage record: {e}"))?;
    let records = read_records(reader).map_err(|e| format!("records: {e}"))?;
    let mut count = [0u8; 4];
    reader.read_exact(&mut count).map_err(|e| e.to_string())?;
    let count = u32::from_le_bytes(count) as usize;
    std::io::copy(&mut reader.by_ref().take(32 * count as u64), &mut std::io::sink()).map_err(|e| e.to_string())?;
    Ok(Some((Inputs::from_records(&records, &hud), car_pose(car), substeps)))
}

/// The load and the loops' and one-shots' columns of a `--log` line.
fn log_line(inputs: &Inputs, commands: &[Command]) -> String {
    let load = rbr_sound::engine::EngineNoise::from_payload(&inputs.sound.engine, inputs.ngp.min_rpm).load;
    let mut cells = vec![load.to_string()];
    for voice in [Loop::Gearbox, Loop::Diff, Loop::Turbo, Loop::Wind, Loop::Siren] {
        let (gain, pitch) = commands
            .iter()
            .find_map(|c| match *c {
                Command::Loop { voice: v, gain, pitch } if v == voice => Some((gain, pitch)),
                _ => None,
            })
            .unwrap_or_default();
        cells.extend([gain.to_string(), pitch.to_string()]);
    }
    let tyres: f32 = commands
        .iter()
        .map(|c| match c {
            Command::Tyre { gain, .. } => *gain,
            _ => 0.0,
        })
        .sum();
    cells.push(tyres.to_string());
    for voice in [WheelLoop::Rumble, WheelLoop::Wetness] {
        let sum: f32 = commands
            .iter()
            .map(|c| match *c {
                Command::WheelLoop { voice: v, gain, .. } if v == voice => gain,
                _ => 0.0,
            })
            .sum();
        cells.push(sum.to_string());
    }
    cells.push(inputs.sound.tyres[0][0x14].to_string());
    let shots: Vec<String> = commands
        .iter()
        .filter_map(|c| match c {
            Command::Shot { shot, gain } => Some(format!("{shot:?}:{gain}")),
            Command::Effect { shot, gain, pitch, .. } | Command::Hit { shot, gain, pitch, .. } => {
                Some(format!("{shot:?}:{gain}@{pitch}"))
            }
            Command::Speech(speech) => Some(format!("{speech:?}")),
            Command::StopSpeech => Some("StopSpeech".to_owned()),
            _ => None,
        })
        .collect();
    cells.push(shots.join(" "));
    cells.join(",")
}

fn run() -> Result<(), String> {
    let args = args()?;
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let [map, car, weather, tyre] = auto(&args.recording)?;
    let car = args.car.clone().unwrap_or(car);
    let car_index: u32 = car.parse().map_err(|e| format!("car {car}: {e}"))?;
    let map: u32 = map.parse().map_err(|e| format!("map {map}: {e}"))?;
    let preset: u32 = weather.parse().map_err(|e| format!("weather {weather}: {e}"))?;
    let (sky, weather_stage) = sky(&data, map, preset)?;
    let stage = args.stage.clone().unwrap_or(weather_stage);
    eprintln!("rbr-sound: {stage}, rain {}, cloud {}", sky.rain, sky.cloud);
    let bytes = std::fs::read(&args.recording).map_err(|e| format!("{}: {e}", args.recording.display()))?;
    let steps = (f64::from(args.seconds) * STEPS_PER_SECOND) as usize;
    let payloads: Vec<[u8; CONTROL_BYTES]> = Frames::new(&bytes)
        .skip(SETTLE_FRAMES)
        .map(|frame| frame.map(|f| f.control).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    drop(bytes);

    let bank = CarBank::load(&data, car_index)?;
    let mut models = Models::new(&bank, sky, spheres(&data, &stage)?);
    models.codriver.muted = !args.codriver;
    if let Some(m) = &args.codriver_mod {
        let voice = m.voice.clone().or_else(|| rbr_formats::rsf::codriver::settings(&m.plugin).voice);
        let choice = rbr_sound::custom_codriver::Choice {
            plugin: m.plugin.clone(),
            voice: voice.ok_or("--codriver-mod: no voice (PaceNote.ini names none)")?,
            style: m.style.clone(),
            ranges: m.ranges.clone(),
            notes: m.notes.clone(),
        };
        let dls =
            if choice.notes.is_none() { rbr_sound::custom_codriver::stage_notes(&data, &stage)? } else { Vec::new() };
        let callout_time = rbr_gameplay::markers::MarkerSettings::default().callout_distance;
        let mut custom = rbr_sound::custom_codriver::CustomCoDriver::load(&choice, dls, callout_time)?;
        custom.muted = !args.codriver;
        eprintln!("rbr-sound: co-driver {} ({} samples)", choice.voice, custom.samples().len());
        models.custom = Some(custom);
    }
    let mut camera = args.camera;
    let mut scene = Scene::default();
    let mut mixer = Mixer::new(&bank, RATE, args.volume);
    mixer.solo = args.solo;
    mixer.volumes = args.volumes;
    if let Some(custom) = &models.custom {
        mixer.set_voice(custom.samples());
    }
    if let Some(key) = args.rsf_car.as_ref().filter(|_| args.fmod) {
        let root = std::env::var_os("RBR_RSF").map(PathBuf::from);
        let (install, car, _) = rbr_formats::rsf::locate(key, root.as_deref())?;
        if let Some(path) = install.audio_bank(&car) {
            let fmod = rbr_sound::fmod::FmodCar::load(&path)?;
            eprintln!("rbr-sound: {} ({} slots, {} recordings)", path.display(), fmod.slots.len(), fmod.waves.len());
            mixer.set_fmod(fmod.loop_waves(), fmod.waves.clone(), &fmod.buses);
            models.fmod = Some(fmod);
        }
    }

    let mut command = process::Command::new(&args.standalone);
    command.args(["--live", "--boot", "--stage", &stage, "--car", &car, "--weather", &weather, "--tyre", &tyre]);
    command.args(["--frames", "-"]);
    command.arg("--ngp-physics").arg(&args.ngp_physics);
    if let Some(key) = &args.rsf_car {
        command.args(["--rsf-car", key]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", args.standalone.display()))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    stdin.write_all(&realtime::LOG_MAGIC).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(child.stdout.take().ok_or("no stdout")?);
    let spec =
        hound::WavSpec { channels: 2, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut wav = hound::WavWriter::create(&args.out, spec).map_err(|e| e.to_string())?;
    // A step is 308.7 samples: each block ends at its step's end, rounded, so the blocks keep to the drive's time.
    let sample_at = |step: usize| (step as f64 * f64::from(RATE) / STEPS_PER_SECOND).round() as usize;
    let mut block = Vec::new();
    let mut answered = 0;
    let mut peak = 0.0f32;
    let (mut busy, mut rendered) = (std::time::Duration::ZERO, 0usize);
    let mut log = match &args.log {
        Some(path) => {
            let mut file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
            writeln!(
                file,
                "step,rpm,load,gearbox_gain,gearbox_pitch,diff_gain,diff_pitch,turbo_gain,turbo_pitch,wind_gain,wind_pitch,\
                 siren_gain,siren_pitch,tyres,rumble,wetness,surface,shots"
            )
            .map_err(|e| e.to_string())?;
            Some(std::io::BufWriter::new(file))
        }
        None => None,
    };
    let from = (f64::from(args.from) * STEPS_PER_SECOND) as usize;
    let events = args.events.as_deref().map(read_events).transpose()?.unwrap_or_default();
    // One frame per answer (`--frames -`, a frame a step): the live boot's countdown frames run no step (`rbr-standalone`'s `live`), so each
    // recorded frame's control goes to the frame that steps it, as the recording's (its frames step one each).
    let mut driven = 0;
    loop {
        if answered == steps {
            break;
        }
        let Some(payload) = payloads.get(driven) else { break };
        stdin
            .write_all(&realtime::Logged::unchecked(*payload, 1).to_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| e.to_string())?;
        let Some((inputs, pose, substeps)) = read_answer(&mut reader)? else { break };
        let frame = driven + SETTLE_FRAMES;
        if substeps > 0 {
            driven += 1;
        }
        if let Some(pose) = pose {
            scene = camera.scene(&pose);
        }
        if let Some(inputs) = inputs {
            if answered == 0 {
                eprintln!("rbr-sound: NGP's sound words {:?}", inputs.ngp);
            }
            let commands = models.step(&inputs, &scene);
            mixer.apply(&commands);
            if !events.is_empty() {
                let none = Events::default();
                mixer.apply(&models.events(events.get(&frame).unwrap_or(&none), &scene));
            }
            if let Some(log) = &mut log {
                writeln!(log, "{answered},{},{}", mixer.rpm(), log_line(&inputs, &commands))
                    .map_err(|e| e.to_string())?;
            }
        }
        block.resize(sample_at(answered + 1) - sample_at(answered), [0.0; 2]);
        let started = std::time::Instant::now();
        mixer.render(&mut block);
        busy += started.elapsed();
        rendered += block.len();
        if answered >= from {
            for &s in block.iter().flatten() {
                peak = peak.max(s.abs());
                wav.write_sample((s * 32767.0) as i16).map_err(|e| e.to_string())?;
            }
        }
        answered += 1;
        if answered % 1000 == 0 {
            eprintln!("rbr-sound: step {answered}, rpm {}", mixer.rpm());
        }
    }
    wav.finalize().map_err(|e| e.to_string())?;
    drop(stdin);
    let status = child.wait().map_err(|e| e.to_string())?;
    eprintln!("rbr-sound: {answered} steps, peak {peak:.3}, {} ({status})", args.out.display());
    for (kind, [made, refused]) in mixer.tally() {
        eprintln!("rbr-sound: {kind} made {made} refused {refused}");
    }
    let seconds = rendered as f64 / f64::from(RATE);
    eprintln!("rbr-sound: mixer {:.2} ms per second of audio", busy.as_secs_f64() * 1000.0 / seconds.max(1e-9));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `parse` of a whitespace-separated command line (no value holds a space).
    fn run(argv: &str) -> Result<Args, String> {
        parse(argv.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn good_command_lines_parse() {
        let args = run("--recording R --out O --ngp-physics N").unwrap();
        assert_eq!(args.recording, PathBuf::from("R"));
        assert_eq!((args.stage, args.log, args.events), (None, None, None));
        assert_eq!(args.out, PathBuf::from("O"));
        assert_eq!((args.seconds, args.volume, args.from), (60.0, 1.0, 0.0));
        assert!(matches!(args.camera, Camera::Chase));
        assert!(args.codriver && args.fmod);
        assert_eq!((args.solo, args.car, args.rsf_car), (None, None, None));
        assert_eq!(args.volumes, rbr_sound::mixer::Volumes::default());
        assert!(args.codriver_mod.is_none());
        assert_eq!(
            args.standalone.file_name().unwrap(),
            format!("rbr-standalone{}", std::env::consts::EXE_SUFFIX).as_str()
        );
        let args = run("--recording R --stage S --out O --seconds 10 --inside --no-codriver --volume 0.5 \
                         --volumes 0.5,1,2,0 --standalone BIN --log L --from 5 --events E --car 5 \
                         --ngp-physics N --solo engine --codriver-mod P --codriver-voice V \
                         --codriver-style S --codriver-ranges R --pacenotes PN")
        .unwrap();
        assert_eq!(args.stage.as_deref(), Some("S"));
        assert_eq!((args.seconds, args.volume, args.from), (10.0, 0.5, 5.0));
        assert!(matches!(args.camera, Camera::Inside));
        assert!(!args.codriver);
        assert_eq!(args.volumes, rbr_sound::mixer::Volumes { engine: 0.5, codriver: 1.0, effects: 2.0, tyres: 0.0 });
        assert_eq!((args.standalone, args.car), (PathBuf::from("BIN"), Some("5".to_owned())));
        assert_eq!((args.log, args.events), (Some(PathBuf::from("L")), Some(PathBuf::from("E"))));
        assert_eq!(args.solo, Some(0));
        let custom = args.codriver_mod.unwrap();
        assert_eq!(
            (custom.plugin, custom.voice, custom.style, custom.ranges, custom.notes),
            (PathBuf::from("P"), Some("V".to_owned()), "S".to_owned(), "R".to_owned(), Some(PathBuf::from("PN")))
        );
        // An RSF car runs in its slot, and `--camera` picks the listener.
        let args = run("--recording R --out O --ngp-physics N --rsf-car PACK --car 2 --camera chase").unwrap();
        assert_eq!((args.car, args.rsf_car), (Some("5".to_owned()), Some("PACK".to_owned())));
        assert!(matches!(args.camera, Camera::Chase));
        let args = run("--recording R --out O --ngp-physics N --camera trackside --no-fmod").unwrap();
        assert!(matches!(args.camera, Camera::Trackside { .. }) && !args.fmod);
    }

    #[test]
    fn bad_command_lines_fail() {
        let bad: Vec<(&str, &str)> = vec![
            ("", "--recording is required"),
            ("--recording", "--recording needs a value"),
            ("--recording R", "--out is required"),
            ("--recording R --out O", "--ngp-physics is required (ReRBR steps NGP physics only)"),
            ("--recording R --out O --ngp-physics N --warp x", "unknown argument --warp"),
            ("--recording R --out O --ngp-physics N --camera warp", "--camera warp: inside, chase or trackside"),
            ("--recording R --out O --ngp-physics N --volumes 1,1,1", "--volumes 1,1,1: engine,codriver,effects,tyres"),
            (
                "--recording R --out O --ngp-physics N --solo warp",
                "--solo warp: one of engine, reflections, loops, tyres, wheel, fmod, rain, shots, speech",
            ),
        ];
        for (argv, want) in &bad {
            match run(argv) {
                Err(e) => assert_eq!(&e, want, "{argv}"),
                Ok(_) => panic!("{argv}: parsed, want error {want}"),
            }
        }
    }
}
