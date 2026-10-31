//! Autopilot (`rbr-standalone --autopilot OUT`): a driveline-following controller
//! that drives the pure step from a snapshot and writes the controls as a playback (`harness/play.sh`'s
//! `RBRRecompPlayback.bin`), so the original can replay them open-loop.
//!
//! The playback is the drive's own recorded frames before the snapshot's step, then one frame per live step
//! (`live/`): the snapshot's own step with its recorded control, and from the next step on the controller's. Each
//! frame holds the pure car and wheels after its step, so the playback is also the Rust run's recording.
//! From `--boot` the live steps start after the boot's load settle and take a neutral
//! control first; the playback then has no settle frames (the plugin plays single steps only). The race clock's 7 s
//! start countdown runs over those steps, so the controller holds the car on the handbrake until it is out: a
//! playback that left before it would take the jump start again wherever it is replayed (`--live --pilot`, the game).
//!
//! The controller is not an original function: it steers to a look-ahead point on the `.trk` driveline (a Hermite
//! spline), holds a target speed from the driveline's curvature ahead, and shifts by engine speed. It stops a few
//! hundred steps after the bound time keeper records the finish (with `--saves`, at the race's end, which writes the
//! records as a live drive does), or when the car is stuck or off the road.
//!
//! Stuck recovery (ours): when the car, having moved, stays under `STUCK_SPEED` (or under `BACKWARDS_SPEED` facing
//! away from the driveline) for `RECOVER_AFTER` steps, the controller brakes, shifts to reverse and backs up with the
//! wheels turned to swing the nose towards the driveline, then shifts back to first and drives on. It tries
//! `ATTEMPTS` times; the count starts again once the car is `PROGRESS` m further along the driveline. After the last
//! attempt, or at once when the car is on its side or roof, the run ends as stuck. Upshifts wait while the driven
//! wheels spin (engine speed over `SLIP` times the lowest engine speed per m/s seen in the gear): shifting up on
//! wheelspin left cars in a high gear in slides and on climbs, where they stalled or rolled (track-25_O, track-35_N).
//!
//! On RSF stages (ours): the target swings around heavy props near the driveline (`HEAVY`),
//! a driveline segment that loops is flattened (`LOOP`, `BACK`), and a car leaning over steers towards its low side
//! (`TIP`).
//!
//! Under NGP (ours): upshifts come before the car's rev limit, a standing start revs in
//! neutral before it selects first, a car bogged in first goes back to neutral and starts again, the throttle
//! eases while the wheels spin, and a recovery backs up no faster than `REVERSE_TOP`. NGP's clutch help closes the clutch from idle, where the Escort Mk II and the 208 R2
//! have too little torque to pull away, and they sat in first at the clutch help's speed.

use rbr_formats::trk::{DriveLineRecord, Vec3};
use rbr_gameplay::control::QuantizedControl;
use rbr_hybrid::pure;
use rbr_recording::hud::HUD2_BYTES;
use rbr_recording::pose::{self, CarPose};
use rbr_recording::{read_f32, read_u32, CONTROL_BYTES, HEADER_BYTES, WHEELS, WHEEL_BYTES};

mod drive;

pub use drive::{drive, from_snapshot, Pilot};

use crate::live::Live;
use crate::physics::DRIVELINE_OFFSET;

