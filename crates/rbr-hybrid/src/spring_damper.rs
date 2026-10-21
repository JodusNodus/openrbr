//! Bindings of the spring-damper ports (`rbr_physics::spring_damper`, exe 0x52dd70-0x52e8a0). The ABIs are the
//! original's (`harness/vector_specs/spring_damper.py`): the leaves are cdecl, the `CSpringDamper` methods thiscall.
//! eax is the residue the original leaves where it is an argument or `this` field; elsewhere it is a scratch value
//! no caller reads, given as 0.

use rbr_physics::spring_damper::{self as sd, layout};

use crate::engine_driveline::SIGNALS;
use crate::{Binding, Ctx, Decline, Regs, Return};

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "spring_damper", run }
}

/// ResetOutputs' call of `CDamperSignal::Emit` (0x52de60, thiscall `(payload*)`, the signal manager's dispatch to the
/// listeners), which NGP retargets (to 0x100edb20).
const EMIT_SITE: u32 = 0x52E024;

pub static BINDINGS: [Binding; 22] = [
    bind(0x52dd70, "CSpringDamper::UpdateForces", update_forces),
    bind(0x52e010, "CSpringDamper::ResetOutputs", reset_outputs),
    bind(0x52e060, "CSpringDamper::UpdateAndReset", update_and_reset),
    bind(0x52e090, "CSpringDamper::Step", step),
    bind(0x52e350, "DamperStep1", heat_flow),
    bind(0x52e370, "DamperStep2", conduction),
    bind(0x52e390, "DamperBiasSub", kelvin_to_celsius),
    bind(0x52e3a0, "DamperAccumA", accumulate_heat),
    bind(0x52e3d0, "DamperAccumB", accumulate_heat_scaled),
    bind(0x52e400, "DamperClamp", clamp),
    bind(0x52e460, "DamperMerge", merge),
    bind(0x52e4c0, "DamperUpdateA", solve_response),
    bind(0x52e4f0, "DamperUpdateB", solve_scale),
    bind(0x52e530, "DamperUpdateC", solve_numerator),
    bind(0x52e590, "DamperScale", scale),
    bind(0x52e5a0, "DamperSqrt4", sqrt4),
    bind(0x52e5e0, "DamperCurve", curve),
    bind(0x52e630, "DamperCombine", combine),
    bind(0x52e6d0, "DamperRelax", relax),
    bind(0x52e760, "DamperSolve", solve),
    bind(0x52e8a0, "DamperForce", force),
    bind(0x52dbd0, "CSpringDamper::CopyStateToDef", copy_state_to_def),
];

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// thiscall `()`; eax is the state pointer (`this+8`).
fn update_forces(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::update_forces(&mut ctx.mem, r.ecx);
    Ok(Return::void(ctx.mem.u32(r.ecx + 8), 0))
}

/// thiscall `()`: emits the payload at `this+0x1c` through the original's signal manager, then resets it. eax is
/// the state pointer.
fn reset_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    // The signal manager's damper signal.
    let signal = ctx.mem.u32(SIGNALS) + 0xc2b8;
    let target = crate::ngp::call_target(&ctx.mem, EMIT_SITE);
    ctx.host.call(target, signal, &[r.ecx + 0x1c]);
    layout::reset_outputs(&mut ctx.mem, r.ecx);
    Ok(Return::void(ctx.mem.u32(r.ecx + 8), 0))
}

/// thiscall `()`: `UpdateForces`, then `ResetOutputs`' reset without the emit. The wheel's setup update
/// (`E51Wheel::M3D030`) tail-jumps here. eax is the owner wheel's id.
fn update_and_reset(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::update_forces(&mut ctx.mem, r.ecx);
    layout::reset_outputs(&mut ctx.mem, r.ecx);
    Ok(Return::void(ctx.mem.u32(r.ecx + 0x1c), 0))
}

/// thiscall `(travel, coordinate, velocity, spring_force*, breakpoint*, damping*)`.
fn step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [travel, coordinate, velocity, spring_force, breakpoint, damping, ..] = r.stack;
    let args = [r.ecx, travel, coordinate, velocity, spring_force, breakpoint, damping];
    let ngp = crate::ngp::damper_step(&ctx.mem)?;
    layout::step(&mut ctx.mem, &args, ngp);
    Ok(Return::void(0, 0x18))
}

fn heat_flow(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, ..] = r.stack;
    Ok(Return::float(sd::heat_flow(f(a), f(b), f(c), f(d)), 0))
}

fn conduction(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    Ok(Return::float(sd::conduction(f(a), f(b), f(c)), 0))
}

fn kelvin_to_celsius(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(sd::kelvin_to_celsius(f(r.stack[0])), 0))
}

fn accumulate_heat(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [factors, input, total, ..] = r.stack;
    layout::accumulate_heat(&mut ctx.mem, factors, f(input), total);
    Ok(Return::void(total, 0))
}

fn accumulate_heat_scaled(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [source, input, accumulator, ..] = r.stack;
    layout::accumulate_heat_scaled(&mut ctx.mem, source, f(input), accumulator);
    Ok(Return::void(accumulator, 0))
}

fn clamp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [parameters, target, ..] = r.stack;
    let st0 = layout::clamp_value(&mut ctx.mem, parameters, target);
    Ok(Return { eax: target, st0: Some(st0 as f64), pop: 0 })
}

fn merge(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [source, target, ..] = r.stack;
    layout::merge_value(&mut ctx.mem, source, target);
    Ok(Return::void(target, 0))
}

fn solve_response(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::solve_response(&mut ctx.mem, r.stack[0], f(r.stack[1]));
    Ok(Return::void(0, 0))
}

fn solve_scale(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::solve_scale(&mut ctx.mem, r.stack[0], f(r.stack[1]));
    Ok(Return::void(0, 0))
}

fn solve_numerator(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::solve_numerator(&mut ctx.mem, r.stack[0], f(r.stack[1]));
    Ok(Return::void(0, 0))
}

fn scale(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(sd::scale(f(r.stack[0])), 0))
}

fn sqrt4(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(sd::sqrt4(f(r.stack[0])), 0))
}

fn curve(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(sd::curve(f(r.stack[0])), 0))
}

fn combine(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [target, source, input, ..] = r.stack;
    layout::combine(&mut ctx.mem, target, source, f(input));
    Ok(Return::void(target, 0))
}

fn relax(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [state, amount, divisor, ..] = r.stack;
    layout::relax(&mut ctx.mem, state, f(amount), f(divisor));
    Ok(Return::void(state, 0))
}

fn solve(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [state, input, limit, divisor, ..] = r.stack;
    let st0 = layout::solve(&mut ctx.mem, state, f(input), f(limit), f(divisor));
    Ok(Return { eax: state, st0: Some(st0 as f64), pop: 0 })
}

fn force(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [state, input, divisor, velocity, ..] = r.stack;
    layout::force(&mut ctx.mem, state, f(input), f(divisor), f(velocity));
    Ok(Return::void(0, 0))
}

/// thiscall `(setup)`, `ret 4`: `CSpringDamper::CopyStateToDef` (0x52dbd0), the setup into the strut's definition
/// (`layout::apply_setup`).
fn copy_state_to_def(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::apply_setup(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}
