//! NGP's damper and spring wrappers with its heat steps.

use rbr_physics::ngp::heat;
use rbr_physics::spring_damper::layout as damper_layout;
use rbr_physics::spring_damper::suspension::{self, Environment};
use rbr_physics::spring_damper::StepInput;

use crate::{Ctx, Decline, Regs, Return};

use super::address::at;
use super::tuning::{damper_step, thermal, tyre_shape};
use super::{data_f32, f, SETTINGS};

/// NGP's game object (`*(*10174ba0)`, `0x10011cb0`), whose `+0x69c` bit 1 enables the progressive bump stop with the
/// settings' bit `0x20000`.
const GAME: u32 = 0x1017_4ba0;

/// `0x10059f70` (thiscall on the strut, 1.02's six words, `ret 0x18`): NGP's wrapper of `CSpringDamper::Step`
/// (`rbr_physics::ngp::spring`) around the ported `Step` and `UpdateForces`. eax is the strut's `+8`.
pub(crate) fn spring_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let strut = r.ecx;
    let [a0, a1, a2, a3, a4, a5, ..] = r.stack;
    let m = &ctx.mem;
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    let game = m.u32(m.u32(at(GAME).unwrap()));
    if settings == 0 || game == 0 {
        return Err(Decline::Unsupported("NGP's settings or game object is not made yet"));
    }
    let obj = m.u32(strut + 8);
    let car = m.u32(obj + 0x18);
    let car_def = m.u32(car + 0x34);
    // The owned suspension runs the wrapper's logic; memory is only loaded and stored at the edge.
    let mut suspension = suspension::Suspension::load(m, strut);
    let environment = Environment {
        progressive: m.u32(settings + 0xe38) & 0x20000 != 0 && m.u32(game + 0x69c) & 1 != 0,
        gap_offset: m.f32(car_def + 0x41c),
        rebound_ratio: m.f32(car_def + 0x1fc),
    };
    let input = StepInput { travel: f(a0), coordinate: f(a1), velocity: f(a2) };
    let ngp = damper_step(m)?;
    let out = suspension.step(input, environment, ngp);
    let mem = &mut ctx.mem;
    mem.set_f32(strut + 0x30, out.diagnostics.bump_ratio);
    mem.set_f32(strut + 0x34, out.diagnostics.bump_stiffness);
    mem.set_f32(strut + 0x38, out.diagnostics.bump_damping);
    mem.set_u32(car + 0xe80, out.diagnostics.effective_damping[0].to_bits());
    mem.set_u32(car + 0xe84, out.diagnostics.effective_damping[1].to_bits());
    mem.set_u32(car + 0xe88, out.diagnostics.break_speeds[0].to_bits());
    mem.set_u32(car + 0xe8c, out.diagnostics.break_speeds[1].to_bits());
    mem.set_f32(a3, out.forces.spring_stop_force);
    mem.set_f32(a4, out.forces.breakpoint_offset);
    mem.set_f32(a5, out.forces.damping_coefficient);
    suspension.store_state(mem, strut);
    Ok(Return::void(mem.u32(strut + 8), 0x18))
}

/// `0x10060230` (cdecl: state, input, limit, divisor): 1.02's `DamperSolve` with the air pressure as the limit.
pub(crate) fn damper_solve(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [state, input, _, divisor, ..] = r.stack;
    let pressure = data_f32(ctx, 0x1017_1ad8);
    let st0 = damper_layout::solve(&mut ctx.mem, state, f(input), pressure, f(divisor));
    Ok(Return::float_with_eax(state, f64::from(st0), 0))
}

/// `0x1004d2f0` (cdecl: a, b, c, d): [`heat::heat_flow_to_air`].
pub(crate) fn damper_step1(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, ..] = r.stack;
    let air = data_f32(ctx, 0x1017_1adc);
    let st0 = heat::heat_flow_to_air(f(a), f(b), f(c), f(d), air);
    Ok(Return::float_with_eax(r.eax, f64::from(st0), 0))
}

/// The tread segment's area (`0x1004d340`) of the scratch in `esi`, and the word it leaves in eax (`*(scratch+0x10)`).
fn tread_segment_area(ctx: &Ctx, r: &Regs) -> (f32, u32) {
    let def = ctx.mem.u32(r.esi + 0x10);
    (tyre_shape(&ctx.mem, def).tread_segment_area(), def)
}

/// `0x1004d3c0` (cdecl: a, b, c; the scratch in esi): `DamperStep1` with the tread segment's area as d.
pub(crate) fn damper_step1_scratch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let (area, eax) = tread_segment_area(ctx, r);
    let air = data_f32(ctx, 0x1017_1adc);
    let st0 = heat::heat_flow_to_air(f(a), f(b), f(c), area, air);
    Ok(Return::float_with_eax(eax, f64::from(st0), 0))
}

/// `0x1004d380` (cdecl: a, b, unused c; the scratch in esi): [`heat::conduction`] through the tread segment.
pub(crate) fn damper_step2(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let (area, eax) = tread_segment_area(ctx, r);
    Ok(Return::float_with_eax(eax, heat::conduction(f(a), f(b), area), 0))
}

/// `0x1004d410` (cdecl `(a, b, c, _)`, the scratch in esi): `DamperStep1` with NGP's convection term. Its eax (the
/// CRT `pow`'s leftovers) isn't modelled.
pub(crate) fn convection(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let n = thermal(ctx, r.esi)?;
    let st0 = heat::sidewall_convection(f(a), f(b), f(c), &n.shape, &n.airflow, n.air_temperature);
    Ok(Return::float_with_eax(0, f64::from(st0), 0))
}
