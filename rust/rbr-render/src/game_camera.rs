//! The game's chase camera (mode 1, `notes/cameras.md` sections 2 and 9), beside our own chase
//! ([`crate::playback::Playback::camera`]): `E15CamHost::M8840` (0x488840) moves the eye after the car each frame, and
//! `M6530` (0x486530) looks from it at the car's look point. The start orbit (mode 10) is the same camera turned by
//! the swing angle, with the eye snapped to its place every frame (`M487680` sets the host's `+0x08` on entering
//! mode 10).
//!
//! All of it runs in the z-up physics frame on the car pose of the frame shown (`CSigCarGeneralState`, read by
//! `M6620`, 0x486620: the position and the body axes from the orientation).
//!
//! Faithful (asm of 0x488840, 0x486530, 0x486620, 0x487680; constants read from the 1.02 image):
//! - the settings: [`RADIUS`], [`ANGLE_DEGREES`], [`VIEW_ANGLE_DEGREES`], [`PAD`], [`FOV_DEGREES`], [`MIN_DISTANCE`]
//!   and [`LAG_SECONDS`]. They are `SCameraSettings`' defaults (`InitCAM` 0x587a90, words 3 to 13), and the
//!   race-time globals 0x1660d94..0x1660dbc hold them (read from a harness snapshot, `notes/cameras.md` 9). The
//!   camera page's sliders set the first three ([`Settings::GAME`]). With these values the eye rides the distance
//!   clamp 5.4 m from the look point at speed, about 3.8 m behind the car's centre and 1.7 m up, looking about 2
//!   degrees down, so the car's rear is below the image at our chase's field of view. The viewer's chase takes our
//!   values instead ([`Settings::default`]), nudged per car by the player;
//! - the look point 1.544 m ahead of the body and 0.75 m up ([`LOOK_AT`], the camera init's default at host
//!   `+0x420`; `CSigCarCameras` leaves it, since `E09_LoadCarCameras` sends it as zeros);
//! - the eye's place behind the car, the downhill lift, the 64-frame lag, the height window and the distance clamp.
//!
//! - the collision probes (asm of 0x488840, the queries in [`crate::stage_probe`]): before the lag, the wanted eye is
//!   pulled in front of what the segments from the eye and from the look point to it cross, at most [`REACH`] from
//!   the look point, and pushed off nearby faces; its push (host `+0x270`) decays by 0.7 a frame without a hit, and
//!   while it is over 0.1 m the eye moves at `2 dt` towards the wanted place instead of the lag and the height window.
//!   After the distance clamp, the eye is put in front of what the segments from the look point and from its last
//!   place cross, or off nearby faces; that push (`+0x280`) decays by 0.8;
//! - the bonnet view's ground probe (mode 3, `M5760` 0x485760): [`bonnet_lift`].
//!
//! Ours:
//! - the look-back swing (host `+0x2e0`, `+0x2e8`): 1.02 has no action that sets `+0x2e0` (only the camera's
//!   constructor writes it), so the swing is dead code in the game. The viewer's look-back key ([`Chase::tic`]'s
//!   input) drives it;
//! - the eye snaps to its place on the first frame [`Follow`] is shown and when the drive jumps, starting from its place
//!   without the probes (the game's starts from the origin, host `+0x2a0` zeroed by 0x4868a0, so its first probe from
//!   the eye crosses the stage). The game snaps while the host's `+0x08` is set: from a mode change until a CH_CAM
//!   press clears it. Whether it is cleared after the start orbit's go is not traced;
//! - the start orbit's clearance ([`orbit_reach`]): the game's probes are segments, so a start gantry's pillar
//!   (RSF 138) still fills the view while the orbit passes behind it, at the game's 4.4 m as at ours. The orbit is
//!   drawn from nearer the car while the line of sight, or one [`ORBIT_CLEARANCE`] to either side, crosses the stage;
//! - the field of view: our chase's fixed horizontal one ([`crate::playback::CHASE_FOV_DEGREES`]). The game's is [`FOV_DEGREES`], which HedgeHog3D draws as `75 * 0.75` vertical at
//!   4:3 (`CHH_World::OnCamera` 0x10066dc0, `D3DXMatrixPerspectiveFovLH(fov, 4/3)`): [`fov`], about 71 horizontal.

use crate::camera::Lens;
use crate::stage_probe::StageProbe;
use glam::Vec3;
use rbr_recording::pose::CarPose;

/// `CAM_RADIUS` (0x1660d94), m.
pub const RADIUS: f32 = 4.4;
/// `CAM_ANGLE` (0x1660d98), degrees: the eye's elevation above the look point.
pub const ANGLE_DEGREES: f32 = 13.0;
/// `VW_ANGLE` (0x1660d9c), degrees: tilts the view from the eye-to-look-point line (`M6530`, [`Chase::view`]).
pub const VIEW_ANGLE_DEGREES: f32 = 11.0;
/// 0x1660da8, m: the eye's horizontal distance from the look point is held within `RADIUS + PAD`.
pub const PAD: f32 = 1.0;

