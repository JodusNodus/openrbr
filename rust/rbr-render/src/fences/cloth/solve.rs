//! A line's step kernels (`RBR_E91_Kernels.cpp`): forces, integration, the spring solve and tearing, the car's
//! clamp and push-out, the guard and sleep; and the box queries they use.

use super::{CarShape, Line, ASLEEP, DAMPING, GRAVITY, LOOSE, TOUCHED, TYPES};
use glam::{Mat4, Vec3};

impl Line {
    fn range(&self, j: usize) -> std::ops::Range<usize> {
        let seg = &self.segments[j];
        seg.start..seg.start + seg.count
    }

    /// `E91ScaleStates` (0x6080e0): loose and awake segments only; never the two-layer tape types (ids 13, 14).
    pub(super) fn forces(&mut self, j: usize, wind: Vec3) {
        let flags = self.segments[j].flags;
        if flags & LOOSE == 0 || flags & ASLEEP != 0 || matches!(TYPES[self.span_type].id, 13 | 14) {
            return;
        }
        for i in self.range(j) {
            let p = &mut self.particles[i];
            p.force = (wind + GRAVITY) * p.inv_mass;
        }
    }

    /// `E91Integrate` (0x6083c0).
    pub(super) fn integrate(&mut self, j: usize, dt: f32) {
        if self.segments[j].flags & ASLEEP != 0 {
            return;
        }
        let dt2 = dt * dt;
        for i in self.range(j) {
            let p = &mut self.particles[i];
            let pos = p.pos;
            p.pos = p.pos + (p.pos * DAMPING - p.old * DAMPING) + p.force * dt2;
            p.old = pos;
        }
    }

    /// `E91SolveDeform` (0x608860).
    pub(super) fn solve(&mut self, j: usize) {
        if self.segments[j].flags & ASLEEP != 0 {
            return;
        }
        let seg = &self.segments[j];
        if !seg.bends.is_empty() {
            // The post's top follows its column's first particle; the springs then put it back on its line.
            let top = seg.start + seg.count - 3;
            self.particles[top].pos = self.particles[seg.start].pos;
        }
        let loose = seg.flags & LOOSE != 0;
        let mut k = 0;
        while k < self.segments[j].active {
            let seg = &self.segments[j];
            let s = seg.springs[seg.order[k]];
            let mut d = self.particles[s.b].pos - self.particles[s.a].pos;
            let d2 = d.z * d.z + d.y * d.y + d.x * d.x;
            if loose {
                if s.len != 0.0 {
                    d *= 0.0 - (s.len2 / (d2 + s.len2) - 0.5);
                }
            } else if s.len == 0.0 {
                if s.kind != 0 && 0.25 < d2 {
                    let seg = &mut self.segments[j];
                    seg.flags |= LOOSE;
                    let last = seg.active - 1;
                    seg.order.swap(k, last);
                    seg.active = last;
                    return;
                }
            } else {
                let corr = (d2 - s.len2) / (s.len * 2.0) + s.len;
                let diff = s.len - corr;
                if s.kind != 0 && 0.2 < diff.abs() {
                    self.tear(j, k, s.b);
                    return;
                }
                d *= 0.0 - diff / corr;
            }
            let wa = s.wa * s.stiffness;
            let wb = s.wb * s.stiffness;
            self.particles[s.a].pos += d * wa;
            self.particles[s.b].pos -= d * wb;
            k += 1;
        }
    }

