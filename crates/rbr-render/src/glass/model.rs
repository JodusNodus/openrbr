//! The water, snow and bugs on the windscreen, and the wipers that clear them: pure and deterministic for a seed.
//!
//! Everything lives in the glass's chart (`super::Windscreen`): metres across the glass (`x`, to the driver's right)
//! and up it (`y`). Ours, all of it (the 1.02 screen-space sprites of `NParticleSystem2D` are not ported, by the user's
//! decision in rbr-recomp `notes/particles.md`, "In-car glass effects"). From the data: the bug splash's four colours, its life and
//! fade and its car speeds (`misc/Part2D.ini` `[Effect Bug Splash]` and `[Particle System]`). The mud comes from the
//! car's tyre spray (`crate::dirt`), as the paint's dirt does, by what each surface throws (`crate::dirt::thrown`: mud,
//! snow or water); how much reaches the glass is ours.

use glam::Vec2;

use crate::dirt::Thrown;
use crate::particles::sim::Rng;

/// What lies on the glass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Water,
    Snow,
    Bug,
    /// A speck of the tyre spray's mud or snow (ours): its colour index says which ([`speck_color`]).
    Mud,
    /// What a running drop left behind (ours), drying: a ribbon along its path (`span` set) or a bead.
    Trail,
}

/// One drop, flake or splash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drop {
    pub at: Vec2,
    /// Metres.
    pub radius: f32,
    pub kind: Kind,
    pub velocity: Vec2,
    /// Water: how far along its run its open ribbon reaches back, to where it began (zero while it holds). Trails: the
    /// ribbon's far end from `at`, metres; zero for a bead.
    pub span: Vec2,
    pub age: f32,
    /// Snow: the part not melted yet (1 to 0). Trails: the part of their life left (1 to 0). Bugs and mud: the colour
    /// index (mud: [`speck_color`]'s).
    pub left: f32,
    /// A number in [0, 1) for the shader's irregular edges.
    pub seed: f32,
}

/// The weather and the car for a step.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Conditions {
    /// Rain or snow intensity, 0 to 1 (`crate::particles::Precipitation`).
    pub rain: f32,
    pub snow: f32,
    /// The car's speed, m/s.
    pub speed: f32,
    /// Gravity along the glass's chart (`y` down the slope is negative), m/s².
    pub gravity: Vec2,
    /// What the step's tyre spray throws at the glass (`crate::dirt::Spray::thrown`).
    pub spray: Thrown,
}

/// A wiper in the chart: its pivot, the blade's reach from it, and its parked direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wiper {
    pub pivot: Vec2,
    pub reach: [f32; 2],
    /// The parked blade's direction from the pivot, radians.
    pub park: f32,
    /// +1 when the blade sweeps counterclockwise in the chart (up from the park), -1 otherwise.
    pub turn: f32,
    /// The sweep, radians.
    pub sweep: f32,
}

impl Wiper {
    /// Whether a point lies in the area the blade swept between the angles `from` and `to` (from the park), with a
    /// margin of `pad` metres.
    pub fn swept(&self, at: Vec2, from: f32, to: f32, pad: f32) -> bool {
        let d = at - self.pivot;
        let r = d.length();
        if r < self.reach[0] - pad || r > self.reach[1] + pad {
            return false;
        }
        let angle = wrap((d.y.atan2(d.x) - self.park) * self.turn);
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        let slack = pad / r.max(0.05);
        angle >= lo - slack && angle <= hi + slack
    }
}

/// An angle in (-pi, pi].
fn wrap(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let a = a.rem_euclid(tau);
    if a > std::f32::consts::PI {
        a - tau
    } else {
        a
    }
}

/// The wipers' schedule: parked, or one sweep out and back every [`STROKE`] seconds, with a [`PAUSE`] at the park
/// when intermittent. The continuous one is 1.02's: `E88Wiper::Tick` (0x5f8720) turns each blade at a fixed rate
/// from its stop to its other stop and back (`InitTable` 0x5f8a50: 3.4558 and 3.2987 rad/s over 1.3823 and 1.3195
/// rad from 0.0942, 0.4 s each way), whenever the precipitation is above 0 (`E87EffectHost::Tick` 0x5f6790 sets its
/// enable, `0x167e8e8`, to `M9b80(0) * 0.01` each frame), rain or snow; each blade's sweep from the park is its own
/// ([`Wiper::sweep`], `super::SWEEPS`). Ours: the intermittent schedule, and the blades go back to the park when the
/// schedule ends (the game's stop where they are when the enable drops to 0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wiping {
    Off,
    Intermittent,
    Continuous,
}

/// Seconds for a sweep up and back (`E88Wiper`: 0.4 s each way).
pub const STROKE: f32 = 0.8;
/// Seconds parked between intermittent sweeps.
pub const PAUSE: f32 = 2.0;
/// Light rain at or below this intensity wipes now and then (ours): drizzle (`Rain::Light`, 0.5); moderate (0.75)
/// and heavy rain wipe continuously, snow always does.
pub const DRIZZLE_MOST: f32 = 0.6;
/// The specks of mud from which the automatic wipers clear the dry glass (ours).
pub const MUD_WIPE: usize = 300;

/// The wiper switch (ours: the launcher's setting and the wiper key; the 1.02 control is not traced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Switch {
    Off,
    Intermittent,
    Continuous,
    /// By the weather, and in the dry by the mud on the glass ([`Wiping::by_weather`]).
    #[default]
    Auto,
}

impl Switch {
    pub const ALL: [Switch; 4] = [Switch::Off, Switch::Intermittent, Switch::Continuous, Switch::Auto];

    /// The name in the launcher's settings and `viewer --wipers`.
    pub fn name(self) -> &'static str {
        match self {
            Switch::Off => "off",
            Switch::Intermittent => "intermittent",
            Switch::Continuous => "continuous",
            Switch::Auto => "auto",
        }
    }

    pub fn parse(text: &str) -> Option<Switch> {
        Switch::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(text.trim()))
    }

    /// The next position, as the wiper key steps through them.
    pub fn next(self) -> Switch {
        Switch::ALL[(Switch::ALL.iter().position(|&s| s == self).unwrap_or(0) + 1) % Switch::ALL.len()]
    }

    /// The schedule in `rain` and `snow` (intensity, 0 to 1) with `mud` specks on the glass.
    pub fn wiping(self, rain: f32, snow: f32, mud: usize) -> Wiping {
        match self {
            Switch::Off => Wiping::Off,
            Switch::Intermittent => Wiping::Intermittent,
            Switch::Continuous => Wiping::Continuous,
            Switch::Auto => match Wiping::by_weather(rain, snow) {
                Wiping::Off if mud >= MUD_WIPE => Wiping::Intermittent,
                wiping => wiping,
            },
        }
    }
}

