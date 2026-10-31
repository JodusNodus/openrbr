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

use std::collections::BTreeMap;

use glam::{Mat4, Vec2, Vec3};
use rbr_formats::data::Data;
use rbr_formats::sgc;

use crate::car;
use crate::coords;
use crate::damage::{self, MeshDamage};
use crate::dirt::Spray;
use crate::parallel::in_parallel;
use crate::particles::Precipitation;
use crate::playback::Playback;
use crate::renderer::Renderer;
use crate::stage::{Bounds, Draw, DrawState, Mesh, StageTexture, TextureRef, Vertex};
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
/// view sees the sunlit side), and with the cabin's light the outside's light through its thin rim (`glass.wgsl`,
/// kind 5).
const MUD_INSIDE: f32 = 0.2;
/// Ours: thrown snow lets the light through, so seen from inside it keeps more of its colour.
const SNOW_INSIDE: f32 = 0.75;

/// The windscreen's chart: its mean plane (`origin`, `axes` across, up and out), the glass's height over it on a
/// grid (`None` off the glass), and its triangles (the chart's x and y, the height in z) and outline in the chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Windscreen {
    origin: Vec3,
    axes: [Vec3; 3],
    bounds: [Vec2; 2],
    heights: Vec<Option<f32>>,
    area: f32,
    faces: Vec<[Vec3; 3]>,
    outline: Vec<[Vec2; 2]>,
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
        let faces: Vec<[Vec3; 3]> =
            triangles.iter().map(|t| t.map(|p| chart(p).extend((p - origin).dot(out)))).collect();
        let on_face = |p: Vec2, t: &[Vec3; 3]| barycentric(p, t[0].truncate(), t[1].truncate(), t[2].truncate());
        // The faces that a point in each cell of the height grid may lie on, in order: those whose box, a little wider
        // than the barycentric test's slack, reaches into it.
        let cell = |p: Vec2| {
            let f = ((p - lo) / (hi - lo) * Vec2::new(GRID[0] as f32, GRID[1] as f32)).floor();
            let (x, y) = (f.x.clamp(0.0, (GRID[0] - 1) as f32) as usize, f.y.clamp(0.0, (GRID[1] - 1) as f32) as usize);
            y * GRID[0] + x
        };
        let mut near: Vec<Vec<usize>> = vec![Vec::new(); GRID[0] * GRID[1]];
        for (k, t) in faces.iter().enumerate() {
            let (low, high) =
                t.iter().fold((Vec2::MAX, Vec2::MIN), |(l, h), p| (l.min(p.truncate()), h.max(p.truncate())));
            let (first, last) = (cell(low - Vec2::splat(NEAR_SLACK)), cell(high + Vec2::splat(NEAR_SLACK)));
            for y in first / GRID[0]..=last / GRID[0] {
                (first % GRID[0]..=last % GRID[0]).for_each(|x| near[y * GRID[0] + x].push(k));
            }
        }
        let near_faces = |p: Vec2| near[cell(p)].iter().map(|&k| &faces[k]);
        let heights = (0..GRID[0] * GRID[1])
            .map(|i| {
                let at = cell_centre([lo, hi], i);
                near_faces(at).find_map(|t| {
                    let w = on_face(at, t)?;
                    Some(t.iter().zip(w).map(|(p, w)| p.z * w).sum())
                })
            })
            .collect();
        let on = |p: Vec2| near_faces(p).any(|t| on_face(p, t).is_some());
        // An edge is on the outline when the glass does not go on past it.
        let outline = faces
            .iter()
            .map(|t| t.map(Vec3::truncate))
            .flat_map(|[a, b, c]| [[a, b, c], [b, c, a], [c, a, b]])
            .filter_map(|[a, b, c]| {
                let across = (b - a).perp().normalize_or_zero();
                let away = if across.dot(c - a) > 0.0 { -across } else { across };
                (across != Vec2::ZERO && !on((a + b) * 0.5 + away * OUTLINE_PROBE)).then_some([a, b])
            })
            .collect();
        Some(Windscreen { origin, axes, bounds: [lo, hi], heights, area, faces, outline })
    }

    /// How far chart point `at` lies inside the glass's outline, metres; negative off the glass.
    pub fn edge(&self, at: Vec2) -> f32 {
        let distance = self.outline.iter().map(|&[a, b]| at.distance(closest(at, a, b))).fold(f32::MAX, f32::min);
        match self.faces.iter().any(|t| barycentric(at, t[0].truncate(), t[1].truncate(), t[2].truncate()).is_some()) {
            true => distance,
            false => -distance,
        }
    }

    /// The model-space point of the glass at chart point `at`, on its triangles (the grid's heights step from cell to
    /// cell); off the glass at the height of the nearest point on it, so that a cell over the outline lies along it.
    fn point_near(&self, at: Vec2) -> Vec3 {
        // Each triangle's nearest point (`at` itself on it), how far, and its height there.
        let nearest = |t: &[Vec3; 3]| {
            let [a, b, c] = t.map(Vec3::truncate);
            let near = match barycentric(at, a, b, c) {
                Some(_) => at,
                None => [(a, b), (b, c), (c, a)]
                    .map(|(p, q)| closest(at, p, q))
                    .into_iter()
                    .min_by(|p, q| p.distance_squared(at).total_cmp(&q.distance_squared(at)))?,
            };
            let w = weights(near, a, b, c)?.map(|w| w.clamp(0.0, 1.0));
            Some((near.distance_squared(at), t[0].z * w[0] + t[1].z * w[1] + t[2].z * w[2]))
        };
        let h = self.faces.iter().filter_map(nearest).min_by(|a, b| a.0.total_cmp(&b.0)).map_or(0.0, |n| n.1);
        self.origin + self.axes[0] * at.x + self.axes[1] * at.y + self.axes[2] * h
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

    /// The chart's mean plane over its bounds, by model space to stage space `to_stage`, facing the cabin: the
    /// windscreen as a mirror (`renderer::mirrors`).
    pub fn mirror(&self, to_stage: Mat4) -> crate::mirrors::Placed {
        let [lo, hi] = self.bounds;
        let corner = |x: f32, y: f32| to_stage.transform_point3(self.origin + self.axes[0] * x + self.axes[1] * y);
        crate::mirrors::Placed {
            corners: [corner(lo.x, lo.y), corner(hi.x, lo.y), corner(hi.x, hi.y), corner(lo.x, hi.y)],
            normal: -to_stage.transform_vector3(self.axes[2]).normalize(),
            side: false,
        }
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

/// How far past an edge [`Windscreen::new`] looks for more glass, metres.
const OUTLINE_PROBE: f32 = 0.001;
/// How much wider than a face's box [`Windscreen::new`] looks for points on it, metres: far more than [`barycentric`]'s
/// slack lets a point lie off a face (1e-4 of its size).
const NEAR_SLACK: f32 = 0.01;

/// The point of the segment `a b` nearest `p`.
fn closest(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    a + ab * ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0)
}

/// The barycentric weights of `p` in the triangle `a b c`, or `None` outside it.
fn barycentric(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<[f32; 3]> {
    weights(p, a, b, c).filter(|w| w.iter().all(|&w| w >= -1e-4))
}

/// The barycentric weights of `p` over the triangle `a b c`, inside it or not; `None` for a degenerate one.
fn weights(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> Option<[f32; 3]> {
    let (v0, v1, v2) = (b - a, c - a, p - a);
    let d = v0.perp_dot(v1);
    if d.abs() < 1e-12 {
        return None;
    }
    let v = v2.perp_dot(v1) / d;
    let w = v0.perp_dot(v2) / d;
    Some([1.0 - v - w, v, w])
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

/// A side window (ours): its interior part, its interior pane's chart, and its exterior pane's when the car has one.
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub part: &'static str,
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
                Some(Side { part: inside, inside: pane(inside, out)?, outside: pane(outside, out) })
            })
            .collect();
        Ok(CarGlass { windscreen, wipers, exterior, sides })
    }
}

