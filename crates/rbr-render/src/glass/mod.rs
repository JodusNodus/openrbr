//! The in-car glass (rbr-recomp `notes/particles.md`, "In-car glass effects"; replaces 1.02's `NParticleSystem2D`): rain and snow
//! on the windscreen by weather and speed, bugs in the dry, and the wipers sweeping them off. Drawn on the car's
//! windscreen (`i_window_f`) in the in-car views; the wiper meshes (`i_wiper_l`, `i_wiper_r`) turn on their pivots.
//!
//! The exterior wipers (`e_wiper_l`, `e_wiper_r`) turn in step with them on the exterior windscreen (`e_window_f`), by
//! the same rule, in the outside views, and the glass's drops, film and dirt are drawn there too (ours: on `e_window_f`
//! at the points over `i_window_f`'s). The tyre spray spatters mud on the glass ([`crate::dirt`]). The wipers follow
//! their switch ([`model::Switch`]: the launcher's setting and the wiper key).
//!
//! The side windows (`i_window_lf`, `lb`, `rf`, `rb`; ours) each have their own glass by the same model: no wipers or
//! bugs, less of the rain and spray, and the airflow running back along them ([`model::Glass::side`]); outside they
//! are drawn on the exterior side windows.
//!
//! - [`model`]: the pure drop and wiper model, in the glass's chart.
//! - [`gpu`]: the drops as lenses over the frame.
//!
//! The glass's chart: the windscreen's mean plane, `x` across it to the driver's right and `y` up it, with a height
//! grid that puts a chart point back on the curved glass ([`Windscreen`]). Each wiper's pivot is the low end of its mesh
//! and its blade the far part ([`wiper_of`]). Ours: all of it but the bug data (`model`) and the spray amount; the
//! automatic switch runs the wipers by the weather and the mud ([`model::Switch::wiping`]).

pub mod gpu;
pub mod model;

use glam::{Mat4, Vec2, Vec3};
use rbr_formats::data::Data;
use rbr_formats::sgc;

use crate::car;
use crate::coords;
use crate::dirt::Spray;
use crate::particles::Precipitation;
use crate::playback::Playback;
use crate::renderer::Renderer;
use model::{Conditions, Glass, Kind, Switch, Wiper, Wiping};

/// The windscreen and wiper parts of the interior set, and of the exterior set.
const WINDSCREEN: &str = "i_window_f";
const WIPERS: [&str; 2] = ["i_wiper_r", "i_wiper_l"];
const EXTERIOR_WINDSCREEN: &str = "e_window_f";
const EXTERIOR_WIPERS: [&str; 2] = ["e_wiper_r", "e_wiper_l"];
/// The side windows (ours): the interior part, the exterior one, and the way out (`+x` the car's left).
const SIDES: [(&str, &str, f32); 4] = [
    ("i_window_lf", "e_window_lf", 1.0),
    ("i_window_lb", "e_window_lb", 1.0),
    ("i_window_rf", "e_window_rf", -1.0),
    ("i_window_rb", "e_window_rb", -1.0),
];
/// The physics step of one frame, seconds.
const STEP: f32 = rbr_recording::STEP_SECONDS;
/// Frames replayed before a jump in the drive (a screenshot, a seek): 8 s, so that the glass has its water.
pub const WARM_UP: usize = (8.0 / STEP) as usize;
/// The height grid's cells across and up.
const GRID: [usize; 2] = [64, 32];
/// Each wiper's sweep from its park, radians, in [`WIPERS`]' order (`E88Wiper::InitTable` 0x5f8a50: blade 1 from
/// 0.0942 to 1.4137 turns `*_wiper_r`, blade 0 from 0.0942 to 1.4765 turns `*_wiper_l`, `CLM_CharacterCar` LM
/// 0x1001cb20 (`Unk1cb20`) turns the parked node by `RotationY`).
const SWEEPS: [f32; 2] = [1.413_716_7 - 0.094_247_78, 1.476_548_6 - 0.094_247_78];
/// Ours: drops sit this far outside the glass's surface, metres.
const LIFT: f32 = 0.001;
/// Ours: a sliding drop is drawn this much longer along its run per m/s.
const STRETCH: f32 = 6.0;
/// Ours: mud seen from inside shows the side stuck to the glass, in its own shade: its colour times this (the outside
/// view sees the sunlit side).
const MUD_INSIDE: f32 = 0.2;
/// Ours: thrown snow lets the light through, so seen from inside it keeps more of its colour.
const SNOW_INSIDE: f32 = 0.75;

