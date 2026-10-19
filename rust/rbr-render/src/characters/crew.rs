//! The in-car crew's pure model (rbr-recomp `notes/stage-characters.md`, section 7): the driver's steering scrub, the hand-move
//! ring for the gear and handbrake, the Hermite hand and elbow keys, the two-bone arm IK and the seat shake. No files,
//! no GPU; [`super::incar`] loads the crew and drives this.
//!
//! What follows LocoMotion (`CLM_CharacterDriver`, `CLM_CharacterIncarPeople`, LM `0x1001e800`-`0x10023fa0`):
//!
//! - **Steering** (`Unk18`, LM `0x10020cf0`): the car's `i_steeringwheel` turns by `steer * Max Steeringwheel
//!   degrees`, and the driver clip is scrubbed to `Normal * (steer + 1) / 30` s ([`steer_time`]).
//! - **Moves** (`OnCarControl`, LM `0x10020ac0`; `Unk22a20`, `Unk22030`, `Unk22060`, `Unk220b0`, `Unk22b90`): a ring
//!   of 20 `{move, direction, hold}` entries ([`Ring`]), each played for its `Animation Lengths` time; the hand and
//!   elbow follow a Hermite curve between two keys ([`hermite`], `SBlend21ea0::Blend`, LM `0x10021ea0`).
//! - **Arms** (`Unk237a0`, `Unk22380`, `Unk22ce0`, the asm): two chains (upper arm, lower arm, hand) solved towards
//!   the hand nodes every update ([`solve`], [`reach`]), the frames rebuilt with `x` along the bones and the forearm's
//!   twist node (`B_hand rotation`) keeping its own `y`. Out of reach the arm is left as it is.
//! - **Shake** (`Unk23000`, LM `0x10023000`, the asm): the five `CSigIncarShake` vectors lean `B_spine01`,
//!   `B_spine02`, `B_head` and both upper arms ([`lean`]), each term clamped to ±0.2; the hands are put back, so the
//!   IK takes the motion up. The signal's seat slide (`+0x60`) is always zero (`E45_P3210` leaves it cleared).
//!
//! Ours (the C++ of these functions is partial, and the Ghidra reading leaves gaps):
//!
//! - Replacing the entry that is playing turns its progress round (`1 - progress`), our reading of `Unk220b0`; an
//!   entry that ends hands over to the next with no carried time.

use glam::{Mat4, Vec3};
use rbr_formats::ini::Section;
use std::collections::VecDeque;

/// Entries in the move ring (`+0x3b8`, 20 of 12 bytes).
pub const RING: usize = 20;
/// `Unk18`'s frame rate: `0.033333335 = 1/30`.
pub const FRAMES_PER_SECOND: f32 = 30.0;
/// `Unk23000`'s clamp of every shake term (LM `0x1005a440/444`).
pub const SHAKE_LIMIT: f32 = 0.2;

/// `Misc/LM_Driver.ini`, with the defaults `Unk14` (LM `0x1001ea00`) passes to `CConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// `Max Steeringwheel degrees`, in radians.
    pub max_steer: f32,
    /// `Animation Frames`.
    pub normal: i32,
    pub gear_start: i32,
    pub gear: i32,
    pub gear_pushed: i32,
    pub handbrake_start: i32,
    pub handbrake: i32,
    pub handbrake_pulled: i32,
    pub gear_to_handbrake: i32,
    pub handbrake_from_gear: i32,
    pub wheel_gear_start: i32,
    pub wheel_gear: i32,
    /// `Animation Lengths`, seconds.
    pub to_gear: f32,
    pub to_gear_pushed: f32,
    pub to_handbrake: f32,
    pub to_handbrake_pulled: f32,
    pub gear_to_handbrake_length: f32,
    pub wheel_gear_length: f32,
    /// `Animation Tangents`, by key name (`Hand From Steeringwheel`, ...).
    pub tangents: Vec<(String, Vec3)>,
}

