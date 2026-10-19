//! The time card over the tanner-corolla drive: the car's driveline distance (`sample_x` of +0x17c after each frame)
//! against the `track-54_E` markers gives the start, both splits and the finish at the steps of the predicted
//! crossing table (`notes/gameplay-loop.md`, "Time-card peek"; design note section 3). The end key is a plain
//! crossing, not a timekeeper event.
//!
//! Runs only when `RBR_RECORDINGS` names the directory with `drive-tanner-corolla.bin` and `RBR_DATA` the retail
//! install (`Maps/track-54_E.dls` and `.trk`). The clock is the original's (`advance_time`); `tests/timing_peek.rs`
//! checks it and the card against the peek.

use rbr_formats::markers;
use rbr_formats::trk::{self, ChunkPayload};
use rbr_gameplay::driveline::sample_x;
use rbr_gameplay::timing::{advance_time, CheckpointKind, TimeKeeper, CLOCK_START};
use rbr_recording::{read_f32, Frames};
use std::path::{Path, PathBuf};

const DRIVE: &str = "drive-tanner-corolla.bin";
const STAGE: &str = "track-54_E";
/// `PhysicsEngine.MaxTimeStep` on the reference drives.
const MAX_TIME_STEP: f32 = 0.007;
/// The car's driveline position, `CCar` +0x17c (`CCar::Vf6cx`).
const DRIVELINE_OFFSET: usize = 0x17c;

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

#[test]
fn tanner_corolla_card_crosses_at_the_predicted_steps() {
    let (Some(data), Some(recordings)) =
        (std::env::var_os("RBR_DATA").map(PathBuf::from), std::env::var_os("RBR_RECORDINGS").map(PathBuf::from))
    else {
        eprintln!("RBR_DATA or RBR_RECORDINGS unset; skipped");
        return;
    };
    let maps = data.join("Maps");
    let dls = maps.join(format!("{STAGE}.dls"));
    let stage_markers = markers::parse(&std::fs::read(&dls).unwrap_or_else(|e| panic!("{}: {e}", dls.display())))
        .unwrap_or_else(|e| panic!("{}: {e}", dls.display()));
    let values = driveline_values(&maps.join(format!("{STAGE}.trk")));
    let path = recordings.join(DRIVE);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    let mut keeper = TimeKeeper::load(&stage_markers);
    let mut events = Vec::new();
    let mut end_step = None;
    let mut elapsed = CLOCK_START;
    let end = stage_markers.end().expect("track-54_E has an end key");
    for frame in Frames::new(&bytes) {
        let frame = frame.expect("frame");
        let distance = read_f32(frame.car, DRIVELINE_OFFSET).expect("car block");
        let x = sample_x(&values, distance).expect("car inside the driveline");
        if end_step.is_none() && x >= end {
            end_step = Some(frame.post);
        }
        for _ in frame.pre..frame.post {
            let (next, checkpoint) = keeper.record(x, elapsed, MAX_TIME_STEP);
            keeper = next;
            if let Some(cp) = checkpoint {
                events.push((frame.post, cp.kind));
            }
            elapsed = advance_time(elapsed, MAX_TIME_STEP, false);
        }
    }

    let first_frame = Frames::new(&bytes).next().expect("frame").expect("frame");
    let expected = [
        (first_frame.post, CheckpointKind::Start),
        (11139, CheckpointKind::Split(0)),
        (21664, CheckpointKind::Split(1)),
        (34530, CheckpointKind::Finish),
    ];
    assert_eq!(events, expected);
    assert_eq!(end_step, Some(35404));
}
