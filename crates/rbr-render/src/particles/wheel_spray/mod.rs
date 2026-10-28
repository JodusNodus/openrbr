//! Ours: the dust and water the tyres throw, in place of the game's tyre handlers, fake smoke, splash and wave when
//! the renderer draws its own particles (`--enhance particles`; not the game's look). Each wheel on the ground throws
//! by its surface ([`ground::material`]: kind, wetness, how loose), its slip (`rubber`, +0x80), the car's speed and
//! the standing water under it:
//!
//! - dry loose ground: a dust trail that hangs and drifts, in the region's colour ([`dust_colour`]), more with slip;
//! - snow: powder clouds, as the dust;
//! - wet road: a grey mist plume and thin streaked drops off every tyre, more with speed; wet loose ground: the same
//!   in the ground's brown ([`MUD_SHADE`]), coarser and with less mist (a damp road: a third of the mist, no drops);
//! - standing water (the renderer's puddles with `--enhance wet`, the water surface, or the payload's +0x98): a splash
//!   out of both sides.
//!
//! The particles live in the stage's [`Simulation`] pools of their own, drawn lit and soft like the rest, with
//! sprites drawn here ([`sprites`]).

pub mod ground;
pub mod sprites;

use glam::Vec3;
use rbr_recording::particles::TyreParticles;

use super::car::Pose;
use super::sim::{EmitterDef, Look, Rng, Simulation};
use ground::{smoothstep, Kind, Material};
use rbr_formats::pss::Shape;

/// The spray's pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pool {
    Dust,
    Powder,
    Mist,
    Drops,
    Splash,
    WetMist,
    MudMist,
    MudDrops,
    SplashMist,
}

const POOLS: [Pool; 9] = [
    Pool::Dust,
    Pool::Powder,
    Pool::Mist,
    Pool::Drops,
    Pool::Splash,
    Pool::WetMist,
    Pool::MudMist,
    Pool::MudDrops,
    Pool::SplashMist,
];

/// The share of the airborne dust's brightness that loose ground's spray has: wet earth is dark.
const MUD_SHADE: f32 = 0.6;
/// Wet clods are the ground's colour squared (darker, more saturated) times this.
const CLOD_SHADE: f32 = 0.45;
/// The dust in the air against the ground's colour ([`airborne`]).
const DUST_GREY: f32 = 0.35;
const DUST_LIFT: f32 = 1.1;
/// Particles a second from one wheel at full strength.
const DUST_RATE: f32 = 60.0;
const POWDER_RATE: f32 = 70.0;
const MIST_RATE: f32 = 190.0;
/// How many times `MIST_RATE` a wheel on wet ground throws at full strength (the pools are sized for three times it).
const MIST_GAIN: f32 = 1.5;
/// A wet road's mist is fewer, larger, denser puffs than the damp haze: they stand out as lumps.
const WET_MIST_SHARE: f32 = 0.5;
/// A damp road's mist, as a share of the wet road's (barely a haze).
const DAMP_MIST: f32 = 1.0 / 3.0;
/// The mist off a damp loose ground (gravel, dirt, grass) as a share of a damp road's, and off a wet one as a share of
/// a wet road's: the water soaks into the mud, and the wheels throw it as drops more than as a fine spray.
const LOOSE_MIST: f32 = 0.25;
const MUD_MIST: f32 = 0.4;
const DROP_RATE: f32 = 600.0;
/// Loose wet ground throws this many times a road's drops: it is mud and clods, not film.
const MUD_DROP_GAIN: f32 = 2.5;
/// The most of the tread's speed a clod of mud keeps.
const MUD_KEPT: f32 = 0.3;
const SPLASH_RATE: f32 = 2500.0;
/// The white spray hanging in a splash's sheet, a second.
const SPLASH_MIST_RATE: f32 = 300.0;
/// The car speeds, m/s, over which the trailing spray grows to full; the slip speeds over which a sliding or
/// spinning tyre's does; the speeds over which standing water splashes.
const MOTION: [f32; 2] = [3.0, 25.0];
/// The speeds, m/s, over which the mist grows from nothing (about 20 km/h) to the dense tail (100 km/h).
const MIST_MOTION: [f32; 2] = [5.0, 28.0];
const SPIN: [f32; 2] = [1.0, 8.0];
const SPLASH_SPEED: [f32; 2] = [2.0, 15.0];
/// The wetness over which the ground sprays water: a damp road binds the dust and throws a faint mist, no drops.
const SPRAYS: [f32; 2] = [0.5, 1.0];
/// The water level of +0x98 that counts as a full puddle.
const FULL_WATER: f32 = 0.5;
/// The tyre's radius, m (ours: the payload has none; rally tyres are 0.3 to 0.33).
const TYRE_RADIUS: f32 = 0.32;
/// Where the tread lets go of its water: from this angle behind the contact to this one, radians up the wheel's back.
const RELEASE: [f32; 2] = [0.2, 0.9];
/// The share of the tread's speed (relative to the hub) the water keeps when it lets go; the rest the arch, the mud
/// flap and the air take.
const KEPT: [f32; 2] = [0.06, 0.15];
/// A wheel entering standing water throws this many drops at once at full speed, when its water rises by at least
/// `BURST_RISE` in a frame.
const BURST: f32 = 60.0;
const BURST_RISE: f32 = 0.25;
/// How far below the wheel a cloud (dust, powder, mist) may sink before it is dropped, m.
const CLOUD_FLOOR: f32 = 4.0;

