//! The scripted events' pure parts (`notes/gameplay.md` "Scripted events"): the car-flip spectators' trigger and placement, the
//! rescue helicopter's flight and its camera (mode 8), and the bird strike's host calls. [`super::Characters`] runs
//! them on the characters the exe loads itself (`load_car_flip1..5.lua`, `load_rescuechopper.lua`) and on the stage's
//! `run_birdstrike` items.
//!
//! - **Flip spectators.** `CCar::Assist8DF0` (0x4e8df0) emits `CSigOutsideAssistance` with a code; the HUD restarts
//!   its assistance timer, and at 1.0 s and 4.5 s of it emits `CSigInitSpectatorFlipAnimation` start and end
//!   (`rbr_gameplay::race_clock`, which this reuses through [`FlipTimer`]). The start handler (0x43d550, our symbols'
//!   `E01Cam::ApplyState`) puts three of the five spectators at the car's driveline point, turned to the driveline's
//!   heading ([`flip_group`], [`flip_heading`]); the end releases them. The recordings carry no signal, so
//!   [`Assist::emitted`] reads the emit from the car block: `Assist8DF0` arms the countdown +0x15e4 with 0.5 s
//!   (30 s on its no-emit path), sets the flip flag +0x9f0 and the override +0x1670 = 4.0.
//! - **Rescue helicopter.** `CRescueHelicopter::Activate` (0x49bc90) picks a side and lands the helicopter on the
//!   driveline at the car's distance ([`Rescue::call`]); `Tic` (0x49c170) posts `MSG_RETIRE_BYSTAND` after 24 s
//!   ([`Rescue::tic`]); `UpdateHeli` (0x49c2e0) flies its point in from 100 m along the driveline and 50 m up over 21 s
//!   ([`Rescue::flight`]); the mode-8 camera `M5010` (0x485010) stands on the driveline 30 m short of the landing
//!   point, 1.8 m up, and looks at that point ([`Rescue::camera`]). The helicopter lands at the anchor
//!   (`FillMatrix`).
//! - **Bird strike.** `run_birdstrike.lua` calls `app:PlaceCharacterAtCar(a, b, c)` and `app:Birdstrike()`
//!   ([`HostCall`]).
//!
//! Ours: the ground probe (`0x7c755c`) is the driveline's own point; `CDriveLine::Sample` by distance
//! ([`point_at`]) interpolates the records' points between the two records whose distances bracket it.

use glam::Vec3;
use rbr_formats::trk::DriveLineRecord;
use rbr_gameplay::race_clock::{AssistSettings, RaceClock};
use std::f32::consts::PI;
use std::ops::Range;

/// `Assist8DF0`'s countdown on the emitting path (`0x3f000000`).
const ASSIST_COUNTDOWN: f32 = 0.5;

/// The car words `Assist8DF0` writes, from a frame's car block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Assist {
    /// The put-back countdown, +0x15e4.
    pub countdown: f32,
    /// The flip flag, +0x9f0: the car had partial contact for more than 1 s.
    pub flipped: bool,
    /// `m_tOverride`, +0x1670.
    pub override_time: f32,
}

impl Assist {
    pub const COUNTDOWN: usize = 0x15e4;
    pub const FLIPPED: usize = 0x9f0;
    pub const OVERRIDE: usize = 0x1670;

    /// Reads the words from a car block (`None` when it is too short).
    pub fn read(car: &[u8]) -> Option<Assist> {
        Some(Assist {
            countdown: rbr_recording::read_f32(car, Self::COUNTDOWN)?,
            flipped: rbr_recording::read_u32(car, Self::FLIPPED)? != 0,
            override_time: rbr_recording::read_f32(car, Self::OVERRIDE)?,
        })
    }

