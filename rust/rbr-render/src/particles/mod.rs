//! Particles (`notes/renderer.md` "Particles"): the stage's `PS_*.ini` emitters driven by the car's wheels, its exhaust and the
//! weather, drawn as camera-facing sprites from `Textures/particles.dat`.
//!
//! - [`sim`]: the pure simulation (pools, emission, motion, billboards), deterministic per seed.
//! - [`tyres`]: the tyre signal handlers and the fake-smoke dust.
//! - [`ground`]: the surface under a wheel for playback (`.col` and `.mat`).
//! - [`gpu`]: the billboard pass.
//!
//! The particles follow the drive's frames ([`Particles::follow`]), each a game frame of its payload periods (7 ms
//! each), so their state is a function of the frames seen: a screenshot at a step replays the [`WARM_UP`] frames before
//! it. In live mode the standalone's PTC1 record gives each step's tyre payloads (`rbr_recording::particles`); for a
//! recording its replay gives them ([`sidecar`]), else [`Particles::follow`] makes them from the recorded poses
//! ([`Particles::payloads_from_poses`], ours). The same payloads step the skid marks ([`crate::skidmarks`]), so a
//! drive, a live run and a jump all lay them alike.

pub mod car;
pub mod damping;
pub mod gpu;
pub mod ground;
pub mod rain;
pub mod sidecar;
pub mod sim;
pub mod track;
pub mod tyres;
pub mod wave;
pub mod weather;

use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::Path;

use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_formats::{col, dat, ini, mat, pss, rbz, rsf, stage_files};
use rbr_recording::particles::{self as records, Event, TyreParticles};

use crate::dirt::{self, Spray, Thrown};
use crate::live::Live;
use crate::playback::{Playback, Sample};
use crate::renderer::Renderer;
use crate::skidmarks::SkidMarks;
use crate::stage::{map_number, Selection, Stage, Weather};
use car::CarEffects;
use damping::Damping;
use ground::Ground;
use rain::Rain;
use sim::{Billboard, Facing, Simulation};
use track::TrackEmitters;
use tyres::{Tyres, Wheel};
use weather::{TrackWeather, Wind};

/// The physics step of one frame, seconds.
const STEP: f32 = rbr_recording::STEP_SECONDS;
/// Ours: the game's frame time the per-frame sums are taken at (the tyre counts, the exhaust, the rain's trail, the
/// skid marks' accumulator). The game adds its last rendered frame's time (`E83System` +0x94) once a signal or a frame,
/// so its particle counts and mark spacing follow its frame rate; ours are those of the game at 60 frames a second,
/// one live answer (a 7 ms frame, under 1.02 and NGP alike) being one of its physics steps.
pub const GAME_FRAME: f32 = 1.0 / 60.0;
/// Frames replayed before a jump in the drive (a screenshot, a seek): 4 s.
pub const WARM_UP: usize = (4.0 / STEP) as usize;
/// Ours: the recorded anchors' roll per metre of travel (`rbr-render/README.md` (git 9c28c0b8), Playback).
const ROLL_PER_METRE: f32 = 2.2;
/// Ours: the tyre's height over the ground when it touches, and the slack allowed, metres.
const RIDE: f32 = 0.31;
const CONTACT_SLACK: f32 = 0.12;
/// Ours: the mass flow made from a wheel's slip speed (m/s) in playback, by slip; the live payloads' mean mass for each
/// slip of the poses on the Tanner damage drive (gravel).
const SLIP_KEYS: [f32; 11] = [0.0, 0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 7.0, 10.0];
const SLIP_MASS: [f32; 11] = [0.0, 0.1, 0.16, 0.47, 1.7, 2.6, 4.3, 13.0, 22.0, 33.0, 68.0];
/// Ours: half the contact patch's width, m, for the edges of a playback payload (the skid marks' trail width).
const TYRE_HALF_WIDTH: f32 = 0.1;
/// Ours: the exhaust's particles per second at idle and at full throttle, where the frame has no `CSigExhaust`
/// (playback).
const EXHAUST_RATE: [f32; 2] = [6.0, 30.0];

/// Rain or snow and how much: the weather daemon's amount over 100 (`E25AppObj::M9b80`; light 0.5, heavy 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Precipitation {
    pub snow: bool,
    pub intensity: f32,
}

