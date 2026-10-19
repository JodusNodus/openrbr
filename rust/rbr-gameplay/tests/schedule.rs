//! The physics schedule of every recorded step against the frame headers: the four load settles of 40 substeps, then one
//! `StepForward(1)` per substep, with the step counter continuous. Runs only when `RBR_RECORDINGS` names the directory
//! with the `drive-<name>.bin` files.

use rbr_gameplay::tick::SETTLE_SUBSTEPS;
use rbr_recording::Frames;
use std::path::PathBuf;

const DRIVES: [&str; 4] = ["tanner-corolla", "tanner-damage", "arctic-snow", "tarmac-lancer"];

#[test]
fn recorded_substeps_follow_the_settle_then_one_per_step() {
    let Some(dir) = std::env::var_os("RBR_RECORDINGS").map(PathBuf::from) else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    for name in DRIVES {
        let path = dir.join(format!("drive-{name}.bin"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut step = 0u32;
        let mut frames = 0;
        // The schedule never ends: the zip stops at the last frame, and a bad frame panics in the loop.
        let expected = SETTLE_SUBSTEPS.iter().copied().chain(std::iter::repeat(1));
        for (index, (frame, substeps)) in Frames::new(&bytes).zip(expected).enumerate() {
            let frame = frame.unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                (frame.pre, frame.substeps),
                (step, substeps),
                "{name}: frame {index} at offset {} (pre, substeps)",
                frame.offset
            );
            assert_eq!(frame.post, step + substeps, "{name}: frame {index} post counter");
            step = frame.post;
            frames += 1;
        }
        assert!(frames > SETTLE_SUBSTEPS.len(), "{name}: {frames} frames, no driving");
        eprintln!("{name}: {frames} frames, the settle then one substep each, to step {step}");
    }
}
