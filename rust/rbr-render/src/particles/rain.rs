//! The game's rain and snow: `NParticleSystem::CEmitterRain` (`Init` 0x5daa90, `Step` 0x5da450, the camera handler
//! 0x5da2f0), which `E83System::LoadFile` (0x5dffd0) sets up after the file when the weather has precipitation, not the
//! files' `g_rain_emitter`/`g_snow_emitter` (their pools hold one particle). Physics frame, z up.
//!
//! Each instance is a fixed set of particles in a box about the camera whose positions are 16-bit integers: they wrap,
//! so the box is a torus that moves with the camera while its particles stay put in the world. The instances, from
//! `LoadFile` (full detail):
//!
//! | class | particles | fall m/s | jitter | wind | camera trail | box m |
//! |---|---|---|---|---|---|---|
//! | `g_rain_particleclass` | 1000 | 25 | 2 | 0.2 | 10 | 15 |
//! | `g_rain2_particleclass` | 20 | 8 | 1 | 0.2 | 0 | 15 |
//! | `g_snow_particleclass` | 200 | 5 | 2 | 0.3 | 0 | 10 |
//! | `g_snow2_particleclass` (amount over 75) | 10 | 4 | 1 | 0.3 | 0 | 10 |
//! | `g_snow3_particleclass` | 100 | 5 | 2 | 0.3 | 0 | 10 |
//!
//! Faithful: the instances; each particle's velocity drawn once (the weather daemon's wind times the wind factor, plus
//! a jitter of +-half the jitter across, minus the fall); the box half a box ahead of the camera and raised 0.15 of it,
//! 0.8 of it tall; the integer motion (the camera's motion taken off, none after a jump over 100 m/s); the age from the
//! height (0 at the top, 1 at the bottom, the class's life set to 1); the trail velocity, the particle's minus the
//! camera's motion times the camera-trail factor; rain shows `amount * n / 100` of its particles, snow all of them
//! (`E25AppObj::M9e60`). Ours: the camera's motion for the trail is taken over a 60 fps frame (the game uses its own
//! frame's), and the draws come from our generator.

use glam::Vec3;
use rbr_formats::pss::ParticleFile;

use super::sim::{Particle, Rng, Simulation};
use super::Precipitation;

/// One `CEmitterRain::Init`'s arguments.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Kind {
    class: &'static str,
    count: usize,
    fall: f32,
    jitter: f32,
    wind: f32,
    trail: f32,
    /// `Init`'s last argument: a 15 m box, else 10 m.
    wide: bool,
}

const RAIN: [Kind; 2] = [
    Kind { class: "g_rain_particleclass", count: 1000, fall: 25.0, jitter: 2.0, wind: 0.2, trail: 10.0, wide: true },
    Kind { class: "g_rain2_particleclass", count: 20, fall: 8.0, jitter: 1.0, wind: 0.2, trail: 0.0, wide: true },
];
const SNOW: [Kind; 3] = [
    Kind { class: "g_snow_particleclass", count: 200, fall: 5.0, jitter: 2.0, wind: 0.3, trail: 0.0, wide: false },
    Kind { class: "g_snow2_particleclass", count: 10, fall: 4.0, jitter: 1.0, wind: 0.3, trail: 0.0, wide: false },
    Kind { class: "g_snow3_particleclass", count: 100, fall: 5.0, jitter: 2.0, wind: 0.3, trail: 0.0, wide: false },
];
/// `g_snow2` only above this amount.
const SNOW2_AMOUNT: f32 = 75.0;
/// A camera faster than this (m/s) jumped: its motion is not taken off (0x72bf10).
const JUMP_SPEED: f32 = 100.0;
/// The box's height over its width (0x74d1a0 against 0x74d1a4) and its lift (0x72d154).
const HEIGHT: f32 = 0.8;
const LIFT: f32 = 0.15;
/// Ours: the game frame the trail's camera motion is taken over.
const TRAIL_FRAME: f32 = super::GAME_FRAME;

