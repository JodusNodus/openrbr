//! Tyre (skid) marks, as a pure model: the emission of `E83System::EmitTire` (`0x5e3050`), the four rings of marks
//! (`E83TrackRing`, `0x7d6d48`) and their fade. `notes/marks-sound-ghost.md` section 1 has the evidence: 1.2 emission,
//! 1.3 storage, 1.4 fade, 1.5 surface. No GPU or file access here; [`crate::skidmarks_pass`] draws the [`Quad`]s.
//!
//! The inputs are each wheel's `CSigTireParticles` payload of the step ([`TyreParticles`]): the live process's PTC1
//! record, or the same values made from the recorded poses in playback ([`crate::particles`]). One [`SkidMarks::step`]
//! takes one step's payloads and the step's time. Each wheel lays at most one open mark at a time: a mark is extended
//! while the wheel keeps touching and its accumulated time stays at or under the record's flush threshold, and a new
//! mark starts past it. A wheel that lays nothing in a step closes its open mark, so the next mark starts a new trail.
//!
//! The cadence: `EmitTire` adds the last rendered frame's time (`E83System` +0x94, which `Update` 0x5e2850 stores) to
//! the wheel's accumulator at each payload, one a physics step; the caller passes the game frame it draws at
//! (`particles::GAME_FRAME`, ours: 60 frames a second). The texture coordinate along the trail is `EmitTire`'s
//! (`0x5e9120` and `0x5e9010`, mark +0x3c): the last mark's plus twice the distance between the marks over the new
//! mark's width. Ours: the fade is computed from the mark's rank in its ring when drawn; the original stores the same
//! ramp when a mark starts (1.4).

use rbr_formats::skidmarks::{Record, Skidmarks};
use rbr_recording::particles::TyreParticles;

/// Wheels with a ring.
pub const WHEELS: usize = 4;
/// Marks per ring: `0x100` marks of `0x40` bytes (`notes/marks-sound-ghost.md` 1.3).
pub const RING_MARKS: usize = 256;
/// Marks from the oldest that fade in: the ramp is `(rank + 1) / FADE_MARKS` for those (1.4).
pub const FADE_MARKS: usize = 32;
/// Lift of both edge points above the contact point, m (`EmitTire`'s `0.03`).
pub const LIFT: f32 = 0.03;
/// The least alpha a snow mark is drawn with, ours: the Arctic record is white with a general add of 0.075, which is
/// invisible on white snow, so its alpha is raised to this and no higher. Other surfaces keep the data's intensity.
pub const MIN_ALPHA: f32 = 0.3;
/// The label of the snow record, ours: the only snow surface in the retail data is the one the particle file names
/// `"Arctic"` (`PS_Arctic.ini`). The data has no surface class to read instead.
pub const SNOW_LABEL: &str = "Arctic";

/// One mark: a cross-section of a trail, between its two edge points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub colour: [f32; 3],
    /// The intensity `k`, clamped to 1 (the colour's alpha, `EmitTire`).
    pub alpha: f32,
    /// The record's texture index.
    pub texture: u32,
    /// Closed marks end a trail; a ring starts with all its marks closed, as the original's zeroed marks are.
    pub closed: bool,
}

impl Default for Mark {
    fn default() -> Self {
        Mark { a: [0.0; 3], b: [0.0; 3], colour: [0.0; 3], alpha: 0.0, texture: 0, closed: true }
    }
}

/// One wheel's ring: `RING_MARKS` marks, and the slot the next one goes to.
#[derive(Debug, Clone, PartialEq)]
struct Ring {
    marks: Vec<Mark>,
    next: usize,
    filled: usize,
}

impl Ring {
    fn new() -> Ring {
        Ring { marks: vec![Mark::default(); RING_MARKS], next: 0, filled: 0 }
    }

    /// The newest mark, if any has been started.
    fn newest_mut(&mut self) -> Option<&mut Mark> {
        (self.filled > 0).then(|| {
            let at = (self.next + RING_MARKS - 1) % RING_MARKS;
            &mut self.marks[at]
        })
    }

    fn push(&mut self, mark: Mark) {
        self.marks[self.next] = mark;
        self.next = (self.next + 1) % RING_MARKS;
        self.filled = (self.filled + 1).min(RING_MARKS);
    }

    /// The filled marks, oldest first, with their rank from the oldest.
    fn oldest_first(&self) -> impl Iterator<Item = (usize, &Mark)> {
        let start = if self.filled < RING_MARKS { 0 } else { self.next };
        (0..self.filled).map(move |rank| (rank, &self.marks[(start + rank) % RING_MARKS]))
    }
}

