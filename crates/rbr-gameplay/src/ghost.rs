//! The ghost car: recording the player's car and playing a recorded ghost back (`CGhostCarManager`,
//! 0x4925f0-0x493f50; the sample stream is `rbr_formats::ghost`).
//!
//! Pure: the host gives the car's pose and the frame time; nothing here reads or writes the physics.
//!
//! The game's frame (rbr-recomp `notes/marks-sound-ghost.md` 3.2-3.4):
//! - the car's per-frame update (`CCar::Vf28x` 0x4ea020) emits `CSigCarGeneralState` (position `car+0x140`, quaternion
//!   `car+0x100`), which reaches [`Recorder::car_state`] (`Update` 0x493240);
//! - then `CApplication::M004825b0` (0x4825b0), in state 1 and not paused, calls `Accumulate` (0x4939f0) with the
//!   frame's time: [`Recorder::frame`], and in playback [`Player::advance`] at the new time.
//!
//! So a sample is written on every second frame (the frame counter must be odd and new), stamped with the race time
//! before the frame's `Accumulate`; the playback reads the time after it, so a ghost played against the drive it was
//! recorded from runs one frame ahead of the car. A respawn ([`Recorder::respawn`], the `CSigRespawn` handler `SetPlayback` 0x492e90) makes the next
//! sample a key. A sample that moved more than about 1.95 m since the last one is preceded by in-between samples
//! interpolated from the last sample's pose, so each delta fits its 1/64 m bytes. The order of the car update and
//! `Accumulate` in one frame is inferred from the frame's call order (rbr-recomp `notes/cameras.md` 1), not traced to the call.

use crate::flow::Mode;
use rbr_formats::ghost::{Delta, Key, Sample, Stream, DELTA_SCALE, QUAT_SCALE, TIME_SCALE};

/// A car pose: the position (m, the physics frame) and the rotation quaternion `[x, y, z, w]`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}

/// Stream bytes a recording may fill: writing stops once the write offset reaches 0x4afe8 of the 0x4b000-byte buffer
/// (`Update`).
pub const BUFFER_LIMIT: usize = 0x4afe8;
/// `Update`'s in-between step factor (0x795e00 area, the f32 -0.5128205): one more sample per 1.95 m moved.
const STEP_FACTOR: f32 = -0.512_820_5;
/// `E18_QuatSlerp` (0x492160) blends linearly above this dot product (the f32 at 0x72fedc).
const SLERP_LINEAR: f32 = 0.99999;
/// `Playback`'s quaternion scale, the f32 0.0078431377 (1 / 127.5).
const QUAT_UNIT: f32 = f32::from_bits(0x3c00_8081);
/// `Playback`'s and `Record2`'s position step, 1/64 m.
const DELTA_UNIT: f32 = 0.015625;
/// `Playback`'s time scale, 1/1024 s.
const TIME_UNIT: f32 = 0.000_976_562_5;
/// `CGhostCarManager`'s frame counter pair at construction (`CtorA` 0x493db0 sets the sampled count to this).
const NEVER_SAMPLED: u32 = 0xdead_beaf;

/// Records the player's car (`Start` 0x492b60, `Update`, `Record1` 0x492730, `Record2` 0x4928c0, `Accumulate`).
#[derive(Debug, Clone, PartialEq)]
pub struct Recorder {
    samples: Vec<Sample>,
    /// The stream's length so far, run headers included (`m_write` less the buffer start).
    written: usize,
    /// The run being written: its kind as a key or not, and its count.
    run: Option<(bool, usize)>,
    /// Race time, s (`m_wtime->t`).
    time: f32,
    /// Frame time since the last sample, s (`m_accum`).
    accum: f32,
    /// Frames counted (`m_cntA`) and the count of the last sample (`m_cntB`).
    frames: u32,
    sampled: u32,
    /// The next sample is a key (`m_playback`).
    key_due: bool,
    /// The position the deltas have reached, after their rounding (`m_lastPos`).
    last: [f32; 3],
    /// The car's pose at the last sample (`m_state`), which the in-between samples start from.
    previous: Pose,
    /// `SetFlag(1)` (0x492ae0): bit 31 of `m_mode`, which makes `Accumulate` skip the frame.
    paused: bool,
}