/// One instance: its pool in the simulation (drawn from there) and its particles.
#[derive(Debug, Clone)]
struct Emitter {
    pool: usize,
    kind: Kind,
    /// The particles shown.
    shown: usize,
    positions: Vec<[i16; 3]>,
    velocities: Vec<Vec3>,
    /// The box's centre at the last step.
    last: Option<Vec3>,
}

/// The weather's instances.
#[derive(Debug, Clone, Default)]
pub struct Rain {
    emitters: Vec<Emitter>,
}

impl Rain {
    /// The instances `LoadFile` makes for `weather` (amount `intensity * 100`), with their pools added to `sim`;
    /// `wind` is the weather daemon's wind when they are made. None for a class the file lacks.
    pub fn new(file: &ParticleFile, sim: &mut Simulation, weather: Precipitation, wind: Vec3, seed: u32) -> Rain {
        let amount = weather.intensity * 100.0;
        let kinds: &[Kind] = if weather.snow { &SNOW } else { &RAIN };
        let mut rng = Rng::new(seed);
        let emitters = kinds
            .iter()
            .filter(|k| k.class != SNOW[1].class || amount > SNOW2_AMOUNT)
            .filter_map(|&kind| {
                let pool = sim.add_class(file, kind.class, kind.count)?;
                sim.pools[pool].def.look.life = 1.0;
                let shown = if weather.snow { kind.count } else { (amount * kind.count as f32 * 0.01) as usize };
                let positions =
                    (0..kind.count).map(|_| std::array::from_fn(|_| (rng.unit() * 65535.0) as i32 as i16)).collect();
                let velocities = (0..kind.count)
                    .map(|_| {
                        let x = (rng.unit() - 0.5) * kind.jitter;
                        let y = (rng.unit() - 0.5) * kind.jitter;
                        wind * kind.wind + Vec3::new(x, y, -kind.fall)
                    })
                    .collect();
                Some(Emitter { pool, kind, shown, positions, velocities, last: None })
            })
            .collect();
        Rain { emitters }
    }

    /// Back to no camera motion (after a jump in the drive).
    pub fn reset(&mut self) {
        for e in &mut self.emitters {
            e.last = None;
        }
    }

    /// One step of `dt` seconds with the camera at `eye` looking along `forward`: moves the particles and gives their
    /// pools the shown ones.
    pub fn step(&mut self, sim: &mut Simulation, eye: Vec3, forward: Vec3, dt: f32) {
        for e in &mut self.emitters {
            let size = if e.kind.wide { 15.0 } else { 10.0 };
            let centre = eye + forward * size * 0.5;
            let mut moved = e.last.map_or(Vec3::ZERO, |last| centre - last);
            e.last = Some(centre);
            if moved.length() / dt > JUMP_SPEED {
                moved = Vec3::ZERO;
            }
            let unit = size / 65536.0;
            let origin = centre + Vec3::Z * size * LIFT;
            let trail = moved / dt * TRAIL_FRAME * e.kind.trail;
            let particles = &mut sim.pools[e.pool].particles;
            particles.clear();
            for (p, v) in e.positions.iter_mut().zip(&e.velocities).take(e.shown) {
                let d = (*v * dt - moved) / unit;
                for (axis, d) in p.iter_mut().zip(d.to_array()) {
                    *axis = axis.wrapping_add(d as i32 as i16);
                }
                let at = Vec3::new(p[0] as f32 * unit, p[1] as f32 * unit, p[2] as f32 * unit * HEIGHT);
                particles.push(Particle {
                    position: origin + at,
                    velocity: *v - trail,
                    age: 0.5 - p[2] as f32 / 65536.0,
                    life: 1.0,
                    scale: 1.0,
                    floor: f32::MIN,
                });
            }
        }
    }

