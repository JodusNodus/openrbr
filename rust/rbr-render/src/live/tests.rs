use super::read::{read_frame, read_live_frame};
use super::*;
use crate::stage::InteractivePose;
use rbr_recording::camera::{self, Camera};
use rbr_recording::clock::StepClock;
use rbr_recording::damage::Damage;
use rbr_recording::detach::{self, PartPose};
use rbr_recording::hud;
use rbr_recording::hud::RaceHud;
use rbr_recording::input::CELL_BUTTON;
use rbr_recording::live as records;
use rbr_recording::particles;
use rbr_recording::sound::Sound;
use rbr_recording::splits::Splits;
use rbr_recording::{CONTROL_BYTES, HEADER_BYTES, MAGIC, WHEELS, WHEEL_BYTES};
use std::time::Instant;

/// The eight payload words.
fn words(p: [u8; CONTROL_BYTES]) -> [u32; 8] {
    std::array::from_fn(|i| u32::from_le_bytes(p[4 * i..4 * i + 4].try_into().unwrap()))
}

/// The payload periods each drawn frame sends at 60 frames a second for `drive` seconds, with a pause of `paused`
/// seconds after `before` seconds of it (the pause menu: no pump while it is up).
fn periods_per_frame(drive: f64, before: f64, paused: f64) -> Vec<u64> {
    let start = Instant::now();
    let at = |s: f64| start + std::time::Duration::from_secs_f64(s);
    let mut pace = Pace::new(start);
    let (mut sent, mut out) = (0, Vec::new());
    for frame in 1..=(drive * 60.0) as u32 {
        let mut t = f64::from(frame) / 60.0;
        if t > before {
            if pace.paused_at.is_none() && pace.paused == 0.0 {
                pace.pause(at(before));
                assert_eq!(pace.due(at(before + paused / 2.0), sent).0, sent, "nothing is due while paused");
                assert_eq!(pace.resume(at(before + paused)), paused);
            }
            t += paused;
        }
        let (due, _) = pace.due(at(t), sent);
        out.push(due - sent);
        sent = due;
    }
    out
}

/// Pausing for 3 s and resuming sends the same payload periods frame by frame as not pausing: no catch-up after
/// the pause, the same steps at the same real-time rate (142.9 a second) as before it.
#[test]
fn a_pause_sends_the_same_periods_as_no_pause() {
    let plain = periods_per_frame(4.0, 4.0, 0.0);
    let paused = periods_per_frame(4.0, 2.0, 3.0);
    assert_eq!(paused, plain);
    let sent: u64 = paused.iter().sum();
    assert_eq!(sent, (4.0 * STEPS_PER_SECOND) as u64 + 1, "4 s of payloads and the first");
    assert!(paused.iter().all(|&p| p <= 3), "no frame catches up");
}

/// The viewer's load between the first answer and the first pump (24 s on the test machine) and its first drawn frame are
/// not due: the first two pumps send the first period only, and nothing counts as behind or not played.
#[test]
fn the_load_and_the_first_frame_are_not_due() {
    let command = ["sh", "-c", "exec cat >/dev/null"].map(String::from);
    let mut live = Live::spawn(&command).unwrap();
    live.pace = Pace::new(Instant::now() - std::time::Duration::from_secs(24));
    live.pump(&mut Controls::default()).unwrap();
    live.pace = Pace::new(Instant::now() - std::time::Duration::from_millis(400));
    live.pump(&mut Controls::default()).unwrap();
    assert_eq!((live.sent, live.stats.most_behind, live.pace.skipped), (1, 0, 0.0));
    assert_eq!(live.stats.periods, 0);
}

#[test]
fn payload_is_the_input_at_full_precision() {
    use rbr_recording::input::Payload;
    let controls = Controls {
        throttle: 0.3,
        brake: 1.0,
        steering: -0.123_456,
        flags: GEAR_UP,
        keys_steer: true,
        ..Default::default()
    };
    let Ok(Payload::Input(input)) = Payload::read(&controls.payload()) else { panic!("an input payload") };
    assert_eq!((input.throttle, input.brake, input.steering), (0.3, 1.0, -0.123_456));
    assert_eq!((input.flags, input.cells), (GEAR_UP as u8, [CELL_BUTTON, 0, 0, 0, 0]));
    let w = words(Controls { flags: SELECT_GEAR, gear: 3, ..Default::default() }.payload());
    assert_eq!((w[5], w[6], w[7]), (SELECT_GEAR, 3, rbr_recording::input::INPUT_MAGIC));
    let late = Controls { gear: 3, ..Default::default() }.frame_payload(4);
    assert_eq!(words(late)[6], 3 | 4 << 8);
    assert_eq!(Payload::read(&late).map(|p| p.periods()), Ok(4));
    assert_eq!(Payload::read(&controls.payload()).map(|p| p.periods()), Ok(1));
}

#[test]
fn hud_labels_the_gears_and_converts_units() {
    let hud = |gear| Hud { gear, engine_speed: 100.0, speed: 10.0, ..Hud::default() };
    let labels: Vec<String> = (0..4).map(|g| hud(g).gear_label()).collect();
    assert_eq!(labels, ["R", "N", "1", "2"]);
    assert!((hud(2).rpm() - 954.93).abs() < 0.01);
    assert!((hud(2).kmh() - 36.0).abs() < 1e-4);
}

