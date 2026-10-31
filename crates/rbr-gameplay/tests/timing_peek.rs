//! The time card against the original's own timing: each reference drive's FRM1 recording drives the Rust timekeeper
//! (the car's driveline position, `sample_x` of `CCar` +0x17c, and the step counter), and after every frame the card
//! and the clock must equal the time-card peek the harness took after that `StepForward` call (`peek/*.peek.bin`, probe
//! list `peek/g3-timecard.peek`). The events are the peek's transitions: start, each split, the finish.
//!
//! The settle frames (40 substeps, car not moving) pass the car's distance to each substep, as the original's
//! `CCar::Vf6cx` does; the frames of one substep are exact. `Record` runs inside the step, before the substep's clock
//! advance (`advance_time`), which is what the peek shows (design note section 2; `rbr-gameplay/README.md` (git 9c28c0b8)).
//!
//! Runs only when `RBR_DATA` names the install (`Maps/<stage>.dls` and `.trk`) and `RBR_RECORDINGS` the recordings
//! directory with `peek/<drive>.bin` and `peek/<drive>.peek.bin`; skipped otherwise.

use rbr_formats::markers::{self, StageMarkers};
use rbr_formats::trk::{self, ChunkPayload};
use rbr_gameplay::driveline::sample_x;
use rbr_gameplay::timing::{advance_time, CheckpointKind, TimeCard, TimeKeeper, CLOCK_START};
use rbr_recording::peek::{PeekFile, Probe, Record};
use rbr_recording::{read_f32, Frame, Frames};
use std::fmt::Debug;
use std::path::{Path, PathBuf};

mod common;
use common::{peeked_f32, peeked_u32, read};

/// `PhysicsEngine.MaxTimeStep` (`physics.lsp`, 0.0070), as `CDef` +0x1608c holds it.
const MAX_TIME_STEP: f32 = 0.007;
/// The car's driveline position, `CCar` +0x17c (`CCar::Vf6cx`).
const DRIVELINE_OFFSET: usize = 0x17c;

/// One reference drive: its recording and the stage it runs on.
struct Drive {
    name: &'static str,
    stage: &'static str,
}

const DRIVES: [Drive; 2] =
    [Drive { name: "h1-tanner-corolla", stage: "track-54_E" }, Drive { name: "g3-tarmac-lancer", stage: "track-41_M" }];

