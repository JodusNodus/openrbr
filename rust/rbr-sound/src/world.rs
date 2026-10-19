//! The world's sounds around the car (`notes/marks-sound-ghost.md` section 2, `notes/sound.md` section 8): the wind
//! and the birds of `CAmbientSounds::Vf08` (0x4aaf50, `Vf44` 0x4ab1b0, `Vf48` 0x4ab310), the siren of the `.dls`
//! table-6 spheres (`E31_5d00` 0x4d5d00), and the rain of `E30Rain::Vf08` (0x4d17e0). Also where the car is from the
//! camera, which the mixer takes the car voices' `SOUNDDEF` distance gains from. Pure: commands per step.
//!
//! Positions are the physics frame's (z up), as the car pose and the `.dls` spheres are.

use crate::voice::{Command, Loop, Shot};

/// The ambient volume reaches 1 at this car distance (0x72bf10), rising by 0.01 per metre (0x72f440).
const AMBIENT_FULL: f32 = 100.0;
const AMBIENT_PER_METRE: f32 = 0.01;
/// The wind's pitch per cloud level (`Vf48` 0x730888).
const WIND_PITCH: f32 = 0.25;
/// A bird plays only with the car farther than this from the camera (0x7304a0).
const BIRD_DISTANCE: f32 = 50.0;
/// The bird timer's first value (0x72ae44) and its base after each bird (0x72ad94).
const BIRD_FIRST: f32 = 5.0;
const BIRD_BASE: f32 = 2.0;
/// The bird samples (`bird1` to `bird7`).
pub const BIRDS: usize = 7;
/// The siren needs the car above this speed, km/h. It plays from 1 m to 30 m under NGP (`E31_5bf0`: `+0x78` 1 at
/// 0x4d5c84, then `+0x74` 30 at 0x4d5c91); 1.02 keeps the last as the min, full volume within 30 m.
const SIREN_SPEED: f32 = 70.0;
const SIREN_DISTANCES: (f32, f32) = (30.0, DEFAULT_MAX_DISTANCE);
const NGP_SIREN_DISTANCES: (f32, f32) = (1.0, 30.0);
/// The rain's gain with the camera in the car and outside (`E30Rain::Vf08`, 0x795e14 and 0x72ad94).
const RAIN_INSIDE: f32 = 1.0;
const RAIN_OUTSIDE: f32 = 2.0;

/// DirectSound's default max distance (`DS3D_DEFAULTMAXDISTANCE`), which every 1.02 voice keeps: `E30DXSound`'s
/// `+0x74` (0x4d2940) and `+0x78` (0x4d2970) both call the 3D buffer's `SetMinDistance` (`+0x44`), so of a min and a
/// max the last call's value is the min. NGP patches `+0x74` to `SetMaxDistance` (0x4d2961), which gives the pairs
/// as called.
pub const DEFAULT_MAX_DISTANCE: f32 = 1e9;

/// A voice's gain at `distance` from the camera, with DirectSound's default rolloff: full up to `min`, falling as
/// `min / distance`, held from `max` on.
pub fn distance_gain(distance: f32, (min, max): (f32, f32)) -> f32 {
    min / distance.clamp(min, max.max(min))
}

/// The side a source at `at` is on for a camera at `from` whose right is `right`: -1 left to 1 right, going to the
/// middle within 2 m (ours: a plain balance, not DirectSound's 3D panning).
pub fn pan(from: [f32; 3], right: [f32; 3], at: [f32; 3]) -> f32 {
    let d = sub(at, from);
    (dot(d, right) / length(d).max(2.0)).clamp(-1.0, 1.0)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

/// The camera and the car at one step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scene {
    /// The camera's position and its right, physics frame.
    pub listener: [f32; 3],
    pub right: [f32; 3],
    /// The car's position and velocity (m/s).
    pub car: [f32; 3],
    pub velocity: [f32; 3],
    /// The camera is in the car (`g_8d5338[0x41bb]`).
    pub inside: bool,
    /// A replay is playing (app state 8 and `E19IsState2`): the birds and the far wind play then. Rust has no
    /// replay; the offline render's trackside camera stands for its TV cameras.
    pub replay: bool,
    /// The active camera's id (app `+1000`) and whether it follows the car (`E12_Classify` -14 or -16).
    pub camera: u32,
    pub tracking: bool,
}

