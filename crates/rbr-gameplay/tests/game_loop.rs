//! Composition of the G1-G8 pieces against every full FRM1 drive. The test
//! uses the original frame's car block as the physics host's returned sample;
//! replay payloads are injected after the local filter path, as the original
//! does in `TicPhysicsRecording`.

use rbr_formats::trk::{ChunkPayload, DriveLineRecord};
use rbr_formats::{markers, trk};
use rbr_gameplay::control::{CarControl, QuantizedControl};
use rbr_gameplay::filter::{Grid, Raw};
use rbr_gameplay::flow::EndCause;
use rbr_gameplay::game::{FrameInput, Game, PhysicsStep, StepSample};
use rbr_gameplay::ini::Ini;
use rbr_gameplay::markers::{MarkerSettings, TickInput};
use rbr_gameplay::stage::{Stage, State};
use rbr_recording::peek::{PeekFile, Value};
use rbr_recording::{read_f32, Frame, Frames};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DT: f32 = 0.007;
const DRIVELINE_OFFSET: usize = 0x17c;
const VELOCITY_OFFSET: usize = 0x1c0;
const POSITION_OFFSET: usize = 0x140;
const NOTIFY_A_SPEED_OFFSET: usize = 0x15f8;
const VANILLA_INI: &str = "[STEER::BUTTON_DIGITAL]\nActive = true\nRiseRate = 3.0\nFallRate = 5.0\nmaxSpeedFade = 0.5\nMaxSpeedFadeSpeed = 27.0\n\n[STEER::STICK]\nActive = true\nParabolic = true\nmaxSpeedFade = 0.5\nMaxSpeedFadeSpeed = 27.0\n\n[STEER::AXIS]\nActive = false\nParabolic = false\nmaxSpeedFade = 0.3\nMaxSpeedFadeSpeed = 35.0\n";

#[derive(Clone, Copy)]
struct Drive {
    name: &'static str,
    stage: &'static str,
    track: u32,
    car: u32,
}

const DRIVES: [Drive; 4] = [
    Drive { name: "tanner-corolla", stage: "track-54_E", track: 54, car: 6 },
    Drive { name: "tanner-damage", stage: "track-54_E", track: 54, car: 6 },
    Drive { name: "arctic-snow", stage: "track-13_O", track: 13, car: 6 },
    Drive { name: "tarmac-lancer", stage: "track-41_M", track: 41, car: 6 },
];

