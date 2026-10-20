//! Tyre marks (renderer addition `marks`, ours, not the game's look), as a pure model: per wheel, a trail of
//! cross-sections laid from the same per-step [`TyreParticles`] payloads the 1.02 marks take ([`crate::skidmarks`]).
//! [`crate::skidmarks_pass`] draws the [`Quad`]s (`tyre_marks.wgsl`).
//!
//! - Width: a real rally tyre's tread ([`TREAD_LOOSE`], [`TREAD_TARMAC`]) swept along the direction of travel, so a
//!   sliding tyre leaves a wider mark: `w |cos a| + L |sin a|`, `a` the slip angle between the wheel's heading (across
//!   the payload's edges, which lie along the axle) and its travel (the patch centre's motion between payloads; the
//!   payload's velocity is the particles' launch velocity, not the patch's), `L` the contact patch's length.
//! - What is laid: gravel, dirt, grass and snow keep a mark of every rolling tyre; tarmac only of a sliding, locking or
//!   spinning one, by the slip angle and the payload's rubber (+0x80; the game's payloads only: playback's from the
//!   poses has a slip speed there, mostly over its 0-1 range when rolling).
//! - Trails: a cross-section is fixed every [`MAX_SPACING`], or sooner where the direction, slide or strength change;
//!   the newest follows the wheel. Each fades out over its wheel's last [`FADE_LENGTH`] of [`TRAIL_LENGTH`] travelled.

use std::collections::VecDeque;

use rbr_recording::particles::TyreParticles;

use crate::particles::wheel_spray::ground::{self, smoothstep, Kind as Ground, Material};

/// Wheels with a trail.
pub const WHEELS: usize = 4;
/// Tread width on loose ground and snow, and on tarmac, m (a gravel rally tyre 0.20-0.21 m, a tarmac one ~0.235 m).
pub const TREAD_LOOSE: f32 = 0.21;
pub const TREAD_TARMAC: f32 = 0.235;
/// The contact patch's length, m: the width a tyre sliding sideways sweeps.
pub const PATCH_LENGTH: f32 = 0.16;
/// Lift of the trail above the contact points, m (the pass also biases its depth).
pub const LIFT: f32 = 0.03;
/// The longest and the shortest distance between fixed cross-sections, m.
pub const MAX_SPACING: f32 = 1.0;
pub const MIN_SPACING: f32 = 0.1;
/// The travel a wheel's trail reaches back, and the last part of it fading out, m.
pub const TRAIL_LENGTH: f64 = 800.0;
pub const FADE_LENGTH: f64 = 250.0;
/// A patch moving further than this between two payloads has jumped (a reset, a recovery): the trail breaks.
pub const JUMP: f32 = 2.0;
/// The texture coordinate along a trail wraps at this many metres (the shader's patterns repeat within it).
pub const ALONG_WRAP: f64 = 64.0;
/// The slip angle's sine where a tyre starts and ends sliding (3 and 20 degrees).
const SLIDE_SINE: [f32; 2] = [0.052, 0.342];
/// The slide where tarmac starts and is fully marked.
const TARMAC_SLIDE: [f32; 2] = [0.3, 0.85];

/// The ground a mark is laid on; its number is the shader's (`tyre_marks.wgsl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// Bushes, trees, rocks, walls, water: no mark.
    #[default]
    None,
    /// Gravel, dirt and rough ground: a rut.
    Loose,
    Tarmac,
    Snow,
    Grass,
}

impl Kind {
    /// The shader's number of the kind.
    pub fn id(self) -> f32 {
        match self {
            Kind::None | Kind::Loose => 0.0,
            Kind::Tarmac => 1.0,
            Kind::Snow => 2.0,
            Kind::Grass => 3.0,
        }
    }

    /// How far a mark's quad reaches past the tyre's edge, in half tread widths: the berms of pushed gravel and snow,
    /// the soft edge elsewhere.
    pub fn spread(self) -> f32 {
        match self {
            Kind::Loose | Kind::Snow => 1.5,
            Kind::None | Kind::Tarmac | Kind::Grass => 1.15,
        }
    }

    pub fn tread(self) -> f32 {
        if self == Kind::Tarmac {
            TREAD_TARMAC
        } else {
            TREAD_LOOSE
        }
    }
}