#[test]
fn frames_are_read_whole() {
    let car = 8;
    let mut bytes = Vec::new();
    for w in [MAGIC, 1, 2, 1, car as u32] {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    bytes.resize(HEADER_BYTES + CONTROL_BYTES + car + WHEELS * WHEEL_BYTES, 7);
    let mut reader = &bytes[..];
    assert_eq!(read_frame(&mut reader).unwrap().unwrap(), bytes);
    assert_eq!(read_frame(&mut reader).unwrap(), None);
}

#[test]
fn a_split_record_between_the_damage_and_the_scenery_is_read() {
    use rbr_recording::splits::Split;
    let car = 8;
    let mut stream = Vec::new();
    for w in [MAGIC, 1, 2, 1, car as u32] {
        stream.extend_from_slice(&w.to_le_bytes());
    }
    stream.resize(HEADER_BYTES + CONTROL_BYTES + car + WHEELS * WHEEL_BYTES, 7);
    stream.extend(hud::Hud { gear: 2, engine_speed: 1.0, speed: 2.0, race: None }.to_bytes());
    stream.extend(Damage::default().to_bytes());
    let record = Splits {
        finished: false,
        elapsed: 12.0,
        race_time: 12.5,
        penalties: 10.0,
        reference_finish: Some(185.0),
        splits: vec![Split { time: Some(11.0), reference: Some(10.5) }, Split::default()],
        ..Splits::default()
    };
    stream.extend(record.to_bytes());
    stream.extend_from_slice(&records::SCN1);
    stream.extend_from_slice(&0u32.to_le_bytes());
    let answer = read_live_frame(&mut &stream[..]).unwrap().unwrap();
    assert_eq!(answer.splits, Some(record));
    assert!(answer.poses.is_empty());
}

#[test]
fn live_frames_take_live_py_s_hud1_and_the_standalone_s_hud2() {
    let car = 8;
    let mut frame = Vec::new();
    for w in [MAGIC, 1, 2, 1, car as u32] {
        frame.extend_from_slice(&w.to_le_bytes());
    }
    frame.resize(HEADER_BYTES + CONTROL_BYTES + car + WHEELS * WHEEL_BYTES, 7);
    let race = RaceHud { time: 16.5, countdown: -3.0, penalties: 10.0, running: true, jump_start: false };
    let hud1 = hud::Hud { gear: 2, engine_speed: 300.0, speed: 12.0, race: None };
    let hud2 = hud::Hud { race: Some(race), ..hud1 };
    let mut damage = Damage { code: 4, index: 4, ..Damage::default() };
    damage.zones[3] = 0.5;
    let orbit = Camera { mode: camera::MODE_START_ORBIT, angle: 2.5 };
    let ngp_clock = StepClock { step: 1.0 / 720.0, settle: 1520 };
    let mut stream = Vec::new();
    let mut payload = [0u8; particles::PAYLOAD_BYTES];
    payload[0] = 3;
    let body = PartPose { part: 4, position: [0.5, 0.0, 1.0], orientation: [0.0, 0.0, 0.0, 1.0] };
    for (record, extra) in [(hud1, false), (hud2, true)] {
        stream.extend_from_slice(&frame);
        stream.extend(record.to_bytes());
        stream.extend(damage.to_bytes());
        stream.extend(detach::to_bytes(&[body]));
        if extra {
            // The standalone's tyre and sound records, and a record this reader does not know, skipped by its
            // length.
            stream.extend(particles::record(&[payload]));
            records::write_record(&mut stream, *b"XYZ1", &[9, 9, 9]);
            records::write_record(
                &mut stream,
                rbr_recording::sound::SND1,
                &Sound { speed: 4.0, ..Sound::default() }.to_bytes(),
            );
            stream.extend(orbit.record());
            stream.extend(ngp_clock.record());
        }
        stream.extend_from_slice(&records::SCN1);
        stream.extend_from_slice(&1u32.to_le_bytes());
        for word in [40_001u32, 1.0f32.to_bits(), 2.0f32.to_bits(), 3.0f32.to_bits(), 0, 0, 0, 1.0f32.to_bits()] {
            stream.extend_from_slice(&word.to_le_bytes());
        }
    }
    let mut reader = &stream[..];
    let first = read_live_frame(&mut reader).unwrap().unwrap();
    let second = read_live_frame(&mut reader).unwrap().unwrap();
    assert!(first.tyres.is_empty() && first.sound.is_none() && first.splits.is_none());
    assert_eq!(second.tyres.iter().map(|t| t.wheel).collect::<Vec<_>>(), [3]);
    assert_eq!(second.sound.map(|s| s.sound.speed), Some(4.0));
    assert_eq!((first.clock, second.clock), (None, Some(ngp_clock)));
    let (first, a, dented, first_detached, first_poses) =
        (first.frame, first.hud, first.damage, first.detached, first.poses);
    let (b, second_detached, second_poses) = (second.hud, second.detached, second.poses);
    assert_eq!(first, frame);
    assert_eq!(dented, damage);
    assert_eq!(first_detached, [body]);
    assert_eq!(second_detached, [body]);
    let pose = InteractivePose { handle: 40_001, position: [1.0, 2.0, 3.0], orientation: [0.0, 0.0, 0.0, 1.0] };
    assert_eq!(first_poses, [pose]);
    assert_eq!(second_poses, [pose]);
    assert_eq!(a, Hud { gear: 2, engine_speed: 300.0, speed: 12.0, ..Hud::default() });
    assert_eq!(b, Hud { race: Some(race), camera: Some(orbit), ..a });
    assert_eq!(read_live_frame(&mut reader).unwrap(), None);
}
