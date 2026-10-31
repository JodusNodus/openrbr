//! One VCU step. Step runs the gear logic first; both then set the clutch, cut the rev limit, route the pedals
//! to the engine and the drive, set the brake and neutral lights, lock the diffs and decay the shift cooldown.
//! The order follows `notes/physics/vcu.md`. Step without the gear logic keeps the clutch.

use super::clutch;
use super::cut;
use super::gear;
use super::layout::{Def, Gearbox, Outputs, Scratch, State, Tuning};
use super::locks;

/// The brake light is on above this value.
const LIGHT_THRESHOLD: f32 = 0.1;

/// A call the step makes on the engine, the drive or the lights, in step order. The
/// objects they go to (engine, drive, lights) are the scratch's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Send {
    /// The throttle for the engine (the brake when the pedals are swapped).
    EnginePedal(f32),
    /// The throttle for the drive, the same value as `EnginePedal`.
    DriveThrottle(f32),
    /// The brake for the drive. The brake light follows it.
    DriveBrake(f32),
    /// The clutch slip, from the launch clutch or the fixed clutch.
    DriveClutch(f32),
    /// The diff lock of a channel (0 centre, 1 front, 2 rear).
    DiffLock { channel: u32, value: f32 },
    /// The four rev-limit cut flags.
    CutFlags([u32; 4]),
    /// The brake light.
    BrakeLight(bool),
    /// The neutral light, on in gear 0.
    NeutralLight(bool),
}

/// The values the step reads through the drive and the engine accessors, which the caller passes in.
#[derive(Clone, Copy, Debug)]
pub struct Input {
    /// Frame time (seconds).
    pub dt: f32,
    /// Engine speed, rad/s.
    pub engine: f32,
    /// Car speed.
    pub car_speed: f32,
    /// Drive speed for the diff locks.
    pub drive_speed: f32,
    /// Number of gears in the drive's definition (see `Gearbox::gear_count`).
    pub gear_count: u32,
    pub tuning: Tuning,
}

/// The gear logic, then the rest of the step (`step_basic`).
pub fn step(
    input: Input,
    scratch: &Scratch,
    def: &Def,
    state: &mut State,
    drive: &mut Gearbox,
    out: &mut Outputs,
    send: &mut dyn FnMut(Send),
) {
    let gear = gear::Input {
        launch_enabled: scratch.launch_enabled,
        vcu_enabled: scratch.vcu_enabled,
        pedals: scratch.pedals,
        engine: input.engine,
        speed: input.car_speed,
        dt: input.dt,
        gear_count: input.gear_count,
        tuning: input.tuning,
    };
    gear::update(gear, def, state, drive);
    step_basic(input, scratch, def, state, drive, out, send);
}

/// The step without the gear logic.
pub fn step_basic(
    input: Input,
    scratch: &Scratch,
    def: &Def,
    state: &mut State,
    drive: &mut Gearbox,
    out: &mut Outputs,
    send: &mut dyn FnMut(Send),
) {
    // The launch clutch sends its own value; with the launch mode off it returns None and the fixed clutch goes.
    let clutch =
        clutch::Input { launch_mode: scratch.launch_mode, engine: input.engine, dt: input.dt, tuning: input.tuning };
    let clutch = clutch::launch_clutch(clutch, drive, def, state).unwrap_or(scratch.fixed_clutch);
    out.reals.clutch_slip = clutch;
    send(Send::DriveClutch(clutch));
    send(Send::CutFlags(cut::rev_limit_cut(input.engine, def, state, input.tuning.cut_margin)));

    // The pedals are swapped only with the VCU on and the swap flag set.
    let pedals = scratch.pedals;
    let swapped = scratch.vcu_enabled != 0 && state.pedal_swap != 0;
    let (first, second) = if swapped { (pedals.brake, pedals.throttle) } else { (pedals.throttle, pedals.brake) };
    out.reals.throttle = first;
    out.reals.brake = second;
    out.reals.brake_scaled = out.reals.brake_scale * second;
    send(Send::EnginePedal(first));
    send(Send::DriveThrottle(first));
    send(Send::DriveBrake(second));
    send(Send::BrakeLight(second > LIGHT_THRESHOLD));

    let values = locks::diff_locks(pedals, def, input.drive_speed);
    for (channel, value) in values.into_iter().enumerate() {
        out.locks.set(channel, value);
        send(Send::DiffLock { channel: channel as u32, value });
    }
    send(Send::NeutralLight(drive.gear == 0));

    // The cooldown runs down while it is positive, and stops at zero.
    if state.shift_cooldown > 0.0 {
        state.shift_cooldown -= input.dt;
        if 0.0 > state.shift_cooldown {
            state.shift_cooldown = 0.0;
        }
    }
}
