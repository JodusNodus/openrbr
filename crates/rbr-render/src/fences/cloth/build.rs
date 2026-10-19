//! Building a line (`E90Group::Build` 0x601470): its posts, spans, particles and springs.

use super::{Bend, Line, Particle, Segment, Spring, POST_HEIGHT, TYPES};
use glam::Vec3;

impl Line {
    /// `E90Group::Build` (0x601470): a post at every `.fnc` post and a span between each pair.
    pub(super) fn build(&mut self) {
        let n = self.posts.len();
        self.post_segment = vec![0; n];
        self.span_segment = vec![None; n];
        if n == 0 {
            return;
        }
        let first = self.build_post(0);
        self.post_segment[0] = first;
        let mut prev = first;
        for i in 0..n - 1 {
            // The span's slot comes before the next post's, though the post is built first.
            let span = self.segments.len();
            let placeholder = self.new_segment(i, false);
            self.segments.push(placeholder);
            let post = self.build_post(i + 1);
            self.segments[span] = self.build_span(i, prev, post);
            self.segments[prev].next = Some(span);
            self.segments[span].prev = Some(prev);
            self.segments[span].next = Some(post);
            self.segments[post].prev = Some(span);
            self.post_segment[i + 1] = post;
            self.span_segment[i] = Some(span);
            prev = post;
        }
    }

    fn new_segment(&mut self, dummy: usize, post: bool) -> Segment {
        Segment {
            start: self.particles.len(),
            count: 0,
            springs: Vec::new(),
            order: Vec::new(),
            active: 0,
            fixed: Vec::new(),
            bends: Vec::new(),
            pins: Vec::new(),
            post,
            dummy,
            prev: None,
            next: None,
            flags: 0,
            rest: 0.0,
            tear: 0,
            plane: self.plane(dummy),
        }
    }

    /// `E90Seg::Init` (0x605b60): the plane through the post along the line's slope, 0.05 m up.
    fn plane(&self, dummy: usize) -> [f32; 4] {
        let a = self.posts[dummy.saturating_sub(1)].pos;
        let b = self.posts[(dummy + 1).min(self.posts.len() - 1)].pos;
        let dir = (b - a).normalize_or_zero();
        let side = dir.cross(Vec3::Y).normalize_or_zero();
        let n = side.cross(dir);
        let p = self.posts[dummy].pos + Vec3::new(0.0, 0.05, 0.0);
        [n.x, n.y, n.z, -n.dot(p)]
    }

    fn add_particle(&mut self, seg: &mut Segment, pos: Vec3, inv_mass: f32) -> usize {
        self.particles.push(Particle { pos, old: pos, force: Vec3::ZERO, inv_mass });
        seg.count += 1;
        self.particles.len() - 1
    }

    /// `E90Seg::AddSpring` (0x605fb0); none between two fixed particles.
    fn add_spring(&self, seg: &mut Segment, a: usize, b: usize, kind: i32, stiffness: f32) -> Option<usize> {
        let (pa, pb) = (self.particles[a], self.particles[b]);
        if pb.inv_mass + pa.inv_mass == 0.0 {
            return None;
        }
        let w = 1.0 / (pb.inv_mass + pa.inv_mass);
        let len = (pb.pos - pa.pos).length();
        seg.springs.push(Spring {
            a,
            b,
            wa: w * pa.inv_mass,
            wb: pb.inv_mass * w,
            len,
            len2: len * len,
            stiffness,
            kind,
        });
        seg.order.push(seg.springs.len() - 1);
        seg.active += 1;
        Some(seg.springs.len() - 1)
    }