impl Precipitation {
    /// The game's rule: the `TrackSettings.ini` section's weather preset (weather ids 3, 4, 6 and 7:
    /// `lightrain`, `heavyrain`, `lightsnow`, `heavysnow`) gives rain or snow at amount 50 or 100
    /// (`E23SndParam::Update`); the stage's particle file must have its emitter (`SetWheelEmitter` finds none
    /// otherwise). The wetness of the surfaces plays no part.
    pub fn of(section: Option<&str>, has_rain: bool, has_snow: bool) -> Option<Precipitation> {
        let section = section.unwrap_or("").to_ascii_lowercase();
        let preset = ["lightrain", "heavyrain", "lightsnow", "heavysnow"].into_iter().find(|p| section.contains(p))?;
        let snow = preset.ends_with("snow");
        let intensity = if preset.starts_with("heavy") { 1.0 } else { 0.5 };
        (if snow { has_snow } else { has_rain }).then_some(Precipitation { snow, intensity })
    }

    /// The renderer addition's rule for `--enhance particles` (ours, not the game's): the wet weather always falls,
    /// fully; the damp weather at 0.35 when the `TrackSettings.ini` section's preset names rain or snow. Snow where the
    /// preset names it or the file has no rain emitter.
    pub fn by_wetness(
        section: Option<&str>,
        weather: Weather,
        has_rain: bool,
        has_snow: bool,
    ) -> Option<Precipitation> {
        let section = section.unwrap_or("").to_ascii_lowercase();
        let names_snow = section.contains("snow") && !section.contains("nosnow");
        let names_rain = section.contains("rain") && !section.contains("norain");
        let intensity = match weather {
            Weather::Dry => return None,
            Weather::Damp if !(names_rain || names_snow) => return None,
            Weather::Damp => 0.35,
            Weather::Wet => 1.0,
        };
        let snow = has_snow && (names_snow || !has_rain);
        (snow || has_rain).then_some(Precipitation { snow, intensity })
    }
}

/// One step's tyre payloads and particle events (`rbr_recording::particles`).
pub type Step = (Vec<TyreParticles>, Vec<Event>);

/// A stage's particles for a car.
pub struct Particles {
    pub sim: Simulation,
    tyres: Tyres,
    ground: Option<Ground>,
    precipitation: Option<Precipitation>,
    /// [`Precipitation::by_wetness`], for `--enhance particles`.
    wetness: Option<Precipitation>,
    /// The wind of the stage's `track_w.ini`.
    wind: Wind,
    /// The game's rain or snow about the camera (`CEmitterRain`).
    rain: Rain,
    exhaust: Option<(usize, Vec3)>,
    /// The car's exhaust and engine points, model space.
    points: [Option<Vec3>; 2],
    car: CarEffects,
    damping: Damping,
    /// The game's own tyre payloads and events by step, for playback ([`sidecar`], [`Particles::set_recorded`]).
    recorded: HashMap<u32, Step>,
    /// The texture of each sprite id: its `particles.dat` entry name.
    pub sprites: Vec<String>,
    /// The skid marks the wheels lay; `None` when the stage's skid table is missing.
    pub marks: Option<SkidMarks>,
    /// The last frame followed, and the step it had.
    last: Option<(usize, u32)>,
    /// The tyre spray of every frame up to the last followed, for the car's dirt ([`crate::dirt`]).
    pub spray: Spray,
    /// Live tyre payloads and events, one step per frame, not followed yet.
    live: VecDeque<Step>,
    /// Whether the live process sends payloads (from its first PTC1 on).
    live_tyres: bool,
    eye: Vec3,
    /// The pools whose life the helicopter's `Activate` cut, and its dust to throw: frame, pool, point, velocity.
    rescue_pools: std::collections::HashSet<usize>,
    smoke: VecDeque<(usize, usize, Vec3, Vec3)>,
    /// The stage's track emitters (`.dls` table 4).
    track: TrackEmitters,
}