    /// The code of the `CSigOutsideAssistance` the frames between `before` and `self` emitted: the countdown was
    /// armed again (it only falls otherwise) with the emitting path's 0.5 s. The code is 1 with the flip flag, else
    /// `3 - Test1670()`: 2 with the override positive, else 3.
    pub fn emitted(self, before: Assist) -> Option<u32> {
        if !(self.countdown > before.countdown && self.countdown <= ASSIST_COUNTDOWN) {
            return None;
        }
        Some(if self.flipped {
            1
        } else if self.override_time > 0.0 {
            2
        } else {
            3
        })
    }
}

/// The HUD's assistance timer, turned into the spectators' start and end (`E62GameMenu::M576710`, 0x576710).
#[derive(Debug, Clone, Copy, Default)]
pub struct FlipTimer(RaceClock);

/// `CSigInitSpectatorFlipAnimation {code, flag}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flip {
    Start(u32),
    End,
}

impl FlipTimer {
    /// One HUD frame of `dt` seconds, after the frame's steps emitted `emitted` (`M573900` keeps it first).
    pub fn tick(&mut self, dt: f32, emitted: Option<u32>) -> Option<Flip> {
        if let Some(code) = emitted {
            self.0 = self.0.outside_assistance(code);
        }
        let (clock, events) = self.0.tick(dt, AssistSettings::QUICK_RACE);
        self.0 = clock;
        events.spectators.map(|(code, end)| if end { Flip::End } else { Flip::Start(code) })
    }
}

/// The spectators a start places, of the five, and whether they turn round: code 1 (or 0) takes 0..3, codes 2 and 3
/// take 2..5, and code 3 adds pi to the heading.
pub fn flip_group(code: u32) -> (Range<usize>, bool) {
    match code {
        2 => (2..5, false),
        3 => (2..5, true),
        _ => (0..3, false),
    }
}

/// The spectators' turn about the vertical from the driveline's direction (physics axes, z up): `acos(-x)` of the
/// horizontal unit direction, negated when its `y` is positive (the sign test is our reading of the handler).
pub fn flip_heading(direction: [f32; 3], turn: bool) -> f32 {
    let horizontal = Vec3::new(direction[0], direction[1], 0.0).normalize_or(Vec3::X);
    let mut angle = (-horizontal.x).clamp(-1.0, 1.0).acos();
    if horizontal.y > 0.0 {
        angle = -angle;
    }
    if turn {
        angle += PI;
    }
    angle
}

/// `FUN_00487980`'s eye offsets for codes 1 to 3 (across, along and up the driveline frame; 0x72f580..0x72f59c).
const FLIP_EYES: [[f32; 3]; 3] = [[-6.63, 2.668, 0.967], [3.709, 3.709, 1.041], [-3.919, -4.49, 0.86]];
/// The look point is this far along the driveline from the car's point (0x72dcdc).
const FLIP_AHEAD: f32 = 1.5;
/// The eye stands this far over the ground under it, or over its own point when that is higher (0x795e14).
const FLIP_LIFT: f32 = 1.0;
/// The probes' reach: the drop onto the eye along the view's up, and the ground probe above and below it (0x72b154).
const FLIP_REACH: f32 = 10.0;
/// The eye stands this far over a hit of the drop (0x72b068).
const FLIP_DROP_LIFT: f32 = 0.1;
/// Rounds of the probe loop.
const FLIP_ROUNDS: usize = 3;