/// The dust's colour by the stage's particle file (`PS_Australia.ini`, ...): red in Australia, brown in Britain,
/// grey in Japan, tan in the USA, otherwise [`NEUTRAL_DUST`] (display colour, tinting the sprite's white).
pub fn dust_colour(particle_file: &str) -> [f32; 3] {
    let file = particle_file.to_ascii_lowercase();
    if file.contains("australia") {
        [0.82, 0.56, 0.38]
    } else if file.contains("british") {
        [0.64, 0.57, 0.47]
    } else if file.contains("japan") {
        [0.68, 0.66, 0.62]
    } else if file.contains("usa") {
        [0.74, 0.63, 0.49]
    } else {
        NEUTRAL_DUST
    }
}

/// The dust's colour on the other stages: a neutral grey-brown.
pub const NEUTRAL_DUST: [f32; 3] = [0.7, 0.66, 0.6];

/// The spray's pools in the simulation and each wheel's carried fraction of a particle per pool.
#[derive(Debug, Clone)]
pub struct WheelSpray {
    pools: [usize; POOLS.len()],
    carry: [[f32; POOLS.len()]; 4],
    /// Each wheel's standing water at its last frame, for the burst on entering it.
    standing: [f32; 4],
    /// Whether the renderer draws puddles to splash through.
    puddles: bool,
    rng: Rng,
}

/// One wheel's values for a frame.
struct Throw {
    edges: [Vec3; 2],
    velocity: Vec3,
    up: Vec3,
    out: Vec3,
    forward: Vec3,
    back: Vec3,
    slip: f32,
    speed: f32,
}

impl WheelSpray {
    /// Adds the spray's pools to `sim`, the dust of `dust` colour; `puddles` when the renderer draws them.
    pub fn new(sim: &mut Simulation, dust: [f32; 3], puddles: bool) -> WheelSpray {
        let pools = POOLS.map(|pool| sim.add_pool(def(pool, dust)));
        WheelSpray { pools, carry: [[0.0; POOLS.len()]; 4], standing: [0.0; 4], puddles, rng: Rng::new(0x5eed_0006) }
    }

    /// Back to nothing carried and dry wheels.
    pub fn clear(&mut self) {
        self.carry = [[0.0; POOLS.len()]; 4];
        self.standing = [0.0; 4];
    }