impl Default for Recorder {
    fn default() -> Self {
        Recorder::new()
    }
}

impl Recorder {
    /// A recording from race time 0 (`Start`): the first car state is a key.
    pub fn new() -> Recorder {
        Recorder {
            samples: Vec::new(),
            written: 0,
            run: None,
            time: 0.0,
            accum: 0.0,
            frames: 0,
            sampled: NEVER_SAMPLED,
            key_due: true,
            last: [0.0; 3],
            previous: Pose::default(),
            paused: false,
        }
    }

    /// `SetFlag` (0x492ae0): sets or clears bit 31 of the mode. While it is set `Accumulate` returns at once, so the
    /// clock and the frame count stand still and no sample is written (the frame count must change for one).
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Race time, s.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// The car's `CSigCarGeneralState` of this frame (`Update`).
    pub fn car_state(&mut self, pose: Pose) {
        if self.key_due {
            if self.written < BUFFER_LIMIT {
                self.key(pose);
            }
            self.key_due = false;
            if self.written >= BUFFER_LIMIT {
                return;
            }
        } else {
            let count = self.frames;
            if self.sampled == count || count & 1 == 0 || self.written >= BUFFER_LIMIT {
                return;
            }
            self.sampled = count;
            let far = (0..3).map(|i| (f64::from(self.last[i]) - f64::from(pose.position[i])).abs()).fold(0.0, f64::max);
            // `_ftol2` of the x87 product: truncation toward zero.
            let steps = (far * f64::from(STEP_FACTOR)) as i32;
            let parts = (1 - steps) as u32;
            if parts > 1 {
                let save = self.time;
                let base = (save - self.accum).max(0.0);
                for i in 1..parts {
                    let f = i as f32 / parts as f32;
                    let between = Pose {
                        position: lerp(self.previous.position, pose.position, f),
                        rotation: slerp(self.previous.rotation, pose.rotation, f),
                    };
                    self.time = f * self.accum + base;
                    self.delta(between);
                }
                self.time = save;
            }
            self.delta(pose);
        }
        self.previous = pose;
        self.accum = 0.0;
    }

    /// One frame of `frame_seconds` in state 1 (`Accumulate`).
    pub fn frame(&mut self, frame_seconds: f32) {
        if self.paused {
            return;
        }
        self.frames = self.frames.wrapping_add(1);
        self.time += frame_seconds;
        self.accum += frame_seconds;
    }

    /// An outside-assistance penalty of `seconds`, which the HUD adds while the race clock runs (`M576710` calls
    /// `Accumulate` with it at 0x576828, after the frame's own `Accumulate` at 0x482677): the clock jumps by it and it
    /// counts as a frame, so the samples' times and the stream's end time ([`end_time`], the best ghost's ranking)
    /// include the penalties as the race time does. In playback the ghost is played at that time too. The jump start's
    /// penalty does not reach it.
    pub fn penalty(&mut self, seconds: f32) {
        self.frame(seconds);
    }

    /// The car was put back on the road (`SetPlayback`): the next sample is a key.
    pub fn respawn(&mut self) {
        self.key_due = true;
    }

    /// The recording so far, ended with the zero header word `BeginWrite` (0x492ca0) writes.
    pub fn finish(&self) -> Stream {
        Stream { samples: self.samples.clone(), tail: vec![0; 4] }
    }

    fn time_word(&self) -> u32 {
        ((self.time * TIME_SCALE) as u32) & 0xff_ffff
    }

    fn push(&mut self, sample: Sample, key: bool, len: usize) {
        match self.run {
            Some((k, n)) if k == key && n < rbr_formats::ghost::RUN_MAX => self.run = Some((k, n + 1)),
            _ => {
                self.run = Some((key, 1));
                self.written += 4;
            }
        }
        self.written += len;
        self.samples.push(sample);
    }