/// Look-ahead of the steering target: `LOOK_BASE + LOOK_TIME * speed` metres. A base of 6 m cut the inside of slow
/// hairpins into barriers and banks (RSF 125, 143, 310, 338, 428, 445 rolled or stuck); 3 m drives 465 of the 470 plain
/// RSF variants to the step limit.
const LOOK_BASE: f32 = 3.0;
const LOOK_TIME: f32 = 0.5;
/// Steering per radian of heading error to the target (full lock at about 20 degrees).
const STEER_GAIN: f32 = 3.0;
/// The speed plan: lateral and braking acceleration (m/s²), the default top speed (m/s, `--top-speed`), and how far
/// ahead it looks (m).
const LATERAL: f32 = 5.0;
const BRAKING: f32 = 4.0;
const TOP_SPEED: f32 = 30.0;
/// The target speed while the steering is past full lock (m/s): the curve plan missed hairpins at full lock on RSF
/// stages 122 and 302.
const FULL_LOCK_TOP: f32 = 6.0;
const PLAN_AHEAD: f32 = 150.0;
const PLAN_STEP: f32 = 3.0;
/// Shift points (engine speed, rad/s), the steps between shifts, and the gear index of first and the top gear (neutral is below first).
const SHIFT_UP: f32 = 680.0;
const SHIFT_DOWN: f32 = 380.0;
const SHIFT_GAP: u32 = 150;
/// Upshifts come at `UP_LIMIT` of the car's rev limit when that is below `SHIFT_UP` (NGP cars: the Evo IX's limiter is
/// at 6200 rpm; 1.02's 7800 and 8500 rpm leave `SHIFT_UP`).
const UP_LIMIT: f32 = 0.93;
/// Under NGP a standing start revs to `LAUNCH` of the rev limit in neutral before it selects first, and a car bogged in
/// first (engine under `BOG` of the limit below `BOG_SPEED` m/s for `BOG_STEPS` steps) goes back to neutral to do it
/// again. NGP's clutch help closes the clutch from about idle on (`notes/ngp.md`, "Controls under NGP"), and an engine
/// with little torque there (Escort Mk II, 208 R2) cannot pull the car from it.
const LAUNCH: f32 = 0.6;
const BOG: f32 = 0.2;
const BOG_SPEED: f32 = 3.0;
const BOG_STEPS: u32 = 100;
/// Under NGP the throttle while the driven wheels spin (`SLIP`) above `BOG_SPEED`: full throttle spun the rear-driven
/// Escort Mk II off the road on gravel.
const TRACTION: f32 = 0.5;
const FIRST: u32 = 2;
const TOP_GEAR: u32 = 7;
/// Steps driven on (braking) after the finish.
const AFTER_FINISH: u32 = 300;
/// The car counts as stuck, once it has moved faster than `MOVING` (m/s), while under `STUCK_SPEED` before the finish
/// (recovery: module comment). The run ends as off the road after `OFF_ROAD_STEPS` steps (2 s) over `OFF_ROAD` m from
/// the driveline. Neither with `--aim`, which leaves the road on purpose.
const MOVING: f32 = 5.0;
const STUCK_SPEED: f32 = 1.0;
const OFF_ROAD: f32 = 15.0;
const OFF_ROAD_STEPS: u32 = 300;
/// Stuck recovery (module comment). Step counts here are 1.02's steps of 7 ms; a run of shorter steps (NGP's 1/720 s)
/// scales them (`Driver::with_step`).
const RECOVER_AFTER: u32 = 280;
const BACKWARDS: f32 = 2.0;
const BACKWARDS_SPEED: f32 = 3.0;
const ATTEMPTS: u32 = 4;
const PROGRESS: f32 = 50.0;
/// Steps between gear pulses while recovering, the reverse's throttle, its shortest and longest time (from the start
/// of the recovery), and the heading error (rad) to the steering target that ends it early.
const RECOVER_GAP: u32 = 20;
const REVERSE_THROTTLE: f32 = 0.5;
/// Under NGP the reverse's throttle is off above this speed (m/s): at `REVERSE_THROTTLE` an NGP Evo IX backed up at
/// 11.7 m/s and off the road (RSF stage 122).
const REVERSE_TOP: f32 = 3.0;
const REVERSE_MIN: u32 = 300;
const REVERSE_MAX: u32 = 700;
const AIMED: f32 = 0.3;
const REVERSE: u32 = 0;
/// An outside assistance holds the car until its spectators are done (`race_clock`'s timer at 4.5 s, and a step or two:
/// a pulse at 4.48 s did nothing, one at 4.6 s took): gear pulses in that time do nothing (RSF stages 122 and 302 sat
/// in neutral after one, then backed up without reverse). The controller waits on the brake until then and starts
/// again from neutral.
const HELD: f32 = 4.6;
/// A stuck car whose up axis has a world z under this is on its side or roof: backing up cannot help.
const ROLLED: f32 = 0.5;
/// While the car leans sideways by more than this (the up axis's component along the car's horizontal right, about 15
/// degrees), it steers towards the low side: on RSF 396's 90-degree left at 636 m the driveline runs along the foot
/// of a steep bank, and at full lock the left wheels climbed it until the car fell on its side at 6 m/s.
const TIP: f32 = 0.26;
/// An upshift waits while the engine speed is over `SLIP` times the gear's lowest engine speed per m/s (learnt above
/// `RATIO_SPEED` m/s under throttle without brake, `RATIO_SETTLE` steps after a shift: the clutch is in, and the wheels
/// can only turn faster than the road).
const SLIP: f32 = 1.4;
const RATIO_SPEED: f32 = 3.0;
const RATIO_SETTLE: u32 = 70;
/// `CCar` +0x4b0 +8: the drive state; its +0x45c the engine speed the VCU reads (`CDrive::Get4520`, 0x504520).
const DRIVE_STATE: u32 = 0x4b8;
const DRIVE_ENGINE_SPEED: u32 = 0x45c;
/// `CCar` +0x850: the VCU's definition; its +0x188 the rev limit (`RPMLimit`, rpm).
const VCU_DEF: u32 = 0x850;
const VCU_RPM_LIMIT: u32 = 0x188;
/// An answer's frame (`live/`); the HUD2 record follows it, with the gear as its word 1.
const FRAME_BYTES: usize = HEADER_BYTES + CONTROL_BYTES + pure::CAR_BYTES + WHEELS * WHEEL_BYTES;
/// `--aim`: the controller steers at the point instead of the driveline while it is closer than `AIM_RANGE` metres and
/// ahead; the drive ends `AIM_AFTER` steps after the car first comes within `AIM_HIT` metres of it.
const AIM_RANGE: f32 = 60.0;
const AIM_HIT: f32 = 4.0;
const AIM_AFTER: u32 = 600;