/// The mark kind and the material of surface id `id` ([`ground::material`]).
fn material(id: u32) -> (Kind, Material) {
    let m = ground::material(id);
    let kind = match m.kind {
        Ground::Gravel | Ground::Dirt => Kind::Loose,
        Ground::Grass => Kind::Grass,
        Ground::Tarmac => Kind::Tarmac,
        Ground::Snow => Kind::Snow,
        Ground::None | Ground::Water => Kind::None,
    };
    (kind, m)
}

/// How a cross-section looks: the shader's inputs at one end of a quad.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Look {
    pub kind: Kind,
    pub wet: f32,
    pub loose: f32,
    /// 0 rolling to 1 sliding, locked or spinning.
    pub slide: f32,
    /// How strong the mark is, 0 to 1.
    pub strength: f32,
    /// The swept width, m.
    pub width: f32,
    /// The fade by the travel since it was laid, 1 new to 0 gone.
    pub fade: f32,
}

/// One cross-section of a trail.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Section {
    centre: [f32; 3],
    /// From the centre to the edge of the swept width, at right angles to the travel, following the ground's slope
    /// across.
    half: [f32; 3],
    /// The wheel's travel when laid, m.
    along: f64,
    look: Look,
    /// The last section of its trail.
    closed: bool,
}

/// One wheel's trail: its sections, oldest first, the newest following the wheel while open.
#[derive(Debug, Clone, Default, PartialEq)]
struct Trail {
    sections: VecDeque<Section>,
    open: bool,
    travel: f64,
    last: Option<[f32; 3]>,
}

/// One piece of a trail between two consecutive cross-sections: `a`/`b` the edges of the swept width (its berms or
/// soft edge reach [`Kind::spread`] times as far), `v` the metres along the trail ([`ALONG_WRAP`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub a0: [f32; 3],
    pub b0: [f32; 3],
    pub a1: [f32; 3],
    pub b1: [f32; 3],
    pub v0: f32,
    pub v1: f32,
    pub look0: Look,
    pub look1: Look,
}

/// The tyre marks of a drive, as the trails of its four wheels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TyreMarks {
    trails: [Trail; WHEELS],
}

impl TyreMarks {
    pub fn new() -> TyreMarks {
        TyreMarks::default()
    }

    /// One physics step's payloads; `rubber` when their rubber is the game's ([`TyreParticles::rubber`], 0 to 1). A
    /// wheel off the ground or on ground that keeps no mark ends its trail; one without a payload keeps it (the game
    /// skips a wheel's payload in some steps), up to a [`JUMP`].
    pub fn step(&mut self, payloads: &[TyreParticles], rubber: bool) {
        for payload in payloads {
            if let Some(trail) = self.trails.get_mut(payload.wheel as usize) {
                trail.step(payload, rubber);
            }
        }
    }

    /// The quads of every trail, oldest first.
    pub fn quads(&self) -> Vec<Quad> {
        let mut out = Vec::new();
        for trail in &self.trails {
            let fade = |s: &Section| {
                let behind = trail.travel - s.along;
                Look { fade: ((TRAIL_LENGTH - behind) / FADE_LENGTH).clamp(0.0, 1.0) as f32, ..s.look }
            };
            for (s0, s1) in trail.sections.iter().zip(trail.sections.iter().skip(1)) {
                if s0.closed {
                    continue;
                }
                let v0 = s0.along.rem_euclid(ALONG_WRAP);
                let edge = |s: &Section, sign: f32| std::array::from_fn(|i| s.centre[i] + sign * s.half[i]);
                out.push(Quad {
                    a0: edge(s0, -1.0),
                    b0: edge(s0, 1.0),
                    a1: edge(s1, -1.0),
                    b1: edge(s1, 1.0),
                    v0: v0 as f32,
                    v1: (v0 + s1.along - s0.along) as f32,
                    look0: fade(s0),
                    look1: fade(s1),
                });
            }
        }
        out
    }

    /// Clears every trail.
    pub fn reset(&mut self) {
        *self = TyreMarks::default();
    }
}

