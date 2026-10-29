//! The gameplay sound of Richard Burns Rally 1.02 (`notes/sound.md`), in two layers:
//!
//! - pure models that turn one physics step's inputs (`rbr_recording::sound`, the SND1 record) into voice commands
//!   ([`voice`]): the granular engine ([`engine`]: rpm slew, bank choice, crossfade, grains, the renderer), the gear
//!   and diff whine, the turbo and the blow-off valve ([`car`]), the tyre loops and off-road rattles ([`tyre`]), the
//!   impacts ([`impact`]), the suspension's squeaks and knocks ([`suspension`]), the broken driveshaft's clunks
//!   ([`driveshaft`]), the exhaust pops, bangs and the engine fire ([`exhaust`]), the co-driver's pace notes,
//!   countdown and damage calls ([`codriver`]) or a co-driver mod's calls ([`custom_codriver`]), the wind, birds,
//!   siren and rain around the car ([`world`]), and the
//!   rescue helicopter, bird hit and rally incident ([`scripted`]); [`Models`] runs them all on one step's [`Inputs`]
//!   and the camera's [`Scene`];
//! - the backend: the [`mixer`] plays the commands (engine stream, loops, one-shots, the in-car low-pass, the car
//!   voices' distance from the camera) into stereo frames, and [`device`] sends them to the audio output through
//!   cpal.
//!
//! The sound is NGP 7.4.778's (its NSD1 record, [`Inputs::ngp`]): the models take NGP's patches of the 1.02 sound code:
//! the engine voice's low rpm and the whine caps from NGP.dll ([`car`], [`exhaust`]), the tyres' second surface, road
//! list and skid threshold ([`tyre::wheel`]), the max distances NGP's fix of `E30DXSound` +0x74 gives (1.02's sets the
//! min: `world::DEFAULT_MAX_DISTANCE`), and the damper's and the fire's damage calls the right way round. NGP's FMOD
//! sound is modelled only for a car with a bank ([`fmod`]).
//!
//! [`Player`] puts them together for a live drive; the `rbr-sound` binary renders a recorded drive to a WAV. The
//! menus' music and service park ambience are [`menu`], with their own mixer and player.

pub mod car;
pub mod codriver;
pub mod custom_codriver;
pub mod data;
pub mod device;
pub mod driveshaft;
pub mod effects;
pub mod engine;
pub mod exhaust;
pub mod fmod;
pub mod impact;
pub mod menu;
pub mod mixer;
pub mod preview;
pub mod reflections;
pub mod scripted;
pub mod suspension;
mod tables;
pub mod tyre;
pub mod voice;
pub mod world;

use std::sync::{Arc, Mutex};

use rbr_formats::data::Data;
pub use rbr_recording::sound::{Inputs, Sound};

use crate::car::Car;
use crate::codriver::CoDriver;
use crate::custom_codriver::CustomCoDriver;
use crate::data::CarBank;
use crate::driveshaft::DriveShafts;
use crate::effects::Effects;
use crate::engine::{EngineNoise, Rng};
use crate::exhaust::Exhaust;
use crate::fmod::FmodCar;
use crate::impact::Impacts;
use crate::mixer::Mixer;
pub use crate::mixer::Volumes;
pub use crate::scripted::Events;
use crate::scripted::Scripted;
use crate::suspension::Suspension;
use crate::tyre::Tyres;
use crate::voice::Command;
use crate::world::World;
pub use crate::world::{Scene, Sky, Sphere};

/// The H-shifter's grind gain: the sample at full volume.
const GRIND_GAIN: f32 = 1.0;
/// The physics step, seconds.
const STEP: f32 = rbr_recording::STEP_SECONDS;
/// The pause after each word while the race clock is not running (`E31Speech::Vf08` 0x4d2e30: 0x7354e8 when the
/// HUD's +0x140 is at most 0), seconds.
const SPEECH_GAP: f32 = 0.371_519_27;

/// NGP's code for a damage call of the damper or the fire handler: its patches of the two pushes (0x4ae2eb in the
/// damper handler, 0x4ae3c0 in the fire handler; read in the `ngp-c6-bis` snapshots) swap 1.02's 7 and 6, so the
/// broken damper says `sus_broke` and the fire `weve_got_fire`.
fn ngp_call(code: u32) -> u32 {
    match code {
        suspension::BROKEN_CALL => exhaust::FIRE_CALL,
        exhaust::FIRE_CALL => suspension::BROKEN_CALL,
        other => other,
    }
}

/// Every model of the gameplay sound, stepped together.
pub struct Models {
    pub car: Car,
    pub tyres: Tyres,
    pub impacts: Impacts,
    pub suspension: Suspension,
    pub drive_shafts: DriveShafts,
    pub exhaust: Exhaust,
    pub codriver: CoDriver,
    /// A co-driver mod, calling the notes instead of 1.02's co-driver (whose countdown and damage calls stay).
    pub custom: Option<CustomCoDriver>,
    pub world: World,
    pub scripted: Scripted,
    pub effects: Effects,
    /// The car's FMOD bank, playing instead of the 1.02 engine, turbo and transmission voices (`fmod`, ours).
    pub fmod: Option<FmodCar>,
    /// The random numbers the models pick with (ours; the original uses the CRT `rand()`).
    rng: Rng,
    /// The part of a game frame ([`tyre::FRAME_RATE`]) not yet run.
    frame: f32,
}