    fn key(&mut self, pose: Pose) {
        let key = Key { time: self.time_word(), position: pose.position, rotation: quantize(pose.rotation), spare: 0 };
        self.push(Sample::Key(key), true, 20);
        self.last = pose.position;
    }

    fn delta(&mut self, pose: Pose) {
        let offset: [i8; 3] = std::array::from_fn(|i| ((pose.position[i] - self.last[i]) * DELTA_SCALE) as i32 as i8);
        let delta = Delta { time: self.time_word(), offset, rotation: quantize(pose.rotation) };
        self.push(Sample::Delta(delta), false, 10);
        for (last, step) in self.last.iter_mut().zip(offset) {
            *last += f32::from(step) * DELTA_UNIT;
        }
    }
}

/// `(char)(int)(q * 127.5)`.
fn quantize(q: [f32; 4]) -> [i8; 4] {
    q.map(|v| (v * QUAT_SCALE) as i32 as i8)
}

/// Plays a recorded stream back (`Select` 0x492c20, `Playback` 0x493650).
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    samples: Vec<Sample>,
    next: usize,
    /// The pose the interpolation runs from and to (`+0x10`/`+0x20`, `+0x30`/`+0x40`), and its times
    /// (`m_f50`, `m_f54`).
    from: Pose,
    to: Pose,
    from_time: f32,
    to_time: f32,
    /// The stream has ended: nothing more is emitted, and the ghost stays at its last pose.
    ended: bool,
    /// `SetFlag(1)`: `Playback` runs only with bit 31 of the mode clear.
    paused: bool,
}

impl Player {
    pub fn new(stream: &Stream) -> Player {
        Player {
            samples: stream.samples.clone(),
            next: 0,
            from: Pose::default(),
            to: Pose::default(),
            from_time: 0.0,
            to_time: 0.0,
            ended: false,
            paused: false,
        }
    }

    /// `SetFlag` (0x492ae0): while paused nothing is emitted. The game's playback time is the manager's clock, which
    /// `Accumulate` stops too, so the host stops the time it passes to [`Player::advance`] as well.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// The race time of the last sample, s.
    pub fn duration(&self) -> f32 {
        self.samples.last().map_or(0.0, |s| s.time() as f32 * TIME_UNIT)
    }

    /// The ghost's pose at race time `t` (`Playback`, which emits `CSigGhostCarState`), or `None` once the stream has
    /// ended (the game then emits nothing and the ghost keeps its last pose). `t` must not go back; start a new
    /// player to rewind.
    pub fn advance(&mut self, t: f32) -> Option<Pose> {
        if self.ended || self.paused {
            return None;
        }
        while t >= self.to_time {
            let Some(&sample) = self.samples.get(self.next) else {
                self.ended = true;
                break;
            };
            self.next += 1;
            match sample {
                Sample::Delta(d) => {
                    self.from = self.to;
                    self.from_time = self.to_time;
                    for i in 0..3 {
                        self.to.position[i] += f32::from(d.offset[i]) * DELTA_UNIT;
                    }
                    self.to.rotation = normalize(d.rotation.map(|q| f32::from(q) * QUAT_UNIT));
                    self.to_time = d.time as f32 * TIME_UNIT;
                }
                Sample::Key(k) => {
                    let rotation = normalize(k.rotation.map(|q| f32::from(q) * QUAT_UNIT));
                    self.from = Pose { position: k.position, rotation };
                    self.to = self.from;
                    self.to_time = k.time as f32 * TIME_UNIT;
                    self.from_time = t;
                }
            }
        }
        let span = self.to_time - self.from_time;
        let f = if span <= 0.0 { 0.0 } else { ((t - self.from_time) / span).min(1.0) };
        Some(Pose {
            position: lerp(self.from.position, self.to.position, f),
            rotation: slerp(self.from.rotation, self.to.rotation, f),
        })
    }
}

