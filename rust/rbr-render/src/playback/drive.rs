//! A recorded drive: its frames ([`Sample`]) with the drive time at the end of each, read from an `rbr-recording` file.

use crate::characters::scripted::Assist;
use glam::Vec3;
use rbr_recording::clock::StepClock;
use rbr_recording::input::Payload;
use rbr_recording::pose::{self, CarPose, WheelPose};
use rbr_recording::{Frames, WHEELS};

/// One frame's car and wheel state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// The step counter after the frame (`post`).
    pub step: u32,
    pub car: CarPose,
    pub wheels: [WheelPose; WHEELS],
    /// The control payload fed to the step, as floats: throttle, brake, handbrake, clutch and steering (positive to
    /// the right) in words 0 to 4 (`crate::live`).
    pub control: [f32; 8],
    /// The car's position along the stage's driveline, `CCar+0x17c` (`rbr_gameplay::driveline`): a `.trk` driveline
    /// record index with its fraction. 0 when the block is too short.
    pub track: f32,
    /// The car words the outside assistance writes (`characters::scripted::Assist`).
    pub assist: Option<Assist>,
    /// `CSigIncarShake` as `E45_P3210` (0x523210) builds it from the driver's seat spring: its five vectors (signal
    /// `+0x10` to `+0x50`), body frame, m. [`CAMERA_SHAKE`] is the views' (`crate::views::Mount::shaken`), and
    /// `characters::crew::lean` leans the driver by all five. Zero when the block is too short.
    pub shake: [Vec3; 5],
    /// The start hold `CCar+0x1670`: while it is positive the driver ignores the controls (`OnCarControl`'s gate).
    pub hold: f32,
    /// The brake the drive was given (`CDrive::Set30` 0x504450 stores it at `CCar+0x4b0` +0x3c): the pedal that brakes
    /// after the VCU's pedal swap, which the brake lamps follow (`CCarLightSystem::Set1e40` at `> 0.1`, in the same
    /// VCU calls). The control's brake word when the block is too short.
    pub brake_lamp: f32,
}

/// The shake vector the in-car views take: `CSigIncarShake` +0x30, the seat spring host's `+0x70`.
pub const CAMERA_SHAKE: usize = 2;

/// `CCar+0x17c`, the car's driveline position.
const CAR_TRACK: usize = 0x17c;
/// The driver's seat spring: `CPassenger` at `CCar+0x8b0` has its host at `CCar+0x1450`, whose `+0x90`, `+0x80`,
/// `+0x70`, `+0xa0` and `+0xb0` the incar shake (`E45_P3210`, 0x523210) sends as `CSigIncarShake` +0x10 to +0x50.
const CAR_SEAT_SHAKE: [usize; 5] = [0x1450 + 0x90, 0x1450 + 0x80, 0x1450 + 0x70, 0x1450 + 0xa0, 0x1450 + 0xb0];
/// `CCar+0x1670`, the start hold (`m_tOverride`; `CCar::Test1670` 0x4e6ec0).
const CAR_HOLD: usize = 0x1670;
/// `CCar+0x4b0` is the `CDrive`; its brake lane's value (`+0x30`, value at `+0xc`).
const CAR_DRIVE_BRAKE: usize = 0x4b0 + 0x3c;

/// A recording's frames with the drive time at the end of each.
#[derive(Debug, Clone, PartialEq)]
pub struct Drive {
    pub(super) samples: Vec<Sample>,
    pub(super) ends: Vec<f64>,
    /// The payload periods each frame stands for (a live frame of several, `rbr_recording::input::Input::frames`; 1
    /// for a recording's).
    pub(super) periods: Vec<u32>,
    /// What a step of the frames' counters is ([`StepClock`]: 1.02's unless a live NGP run says otherwise).
    pub(super) clock: StepClock,
}

impl Drive {
    /// Reads every frame of a recording file's bytes (1.02 steps).
    pub fn parse(bytes: &[u8]) -> Result<Drive, String> {
        Self::parse_with(bytes, StepClock::default())
    }

    /// Reads every frame of `bytes`, whose steps are `clock`'s (a live run's CLK1 record).
    pub fn parse_with(bytes: &[u8], clock: StepClock) -> Result<Drive, String> {
        let mut samples = Vec::new();
        let mut ends = Vec::new();
        let mut periods = Vec::new();
        let mut substeps = 0u64;
        for frame in Frames::new(bytes) {
            let frame = frame.map_err(|e| e.to_string())?;
            let car = pose::car_pose(frame.car).ok_or("car block too short")?;
            let wheels = (0..WHEELS)
                .map(|i| frame.wheel(i).and_then(pose::wheel_pose))
                .collect::<Option<Vec<_>>>()
                .and_then(|w| w.try_into().ok())
                .ok_or("wheel block too short")?;
            substeps += u64::from(frame.substeps);
            ends.push(clock.seconds(substeps));
            periods.push(Payload::read(&frame.control).map_or(1, |p| p.periods()));
            let track = rbr_recording::read_f32(frame.car, CAR_TRACK).unwrap_or(0.0);
            let assist = Assist::read(frame.car);
            let shake = CAR_SEAT_SHAKE.map(|at| {
                let v = (0..3).map(|k| rbr_recording::read_f32(frame.car, at + 4 * k)).collect::<Option<Vec<f32>>>();
                v.map_or(Vec3::ZERO, |v| Vec3::from_slice(&v))
            });
            let hold = rbr_recording::read_f32(frame.car, CAR_HOLD).unwrap_or(0.0);
            let control = frame.control_f32s();
            let brake_lamp = rbr_recording::read_f32(frame.car, CAR_DRIVE_BRAKE).unwrap_or(control[1]);
            samples.push(Sample { step: frame.post, car, wheels, control, track, assist, shake, hold, brake_lamp });
        }
        if samples.is_empty() {
            return Err("the recording has no frames".to_owned());
        }
        Ok(Drive { samples, ends, periods, clock })
    }

    /// Adds `more`'s frames after this drive's last (the live mode's frames as they come).
    pub fn extend(&mut self, more: Drive) {
        let end = self.ends.last().copied().unwrap_or(0.0);
        self.ends.extend(more.ends.iter().map(|e| e + end));
        self.periods.extend(more.periods);
        self.samples.extend(more.samples);
    }

    /// Every frame with the drive time at its end, in order.
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn frames(&self) -> impl Iterator<Item = (f64, &Sample)> + '_ {
        self.ends.iter().copied().zip(&self.samples)
    }

    /// The mean anchor of wheel `k` over the drive: its rest mount point in the car frame.
    pub(super) fn mean_anchor(&self, k: usize) -> Vec3 {
        let sum: Vec3 = self.samples.iter().map(|s| Vec3::from(s.wheels[k].anchor)).sum();
        sum / self.samples.len() as f32
    }
}