/// The windscreen's chart: its mean plane (`origin`, `axes` across, up and out) and the glass's height over it on a
/// grid (`None` off the glass).
#[derive(Debug, Clone, PartialEq)]
pub struct Windscreen {
    origin: Vec3,
    axes: [Vec3; 3],
    bounds: [Vec2; 2],
    heights: Vec<Option<f32>>,
    area: f32,
}

impl Windscreen {
    /// The chart of a windscreen's model-space triangles. The model's front is `-z`, its left `+x` and its up `+y`.
    pub fn new(triangles: &[[Vec3; 3]]) -> Option<Windscreen> {
        Windscreen::chart_of(triangles, Vec3::new(0.0, 1.0, -0.6), |out| Vec3::NEG_X - out * out.dot(Vec3::NEG_X))
    }

    /// The chart of a side window's triangles, facing out along `out_hint` (`+x` left, `-x` right): `x` across it,
    /// level, and `y` up it.
    pub fn side(triangles: &[[Vec3; 3]], out_hint: Vec3) -> Option<Windscreen> {
        Windscreen::chart_of(triangles, out_hint, |out| Vec3::Y.cross(out))
    }

    /// The chart of `triangles` facing out along `out_hint`, with `x` along `across` of the outward normal.
    fn chart_of(triangles: &[[Vec3; 3]], out_hint: Vec3, across: impl Fn(Vec3) -> Vec3) -> Option<Windscreen> {
        let (mut normal, mut area, mut origin) = (Vec3::ZERO, 0.0, Vec3::ZERO);
        for [a, b, c] in triangles {
            let n = (*b - *a).cross(*c - *a);
            let n = if n.dot(out_hint) < 0.0 { -n } else { n };
            normal += n;
            area += n.length() * 0.5;
            origin += (*a + *b + *c) / 3.0 * n.length() * 0.5;
        }
        if area <= 0.0 {
            return None;
        }
        let out = normal.normalize();
        let origin = origin / area;
        let across = across(out).normalize();
        let up = out.cross(across);
        let axes = [across, up, out];
        let chart = |p: Vec3| Vec2::new((p - origin).dot(across), (p - origin).dot(up));
        let (lo, hi) = triangles.iter().flatten().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), &p| {
            let q = chart(p);
            (lo.min(q), hi.max(q))
        });
        let mut heights = vec![None; GRID[0] * GRID[1]];
        for (i, h) in heights.iter_mut().enumerate() {
            let at = cell_centre([lo, hi], i);
            *h = triangles.iter().find_map(|t| {
                let [a, b, c] = t.map(chart);
                let w = barycentric(at, a, b, c)?;
                Some(t.iter().zip(w).map(|(p, w)| (*p - origin).dot(out) * w).sum())
            });
        }
        Some(Windscreen { origin, axes, bounds: [lo, hi], heights, area })
    }

    /// A model-space point in the chart.
    pub fn chart(&self, p: Vec3) -> Vec2 {
        Vec2::new((p - self.origin).dot(self.axes[0]), (p - self.origin).dot(self.axes[1]))
    }

    fn height(&self, at: Vec2) -> Option<f32> {
        let [lo, hi] = self.bounds;
        let f = (at - lo) / (hi - lo);
        if !(0.0..1.0).contains(&f.x) || !(0.0..1.0).contains(&f.y) {
            return None;
        }
        let (i, j) = ((f.x * GRID[0] as f32) as usize, (f.y * GRID[1] as f32) as usize);
        self.heights[j * GRID[0] + i]
    }

    /// Whether a chart point is on the glass.
    pub fn inside(&self, at: Vec2) -> bool {
        self.height(at).is_some()
    }

    /// The model-space point of the glass at the fractions `f` of the chart's bounds, across and up.
    pub fn at_fraction(&self, f: Vec2) -> Vec3 {
        let [lo, hi] = self.bounds;
        self.point(lo + (hi - lo) * f.clamp(Vec2::ZERO, Vec2::ONE), 0.0)
    }

    /// The chart's width over its height.
    pub fn ratio(&self) -> f32 {
        let [lo, hi] = self.bounds;
        (hi.x - lo.x) / (hi.y - lo.y).max(1e-6)
    }

    /// The model-space point of the glass at a chart point (on the mean plane off the glass), lifted by `lift`.
    pub fn point(&self, at: Vec2, lift: f32) -> Vec3 {
        let h = self.height(at).unwrap_or(0.0) + lift;
        self.origin + self.axes[0] * at.x + self.axes[1] * at.y + self.axes[2] * h
    }
}