/// `--aim X,Y[,COAST]` (scenery drives): a stage point to drive into, and how many steps to
/// keep the last control after the pure step stops (the original replays them; the playback's car state then repeats
/// the last good step's, which the plugin does not read).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub at: [f32; 2],
    pub coast: u32,
}

impl std::str::FromStr for Aim {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let v: Vec<f32> =
            s.split(',').map(|x| x.parse().map_err(|_| format!("--aim {s}"))).collect::<Result<_, _>>()?;
        match v[..] {
            [x, y] => Ok(Aim { at: [x, y], coast: 0 }),
            [x, y, c] => Ok(Aim { at: [x, y], coast: c as u32 }),
            _ => Err(format!("--aim {s}: want X,Y[,COAST]")),
        }
    }
}

/// Heavy props (ours): the collision meshes of kinds 2 (concrete block, `PIG`, 4000 kg), 8 (`LOG_POLE`) and 10
/// (`WOODEN_FENCE_POLE`, both 1000 kg; rbr-recomp `notes/interactive-scenery.md` 1c, 3.2). The controller steers around those
/// near the driveline, at most `DODGE_TOP` m/s: RSF 468's driveline runs through a chicane of 1000 kg boxes at
/// 711-738 m, and the car hit the first at 22 m/s and rolled. Lighter props (cones, signs, banners, logs, fence bars)
/// are driven through, and the fixed scenery is not looked at.
const HEAVY: [u32; 3] = [2, 8, 10];
/// Lateral clearance from a prop's footprint to the car's centre line (m; the car is about 1.75 m wide); how far the
/// target may swing off the driveline (m: the road's edges are not known, so a chicane is passed prop by prop); how
/// long a prop stays in view after the car's centre passes it (m); how far before a prop the target swings past the
/// look-ahead point, and the speed is held at `DODGE_TOP` m/s for one on the line (m).
const CLEAR: f32 = 1.6;
const SWING: f32 = 2.0;
const PASSED: f32 = 2.2;
const LEAD: f32 = 10.0;
const DODGE_TOP: f32 = 4.0;

/// A heavy prop's footprint: its collision mesh's vertices placed in the stage (x, y).
#[derive(Clone, Debug, PartialEq)]
pub struct Obstacle {
    pub footprint: Vec<[f32; 2]>,
}