    /// Throws `dt` seconds of each wheel's spray from its payload.
    pub fn feed(&mut self, sim: &mut Simulation, payloads: &[TyreParticles], pose: &Pose, dt: f32) {
        let speed = pose.velocity.length();
        let forward = pose.velocity.try_normalize().unwrap_or(pose.side.cross(pose.up));
        let grade = pose.up.truncate().length() / pose.up.z.max(1e-3);
        let level = 1.0 - smoothstep(ground::PUDDLE_GRADE, grade);
        let mut standing = [0.0; 4];
        for p in payloads.iter().filter(|p| p.contact && p.wheel < 4) {
            let m = ground::material(p.surface());
            if m.kind == Kind::None {
                continue;
            }
            let at = Vec3::from(p.centre());
            let w = Throw {
                edges: p.edges.map(Vec3::from),
                velocity: pose.velocity,
                up: pose.up,
                out: if p.wheel % 2 == 0 { pose.side } else { -pose.side },
                forward,
                back: -forward,
                slip: p.rubber,
                speed,
            };
            let puddle = if self.puddles && m.wet >= 1.0 && matches!(m.kind, Kind::Gravel | Kind::Dirt | Kind::Tarmac) {
                level * ground::puddle_cover(at.x, at.y)
            } else {
                0.0
            };
            let wheel = p.wheel as usize;
            standing[wheel] = if m.kind == Kind::Water { 1.0 } else { puddle.max(p.splash / FULL_WATER).min(1.0) };
            if standing[wheel] - self.standing[wheel] >= BURST_RISE {
                let k = POOLS.iter().position(|&p| p == Pool::Splash).unwrap();
                self.burst(sim, k, BURST * standing[wheel] * smoothstep(SPLASH_SPEED, speed), &w);
            }
            let rates = rates(&m, &w, standing[wheel]);
            for (k, rate) in rates.into_iter().enumerate() {
                self.throw(sim, wheel, k, rate * dt, &w);
            }
        }
        self.standing = standing;
    }

    /// A wheel's crown of `count` drops of pool `k` on entering standing water: out all round its front half and up.
    fn burst(&mut self, sim: &mut Simulation, k: usize, count: f32, w: &Throw) {
        let hard = (w.speed / 15.0).clamp(0.3, 1.0);
        for _ in 0..count as usize {
            let r = &mut self.rng;
            let at = w.edges[0].lerp(w.edges[1], r.unit());
            let angle = std::f32::consts::PI * r.unit();
            let around = w.out * angle.cos() + w.forward * angle.sin() * 0.6;
            let launch = w.velocity * 0.6 + (around * (1.5 + 2.0 * r.unit()) + w.up * (1.5 + 1.5 * r.unit())) * hard;
            sim.emit(self.pools[k], at, at, launch, 1.0, at.z - 0.2);
        }
    }

    /// Emits `count` (with the wheel's carried fraction) of pool `k`.
    fn throw(&mut self, sim: &mut Simulation, wheel: usize, k: usize, count: f32, w: &Throw) {
        let carry = &mut self.carry[wheel][k];
        *carry += count.max(0.0);
        let n = carry.floor();
        *carry -= n;
        for _ in 0..n as usize {
            let r = &mut self.rng;
            let at = w.edges[0].lerp(w.edges[1], r.unit());
            let (at, launch, floor) = launch(POOLS[k], at, w, r);
            sim.emit(self.pools[k], at, at, launch, 1.0, floor);
        }
    }
}

