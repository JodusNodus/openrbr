//! The particle simulation: emitter pools built from a `PS_*.ini` file, emission, motion and the billboards to draw.
//! Pure and deterministic for a seed: no GPU, no clock, no global random state. Positions and velocities are in the
//! game's particle space, the physics frame (z up, gravity `0 0 -9.81`).
//!
//! From the data (`notes/renderer.md` "Particles"): each emitter's class, shape, forces and pool size; each particle class's
//! colours, sizes, life, trail, blend and sprite. Ours (the original's maths is not ported, by the user's decision):
//! the launch directions, the drag from the air friction and the class weight, the size variance, the ground kill and
//! the billboard placement.

use glam::Vec3;
use rbr_formats::pss::{self, Blend, Force, ParticleFile, Shape};

/// A small deterministic generator (xorshift32).
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    /// A point of the unit disc.
    pub fn disc(&mut self) -> (f32, f32) {
        loop {
            let (x, y) = (self.signed(), self.signed());
            if x * x + y * y <= 1.0 {
                return (x, y);
            }
        }
    }
}

/// A particle class's look over its life, from the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub sprite: u32,
    pub colors: [[f32; 4]; 3],
    pub color_at: f32,
    pub sizes: [[f32; 2]; 3],
    pub size_at: f32,
    pub size_variance: f32,
    pub life: f32,
    pub weight: f32,
    pub trail: f32,
    pub additive: bool,
    pub attenuation: [f32; 2],
    /// Where the particle's point is on the sprite, from its bottom (0.5: the centre).
    pub offset_height: f32,
    /// A thrown stone ([`Look::is_stone`]).
    pub stone: bool,
    /// A spark (ours, `car::spark_look`): drawn as small as it is.
    pub spark: bool,
    /// A wisp (ours, the wheel spray's mist): a faint cloud the shader leaves uneroded and fades near the eye.
    pub wisp: bool,
}

impl Look {
    fn of(class: &pss::ParticleClass) -> Look {
        Look {
            sprite: class.sprite,
            colors: [class.start_color, class.mid_color, class.end_color],
            color_at: class.mid_color_at,
            sizes: [class.start_size, class.mid_size, class.end_size],
            size_at: class.mid_size_at,
            size_variance: class.size_variance,
            life: class.life,
            weight: class.weight,
            trail: class.trail_length,
            additive: class.blend == Blend::Add,
            attenuation: class.camera_attenuation,
            offset_height: class.offset_height,
            stone: Look::is_stone(class),
            spark: false,
            wisp: false,
        }
    }

    /// Whether a class is thrown stones (ours): blended, heavy, without a trail and a few centimetres across, the files'
    /// pebble classes (weight 0.41, 1.5 to 2.5 cm; the rain's streaks have size 0 and a trail). They are drawn small and dark ([`STONE_SHADE`]), and fly without the spray's kick.
    fn is_stone(class: &pss::ParticleClass) -> bool {
        let sizes = [class.start_size, class.mid_size, class.end_size];
        let small = sizes.iter().flatten().all(|&s| s > 0.0 && s < STONE_SIZE);
        class.blend != Blend::Add && class.weight >= STONE_WEIGHT && class.trail_length == 0.0 && small
    }

    /// The colour at `t`, the fraction of the life gone: start, to the intermediate colour at `color_at`, to the end.
    pub fn color(&self, t: f32) -> [f32; 4] {
        let [a, b, c] = self.colors;
        let (from, to, f) = three_keys(a, b, c, self.color_at, t);
        std::array::from_fn(|i| from[i] + (to[i] - from[i]) * f)
    }

    /// The size (width, height) at `t`, as [`Look::color`].
    pub fn size(&self, t: f32) -> [f32; 2] {
        let [a, b, c] = self.sizes;
        let (from, to, f) = three_keys(a, b, c, self.size_at, t);
        std::array::from_fn(|i| from[i] + (to[i] - from[i]) * f)
    }
}