/// What the interior set's window faces let through (ours): `1 - alpha` of their texture's texel under a point of a
/// pane, 1 off them. The sun strips and visors printed on the glass keep the sun and the sky off the dust behind them.
#[derive(Debug, Clone, Default)]
pub struct Tints {
    /// The faces: their window slot ([`damage::window_slot`]), model-space corners, uvs and image in `images`.
    faces: Vec<(usize, [Vec3; 3], [Vec2; 3], usize)>,
    /// The textures as RGBA rows, their width and height.
    images: Vec<(usize, usize, Vec<u8>)>,
}

/// The largest texture level [`Tints`] reads.
const TINT_LEVEL: u32 = 512;

impl Tints {
    /// The blended faces of the window parts among `meshes` (the interior set, model space), `damage` per mesh.
    pub fn measure(meshes: &[Mesh], damage: &[MeshDamage], textures: &BTreeMap<TextureRef, StageTexture>) -> Tints {
        let mut tints = Tints::default();
        let mut keys: Vec<TextureRef> = Vec::new();
        for (mesh, slot) in meshes.iter().zip(damage).filter_map(|(mesh, d)| Some((mesh, d.window?))) {
            for draw in mesh.draws.iter().filter(|draw| draw.state.blend) {
                let Some(key) = draw.state.textures[0] else { continue };
                let image = match keys.iter().position(|k| *k == key) {
                    Some(image) => image,
                    None => {
                        let image = textures.get(&key).and_then(|t| t.image.as_ref());
                        let Some(rgba) = image.and_then(|image| car::cabin::rgba_at_most(image, TINT_LEVEL)) else {
                            continue;
                        };
                        keys.push(key);
                        tints.images.push(rgba);
                        keys.len() - 1
                    }
                };
                for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                    let v = [t[0], t[1], t[2]].map(|i| &mesh.vertices[i as usize]);
                    tints.faces.push((slot, v.map(|v| Vec3::from(v.position)), v.map(|v| Vec2::from(v.uv0)), image));
                }
            }
        }
        tints
    }

    /// What window part `part`'s faces let through at chart point `at` of its chart `w`.
    fn through(&self, part: &str, w: &Windscreen, at: Vec2) -> f32 {
        let Some(slot) = damage::window_slot(part) else { return 1.0 };
        self.faces
            .iter()
            .filter(|face| face.0 == slot)
            .find_map(|(_, corners, uvs, image)| {
                let [a, b, c] = corners.map(|p| w.chart(p));
                let weights = barycentric(at, a, b, c)?;
                let uv = uvs[0] * weights[0] + uvs[1] * weights[1] + uvs[2] * weights[2];
                let (width, height, rgba) = &self.images[*image];
                let x = ((uv.x.rem_euclid(1.0) * *width as f32) as usize).min(width - 1);
                let y = ((uv.y.rem_euclid(1.0) * *height as f32) as usize).min(height - 1);
                Some(1.0 - rgba[(y * width + x) * 4 + 3] as f32 / 255.0)
            })
            .unwrap_or(1.0)
    }

    /// What part `part`'s glass lets through over `glass`'s film, [`gpu::PANE_TINTS`] steps a cell, row by row.
    fn film_tints(&self, part: &str, glass: &Glass, w: &Windscreen) -> Vec<f32> {
        let [columns, rows] = model::FILM_CELLS.map(|n| n * gpu::PANE_TINTS);
        let (lo, hi) = (glass.film_corner(0, 0), glass.film_corner(model::FILM_CELLS[0], model::FILM_CELLS[1]));
        let points: Vec<Vec2> = (0..=rows)
            .flat_map(|row| (0..=columns).map(move |column| (column, row)))
            .map(|(column, row)| lo + (hi - lo) * Vec2::new(column as f32 / columns as f32, row as f32 / rows as f32))
            .collect();
        // Over the load's threads: one after another the five panes' took 0.22 s (the 208 Rally4's 4200 faces).
        in_parallel(&points, |&at| self.through(part, w, at))
    }
}

