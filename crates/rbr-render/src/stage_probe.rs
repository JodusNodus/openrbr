//! The stage's collision mesh as the cameras probe it: the collision system's segment query
//! (`CCollisionSystem::Vf00` 0x4fe810, through `PTR_DAT_007c755c`) and its sphere push (`0x4fe5e0`), on the triangles
//! of the stage's `.col` (physics frame, z up).
//!
//! Faithful (the asm and the physics port, `rbr_physics::collision`):
//! - [`StageProbe::segment`]: the nearest front face the segment from `a` to `b` crosses (`triangle::ray_triangle`, the mesh
//!   ray's test, single-sided), at a ray parameter in `(0, 1]`. Its record gives the point, the unit normal
//!   `e2 x e1` and the plane's `d = -(v0 . n)` (record words 0..2, 4..6 and 7);
//! - [`StageProbe::push_out`]: `0x4fe5e0`. Every triangle of the tree leaves whose boxes meet the cube of half side
//!   `radius` about the point moves the point out along its plane's normal to `radius` when it is in front of the
//!   plane and nearer than `radius`, whether or not it lies over the triangle. The point moves as it goes, so later
//!   triangles see it moved.
//!
//! Ours: the leaves come from a grid of [`CELL`] squares instead of the tree walk, so [`StageProbe::push_out`] visits
//! them in the file's order, not the walk's (it matters only where two planes push the same point). The leaf boxes
//! are the nodes' centre and half extents, as `e38::SatBox` reads them.

use std::collections::HashMap;

use glam::Vec3;
use rbr_formats::col::{self, NodeKind};
use rbr_physics::collision::triangle;
use rbr_physics::math::vec::Vec3 as PVec3;

/// The grid's square, m.
const CELL: f32 = 16.0;
/// A leaf over more squares than this on a side goes into the list every query walks.
const WIDE: i32 = 32;

/// A segment query's hit: the point, the face's unit normal and its plane's `d` (`n . p + d = 0` on the plane).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    pub d: f32,
}

