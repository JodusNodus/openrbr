//! `CCar::StepEngine` (exe 0x4e9930, `ret 4`): the per-step update of the steering cells and the force-feedback torque. The slew moves toward the
//! target seed by `(1 / dt) * (seed - slew)`, its step capped at 20 per second. The steering target is the new slew
//! times the def's two gains, and its rate is the change per second. The demand is then cleared, and the scratch sim,
//! drive and entity steps, `Damp66D0`, `UpdateFfbTorque` and (with the snapshot flag set) `Snap9360` run. The profiler
//! channel (`E46ProfChan`) is the binding's scope, not this port's. The asm fixes the order (`car_body/README.md` (git 9c28c0b8)).

use super::accessors::set_cell;
use super::damper;
use super::layout::{self, Real};
use crate::car_body::store::CarStore;
use crate::mem::Mem;

/// The calls `StepEngine` makes out of its own body. Each cell write is its own `SetDirty` (inside `set_cell`).
pub trait StepEngineCalls<S = Mem> {
    /// `CScratchSim::Step(dt)` (0x537f50) on the car's scratch sim at +0x850.
    fn scratch_step(&mut self, mem: &mut S, car: u32, dt: f32);
    /// `CDrive::Step1E0(dt)` (0x50a1e0) on the drive at +0x4b0.
    fn drive_step(&mut self, mem: &mut S, car: u32, dt: f32);
    /// `CEntity::E43Vf58(dt)` (0x51a250) on the car.
    fn entity_vf58(&mut self, mem: &mut S, car: u32, dt: f32);
    /// `CCar::UpdateFfbTorque` (0x4e7ff0). Only its kernel is ported (`steering_ffb::update`).
    fn update_ffb_torque(&mut self, mem: &mut S, car: u32);
    /// `CCar::Snap9360` (0x4e9360), run only while the snapshot flag (+0x9e4) is set.
    fn snap_9360(&mut self, mem: &mut S, car: u32);
}

/// `CCar::StepEngine(this, dt)`.
pub fn step_engine<S: CarStore>(calls: &mut impl StepEngineCalls<S>, mem: &mut S, car: u32, dt: f32) {
    let inv_dt = 1.0 / dt;
    let slew_at = layout::steer_slew_at(car);
    let slew = Real::load(mem, slew_at);
    let mut f5 = mem.steer_input(car) - slew.value;
    let mut f4 = inv_dt * f5;
    // `|f4|`, with the asm's compare: NaN takes the negated branch (`comiss` then `jbe`).
    f4 = if f4 > 0.0 { f4 } else { 0.0 - f4 };
    if f4 > 20.0 {
        f5 = (f5 / f4) * 20.0;
    }
    let new_slew = slew.value + f5;
    set_cell(slew, new_slew).store(mem, slew_at);

    let old_target = mem.target_b0c(car);
    let target_at = layout::target_b0c_at(car);
    let target = (new_slew * mem.max_steering_lock(car)) * mem.steering_rack_ratio(car);
    set_cell(Real::load(mem, target_at), target).store(mem, target_at);

    let rate_at = layout::rate_b1c_at(car);
    let rate = (target - old_target) * inv_dt;
    set_cell(Real::load(mem, rate_at), rate).store(mem, rate_at);

    let demand_at = layout::steer_torque_sum_at(car);
    set_cell(Real::load(mem, demand_at), 0.0).store(mem, demand_at);

    calls.scratch_step(mem, car, dt);
    calls.drive_step(mem, car, dt);
    calls.entity_vf58(mem, car, dt);
    damper::update_dampers(mem, car);
    calls.update_ffb_torque(mem, car);
    if mem.snapshot_on(car) {
        calls.snap_9360(mem, car);
    }
}