/// The chase's place: the game's three camera-page words (`CGameMenuDaemon::M56da50` 0x56da50 sets them from its
/// sliders). The default is our framing ([`Settings::default`]), [`Settings::GAME`] the game's; the player nudges it
/// per car ([`Settings::nudged`]). The game's other camera words (the pad, the field of view, the minimum distance,
/// the lag) stay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// `CAM_RADIUS`, m.
    pub radius: f32,
    /// `CAM_ANGLE`, degrees.
    pub angle_degrees: f32,
    /// `VW_ANGLE`, degrees.
    pub view_angle_degrees: f32,
    /// Ours: the eye's horizontal distance from the look point is held at `CAM_RADIUS * cos(CAM_ANGLE)`, the snapped
    /// distance, instead of between [`MIN_DISTANCE`] and `CAM_RADIUS + PAD`, so the car sits as far at the start as at
    /// speed. The lag then only turns the eye about the car.
    pub steady: bool,
}

impl Settings {
    /// `SCameraSettings`' defaults (above).
    pub const GAME: Settings = Settings {
        radius: RADIUS,
        angle_degrees: ANGLE_DEGREES,
        view_angle_degrees: VIEW_ANGLE_DEGREES,
        steady: false,
    };
    /// How far the nudges reach (ours): the distance in metres and the height angle in degrees.
    pub const RADIUS_RANGE: std::ops::RangeInclusive<f32> = 2.5..=12.0;
    pub const ANGLE_RANGE: std::ops::RangeInclusive<f32> = 0.0..=45.0;

    /// Each value held within its range.
    pub fn clamped(self) -> Settings {
        let within = |v: f32, r: std::ops::RangeInclusive<f32>| v.clamp(*r.start(), *r.end());
        Settings {
            radius: within(self.radius, Settings::RADIUS_RANGE),
            angle_degrees: within(self.angle_degrees, Settings::ANGLE_RANGE),
            ..self
        }
    }

    /// The eye moved by the player's `nudge` (`crate::camera_prefs`): its `z` m forward (nearer the car, a shorter
    /// `CAM_RADIUS`) and its `y` m up (the height `CAM_RADIUS * sin(CAM_ANGLE)`), within the ranges.
    pub fn nudged(self, nudge: Vec3) -> Settings {
        let radius = (self.radius - nudge.z).clamp(*Settings::RADIUS_RANGE.start(), *Settings::RADIUS_RANGE.end());
        let height = self.radius * self.angle_degrees.to_radians().sin() + nudge.y;
        let angle = (height / radius).clamp(-1.0, 1.0).asin().to_degrees();
        Settings { radius, angle_degrees: angle, ..self }.clamped()
    }
}

impl Default for Settings {
    /// Ours: the eye about 6.9 m behind the car's centre and 1.6 m above it, held there at every speed (`steady`),
    /// looking about 4 degrees down. At 16:9 and the chase's field of view ([`crate::playback::CHASE_FOV_DEGREES`])
    /// the whole car sits in the lower third with the horizon about 40% from the top. The game's own (4.4 m, 13
    /// degrees, its distance clamp) puts the eye 1.8 m behind the bumper at speed, which crops the car at our
    /// field of view. Chosen by captures on the COROLLA, the Evo IX, the Alpine A110 and the GR Yaris Rally2
    /// (`lead/reports/cameras-3/`).
    fn default() -> Settings {
        Settings {
            radius: CHASE_RADIUS,
            angle_degrees: CHASE_ANGLE_DEGREES,
            view_angle_degrees: CHASE_VIEW_ANGLE_DEGREES,
            steady: true,
        }
    }
}

/// Our chase's framing ([`Settings::default`]).
const CHASE_RADIUS: f32 = 8.5;
const CHASE_ANGLE_DEGREES: f32 = 5.6;
const CHASE_VIEW_ANGLE_DEGREES: f32 = 1.5;
/// 0x1660db4: the game's field of view of modes 1, 10 and 11, degrees (see [`fov`]).
pub const FOV_DEGREES: f32 = 75.0;
/// 0x1660db8, m: the nearest horizontal distance of the eye from the look point.
pub const MIN_DISTANCE: f32 = 3.0;
/// 0x1660dbc, s: the lag's time constant (each frame's `dt / LAG_SECONDS` goes into the 64-frame mean).
pub const LAG_SECONDS: f32 = 0.8;
/// The near plane `M8840` sets (0x72b068), m.
pub const NEAR: f32 = 0.1;
/// The look point in the body frame (front `-y`, up `+z`), the camera init's host `+0x420` (0x72f5dc, 0x72f448).
pub const LOOK_AT: Vec3 = Vec3::new(0.0, -1.544307, 0.75);

