//! Bindings of the tyre ports (`rbr_physics::tyre`, exe 0x528980 and 0x531ba0-0x535d60). The ABIs are the
//! original's (`harness/vcu_vectors.py` TYRE specs): the `CScratch` methods are thiscall, `TyreClamp`, `FloorS32`
//! and `TyreFrictionCurve` stdcall, `Locate` cdecl. Their callees are vector and CRT helpers the ports compute
//! (they write only the caller's stack), so no binding calls the host. eax is given where the original returns
//! a value; elsewhere it is a scratch value no caller reads, given as 0.
//!
//! Each binding decodes the registers and runs its entry's sequence (`rbr_physics::tyre::layout::entries`). A frame
//! entry that declines with `Outside` leaves memory as it was, so the original runs.

use rbr_physics::physics_core::signal;
use rbr_physics::tyre::layout::{self, entries};
use rbr_physics::tyre::store::TyreStore;
use rbr_physics::tyre::{grip, solve};

use crate::{Binding, Ctx, Decline, Regs, Return, SignalCallees};

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "tyre", run }
}

pub static BINDINGS: [Binding; 45] = [
    bind(0x528980, "Locate", locate),
    bind(0x531ba0, "CScratch::Sign", sign),
    bind(0x531c70, "CScratch::Friction", friction),
    bind(0x531fc0, "CScratch::Align", align),
    bind(0x5320c0, "CScratch::ComputeOutputs", compute_outputs),
    bind(0x5322e0, "TyreClamp", tyre_clamp),
    bind(0x532470, "CScratch::SetStateField", set_state_field),
    bind(0x532490, "CScratch::Set414", set_414),
    bind(0x533490, "CScratch::SetState48", set_state48),
    bind(0x534e70, "CScratch::BlendTable", blend_table),
    bind(0x534f80, "CMultiSignal1<CSigTyreNoise>::Emit", signal_emit::<0x534f80>),
    bind(0x535890, "CScratch::EmitNoise", emit_noise),
    bind(0x5358c0, "CScratch::RelaxCount", relax_count),
    bind(0x52db10, "NTweak::CBase::GetName", name_object),
    bind(0x5326b0, "TyreTail_Get10", tyre_tail10),
    bind(0x5326c0, "TyreTail_Get14", tyre_tail14),
    bind(0x5325f0, "FloorS32", floor_s32),
    bind(0x532a30, "CScratch::ComputeSlip", compute_slip),
    bind(0x532b60, "CScratch::SolveForces", solve_forces),
    bind(0x532f10, "CScratch::FillOutputs", fill_outputs),
    bind(0x533080, "CScratch::WriteOutputs", write_outputs),
    bind(0x5334d0, "TyreFrictionCurve", friction_curve),
    bind(0x533640, "CScratch::SolveDirs", solve_dirs),
    bind(0x533760, "CScratch::ClampGrip", clamp_grip),
    bind(0x5338a0, "CScratch::Accumulate", accumulate),
    bind(0x533ae0, "CScratch::SolveA", solve_a),
    bind(0x533bf0, "CScratch::SolveB", solve_b),
    bind(0x533d40, "CScratch::SolveC", solve_c),
    bind(0x533e80, "CScratch::SolveD", solve_d),
    bind(0x534090, "CScratch::HeatStep", heat_step),
    bind(0x534480, "CScratch::TableStep", table_step),
    bind(0x5345a0, "CScratch::RelaxStep", relax_step),
    bind(0x5346f0, "CScratch::SplitSlip", split_slip),
    bind(0x535040, "CTyreMatData::CopyFrom", copy_from),
    bind(0x5351a0, "CScratch::SolveDispatch", solve_dispatch),
    bind(0x5352a0, "CScratch::SolvePair", solve_pair),
    bind(0x535310, "CScratch::StepInner", step_inner),
    bind(0x5358f0, "CScratch::ContactPatch", contact_patch),
    bind(0x535d60, "CScratch::SolvePatch", solve_patch),
    bind(0x5360c0, "CScratch::Step", step),
    bind(0x530c50, "CDef::CDef", cdef_ctor),
    bind(0x531500, "CState::CState", state_ctor),
    bind(0x5327d0, "CScratch::Init", scratch_init),
    bind(0x5328d0, "CScratch::CScratch", scratch_ctor),
    bind(0x534760, "CScratch::CopyDefFrom", copy_def_from),
];

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// A frame port's `Outside` declines, so the original runs on memory as it was.
fn frame<T>(r: Result<T, layout::Outside>) -> Result<T, Decline> {
    r.map_err(|_| Decline::Unsupported("index outside the frame"))
}

