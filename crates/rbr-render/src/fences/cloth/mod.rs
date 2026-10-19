//! The fence cloth (`NStickSystem::CFenceHandler`, `src/rbr/RBR_E90_Handler.cpp`, kernels in
//! `RBR_E91_Kernels.cpp`): pure, in stage coordinates (y up), from the `.fnc` posts and the car's pose and box.
//!
//! A fence line (a `.fnc` group) is a chain of segments: a post at every `.fnc` post and a span between each pair
//! (`E90Group::Build` 0x601470). A span is a sheet of particles (`cols - 2` inner columns of `rows` particles per
//! layer, `BuildSpan` 0x6029b0) tied by distance springs to each other and to the post columns. A post is a column
//! of particles that follows the line from its top particle to its fixed ground particle (bends), the top held near
//! a fixed anchor by a soft spring (`BuildPost` 0x602fe0). Each step runs, per segment in line order, the force,
//! integrate, spring, contact and sleep kernels (`CFenceHandler::Tic` 0x602240):
//!
//! - Forces (`E91ScaleStates` 0x6080e0) only once the segment is loose: gravity plus wind times the inverse mass.
//! - Verlet with damping 0.96 (`E91Integrate` 0x6083c0).
//! - Springs (`E91SolveDeform` 0x608860), one pass. While intact, a spring with a nonzero kind breaks the segment
//!   loose: the post's top spring past 0.5 m, a tape's vertical spring stretched past 0.2 m (which also cuts the
//!   tape at that column). Loose segments relax with the cheap `0.5 - l0^2 / (d^2 + l0^2)` correction.
//! - Contact (`E91ClampStates` 0x609960): the car (only for segments the car's box touched this step,
//!   `CDummyObject::VfCC` 0x6009b0), the ground plane through the post (`E91GuardStates` 0x609720, with friction),
//!   then the post column back onto its line (bends).
//! - Sleep (`E91ReduceDiff` 0x609060, `E90Seg::Settle` 0x605dd0): a segment whose summed motion is under
//!   `dt * 0.002` sleeps until a moving neighbour wakes it.
//!
//! The car is its collision mesh in its own frame ([`CarShape`]: `CFenceHandler::Tic` builds the fence's grid from
//! the car's each step), tested by segment and inside queries (`E37GridOps::EdgeEdge`, `M40`). Ours (marked where
//! used): the inside query (a ray-parity test in place of the grid's plane tree);
//! the wind's scale (the weather daemon's wind as [`crate::particles::weather::Wind`] gives it, `M8fb0`'s
//! `accA4 / pertA0` factor not modelled); the step is the drive's step, not the frame.

use glam::{Mat4, Vec3};
use rbr_formats::fnc::{FncFile, FncGroup};

mod build;
mod solve;

pub use solve::box_faces;
use solve::overlaps;

/// One row of the fence type table (`g_fenceTypes`, 0x7d81a0, `E90FenceType`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FenceType {
    pub id: i32,
    /// The kind of a span's vertical springs: nonzero tears.
    pub flag: i32,
    pub cols: i32,
    pub rows: i32,
    pub height: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub layers: i32,
    pub layer_step: f32,
}

const fn row(
    id: i32,
    flag: i32,
    [cols, rows]: [i32; 2],
    height: f32,
    uv: [f32; 4],
    layers: i32,
    step: f32,
) -> FenceType {
    FenceType { id, flag, cols, rows, height, u0: uv[0], v0: uv[1], u1: uv[2], v1: uv[3], layers, layer_step: step }
}

/// `g_fenceTypes` (0x7d81a0), 15 rows.
pub const TYPES: [FenceType; 15] = [
    row(1, 1, [9, 2], 0.074, [0.0, 0.0, 14.0, 1.0], 1, 0.0),
    row(2, 1, [17, 2], 0.074, [0.0, 0.0, 28.0, 1.0], 1, 0.0),
    row(3, 0, [9, 5], 0.9, [0.0, 0.0, 32.0, 27.125], 1, 0.0),
    row(4, 0, [9, 5], 0.9, [0.0, 0.0, 64.0, 27.125], 1, 0.0),
    row(5, 1, [0, 2], 0.074, [0.961, 0.031, 0.992, 0.73], 1, 0.0),
    row(6, 1, [0, 5], 0.9, [0.961, 0.031, 0.992, 0.73], 1, 0.0),
    row(7, 1, [0, 2], 0.074, [0.816, 0.031, 0.848, 0.73], 1, 0.0),
    row(8, 1, [0, 5], 0.9, [0.816, 0.031, 0.848, 0.73], 1, 0.0),
    row(9, 1, [0, 2], 0.074, [0.863, 0.027, 0.895, 0.73], 1, 0.0),
    row(10, 1, [0, 5], 0.9, [0.863, 0.027, 0.895, 0.73], 1, 0.0),
    row(11, 1, [0, 2], 0.074, [0.91, 0.027, 0.941, 0.73], 1, 0.0),
    row(12, 1, [0, 5], 0.9, [0.91, 0.027, 0.941, 0.73], 1, 0.0),
    row(13, 0, [9, 2], 0.074, [0.0, 0.0, 14.0, 1.0], 2, 0.4),
    row(14, 0, [17, 2], 0.074, [0.0, 0.0, 28.0, 1.0], 2, 0.4),
    row(15, 0, [0, 2], 0.074, [0.0, 0.0, 1.0, 1.0], 2, 0.4),
];