/// The stage's kind-0x14 driveline `value`s, in record order.
fn driveline_values(trk_path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(trk_path).unwrap_or_else(|e| panic!("{}: {e}", trk_path.display()));
    let file = trk::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", trk_path.display()));
    file.chunks
        .iter()
        .find_map(|chunk| match &chunk.payload {
            ChunkPayload::DriveLine(data) => Some(data.records.iter().map(|r| r.value).collect::<Vec<_>>()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{}: no driveline records", trk_path.display()))
}

fn stage_markers(maps: &Path, stage: &str) -> StageMarkers {
    let dls = maps.join(format!("{stage}.dls"));
    markers::parse(&read(&dls)).unwrap_or_else(|e| panic!("{}: {e}", dls.display()))
}

/// A double the peek writes as two u32 halves, `<name>_lo` and `<name>_hi`.
fn peeked_f64(record: &Record, probes: &[Probe], name: &str) -> f64 {
    let lo = u64::from(peeked_u32(record, probes, &format!("{name}_lo")));
    let hi = u64::from(peeked_u32(record, probes, &format!("{name}_hi")));
    f64::from_bits(hi << 32 | lo)
}

/// The step, the field and both values of the first difference.
fn expect_eq<T: PartialEq + Debug>(step: u32, field: &str, ours: T, theirs: T) {
    assert!(ours == theirs, "step {step}, {field}: ours {ours:?}, original {theirs:?}");
}

/// Every peeked card field of `record` against the card and clock after the frame.
fn check_card(record: &Record, probes: &[Probe], card: &TimeCard, elapsed: f64) {
    let step = record.step;
    let started = peeked_u32(record, probes, "flag40");
    expect_eq(step, "clock (t)", elapsed.to_bits(), peeked_f64(record, probes, "t").to_bits());
    expect_eq(step, "flag40 (started)", card.started, started == 1);
    // `flag20` is uninitialised until the start (1068149419 on tarmac-lancer); only 1 counts as started.
    expect_eq(step, "flag20", card.started, peeked_u32(record, probes, "flag20") == 1);
    expect_eq(step, "i24 (hold)", card.hold, peeked_u32(record, probes, "i24") as i32);
    expect_eq(step, "f1c (last)", card.last.to_bits(), peeked_f32(record, probes, "f1c").to_bits());
    expect_eq(step, "cur", card.cur.to_bits(), peeked_f32(record, probes, "cur").to_bits());
    expect_eq(step, "count2 (passed)", card.passed as u32, peeked_u32(record, probes, "count2"));
    expect_eq(step, "d38 (origin)", card.origin.to_bits(), peeked_f64(record, probes, "d38").to_bits());
    let finished = peeked_u32(record, probes, "flag58") == 1;
    expect_eq(step, "flag58 (finished)", card.finish_time.is_some(), finished);
    expect_eq(
        step,
        "d50 (finish time)",
        card.finish_time.unwrap_or(0.0).to_bits(),
        peeked_f64(record, probes, "d50").to_bits(),
    );
    for (i, name) in ["split0", "split1"].into_iter().enumerate() {
        let ours = card.splits()[i].time.unwrap_or(0.0);
        expect_eq(step, name, ours.to_bits(), peeked_f64(record, probes, name).to_bits());
    }
}

/// The events the peek shows between two records: the start, each split, the finish.
#[derive(Clone, Copy)]
struct Seen {
    started: bool,
    passed: u32,
    finished: bool,
}

fn seen(record: &Record, probes: &[Probe]) -> Seen {
    Seen {
        started: peeked_u32(record, probes, "flag40") == 1,
        passed: peeked_u32(record, probes, "count2"),
        finished: peeked_u32(record, probes, "flag58") == 1,
    }
}

fn peek_events(before: Seen, after: Seen, step: u32) -> Vec<(u32, CheckpointKind)> {
    let mut events = Vec::new();
    if after.started && !before.started {
        events.push((step, CheckpointKind::Start));
    }
    for i in before.passed..after.passed {
        events.push((step, CheckpointKind::Split(i as usize)));
    }
    if after.finished && !before.finished {
        events.push((step, CheckpointKind::Finish));
    }
    events
}

/// Drives the timekeeper from one frame's car position over its substeps, as `CCar::Vf6cx` runs `Record` in each.
fn drive_frame(
    keeper: TimeKeeper,
    elapsed: &mut f64,
    frame: &Frame,
    values: &[f32],
) -> (TimeKeeper, Vec<(u32, CheckpointKind)>) {
    let distance = read_f32(frame.car, DRIVELINE_OFFSET).expect("car block");
    let x = sample_x(values, distance).expect("car inside the driveline");
    let mut keeper = keeper;
    let mut events = Vec::new();
    for _ in frame.pre..frame.post {
        let (next, checkpoint) = keeper.record(x, *elapsed, MAX_TIME_STEP);
        keeper = next;
        if let Some(cp) = checkpoint {
            events.push((frame.post, cp.kind));
        }
        *elapsed = advance_time(*elapsed, MAX_TIME_STEP, false);
    }
    (keeper, events)
}

fn check_drive(drive: &Drive, data: &Path, recordings: &Path) {
    let maps = data.join("Maps");
    let stage_markers = stage_markers(&maps, drive.stage);
    let values = driveline_values(&maps.join(format!("{}.trk", drive.stage)));
    let peek_dir = recordings.join("peek");
    let frame_bytes = read(&peek_dir.join(format!("{}.bin", drive.name)));
    let peek_bytes = read(&peek_dir.join(format!("{}.peek.bin", drive.name)));

    let frames: Vec<Frame> =
        Frames::new(&frame_bytes).collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{}: {e}", drive.name));
    let peek = PeekFile::parse(&peek_bytes).unwrap_or_else(|e| panic!("{}: {e}", drive.name));
    let records: Vec<Record> =
        peek.records().collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{}: {e}", drive.name));
    assert_eq!(frames.len(), records.len(), "{}: one peek record per frame", drive.name);

    let mut keeper = TimeKeeper::load(&stage_markers);
    let mut elapsed = CLOCK_START;
    let mut ours = Vec::new();
    let mut theirs = Vec::new();
    let mut before = Seen { started: false, passed: 0, finished: false };
    for (frame, record) in frames.iter().zip(&records) {
        assert_eq!(record.step, frame.post, "{}: peek step", drive.name);
        let (next, events) = drive_frame(keeper, &mut elapsed, frame, &values);
        keeper = next;
        ours.extend(events);
        check_card(record, &peek.probes, &keeper.card, elapsed);
        let after = seen(record, &peek.probes);
        theirs.extend(peek_events(before, after, record.step));
        before = after;
    }
    assert_eq!(ours, theirs, "{}: events (step, kind)", drive.name);
    assert!(!theirs.is_empty(), "{}: no events in the peek", drive.name);
}

#[test]
fn timekeeper_matches_the_original_card_on_each_peeked_drive() {
    let (Some(data), Some(recordings)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_RECORDINGS").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_RECORDINGS unset; skipped");
        return;
    };
    for drive in &DRIVES {
        check_drive(drive, &data, &recordings);
        eprintln!("{}: card and clock equal at every peeked step", drive.name);
    }
}