/// The heavy props of a stage's `.trk` collision meshes (`HEAVY`).
pub fn obstacles(trk: &rbr_formats::trk::TrkFile) -> Vec<Obstacle> {
    use rbr_formats::trk::ChunkPayload;
    let meshes = trk.chunks.iter().filter_map(|c| match &c.payload {
        ChunkPayload::CollisionMeshes(m) => Some(&m.meshes),
        _ => None,
    });
    let heavy = meshes.flatten().filter(|m| HEAVY.contains(&m.kind) && !m.vertices.is_empty());
    let placed = heavy.flat_map(|mesh| {
        mesh.objects.iter().map(move |o| {
            let at = CarPose {
                position: [o.position.x, o.position.y, o.position.z],
                orientation: o.values,
                velocity: [0.0; 3],
            };
            let footprint = mesh.vertices.iter().map(|v| {
                let w = at.to_world([v.x * o.extent.x, v.y * o.extent.y, v.z * o.extent.z]);
                [w[0], w[1]]
            });
            Obstacle { footprint: footprint.collect() }
        })
    });
    placed.collect()
}

/// A prop along the driveline: its footprint's extent along it (m) and to its left (m, negative right).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placed {
    from: f32,
    to: f32,
    left: [f32; 2],
}

/// A driveline tangent longer than `LOOP` times its segment's chord makes the segment loop: RSF stage 302's driveline
/// starts with a 10.7 m segment that ends on a 164 m tangent, and the car steered off the road at the start. So does a
/// segment that runs back along its chord by more than `BACK` m: RSF 370's 13 m segment at 454 m ends on an 86 m
/// tangent (6.6 times the chord) and bulges 5 m to the side, and the car followed it into the bank. The controller
/// shortens such a segment's tangents to the chord (ours; the game's driveline parameter still follows the loop). The
/// retail drivelines' largest ratio is 5.9 and their furthest run back 1.0 m (both track-51_N), so they are unchanged.
const LOOP: f32 = 8.0;
const BACK: f32 = 1.5;

/// How far the Hermite segment `p0`..`p1` with tangents `m0`, `m1` runs back along its chord (m), sampled.
fn runs_back(p0: [f32; 3], m0: [f32; 3], p1: [f32; 3], m1: [f32; 3]) -> f32 {
    let chord = sub(p1, p0);
    let length = dot(chord, chord).sqrt();
    let (mut ahead, mut back) = (0.0f32, 0.0f32);
    for k in 1..=16 {
        let along = dot(sub(hermite(p0, m0, p1, m1, k as f32 / 16.0), p0), chord) / length;
        ahead = ahead.max(along);
        back = back.max(ahead - along);
    }
    back
}

fn hermite(p0: [f32; 3], m0: [f32; 3], p1: [f32; 3], m1: [f32; 3], t: f32) -> [f32; 3] {
    let (t2, t3) = (t * t, t * t * t);
    let h = [2.0 * t3 - 3.0 * t2 + 1.0, t3 - 2.0 * t2 + t, -2.0 * t3 + 3.0 * t2, t3 - t2];
    std::array::from_fn(|k| h[0] * p0[k] + h[1] * m0[k] + h[2] * p1[k] + h[3] * m1[k])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Hermite position on the driveline at parameter `u` (record index plus fraction; `CCar` +0x17c).
fn point(track: &[DriveLineRecord], u: f32) -> [f32; 3] {
    let last = track.len() - 1;
    let u = u.clamp(0.0, last as f32);
    let i = (u as usize).min(last.saturating_sub(1));
    let t = u - i as f32;
    let v = |v: Vec3| [v.x, v.y, v.z];
    let length = |v: [f32; 3]| dot(v, v).sqrt();
    let (p0, p1) = (v(track[i].vectors[0]), v(track[i + 1].vectors[0]));
    let (m0, m1) = (v(track[i].vectors[1]), v(track[i + 1].vectors[1]));
    let chord = length(sub(p1, p0));
    let loops = runs_back(p0, m0, p1, m1) > BACK;
    let fit = |m: [f32; 3]| {
        let long = length(m) > chord && (loops || length(m) > LOOP * chord);
        let scale = if long { chord / length(m) } else { 1.0 };
        m.map(|x| x * scale)
    };
    hermite(p0, fit(m0), p1, fit(m1), t)
}

/// The driveline parameter at distance `d` along the stage (the inverse of `driveline::sample_x`).
fn parameter(values: &[f32], d: f32) -> f32 {
    let i = values.partition_point(|&v| v <= d).clamp(1, values.len() - 1) - 1;
    let span = values[i + 1] - values[i];
    i as f32 + ((d - values[i]) / span).clamp(0.0, 1.0)
}

fn distance(values: &[f32], u: f32) -> f32 {
    rbr_gameplay::driveline::sample_x(values, u.max(0.0)).unwrap_or(*values.last().unwrap())
}

/// Signed horizontal angle from `from` to `to` (z-up), positive counter-clockwise.
fn angle(from: [f32; 3], to: [f32; 3]) -> f32 {
    (from[0] * to[1] - from[1] * to[0]).atan2(from[0] * to[0] + from[1] * to[1])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|k| a[k] - b[k])
}

