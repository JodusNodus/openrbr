//! The headless run's hooks for `harness/rsf_compare.py`: `--recorded-payload` feeds each step the recording's control
//! payload as it is (an NGP live drive's payload is unquantized, which the replay entry `QuantizedControl::from_payload`
//! rounds), and `--trace-out FILE` writes our car and wheels after each step as FRM1 frames to FILE, and to
//! `FILE.ext` per step: `u32 post, f32 force` (`steering_ffb::force`), `f32 engine speed` (the EngineNoise payload's
//! p[0], rad/s), `u32 gear` (the drive state's +0x54c), `f32 slip[4]` (each tyre noise block's |slip ratio|), which
//! FRM1 does not hold, `f32 max_ff_torque` (the def's `MaxFFTorque`, NGP's `S`, after the sensitivity), `u32 state`
//! (Game's application state after the step: the racing gate, `rbr_gameplay::ffb::racing_gate`, sends 0 outside
//! state 1), `i32 magnitude`: what the wheel is sent for the step while the gate holds, `rbr_gameplay::ffb::{magnitude,
//! dead_zone}` of the force with `--ffb-settings` and the payload's steering (NGP's thread keeps it past the gate's
//! closing and sends it when the next attempt's gate opens), and `i32 recorded magnitude`: the same of NGP's curve (`rbr_physics::ngp::ffb::curve`) of the recorded
//! car's filtered torque (`+0xaec`) and gate (`+0x1ec`) with our `S`, which leaves a physics difference out (i32
//! minimum under 1.02).
//! `--extra-payloads FILE` emits the payloads FILE lists for a step before the step's own: the control input's earlier
//! calls at that step (a call for help's payload, flags 0x10, then the frame's), which FRM1 does not keep.
//! `--wall-clock FILE` gives NGP's starter the recording's wall clock per step (`rbr_hybrid::ngp::set_wall_clock`).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use rbr_gameplay::control::CarControl;
use rbr_gameplay::ffb;
use rbr_gameplay::game::{PhysicsStep, StepSample};
use rbr_gameplay::stage::State;
use rbr_hybrid::pure::{self, CAR_PTR};
use rbr_physics::car_body::steering_ffb;
use rbr_physics::car_body::store::CarStore;
use rbr_physics::mem::Mem;
use rbr_recording::sound::{FmodBlock, Sound};
use rbr_recording::{Frame, MAGIC};

use crate::physics::{payload, Pure};

/// The hooks a headless run takes from the command line.
#[derive(Default, Clone, Copy)]
pub struct Hooks<'a> {
    pub recorded_payload: bool,
    pub extra_payloads: Option<&'a Extra>,
    pub wall_clock: Option<&'a WallClock>,
    pub trace_out: Option<&'a Path>,
    /// `--ffb-settings`: the player's settings for the trace's magnitude.
    pub ffb: ffb::Settings,
}

/// `--extra-payloads`: per step (its pre), the payloads emitted before its own, in order.
pub type Extra = HashMap<u32, Vec<[u8; 0x20]>>;

/// `--extra-payloads`' file: records of `u32 pre` and a 0x20-byte payload.
pub fn read_extra(path: &Path) -> Result<Extra, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() % 0x24 != 0 {
        return Err(format!("{}: not records of 0x24 bytes", path.display()));
    }
    let mut extra = Extra::new();
    for record in bytes.chunks_exact(0x24) {
        let pre = u32::from_le_bytes(record[..4].try_into().unwrap());
        extra.entry(pre).or_default().push(record[4..].try_into().unwrap());
    }
    Ok(extra)
}

/// `--wall-clock`: per step (its pre), the wall clock in us.
pub type WallClock = HashMap<u32, u64>;

/// `--wall-clock`'s file: records of `u32 pre` and `u64 us`.
pub fn read_wall_clock(path: &Path) -> Result<WallClock, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() % 12 != 0 {
        return Err(format!("{}: not records of 12 bytes", path.display()));
    }
    let word = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap());
    Ok(bytes
        .chunks_exact(12)
        .map(|r| (word(&r[..4]), u64::from(word(&r[4..8])) | u64::from(word(&r[8..])) << 32))
        .collect())
}