impl Trail {
    fn step(&mut self, payload: &TyreParticles, rubber: bool) {
        let centre = payload.centre();
        let moved = self.last.map(|last| [centre[0] - last[0], centre[1] - last[1]]);
        let step = moved.map_or(0.0, |[x, y]| (x * x + y * y).sqrt());
        if step > JUMP {
            self.close();
            self.last = Some(centre);
            return;
        }
        self.travel += f64::from(step);
        self.last = Some(centre);
        let Some(section) = section(payload, moved.filter(|_| step > 1e-4), rubber, self.travel) else {
            self.close();
            return;
        };
        let fixed = self.sections.len().checked_sub(2).map(|i| self.sections[i]);
        match (self.open, self.sections.back_mut(), fixed) {
            (true, Some(head), Some(fixed)) if head.look.kind == section.look.kind => {
                if apart(&fixed, head) {
                    self.sections.push_back(section);
                } else {
                    *head = section;
                }
            }
            _ => {
                self.close();
                // A trail starts at the wheel with its first section fixed and the second following it.
                self.sections.push_back(section);
                self.sections.push_back(section);
                self.open = true;
            }
        }
        while self.sections.front().is_some_and(|s| self.travel - s.along > TRAIL_LENGTH) {
            self.sections.pop_front();
        }
    }

    fn close(&mut self) {
        if let (true, Some(head)) = (self.open, self.sections.back_mut()) {
            head.closed = true;
        }
        self.open = false;
    }
}

/// Whether the newest section has moved far enough from the last fixed one to be fixed itself.
fn apart(fixed: &Section, head: &Section) -> bool {
    let d = head.along - fixed.along;
    if d >= f64::from(MAX_SPACING) {
        return true;
    }
    if d < f64::from(MIN_SPACING) {
        return false;
    }
    let unit = |h: [f32; 3]| {
        let l = (h[0] * h[0] + h[1] * h[1]).sqrt().max(1e-6);
        [h[0] / l, h[1] / l]
    };
    let (p, q) = (unit(fixed.half), unit(head.half));
    let turned = (p[0] * q[1] - p[1] * q[0]).abs() > 0.05;
    let (a, b) = (fixed.look, head.look);
    turned || (a.slide - b.slide).abs() > 0.1 || (a.strength - b.strength).abs() > 0.1
}