/// Each pool's particles a second from a wheel on `m` with standing water `standing` (0 to 1), in [`POOLS`] order.
fn rates(m: &Material, w: &Throw, standing: f32) -> [f32; POOLS.len()] {
    let motion = smoothstep(MOTION, w.speed);
    let spin = smoothstep(SPIN, w.slip);
    let dry = 1.0 - m.wet;
    let trail = m.loose * (0.5 * motion + spin);
    let (dust, powder) = match m.kind {
        Kind::Gravel | Kind::Dirt | Kind::Tarmac => (trail * dry * dry, 0.0),
        Kind::Grass => (0.3 * trail * dry * dry, 0.0),
        Kind::Snow => (0.0, trail),
        Kind::Water | Kind::None => (0.0, 0.0),
    };
    let sprays = !matches!(m.kind, Kind::Snow | Kind::None);
    let wet = if sprays { smoothstep(SPRAYS, m.wet) } else { 0.0 };
    let cover = if !sprays {
        0.0
    } else if m.wet >= 1.0 {
        1.0
    } else if m.wet > 0.0 {
        DAMP_MIST
    } else {
        0.0
    };
    let mist = smoothstep(MIST_MOTION, w.speed);
    let hard = matches!(m.kind, Kind::Tarmac | Kind::Water);
    let splash = standing * smoothstep(SPLASH_SPEED, w.speed);
    let fine = (cover * mist + wet * 0.5 * spin) * MIST_RATE;
    let drops = wet * (motion + spin) * DROP_RATE;
    // Wet ground's water is grey on a road and the ground's brown, thrown lower and coarser, on loose ground: pools of
    // their own. The damp road's haze is the first mist pool.
    let (damp, road, loose) = if m.wet < 1.0 && m.kind != Kind::Water {
        (fine * if hard { 1.0 } else { LOOSE_MIST }, 0.0, 0.0)
    } else if hard {
        (0.0, 1.0, 0.0)
    } else {
        (0.0, 0.0, 1.0)
    };
    [
        dust * DUST_RATE,
        powder * POWDER_RATE,
        damp,
        drops * road,
        splash * SPLASH_RATE,
        fine * road * MIST_GAIN * WET_MIST_SHARE,
        fine * loose * MUD_MIST * MIST_GAIN,
        drops * loose * MUD_DROP_GAIN,
        splash * SPLASH_MIST_RATE,
    ]
}

