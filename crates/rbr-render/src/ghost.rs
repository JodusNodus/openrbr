//! The ghost car (`viewer --ghost`): a recorded run (`rbr_formats::ghost`) played back by `rbr_gameplay::ghost::Player`
//! on the car's ghost mesh, drawn the way `CShaderGhostCar` draws it, and the recording of a drive (`--save-ghost`).
//!
//! Faithful:
//! - the mesh: the car's `ghost_*` part, one `GhostCar` mesh per car (`car::load_ghost`, `E09_LoadGhostCar`);
//! - the look: `GhostCar.vso` (`m4x4 oPos, v0, c0; mov oD0, c93`, no pixel shader) gives every vertex the constant
//!   `c93`, which `CShaderGhostCar::SetObject` (HH 0x1007f000) sets to the `Ambient` of `Cars/GhostCarSettings.ini`
//!   and the alpha `min(d² / FadeDistance, MaxOpacity)`, `d` the camera's distance to the car's origin; the shader is
//!   transparent (`IsTransparent`). So the ghost is a flat, translucent silhouette that fades out as the camera
//!   comes within about 8 m of it;
//! - the pose: the stream played at the race time (`Player`);
//! - the placement: the pose is the car model's root, not the body's centre of gravity. `CCar::Vf28x` (0x4ea020) sends
//!   `CSigCarGeneralState` with the world point of the model origin (`Xform8130` of zero: the body position plus the
//!   rotated `-vecCenterOfGravity`, (0, 1.37, -0.22) on the Corolla), the recorder keeps it, and
//!   `CLM_ControllerManager::OnGhostCarState` (LM 0x100377b0) sets the `ghost_parent` node's matrix to the pose. Every
//!   retail car's `ghost_parent` sits where its `parent` root does, and `ghost_external` is placed under it as
//!   `e_body` is under `parent`, so the ghost is drawn at the pose with no offset ([`playback::root_to_stage`]). The
//!   original's own `.ghc` (`golden-tanner-corolla-11000.ghc`, `tests/playback_data.rs`) holds these points.
//!
//! Ours:
//! - the race time: the game starts the ghost clock (`Start`/`Select` in `InitQuickRace`) and counts it in state 1
//!   only (`Accumulate`), which the recordings do not mark. The stage peek (`rbr-gameplay` README, Stage) shows state 1
//!   from the first step after the four load-time settle calls (`tick::SETTLE_SUBSTEPS`, which spend the load
//!   countdown), so the viewer takes the drive's step counter less those 160 steps (NGP's 38 settle calls: 1520 steps
//!   of 1/720 s, the live run's `rbr_recording::clock`). The race clock's 7 s start
//!   countdown runs from there; the `rb_33` ghost leaves the line at 7.32 s;
//! - the recorded root: the drive's body pose moved by the player car's model offset (the wheel fit of [`Playback`],
//!   within 3 cm of `-vecCenterOfGravity` on the Corolla), not the car's `vecCenterOfGravity`;
//! - the vertex alpha travels in the texture: the renderer's alpha is the texel's, so the ghost samples a 16-texel
//!   alpha ramp at the fade's coordinate; the draw has no depth write and culls back faces (the game's render state
//!   block is not traced);
//! - a recorded drive is fed one recording frame (one `StepForward`) per game frame, NGP's one-step frames grouped
//!   into frames of 1/60 s ([`record`]).
//!
//! The best ghost (`--best-ghost FILE`, the launcher's "Ghost: best"): the game keeps the ghost to race in memory, the
//! quick race's last finished drive on the map unless the stored one is faster (`BeginWrite`,
//! `rbr_gameplay::ghost::replaces`), and writes `./GhostCars/<name>_<map:02>.ghc` only when the player saves it. Ours:
//! the viewer keeps that ghost in the file, plays it when its map is the stage's, and replaces it when a live drive
//! finishes faster. The file holds no race time, so the times compared are the streams' last sample times (the
//! recording ends at the finish line, whose step the live split record gives). `--ghost-weather` gives the weather
//! words a written `.ghc` keeps (`rbr_gameplay::weather::Weather::ghost_words`).
//!
//! RSF ghosts (`.gch` files) are not supported: every race uses the game's own `.ghc` ghosts above.