/// cdecl `(table, x, n, out)`: the segment at `out` is the hint, and the result is stored back there.
fn locate(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [table, x, n, out, ..] = r.stack;
    entries::locate(&mut ctx.mem, table, f(x), n, out).map_err(|_| Decline::Unsupported("locate outside the table"))?;
    Ok(Return::void(0, 0))
}

/// thiscall `()`, st0.
fn sign(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(entries::sign(&ctx.mem, r.ecx), 0))
}

/// thiscall `(k)`, st0.
fn friction(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(entries::friction(&mut ctx.mem, r.ecx, f(r.stack[0])), 4))
}

/// thiscall `(a*, b*)`; eax is 1 when it aligned.
fn align(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let aligned = frame(entries::align(&mut ctx.mem, r.ecx, r.stack[0], r.stack[1]))?;
    Ok(Return::void(aligned as u32, 8))
}

/// thiscall with 17 stack words: scales at 1, 2, 4 and 5, `src` at 6, the output pointers at 7 to 16.
fn compute_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    entries::compute_outputs(&mut ctx.mem, r.ecx, &r.stack);
    Ok(Return::void(0, 0x44))
}

/// stdcall `(a, b, c, out*)`.
fn tyre_clamp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, out, ..] = r.stack;
    entries::tyre_clamp(&mut ctx.mem, f(a), f(b), f(c), out);
    Ok(Return::void(0, 0x10))
}

/// thiscall `(value)`.
fn set_414(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ctx.mem.store_414(r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// stdcall `(x, table, count)`, st0.
fn floor_s32(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [x, table, count, ..] = r.stack;
    Ok(Return::float(entries::floor_s32(&ctx.mem, f(x), table, count), 0xc))
}

/// thiscall `(dir*, spin)`.
fn compute_slip(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dir, spin, ..] = r.stack;
    entries::compute_slip(&mut ctx.mem, r.ecx, dir, f(spin));
    Ok(Return::void(0, 8))
}

/// thiscall `(k, out1*, out2*)`: `out2` is read (the asm adds into it), and both are written.
fn solve_forces(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [k, out1, out2, ..] = r.stack;
    frame(entries::solve_forces(&mut ctx.mem, r.ecx, f(k), out1, out2))?;
    Ok(Return::void(0, 0xc))
}

/// thiscall `(src*, pair*, divisor)`.
fn fill_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, pair, divisor, ..] = r.stack;
    frame(entries::fill_outputs(&mut ctx.mem, r.ecx, src, pair, f(divisor)))?;
    Ok(Return::void(0, 0xc))
}

/// thiscall `(out*)`.
fn write_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame(entries::write_outputs(&mut ctx.mem, r.ecx, r.stack[0]))?;
    Ok(Return::void(0, 4))
}

/// stdcall with 13 words: values 0 to 8, out pointers 9 and 10, the table and its count; st0.
fn friction_curve(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(entries::friction_curve(&mut ctx.mem, &r.stack), 0x34))
}

/// thiscall `(a, b, c, out0*, out1*, out2*)`.
fn solve_dirs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let s = &r.stack;
    entries::solve_dirs(&mut ctx.mem, r.ecx, [f(s[0]), f(s[1]), f(s[2])], [s[3], s[4], s[5]]);
    Ok(Return::void(0, 0x18))
}

/// thiscall `(s, a, b, c, d, out_a*, out_b*)`.
fn clamp_grip(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [s, a, b, c, d, out_a, out_b, ..] = r.stack;
    let g = grip::GripInput {
        normal_force: f(s),
        static_friction: f(a),
        kinetic_friction: f(b),
        lateral_stiffness: f(c),
        torsional_stiffness: f(d),
    };
    frame(entries::clamp_grip(&mut ctx.mem, r.ecx, g, out_a, out_b))?;
    Ok(Return::void(0, 0x1c))
}