/// Where a particle of `pool` starts (from `at` on the patch), its launch velocity and its floor (below which it
/// dies). The clouds' floor is well down: the wind's gusts carry them over the road's dips, and they fade where they
/// meet the ground (`fs_soft`).
fn launch(pool: Pool, at: Vec3, w: &Throw, r: &mut Rng) -> (Vec3, Vec3, f32) {
    let mut u = |from: f32, to: f32| from + (to - from) * r.unit();
    let ground = at.z;
    let cloud = ground - CLOUD_FLOOR;
    match pool {
        Pool::Dust => {
            let launch = w.velocity * 0.6 + w.up * u(0.5, 2.0) + w.out * u(-0.5, 2.0) + w.back * w.slip * u(0.0, 0.4);
            (at, launch, cloud)
        }
        Pool::Powder => (at, w.velocity * 0.5 + w.up * u(0.3, 1.5) + w.out * u(0.0, 1.0), cloud),
        Pool::MudMist => {
            // Low out of the contact patch, mostly sideways and back, and it settles.
            let (from, _) = released(at, w, u(0.0, RELEASE[0]), 0.0);
            (from, w.velocity * 0.5 + w.up * u(0.2, 0.9) + w.out * u(-0.8, 1.6) + w.back * u(0.0, 1.0), ground - 1.0)
        }
        Pool::WetMist => {
            // A road's mist hangs tall behind the car: kicked well up, so it rises over a bush, and it lingers.
            let (from, _) = released(at, w, u(RELEASE[0], RELEASE[1]), 0.0);
            let kick = 0.5 + w.speed / 28.0;
            (
                from,
                w.velocity * 0.5
                    + (w.up * u(0.2, 2.2).powi(2) / 2.0 + w.out * u(-0.6, 1.2) + w.back * u(0.0, 2.0)) * kick,
                cloud,
            )
        }
        Pool::Mist => {
            // Left hanging behind the tread, kicked up and out harder the faster the car.
            let (from, _) = released(at, w, u(RELEASE[0], RELEASE[1]), 0.0);
            let kick = 0.5 + w.speed / 28.0;
            (from, w.velocity * 0.6 + (w.up * u(0.4, 1.2) + w.out * u(0.2, 1.0) + w.back * u(0.0, 1.5)) * kick, cloud)
        }
        Pool::MudDrops => {
            // A fan of muddy water and clods off the tread, back and out, landing within a few metres.
            let (from, flung) = released(at, w, u(0.0, RELEASE[1]), u(KEPT[0], MUD_KEPT));
            let side = if u(0.0, 1.0) < 0.5 { w.out } else { -w.out };
            (from, w.velocity * 0.85 + flung + side * u(0.3, 2.0) + w.up * u(0.0, 1.0), ground - 0.2)
        }
        Pool::Drops => {
            // The rooster tail: off the tread up the wheel's back, the higher the faster it turns.
            let (from, flung) = released(at, w, u(RELEASE[0], RELEASE[1]), u(KEPT[0], KEPT[1]));
            (from, w.velocity + flung + w.out * u(0.3, 1.5), ground - 0.2)
        }
        Pool::Splash => {
            // Out of both sides of the tyre, harder with speed.
            let side = if u(0.0, 1.0) < 0.7 { w.out } else { -w.out };
            let hard = (w.speed / 15.0).clamp(0.3, 1.0);
            let launch = w.velocity * 0.6 + side * u(2.0, 6.0) * hard + w.up * u(1.5, 5.5) * hard;
            (at, launch, ground - 0.2)
        }
        Pool::SplashMist => {
            let side = if u(0.0, 1.0) < 0.7 { w.out } else { -w.out };
            let hard = (w.speed / 15.0).clamp(0.3, 1.0);
            (at, w.velocity * 0.5 + side * u(1.0, 4.0) * hard + w.up * u(0.5, 3.0) * hard, ground - 0.5)
        }
    }
}

/// Where water lets go of the tread `angle` radians up the wheel's back from the contact `at`, and the share `kept`
/// of the tread's velocity relative to the hub it leaves with: the tread turns at the car's speed plus the slip, back
/// along the ground and up the wheel's back.
fn released(at: Vec3, w: &Throw, angle: f32, kept: f32) -> (Vec3, Vec3) {
    let (sin, cos) = angle.sin_cos();
    let from = at + w.back * TYRE_RADIUS * sin + w.up * TYRE_RADIUS * (1.0 - cos);
    let tread = (w.speed + w.slip) * (w.back * cos + w.up * sin);
    (from, tread * kept)
}

/// How a pool's particles look and move.
struct Spec {
    sprite: u32,
    /// Start, middle and end colour (straight alpha), and where the middle is (0 to 1 of the life).
    colors: ([[f32; 4]; 3], f32),
    /// Start, middle and end size (width, height), and where the middle is.
    sizes: ([[f32; 2]; 3], f32),
    life: f32,
    /// Seconds of motion a particle is streaked along (0: a round sprite).
    trail: f32,
    /// The most a wheel throws a second, for the pool's size.
    rate: f32,
    /// Upward acceleration, m/s² (gravity -9.81; dust rises a little).
    rise: f32,
    friction: f32,
    wind: f32,
}

/// The colour of the dust in the air from the ground's `dust` colour: paler and greyer (`DUST_GREY` of the way to its
/// grey, lifted by `DUST_LIFT`), as fine dust scatters more than the soil it comes from.
fn airborne(dust: [f32; 3]) -> [f32; 3] {
    let grey = 0.299 * dust[0] + 0.587 * dust[1] + 0.114 * dust[2];
    dust.map(|c| ((c + (grey - c) * DUST_GREY) * DUST_LIFT).min(1.0))
}

