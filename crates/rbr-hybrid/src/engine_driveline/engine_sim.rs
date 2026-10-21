//! The engine (`CEngine`): its options, damper, torque, intake and turbo steps, and the `Sim8890`/`Sim8E20` walks.

use rbr_physics::engine_driveline::{engine, layout};
use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::Mem;

use super::{f, E01LOG, EMIT_NOISE_SITE, FLOOR_S32_FORMAT, GRAPH_PUSH, MB560, SIGNALS, WALKER};
use crate::ngp::engine_tuning;
use crate::{Ctx, Decline, Host, Regs, Return};
use rbr_physics::engine_driveline::store::EngineStore;

/// thiscall `()`, st0.
pub(super) fn abs_rpm_term(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(layout::abs_rpm_term(&mut ctx.mem, r.ecx) as f64, 0))
}

/// thiscall `(slot)`, eax.
pub(super) fn slot_roll(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let i = r.stack[0];
    if i >= 4 {
        return Err(Decline::Unsupported("roll slot outside 0..4"));
    }
    Ok(Return::void(layout::slot_roll(&mut ctx.mem, r.ecx, i), 4))
}

/// thiscall `(a, b, c, table*, d, block*, out*)`.
pub(super) fn smooth_damper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, table, d, block_at, out, ..] = r.stack;
    let g = layout::smooth_damper(&mut ctx.mem, r.ecx, [f(a), f(b), f(c)], table, f(d), block_at);
    ctx.mem.set_f32(out, g);
    Ok(Return::void(0, 0x1c))
}

/// thiscall `(a, b, table*, d, block*, out*)`.
pub(super) fn solve_damper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, table, d, block_at, out, ..] = r.stack;
    let tuning = engine_tuning(ctx);
    let v = layout::solve_damper(&mut ctx.mem, r.ecx, [f(a), f(b)], table, f(d), block_at, &tuning);
    ctx.mem.set_f32(out, v);
    Ok(Return::void(0, 0x18))
}

/// thiscall `(a, b_out*, c_out*)`.
pub(super) fn crank_tick(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b_out, c_out, ..] = r.stack;
    let (b, c) = layout::crank_tick(&mut ctx.mem, r.ecx, f(a));
    ctx.mem.set_u32(b_out, b as u32);
    ctx.mem.set_f32(c_out, c);
    Ok(Return::void(0, 0xc))
}

/// thiscall `()`, st0.
pub(super) fn rev_gate(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(layout::rev_gate(&ctx.mem, r.ecx, &engine_tuning(ctx)), 0))
}

/// thiscall `(a)`.
pub(super) fn misfire_roll(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::misfire_roll(&mut ctx.mem, r.ecx, f(r.stack[0]));
    Ok(Return::void(0, 4))
}

/// thiscall `(a)`, st0.
pub(super) fn engine_torque(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let st0 = layout::engine_torque(&mut ctx.mem, r.ecx, f(r.stack[0]));
    Ok(Return::float(st0 as f64, 4))
}