/// The film corners of `glass` on its chart `w`, row by row.
fn film_corners(glass: &Glass, w: &Windscreen) -> Vec<Corner> {
    let [columns, rows] = model::FILM_CELLS;
    let points: Vec<Vec2> =
        (0..=rows).flat_map(|row| (0..=columns).map(move |column| glass.film_corner(column, row))).collect();
    in_parallel(&points, |&at| Corner { point: w.point_near(at), edge: w.edge(at) })
}

/// A film corner of a pane seen from inside (`pane_cells`): its model-space point and how far inside the glass's
/// outline it lies ([`Windscreen::edge`]).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Corner {
    point: Vec3,
    edge: f32,
}

/// How far the bulkhead ([`bulkhead`]) hangs below the windscreen's foot, metres.
const BULKHEAD_DEPTH: f32 = 0.6;
/// The bulkhead's vertex colour over the face it copies: the dark of the cabin behind the dash.
const BULKHEAD_COLOUR: [u8; 4] = [10, 10, 10, 255];

/// The bulkhead under the windscreen (ours) for the interior set `meshes` (model space, `damage` per mesh): a dark
/// strip hanging from the glass's foot, drawn as the set's first opaque lit face, in the windscreen's reflection only
/// ([`Renderer::set_reflected_only`]). The sets leave gaps between the dash and the glass that the driver never sees
/// past; the reflection does, and showed the road under the car and the sky through them (Clio Rally3, Wythop). On
/// an upright windscreen it hangs in front of the footwell (Escort Mk II). `None` without a windscreen part.
pub fn bulkhead(meshes: &[Mesh], damage: &[MeshDamage]) -> Option<Mesh> {
    let front = damage::window_slot(WINDSCREEN)?;
    let triangles: Vec<[Vec3; 3]> = (meshes.iter().zip(damage))
        .filter(|(_, d)| d.window == Some(front))
        .flat_map(|(mesh, _)| {
            mesh.indices
                .chunks_exact(3)
                .map(|t| [t[0], t[1], t[2]].map(|i| Vec3::from(mesh.vertices[i as usize].position)))
        })
        .collect();
    let w = Windscreen::new(&triangles)?;
    let lit = |draw: &&Draw| draw.state.shade.is_some_and(|shade| !shade.glass) && !draw.state.blend;
    let (source, draw) = meshes.iter().find_map(|mesh| Some((mesh, mesh.draws.iter().find(lit)?)))?;
    // The glass's foot: a cell below each column's lowest cell on the glass, so that it reaches past where the glass
    // ends between the grid's cells.
    let [lo, hi] = w.bounds;
    let cell = (hi - lo) / Vec2::new(GRID[0] as f32, GRID[1] as f32);
    let foot: Vec<Vec3> = (0..GRID[0])
        .filter_map(|x| {
            let i = (0..GRID[1]).map(|y| y * GRID[0] + x).find(|&i| w.heights[i].is_some())?;
            let at = cell_centre([lo, hi], i) - Vec2::new(0.0, cell.y * 1.5);
            Some(w.origin + w.axes[0] * at.x + w.axes[1] * at.y + w.axes[2] * w.heights[i]?)
        })
        .collect();
    if foot.len() < 2 {
        return None;
    }
    // Facing the cabin (the model's back is `+z`), both sides drawn; one of the copied face's vertices, which with the
    // cabin light's bake (`car::cabin::bake`, run before) sees no sky, as the dark behind the dash.
    let mut like = source.vertices[source.indices[draw.indices.start] as usize];
    if like.specular[1] >= car::cabin::INTERIOR {
        like.specular[2] = 1.0;
    }
    let vertex = |p: Vec3| Vertex { position: p.to_array(), color: BULKHEAD_COLOUR, normal: [0.0, 0.0, 1.0], ..like };
    let vertices: Vec<Vertex> =
        foot.iter().flat_map(|&p| [vertex(p), vertex(p + Vec3::NEG_Y * BULKHEAD_DEPTH)]).collect();
    let indices: Vec<u32> = (0..foot.len() as u32 - 1)
        .flat_map(|k| [2 * k, 2 * k + 1, 2 * k + 2, 2 * k + 2, 2 * k + 1, 2 * k + 3])
        .collect();
    let (low, high) =
        vertices.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), v| (l.min(v.position.into()), h.max(v.position.into())));
    let draw = Draw { indices: 0..indices.len(), state: DrawState { cull_back: false, ..draw.state }, ..draw.clone() };
    Some(Mesh {
        bounds: Some(Bounds { center: (low + high) * 0.5, half_extent: (high - low) * 0.5 }),
        vertices,
        indices,
        draws: vec![draw],
        ..source.clone()
    })
}