/// The car-flip camera (mode 13): `FUN_00487980`, the camera's `CSigInitSpectatorFlipAnimation` handler. From the
/// car's driveline point and direction (`Split17C`, physics axes, z up) for flip `code` 1 to 3, the eye and the point
/// it looks at: the look point 1.5 m along the driveline, the eye at the code's offset in the flat driveline frame
/// (`side = dir x up`, `ahead = up x side`). `segment(a, b)` is the stage probe (`PTR_DAT_007c755c`,
/// [`crate::stage_probe::StageProbe::segment`]): the point where the segment first meets the scenery. Three rounds:
/// when the segment from the look point to the eye hits, and the one back from the eye hits nearer the eye than the
/// first does, the eye goes to the middle of the two hits; then a drop from 10 m up the view's up (the world up made
/// square to the view) onto the eye that hits puts the eye on the line to that hit at its first distance, 0.1 m up.
/// The loop ends when the first segment hits nothing. Last, the ground probe 10 m above and below the eye lifts it to
/// 1 m over the hit, or over its own height when that is higher. (Asm read by scripted-2, rbr-recomp `notes/cameras.md` 5d.)
pub fn flip_camera(
    point: Vec3,
    direction: Vec3,
    code: u32,
    segment: impl Fn(Vec3, Vec3) -> Option<Vec3>,
) -> Option<(Vec3, Vec3)> {
    let offset = FLIP_EYES.get((code as usize).checked_sub(1)?)?;
    let direction = direction.try_normalize()?;
    let look = point + direction * FLIP_AHEAD;
    let side = direction.cross(Vec3::Z).try_normalize()?;
    let ahead = Vec3::Z.cross(side).try_normalize()?;
    let mut eye = look + side * offset[0] + ahead * offset[1] + Vec3::Z * offset[2];
    let view = eye - look;
    let distance = view.length();
    let up = Vec3::Z - view * (view.z / view.length_squared());
    // The hit's fraction of the way from `a` to `b`.
    let fraction = |a: Vec3, b: Vec3, hit: Vec3| (hit - a).length() / (b - a).length();
    for _ in 0..FLIP_ROUNDS {
        let Some(out) = segment(look, eye) else { break };
        if let Some(back) = segment(eye, look) {
            if fraction(eye, look, back) < 1.0 - fraction(look, eye, out) {
                eye = (out + back) * 0.5;
            }
        }
        if let Some(drop) = segment(eye + up * FLIP_REACH, eye) {
            eye = look + (drop - look) * (distance / (drop - look).length());
            eye.z += FLIP_DROP_LIFT;
        }
    }
    let ground = segment(eye + Vec3::Z * FLIP_REACH, eye - Vec3::Z * FLIP_REACH).map(|hit| hit.z);
    eye.z = ground.map_or(eye.z, |g| g.max(eye.z)) + FLIP_LIFT;
    Some((eye, look))
}

/// The point (physics axes) the driveline has at stage distance `distance`, from its records' points and distances
/// (`values`), held at either end. Ours, standing in for `CDriveLine::Sample` (0x50b180).
pub fn point_at(records: &[DriveLineRecord], values: &[f32], distance: f32) -> Option<Vec3> {
    let point = |r: &DriveLineRecord| Vec3::new(r.vectors[0].x, r.vectors[0].y, r.vectors[0].z);
    let first = records.first()?;
    let next = values.iter().position(|&v| v > distance);
    let (i, fraction) = match next {
        Some(0) => return Some(point(first)),
        None => return Some(point(records.last()?)),
        Some(k) => {
            let (a, b) = (values[k - 1], values[k]);
            (k - 1, if b > a { (distance - a) / (b - a) } else { 0.0 })
        }
    };
    let (a, b) = (point(records.get(i)?), point(records.get(i + 1)?));
    Some(a.lerp(b, fraction))
}

/// `CRescueHelicopter` (0x49bb10-0x49c5d0): its anchor on the driveline, the side it comes from and its time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rescue {
    /// The car's driveline distance when it was called (`Thunk4F0(+0x48)`).
    pub anchor: f32,
    /// +0x60: +1 or -1, which way along the driveline it comes from.
    pub side: f32,
    /// +0x14: seconds since the call.
    pub level: f32,
    /// Flag bit 0: active (the 24 s run).
    pub active: bool,
}