impl Wiping {
    /// Automatic by the weather: continuous in rain or snow, as in 1.02 — but for light rain, which wipes now and
    /// then (ours: drizzle, below [`DRIZZLE_MOST`], takes the intermittent schedule).
    pub fn by_weather(rain: f32, snow: f32) -> Wiping {
        if rain.max(snow) <= 0.0 {
            Wiping::Off
        } else if rain > 0.0 && snow <= 0.0 && rain <= DRIZZLE_MOST {
            Wiping::Intermittent
        } else {
            Wiping::Continuous
        }
    }

    /// The blade's angle from the park at `time` seconds, as a fraction of its sweep (0 to 1): out and back at a
    /// constant rate (`E88Wiper::Tick`).
    pub fn phase(self, time: f32) -> f32 {
        let period = match self {
            Wiping::Off => return 0.0,
            Wiping::Intermittent => STROKE + PAUSE,
            Wiping::Continuous => STROKE,
        };
        let t = time.rem_euclid(period);
        if t >= STROKE {
            return 0.0;
        }
        1.0 - (t / STROKE * 2.0 - 1.0).abs()
    }
}

/// New raindrops' radius, metres, mostly small with a few large ones (the cube of a uniform number across the range).
const RAIN_RADIUS: [f32; 2] = [0.0012, 0.0070];
/// New flakes' radius.
const SNOW_RADIUS: [f32; 2] = [0.0025, 0.0065];
/// Drops hitting a square metre of glass per second at full intensity: standing, and more per m/s of speed (the glass
/// sweeps through the rain).
const RAIN_STILL: f32 = 800.0;
const RAIN_PER_SPEED: f32 = 30.0;
const SNOW_STILL: f32 = 25.0;
const SNOW_PER_SPEED: f32 = 4.0;
/// A drop sticks until the pull on it is more than this (m/s²) times `PIN_RADIUS / radius`: small drops hold, big
/// ones run.
const PIN: f32 = 8.0;
const PIN_RADIUS: f32 = 0.002;
/// The airflow's push up and out of the glass per (m/s)² over the radius in millimetres, m/s².
const AIR: f32 = 0.025;
/// The share of the airflow's push that goes sideways, outward from the glass's middle.
const AIR_SIDEWAYS: f32 = 0.35;
/// The sliding speed per m/s² of pull beyond the pin.
const MOBILITY: f32 = 0.025;
/// The fastest a drop slides, m/s: bigger drops slide quicker, by the square of their radius over 2 mm.
const MOST_SPEED: f32 = 0.6;
/// Seconds a flake takes to melt into a drop, and the drop's radius over the flake's.
const MELT: f32 = 14.0;
const MELT_SHRINK: f32 = 0.55;
/// Most drops kept on the glass; the oldest still ones go first.
pub const MOST: usize = 2500;
/// Bugs (`[Effect Bug Splash]`): life and fade-out, seconds, radius (its `Size` 0.02 is a screen fraction; ours: 6
/// to 10 mm), and the car speeds of `[Particle System]` (km/h): none below the minimum, the rate grows to the medium.
pub const BUG_LIFE: f32 = 10.0;
pub const BUG_FADE: f32 = 2.0;
const BUG_RADIUS: [f32; 2] = [0.004, 0.007];
const BUG_SPEEDS: [f32; 2] = [25.0 / 3.6, 75.0 / 3.6];
/// Bugs per second at the medium speed and over (ours).
const BUG_RATE: f32 = 0.25;
/// Mud specks (ours): the radius, and the specks per unit of spray mass that reach the glass at [`MUD_SPEED`] and
/// over (fewer below it, none standing).
const MUD_RADIUS: [f32; 2] = [0.002, 0.007];
const MUD_PER_MASS: f32 = 0.01;
const MUD_SPEED: f32 = 20.0;
/// Mud's colours (ours: drying gravel mud, lighter than the paint's dirt colour in `shader.wgsl`).
const MUD_COLORS: [[f32; 3]; 4] = [[0.52, 0.45, 0.34], [0.45, 0.38, 0.27], [0.58, 0.52, 0.41], [0.40, 0.33, 0.23]];
/// Thrown snow's colours (ours: white-grey slush), mud specks of colour index 4 to 7.
const SNOW_COLORS: [[f32; 3]; 4] = [[0.86, 0.87, 0.89], [0.78, 0.79, 0.81], [0.92, 0.92, 0.94], [0.70, 0.71, 0.73]];

/// The colour of a mud speck of colour index `index`: mud's 0 to 3, snow's 4 to 7.
pub fn speck_color(index: f32) -> [f32; 3] {
    let i = index.max(0.0) as usize % 8;
    if i < 4 {
        MUD_COLORS[i]
    } else {
        SNOW_COLORS[i - 4]
    }
}