impl Settings {
    /// Reads the ini's sections; a missing key takes `Unk14`'s default.
    pub fn read(sections: &[Section]) -> Settings {
        let get = |section: &str, key: &str| {
            sections.iter().find(|s| s.name.eq_ignore_ascii_case(section)).and_then(|s| s.get(key)).map(str::trim)
        };
        let float =
            |section: &str, key: &str, default: f32| get(section, key).and_then(|v| v.parse().ok()).unwrap_or(default);
        let int =
            |key: &str, default: i32| get("Animation Frames", key).and_then(|v| v.parse().ok()).unwrap_or(default);
        let length = |key: &str, default: f32| float("Animation Lengths", key, default);
        let tangents = sections
            .iter()
            .filter(|s| s.name.eq_ignore_ascii_case("Animation Tangents"))
            .flat_map(|s| &s.entries)
            .filter_map(|(key, value)| {
                let v: Vec<f32> = value.split_whitespace().filter_map(|w| w.parse().ok()).collect();
                (v.len() == 3).then(|| (key.clone(), Vec3::new(v[0], v[1], v[2])))
            })
            .collect();
        Settings {
            max_steer: float("Settings", "Max Steeringwheel degrees", 60.0).to_radians(),
            normal: int("Normal", 5),
            gear_start: int("Gear Start", 15),
            gear: int("Gear", 20),
            gear_pushed: int("Gear Pushed", 25),
            handbrake_start: int("Handbrake Start", 30),
            handbrake: int("Handbrake", 35),
            handbrake_pulled: int("Handbrake Pulled", 40),
            gear_to_handbrake: int("Gear to Handbrake", 45),
            handbrake_from_gear: int("Handbrake from Gear", 50),
            wheel_gear_start: int("Steeringwheel Gear Start", 55),
            wheel_gear: int("Steeringwheel Gear", 60),
            to_gear: length("From Steeringwheel to Gear", 1.0),
            to_gear_pushed: length("From Gear to Gear Pushed", 0.1),
            to_handbrake: length("From Steeringwheel to Handbrake", 1.0),
            to_handbrake_pulled: length("From Handbrake to Handbrake Pulled", 0.2),
            gear_to_handbrake_length: length("From Gear to Handbrake", 0.5),
            wheel_gear_length: length("Steeringwheel Gear", 1.0),
            tangents,
        }
    }

    /// Tangent `key`, else `default` (`Unk14`'s).
    pub fn tangent(&self, key: &str, default: Vec3) -> Vec3 {
        self.tangents.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map_or(default, |(_, v)| *v)
    }

    /// The `Animation Lengths` time of `mv`, seconds.
    pub fn length(&self, mv: Move) -> f32 {
        match mv {
            Move::WheelToGear => self.to_gear,
            Move::GearPush => self.to_gear_pushed,
            Move::WheelToHandbrake => self.to_handbrake,
            Move::HandbrakePull => self.to_handbrake_pulled,
            Move::GearToHandbrake => self.gear_to_handbrake_length,
            Move::WheelGear => self.wheel_gear_length,
        }
    }

    /// The clip frames `mv` runs between (`SMove::m_start`, `+ m_frames`).
    pub fn frames(&self, mv: Move) -> (i32, i32) {
        match mv {
            Move::WheelToGear => (self.gear_start, self.gear),
            Move::GearPush => (self.gear, self.gear_pushed),
            Move::WheelToHandbrake => (self.handbrake_start, self.handbrake),
            Move::HandbrakePull => (self.handbrake, self.handbrake_pulled),
            Move::GearToHandbrake => (self.gear_to_handbrake, self.handbrake_from_gear),
            Move::WheelGear => (self.wheel_gear_start, self.wheel_gear),
        }
    }
}

/// The driver clip's time for steering `steer` (-1 full left to 1 full right): `Normal * (steer + 1) / 30`, so frames
/// `0..2 * Normal` hold full left to full right (`Unk18`, `m_unk684`).
pub fn steer_time(steer: f32, normal: i32) -> f32 {
    normal as f32 * (steer.clamp(-1.0, 1.0) + 1.0) / FRAMES_PER_SECOND
}

/// The steering wheel's turn for `steer`, radians (`m_unk67c = Max Steeringwheel degrees * steer`).
pub fn wheel_angle(steer: f32, max_steer: f32) -> f32 {
    max_steer * steer.clamp(-1.0, 1.0)
}

