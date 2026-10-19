//! Parses the recording named by `RBR_RECORDING`. Skipped when the variable is unset,
//! so `cargo test` works without a recording.

use rbr_recording::{Frames, WHEELS, WHEEL_BYTES};

#[test]
fn recording_parses_frame_by_frame() {
    let Ok(path) = std::env::var("RBR_RECORDING") else {
        eprintln!("skipping: RBR_RECORDING not set");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let frames: Vec<_> = Frames::new(&bytes).collect::<Result<_, _>>().unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(!frames.is_empty(), "{path}: no frames");
    for frame in &frames {
        assert_eq!(frame.car.len(), 0x1700, "frame at offset {}", frame.offset);
        for i in 0..WHEELS {
            assert_eq!(frame.wheel(i).map(<[u8]>::len), Some(WHEEL_BYTES));
        }
    }
}
