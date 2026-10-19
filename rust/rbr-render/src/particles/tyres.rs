//! The tyre emitters: each wheel's `CSigTireParticles` values through the file's signal handlers into emission
//! (`E83System::EmitTire` 0x5e3050, `E82F_Post::Run` 0x5de950, `E82F_BlkD0::Emit` 0x5ddba0), and the fake-smoke dust
//! clouds behind the car.
//!
//! Faithful: which handlers a surface runs (its own and its alias's), the handler's rate `(mass + reduction) * modifier
//! + (smoke + reduction) * modifier + (speed + reduction) * modifier` with its perturbations, the limit, the smoothing by
//! the equalization, and the launch velocity (the signal's velocity times the velocity modifier, plus the car's
//! velocity times the car-speed factor), and the count: `rate * frame time` particles per physics step, [`RATE_SCALE`] at
//! 60 frames a second. Ours: the stones' count and the whole fake-smoke rate (its use in
//! `E83System::Update` is not traced).

use std::collections::HashMap;

use glam::Vec3;
use rbr_formats::pss::ParticleFile;

use super::sim::{Rng, Simulation};
use super::wave::Waves;

/// One wheel's values for a physics step, from its `CSigTireParticles` payload (live) or made from the recorded
/// poses (playback). Physics frame, z up.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Wheel {
    pub wheel: usize,
    /// The contact patch's centre, and its two edges (+0x10, +0x20).
    pub at: Vec3,
    pub edges: [Vec3; 2],
    /// The payload's velocity (+0x50).
    pub velocity: Vec3,
    /// Mass and smoke flow (+0x88, +0x84).
    pub mass: f32,
    pub smoke: f32,
    /// The water level (+0x98).
    pub splash: f32,
    /// The surface id under the patch (`E36Triple::MinPick`).
    pub surface: u32,
    pub contact: bool,
}

