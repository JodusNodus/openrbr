//! The vector helpers the collision code calls (`E01Vec4`, `E09Vec4`, `E21Vec3`, `E33Vec4`, `CFloat3`).

use rbr_physics::collision::layout::{self};
use rbr_physics::math::vec::{clear_vec4, Vec3};

use crate::{Ctx, Decline, Regs, Return};

/// thiscall on `dst` (`ecx`) with `(src*)`, ret 4: `E01Vec4::CopyFrom` (0x43c5b0), the sixteen bytes of `src` to `dst`.
/// Eax is `dst`.
pub(super) fn vec4_copy_from(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, ..] = r.stack;
    Ok(Return::void(layout::vec4_copy_from(&mut ctx.mem, r.ecx, src), 4))
}

/// thiscall on the row (`ecx`), no arguments: `E09Vec4::Clear` (0x465d80), the zero row. Eax is the row.
pub(super) fn vec4_clear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    clear_vec4().store(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.ecx, 0))
}

/// thiscall on the vector (`ecx`) with `(scale, out*)`, ret 8: `CFloat3::Scale` (0x473cd0), `out = this * scale` over
/// four lanes. Eax is `out`.
pub(super) fn float3_scale(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [scale, out, ..] = r.stack;
    layout::scaled(&mut ctx.mem, r.ecx, out, f32::from_bits(scale));
    Ok(Return::void(out, 8))
}

/// thiscall on the three-float vector (`ecx`), no arguments: SSE `Length`, returned as a float in st0.
pub(super) fn float3_length(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let vector = Vec3::load(&ctx.mem, r.ecx);
    Ok(Return::float(f64::from(vector.length3()), 0))
}

/// thiscall on the 16-byte vector (`ecx`) with the other vector (`other*`); eax is the vector's address.
pub(super) fn vec16_min(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::vec16_min(&mut ctx.mem, r.ecx, r.stack[0]), 4))
}

/// thiscall, as `vec16_min`, with the lane maxima.
pub(super) fn vec16_max(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::vec16_max(&mut ctx.mem, r.ecx, r.stack[0]), 4))
}

/// thiscall on the vector `a` (`ecx`) with `(b*, out*)`: `out = a - b`; eax is `out`.
pub(super) fn vec_sub(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, out, ..] = r.stack;
    Ok(Return::void(layout::vec3_sub(&mut ctx.mem, r.ecx, b, out), 8))
}

/// thiscall on the vector `a` (`ecx`) with `(scale, out*)`: `out = a * scale`; eax is `out`.
pub(super) fn vec_add_sub(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, c, out, ..] = r.stack;
    Ok(Return::void(layout::vec3_add_sub(&mut ctx.mem, r.ecx, b, c, out), 12))
}

pub(super) fn vec_sub_dot(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, out, d, ..] = r.stack;
    let dot = layout::vec3_sub_dot(&mut ctx.mem, r.ecx, b, out, d);
    Ok(Return::float_with_eax(r.ecx, dot as f64, 12))
}

pub(super) fn vec_cross_dot(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, out, d, ..] = r.stack;
    let dot = layout::vec3_cross_dot(&mut ctx.mem, r.ecx, b, out, d);
    Ok(Return::float_with_eax(out, dot as f64, 12))
}

pub(super) fn vec_dot(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, ..] = r.stack;
    let dot = layout::vec3_dot(&ctx.mem, r.ecx, b);
    Ok(Return::float_with_eax(r.ecx, dot as f64, 4))
}

pub(super) fn vec_cross(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [b, out, ..] = r.stack;
    Ok(Return::void(layout::vec3_cross(&mut ctx.mem, r.ecx, b, out), 8))
}

pub(super) fn vec_normalize(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::vec3_normalize(&mut ctx.mem, r.ecx), 0))
}

/// thiscall `(out, s)`: `E33Vec4::Scaled` (0x4e0700), `out = this * s` over four lanes. Returns `out` (eax); pops 8.
pub(super) fn e33_scaled(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, scale, ..] = r.stack;
    layout::scaled(&mut ctx.mem, r.ecx, out, f32::from_bits(scale));
    Ok(Return::void(out, 8))
}

pub(super) fn vec_scale(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [scale, out, ..] = r.stack;
    Ok(Return::void(layout::vec3_scale(&mut ctx.mem, r.ecx, f32::from_bits(scale), out), 8))
}

/// E33Vec4::MinMaxA (0x4e0ae0): thiscall on the pair with `(src, lo, hi)`, ret 0xc; eax is `hi`.
pub(super) fn min_max_a(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, lo, hi, ..] = r.stack;
    let eax = layout::min_max_a(&mut ctx.mem, r.ecx, src, lo, hi);
    Ok(Return::void(eax, 12))
}

/// thiscall on the vector (`ecx`), no arguments: `E01Vec4::Normalize` (0x43c6a0), in place. Eax is the vector.
pub(super) fn vec4_normalize(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::vec4_normalize(&mut ctx.mem, r.ecx), 0))
}

/// thiscall on `a` (`ecx`) with `(out, b)`, ret 8: `E09Vec4::Sub` (0x465da0). Eax is `out`.
pub(super) fn vec4_sub(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, b, ..] = r.stack;
    let eax = layout::vec4_sub_stale(&mut ctx.mem, r.ecx, out, b, r.esp);
    Ok(Return::void(eax, 8))
}

/// thiscall on `a` (`ecx`) with `(out, b)`, ret 8: `E21Vec3::Add` (0x4a19e0). Eax is `out`.
pub(super) fn vec3_add(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [out, b, ..] = r.stack;
    let eax = layout::vec3_add_stale(&mut ctx.mem, r.ecx, out, b, r.esp);
    Ok(Return::void(eax, 8))
}