/// Whether a mud speck of colour index `index` is snow.
pub fn is_snow(index: f32) -> bool {
    index.max(0.0) as usize % 8 >= 4
}
/// The share of the specks a dry blade leaves smeared on the glass (ours); a wet one clears them all.
const MUD_SMEAR: f32 = 0.35;
/// Trails (ours): a running drop lays a ribbon of water behind it every four of its radii (and at least
/// [`RIBBON_MIN`] metres), [`RIBBON_WIDTH`] of its radius wide (half width) and no thinner than [`RIBBON_THINNEST`],
/// taking the volume of a ribbon [`RIBBON_HEIGHT`] of its width high from the drop. A ribbon dries into beads: it is
/// gone in [`RIBBON_LIFE`] seconds, a bead (one in [`BEAD_CHANCE`] laid beside it, [`BEAD_RADIUS`] of the drop's radius)
/// in [`TRAIL_LIFE`].
const RIBBON_RADII: f32 = 4.0;
const RIBBON_MIN: f32 = 0.006;
/// Only drops this big (metres) lay ribbons; smaller ones leave beads.
const RIBBON_FROM: f32 = 0.0022;
const RIBBON_WIDTH: f32 = 0.3;
const RIBBON_THINNEST: f32 = 0.0003;
const RIBBON_HEIGHT: f32 = 0.4;
pub const RIBBON_LIFE: f32 = 2.5;
const BEAD_CHANCE: f32 = 0.5;
const BEAD_RADIUS: [f32; 2] = [0.2, 0.45];
pub const TRAIL_LIFE: f32 = 5.0;
/// Most trail droplets kept, apart from [`MOST`].
const TRAIL_MOST: usize = 3500;
/// Drops grip the glass by this much more or less than the mean (the pin's factor), a running drop's pin falls to
/// [`RUNNING_PIN`] of it, and drops under [`TOO_SMALL`] metres are gone.
const GRIP: [f32; 2] = [0.7, 1.5];
const RUNNING_PIN: f32 = 0.6;
const TOO_SMALL: f32 = 0.0004;
/// Drops touch when their centres are closer than this share of their radii's sum, checked every [`MERGE_EVERY`]
/// seconds; [`MOST_RADIUS`] the largest radius a drop reaches to look for one.
const TOUCH: f32 = 0.92;
const MERGE_EVERY: f32 = 1.0 / 60.0;
const MOST_RADIUS: f32 = 0.02;
/// The wet film (ours): a grid of [`FILM_CELLS`] over the glass's chart, each 0 (dry) to 1. Rain wets it at
/// [`FILM_WETS`] a second at full intensity, it dries at [`FILM_DRIES`] a second, and a blade clears what it swept.
/// The airflow thins it with speed: it holds at most `1 / (1 + speed / FILM_AIRFLOW)` (a third at 24 m/s), losing
/// what is over that at up to [`FILM_BLOWN`] a second.
pub const FILM_CELLS: [usize; 2] = [48, 24];
const FILM_WETS: f32 = 0.6;
const FILM_DRIES: f32 = 0.02;
const FILM_AIRFLOW: f32 = 20.0;
const FILM_BLOWN: f32 = 0.5;
/// What a blade leaves of the film it sweeps: a thin smear.
const FILM_SMEAR: f32 = 0.1;
/// Snow on the blades (ours): each blade carries what it sweeps, in flakes, at most [`BLADE_MOST`], and loses
/// [`BLADE_MELTS`] of it a second (the blade is outside the heated glass).
pub const BLADE_MOST: f32 = 60.0;
const BLADE_MELTS: f32 = 0.01;
/// A side window's share of the rain the windscreen takes, and of the tyre spray (ours: it faces neither the rain
/// the car drives into nor, as squarely, the spray), which lands mostly low on it.
const SIDE_RAIN: f32 = 0.3;
const SIDE_SPRAY: f32 = 0.1;
/// A side window's drops hold this many times harder (ours: on the vertical pane the windscreen's pin lets every drop
/// run; with it, drops under about 2.5 mm stay).
const SIDE_PIN: f32 = 3.0;
/// `[Effect Bug Splash]`'s four colours, 0 to 1.
pub const BUG_COLORS: [[f32; 3]; 4] = [
    [166.0 / 255.0, 153.0 / 255.0, 54.0 / 255.0],
    [116.0 / 255.0, 67.0 / 255.0, 40.0 / 255.0],
    [131.0 / 255.0, 106.0 / 255.0, 40.0 / 255.0],
    [152.0 / 255.0, 131.0 / 255.0, 76.0 / 255.0],
];

/// The specks on the glass after `thrown` of them landed with the automatic wipers on (ours): all of them below
/// [`MUD_WIPE`]; past it, each wipe leaves [`MUD_SMEAR`] of the specks and the glass fills up to [`MUD_WIPE`] again.
pub fn settled_mud(thrown: f32) -> usize {
    let (thrown, most) = (thrown.max(0.0) as usize, MUD_WIPE);
    let smeared = (most as f32 * MUD_SMEAR).round() as usize;
    if thrown < most {
        thrown
    } else {
        smeared + (thrown - most) % (most - smeared)
    }
}

/// The glass's drops and wipers.
#[derive(Debug, Clone)]
pub struct Glass {
    pub drops: Vec<Drop>,
    pub wipers: Vec<Wiper>,
    /// The wipers' angle fraction after the last step.
    pub phase: f32,
    /// The glass's chart: `inside` says whether a point is on the glass, `area` its square metres, `centre_x` its
    /// middle across.
    area: f32,
    bounds: [Vec2; 2],
    centre_x: f32,
    rng: Rng,
    carry: f32,
    /// The spray's mud, snow and water not yet thrown as a whole speck or drop.
    mud_carry: [f32; 3],
    /// Seconds since the drops last merged.
    merge_clock: f32,
    /// The wet film, [`FILM_CELLS`] row by row from the chart's low corner.
    pub film: Vec<f32>,
    /// The snow each blade carries, in flakes.
    pub blade_snow: Vec<f32>,
    /// A side window's (ours): the way the airflow runs along it in the chart, toward the car's back. `None` for the
    /// windscreen, whose airflow runs up and out and which takes the bugs.
    back: Option<Vec2>,
}

impl Glass {
    /// An empty glass whose chart spans `bounds` (low and high corner) with `area` square metres of glass.
    pub fn new(bounds: [Vec2; 2], area: f32, wipers: Vec<Wiper>, seed: u32) -> Glass {
        Glass {
            drops: Vec::new(),
            phase: 0.0,
            area,
            bounds,
            centre_x: (bounds[0].x + bounds[1].x) * 0.5,
            rng: Rng::new(seed),
            carry: 0.0,
            mud_carry: [0.0; 3],
            merge_clock: 0.0,
            film: vec![0.0; FILM_CELLS[0] * FILM_CELLS[1]],
            blade_snow: vec![0.0; wipers.len()],
            wipers,
            back: None,
        }
    }

    /// An empty side window whose chart spans `bounds` with `area` square metres of glass, its airflow running along
    /// `back` (the car's back, in the chart): no wipers, no bugs, [`SIDE_RAIN`] of the rain and [`SIDE_SPRAY`] of the
    /// spray.
    pub fn side(bounds: [Vec2; 2], area: f32, back: Vec2, seed: u32) -> Glass {
        Glass { back: Some(back.normalize_or_zero()), ..Glass::new(bounds, area, Vec::new(), seed) }
    }

    pub fn clear(&mut self) {
        self.drops.clear();
        self.carry = 0.0;
        self.mud_carry = [0.0; 3];
        self.film.fill(0.0);
        self.blade_snow.fill(0.0);
    }

    /// The chart point at the middle of film cell `index`, and a cell's size.
    pub fn film_cell(&self, index: usize) -> (Vec2, Vec2) {
        let [lo, hi] = self.bounds;
        let size = (hi - lo) / Vec2::new(FILM_CELLS[0] as f32, FILM_CELLS[1] as f32);
        let (column, row) = (index % FILM_CELLS[0], index / FILM_CELLS[0]);
        (lo + size * Vec2::new(column as f32 + 0.5, row as f32 + 0.5), size)
    }