/// Emitted particles per unit of handler rate per second of physics. `E82F_BlkD0::Emit` throws `rate * [system +0x94]`
/// a payload, `+0x94` the last frame's time (`E83System::Update` 0x5e2850 stores its `dt` there), and the game signals
/// every physics step: at [`super::GAME_FRAME`] that is `rate / 60` a 7 ms step. One live answer is one such step
/// (under NGP one 7 ms frame with each wheel's last payload), so the count holds for both. Ours: stones get one, at which
/// their pool of the file's size is not used up before they land.
pub const RATE_SCALE: f32 = super::GAME_FRAME / rbr_recording::STEP_SECONDS;
/// Ours: fake-smoke particles per unit of its rate per second.
const FAKE_SMOKE_SCALE: f32 = 40.0;
/// `E82F_Emit::Run` (0x5de140): each wheel's water level follows its payload's (+0x98) by 0.3 a payload
/// (`g_7d6488`); over 1e-4 on the ground it throws the splash and its smoke with particle counts and launch speeds
/// (sideways out of the car, up) from tables keyed by the level (0x7d649c; 0x74d73c to 0x74d7f0).
const WATER_FOLLOW: f32 = 0.3;
const WATER_LEAST: f32 = 1e-4;
pub const WATER_KEYS: [f32; 9] = [0.0, 0.01, 0.15, 0.2, 0.3, 0.35, 0.5, 0.6, 0.7];
const SPLASH_COUNT: [f32; 9] = [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
const SPLASH_SIDE: [f32; 9] = [0.0, 0.0, 0.25, 0.5, 2.0, 3.0, 4.0, 5.0, 6.0];
const SPLASH_UP: [f32; 9] = [0.0, 0.0, 0.0, 0.5, 1.0, 1.5, 2.0, 2.0, 2.0];
const SPLASH_SMOKE_COUNT: [f32; 9] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
const SPLASH_SMOKE_SIDE: [f32; 9] = [0.0, 0.0, 0.5, 0.7, 1.0, 1.5, 2.0, 2.0, 2.0];
const SPLASH_SMOKE_UP: [f32; 9] = [0.0, 0.0, 0.5, 0.7, 1.0, 1.5, 2.0, 2.0, 2.0];

/// `E82_TableLerp` over `keys`: the piecewise-linear value of `vals` at `x`, the last value past the last key.
pub fn table_lerp<const N: usize>(keys: &[f32; N], vals: &[f32; N], x: f32) -> f32 {
    for i in 1..N {
        if x <= keys[i] {
            let t = ((x - keys[i - 1]) / (keys[i] - keys[i - 1])).max(0.0);
            return (vals[i] - vals[i - 1]) * t + vals[i - 1];
        }
    }
    vals[N - 1]
}

#[derive(Debug, Clone)]
struct Handler {
    pool: usize,
    mass: [f32; 3],
    smoke: [f32; 3],
    speed: [f32; 2],
    equalization: f32,
    velocity: [f32; 2],
    flags: u32,
    car_speed_factor: f32,
    limit: f32,
}

#[derive(Debug, Clone)]
struct FakeSmoke {
    material: u32,
    pool: usize,
    dirt: [f32; 2],
    smoke: [f32; 2],
    speed: [f32; 2],
    speed_factor: f32,
}

/// The tyre handlers of a file, and each handler's state per wheel.
#[derive(Debug, Clone)]
pub struct Tyres {
    handlers: Vec<Handler>,
    /// The handlers of each surface id, in file order.
    by_surface: HashMap<u32, Vec<usize>>,
    aliases: HashMap<u32, u32>,
    fake_smoke: Vec<FakeSmoke>,
    /// The splash and splash-smoke pools (`g_splash_emitter`, `g_splash_smoke_emitter`).
    splash: [Option<usize>; 2],
    /// Each wheel's water level (system +0x668).
    pub water: [f32; 4],
    /// The water waves the wheels leave.
    pub waves: Waves,
    /// Per handler and wheel: the smoothed rate and where the wheel was at its last emission.
    state: Vec<[(f32, Option<Vec3>); 4]>,
    rng: Rng,
}

impl Tyres {
    pub fn new(file: &ParticleFile, sim: &Simulation, seed: u32) -> Tyres {
        let mut handlers = Vec::new();
        let mut by_surface: HashMap<u32, Vec<usize>> = HashMap::new();
        for h in &file.handlers {
            let Some(pool) = sim.find(&h.emitter) else { continue };
            by_surface.entry(h.material).or_default().push(handlers.len());
            handlers.push(Handler {
                pool,
                mass: h.mass_flow,
                smoke: h.smoke_flow,
                speed: [h.speed_reduction, h.speed_modifier],
                equalization: h.equalization,
                velocity: [h.velocity_modifier, h.velocity_perturbation],
                flags: h.flags,
                car_speed_factor: h.car_speed_factor,
                limit: h.limit,
            });
        }
        let fake_smoke = file
            .fake_smoke
            .iter()
            .filter_map(|f| {
                Some(FakeSmoke {
                    material: f.material,
                    pool: sim.find(&f.emitter)?,
                    dirt: f.dirt,
                    smoke: f.smoke,
                    speed: f.speed,
                    speed_factor: f.speed_factor,
                })
            })
            .collect();
        Tyres {
            state: vec![[(0.0, None); 4]; handlers.len()],
            handlers,
            by_surface,
            aliases: file.material_aliases.iter().copied().collect(),
            fake_smoke,
            splash: [sim.find("g_splash_emitter"), sim.find("g_splash_smoke_emitter")],
            water: [0.0; 4],
            waves: Waves::new(seed ^ 0x77),
            rng: Rng::new(seed),
        }
    }

    /// Whether a surface id has handlers (directly or through its alias).
    pub fn handles(&self, surface: u32) -> bool {
        self.by_surface.contains_key(&surface)
            || self.aliases.get(&surface).is_some_and(|alias| self.by_surface.contains_key(alias))
    }

    /// Feeds one physics step of `dt` seconds: every wheel through its surface's handlers (and its alias's), the
    /// fake smoke behind the car, and the splashes. `car_velocity` is the car's (the system's `+0x730`), `side` its x
    /// axis (the left).
    pub fn feed(&mut self, sim: &mut Simulation, wheels: &[Wheel], car_velocity: Vec3, side: Vec3, dt: f32) {
        let speed = car_velocity.length();
        let mut fed = vec![false; self.handlers.len()];
        for w in wheels.iter().filter(|w| w.wheel < 4) {
            let alias = self.aliases.get(&w.surface).copied().filter(|&a| a != w.surface);
            let list: Vec<usize> = [Some(w.surface), alias]
                .into_iter()
                .flatten()
                .flat_map(|s| self.by_surface.get(&s).cloned().unwrap_or_default())
                .collect();
            for k in list {
                fed[k] = true;
                self.run(sim, k, w, car_velocity, speed, dt);
            }
            self.splash(sim, w, side);
        }
        // A handler no wheel ran this step starts its next emission where the wheel is then.
        for (k, fed) in fed.into_iter().enumerate() {
            if !fed {
                self.state[k] = [(0.0, None); 4];
            }
        }
        self.fake(sim, wheels, car_velocity, speed, dt);
        self.waves.step(dt);
    }

    /// Back to dry wheels and no waves.
    pub fn clear(&mut self) {
        self.water = [0.0; 4];
        self.waves.clear();
    }

    /// `E82F_Emit::Run`'s splash for wheel `w`: the level follows the payload's water; the splash and its smoke leave
    /// the patch's edge (the +0x20 edge for the even wheels, the +0x10 for the odd), thrown out of the car's side.
    fn splash(&mut self, sim: &mut Simulation, w: &Wheel, side: Vec3) {
        let level = &mut self.water[w.wheel];
        *level += (w.splash - *level) * WATER_FOLLOW;
        let level = *level;
        if level <= WATER_LEAST || !w.contact {
            return;
        }
        let out = if w.wheel.is_multiple_of(2) { side } else { -side };
        let at = w.edges[1 - w.wheel % 2];
        self.waves.emit(w.wheel, level, [at, w.edges[w.wheel % 2]], out);
        let tables = [(SPLASH_COUNT, SPLASH_SIDE, SPLASH_UP), (SPLASH_SMOKE_COUNT, SPLASH_SMOKE_SIDE, SPLASH_SMOKE_UP)];
        for (pool, (count, across, up)) in self.splash.into_iter().zip(tables) {
            let Some(pool) = pool else { continue };
            let n = (table_lerp(&WATER_KEYS, &count, level) + 0.5).floor();
            let launch = out * table_lerp(&WATER_KEYS, &across, level) + Vec3::Z * table_lerp(&WATER_KEYS, &up, level);
            sim.emit(pool, at, at, launch, n, at.z);
        }
    }

    /// `E82F_BlkD0::Emit` for handler `k` and wheel `w`.
    fn run(&mut self, sim: &mut Simulation, k: usize, w: &Wheel, car_velocity: Vec3, speed: f32, dt: f32) {
        let h = &self.handlers[k];
        let mut rate = (w.mass + h.mass[0]) * h.mass[1] + (w.smoke + h.smoke[0]) * h.smoke[1];
        if h.speed[1] != 0.0 {
            rate += (speed + h.speed[0]) * h.speed[1];
        }
        if rate > 0.0 {
            rate += h.mass[2] * self.rng.signed() + h.smoke[2] * self.rng.signed();
        }
        rate = rate.min(h.limit);
        let (smoothed, last) = &mut self.state[k][w.wheel];
        *smoothed += (rate - *smoothed) * h.equalization;
        if *smoothed > 0.0 {
            let jitter = Vec3::new(self.rng.signed(), self.rng.signed(), self.rng.signed()) * h.velocity[1];
            let mut launch = w.velocity * h.velocity[0] + jitter;
            if h.flags & 1 != 0 {
                launch += car_velocity * h.car_speed_factor;
            }
            let stone = sim.pools[h.pool].def.look.stone;
            let from = last.unwrap_or(w.at);
            let scale = if stone { 1.0 } else { RATE_SCALE };
            sim.emit(h.pool, from, w.at, launch, *smoothed * scale * dt, w.at.z);
        }
        *last = Some(w.at);
    }

    /// The fake-smoke dust behind the car: per handler, the wheels on its surface give the mean mass ("dirt") and
    /// smoke; the rate is the sum of the three `(value + add) * mod` terms that are positive. Ours.
    fn fake(&mut self, sim: &mut Simulation, wheels: &[Wheel], car_velocity: Vec3, speed: f32, dt: f32) {
        for f in &self.fake_smoke {
            let on: Vec<&Wheel> = wheels
                .iter()
                .filter(|w| w.contact && (w.surface == f.material || self.aliases.get(&w.surface) == Some(&f.material)))
                .collect();
            if on.is_empty() {
                continue;
            }
            let n = on.len() as f32;
            let dirt = on.iter().map(|w| w.mass).sum::<f32>() / n;
            let smoke = on.iter().map(|w| w.smoke).sum::<f32>() / n;
            let term = |value: f32, [add, modifier]: [f32; 2]| ((value + add) * modifier).max(0.0);
            let rate = term(dirt, f.dirt) + term(smoke, f.smoke) + term(speed, f.speed);
            if rate <= 0.0 {
                continue;
            }
            let at = on.iter().map(|w| w.at).sum::<Vec3>() / n;
            let launch = car_velocity * f.speed_factor;
            sim.emit(f.pool, at, at, launch, rate * on.len() as f32 / 4.0 * FAKE_SMOKE_SCALE * dt, at.z - 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::pss;

    fn file() -> ParticleFile {
        let text = "version3 1 0 0 1 0 1 0 1 0 0 1 1 1 1 1 1 10 \
            1 pc 13 1 1 1 1 0 1 1 1 1 1 1 1 1 0.1 0.1 0.1 0.1 0.1 0.1 0 1 2 0.41 0 0 0 0 BLEND_BLEND 0 0 0.5 0 \
            1 ec 2 0.2 0.2 -3 0.3 0 0 0 0 0 1 pc \
            1 em ec 500 0 0 1 0 0 -9.81 0 0 0 \
            10 0 \
            1 17 em 0 5 0 0 0 0 0.5 0 0 1 0 -10 1 8 \
            0 1 6 17 0 1 17 0 0 0 0 -7 0.02 0.02 em 0 $";
        pss::parse(text).unwrap()
    }

    fn wheel(surface: u32, mass: f32) -> Wheel {
        Wheel { wheel: 0, surface, mass, contact: true, ..Wheel::default() }
    }

    #[test]
    fn handlers_run_for_their_surface_and_its_alias() {
        let file = file();
        let mut sim = Simulation::new(&file, 1);
        let mut tyres = Tyres::new(&file, &sim, 1);
        assert!(tyres.handles(17) && tyres.handles(6) && !tyres.handles(5));
        // The handler's speed term (speed - 10) keeps it quiet below 10 m/s.
        let car_velocity = Vec3::new(20.0, 0.0, 0.0);
        for _ in 0..100 {
            tyres.feed(&mut sim, &[wheel(5, 1.0)], car_velocity, Vec3::X, 0.01);
        }
        assert_eq!(sim.count(), 0, "no handler for surface 5");
        for _ in 0..100 {
            tyres.feed(&mut sim, &[wheel(6, 1.0)], car_velocity, Vec3::X, 0.01);
        }
        assert!(sim.count() > 0, "surface 6 is handled as 17");
    }

    #[test]
    fn the_rate_follows_mass_and_speed_up_to_the_limit() {
        let file = file();
        let count = |mass: f32, speed: f32| {
            let mut sim = Simulation::new(&file, 1);
            let mut tyres = Tyres::new(&file, &sim, 1);
            for _ in 0..100 {
                tyres.feed(&mut sim, &[wheel(17, mass)], Vec3::new(speed, 0.0, 0.0), Vec3::X, 0.01);
            }
            sim.pools[0].particles.len()
        };
        assert_eq!(count(0.0, 5.0), 0, "below the speed reduction and no mass flow");
        assert_eq!(count(1.0, 0.0), 0, "the speed term's reduction outweighs the mass flow");
        assert!(count(2.5, 0.0) > 0);
        assert!(count(2.5, 0.0) < count(3.0, 0.0));
        assert_eq!(count(10.0, 5.0), count(20.0, 6.0), "both at the limit");
    }

    #[test]
    fn the_water_tables_lerp() {
        assert_eq!(table_lerp(&WATER_KEYS, &SPLASH_SIDE, 0.0), 0.0);
        assert!((table_lerp(&WATER_KEYS, &SPLASH_SIDE, 0.25) - 1.25).abs() < 1e-6);
        assert_eq!(table_lerp(&WATER_KEYS, &SPLASH_SIDE, 5.0), 6.0);
    }
}