/// Pool `pool`'s emitter, its dust of `dust` colour.
fn def(pool: Pool, dust: [f32; 3]) -> EmitterDef {
    let rgba = |[r, g, b]: [f32; 3], a: f32| [r, g, b, a];
    let fade = |c: [f32; 3], start: f32, most: f32, at: f32| ([rgba(c, start), rgba(c, most), rgba(c, 0.0)], at);
    let growing = |a: f32, b: f32, c: f32| ([[a, a * 0.85], [b, b * 0.8], [c, c * 0.75]], 0.35);
    let low = |a: f32, b: f32, c: f32| ([[a, a * 0.6], [b, b * 0.5], [c, c * 0.45]], 0.35);
    // Water and its mist in an overcast's grey: lit by the scene they come out no brighter than the sky.
    let water = [0.72, 0.76, 0.8];
    let splash_mist = [0.8, 0.83, 0.86];
    let wet_mist = [0.5, 0.53, 0.56];
    // The damp road's haze: a greyer white than the drops.
    let mist = [0.86, 0.88, 0.9];
    // Loose ground's water: the ground's colour, darker and browner than its dust.
    let mud = airborne(dust).map(|c| c * MUD_SHADE);
    // The clods and muddy water are the wet ground itself: dark and brown, not the dust's pale.
    let clod = dust.map(|c| c * c * CLOD_SHADE);
    let puff = |colors, sizes, life, rate, rise, friction, wind| Spec {
        sprite: sprites::PUFF,
        colors,
        sizes,
        life,
        trail: 0.0,
        rate,
        rise,
        friction,
        wind,
    };
    let drop = |colour, alpha: f32, size: f32, life, trail, rate, friction| Spec {
        sprite: sprites::DROP,
        colors: fade(colour, alpha, alpha * 0.8, 0.5),
        sizes: ([[size, size]; 3], 0.5),
        life,
        trail,
        rate,
        rise: -9.81,
        // Fine drops lose their speed to the air within a second: the tail curls back behind the car.
        friction,
        wind: 0.0,
    };
    let spec = match pool {
        // Spreading low and settling: no rise, so a bend's earlier dust does not hang as a ball above the road.
        Pool::Dust => puff(
            fade(airborne(dust), 0.45, 0.65, 0.03),
            ([[1.0, 0.85], [3.2, 2.6], [6.5, 4.5]], 0.08),
            4.5,
            DUST_RATE,
            0.0,
            0.5,
            0.6,
        ),
        Pool::Powder => {
            puff(fade([0.95, 0.97, 1.0], 0.4, 1.0, 0.1), growing(0.8, 3.0, 7.0), 3.5, POWDER_RATE, 0.15, 0.8, 0.5)
        }
        Pool::Mist => puff(fade(mist, 0.0, 0.2, 0.2), low(0.8, 2.2, 3.4), 2.5, MIST_RATE * 3.0, 0.2, 0.9, 0.5),
        Pool::WetMist => puff(fade(wet_mist, 0.0, 0.22, 0.2), low(1.0, 2.4, 3.8), 3.2, MIST_RATE * 3.0, 0.3, 0.7, 0.5),
        Pool::MudMist => puff(fade(mud, 0.0, 0.4, 0.12), low(0.5, 1.5, 2.2), 2.0, MIST_RATE * 3.0, -0.4, 1.2, 0.3),
        Pool::Drops => drop(water, 0.55, 0.014, 0.7, 0.04, DROP_RATE, 1.2),
        Pool::MudDrops => drop(clod, 0.9, 0.03, 0.55, 0.03, DROP_RATE * MUD_DROP_GAIN, 0.7),
        Pool::Splash => drop(water, 0.6, 0.035, 0.9, 0.03, SPLASH_RATE, 1.0),
        Pool::SplashMist => {
            puff(fade(splash_mist, 0.0, 0.5, 0.15), low(0.8, 2.0, 3.0), 1.3, SPLASH_MIST_RATE, -2.0, 1.5, 0.3)
        }
    };
    let look = Look {
        sprite: spec.sprite,
        colors: spec.colors.0,
        color_at: spec.colors.1,
        sizes: spec.sizes.0,
        size_at: spec.sizes.1,
        size_variance: match pool {
            Pool::MudMist => 2.5,
            Pool::WetMist => 2.0,
            _ => 0.5,
        },
        life: spec.life,
        weight: 1.0,
        trail: spec.trail,
        additive: false,
        attenuation: [0.0, 0.0],
        offset_height: 0.5,
        stone: false,
        // The drops are drawn as small as they are, as the sparks: a few millimetres, streaked.
        spark: spec.sprite == sprites::DROP,
        wisp: matches!(pool, Pool::Mist | Pool::WetMist | Pool::MudMist | Pool::SplashMist),
    };
    EmitterDef {
        name: format!("{pool:?} (wheel spray, ours)"),
        // Four wheels at full rate (the mist's at its splash), a quarter over, and four bursts.
        max: ((spec.rate * spec.life * 4.0 * 1.25).ceil() + 4.0 * BURST) as usize,
        emit_rate: 0.0,
        shape: Shape::Point,
        force_variance: 0.0,
        look,
        acceleration: Vec3::new(0.0, 0.0, spec.rise),
        friction: spec.friction,
        wind: spec.wind,
        lands: false,
        water: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throw(speed: f32, slip: f32) -> Throw {
        Throw {
            edges: [Vec3::ZERO; 2],
            velocity: Vec3::new(0.0, -speed, 0.0),
            up: Vec3::Z,
            out: Vec3::X,
            forward: -Vec3::Y,
            back: Vec3::Y,
            slip,
            speed,
        }
    }

    fn rate(name: &str, pool: Pool, speed: f32, slip: f32, standing: f32) -> f32 {
        let k = POOLS.iter().position(|&p| p == pool).unwrap();
        rates(&ground::by_name(name), &throw(speed, slip), standing)[k]
    }

    #[test]
    fn dry_gravel_dusts_wet_gravel_sprays() {
        assert!(rate("Gravel_Medium_Deep_Dry", Pool::Dust, 25.0, 0.0, 0.0) > 0.0);
        assert_eq!(rate("Gravel_Medium_Deep_Wet", Pool::Dust, 25.0, 0.0, 0.0), 0.0);
        assert!(rate("Gravel_Medium_Deep_Wet", Pool::MudMist, 25.0, 0.0, 0.0) > 0.0);
        assert!(rate("Gravel_Medium_Deep_Wet", Pool::MudDrops, 25.0, 0.0, 0.0) > 0.0);
        assert_eq!(rate("Gravel_Medium_Deep_Dry", Pool::Drops, 25.0, 0.0, 0.0), 0.0);
        assert_eq!(rate("Gravel_Medium_Deep_Damp", Pool::Drops, 25.0, 0.0, 0.0), 0.0, "a damp road throws no water");
        assert!(
            rate("Gravel_Medium_Deep_Dry", Pool::Dust, 10.0, 6.0, 0.0)
                > rate("Gravel_Medium_Deep_Dry", Pool::Dust, 10.0, 0.0, 0.0)
        );
        assert!(rate("SnowOnGravel_Deep", Pool::Powder, 25.0, 0.0, 0.0) > 0.0);
    }

    #[test]
    fn the_mist_grows_with_speed_and_a_damp_road_throws_a_third_of_it() {
        let wet = |speed| rate("Tarmac_Fine_Clean_Wet", Pool::WetMist, speed, 0.0, 0.0);
        let damp = |name, speed| rate(name, Pool::Mist, speed, 0.0, 0.0);
        assert_eq!(wet(3.0), 0.0);
        assert!(wet(15.0) < 0.6 * wet(28.0));
        let road = damp("Tarmac_Fine_Clean_Damp", 28.0);
        assert!((road * 3.0 * MIST_GAIN * WET_MIST_SHARE - wet(28.0)).abs() < 1e-3, "{road}");
        assert_eq!(damp("Tarmac_Fine_Clean_Wet", 28.0), 0.0);
        let mud = rate("Dirt_Hard_Wet", Pool::MudMist, 28.0, 0.0, 0.0);
        assert!((mud - MUD_MIST * wet(28.0) / WET_MIST_SHARE).abs() < 1e-3, "{mud}");
        let loose = damp("Dirt_Hard_Damp", 28.0);
        assert!((loose - LOOSE_MIST * road).abs() < 1e-3, "{loose}");
    }

    #[test]
    fn standing_water_splashes_with_speed() {
        assert_eq!(rate("Tarmac_Fine_Clean_Wet", Pool::Splash, 0.0, 0.0, 1.0), 0.0);
        assert_eq!(rate("Tarmac_Fine_Clean_Wet", Pool::Splash, 20.0, 0.0, 1.0), SPLASH_RATE);
        assert_eq!(rate("Tarmac_Fine_Clean_Wet", Pool::Splash, 20.0, 0.0, 0.0), 0.0);
    }

    #[test]
    fn the_tread_flings_water_up_its_back_faster_with_speed() {
        let (low, slow) = released(Vec3::ZERO, &throw(10.0, 0.0), 0.8, 0.3);
        let (_, fast) = released(Vec3::ZERO, &throw(30.0, 0.0), 0.8, 0.3);
        assert!(low.z > 0.0 && low.y > 0.0, "up the wheel's back: {low}");
        assert!(slow.z > 0.0 && fast.z > 2.5 * slow.z, "{slow} {fast}");
        let (_, at_ground) = released(Vec3::ZERO, &throw(30.0, 0.0), 0.0, 1.0);
        assert_eq!(at_ground.z, 0.0, "the tread at the contact runs back along the ground");
    }

    #[test]
    fn clean_dry_tarmac_throws_nothing() {
        let rates = rates(&ground::by_name("Tarmac_Fine_Clean_Dry"), &throw(25.0, 10.0), 0.0);
        assert!(rates.iter().all(|&r| r == 0.0), "{rates:?}");
    }

    #[test]
    fn a_wheel_throws_into_its_pools() {
        let text = "version3 1 0 0 1 0 1 0 1 0 0 1 1 1 1 1 1 10 \
            1 pc 13 1 1 1 1 0 1 1 1 1 1 1 1 1 0.1 0.1 0.1 0.1 0.1 0.1 0 1 2 0.41 0 0 0 0 BLEND_BLEND 0 0 0.5 0 \
            1 ec 2 0.2 0.2 -3 0.3 0 0 0 0 0 1 pc \
            1 em ec 500 0 0 1 0 0 -9.81 0 0 0 \
            10 0 \
            1 17 em 0 5 0 0 0 0 0.5 0 0 1 0 -10 1 8 \
            0 1 6 17 0 1 17 0 0 0 0 -7 0.02 0.02 em 0 $";
        let mut sim = Simulation::new(&rbr_formats::pss::parse(text).unwrap(), 1);
        let mut spray = WheelSpray::new(&mut sim, dust_colour("PS_Japan.ini"), false);
        let wet_gravel =
            (0..256).find(|&id| ground::material(id).kind == Kind::Gravel && ground::material(id).wet == 1.0);
        let payload = TyreParticles { surfaces: [wet_gravel.unwrap(); 2], contact: true, ..TyreParticles::default() };
        let pose =
            Pose { velocity: Vec3::new(0.0, -25.0, 0.0), up: Vec3::Z, side: Vec3::X, engine: None, exhaust: None };
        for _ in 0..100 {
            spray.feed(&mut sim, &[payload], &pose, 0.01);
        }
        assert!(sim.count() > 100, "{}", sim.count());
    }
}