use crate::camera;
use crate::car;
use crate::playback::{self, Playback, Sample};
use crate::renderer::{mesh_bounds, Renderer};
use crate::stage::{Address, DrawState, Image, Layer, Stage, StageTexture, TextureRef, TextureSet, Vertex};
use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_formats::ghost::Weather;
use rbr_formats::ghost::{Ghost, SavedGhost, Stream};
use rbr_formats::ini;
use rbr_gameplay::ghost::{Best, Player, Pose, Recorder};
use rbr_recording::clock::StepClock;
use rbr_recording::pose::CarPose;
use rbr_recording::splits::Splits;
use std::path::Path;

/// `Cars/GhostCarSettings.ini`, `[Settings]` (`CShaderGhostCar::Load`, HH 0x1007ecb0), with the loader's defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub ambient: [f32; 3],
    pub fade_distance: f32,
    pub max_opacity: f32,
}

impl Default for Look {
    fn default() -> Look {
        Look { ambient: [0.5; 3], fade_distance: 100.0, max_opacity: 0.7 }
    }
}

impl Look {
    pub fn parse(text: &str) -> Look {
        let mut look = Look::default();
        let Ok(sections) = ini::parse(text) else {
            return look;
        };
        let Some(settings) = sections.iter().find(|s| s.name == "Settings") else {
            return look;
        };
        let numbers = |key| -> Vec<f32> {
            settings.get(key).map_or(Vec::new(), |v| v.split_whitespace().filter_map(|n| n.parse().ok()).collect())
        };
        if let [r, g, b] = numbers("Ambient")[..] {
            look.ambient = [r, g, b];
        }
        if let [d] = numbers("FadeDistance")[..] {
            look.fade_distance = d;
        }
        if let [o] = numbers("MaxOpacity")[..] {
            look.max_opacity = o;
        }
        look
    }

    pub fn load(data: &Data) -> Look {
        data.resolve("Cars\\GhostCarSettings.ini")
            .and_then(|path| std::fs::read(path).ok())
            .map_or_else(Look::default, |bytes| Look::parse(&String::from_utf8_lossy(&bytes)))
    }

    /// The ghost's opacity with the camera `distance` m from its origin.
    pub fn opacity(&self, distance: f32) -> f32 {
        (distance * distance / self.fade_distance).min(self.max_opacity)
    }
}

/// Texels of the alpha ramp: texel `i` has the alpha `i / 15`.
const RAMP_TEXELS: u32 = 16;

/// The texture coordinate where the ramp's filtered alpha is `alpha`.
fn ramp_u(alpha: f32) -> f32 {
    (alpha.clamp(0.0, 1.0) * (RAMP_TEXELS - 1) as f32 + 0.5) / RAMP_TEXELS as f32
}

/// A 16x4 DXT3 texture, white, whose alpha rises from 0 to 1 across its 16 columns.
fn ramp_dds() -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[..4].copy_from_slice(b"DDS ");
    let put = |bytes: &mut Vec<u8>, at: usize, v: u32| bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut bytes, 4, 124);
    put(&mut bytes, 12, 4);
    put(&mut bytes, 16, RAMP_TEXELS);
    put(&mut bytes, 28, 1);
    put(&mut bytes, 76, 32);
    put(&mut bytes, 80, 4);
    bytes[84..88].copy_from_slice(b"DXT3");
    for block in 0..RAMP_TEXELS / 4 {
        // Explicit 4-bit alpha, row by row, the first texel in the low nibble; then a white colour block.
        let row = (0..4).fold(0u16, |row, i| row | ((block * 4 + i) as u16) << (4 * i));
        for _ in 0..4 {
            bytes.extend_from_slice(&row.to_le_bytes());
        }
        bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
    }
    bytes
}

/// Reads a ghost file, a `.ghc` (the game's save) or a `.gho` (the shipped format, also what `--save-ghost` writes):
/// its map, its car's `Cars.ini` index and its stream.
pub fn read(path: &Path) -> Result<(u32, u32, Stream), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ghost = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ghc")) {
        SavedGhost::parse(&bytes).map(|g| (g.map, g.car, g.stream))
    } else {
        Ghost::parse(&bytes).map(|g| (g.map, g.car.into(), g.stream))
    };
    ghost.map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes `stream` as a `.gho` of `map` and `car` (`Cars.ini` index), or a `.ghc` with `weather` by the extension.