/// The probes keep the wanted eye within this of the look point (0x72f678), m.
pub const REACH: f32 = 3.25;
/// The wanted eye's push decays by this a frame without a hit (0x72f674), and the eye's by [`EYE_PUSH_DECAY`]
/// (0x72f610).
const WANTED_PUSH_DECAY: f32 = 0.7;
const EYE_PUSH_DECAY: f32 = 0.8;
/// Over this squared length of the wanted eye's push (0x72f440), m^2, the eye eases towards it at [`PUSHED_RATE`] a
/// second (0x72ad94) and the height window opens to [`PUSHED_FLOOR`], [`PUSHED_CEILING`] (0x72f66c, 0x72f680).
const PUSHED: f32 = 0.01;
const PUSHED_RATE: f32 = 2.0;
const PUSHED_FLOOR: f32 = -0.6;
const PUSHED_CEILING: f32 = 1.2;
/// The bonnet probe's segment: from this above the eye (0x795e18) to [`BONNET_BELOW`] below it (0x72f42c), m.
const BONNET_ABOVE: f32 = 0.5;
const BONNET_BELOW: f32 = 30.0;

/// Frames in the lag's mean (host `+0x138`, 64 words, index `+0x238`, sum `+0x23c`).
const LAG_FRAMES: usize = 64;
/// Each lag word's start (0x3ca3d70a), and so the sum's (0x72f60c, 1.28).
const LAG_START: f32 = 0.02;
/// The downhill lift: the body's rear axis's height times this (0x72ae44), followed at `dt` per second, at most
/// [`LIFT_MAX`] (0x72f57c), m.
const LIFT_GAIN: f32 = 5.0;
const LIFT_MAX: f32 = 2.5;
/// The height window around the eye's height: its floor (host `+0x2f0`, from -0.2) eases to -0.3 while it holds the eye
/// up and to -0.2 otherwise; its ceiling (`+0x2ec`, from 0.3) to 0.4 while it holds the eye down and to 0.3 otherwise.
const FLOOR_START: f32 = -0.2;
const FLOOR_HOLDING: f32 = -0.3;
const CEILING_START: f32 = 0.3;
const CEILING_HOLDING: f32 = 0.4;
/// The look-back swing eases towards pi at this rate per second (0x72ad94), and back to a whole turn, where it drops
/// to 0 within 0.0209 rad (0x72f67c).
const LOOK_BACK_RATE: f32 = 2.0;
const LOOK_BACK_SNAP: f32 = 0.020_943_95;
/// The start orbit's sight lines run this far to either side of the eye's ([`orbit_reach`]), and the eye stays this
/// far in front of what they cross, m (ours).
const ORBIT_CLEARANCE: f32 = 0.6;
/// The nearest the start orbit's eye comes to the look point, m: outside the body (ours).
const ORBIT_NEAREST: f32 = 2.0;
/// The start orbit's distance eases to its clear one at this rate per second (ours).
const ORBIT_RATE: f32 = 12.0;
/// [`Follow`] snaps when the drive jumps by more than this, s (ours).
const JUMP_SECONDS: f64 = 1.0;
/// [`Follow`] settles a fresh camera over at most this many frames before the one shown (ours).
const SETTLE_FRAMES: usize = 300;

/// The game's field of view as a horizontal one, radians: `degrees * 0.75` vertical at 4:3.
pub fn fov(degrees: f32) -> f32 {
    2.0 * (((degrees * 0.75).to_radians() * 0.5).tan() * (4.0 / 3.0)).atan()
}

/// `M5760`'s ground probe for the bonnet view (mode 3): an eye less than `near + 0.2` above what lies under it (the
/// segment from 0.5 m above to 30 m below) goes to `near + 0.1` off that face. Physics frame.
pub fn bonnet_lift(probe: &StageProbe, eye: Vec3, near: f32) -> Vec3 {
    let hit = probe.segment(eye + Vec3::Z * BONNET_ABOVE, eye - Vec3::Z * BONNET_BELOW);
    match hit {
        Some(hit) if (hit.point - eye).length() < near + 0.2 => hit.point + hit.normal * (near + 0.1),
        _ => eye,
    }
}

/// The start orbit's distance from `look` (ours, the module docs): the eye's own, or in front of what the line of sight
/// from `look` to `eye`, or one [`ORBIT_CLEARANCE`] to either side of it, crosses first (at least [`ORBIT_NEAREST`]),
/// eased from the `last` frame's over `dt`.
pub fn orbit_reach(probe: &StageProbe, look: Vec3, eye: Vec3, last: Option<f32>, dt: f32) -> f32 {
    let sight = eye - look;
    let along = sight.normalize_or_zero();
    let side = Vec3::new(-along.y, along.x, 0.0).normalize_or_zero() * ORBIT_CLEARANCE;
    let blocked = [-1.0, 0.0, 1.0]
        .into_iter()
        .filter_map(|k| {
            let from = look + side * k;
            probe.segment(from, from + sight).map(|hit| (hit.point - from).dot(along))
        })
        .reduce(f32::min);
    let target = blocked.map_or(sight.length(), |at| (at - ORBIT_CLEARANCE).max(ORBIT_NEAREST));
    let last = last.unwrap_or(target);
    last + (target - last) * (dt * ORBIT_RATE).min(1.0)
}

