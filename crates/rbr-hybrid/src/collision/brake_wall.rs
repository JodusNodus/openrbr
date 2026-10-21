//! The brake walls (`CBrakeWall`, `E34WallVf`): queries, raycasts and the penetration emit.

use rbr_physics::collision::layout::{self};
use rbr_physics::collision::wall::{self, VfE8Host, VfF0Host, WallVfCcHost};
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::signal;

use super::{Callees, FETCH_X};
use crate::{Ctx, Decline, Host, Regs, Return, SignalCallees};

/// CBrakeWall::QueryPoint (0x4e3dc0): thiscall on the wall with the point and the collector; eax is the leaf count's
/// result from `wall::query_point`.
pub(super) fn brake_wall_query_point(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, collector, ..] = r.stack;
    Ok(Return::void(wall::query_point(&mut ctx.mem, r.ecx, point, collector), 8))
}

/// thiscall `(sig)`: `CMultiSignal1<const CSigCarBrakeWallPenetration&>::Emit` (0x4e4490), the signal template's emit
/// with the connections in the emulator (as `E41EngineNoiseSignal::Emit`). A count out of range declines (the original
/// traps).
pub(super) fn brake_wall_emit(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, r.ecx)?;
    signal::emit(&mut ctx.mem, &mut SignalCallees(ctx.host, crate::emit_name(0x4e4490)), r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// The original's call out of `E34WallVf::Raycast`: the penetration emit (0x4e4490), thiscall on the emitter with the
/// buffer. The emit is the call site's target (0x4e4933), which NGP retargets (to 0x100ed470).
struct WallRaycast<'a>(&'a dyn Host, u32);

impl WallRaycast<'_> {
    fn new<'a>(ctx: &Ctx<'a>) -> WallRaycast<'a> {
        WallRaycast(ctx.host, crate::ngp::call_target(&ctx.mem, 0x4e4933))
    }
}

impl wall::RaycastCalls for WallRaycast<'_> {
    fn emit_penetration(&mut self, this: u32, out: u32) {
        self.0.call(self.1, this, &[out]);
    }
}

/// `E34WallVf::Raycast` (0x4e4570): thiscall on the wall (`ecx`) with `(point, out)`; the leaf list of the query is in
/// the original's frame, 0x194 bytes below the entry esp.
pub(super) fn wall_raycast(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, out, ..] = r.stack;
    let mut calls = WallRaycast::new(ctx);
    let eax = layout::wall_raycast(&mut calls, &mut ctx.mem, r.ecx, point, out, r.esp - 0x194);
    Ok(Return::void(eax, 8))
}

/// `CBrakeWallManager::RaycastEntry` (0x4e4960): thiscall `(point, out)`; the point's copy is in the original's frame,
/// 8 bytes below the entry esp.
pub(super) fn raycast_entry(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, out, ..] = r.stack;
    let eax = layout::wall_probe(&mut WallRaycast::new(ctx), &mut ctx.mem, r.ecx, point, out, r.esp - 8);
    Ok(Return::void(eax, 8))
}

/// CBrakeWall::Vf110 (0x4e3860): thiscall on the wall; eax is 1.
pub(super) fn brake_wall_is_active(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(wall::is_active(), 0))
}

/// The calls of `E34WallVf::VfCC`, at the entry stack pointers the port gives.
struct WallCorners<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl WallVfCcHost for WallCorners<'_> {
    fn frame(&mut self, sp: u32, wall: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(wall) + 0x30);
        self.host.call_at(sp, target, wall, &[]).eax
    }

    fn fetch_x(&mut self, sp: u32, corner: u32, out: u32, frame: u32) {
        self.host.call_at(sp, FETCH_X, corner, &[out, frame]);
    }

    fn query_box(&mut self, sp: u32, wall: u32, bounds: u32, collector: u32) -> u32 {
        self.host.call_at(sp, BRAKE_WALL_QUERY_BOX, wall, &[bounds, collector]).eax
    }
}

pub(super) const BRAKE_WALL_QUERY_BOX: u32 = 0x4e3f10;

/// CBrakeWall::VfF0's call of the wall's slot 0xe8.
struct WallSlotE8<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl VfF0Host for WallSlotE8<'_> {
    fn vf_e8(&mut self, sp: u32, wall: u32, a: u32, b: u32, out: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(wall) + 0xe8);
        self.host.call_at(sp, target, wall, &[a, b, 0, out, 1]).eax
    }
}

/// CBrakeWall::VfF0 (0x4e3970): thiscall on the wall with eight pointers, ret 0x20; eax is 1 when `VfE8` hit.
pub(super) fn brake_wall_vf_f0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut args = [0; 8];
    args.copy_from_slice(&r.stack[..8]);
    let mut calls = WallSlotE8 { mem: ctx.mem.clone(), host: ctx.host };
    let eax = layout::brake_wall_vf_f0(&mut calls, &mut ctx.mem, r.ecx, args, r.esp);
    Ok(Return::void(eax, 0x20))
}

impl VfE8Host for Callees<'_> {
    fn fetch_x(&mut self, sp: u32, matrix: u32, out: u32, worker: u32) {
        self.0.call_at(sp, FETCH_X, matrix, &[out, worker]);
    }
}

/// CBrakeWall::VfE8 (0x4e3a10): thiscall on the wall with `(a, b, c, out, flag)`, ret 0x14; eax is 1 on a hit.
pub(super) fn brake_wall_vf_e8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut args = [0; 5];
    args.copy_from_slice(&r.stack[..5]);
    let eax = layout::brake_wall_vf_e8(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, args, r.esp);
    Ok(Return::void(eax, 0x14))
}

/// E34WallVf::VfCC (0x4e42f0): thiscall on the wall with two corner matrices, ret 8; eax is 1 when the corners' box
/// meets the wall's leaves.
pub(super) fn wall_vf_cc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let mut calls = WallCorners { mem: ctx.mem.clone(), host: ctx.host };
    let eax = layout::wall_vf_cc(&mut calls, &mut ctx.mem, r.ecx, a, b, r.esp);
    Ok(Return::void(eax, 8))
}

/// CBrakeWall::QueryBox (0x4e3f10): thiscall on the wall with the box and the collector, ret 8; eax is the leaf count's
/// result from `wall::query_box`.
pub(super) fn brake_wall_query_box(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [bounds, collector, ..] = r.stack;
    Ok(Return::void(wall::query_box(&mut ctx.mem, r.ecx, bounds, collector), 8))
}