pub fn write(path: &Path, map: u32, car: u16, weather: Weather, stream: Stream) -> Result<(), String> {
    let bytes = if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ghc")) {
        SavedGhost { map, car: car.into(), weather, stream }.write()
    } else {
        Ghost { map, car, reserved: [0xff; 12], stream, after: Vec::new() }.write()
    };
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The race time of a drive frame: its step counter less the load's settle steps, in the drive's steps (`clock`:
/// 1.02's 7 ms after 160, NGP's 1/720 s after 1520), s (see the module docs).
pub fn race_time(sample: &Sample, clock: StepClock) -> f32 {
    ((f64::from(sample.step) - f64::from(clock.settle)) * f64::from(clock.step)) as f32
}

/// Records a drive's frames from the race start to the step `end` (all with `None`), one game frame per recording
/// frame (see the module docs), with the outside-assistance `penalties` (each one's step and seconds) after the frame
/// they came in (`rbr_gameplay::ghost::Recorder::penalty`). `root` is the model's root in the body frame: the recorded point is the body's
/// position moved by it, as the game records the model origin (see the module docs). Ours: under NGP's 1/720 s steps a game frame spans at least 1/60 s, so runs of
/// one-step frames (the headless autopilot's) are taken one per 12 steps; one per step would fill the recorder's
/// buffer (sampled every second frame) after 85 s. 1.02's 7 ms frames and the live run's paced frames are game frames.
pub fn record(samples: &[Sample], clock: StepClock, end: Option<u32>, root: Vec3, penalties: &[(u32, f32)]) -> Stream {
    let frame_steps =
        if clock.step < rbr_recording::STEP_SECONDS { (1.0 / 60.0 / clock.step).round() as u32 } else { 1 };
    let mut recorder = Recorder::new();
    let mut before: Option<(u32, f32)> = None;
    let mut previous = None;
    for sample in samples.iter().filter(|s| race_time(s, clock) > 0.0 && end.is_none_or(|e| s.step <= e)) {
        let one_step = previous.replace(sample.step).is_some_and(|p| sample.step == p + 1);
        if one_step && before.is_some_and(|(step, _)| sample.step - step < frame_steps) {
            continue;
        }
        let t = race_time(sample, clock);
        let rotation = glam::Quat::from_array(sample.car.orientation);
        let position = Vec3::from(sample.car.position) + rotation * root;
        recorder.car_state(Pose { position: position.to_array(), rotation: sample.car.orientation });
        recorder.frame(t - before.map_or(0.0, |(_, t)| t));
        let after = before.map_or(0, |(step, _)| step);
        for &(_, seconds) in penalties.iter().filter(|&&(at, _)| after < at && at <= sample.step) {
            recorder.penalty(seconds);
        }
        before = Some((sample.step, t));
    }
    recorder.finish()
}

/// The `Cars.ini` cars: each one's index and folder.
fn cars(data: &Data) -> Vec<(usize, String)> {
    let text = data.resolve("Cars\\Cars.ini").and_then(|path| std::fs::read(path).ok()).unwrap_or_default();
    let entries = data.car_slots(rbr_formats::cars::parse(&String::from_utf8_lossy(&text)).unwrap_or_default());
    let folder = |file: &str| file.split(['\\', '/']).rev().nth(1).unwrap_or_default().to_owned();
    entries.iter().map(|e| (e.index, folder(&e.file_name))).collect()
}

/// The viewer's ghost options: the file `--ghost` plays, the one `--save-ghost` writes, the `--best-ghost` file, the
/// `--ghost-weather` words, and whether the drive is live.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options<'a> {
    pub play: Option<&'a Path>,
    pub save: Option<&'a Path>,
    pub best: Option<&'a Path>,
    pub weather: Weather,
    pub live: bool,
}