/// thiscall `()`.
fn accumulate(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    entries::accumulate(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// The contact solvers: thiscall `(p*, qb*, r*, ...)`, the contact loaded from and stored back to the three blocks.
fn with_contact(ctx: &mut Ctx, r: &Regs, run: impl FnOnce(&mut solve::Contact)) {
    let [p, qb, rr, ..] = r.stack;
    entries::with_contact(&mut ctx.mem, r.ecx, p, qb, rr, run);
}

/// thiscall `(p*, qb*, r*, idx)`.
fn solve_a(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_contact(ctx, r, |c| solve::solve_tread_only(c, r.stack[3] as i32));
    Ok(Return::void(0, 0x10))
}

fn solve_b(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_contact(ctx, r, |c| solve::solve_both(c, r.stack[3] as i32));
    Ok(Return::void(0, 0x10))
}

fn solve_c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_contact(ctx, r, |c| solve::solve_neither(c, r.stack[3] as i32));
    Ok(Return::void(0, 0x10))
}

fn solve_d(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    with_contact(ctx, r, |c| solve::solve_carcass_only(c, r.stack[3] as i32));
    Ok(Return::void(0, 0x10))
}

/// thiscall `(p*, qb*, r*, f1, f2, a6)`.
fn solve_dispatch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [_, _, _, f1, f2, a6, ..] = r.stack;
    with_contact(ctx, r, |c| solve::solve_contact(c, f1 != 0, f2 != 0, a6 as i32));
    Ok(Return::void(0, 0x18))
}

/// thiscall `(a, b, c)`.
fn heat_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let ngp = crate::ngp::thermal(ctx, r.ecx)?;
    entries::heat_step(&mut ctx.mem, r.ecx, f(a), f(b), f(c), &ngp);
    Ok(Return::void(0, 0xc))
}

/// thiscall `(idx, x)`.
fn table_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [idx, x, ..] = r.stack;
    let ngp = crate::ngp::thermal(ctx, r.ecx)?;
    entries::table_step(&mut ctx.mem, r.ecx, idx, f(x), &ngp);
    Ok(Return::void(0, 8))
}

/// thiscall `(flag)`.
fn set_state_field(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame(entries::set_state_field(&mut ctx.mem, r.ecx, r.stack[0]))?;
    Ok(Return::void(0, 4))
}

/// thiscall `(v)`.
fn set_state48(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame(entries::set_state48(&mut ctx.mem, r.ecx, f(r.stack[0])))?;
    Ok(Return::void(0, 4))
}

/// thiscall `(i, j, a, b, o1, o2)`, eax 1.
fn blend_table(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [i, j, a, b, o1, o2, ..] = r.stack;
    entries::blend_table(&mut ctx.mem, r.ecx, i, j, [f(a), f(b)], o1, o2);
    Ok(Return::void(1, 0x18))
}

/// thiscall `(dt)`: RelaxStep, then the counter at state+0x1c.
fn relax_count(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let ngp = crate::ngp::thermal(ctx, r.ecx)?;
    entries::relax_count(&mut ctx.mem, r.ecx, f(r.stack[0]), &ngp);
    Ok(Return::void(0, 4))
}

/// thiscall `()`: the name object (eax).
fn name_object(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.load_name_object(r.ecx), 0))
}

/// thiscall `()`: the tail word at +0x10 (eax).
fn tyre_tail10(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.load_tyre_tail10(r.ecx), 0))
}

/// thiscall `()`: the tail word at +0x14 (eax).
fn tyre_tail14(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.load_tyre_tail14(r.ecx), 0))
}

/// thiscall `(dt)`.
fn relax_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let ngp = crate::ngp::thermal(ctx, r.ecx)?;
    entries::relax_step(&mut ctx.mem, r.ecx, f(r.stack[0]), &ngp);
    Ok(Return::void(0, 4))
}