impl Particles {
    /// The particles of `stage` (`track-54_E`) for `selection`, the `TrackSettings.ini` section `section` and the car
    /// `car` (its `Cars.ini` folder, for the exhaust). `None` when the stage's map has no particle file.
    pub fn load(
        data: &Data,
        stage: &str,
        selection: Selection,
        section: Option<&str>,
        car: Option<&str>,
    ) -> Result<Option<Particles>, String> {
        let maps = data.install().join("Maps");
        let Some(path) = particle_file(data, &maps, stage)? else { return Ok(None) };
        let text = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file = pss::parse(&String::from_utf8_lossy(&text)).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut sim = Simulation::new(&file, 0x5eed_0001);
        let tyres = Tyres::new(&file, &sim, 0x5eed_0002);
        let car_effects = CarEffects::new(&mut sim);
        let damping = Damping::new(&file, &sim);
        let marks = rbr_formats::skidmarks::for_stage(data, stage)?.map(SkidMarks::new);
        let ground = load_ground(&maps, stage, selection);
        let has = |name: &str| file.emitter(name).is_some();
        let precipitation = Precipitation::of(section, has("g_rain_emitter"), has("g_snow_emitter"));
        let wetness =
            Precipitation::by_wetness(section, selection.weather, has("g_rain_emitter"), has("g_snow_emitter"));
        let track_weather = map_number(stage).map_or_else(TrackWeather::default, |map| TrackWeather::load(data, map));
        let rain = precipitation
            .map_or_else(Rain::default, |p| Rain::new(&file, &mut sim, p, track_weather.wind, 0x5eed_0004));
        let points = ["Point_exhaust1", "Point_engine"].map(|name| car.and_then(|car| car_point(data, car, name)));
        let exhaust =
            ["g_exhaust_emitter", "g_exhaust_arctic_emitter"].iter().find_map(|name| sim.find(name)).zip(points[0]);
        let sprites = sprite_names(data)?;
        let track = std::fs::read(stage_files::file_in(&maps, stage, "dls"))
            .ok()
            .and_then(|bytes| rbr_formats::dls::parse(&bytes).ok().map(|dls| TrackEmitters::new(&dls, &sim)))
            .unwrap_or_default();
        eprintln!(
            "particles: {} ({} emitters, {} tyre handlers), ground {}, {:?}",
            path.file_name().unwrap().to_string_lossy(),
            sim.pools.len(),
            file.handlers.len(),
            if ground.is_some() { "loaded" } else { "missing" },
            precipitation
        );
        Ok(Some(Particles {
            sim,
            tyres,
            ground,
            precipitation,
            wetness,
            wind: Wind::new(track_weather, 0x5eed_0003),
            rain,
            exhaust,
            points,
            car: car_effects,
            damping,
            recorded: HashMap::new(),
            sprites,
            marks,
            last: None,
            spray: Spray::default(),
            live: VecDeque::new(),
            live_tyres: false,
            eye: Vec3::ZERO,
            rescue_pools: Default::default(),
            smoke: VecDeque::new(),
            track,
        }))
    }

    /// The weather's rain or snow.
    pub fn precipitation(&self) -> Option<Precipitation> {
        self.precipitation
    }

    /// The rain or snow by the stage's wetness ([`Precipitation::by_wetness`]), for the renderer's own rain and snow.
    pub fn precipitation_by_wetness(&self) -> Option<Precipitation> {
        self.wetness
    }

    /// Stops throwing the weather's particles, for a renderer that draws the rain and snow itself
    /// (`Enhance::particles`); [`Particles::precipitation`] still names the weather.
    pub fn drop_weather(&mut self) {
        self.rain.clear(&mut self.sim);
    }

    /// The game's own tyre payloads and events of the drive by step, for playback ([`sidecar::events`]).
    pub fn set_recorded(&mut self, recorded: HashMap<u32, Step>) {
        self.recorded = recorded;
        self.last = None;
    }

    /// Lights the stage's chimneys (ours, the renderer's particles: [`track`]).
    pub fn light_chimneys(&mut self) {
        self.track.light(&mut self.sim);
    }

    /// Queues the live process's tyre payloads and events, one step per frame it sent
    /// ([`crate::live::Live::take_tyres`]).
    pub fn take_live(&mut self, steps: Vec<Step>) {
        for step in steps {
            self.live_tyres |= !step.0.is_empty();
            self.live.push_back(step);
        }
    }

    /// Follows the drive to its current frame: each frame since the last followed is one game frame of emission and
    /// motion, of its payload periods' time (a late live answer stands for several, as the game's frame takes all the
    /// wall time since the last). A jump back or far ahead starts over [`WARM_UP`] frames before the current one. `eye`
    /// is the camera and `forward` its view direction, stage space (the precipitation falls about it).
    pub fn follow(&mut self, playback: &Playback, eye: Vec3, forward: Vec3) {
        let samples = playback.samples();
        let index = playback.index();
        let eye = Vec3::new(eye.x, eye.z, eye.y);
        let forward = Vec3::new(forward.x, forward.z, forward.y);
        let first = match self.last {
            Some((last, step)) if last <= index && index - last <= WARM_UP && samples[last].step == step => last + 1,
            _ => {
                self.sim.clear();
                self.car.reset();
                self.tyres.clear();
                if let Some(marks) = &mut self.marks {
                    marks.reset();
                }
                self.rain.reset();
                self.track.warm(&mut self.sim, eye, STEP * 10.0);
                self.eye = eye;
                let first = index.saturating_sub(WARM_UP);
                self.spray_before(samples, first);
                first
            }
        };
        // Live payloads belong to the newest frames; older ones were for frames already followed or skipped.
        while self.live.len() > index + 1 - first {
            self.live.pop_front();
        }
        let frames = (index + 1 - first) as f32;
        for i in first..=index {
            // The camera moved once over the frames since the last draw: spread its motion over them.
            let eye_now = self.eye.lerp(eye, (i + 1 - first) as f32 / frames);
            let live = self.live.pop_front().filter(|_| self.live_tyres);
            let step = live.or_else(|| self.recorded.get(&samples[i].step).cloned());
            let previous = i.checked_sub(1).map(|p| &samples[p]);
            self.throw_smoke(i);
            let dt = STEP * playback.periods(i) as f32;
            let model = playback.model_to_physics(&samples[i]);
            let spray = self.frame(&samples[i], previous, step, model, eye_now, forward, dt);
            self.spray.record(i, samples[i].step, spray);
        }
        self.eye = eye;
        self.last = Some((index, samples[index].step));
    }