/// `--ghost-weather`'s value: the eight `.ghc` weather words in file order, comma-separated (the amount and `+0x3c`
/// floats).
pub fn parse_weather(text: &str) -> Result<Weather, String> {
    let words: Vec<&str> = text.split(',').map(str::trim).collect();
    let bad = || format!("--ghost-weather {text}: eight comma-separated numbers");
    let [letter, cloud, weather, cloud_letter, wear, wetness, amount, unk3c] = words[..] else {
        return Err(bad());
    };
    let word = |w: &str| w.parse::<u32>().map_err(|_| bad());
    Ok(Weather {
        track_letter: word(letter)?,
        cloud: word(cloud)?,
        weather: word(weather)?,
        cloud_letter: word(cloud_letter)?,
        wear: word(wear)?,
        wetness: word(wetness)?,
        amount: amount.parse().map_err(|_| bad())?,
        unk3c: unk3c.parse().map_err(|_| bad())?,
    })
}

/// `--ghost-weather`'s value for `weather` ([`parse_weather`] reads it back).
pub fn weather_arg(w: &Weather) -> String {
    let words = [w.track_letter, w.cloud, w.weather, w.cloud_letter, w.wear, w.wetness];
    let words: Vec<String> = words.iter().map(u32::to_string).collect();
    format!("{},{},{}", words.join(","), w.amount, w.unk3c)
}

/// The viewer's ghost: the ghost car `--ghost` plays and the recording `--save-ghost` writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    car: Option<GhostCar>,
    /// The map and car (`Cars.ini` index) a written ghost names.
    key: (u32, u16),
    /// The file the live drive's ghost is written to when the viewer closes.
    save: Option<std::path::PathBuf>,
    /// The `--best-ghost` file, the stored ghost in it (if it is the stage's), and the weather words to write.
    best: Option<(std::path::PathBuf, Option<Best>)>,
    weather: Weather,
    /// The live drive's finish: the step the finish line was passed on, and whether the race ended retired.
    finish: Option<(u32, bool)>,
    /// The live split record at the finish.
    splits: Option<Splits>,
    /// The live drive's outside-assistance penalties: the step each was seen on and its seconds. The ghost's clock takes
    /// them as the game's does (`rbr_gameplay::ghost::Recorder::penalty`).
    penalties: Vec<(u32, f32)>,
}

impl Session {
    /// Loads the ghost `options.play` names (drawn with its own car, or `player_car` when the file's car is unknown)
    /// into `stage`, on the stage `stage_name`. With `options.save`, a recorded drive's ghost is written now and a live
    /// one's when the viewer closes ([`Session::finish`]).
    pub fn load(
        data: &Data,
        stage: &mut Stage,
        stage_name: &str,
        player_car: &str,
        drive: &Playback,
        options: Options,
    ) -> Result<Session, String> {
        let cars = cars(data);
        let map = crate::stage::map_number(stage_name).unwrap_or(0);
        // The stored best ghost, when it is this map's (`IsCurrent` 0x492c00 compares the map).
        let stored = options.best.filter(|p| p.is_file()).map(read).transpose()?.filter(|(m, _, _)| *m == map);
        let best = options.best.map(|p| {
            let best = stored.as_ref().map(|(m, _, s)| Best { map: *m, time: rbr_gameplay::ghost::end_time(s) });
            (p.to_owned(), best)
        });
        let play = match (options.play, &stored) {
            (Some(path), _) => Some((path.to_owned(), read(path)?)),
            (None, Some(ghost)) => options.best.map(|p| (p.to_owned(), ghost.clone())),
            (None, None) => None,
        };
        let slot_car =
            |index: u32| cars.iter().find(|(i, _)| *i as u32 == index).map_or(player_car, |(_, f)| f.as_str());
        let car = match play {
            Some((path, (_, index, stream))) => {
                let name = slot_car(index);
                eprintln!("ghost: {} ({} samples, car {name})", path.display(), stream.samples.len());
                Some(GhostCar::add(data, name, stage, Track::stream(stream))?)
            }
            None => None,
        };
        let index = cars.iter().find(|(_, f)| f.eq_ignore_ascii_case(player_car)).map_or(0, |(i, _)| *i as u16);
        let save = options.save.map(Path::to_owned);
        let session = Session {
            car,
            key: (map, index),
            save,
            best,
            weather: options.weather,
            finish: None,
            splits: None,
            penalties: Vec::new(),
        };
        if !options.live {
            let session = Session { best: None, ..session };
            session.finish(drive);
            return Ok(Session { save: None, ..session });
        }
        Ok(session)
    }