/// The ghost clock's time at a stream's last sample, s: where the recording stopped (`BeginWrite` at the race's end).
/// The clock (`m_wtime`) runs from the start of the 7 s countdown (`rb_33` leaves the line at 7.32 s,
/// rbr-recomp `notes/marks-sound-ghost.md` 3.6), so this is about 7 s past the race time the HUD and the result show.
pub fn end_time(stream: &Stream) -> f32 {
    stream.samples.last().map_or(0.0, |s| s.time() as f32 * TIME_UNIT)
}

/// The stored ghost's map and race time, as `m_rtime` keeps them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Best {
    pub map: u32,
    pub time: f32,
}

/// The `clear` argument `CApplication::M00480810` passes `BeginWrite` at a race's end in game mode `mode` (the switch
/// at 0x480e3c, asm): in the quick rally a retired race clears; in the hot seat modes 4, 5 and 8 nothing clears (a
/// retired race passes 1800.0 s, `0x72e384`, as its time); every other mode clears, the championship (3) included.
pub fn clears(mode: Mode, retired: bool) -> bool {
    match mode.value() {
        1 => retired,
        4 | 5 | 8 => false,
        _ => true,
    }
}

/// `BeginWrite` (0x492ca0) at a race's end: whether the drive just recorded (`new`) becomes the ghost to race. A
/// cleared one ([`clears`]) never does; otherwise it does when there is no stored ghost, the stored one is of another
/// map, or the new time is lower (strictly). The car is not compared.
/// (`RBR_E18_Ghost.cpp` has the time test the other way round; the asm swaps on `new < stored`.)
pub fn replaces(new: Best, clear: bool, stored: Option<Best>) -> bool {
    !clear && stored.is_none_or(|s| s.map != new.map || new.time < s.time)
}

/// The pose at time `t` (s) of timed `poses` (sorted by time): interpolated between the two around `t` as the game's
/// playback does, the first before them and the last after them (ours: RSF's ghosts, `rbr_formats::rsf::ghosts`, are
/// full poses about every 100 ms; RSF's playback is not traced). `None` when there are none.
pub fn pose_at(poses: &[(f32, Pose)], t: f32) -> Option<Pose> {
    let next = poses.partition_point(|(time, _)| *time <= t);
    if next == 0 {
        return poses.first().map(|p| p.1);
    }
    let (from_time, from) = poses[next - 1];
    let Some(&(to_time, to)) = poses.get(next) else {
        return Some(from);
    };
    let f = (t - from_time) / (to_time - from_time);
    Some(Pose { position: lerp(from.position, to.position, f), rotation: slerp(from.rotation, to.rotation, f) })
}

/// `E11_VecLerp` (0x473c90): `a + (b - a) * t`.
fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| (b[i] - a[i]) * t + a[i])
}

/// `E18_QuatNormalize` (0x492170). The game scales by `rsqrtss`, an estimate good to about 12 bits; this divides by
/// the square root (ours).
fn normalize(q: [f32; 4]) -> [f32; 4] {
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if n == 0.0 {
        q
    } else {
        q.map(|v| v / n)
    }
}

/// `E18_QuatSlerp` (0x492160): the shorter arc, linear when the two are within [`SLERP_LINEAR`].
fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = b[3] * a[3] + b[2] * a[2] + a[0] * b[0] + b[1] * a[1];
    let mut s = 1.0f32;
    if dot < 0.0 {
        dot = -dot;
        s = -1.0;
    }
    let (fa, fb) = if dot < SLERP_LINEAR {
        let angle = f64::from(dot).acos();
        let sin = angle.sin();
        (((1.0 - f64::from(t)) * angle).sin() / sin, (f64::from(t) * angle).sin() / sin * f64::from(s))
    } else {
        (f64::from(1.0 - t), f64::from(s * t))
    };
    let (fa, fb) = (fa as f32, fb as f32);
    std::array::from_fn(|i| a[i] * fa + b[i] * fb)
}