    /// The chart point at the film cells' corner `column`, `row` (0 to [`FILM_CELLS`]), the same for every cell
    /// meeting there.
    pub fn film_corner(&self, column: usize, row: usize) -> Vec2 {
        let [lo, hi] = self.bounds;
        lo + (hi - lo) * Vec2::new(column as f32 / FILM_CELLS[0] as f32, row as f32 / FILM_CELLS[1] as f32)
    }

    /// One step of `dt` seconds: new drops, flakes and bugs, the drops' motion and merging, melting and fading, then the
    /// wipers moving to `phase` (their angle fraction) and clearing what they swept. `inside` says whether a chart point
    /// is on the glass.
    pub fn step(&mut self, dt: f32, c: Conditions, phase: f32, inside: impl Fn(Vec2) -> bool) {
        let c = match self.back {
            Some(_) => Conditions {
                rain: c.rain * SIDE_RAIN,
                snow: c.snow * SIDE_RAIN,
                spray: c.spray.scaled(SIDE_SPRAY),
                ..c
            },
            None => c,
        };
        self.spawn(dt, c, &inside);
        self.spatter(c, &inside);
        self.flow(dt, c);
        self.merge(dt);
        // The film thickens to the rain's amount, less with the airflow.
        let most = c.rain.min(1.0) / (1.0 + c.speed.max(0.0) / FILM_AIRFLOW);
        for f in &mut self.film {
            let wetted = *f + (c.rain * FILM_WETS - FILM_DRIES) * dt;
            *f = wetted.min(most.max(*f - FILM_BLOWN * dt)).clamp(0.0, 1.0);
        }
        for b in &mut self.blade_snow {
            *b = (*b * (1.0 - BLADE_MELTS * dt)).max(0.0);
        }
        self.drops.retain_mut(|d| {
            d.age += dt;
            match d.kind {
                Kind::Snow => {
                    d.left -= dt / MELT;
                    if d.left <= 0.0 {
                        d.kind = Kind::Water;
                        d.radius *= MELT_SHRINK;
                        d.left = 0.0;
                    }
                    true
                }
                Kind::Bug => d.age < BUG_LIFE + BUG_FADE,
                Kind::Water => inside(d.at) && d.radius >= TOO_SMALL,
                Kind::Mud => true,
                Kind::Trail => {
                    d.left -= dt / if d.span == Vec2::ZERO { TRAIL_LIFE } else { RIBBON_LIFE };
                    d.left > 0.0
                }
            }
        });
        self.wipe(phase, c.rain > 0.0 || c.snow > 0.0);
        // Past the most kept, the oldest go first (they are in spawn order, merges aside); trails count apart.
        let trails = self.drops.iter().filter(|d| d.kind == Kind::Trail).count();
        let mut extra = [(self.drops.len() - trails).saturating_sub(MOST), trails.saturating_sub(TRAIL_MOST)];
        if extra != [0, 0] {
            self.drops.retain(|d| {
                let k = usize::from(d.kind == Kind::Trail);
                let gone = extra[k] > 0;
                extra[k] -= usize::from(gone);
                !gone
            });
        }
    }

    fn spawn(&mut self, dt: f32, c: Conditions, inside: &impl Fn(Vec2) -> bool) {
        let speed = c.speed.max(0.0);
        let rain = c.rain * (RAIN_STILL + RAIN_PER_SPEED * speed);
        let snow = c.snow * (SNOW_STILL + SNOW_PER_SPEED * speed);
        let bugs = if self.back.is_none() && c.rain <= 0.0 && c.snow <= 0.0 && speed > BUG_SPEEDS[0] {
            BUG_RATE * ((speed - BUG_SPEEDS[0]) / (BUG_SPEEDS[1] - BUG_SPEEDS[0])).min(1.0)
        } else {
            0.0
        };
        let total = (rain + snow) * self.area + bugs;
        if total <= 0.0 {
            return;
        }
        self.carry += total * dt;
        let n = self.carry.floor() as usize;
        self.carry -= n as f32;
        for _ in 0..n {
            let pick = self.rng.unit() * total;
            let (kind, sizes, power) = if pick < rain * self.area {
                (Kind::Water, RAIN_RADIUS, 3)
            } else if pick < (rain + snow) * self.area {
                (Kind::Snow, SNOW_RADIUS, 2)
            } else {
                (Kind::Bug, BUG_RADIUS, 2)
            };
            let [lo, hi] = self.bounds;
            let at = lo + (hi - lo) * Vec2::new(self.rng.unit(), self.rng.unit());
            let radius = sizes[0] + (sizes[1] - sizes[0]) * self.rng.unit().powi(power);
            let left = match kind {
                Kind::Snow => 1.0,
                Kind::Bug | Kind::Mud => (self.rng.unit() * 4.0).floor().min(3.0),
                Kind::Water | Kind::Trail => 0.0,
            };
            let seed = self.rng.unit();
            if inside(at) {
                self.drops.push(Drop {
                    at,
                    radius,
                    kind,
                    velocity: Vec2::ZERO,
                    span: Vec2::ZERO,
                    age: 0.0,
                    left,
                    seed,
                });
            }
        }
    }

    /// Mud, snow and water from the step's tyre spray, at speed (ours).
    fn spatter(&mut self, c: Conditions, inside: &impl Fn(Vec2) -> bool) {
        let share = (c.speed / MUD_SPEED).clamp(0.0, 1.0);
        let Thrown { mud, snow, water } = c.spray;
        for (k, mass) in [mud, snow, water].into_iter().enumerate() {
            self.mud_carry[k] += mass.max(0.0) * MUD_PER_MASS * share;
            while self.mud_carry[k] >= 1.0 {
                self.mud_carry[k] -= 1.0;
                match k {
                    2 => self.splash(inside),
                    _ => _ = self.speck(k == 1, inside),
                }
            }
        }
    }

    /// One speck of mud (or of `snow`) thrown at the glass; whether it landed on it.
    fn speck(&mut self, snow: bool, inside: &impl Fn(Vec2) -> bool) -> bool {
        let at = self.sprayed_at();
        let radius = MUD_RADIUS[0] + (MUD_RADIUS[1] - MUD_RADIUS[0]) * self.rng.unit().powi(2);
        let left = (self.rng.unit() * 4.0).floor().min(3.0) + if snow { 4.0 } else { 0.0 };
        let seed = self.rng.unit();
        let landed = inside(at);
        if landed {
            self.drops.push(Drop {
                at,
                radius,
                kind: Kind::Mud,
                velocity: Vec2::ZERO,
                span: Vec2::ZERO,
                age: 0.0,
                left,
                seed,
            });
        }
        landed
    }

