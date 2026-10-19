//! The tyre functions' sequences, one per original entry, generic over the store: each loads what its port reads,
//! runs the port and stores what it wrote (moved from the bindings in `rbr-hybrid/src/tyre.rs`, which decode the
//! registers and call these). Pointer arguments are the caller's stack or blocks, read and written through `Raw`.
//!
//! The frame entries (`with_frame`) load the `CScratch` once, run on it, and store it once. An `Outside` leaves memory
//! as it was.

use super::frame_types::Frame;
use super::{Outside, StepArgs};
use crate::math::vec::Vec4;
use crate::ngp::damper::Thermal;
use crate::tyre::grip::{Grip, GripInput};
use crate::tyre::solve::Contact;
use crate::tyre::store::TyreStore;
use crate::tyre::{
    align, contact_patch, curve, fill, force, forces, friction, grip, heat, locate, outputs, relax, slip, solve_patch,
    state, step, step_inner, write,
};
use crate::Variant;

/// Runs a frame port on the `CScratch` at `this`: the frame is loaded once, `run` edits it, and it is stored once when
/// `run` succeeds.
pub fn with_frame<S: TyreStore, T>(
    mem: &mut S,
    this: u32,
    run: impl FnOnce(&mut Frame) -> Result<T, Outside>,
) -> Result<T, Outside> {
    let mut frame = mem.load_frame(this);
    let out = run(&mut frame)?;
    mem.store_frame(this, &frame);
    Ok(out)
}

/// `Locate (table, x, n, out)`: the segment at `out` is the hint, and the result is stored back there.
pub fn locate<S: TyreStore>(mem: &mut S, table: u32, x: f32, n: u32, out: u32) -> Result<(), Outside> {
    let table = mem.read_table(table, n as usize);
    let seg = locate::locate(mem.load_segment(out), &table, x)?;
    mem.store_segment(out, &seg);
    Ok(())
}

/// `CScratch::Sign`, st0.
pub fn sign<S: TyreStore>(mem: &S, this: u32, variant: Variant) -> f64 {
    force::signed_term(mem.load_sign_input(this), variant)
}

/// `CScratch::Friction (k)`, st0.
pub fn friction<S: TyreStore>(mem: &mut S, this: u32, k: f32, variant: Variant) -> f64 {
    let out = friction::friction(mem.load_friction(this, k), variant);
    mem.store_friction(this, out.sv);
    out.value
}

/// `CScratch::Align (a*, b*)`: whether it aligned.
pub fn align<S: TyreStore>(mem: &mut S, this: u32, a: u32, b: u32) -> Result<bool, Outside> {
    let a = Vec4::load(mem, a);
    let b = Vec4::load(mem, b);
    with_frame(mem, this, |frame| Ok(align::align(frame, a, b)))
}

/// `CScratch::ComputeOutputs`: the 17 stack words, scales at 1, 2, 4 and 5, `src` at 6, the output pointers at 7 to 16.
pub fn compute_outputs<S: TyreStore>(mem: &mut S, this: u32, s: &[u32], variant: Variant) {
    let f = f32::from_bits;
    let input = mem.load_compute_outputs(this, s[6], f(s[1]), f(s[2]), f(s[4]), f(s[5]));
    let out = outputs::compute_outputs(&input, variant);
    mem.store_compute_outputs(std::array::from_fn(|i| s[7 + i]), &out);
}

/// `TyreClamp (a, b, c, out*)`.
pub fn tyre_clamp<S: TyreStore>(mem: &mut S, a: f32, b: f32, c: f32, out: u32) {
    mem.set_f32(out, force::tyre_clamp(a, b, c));
}

/// `FloorS32 (x, table, count)`, st0.
pub fn floor_s32<S: TyreStore>(mem: &S, x: f32, table: u32, count: u32, variant: Variant) -> f64 {
    curve::floor_s32(x, count as i32, |i| mem.f32(table + 4 * i), variant)
}

/// `CScratch::ComputeSlip (dir*, spin)`.
pub fn compute_slip<S: TyreStore>(mem: &mut S, this: u32, dir: u32, spin: f32, variant: Variant) {
    let dir = [mem.f32(dir), mem.f32(dir + 4), mem.f32(dir + 8)];
    let state = mem.load_state(this);
    let matrix = mem.load_slip_matrix(this);
    let out = slip::compute_slip(&matrix, state.effective_radius, dir, spin, variant);
    mem.store_slip(this, &out);
}