/// thiscall `()`.
pub(super) fn compliance(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::compliance(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(v)`, eax: the options pointer (`layout::set_options`).
pub(super) fn set_options(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::set_options(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(eax, 4))
}

/// thiscall `(a, b, c, d)`, eax: the four cut words (`layout::set_cut_flags`).
pub(super) fn set_cut_flags(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let words = [r.stack[0], r.stack[1], r.stack[2], r.stack[3]];
    let eax = layout::set_cut_flags(&mut ctx.mem, r.ecx, words);
    Ok(Return::void(eax, 0x10))
}

/// thiscall `()`, st0: the options block's float at +0x58, or 0 (`layout::options_gate_58`).
pub(super) fn options_gate_58(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(layout::options_gate_58(&ctx.mem, r.ecx) as f64, 0))
}

/// thiscall `()`, st0: the damper bias of the state's float at +0x28 (`layout::sim_e970`).
pub(super) fn sim_e970(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(layout::sim_e970(&ctx.mem, r.ecx), 0))
}

/// thiscall `()`, eax: 1 when the state's float at +0x188 is above 40 (`layout::rpm_above_40`).
pub(super) fn rpm_above_40(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::rpm_above_40(&ctx.mem, r.ecx), 0))
}

/// thiscall `()`, eax: the state or the world object (`layout::sim_e9a0`); nothing is written.
pub(super) fn sim_e9a0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::sim_e9a0(&ctx.mem, r.ecx), 0))
}

/// thiscall `()`, st0: the state's float at +0x68 when the definition's flag is set, else 0 (`layout::sim_ee60`).
pub(super) fn sim_ee60(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(layout::sim_ee60(&ctx.mem, r.ecx) as f64, 0))
}

/// thiscall `()`, st0: the state's float at +0x188 (`layout::engine_state_speed`).
pub(super) fn engine_state_speed(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(layout::engine_state_speed(&ctx.mem, r.ecx) as f64, 0))
}

/// thiscall `(x)`: the brake-side damper step; the `eax` is the options block or the state (`layout::sim8610`).
pub(super) fn sim8610(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::sim8610(&mut ctx.mem, r.ecx, f(r.stack[0]));
    Ok(Return::void(eax, 4))
}

/// thiscall `(x)`: the fuel and air terms and the throttle; the `eax` is the options block (`layout::fuel_air_terms`).
pub(super) fn fuel_air_terms(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::fuel_air_terms(&mut ctx.mem, r.ecx, f(r.stack[0]));
    Ok(Return::void(eax, 4))
}

/// thiscall `(pedal)`.
pub(super) fn set_scratch08(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut this = layout::EngineThis::load(&ctx.mem, r.ecx);
    engine::set_pedal(&mut this.pedal, r.stack[0]);
    this.store(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 4))
}

/// thiscall `(row, s*)`, st0.
pub(super) fn exhaust_blend(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [row, q, ..] = r.stack;
    Ok(Return::float(layout::exhaust_blend_at(&ctx.mem, r.ecx, row, q), 8))
}

/// `E43EngineSim::Sim80D0` (thiscall, `x`): the reset of the table entry `x`, run by the original.
const SIM80D0: u32 = 0x5180d0;

/// The host calls of `Sim8890`: the car's slot 12 (`S30`, a virtual call on the car) and `MB560` on the walker, whose
/// output is the scratch below the binding's frame (the original's frame held it).
struct Sim8890Calls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    scratch: u32,
}

impl engine::Sim8890Host for Sim8890Calls<'_> {
    fn s30(&mut self, car: u32) -> u32 {
        let slot12 = self.mem.u32(self.mem.u32(car) + 4 * 12);
        self.host.call(slot12, car, &[]).eax
    }

    fn mb560(&mut self, a: u32, b: u32) -> [f32; 4] {
        let c = self.scratch;
        self.host.call(MB560, self.mem.u32(WALKER), &[a, b, c]);
        self.mem.f32s(c)
    }
}

/// The host calls of `Sim8E20`: the walk (`Sim8890Calls`), `Sim80D0` (through `Sim8D90`), and the three sinks. The sinks run
/// the original code.
struct Sim8E20Calls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    walk: Sim8890Calls<'a>,
}

impl engine::E43Host for Sim8E20Calls<'_> {
    fn intake_state_reset(&mut self, this: u32, x: u32) {
        self.host.call(SIM80D0, this, &[x]);
    }
}

impl engine::Sim8890Host for Sim8E20Calls<'_> {
    fn s30(&mut self, car: u32) -> u32 {
        self.walk.s30(car)
    }

    fn mb560(&mut self, a: u32, b: u32) -> [f32; 4] {
        self.walk.mb560(a, b)
    }
}

impl engine::EngineStepHost for Sim8E20Calls<'_> {
    fn emit(&mut self, signal: u32) {
        let manager = self.mem.u32(SIGNALS) + 0x6050;
        let target = crate::ngp::call_target(&self.mem, EMIT_NOISE_SITE);
        self.host.call(target, manager, &[signal]);
    }

    fn graph_push(&mut self, graph: u32, value: f32) {
        self.host.call(GRAPH_PUSH, graph, &[value.to_bits()]);
    }

    /// `E01Log(6, 5, "FloorS32: Invalid argument %f\n", value)`: the double of `value` on the stack after the format.
    fn floor_log(&mut self, value: f32) {
        let bits = (value as f64).to_bits();
        self.host.call(E01LOG, 0, &[6, 5, FLOOR_S32_FORMAT, bits as u32, (bits >> 32) as u32]);
    }
}

/// The emit of `Sim8CF0` on the signal object (0x50ee90, `ret 4`), run by the original: its call site, which NGP
/// retargets (to 0x100ee0f0).
const SIGNAL_EMIT_SITE: u32 = 0x518D70;

/// `Sim8CF0`'s host: the emit runs as the original does.
struct Sim8CF0Calls<'a> {
    host: &'a dyn Host,
    /// The target of `SIGNAL_EMIT_SITE`.
    emit: u32,
}

impl engine::Sim8CF0Host for Sim8CF0Calls<'_> {
    fn signal(&mut self, obj: u32, arg: u32) -> u32 {
        self.host.call(self.emit, obj, &[arg]).eax
    }
}

/// thiscall `()`, eax: the emit's eax (`layout::sim8cf0`).
pub(super) fn sim8cf0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let host = ctx.host;
    let emit = crate::ngp::call_target(&ctx.mem, SIGNAL_EMIT_SITE);
    let eax = layout::sim8cf0(&mut Sim8CF0Calls { host, emit }, &mut ctx.mem, r.ecx);
    Ok(Return::void(eax, 0))
}

/// thiscall `(a, b)`, st0: one engine step (`layout::engine_step`); the walk and the sinks are host calls.
pub(crate) fn engine_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let walk = Sim8890Calls { mem: ctx.mem.clone(), host: ctx.host, scratch: r.esp - 0x800 };
    let mut calls = Sim8E20Calls { mem: ctx.mem.clone(), host: ctx.host, walk };
    let tuning = engine_tuning(ctx);
    let st0 = layout::engine_step(&mut calls, &mut ctx.mem, r.ecx, f(r.stack[0]), f(r.stack[1]), &tuning);
    Ok(Return::float(st0 as f64, 8))
}

/// thiscall `(x)`, void: the car's damper step (`layout::sim8890`), its walk through the host.
pub(super) fn sim8890(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let mut calls = Sim8890Calls { mem: ctx.mem.clone(), host: ctx.host, scratch: r.esp - 0x800 };
    let tuning = engine_tuning(ctx);
    let eax = layout::sim8890(&mut calls, &mut ctx.mem, r.ecx, f(r.stack[0]), &tuning);
    Ok(Return::void(eax, 4))
}

/// The host calls of `Sim8D90`: `Sim80D0` runs in the original and writes the entry it resets.
struct Sim43Calls<'a> {
    host: &'a dyn Host,
}

impl engine::E43Host for Sim43Calls<'_> {
    fn intake_state_reset(&mut self, this: u32, x: u32) {
        self.host.call(SIM80D0, this, &[x]);
    }
}

/// thiscall, no arguments: `1 - cos` of the engine's value into the lane at `+0x24` (`layout::sim_ebb0`).
pub(super) fn sim_ebb0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::sim_ebb0(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(pos, x)`, st0: the arc position of `pos` on the drive line (`drive_line::sample_big`).
pub(super) fn sample_big(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [pos, x, ..] = r.stack;
    Ok(Return::float(layout::sample_big(&ctx.mem, r.ecx, pos, f(x)) as f64, 8))
}

/// thiscall `(b, s, out)`, eax = `out`: `out = this + (b - this) * s` (`Vec4::lerp`, the `E11_VecLerp` of the drive line).
pub(super) fn vec_lerp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, s, out, ..] = r.stack;
    let a = Vec4::load(&ctx.mem, r.ecx);
    let v = a.lerp(Vec4::load(&ctx.mem, b), f(s));
    v.store(&mut ctx.mem, out);
    Ok(Return::void(out, 12))
}