    /// One drop of a wet road's spray thrown at the glass: water, as rain's.
    fn splash(&mut self, inside: &impl Fn(Vec2) -> bool) {
        let at = self.sprayed_at();
        let radius = RAIN_RADIUS[0] + (RAIN_RADIUS[1] - RAIN_RADIUS[0]) * self.rng.unit().powi(3);
        let seed = self.rng.unit();
        if inside(at) {
            self.drops.push(Drop {
                at,
                radius,
                kind: Kind::Water,
                velocity: Vec2::ZERO,
                span: Vec2::ZERO,
                age: 0.0,
                left: 0.0,
                seed,
            });
        }
    }

    /// Where a speck of spray lands: anywhere on the windscreen, low on a side window (the square of a uniform number
    /// up it).
    fn sprayed_at(&mut self) -> Vec2 {
        let [lo, hi] = self.bounds;
        let (x, y) = (self.rng.unit(), self.rng.unit());
        lo + (hi - lo) * Vec2::new(x, if self.back.is_some() { y * y } else { y })
    }

    /// Spatters the mud and snow `thrown` at once (ours: the spray before a jump in the drive): the specks
    /// [`settled_mud`] leaves on the glass, so that the automatic wipers go on clearing them at [`MUD_WIPE`]. The
    /// water has run off.
    pub fn spatter_all(&mut self, thrown: Thrown, inside: impl Fn(Vec2) -> bool) {
        let thrown = if self.back.is_some() { thrown.scaled(SIDE_SPRAY) } else { thrown };
        let mass = thrown.mud.max(0.0) + thrown.snow.max(0.0);
        let count = settled_mud(mass * MUD_PER_MASS);
        let snow = if mass > 0.0 { thrown.snow.max(0.0) / mass } else { 0.0 };
        let mut landed = 0;
        // Specks off the glass are thrown again, within a bound for a glass that catches few.
        for _ in 0..count * 8 {
            if landed == count {
                break;
            }
            let is_snow = snow > 0.0 && self.rng.unit() < snow;
            landed += usize::from(self.speck(is_snow, &inside));
        }
    }

    /// The specks of mud on the glass.
    pub fn mud(&self) -> usize {
        self.drops.iter().filter(|d| d.kind == Kind::Mud).count()
    }

    /// The pull on each water drop: gravity down the glass and the airflow up and out of it. A drop holds until the pull
    /// passes its pin (each drop grips its own: contact angles differ), then runs, and keeps running down to a lower
    /// pull; it lays a ribbon of water behind it every [`RIBBON_MIN`] or four radii and a bead now and then, from its own
    /// volume, so it shrinks as it runs and stops when it is too small to run.
    fn flow(&mut self, dt: f32, c: Conditions) {
        let speed2 = c.speed * c.speed;
        let mut laid = Vec::new();
        for d in self.drops.iter_mut().filter(|d| d.kind == Kind::Water) {
            let air = AIR * speed2 / (d.radius * 1000.0);
            let outward = if d.at.x >= self.centre_x { 1.0 } else { -1.0 };
            let pull = c.gravity + self.back.map_or(Vec2::new(outward * air * AIR_SIDEWAYS, air), |back| back * air);
            let grip = GRIP[0] + (GRIP[1] - GRIP[0]) * (d.seed * 13.7).fract();
            let pin = PIN * PIN_RADIUS / d.radius * grip * if self.back.is_some() { SIDE_PIN } else { 1.0 };
            let beyond = pull.length() - pin * if d.velocity == Vec2::ZERO { 1.0 } else { RUNNING_PIN };
            let mobility = MOBILITY * (d.radius * 500.0).powi(2);
            d.velocity =
                if beyond > 0.0 { pull.normalize_or_zero() * (beyond * mobility).min(MOST_SPEED) } else { Vec2::ZERO };
            if d.velocity == Vec2::ZERO {
                d.span = Vec2::ZERO;
                continue;
            }
            let step = d.velocity * dt;
            d.at += step;
            d.span -= step;
            if d.radius >= RIBBON_FROM && d.span.length() >= (d.radius * RIBBON_RADII).max(RIBBON_MIN) {
                let seed = self.rng.unit();
                let ribbon = Drop {
                    at: d.at + d.span,
                    radius: (d.radius * RIBBON_WIDTH).max(RIBBON_THINNEST),
                    kind: Kind::Trail,
                    velocity: Vec2::ZERO,
                    span: -d.span,
                    age: 0.0,
                    left: 1.0,
                    seed,
                };
                // Its water goes into the ribbon (the volume of a ribbon of this width and a fifth of it as high).
                let volume = d.span.length() * ribbon.radius * ribbon.radius * RIBBON_HEIGHT;
                d.radius = (d.radius.powi(3) - volume * 0.75 / std::f32::consts::PI).max(0.0).cbrt();
                laid.push(ribbon);
                // And now and then a bead where it was.
                if self.rng.unit() < BEAD_CHANCE {
                    let radius = d.radius * (BEAD_RADIUS[0] + (BEAD_RADIUS[1] - BEAD_RADIUS[0]) * self.rng.unit());
                    let side = Vec2::new(-d.velocity.y, d.velocity.x).normalize_or_zero() * (self.rng.unit() - 0.5);
                    let seed = self.rng.unit();
                    laid.push(Drop {
                        at: d.at + d.span * self.rng.unit() + side * d.radius * 2.0,
                        radius: radius.max(RIBBON_THINNEST),
                        kind: Kind::Trail,
                        velocity: Vec2::ZERO,
                        span: Vec2::ZERO,
                        age: 0.0,
                        left: 1.0,
                        seed,
                    });
                }
                d.span = Vec2::ZERO;
            }
        }
        self.drops.extend(laid);
    }