/// The car as the camera sees it, in the physics frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Car {
    pub position: Vec3,
    /// The body's x, y (rear) and z (up) axes (host `+0xfc`, `+0x108`, `+0x114`).
    pub axes: [Vec3; 3],
}

impl Car {
    pub fn from_pose(pose: &CarPose) -> Car {
        let axis = |v| Vec3::from(pose.rotate(v));
        Car {
            position: Vec3::from(pose.position),
            axes: [axis([1.0, 0.0, 0.0]), axis([0.0, 1.0, 0.0]), axis([0.0, 0.0, 1.0])],
        }
    }

    /// A body-frame point in the physics frame (`M483c90`'s rows times the vector, plus the position).
    fn point(&self, v: Vec3) -> Vec3 {
        self.position + self.axes[0] * v.x + self.axes[1] * v.y + self.axes[2] * v.z
    }

    /// The look point (host `+0x260`).
    pub fn look_at(&self) -> Vec3 {
        self.point(LOOK_AT)
    }
}

/// The chase camera's state in the host.
#[derive(Debug, Clone, PartialEq)]
pub struct Chase {
    /// The eye (`+0x2a0`).
    pub eye: Vec3,
    /// The downhill lift (`DAT_007eacf8`), m.
    lift: f32,
    /// The look-back swing (`+0x2e8`), radians.
    look_back: f32,
    /// The height window's ceiling (`+0x2ec`) and floor (`+0x2f0`) about the eye's height, m.
    ceiling: f32,
    floor: f32,
    lag: [f32; LAG_FRAMES],
    lag_sum: f32,
    lag_at: usize,
    /// The probes' pushes of the wanted eye (`+0x270`) and of the eye (`+0x280`).
    wanted_push: Vec3,
    eye_push: Vec3,
    /// The player's camera settings.
    pub settings: Settings,
}

impl Default for Chase {
    fn default() -> Chase {
        Chase {
            eye: Vec3::ZERO,
            lift: 0.0,
            look_back: 0.0,
            ceiling: CEILING_START,
            floor: FLOOR_START,
            lag: [LAG_START; LAG_FRAMES],
            lag_sum: LAG_START * LAG_FRAMES as f32,
            lag_at: 0,
            wanted_push: Vec3::ZERO,
            eye_push: Vec3::ZERO,
            settings: Settings::GAME,
        }
    }
}

impl Chase {
    /// `M8840` for modes 1 and 10 after a frame of `dt` seconds: `swing` is the start orbit's angle (0 outside it),
    /// `looking_back` the look-back input, `snap` puts the eye at its place (the host's `+0x08`), and `probe` is the
    /// stage's collision mesh (no probes without it).
    pub fn tic(&mut self, car: &Car, dt: f32, swing: f32, looking_back: bool, snap: bool, probe: Option<&StageProbe>) {
        let (radius, angle) = (self.settings.radius, self.settings.angle_degrees.to_radians());
        let rear = car.axes[1];
        let lift_target = if rear.z > 0.0 { rear.z * LIFT_GAIN } else { 0.0 };
        self.lift = ((lift_target - self.lift) * dt + self.lift).clamp(0.0, LIFT_MAX);
        let mut offset = rear * ((-angle).cos() * radius);
        self.look_back = self.next_look_back(dt, looking_back);
        let turn = swing + self.look_back;
        if turn != 0.0 {
            let (s, c) = turn.sin_cos();
            offset = Vec3::new(c * offset.x - s * offset.y, s * offset.x + c * offset.y, offset.z);
        }
        let look_at = car.look_at();
        let mut wanted = look_at + offset;
        wanted.z += angle.sin() * radius;
        if let Some(probe) = probe {
            wanted = self.probe_wanted(probe, look_at, wanted, radius.min(REACH));
        }
        if snap {
            self.eye = wanted;
            return;
        }
        let before = self.eye;
        self.lag_sum -= self.lag[self.lag_at];
        self.lag[self.lag_at] = dt / LAG_SECONDS;
        self.lag_sum += self.lag[self.lag_at];
        self.lag_at = (self.lag_at + 1) % LAG_FRAMES;
        let follow = (self.lag_sum / LAG_FRAMES as f32).min(1.0);
        if let (Some(probe), true) = (probe, self.wanted_push.length_squared() > PUSHED) {
            // Pushed: straight towards the wanted place, kept in front of what lies between.
            let mut eye = self.eye + (wanted - self.eye) * (dt * PUSHED_RATE);
            for _ in 0..6 {
                let Some(hit) = probe.segment(eye, self.eye) else { break };
                eye = hit.point + hit.normal * NEAR;
            }
            self.eye = eye;
            self.ceiling = PUSHED_CEILING;
            self.floor = PUSHED_FLOOR;
        } else {
            // The eye's height is held in a window about its wanted height, which widens while it holds.
            let height = angle.sin() * radius + look_at.z + self.lift;
            let rate = dt.clamp(0.0, 1.0);
            let floor = height + self.floor;
            let floor_target = if floor > self.eye.z {
                self.eye.z = floor;
                FLOOR_HOLDING
            } else {
                FLOOR_START
            };
            self.floor += (floor_target - self.floor) * rate;
            let ceiling = height + self.ceiling;
            let ceiling_target = if self.eye.z > ceiling {
                self.eye.z = ceiling;
                CEILING_HOLDING
            } else {
                CEILING_START
            };
            self.ceiling += (ceiling_target - self.ceiling) * rate;
            self.eye += (wanted - self.eye) * follow;
        }
        // The horizontal distance to the look point stays between MIN_DISTANCE and RADIUS + PAD (steady: at the
        // snapped distance).
        let (nearest, farthest) = match self.settings.steady {
            true => (angle.cos() * radius, angle.cos() * radius),
            false => (MIN_DISTANCE, PAD + radius),
        };
        let flat = Vec3::new(look_at.x - self.eye.x, look_at.y - self.eye.y, 0.0);
        let distance = flat.length();
        let beyond = distance - farthest;
        if beyond > 0.0 {
            self.eye += flat * (beyond / distance);
        } else if distance < nearest && distance > 0.0 {
            self.eye += flat * ((distance - nearest) / distance);
        }
        if let Some(probe) = probe {
            self.probe_eye(probe, look_at, before);
        }
    }