    /// Plays the ghost to the race time of `drive`'s current frame (see [`GhostCar::apply`]).
    pub fn apply(&mut self, renderer: &mut Renderer, drive: &Playback, view_projection: Mat4) {
        if let Some(car) = &mut self.car {
            let sample = drive.sample();
            let penalties: f32 = self.penalties.iter().filter(|&&(at, _)| at <= sample.step).map(|&(_, p)| p).sum();
            car.apply(renderer, race_time(sample, drive.step_clock()) + penalties, view_projection);
        }
    }

    /// Takes the live split record and the race clock's outside-assistance `penalties` so far (HUD2) of the frame whose
    /// step is `step`: the step the finish line is passed on, and each penalty's step. A restart (the record not
    /// finished again, the penalties back to 0) forgets them.
    pub fn observe(&mut self, splits: Option<&Splits>, penalties: f32, step: u32) {
        let seen: f32 = self.penalties.iter().map(|&(_, p)| p).sum();
        if penalties < seen {
            self.penalties.clear();
        }
        let seen: f32 = self.penalties.iter().map(|&(_, p)| p).sum();
        if penalties > seen {
            self.penalties.push((step, penalties - seen));
        }
        match splits {
            Some(s) if s.finished => {
                let at = self.finish.map_or(step, |(at, _)| at);
                self.finish = Some((at, s.retired));
                self.splits = Some(s.clone());
            }
            _ => (self.finish, self.splits) = (None, None),
        }
    }

    /// Writes the ghost of `drive` to the `--save-ghost` file, if there is one, and to the `--best-ghost` file when
    /// the drive finished and beats the stored one.
    pub fn finish(&self, drive: &Playback) {
        let (map, car) = self.key;
        if let Some(path) = &self.save {
            let stream = record(drive.samples(), drive.step_clock(), None, model_root(drive), &self.penalties);
            let samples = stream.samples.len();
            match write(path, map, car, self.weather, stream) {
                Ok(()) => eprintln!("ghost: wrote {} ({samples} samples, map {map}, car {car})", path.display()),
                Err(e) => eprintln!("ghost: {e}"),
            }
        }
        let Some((path, stored)) = &self.best else {
            return;
        };
        let (samples, clock, root) = (drive.samples(), drive.step_clock(), model_root(drive));
        let stream = match best_ghost(samples, clock, root, map, self.finish, *stored, &self.penalties) {
            Ok(stream) => stream,
            Err(why) => {
                eprintln!("ghost: {why}; the best ghost stays");
                return;
            }
        };
        // The ghost's clock (`m_wtime`) runs from the start of the countdown, so its end is about 7 s past the race
        // time (`rbr_gameplay::ghost::end_time`); the log names the race time.
        let end = rbr_gameplay::ghost::end_time(&stream);
        let time = self.splits.as_ref().map_or(end, |s| s.race_time);
        let made =
            path.parent().map_or(Ok(()), std::fs::create_dir_all).map_err(|e| format!("{}: {e}", path.display()));
        match made.and_then(|()| write(path, map, car, self.weather, stream)) {
            Ok(()) => {
                eprintln!("ghost: {time:.2} s is the new best ghost (its clock ends at {end:.2} s), {}", path.display())
            }
            Err(e) => eprintln!("ghost: {e}"),
        }
    }
}

/// The player car's model root in the body frame (see [`record`]).
fn model_root(drive: &Playback) -> Vec3 {
    drive.model_to_car().transform_point3(Vec3::ZERO)
}

/// The ghost of a live drive on map `map` that ended at `finish` (the finish line's step, and whether it retired),
/// when it replaces the `stored` best (`rbr_gameplay::ghost::replaces`), or why not. `root` and `penalties` as for
/// [`record`]: the times compared include the penalties.
pub fn best_ghost(
    samples: &[Sample],
    clock: StepClock,
    root: Vec3,
    map: u32,
    finish: Option<(u32, bool)>,
    stored: Option<Best>,
    penalties: &[(u32, f32)],
) -> Result<Stream, String> {
    let (end, retired) = finish.ok_or("the drive did not finish")?;
    let stream = record(samples, clock, Some(end), root, penalties);
    let new = Best { map, time: rbr_gameplay::ghost::end_time(&stream) };
    let clear = rbr_gameplay::ghost::clears(rbr_gameplay::flow::Mode::QUICK_RALLY, retired);
    if !rbr_gameplay::ghost::replaces(new, clear, stored) {
        return Err(format!("its clock's {:.2} s does not beat it", new.time));
    }
    Ok(stream)
}