impl Models {
    /// The car's models on a stage with weather `sky` and table-6 spheres `spheres`.
    pub fn new(bank: &CarBank, sky: Sky, spheres: Vec<Sphere>) -> Models {
        let g = bank.gains;
        Models {
            car: Car::new(g),
            tyres: Tyres::new(g.tyre, g.offroad, bank.grus.len(), bank.road_surfaces.clone()),
            impacts: Impacts::new(g.ranged, g.scream, bank.ranged.len(), bank.screams.len()),
            suspension: Suspension::new(
                [g.squeak, g.suspension, g.ranged],
                [bank.squeaks.len(), bank.bottomings.len(), bank.ranged.len()],
            ),
            drive_shafts: DriveShafts::new(g.drive_shaft, bank.drive_shafts.len()),
            exhaust: Exhaust::new(
                [g.exhaust_pop, g.pop, g.bang, g.fire],
                [bank.exhaust_pops.len(), bank.pops.len(), bank.bangs.len()],
            ),
            codriver: CoDriver::default(),
            custom: None,
            world: World::new(bank.world_gains, sky, spheres),
            scripted: Scripted::new(bank.scripted_gains),
            effects: Effects::new(bank.effect_gains, bank.dog_rings.len()),
            fmod: None,
            rng: Rng::new(0x51d2),
            frame: 0.0,
        }
    }

    /// The commands for one step's inputs, with the camera and the car at `scene`.
    pub fn step(&mut self, inputs: &Inputs, scene: &Scene) -> Vec<Command> {
        let sound = &inputs.sound;
        let inside = scene.inside;
        let rng = &mut self.rng;
        let ngp = &inputs.ngp;
        // The world first: its listener command places the car voices that follow.
        let mut out = self.world.step(scene, STEP, || rng.rand());
        // A door stays off once it has come off (DET1 leaves out a sleeping part).
        self.car.doors_off = std::array::from_fn(|i| self.car.doors_off[i] || inputs.doors_off[i]);
        // A game frame in this step (at most one: a step is shorter than a frame).
        self.frame += STEP * tyre::FRAME_RATE;
        let frame = self.frame >= 1.0;
        if frame {
            self.frame -= 1.0;
        }
        self.car.gear_click = self.fmod.as_ref().and_then(FmodCar::gear_click);
        out.extend(self.car.step(sound, inside, ngp, frame));
        let car_distance = (0..3).map(|i| (scene.car[i] - scene.listener[i]).powi(2)).sum::<f32>().sqrt();
        out.extend(self.tyres.step(
            &sound.tyres,
            &inputs.splash,
            sound.speed,
            STEP,
            inside,
            (car_distance, scene.listener),
            || rng.rand(),
        ));
        out.extend(self.impacts.step(&sound.hit, inside, || rng.rand()));
        let wheels = std::array::from_fn(|k| tyre::distance(&sound.tyres[k], scene.listener));
        let (suspension, mut calls) = self.suspension.step(sound, inside, wheels, STEP, || rng.rand());
        if frame {
            out.extend(self.drive_shafts.frame(sound, wheels, 1.0 / tyre::FRAME_RATE, || rng.rand()));
        }
        let min_rpm = ngp.min_rpm;
        let (exhaust, fire) = self.exhaust.step(sound, min_rpm, STEP, || rng.rand());
        out.extend(suspension.into_iter().chain(exhaust));
        let stopped = EngineNoise::from_payload(&sound.engine, min_rpm).stopped;
        let precipitation = self.world.sky.precipitation;
        out.extend(self.effects.step(sound, stopped, inside, precipitation, STEP, || rng.rand()));
        calls.extend(fire);
        calls.iter_mut().for_each(|c| *c = ngp_call(*c));
        let callouts = if self.custom.is_some() { &[][..] } else { &inputs.callouts };
        out.extend(self.codriver.step(sound, callouts, inputs.countdown));
        if let (Some(custom), Some(x)) = (&mut self.custom, inputs.driveline) {
            out.extend(custom.step(x, sound.speed, || rng.rand()));
        }
        out.extend(self.codriver.damage_calls(&calls));
        if frame {
            out.push(Command::Frame);
        }
        out.push(Command::SpeechGap(if inputs.race_time.is_some_and(|t| t <= 0.0) { SPEECH_GAP } else { 0.0 }));
        if let Some(fmod) = &mut self.fmod {
            out.retain(|c| !fmod.replaces(c));
            out.iter_mut().for_each(|c| fmod.mute(c));
            out.push(Command::Gain { voice: voice::Loop::Engine, gain: 0.0 });
            out.push(Command::Reflections { gain: 0.0, doors_off: self.car.doors_off });
            // NGP sends the daemon its block in app states 1 and 8 only, not in the start orbit (app state 10), where the
            // HUD's countdown has not started.
            let sent = !inputs.countdown.is_some_and(|c| c >= rbr_gameplay::race_clock::COUNTDOWN);
            out.extend(fmod.step(inputs, scene, stopped, sent));
        }
        out
    }