/// `SBlend21ea0::Blend`: the cubic Hermite from `p0` (outgoing tangent `m0`) to `p1` (incoming tangent `m1`) at `t`.
pub fn hermite(p0: Vec3, m0: Vec3, p1: Vec3, m1: Vec3, t: f32) -> Vec3 {
    let (t2, t3) = (t * t, t * t * t);
    let h00 = (t3 * 2.0 - t2 * 3.0) + 1.0;
    let h01 = t2 * 3.0 - t3 * 2.0;
    let h10 = (t3 - t2 * 2.0) + t;
    let h11 = t3 - t2;
    p0 * h00 + p1 * h01 + m0 * h10 + m1 * h11
}

/// The driver's hand moves (`m_unk4b8` / `m_unk4d4` slots 1-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// 1: the wheel to the gear stick.
    WheelToGear,
    /// 2: the gear stick to pushed.
    GearPush,
    /// 3: the wheel to the handbrake.
    WheelToHandbrake,
    /// 4: the handbrake to pulled.
    HandbrakePull,
    /// 5: the gear stick to the handbrake.
    GearToHandbrake,
    /// 6: a gear change with the hand on the wheel (cars without a gear stick).
    WheelGear,
}

/// One ring entry: `{a: move, b: backwards, c: hold at the end}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub mv: Move,
    pub back: bool,
    pub hold: bool,
}

const fn entry(mv: Move, back: bool, hold: bool) -> Entry {
    Entry { mv, back, hold }
}

/// Where the right hand is going (`m_unk4b0`: -1 the wheel, 1 the gear, 3 the handbrake).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Wheel,
    Gear,
    Handbrake,
}

/// The move ring and its player. The front entry is the one playing.
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    entries: VecDeque<Entry>,
    /// `m_unk4b4`: the playing entry's progress, 0 to 1.
    progress: f32,
    pub hand: Hand,
    /// `DAT_1006c480`: the handbrake value last acted on.
    last_handbrake: f32,
    /// The car has the gear nodes (`CLM_CharacterCar+0x2e4 == 0`).
    stick: bool,
    /// The last entry was taken off while it played: the next push replaces it.
    replacing: bool,
}

impl Ring {
    pub fn new(stick: bool) -> Ring {
        Ring {
            entries: VecDeque::new(),
            progress: 0.0,
            hand: Hand::Wheel,
            last_handbrake: 0.0,
            stick,
            replacing: false,
        }
    }

    /// The entry playing and its curve parameter (`progress`, or `1 - progress` backwards), `None` at the wheel.
    pub fn playing(&self) -> Option<(Entry, f32)> {
        let front = *self.entries.front()?;
        let p = self.progress.min(1.0);
        Some((front, if front.back { 1.0 - p } else { p }))
    }

    /// The queued entries, the playing one first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// `Unk22060`: appends an entry.
    fn push(&mut self, e: Entry) {
        if self.replacing {
            self.replacing = false;
            self.progress = 1.0 - self.progress.min(1.0);
        } else if self.entries.is_empty() {
            self.progress = 0.0;
        }
        if self.entries.len() < RING {
            self.entries.push_back(e);
        }
    }

    /// `Unk22030`: takes the last entry off (the write index steps back).
    fn drop_last(&mut self) {
        if self.entries.len() == 1 {
            self.replacing = true;
        }
        self.entries.pop_back();
    }

    /// `m_unk3b8[m_unk4ac].c = 0`: the last entry no longer holds.
    fn release_last(&mut self) {
        if let Some(last) = self.entries.back_mut() {
            last.hold = false;
        }
    }