    /// The spray of the frames before `first` that the warm-up skips: the recorded payloads, else the poses'.
    fn spray_before(&mut self, samples: &[Sample], first: usize) {
        for i in 0..first {
            if self.spray.knows(i, samples[i].step) {
                continue;
            }
            let payloads = match self.recorded.get(&samples[i].step) {
                Some((payloads, _)) => payloads.clone(),
                None => self.payloads_from_poses(&samples[i], i.checked_sub(1).map(|p| &samples[p])),
            };
            self.spray.record(i, samples[i].step, dirt::frame_spray(payloads.iter().map(|p| (p.surface(), p.mass))));
        }
    }

    /// One frame of `dt` seconds: the tyres, the car's events, the exhaust, the weather, then the motion. `step` is the
    /// game's own payloads and events (live, or recorded), else they come from the poses. Gives the payloads' summed
    /// mass and what they throw at the windscreen ([`dirt::frame_spray`]). A frame of several periods has each wheel's last payload (the standalone's), which emits for all of them.
    #[allow(clippy::too_many_arguments)]
    fn frame(
        &mut self,
        sample: &Sample,
        previous: Option<&Sample>,
        step: Option<Step>,
        model: Mat4,
        eye: Vec3,
        forward: Vec3,
        dt: f32,
    ) -> (f32, Thrown) {
        let car_velocity = Vec3::from(sample.car.velocity);
        let pose = car::Pose {
            velocity: car_velocity,
            up: Vec3::from(sample.car.rotate([0.0, 0.0, 1.0])),
            side: Vec3::from(sample.car.rotate([1.0, 0.0, 0.0])),
            exhaust: self.points[0].map(|p| model.transform_point3(p)),
            engine: self.points[1].map(|p| model.transform_point3(p)),
        };
        let (payloads, events) = match step {
            Some(step) => step,
            None => (self.payloads_from_poses(sample, previous), self.events_from_poses(sample, previous)),
        };
        for p in &payloads {
            self.car.tyre_water(p.splash);
        }
        let exhausts = events.iter().filter(|e| matches!(e, Event::Exhaust { .. })).count();
        for event in events {
            self.event(event, &pose);
        }
        self.car.step(&mut self.sim, &pose, dt);
        if let Some(marks) = &mut self.marks {
            marks.step(&payloads, GAME_FRAME * dt / STEP);
        }
        let wheels: Vec<Wheel> = payloads.iter().map(wheel_of_payload).collect();
        self.tyres.feed(&mut self.sim, &wheels, car_velocity, pose.side, dt);
        if let (Some((pool, point)), 0) = (self.exhaust, exhausts) {
            let at = model.transform_point3(point);
            let throttle = sample.control[0].clamp(0.0, 1.0);
            let rate = EXHAUST_RATE[0] + (EXHAUST_RATE[1] - EXHAUST_RATE[0]) * throttle;
            let back = model.transform_vector3(Vec3::new(0.0, 0.0, 1.0)).normalize_or_zero();
            self.sim.emit(pool, at, at, car_velocity * 0.8 + back, rate * dt, f32::MIN);
        }
        self.sim.wind = self.wind.step(dt);
        self.track.step(&mut self.sim, eye, dt);
        self.sim.step(dt);
        self.rain.step(&mut self.sim, eye, forward, dt);
        dirt::frame_spray(payloads.iter().map(|p| (p.surface(), p.mass)))
    }