    /// `CFenceHandler::BuildPost` (0x602fe0).
    fn build_post(&mut self, dummy: usize) -> usize {
        let t = TYPES[self.post_type];
        let mut seg = self.new_segment(dummy, true);
        let d = self.posts[dummy].pos;
        let row_step = t.height / (t.rows - 1) as f32;
        for layer in 0..t.layers {
            for r in 0..t.rows {
                let y = (1.0 - t.layer_step * layer as f32) - r as f32 * row_step + d.y;
                self.add_particle(&mut seg, Vec3::new(d.x, y, d.z), 0.1);
            }
        }
        let top_pos = d + Vec3::new(0.0, POST_HEIGHT, 0.0);
        let top = self.add_particle(&mut seg, top_pos, 0.1);
        seg.pins.push(top);
        self.add_particle(&mut seg, top_pos, 0.0);
        self.add_particle(&mut seg, d, 0.0);
        self.add_spring(&mut seg, top, top + 1, 1, 0.1);
        if let Some(s) = self.add_spring(&mut seg, top, top + 2, 0, 1.0) {
            seg.fixed.push(s);
            for p in seg.start..seg.start + (t.layers * t.rows) as usize {
                let spring = seg.springs[s];
                let (a, b) = (self.particles[spring.a].pos, self.particles[spring.b].pos);
                let inv = 1.0 / (b - a).length();
                let proj = (self.particles[p].pos - a).dot((b - a) * inv) * inv;
                seg.bends.push(Bend { spring: s, p, proj });
            }
        }
        self.segments.push(seg);
        self.segments.len() - 1
    }

    /// `CFenceHandler::BuildSpan` (0x6029b0): from post `dummy` to the next, between segments `prev` and `next`.
    fn build_span(&mut self, dummy: usize, prev: usize, next: usize) -> Segment {
        let t = TYPES[self.span_type];
        let (cols, rows, layers) = (t.cols as usize, t.rows as usize, t.layers as usize);
        let mut seg = self.new_segment(dummy, false);
        if cols < 3 || rows < 2 {
            return seg;
        }
        let inner = cols - 2;
        let (a, b) = (self.posts[dummy].pos, self.posts[dummy + 1].pos);
        let step = 1.0 / (cols as f32 - 1.0);
        let dy = (b.y - a.y) * step;
        let (dx, dz) = (b.x - a.x, b.z - a.z);
        let row_step = -t.height / (rows - 1) as f32;
        let (prev_start, next_start) = (self.segments[prev].start, self.segments[next].start);
        let (prev_count, next_count) = (self.segments[prev].count, self.segments[next].count);
        for layer in 0..layers {
            let (mut x, mut z) = (a.x, a.z);
            let mut y = (1.0 - layer as f32 * t.layer_step) + a.y + dy;
            let mut last = 0;
            for _ in 1..cols - 1 {
                z += dz * step;
                x += step * dx;
                for r in 0..rows {
                    last = self.add_particle(&mut seg, Vec3::new(x, row_step * r as f32 + y, z), 1.0);
                    seg.pins.push(last);
                }
                y += dy;
            }
            let base = seg.start + layer * inner * rows;
            let mut q = base;
            for _ in 1..inner {
                for _ in 0..rows - 1 {
                    self.add_spring(&mut seg, q, q + 1, t.flag, 1.0);
                    self.add_spring(&mut seg, q, q + rows, 0, 1.0);
                    self.add_spring(&mut seg, q, q + rows + 1, 0, 1.0);
                    self.add_spring(&mut seg, q + 1, q + rows, 0, 1.0);
                    q += 1;
                }
                q += 1;
            }
            let mut q = base + rows - 1;
            for _ in 1..inner {
                self.add_spring(&mut seg, q, q + rows, 0, 1.0);
                q += rows;
            }
            let last_column = base + (inner - 1) * rows;
            for q in last_column..last_column + rows - 1 {
                self.add_spring(&mut seg, q, q + 1, 0, 1.0);
            }
            // The post columns: the same layer's rows of the posts at both ends.
            if (layer + 1) * rows > prev_count.min(next_count) {
                continue;
            }
            let pa = prev_start + layer * rows;
            let pb = next_start + layer * rows;
            let lp = last + 1 - rows;
            for r in 0..rows - 1 {
                self.add_spring(&mut seg, pa + r, base + r, 0, 1.0);
                self.add_spring(&mut seg, pa + r, base + r + 1, 0, 1.0);
                self.add_spring(&mut seg, pa + r + 1, base + r, 0, 1.0);
                self.add_spring(&mut seg, pb + r, lp + r, 0, 1.0);
                self.add_spring(&mut seg, pb + r, lp + r + 1, 0, 1.0);
                self.add_spring(&mut seg, pb + r + 1, lp + r, 0, 1.0);
            }
            self.add_spring(&mut seg, pa + rows - 1, base + rows - 1, 0, 1.0);
            self.add_spring(&mut seg, pb + rows - 1, lp + rows - 1, 0, 1.0);
        }
        seg
    }
}