/// The centre of grid cell `i` over `bounds`.
fn cell_centre([lo, hi]: [Vec2; 2], i: usize) -> Vec2 {
    let (x, y) = (i % GRID[0], i / GRID[0]);
    lo + (hi - lo) * Vec2::new((x as f32 + 0.5) / GRID[0] as f32, (y as f32 + 0.5) / GRID[1] as f32)
}

/// The barycentric weights of `p` in the triangle `a b c`, or `None` outside it.
fn barycentric(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<[f32; 3]> {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let d = v0.perp_dot(v1);
    if d.abs() < 1e-12 {
        return None;
    }
    let v = v2.perp_dot(v1) / d;
    let w = v0.perp_dot(v2) / d;
    let u = 1.0 - v - w;
    (u >= -1e-4 && v >= -1e-4 && w >= -1e-4).then_some([u, v, w])
}

/// A wiper from its mesh's model-space vertices: the pivot is the vertex group at the end of the parked wiper that
/// sits lower on the glass (the arm's hub below the glass), the blade the vertices beyond a third of the far reach. It
/// sweeps `sweep` radians up from its park.
pub fn wiper_of(glass: &Windscreen, vertices: &[Vec3], sweep: f32) -> Option<Wiper> {
    let points: Vec<Vec2> = vertices.iter().map(|&p| glass.chart(p)).collect();
    let (lo, hi) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
    let end = |x: f32| {
        let near: Vec<Vec2> = points.iter().copied().filter(|p| (p.x - x).abs() < 0.04).collect();
        near.iter().copied().sum::<Vec2>() / near.len().max(1) as f32
    };
    let (a, b) = (end(lo), end(hi));
    let pivot = if a.y <= b.y { a } else { b };
    let far = points.iter().map(|p| p.distance(pivot)).fold(0.0, f32::max);
    let blade: Vec<Vec2> = points.iter().copied().filter(|p| p.distance(pivot) > far / 3.0).collect();
    if blade.is_empty() {
        return None;
    }
    let near = blade.iter().map(|p| p.distance(pivot)).fold(f32::MAX, f32::min);
    let direction = blade.iter().copied().sum::<Vec2>() / blade.len() as f32 - pivot;
    let park = direction.y.atan2(direction.x);
    let turn = if direction.x >= 0.0 { 1.0 } else { -1.0 };
    Some(Wiper { pivot, reach: [near, far], park, turn, sweep })
}

/// A car's glass: the windscreen's chart and each wiper with its mesh's box centre (model space), for the interior set,
/// and the same for the exterior set when the car has its parts; and its side windows.
#[derive(Debug, Clone, PartialEq)]
pub struct CarGlass {
    pub windscreen: Windscreen,
    pub wipers: Vec<(Wiper, Vec3)>,
    pub exterior: Option<(Windscreen, Vec<(Wiper, Vec3)>)>,
    pub sides: Vec<Side>,
}

/// A side window (ours): its interior pane's chart, and its exterior pane's when the car has one.
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub inside: Windscreen,
    pub outside: Option<Windscreen>,
}