/// How far the car leans to its right: its up axis's component along its horizontal right.
fn lean(pose: &CarPose) -> f32 {
    let (forward, up) = (pose.forward(), pose.rotate([0.0, 0.0, 1.0]));
    let length = forward[0].hypot(forward[1]);
    (up[0] * forward[1] - up[1] * forward[0]) / length
}

/// What the controller reads after a step.
pub struct Sense {
    pub pose: CarPose,
    /// `CCar` +0x17c.
    pub parameter: f32,
    pub gear: u32,
    /// rad/s.
    pub engine: f32,
    /// An outside assistance holds the car (`HELD`).
    pub held: bool,
}

/// The car's sense from an answer's frame and HUD record, and the drive's engine speed as the VCU reads it (the drive
/// state's +0x45c, which the shift points were tuned on; the HUD's is the engine's own).
pub(crate) fn sense(answer: &[u8], live: &Live) -> Sense {
    let mem = live.mem();
    let assist = live.game().race_clock().assist_timer;
    let car = &answer[HEADER_BYTES + CONTROL_BYTES..];
    let hud = &answer[FRAME_BYTES..FRAME_BYTES + HUD2_BYTES];
    Sense {
        pose: pose::car_pose(car).expect("car block"),
        parameter: read_f32(car, DRIVELINE_OFFSET).expect("car +0x17c"),
        gear: read_u32(hud, 4).expect("HUD gear"),
        engine: mem.f32(mem.u32(mem.u32(pure::CAR_PTR) + DRIVE_STATE) + DRIVE_ENGINE_SPEED),
        held: (0.0..HELD).contains(&assist),
    }
}

/// What the controller is doing: following the driveline, backing up (since the step), or shifting back to first.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Drive,
    Reverse(u32),
    Forward,
}

/// The controller's state.
pub struct Driver<'a> {
    track: &'a [DriveLineRecord],
    values: Vec<f32>,
    /// The step of the last shift.
    shifted: u32,
    /// m/s.
    top: f32,
    aim: Option<Aim>,
    mode: Mode,
    /// The car has moved faster than `MOVING`.
    moved: bool,
    /// Steps stuck in a row while driving.
    slow: u32,
    /// Recoveries since the car last made `PROGRESS` m, and the distance at the last one.
    attempts: u32,
    stuck_at: f32,
    /// The run is stuck for good: why.
    pub gave_up: Option<String>,
    /// Per gear: the lowest engine speed per m/s seen.
    ratio: [f32; TOP_GEAR as usize + 1],
    /// The last control had throttle and no brake.
    pulling: bool,
    /// Physics steps per 1.02 step: the step counts here are 1.02's (7 ms), and NGP steps 1/720 s (5 per 7 ms).
    per: u32,
    /// The car's rev limit, rad/s.
    limit: f32,
    /// Steps bogged in first in a row (`BOG`).
    bogged: u32,
    /// The heavy props near the driveline, by `from`.
    props: Vec<Placed>,
}

impl<'a> Driver<'a> {
    pub fn new(track: &'a [DriveLineRecord], values: Vec<f32>, top: f32, aim: Option<Aim>) -> Self {
        Self {
            track,
            values,
            shifted: 0,
            top,
            aim,
            mode: Mode::Drive,
            moved: false,
            slow: 0,
            attempts: 0,
            stuck_at: 0.0,
            gave_up: None,
            ratio: [f32::INFINITY; TOP_GEAR as usize + 1],
            pulling: false,
            per: 1,
            limit: f32::INFINITY,
            bogged: 0,
            props: Vec::new(),
        }
    }