/// The fade of a mark at `rank` from the oldest, in a ring of `filled` marks (1.4): the oldest [`FADE_MARKS`] ramp up,
/// the rest are full. Ours for a ring under [`FADE_MARKS`] marks: the ramp spans the marks there are, so the newest is
/// full (the notes' open question, "ring fade for a non-full ring").
pub fn fade(rank: usize, filled: usize) -> f32 {
    let ramp = FADE_MARKS.min(filled).max(1);
    if rank < ramp {
        (rank + 1) as f32 / ramp as f32
    } else {
        1.0
    }
}

/// One piece of a trail: the quad between two consecutive marks of the same trail. `a0`/`b0` and `a1`/`b1` are the
/// marks' edge points, `v0` and `v1` their texture coordinates along the trail, and the colours carry the fade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    pub a0: [f32; 3],
    pub b0: [f32; 3],
    pub a1: [f32; 3],
    pub b1: [f32; 3],
    /// RGBA at each end: the mark's colour, alpha times its fade.
    pub rgba0: [f32; 4],
    pub rgba1: [f32; 4],
    pub v0: f32,
    pub v1: f32,
    pub texture: u32,
}

/// The skid marks of one stage, as the rings of the four wheels.
#[derive(Debug, Clone, PartialEq)]
pub struct SkidMarks {
    table: Skidmarks,
    rings: Vec<Ring>,
    /// Accumulated time per wheel: the dash cadence (`EmitTire`'s `+0x98`).
    accumulated: [f32; WHEELS],
}

impl SkidMarks {
    /// Marks laid from the stage's skid table (`Skidmarks`, the `PS_*.ini` of the stage).
    pub fn new(table: Skidmarks) -> SkidMarks {
        SkidMarks { table, rings: (0..WHEELS).map(|_| Ring::new()).collect(), accumulated: [0.0; WHEELS] }
    }

    /// One physics step: its wheels' payloads, and the game frame's time the accumulators take. A wheel with no payload that lays a mark in this
    /// step closes its open mark.
    pub fn step(&mut self, payloads: &[TyreParticles], dt: f32) {
        let mut laid = [false; WHEELS];
        for payload in payloads {
            let wheel = payload.wheel as usize;
            if wheel < WHEELS {
                laid[wheel] |= self.emit(payload, dt);
            }
        }
        for (wheel, laid) in laid.iter().enumerate() {
            if !laid {
                if let Some(mark) = self.rings[wheel].newest_mut() {
                    mark.closed = true;
                }
            }
        }
    }

    /// `E83System::EmitTire` for one payload: returns whether it laid (or extended) a mark.
    fn emit(&mut self, payload: &TyreParticles, dt: f32) -> bool {
        if !payload.contact {
            return false;
        }
        let Some(record) = self.table.record(self.table.alias(payload.surface())) else {
            return false;
        };
        let k = intensity(record, payload);
        if k.is_nan() || k <= 0.0 {
            return false;
        }
        let (a, b) = edges(payload, record.size);
        let wheel = payload.wheel as usize;
        self.accumulated[wheel] += dt;
        let ring = &mut self.rings[wheel];
        if self.accumulated[wheel] <= record.flush {
            if let Some(mark) = ring.newest_mut().filter(|mark| !mark.closed) {
                mark.a = a;
                mark.b = b;
                return true;
            }
        }
        self.accumulated[wheel] -= record.flush;
        let alpha = if record.label == SNOW_LABEL { k.clamp(MIN_ALPHA, 1.0) } else { k.min(1.0) };
        ring.push(Mark { a, b, colour: record.rgb, alpha, texture: record.texture, closed: false });
        true
    }

    /// The quads of every trail in every ring: consecutive marks of one trail, oldest first, with each mark's fade.
    /// A trail breaks at a closed mark and at a change of texture.
    pub fn quads(&self) -> Vec<Quad> {
        let mut out = Vec::new();
        for ring in &self.rings {
            let marks: Vec<(usize, Mark)> = ring.oldest_first().map(|(rank, m)| (rank, *m)).collect();
            let mut v = 0.0;
            for pair in marks.windows(2) {
                let ((rank0, m0), (rank1, m1)) = (pair[0], pair[1]);
                if m0.closed || m0.texture != m1.texture {
                    v = 0.0;
                    continue;
                }
                let (mid0, mid1) = (midpoint(&m0), midpoint(&m1));
                let v0 = v;
                v += 2.0 * distance(mid0, mid1) / distance(m1.a, m1.b).max(1e-3);
                out.push(Quad {
                    a0: m0.a,
                    b0: m0.b,
                    a1: m1.a,
                    b1: m1.b,
                    rgba0: rgba(&m0, rank0, ring.filled),
                    rgba1: rgba(&m1, rank1, ring.filled),
                    v0,
                    v1: v,
                    texture: m0.texture,
                });
            }
        }
        out
    }