impl CarGlass {
    /// Car `name`'s windscreen and wipers from its `.sgc`, placed as `car::load_interior` and `car::load` place them.
    pub fn load(data: &Data, name: &str) -> Result<CarGlass, String> {
        let model = car::model(data, name).map_err(|e| e.to_string())?;
        let chained = sgc::chained_worlds(&model);
        let placed = |part: &str| {
            model.iter().find_map(|e| match e {
                sgc::Element::Mesh(m) if m.node.name == part => {
                    let world = chained.get(&m.node.id).copied().unwrap_or(m.node.world);
                    let matrix = coords::part_to_car(world, m.node.transform.words());
                    let vertices: Vec<Vec3> =
                        m.vertices.iter().map(|v| matrix.transform_point3(Vec3::from(v.position))).collect();
                    Some((vertices, m.triangles.clone()))
                }
                _ => None,
            })
        };
        let triangles = |part: &str| {
            let (vertices, triangles) = placed(part)?;
            let triangles: Vec<[Vec3; 3]> = triangles
                .iter()
                .filter_map(|t| {
                    let [a, b, c] = t.map(|i| vertices.get(i as usize).copied());
                    Some([a?, b?, c?])
                })
                .collect();
            Some(triangles)
        };
        let set = |glass: &str, wipers: [&str; 2]| {
            let triangles = triangles(glass).ok_or(format!("{name}: no {glass}"))?;
            let windscreen = Windscreen::new(&triangles).ok_or(format!("{name}: {glass} has no area"))?;
            let wipers = wipers
                .iter()
                .zip(SWEEPS)
                .filter_map(|(part, sweep)| {
                    let (vertices, _) = placed(part)?;
                    let (lo, hi) = vertices.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
                    Some((wiper_of(&windscreen, &vertices, sweep)?, (lo + hi) * 0.5))
                })
                .collect();
            Ok::<_, String>((windscreen, wipers))
        };
        let (windscreen, wipers) = set(WINDSCREEN, WIPERS)?;
        let exterior = set(EXTERIOR_WINDSCREEN, EXTERIOR_WIPERS).map_err(|e| eprintln!("glass: {e}")).ok();
        let pane = |part: &str, out: f32| Windscreen::side(&triangles(part)?, Vec3::new(out, 0.0, 0.0));
        let sides = SIDES
            .iter()
            .filter_map(|&(inside, outside, out)| {
                Some(Side { inside: pane(inside, out)?, outside: pane(outside, out) })
            })
            .collect();
        Ok(CarGlass { windscreen, wipers, exterior, sides })
    }
}

/// The glass as the viewer draws it: the car's glass, the model and the drive's weather.
pub struct Drawn {
    car: CarGlass,
    glass: Glass,
    precipitation: Option<Precipitation>,
    /// The wiper switch, and the schedule it gives (changed only while the blades are parked).
    pub switch: Switch,
    wiping: Wiping,
    /// The playback's interior parts of each wiper, and its exterior parts of each exterior wiper.
    parts: Vec<Vec<usize>>,
    exterior_parts: Vec<Vec<usize>>,
    /// The side windows' glass, in [`CarGlass::sides`]' order.
    sides: Vec<Glass>,
    /// The last frame followed, and its step.
    last: Option<(usize, u32)>,
}

impl Drawn {
    /// The car's windscreen chart.
    pub fn windscreen(&self) -> &Windscreen {
        &self.car.windscreen
    }