impl Default for Scene {
    /// The camera in the car, at the origin.
    fn default() -> Scene {
        Scene {
            listener: [0.0; 3],
            right: [1.0, 0.0, 0.0],
            car: [0.0; 3],
            velocity: [0.0; 3],
            inside: true,
            replay: false,
            camera: 0,
            tracking: true,
        }
    }
}

/// A table-6 sphere (`E12_Spheres`): centre and radius, physics frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    pub center: [f32; 3],
    pub radius: f32,
}

/// The `audio.ini` amps of the world's sounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldGains {
    pub wind: f32,
    pub bird: f32,
    /// The `Horn` amp (`g_8d5338[0x41cb]`), which the siren takes.
    pub horn: f32,
    pub rain: f32,
}

impl Default for WorldGains {
    /// The retail values.
    fn default() -> WorldGains {
        WorldGains { wind: 0.5, bird: 0.25, horn: 0.5, rain: 1.0 }
    }
}

/// The stage's weather as the sounds read it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sky {
    /// The cloud id, 0 clear to 3 heavy cloud (`CWeatherDaemon::M9c90`).
    pub cloud: u32,
    /// The rain amount (`E25AppObj::M9b80`, 50 light or 100 heavy); 0 for none and for snow (`M9e60`).
    pub rain: f32,
    /// The precipitation amount, rain or snow (`M9b80`).
    pub precipitation: f32,
}

impl Sky {
    /// From the season row's weather id (crisp ... heavyfog, `notes/weather.md`) and cloud id.
    pub fn from_weather(weather: u32, cloud: u32) -> Sky {
        let precipitation = match weather {
            3 | 6 => 50.0,
            4 | 7 => 100.0,
            _ => 0.0,
        };
        let rain = if matches!(weather, 3 | 4) { precipitation } else { 0.0 };
        Sky { cloud, rain, precipitation }
    }
}

/// The world sounds' state between steps.
#[derive(Debug, Clone)]
pub struct World {
    pub gains: WorldGains,
    pub sky: Sky,
    pub spheres: Vec<Sphere>,
    /// Seconds to the next bird, and the next bird.
    bird_timer: f32,
    next_bird: usize,
    /// The last camera id, the siren's played flag, and where the siren plays.
    camera: Option<u32>,
    played: bool,
    siren: Option<[f32; 3]>,
    /// The wind voice runs.
    wind: bool,
    /// NGP's sound patches apply ([`crate::Inputs::ngp`]).
    pub ngp: bool,
}

impl World {
    pub fn new(gains: WorldGains, sky: Sky, spheres: Vec<Sphere>) -> World {
        World {
            gains,
            sky,
            spheres,
            bird_timer: BIRD_FIRST,
            next_bird: 0,
            camera: None,
            played: false,
            siren: None,
            wind: false,
            ngp: false,
        }
    }