    /// The commands for the scripted events of a step (the helicopter, bird strikes, the rescue call).
    pub fn events(&mut self, events: &Events, scene: &Scene) -> Vec<Command> {
        self.scripted.step(scene, events)
    }
}

/// One car's sound, playing on the audio device.
pub struct Player {
    models: Models,
    mixer: Arc<Mutex<Mixer>>,
    scene: Scene,
    output: device::Output,
}

impl Player {
    /// Car `car`'s (the `Cars.ini` index) sound at `volume` (linear, 1 = the game's), under `sky`, with the
    /// co-driver's pace notes or not, and the player's group volumes (`mixer::Volumes`).
    pub fn start(
        data: &Data,
        car: u32,
        volume: f32,
        sky: Sky,
        codriver: bool,
        volumes: Volumes,
    ) -> Result<Player, String> {
        let bank = CarBank::load(data, car)?;
        let (output, mixer) = device::Output::start(|rate| {
            let mut mixer = Mixer::new(&bank, rate, volume);
            mixer.volumes = volumes;
            mixer
        })?;
        let mut models = Models::new(&bank, sky, Vec::new());
        models.codriver.muted = !codriver;
        Ok(Player { models, mixer, scene: Scene::default(), output })
    }

    /// One physics step's inputs.
    pub fn step(&mut self, inputs: &Inputs) {
        let commands = self.models.step(inputs, &self.scene);
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.apply(&commands);
        }
    }

    /// The scripted events since the last call (`rbr_render::characters`).
    pub fn events(&mut self, events: &Events) {
        let commands = self.models.events(events, &self.scene);
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.apply(&commands);
        }
    }

    /// Plays the car's FMOD bank (`fmod`) instead of its 1.02 engine, turbo and transmission voices. The loop
    /// waves are cloned and the mixing tree is built before taking the mixer lock; only the swap runs under it,
    /// and the old bank drops after it is released, so the audio callback never waits on the work.
    pub fn use_fmod(&mut self, car: FmodCar) {
        let rate = self.mixer.lock().map(|mixer| mixer.rate()).unwrap_or(44100);
        let prepared = Mixer::prepare_fmod(car.loop_waves(), car.waves.clone(), &car.buses, rate);
        let old = if let Ok(mut mixer) = self.mixer.lock() { Some(mixer.swap_fmod(prepared)) } else { None };
        drop(old);
        self.models.fmod = Some(car);
    }

    /// Calls the notes with a co-driver mod instead of 1.02's co-driver, muted as the co-driver switch is. The
    /// samples are copied before taking the mixer lock; only the swap runs under it.
    pub fn use_codriver(&mut self, mut custom: CustomCoDriver) {
        let prepared: Vec<Arc<data::Wave>> = custom.samples().iter().cloned().map(Arc::new).collect();
        let old = if let Ok(mut mixer) = self.mixer.lock() { Some(mixer.swap_voice(prepared)) } else { None };
        drop(old);
        custom.muted = self.models.codriver.muted;
        self.models.custom = Some(custom);
    }

    /// Gives the H-shifter's grind its sample (`Loop::Grind`). The seamless loop is built before taking the mixer
    /// lock; only the swap runs under it.
    pub fn set_grind(&mut self, wave: data::Wave) {
        let prepared = Arc::new(crate::mixer::voices::seamless(&wave));
        drop(wave);
        let old = if let Ok(mut mixer) = self.mixer.lock() { Some(mixer.swap_grind(prepared)) } else { None };
        drop(old);
    }

    /// The H-shifter's grind at `pitch` (1 = the sample's rate), or silent.
    pub fn grind(&mut self, pitch: Option<f32>) {
        let (gain, pitch) = pitch.map_or((0.0, 1.0), |p| (GRIND_GAIN, p));
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.apply(&[Command::Loop { voice: voice::Loop::Grind, gain, pitch }]);
        }
    }

    /// Pauses or resumes the sound (the viewer's pause menu): the device stops asking the mixer, so every voice and
    /// the co-driver go on from where they stopped.
    pub fn set_paused(&self, paused: bool) {
        self.output.set_paused(paused);
    }

    /// Where the camera and the car are, and whether the camera is in the car (the in-car low-pass and the
    /// transmission voices).
    pub fn set_scene(&mut self, scene: Scene) {
        self.scene = scene;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_damper_and_fire_calls_are_swapped_only() {
        assert_eq!([7, 6, 8].map(ngp_call), [6, 7, 8]);
    }
}