    /// The glass of car `name` in `precipitation`, with the wiper meshes found among `playback`'s interior parts.
    /// `None` (reported on stderr) when the car has no windscreen part.
    /// The wipers start at `switch`.
    pub fn load(
        data: &Data,
        name: &str,
        precipitation: Option<Precipitation>,
        playback: &Playback,
        switch: Switch,
    ) -> Option<Drawn> {
        let car = CarGlass::load(data, name).map_err(|e| eprintln!("glass: {e}")).ok()?;
        let [lo, hi] = car.windscreen.bounds;
        let glass = Glass::new([lo, hi], car.windscreen.area, car.wipers.iter().map(|w| w.0).collect(), 0x6c_a55);
        let sides = (car.sides.iter().zip(1..))
            .map(|(side, k)| {
                let w = &side.inside;
                let back = Vec2::new(Vec3::Z.dot(w.axes[0]), Vec3::Z.dot(w.axes[1]));
                Glass::side(w.bounds, w.area, back, 0x6c_a55 + k)
            })
            .collect();
        let parts: Vec<Vec<usize>> = car.wipers.iter().map(|(_, centre)| playback.parts_at(*centre, true)).collect();
        let exterior_parts: Vec<Vec<usize>> = car
            .exterior
            .iter()
            .flat_map(|(_, wipers)| wipers)
            .map(|(_, centre)| playback.parts_at(*centre, false))
            .collect();
        let (rain, snow) = amounts(precipitation);
        let wiping = switch.wiping(rain, snow, 0);
        eprintln!(
            "glass: windscreen {:.2} m², {} side windows, {} wipers ({} meshes, {} outside), {:?}, wipers {} ({:?})",
            car.windscreen.area,
            car.sides.len(),
            car.wipers.len(),
            parts.iter().map(Vec::len).sum::<usize>(),
            exterior_parts.iter().map(Vec::len).sum::<usize>(),
            precipitation,
            switch.name(),
            wiping
        );
        Some(Drawn { car, glass, precipitation, switch, wiping, parts, exterior_parts, sides, last: None })
    }

    /// Follows the drive to its current frame (each frame one physics step, with its tyre `spray`), turns the wiper
    /// meshes in `playback` (call before [`Playback::apply`]) and gives `renderer` the drops: on the windscreen inside
    /// for the `interior` views, else on the outside one (ours: the same drops, mud and wiped arcs seen from outside),
    /// or none for a car without one. A frame past the spray's known ones takes the last known frame's.
    pub fn draw(&mut self, renderer: &mut Renderer, playback: &mut Playback, interior: bool, spray: Option<&Spray>) {
        let samples = playback.samples();
        let index = playback.index();
        let first = match self.last {
            Some((last, step)) if last <= index && index - last <= WARM_UP && samples[last].step == step => last + 1,
            _ => {
                self.glass.clear();
                let first = index.saturating_sub(WARM_UP);
                let before = spray.map_or_else(Default::default, |s| s.before(first));
                let windscreen = &self.car.windscreen;
                self.glass.spatter_all(before, |at| windscreen.inside(at));
                for (side, glass) in self.car.sides.iter().zip(&mut self.sides) {
                    glass.clear();
                    glass.spatter_all(before, |at| side.inside.inside(at));
                }
                first
            }
        };
        let (rain, snow) = amounts(self.precipitation);
        let windscreen = &self.car.windscreen;
        // The wipers run on the game's clock: the drive's steps (NGP's 1/720 s in a live NGP run).
        let step = playback.step_clock().step;
        for (i, sample) in samples.iter().enumerate().take(index + 1).skip(first) {
            let to_model = playback.model_to_physics(sample).inverse();
            let gravity = to_model.transform_vector3(Vec3::new(0.0, 0.0, -9.81));
            let conditions = Conditions {
                rain,
                snow,
                speed: Vec3::from(sample.car.velocity).length(),
                gravity: Vec2::new(gravity.dot(windscreen.axes[0]), gravity.dot(windscreen.axes[1])),
                spray: spray.map_or_else(Default::default, |s| s.thrown(i.min(s.len().saturating_sub(1)))),
            };
            let time = sample.step as f32 * step;
            // The switch takes effect at the park, so the blades never jump.
            if self.wiping.phase(time) == 0.0 {
                self.wiping = self.switch.wiping(rain, snow, self.glass.mud());
            }
            self.glass.step(STEP, conditions, self.wiping.phase(time), |at| windscreen.inside(at));
            for (side, glass) in self.car.sides.iter().zip(&mut self.sides) {
                let [across, up, _] = side.inside.axes;
                let gravity = Vec2::new(gravity.dot(across), gravity.dot(up));
                glass.step(STEP, Conditions { gravity, ..conditions }, 0.0, |at| side.inside.inside(at));
            }
        }
        self.last = Some((index, samples[index].step));
        let phase = self.glass.phase;
        let turn =
            |windscreen: &Windscreen, wipers: &[(Wiper, Vec3)], parts: &[Vec<usize>], playback: &mut Playback| {
                for ((wiper, _), parts) in wipers.iter().zip(parts) {
                    let pivot = windscreen.point(wiper.pivot, 0.0);
                    let motion = Mat4::from_translation(pivot)
                        * Mat4::from_axis_angle(windscreen.axes[2], wiper.turn * wiper.sweep * phase)
                        * Mat4::from_translation(-pivot);
                    for &part in parts {
                        playback.set_motion(part, motion);
                    }
                }
            };
        turn(windscreen, &self.car.wipers, &self.parts, playback);
        if let Some((outside, wipers)) = &self.car.exterior {
            turn(outside, wipers, &self.exterior_parts, playback);
        }
        let drops = match (interior, &self.car.exterior) {
            (true, _) => self.drops(playback, None),
            (false, Some((outside, _))) => self.drops(playback, Some(outside)),
            (false, None) => Vec::new(),
        };
        renderer.set_glass(&drops);
    }

