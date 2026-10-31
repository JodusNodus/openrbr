//! Bindings of the spring-damper ports (`rbr_physics::spring_damper`, exe 0x52dd70-0x52e8a0). The ABIs are the
//! original's (`harness/vector_specs/spring_damper.py`): the leaves are cdecl, the `CSpringDamper` methods thiscall.
//! eax is the residue the original leaves where it is an argument or `this` field; elsewhere it is a scratch value
//! no caller reads, given as 0.

use rbr_physics::spring_damper::{self as sd, layout};

use crate::engine_driveline::SIGNALS;
use crate::{Binding, Ctx, Decline, Regs, Return};

pub static BINDINGS: [Binding; 22] = [
    crate::bind("spring_damper", 0x52dd70, "CSpringDamper::UpdateForces", update_forces),
    crate::bind("spring_damper", 0x52e010, "CSpringDamper::ResetOutputs", reset_outputs),
    crate::bind("spring_damper", 0x52e060, "CSpringDamper::UpdateAndReset", update_and_reset),
    crate::bind("spring_damper", 0x52e090, "CSpringDamper::Step", step),
    crate::bind("spring_damper", 0x52e350, "DamperStep1", heat_flow),
    crate::bind("spring_damper", 0x52e370, "DamperStep2", conduction),
    crate::bind("spring_damper", 0x52e390, "DamperBiasSub", kelvin_to_celsius),
    crate::bind("spring_damper", 0x52e3a0, "DamperAccumA", accumulate_heat),
    crate::bind("spring_damper", 0x52e3d0, "DamperAccumB", accumulate_heat_scaled),
    crate::bind("spring_damper", 0x52e400, "DamperClamp", clamp),
    crate::bind("spring_damper", 0x52e460, "DamperMerge", merge),
    crate::bind("spring_damper", 0x52e4c0, "DamperUpdateA", solve_response),
    crate::bind("spring_damper", 0x52e4f0, "DamperUpdateB", solve_scale),
    crate::bind("spring_damper", 0x52e530, "DamperUpdateC", solve_numerator),
    crate::bind("spring_damper", 0x52e590, "DamperScale", scale),
    crate::bind("spring_damper", 0x52e5a0, "DamperSqrt4", sqrt4),
    crate::bind("spring_damper", 0x52e5e0, "DamperCurve", curve),
    crate::bind("spring_damper", 0x52e630, "DamperCombine", combine),
    crate::bind("spring_damper", 0x52e6d0, "DamperRelax", relax),
    crate::bind("spring_damper", 0x52e760, "DamperSolve", solve),
    crate::bind("spring_damper", 0x52e8a0, "DamperForce", force),
    crate::bind("spring_damper", 0x52dbd0, "CSpringDamper::CopyStateToDef", copy_state_to_def),
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
/// the state pointer. The payload is also handed to the host as a typed damper event for the game.
fn reset_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    // The signal manager's damper signal.
    let signal = ctx.mem.u32(SIGNALS) + 0xc2b8;
    let target = crate::ngp::Call::DamperSignal.address();
    ctx.host.call(target, signal, &[r.ecx + 0x1c]);
    let emitted = layout::reset_outputs(&mut ctx.mem, r.ecx);
    ctx.host.emit_damper(r.ecx, emitted);
    Ok(Return::void(ctx.mem.u32(r.ecx + 8), 0))
}

/// thiscall `()`: `UpdateForces`, then `ResetOutputs`' reset without the emit. The wheel's setup update
/// (`E51Wheel::M3D030`) tail-jumps here. eax is the owner wheel's id.
fn update_and_reset(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::update_forces(&mut ctx.mem, r.ecx);
    let _ = layout::reset_outputs(&mut ctx.mem, r.ecx);
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

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_physics::mem::Mem;
    use rbr_physics::spring_damper::DamperSignal;
    use std::cell::RefCell;

    /// A host that logs its calls and keeps the typed damper events.
    #[derive(Default)]
    struct Sink {
        calls: RefCell<Vec<(u32, u32, Vec<u32>)>>,
        dampers: RefCell<Vec<(u32, DamperSignal)>>,
    }

    impl crate::Host for Sink {
        fn call_at(&self, _sp: u32, addr: u32, ecx: u32, args: &[u32]) -> crate::Called {
            self.calls.borrow_mut().push((addr, ecx, args.to_vec()));
            crate::Called { eax: 0, st0: 0.0 }
        }

        fn emit_damper(&self, strut: u32, signal: DamperSignal) {
            self.dampers.borrow_mut().push((strut, signal));
        }
    }

    #[test]
    fn reset_outputs_emits_the_typed_payload_and_resets() {
        let host = Sink::default();
        let mut mem = Mem::new();
        let (def, state, fade, strut, wheel) = (0x10000, 0x11000, 0x12000, 0x13000, 0x14000);
        mem.map(def, &[0; 0x200]);
        mem.map(state, &[0; 0x100]);
        mem.map(fade, &[0; 8]);
        mem.map(strut, &[0; 0x40]);
        mem.map(wheel, &[0; 0x1000]);
        mem.map(SIGNALS, &[0; 4]);
        layout::construct_definition(&mut mem, def);
        layout::construct_state(&mut mem, state, wheel);
        layout::construct_controller(&mut mem, strut, def, state);
        mem.set_u32(SIGNALS, 0x20000);
        mem.set_u32(wheel + 0xeb4, 0x1234);
        let old =
            DamperSignal { owner_id: 0x77, peak_velocity: 1.5, bump_velocity: 0.25, droop_velocity: -0.5, valid: 1 };
        mem.set_u32(strut + 0x1c, old.owner_id);
        mem.set_f32(strut + 0x20, old.peak_velocity);
        mem.set_f32(strut + 0x24, old.bump_velocity);
        mem.set_f32(strut + 0x28, old.droop_velocity);
        mem.set_u32(strut + 0x2c, old.valid as u32);
        let mut ctx = Ctx { mem, host: &host };
        let ret = reset_outputs(&mut ctx, &Regs { ecx: strut, ..Regs::default() }).unwrap();
        assert_eq!(ret.eax, state);
        // The typed event carries the pre-reset payload.
        assert_eq!(*host.dampers.borrow(), [(strut, old)]);
        // The signal-manager emit still runs for the emulator's fidelity.
        assert_eq!(*host.calls.borrow(), [(0x52de60, 0x20000 + 0xc2b8, vec![strut + 0x1c])]);
        // The words are reset to the new owner.
        assert_eq!(ctx.mem.u32(strut + 0x1c), 0x1234);
        assert_eq!(ctx.mem.f32(strut + 0x20), 0.0);
        assert_eq!(ctx.mem.f32(strut + 0x24), 0.0);
        assert_eq!(ctx.mem.f32(strut + 0x28), 0.0);
        assert_eq!(ctx.mem.u32(strut + 0x2c), 1);
    }
}
