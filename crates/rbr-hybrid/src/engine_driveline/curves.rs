//! The `E39_*` row and table helpers, `E12_SampleX` and the drive line's lerps and blends.

use rbr_physics::engine_driveline::{drive_curve, drive_line, drive_worker, layout, sim};
use rbr_physics::mem::Mem;

use super::{f, E12_SAMPLE_X};
use crate::ngp::variant;
use crate::{Ctx, Decline, Regs, Return};
use rbr_physics::engine_driveline::store::EngineStore;

/// stdcall `(v, p)`, st0: the ring at `p` takes `v`, and the mean is returned.
pub(super) fn ring_average(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let p = r.stack[1];
    let mut ring = layout::RatioRing::load(&ctx.mem, p);
    let mean = drive_worker::ring_average(&mut ring, f(r.stack[0]));
    ring.store(&mut ctx.mem, p);
    Ok(Return::float(mean as f64, 8))
}

/// stdcall `(table, a, b, c, n)`, st0: the table lerp `E39_TabE00` (ten knots at `table`).
pub(super) fn unsigned_table_lerp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let table = ctx.mem.f32_table(r.stack[0], 10);
    let v = drive_curve::unsigned_table_lerp(
        &table,
        r.stack[1] as i32,
        r.stack[2] as i32,
        r.stack[3] as i32,
        r.stack[4] as i32,
        variant(),
    );
    Ok(Return::float(v, 0x14))
}

/// stdcall `(dt, a, b, c, n, s)`, eax: `E39_WorkerF60`, the row `a` and the state `b`; eax is 1 when the counter counted.
pub(super) fn count_down(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let row = layout::CurveRow::load(&ctx.mem, r.stack[1]);
    let tables = layout::CurveTables::load(&ctx.mem, r.stack[4]);
    let block_gain = ctx.mem.curve_gain(r.stack[3]);
    let mut state = layout::CurveState::load(&ctx.mem, r.stack[2]);
    let ok = drive_curve::count_down(f(r.stack[0]), &row, &mut state, &tables, block_gain, f(r.stack[5]), variant());
    state.store(&mut ctx.mem, r.stack[2]);
    Ok(Return::void(u32::from(ok), 0x18))
}

/// `(a1, c, n, x, s)` with `this` in ecx (stdcall, ret 0x14), st0: `E39_Hook4160`, the car's speed term. The gate is the
/// drive object's word at `this+0x170`, the row is the block's index in the def `n`.
pub(super) fn wheel_curve_term(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let (a1, c, n, x, s) = (r.stack[0], r.stack[1], r.stack[2], r.stack[3], f(r.stack[4]));
    let gate = ctx.mem.obj_gate(r.ecx);
    let block = layout::HookBlock::load(&ctx.mem, c);
    let row = layout::CurveRow::load(&ctx.mem, layout::curve_row_at(n, block.index));
    let tables = layout::CurveTables::load(&ctx.mem, n);
    let input = layout::CurveInput::load(&ctx.mem, x);
    let mut state = layout::CurveState::load(&ctx.mem, a1);
    let aval = drive_curve::wheel_curve_term(gate, &block, &row, &tables, &mut state, &input, s, variant());
    state.store(&mut ctx.mem, a1);
    Ok(Return::float(aval as f64, 0x14))
}

/// stdcall `(dt, b, c, n, s)`, st0: `E39_Worker4060`, the row the block `c` picks in the def `n`.
pub(super) fn row_value(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dt, b, c, n, s, ..] = r.stack;
    let v = row_value_at(&mut ctx.mem, f(dt), b, c, n, f(s));
    Ok(Return::float(v, 0x14))
}

/// `E39_Worker4060` on the state `b`, the block `c` and the def `n`.
pub(crate) fn row_value_at(mem: &mut Mem, dt: f32, b: u32, c: u32, n: u32, s: f32) -> f64 {
    let row = layout::CurveRow::load(mem, layout::curve_row_at(n, mem.curve_index(c)));
    let tables = layout::CurveTables::load(mem, n);
    let block_gain = mem.curve_gain(c);
    let mut state = layout::CurveState::load(mem, b);
    let v = drive_curve::row_value(dt, &row, &mut state, &tables, block_gain, s, variant());
    state.store(mem, b);
    v
}

/// stdcall `(x)`, `ret 4`, st0: `E12_SampleX` (0x475ea0), the drive line's value at `x`. The original asserts outside the
/// table, so the port declines there (before anything is written).
pub(super) fn e12_sample_x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    match layout::sample_x(&ctx.mem, f32::from_bits(r.stack[0])) {
        Some(st0) => Ok(Return::float(st0 as f64, 4)),
        None => Err(Decline::Unsupported("E12_SampleX outside the table (the original asserts)")),
    }
}