    /// The tear of an intact tape at spring `k` (whose second particle is `b`), as `E91SolveDeform` does it: the
    /// segment goes loose and the cut column's bottom spring and the three springs after `k` (the cell's top,
    /// and both diagonals) are swapped out. Laid out for one-layer two-row tape; other layouts only go loose.
    fn tear(&mut self, j: usize, k: usize, b: usize) {
        let cols = TYPES[self.span_type].cols as isize;
        let next_start = self.segments[j].next.map(|n| self.segments[n].start);
        let seg = &mut self.segments[j];
        seg.flags |= LOOSE;
        let column = if Some(b) == next_start { cols - 1 } else { (b as isize - seg.start as isize) / 2 };
        seg.tear = column as i32;
        let cut = column - 12 + cols * 4;
        for at in [cut, k as isize + 3, k as isize + 2, k as isize + 1] {
            if at < 0 || at as usize >= seg.active {
                return;
            }
            let last = seg.active - 1;
            seg.order.swap(at as usize, last);
            seg.active = last;
        }
    }

    /// `E91ClampStates` (0x609960): the car if touched, the ground if awake, then the bends.
    pub(super) fn clamp(&mut self, j: usize, car: &CarShape, to_local: &Mat4) {
        let flags = self.segments[j].flags;
        if flags & TOUCHED != 0 {
            self.push_out(j, car, to_local);
        }
        if flags & ASLEEP == 0 {
            self.guard(j);
        }
        let seg = &self.segments[j];
        for bend in &seg.bends {
            let s = seg.springs[bend.spring];
            let (a, b) = (self.particles[s.a].pos, self.particles[s.b].pos);
            self.particles[bend.p].pos = a + (b - a) * bend.proj;
        }
        self.segments[j].flags &= !TOUCHED;
    }

    /// `E91AttachPairs` (0x6091d0) against the car's mesh (local frame; `E37GridOps::EdgeEdge` 0x4f8280 and `M40`
    /// 0x4f8a20 on the fence's car grid). A post line (fixed spring) that the mesh cuts top to foot: when it does not
    /// cut it foot to top (the foot inside), the top goes with the car unless the post leans against its motion;
    /// else both ends move along the two hits' mean normal out to the surface, from the middle of the hits. A pin
    /// inside the mesh goes to its surface on the line from the mesh's centre.
    fn push_out(&mut self, j: usize, car: &CarShape, to_local: &Mat4) {
        let boxed;
        let faces = if car.faces.is_empty() {
            boxed = box_faces(car.lo, car.hi);
            &boxed[..]
        } else {
            car.faces
        };
        let centre = (car.lo + car.hi) * 0.5;
        let seg = &self.segments[j];
        for &f in &seg.fixed {
            let s = seg.springs[f];
            let top = to_local.transform_point3(self.particles[s.a].pos);
            let foot = to_local.transform_point3(self.particles[s.b].pos);
            let Some((in_top, normal_top)) = edge_hit(faces, top, foot) else { continue };
            match edge_hit(faces, foot, top) {
                None => {
                    if (self.particles[s.a].pos - self.particles[s.b].pos).dot(car.motion) > -0.4 {
                        self.particles[s.a].pos += car.motion * s.wa;
                    }
                }
                Some((in_foot, normal_foot)) => {
                    let mean = (normal_top + normal_foot) * 0.5;
                    let out = if mean.length() == 0.0 { normal_top * 5.0 } else { mean * (5.0 / mean.length()) };
                    let middle = (in_top + in_foot) * 0.5;
                    if let Some((surface, _)) = edge_hit(faces, middle + out, middle) {
                        let push = car.to_stage.transform_vector3(surface - middle);
                        self.particles[s.a].pos += push * s.wa;
                        self.particles[s.b].pos += push * s.wb;
                    }
                }
            }
        }
        for &p in &seg.pins {
            let local = to_local.transform_point3(self.particles[p].pos);
            if inside(faces, local) {
                if let Some((surface, _)) = edge_hit(faces, local + (local - centre), centre) {
                    self.particles[p].pos = car.to_stage.transform_point3(surface);
                }
            }
        }
    }