/// The keys around `t` of a start, intermediate (at `at`) and end key, and the fraction between them. An
/// intermediate at 0 is not a key (the files' "set-in 0": start to end).
fn three_keys<const N: usize>(a: [f32; N], b: [f32; N], c: [f32; N], at: f32, t: f32) -> ([f32; N], [f32; N], f32) {
    let t = t.clamp(0.0, 1.0);
    if at <= 0.0 || at >= 1.0 {
        (a, c, t)
    } else if t < at {
        (a, b, t / at)
    } else {
        (b, c, (t - at) / (1.0 - at))
    }
}

/// An emitter of the file, resolved: its pool size, shape, forces and its particles' look.
#[derive(Debug, Clone, PartialEq)]
pub struct EmitterDef {
    pub name: String,
    pub max: usize,
    pub emit_rate: f32,
    pub shape: Shape,
    pub force_variance: f32,
    pub look: Look,
    pub acceleration: Vec3,
    pub friction: f32,
    pub wind: f32,
    /// The particles stop where they reach their floor and rest there (the debris: `E83System::Update`'s ground test
    /// for emitters 14 to 17) instead of dying.
    pub lands: bool,
    /// The water's splash (`g_splash*`): it keeps the file's pool and is not smoke-densified (see [`EmitterDef::of`]).
    pub water: bool,
}

impl EmitterDef {
    fn of(file: &ParticleFile, emitter: &pss::Emitter) -> Option<EmitterDef> {
        let class = file.emitter_class(&emitter.class)?;
        let look = Look::of(file.emitter_particles(emitter)?);
        // Stones keep the file's pool: more of them read as a cloud of hail. So does the water's splash (ours: its
        // counts are the game's table per frame, `tyres.rs`), whose smoke would otherwise cover the road at 0.3 water.
        let water = emitter.name.starts_with("g_splash");
        let scale = if look.stone || water { 1 } else { POOL_SCALE };
        let mut def = EmitterDef {
            name: emitter.name.clone(),
            max: emitter.max_particles as usize * scale,
            emit_rate: emitter.emit_rate,
            shape: class.shape.clone(),
            force_variance: class.force_variance,
            look,
            acceleration: Vec3::ZERO,
            friction: 0.0,
            wind: 0.0,
            lands: false,
            water,
        };
        for force in &emitter.forces {
            match *force {
                Force::Acceleration(a) => def.acceleration += Vec3::from(a),
                Force::Friction(f) => def.friction += f,
                Force::Wind(f) => def.wind += f,
            }
        }
        Some(def)
    }

    /// A launch offset and velocity of the shape (ours: the directions spread about straight down, which the files'
    /// signs imply: rain falls with a positive force, spray rises with a negative one).
    fn launch(&self, rng: &mut Rng) -> (Vec3, Vec3) {
        let spread = |rng: &mut Rng, deviation: f32| {
            let (x, y) = rng.disc();
            Vec3::new(x * deviation, y * deviation, -1.0).normalize()
        };
        let variance = 1.0 + self.force_variance * rng.signed();
        match self.shape {
            Shape::Point => (Vec3::ZERO, Vec3::ZERO),
            Shape::Cone { speed, diversion } => (Vec3::ZERO, spread(rng, diversion) * speed * variance),
            Shape::Box { base, force, deviation, shear } => {
                let offset = Vec3::new(base[0] * (rng.unit() - 0.5), base[1] * (rng.unit() - 0.5), 0.0);
                (offset, spread(rng, deviation) * force * variance + Vec3::new(shear[0], shear[1], 0.0))
            }
            Shape::Strip { speed, deviation, .. } => (Vec3::ZERO, spread(rng, deviation) * speed * variance),
            Shape::Kind4 { a, .. } => (Vec3::ZERO, Vec3::new(0.0, 0.0, -a)),
        }
    }
}

/// One live particle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub life: f32,
    /// The size factor drawn at birth (the class's size variance).
    pub scale: f32,
    /// Below this height the particle dies (the ground it was thrown from; `f32::MIN`: none).
    pub floor: f32,
}

/// An emitter's live particles, at most its `max`.
#[derive(Debug, Clone)]
pub struct Pool {
    pub def: EmitterDef,
    pub particles: Vec<Particle>,
    /// The oldest slot, reused when the pool is full.
    next: usize,
    /// The emission's fraction not emitted yet.
    carry: f32,
}