    /// Clears every ring and accumulator, keeping the table.
    pub fn reset(&mut self) {
        self.rings = (0..WHEELS).map(|_| Ring::new()).collect();
        self.accumulated = [0.0; WHEELS];
    }

    /// The marks of one wheel's ring, oldest first, for tests and captures.
    pub fn marks(&self, wheel: usize) -> Vec<Mark> {
        self.rings[wheel].oldest_first().map(|(_, m)| *m).collect()
    }
}

/// The intensity `k` of a payload (1.2 step 4): the dirt, depth and rubber terms, each with its reduction and
/// multiplier, and the general add.
fn intensity(record: &Record, payload: &TyreParticles) -> f32 {
    (payload.dirt + record.dirt_reduction) * record.dirt_multiplier
        + (payload.depth + record.depth_reduction) * record.depth_multiplier
        + (payload.rubber + record.rubber_reduction) * record.rubber_multiplier
        + record.general
}

/// The two edge points of the mark (1.2 step 6): each edge scaled about the midpoint by the size modifier, and lifted.
fn edges(payload: &TyreParticles, size: f32) -> ([f32; 3], [f32; 3]) {
    let [p0, p1] = payload.edges;
    let mid = midpoint_of(p0, p1);
    let about = |p: [f32; 3]| {
        let mut q = [0.0; 3];
        for i in 0..3 {
            q[i] = (p[i] - mid[i]) * size + mid[i];
        }
        q[2] += LIFT;
        q
    };
    (about(p0), about(p1))
}

fn midpoint_of(p0: [f32; 3], p1: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (p0[i] + p1[i]) * 0.5)
}

fn midpoint(mark: &Mark) -> [f32; 3] {
    midpoint_of(mark.a, mark.b)
}

fn distance(p: [f32; 3], q: [f32; 3]) -> f32 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

