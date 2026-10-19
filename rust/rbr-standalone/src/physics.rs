//! The physics step the gameplay loop drives (`rbr_gameplay::game::PhysicsStep`): the pure Rust physics on an image's
//! memory, and the pieces the headless, live and autopilot runs share around it (the boot's playback, the control
//! payload, a step's stop).

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::Ordering;

use rbr_formats::data::Data;
use rbr_formats::trk::DriveLineRecord;
use rbr_gameplay::control::CarControl;
use rbr_gameplay::driveline;
use rbr_gameplay::game::{PhysicsStep, StepSample};
use rbr_gameplay::markers::TickInput;
use rbr_gameplay::stage::{Stage, State, LOAD_SUBSTEPS};
use rbr_hybrid::pure::{self, Playback, ReadTrace, Stopped, CAR_PTR};
use rbr_recording::sound::{TyreMax, CAR_WHEELS, WHEEL_TYRE_SCRATCH};
use rbr_recording::{read_f32, CONTROL_BYTES};

use crate::boot;
use crate::stage::StageData;
use crate::twin::Twin;

/// `PhysicsEngine.MaxTimeStep` of the retail `physics.lsp`, for an image that does not hold the loaded definition.
pub const DT: f32 = rbr_recording::STEP_SECONDS;
pub const DRIVELINE_OFFSET: usize = 0x17c;
const VELOCITY_OFFSET: usize = 0x1c0;
const NOTIFY_A_SPEED_OFFSET: usize = 0x15f8;
/// The car bytes a step sample reads.
const SAMPLE_BYTES: usize = NOTIFY_A_SPEED_OFFSET + 4;

/// The sample `CCar::Vf6cx` gives the time keeper and the marker ticker, from the car block `car`.
pub fn sample(car: &[u8], track: &[DriveLineRecord]) -> StepSample {
    let position = read_f32(car, DRIVELINE_OFFSET).expect("car +0x17c");
    let velocity = std::array::from_fn(|lane| read_f32(car, VELOCITY_OFFSET + 4 * lane).expect("car +0x1c0"));
    StepSample {
        driveline_parameter: position,
        // Vf6cx writes -MB560[1] to +0x15f8; NotifyA reads that scalar from p[0].
        notify_a_speed: read_f32(car, NOTIFY_A_SPEED_OFFSET).expect("car +0x15f8"),
        notify_a_vector: None,
        marker: TickInput {
            dot150: driveline::dot150(track, position, velocity).unwrap_or_default(),
            finish_to_menu: true,
            ..TickInput::default()
        },
        outside_assistance: None,
        speed: velocity.iter().map(|v| v * v).sum::<f32>().sqrt(),
        retire: None,
    }
}

/// The pure Rust physics on the snapshot's memory.
pub struct Pure<'a> {
    pub playback: &'a Playback,
    /// Logs the image's bytes the steps read (`--read-set`).
    pub trace: Option<&'a ReadTrace>,
    pub track: &'a [DriveLineRecord],
    /// The snapshot's own step: its control was emitted before the snapshot, so it is not emitted again.
    pub own_step: bool,
    /// Word 6 of the recorded payload: the original's uninitialised stack word, which `CarControl` leaves out.
    pub word6: u32,
    /// The `StepForward` count of the frame's first substep (the load settle's 40 as the original's one
    /// `StepForward(40)`; the snapshot's own call). Each later driving substep is one `StepForward(1)`, as
    /// `TicPhysicsRecording` runs them; the load's later substeps are inside the first call.
    pub substeps: u32,
    /// The frame's tyre block word 1 (`rbr_recording::sound::TyreMax`), for the live sound record (the terms are kept
    /// with the tyre particles).
    pub tyre_max: TyreMax,
    /// `--twin`: the second side, stepped with the same payloads and compared after each step.
    pub twin: Option<&'a mut Twin>,
}

impl PhysicsStep for Pure<'_> {
    fn step(&mut self, control: Option<CarControl>, index: u32, inside_step: &mut dyn FnMut(StepSample)) {
        if index == 0 || control.is_some() {
            let payload = control.filter(|_| !self.own_step).map(|c| payload(&c, self.word6));
            if let Some(trace) = self.trace {
                trace.on.store(true, Ordering::Relaxed);
            }
            let substeps = if index == 0 { self.substeps } else { 1 };
            self.playback.step(payload.as_ref(), substeps);
            if let Some(twin) = self.twin.as_deref_mut() {
                twin.step(self.playback, payload.as_ref(), substeps);
            }
            if let Some(trace) = self.trace {
                trace.on.store(false, Ordering::Relaxed);
            }
            self.own_step = false;
            let mem = &self.playback.pure.mem;
            let wheels: [u32; 4] = std::array::from_fn(|k| mem.u32(mem.u32(CAR_PTR) + CAR_WHEELS + 4 * k as u32));
            for (scratch, term) in self.playback.pure.take_tyre_terms() {
                if let Some(k) = wheels.iter().position(|&w| w != 0 && w + WHEEL_TYRE_SCRATCH == scratch) {
                    self.tyre_max.add(k, term);
                }
            }
        }
        let mem = &self.playback.pure.mem;
        inside_step(StepSample {
            notify_a_vector: Some(self.playback.notify_a_vector()),
            outside_assistance: self.playback.pure.take_outside_assistance(),
            retire: self.playback.pure.take_retire(),
            ..sample(&mem.read(mem.u32(CAR_PTR), SAMPLE_BYTES), self.track)
        });
    }

    /// The state goes where the control emit reads its slot (S2, `lead/reports/standalone-s2.md`).
    fn app_state(&mut self, state: State) {
        pure::set_app_state(&mut self.playback.pure.mem.clone(), state.value());
        if let Some(twin) = self.twin.as_deref_mut() {
            pure::set_app_state(&mut twin.playback.pure.mem.clone(), state.value());
        }
    }

    /// The drive state's +0x570 (car +0x4b0, its +8), which NGP's live handler compares a gear selection with.
    fn drive_gear(&self) -> Option<u32> {
        let mem = &self.playback.pure.mem;
        Some(mem.u32(mem.u32(mem.u32(CAR_PTR) + 0x4b0 + 8) + 0x570))
    }
}