    /// Drops that touch run together, volume kept, at the place of their centres of volume; the bigger one's motion
    /// carries on. A swept line of the drops by x finds the neighbours. Only [`MERGE_EVERY`] seconds apart.
    fn merge(&mut self, dt: f32) {
        self.merge_clock += dt;
        if self.merge_clock < MERGE_EVERY {
            return;
        }
        self.merge_clock = 0.0;
        let mut order: Vec<usize> = (0..self.drops.len()).filter(|&i| self.drops[i].kind == Kind::Water).collect();
        order.sort_unstable_by(|&a, &b| self.drops[a].at.x.total_cmp(&self.drops[b].at.x));
        let mut gone = vec![false; self.drops.len()];
        for (k, &i) in order.iter().enumerate() {
            if gone[i] {
                continue;
            }
            for &j in &order[k + 1..] {
                let (a, b) = (self.drops[i], self.drops[j]);
                if b.at.x - a.at.x > a.radius + MOST_RADIUS {
                    break;
                }
                if gone[j] || a.at.distance(b.at) >= (a.radius + b.radius) * TOUCH {
                    continue;
                }
                let (va, vb) = (a.radius.powi(3), b.radius.powi(3));
                let bigger = if va >= vb { a } else { b };
                self.drops[i] = Drop {
                    at: (a.at * va + b.at * vb) / (va + vb),
                    radius: (va + vb).cbrt(),
                    velocity: bigger.velocity,
                    span: Vec2::ZERO,
                    ..bigger
                };
                gone[j] = true;
            }
        }
        let mut k = 0;
        self.drops.retain(|_| {
            k += 1;
            !gone[k - 1]
        });
    }

