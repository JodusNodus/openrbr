//! The car's own particles: debris and dust from impacts (`E83System::EmitDebris` 0x5df5f0), the engine fire and its
//! smoke (`CSigEngineFire`, `E82E_Sys::Set844` 0x5dd4c0), the engine steam (`E83System::Update`'s level at +0x838,
//! fed by `EmitTire` and `E82E_Sys::Scale838` 0x5dd400), the exhaust and its pops (`CSigExhaust`, `EmitExhaust`
//! 0x5dea10), and sparks (ours). Physics frame, z up.
//!
//! Faithful: the debris counts per emitter from the impact force (`g_e83DamageKeys` and the four count tables, with the
//! tilt rule), the debris origin (the record's +0x20), the launch plus 0.025 of the car's velocity (none for the
//! smoke), debris resting where it lands (`Update`'s ground test for emitters 14 to 17); the fire and fire-smoke rates
//! (the file's rate times the fire level, which stays once set); the steam level (+0.1 of each tyre payload's water,
//! `EngineNoise` word 5 times 20 when positive, falling by 2.5 a second, at most one particle a step, launched with half
//! the car's velocity); the exhaust (`CSigExhaust`, which the car's frame update `CCar::Vf28x` sends once a rendered
//! frame through `E43EngineSim::Sim8CF0` 0x518cf0): the flow is the engine state's +0x200 smoothed (0.7 new, 0.3
//! old), `g_exhaust_emitter` (or the arctic one) throws a particle each time twice the flow summed reaches 1,
//! `g_exhaust_broken_emitter` each time the broken flow does, and a pop level over 0.8 throws `(v - 0.8) * 25` pops (at
//! most 2), all at the exhaust point with 0.95 of the car's velocity.
//!
//! Ours: the engine strip is the car's `[Point_engine]` with 0.3 m either side across the car (the game's
//! `CAR_ENGINE_STRIP` points, system +0x7d0, are not traced); the exhaust's launch leaves out the car's spin (the game
//! adds its angular velocity crossed with the exhaust's offset); the exhaust runs once a live answer (a 7 ms frame,
//! under 1.02 and NGP alike) with a frame's sums scaled to [`super::GAME_FRAME`] ([`EXHAUST_FRAME`]); the pops take the
//! `EngineNoise` misfire word (the standalone has no frame update to send `CSigExhaust`, whose pop word is the engine's
//! +0x194, the misfire level, cleared after each send); sparks
//! (1.02 looks up `g_spark_emitter`, which no file defines, and the files' `g_car_sparks` class is empty, so the game
//! throws none): a look of our own on the files' `g_car_sparks_emitter`; and, in playback, impacts guessed from the
//! recorded speed ([`impact_from_speeds`]).

use glam::Vec3;

use super::sim::{Look, Simulation};

/// `g_e83DamageKeys` (0x74d814): the impact forces the count tables are keyed by.
pub const DAMAGE_KEYS: [f32; 8] = [0.0, 7430.0, 11487.0, 19688.0, 25636.0, 33162.0, 37781.0, 46000.0];
/// `g_e83DebrisA/B/C/Smoke` (0x74d834 to 0x74d894): particles per emitter by force.
pub const DEBRIS_COUNTS: [[f32; 8]; 4] = [
    [0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 4.0, 4.0],
    [0.0, 0.0, 1.0, 3.0, 4.0, 5.0, 5.0, 6.0],
    [0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 4.0, 4.0],
    [0.0, 7.0, 12.0, 18.0, 30.0, 30.0, 30.0, 30.0],
];
const DEBRIS_EMITTERS: [&str; 4] =
    ["g_debris_a_emitter", "g_debris_b_emitter", "g_debris_c_emitter", "g_debris_smoke_emitter"];