    /// `OnCarControl` for one frame's control: `handbrake` (`+0x08`) and a gear change (`+0x14` bits 0 or 1).
    pub fn on_control(&mut self, handbrake: f32, gear_change: bool) {
        use Move::*;
        if gear_change {
            if self.stick {
                if self.hand == Hand::Handbrake {
                    self.release_last();
                    self.last_handbrake = 0.0;
                    self.hand = Hand::Gear;
                    self.drop_last();
                    for e in [
                        entry(HandbrakePull, true, false),
                        entry(GearToHandbrake, true, false),
                        entry(GearPush, false, false),
                        entry(GearPush, true, false),
                        entry(WheelToGear, true, false),
                    ] {
                        self.push(e);
                    }
                } else {
                    // `Unk22a20(1, 2)`: from the wheel (idle, or the last entry playing) the hand goes to the stick
                    // first; else the pending return to the wheel is replaced.
                    let from_wheel = self.entries.len() <= 1;
                    self.hand = Hand::Gear;
                    self.drop_last();
                    if from_wheel {
                        self.push(entry(WheelToGear, false, false));
                    }
                    for e in
                        [entry(GearPush, false, false), entry(GearPush, true, false), entry(WheelToGear, true, false)]
                    {
                        self.push(e);
                    }
                }
            } else {
                if self.hand == Hand::Handbrake {
                    self.last_handbrake = 0.0;
                    self.release_last();
                    self.hand = Hand::Gear;
                    self.drop_last();
                    self.push(entry(HandbrakePull, true, false));
                    self.push(entry(WheelToHandbrake, true, false));
                } else {
                    self.hand = Hand::Gear;
                    self.drop_last();
                }
                self.push(entry(WheelGear, false, false));
                self.push(entry(WheelGear, true, false));
            }
        }
        if handbrake == self.last_handbrake {
            return;
        }
        if handbrake == 0.0 {
            if self.hand != Hand::Handbrake {
                return;
            }
            self.release_last();
            self.drop_last();
            self.push(entry(HandbrakePull, true, false));
            self.push(entry(WheelToHandbrake, true, false));
        } else if self.hand == Hand::Gear {
            if !self.stick {
                return;
            }
            self.hand = Hand::Handbrake;
            self.drop_last();
            self.push(entry(GearToHandbrake, false, false));
            self.push(entry(HandbrakePull, false, true));
        } else {
            if self.entries.back().is_some_and(|e| e.hold) {
                return;
            }
            self.hand = Hand::Handbrake;
            self.drop_last();
            self.push(entry(WheelToHandbrake, false, false));
            self.push(entry(HandbrakePull, false, true));
        }
        self.last_handbrake = handbrake;
    }

    /// `Unk22b90`'s clock: the playing entry runs on by `dt / length`; one that has ended hands over to the next, holds,
    /// or leaves the hand at the wheel.
    pub fn advance(&mut self, dt: f32, length: impl Fn(Move) -> f32) {
        self.replacing = false;
        let Some(&front) = self.entries.front() else {
            self.progress = 0.0;
            return;
        };
        if self.progress < 1.0 {
            self.progress = (self.progress + dt / length(front.mv).max(1e-3)).min(1.0);
        } else if self.entries.len() > 1 {
            self.entries.pop_front();
            self.progress = 0.0;
        } else if !front.hold {
            self.entries.pop_front();
            self.progress = 0.0;
            self.hand = Hand::Wheel;
        }
    }
}

/// `CLM_IKChain::Solve` (LM `0x10027190`, with `Setup` 0x10026fa0): the elbow, from the shoulder, that puts a chain
/// of `upper` and `lower` long links from the shoulder to `to_target`, bending towards `to_pole`. `Setup` builds a
/// frame with `x` towards the target and `y` the pole's part across it; the elbow is `a x + h y` with
/// `a = ((upper² - lower²) / d + d) / 2` clamped to `0..=upper` and `h = sqrt(upper² - a²)`. `None` (the game leaves
/// the arm as it is) unless `0 < a < upper`: out of reach, or the target at the shoulder.
pub fn solve(to_target: Vec3, to_pole: Vec3, upper: f32, lower: f32) -> Option<Vec3> {
    let x = to_target.normalize_or_zero();
    let y = (to_pole - x * to_pole.dot(x)).normalize_or_zero();
    let d = to_target.length();
    let a = (((upper * upper - lower * lower) / d + d) * 0.5).min(upper).max(0.0);
    let h = (upper * upper - a * a).sqrt();
    (a > 0.0 && upper > a).then_some(x * a + y * h)
}

/// The frame with `x` along `x` and `y` as near `y` as an orthonormal frame allows, at `at`: `z = x × y`, then
/// `y = z × x` (`Unk22380`'s and `Unk23000`'s rebuild, D3DX's row axes as columns).
fn frame(x: Vec3, y: Vec3, at: Vec3) -> Mat4 {
    let x = x.normalize_or_zero();
    let z = x.cross(y.normalize_or_zero()).normalize_or_zero();
    let y = z.cross(x);
    Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), at.extend(1.0))
}

