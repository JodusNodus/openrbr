//! The input filter and the quantizer's pack against the original's peek: `peek/g5-filter.peek.bin` (the probes in
//! `peek/g5-filter.probes`, read after each `StepForward` of tanner-corolla) and the run's recording
//! `peek/g5-filter.bin`. The run is a playback, and the recorded controls are emitted after the filter, so the filter
//! saw the player's idle input: every raw channel is 0 on every step. The per-step check is therefore the grid's load
//! (every probed field, every step) and the filter and pack on zero input; the payload against the recording needs the
//! filter to see the recorded input, and is `#[ignore]`d (see its note).
//!
//! The settings are the vanilla install's `PC_InputFilter.ini`. Runs only when `RBR_RECORDINGS` names the recordings
//! directory; skipped otherwise.

use rbr_gameplay::control::QuantizedControl;
use rbr_gameplay::filter::{self, Filter, Filtered, Grid, Raw};
use rbr_gameplay::ini::Ini;
use rbr_physics::math::vec::Vec4;
use rbr_physics::rigid_body::rows::{self, RowFrame};
use rbr_recording::peek::{PeekFile, Probe, Record, Value};
use rbr_recording::{read_f32, Frame, Frames};
use std::fmt::Debug;
use std::path::{Path, PathBuf};

const DRIVE: &str = "g5-filter";
/// The physics step, `MaxTimeStep` (the f32 0.0070 of `Root/physics.rbz`, which the drive runs at).
const DT: f32 = 0.007;
/// `PC_InputFilter.ini` of the vanilla 1.02 install.
const VANILLA_INI: &str = "[STEER::BUTTON_DIGITAL]
Active = true
RiseRate = 3.0
FallRate = 5.0
maxSpeedFade = 0.5
MaxSpeedFadeSpeed = 27.0

[STEER::STICK]
Active = true
Parabolic = true
maxSpeedFade = 0.5
MaxSpeedFadeSpeed = 27.0

[STEER::AXIS]
Active = false
Parabolic = false
maxSpeedFade = 0.3
MaxSpeedFadeSpeed = 35.0
";

/// The bytes of the run: the recording and the peek file, or `None` when `RBR_RECORDINGS` is unset.
fn read_run() -> Option<(Vec<u8>, Vec<u8>)> {
    let recordings = std::env::var_os("RBR_RECORDINGS").map(PathBuf::from)?;
    let read = |path: &Path| std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let dir = recordings.join("peek");
    Some((read(&dir.join(format!("{DRIVE}.bin"))), read(&dir.join(format!("{DRIVE}.peek.bin")))))
}

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

fn peeked_u8(record: &Record, probes: &[Probe], name: &str) -> u8 {
    match peeked(record, probes, name) {
        Value::U8(v) => v,
        other => panic!("{name}: {other:?}, expected u8"),
    }
}

fn peeked_f32(record: &Record, probes: &[Probe], name: &str) -> f32 {
    match peeked(record, probes, name) {
        Value::F32(v) => v,
        other => panic!("{name}: {other:?}, expected f32"),
    }
}

fn expect_eq<T: PartialEq + Debug>(step: u32, field: &str, ours: T, theirs: T) {
    assert!(ours == theirs, "step {step}, {field}: ours {ours:?}, original {theirs:?}");
}

fn expect_f32(step: u32, field: &str, ours: f32, theirs: f32) {
    assert_eq!(ours.to_bits(), theirs.to_bits(), "step {step}, {field}: ours {ours:?}, original {theirs:?}");
}

/// The raw control the poll delivered before this tic, as the peek holds it: the accumulated action bits are the
/// peek's copy of them (the filter clears them at the tic), the rest the poll's values.
fn raw_of(record: &Record, p: &[Probe]) -> Raw {
    Raw {
        flags: peeked_u32(record, p, "fl_flags"),
        aux: peeked_u32(record, p, "fl_aux"),
        byte2: peeked_u32(record, p, "fl_b2"),
        channels: [
            peeked_f32(record, p, "raw_steer"),
            peeked_f32(record, p, "raw_ch1"),
            peeked_f32(record, p, "raw_ch2"),
            peeked_f32(record, p, "raw_ch3"),
            peeked_f32(record, p, "raw_ch4"),
        ],
        gear: peeked_u8(record, p, "raw_gear"),
        select: [0, 1, 2, 3, 4].map(|g| peeked_u32(record, p, &format!("grp{g}"))),
    }
}