/// The car's velocity each debris emitter's particles take.
const DEBRIS_CAR_VELOCITY: [f32; 4] = [0.025, 0.025, 0.025, 0.0];
/// `E82E_Sys::Scale838`'s factor and `Update`'s steam rules: particles per level, the fall per second, the car
/// velocity taken.
const STEAM_PER_NOISE: f32 = 20.0;
const STEAM_PER_SPLASH: f32 = 0.1;
const STEAM_PER_LEVEL: f32 = 0.4;
const STEAM_FALL: f32 = 2.5;
const STEAM_CAR_VELOCITY: f32 = 0.5;
/// `EmitExhaust`'s pop rule: above 0.8, `(v - 0.8) * 25` pops, at most 2; launched with 0.95 of the car's velocity.
const POP_FROM: f32 = 0.8;
const POP_PER: f32 = 25.000002;
const POP_MOST: u32 = 2;
const POP_CAR_VELOCITY: f32 = 0.95;
/// `EmitExhaust`: particles per unit of the flow and of the broken flow (0x7d648c, 0x7d6490).
const EXHAUST_PER_FLOW: f32 = 2.0;
const BROKEN_PER_FLOW: f32 = 1.0;
/// `Sim8CF0`'s smoothing of the flow: the new value's share.
const FLOW_NEW: f32 = 0.7;
/// Ours: a live answer's exhaust sums are scaled to a game frame (the game sums once a rendered frame).
const EXHAUST_FRAME: f32 = rbr_recording::STEP_SECONDS / super::GAME_FRAME;
/// Ours: half the engine strip's width across the car, metres.
const STRIP_HALF: f32 = 0.3;
/// Ours: sparks per debris-A particle, plus this many for any impact over the first key, their pool and launch speed.
const SPARKS_PER_DEBRIS: f32 = 6.0;
const SPARKS_BASE: f32 = 4.0;
const SPARK_POOL: usize = 160;
const SPARK_SPEED: f32 = 5.0;

/// `E83_TableLerp8` (0x5df570): the piecewise-linear value of `vals` at `x` over [`DAMAGE_KEYS`].
pub fn table_lerp8(x: f32, vals: &[f32; 8]) -> f32 {
    for i in 1..8 {
        if x <= DAMAGE_KEYS[i] {
            let t = ((x - DAMAGE_KEYS[i - 1]) / (DAMAGE_KEYS[i] - DAMAGE_KEYS[i - 1])).max(0.0);
            return (vals[i] - vals[i - 1]) * t + vals[i - 1];
        }
    }
    vals[7]
}

/// `EmitDebris`' particles per debris emitter (a, b, c, smoke) for an impact of `force` with the car's up axis `up`:
/// past 45 degrees of tilt the force grows, up to 5 times at 90 degrees.
pub fn debris_counts(force: f32, up: Vec3) -> [u32; 4] {
    let c = if (-1.0..=1.0).contains(&up.z) { up.z } else { 1.0 };
    let tilt = c.acos();
    let quarter = std::f32::consts::FRAC_PI_4;
    let force = if tilt > quarter { ((tilt - quarter) * 1.273_239_5).min(1.0) * force * 5.0 } else { force };
    DEBRIS_COUNTS.map(|table| (table_lerp8(force, &table) + 0.5).floor().max(0.0) as u32)
}

/// Ours, for playback (the recordings hold no damage event): an impact when the car's horizontal velocity changes by
/// more than [`IMPACT_SPEED_DROP`] m/s in one step while its speed drops, with a force of [`FORCE_PER_SPEED_DROP`] per
/// m/s, [`IMPACT_AHEAD`] metres ahead of the car along its velocity before. Calibrated on the live events of
/// drive-tanner-damage (the Corolla, `rbr-standalone --live --boot`): its three impacts over 1000 (forces 5434, 23377
/// and 4790) are its only steps over 1 m/s, at 1030 to 1400 per m/s, 1.9 to 2.2 m from the car's centre.
pub const IMPACT_SPEED_DROP: f32 = 1.0;
pub const FORCE_PER_SPEED_DROP: f32 = 1400.0;
pub const IMPACT_AHEAD: f32 = 2.0;

/// The impact force guessed from the velocities before and after a step, and where it hit from `position` (`None`:
/// no impact).
pub fn impact_from_speeds(position: Vec3, before: Vec3, after: Vec3) -> Option<(f32, Vec3)> {
    let flat = |v: Vec3| Vec3::new(v.x, v.y, 0.0);
    let drop = (flat(after) - flat(before)).length();
    let slowed = flat(after).length() < flat(before).length();
    (drop > IMPACT_SPEED_DROP && slowed)
        .then(|| (drop * FORCE_PER_SPEED_DROP, position + flat(before).normalize_or_zero() * IMPACT_AHEAD))
}

/// Where the car is this step, for the effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub velocity: Vec3,
    /// The car's up and x axes, world (x is the car's left: it drives along -y).
    pub up: Vec3,
    pub side: Vec3,
    /// The engine and the exhaust, world (`None`: the car's ini has no such point).
    pub engine: Option<Vec3>,
    pub exhaust: Option<Vec3>,
}

