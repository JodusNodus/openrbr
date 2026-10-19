//! The car-control payload of every step of the reference drives, against the original's: each recorded payload
//! must come out of `QuantizedControl::car_control` (`NotifyFilt`, 0x48d130) bit for bit from the quantized bytes it
//! rounds to. Runs only when `RBR_RECORDINGS` names the directory with the `drive-<name>.bin` files.

use rbr_gameplay::control::QuantizedControl;
use rbr_recording::Frames;
use std::path::PathBuf;

const DRIVES: [&str; 4] = ["tanner-corolla", "tanner-damage", "arctic-snow", "tarmac-lancer"];

#[test]
fn recorded_payloads_are_unpacked_quantized_controls() {
    let Some(dir) = std::env::var_os("RBR_RECORDINGS").map(PathBuf::from) else {
        eprintln!("RBR_RECORDINGS unset; skipped");
        return;
    };
    for name in DRIVES {
        let path = dir.join(format!("drive-{name}.bin"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut steps = 0;
        for frame in Frames::new(&bytes) {
            let frame = frame.unwrap_or_else(|e| panic!("{name}: {e}"));
            let words = frame.control_u32s();
            let quantized = QuantizedControl::from_payload(words)
                .unwrap_or_else(|| panic!("{name} step {}: {words:08x?} is no quantized control", frame.pre));
            let ours = quantized.car_control();
            if let Some(i) = ours.first_difference(words) {
                panic!("{name} step {}: word {i} is {:08x?}, recorded {:08x}", frame.pre, ours.words()[i], words[i]);
            }
            steps += 1;
        }
        assert!(steps > 0, "{name}: no frames");
        eprintln!("{name}: {steps} steps equal");
    }
}