    /// The wanted eye `wanted` after the probes from the eye and from the look point `look_at`, each place on a hit
    /// at most `reach` from the look point (0x488c49..0x489190).
    fn probe_wanted(&mut self, probe: &StageProbe, look_at: Vec3, wanted: Vec3, reach: f32) -> Vec3 {
        // The point moved onto the hit's plane, back along its line from the look point to `reach`.
        let onto = |p: Vec3, hit: &crate::stage_probe::Hit| {
            look_at + (p + hit.normal * hit.distance(p) - look_at).normalize_or_zero() * reach
        };
        let start = match probe.segment(self.eye, wanted) {
            Some(hit) => Some(onto(wanted, &hit)),
            None => probe.segment(look_at, wanted).map(|_| wanted),
        };
        let Some(mut place) = start else {
            self.wanted_push *= WANTED_PUSH_DECAY;
            return wanted + self.wanted_push;
        };
        let mut hit = None;
        for _ in 0..5 {
            hit = probe.segment(look_at, place);
            let Some(h) = &hit else { break };
            place = onto(place, h);
        }
        if hit.is_some() {
            for _ in 0..6 {
                hit = probe.segment(look_at, place);
                let Some(h) = &hit else { break };
                place += h.normal * (h.distance(place) + NEAR + 0.2);
            }
            if let Some(h) = hit {
                let off = h.point - look_at;
                let length = off.length();
                place = if reach > length {
                    h.point + h.normal * (NEAR + 0.1)
                } else if length > 0.0 {
                    look_at + off * (reach / length)
                } else {
                    off
                };
            }
        }
        self.wanted_push = probe.push_out(place, NEAR + 0.3).1 - wanted;
        wanted + self.wanted_push
    }

    /// The eye after the probes from the look point and from its place `before` this frame, or off nearby faces
    /// (0x4895db..0x48976f).
    fn probe_eye(&mut self, probe: &StageProbe, look_at: Vec3, before: Vec3) {
        let eye = self.eye;
        match probe.segment(look_at, eye).or_else(|| probe.segment(before, eye)) {
            Some(hit) => self.eye_push = hit.normal * (hit.distance(eye) + NEAR + 0.1),
            None => match probe.push_out(eye, NEAR) {
                (true, out) => self.eye_push = out - eye,
                (false, _) => self.eye_push *= EYE_PUSH_DECAY,
            },
        }
        self.eye += self.eye_push;
    }

    /// The look-back swing after `dt`: towards pi while looking back, else on to a whole turn and then 0.
    fn next_look_back(&self, dt: f32, looking_back: bool) -> f32 {
        let pi = std::f32::consts::PI;
        let tau = std::f32::consts::TAU;
        if looking_back {
            (pi - self.look_back) * dt * LOOK_BACK_RATE + self.look_back
        } else if self.look_back == 0.0 || (self.look_back - tau).abs() < LOOK_BACK_SNAP {
            0.0
        } else {
            (tau - self.look_back) * dt * LOOK_BACK_RATE + self.look_back
        }
    }