/// Every emitter of a file and their particles.
#[derive(Debug, Clone)]
pub struct Simulation {
    pub pools: Vec<Pool>,
    pub air_friction: f32,
    /// The weather's wind, m/s, which the emitters with a wind force follow.
    pub wind: Vec3,
    rng: Rng,
}

/// Ours: each pool holds this many times the file's maximum (the files' pools of 25 to 200 are for 2004 hardware).
pub const POOL_SCALE: usize = 4;
/// Ours: the drag per second is the air friction times this, over the class weight.
const DRAG_SCALE: f32 = 0.01;
/// Ours: a particle at least this far below its floor dies.
const FLOOR_MARGIN: f32 = 0.05;

impl Simulation {
    /// Pools for every emitter of `file` whose names resolve, empty, with the generator at `seed`.
    pub fn new(file: &ParticleFile, seed: u32) -> Simulation {
        let pools = file
            .emitters
            .iter()
            .filter_map(|e| EmitterDef::of(file, e))
            .map(|def| Pool { particles: Vec::new(), next: 0, carry: 0.0, def })
            .collect();
        Simulation { pools, air_friction: file.air_friction, wind: Vec3::ZERO, rng: Rng::new(seed) }
    }

    /// The pool of the emitter named `name`.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.pools.iter().position(|p| p.def.name.eq_ignore_ascii_case(name))
    }

    /// Adds an empty pool of at most `max` particles of the file's particle class `class`, which its owner moves (no
    /// shape or force; `CEmitterRain`, [`super::rain`]). `None` when the file has no such class.
    pub fn add_class(&mut self, file: &ParticleFile, class: &str, max: usize) -> Option<usize> {
        let look = Look::of(file.class(class)?);
        let def = EmitterDef {
            name: class.to_owned(),
            max,
            emit_rate: 0.0,
            shape: Shape::Point,
            force_variance: 0.0,
            look,
            acceleration: Vec3::ZERO,
            friction: 0.0,
            wind: 0.0,
            lands: false,
            water: false,
        };
        self.pools.push(Pool { particles: Vec::new(), next: 0, carry: 0.0, def });
        Some(self.pools.len() - 1)
    }

    /// Adds an empty pool of `def` (ours: an emitter the file does not have).
    pub fn add_pool(&mut self, def: EmitterDef) -> usize {
        self.pools.push(Pool { particles: Vec::new(), next: 0, carry: 0.0, def });
        self.pools.len() - 1
    }

    /// Live particles over all pools.
    pub fn count(&self) -> usize {
        self.pools.iter().map(|p| p.particles.len()).sum()
    }

    /// Removes every particle.
    pub fn clear(&mut self) {
        for pool in &mut self.pools {
            pool.particles.clear();
            pool.carry = 0.0;
            pool.next = 0;
        }
    }

    /// Emits `count` particles (the fraction carried over) of pool `pool`, spread along the way from `from` to `to`
    /// (the emitter's motion since its last emission), each launched with the shape's velocity plus `launch`.
    /// Particles below `floor` die.
    pub fn emit(&mut self, pool: usize, from: Vec3, to: Vec3, launch: Vec3, count: f32, floor: f32) {
        self.spawn(pool, launch, count, floor, |rng| from.lerp(to, rng.unit()));
    }

    /// Emits `count` particles of pool `pool` at `at`, each launched with `launch` plus `speed` in a random direction
    /// of the upper half (ours: the sparks).
    pub fn emit_scattered(&mut self, pool: usize, at: Vec3, launch: Vec3, speed: f32, count: f32, floor: f32) {
        let mut directions = Vec::new();
        for _ in 0..count.max(0.0) as usize {
            let (x, y) = self.rng.disc();
            let up = 0.2 + 0.8 * self.rng.unit();
            directions.push(Vec3::new(x, y, up).normalize() * speed * (0.5 + self.rng.unit()));
        }
        for d in directions {
            self.spawn(pool, launch + d, 1.0, floor, |_| at);
        }
    }

    fn spawn(&mut self, pool: usize, launch: Vec3, count: f32, floor: f32, mut place: impl FnMut(&mut Rng) -> Vec3) {
        let Some(p) = self.pools.get_mut(pool) else { return };
        if p.def.max == 0 || count.is_nan() || count <= 0.0 {
            return;
        }
        p.carry += count;
        // More than a full pool in one call would only overwrite itself.
        let n = (p.carry.floor() as usize).min(p.def.max);
        p.carry -= p.carry.floor();
        for _ in 0..n {
            let at = place(&mut self.rng);
            let (offset, velocity) = p.def.launch(&mut self.rng);
            let variance = p.def.look.size_variance;
            let particle = Particle {
                position: at + offset,
                velocity: velocity + launch,
                age: 0.0,
                life: p.def.look.life * (0.8 + 0.4 * self.rng.unit()),
                scale: (1.0 + variance * 0.25 * self.rng.signed()).max(0.2),
                floor,
            };
            if p.particles.len() < p.def.max {
                p.particles.push(particle);
            } else {
                let slot = p.next % p.def.max;
                p.particles[slot] = particle;
                p.next = slot + 1;
            }
        }
    }

    /// Moves every particle on by `dt` seconds and removes the dead.
    pub fn step(&mut self, dt: f32) {
        for pool in 0..self.pools.len() {
            self.step_pool(pool, dt);
        }
    }

    /// Moves pool `pool`'s particles on by `dt` seconds and removes its dead.
    pub fn step_pool(&mut self, pool: usize, dt: f32) {
        let (wind, air_friction) = (self.wind, self.air_friction);
        let Some(pool) = self.pools.get_mut(pool) else { return };
        let def = &pool.def;
        let drag = (air_friction * DRAG_SCALE / def.look.weight.max(0.01) + def.friction).max(0.0);
        let keep = (-drag * dt).exp();
        let follow = 1.0 - (-def.wind * dt).exp();
        pool.particles.retain_mut(|p| {
            p.age += dt;
            p.velocity = (p.velocity + def.acceleration * dt) * keep;
            if def.wind != 0.0 {
                p.velocity += (wind - p.velocity) * follow;
            }
            p.position += p.velocity * dt;
            if def.lands && p.position.z < p.floor {
                p.position.z = p.floor;
                p.velocity = Vec3::ZERO;
            }
            p.age < p.life && p.position.z > p.floor - FLOOR_MARGIN
        });
        pool.next = pool.next.min(pool.particles.len());
    }
}