/// Gravity (`g_7d7cf4`, stage axes).
pub const GRAVITY: Vec3 = Vec3::new(0.0, -9.82, 0.0);
/// Verlet damping (`E91Integrate`).
const DAMPING: f32 = 0.96;
/// The post's top over its foot, m (`BuildPost`, and the static post).
pub const POST_HEIGHT: f32 = 1.1;
/// A line resets when the car is this far past its box on an axis, m (`E90BoxNear`).
const RESET_DISTANCE: f32 = 100.0;

/// Segment flags (`E90Seg::m_flags`).
pub const LOOSE: u32 = 2;
pub const ASLEEP: u32 = 4;
pub const TOUCHED: u32 = 8;

/// A particle (`E90Particle`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub pos: Vec3,
    pub old: Vec3,
    pub force: Vec3,
    pub inv_mass: f32,
}

/// A distance spring (`E90Spring`) between two particles of the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub a: usize,
    pub b: usize,
    pub wa: f32,
    pub wb: f32,
    pub len: f32,
    pub len2: f32,
    pub stiffness: f32,
    pub kind: i32,
}

/// A particle kept on a spring's line at `proj` from its first end (`E90Bend`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    pub spring: usize,
    pub p: usize,
    pub proj: f32,
}

/// A post or a span (`E90Seg`).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Its particles in the line's list: `start..start + count`.
    pub start: usize,
    pub count: usize,
    /// Its springs, and their order (`m_springs.m_ptrs`): the first `active` are solved; breaking swaps a spring
    /// past the end.
    pub springs: Vec<Spring>,
    pub order: Vec<usize>,
    pub active: usize,
    /// Springs whose line the car can push (the post's top to foot).
    pub fixed: Vec<usize>,
    pub bends: Vec<Bend>,
    /// Particles kept out of the car and over the ground.
    pub pins: Vec<usize>,
    pub post: bool,
    /// The `.fnc` post it starts at.
    pub dummy: usize,
    pub prev: Option<usize>,
    pub next: Option<usize>,
    pub flags: u32,
    /// The summed motion of the last step, squared.
    pub rest: f32,
    /// The column a tear cut (`m_unk64`).
    pub tear: i32,
    /// The ground plane through the post, `n·p + d` (`E90Seg::Init`).
    pub plane: [f32; 4],
}

/// One fence line (`E90Group`).
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub span_type: usize,
    pub post_type: usize,
    pub material: u32,
    pub rail_material: u32,
    pub center: Vec3,
    pub half: Vec3,
    pub posts: Vec<Post>,
    /// Whether the cloth is built and simulating (`m_mem != 0`); otherwise it is at rest.
    pub built: bool,
    pub particles: Vec<Particle>,
    /// In build order: post 0, then span `i` and post `i + 1`.
    pub segments: Vec<Segment>,
    /// Per `.fnc` post: its post segment and the span starting at it.
    pub post_segment: Vec<usize>,
    pub span_segment: Vec<Option<usize>>,
    rest_particles: Vec<Particle>,
    rest_segments: Vec<Segment>,
}

/// A `.fnc` post (`CDummyObject`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Post {
    pub pos: Vec3,
    pub center: Vec3,
    pub half: Vec3,
    pub color: u32,
}

/// The car as the fences see it: its collision mesh's faces in its own frame (empty: the box `lo`-`hi` stands in),
/// that mesh's box, and its frame (body to stage, rigid).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarShape<'a> {
    pub to_stage: Mat4,
    pub faces: &'a [[Vec3; 3]],
    pub lo: Vec3,
    pub hi: Vec3,
    /// The car's motion over the step, stage axes (`CFenceHandler +0x710`, the car state's velocity times dt).
    pub motion: Vec3,
}

impl CarShape<'_> {
    fn stage_to_local(&self) -> Mat4 {
        self.to_stage.inverse()
    }

    /// Its box's stage-axis bounds.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for k in 0..8 {
            let corner = Vec3::new(
                if k & 1 == 0 { self.lo.x } else { self.hi.x },
                if k & 2 == 0 { self.lo.y } else { self.hi.y },
                if k & 4 == 0 { self.lo.z } else { self.hi.z },
            );
            let p = self.to_stage.transform_point3(corner);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        (lo, hi)
    }
}