    /// `M6530`: the eye and the point it looks at, the look point lowered by `RADIUS * sin(VW_ANGLE) /
    /// sin(3 pi / 2 - VW_ANGLE + CAM_ANGLE)`, which is negative: it rises by 0.84 m.
    pub fn view(&self, car: &Car) -> (Vec3, Vec3) {
        let (a, b) = (self.settings.angle_degrees.to_radians(), self.settings.view_angle_degrees.to_radians());
        let drop = b.sin() * self.settings.radius / (std::f32::consts::FRAC_PI_2 * 3.0 - b + a).sin();
        (self.eye, car.look_at() - Vec3::Z * drop)
    }
}

/// The chase camera following a drive's frames: it ticks by the drive time between the frames it is shown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Follow {
    chase: Chase,
    /// The frame last shown and its drive time, `None` after a reset.
    last: Option<(usize, f64)>,
    car: Option<Car>,
    /// The start orbit's distance from the look point while it runs ([`orbit_reach`]).
    orbit: Option<f32>,
}

impl Follow {
    /// A camera with `settings`.
    pub fn new(settings: Settings) -> Follow {
        Follow { chase: Chase { settings: settings.clamped(), ..Chase::default() }, ..Follow::default() }
    }

    /// Uses the player's camera settings from the next frame on.
    pub fn set_settings(&mut self, settings: Settings) {
        self.chase.settings = settings.clamped();
    }

    /// Moves the camera to frame `index` of a drive whose frame `i` ends at drive time `ends[i]` with the car at
    /// `pose(i)`. `swing` is the start orbit's angle while it runs, `looking_back` the look-back input and `probe` the
    /// stage's collision mesh. A fresh camera is settled over the frames before `index`.
    pub fn follow(
        &mut self,
        ends: &[f64],
        pose: impl Fn(usize) -> CarPose,
        index: usize,
        swing: Option<f32>,
        looking_back: bool,
        probe: Option<&StageProbe>,
    ) {
        let Some(&now) = ends.get(index) else {
            return;
        };
        let (first, mut then) = match self.last {
            Some((i, then)) if i <= index && now - then <= JUMP_SECONDS => (i + 1, Some(then)),
            _ => (index.saturating_sub(SETTLE_FRAMES), None),
        };
        for (i, &end) in ends.iter().enumerate().take(index + 1).skip(first) {
            let car = Car::from_pose(&pose(i));
            let dt = then.map_or(0.0, |t| (end - t) as f32);
            let snap = then.is_none() || swing.is_some();
            let looking_back = looking_back && swing.is_none();
            if then.is_none() {
                // A fresh eye starts at its place without the probes, not at the origin (ours).
                self.chase.tic(&car, dt, swing.unwrap_or(0.0), looking_back, true, None);
            }
            self.chase.tic(&car, dt, swing.unwrap_or(0.0), looking_back, snap, probe);
            self.orbit = match (swing, probe) {
                (Some(_), Some(probe)) => Some(orbit_reach(probe, car.look_at(), self.chase.eye, self.orbit, dt)),
                _ => None,
            };
            then = Some(end);
            self.car = Some(car);
        }
        self.last = Some((index, now));
    }