/// `Tic` ends the run here (`E20PastLimit`).
pub const RESCUE_SECONDS: f32 = 24.0;
/// `UpdateHeli`'s flight time, distance along the driveline and climb.
const FLIGHT_SECONDS: f32 = 21.0;
const FLIGHT_DISTANCE: f32 = 100.0;
const FLIGHT_CLIMB: f32 = 40.0;
const FLIGHT_FLOOR: f32 = 10.0;
/// Where the mode-8 camera stands: `Activate` samples the driveline 30 m (`0x72f42c`) back from the anchor, away
/// from the side the helicopter comes from, and lifts the point by 1.8 m (`0x73088c`). The C++ reconstruction
/// (`RBR_E20_Heli.cpp`) drops the 30 m; the decompile and the image have it.
const CAMERA_BACK: f32 = 30.0;
pub const CAMERA_LIFT: f32 = 1.8;

impl Rescue {
    /// `Activate`: `anchor` is the car's driveline distance, `length` the driveline's last distance (`Last`), `roll`
    /// a `rand()` (0..=0x7fff). From 200 m into the stage it comes from behind when the stage ends within 200 m,
    /// else from either side by the roll; before 200 m from ahead.
    pub fn call(anchor: f32, length: f32, roll: u32) -> Rescue {
        let side = if 0.0 <= anchor - 200.0 {
            if length < anchor + 200.0 || roll > 0x3ffe {
                -1.0
            } else {
                1.0
            }
        } else {
            1.0
        };
        Rescue { anchor, side, level: 0.0, active: true }
    }

    /// `Tic`: the time runs while active; true on the frame it reaches 24 s, which deactivates it and posts
    /// `MSG_RETIRE_BYSTAND` (`E60_56a120`).
    pub fn tic(&mut self, dt: f32) -> bool {
        if !self.active {
            return false;
        }
        self.level += dt;
        if RESCUE_SECONDS <= self.level {
            self.active = false;
            self.level = 0.0;
            return true;
        }
        false
    }

    /// The mode-8 camera's driveline distance (+0x50, without its lift).
    pub fn camera(&self) -> f32 {
        self.anchor - self.side * CAMERA_BACK
    }

    /// `UpdateHeli`: the flight point as a driveline distance and a height over the ground there. `v` falls from 1 to
    /// 0 over 21 s: the point comes from 100 m along the driveline (by the side) and 50 m up to 10 m over the anchor.
    pub fn flight(&self) -> (f32, f32) {
        let v = if self.active { (1.0 - self.level * (1.0 / FLIGHT_SECONDS)).clamp(0.0, 1.0) } else { 0.0 };
        (self.side * v * FLIGHT_DISTANCE + self.anchor, v * FLIGHT_CLIMB + FLIGHT_FLOOR)
    }
}

/// The helicopter's dust on a map ([`smoke`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Smoke {
    pub emitter: &'static str,
    /// `Activate` writes it into the emitter's particle class at `+0x6c`: the intermediate colour's alpha (the
    /// class's colours are read from `+0x50` by `E83_Read13`, 0x5dfb50: start RGBA, the set-in at `+0x80`, then the
    /// intermediate and the end RGBA).
    pub mid_alpha: f32,
}

/// `Activate`'s smoke by rally (the map's tens, index `map / 10 - 1` of its two stack tables): Arctic, Pirelli
/// (British), Canberra, Mont Blanc (the British emitter again), Hokkaido and Ramada; the alphas are 0x72f674,
/// 0x72f664, 0x730888, 0x730884 and 0x72f428 twice.
const SMOKES: [Smoke; 6] = [
    Smoke { emitter: "ar_smoke_b_emitter", mid_alpha: 0.7 },
    Smoke { emitter: "br2_fakesmoke_a_emitter", mid_alpha: 0.4 },
    Smoke { emitter: "au_fakesmoke_a_emitter", mid_alpha: 0.25 },
    Smoke { emitter: "br2_fakesmoke_a_emitter", mid_alpha: 0.075 },
    Smoke { emitter: "ho_DAMP_fakesmoke_emitter", mid_alpha: 0.2 },
    Smoke { emitter: "us1_fakesmoke_a_emitter", mid_alpha: 0.2 },
];
/// `Activate` scales the emitter's particle class's life (`+0xb0`) by this (0x72f428).
pub const SMOKE_LIFE: f32 = 0.2;
/// `Burst`: particles per second of its timer (0x72f5f8), the full rate below this height over the ground (0x730898
/// = 1/30), the speed they leave at (0x730894) and their upward speed (0x7307f8).
const BURST_RATE: f32 = 60.0;
const BURST_HEIGHT: f32 = 30.0;
const BURST_SPEED: f32 = 15.0;
const BURST_UP: f32 = 0.5;
/// The ground probe's hit is lifted by this (0x72b068) before the particles start there.
pub const BURST_LIFT: f32 = 0.1;