/// One camera-facing quad: its centre and half axes in stage space (y up), the colour (straight alpha) and the
/// sprite layer. Additive particles add their colour and keep the frame's alpha.
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Billboard {
    pub centre: [f32; 3],
    pub layer: f32,
    pub axis_u: [f32; 3],
    /// 1 additive, -1 a wisp, -2 a holed wisp, -3 billowing dust (`fs_soft`).
    pub additive: f32,
    pub axis_v: [f32; 3],
    pub color: [f32; 4],
}

/// The camera a frame's billboards face: its eye and its right and up axes, stage space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Facing {
    pub eye: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

/// Physics frame (z up) to stage space (y up): `(x, y, z)` to `(x, z, y)`.
pub fn to_stage(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y)
}

/// Ours: the smallest sprite drawn, metres, and the smallest stone ([`Look::is_stone`]): the pebble classes are 1.5 to
/// 2.5 cm across, which would be specks at a few metres.
const MIN_SIZE: f32 = 0.07;
const MIN_STONE_SIZE: f32 = 0.035;
const MIN_SPARK_SIZE: f32 = 0.012;
/// Ours: the classes that are stones: at least this heavy and every size below this, metres.
const STONE_WEIGHT: f32 = 0.3;
const STONE_SIZE: f32 = 0.05;
/// Ours: a stone's colour is the file's times this. The pebble sprites are mid grey and the files' colours go to
/// white, which under the stage light drew them twice as bright as the road; at this shade they read as dark wet
/// gravel against Tanner's road (the particle light is the stage's ambient and part of its diffuse, `gpu.rs`).
pub const STONE_SHADE: f32 = 0.4;
/// Ours: the light classes (smoke and dust, weight at most `SMOKE_WEIGHT`, not the water's) are drawn this much denser:
/// at the files' 0.14 to 0.3 alpha their clouds barely showed over the stage.
const SMOKE_WEIGHT: f32 = 0.05;
const SMOKE_DENSITY: f32 = 2.0;
/// Ours: the wet road's holed spray sprite; its billboards are marked -2 (`fs_soft` lifts them off the ground softly).
const WISP_SPRITE: u32 = super::wheel_spray::sprites::WISP;
/// Ours: the dry dust's billowing sprite; its billboards are marked -3 (`fs_soft` lights it as a plume).
const DUST_SPRITE: u32 = super::wheel_spray::sprites::DUST;
/// Ours: particles closer to the eye than this fade out, over the next metre.
const NEAR_FADE: f32 = 0.3;