    /// Moves the wipers to `phase` and clears the water and snow in the area each blade swept since the last step.
    /// Bugs stay (the blades smear them, ours: they are left as they are); on `wet` glass the mud goes too, on dry
    /// glass [`MUD_SMEAR`] of it stays. A blade gathers the snow it sweeps and clears the film.
    fn wipe(&mut self, phase: f32, wet: bool) {
        let from = self.phase;
        self.phase = phase;
        if from == phase {
            return;
        }
        let (wipers, blade_snow) = (&self.wipers, &mut self.blade_snow);
        self.drops.retain(|d| {
            let stays = match d.kind {
                Kind::Bug => true,
                Kind::Mud => !wet && d.seed < MUD_SMEAR,
                Kind::Water | Kind::Snow | Kind::Trail => false,
            };
            let by = wipers.iter().position(|w| w.swept(d.at, from * w.sweep, phase * w.sweep, d.radius));
            if let (Some(k), Kind::Snow, false) = (by, d.kind, stays) {
                blade_snow[k] = (blade_snow[k] + d.left.max(0.3)).min(BLADE_MOST);
            }
            stays || by.is_none()
        });
        for index in 0..self.film.len() {
            let at = self.film_cell(index).0;
            if self.wipers.iter().any(|w| w.swept(at, from * w.sweep, phase * w.sweep, 0.0)) {
                self.film[index] = self.film[index].min(FILM_SMEAR);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glass(wipers: Vec<Wiper>) -> Glass {
        Glass::new([Vec2::new(-0.7, -0.4), Vec2::new(0.7, 0.4)], 1.12, wipers, 9)
    }

    const ALL: fn(Vec2) -> bool = |_| true;
    const SLOPE: Vec2 = Vec2::new(0.0, -5.2);

    fn wiper() -> Wiper {
        Wiper { pivot: Vec2::new(-0.6, -0.4), reach: [0.2, 0.6], park: 0.0, turn: 1.0, sweep: 1.6 }
    }

    #[test]
    fn rain_falls_on_the_glass_faster_when_driving() {
        let count = |speed| {
            let mut g = glass(Vec::new());
            for _ in 0..100 {
                g.step(0.01, Conditions { rain: 1.0, speed, gravity: SLOPE, ..Default::default() }, 0.0, ALL);
            }
            g.drops.iter().filter(|d| d.kind != Kind::Trail).count()
        };
        let (still, fast) = (count(0.0), count(25.0));
        assert!(still > 600 && still < 900, "{still}");
        assert!(fast * 2 > still * 3, "{fast} vs {still}");
        assert_eq!(count(0.0), still, "deterministic");
    }

    #[test]
    fn dry_glass_stays_dry_and_bugs_come_only_at_speed() {
        let mut g = glass(Vec::new());
        let dry = Conditions { speed: 3.0, gravity: SLOPE, ..Default::default() };
        for _ in 0..3000 {
            g.step(0.01, dry, 0.0, ALL);
        }
        assert!(g.drops.is_empty());
        for _ in 0..3000 {
            g.step(0.01, Conditions { speed: 30.0, ..dry }, 0.0, ALL);
        }
        assert!(g.drops.iter().all(|d| d.kind == Kind::Bug) && !g.drops.is_empty());
        assert!(g.drops.iter().all(|d| d.age < BUG_LIFE + BUG_FADE));
    }

    #[test]
    fn small_drops_hold_big_ones_run_down_and_airflow_pushes_them_up() {
        let at = Vec2::new(0.2, 0.0);
        let run = |radius: f32, speed: f32| {
            let mut g = glass(Vec::new());
            g.drops.push(Drop {
                at,
                radius,
                kind: Kind::Water,
                velocity: Vec2::ZERO,
                span: Vec2::ZERO,
                age: 0.0,
                left: 0.0,
                seed: 0.0,
            });
            for _ in 0..50 {
                g.step(0.01, Conditions { speed, gravity: SLOPE, ..Default::default() }, 0.0, ALL);
            }
            g.drops[0].at - at
        };
        assert_eq!(run(0.0008, 0.0), Vec2::ZERO);
        assert!(run(0.004, 0.0).y < -0.01);
        let pushed = run(0.0015, 30.0);
        assert!(pushed.y > 0.01 && pushed.x > 0.0, "{pushed}");
    }

    #[test]
    fn a_side_window_holds_its_small_drops_streams_them_back_and_takes_no_bugs() {
        let at = Vec2::new(0.0, 0.0);
        let back = Vec2::new(-1.0, 0.0);
        let down = Vec2::new(0.0, -9.81);
        let run = |radius: f32, speed: f32| {
            let mut g = Glass::side([Vec2::new(-0.5, -0.25), Vec2::new(0.5, 0.25)], 0.5, back, 9);
            g.drops.push(Drop {
                at,
                radius,
                kind: Kind::Water,
                velocity: Vec2::ZERO,
                span: Vec2::ZERO,
                age: 0.0,
                left: 0.0,
                seed: 0.0,
            });
            for _ in 0..50 {
                g.step(0.01, Conditions { speed, gravity: down, ..Default::default() }, 0.0, ALL);
            }
            g.drops[0].at - at
        };
        assert_eq!(run(0.002, 0.0), Vec2::ZERO, "held on the vertical pane");
        assert!(run(0.006, 0.0).y < -0.01, "a big one runs down");
        assert!(run(0.003, 30.0).x < -0.01, "the airflow takes it back");
        let mut g = Glass::side([Vec2::new(-0.5, -0.25), Vec2::new(0.5, 0.25)], 0.5, back, 9);
        for _ in 0..3000 {
            g.step(0.01, Conditions { speed: 30.0, gravity: down, ..Default::default() }, 0.0, ALL);
        }
        assert!(g.drops.is_empty());
    }

    #[test]
    fn a_running_drop_takes_in_what_it_meets() {
        let mut g = glass(Vec::new());
        let drop = |y: f32, radius| Drop {
            at: Vec2::new(0.0, y),
            radius,
            kind: Kind::Water,
            velocity: Vec2::ZERO,
            span: Vec2::ZERO,
            age: 0.0,
            left: 0.0,
            seed: 0.0,
        };
        g.drops = vec![drop(0.0, 0.004), drop(-0.02, 0.001), drop(-0.05, 0.001)];
        for _ in 0..100 {
            g.step(0.01, Conditions { gravity: SLOPE, ..Default::default() }, 0.0, ALL);
        }
        let (trails, drops): (Vec<Drop>, Vec<Drop>) = g.drops.iter().partition(|d| d.kind == Kind::Trail);
        assert_eq!(drops.len(), 1);
        // The volume of the three, less what its ribbons took.
        let volume: f32 = 0.004f32.powi(3) + 2.0 * 0.001f32.powi(3);
        assert!(drops[0].radius < volume.cbrt() && drops[0].radius > 0.7 * volume.cbrt(), "{}", drops[0].radius);
        // It left ribbons up its path and beads beside it, which dry.
        let ribbons: Vec<&Drop> = trails.iter().filter(|t| t.span != Vec2::ZERO).collect();
        assert!(!ribbons.is_empty() && ribbons.iter().all(|t| t.at.x == 0.0 && t.at.y > drops[0].at.y), "{trails:?}");
        assert!(ribbons.iter().all(|t| t.span.x == 0.0 && t.span.y < 0.0), "they reach down to where it went");
        assert!(trails.iter().any(|t| t.span == Vec2::ZERO), "beads");
        for _ in 0..(TRAIL_LIFE * 100.0) as usize + 1 {
            g.step(0.01, Conditions::default(), 0.0, ALL);
        }
        assert!(g.drops.iter().all(|d| d.kind != Kind::Trail));
    }

    #[test]
    fn rain_wets_the_film_a_blade_clears_it_and_the_airflow_thins_it() {
        let mut g = glass(vec![wiper()]);
        let rain = Conditions { rain: 1.0, gravity: SLOPE, ..Default::default() };
        for _ in 0..600 {
            g.step(0.01, rain, 0.0, ALL);
        }
        assert!(g.film.iter().all(|&f| f > 0.9), "{:?}", g.film);
        let [lo, hi] = g.bounds;
        let cell = |at: Vec2| {
            let f = (at - lo) / (hi - lo);
            ((f.y * FILM_CELLS[1] as f32) as usize) * FILM_CELLS[0] + (f.x * FILM_CELLS[0] as f32) as usize
        };
        // Half a sweep: behind the blade the glass is clear, ahead of it and out of reach still wet.
        g.step(0.01, rain, 0.5, ALL);
        let w = wiper();
        let towards = |angle: f32, r: f32| w.pivot + Vec2::new(angle.cos(), angle.sin()) * r;
        assert!(g.film[cell(towards(0.3, 0.45))] <= FILM_SMEAR + 1e-3, "a blade leaves a thin smear");
        assert!(g.film[cell(towards(1.4, 0.45))] > 0.9);
        assert!(g.film[cell(Vec2::new(0.6, 0.3))] > 0.9);
        // At 24 m/s the airflow leaves a third of it.
        for _ in 0..1000 {
            g.step(0.01, Conditions { speed: 24.0, ..rain }, 0.5, ALL);
        }
        let out_of_reach = g.film[cell(Vec2::new(0.6, 0.3))];
        assert!((out_of_reach - 1.0 / 3.0).abs() < 0.02, "{out_of_reach}");
        // Dry, it all dries off.
        for _ in 0..6000 {
            g.step(0.01, Conditions { gravity: SLOPE, ..Default::default() }, 0.5, ALL);
        }
        assert!(g.film.iter().all(|&f| f == 0.0));
    }

    #[test]
    fn a_blade_gathers_the_snow_it_sweeps() {
        let mut g = glass(vec![wiper()]);
        let snow = Conditions { snow: 1.0, speed: 10.0, gravity: SLOPE, ..Default::default() };
        for _ in 0..300 {
            g.step(0.01, snow, 0.0, ALL);
        }
        assert_eq!(g.blade_snow, [0.0]);
        g.step(0.01, snow, 1.0, ALL);
        assert!(g.blade_snow[0] > 1.0 && g.blade_snow[0] <= BLADE_MOST, "{:?}", g.blade_snow);
        let carried = g.blade_snow[0];
        g.step(1.0, Conditions::default(), 1.0, ALL);
        assert!(g.blade_snow[0] < carried && g.blade_snow[0] > 0.9 * carried);
    }

    #[test]
    fn the_wiper_clears_what_it_sweeps_and_nothing_else() {
        let w = wiper();
        // On the blade's path at 45 degrees, beyond its reach, and below its park.
        let swept = w.pivot + Vec2::from_angle(0.8) * 0.4;
        let beyond = w.pivot + Vec2::from_angle(0.8) * 0.75;
        let below = w.pivot + Vec2::from_angle(-0.5) * 0.4;
        let mut g = glass(vec![w]);
        for at in [swept, beyond, below] {
            g.drops.push(Drop {
                at,
                radius: 0.001,
                kind: Kind::Water,
                velocity: Vec2::ZERO,
                span: Vec2::ZERO,
                age: 0.0,
                left: 0.0,
                seed: 0.0,
            });
        }
        g.step(0.01, Conditions::default(), 0.3, ALL);
        assert_eq!(g.drops.len(), 3, "the blade has not reached 0.8 rad yet");
        g.step(0.01, Conditions::default(), 0.6, ALL);
        let left: Vec<Vec2> = g.drops.iter().map(|d| d.at).collect();
        assert_eq!(left, [beyond, below]);
    }

    #[test]
    fn the_wiper_turns_either_way() {
        let w = Wiper { pivot: Vec2::ZERO, reach: [0.1, 0.5], park: std::f32::consts::PI, turn: -1.0, sweep: 1.6 };
        // Parked pointing left, it sweeps up clockwise.
        assert!(w.swept(Vec2::new(-0.2, 0.2), 0.0, 1.0, 0.0));
        assert!(!w.swept(Vec2::new(-0.2, -0.2), 0.0, 1.0, 0.0));
    }

    #[test]
    fn wiping_by_weather() {
        assert_eq!(Wiping::by_weather(0.0, 0.0), Wiping::Off);
        assert_eq!(Wiping::by_weather(0.5, 0.0), Wiping::Intermittent, "drizzle wipes now and then");
        assert_eq!(Wiping::by_weather(0.75, 0.0), Wiping::Continuous);
        assert_eq!(Wiping::by_weather(1.0, 0.0), Wiping::Continuous);
        assert_eq!(Wiping::by_weather(0.0, 1.0), Wiping::Continuous);
        assert_eq!(Wiping::by_weather(0.0, 0.5), Wiping::Continuous, "light snow as before");
        assert_eq!(Wiping::Off.phase(0.4), 0.0);
        let c = Wiping::Continuous;
        assert_eq!(c.phase(0.0), 0.0);
        assert!((c.phase(STROKE * 0.5) - 1.0).abs() < 1e-6);
        assert!((c.phase(STROKE * 0.25) - 0.5).abs() < 1e-5);
        assert_eq!(Wiping::Intermittent.phase(STROKE + 1.0), 0.0, "parked in the pause");
    }

    #[test]
    fn spray_spatters_mud_at_speed_and_wet_blades_clear_it() {
        let mut g = glass(vec![wiper()]);
        let spray = Thrown { mud: 20.0, ..Thrown::default() };
        let dusty = Conditions { speed: 25.0, spray, gravity: SLOPE, ..Default::default() };
        for _ in 0..500 {
            g.step(0.01, dusty, 0.0, ALL);
        }
        let mud = g.mud();
        assert!(mud > 10, "{mud}");
        let mut standing = glass(Vec::new());
        standing.step(0.01, Conditions { speed: 0.0, ..dusty }, 0.0, ALL);
        assert_eq!(standing.mud(), 0);
        // A dry sweep and back leaves some smeared; a wet one clears what it reaches.
        let mut dry = g.clone();
        for phase in [0.5, 1.0, 0.5, 0.0] {
            dry.step(0.01, Conditions::default(), phase, ALL);
        }
        let w = wiper();
        let reached =
            |g: &Glass| g.drops.iter().filter(|d| d.kind == Kind::Mud && w.swept(d.at, 0.0, w.sweep, 0.0)).count();
        assert!(reached(&dry) > 0 && reached(&dry) < reached(&g));
        let mut wet = g.clone();
        for phase in [0.5, 1.0, 0.5, 0.0] {
            wet.step(0.01, Conditions { rain: 0.01, ..Default::default() }, phase, ALL);
        }
        assert_eq!(wet.drops.iter().filter(|d| d.kind == Kind::Mud && w.swept(d.at, 0.0, w.sweep, 0.0)).count(), 0);
    }

    #[test]
    fn a_jump_leaves_the_mud_the_wipers_would_have() {
        assert_eq!(settled_mud(-5.0), 0);
        assert_eq!(settled_mud(120.7), 120);
        assert_eq!(settled_mud(MUD_WIPE as f32), 105);
        assert_eq!(settled_mud(MUD_WIPE as f32 + 194.0), 299);
        assert_eq!(settled_mud(MUD_WIPE as f32 + 195.0), 105);
        let mut g = glass(Vec::new());
        g.spatter_all(Thrown { mud: 250.0 / MUD_PER_MASS, water: 1e6, ..Thrown::default() }, |at| at.x < 0.5);
        assert_eq!(g.mud(), 250, "specks off the glass are thrown again");
        assert_eq!(g.drops.len(), 250, "the water has run off");
    }

    #[test]
    fn the_spray_throws_its_surface_s_dirt() {
        let throw = |spray: Thrown| {
            let mut g = glass(Vec::new());
            for _ in 0..500 {
                g.step(0.01, Conditions { speed: 25.0, spray, gravity: SLOPE, ..Default::default() }, 0.0, ALL);
            }
            g
        };
        assert!(throw(Thrown::default()).drops.iter().all(|d| d.kind == Kind::Bug), "dry tarmac throws nothing");
        let snow = throw(Thrown { snow: 20.0, ..Thrown::default() });
        let specks: Vec<&Drop> = snow.drops.iter().filter(|d| d.kind == Kind::Mud).collect();
        assert!(specks.len() > 10 && specks.iter().all(|d| is_snow(d.left)), "snow's specks");
        assert!(speck_color(specks[0].left).iter().all(|&c| c > 0.65), "white-grey");
        assert!(speck_color(0.0)[0] < 0.6, "mud's brown");
        let wet = throw(Thrown { water: 20.0, ..Thrown::default() });
        assert_eq!(wet.mud(), 0, "wet tarmac throws water");
        assert!(wet.drops.iter().any(|d| d.kind == Kind::Water));
    }

    #[test]
    fn the_switch_steps_round_and_auto_wipes_off_the_mud() {
        assert_eq!(Switch::Auto.next(), Switch::Off);
        assert_eq!(Switch::Off.next(), Switch::Intermittent);
        assert_eq!(Switch::parse("Continuous"), Some(Switch::Continuous));
        assert_eq!(Switch::parse("fast"), None);
        assert_eq!(Switch::Auto.wiping(0.0, 0.0, 0), Wiping::Off);
        assert_eq!(Switch::Auto.wiping(0.0, 0.0, MUD_WIPE), Wiping::Intermittent);
        assert_eq!(Switch::Auto.wiping(1.0, 0.0, 0), Wiping::Continuous);
        assert_eq!(Switch::Off.wiping(1.0, 0.0, 999), Wiping::Off);
    }

    #[test]
    fn snow_sticks_then_melts_into_drops() {
        let mut g = glass(Vec::new());
        for _ in 0..200 {
            g.step(0.01, Conditions { snow: 1.0, speed: 10.0, gravity: SLOPE, ..Default::default() }, 0.0, ALL);
        }
        assert!(!g.drops.is_empty() && g.drops.iter().all(|d| d.kind == Kind::Snow && d.velocity == Vec2::ZERO));
        for _ in 0..(MELT * 100.0) as usize + 10 {
            g.step(0.01, Conditions { gravity: SLOPE, ..Default::default() }, 0.0, ALL);
        }
        assert!(g.drops.iter().all(|d| d.kind == Kind::Water || d.kind == Kind::Trail));
    }
}