/// The pure physics fed `words`, the recorded payload, in place of each control the game emits, after emitting
/// `extra`.
pub struct Exact<'p, 'a> {
    pub pure: &'p mut Pure<'a>,
    pub words: Option<[u32; 8]>,
    pub extra: &'p [[u8; 0x20]],
}

impl PhysicsStep for Exact<'_, '_> {
    fn step(&mut self, control: Option<CarControl>, index: u32, inside_step: &mut dyn FnMut(StepSample)) {
        let control = control.map(|c| self.words.map_or(c, recorded));
        if control.is_some() {
            for payload in self.extra {
                self.pure.playback.emit(payload);
                if let Some(twin) = self.pure.twin.as_deref() {
                    twin.playback.emit(payload);
                }
            }
        }
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

/// `words` with its steering and pedals clamped to what the replay's quantized control holds (steering up to 127/127.5,
/// pedals 0..1), for the game side's control: NGP's unquantized payload reaches a full lock of 1.0, which rounds to
/// step 128. The physics is still fed `words` as they are ([`Exact`]).
pub fn representable(mut words: [u32; 8]) -> [u32; 8] {
    let clamp = |w: u32, lo: f32, hi: f32| f32::from_bits(w).clamp(lo, hi).to_bits();
    for w in &mut words[..4] {
        *w = clamp(*w, 0.0, 1.0);
    }
    words[4] = clamp(words[4], -1.0, 127.0 / 127.5);
    words
}

/// `--trace-out`'s two files.
pub struct Trace {
    frames: BufWriter<File>,
    ext: BufWriter<File>,
}

impl Trace {
    pub fn create(path: &Path) -> Result<Self, String> {
        let open = |p: &Path| File::create(p).map(BufWriter::new).map_err(|e| format!("{}: {e}", p.display()));
        let mut ext = path.as_os_str().to_owned();
        ext.push(".ext");
        Ok(Self { frames: open(path)?, ext: open(Path::new(&ext))? })
    }

    /// One step: `frame`'s counters, the control fed (`None`: the recorded one), Game's state after the step, the
    /// force-feedback settings, our car and wheels in `mem`.
    pub fn write(
        &mut self,
        frame: &Frame,
        control: Option<CarControl>,
        state: State,
        settings: &ffb::Settings,
        mem: &Mem,
    ) -> Result<(), String> {
        let control = control.map_or(frame.control, |c| payload(&c, frame.control_u32s()[6]));
        let steering = f32::from_le_bytes(control[16..20].try_into().unwrap());
        let body = pure::frame_bytes(mem);
        let car = mem.u32(CAR_PTR);
        let car_bytes = (body.len() - rbr_recording::WHEELS * rbr_recording::WHEEL_BYTES) as u32;
        let u32_at = |at: u32| mem.try_read(at, 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let sound = Sound::read_from(car, u32_at);
        let gear = FmodBlock::read_from(car, u32_at).gear as u32;
        let force = steering_ffb::force(mem, car);
        let mut ext = vec![frame.post, force.to_bits(), sound.engine[0], gear];
        ext.extend(sound.tyres.map(|t| t[8]));
        ext.push(mem.max_ff_torque(car).to_bits());
        let sent = |force: f32| ffb::dead_zone(ffb::magnitude(force, settings), steering, settings) as u32;
        let word = |at: usize| u32::from_le_bytes(frame.car[at..at + 4].try_into().unwrap());
        let recorded =
            sent(rbr_physics::ngp::ffb::curve(f32::from_bits(word(0xaec)), word(0x1ec) != 0, mem.max_ff_torque(car)));
        ext.extend([state.value(), sent(force), recorded]);
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

    #[test]
    fn a_full_lock_is_representable() {
        let mut words = [0; 8];
        words[4] = 1.0f32.to_bits();
        assert!(rbr_gameplay::control::QuantizedControl::from_payload(words).is_none());
        assert!(rbr_gameplay::control::QuantizedControl::from_payload(representable(words)).is_some());
    }
}