/// The helicopter's smoke on map `map` (`Activate`; none outside the six rallies).
pub fn smoke(map: u32) -> Option<Smoke> {
    SMOKES.get((map / 10).checked_sub(1)? as usize).copied()
}

/// `CRescueHelicopter::Burst` (0x49bf90): the dust the rotor blows off the ground under the helicopter.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Burst {
    /// +0x64: the time since `Activate`.
    time: f32,
    /// +0x68: the time of the last throw.
    last: f32,
}

impl Burst {
    /// One frame of `dt` seconds with the helicopter `height` over the ground under it (`None`: the probe 100 m down
    /// found none): how many particles to throw. They come in 60 a second, at least two at a time, all of them up to
    /// 30 m over the ground and fewer above (none from 30.1 m).
    pub fn tick(&mut self, dt: f32, height: Option<f32>) -> u32 {
        self.time += dt;
        let n = ((self.time - self.last) * BURST_RATE) as i32;
        if n < 2 {
            return 0;
        }
        self.last = self.time;
        let Some(height) = height else { return 0 };
        let f = (1.0 - (height - BURST_LIFT) * (1.0 / BURST_HEIGHT)).clamp(0.0, 1.0);
        ((n as f32 * f) as i32).max(0) as u32
    }
}

/// A thrown particle's velocity (physics axes): a random horizontal direction from two `rand()`s, scaled to 15 m/s
/// over its length with the upward 0.5, then 0.5 up. Ours: the first particle of a throw takes 0.5 for that
/// upward lane too (the game's is the stack's old value).
pub fn burst_velocity(crt: &mut super::random::CrtRand) -> Vec3 {
    let dx = crt.draw() as f32 * (1.0 / 32767.0) - 0.5;
    let dy = crt.draw() as f32 * (1.0 / 32767.0) - 0.5;
    let d = Vec3::new(dx, dy, BURST_UP);
    let scaled = d * (BURST_SPEED / d.length());
    Vec3::new(scaled.x, scaled.y, BURST_UP)
}

/// A run script's call to the host (`CPC_Application`'s Lua bindings).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostCall {
    /// `app:PlaceCharacterAtCar(a, b, c)` (0x4481e0): all false drops the character's pair; else the pair is kept and
    /// `E02AnimUpdate` moves it each frame (`c`: to the bird-strike point).
    PlaceAtCar { translate: bool, rotate: bool, strike: bool },
    /// `app:Birdstrike()` (0x4481f0): `CSigBirdstrike` at the strike point.
    Birdstrike,
}