    /// Drops every instance (a renderer that draws its own rain and snow).
    pub fn clear(&mut self, sim: &mut Simulation) {
        for e in self.emitters.drain(..) {
            sim.pools[e.pool].particles.clear();
        }
    }

    /// The particles shown.
    pub fn shown(&self) -> usize {
        self.emitters.iter().map(|e| e.shown).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::pss;

    fn file() -> ParticleFile {
        let class = |name: &str| {
            format!("{name} 26 0.68 0.68 0.68 0.2 0.5 0.56 0.56 0.56 0.2 0.56 0.56 0.56 0.2 0 0 0 0 0 0 0.5 0 0.6 4 0.035 1 0 0 BLEND_BLEND 0 0 0 0 ")
        };
        let names = ["g_rain", "g_rain2", "g_snow", "g_snow2", "g_snow3"];
        let classes: String = names.iter().map(|n| class(&format!("{n}_particleclass"))).collect();
        let text = format!("version3 1 0 0 1 0 1 0 1 0 0 1 1 1 1 1 1 10 5 {classes}0 0 10 0 0 0 0 0 0 0 $");
        pss::parse(&text).unwrap()
    }

    #[test]
    fn rain_falls_in_a_box_that_follows_the_camera() {
        let file = file();
        let mut sim = Simulation::new(&file, 1);
        let light = Precipitation { snow: false, intensity: 0.5 };
        let mut rain = Rain::new(&file, &mut sim, light, Vec3::ZERO, 7);
        assert_eq!(rain.shown(), 500 + 10, "light rain shows half of each instance");
        let dt = rbr_recording::STEP_SECONDS;
        let mut eye = Vec3::new(100.0, 50.0, 10.0);
        rain.step(&mut sim, eye, Vec3::X, dt);
        let pool = sim.find("g_rain_particleclass").unwrap();
        let before: Vec<Particle> = sim.pools[pool].particles.clone();
        assert_eq!(before.len(), 500);
        let centre = eye + Vec3::X * 7.5 + Vec3::Z * 15.0 * LIFT;
        for p in &before {
            let d = p.position - centre;
            assert!(d.x.abs() <= 7.5 && d.y.abs() <= 7.5 && d.z.abs() <= 6.0, "{d}");
            assert!((0.0..=1.0).contains(&p.age));
        }
        // The camera moves 0.1 m along x: the particles keep their world places apart from their fall.
        eye.x += 0.1;
        rain.step(&mut sim, eye, Vec3::X, dt);
        let after = &sim.pools[pool].particles;
        let wrapped = |a: f32, b: f32, span: f32| ((a - b) + span * 0.5).rem_euclid(span) - span * 0.5;
        for (a, b) in before.iter().zip(after) {
            assert!(wrapped(b.position.x, a.position.x, 15.0).abs() < 0.03, "x kept");
            let fall = wrapped(b.position.z, a.position.z, 12.0);
            assert!((fall + 25.0 * dt * HEIGHT).abs() < 0.01, "falls 25 m/s, seen 0.8 of it: {fall}");
        }
        // Rain's trail takes the camera's motion off ten times over a 60 fps frame.
        let trail = 0.1 / dt * TRAIL_FRAME * 10.0;
        assert!((after[0].velocity.x - (rain.emitters[0].velocities[0].x - trail)).abs() < 1e-4);
    }

    #[test]
    fn heavy_snow_has_its_second_instance_and_shows_all() {
        let file = file();
        let shown = |intensity: f32| {
            let mut sim = Simulation::new(&file, 1);
            Rain::new(&file, &mut sim, Precipitation { snow: true, intensity }, Vec3::ZERO, 1).shown()
        };
        assert_eq!(shown(0.5), 200 + 100, "light snow: every particle of g_snow and g_snow3");
        assert_eq!(shown(1.0), 200 + 10 + 100, "heavy snow adds g_snow2");
    }
}
