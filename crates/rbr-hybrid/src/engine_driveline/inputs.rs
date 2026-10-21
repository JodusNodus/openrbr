//! `CDrive`'s inputs and accessors, the signal emits, the matrix helpers and the forwarded members.

use rbr_physics::engine_driveline::{drive, layout};
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::signal_emit;

use super::f;
use crate::{Ctx, Decline, Regs, Return, SignalCallees};
use rbr_physics::engine_driveline::store::EngineStore;

/// `ret 4` with no body (0x503860, 0x5043b0): the original does nothing with its argument.
pub(super) fn nop_ret4(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(0, 4))
}

/// thiscall, no arguments: clears the two lanes (`layout::clear_lanes`).
pub(crate) fn clear_lanes(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::clear_lanes(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(sig)`: `E41EngineNoiseSignal::Emit` (0x50ef50), the physics core's emit with the connections in the
/// emulator. A count out of range declines (the original traps).
pub(super) fn emit_noise(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, r.ecx)?;
    signal_emit::emit(&mut ctx.mem, &mut SignalCallees(ctx.host, crate::emit_name(0x50ef50)), r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(sig)`: the gear-change signal's `CMultiSignal1::Emit` instantiation.
pub(super) fn emit_gear_change(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, r.ecx)?;
    signal_emit::emit(&mut ctx.mem, &mut SignalCallees(ctx.host, crate::emit_name(0x506f20)), r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// cdecl `(buf)`, eax: `E41Plain::InitRealLerp` (0x50f010). The three floats are the caller's frame words at the
/// buffer's +0x10, +0x20 and +0x24 (`stack[4]`, `stack[8]` and `stack[9]`).
pub(super) fn init_real_lerp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let buf = r.stack[0];
    let eax = layout::init_real_lerp(&mut ctx.mem, buf, f(r.stack[4]), f(r.stack[8]), f(r.stack[9]));
    Ok(Return::void(eax, 0))
}

// CDrive setters and getters.

/// thiscall `(brake)`.
pub(super) fn set_brake_in(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mem = &mut ctx.mem;
    let def = layout::DriveDef::load(mem, layout::DriveObjects::at(mem, r.ecx).def);
    let mut d = layout::Drive::load(mem, r.ecx);
    drive::set_brake_in(&mut d, &def, f(r.stack[0]));
    d.store(mem, r.ecx);
    Ok(Return::void(0, 4))
}

/// thiscall `(throttle)`.
pub(super) fn set_throttle_in(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut d = layout::Drive::load(&ctx.mem, r.ecx);
    drive::set_throttle_in(&mut d, f(r.stack[0]));
    d.store(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 4))
}

/// thiscall `(slip)`.
pub(super) fn set_clutch_slip(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut d = layout::Drive::load(&ctx.mem, r.ecx);
    drive::set_clutch_slip(&mut d, f(r.stack[0]));
    d.store(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 4))
}

/// thiscall `(delta)`.
pub(super) fn request_gear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mem = &mut ctx.mem;
    let objects = layout::DriveObjects::at(mem, r.ecx);
    let def = layout::DriveDef::load(mem, objects.def);
    let mut state = layout::DriveState::load(mem, objects.state);
    drive::request_gear(&mut state, def.gear_count, r.stack[0] as i32);
    state.store_commanded_gear(mem, objects.state);
    Ok(Return::void(0, 4))
}

fn drive_state(mem: &Mem, this: u32) -> layout::DriveState {
    layout::DriveState::load(mem, layout::DriveObjects::at(mem, this).state)
}

/// thiscall `()`, st0.
pub(super) fn get_engine_speed(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let st0 = drive::engine_speed(&drive_state(&ctx.mem, r.ecx));
    Ok(Return::float(st0 as f64, 0))
}

/// thiscall `()`, eax.
pub(super) fn get_gear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let gear = drive::gear(&drive_state(&ctx.mem, r.ecx));
    Ok(Return::void(gear as u32, 0))
}

