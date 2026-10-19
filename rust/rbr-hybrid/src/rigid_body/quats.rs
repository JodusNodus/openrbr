//! The quaternion helpers (`E46Quat`).

use rbr_physics::math::quat::{from_quat, mul, normalize, Quat};
use rbr_physics::mem::Mem;
use rbr_physics::rigid_body::layout::Vec4;

use crate::{Ctx, Decline, Regs, Return};

fn load_quat(mem: &Mem, at: u32) -> Quat {
    let v = Vec4::load(mem, at);
    Quat { x: v.x, y: v.y, z: v.z, w: v.w }
}

fn store_quat(mem: &mut Mem, at: u32, q: Quat) {
    Vec4 { x: q.x, y: q.y, z: q.z, w: q.w }.store(mem, at);
}

/// thiscall `(q*)`: the 3x3 matrix at `this` (rows of three floats) from the quaternion; returns `this`.
pub(super) fn set_from_quat(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let m = from_quat(load_quat(&ctx.mem, r.stack[0]));
    for (k, v) in m.iter().flatten().enumerate() {
        ctx.mem.set_f32(r.ecx + 4 * k as u32, *v);
    }
    Ok(Return::void(r.ecx, 4))
}

/// thiscall `(q*, out*)`: `out = this * q`; returns `out`.
pub(super) fn quat_mul(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [q, out, ..] = r.stack;
    let p = mul(load_quat(&ctx.mem, r.ecx), load_quat(&ctx.mem, q));
    store_quat(&mut ctx.mem, out, p);
    Ok(Return::void(out, 8))
}

/// fastcall `(q*)` in ecx: normalises in place; returns `q`.
pub(super) fn quat_normalize(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let q = normalize(load_quat(&ctx.mem, r.ecx));
    store_quat(&mut ctx.mem, r.ecx, q);
    Ok(Return::void(r.ecx, 0))
}

/// thiscall `(out*)`: `out = normalize(this)`; returns `out`.
pub(super) fn quat_norm(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let out = r.stack[0];
    let q = normalize(load_quat(&ctx.mem, r.ecx));
    store_quat(&mut ctx.mem, out, q);
    Ok(Return::void(out, 4))
}
