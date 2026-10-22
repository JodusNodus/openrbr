//! `CMountedRigidBody`: its pull (`Vf64`), step (`Vf6c`), state and pose.

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::{self, mounted, mounted_pose};
use rbr_physics::physics_core::layout::{MountedVf64Host, MountedVf6cHost};

use super::{BODY_EVENT_EMIT, E43VF64, E43VF6C, MB560, STATE_EMIT, WALKER, WORLD_COMPOSE};
use crate::collision::E01_VEC4_COPY_FROM;
use crate::entities::BODY_COMPOSE;
use crate::vcu::SET_DIRTY;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

/// The calls of `CMountedRigidBody::Vf64`, at the entry stack pointers the port gives.
struct MountedCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl MountedVf64Host for MountedCalls<'_> {
    fn solid_vf64(&mut self, sp: u32, body: u32) -> u32 {
        self.host.call_at(sp, E43VF64, body, &[]).eax
    }

    fn world_compose(&mut self, sp: u32, owner: u32, point: u32, out: u32) {
        self.host.call_at(sp, WORLD_COMPOSE, owner, &[point, out]);
    }

    fn walk(&mut self, sp: u32, src: u32, owner: u32, out: u32) {
        let walker = self.mem.u32(WALKER);
        self.host.call_at(sp, MB560, walker, &[src, owner, out]);
    }
}

/// thiscall `()`: `CMountedRigidBody::Vf64`, the mounted body's pull towards its mount point (`layout::mounted_vf64`).
/// The original's frame is `((esp - 4) & !0xf) - 0x70`. A null CDef origin declines before any call.
pub(super) fn mounted_vf64(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = MountedCalls { mem: ctx.mem.clone(), host: ctx.host };
    let frame = aligned_frame(r.esp) - 0x70;
    let eax = layout::mounted_vf64(&mut ctx.mem, &mut calls, r.ecx, frame)
        .ok_or(Decline::Unsupported("the CDef's origin is the null sentinel"))?;
    Ok(Return::void(eax, 0))
}

impl MountedVf6cHost for MountedCalls<'_> {
    fn solid_vf6c(&mut self, sp: u32, body: u32, dt: f32) -> u32 {
        self.host.call_at(sp, E43VF6C, body, &[dt.to_bits()]).eax
    }

    fn emit_event(&mut self, sp: u32, signal: u32, sig: u32) -> u32 {
        self.host.call_at(sp, BODY_EVENT_EMIT, signal, &[sig]).eax
    }

    fn emit_state(&mut self, sp: u32, signal: u32, sig: u32) -> u32 {
        self.host.call_at(sp, STATE_EMIT, signal, &[sig]).eax
    }
}

/// thiscall `(dt)`, `ret 4`: `CMountedRigidBody::Vf6c`, the mounted body's step (`layout::mounted_vf6c`). The original's
/// frame is `((esp - 4) & !0xf) - 0xa0`.
pub(super) fn mounted_vf6c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = MountedCalls { mem: ctx.mem.clone(), host: ctx.host };
    let frame = aligned_frame(r.esp) - 0xa0;
    let dt = f32::from_bits(r.stack[0]);
    let eax = layout::mounted_vf6c(&mut ctx.mem, &mut calls, r.ecx, dt, frame);
    Ok(Return::void(eax, 4))
}

/// thiscall `(state)`, `ret 4`: `E45Smalls::M1910` (0x521910), the mounted body's state pointer (`mounted::set_state`).
/// Leaves eax the argument.
pub(super) fn mounted_set_state(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    mounted::set_state(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(r.stack[0], 4))
}

/// `E45_M1980`'s calls, the original's: `NTweak::CBase::SetDirty`, `CCar::BodyCompose` and `E01Vec4::CopyFrom`.
struct PoseCalls<'a>(&'a dyn Host);

impl mounted_pose::PoseHost for PoseCalls<'_> {
    fn set_dirty(&mut self, _: &mut Mem, base: u32) {
        self.0.call(SET_DIRTY, base, &[]);
    }

    fn body_compose(&mut self, _: &mut Mem, owner: u32, a: u32, b: u32) {
        self.0.call(BODY_COMPOSE, owner, &[a, b]);
    }

    fn copy_from(&mut self, _: &mut Mem, to: u32, src: u32) {
        self.0.call(E01_VEC4_COPY_FROM, to, &[src]);
    }
}

/// fastcall `(body)`: `E45_M1980` (0x521980), the mounted body's pose from its state (`mounted_pose::pose_update`).
pub(super) fn mounted_pose_update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    mounted_pose::pose_update(&mut ctx.mem, &mut PoseCalls(ctx.host), r.ecx);
    Ok(Return::void(0, 0))
}