/// thiscall `(x)`, st0: `E12_SampleX` on the E12 host word.
pub(super) fn thunk_4f0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let host = ctx.host;
    let st0 =
        drive_line::thunk_4f0(&ctx.mem, r.stack[0], |e12, x| host.call(E12_SAMPLE_X, e12, &[x.to_bits()]).st0 as f32);
    Ok(Return::float(st0 as f64, 4))
}

/// thiscall `(idx, t, out*)`.
pub(super) fn blend_b(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [idx, t, out, ..] = r.stack;
    let v = drive_line::blend_b(&ctx.mem, r.ecx, idx as i32, f(t));
    v.store(&mut ctx.mem, out);
    Ok(Return::void(0, 0xc))
}

/// thiscall `(i, t, out)`, `ret 0xc`: `CDriveLine::LerpAt` (0x50a920), which jumps into `E11_VecLerp`. Eax is `out`.
pub(super) fn lerp_at(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [idx, t, out, ..] = r.stack;
    if idx >= ctx.mem.u32(r.ecx + drive_line::COUNT_AT) {
        return Err(Decline::Unsupported("drive line index past its count (LerpAt traps)"));
    }
    let v = ctx.mem.lerp_at(r.ecx, idx as i32, f(t));
    v.store(&mut ctx.mem, out);
    Ok(Return::void(out, 0xc))
}

/// thiscall `()`, st0.
pub(super) fn last(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(drive_line::last(&ctx.mem, r.ecx) as f64, 0))
}

/// thiscall `(signal*)`. Declines a count over 10000 (the original traps).
pub(super) fn set_drive_line(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    if ctx.mem.u32(r.stack[0]) > 10000 {
        return Err(Decline::Unsupported("drive line count over 10000"));
    }
    drive_line::set_drive_line(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(x, out*)`.
pub(super) fn clamp_mix(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [x, out, ..] = r.stack;
    let v = drive_line::clamp_mix(&ctx.mem, r.ecx, f(x));
    v.store(&mut ctx.mem, out);
    Ok(Return::void(0, 8))
}

// The E41 helpers (stdcall).

/// `(a, b_desc*, c_desc*, d, e, out_f*, out_g*, out_h*)`.
pub(super) fn blend_x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b_desc, c_desc, d, e, out_f, out_g, out_h, ..] = r.stack;
    let mem = &mut ctx.mem;
    let count = mem.u32(b_desc + 4) as usize;
    let b = mem.blend_records(b_desc, count);
    let c = mem.blend_records(c_desc, count);
    let (vf, vg, vh) = sim::blend_x(f(a), f(d), f(e), &b, &c);
    mem.set_f32(out_f, vf);
    mem.set_f32(out_g, vg);
    mem.set_u32(out_h, vh);
    Ok(Return::void(0, 0x20))
}

/// `(a, b_desc*, c_desc*, d, e, out_f*, out_g*)`.
pub(super) fn blend_y(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b_desc, c_desc, d, e, out_f, out_g, ..] = r.stack;
    let mem = &mut ctx.mem;
    let count = mem.u32(b_desc + 4) as usize;
    let b = mem.blend_records(b_desc, count);
    let c = mem.blend_records(c_desc, count);
    let (vf, vg) = sim::blend_y(f(a), f(d), f(e), &b, &c);
    mem.set_f32(out_f, vf);
    mem.set_f32(out_g, vg);
    Ok(Return::void(0, 0x1c))
}

/// `(keys*, x, out0*, out1*, out2*, out3*)`.
pub(super) fn blend_w(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [keys, x, o0, o1, o2, o3, ..] = r.stack;
    let keys = ctx.mem.bracket_keys(keys);
    let b = sim::blend_w(&keys, f(x));
    ctx.mem.store_bracket(&b, [o0, o1, o2, o3]);
    Ok(Return::void(0, 0x18))
}

/// `(table*, x, lo, hi, n)`, st0.
pub(super) fn table_lerp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [table, x, lo, hi, n, ..] = r.stack;
    let table = ctx.mem.f32_table(table, n as i32 as usize);
    Ok(Return::float(sim::table_lerp(&table, f(x), f(lo), f(hi), variant()), 0x14))
}

/// `(x, out*)`.
pub(super) fn unk_d6f0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    sim::unk_d6f0(f(r.stack[0])).store(&mut ctx.mem, r.stack[1]);
    Ok(Return::void(0, 8))
}

/// `(curve*, x, n)`, st0.
pub(super) fn curve_eta(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [curve, x, n, ..] = r.stack;
    let curve = ctx.mem.curve_knots(curve, n as i32 as usize);
    Ok(Return::float(sim::curve_eta(&curve, f(x), variant()), 0xc))
}

// CEngine.