#[test]
fn grid_loads_as_the_original_has_it() {
    let Some((_, peek_bytes)) = read_run() else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    let peek = PeekFile::parse(&peek_bytes).unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let grid = Grid::from_settings(&Ini::parse(VANILLA_INI));
    let mut steps = 0;
    for record in peek.records() {
        let record = record.unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
        let step = record.step;
        for (cell, ours) in grid.cells[0].iter().enumerate() {
            let fields = [
                ("active", f32::from(u8::from(ours.active))),
                ("nonlin", f32::from(u8::from(ours.nonlinear))),
                ("lin", ours.lin),
                ("c2", ours.c2),
                ("c3", ours.c3),
                ("rise", ours.rise),
                ("fall", ours.fall),
                ("respa", ours.fade),
                ("respb", ours.fade_speed),
            ];
            for (field, ours) in fields {
                let name = format!("g0c{cell}_{field}");
                let theirs = match peeked(&record, &peek.probes, &name) {
                    Value::U32(v) => v as f32,
                    Value::F32(v) => v,
                    other => panic!("{name}: {other:?}"),
                };
                expect_f32(step, &name, ours, theirs);
            }
        }
        for (group, cells) in grid.cells.iter().enumerate().skip(1) {
            let name = format!("g{group}c0_active");
            let theirs = peeked_u32(&record, &peek.probes, &name) != 0;
            expect_eq(step, &name, cells[0].active, theirs);
        }
        steps += 1;
    }
    eprintln!("{DRIVE}: the grid equal at {steps} steps");
}

#[test]
fn filter_and_pack_match_the_original_on_every_step() {
    let Some((frame_bytes, peek_bytes)) = read_run() else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    let frames: Vec<_> = Frames::new(&frame_bytes).collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let peek = PeekFile::parse(&peek_bytes).unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let records: Vec<Record> = peek.records().collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    assert_eq!(frames.len(), records.len(), "one peek record per frame");
    let p = &peek.probes;

    let mut filter = Filter::new(Grid::from_settings(&Ini::parse(VANILLA_INI)));
    // A settle frame (40 substeps) makes no tic: the filter holds its start values, which the peek shows as zeros.
    let mut out = Filtered::default();
    let (mut tics, mut settles) = (0, 0);
    for (frame, record) in frames.iter().zip(&records) {
        let step = record.step;
        if frame.post - frame.pre == 1 {
            let raw = raw_of(record, p);
            filter.set_speed(peeked_f32(record, p, "m36c"));
            filter.notify(&raw);
            out = filter.tic(DT);
            tics += 1;
        } else {
            settles += 1;
        }

        for (channel, name) in ["fl_ch1", "fl_ch2", "fl_ch3", "fl_ch4"].iter().enumerate() {
            let theirs = peeked_f32(record, p, name);
            expect_f32(step, name, out.channels[channel + 1], theirs);
        }
        expect_eq(step, "fl_flags", out.flags, peeked_u32(record, p, "fl_flags"));
        expect_eq(step, "fl_aux", out.aux, peeked_u32(record, p, "fl_aux"));
        expect_eq(step, "fl_b2", out.byte2, peeked_u32(record, p, "fl_b2"));

        let q = QuantizedControl::pack(&out);
        let ours = [q.flags, q.aux, q.byte2, q.steering as u8, q.throttle, q.brake, q.handbrake, q.clutch, q.gear];
        let w0 = peeked_u32(record, p, "qb0").to_le_bytes();
        let w4 = peeked_u32(record, p, "qb4").to_le_bytes();
        let theirs = [w0[0], w0[1], w0[2], w0[3], w4[0], w4[1], w4[2], w4[3], peeked_u8(record, p, "qb8")];
        for (i, (ours, theirs)) in ours.iter().zip(theirs).enumerate() {
            expect_eq(step, &format!("quantized byte {i}"), *ours, theirs);
        }
    }
    eprintln!("{DRIVE}: {tics} tics and {settles} settle frames; filter and pack equal at every step");
    assert_eq!(settles, 4, "four settle frames, no tic");
}