/// thiscall `(x, y)`, x87 return: the table entry `x` takes the key `y` (`layout::sim8d90`).
pub(super) fn sim8d90(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [x, y, ..] = r.stack;
    let v = layout::sim8d90(&mut Sim43Calls { host: ctx.host }, &mut ctx.mem, r.ecx, x, y);
    Ok(Return::float(v, 8))
}

/// thiscall `()`, eax.
pub(super) fn stall_guard(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(engine_running(ctx, r.ecx) as u32, 0))
}

/// `CEngine::SimECF0` on `engine`: whether it runs.
pub(crate) fn engine_running(ctx: &Ctx, engine: u32) -> bool {
    let mem = &ctx.mem;
    let objects = layout::EngineObjects::at(mem, engine);
    let state = layout::EngineState::load(mem, objects.state);
    let options = layout::EngineOptions::load(mem, objects.options);
    engine::stall_guard(state.speed, &options, engine_tuning(ctx).stall_speed)
}

/// thiscall `(x, q*)`.
pub(super) fn torque_curve(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [x, q_at, ..] = r.stack;
    layout::torque_curve(&mut ctx.mem, r.ecx, f(x), q_at);
    Ok(Return::void(0, 8))
}

/// thiscall `(i)`: the table entry `i` and its roll (`layout::intake_state_reset`). The binding declines slots past the four
/// stall terms, as `SimD870` does.
pub(super) fn intake_state_reset(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let i = r.stack[0];
    if i >= 4 {
        return Err(Decline::Unsupported("entry outside 0..4"));
    }
    let tuning = engine_tuning(ctx);
    Ok(Return::void(layout::intake_state_reset(&mut ctx.mem, r.ecx, i, &tuning), 4))
}

/// thiscall `(x)`: the damper and load stage (`layout::turbo_energy_step`). Returns the eax the original leaves.
pub(super) fn turbo_energy_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let [x, ..] = r.stack;
    let tuning = engine_tuning(ctx);
    Ok(Return::void(layout::turbo_energy_step(&mut ctx.mem, r.ecx, f(x), &tuning), 4))
}

/// thiscall `(setup)`, `ret 4`: `CDrive::M8770` (0x508770), the setup into the drive's definition
/// (`layout::drive_setup_into_def`).
pub(super) fn drive_setup(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let def = ctx.mem.drive_def(r.ecx);
    layout::drive_setup_into_def(&mut ctx.mem, def, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(worker)`, `ret 4`: `E39_Link3AB0` (0x503ab0), the worker object stored and enabled. Leaves eax the
/// argument.
pub(super) fn link_worker(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::link_worker(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(r.stack[0], 4))
}