    /// The driver steering around `obstacles`: each is placed by its centre's nearest driveline point (sampled).
    pub fn with_obstacles(self, obstacles: &[Obstacle]) -> Self {
        let samples: Vec<(f32, [f32; 3])> =
            (0..(self.track.len() - 1) * 8).map(|k| k as f32 / 8.0).map(|u| (u, point(self.track, u))).collect();
        let mut props: Vec<Placed> = obstacles
            .iter()
            .filter_map(|o| {
                let n = o.footprint.len() as f32;
                let c = o.footprint.iter().fold([0.0; 2], |c, v| [c[0] + v[0] / n, c[1] + v[1] / n]);
                let off = |p: [f32; 3]| (p[0] - c[0]).hypot(p[1] - c[1]);
                let &(u, p) = samples.iter().min_by(|a, b| off(a.1).total_cmp(&off(b.1)))?;
                let ahead = sub(point(self.track, u + 0.01), p);
                let length = ahead[0].hypot(ahead[1]);
                let (t, at) = ([ahead[0] / length, ahead[1] / length], distance(&self.values, u));
                let span = |f: &dyn Fn([f32; 2]) -> f32| {
                    let values = o.footprint.iter().map(|v| f([v[0] - p[0], v[1] - p[1]]));
                    values.fold([f32::INFINITY, f32::NEG_INFINITY], |r, x| [r[0].min(x), r[1].max(x)])
                };
                let along = span(&|d| at + d[0] * t[0] + d[1] * t[1]);
                let left = span(&|d| d[1] * t[0] - d[0] * t[1]);
                (left[0] < SWING + CLEAR && left[1] > -SWING - CLEAR).then_some(Placed {
                    from: along[0],
                    to: along[1],
                    left,
                })
            })
            .collect();
        props.sort_by(|a, b| a.from.total_cmp(&b.from));
        Self { props, ..self }
    }

    /// How far left of the driveline (m) the car passes the props between `at` and `ahead`: the free offset nearest the
    /// driveline, within `SWING`, from as many of the props as allow one, nearest first.
    fn dodge(&self, at: f32, ahead: f32) -> f32 {
        let near: Vec<[f32; 2]> = self
            .props
            .iter()
            .filter(|p| p.to + PASSED >= at && p.from <= ahead)
            .map(|p| [p.left[0] - CLEAR, p.left[1] + CLEAR])
            .collect();
        (0..=near.len())
            .rev()
            .find_map(|n| {
                let blocked = |s: f32| near[..n].iter().any(|b| s > b[0] && s < b[1]);
                let edges = near[..n].iter().flatten().copied();
                let free = std::iter::once(0.0).chain(edges).filter(|s| s.abs() <= SWING && !blocked(*s));
                free.min_by(|a, b| a.abs().total_cmp(&b.abs()))
            })
            .unwrap_or(0.0)
    }

    /// The aim point when it is in range and ahead of the car.
    fn aim_target(&self, pose: &CarPose) -> Option<[f32; 3]> {
        let at = self.aim?.at;
        let to = sub([at[0], at[1], pose.position[2]], pose.position);
        let ahead = angle(pose.forward(), to).abs() < std::f32::consts::FRAC_PI_2;
        (to[0].hypot(to[1]) < AIM_RANGE && ahead).then_some([at[0], at[1], pose.position[2]])
    }

    /// The target speed: the slowest of the curve limits ahead, each reachable by braking.
    pub fn target_speed(&self, at: f32) -> f32 {
        let mut target = self.top;
        let mut s = 0.0;
        while s < PLAN_AHEAD {
            let u = |d: f32| parameter(&self.values, at + d);
            let (a, b, c) = (
                point(self.track, u(s)),
                point(self.track, u(s + PLAN_STEP)),
                point(self.track, u(s + 2.0 * PLAN_STEP)),
            );
            let turn = angle(sub(b, a), sub(c, b)).abs();
            let curvature = turn / PLAN_STEP;
            let limit = (LATERAL / curvature.max(1e-4)).sqrt().min(self.top);
            target = target.min((limit * limit + 2.0 * BRAKING * s).sqrt());
            s += PLAN_STEP;
        }
        let on_line = self.props.iter().filter(|p| p.left[0] - CLEAR < 0.0 && p.left[1] + CLEAR > 0.0);
        for p in on_line.filter(|p| p.to + PASSED >= at && p.from < at + PLAN_AHEAD) {
            let s = (p.from - LEAD - at).max(0.0);
            target = target.min((DODGE_TOP * DODGE_TOP + 2.0 * BRAKING * s).sqrt());
        }
        target
    }