    /// The rescue helicopter's dust (`CRescueHelicopter::Burst`, [`crate::characters::Characters::take_smoke`]):
    /// one particle of `emitter` at each point (physics axes) with its velocity, thrown when [`Particles::follow`]
    /// steps the playback frame given with it. `Activate` cut the emitter's particle life to a fifth
    /// ([`crate::characters::scripted::SMOKE_LIFE`]) and set its intermediate alpha by rally; here on its first throw.
    pub fn rescue_smoke(
        &mut self,
        smoke: crate::characters::scripted::Smoke,
        particles: Vec<crate::characters::Thrown>,
    ) {
        let Some(pool) = self.sim.find(smoke.emitter) else { return };
        if self.rescue_pools.insert(pool) {
            let look = &mut self.sim.pools[pool].def.look;
            look.life *= crate::characters::scripted::SMOKE_LIFE;
            look.colors[1][3] = smoke.mid_alpha;
        }
        self.smoke.extend(particles.into_iter().map(|(frame, at, velocity)| (frame, pool, at, velocity)));
    }

    /// The helicopter's dust of playback frame `frame` (the older is dropped).
    fn throw_smoke(&mut self, frame: usize) {
        while let Some(&(at_frame, pool, at, velocity)) = self.smoke.front() {
            if at_frame > frame {
                break;
            }
            self.smoke.pop_front();
            if at_frame == frame {
                self.sim.emit(pool, at, at, velocity, 1.0, f32::MIN);
            }
        }
    }

    /// One of the car's particle events.
    fn event(&mut self, event: Event, pose: &car::Pose) {
        match event {
            Event::CarDamage { kind, force, at } => {
                let at = Vec3::from(at);
                let floor = self.ground.as_ref().and_then(|g| g.below(at.to_array())).map_or(at.z - 0.5, |h| h.z);
                self.car.damage(&mut self.sim, kind, force, at, pose, floor);
            }
            Event::Damping { penetration, material, at, push } => {
                self.damping.event(&mut self.sim, penetration, material, Vec3::from(at), Vec3::from(push))
            }
            Event::EngineFire(level) => self.car.engine_fire(level),
            Event::EngineNoise { misfire, steam } => self.car.engine_noise(&mut self.sim, misfire, steam, pose),
            Event::Exhaust { flow, broken } => self.car.exhaust(&mut self.sim, flow, broken, pose),
            Event::Lights { .. } => {}
        }
    }

    /// The impacts guessed from the recorded speeds (ours, [`car::impact_from_speeds`]): playback has no damage
    /// events.
    fn events_from_poses(&self, sample: &Sample, previous: Option<&Sample>) -> Vec<Event> {
        let Some(previous) = previous else { return Vec::new() };
        let (before, after) = (Vec3::from(previous.car.velocity), Vec3::from(sample.car.velocity));
        car::impact_from_speeds(Vec3::from(sample.car.position), before, after)
            .map(|(force, at)| Event::CarDamage { kind: 2, force, at: at.to_array() })
            .into_iter()
            .collect()
    }

    /// The wheels' tyre values made from the recorded poses (ours): the patch under the wheel's anchor, the surface
    /// there ([`Ground`]), the contact by the anchor's height over it, and a mass flow from the slip speed: the wheel's
    /// ground speed against its rolling speed (the recorded roll's rate) and its sideways speed. The slip also stands
    /// for the rubber the skid marks take (ours: depth and dirt are 0, the poses hold neither).
    pub fn payloads_from_poses(&self, sample: &Sample, previous: Option<&Sample>) -> Vec<TyreParticles> {
        let car = &sample.car;
        let car_velocity = Vec3::from(car.velocity);
        let forward = Vec3::from(car.forward());
        let side = Vec3::from(car.rotate([1.0, 0.0, 0.0]));
        (0..4)
            .map(|k| {
                let anchor = Vec3::from(car.to_world(sample.wheels[k].anchor));
                let hit = self.ground.as_ref().and_then(|g| g.below(anchor.to_array()));
                let (ground_z, surface) = hit.map_or((anchor.z - RIDE, 0), |h| (h.z, h.surface));
                let contact = anchor.z - ground_z < RIDE + CONTACT_SLACK;
                let rolling = previous.map_or(0.0, |p| {
                    let d = sample.wheels[k].roll - p.wheels[k].roll;
                    // The roll wraps at 20 pi.
                    let d = (d + 10.0 * std::f32::consts::PI).rem_euclid(20.0 * std::f32::consts::PI)
                        - 10.0 * std::f32::consts::PI;
                    d / STEP / ROLL_PER_METRE
                });
                let along = car_velocity.dot(forward);
                let slip = Vec3::new(along.abs() - rolling.abs(), car_velocity.dot(side), 0.0).length();
                let at = Vec3::new(anchor.x, anchor.y, ground_z);
                // The live payload's velocity is the patch's (within a few cm/s on tanner-damage): the anchor's.
                let velocity =
                    previous.map_or(car_velocity, |p| (anchor - Vec3::from(p.car.to_world(p.wheels[k].anchor))) / STEP);
                let across = Vec3::new(side.x, side.y, 0.0).normalize_or_zero() * TYRE_HALF_WIDTH;
                TyreParticles {
                    wheel: k as u32,
                    edges: [(at - across).to_array(), (at + across).to_array()],
                    velocity: velocity.to_array(),
                    rubber: if contact { slip } else { 0.0 },
                    mass: if contact { tyres::table_lerp(&SLIP_KEYS, &SLIP_MASS, slip) } else { 0.0 },
                    surfaces: [surface, surface],
                    contact,
                    ..TyreParticles::default()
                }
            })
            .collect()
    }