type PeekRows = BTreeMap<u32, BTreeMap<String, Value>>;

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn driveline_records(path: &Path) -> Vec<DriveLineRecord> {
    let bytes = read(path);
    let file = trk::parse(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    file.chunks
        .iter()
        .find_map(|chunk| match &chunk.payload {
            ChunkPayload::DriveLine(data) => Some(data.records.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{}: no driveline", path.display()))
}

fn peek_rows(path: &Path) -> (Vec<String>, PeekRows) {
    let bytes = read(path);
    let peek = PeekFile::parse(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let names = peek.probes.iter().map(|probe| probe.name.clone()).collect::<Vec<_>>();
    let mut rows = BTreeMap::new();
    for record in peek.records() {
        let record = record.unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let values =
            names.iter().enumerate().map(|(index, name)| (name.clone(), record.value(index).unwrap())).collect();
        rows.insert(record.step, values);
    }
    (names, rows)
}

fn u32_at(row: &BTreeMap<String, Value>, name: &str) -> u32 {
    match row[name] {
        Value::U32(value) => value,
        other => panic!("{name}: expected u32, got {other:?}"),
    }
}

fn f32_at(row: &BTreeMap<String, Value>, name: &str) -> f32 {
    match row[name] {
        Value::F32(value) => value,
        other => panic!("{name}: expected f32, got {other:?}"),
    }
}

fn f64_at(row: &BTreeMap<String, Value>, name: &str) -> f64 {
    let lo = u64::from(u32_at(row, &format!("{name}_lo")));
    let hi = u64::from(u32_at(row, &format!("{name}_hi")));
    f64::from_bits((hi << 32) | lo)
}

fn check_stage(stage: Stage, row: &BTreeMap<String, Value>, step: u32) {
    let expected_state = u32_at(row, "app_state");
    assert_eq!(stage.state().value(), expected_state, "step {step}: app state");
    assert_eq!(stage.load_left(), u32_at(row, "cd72c"), "step {step}: load countdown");
    assert_eq!(stage.countdown().to_bits(), f32_at(row, "cd714").to_bits(), "step {step}: go countdown");
    assert_eq!(u32::from(stage.armed()), u32_at(row, "cd718"), "step {step}: go armed");
    assert_eq!(stage.track(), u32_at(row, "car730"), "step {step}: track");
    assert_eq!(stage.car(), u32_at(row, "car734"), "step {step}: car");
}

fn check_time_card(game: &Game, row: &BTreeMap<String, Value>, step: u32) {
    let card = game.time_card();
    assert_eq!(game.elapsed().to_bits(), f64_at(row, "t").to_bits(), "step {step}: elapsed");
    assert_eq!(card.started, u32_at(row, "flag40") == 1, "step {step}: started");
    assert_eq!(card.started, u32_at(row, "flag20") == 1, "step {step}: flag20");
    assert_eq!(card.hold as u32, u32_at(row, "i24"), "step {step}: hold");
    assert_eq!(card.last.to_bits(), f32_at(row, "f1c").to_bits(), "step {step}: last distance");
    assert_eq!(card.cur.to_bits(), f32_at(row, "cur").to_bits(), "step {step}: current threshold");
    assert_eq!(card.passed as u32, u32_at(row, "count2"), "step {step}: passed splits");
    assert_eq!(card.origin.to_bits(), f64_at(row, "d38").to_bits(), "step {step}: origin");
    let finish_time = card.finish_time.unwrap_or(0.0);
    assert_eq!(card.finish_time.is_some(), u32_at(row, "flag58") == 1, "step {step}: finish flag");
    assert_eq!(finish_time.to_bits(), f64_at(row, "d50").to_bits(), "step {step}: finish time");
    for (index, field) in ["split0", "split1"].into_iter().enumerate() {
        let ours = card.splits().get(index).and_then(|split| split.time).unwrap_or(0.0);
        assert_eq!(ours.to_bits(), f64_at(row, field).to_bits(), "step {step}: {field}");
    }
}

fn check_time_peek(data: &Path, recordings: &Path, drive: Drive, peek_name: &str) {
    let peek_dir = recordings.join("peek");
    let (_, rows) = peek_rows(&peek_dir.join(format!("{peek_name}.peek.bin")));
    let frames_bytes = read(&peek_dir.join(format!("{peek_name}.bin")));
    let frames: Vec<Frame> =
        Frames::new(&frames_bytes).collect::<Result<_, _>>().unwrap_or_else(|error| panic!("{peek_name}: {error}"));
    let maps = data.join("Maps");
    let dls = markers::parse(&read(&maps.join(format!("{}.dls", drive.stage))))
        .unwrap_or_else(|error| panic!("{}: {error}", drive.stage));
    let track_records = driveline_records(&maps.join(format!("{}.trk", drive.stage)));
    let track_values = track_records.iter().map(|record| record.value).collect();
    let grid = Grid::from_settings(&Ini::parse(""));
    let mut game = Game::new(
        Stage::quick_race(drive.track, drive.car).load_ready(),
        grid,
        &dls,
        track_values,
        DT,
        0.0,
        MarkerSettings::default(),
    );
    let mut physics =
        RecordingPhysics { car: &[], track: &track_records, calls: 0, state: State::Load, state_in_step: State::Load };
    let mut now = 0.0f64;
    for (index, frame) in frames.iter().enumerate() {
        physics.car = frame.car;
        if index == 4 {
            let go = game.frame(now, FrameInput { camera_go: true, ..FrameInput::default() }, &mut physics);
            assert!(go.substeps.is_empty(), "{peek_name}: camera go does not step");
            assert!(game.stage().armed(), "{peek_name}: camera arms countdown after Tic");
            let countdown = game.frame(now, FrameInput::default(), &mut physics);
            assert!(countdown.substeps.is_empty(), "{peek_name}: countdown transition does not step");
        }
        if index >= 4 {
            now += f64::from(DT);
        }
        let replay_control =
            if index < 4 { None } else { Some(QuantizedControl::from_payload(frame.control_u32s()).unwrap()) };
        let events =
            game.frame(now, FrameInput { raw: Raw::default(), replay_control, ..FrameInput::default() }, &mut physics);
        assert_eq!(events.substeps.len() as u32, frame.substeps, "{peek_name} step {}: schedule", frame.post);
        if let Some(row) = rows.get(&frame.post) {
            check_time_card(&game, row, frame.post);
        }
    }
    assert_eq!(frames.len(), rows.len(), "{peek_name}: one time card per recorded frame");
    eprintln!("{peek_name}: time card and clock equal at every peeked frame");
}

struct RecordingPhysics<'a> {
    car: &'a [u8],
    track: &'a [DriveLineRecord],
    calls: u32,
    /// The application state `Game` gave the host last (`app_state`), and the one the last substep ran in.
    state: State,
    state_in_step: State,
}

impl PhysicsStep for RecordingPhysics<'_> {
    fn step(&mut self, control: Option<CarControl>, _index: u32, inside_step: &mut dyn FnMut(StepSample)) {
        self.calls += 1;
        // The emit's slot is the state: a control reaches the car's connection (slot 1) only while driving.
        assert_eq!(control.is_some(), self.state == State::Driving, "a control in state {:?}", self.state);
        self.state_in_step = self.state;
        let position = read_f32(self.car, DRIVELINE_OFFSET).expect("recorded car +0x17c");
        let velocity =
            std::array::from_fn(|lane| read_f32(self.car, VELOCITY_OFFSET + lane * 4).expect("recorded car velocity"));
        inside_step(StepSample {
            driveline_parameter: position,
            // Vf6cx stores -MB560[1] at CCar+0x15f8, the scalar NotifyA reads from p[0].
            notify_a_speed: read_f32(self.car, NOTIFY_A_SPEED_OFFSET).expect("recorded car +0x15f8"),
            notify_a_vector: None,
            marker: TickInput {
                dot150: rbr_gameplay::driveline::dot150(self.track, position, velocity)
                    .expect("recorded track sampler input"),
                finish_to_menu: true,
                ..TickInput::default()
            },
            outside_assistance: None,
            speed: velocity.iter().map(|v| v * v).sum::<f32>().sqrt(),
            retire: None,
            position: std::array::from_fn(|lane| read_f32(self.car, POSITION_OFFSET + lane * 4).expect("car +0x140")),
        });
    }

    fn app_state(&mut self, state: State) {
        assert_ne!(state, self.state, "app_state without a change");
        self.state = state;
    }
}

#[test]
fn game_composes_the_four_reference_drives() {
    let (Some(data), Some(recordings)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_RECORDINGS").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_RECORDINGS unset; skipped");
        return;
    };
    let filter_grid = Grid::from_settings(&Ini::parse(VANILLA_INI));
    let mut checked_frames = 0;

    for drive in DRIVES {
        let maps = data.join("Maps");
        let stage_markers = markers::parse(&read(&maps.join(format!("{}.dls", drive.stage))))
            .unwrap_or_else(|error| panic!("{}: {error}", drive.stage));
        let track_records = driveline_records(&maps.join(format!("{}.trk", drive.stage)));
        let track = track_records.iter().map(|record| record.value).collect();
        let frame_bytes = read(&recordings.join(format!("drive-{}.bin", drive.name)));
        let frames: Vec<Frame> = Frames::new(&frame_bytes)
            .collect::<Result<_, _>>()
            .unwrap_or_else(|error| panic!("{}: {error}", drive.name));

        let peek_dir = recordings.join("peek");
        let stage_peek =
            (drive.name == "tanner-corolla").then(|| peek_rows(&peek_dir.join("g7-tanner-corolla.peek.bin")).1);
        let initial_stage = Stage::quick_race(drive.track, drive.car).load_ready();
        let mut game =
            Game::new(initial_stage, filter_grid.clone(), &stage_markers, track, DT, 0.0, MarkerSettings::default());
        let mut physics = RecordingPhysics {
            car: &[],
            track: &track_records,
            calls: 0,
            state: initial_stage.state(),
            state_in_step: initial_stage.state(),
        };
        let mut now = 0.0f64;
        let mut step = 0u32;
        let mut ended = None;

        for (frame_index, frame) in frames.iter().enumerate() {
            physics.car = frame.car;
            let expected_substeps = if frame_index < 4 { 40 } else { 1 };
            assert_eq!(frame.substeps, expected_substeps, "{} frame {frame_index}: source schedule", drive.name);
            assert_eq!(frame.pre, step, "{} frame {frame_index}: pre-step", drive.name);
            assert_eq!(frame.post, step + frame.substeps, "{} frame {frame_index}: post-step", drive.name);

            if frame_index == 4 {
                // The camera timer and the countdown can elapse on frames
                // without StepForward, so this host event sits between the
                // last settle frame and the first recorded physics frame.
                let go = game.frame(now, FrameInput { camera_go: true, ..FrameInput::default() }, &mut physics);
                assert!(go.substeps.is_empty(), "{}: camera countdown frame steps", drive.name);
                assert!(game.stage().armed(), "{}: camera arms countdown after Tic", drive.name);
                let countdown = game.frame(now, FrameInput::default(), &mut physics);
                assert!(countdown.substeps.is_empty(), "{}: countdown transition frame steps", drive.name);
                assert_eq!(game.stage().state(), State::Driving, "{}: camera go", drive.name);
            }

            if frame_index >= 4 {
                now += f64::from(DT);
            }
            let recorded = frame.control_u32s();
            let replay_control = if frame_index < 4 {
                assert_eq!(recorded, [0; 8], "{} frame {frame_index}: settle has no payload", drive.name);
                None
            } else {
                Some(QuantizedControl::from_payload(recorded).unwrap_or_else(|| {
                    panic!("{} frame {frame_index}, step {}: recorded payload is not quantized", drive.name, frame.post)
                }))
            };
            let events = game.frame(
                now,
                FrameInput { raw: Raw::default(), replay_control, ..FrameInput::default() },
                &mut physics,
            );
            assert_eq!(
                events.substeps.len() as u32,
                frame.substeps,
                "{} frame {frame_index}: Game schedule",
                drive.name
            );
            let expected_tick_state = if frame_index < 3 {
                State::Load
            } else if frame_index < 4 {
                State::Countdown
            } else {
                State::Driving
            };
            assert_eq!(events.tick_state, expected_tick_state, "{} frame {frame_index}: Tic state", drive.name);
            if frame_index < 4 {
                assert!(events.substeps.iter().all(|event| event.control.is_none()));
            } else {
                assert_eq!(events.substeps.len(), 1);
                let substep = events.substeps[0];
                let generated = substep.generated_control.expect("local filter payload");
                assert_eq!(
                    generated.words(),
                    [Some(0), Some(0), Some(0), Some(0), Some(0), Some(0), None, Some(0)],
                    "{} step {}: idle raw payload",
                    drive.name,
                    frame.post
                );
                let actual = substep.control.expect("recorded replay payload");
                if let Some(index) = actual.first_difference(recorded) {
                    panic!(
                        "{} step {}: payload word {index}: got {:?}, recorded {:08x}",
                        drive.name,
                        frame.post,
                        actual.words()[index],
                        recorded[index]
                    );
                }
            }

            if let Some(rows) = &stage_peek {
                if let Some(row) = rows.get(&frame.post) {
                    let stage_during_step = events.substeps[0].stage;
                    check_stage(stage_during_step, row, frame.post);
                    assert_eq!(
                        physics.state_in_step.value(),
                        u32_at(row, "app_state"),
                        "step {}: the host's app state",
                        frame.post
                    );
                }
            }
            let snapshot_count = match (drive.name, frame.post) {
                ("tarmac-lancer", 5000) => Some(2),
                ("tarmac-lancer", 9000) => Some(16),
                ("tarmac-lancer", 12000) => Some(24),
                ("tarmac-lancer", 20500) => Some(51),
                ("arctic-snow", 11880) => Some(42),
                ("tanner-damage", 11820) => Some(40),
                _ => None,
            };
            if let Some(expected) = snapshot_count {
                if events.marker_fired != expected {
                    let fired: Vec<_> = game
                        .markers()
                        .keys()
                        .iter()
                        .enumerate()
                        .filter(|(_, key)| key.word1 & rbr_gameplay::markers::FIRED != 0)
                        .map(|(index, key)| (index, key.tag, key.distance))
                        .collect();
                    panic!(
                        "{} step {}: Game fired {}, the original's snapshot {expected}; fired keys: {fired:?}",
                        drive.name, frame.post, events.marker_fired
                    );
                }
            }

            step = frame.post;
            checked_frames += 1;
            if events.flow.finish_line {
                eprintln!("{}: the finish line at step {}", drive.name, frame.post);
            }
            // The race flow ends the race at the end-of-stage marker (or 20 s of frame time after the finish line).
            // tanner-corolla passes the end marker: the original's recording ends one frame after the flow's end.
            if let Some(end) = events.flow.end {
                ended = Some((frame_index, end));
                break;
            }
        }
        let run = match ended {
            Some((index, end)) => {
                assert!(matches!(end.cause, EndCause::Finish | EndCause::Marker), "{}: {end:?}", drive.name);
                assert!(end.finished(), "{}: {end:?}", drive.name);
                assert_eq!(index + 2, frames.len(), "{}: the recording ends a frame after the race", drive.name);
                eprintln!("{}: the flow ended the race at step {step}: {end:?}", drive.name);
                &frames[..=index]
            }
            None => &frames[..],
        };
        assert_eq!(step, run.last().unwrap().post, "{}: final counter", drive.name);
        assert_eq!(physics.calls, run.iter().map(|frame| frame.substeps).sum::<u32>(), "{}: host substeps", drive.name);
        eprintln!("{}: {} frames / {} substeps composed", drive.name, run.len(), physics.calls);
    }
    assert_eq!(checked_frames, 80_750);
    check_time_peek(&data, &recordings, DRIVES[0], "h1-tanner-corolla");
    check_time_peek(&data, &recordings, DRIVES[3], "g3-tarmac-lancer");
}