/// The cross-section a payload lays, or `None` where it lays no mark. `moved` is the patch centre's motion since the
/// last payload (none for the first, or standing).
fn section(payload: &TyreParticles, moved: Option<[f32; 2]>, rubber: bool, along: f64) -> Option<Section> {
    if !payload.contact {
        return None;
    }
    let (kind, material) = material(payload.surface());
    if kind == Kind::None {
        return None;
    }
    let [e0, e1] = payload.edges;
    let axle = [e1[0] - e0[0], e1[1] - e0[1], e1[2] - e0[2]];
    let axle_length = (axle[0] * axle[0] + axle[1] * axle[1]).sqrt();
    if axle_length < 1e-4 {
        return None;
    }
    let axle_unit = [axle[0] / axle_length, axle[1] / axle_length];
    let heading = [-axle_unit[1], axle_unit[0]];
    let travel = moved.map_or(heading, |[x, y]| {
        let l = (x * x + y * y).sqrt();
        [x / l, y / l]
    });
    let cos = (heading[0] * travel[0] + heading[1] * travel[1]).abs();
    let sin = (heading[0] * travel[1] - heading[1] * travel[0]).abs();
    let tread = kind.tread();
    let width = tread * cos + PATCH_LENGTH * sin;
    let slide = smoothstep(SLIDE_SINE, sin).max(if rubber { payload.rubber.clamp(0.0, 1.0) } else { 0.0 });
    let strength = match kind {
        // Rubber on dry tarmac when sliding; little on wet; gravel on tarmac swept off by a rolling tyre too.
        Kind::Tarmac => {
            let rubber = smoothstep(TARMAC_SLIDE, slide) * (1.0 - 0.6 * material.wet);
            rubber.max(0.4 * material.loose)
        }
        Kind::Loose | Kind::Snow | Kind::Grass => (0.6 + 0.4 * material.loose) * (0.85 + 0.15 * slide),
        Kind::None => 0.0,
    };
    if strength < 0.02 {
        return None;
    }
    // At right angles to the travel, rising across with the ground as the axle does.
    let across = [-travel[1], travel[0]];
    let half_width = 0.5 * width * kind.spread();
    let rise = axle[2] / axle_length * (across[0] * axle_unit[0] + across[1] * axle_unit[1]);
    let centre = payload.centre();
    Some(Section {
        centre: [centre[0], centre[1], centre[2] + LIFT],
        half: [across[0] * half_width, across[1] * half_width, rise * half_width],
        along,
        look: Look { kind, wet: material.wet, loose: material.loose, slide, strength, width, fade: 1.0 },
        closed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gravel (`Gravel_Fine_Shallow_Dry`), dry tarmac (`Tarmac_Fine_Clean_Dry`), a bush (`Spret_Bush_Small`).
    const GRAVEL: u32 = 0x11;
    const TARMAC: u32 = 0x25;
    const BUSH: u32 = 0x18;

    /// A payload of `wheel` at `(x, y)` with its axle at `yaw` radians from the x axis, on `surface`.
    fn payload(wheel: u32, x: f32, y: f32, yaw: f32, surface: u32) -> TyreParticles {
        let (s, c) = yaw.sin_cos();
        TyreParticles {
            wheel,
            contact: true,
            edges: [[x - 0.1 * c, y - 0.1 * s, 0.0], [x + 0.1 * c, y + 0.1 * s, 0.0]],
            surfaces: [surface, surface],
            ..TyreParticles::default()
        }
    }

    /// The width of a trail's newest section.
    fn width(marks: &TyreMarks, wheel: usize) -> f32 {
        marks.trails[wheel].sections.back().unwrap().look.width
    }

    #[test]
    fn surfaces_by_name() {
        let kind = |id| material(id).0;
        assert_eq!(kind(GRAVEL), Kind::Loose);
        assert_eq!(kind(TARMAC), Kind::Tarmac);
        assert_eq!(kind(0x75), Kind::Snow, "SnowOnIce_Shallow");
        assert_eq!(kind(0x58), Kind::Snow, "Snowwall_Bottom, the ground along the snow walls");
        assert_eq!(kind(0x3e), Kind::None, "Snowwall");
        assert_eq!(kind(0x6b), Kind::Grass, "Grass_Medium_Wet");
        assert_eq!(kind(BUSH), Kind::None);
        assert_eq!(kind(0x9d), Kind::None, "Water_Surface");
        assert_eq!(kind(999), Kind::None);
    }

    #[test]
    fn a_rolling_tyre_lays_its_tread_width_at_right_angles_to_its_travel() {
        let mut marks = TyreMarks::new();
        // The axle along x, so the heading is along y; the wheel rolls along y.
        for i in 0..5 {
            marks.step(&[payload(0, 0.0, i as f32 * 0.2, 0.0, GRAVEL)], true);
        }
        assert!((width(&marks, 0) - TREAD_LOOSE).abs() < 1e-5);
        let half = marks.trails[0].sections.back().unwrap().half;
        assert!(half[1].abs() < 1e-6 && half[0].abs() > 0.1, "across the travel: {half:?}");
    }

    #[test]
    fn a_sliding_tyre_sweeps_a_wider_mark_across_its_travel() {
        let mut marks = TyreMarks::new();
        // Heading along y, travelling along x: fully sideways, the patch's length is swept.
        for i in 0..5 {
            marks.step(&[payload(1, i as f32 * 0.2, 0.0, 0.0, GRAVEL)], true);
        }
        assert!((width(&marks, 1) - PATCH_LENGTH).abs() < 1e-5);
        let half = marks.trails[1].sections.back().unwrap().half;
        assert!(half[0].abs() < 1e-6, "across the travel (along y): {half:?}");
        // 30 degrees of slip: w cos a + L sin a, and the slide is full.
        let mut marks = TyreMarks::new();
        let a = 30f32.to_radians();
        for i in 0..5 {
            let d = i as f32 * 0.2;
            marks.step(&[payload(1, d * a.sin(), d * a.cos(), 0.0, GRAVEL)], true);
        }
        let expected = TREAD_LOOSE * a.cos() + PATCH_LENGTH * a.sin();
        assert!((width(&marks, 1) - expected).abs() < 1e-4);
        assert_eq!(marks.trails[1].sections.back().unwrap().look.slide, 1.0);
    }

    #[test]
    fn tarmac_keeps_only_slides() {
        let mut marks = TyreMarks::new();
        for i in 0..5 {
            marks.step(&[payload(0, 0.0, i as f32 * 0.2, 0.0, TARMAC)], true);
        }
        assert!(marks.quads().is_empty(), "rolling cleanly on tarmac leaves nothing");
        // Locked or spinning: the rubber marks it.
        for i in 5..10 {
            marks.step(&[TyreParticles { rubber: 1.0, ..payload(0, 0.0, i as f32 * 0.2, 0.0, TARMAC) }], true);
        }
        let quads = marks.quads();
        assert!(!quads.is_empty());
        assert!(quads.iter().all(|q| q.look0.kind == Kind::Tarmac && q.look0.strength > 0.9));
    }

    #[test]
    fn a_trail_is_continuous_fixing_sections_by_distance() {
        let mut marks = TyreMarks::new();
        // 0.01 m a step, straight: a section fixed each MAX_SPACING, the newest at the wheel.
        for i in 0..=300 {
            marks.step(&[payload(2, 0.0, i as f32 * 0.01, 0.0, GRAVEL)], true);
        }
        let quads = marks.quads();
        assert_eq!(quads.len(), 3, "sections at 0, 1, 2 and the head at 3 m");
        for pair in quads.windows(2) {
            assert_eq!(pair[0].a1, pair[1].a0, "each quad starts where the last ended");
            assert_eq!(pair[0].v1, pair[1].v0);
        }
        assert!((quads[2].b1[1] - 3.0).abs() < 1e-4, "the head follows the wheel");
        assert!((quads[2].v1 - 3.0).abs() < 1e-4);
    }

    #[test]
    fn a_turn_fixes_sections_sooner() {
        let mut marks = TyreMarks::new();
        // A circle of 5 m radius, the axle pointing at the centre, 2 cm a step over 3 m.
        for i in 0..150 {
            let t = i as f32 * 0.02 / 5.0;
            marks.step(&[payload(0, 5.0 * t.cos(), 5.0 * t.sin(), t, GRAVEL)], true);
        }
        assert!(marks.quads().len() >= 9, "{} quads over 3 m of a tight turn", marks.quads().len());
    }

    #[test]
    fn leaving_the_ground_or_the_marking_surfaces_breaks_the_trail() {
        let mut marks = TyreMarks::new();
        let at = |i: i32, surface| payload(0, 0.0, i as f32 * 0.5, 0.0, surface);
        marks.step(&[at(0, GRAVEL)], true);
        marks.step(&[at(1, GRAVEL)], true);
        marks.step(&[at(2, BUSH)], true);
        marks.step(&[at(3, GRAVEL)], true);
        marks.step(&[at(4, GRAVEL)], true);
        marks.step(&[TyreParticles { contact: false, ..at(5, GRAVEL) }], true);
        marks.step(&[at(6, GRAVEL)], true);
        marks.step(&[], true);
        marks.step(&[at(7, GRAVEL)], true);
        // Trails at 0-1, 3-4 and 6-7 (a step without a payload keeps it) have one quad each.
        let long: Vec<Quad> = marks.quads().into_iter().filter(|q| q.v1 > q.v0).collect();
        assert_eq!(long.len(), 3);
        // A jump breaks it too.
        let mut marks = TyreMarks::new();
        marks.step(&[at(0, GRAVEL)], true);
        marks.step(&[at(10, GRAVEL)], true);
        assert!(marks.quads().iter().all(|q| q.v1 == q.v0));
    }

    #[test]
    fn old_marks_fade_by_the_travel_and_drop_off() {
        let mut marks = TyreMarks::new();
        let total = TRAIL_LENGTH + 100.0;
        for i in 0..=(total * 2.0) as i32 {
            marks.step(&[payload(3, 0.0, i as f32 * 0.5, 0.0, GRAVEL)], true);
        }
        let quads = marks.quads();
        let first = quads.first().unwrap();
        assert!(f64::from(first.a0[1]) >= total - TRAIL_LENGTH - 1.0, "older sections dropped: {}", first.a0[1]);
        assert!(first.look0.fade < 0.01, "the oldest has faded: {}", first.look0.fade);
        let newest = quads.last().unwrap();
        assert_eq!(newest.look1.fade, 1.0);
        // Halfway into the fade.
        let mid = quads.iter().find(|q| f64::from(q.a0[1]) >= total - TRAIL_LENGTH + FADE_LENGTH * 0.5).unwrap();
        assert!((mid.look0.fade - 0.5).abs() < 0.01, "{}", mid.look0.fade);
    }
}