/// The glass as the viewer draws it: the car's glass, the model and the drive's weather.
pub struct Drawn {
    car: CarGlass,
    glass: Glass,
    /// The windscreen's, then each side window's film corners and what its glass lets through
    /// ([`film_corners`], [`Tints::film_tints`]).
    corners: Vec<(Vec<Corner>, Vec<f32>)>,
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

    /// The glass of car `name` in `precipitation`, with the wiper meshes found among `playback`'s interior parts and
    /// what its windows' faces let through, `tints`. `None` (reported on stderr) when the car has no windscreen part.
    /// The wipers start at `switch`.
    pub fn load(
        data: &Data,
        name: &str,
        precipitation: Option<Precipitation>,
        playback: &Playback,
        switch: Switch,
        tints: &Tints,
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
            .collect::<Vec<_>>();
        let pane =
            |part: &str, glass: &Glass, w: &Windscreen| (film_corners(glass, w), tints.film_tints(part, glass, w));
        let corners = std::iter::once(pane(WINDSCREEN, &glass, &car.windscreen))
            .chain(car.sides.iter().zip(&sides).map(|(side, g)| pane(side.part, g, &side.inside)))
            .collect();
        let parts: Vec<Vec<usize>> = car.wipers.iter().map(|(_, centre)| playback.parts_at(*centre, true)).collect();
        let exterior_parts: Vec<Vec<usize>> = car
            .exterior
            .iter()
            .flat_map(|(_, wipers)| wipers)
            .map(|(_, centre)| playback.parts_at(*centre, false))
            .collect();
        let wiping = switch.wiping(falling(precipitation), amounts(precipitation).1, 0);
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
        Some(Drawn { car, glass, corners, precipitation, switch, wiping, parts, exterior_parts, sides, last: None })
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
        // The wipers read the raw fall, not the shower's share: drizzle wipes now and then, heavier rain always.
        let falling = falling(self.precipitation);
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
                self.wiping = self.switch.wiping(falling, snow, self.glass.mud());
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
        let lit = renderer.enhance().cabin;
        let effects = renderer.enhance().windscreen;
        let drops = match (interior, &self.car.exterior) {
            (true, _) => self.drops(playback, None, lit, effects),
            (false, Some((outside, _))) => self.drops(playback, Some(outside), lit, effects),
            (false, None) => Vec::new(),
        };
        // The pane's wipers (kind 6), the windscreen's seen from inside.
        let mut wipers = [[0.0; 4]; 4];
        for ((w, _), rows) in self.car.wipers.iter().zip(wipers.chunks_exact_mut(2)) {
            rows[0] = [w.pivot.x, w.pivot.y, w.reach[0], w.reach[1]];
            rows[1] = [w.park, w.turn * w.sweep, 1.0, 0.0];
        }
        renderer.set_glass(&drops, wipers);
        // The cabin reflected in the windscreen's pane.
        let to_stage = coords::physics_to_stage() * playback.model_to_physics(playback.sample());
        renderer.set_windscreen_mirror((interior && effects).then(|| self.car.windscreen.mirror(to_stage)));
    }