impl Hit {
    /// `|n . p + d|`: the distance of `p` from the face's plane.
    pub fn distance(&self, p: Vec3) -> f32 {
        (self.normal.dot(p) + self.d).abs()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Leaf {
    lo: Vec3,
    hi: Vec3,
    triangles: std::ops::Range<u32>,
}

/// The stage's collision triangles by tree leaf, in a grid.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StageProbe {
    leaves: Vec<Leaf>,
    triangles: Vec<[Vec3; 3]>,
    cells: HashMap<(i32, i32), Vec<u32>>,
    wide: Vec<u32>,
}

fn pv(v: Vec3) -> PVec3 {
    PVec3 { x: v.x, y: v.y, z: v.z }
}

fn key(x: f32, y: f32) -> (i32, i32) {
    ((x / CELL).floor() as i32, (y / CELL).floor() as i32)
}

impl StageProbe {
    /// The probe over every cell tree of `col`.
    pub fn new(col: &col::ColFile) -> StageProbe {
        let mut probe = StageProbe::default();
        for cell in &col.cells {
            for node in &cell.tree.nodes {
                let NodeKind::Leaf { triangles } = &node.kind else { continue };
                let first = probe.triangles.len() as u32;
                for t in &cell.tree.triangles[triangles.clone()] {
                    let corner = |i: usize| cell.vertices.get(t.vertices[i] as usize).copied().map(Vec3::from);
                    if let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) {
                        probe.triangles.push([a, b, c]);
                    }
                }
                let b = node.bounds;
                let (centre, half) = (Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5]).abs());
                probe.add(Leaf {
                    lo: centre - half,
                    hi: centre + half,
                    triangles: first..probe.triangles.len() as u32,
                });
            }
        }
        probe
    }

    /// A probe of loose triangles, a leaf each (tests and synthetic scenes).
    pub fn from_triangles(triangles: &[[Vec3; 3]]) -> StageProbe {
        let mut probe = StageProbe::default();
        for (i, tri) in triangles.iter().enumerate() {
            probe.triangles.push(*tri);
            let (lo, hi) = (tri[0].min(tri[1]).min(tri[2]), tri[0].max(tri[1]).max(tri[2]));
            probe.add(Leaf { lo, hi, triangles: i as u32..i as u32 + 1 });
        }
        probe
    }

    /// The probe of stage `stage`'s `.col` (retail or a stage folder), or `None` when it is missing or unreadable
    /// (a RallySimFans-encrypted file).
    pub fn load(data: &rbr_formats::data::Data, stage: &str) -> Option<StageProbe> {
        let bytes = rbr_formats::stage_files::read(data, stage, "col").ok()?;
        Some(StageProbe::new(&col::parse(&bytes).ok()?))
    }

    fn add(&mut self, leaf: Leaf) {
        let index = self.leaves.len() as u32;
        let (x0, y0) = key(leaf.lo.x, leaf.lo.y);
        let (x1, y1) = key(leaf.hi.x, leaf.hi.y);
        if x1 - x0 > WIDE || y1 - y0 > WIDE {
            self.wide.push(index);
        } else {
            for x in x0..=x1 {
                for y in y0..=y1 {
                    self.cells.entry((x, y)).or_default().push(index);
                }
            }
        }
        self.leaves.push(leaf);
    }

    /// The leaves whose boxes meet the box `lo`..`hi`, in the order they were added.
    fn leaves_in(&self, lo: Vec3, hi: Vec3) -> Vec<&Leaf> {
        let (x0, y0) = key(lo.x, lo.y);
        let (x1, y1) = key(hi.x, hi.y);
        let mut found: Vec<u32> = self.wide.clone();
        for x in x0..=x1 {
            for y in y0..=y1 {
                found.extend(self.cells.get(&(x, y)).into_iter().flatten());
            }
        }
        found.sort_unstable();
        found.dedup();
        found
            .into_iter()
            .map(|i| &self.leaves[i as usize])
            .filter(|l| l.lo.cmple(hi).all() && l.hi.cmpge(lo).all())
            .collect()
    }

    /// `E39_Query7F0` (0x4ff7f0) with the vertex flag: every triangle of the leaves whose boxes meet the box `lo`..`hi`,
    /// whole and unclipped, at most `most` (the caller's buffer; the sweep stops when it is full).
    pub fn triangles_in(&self, lo: Vec3, hi: Vec3, most: usize) -> Vec<[Vec3; 3]> {
        self.leaves_in(lo, hi)
            .into_iter()
            .flat_map(|leaf| &self.triangles[leaf.triangles.start as usize..leaf.triangles.end as usize])
            .copied()
            .take(most)
            .collect()
    }

    /// `CCollisionSystem::Vf00`: the nearest front face crossed going from `a` to `b`, or `None`.
    pub fn segment(&self, a: Vec3, b: Vec3) -> Option<Hit> {
        let direction = b - a;
        let mut best: Option<(f32, [Vec3; 3])> = None;
        for leaf in self.leaves_in(a.min(b), a.max(b)) {
            for &tri in &self.triangles[leaf.triangles.start as usize..leaf.triangles.end as usize] {
                let hit = triangle::ray_triangle(pv(a), pv(direction), pv(tri[0]), pv(tri[1]), pv(tri[2]));
                if let (true, Some(t)) = (hit.hit, hit.distance) {
                    if t > 0.0 && t <= 1.0 && best.is_none_or(|(bt, _)| t < bt) {
                        best = Some((t, tri));
                    }
                }
            }
        }
        let (t, [v0, v1, v2]) = best?;
        let normal = (v2 - v0).cross(v1 - v0).normalize_or_zero();
        Some(Hit { point: a + direction * t, normal, d: -v0.dot(normal) })
    }

    /// `0x4fe5e0`: `p` pushed out to `radius` from every plane in front of which it lies nearer than that, and whether
    /// any did.
    pub fn push_out(&self, p: Vec3, radius: f32) -> (bool, Vec3) {
        let radius = radius.abs();
        let mut point = p;
        let mut pushed = false;
        for leaf in self.leaves_in(p - Vec3::splat(radius), p + Vec3::splat(radius)) {
            for &[v0, v1, v2] in &self.triangles[leaf.triangles.start as usize..leaf.triangles.end as usize] {
                let normal = (v2 - v0).cross(v1 - v0).normalize_or_zero();
                let s = (point - v0).dot(normal);
                if s > 0.0 && s < radius {
                    point += normal * (radius - s);
                    pushed = true;
                }
            }
        }
        (pushed, point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor at z = 0 facing up, 20 m square about the origin, wound so that `e2 x e1` points up.
    fn floor() -> StageProbe {
        let (a, b, c, d) = (
            Vec3::new(-10.0, -10.0, 0.0),
            Vec3::new(10.0, -10.0, 0.0),
            Vec3::new(10.0, 10.0, 0.0),
            Vec3::new(-10.0, 10.0, 0.0),
        );
        StageProbe::from_triangles(&[[a, c, b], [a, d, c]])
    }

    #[test]
    fn a_segment_hits_the_front_face_only() {
        let probe = floor();
        let hit = probe.segment(Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 2.0, -1.0)).unwrap();
        assert!((hit.point - Vec3::new(1.0, 2.0, 0.0)).length() < 1e-5, "{hit:?}");
        assert!((hit.normal - Vec3::Z).length() < 1e-6 && hit.d.abs() < 1e-6);
        assert!((hit.distance(Vec3::new(0.0, 0.0, -2.0)) - 2.0).abs() < 1e-6);
        // From below, and short of the floor, nothing.
        assert!(probe.segment(Vec3::new(1.0, 2.0, -1.0), Vec3::new(1.0, 2.0, 3.0)).is_none());
        assert!(probe.segment(Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 2.0, 0.5)).is_none());
    }

    #[test]
    fn the_sphere_push_lifts_a_point_to_the_radius() {
        let probe = floor();
        let (pushed, p) = probe.push_out(Vec3::new(0.0, 0.0, 0.1), 0.4);
        assert!(pushed && (p.z - 0.4).abs() < 1e-6, "{p:?}");
        // Behind the plane or beyond the radius: left where it is.
        assert_eq!(probe.push_out(Vec3::new(0.0, 0.0, -0.1), 0.4), (false, Vec3::new(0.0, 0.0, -0.1)));
        assert_eq!(probe.push_out(Vec3::new(0.0, 0.0, 1.0), 0.4), (false, Vec3::new(0.0, 0.0, 1.0)));
    }
}
