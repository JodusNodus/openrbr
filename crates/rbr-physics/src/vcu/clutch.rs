//! Launch clutch (`CScratchSim::LaunchClutch`, 0x5375b0): the clutch value ramps down at the
//! def's rate toward a target set by the engine speed, and the value goes to the drive (`Set4820`).
//! The asm decides the branches and the NaN behaviour; `src/rbr/RBR_E50_Sim.cpp` is a 28% skeleton.

use super::layout::{Def, Gearbox, State, Tuning};

/// What the launch clutch reads outside the state and the drive's gear fields: the scratch's launch
/// mode, the engine speed (`CDrive::Get4520`, rad/s) and the frame time.
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub launch_mode: u32,
    pub engine: f32,
    pub dt: f32,
    /// The clutch band: the engine speed (rad/s) below which the clutch is fully closed, above which it is fully
    /// open, and the slope between, in clutch per rad/s.
    pub tuning: Tuning,
}

/// One step of the launch clutch. Returns the value the caller sends to the drive (`CDrive::Set4820`),
/// or `None` when the launch mode is off and nothing is sent.
pub fn launch_clutch(input: Input, drive: &Gearbox, def: &Def, state: &mut State) -> Option<f32> {
    if input.launch_mode == 0 {
        return None;
    }
    // Fully closed while in gear 1 with the commanded gear 0 or 2 (neutral and second gear).
    let target = if drive.gear == 1 && (drive.commanded == 2 || drive.commanded == 0) {
        1.0
    } else {
        engine_target(input.engine, &input.tuning)
    };
    state.launch_clutch_dirty = 1;
    if state.launch_clutch > target {
        let ramped = state.launch_clutch - def.clutch_ramp_rate * input.dt;
        state.launch_clutch = ramped;
        if target > ramped {
            state.launch_clutch = target;
        }
    } else {
        state.launch_clutch = target;
    }
    Some(state.launch_clutch)
}

/// The clutch target at an engine speed. NaN falls through to NaN, as the asm's compares do.
fn engine_target(engine: f32, t: &Tuning) -> f32 {
    if t.clutch_from > engine {
        1.0
    } else if engine > t.clutch_to {
        0.0
    } else {
        (engine - t.clutch_to) * t.clutch_slope
    }
}