    /// The drops at the current frame, stage space, on the windscreen inside, or on `outside` (the outside one) at the
    /// points over the inside one's. `lit` with [`Enhance::cabin`](crate::renderer::Enhance::cabin) (`pane_drops`), the
    /// panes themselves seen from inside with [`Enhance::windscreen`](crate::renderer::Enhance::windscreen) (`effects`).
    fn drops(&self, playback: &Playback, outside: Option<&Windscreen>, lit: bool, effects: bool) -> Vec<gpu::Drop> {
        let to_stage = coords::physics_to_stage() * playback.model_to_physics(playback.sample());
        let w = &self.car.windscreen;
        let place = |at: Vec2, lift: f32| match outside {
            Some(o) => o.point(o.chart(w.point(at, 0.0)), lift),
            None => w.point(at, lift),
        };
        // The film's streaks along the airflow, longer with speed: up the windscreen, back along the side windows.
        let streak = 1.0 + (Vec3::from(playback.sample().car.velocity).length() / FILM_STREAK_SPEED).min(2.0);
        let [across, up, _] = outside.unwrap_or(w).axes.map(|a| to_stage.transform_vector3(a));
        // The panes themselves seen from inside, under all else on them.
        let mut drops = Vec::new();
        if outside.is_none() && effects {
            drops.extend(pane_cells(&self.glass, w, to_stage, 0.0, &self.corners[0]));
            for ((side, glass), corners) in self.car.sides.iter().zip(&self.sides).zip(&self.corners[1..]) {
                drops.extend(pane_cells(glass, &side.inside, to_stage, 1.0, corners));
            }
        }
        drops.extend(pane_drops(&self.glass, w, outside, to_stage, streak, lit));
        for (side, glass) in self.car.sides.iter().zip(&self.sides) {
            // Outside, on the exterior pane over it, when the car has one.
            let on = match outside {
                Some(_) => side.outside.as_ref().map(Some),
                None => Some(None),
            };
            if let Some(on) = on {
                drops.extend(pane_drops(glass, &side.inside, on, to_stage, -streak, lit));
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
                    normal: [0.0; 4],
                    far: [0.0; 3],
                    tint: [0; 4],
                    edge: [0; 4],
                });
            }
        }
        drops
    }
}

