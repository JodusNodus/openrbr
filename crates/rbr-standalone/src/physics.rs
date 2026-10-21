//! The physics step used by gameplay, headless replay and live driving. Each step consumes controls and returns
//! car state and stage events. The backend's memory representation and control encoding stay in `layout`.

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
use rbr_gameplay::stage::{Stage, State};
use rbr_hybrid::pure::{self, Playback, ReadTrace, Stopped};
use rbr_recording::sound::TyreMax;

use crate::boot;
use crate::stage::StageData;
use crate::twin::Twin;

/// `PhysicsEngine.MaxTimeStep` of the retail `physics.lsp`, for an image that does not hold the loaded definition.
pub const DT: f32 = rbr_recording::STEP_SECONDS;
mod layout;

pub use layout::{load_substeps, payload, DRIVELINE_OFFSET};

/// The car state gameplay reads after a step, independent of its backend storage.
#[derive(Clone, Copy, Debug)]
struct CarState {
    driveline_parameter: f32,
    velocity: [f32; 3],
    position: [f32; 3],
    notify_a_speed: f32,
}

impl CarState {
    fn sample(self, track: &[DriveLineRecord]) -> StepSample {
        StepSample {
            driveline_parameter: self.driveline_parameter,
            notify_a_speed: self.notify_a_speed,
            notify_a_vector: None,
            marker: TickInput {
                dot150: driveline::dot150(track, self.driveline_parameter, self.velocity).unwrap_or_default(),
                finish_to_menu: true,
                ..TickInput::default()
            },
            outside_assistance: None,
            speed: self.velocity.iter().map(|v| v * v).sum::<f32>().sqrt(),
            retire: None,
            position: self.position,
        }
    }
}

/// Decodes a recorded car state for the same gameplay sampling used by live physics.
pub fn sample(car: &[u8], track: &[DriveLineRecord]) -> StepSample {
    layout::recorded_car(car).sample(track)
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
        // A load settle call's later substeps ran inside its first; a driving substep is its own `StepForward(1)`, with
        // no control on the full replay buffer's step.
        if index == 0 || control.is_some() || self.substeps == 1 {
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
            layout::accumulate_tyre_terms(self.playback, &mut self.tyre_max);
        }
        inside_step(StepSample {
            notify_a_vector: Some(self.playback.notify_a_vector()),
            outside_assistance: self.playback.pure.take_outside_assistance(),
            retire: self.playback.pure.take_retire(),
            ..layout::car_state(self.playback).sample(self.track)
        });
    }

    /// The state goes where the control emit reads its slot (S2).
    fn app_state(&mut self, state: State) {
        layout::set_app_state(self.playback, state);
        if let Some(twin) = self.twin.as_deref_mut() {
            layout::set_app_state(&twin.playback, state);
        }
    }

    fn drive_gear(&self) -> Option<u32> {
        Some(layout::drive_gear(self.playback))
    }
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
    playback.pure.ngp_base = image.ngp.base;
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

/// Refuses a snapshot taken without NGP.dll: ReRBR steps NGP physics only.
pub fn require_ngp(playback: &Playback) -> Result<(), String> {
    if rbr_hybrid::ngp::runs_ngp(playback.pure.ngp_base) {
        Ok(())
    } else {
        Err("a 1.02 snapshot; ReRBR steps NGP physics only".into())
    }
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