/// `CScratch::SolveForces (k, out1*, out2*)`: `out2` is read (the asm adds into it), and both are written.
pub fn solve_forces<S: TyreStore>(
    mem: &mut S,
    this: u32,
    k: f32,
    out1: u32,
    out2: u32,
    variant: Variant,
) -> Result<(), Outside> {
    let out2_in = Vec4::load(mem, out2);
    let (v1, v2) = with_frame(mem, this, |frame| Ok(forces::solve_forces(frame, k, out2_in, variant)))?;
    v1.store(mem, out1);
    v2.store(mem, out2);
    Ok(())
}

/// `CScratch::FillOutputs (src*, pair*, divisor)`.
pub fn fill_outputs<S: TyreStore>(mem: &mut S, this: u32, src: u32, pair: u32, divisor: f32) -> Result<(), Outside> {
    let src = Vec4::load(mem, src);
    let pair: [u32; 3] = std::array::from_fn(|k| mem.u32(pair + 4 * k as u32));
    with_frame(mem, this, |frame| {
        fill::fill_outputs(frame, src, pair, divisor);
        Ok(())
    })
}

/// `CScratch::WriteOutputs (out*)`.
pub fn write_outputs<S: TyreStore>(mem: &mut S, this: u32, out: u32) -> Result<(), Outside> {
    let block = with_frame(mem, this, |frame| Ok(write::write_outputs(frame)))?;
    mem.store_output_block(out, &block);
    Ok(())
}

/// `TyreFrictionCurve`: 13 stack words, values 0 to 8, out pointers 9 and 10, the table and its count; st0.
pub fn friction_curve<S: TyreStore>(mem: &mut S, s: &[u32], variant: Variant) -> f64 {
    let (table, count) = (s[11], s[12] as i32);
    let input = curve::CurveInput { args: std::array::from_fn(|k| f32::from_bits(s[k])), count };
    let out = curve::friction_curve(&input, |i| mem.f32(table + 4 * i), variant);
    mem.set_f32(s[9], out.out9);
    mem.set_f32(s[10], out.out10);
    out.value
}

/// `CScratch::SolveDirs (a, b, c, out0*, out1*, out2*)`.
pub fn solve_dirs<S: TyreStore>(mem: &mut S, this: u32, abc: [f32; 3], outs: [u32; 3], variant: Variant) {
    let radius = mem.load_state(this).effective_radius;
    let out = slip::solve_dirs(abc[0], abc[1], abc[2], radius, variant);
    for (at, value) in outs.into_iter().zip(out) {
        mem.set_f32(at, value);
    }
}

/// `CScratch::ClampGrip (s, a, b, c, d, out_a*, out_b*)`.
pub fn clamp_grip<S: TyreStore>(
    mem: &mut S,
    this: u32,
    g: GripInput,
    out_a: u32,
    out_b: u32,
    variant: Variant,
) -> Result<(), Outside> {
    let out: Grip = with_frame(mem, this, |frame| {
        let out = grip::clamp_grip(&frame.state, g, variant);
        frame.state.grip_flag = out.flag;
        frame.scratch.f3f4 = out.f3f4;
        frame.scratch.f3f8 = out.f3f8;
        Ok(out)
    })?;
    mem.set_u32(out_a, out.out_a);
    mem.set_u32(out_b, out.out_b);
    Ok(())
}

/// `CScratch::Accumulate`.
pub fn accumulate<S: TyreStore>(mem: &mut S, this: u32) {
    let out = force::accumulate(mem.load_accumulate(this));
    mem.store_accumulate(this, &out);
}

/// The contact solvers `(p*, qb*, r*, ...)`: the contact loaded from and stored back to the three blocks.
pub fn with_contact<S: TyreStore>(mem: &mut S, this: u32, p: u32, qb: u32, r: u32, run: impl FnOnce(&mut Contact)) {
    let mut contact = mem.load_contact(this, p, qb, r);
    run(&mut contact);
    mem.store_contact(this, p, qb, &contact);
}

/// `CScratch::HeatStep (a, b, c)`.
pub fn heat_step<S: TyreStore>(mem: &mut S, this: u32, a: f32, b: f32, c: f32, ngp: Option<&Thermal>) {
    let input = mem.load_heat(this, a, b, c);
    mem.store_heat(this, &heat::heat_step(input, ngp));
}