    /// The driver for physics steps of `seconds` (the step counts scale to 1.02's 7 ms steps).
    pub fn with_step(self, seconds: f32) -> Self {
        Self { per: (rbr_recording::STEP_SECONDS / seconds).round().max(1.0) as u32, ..self }
    }

    /// The driver for a car whose rev limit is `rpm`.
    pub fn with_limit(self, rpm: f32) -> Self {
        Self { limit: rpm * std::f32::consts::PI / 30.0, ..self }
    }

    /// `n` 1.02 steps in this run's steps.
    fn steps(&self, n: u32) -> u32 {
        n * self.per
    }

    /// A gear pulse (`flags`) when `gap` 1.02 steps have passed since the last one.
    fn shift(&mut self, step: u32, gap: u32, flags: u8) -> u8 {
        if step < self.shifted + self.steps(gap) {
            return 0;
        }
        self.shifted = step;
        flags
    }

    /// Counts the steps stuck while driving and starts a recovery, or gives up, after `RECOVER_AFTER` of them.
    fn watch(&mut self, step: u32, at: f32, sense: &Sense, error: f32) {
        if self.mode != Mode::Drive {
            return;
        }
        let speed = sense.pose.speed();
        self.moved |= speed > MOVING;
        let stuck = speed < STUCK_SPEED || (speed < BACKWARDS_SPEED && error.abs() > BACKWARDS);
        self.slow = if self.moved && stuck { self.slow + 1 } else { 0 };
        if at > self.stuck_at + PROGRESS {
            self.attempts = 0;
        }
        if self.slow < self.steps(RECOVER_AFTER) {
            return;
        }
        self.slow = 0;
        if sense.pose.rotate([0.0, 0.0, 1.0])[2] < ROLLED {
            self.gave_up = Some(format!("stuck at step {step} (on its side or roof)"));
            return;
        }
        if self.attempts == ATTEMPTS {
            self.gave_up =
                Some(format!("stuck at step {step} (for {RECOVER_AFTER} steps after {ATTEMPTS} recoveries)"));
            return;
        }
        self.attempts += 1;
        self.stuck_at = at;
        self.mode = Mode::Reverse(step);
        eprintln!(
            "autopilot: step {step}: stuck at {at:.1} m in gear {}, backing up (attempt {})",
            sense.gear, self.attempts
        );
    }