/// A drawn ghost mesh: its stage index and its model-space vertices.
#[derive(Debug, Clone, PartialEq)]
struct Part {
    stage: usize,
    rest: Vec<Vertex>,
}

/// What a ghost plays: the game's sample stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    stream: Stream,
    player: Player,
}

impl Track {
    pub fn stream(stream: Stream) -> Track {
        Track { player: Player::new(&stream), stream }
    }

    /// Plays to race time `t`, from the start again when `t` went back (`rewind`).
    fn advance(&mut self, t: f32, rewind: bool) -> Option<Pose> {
        if rewind {
            self.player = Player::new(&self.stream);
        }
        self.player.advance(t)
    }
}

/// The ghost car in a stage.
#[derive(Debug, Clone, PartialEq)]
pub struct GhostCar {
    parts: Vec<Part>,
    track: Track,
    look: Look,
    /// The race time last played, to start over when the clock goes back.
    played: f32,
    /// The pose last emitted: the ghost stays there once its stream ends.
    pose: Option<Pose>,
}

impl GhostCar {
    /// Adds car `car_name`'s ghost mesh to `stage` and plays `track` on it. The meshes go to the blended layer, the
    /// last one, so adding them moves no mesh another part of the viewer has found.
    pub fn add(data: &Data, car_name: &str, stage: &mut Stage, track: Track) -> Result<GhostCar, String> {
        let mut ghost = car::load_ghost(data, car_name, Mat4::IDENTITY).map_err(|e| e.to_string())?;
        if ghost.meshes.is_empty() {
            return Err(format!("the car {car_name} has no ghost mesh"));
        }
        let ramp = TextureRef {
            set: TextureSet::Car,
            index: stage.textures.keys().filter(|k| k.set == TextureSet::Car).map(|k| k.index + 1).max().unwrap_or(0),
        };
        stage.textures.insert(
            ramp,
            StageTexture { request: "ghost alpha ramp (ours)".to_owned(), image: Some(Image::Dds(ramp_dds())) },
        );
        let look = Look::load(data);
        let colour = look.ambient.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        let state = DrawState {
            textures: [Some(ramp), None],
            shadow: None,
            address: [Address::Clamp; 2],
            second_texture: false,
            alpha_ref: None,
            mip_bias: false,
            cull_back: true,
            blend: true,
            depth_test: true,
            depth_write: false,
            shade: None,
            specular: None,
        };
        let first = stage.meshes.len();
        for mesh in &mut ghost.meshes {
            mesh.layer = Layer::Blended;
            for draw in &mut mesh.draws {
                draw.state = state;
            }
            for v in &mut mesh.vertices {
                v.color = [colour[0], colour[1], colour[2], 255];
            }
        }
        let parts = ghost.meshes.iter().enumerate().map(|(i, m)| Part { stage: first + i, rest: m.vertices.clone() });
        let parts = parts.collect();
        stage.meshes.extend(ghost.meshes);
        Ok(GhostCar { parts, track, look, played: f32::NEG_INFINITY, pose: None })
    }

