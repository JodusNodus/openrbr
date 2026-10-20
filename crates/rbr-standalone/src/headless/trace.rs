//! The headless run's hooks for `harness/rsf_compare.py`: `--recorded-payload` feeds each step the recording's control
//! payload as it is (an NGP live drive's payload is unquantized, which the replay entry `QuantizedControl::from_payload`
//! rounds), and `--trace-out FILE` writes our car and wheels after each step as FRM1 frames to FILE, and to
//! `FILE.ext` per step: `u32 post, f32 force` (`steering_ffb::force`), `f32 engine speed` (the EngineNoise payload's
//! p[0], rad/s), `u32 gear` (the drive state's +0x54c), `f32 slip[4]` (each tyre noise block's |slip ratio|), which
//! FRM1 does not hold.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use rbr_gameplay::control::CarControl;
use rbr_gameplay::game::{PhysicsStep, StepSample};
use rbr_gameplay::stage::State;
use rbr_hybrid::pure::{self, CAR_PTR};
use rbr_physics::car_body::steering_ffb;
use rbr_physics::mem::Mem;
use rbr_physics::Variant;
use rbr_recording::sound::{FmodBlock, Sound};
use rbr_recording::{Frame, MAGIC};

use crate::physics::{payload, Pure};

/// The hooks a headless run takes from the command line.
#[derive(Default, Clone, Copy)]
pub struct Hooks<'a> {
    pub recorded_payload: bool,
    pub trace_out: Option<&'a Path>,
}

/// The pure physics fed `words`, the recorded payload, in place of each control the game emits.
pub struct Exact<'p, 'a> {
    pub pure: &'p mut Pure<'a>,
    pub words: Option<[u32; 8]>,
}

impl PhysicsStep for Exact<'_, '_> {
    fn step(&mut self, control: Option<CarControl>, index: u32, inside_step: &mut dyn FnMut(StepSample)) {
        let control = control.map(|c| self.words.map_or(c, recorded));
        self.pure.step(control, index, inside_step);
    }

    fn app_state(&mut self, state: State) {
        self.pure.app_state(state);
    }

    fn drive_gear(&self) -> Option<u32> {
        self.pure.drive_gear()
    }
}

/// The payload words as a control, word 6 included.
fn recorded(w: [u32; 8]) -> CarControl {
    let f = f32::from_bits;
    CarControl {
        throttle: f(w[0]),
        brake: f(w[1]),
        handbrake: f(w[2]),
        clutch: f(w[3]),
        steering: f(w[4]),
        flags: w[5],
        gear: Some(w[6]),
        aux: w[7],
    }
}

/// `--trace-out`'s two files.
pub struct Trace {
    frames: BufWriter<File>,
    ext: BufWriter<File>,
    variant: Variant,
}

impl Trace {
    pub fn create(path: &Path, variant: Variant) -> Result<Self, String> {
        let open = |p: &Path| File::create(p).map(BufWriter::new).map_err(|e| format!("{}: {e}", p.display()));
        let mut ext = path.as_os_str().to_owned();
        ext.push(".ext");
        Ok(Self { frames: open(path)?, ext: open(Path::new(&ext))?, variant })
    }

    /// One step: `frame`'s counters, the control fed (`None`: the recorded one), our car and wheels in `mem`.
    pub fn write(&mut self, frame: &Frame, control: Option<CarControl>, mem: &Mem) -> Result<(), String> {
        let control = control.map_or(frame.control, |c| payload(&c, frame.control_u32s()[6]));
        let body = pure::frame_bytes(mem);
        let car = mem.u32(CAR_PTR);
        let car_bytes = (body.len() - rbr_recording::WHEELS * rbr_recording::WHEEL_BYTES) as u32;
        let u32_at = |at: u32| mem.try_read(at, 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let sound = Sound::read_from(car, u32_at);
        let gear = FmodBlock::read_from(car, u32_at).gear as u32;
        let force = steering_ffb::force(mem, car, self.variant);
        let mut ext = vec![frame.post, force.to_bits(), sound.engine[0], gear];
        ext.extend(sound.tyres.map(|t| t[8]));
        let header = [MAGIC, frame.pre, frame.post, frame.substeps, car_bytes];
        let frame_bytes: Vec<u8> = header.iter().flat_map(|w| w.to_le_bytes()).chain(control).chain(body).collect();
        let ext: Vec<u8> = ext.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.frames
            .write_all(&frame_bytes)
            .and_then(|()| self.ext.write_all(&ext))
            .map_err(|e| format!("--trace-out: {e}"))
    }

    pub fn finish(mut self) -> Result<(), String> {
        self.frames.flush().and_then(|()| self.ext.flush()).map_err(|e| format!("--trace-out: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_payload_feeds_its_own_words() {
        let words = [0.3f32.to_bits(), 0, 1.0f32.to_bits(), 0x3c00_0001, (-0.123_456_7f32).to_bits(), 4, 3, 1];
        let bytes = payload(&recorded(words), 0xdead_beef);
        let back: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(back, words);
    }
}