/// The car effects' pools and levels.
#[derive(Debug, Clone)]
pub struct CarEffects {
    debris: [Option<usize>; 4],
    sparks: Option<usize>,
    /// The fire and fire-smoke pools with the file's rates (`LoadFile` keeps them at +0x83c/+0x840).
    fire: [Option<(usize, f32)>; 2],
    steam: Option<usize>,
    pop: Option<usize>,
    /// `g_exhaust_emitter` (or `g_exhaust_arctic_emitter`) and `g_exhaust_broken_emitter`, and their sums (+0x6d4,
    /// +0x6d8).
    exhaust: [Option<usize>; 2],
    exhaust_sum: [f32; 2],
    /// `Sim8CF0`'s smoothed flow (the engine's +0x184).
    flow: f32,
    /// +0x844: the fire's level.
    pub fire_level: f32,
    /// +0x838: the steam's level.
    pub steam_level: f32,
}

impl CarEffects {
    /// The effects of `sim`'s file: finds their pools, makes the debris rest where it lands and gives the sparks
    /// their look.
    pub fn new(sim: &mut Simulation) -> CarEffects {
        let debris = DEBRIS_EMITTERS.map(|name| sim.find(name));
        for pool in debris.iter().flatten() {
            sim.pools[*pool].def.lands = true;
        }
        let sparks = sim.find("g_car_sparks_emitter");
        if let Some(pool) = sparks {
            let def = &mut sim.pools[pool].def;
            def.look = spark_look(def.look.sprite);
            def.max = SPARK_POOL;
            def.shape = rbr_formats::pss::Shape::Point;
            def.acceleration = Vec3::new(0.0, 0.0, -9.81);
            def.lands = true;
        }
        let rated = |name: &str| sim.find(name).map(|pool| (pool, sim.pools[pool].def.emit_rate));
        CarEffects {
            debris,
            sparks,
            fire: [rated("g_fire_emitter"), rated("g_firesmoke_emitter")],
            steam: sim.find("g_engine_steam_emitter"),
            pop: sim.find("g_pop_emitter"),
            exhaust: [
                sim.find("g_exhaust_emitter").or_else(|| sim.find("g_exhaust_arctic_emitter")),
                sim.find("g_exhaust_broken_emitter"),
            ],
            exhaust_sum: [0.0; 2],
            flow: 0.0,
            fire_level: 0.0,
            steam_level: 0.0,
        }
    }

    /// Back to an undamaged, cold car.
    pub fn reset(&mut self) {
        self.fire_level = 0.0;
        self.steam_level = 0.0;
        self.exhaust_sum = [0.0; 2];
        self.flow = 0.0;
    }

    /// A `CSigCarDamage` of `kind` (`EmitDebris` runs for 1 to 3) and `force` at `at`; the debris rests at `floor`.
    pub fn damage(&self, sim: &mut Simulation, kind: u32, force: f32, at: Vec3, pose: &Pose, floor: f32) {
        if !(1..=3).contains(&kind) {
            return;
        }
        let counts = debris_counts(force, pose.up);
        for ((pool, n), scale) in self.debris.iter().zip(counts).zip(DEBRIS_CAR_VELOCITY) {
            if let Some(pool) = *pool {
                sim.emit(pool, at, at, pose.velocity * scale, n as f32, floor);
            }
        }
        if let (Some(pool), true) = (self.sparks, force > DAMAGE_KEYS[1]) {
            let n = SPARKS_BASE + SPARKS_PER_DEBRIS * counts[0] as f32;
            sim.emit_scattered(pool, at, pose.velocity * 0.3, SPARK_SPEED, n, floor);
        }
    }

    /// `CSigEngineFire`: the fire's level (`Set844`).
    pub fn engine_fire(&mut self, level: f32) {
        self.fire_level = level;
    }

    /// One step's `CSigEngineNoise` words: word 5 sets the steam level (`Scale838`); the misfire word throws pops by
    /// `EmitExhaust`'s rule.
    pub fn engine_noise(&mut self, sim: &mut Simulation, misfire: f32, steam: f32, pose: &Pose) {
        if steam > 0.0 {
            self.steam_level = steam * STEAM_PER_NOISE;
        }
        if let (Some(pool), Some(at), true) = (self.pop, pose.exhaust, misfire > POP_FROM) {
            let n = (((misfire - POP_FROM) * POP_PER) as u32).min(POP_MOST);
            sim.emit(pool, at, at, pose.velocity * POP_CAR_VELOCITY, n as f32, f32::MIN);
        }
    }