fn rgba(mark: &Mark, rank: usize, filled: usize) -> [f32; 4] {
    [mark.colour[0], mark.colour[1], mark.colour[2], mark.alpha * fade(rank, filled)]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tarmac-like record: flush 0.3, width 1, no rubber or depth, the dirt term from `dirt`.
    fn record(material: u32, texture: u32, general: f32, dirt_multiplier: f32) -> Record {
        Record {
            material,
            label: String::new(),
            texture,
            rgb: [0.5, 0.25, 0.125],
            size: 1.0,
            flush: 0.3,
            general,
            rubber_reduction: 0.0,
            rubber_multiplier: 0.0,
            depth_reduction: 0.0,
            depth_multiplier: 0.0,
            dirt_reduction: 0.0,
            dirt_multiplier,
        }
    }

    fn table(records: Vec<Record>, aliases: Vec<(u32, u32)>) -> Skidmarks {
        Skidmarks { records, aliases }
    }

    /// A payload on `wheel` at `x` (the contact patch is 0.2 m across, along y), on surface `material`.
    fn payload(wheel: u32, x: f32, material: u32) -> TyreParticles {
        TyreParticles {
            wheel,
            contact: true,
            edges: [[x, -0.1, 0.0], [x, 0.1, 0.0]],
            dirt: 0.5,
            surfaces: [material, material],
            ..TyreParticles::default()
        }
    }

    #[test]
    fn a_touching_wheel_lays_a_mark_and_the_flush_threshold_extends_it() {
        let mut marks = SkidMarks::new(table(vec![record(17, 3, 0.0, 1.0)], vec![]));
        marks.step(&[payload(0, 0.0, 17)], 0.2);
        marks.step(&[payload(0, 0.1, 17)], 0.2);
        let ring = marks.marks(0);
        assert_eq!(ring.len(), 1, "the second step is under the flush threshold, so it extends the mark");
        assert_eq!(ring[0].a, [0.1, -0.1, 0.03], "the extension moves the mark's edges to the new pair");
        assert_eq!(ring[0].b, [0.1, 0.1, 0.03]);
        assert!(!ring[0].closed);
        // Past the threshold again, the accumulated time starts a new mark.
        marks.step(&[payload(0, 0.2, 17)], 0.25);
        assert_eq!(marks.marks(0).len(), 2);
    }

    #[test]
    fn only_the_snow_record_is_raised_to_the_visible_alpha() {
        let snow = Record { label: SNOW_LABEL.to_string(), ..record(17, 3, 0.075, 0.0) };
        let mut marks = SkidMarks::new(table(vec![snow], vec![]));
        marks.step(&[payload(0, 0.0, 17)], 0.5);
        assert_eq!(marks.marks(0)[0].alpha, MIN_ALPHA, "k 0.075 on snow is drawn at the floor");
        let gravel = Record { label: "Gravel".to_string(), ..record(17, 3, 0.075, 0.0) };
        let mut marks = SkidMarks::new(table(vec![gravel], vec![]));
        marks.step(&[payload(0, 0.0, 17)], 0.5);
        assert_eq!(marks.marks(0)[0].alpha, 0.075, "other surfaces keep the data's intensity");
    }

    #[test]
    fn intensity_is_clamped_and_zero_lays_nothing() {
        let mut marks = SkidMarks::new(table(vec![record(17, 3, 0.0, 4.0)], vec![]));
        marks.step(&[payload(0, 0.0, 17)], 0.5);
        assert_eq!(marks.marks(0)[0].alpha, 1.0, "k = 2.0 is clamped to 1");
        let mut none = SkidMarks::new(table(vec![record(17, 3, 0.0, 0.0)], vec![]));
        none.step(&[payload(0, 0.0, 17)], 0.5);
        assert!(none.marks(0).is_empty(), "k = 0 lays no mark");
    }

    #[test]
    fn no_contact_or_no_record_lays_nothing() {
        let mut marks = SkidMarks::new(table(vec![record(17, 3, 0.0, 1.0)], vec![]));
        marks.step(&[TyreParticles { contact: false, ..payload(1, 0.0, 17) }], 0.5);
        marks.step(&[payload(1, 0.0, 99)], 0.5);
        assert!(marks.marks(1).is_empty());
    }

    #[test]
    fn surface_goes_through_the_skid_alias_before_the_record() {
        // 105 is aliased to 17, so the payload on 105 uses the record of 17.
        let mut marks = SkidMarks::new(table(vec![record(17, 9, 0.0, 1.0)], vec![(105, 17)]));
        marks.step(&[payload(2, 0.0, 105)], 0.5);
        assert_eq!(marks.marks(2)[0].texture, 9);
    }

    #[test]
    fn a_step_without_a_mark_closes_the_open_one() {
        let mut marks = SkidMarks::new(table(vec![record(17, 3, 0.0, 1.0)], vec![]));
        marks.step(&[payload(0, 0.0, 17)], 0.5);
        marks.step(&[], 0.5);
        assert!(marks.marks(0)[0].closed);
        // The next touch starts a new mark, not an extension of the closed one.
        marks.step(&[payload(0, 1.0, 17)], 0.01);
        assert_eq!(marks.marks(0).len(), 2);
    }

    #[test]
    fn the_ring_overwrites_the_oldest_mark_and_fades_the_oldest_32() {
        let mut marks = SkidMarks::new(table(vec![record(17, 3, 0.0, 1.0)], vec![]));
        for i in 0..(RING_MARKS + 5) {
            // Each step's time passes the flush threshold, so every payload starts a new mark.
            marks.step(&[payload(3, i as f32, 17)], 1.0);
        }
        let ring = marks.marks(3);
        assert_eq!(ring.len(), RING_MARKS);
        assert_eq!(ring[0].a[0], 5.0, "the oldest is the fifth mark: the first five were overwritten");
        let quads = marks.quads();
        let on_wheel_3: Vec<&Quad> = quads.iter().filter(|q| q.texture == 3).collect();
        assert!(!on_wheel_3.is_empty());
        assert!(
            (on_wheel_3[0].rgba0[3] - 0.5 / 32.0).abs() < 1e-6,
            "the oldest mark is the faintest: k 0.5 at fade 1/32"
        );
        assert_eq!(fade(31, RING_MARKS), 1.0);
        assert_eq!(fade(40, RING_MARKS), 1.0);
    }

    #[test]
    fn a_ring_under_32_marks_ramps_over_the_marks_there_are() {
        assert_eq!(fade(0, 256), 1.0 / 32.0, "a full ring's oldest is the faintest");
        assert_eq!(fade(0, 3), 1.0 / 3.0, "a short ring's oldest is 1/3");
        assert_eq!(fade(2, 3), 1.0, "its newest is full");
        assert_eq!(fade(0, 1), 1.0, "a single mark is full");
    }

    #[test]
    fn a_trail_is_a_run_of_quads_broken_by_closes_and_textures() {
        let mut marks = SkidMarks::new(table(vec![record(17, 3, 0.0, 1.0), record(18, 4, 0.0, 1.0)], vec![]));
        marks.step(&[payload(0, 0.0, 17)], 0.5);
        marks.step(&[payload(0, 1.0, 17)], 0.5);
        marks.step(&[payload(0, 2.0, 18)], 0.5);
        let quads = marks.quads();
        assert_eq!(quads.len(), 1, "the texture change breaks the trail, leaving one quad before it");
        assert_eq!(quads[0].texture, 3);
        assert!(quads[0].v1 > quads[0].v0);
    }
}