    /// The commands for one step of `dt` seconds; `rand` stands for the original's `rand()`.
    pub fn step(&mut self, scene: &Scene, dt: f32, mut rand: impl FnMut() -> u32) -> Vec<Command> {
        let d = length(sub(scene.car, scene.listener));
        let mut out = vec![Command::Listener { car_distance: d, car_pan: pan(scene.listener, scene.right, scene.car) }];
        let ambient = if d < AMBIENT_FULL { d * AMBIENT_PER_METRE } else { 1.0 };
        let speed = length(scene.velocity);

        // Vf08 runs only in a replay (app state 8; the race stays in state 1, by the harness's state log): the wind
        // then plays outside the car by the cloud and is left as it is inside. Its in-car wind by the car's speed is
        // for state 8 without `E19IsState2`, which Rust has no case of.
        if scene.replay && !scene.inside {
            let gain = self.gains.wind * ambient;
            if gain > 0.0 || self.wind {
                let pitch = (self.sky.cloud + 1) as f32 * WIND_PITCH;
                out.push(Command::Loop { voice: Loop::Wind, gain, pitch });
            }
            self.wind = gain > 0.0;
        }

        // The birds, in a replay with the car far from the camera. Ours: the original keeps the timer in an int
        // seeded with 5.0's bit pattern, so no bird ever plays (notes/marks-sound-ghost.md 2.2); this is the timer
        // as meant, 5 s and then 2 to 5 s.
        if scene.replay {
            self.bird_timer -= dt;
            if self.bird_timer < 0.0 {
                self.bird_timer = (rand() & 3) as f32 + BIRD_BASE;
                if d > BIRD_DISTANCE {
                    out.push(Command::Shot { shot: Shot::Bird(self.next_bird), gain: self.gains.bird * ambient });
                    self.next_bird = (self.next_bird + 1) % BIRDS;
                }
            }
        }

        // E31_5d00's siren: on a change to a camera that does not follow the car, inside a sphere, with the car
        // above 70 km/h; every other camera change clears the played flag. Ours: the siren plays at the sphere's
        // centre and stops at the next camera change (the original loops it with no position set).
        if self.camera != Some(scene.camera) {
            self.camera = Some(scene.camera);
            if self.siren.take().is_some() {
                out.push(Command::Loop { voice: Loop::Siren, gain: 0.0, pitch: 1.0 });
            }
            if self.played || speed * 3.6 <= SIREN_SPEED {
                self.played = false;
            } else if !scene.tracking {
                let inside = |s: &&Sphere| length(sub(scene.listener, s.center)) < s.radius;
                if let Some(sphere) = self.spheres.iter().find(inside) {
                    self.siren = Some(sphere.center);
                    self.played = true;
                }
            }
        }
        if let Some(at) = self.siren {
            let range = if self.ngp { NGP_SIREN_DISTANCES } else { SIREN_DISTANCES };
            let gain = self.gains.horn * distance_gain(length(sub(at, scene.listener)), range);
            out.push(Command::Loop { voice: Loop::Siren, gain, pitch: 1.0 });
            out.push(Command::Pan { voice: Loop::Siren, pan: pan(scene.listener, scene.right, at) });
        }

        // E30Rain: with rain (not snow) falling, louder outside the car.
        if self.sky.rain > 0.0 {
            let gain = self.gains.rain * if scene.inside { RAIN_INSIDE } else { RAIN_OUTSIDE };
            out.push(Command::Rain { gain, amount: self.sky.rain });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(d: f32) -> Scene {
        Scene { listener: [0.0, -d, 0.0], car: [0.0; 3], inside: false, replay: true, ..Scene::default() }
    }

    fn find(out: &[Command], voice: Loop) -> Option<(f32, f32)> {
        out.iter().find_map(|c| match *c {
            Command::Loop { voice: v, gain, pitch } if v == voice => Some((gain, pitch)),
            _ => None,
        })
    }

    #[test]
    fn distance_gain_is_full_inside_min_and_held_past_max() {
        assert_eq!(distance_gain(2.0, (10.0, 40.0)), 1.0);
        assert_eq!(distance_gain(20.0, (10.0, 40.0)), 0.5);
        assert_eq!(distance_gain(400.0, (10.0, 40.0)), 0.25);
    }

    #[test]
    fn pan_follows_the_cameras_right() {
        assert_eq!(pan([0.0; 3], [1.0, 0.0, 0.0], [10.0, 0.0, 0.0]), 1.0);
        assert_eq!(pan([0.0; 3], [1.0, 0.0, 0.0], [-10.0, 0.0, 0.0]), -1.0);
        assert_eq!(pan([0.0; 3], [1.0, 0.0, 0.0], [0.5, 0.0, 0.0]), 0.25);
    }

    #[test]
    fn the_wind_plays_only_outside_the_car_in_a_replay_by_the_cloud() {
        let mut world = World::new(WorldGains::default(), Sky { cloud: 1, ..Sky::default() }, Vec::new());
        let car = Scene { velocity: [0.0, 20.0, 0.0], ..Scene::default() };
        assert_eq!(find(&world.step(&car, 0.01, || 0), Loop::Wind), None);
        let chase = Scene { inside: false, ..car };
        assert_eq!(find(&world.step(&chase, 0.01, || 0), Loop::Wind), None);
        let (gain, pitch) = find(&world.step(&scene(40.0), 0.01, || 0), Loop::Wind).unwrap();
        assert!((gain - 0.2).abs() < 1e-6 && pitch == 0.5);
    }

    #[test]
    fn birds_play_in_turn_with_the_car_far_away() {
        let mut world = World::new(WorldGains::default(), Sky::default(), Vec::new());
        let birds = |out: Vec<Command>| -> Vec<Shot> {
            out.into_iter()
                .filter_map(|c| match c {
                    Command::Shot { shot, .. } => Some(shot),
                    _ => None,
                })
                .collect()
        };
        assert!(birds(world.step(&scene(80.0), 4.99, || 0)).is_empty());
        assert_eq!(birds(world.step(&scene(80.0), 0.02, || 1)), [Shot::Bird(0)]);
        // The next timer is 1 + 2 s; near the car the timer runs but no bird plays.
        assert!(birds(world.step(&scene(20.0), 3.01, || 0)).is_empty());
        assert!(birds(world.step(&scene(80.0), 1.0, || 0)).is_empty());
        assert_eq!(birds(world.step(&scene(80.0), 1.01, || 0)), [Shot::Bird(1)]);
    }

    #[test]
    fn the_siren_plays_on_a_camera_change_into_a_sphere_at_speed() {
        let sphere = Sphere { center: [0.0, 0.0, 0.0], radius: 100.0 };
        let mut world = World::new(WorldGains::default(), Sky::default(), vec![sphere]);
        let at = |camera: u32, speed: f32| Scene {
            listener: [10.0, 0.0, 0.0],
            car: [0.0, 50.0, 0.0],
            velocity: [0.0, speed / 3.6, 0.0],
            inside: false,
            camera,
            tracking: false,
            ..Scene::default()
        };
        assert_eq!(find(&world.step(&at(1, 60.0), 0.01, || 0), Loop::Siren), None);
        // 10 m from the sphere: within 1.02's min of 30 m.
        let out = world.step(&at(2, 100.0), 0.01, || 0);
        assert_eq!(find(&out, Loop::Siren), Some((0.5, 1.0)));
        assert!(out.contains(&Command::Pan { voice: Loop::Siren, pan: -1.0 }));
        // The next change clears the played flag and stops the siren; the one after plays it again, under NGP from
        // 1 m.
        assert_eq!(find(&world.step(&at(3, 100.0), 0.01, || 0), Loop::Siren), Some((0.0, 1.0)));
        world.ngp = true;
        assert_eq!(find(&world.step(&at(4, 100.0), 0.01, || 0), Loop::Siren), Some((0.05, 1.0)));
    }

    #[test]
    fn rain_falls_for_the_rain_weathers_only() {
        assert_eq!(Sky::from_weather(3, 2).rain, 50.0);
        assert_eq!(Sky::from_weather(7, 3).rain, 0.0);
        let mut world = World::new(WorldGains::default(), Sky::from_weather(4, 3), Vec::new());
        assert!(world.step(&Scene::default(), 0.01, || 0).contains(&Command::Rain { gain: 1.0, amount: 100.0 }));
    }
}