/// thiscall `()`, st0.
pub(super) fn drive_speed_key(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let st0 = drive::drive_speed_key(&drive_state(&ctx.mem, r.ecx));
    Ok(Return::float(st0 as f64, 0))
}

/// thiscall `()`, eax: the word at `+8` (`layout::mgr_b_word8`).
pub(super) fn mgr_b_word8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.mgr_b_word8(r.ecx), 0))
}

/// thiscall `(idx)`, eax: entry `idx` of the table (`layout::mgr_b_entry`).
pub(super) fn mgr_b_entry(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.mgr_b_entry(r.ecx, r.stack[0]), 4))
}

/// thiscall `(idx)`, eax: the field at `+0xc` of entry `idx` (`layout::mgr_b_field_at`).
pub(super) fn mgr_b_field_at(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::mgr_b_field_at(r.ecx, r.stack[0]), 4))
}

/// thiscall `(v, out)`, eax: the matrix times the vector into `out` (`layout::mat4_mul_vec4`).
pub(super) fn mat4_mul_vec4(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::mat4_mul_vec4(&mut ctx.mem, r.ecx, r.stack[0], r.stack[1]);
    Ok(Return::void(eax, 8))
}

/// thiscall `(idx, out)`, eax: copies row `idx` of the table to `out` (`layout::copy_row`).
pub(super) fn copy_row(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::copy_row(&mut ctx.mem, r.ecx, r.stack[0], r.stack[1]);
    Ok(Return::void(eax, 8))
}

/// thiscall `(out)`: the transpose of the matrix at `this` into `out` (`layout::rows_copy`).
pub(super) fn rows_copy(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::rows_copy(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(out)`: the inverse of the transposed matrix at `this` into `out` (`layout::mat4_inverse_transposed`).
pub(super) fn mat4_inverse_transposed(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::mat4_inverse_transposed(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(a, x, y)`, eax: `a * 0x10` (`layout::set_two`).
pub(crate) fn set_two(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::set_two(&mut ctx.mem, r.ecx, r.stack[0], f(r.stack[1]), f(r.stack[2]));
    Ok(Return::void(eax, 0xc))
}

/// thiscall `(channel, value)`, eax: the state pointer when disabled, `channel * 0x60` otherwise (`layout::set_diff_lock`).
pub(super) fn set_diff_lock(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::set_diff_lock(&mut ctx.mem, r.ecx, r.stack[0], f(r.stack[1]));
    Ok(Return::void(eax, 8))
}

/// thiscall `(a)`, st0, eax: `a * 0xc8` (`layout::m6db0`).
pub(crate) fn m6db0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let st0 = layout::m6db0(&ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::float_with_eax(r.stack[0].wrapping_mul(0xc8), st0 as f64, 4))
}

/// `CDrive::Set4510` (0x504510): `jmp [[this+0x10]+4]` with `this+0x10`, the scratch member. Its slot 1 is
/// `CScratch::Vf04` (0x502c00, `clear_lanes`); any other target is left to the original.
pub(super) fn drive_reset_part(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    const SCRATCH_VF04: u32 = 0x502c00;
    let scratch = r.ecx + 0x10;
    if ctx.mem.u32(ctx.mem.u32(scratch) + 4) != SCRATCH_VF04 {
        return Err(Decline::Unsupported("the scratch member's slot 1 is not Vf04"));
    }
    clear_lanes(ctx, &Regs { ecx: scratch, ..*r })
}

/// thiscall `()`, eax: 1 when the drive's gate holds (`layout::drive_gate`).
pub(super) fn drive_gate(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::drive_gate(&ctx.mem, r.ecx), 0))
}

/// thiscall `(a)`, st0, eax: `a * 0xc8` (`layout::m6dd0`).
pub(crate) fn m6dd0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let st0 = layout::m6dd0(&ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::float_with_eax(r.stack[0].wrapping_mul(0xc8), st0 as f64, 4))
}

// CDrive leaves and workers.