/// `CScratch::TableStep (idx, x)`.
pub fn table_step<S: TyreStore>(mem: &mut S, this: u32, idx: u32, x: f32, ngp: Option<&Thermal>) {
    let input = mem.load_table(this, idx);
    let out = relax::table_step(&input, x, ngp);
    mem.store_table(this, idx, &out);
}

/// `CScratch::SetStateField (flag)`.
pub fn set_state_field<S: TyreStore>(mem: &mut S, this: u32, flag: u32) -> Result<(), Outside> {
    with_frame(mem, this, |frame| {
        state::set_state_field(frame, flag);
        Ok(())
    })
}

/// `CScratch::SetState48 (v)`.
pub fn set_state48<S: TyreStore>(mem: &mut S, this: u32, v: f32) -> Result<(), Outside> {
    with_frame(mem, this, |frame| {
        state::set_state48(frame, v);
        Ok(())
    })
}

/// `CScratch::BlendTable (i, j, a, b, o1, o2)`.
pub fn blend_table<S: TyreStore>(mem: &mut S, this: u32, i: u32, j: u32, ab: [f32; 2], o1: u32, o2: u32) {
    let input = mem.load_blend_table(this, i, j, ab[0], ab[1]);
    mem.store_blend(o1, o2, relax::blend_table(&input));
}

/// `CScratch::RelaxCount (dt)`: RelaxStep, then the counter at state+0x1c.
pub fn relax_count<S: TyreStore>(mem: &mut S, this: u32, dt: f32, ngp: Option<&Thermal>) {
    let input = mem.load_relax(this, dt);
    let out = relax::relax_step(&input, ngp);
    mem.store_relax_count(this, &out);
}

/// `CScratch::RelaxStep (dt)`.
pub fn relax_step<S: TyreStore>(mem: &mut S, this: u32, dt: f32, ngp: Option<&Thermal>) {
    let input = mem.load_relax(this, dt);
    let out = relax::relax_step(&input, ngp);
    mem.store_relax(this, &out);
}

/// `CScratch::SplitSlip`.
pub fn split_slip<S: TyreStore>(mem: &mut S, this: u32) -> Result<(), Outside> {
    with_frame(mem, this, |frame| {
        slip::split_slip(frame);
        Ok(())
    })
}

/// `CTyreMatData::CopyFrom (src*)`.
pub fn copy_from<S: TyreStore>(mem: &mut S, this: u32, src: u32) {
    let src = mem.load_material(src);
    mem.store_material(this, &src);
}

/// `CScratch::SolvePair`.
pub fn solve_pair<S: TyreStore>(mem: &mut S, this: u32, variant: Variant) -> Result<(), Outside> {
    with_frame(mem, this, |frame| {
        grip::solve_pair(frame, variant);
        Ok(())
    })
}

/// `CScratch::StepInner (rec*, vec3*, vec4*, out3*, out4*, out5*)`.
pub fn step_inner<S: TyreStore>(mem: &mut S, this: u32, rec: u32, outs: [u32; 5], variant: Variant) {
    let input = mem.load_step_inner(this, rec);
    let out = step_inner::step_inner(&input, variant);
    mem.store_step_inner(outs, &out);
}

/// `CScratch::ContactPatch (out*)`: whether it solved; the value the port returns is stored at `out`.
pub fn contact_patch<S: TyreStore>(mem: &mut S, this: u32, out: u32, variant: Variant) -> Result<bool, Outside> {
    let solved = with_frame(mem, this, |frame| Ok(contact_patch::contact_patch(frame, variant)))?;
    if let Some(value) = solved {
        mem.set_f32(out, value);
    }
    Ok(solved.is_some())
}

/// `CScratch::SolvePatch`.
pub fn solve_patch<S: TyreStore>(mem: &mut S, this: u32, variant: Variant) -> Result<(), Outside> {
    with_frame(mem, this, |frame| solve_patch::solve_patch(frame, variant))
}

/// `CScratch::Step`: its 16 stack words are loaded once, `Step` runs on the frame, and its outputs are stored after
/// the frame.
pub fn step<S: TyreStore>(mem: &mut S, this: u32, words: &[u32], ngp: Option<&Thermal>) -> Result<(), Outside> {
    let (args, ptrs): (StepArgs, _) = mem.load_step(&words[..16]);
    let out = with_frame(mem, this, |frame| step::step(frame, &args, ngp))?;
    mem.store_step(&ptrs, &out);
    Ok(())
}