    /// The drops at the current frame, stage space, on the windscreen inside, or on `outside` (the outside one) at the
    /// points over the inside one's.
    fn drops(&self, playback: &Playback, outside: Option<&Windscreen>) -> Vec<gpu::Drop> {
        let to_stage = coords::physics_to_stage() * playback.model_to_physics(playback.sample());
        let w = &self.car.windscreen;
        let place = |at: Vec2, lift: f32| match outside {
            Some(o) => o.point(o.chart(w.point(at, 0.0)), lift),
            None => w.point(at, lift),
        };
        // The film's streaks along the airflow, longer with speed: up the windscreen, back along the side windows.
        let streak = 1.0 + (Vec3::from(playback.sample().car.velocity).length() / FILM_STREAK_SPEED).min(2.0);
        let [across, up, _] = outside.unwrap_or(w).axes.map(|a| to_stage.transform_vector3(a));
        let mut drops = pane_drops(&self.glass, w, outside, to_stage, streak);
        for (side, glass) in self.car.sides.iter().zip(&self.sides) {
            // Outside, on the exterior pane over it, when the car has one.
            let on = match outside {
                Some(_) => side.outside.as_ref().map(Some),
                None => Some(None),
            };
            if let Some(on) = on {
                drops.extend(pane_drops(glass, &side.inside, on, to_stage, -streak));
            }
        }
        // The snow each blade carries, along its edge at its angle.
        let phase = self.glass.phase;
        for (wiper, &load) in self.glass.wipers.iter().zip(&self.glass.blade_snow) {
            let angle = wiper.park + wiper.turn * wiper.sweep * phase;
            let along = Vec2::new(angle.cos(), angle.sin());
            let ahead = Vec2::new(-along.y, along.x) * wiper.turn;
            for k in 0..load as usize {
                let (a, b) = (hash(k as u32 * 2 + 1), hash(k as u32 * 2 + 2));
                let r = wiper.reach[0] + (wiper.reach[1] - wiper.reach[0]) * a;
                let at = wiper.pivot + along * r + ahead * (b - 0.3) * BLADE_DEPTH;
                let radius = BLADE_FLAKE[0] + (BLADE_FLAKE[1] - BLADE_FLAKE[0]) * b;
                drops.push(gpu::Drop {
                    centre: to_stage.transform_point3(place(at, BLADE_LIFT)).to_array(),
                    kind: 1.0,
                    axis_u: (across * radius).to_array(),
                    seed: a,
                    axis_v: (up * radius).to_array(),
                    alpha: 1.0,
                    color: [0.95, 0.96, 1.0, 1.0],
                });
            }
        }
        drops
    }
}