    /// The wheels' values for [`Particles::frame`] made from the recorded poses ([`Particles::payloads_from_poses`]).
    pub fn wheels_from_poses(&self, sample: &Sample, previous: Option<&Sample>) -> Vec<Wheel> {
        self.payloads_from_poses(sample, previous).iter().map(wheel_of_payload).collect()
    }

    /// The billboards for a frame seen with `view_projection`, back to front; `layer` maps a sprite id to a texture
    /// layer ([`gpu::Sprites::layer`]).
    pub fn billboards(&self, view_projection: Mat4, layer: impl Fn(u32) -> Option<u32>) -> Vec<Billboard> {
        self.sim.billboards(facing(view_projection), layer)
    }

    /// The water waves' triangles, with the wave texture's layer ([`gpu::Sprites::layer`]).
    pub fn waves(&self, layer: impl Fn(u32) -> Option<u32>) -> Vec<wave::WaveVertex> {
        layer(wave::SPRITE).map_or_else(Vec::new, |l| self.tyres.waves.vertices(l))
    }
}

/// A stage's particles with their sprites, as the viewer draws them.
pub struct Drawn {
    pub particles: Particles,
    pub sprites: gpu::Sprites,
    /// `skidmarks.dds`, when the stage has skid marks.
    skid_texture: Option<Vec<u8>>,
}

impl Drawn {
    /// The particles of `stage` for `car`, or `None` (reported on stderr) when the stage has none or they do not load.
    pub fn load(data: &Data, stage: &Stage, car: Option<&str>) -> Option<Drawn> {
        let section = stage.atmosphere.section.as_deref();
        let loaded = Particles::load(data, &stage.name, stage.selection, section, car).and_then(|particles| {
            let Some(particles) = particles else { return Ok(None) };
            let mut sprites = gpu::Sprites::decode(&sprite_files(data, &particles.sprites)?);
            let splash = sprite_files(data, &["splashcolor.dds".to_owned()])?.pop().flatten();
            let alpha = std::fs::read(data.install().join("Textures").join("splashalpha.tga")).ok();
            if let (Some(colour), Some(alpha)) = (splash, alpha) {
                sprites.add(wave::SPRITE, &colour, &alpha);
            }
            let skid_texture = particles.marks.as_ref().map(|_| rbr_formats::skidmarks::texture(data)).transpose()?;
            Ok(Some(Drawn { particles, sprites, skid_texture }))
        });
        loaded.unwrap_or_else(|e| {
            eprintln!("particles: {e}");
            None
        })
    }

    /// Follows the drive to its current frame (with the live process's tyre payloads) and gives `renderer` the
    /// billboards seen with `view_projection`.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        playback: &Playback,
        live: Option<&mut Live>,
        view_projection: Mat4,
    ) {
        if let Some(live) = live {
            self.particles.take_live(live.take_tyres());
        }
        let started = std::time::Instant::now();
        let from = self.particles.last.map_or(0, |(last, _)| last);
        let forward = view_projection.row(3).truncate().normalize_or_zero();
        self.particles.follow(playback, crate::camera::eye(view_projection), forward);
        if let (Some(marks), Some(texture)) = (&self.particles.marks, &self.skid_texture) {
            if let Ok(dds) = rbr_formats::dds::parse(texture) {
                renderer.set_skid_marks(&dds, &marks.quads());
            }
        }
        let followed = started.elapsed();
        let billboards = self.particles.billboards(view_projection, |id| self.sprites.layer(id));
        let waves = self.particles.waves(|id| self.sprites.layer(id));
        if std::env::var_os("RBR_PARTICLES_TIMING").is_some() {
            let steps = playback.index().abs_diff(from).min(WARM_UP);
            eprintln!(
                "particles: {} alive, {steps} steps followed in {followed:?}, {} billboards built in {:?}",
                self.particles.sim.count(),
                billboards.len(),
                started.elapsed() - followed
            );
            for pool in self.particles.sim.pools.iter().filter(|p| !p.particles.is_empty()) {
                let first = pool.particles[0].position;
                eprintln!("particles:   {} {} alive, first at {first:.2}", pool.def.name, pool.particles.len());
            }
        }
        renderer.set_particles(&self.sprites, &billboards, &waves);
        renderer.set_car_dirt(self.particles.spray.level(playback.index()));
    }
}