/// A scripted event for the sound and particle systems.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// `CSigInitSpectatorFlipAnimation` start, with its code.
    FlipStart(u32),
    FlipEnd,
    /// The rescue helicopter was called, landing at this stage point.
    RescueCalled(Vec3),
    /// 24 s after the call: `MSG_RETIRE_BYSTAND`.
    RetireBystand,
    /// `CSigBirdstrike` at this stage point (`CCollisionSnd::OnBirdstrike` plays `birdhit.wav`).
    Birdstrike(Vec3),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assist(countdown: f32, flipped: bool) -> Assist {
        Assist { countdown, flipped, override_time: 4.0 }
    }

    #[test]
    fn the_emit_is_the_countdown_armed_with_half_a_second() {
        let idle = Assist::default();
        // drive-rcv-help-corolla-34, post 1957: on its side.
        assert_eq!(assist(0.493, true).emitted(idle), Some(1));
        // drive-rcv-stuck-206-64, post 2700.
        assert_eq!(assist(0.493, false).emitted(idle), Some(2));
        assert_eq!(Assist { override_time: 0.0, ..assist(0.5, false) }.emitted(idle), Some(3));
        // The countdown falling, and the no-emit path's 30 s.
        assert_eq!(assist(0.486, true).emitted(assist(0.493, true)), None);
        assert_eq!(assist(29.99, false).emitted(idle), None);
        assert_eq!(idle.emitted(idle), None);
    }

    #[test]
    fn the_spectators_start_at_one_second_and_end_at_four_and_a_half() {
        let mut timer = FlipTimer::default();
        let dt = 0.02;
        let mut seen = Vec::new();
        for frame in 0..400 {
            let emitted = (frame == 10).then_some(2);
            if let Some(flip) = timer.tick(dt, emitted) {
                seen.push((frame, flip));
            }
        }
        // The emit's frame counts its own dt (the HUD ticks after the steps); f32 sums cross 1.0 and 4.5 a frame late.
        assert_eq!(seen, [(60, Flip::Start(2)), (235, Flip::End)]);
    }

    #[test]
    fn the_groups_and_the_heading() {
        assert_eq!(flip_group(1), (0..3, false));
        assert_eq!(flip_group(2), (2..5, false));
        assert_eq!(flip_group(3), (2..5, true));
        assert!((flip_heading([-1.0, 0.0, 0.3], false)).abs() < 1e-6);
        assert!((flip_heading([1.0, 0.0, 0.0], false) - PI).abs() < 1e-6);
        assert!((flip_heading([0.0, 1.0, 0.0], false) + PI / 2.0).abs() < 1e-6);
        assert!((flip_heading([0.0, -1.0, 0.0], false) - PI / 2.0).abs() < 1e-6);
        assert!((flip_heading([-1.0, 0.0, 0.0], true) - PI).abs() < 1e-6);
    }

    #[test]
    fn the_helicopter_side_flight_and_retire() {
        assert_eq!(Rescue::call(150.0, 5000.0, 0x7fff).side, 1.0);
        assert_eq!(Rescue::call(4900.0, 5000.0, 0).side, -1.0);
        assert_eq!(Rescue::call(1000.0, 5000.0, 0x3ffe).side, 1.0);
        assert_eq!(Rescue::call(1000.0, 5000.0, 0x3fff).side, -1.0);

        let mut rescue = Rescue::call(1000.0, 5000.0, 0);
        assert_eq!(rescue.flight(), (1100.0, 50.0));
        assert_eq!(rescue.camera(), 970.0);
        let mut posted = Vec::new();
        for frame in 1..=1300 {
            if rescue.tic(0.02) {
                posted.push(frame);
            }
            if frame == 1050 {
                assert_eq!(rescue.flight(), (1000.0, 10.0));
            }
        }
        // 24 s of 0.02 s frames (f32 sums reach 24 a frame late).
        assert_eq!(posted.len(), 1);
        assert!((1200..=1201).contains(&posted[0]), "{posted:?}");
        assert!(!rescue.active);
    }

    #[test]
    fn the_flip_camera_stands_at_the_codes_side() {
        // Driveline along +y: side = y x z = +x, ahead = +y.
        let open = |_, _| None;
        let (eye, look) = flip_camera(Vec3::ZERO, Vec3::new(0.0, 2.0, 0.0), 1, open).unwrap();
        assert_eq!(look, Vec3::new(0.0, 1.5, 0.0));
        assert!((eye - Vec3::new(-6.63, 1.5 + 2.668, 0.967 + 1.0)).length() < 1e-5, "{eye}");
        // Ground at z = 5 under the eye: it stands 1 m over it.
        let ground = |a: Vec3, b: Vec3| (a.z > 5.0 && b.z < 5.0 && a.x == b.x).then(|| Vec3::new(a.x, a.y, 5.0));
        let (eye, _) = flip_camera(Vec3::ZERO, Vec3::Y, 3, ground).unwrap();
        assert!((eye.z - 6.0).abs() < 1e-5, "{eye}");
        assert!(flip_camera(Vec3::ZERO, Vec3::Y, 0, open).is_none());
        assert!(flip_camera(Vec3::ZERO, Vec3::Z, 2, open).is_none());
    }

    #[test]
    fn the_flip_camera_comes_in_front_of_a_wall() {
        use crate::stage_probe::StageProbe;
        // Code 1's eye is 6.63 m to the -x side. A block from x = -4 to -2 stands between: its near face (x = -2)
        // faces the look point, its far face (x = -4) the eye.
        let face = |x: f32, towards: f32| {
            let (a, b, c, d) = (
                Vec3::new(x, -20.0, -5.0),
                Vec3::new(x, 20.0, -5.0),
                Vec3::new(x, 20.0, 5.0),
                Vec3::new(x, -20.0, 5.0),
            );
            if towards > 0.0 {
                [[a, c, b], [a, d, c]]
            } else {
                [[a, b, c], [a, c, d]]
            }
        };
        let block = StageProbe::from_triangles(&[face(-2.0, 1.0), face(-4.0, -1.0)].concat());
        let segment = |a, b| block.segment(a, b).map(|h| h.point);
        let (eye, _) = flip_camera(Vec3::ZERO, Vec3::Y, 1, segment).unwrap();
        // The two hits are 2 m apart, nearer each end than the other: the eye goes to their middle.
        assert!((eye.x + 3.0).abs() < 1e-4, "{eye}");
        // A thin wall (both hits at one point) leaves the eye where it was.
        let wall = StageProbe::from_triangles(&[face(-3.0, 1.0), face(-3.0, -1.0)].concat());
        let (eye, _) = flip_camera(Vec3::ZERO, Vec3::Y, 1, |a, b| wall.segment(a, b).map(|h| h.point)).unwrap();
        assert!((eye.x + 6.63).abs() < 1e-4, "{eye}");
    }

    #[test]
    fn the_burst_throws_sixty_a_second_below_thirty_metres() {
        let mut burst = Burst::default();
        let thrown: u32 = (0..1000).map(|_| burst.tick(0.007, Some(0.1))).sum();
        // 7 s of 7 ms frames: two every fifth frame (35 ms), so 57 a second.
        assert!((395..=405).contains(&thrown), "{thrown}");
        let mut high = Burst::default();
        assert_eq!((0..1000).map(|_| high.tick(0.007, Some(31.0))).sum::<u32>(), 0);
        assert_eq!(smoke(15), Some(Smoke { emitter: "ar_smoke_b_emitter", mid_alpha: 0.7 }));
        assert_eq!(smoke(41), Some(Smoke { emitter: "br2_fakesmoke_a_emitter", mid_alpha: 0.075 }));
        assert_eq!(smoke(5), None);
        let mut crt = super::super::random::CrtRand::srand(1);
        let v = burst_velocity(&mut crt);
        assert!(v.z == 0.5 && v.truncate().length() < 15.0 && v.truncate().length() > 1.0, "{v}");
    }

    #[test]
    fn the_driveline_point_by_distance() {
        let v = |x, y, z| rbr_formats::trk::Vec3 { x, y, z };
        let record =
            |x: f32| DriveLineRecord { vectors: [v(x, 0.0, 1.0), v(1.0, 0.0, 0.0)], value: x, unknown_u16: [0; 2] };
        let records = [record(0.0), record(10.0), record(30.0)];
        let values = [0.0, 10.0, 30.0];
        assert_eq!(point_at(&records, &values, 20.0), Some(Vec3::new(20.0, 0.0, 1.0)));
        assert_eq!(point_at(&records, &values, -5.0), Some(Vec3::new(0.0, 0.0, 1.0)));
        assert_eq!(point_at(&records, &values, 99.0), Some(Vec3::new(30.0, 0.0, 1.0)));
    }
}