/// The drops, film and mud of one pane's `glass` at the current frame, stage space (`to_stage` from the car's model),
/// on its chart `w` inside, or on `outside` at the points over `w`'s. `streak` is the film's streak length, up the
/// pane, or along it when negative.
fn pane_drops(
    glass: &Glass,
    w: &Windscreen,
    outside: Option<&Windscreen>,
    to_stage: Mat4,
    streak: f32,
) -> Vec<gpu::Drop> {
    let place = |at: Vec2, lift: f32| match outside {
        Some(o) => o.point(o.chart(w.point(at, 0.0)), lift),
        None => w.point(at, lift),
    };
    let [across, up, _] = outside.unwrap_or(w).axes.map(|a| to_stage.transform_vector3(a));
    // The wet film first, under the drops: a patch per cell, its corners' film (the mean of the cells around each
    // corner, none off the glass) in `color`, so that the shader blends them into one smooth sheet that fades out
    // at the glass's edge.
    let [columns, rows] = model::FILM_CELLS;
    let (film, size) = (&glass.film, glass.film_cell(0).1);
    let low_corner = glass.film_cell(0).0 - size * 0.5;
    let corner = |column: usize, row: usize| {
        if !w.inside(low_corner + size * Vec2::new(column as f32, row as f32)) {
            return 0.0;
        }
        let (mut sum, mut count) = (0.0, 0.0);
        for r in row.saturating_sub(1)..(row + 1).min(rows) {
            for c in column.saturating_sub(1)..(column + 1).min(columns) {
                sum += film[r * columns + c];
                count += 1.0;
            }
        }
        sum / count
    };
    let corners = [Vec2::new(-0.5, -0.5), Vec2::new(0.5, -0.5), Vec2::new(-0.5, 0.5), Vec2::new(0.5, 0.5)];
    let mut drops = Vec::new();
    for index in 0..film.len() {
        let (column, row) = (index % columns, index / columns);
        let color =
            [corner(column, row), corner(column + 1, row), corner(column, row + 1), corner(column + 1, row + 1)];
        if color.iter().all(|&f| f <= FILM_SHOWN) {
            continue;
        }
        // The cell's corners on the curved glass, so that neighbouring cells meet without seams.
        let at = glass.film_cell(index).0;
        let [low, right, top, far] = corners.map(|c| to_stage.transform_point3(place(at + size * c, LIFT * 0.5)));
        drops.push(gpu::Drop {
            centre: ((low + right + top + far) * 0.25).to_array(),
            kind: 4.0,
            axis_u: ((right + far - low - top) * 0.25).to_array(),
            seed: streak,
            axis_v: ((top + far - low - right) * 0.25).to_array(),
            // The cell, for its beads' places (`glass.wgsl`).
            alpha: index as f32,
            color,
        });
    }
    drops.extend(glass.drops.iter().map(|d| {
        let centre = to_stage.transform_point3(place(d.at, LIFT));
        let run = d.velocity.length();
        let (u, v) = if run > 0.0 {
            let along = d.velocity / run;
            let side = Vec2::new(-along.y, along.x);
            let to = |c: Vec2| across * c.x + up * c.y;
            (to(side) * d.radius, to(along) * d.radius * (1.0 + run * STRETCH))
        } else {
            (across * d.radius, up * d.radius)
        };
        let (kind, alpha, color) = match d.kind {
            Kind::Water => (0.0, 1.0, [1.0; 4]),
            Kind::Trail => (0.0, d.left.min(1.0), [1.0; 4]),
            Kind::Mud => {
                let shade = match (outside, model::is_snow(d.left)) {
                    (Some(_), _) => 1.0,
                    (None, false) => MUD_INSIDE,
                    (None, true) => SNOW_INSIDE,
                };
                let [r, g, b] = model::speck_color(d.left).map(|c| c * shade);
                (3.0, 1.0, [r, g, b, 1.0])
            }
            Kind::Snow => (1.0, 0.5 + 0.5 * d.left, [0.95, 0.96, 1.0, 1.0]),
            Kind::Bug => {
                let fade = ((model::BUG_LIFE + model::BUG_FADE - d.age) / model::BUG_FADE).clamp(0.0, 1.0);
                let [r, g, b] = model::BUG_COLORS[d.left as usize % 4];
                (2.0, fade, [r, g, b, 1.0])
            }
        };
        gpu::Drop {
            centre: centre.to_array(),
            kind,
            axis_u: u.to_array(),
            seed: d.seed,
            axis_v: v.to_array(),
            alpha,
            color,
        }
    }));
    drops
}