    /// Plays the ghost to race time `t` and moves its meshes in `renderer`, seen by `view_projection`'s camera. Before
    /// the race start (and when the stream holds nothing) the ghost is hidden.
    pub fn apply(&mut self, renderer: &mut Renderer, t: f32, view_projection: Mat4) {
        let rewind = t < self.played;
        if rewind {
            self.pose = None;
        }
        self.played = t;
        if t > 0.0 {
            if let Some(pose) = self.track.advance(t, rewind) {
                self.pose = Some(pose);
            }
        }
        let Some(pose) = self.pose.filter(|_| t > 0.0) else {
            for part in &self.parts {
                renderer.set_visible(part.stage, false);
            }
            return;
        };
        let car = CarPose { position: pose.position, orientation: pose.rotation, velocity: [0.0; 3] };
        let model = playback::root_to_stage(&car);
        let origin = model.transform_point3(Vec3::ZERO);
        let u = ramp_u(self.look.opacity(camera::eye(view_projection).distance(origin)));
        for part in &self.parts {
            let vertices: Vec<Vertex> = part
                .rest
                .iter()
                .map(|v| Vertex {
                    position: model.transform_point3(Vec3::from(v.position)).to_array(),
                    normal: model.transform_vector3(Vec3::from(v.normal)).to_array(),
                    uv0: [u, 0.5],
                    ..*v
                })
                .collect();
            renderer.set_vertices(part.stage, &vertices, mesh_bounds(&vertices));
            renderer.set_visible(part.stage, true);
        }
    }

    /// Where the ghost is, in stage space (`None` before it shows).
    pub fn position(&self) -> Option<Vec3> {
        let pose = self.pose?;
        let car = CarPose { position: pose.position, orientation: pose.rotation, velocity: [0.0; 3] };
        Some(playback::root_to_stage(&car).transform_point3(Vec3::ZERO))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weather_argument_reads_back() {
        let w = Weather {
            track_letter: 3,
            cloud: 2,
            weather: 4,
            cloud_letter: 1,
            wear: 2,
            wetness: 2,
            amount: 100.0,
            unk3c: 15.0,
        };
        assert_eq!(parse_weather(&weather_arg(&w)), Ok(w));
        assert!(parse_weather("1,2,3").is_err());
    }

    #[test]
    fn a_finish_is_kept_until_a_restart() {
        let mut session = Session {
            car: None,
            key: (54, 6),
            save: None,
            best: None,
            weather: Weather::default(),
            finish: None,
            splits: None,
            penalties: Vec::new(),
        };
        let finished = Splits { finished: true, ..Splits::default() };
        session.observe(Some(&Splits::default()), 0.0, 100);
        assert_eq!(session.finish, None);
        session.observe(Some(&finished), 0.0, 200);
        session.observe(Some(&Splits { over: true, ..finished.clone() }), 0.0, 300);
        assert_eq!(session.finish, Some((200, false)));
        session.observe(Some(&Splits::default()), 0.0, 10);
        assert_eq!(session.finish, None);
    }

    /// Each outside-assistance penalty is kept at the step it was first seen on, and a restart forgets them.
    #[test]
    fn the_penalties_are_kept_by_step_until_a_restart() {
        let mut session = Session {
            car: None,
            key: (34, 6),
            save: None,
            best: None,
            weather: Weather::default(),
            finish: None,
            splits: None,
            penalties: Vec::new(),
        };
        for (step, total) in [(100, 0.0), (2100, 10.0), (2101, 10.0), (5000, 30.0)] {
            session.observe(None, total, step);
        }
        assert_eq!(session.penalties, [(2100, 10.0), (5000, 20.0)]);
        session.observe(None, 0.0, 10);
        assert!(session.penalties.is_empty());
    }

    #[test]
    fn the_look_reads_the_retail_settings() {
        let look = Look::parse("[Settings]\nAmbient = 0.5 0.5 0.7\nFadeDistance = 100\nMaxOpacity = 0.7\n");
        assert_eq!(look, Look { ambient: [0.5, 0.5, 0.7], fade_distance: 100.0, max_opacity: 0.7 });
        assert_eq!(Look::parse("garbage"), Look::default());
        assert!((look.opacity(5.0) - 0.25).abs() < 1e-6);
        assert_eq!(look.opacity(9.0), 0.7);
    }

    #[test]
    fn the_ramp_is_a_dxt3_with_one_level() {
        let bytes = ramp_dds();
        let dds = rbr_formats::dds::parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.mip_count), (16, 4, 1));
        // Block 1, its first row: texels 4..8.
        assert_eq!(&dds.data[16..18], &[0x54, 0x76]);
        assert!((ramp_u(0.0) - 1.0 / 32.0).abs() < 1e-6 && (ramp_u(1.0) - 31.0 / 32.0).abs() < 1e-6);
    }
}