    /// A frame's exhaust (`Sim8CF0` then `EmitExhaust`): `flow` the engine state's +0x200, `broken` the engine's
    /// +0x188.
    pub fn exhaust(&mut self, sim: &mut Simulation, flow: f32, broken: f32, pose: &Pose) {
        self.flow = flow * FLOW_NEW + self.flow * (1.0 - FLOW_NEW);
        let Some(at) = pose.exhaust else { return };
        let launch = pose.velocity * POP_CAR_VELOCITY;
        for (k, value) in [self.flow * EXHAUST_PER_FLOW, broken * BROKEN_PER_FLOW].into_iter().enumerate() {
            let Some(pool) = self.exhaust[k] else { continue };
            let sum = &mut self.exhaust_sum[k];
            *sum += value * EXHAUST_FRAME;
            let n = sum.floor().max(0.0);
            *sum -= n;
            sim.emit(pool, at, at, launch, n, f32::MIN);
        }
    }

    /// A tyre payload's water (`EmitTire` adds a tenth of it to the steam level).
    pub fn tyre_water(&mut self, splash: f32) {
        self.steam_level += splash * STEAM_PER_SPLASH;
    }

    /// One step of `dt`: the fire, its smoke and the steam at the engine.
    pub fn step(&mut self, sim: &mut Simulation, pose: &Pose, dt: f32) {
        let Some(engine) = pose.engine else { return };
        let (from, to) = (engine - pose.side * STRIP_HALF, engine + pose.side * STRIP_HALF);
        for (pool, rate) in self.fire.iter().flatten() {
            sim.emit(*pool, from, to, Vec3::ZERO, rate * self.fire_level * dt, f32::MIN);
        }
        if self.steam_level > 0.0 {
            if let Some(pool) = self.steam {
                let n = ((self.steam_level * STEAM_PER_LEVEL) as u32).min(1);
                sim.emit(pool, from, to, pose.velocity * STEAM_CAR_VELOCITY, n as f32, f32::MIN);
            }
            self.steam_level -= dt * STEAM_FALL;
        }
    }
}

/// Ours: hot sparks, small additive streaks that fade from yellow to red in a third of a second.
fn spark_look(sprite: u32) -> Look {
    Look {
        sprite,
        colors: [[1.0, 0.9, 0.55, 1.0], [1.0, 0.55, 0.15, 0.9], [0.8, 0.2, 0.05, 0.0]],
        color_at: 0.3,
        sizes: [[0.025, 0.025], [0.02, 0.02], [0.01, 0.01]],
        size_at: 0.5,
        size_variance: 0.5,
        life: 0.35,
        weight: 1.0,
        trail: 0.04,
        additive: true,
        attenuation: [0.0, 0.0],
        offset_height: 0.5,
        stone: false,
        spark: true,
        wisp: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_lerp_between_keys() {
        assert_eq!(table_lerp8(0.0, &DEBRIS_COUNTS[3]), 0.0);
        assert_eq!(table_lerp8(7430.0, &DEBRIS_COUNTS[3]), 7.0);
        assert!((table_lerp8(9458.5, &DEBRIS_COUNTS[3]) - 9.5).abs() < 1e-3);
        assert_eq!(table_lerp8(1e6, &DEBRIS_COUNTS[1]), 6.0);
    }

    #[test]
    fn debris_counts_grow_with_force_and_tilt() {
        assert_eq!(debris_counts(1000.0, Vec3::Z), [0, 0, 0, 1]);
        assert_eq!(debris_counts(19688.0, Vec3::Z), [2, 3, 2, 18]);
        // On its side the force is 5 times: past the last key.
        assert_eq!(debris_counts(19688.0, Vec3::X), [4, 6, 4, 30]);
    }

    #[test]
    fn speed_drops_are_impacts() {
        let v = Vec3::new(16.8, 0.0, 0.0);
        assert_eq!(impact_from_speeds(Vec3::ZERO, v, v * 0.99), None);
        let (force, at) = impact_from_speeds(Vec3::ZERO, v, Vec3::new(4.1, 0.0, 0.0)).unwrap();
        assert!(force > DAMAGE_KEYS[2] && force < DAMAGE_KEYS[3]);
        assert_eq!(at, Vec3::new(IMPACT_AHEAD, 0.0, 0.0));
        assert_eq!(impact_from_speeds(Vec3::ZERO, v * 0.5, v), None, "speeding up is no impact");
    }
}