/// The film below this is not drawn.
const FILM_SHOWN: f32 = 0.02;
/// Ours: the film's streaks grow by their length per this many m/s, up to three times it.
const FILM_STREAK_SPEED: f32 = 12.0;
/// Snow on a blade (ours): the flakes' radius, metres, how far they spread across the blade, and their height over
/// the glass (on top of the blade).
const BLADE_FLAKE: [f32; 2] = [0.004, 0.008];
const BLADE_DEPTH: f32 = 0.02;
const BLADE_LIFT: f32 = 0.012;

/// A number in [0, 1) from `k` (a fixed scatter for the blade's flakes).
fn hash(k: u32) -> f32 {
    let mut x = k.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Rain and snow intensity of a precipitation.
fn amounts(p: Option<Precipitation>) -> (f32, f32) {
    match p {
        Some(p) if p.snow => (0.0, p.intensity),
        Some(p) => (p.intensity, 0.0),
        None => (0.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat windscreen 1.4 m wide and 0.8 m up its slope, leaning back like COROLLA's, as two triangles.
    fn flat() -> Windscreen {
        let up = Vec3::new(0.0, 0.53, 0.85) * 0.8;
        let (a, b) = (Vec3::new(0.7, 0.72, -2.13), Vec3::new(-0.7, 0.72, -2.13));
        Windscreen::new(&[[a, b, b + up], [a, b + up, a + up]]).unwrap()
    }

    #[test]
    fn the_chart_runs_right_and_up_the_glass_and_maps_back() {
        let w = flat();
        assert!((w.area - 1.12).abs() < 0.01);
        assert!(w.axes[0].x < -0.99, "across is to the driver's right, -x");
        assert!(w.axes[1].y > 0.5 && w.axes[1].z > 0.8, "up the slope, back toward the driver");
        assert!(w.axes[2].y > 0.8 && w.axes[2].z < -0.5, "out, up and forward");
        let p = Vec3::new(0.3, 0.72, -2.13) + Vec3::new(0.0, 0.53, 0.85) * 0.2;
        let at = w.chart(p);
        assert!(w.inside(at));
        assert!(w.point(at, 0.0).distance(p) < 1e-4);
        assert!(!w.inside(Vec2::new(0.0, 5.0)));
    }

    #[test]
    fn a_side_window_charts_level_across_and_up() {
        // A left front window: in the plane x = 0.7, leaning in 20 degrees at the top.
        let (lo, up) = (Vec3::new(0.7, 0.8, -1.5), Vec3::new(-0.17, 0.47, 0.0));
        let (a, b) = (lo, lo + Vec3::new(0.0, 0.0, 0.9));
        let w = Windscreen::side(&[[a, b, b + up], [a, b + up, a + up]], Vec3::X).unwrap();
        assert!(w.axes[0].z < -0.99, "across is forward, -z");
        assert!(w.axes[1].y > 0.9, "up the pane");
        assert!(w.axes[2].x > 0.9, "out, to the left");
    }

    #[test]
    fn a_wiper_parked_along_the_bottom_pivots_at_its_low_end() {
        let w = flat();
        // A hub 2 cm lower at the left (model +x), an arm, and a blade out to the right.
        let base = |x: f32, drop: f32| Vec3::new(x, 0.74 - drop, -2.12 - drop);
        let mut vertices = vec![base(0.15, 0.02), base(0.14, 0.02), base(0.16, 0.02)];
        vertices.extend((0..20).map(|i| base(-0.2 - i as f32 * 0.025, 0.0)));
        let wiper = wiper_of(&w, &vertices, SWEEPS[1]).unwrap();
        assert!((wiper.pivot.x + 0.15).abs() < 0.01, "{:?}", wiper.pivot);
        assert!(wiper.park.abs() < 0.1 && wiper.turn == 1.0);
        assert!(wiper.reach[0] > 0.3 && wiper.reach[1] > 0.8);
    }
}