fn type_of(index: u32) -> usize {
    (index as usize).min(TYPES.len() - 1)
}

impl Line {
    /// A line of `group` at rest (built, not simulating).
    pub fn new(group: &FncGroup) -> Line {
        let posts = group
            .posts
            .iter()
            .map(|p| Post {
                pos: Vec3::from(p.position),
                center: Vec3::from(p.center),
                half: Vec3::from(p.half_extent),
                color: p.color,
            })
            .collect();
        let mut line = Line {
            span_type: type_of(group.fence_type),
            post_type: type_of(group.post_type),
            material: group.material,
            rail_material: group.rail_material,
            center: Vec3::from(group.center),
            half: Vec3::from(group.half_extent),
            posts,
            built: false,
            particles: Vec::new(),
            segments: Vec::new(),
            post_segment: Vec::new(),
            span_segment: Vec::new(),
            rest_particles: Vec::new(),
            rest_segments: Vec::new(),
        };
        line.build();
        line.rest_particles = line.particles.clone();
        line.rest_segments = line.segments.clone();
        line
    }

    /// Back to rest (`E90Group::Reset` 0x6011b0, then a later `Build`).
    pub fn reset(&mut self) {
        self.particles.clone_from(&self.rest_particles);
        self.segments.clone_from(&self.rest_segments);
        self.built = false;
    }

    /// `CDummyObject::VfCC` (0x6009b0), ours in its inputs: the car's stage bounds against each post's box. A hit
    /// builds the line and marks the span from that post and the post itself touched for this step.
    pub fn touch(&mut self, car_lo: Vec3, car_hi: Vec3) {
        if !overlaps(self.center, self.half, car_lo, car_hi) {
            return;
        }
        for i in 0..self.posts.len() {
            let post = self.posts[i];
            if !overlaps(post.center, post.half, car_lo, car_hi) {
                continue;
            }
            self.built = true;
            if let Some(span) = self.span_segment[i] {
                self.segments[span].flags |= TOUCHED;
            }
            let post = self.post_segment[i];
            self.segments[post].flags |= TOUCHED;
        }
    }

    /// Whether the car is more than [`RESET_DISTANCE`] past the line's box on an axis (`E90BoxNear` false).
    pub fn far_from(&self, car: Vec3) -> bool {
        let off = (self.center - car).abs();
        off.cmpge(self.half + Vec3::splat(RESET_DISTANCE)).any()
    }

    /// One step of the built line (the group body of `CFenceHandler::Tic`). `wind` is the fence wind, stage axes.
    pub fn step(&mut self, dt: f32, wind: Vec3, car: &CarShape) {
        if !self.built || dt <= 0.0 {
            return;
        }
        let sleep = dt * 0.002;
        let to_local = car.stage_to_local();
        for j in 0..self.segments.len() {
            self.forces(j, wind);
            self.integrate(j, dt);
            self.solve(j);
            self.clamp(j, car, &to_local);
            self.reduce(j, sleep);
        }
        for j in 0..self.segments.len() {
            let seg = &self.segments[j];
            if seg.rest > sleep {
                let (prev, next) = (seg.prev, seg.next);
                self.segments[j].flags &= !ASLEEP;
                for k in [prev, next].into_iter().flatten() {
                    self.segments[k].flags &= !ASLEEP;
                }
            }
        }
    }
}

/// Every line of a `.fnc`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fences {
    pub lines: Vec<Line>,
}

impl Fences {
    pub fn new(file: &FncFile) -> Fences {
        Fences { lines: file.groups.iter().map(Line::new).collect() }
    }

    /// One step (`CFenceHandler::Tic` with the car's broadphase calls before it): the built lines' kernels, then a
    /// line the camera does not see (`seen`, its box's stage bounds: `CPC_GraphicsDaemon::CullPoints`) goes back to
    /// rest once the car is far from it. Returns the lines that changed.
    pub fn step(&mut self, dt: f32, wind: Vec3, car: &CarShape, seen: &dyn Fn(Vec3, Vec3) -> bool) -> Vec<usize> {
        let (lo, hi) = car.bounds();
        let at = car.to_stage.transform_point3((car.lo + car.hi) * 0.5);
        let mut changed = Vec::new();
        for (i, line) in self.lines.iter_mut().enumerate() {
            line.touch(lo, hi);
            if !line.built {
                continue;
            }
            line.step(dt, wind, car);
            if !seen(line.center - line.half, line.center + line.half) && line.far_from(at) {
                line.reset();
            }
            changed.push(i);
        }
        changed
    }

    /// All lines back to rest. Returns those that were not.
    pub fn reset(&mut self) -> Vec<usize> {
        let built: Vec<usize> = (0..self.lines.len()).filter(|&i| self.lines[i].built).collect();
        for &i in &built {
            self.lines[i].reset();
        }
        built
    }
}

#[cfg(test)]
mod tests;