impl Simulation {
    /// The billboards of every live particle, back to front from `facing.eye`. `layer` maps a sprite id to its texture
    /// layer (`None`: not drawn).
    pub fn billboards(&self, facing: Facing, layer: impl Fn(u32) -> Option<u32>) -> Vec<Billboard> {
        let mut out: Vec<(f32, Billboard)> = Vec::with_capacity(self.count());
        for pool in &self.pools {
            let look = &pool.def.look;
            let Some(layer) = layer(look.sprite) else { continue };
            for p in &pool.particles {
                let t = p.age / p.life;
                let mut color = look.color(t);
                if look.weight <= SMOKE_WEIGHT && !look.additive && !pool.def.water {
                    color[3] = (color[3] * SMOKE_DENSITY).min(1.0);
                }
                if look.stone {
                    for c in &mut color[..3] {
                        *c *= STONE_SHADE;
                    }
                }
                let least = if look.stone {
                    MIN_STONE_SIZE
                } else if look.spark {
                    MIN_SPARK_SIZE
                } else {
                    MIN_SIZE
                };
                let [w, h] = look.size(t).map(|s| (s * p.scale).max(least));
                let centre = to_stage(p.position);
                let distance = centre.distance(facing.eye);
                color[3] *= ((distance - NEAR_FADE) / 1.0).clamp(0.0, 1.0);
                if look.attenuation[1] > look.attenuation[0] {
                    let [start, end] = look.attenuation;
                    color[3] *= 1.0 - ((distance - start) / (end - start)).clamp(0.0, 1.0);
                }
                if color[3] <= 0.002 || w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let velocity = to_stage(p.velocity);
                let (axis_u, axis_v, centre) = if look.trail > 0.0 && velocity.length() > 0.1 {
                    // A streak along the motion, `trail` seconds long, facing the eye.
                    let along = velocity * look.trail * 0.5;
                    let side = along.cross(centre - facing.eye).normalize_or_zero() * w * 0.5;
                    (side, along.normalize_or_zero() * (along.length() + h * 0.5), centre - along)
                } else {
                    let lift = facing.up * h * (look.offset_height - 0.5);
                    (facing.right * w * 0.5, facing.up * h * 0.5, centre + lift)
                };
                let billboard = Billboard {
                    centre: centre.to_array(),
                    layer: layer as f32,
                    axis_u: axis_u.to_array(),
                    additive: if look.sprite == DUST_SPRITE {
                        -3.0
                    } else if look.wisp && look.sprite == WISP_SPRITE {
                        -2.0
                    } else if look.wisp {
                        -1.0
                    } else {
                        f32::from(u8::from(look.additive))
                    },
                    axis_v: axis_v.to_array(),
                    color,
                };
                out.push((distance, billboard));
            }
        }
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.into_iter().map(|(_, b)| b).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> ParticleFile {
        let text = "version3 1 0 0 1 0 1 0 1 0 0 1 1 1 1 1 1 10 \
            1 pc 13 0 0 0 1 0.5 0.5 0.5 0.5 1 1 1 1 1 0.1 0.1 0.2 0.2 0.4 0.4 0.5 1 2 0.41 0 0 0 0 BLEND_BLEND 0 0 0.5 0 \
            1 ec 2 0.2 0.2 -3 0.3 0 0 0 0 0 1 pc \
            1 em ec 50 0 0 1 0 0 -9.81 5 0.01 0 0 \
            10 0 0 0 0 0 0 0 $";
        pss::parse(text).unwrap()
    }

    #[test]
    fn pebbles_are_stones_and_rain_is_not() {
        let class = |sizes: &str, weight_and_trail: &str| {
            let text = format!(
                "version3 1 0 0 1 0 1 0 1 0 0 1 1 1 1 1 1 10 \
                 1 pc 13 0 0 0 1 0.5 0.5 0.5 0.5 1 1 1 1 1 {sizes} 0.5 1 2 {weight_and_trail} 0 0 0 BLEND_BLEND 0 0 0.5 0 \
                 1 ec 2 0.2 0.2 -3 0.3 0 0 0 0 0 1 pc \
                 1 em ec 50 0 0 1 0 0 -9.81 5 0.01 0 0 \
                 10 0 0 0 0 0 0 0 $"
            );
            Simulation::new(&pss::parse(&text).unwrap(), 1).pools.remove(0).def
        };
        let pebble = class("0.025 0.025 0.025 0.025 0.025 0.025", "0.41 0");
        assert!(pebble.look.stone);
        assert_eq!(pebble.max, 50, "stones keep the file's pool");
        assert!(!class("0 0 0 0 0 0", "4 0.035").look.stone, "rain streaks");
        assert!(!class("0.1 0.1 0.2 0.2 0.4 0.4", "0.41 0").look.stone, "spray");
    }

    #[test]
    fn looks_lerp_through_the_intermediate_key() {
        let sim = Simulation::new(&file(), 1);
        let look = &sim.pools[0].def.look;
        assert_eq!(look.color(0.0), [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(look.color(0.5), [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(look.color(1.0), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(look.size(0.25), [0.15, 0.15]);
    }

    #[test]
    fn emission_carries_fractions_and_caps_the_pool() {
        let mut sim = Simulation::new(&file(), 7);
        let pool = sim.find("EM").unwrap();
        sim.emit(pool, Vec3::ZERO, Vec3::X, Vec3::ZERO, 0.6, f32::MIN);
        assert_eq!(sim.count(), 0);
        sim.emit(pool, Vec3::ZERO, Vec3::X, Vec3::ZERO, 0.6, f32::MIN);
        assert_eq!(sim.count(), 1);
        sim.emit(pool, Vec3::ZERO, Vec3::X, Vec3::ZERO, 5000.0, f32::MIN);
        assert_eq!(sim.count(), 50 * POOL_SCALE);
    }

    #[test]
    fn spray_rises_falls_and_dies_at_the_floor() {
        let mut sim = Simulation::new(&file(), 3);
        sim.emit(0, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 20.0, 0.0);
        assert!(sim.pools[0].particles.iter().all(|p| p.velocity.z > 0.0), "a negative force throws up");
        for _ in 0..10 {
            sim.step(0.01);
        }
        assert_eq!(sim.count(), 20);
        for _ in 0..200 {
            sim.step(0.01);
        }
        assert_eq!(sim.count(), 0, "gravity brings them back below the floor");
    }

    #[test]
    fn same_seed_same_particles() {
        let run = |seed| {
            let mut sim = Simulation::new(&file(), seed);
            sim.emit(0, Vec3::ZERO, Vec3::ONE, Vec3::X, 10.0, f32::MIN);
            sim.step(0.1);
            sim.pools[0].particles.clone()
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
    }

    #[test]
    fn billboards_are_back_to_front() {
        let mut sim = Simulation::new(&file(), 2);
        sim.emit(0, Vec3::new(0.0, 5.0, 0.0), Vec3::new(0.0, 50.0, 0.0), Vec3::ZERO, 30.0, f32::MIN);
        let facing = Facing { eye: Vec3::ZERO, right: Vec3::X, up: Vec3::Y };
        let quads = sim.billboards(facing, |_| Some(0));
        assert_eq!(quads.len(), 30);
        let distance = |b: &Billboard| Vec3::from(b.centre).length();
        assert!(quads.windows(2).all(|w| distance(&w[0]) >= distance(&w[1]) - 0.2));
        assert!(sim.billboards(facing, |_| None).is_empty());
    }
}