/// The camera a view-projection looks from: its eye, and its right and up axes from the matrix's first two rows.
pub fn facing(view_projection: Mat4) -> Facing {
    let row = |i: usize| view_projection.row(i).truncate().normalize_or_zero();
    Facing { eye: crate::camera::eye(view_projection), right: row(0), up: row(1) }
}

/// A live payload as the handlers take it.
fn wheel_of_payload(p: &TyreParticles) -> Wheel {
    Wheel {
        wheel: p.wheel as usize,
        at: Vec3::from(p.centre()),
        edges: p.edges.map(Vec3::from),
        velocity: Vec3::from(p.velocity),
        mass: p.mass,
        smoke: p.smoke,
        splash: p.splash,
        surface: p.surface(),
        contact: p.contact,
    }
}

/// The stage's particle file: `Tracks.ini` `[MapNN] Particles`, always the full file (ours, the highest quality: the
/// game takes `_LOW` with `ParticleQuality = low`, and for every map from 0x48 but 0x4a, 0x4f and 0x50 at any quality;
/// `CPC_TrackState`, `RBR_E09_TrackHead.cpp:436`). The name is matched without case.
fn particle_file(data: &Data, maps: &Path, stage: &str) -> Result<Option<std::path::PathBuf>, String> {
    // A stage folder's `Tracks<id>.ini` (RallySimFans): the full file (ours: the exe's map-number rule is retail's).
    if let Some(folder) = stage_files::folder(stage) {
        return Ok(rsf::open(folder)?.particle_file(data, false));
    }
    let Some(number) = map_number(stage) else { return Ok(None) };
    let text = std::fs::read(maps.join("Tracks.ini")).map_err(|e| format!("Tracks.ini: {e}"))?;
    let sections = ini::parse(&String::from_utf8_lossy(&text)).map_err(|e| format!("Tracks.ini: {e}"))?;
    let Some(value) = sections.iter().find(|s| s.name == format!("Map{number}")).and_then(|s| s.get("Particles"))
    else {
        return Ok(None);
    };
    let stem = value.trim_matches('"').rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
    let wanted = format!("{stem}.ini");
    let entries = std::fs::read_dir(maps).map_err(|e| format!("{}: {e}", maps.display()))?;
    Ok(entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_ascii_lowercase() == wanted)))
}

/// The stage's ground for playback: its `.col` and `.mat` for the selection's wear and weather.
fn load_ground(maps: &Path, stage: &str, selection: Selection) -> Option<Ground> {
    let col = col::parse(&std::fs::read(stage_files::file_in(maps, stage, "col")).ok()?).ok()?;
    let mat = mat::parse(&std::fs::read(stage_files::file_in(maps, stage, "mat")).ok()?).ok()?;
    Some(Ground::new(&col, &mat, selection.wear.as_str(), selection.weather.as_str()))
}

/// The game's lamps in a drive's recorded events ([`sidecar::events`]), by step: the light system's state the recording
/// itself lacks (the reverse switch, lamp damage).
pub fn recorded_lamps(recorded: &HashMap<u32, Step>) -> std::collections::BTreeMap<u32, crate::lights::Lamps> {
    let mut out = std::collections::BTreeMap::new();
    for (&step, (_, events)) in recorded {
        if let Some(lamps) = events.iter().rev().find_map(|e| match *e {
            Event::Lights { switches, master, lamps } => Some(crate::lights::Lamps { switches, master, lamps }),
            _ => None,
        }) {
            out.insert(step, lamps);
        }
    }
    out
}