    /// The camera at the frame last followed, in stage axes, or `None` before the first. Its `forward` reaches the point
    /// it looks at (not unit): [`crate::playback::Playback::lens`] turns the view about that point.
    pub fn lens(&self) -> Option<Lens> {
        let car = self.car?;
        let (mut eye, at) = self.chase.view(&car);
        if let Some(reach) = self.orbit {
            let look = car.look_at();
            eye = look + (eye - look).clamp_length_max(reach);
        }
        let stage = |v: Vec3| Vec3::new(v.x, v.z, v.y);
        let fov = crate::playback::CHASE_FOV_DEGREES.to_radians();
        Some(Lens { eye: stage(eye), forward: stage(at - eye), up: Vec3::Y, fov, near: NEAR })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level_car(position: Vec3) -> Car {
        Car { position, axes: [Vec3::X, Vec3::Y, Vec3::Z] }
    }

    #[test]
    fn the_game_field_of_view_is_about_71_degrees_across() {
        assert!((fov(FOV_DEGREES).to_degrees() - 70.954).abs() < 0.01);
    }

    #[test]
    fn the_camera_page_moves_the_eye() {
        let car = level_car(Vec3::ZERO);
        let settings = Settings { radius: 6.0, angle_degrees: 16.0, view_angle_degrees: 9.0, ..Settings::GAME };
        let mut chase = Chase { settings, ..Chase::default() };
        chase.tic(&car, 0.01, 0.0, false, true, None);
        let a = 16f32.to_radians();
        let wanted = car.look_at() + Vec3::new(0.0, a.cos() * 6.0, a.sin() * 6.0);
        assert!((chase.eye - wanted).length() < 1e-5, "{:?}", chase.eye);
        let far = Settings { radius: 19.0, angle_degrees: -5.0, view_angle_degrees: 20.0, ..Settings::GAME }.clamped();
        assert_eq!(far, Settings { radius: 12.0, angle_degrees: 0.0, view_angle_degrees: 20.0, ..Settings::GAME });
    }

    #[test]
    fn a_nudge_moves_the_chase_eye_nearer_and_up() {
        let base = Settings { radius: 5.0, angle_degrees: 20.0, view_angle_degrees: 4.0, ..Settings::GAME };
        let nudged = base.nudged(Vec3::new(0.0, 0.5, 1.0));
        assert!((nudged.radius - 4.0).abs() < 1e-5);
        // The height 5 sin 20 goes up by 0.5 at radius 4.
        let height = 5.0 * 20f32.to_radians().sin() + 0.5;
        assert!((nudged.radius * nudged.angle_degrees.to_radians().sin() - height).abs() < 1e-4, "{nudged:?}");
        assert_eq!(nudged.view_angle_degrees, 4.0);
        assert!((base.nudged(Vec3::ZERO).angle_degrees - 20.0).abs() < 1e-4);
        assert_eq!(base.nudged(Vec3::new(0.0, 0.0, 10.0)).radius, 2.5);
    }

    #[test]
    fn the_eye_snaps_behind_and_above_the_look_point() {
        let car = level_car(Vec3::new(10.0, 20.0, 5.0));
        let mut chase = Chase::default();
        chase.tic(&car, 0.01, 0.0, false, true, None);
        let a = ANGLE_DEGREES.to_radians();
        let look_at = Vec3::new(10.0, 20.0 - 1.544307, 5.75);
        let wanted = look_at + Vec3::new(0.0, a.cos() * RADIUS, a.sin() * RADIUS);
        assert!((chase.eye - wanted).length() < 1e-5, "{:?}", chase.eye);
        // M6530 raises the look point by 0.84 m.
        let (_, at) = chase.view(&car);
        assert!((at.z - look_at.z - 0.8402).abs() < 1e-3, "{at:?}");
    }

    #[test]
    fn the_eye_lags_and_keeps_its_distance() {
        let mut chase = Chase::default();
        chase.tic(&level_car(Vec3::ZERO), 0.01, 0.0, false, true, None);
        let start = chase.eye;
        // The car jumps 0.5 m forward (-y): the eye follows by the lag's share, about 1.25% at 100 frames a second.
        let car = level_car(Vec3::new(0.0, -0.5, 0.0));
        chase.tic(&car, 0.01, 0.0, false, false, None);
        let moved = start.y - chase.eye.y;
        assert!(moved > 0.0 && moved < 0.05, "{moved}");
        // Far away, the distance clamp pulls it to RADIUS + PAD in the horizontal plane.
        let far = level_car(Vec3::new(0.0, -100.0, 0.0));
        chase.tic(&far, 0.01, 0.0, false, false, None);
        let flat = (far.look_at() - chase.eye).truncate().length();
        assert!((flat - (RADIUS + PAD)).abs() < 1e-3, "{flat}");
    }

    #[test]
    fn the_steady_chase_keeps_its_distance_at_the_start_and_at_speed() {
        let mut chase = Chase { settings: Settings::default(), ..Chase::default() };
        let snapped = |chase: &Chase, car: &Car| (car.look_at() - chase.eye).truncate().length();
        chase.tic(&level_car(Vec3::ZERO), 0.01, 0.0, false, true, None);
        let start = snapped(&chase, &level_car(Vec3::ZERO));
        let a = CHASE_ANGLE_DEGREES.to_radians();
        assert!((start - CHASE_RADIUS * a.cos()).abs() < 1e-4, "{start}");
        // 30 m/s for 5 s: the game's eye trails out to RADIUS + PAD, the steady one stays.
        let mut car = level_car(Vec3::ZERO);
        for i in 1..=500 {
            car = level_car(Vec3::new(0.0, -0.3 * i as f32, 0.0));
            chase.tic(&car, 0.01, 0.0, false, false, None);
        }
        assert!((snapped(&chase, &car) - start).abs() < 1e-3, "{}", snapped(&chase, &car));
    }

    #[test]
    fn the_start_orbit_swings_the_eye_in_front_of_the_car() {
        let car = level_car(Vec3::ZERO);
        let mut chase = Chase::default();
        chase.tic(&car, 0.01, std::f32::consts::PI, false, true, None);
        // At pi the eye is ahead (-y) of the look point.
        assert!(chase.eye.y < car.look_at().y - 4.0, "{:?}", chase.eye);
    }

    #[test]
    fn looking_back_turns_the_eye_and_returns() {
        let car = level_car(Vec3::ZERO);
        let mut chase = Chase::default();
        for _ in 0..500 {
            chase.tic(&car, 0.01, 0.0, true, true, None);
        }
        assert!((chase.look_back - std::f32::consts::PI).abs() < 1e-2);
        for _ in 0..1000 {
            chase.tic(&car, 0.01, 0.0, false, true, None);
        }
        assert_eq!(chase.look_back, 0.0);
    }

    #[test]
    fn a_wall_behind_the_car_keeps_the_eye_in_front_of_it() {
        // A wall across the car's rear 2 m behind the look point, facing the car (+y is the rear).
        let y = -1.544307 + 2.0;
        let (a, b, c, d) =
            (Vec3::new(-20.0, y, -5.0), Vec3::new(20.0, y, -5.0), Vec3::new(20.0, y, 20.0), Vec3::new(-20.0, y, 20.0));
        let wall = StageProbe::from_triangles(&[[a, c, b], [a, d, c]]);
        assert!(wall.segment(Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 5.0, 1.0)).is_some(), "the wall faces the car");
        let car = level_car(Vec3::ZERO);
        let mut chase = Chase::default();
        chase.tic(&car, 0.01, 0.0, false, true, Some(&wall));
        assert!(chase.eye.y < y, "snapped in front of the wall: {:?}", chase.eye);
        for _ in 0..200 {
            chase.tic(&car, 0.01, 0.0, false, false, Some(&wall));
            assert!(chase.eye.y < y, "{:?}", chase.eye);
        }
        // Without the wall the eye sits behind where it was.
        let mut free = Chase::default();
        free.tic(&car, 0.01, 0.0, false, true, None);
        assert!(free.eye.y > y);
    }