    /// `E91GuardStates` (0x609720): pins under the plane come back to where they crossed it, sliding with friction.
    fn guard(&mut self, j: usize) {
        let [nx, ny, nz, nd] = self.segments[j].plane;
        let n = Vec3::new(nx, ny, nz);
        for &i in &self.segments[j].pins {
            let p = &mut self.particles[i];
            let d = n.dot(p.pos) + nd;
            if d >= 0.0 {
                continue;
            }
            let e = n.dot(p.old) + nd;
            if e - d == 0.0 {
                p.pos = p.old;
                return;
            }
            let hit = p.old + (p.pos - p.old) * (e / (e - d));
            let friction = d * -0.8;
            let slide = (p.pos - n * d) - hit;
            let q = slide.length_squared();
            let len = q.sqrt();
            p.pos = if q < friction * friction || len == 0.0 { hit } else { hit + slide * ((len - friction) / len) };
        }
    }

    /// `E91ReduceDiff` (0x609060).
    pub(super) fn reduce(&mut self, j: usize, sleep: f32) {
        let sum: Vec3 = self.range(j).map(|i| self.particles[i].pos - self.particles[i].old).sum();
        let q = sum.z * sum.z + sum.x * sum.x + sum.y * sum.y;
        let seg = &mut self.segments[j];
        seg.rest = q;
        if q <= sleep {
            seg.flags |= ASLEEP;
        }
    }
}

pub(super) fn overlaps(center: Vec3, half: Vec3, lo: Vec3, hi: Vec3) -> bool {
    let (c, h) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
    ((center - c).abs()).cmplt(half + h).all()
}

/// `EdgeEdge`: the first face, in the mesh's order, that the segment `a`-`b` enters from its front (`E37Tri5A90`
/// 0x4f5a90 takes a face only when its determinant is at least 1e-6, so a face is hit from the side its winding's
/// normal faces), at a fraction over 0 and under 1.01: the point and the face's unit normal. The mesh's
/// bounding-sphere reject (`M7640`) is left out: it only saves time.
pub(super) fn edge_hit(faces: &[[Vec3; 3]], a: Vec3, b: Vec3) -> Option<(Vec3, Vec3)> {
    let d = b - a;
    faces.iter().find_map(|&[p, q, r]| {
        let (e1, e2) = (q - p, r - p);
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det < 1e-6 {
            return None;
        }
        let s = a - p;
        let u = s.dot(h) / det;
        let k = s.cross(e1);
        let v = d.dot(k) / det;
        let t = e2.dot(k) / det;
        let on = (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && 0.0 < t && t < 1.01;
        on.then(|| (a + d * t, e1.cross(e2).normalize_or_zero()))
    })
}

/// `M40`: whether a point is inside the mesh. Ours: the crossings of a ray up from it (the game walks the grid's
/// plane tree).
pub(super) fn inside(faces: &[[Vec3; 3]], p: Vec3) -> bool {
    let up = Vec3::new(0.0123, 1.0, 0.0071);
    let crossings = faces
        .iter()
        .filter(|&&[a, b, c]| {
            let (e1, e2) = (b - a, c - a);
            let h = up.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-12 {
                return false;
            }
            let s = p - a;
            let u = s.dot(h) / det;
            let k = s.cross(e1);
            let v = up.dot(k) / det;
            (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && e2.dot(k) / det > 0.0
        })
        .count();
    crossings % 2 == 1
}

/// The twelve faces of the box `lo`-`hi`, wound outwards.
pub fn box_faces(lo: Vec3, hi: Vec3) -> Vec<[Vec3; 3]> {
    let c = |k: usize| {
        Vec3::new(
            if k & 1 == 0 { lo.x } else { hi.x },
            if k & 2 == 0 { lo.y } else { hi.y },
            if k & 4 == 0 { lo.z } else { hi.z },
        )
    };
    // Each side as four corners counter-clockwise seen from outside.
    let sides = [[0, 4, 6, 2], [1, 3, 7, 5], [0, 1, 5, 4], [2, 6, 7, 3], [0, 2, 3, 1], [4, 5, 7, 6]];
    sides.iter().flat_map(|&[a, b, q, d]| [[c(a), c(b), c(q)], [c(a), c(q), c(d)]]).collect()
}