    /// The control for `step` (emitted before it), from the state after the step before.
    pub fn control(&mut self, step: u32, sense: &Sense, finished: bool) -> QuantizedControl {
        if sense.held {
            (self.mode, self.slow, self.bogged) = (Mode::Drive, 0, 0);
            return QuantizedControl { brake: 255, ..QuantizedControl::default() };
        }
        let speed = sense.pose.speed();
        let at = distance(&self.values, sense.parameter);
        let look = LOOK_BASE + LOOK_TIME * speed;
        let target = self.aim_target(&sense.pose).unwrap_or_else(|| {
            let u = parameter(&self.values, at + look);
            let (p, s) = (point(self.track, u), self.dodge(at, at + look + LEAD));
            if s == 0.0 {
                return p;
            }
            let ahead = sub(point(self.track, u + 0.01), p);
            let length = ahead[0].hypot(ahead[1]);
            [p[0] - s * ahead[1] / length, p[1] + s * ahead[0] / length, p[2]]
        });
        let error = angle(sense.pose.forward(), sub(target, sense.pose.position));
        if finished {
            self.mode = Mode::Drive;
        } else if self.aim.is_none() {
            self.watch(step, at, sense, error);
        }
        // Counter-clockwise (left) error wants left (negative) steering; backing up, right swings the nose left.
        let (mut steer, mut throttle, mut brake, mut flags) = ((-error * STEER_GAIN).clamp(-1.0, 1.0), 0.0, 1.0, 0);
        match self.mode {
            Mode::Reverse(_) if sense.gear > REVERSE => {
                steer = 0.0;
                flags = self.shift(step, RECOVER_GAP, 0x02);
            }
            Mode::Reverse(since) => {
                let fast = self.per > 1 && speed > REVERSE_TOP;
                (steer, throttle, brake) = (-steer, if fast { 0.0 } else { REVERSE_THROTTLE }, 0.0);
                if step >= since + self.steps(REVERSE_MAX)
                    || (step >= since + self.steps(REVERSE_MIN) && error.abs() < AIMED)
                {
                    self.mode = Mode::Forward;
                }
            }
            Mode::Forward if sense.gear < FIRST => flags = self.shift(step, RECOVER_GAP, 0x01),
            Mode::Forward | Mode::Drive => {
                self.mode = Mode::Drive;
                let mut want = if finished { 0.0 } else { self.target_speed(at) };
                if (error * STEER_GAIN).abs() > 1.0 {
                    want = want.min(FULL_LOCK_TOP);
                }
                let lean = lean(&sense.pose);
                if lean.abs() > TIP {
                    steer = lean.signum();
                }
                let gap = want - speed;
                (throttle, brake) = if finished {
                    (0.0, 1.0)
                } else if gap > 0.0 {
                    ((0.4 + 0.2 * gap).min(1.0), 0.0)
                } else if gap > -1.5 {
                    (0.15, 0.0)
                } else {
                    (0.0, (-0.15 * gap).min(0.8))
                };
                let g = sense.gear.min(TOP_GEAR) as usize;
                // NGP's clutch help opens the clutch below about 100 rad/s even under a light throttle (`notes/ngp.md`,
                // "Controls under NGP"), so a run of NGP's steps learns only above the downshift speed.
                let engaged = self.per == 1 || sense.engine > SHIFT_DOWN;
                if self.pulling
                    && engaged
                    && speed > RATIO_SPEED
                    && sense.gear >= FIRST
                    && step >= self.shifted + self.steps(RATIO_SETTLE)
                {
                    self.ratio[g] = self.ratio[g].min(sense.engine / speed);
                }
                let spinning = sense.engine > SLIP * self.ratio[g] * speed;
                // NGP's steps (the clutch help above).
                let ngp = self.per > 1;
                let bogged = ngp && sense.gear == FIRST && speed < BOG_SPEED && sense.engine < BOG * self.limit;
                self.bogged = if bogged && step > self.shifted { self.bogged + 1 } else { 0 };
                let launch = !ngp || speed > BOG_SPEED || sense.engine > LAUNCH * self.limit;
                let up = SHIFT_UP.min(UP_LIMIT * self.limit);
                if ngp && spinning && speed > BOG_SPEED {
                    throttle = throttle.min(TRACTION);
                }
                if !finished {
                    if self.bogged > self.steps(BOG_STEPS) {
                        flags = self.shift(step, SHIFT_GAP, 0x02);
                    } else if (sense.gear < FIRST && launch)
                        || (sense.gear >= FIRST && sense.gear < TOP_GEAR && sense.engine > up && !spinning)
                    {
                        flags = self.shift(step, SHIFT_GAP, 0x01);
                    } else if sense.gear > FIRST && sense.engine < SHIFT_DOWN {
                        flags = self.shift(step, SHIFT_GAP, 0x02);
                    }
                }
            }
        }
        self.pulling = self.mode == Mode::Drive && throttle > 0.0 && brake == 0.0;
        let pedal = |x: f32| (x * 255.0).round() as u8;
        QuantizedControl {
            flags,
            steering: (steer * 127.5).round().clamp(-127.0, 127.0) as i8,
            throttle: pedal(throttle),
            brake: pedal(brake),
            ..QuantizedControl::default()
        }
    }
}

/// The car's rev limit (rpm).
fn rev_limit(live: &Live) -> f32 {
    let mem = live.mem();
    mem.f32(mem.u32(mem.u32(pure::CAR_PTR) + VCU_DEF) + VCU_RPM_LIMIT)
}

#[cfg(test)]
mod tests;
