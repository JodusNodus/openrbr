//! The drive workers (`rbr_physics::engine_driveline::drive_worker`): wheels, gears, pair, clutch, and `Step1E0`.

use rbr_physics::engine_driveline::drive_step::Step1Host;
use rbr_physics::engine_driveline::drive_worker::{GearHost, MatVecHost, Step3870Host, Step4990Host};
use rbr_physics::engine_driveline::{drive_leaf, drive_worker, layout, mat4};
use rbr_physics::mem::Mem;

use super::{f, CAR_GET30, EMIT_GEAR_CHANGE, FWD18E20, GEAR_CHANGE_SLOT, MATVEC, SIGNALS};
use crate::ngp::{call_target, clutch_scale};
use crate::{Ctx, Decline, Host, Regs, Return};

/// Runs a typed drive-worker port on the worker at `this`: loads its view, runs the port, stores the view back (the
/// same edge as the vector tests).
fn with_worker<T>(mem: &mut Mem, this: u32, port: impl FnOnce(&mut layout::DriveWorker) -> T) -> T {
    let mut w = layout::DriveWorker::load(mem, this);
    let out = port(&mut w);
    w.store(mem, this);
    out
}

/// `with_worker` for the gear workers, which also load the gear's words (stored last).
fn with_gear<T>(
    mem: &mut Mem,
    this: u32,
    port: impl FnOnce(&mut layout::DriveWorker, &mut layout::GearWork) -> T,
) -> T {
    let mut w = layout::DriveWorker::load(mem, this);
    let mut g = layout::GearWork::load(mem, this);
    let out = port(&mut w, &mut g);
    w.store(mem, this);
    g.store(mem, this);
    out
}

pub(super) fn set_lane_scale(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let x = layout::LaneScale::load(&ctx.mem, r.stack[0]);
    with_worker(&mut ctx.mem, r.ecx, |w| drive_leaf::set_lane_scale(w, x));
    Ok(Return::void(0, 4))
}

/// thiscall `()`, st0.
pub(super) fn gear_speed_error(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let w = layout::DriveWorker::load(&ctx.mem, r.ecx);
    let g = layout::GearWork::load(&ctx.mem, r.ecx);
    Ok(Return::float(drive_leaf::gear_speed_error(&w, &g), 0))
}

pub(super) fn clutch_values(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| drive_leaf::clutch_values(w, x));
    Ok(Return::void(0, 4))
}

pub(super) fn clutch_flags(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_worker(&mut ctx.mem, r.ecx, drive_leaf::clutch_flags);
    Ok(Return::void(0, 0))
}

pub(super) fn axle_pair_stages(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_worker(&mut ctx.mem, r.ecx, drive_leaf::axle_pair_stages);
    Ok(Return::void(0, 0))
}

pub(super) fn block_lane_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| drive_leaf::block_lane_step(w, x));
    Ok(Return::void(0, 4))
}

/// thiscall `(x)`, eax.
pub(super) fn block_pair_gate(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let x = r.stack[0] as usize;
    let gate = with_worker(&mut ctx.mem, r.ecx, |w| drive_leaf::block_pair_gate(w, x));
    Ok(Return::void(u32::from(gate), 4))
}

/// The wheel worker's external calls, run by the host on the emulator's memory. The port's values are written where the
/// original reads them, the original runs, and its results are read back. `Step4990` and `Step3870` are ports.
struct WheelCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    state: u32,
}

fn store_floats(mem: &mut Mem, at: u32, values: &[f32]) {
    for (k, v) in values.iter().enumerate() {
        mem.set_f32(at + 4 * k as u32, *v);
    }
}

impl drive_worker::Step4990Host for WheelCalls<'_> {
    fn mat4_inverse_transposed(&mut self, mat: [f32; 16]) -> [f32; 16] {
        mat4::inverse_transposed(mat)
    }
}

impl drive_worker::MatVecHost for WheelCalls<'_> {
    fn mat_vec(&mut self, m: [f32; 16], v: [f32; 4]) -> [f32; 4] {
        let matrix = self.state + layout::STEP_MATRIX_AT;
        let vec = self.state + layout::STEP_VEC_AT;
        let out = self.this + layout::LOC_AT;
        store_floats(&mut self.mem, matrix, &m);
        store_floats(&mut self.mem, vec, &v);
        self.host.call(MATVEC, matrix, &[vec, out]);
        self.mem.f32s(out)
    }
}

impl drive_worker::Step3870Host for WheelCalls<'_> {
    fn free_wheel_step(&mut self, w: &mut layout::DriveWorker, i: usize, dt: f32) {
        drive_worker::free_wheel_step(w, i, dt);
    }
}