/// The 0x20-byte `CSigCarControl` payload of `control`, `word6` where the original leaves its stack word.
pub fn payload(control: &CarControl, word6: u32) -> [u8; CONTROL_BYTES] {
    let mut out = [0; CONTROL_BYTES];
    for (k, w) in control.words().iter().enumerate() {
        out[4 * k..4 * k + 4].copy_from_slice(&w.unwrap_or(word6).to_le_bytes());
    }
    out
}

/// Why a step's panic stopped the run: the pure host's stop, or a port's own failure.
pub fn why(e: &(dyn Any + Send)) -> String {
    match e.downcast_ref::<Stopped>() {
        Some(Stopped(s)) => s.to_string(),
        None => format!(
            "a port failed: {}",
            e.downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default()
        ),
    }
}

/// The load countdown the image's exe starts the load with: the immediate of `mov [esi+0x72c], 0x96` at 0x482bb6
/// (case 5 of `M004801d0`), which NGP patches to [`NGP_LOAD_SUBSTEPS`](rbr_gameplay::stage::NGP_LOAD_SUBSTEPS). The
/// stock value when the image has no code there.
pub fn load_substeps(mem: &rbr_physics::mem::Mem) -> u32 {
    mem.try_read(0x48_2bc0, 4).map_or(LOAD_SUBSTEPS, |b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// The boot's image at step 0 in the application state `Game` starts in, after `CCar::UpdateA720` (written to `dump`
/// when given), and its playback. Without `track_allocator` the pure allocator's blocks are in the image's pages and
/// its unwritten reads are not logged ([`rbr_hybrid::pure::Pure::untrack_allocator`]).
pub fn boot_playback(
    data: &Data,
    stage: &StageData,
    name: &str,
    race: &boot::Race,
    dump: Option<&Path>,
    track_allocator: bool,
) -> Result<(Playback, boot::Image), String> {
    let state = Stage::quick_race(stage.number, stage.car).load_ready().state();
    let image = boot::build(data, name, state.value(), race)?;
    let mut playback = Playback::from_image(image.mem.clone(), image.regs, image.slot, image.control_at, false);
    if let Some(ngp) = image.ngp {
        playback.pure.ngp_base = ngp.base;
    }
    if !track_allocator {
        playback.pure.untrack_allocator();
    }
    panic::catch_unwind(AssertUnwindSafe(|| boot::update_a720(&playback.pure, &image)))
        .map_err(|e| format!("the boot's CCar::UpdateA720: {} in {}", why(e.as_ref()), chain(&playback.pure)))?;
    boot::ngp_init(&mut playback.pure.mem.clone(), &image);
    if let Some(path) = dump {
        boot::dump(&playback.pure.mem, &image, path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok((playback, image))
}

/// The running bindings and stand-ins (`pure::Pure::chain`, outermost first) as a stop prints them:
/// `0x4ea720 > 0x4e9800`.
pub fn chain(pure: &pure::Pure) -> String {
    pure.chain().iter().map(|a| format!("{a:#x}")).collect::<Vec<_>>().join(" > ")
}

/// Runs `f` on a thread of its own: bindings call each other directly, and a deep step wants more stack than the main
/// thread's.
pub fn on_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> std::thread::Result<T> {
    std::thread::scope(|s| std::thread::Builder::new().stack_size(1 << 28).spawn_scoped(s, f).expect("spawn").join())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payload_takes_the_recorded_word_6_unless_a_gear_is_selected() {
        let control = CarControl { throttle: 1.0, flags: 0x10, ..CarControl::default() };
        let words = |bytes: [u8; CONTROL_BYTES]| -> Vec<u32> {
            bytes.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
        };
        assert_eq!(words(payload(&control, 0xdead)), [1.0f32.to_bits(), 0, 0, 0, 0, 0x10, 0xdead, 0]);
        let select = CarControl { flags: 0x04, gear: Some(3), ..control };
        assert_eq!(words(payload(&select, 0xdead))[6], 3);
    }
}