/// Sets node matrices in a pose (world, by node index, parents first) and moves each other node with its nearest
/// edited ancestor, as `CLM_Node::Update` passes a changed matrix down to the children.
pub fn edit(parents: &[Option<usize>], world: &mut [Mat4], edits: &[(usize, Mat4)]) {
    let mut delta: Vec<Option<Mat4>> = vec![None; world.len()];
    for i in 0..world.len() {
        let own = edits.iter().rev().find(|(k, _)| *k == i).map(|(_, m)| *m * world[i].inverse());
        delta[i] = own.or_else(|| parents.get(i).copied().flatten().and_then(|p| delta[p]));
        if let Some(d) = delta[i] {
            world[i] = d * world[i];
        }
    }
}

/// One arm chain (`Unk237a0`): node indices and the link lengths `UpdateLengths` measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arm {
    pub upper: usize,
    pub lower: usize,
    /// `B_hand rotation`, the forearm's twist node.
    pub twist: usize,
    pub hand: usize,
    pub upper_length: f32,
    pub lower_length: f32,
}

impl Arm {
    /// The chain of `upper`, `lower`, `hand` with the twist node `twist`, with its lengths in `world`.
    pub fn new(world: &[Mat4], upper: usize, lower: usize, twist: usize, hand: usize) -> Arm {
        let at = |i: usize| world[i].w_axis.truncate();
        Arm {
            upper,
            lower,
            twist,
            hand,
            upper_length: at(upper).distance(at(lower)),
            lower_length: at(lower).distance(at(hand)),
        }
    }
}

/// `Unk22380` (LM `0x10022380`): solves the arm towards its hand node's place, bending towards `pole` (the elbow
/// where it is unless a move gives its key), and rebuilds the three frames with `x` along the bones: the upper arm's
/// `y` from its own, the lower arm's from the upper arm's new one, the twist node at the elbow with the lower arm's
/// `x` and its own `y` (so the forearm takes the wrist's turn). The hand and every other node stay as they are, as in
/// the game. Returns false, changing nothing, when [`solve`] fails.
pub fn reach(world: &mut [Mat4], arm: &Arm, pole: Vec3) -> bool {
    let at = |i: usize| world[i].w_axis.truncate();
    let shoulder = at(arm.upper);
    let to_target = at(arm.hand) - shoulder;
    let Some(elbow) = solve(to_target, pole - shoulder, arm.upper_length, arm.lower_length) else { return false };
    let upper = frame(elbow, world[arm.upper].y_axis.truncate(), shoulder);
    let lower = frame(to_target - elbow, upper.y_axis.truncate(), shoulder + elbow);
    let twist = frame(lower.x_axis.truncate(), world[arm.twist].y_axis.truncate(), shoulder + elbow);
    world[arm.upper] = upper;
    world[arm.lower] = lower;
    world[arm.twist] = twist;
    true
}