fn wheel_calls<'a>(ctx: &Ctx<'a>, r: &Regs) -> WheelCalls<'a> {
    WheelCalls { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, state: ctx.mem.u32(r.ecx + 8) }
}

/// The gear workers' external calls, run by the host. The payload goes where the original's `M79B0` and `M9210`
/// keep it, their frame below the return address.
struct Gear<'a> {
    mem: Mem,
    host: &'a dyn Host,
    payload_at: u32,
}

impl GearHost for Gear<'_> {
    fn car_get30(&mut self, car: u32) -> u32 {
        self.host.call(CAR_GET30, car, &[]).eax
    }

    fn emit_gear_change(&mut self, payload: [u32; 4]) {
        for (k, word) in payload.iter().enumerate() {
            self.mem.set_u32(self.payload_at + 4 * k as u32, *word);
        }
        let slot = self.mem.u32(SIGNALS) + GEAR_CHANGE_SLOT;
        self.host.call(EMIT_GEAR_CHANGE, slot, &[self.payload_at]);
    }
}

fn gear<'a>(ctx: &Ctx<'a>, r: &Regs) -> Gear<'a> {
    Gear { mem: ctx.mem.clone(), host: ctx.host, payload_at: r.esp - 0x10 }
}

/// thiscall `(x)`.
pub(super) fn signal_gear_change(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = gear(ctx, r);
    let w = layout::DriveWorker::load(&ctx.mem, r.ecx);
    drive_worker::signal_gear_change(&w, &mut host, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(dt)`.
pub(super) fn gear_trim(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = gear(ctx, r);
    let dt = f(r.stack[0]);
    with_gear(&mut ctx.mem, r.ecx, |w, g| drive_worker::gear_trim(w, g, &mut host, dt));
    Ok(Return::void(0, 4))
}

/// thiscall `(dt)`, eax.
pub(super) fn gear_band_guard(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = gear(ctx, r);
    let dt = f(r.stack[0]);
    let changed = with_gear(&mut ctx.mem, r.ecx, |w, g| drive_worker::gear_band_guard(w, g, &mut host, dt));
    Ok(Return::void(u32::from(changed), 4))
}

/// thiscall `(dt)`, void.
pub(super) fn wheel_worker(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = wheel_calls(ctx, r);
    let dt = f(r.stack[0]);
    let split = crate::ngp::drive_split(ctx)?;
    with_worker(&mut ctx.mem, r.ecx, |w| drive_worker::wheel_worker(w, &mut host, dt, split));
    Ok(Return::void(0, 4))
}

/// thiscall `()`: the wheels' reset.
pub(super) fn reset_wheels(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = wheel_calls(ctx, r);
    with_worker(&mut ctx.mem, r.ecx, |w| drive_worker::reset_wheels(w, &mut host));
    Ok(Return::void(0, 0))
}

/// thiscall `(dt)`, void: the pair worker.
pub(super) fn pair_worker(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let dt = f(r.stack[0]);
    with_worker(&mut ctx.mem, r.ecx, |w| drive_worker::pair_worker(w, dt));
    Ok(Return::void(0, 4))
}

/// thiscall `()`: the pair worker's reset.
pub(super) fn reset_pair(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_worker(&mut ctx.mem, r.ecx, drive_worker::reset_pair);
    Ok(Return::void(0, 0))
}

/// thiscall `(i, dt)`: the free wheel's step.
pub(super) fn free_wheel_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let (i, dt) = (r.stack[0] as usize, f(r.stack[1]));
    with_worker(&mut ctx.mem, r.ecx, |w| drive_worker::free_wheel_step(w, i, dt));
    Ok(Return::void(0, 8))
}

/// thiscall `(x)`: one of the pair worker's leaves on wheel `x`.
fn pair_leaf(ctx: &mut Ctx, r: &Regs, leaf: fn(&mut layout::DriveWorker, usize)) -> Result<Return, Decline> {
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| leaf(w, x));
    Ok(Return::void(0, 4))
}

pub(super) fn pair_clutch_values(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    pair_leaf(ctx, r, drive_leaf::pair_clutch_values)
}

pub(super) fn pair_slip_value(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    pair_leaf(ctx, r, drive_leaf::pair_slip_value)
}

pub(super) fn pair_latch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    pair_leaf(ctx, r, drive_leaf::pair_latch)
}

/// thiscall `()`: the pair worker's axle stage.
pub(super) fn pair_axle_stage(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_worker(&mut ctx.mem, r.ecx, drive_leaf::pair_axle_stage);
    Ok(Return::void(0, 0))
}

/// thiscall `(a, out*, b, c, d)`: NGP's rewrite (`0x10047300`) of the clutch-torque worker (`M93A0`), stored at `out`
/// when the port gives a value.
pub(crate) fn clutch_torque(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = gear(ctx, r);
    let [a, out, b, c, d, ..] = r.stack;
    let written =
        with_gear(&mut ctx.mem, r.ecx, |w, g| drive_worker::clutch_torque(w, g, &mut host, f(a), [b, c, d].map(f)));
    if let Some(value) = written {
        ctx.mem.set_f32(out, value);
    }
    Ok(Return::void(0, 0x14))
}

/// The callees of `Step1E0`: the wheel and gear workers' (run as their bindings run them), and the car forwarder, which
/// the host runs.
struct Step1Calls<'a> {
    wheel: WheelCalls<'a>,
    gear: Gear<'a>,
    host: &'a dyn Host,
    this: u32,
}

impl Step4990Host for Step1Calls<'_> {
    fn mat4_inverse_transposed(&mut self, mat: [f32; 16]) -> [f32; 16] {
        self.wheel.mat4_inverse_transposed(mat)
    }
}

impl MatVecHost for Step1Calls<'_> {
    fn mat_vec(&mut self, m: [f32; 16], v: [f32; 4]) -> [f32; 4] {
        self.wheel.mat_vec(m, v)
    }
}

impl Step3870Host for Step1Calls<'_> {
    fn free_wheel_step(&mut self, w: &mut layout::DriveWorker, i: usize, dt: f32) {
        self.wheel.free_wheel_step(w, i, dt);
    }
}

impl GearHost for Step1Calls<'_> {
    fn car_get30(&mut self, car: u32) -> u32 {
        self.gear.car_get30(car)
    }

    fn emit_gear_change(&mut self, payload: [u32; 4]) {
        self.gear.emit_gear_change(payload);
    }
}

impl Step1Host for Step1Calls<'_> {
    fn fwd18e20(&mut self, car: u32, dt: f32, speed: f32) -> f64 {
        self.host.call(FWD18E20, car, &[dt.to_bits(), speed.to_bits()]).st0
    }

    fn ngp_speed_term(&mut self, dt: f32) -> f32 {
        let target = call_target(&self.gear.mem, STEP1E0_SPEED_TERM);
        if crate::ngp::file(target) == crate::ngp::STARTER {
            return crate::ngp::starter_step(&mut self.gear.mem, self.this, dt);
        }
        self.host.call(target, self.this, &[dt.to_bits()]).st0 as f32
    }

    fn ngp_curve_term(&mut self, args: [u32; 4], dt: f32) -> f32 {
        let [a, b, c, d] = args;
        let args = [a, b, c, d, dt.to_bits()];
        let target = call_target(&self.gear.mem, STEP1E0_CURVE_TERM);
        if crate::ngp::file(target) == crate::ngp::CURVE_TERM {
            if let Ok(v) = crate::ngp::curve_term(&mut self.gear.mem, self.this, args) {
                return v;
            }
        }
        self.host.call(target, self.this, &args).st0 as f32
    }

    fn ngp_wheel_worker(&mut self, dt: f32) {
        let target = call_target(&self.gear.mem, STEP1E0_WHEEL_WORKER);
        self.host.call(target, self.this, &[dt.to_bits()]);
    }
}

/// `Step1E0`'s calls of `M6C90`, `Hook4160` and `M9BC0`, which NGP retargets to its rewrites.
const STEP1E0_SPEED_TERM: u32 = 0x50a25c;
const STEP1E0_CURVE_TERM: u32 = 0x50a2a8;
const STEP1E0_WHEEL_WORKER: u32 = 0x50a46d;

/// thiscall `(dt)`: `CDrive::Step1E0`, the drive's step, with the car forwarder (and NGP's rewrites) run by the host.
pub(super) fn step1e0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let mut calls = Step1Calls { wheel: wheel_calls(ctx, r), gear: gear(ctx, r), host: ctx.host, this: r.ecx };
    let scale = clutch_scale(ctx);
    layout::step1e0(&mut calls, &mut ctx.mem, r.ecx, f(r.stack[0]), scale);
    Ok(Return::void(0, 4))
}

/// thiscall `(dt)`.
pub(super) fn gear_logic(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut host = gear(ctx, r);
    let dt = f(r.stack[0]);
    with_gear(&mut ctx.mem, r.ecx, |w, g| drive_worker::gear_logic(w, g, &mut host, dt));
    Ok(Return::void(0, 4))
}

// CDriveLine.
