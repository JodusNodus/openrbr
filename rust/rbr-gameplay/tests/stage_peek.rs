//! The stage state machine against the original's own state: `peek/g7-tanner-corolla.bin` (the FRM1 recording of the
//! run) and `peek/g7-tanner-corolla.peek.bin` (the state probes `peek/g7-stage.peek` took after each `StepForward`).
//! Before each frame's steps the Rust stage must hold the state, the load countdown (`m_unk72c`), the go countdown
//! (`m_unk714`, armed `m_unk718`) and the quick race's track and car (`m_unk730`, `m_unk734`) the original held when
//! that step's call ran; the load then runs the frame's settle steps, as `M004801d0` case 0xd does.
//!
//! The camera gives the go (`E15CamHost::M8840` calls `M0047ca40`). Its timer is wall time, which no recording holds, so
//! the test gives the go as soon as the countdown starts, and `countdown_tick` with `dt` 0: the peek shows the countdown
//! at exactly -1.0 on the first driving step, which only a `dt` below about 6e-8 s leaves unchanged. The first frame
//! holds the countdown's start (`m_unk72c` 150); the set-up (state 5) and the race host's ready signal happen before it.
//!
//! Runs only when `RBR_RECORDINGS` names the recordings directory; skipped otherwise.

use rbr_gameplay::stage::{Stage, State};
use rbr_recording::peek::{PeekFile, Probe, Record, Value};
use rbr_recording::{Frame, Frames};
use std::fmt::Debug;
use std::path::{Path, PathBuf};

const DRIVE: &str = "g7-tanner-corolla";

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The peek's value of the named probe in `record`.
fn peeked(record: &Record, probes: &[Probe], name: &str) -> Value {
    let index = probes.iter().position(|probe| probe.name == name).unwrap_or_else(|| panic!("no probe {name}"));
    record.value(index).expect("record holds every probe")
}

fn peeked_u32(record: &Record, probes: &[Probe], name: &str) -> u32 {
    match peeked(record, probes, name) {
        Value::U32(v) => v,
        other => panic!("{name}: {other:?}, expected u32"),
    }
}

fn peeked_f32(record: &Record, probes: &[Probe], name: &str) -> f32 {
    match peeked(record, probes, name) {
        Value::F32(v) => v,
        other => panic!("{name}: {other:?}, expected f32"),
    }
}

/// The step, the field and both values of the first difference.
fn expect_eq<T: PartialEq + Debug>(step: u32, field: &str, ours: T, theirs: T) {
    assert!(ours == theirs, "step {step}, {field}: ours {ours:?}, original {theirs:?}");
}

/// The stage as the original's probes show it at one step.
struct Seen {
    state: u32,
    load_left: u32,
    countdown: f32,
    armed: bool,
    track: u32,
    car: u32,
}

fn seen(record: &Record, probes: &[Probe]) -> Seen {
    Seen {
        state: peeked_u32(record, probes, "app_state"),
        load_left: peeked_u32(record, probes, "cd72c"),
        countdown: peeked_f32(record, probes, "cd714"),
        armed: peeked_u32(record, probes, "cd718") == 1,
        track: peeked_u32(record, probes, "car730"),
        car: peeked_u32(record, probes, "car734"),
    }
}

#[test]
fn stage_matches_the_original_state_on_the_peeked_drive() {
    let Some(recordings) = std::env::var_os("RBR_RECORDINGS").map(PathBuf::from) else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    let peek_dir = recordings.join("peek");
    let frame_bytes = read(&peek_dir.join(format!("{DRIVE}.bin")));
    let peek_bytes = read(&peek_dir.join(format!("{DRIVE}.peek.bin")));

    let frames: Vec<Frame> =
        Frames::new(&frame_bytes).collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let peek = PeekFile::parse(&peek_bytes).unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let records: Vec<Record> = peek.records().collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    assert_eq!(frames.len(), records.len(), "one peek record per frame");

    let first = seen(&records[0], &peek.probes);
    let mut stage = Stage::quick_race(first.track, first.car).load_ready();
    let mut counts = [0u32; 2];
    for (frame, record) in frames.iter().zip(&records) {
        let step = record.step;
        assert_eq!(step, frame.post, "peek step");
        let theirs = seen(record, &peek.probes);
        // The camera's go, given as soon as the countdown runs (see the module note).
        if stage.state() == State::Countdown {
            stage = stage.go().countdown_tick(0.0);
        }

        expect_eq(step, "app state", stage.state().value(), theirs.state);
        expect_eq(step, "load countdown (m_unk72c)", stage.load_left(), theirs.load_left);
        expect_eq(step, "countdown (m_unk714)", stage.countdown().to_bits(), theirs.countdown.to_bits());
        expect_eq(step, "armed (m_unk718)", stage.armed(), theirs.armed);
        expect_eq(step, "track (m_unk730)", stage.track(), theirs.track);
        expect_eq(step, "car (m_unk734)", stage.car(), theirs.car);

        let substeps = frame.post - frame.pre;
        match stage.state() {
            State::Load => {
                assert_eq!(substeps, 40, "step {step}: a settle call is 40 substeps");
                counts[0] += 1;
                stage = stage.load_tick(substeps);
            }
            State::Driving => {
                assert_eq!(substeps, 1, "step {step}: a driving frame is one substep");
                counts[1] += 1;
            }
            other => panic!("step {step}: unexpected state {other:?}"),
        }
    }
    eprintln!("{DRIVE}: {} load frames, {} driving frames; stage equal at every peeked step", counts[0], counts[1]);
    assert_eq!(counts[0], 4, "four settle calls");
}