#[cfg(test)]
mod pose_tests {
    use super::*;

    #[test]
    fn timed_poses_are_interpolated_and_held() {
        let at = |x: f32| Pose { position: [x, 0.0, 0.0], rotation: [0.0, 0.0, 0.0, 1.0] };
        let poses = [(1.0, at(0.0)), (2.0, at(10.0))];
        assert_eq!(pose_at(&poses, 0.5), Some(at(0.0)));
        assert_eq!(pose_at(&poses, 1.25), Some(at(2.5)));
        assert_eq!(pose_at(&poses, 3.0), Some(at(10.0)));
        assert_eq!(pose_at(&[], 1.0), None);
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    #[test]
    fn a_finished_faster_drive_or_another_map_replaces_the_ghost() {
        let best = |map, time| Best { map, time };
        assert!(replaces(best(54, 300.0), false, None));
        assert!(replaces(best(54, 299.0), false, Some(best(54, 300.0))));
        assert!(!replaces(best(54, 300.0), false, Some(best(54, 300.0))));
        assert!(!replaces(best(54, 301.0), false, Some(best(54, 300.0))));
        assert!(replaces(best(33, 400.0), false, Some(best(54, 300.0))));
        assert!(!replaces(best(54, 200.0), true, None));
    }

    #[test]
    fn only_a_finished_quick_rally_or_a_hot_seat_race_keeps_its_recording() {
        assert!(!clears(Mode::QUICK_RALLY, false));
        assert!(clears(Mode::QUICK_RALLY, true));
        for retired in [false, true] {
            assert!(clears(Mode::CHAMPIONSHIP, retired));
            assert!([0, 2, 6, 7, 9].iter().all(|&m| clears(Mode(m), retired)));
            assert!([4, 5, 8].iter().all(|&m| !clears(Mode(m), retired)));
        }
    }

    #[test]
    fn a_paused_recorder_and_player_stand_still() {
        let mut recorder = Recorder::new();
        let pose = Pose { position: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0] };
        recorder.car_state(pose);
        recorder.set_paused(true);
        for _ in 0..4 {
            recorder.frame(0.01);
            recorder.car_state(pose);
        }
        assert_eq!((recorder.time(), recorder.finish().samples.len()), (0.0, 1));
        recorder.set_paused(false);
        recorder.frame(0.01);
        recorder.car_state(pose);
        assert_eq!(recorder.finish().samples.len(), 2);
        assert!((end_time(&recorder.finish()) - 0.01).abs() < 1e-3);
        let mut player = Player::new(&recorder.finish());
        player.set_paused(true);
        assert_eq!(player.advance(0.0), None);
        player.set_paused(false);
        assert!(player.advance(0.0).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVEL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    fn at(x: f32) -> Pose {
        Pose { position: [x, 2.0, 3.0], rotation: LEVEL }
    }

    #[test]
    fn records_a_key_then_every_second_frame() {
        let mut r = Recorder::new();
        let mut x = 0.0;
        for _ in 0..6 {
            r.car_state(at(x));
            r.frame(0.04);
            x += 0.5;
        }
        let s = r.finish();
        // The key at 0, then the frames counted 1, 3 and 5: times 0.04, 0.12, 0.20 as the retail ghosts have them.
        let times: Vec<u32> = s.samples.iter().map(Sample::time).collect();
        assert_eq!(times, [0, 40, 122, 204]);
        assert!(
            matches!(s.samples[0], Sample::Key(k) if k.position == [0.0, 2.0, 3.0] && k.rotation == [0, 0, 0, 127])
        );
        assert!(matches!(s.samples[1], Sample::Delta(d) if d.offset == [32, 0, 0]));
        assert!(matches!(s.samples[2], Sample::Delta(d) if d.offset == [64, 0, 0]));
    }

    /// `M576710`'s `Accumulate`: a penalty moves the clock on and counts as a frame: after the frame counted 2 it is
    /// count 3, so the next car state is sampled at once with the penalty in its time, and the stream ends 10 s later.
    #[test]
    fn m576710_a_penalty_moves_the_clock_and_counts_a_frame() {
        let mut r = Recorder::new();
        for i in 0..6 {
            r.car_state(at(i as f32 * 0.5));
            r.frame(0.04);
            if i == 1 {
                r.penalty(10.0);
            }
        }
        let word = |t: f32| (t * TIME_SCALE) as u32;
        let times: Vec<u32> = r.finish().samples.iter().map(Sample::time).collect();
        assert_eq!(times, [0, 40, word(0.04 + 0.04 + 10.0), word(0.04 + 0.04 + 10.0 + 0.04 + 0.04)]);
        assert_eq!(r.time(), 0.04 + 0.04 + 10.0 + 0.04 + 0.04 + 0.04 + 0.04);
    }

    #[test]
    fn a_long_move_is_split_so_each_delta_fits() {
        let mut r = Recorder::new();
        r.car_state(at(0.0));
        r.frame(0.04);
        // 5 m in one sample: 1 + trunc(5 * 0.5128) = 3 parts.
        r.car_state(at(5.0));
        let s = r.finish();
        let deltas: Vec<i8> = s.samples[1..]
            .iter()
            .map(|s| match s {
                Sample::Delta(d) => d.offset[0],
                _ => panic!("a key"),
            })
            .collect();
        assert_eq!(deltas, [106, 107, 107]);
        assert_eq!(s.samples.iter().map(Sample::time).collect::<Vec<_>>(), [0, 13, 27, 40]);
    }

    #[test]
    fn a_respawn_starts_a_key_run() {
        let mut r = Recorder::new();
        for i in 0..4 {
            if i == 3 {
                r.respawn();
            }
            r.car_state(at(i as f32));
            r.frame(0.02);
        }
        let bytes = r.finish().write();
        let back = Stream::parse(&bytes);
        assert_eq!(back.samples.len(), 3);
        assert!(matches!(back.samples[2], Sample::Key(k) if k.position[0] == 3.0));
        assert_eq!(back.tail, vec![0; 4]);
        assert_eq!(r.written, bytes.len() - 4);
    }

    #[test]
    fn plays_back_what_it_recorded() {
        let mut r = Recorder::new();
        let mut t = 0.0;
        let mut poses = Vec::new();
        for i in 0..200 {
            let a = i as f32 * 0.01;
            let pose = Pose { position: [i as f32 * 0.3, 1.0, -2.0], rotation: [0.0, 0.0, a.sin(), a.cos()] };
            r.car_state(pose);
            r.frame(0.02);
            t += 0.02;
            poses.push((t, pose));
        }
        let mut p = Player::new(&r.finish());
        // The sample written at a frame holds the race time before the frame's `Accumulate`, and the playback runs after
        // it: played against the same drive, the ghost is a frame ahead of the car, and within 1/64 m of that.
        for &(t, pose) in &poses[..190] {
            let ghost = p.advance(t).unwrap();
            let x = pose.position[0] + 0.3;
            assert!((ghost.position[0] - x).abs() < 0.05, "at {t}: ghost {:?}, car {x}", ghost.position);
            assert!((ghost.rotation[2] - pose.rotation[2]).abs() < 0.02);
        }
        assert!(p.advance(10.0).is_some(), "the frame the stream ends on still emits");
        assert_eq!(p.advance(10.1), None);
    }

    #[test]
    fn slerp_takes_the_short_arc() {
        let q = slerp(LEVEL, [0.0, 0.0, 0.0, -1.0], 0.5);
        assert_eq!(q, LEVEL);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let q = slerp(LEVEL, [0.0, 0.0, 1.0, 0.0], 0.5);
        assert!((q[2] - half).abs() < 1e-6 && (q[3] - half).abs() < 1e-6);
    }
}