/// The drops, film and mud of one pane's `glass` at the current frame, stage space (`to_stage` from the car's model),
/// on its chart `w` inside, or on `outside` at the points over `w`'s. `streak` is the film's streak length, up the
/// pane, or along it when negative. `lit`: the mud seen from inside lets the outside's light through (kind 5).
fn pane_drops(
    glass: &Glass,
    w: &Windscreen,
    outside: Option<&Windscreen>,
    to_stage: Mat4,
    streak: f32,
    lit: bool,
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
            normal: [0.0; 4],
            far: [0.0; 3],
            tint: [0; 4],
            edge: [0; 4],
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
                let (kind, shade) = match (outside, model::is_snow(d.left)) {
                    (Some(_), _) => (3.0, 1.0),
                    (None, false) => (if lit { 5.0 } else { 3.0 }, MUD_INSIDE),
                    (None, true) => (3.0, SNOW_INSIDE),
                };
                let [r, g, b] = model::speck_color(d.left).map(|c| c * shade);
                (kind, 1.0, [r, g, b, 1.0])
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
            normal: [0.0; 4],
            far: [0.0; 3],
            tint: [0; 4],
            edge: [0; 4],
        }
    }));
    drops
}

/// The cells of one pane's chart `w` (its `glass`'s film cells) on the glass, stage space, for the pane seen from
/// inside (kind 6, `glass.wgsl`): each with its middle and size in the chart, metres, in `color`, and from its film
/// `corners` ([`Corner`]) how far inside the outline they lie in `edge` (over [`gpu::PANE_EDGE`]), and `tints`
/// ([`Tints::film_tints`]) what the glass lets through across each in `tint`; `seed` 0 the windscreen, 1 a side window.
/// Its corners as [`gpu::Drop`] says, exact: the glass's corners need not lie in a plane. Only the cells with a corner
/// on the glass.
fn pane_cells(
    glass: &Glass,
    w: &Windscreen,
    to_stage: Mat4,
    seed: f32,
    (corners, tints): &(Vec<Corner>, Vec<f32>),
) -> Vec<gpu::Drop> {
    let steps = gpu::PANE_TINTS;
    let across = model::FILM_CELLS[0] * steps + 1;
    // The chart's mean plane's: the grid's heights step from cell to cell.
    let out = to_stage.transform_vector3(w.axes[2]).normalize();
    (0..glass.film.len())
        .filter_map(|index| {
            let (at, size) = glass.film_cell(index);
            // Neighbours take a shared corner's point, so its height, from the same sum.
            let (column, row) = (index % model::FILM_CELLS[0], index / model::FILM_CELLS[0]);
            let cell = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .map(|(i, j)| corners[(row + j) * (model::FILM_CELLS[0] + 1) + column + i]);
            if cell.iter().all(|c| c.edge <= 0.0) {
                return None;
            }
            let [low, right, top, far] = cell.map(|c| to_stage.transform_point3(c.point));
            Some(gpu::Drop {
                centre: low.to_array(),
                kind: 6.0,
                axis_u: (right - low).to_array(),
                seed,
                axis_v: (top - low).to_array(),
                alpha: 1.0,
                color: [at.x, at.y, size.x, size.y],
                normal: out.extend(0.0).to_array(),
                far: (far - low).to_array(),
                tint: (0..(steps + 1) * (steps + 1)).fold([0; 4], |mut tint, k| {
                    let through = tints[(row * steps + k / (steps + 1)) * across + column * steps + k % (steps + 1)];
                    tint[k / 8] |= ((through.clamp(0.0, 1.0) * 15.0).round() as u32) << (4 * (k % 8));
                    tint
                }),
                edge: cell.map(|c| ((c.edge / gpu::PANE_EDGE).clamp(-1.0, 1.0) * 127.0).round() as i8),
            })
        })
        .collect()
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

/// Rain and snow intensity of a precipitation: the rain's shower share, so drizzle wets the glass slowly and
/// heavy rain as it is; snow unchanged ([`Precipitation::shower`]).
fn amounts(p: Option<Precipitation>) -> (f32, f32) {
    match p {
        Some(p) if p.snow => (0.0, p.intensity),
        Some(p) => (p.shower(), 0.0),
        None => (0.0, 0.0),
    }
}

/// The raw rain of a precipitation (no snow), for the wipers' switch: drizzle wipes now and then.
fn falling(p: Option<Precipitation>) -> f32 {
    p.filter(|p| !p.snow).map_or(0.0, |p| p.intensity)
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
    fn the_outline_is_the_glass_s_rim_not_the_edges_its_triangles_share() {
        let w = flat();
        assert_eq!(w.outline.len(), 4, "the diagonal is not on it");
        let [lo, hi] = w.bounds;
        let middle = (lo + hi) * 0.5;
        assert!((w.edge(middle) - 0.4).abs() < 1e-3, "half the 0.8 m height in from the top and the foot");
        assert!((w.edge(Vec2::new(middle.x, lo.y - 0.1)) + 0.1).abs() < 1e-3, "off the glass, negative");
        // Off the glass a point lies at the height of the nearest point on it: here, in the glass's plane.
        let off = w.point_near(Vec2::new(hi.x + 0.2, middle.y));
        assert!((off - w.origin).dot(w.axes[2]).abs() < 1e-4);
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

    #[test]
    fn a_sun_strip_on_the_glass_lets_little_through() {
        // The windscreen's faces with uv v 0 at the top, on a texture clear below and a near-opaque strip on top.
        let up = Vec3::new(0.0, 0.53, 0.85) * 0.8;
        let (a, b) = (Vec3::new(0.7, 0.72, -2.13), Vec3::new(-0.7, 0.72, -2.13));
        let uv = |u: f32, v: f32| Vec2::new(u, v);
        let slot = damage::window_slot(WINDSCREEN).unwrap();
        let tints = Tints {
            faces: vec![
                (slot, [a, b, b + up], [uv(0.0, 1.0), uv(1.0, 1.0), uv(1.0, 0.0)], 0),
                (slot, [a, b + up, a + up], [uv(0.0, 1.0), uv(1.0, 0.0), uv(0.0, 0.0)], 0),
            ],
            images: vec![(1, 4, [[0, 0, 0, 240], [9, 9, 9, 30], [9, 9, 9, 30], [9, 9, 9, 30]].concat())],
        };
        let w = flat();
        let on = |up_the_glass: f32| w.chart(a.lerp(b, 0.5) + up * up_the_glass);
        assert!(tints.through(WINDSCREEN, &w, on(0.9)) < 0.1, "the strip");
        assert!((tints.through(WINDSCREEN, &w, on(0.3)) - 0.88).abs() < 0.01, "the clear glass");
        assert_eq!(tints.through(WINDSCREEN, &w, Vec2::new(0.0, 5.0)), 1.0, "off the glass");
        assert_eq!(tints.through("i_window_lf", &w, on(0.9)), 1.0, "another window's faces");
        // The pane's samples find the strip's edge (three quarters up the glass) to a quarter of a cell.
        let glass = Glass::new(w.bounds, w.area, Vec::new(), 1);
        let samples = tints.film_tints(WINDSCREEN, &glass, &w);
        let [columns, rows] = model::FILM_CELLS.map(|n| n * gpu::PANE_TINTS);
        let [lo, hi] = w.bounds;
        let step = (hi.y - lo.y) / rows as f32;
        let strip: Vec<f32> = (0..=rows)
            .filter(|row| samples[row * (columns + 1) + columns / 2] < 0.5)
            .map(|row| lo.y + step * row as f32)
            .collect();
        let edge = on(0.75).y;
        let nearest = strip.iter().map(|y| (y - edge).abs()).fold(f32::MAX, f32::min);
        assert!(!strip.is_empty() && nearest <= step * 1.01, "{nearest} m from the edge, a step {step} m");
        // Packed 4 bits each, a cell's samples row by row from its low corner.
        let pane = (film_corners(&glass, &w), samples);
        let cells = pane_cells(&glass, &w, Mat4::IDENTITY, 0.0, &pane);
        let packed = |d: &gpu::Drop| (0..25).map(|k| (d.tint[k / 8] >> (4 * (k % 8))) & 15).collect::<Vec<_>>();
        assert!(cells.iter().map(packed).any(|s| s.contains(&1) && s.contains(&13)), "strip 1/15, clear 13/15");
    }

    #[test]
    fn the_bulkhead_hangs_from_the_windscreen_s_foot_as_the_cabin_s_first_lit_face() {
        use crate::stage::{Address, Anchor, Layer, Lod, Shade};
        use bytemuck::Zeroable;
        let mesh = |triangles: &[[Vec3; 3]], blend: bool, shade: Option<Shade>| {
            let vertices: Vec<Vertex> = triangles
                .iter()
                .flatten()
                .map(|&p| Vertex { position: p.into(), color: [200; 4], ..Vertex::zeroed() })
                .collect();
            let state = DrawState {
                textures: [None, None],
                shadow: None,
                address: [Address::Wrap; 2],
                second_texture: false,
                alpha_ref: None,
                mip_bias: false,
                cull_back: true,
                blend,
                additive: false,
                depth_test: true,
                depth_write: !blend,
                shade,
                specular: None,
            };
            Mesh {
                layer: Layer::Solid,
                anchor: Anchor::World,
                bounds: None,
                cell: None,
                indices: (0..vertices.len() as u32).collect(),
                draws: vec![Draw { indices: 0..vertices.len(), state, lod: Lod::Always }],
                vertices,
            }
        };
        let up = Vec3::new(0.0, 0.53, 0.85) * 0.8;
        let (a, b) = (Vec3::new(0.7, 0.72, -2.13), Vec3::new(-0.7, 0.72, -2.13));
        let glass = mesh(&[[a, b, b + up], [a, b + up, a + up]], true, Some(Shade { glass: true, ..Shade::default() }));
        let dash = mesh(&[[Vec3::ZERO, Vec3::X, Vec3::Z]], false, Some(Shade::default()));
        let windscreen = MeshDamage { window: damage::window_slot(WINDSCREEN), ..MeshDamage::default() };
        let hung = bulkhead(&[glass.clone(), dash], &[windscreen, MeshDamage::default()]).unwrap();
        let (top, bottom) = (hung.vertices[0].position, hung.vertices[1].position);
        assert!((top[1] - 0.72).abs() < 0.03 && (top[2] + 2.13).abs() < 0.05, "at the foot: {top:?}");
        assert!((bottom[1] - (top[1] - BULKHEAD_DEPTH)).abs() < 1e-5 && bottom[2] == top[2], "straight down");
        let draw = &hung.draws[0];
        assert!(!draw.state.blend && !draw.state.cull_back && draw.indices == (0..hung.indices.len()));
        assert_eq!(hung.vertices[0].color, BULKHEAD_COLOUR);
        assert!(bulkhead(&[glass], &[MeshDamage::default()]).is_none(), "no windscreen part");
    }
}