/// thiscall `()`.
fn split_slip(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame(entries::split_slip(&mut ctx.mem, r.ecx))?;
    Ok(Return::void(0, 0))
}

/// thiscall `(src*)`.
fn copy_from(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    entries::copy_from(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `()`.
fn solve_pair(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame(entries::solve_pair(&mut ctx.mem, r.ecx))?;
    Ok(Return::void(0, 0))
}

/// thiscall `(rec*, vec3*, vec4*, out3*, out4*, out5*)`.
fn step_inner(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let s = &r.stack;
    // The model reads the scratch's own words, which are the record's when the record is the scratch's (at +0x20).
    if s[0] != r.ecx + 0x20 {
        return Err(Decline::Unsupported("record is not the scratch's"));
    }
    entries::step_inner(&mut ctx.mem, r.ecx, std::array::from_fn(|i| s[1 + i]));
    Ok(Return::void(0, 0x18))
}

/// thiscall `(out*)`; eax is 1 when solved, and the value the port returns is stored at `out`.
fn contact_patch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let solved = frame(entries::contact_patch(&mut ctx.mem, r.ecx, r.stack[0]))?;
    Ok(Return::void(solved as u32, 4))
}

/// thiscall `()`.
fn solve_patch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    frame(entries::solve_patch(&mut ctx.mem, r.ecx))?;
    Ok(Return::void(0, 0))
}

/// thiscall with 16 stack words (`a` to `p`), `ret 0x40`. The arguments are loaded once, `Step` runs on the frame,
/// and its outputs are stored after the frame.
pub(crate) fn step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let ngp = crate::ngp::thermal(ctx, r.ecx)?;
    frame(entries::step(&mut ctx.mem, r.ecx, &r.stack, &ngp))?;
    Ok(Return::void(0, 0x40))
}

/// thiscall `()`: the CDef's defaults, the object in eax.
fn cdef_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    layout::construct_cdef(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.ecx, 0))
}

/// thiscall `(a, b, c)`: the state's defaults, the object in eax; the three words are not stored.
fn state_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::construct_state(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.ecx, 0xC))
}

/// thiscall `()`: the scratch's sub-block defaults; the original leaves `this` in eax.
fn scratch_init(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::init_scratch(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.ecx, 0))
}

/// thiscall `(def, state, record)`: the scratch's constructor, the object in eax.
fn scratch_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [def, state, record, ..] = r.stack;
    layout::construct_scratch(&mut ctx.mem, r.ecx, def, state, record);
    Ok(Return::void(r.ecx, 0xC))
}

/// thiscall `(src)`: the setup copied into the def, the state's copies, and `DamperUpdateB` (in the port).
fn copy_def_from(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::copy_def_from(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(sig)`: `CMultiSignal1<CSigTyreNoise>::Emit` and the other instances of the template (the tire-particle one),
/// the physics core's emit with the connections in the emulator; `EMIT` is the instance. A count out of range
/// declines (the original traps).
pub(crate) fn signal_emit<const EMIT: u32>(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, r.ecx)?;
    signal::emit(&mut ctx.mem, &mut SignalCallees(ctx.host, crate::emit_name(EMIT)), r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall: `CScratch::EmitNoise`, the tyre-noise signal emitted with the scratch's signal block. Declines like
/// `signal_emit` when the emit would trap. Under NGP the emit is NGP's wrapper (`ngp::original_for`), which the host
/// runs; eax is what it leaves.
fn emit_noise(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, ctx.mem.noise_signal())?;
    let target = crate::ngp::call_target(&ctx.mem, 0x5358A6);
    if target != 0x534F80 {
        // NGP's emit wrapper (0x100ee970): the host runs it, as `layout::emit_noise` runs the emit.
        let signal = ctx.mem.noise_signal();
        let called = ctx.host.call(target, signal, &[r.ecx + 0x420]);
        ctx.mem.set_u32(r.ecx + 0x424, 0);
        return Ok(Return::void(called.eax, 0));
    }
    layout::emit_noise(&mut ctx.mem, &mut SignalCallees(ctx.host, crate::emit_name(0x534f80)), r.ecx);
    Ok(Return::void(0, 0))
}