    #[test]
    fn the_start_orbit_is_drawn_in_front_of_a_pillar_beside_its_sight_line() {
        // A pillar face 3 m out along +x from the look point, facing it, 0.4 m off the line of sight.
        let (a, b, c, d) = (
            Vec3::new(3.0, 0.4, -5.0),
            Vec3::new(3.0, 1.0, -5.0),
            Vec3::new(3.0, 1.0, 10.0),
            Vec3::new(3.0, 0.4, 10.0),
        );
        let pillar = StageProbe::from_triangles(&[[a, b, c], [a, c, d], [a, c, b], [a, d, c]]);
        let (look, eye) = (Vec3::ZERO, Vec3::new(8.0, 0.0, 0.5));
        assert!(pillar.segment(look, eye).is_none(), "the sight line itself is clear");
        let reach = orbit_reach(&pillar, look, eye, None, 0.007);
        assert!((reach - 2.406).abs() < 1e-3, "{reach}");
        // Eased from the full distance, and back out once clear.
        let eased = orbit_reach(&pillar, look, eye, Some(eye.length()), 0.007);
        assert!(eased < eye.length() && eased > 2.4, "{eased}");
        assert_eq!(orbit_reach(&StageProbe::from_triangles(&[]), look, eye, None, 0.007), eye.length());
        let mut follow = Follow::default();
        let pose = CarPose { position: [0.0; 3], orientation: [0.0, 0.0, 0.0, 1.0], velocity: [0.0; 3] };
        follow.follow(&[0.0], |_| pose, 0, Some(1.0), false, Some(&pillar));
        assert!(follow.orbit.is_some());
        follow.follow(&[0.0, 0.007], |_| pose, 1, None, false, Some(&pillar));
        assert_eq!(follow.orbit, None, "only in the orbit");
    }

    #[test]
    fn the_bonnet_eye_is_lifted_off_the_ground_only_when_close() {
        let (a, b, c, d) = (
            Vec3::new(-10.0, -10.0, 0.0),
            Vec3::new(10.0, -10.0, 0.0),
            Vec3::new(10.0, 10.0, 0.0),
            Vec3::new(-10.0, 10.0, 0.0),
        );
        let floor = StageProbe::from_triangles(&[[a, c, b], [a, d, c]]);
        let low = bonnet_lift(&floor, Vec3::new(0.0, 0.0, 0.1), 0.08);
        assert!((low.z - 0.18).abs() < 1e-5, "{low:?}");
        assert_eq!(bonnet_lift(&floor, Vec3::new(0.0, 0.0, 1.0), 0.08), Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn following_settles_a_fresh_camera_and_snaps_on_a_jump() {
        let pose = |y: f32| CarPose { position: [0.0, y, 0.0], orientation: [0.0, 0.0, 0.0, 1.0], velocity: [0.0; 3] };
        let poses: Vec<CarPose> = (0..1000).map(|i| pose(-0.2 * i as f32)).collect();
        let ends: Vec<f64> = (1..=1000).map(|i| i as f64 * 0.01).collect();
        let mut follow = Follow::default();
        follow.follow(&ends, |i| poses[i], 500, None, false, None);
        // Settled at 20 m/s: the eye trails its snapped place.
        let lens = follow.lens().unwrap();
        let car = Car::from_pose(&poses[500]);
        let behind = lens.eye.z - car.look_at().y;
        assert!(behind > 4.0 && behind <= RADIUS + PAD + 1e-3, "{behind}");
        follow.follow(&ends, |i| poses[i], 900, None, false, None);
        let car = Car::from_pose(&poses[900]);
        assert!((follow.lens().unwrap().eye.z - car.look_at().y - behind).abs() < 0.05);
    }
}