/// `Unk23000`'s lean of one node by its `CSigIncarShake` vector `shake` (body frame: the model's `(x, z, y)`): each
/// term clamped to [`SHAKE_LIMIT`] (the tables at LM `0x1006c4b0`) is added to the node's `x` axis, which is
/// normalised; then `z = x × y` and `y = z × x`. The place stays.
pub fn lean(m: Mat4, shake: Vec3) -> Mat4 {
    let shake = Vec3::new(shake.x, shake.z, shake.y).clamp(Vec3::splat(-SHAKE_LIMIT), Vec3::splat(SHAKE_LIMIT));
    frame(m.x_axis.truncate() + shake, m.y_axis.truncate(), m.w_axis.truncate())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steering_scrubs_frames_zero_to_ten_and_turns_the_wheel_sixty_degrees() {
        assert_eq!(steer_time(-1.0, 5), 0.0);
        assert!((steer_time(0.0, 5) - 5.0 / 30.0).abs() < 1e-6);
        assert!((steer_time(1.0, 5) - 10.0 / 30.0).abs() < 1e-6);
        assert!((steer_time(3.0, 5) - 10.0 / 30.0).abs() < 1e-6);
        let max = 60f32.to_radians();
        assert!((wheel_angle(0.5, max) - 30f32.to_radians()).abs() < 1e-6);
        assert!((wheel_angle(-1.0, max) + max).abs() < 1e-6);
    }

    #[test]
    fn hermite_meets_its_keys_and_leaves_along_the_tangent() {
        let (p0, p1) = (Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0));
        let m = Vec3::new(0.0, 0.0, 300.0);
        assert_eq!(hermite(p0, m, p1, -m, 0.0), p0);
        assert!((hermite(p0, m, p1, -m, 1.0) - p1).length() < 1e-4);
        // Early on, the outgoing tangent dominates: the hand moves back (+z) before it moves across.
        let early = hermite(p0, m, p1, -m, 0.1);
        assert!(early.z > 20.0 && early.x < 1.0, "{early}");
    }

    fn settings() -> Settings {
        let text = "[Animation Lengths]\nFrom Steeringwheel to Gear = 0.3\nFrom Gear to Gear Pushed = 0.2\n\
                    From Steeringwheel to Handbrake = 0.3\nFrom Handbrake to Handbrake Pulled = 0.1\n\
                    From Gear to Handbrake = 0.3\n[Animation Tangents]\nHand From Steeringwheel = 0 0 300\n";
        Settings::read(&rbr_formats::ini::parse(text).unwrap())
    }

    /// Runs the ring for `seconds` in 1/100 s steps.
    fn run(ring: &mut Ring, settings: &Settings, seconds: f32) {
        for _ in 0..(seconds * 100.0).round() as usize {
            ring.advance(0.01, |m| settings.length(m));
        }
    }

    #[test]
    fn a_gear_change_goes_to_the_stick_pushes_and_comes_back() {
        let s = settings();
        assert_eq!((s.length(Move::WheelToGear), s.normal, s.max_steer), (0.3, 5, 60f32.to_radians()));
        assert_eq!(s.tangent("Hand From Steeringwheel", Vec3::ZERO), Vec3::new(0.0, 0.0, 300.0));
        let mut ring = Ring::new(true);
        ring.on_control(0.0, true);
        assert_eq!(ring.hand, Hand::Gear);
        let queued: Vec<_> = ring.entries().copied().collect();
        use Move::*;
        assert_eq!(
            queued,
            [
                entry(WheelToGear, false, false),
                entry(GearPush, false, false),
                entry(GearPush, true, false),
                entry(WheelToGear, true, false)
            ]
        );
        // 0.3 + 0.2 + 0.2 + 0.3 s, and a frame or two to hand over each.
        run(&mut ring, &s, 0.95);
        assert!(ring.playing().is_some());
        run(&mut ring, &s, 0.15);
        assert!(ring.playing().is_none());
        assert_eq!(ring.hand, Hand::Wheel);
    }

    #[test]
    fn a_second_gear_change_replaces_the_return_to_the_wheel() {
        let s = settings();
        let mut ring = Ring::new(true);
        ring.on_control(0.0, true);
        run(&mut ring, &s, 0.4);
        assert_eq!(ring.playing().map(|p| p.0.mv), Some(Move::GearPush));
        ring.on_control(0.0, true);
        let moves: Vec<_> = ring.entries().map(|e| (e.mv, e.back)).collect();
        use Move::*;
        assert_eq!(
            moves,
            [(GearPush, false), (GearPush, true), (GearPush, false), (GearPush, true), (WheelToGear, true)]
        );
    }

    #[test]
    fn the_handbrake_holds_until_released_and_turning_round_keeps_the_hand_in_place() {
        let s = settings();
        let mut ring = Ring::new(true);
        ring.on_control(1.0, false);
        assert_eq!(ring.hand, Hand::Handbrake);
        run(&mut ring, &s, 1.0);
        let (held, t) = ring.playing().unwrap();
        assert_eq!((held.mv, held.hold, t), (Move::HandbrakePull, true, 1.0));
        // Pulling harder does nothing while it holds.
        ring.on_control(0.5, false);
        assert_eq!(ring.entries().count(), 1);
        ring.on_control(0.0, false);
        let (now, t) = ring.playing().unwrap();
        assert_eq!((now.mv, now.back, t), (Move::HandbrakePull, true, 1.0));
        run(&mut ring, &s, 1.0);
        assert!(ring.playing().is_none());
        assert_eq!(ring.hand, Hand::Wheel);
    }

    #[test]
    fn a_car_without_a_stick_changes_gear_at_the_wheel() {
        let s = settings();
        let mut ring = Ring::new(false);
        ring.on_control(0.0, true);
        let moves: Vec<_> = ring.entries().map(|e| (e.mv, e.back)).collect();
        assert_eq!(moves, [(Move::WheelGear, false), (Move::WheelGear, true)]);
        // No stick: the handbrake from the gear position is ignored.
        ring.on_control(1.0, false);
        assert_eq!(ring.entries().count(), 2);
        run(&mut ring, &s, 2.1);
        assert!(ring.playing().is_none());
    }

    #[test]
    fn the_ring_holds_at_most_twenty() {
        let mut ring = Ring::new(true);
        for _ in 0..30 {
            ring.on_control(0.0, true);
            ring.on_control(1.0, false);
            ring.on_control(0.0, false);
        }
        assert!(ring.entries().count() <= RING);
    }

    #[test]
    fn ik_keeps_the_bone_lengths_and_fails_out_of_reach() {
        let (pole, target) = (Vec3::new(0.0, -1.0, 0.0), Vec3::new(1.2, 0.0, 0.0));
        let elbow = solve(target, pole, 1.0, 1.0).unwrap();
        assert!((elbow.length() - 1.0).abs() < 1e-5);
        assert!((elbow.distance(target) - 1.0).abs() < 1e-5);
        assert!(elbow.y < 0.0, "bends towards the pole: {elbow}");
        // Out of reach, or at the shoulder: the game leaves the arm.
        assert_eq!(solve(Vec3::new(5.0, 0.0, 0.0), pole, 1.0, 1.0), None);
        assert_eq!(solve(Vec3::new(2.0, 0.0, 0.0), pole, 1.0, 1.0), None);
    }

    #[test]
    fn reach_builds_the_frames_along_the_bones_and_leaves_the_hand() {
        // shoulder (0) - elbow (1) - twist (2) - hand (3), straight along +x, one metre each but the twist.
        let mut world: Vec<Mat4> = [0.0, 1.0, 1.5, 2.0].iter().map(|&x| Mat4::from_translation(Vec3::X * x)).collect();
        world[2] = Mat4::from_rotation_x(0.5) * world[2];
        world[3] = Mat4::from_translation(Vec3::new(1.0, 1.0, 0.0));
        let arm = Arm::new(&world, 0, 1, 2, 3);
        assert_eq!((arm.upper_length, arm.lower_length), (1.0, 1.0));
        assert!(reach(&mut world, &arm, Vec3::new(1.0, -1.0, 0.0)));
        let at = |i: usize| world[i].w_axis.truncate();
        assert!(at(0).length() < 1e-5);
        assert!((at(1) - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-4, "{}", at(1));
        assert!((at(3) - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-6);
        // x along each bone; the twist node at the elbow along the forearm.
        assert!((world[0].x_axis.truncate() - Vec3::X).length() < 1e-4);
        assert!((world[1].x_axis.truncate() - Vec3::Y).length() < 1e-4);
        assert!((at(2) - at(1)).length() < 1e-6 && (world[2].x_axis.truncate() - Vec3::Y).length() < 1e-4);
        for m in &world[..3] {
            assert!((m.determinant() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn lean_tilts_the_x_axis_by_the_clamped_shake() {
        let m = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        // Body (x, y, z) is the model's (x, z, y): a body-y shake tilts x towards model z.
        let leaned = lean(m, Vec3::new(0.0, 0.1, 0.0));
        let x = leaned.x_axis.truncate();
        assert!((x - Vec3::new(1.0, 0.0, 0.1).normalize()).length() < 1e-6, "{x}");
        assert_eq!(leaned.w_axis, m.w_axis);
        assert!((leaned.y_axis.truncate() - Vec3::Y).length() < 1e-6);
        // Clamped to 0.2.
        let far = lean(m, Vec3::new(5.0, 0.0, 0.0)).x_axis.truncate();
        assert!((far - Vec3::X).length() < 1e-6);
        let back = lean(m, Vec3::new(-5.0, 0.0, 0.0)).x_axis.truncate();
        assert!((back - Vec3::X).length() < 1e-6, "x - 0.2 still points along x: {back}");
        let up = lean(m, Vec3::new(0.0, 0.0, 9.0)).x_axis.truncate();
        assert!((up - Vec3::new(1.0, 0.2, 0.0).normalize()).length() < 1e-6);
    }
}