/// The game's own tyre payloads and particle events of a drive, by step: a file of `u32 step, u32 length` then that
/// step's PTC1 and PEV1 records (`rbr-standalone --events-out`, [`sidecar`]).
fn load_events(path: &Path) -> Result<HashMap<u32, Step>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = HashMap::new();
    let mut at = 0;
    let word = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    while let (Some(step), Some(length)) = (word(at), word(at + 4)) {
        let body = bytes.get(at + 8..at + 8 + length as usize).ok_or("a truncated step")?;
        let records = rbr_recording::live::read_records(&mut [body, &rbr_recording::live::SCN1].concat().as_slice())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut step_data: Step = (Vec::new(), Vec::new());
        for (tag, payload) in records {
            match tag {
                records::PTC1 => step_data.0 = records::read_body(&payload).map_err(|e| e.to_string())?,
                records::PEV1 => step_data.1 = records::read_events(&payload).map_err(|e| e.to_string())?,
                _ => {}
            }
        }
        out.insert(step, step_data);
        at += 8 + length as usize;
    }
    Ok(out)
}

/// A point of the car (`[<name>] Pos` of its `.ini`: `Point_exhaust1`, `Point_engine`), model space.
fn car_point(data: &Data, car: &str, name: &str) -> Option<Vec3> {
    let folder = data.install().join("Cars").join(car);
    std::fs::read_dir(&folder).ok()?.filter_map(Result::ok).find_map(|entry| {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) {
            return None;
        }
        let sections = ini::parse(&String::from_utf8_lossy(&std::fs::read(&path).ok()?)).ok()?;
        let pos = sections.iter().find(|s| s.name.eq_ignore_ascii_case(name))?.get("Pos")?;
        let v: Vec<f32> = pos.split_whitespace().filter_map(|t| t.parse().ok()).collect();
        (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
    })
}

/// `Misc/particletextures.ini` (in `Root/misc.rbz`) as `particles.dat` entry names, by sprite id.
fn sprite_names(data: &Data) -> Result<Vec<String>, String> {
    let path = data.resolve("Root/misc.rbz").ok_or("misc.rbz: not found")?;
    let mut zip = rbz::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut text = String::new();
    zip.by_name("Misc/particletextures.ini")
        .map_err(|e| format!("particletextures.ini: {e}"))?
        .read_to_string(&mut text)
        .map_err(|e| format!("particletextures.ini: {e}"))?;
    let names = pss::parse_textures(&text).map_err(|e| format!("particletextures.ini: {e}"))?;
    Ok(names.iter().map(|n| pss::sprite_file(n)).collect())
}

/// The sprites of `names` from `Textures/particles.dat`: each name's DDS bytes, or `None` when missing.
pub fn sprite_files(data: &Data, names: &[String]) -> Result<Vec<Option<Vec<u8>>>, String> {
    let path = data.install().join("Textures").join("particles.dat");
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = dat::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(names
        .iter()
        .map(|name| {
            let entry = entries
                .iter()
                .find(|e| !e.is_dir && e.path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
            dat::file_data(&bytes, entry).map(<[u8]>::to_vec)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precipitation_by_preset() {
        let heavy_rain = Some(Precipitation { snow: false, intensity: 1.0 });
        assert_eq!(Precipitation::of(Some("41O_M_heavyrain_heavycloud"), true, false), heavy_rain);
        assert_eq!(Precipitation::of(Some("54E_hazy_clear"), true, false), None);
        assert_eq!(
            Precipitation::of(Some("13O_E_lightsnow_heavycloud"), false, true),
            Some(Precipitation { snow: true, intensity: 0.5 })
        );
        assert_eq!(Precipitation::of(Some("61_lightrain"), true, true).map(|p| p.snow), Some(false));
        assert_eq!(Precipitation::of(Some("5O_N_norain_lightcloud"), true, false), None);
        assert_eq!(Precipitation::of(Some("31O_heavyrain"), false, false), None, "no rain emitter (Australia)");
        assert_eq!(Precipitation::of(None, true, true), None);
    }

    #[test]
    fn precipitation_by_wetness() {
        let rain = Some(Precipitation { snow: false, intensity: 1.0 });
        assert_eq!(Precipitation::by_wetness(Some("41O_M_heavyrain_heavycloud"), Weather::Wet, true, false), rain);
        assert_eq!(Precipitation::by_wetness(Some("54E_hazy_clear"), Weather::Wet, true, false), rain);
        assert_eq!(Precipitation::by_wetness(Some("54E_hazy_clear"), Weather::Damp, true, false), None);
        assert_eq!(
            Precipitation::by_wetness(Some("13O_lightsnow"), Weather::Damp, false, true).map(|p| p.snow),
            Some(true)
        );
        assert_eq!(Precipitation::by_wetness(Some("54E_heavyrain"), Weather::Dry, true, false), None);
    }
}