/// `CCar::Vf28x`'s `CSigCarToCarInputFilter` payload from a recorded car block, through `NotifyA`: the speed at +0x15f8
/// clamped at 0, and `MB560` of the vector at +0x220. Between steps the walk from the root frame to the car is the car's
/// one hook, `CSolid::E43VfB0` (0x52add0) on the +0x110 matrix: a trace of `MB560` on a tanner snapshot reads only
/// the car's +0x110..+0x134 and +0x220..+0x230.
fn speed_fade_input(frame: &Frame) -> f32 {
    let word = |offset: usize| read_f32(frame.car, offset).expect("car block");
    let mut vector = [Vec4 { x: word(0x220), y: word(0x224), z: word(0x228), w: word(0x22c) }];
    let m = std::array::from_fn(|k| if k < 9 { word(0x110 + 4 * k) } else { 0.0 });
    rows::transform_rows3(&RowFrame { m, origin: Vec4::default() }, &mut vector);
    let speed = word(0x15f8);
    let [v] = vector;
    filter::notify_a(if speed < 0.0 { 0.0 } else { speed }, [v.x, v.y, v.z, v.w])
}

/// The speed fade input against the original's: each new `m36c` the peek shows after a `StepForward` is `NotifyA` of
/// the car the step before left (`CCar::Vf28x` runs once a rendered frame, between steps; this playback renders about
/// every 8th step). Nothing `Vf28x` calls before its walk (`CEntity::Vf28` 0x51a3e0 and the rest) changes what the
/// walk reads, so the input needs no more than the step's state.
#[test]
fn speed_fade_input_matches_the_original() {
    let Some((frame_bytes, peek_bytes)) = read_run() else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    let frames: Vec<_> = Frames::new(&frame_bytes).collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let peek = PeekFile::parse(&peek_bytes).unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let records: Vec<Record> = peek.records().collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let mut updates = 0;
    for k in 1..records.len() {
        let (before, now) =
            (peeked_f32(&records[k - 1], &peek.probes, "m36c"), peeked_f32(&records[k], &peek.probes, "m36c"));
        if now.to_bits() != before.to_bits() {
            expect_f32(records[k].step, "m36c", speed_fade_input(&frames[k - 1]), now);
            updates += 1;
        }
    }
    eprintln!("{DRIVE}: {updates} speed fade updates, each equal to NotifyA of the step before");
    assert!(updates > 1000, "{updates} updates");
}

/// The recorded payload against the payload of the filter on the same run. Ignored: the playback emits the recorded
/// control at the quantizer's output (`harness/plugin`, `OnPlayerControl` and `CallEmit`), after the filter. The
/// filter's own input is the player's, which is 0 on every step (the peek), so the first recorded step with a non-zero
/// control is where this fails. Making it pass needs a run whose raw input is the recorded one (a live recording, or a
/// playback that feeds `NotifyB`).
#[test]
#[ignore = "the playback injects the recorded payload after the filter; the filter saw idle input"]
fn payload_matches_the_recording() {
    let Some((frame_bytes, peek_bytes)) = read_run() else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    let frames: Vec<_> = Frames::new(&frame_bytes).collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let peek = PeekFile::parse(&peek_bytes).unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let records: Vec<Record> = peek.records().collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{DRIVE}: {e}"));
    let p = &peek.probes;
    let mut filter = Filter::new(Grid::from_settings(&Ini::parse(VANILLA_INI)));
    let mut out = Filtered::default();
    for (frame, record) in frames.iter().zip(&records) {
        if frame.post - frame.pre == 1 {
            let raw = raw_of(record, p);
            filter.set_speed(peeked_f32(record, p, "m36c"));
            filter.notify(&raw);
            out = filter.tic(DT);
        }
        let words = frame.control_u32s();
        let ours = QuantizedControl::pack(&out).car_control();
        if let Some(i) = ours.first_difference(words) {
            panic!("step {}: word {i} is {:08x?}, recorded {:08x}", record.step, ours.words()[i], words[i]);
        }
    }
}
