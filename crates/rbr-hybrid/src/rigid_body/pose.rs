//! The pose step (`CSolid::VfC8`) and the integrator pass (`CSolid::E43Vf6C`).

use rbr_physics::mem::Mem;
use rbr_physics::rigid_body::host::{CellHost, HostUnavailable};
use rbr_physics::rigid_body::integrate::{integrate, IntegrateHost, StateSignal};
use rbr_physics::rigid_body::layout::{Body, Def, ParentFrame, Vec4};
use rbr_physics::rigid_body::pose::{pose_step, PoseHost};

use super::{
    unavailable, APPEND_CELL, CELL_GRID, COPY16_WALK, DRIVE, INTEGRATE, LOOKUP_TWIN, SCRATCH_BELOW_ESP, SIGNALS,
    STATE_EMIT, WALKER,
};
use crate::{Ctx, Decline, Host, Regs, Return};

/// The entity walk's callee entry, below the binding's entry stack pointer `esp`. `CSolid::E43Vf6C` (0x52cc30) makes its
/// frame `push ebp; mov ebp, esp; and esp, -0x10; sub esp, 0x44`, pushes `ebx`, `esi`, `edi` (0x50 below the aligned
/// frame), then pushes the step length and calls the walk at 0x52cf4e: the walk's return address slot is 0x58 below.
fn walk_sp(esp: u32) -> u32 {
    ((esp - 4) & !0xF) - 0x58
}

/// The calls the solid's pose step and integrator make outside the body, as the original makes them (`CellHost`,
/// `PoseHost`, `IntegrateHost`). The temps the original passes by pointer are in scratch memory below the call.
struct Pose<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    scratch: u32,
    esp: u32,
}

/// `CCellGrid::LookupTwin` at `(x, y)`, then `CCar::AppendA980` with the link node at `link_at`: the cell registration.
pub(super) fn register_cell_at(mem: &Mem, host: &dyn Host, link_at: u32, x: f32, y: f32) {
    let grid = mem.u32(CELL_GRID);
    let hit = host.call(LOOKUP_TWIN, grid, &[x.to_bits(), y.to_bits(), link_at]).eax;
    host.call(APPEND_CELL, hit, &[link_at]);
}

impl CellHost for Pose<'_> {
    fn register_cell(&mut self, link: u32, x: f32, y: f32) -> Result<(), HostUnavailable> {
        register_cell_at(&self.mem, self.host, self.this + link, x, y);
        Ok(())
    }
}

impl PoseHost for Pose<'_> {
    fn to_parent_frame(&mut self, local: Vec4) -> Vec4 {
        let parent = self.mem.u32(self.this + 0x44);
        let (local_at, out_at) = (self.scratch + 0x38, self.scratch + 0x20);
        local.store(&mut self.mem, local_at);
        let s30 = self.mem.u32(self.mem.u32(parent) + 4 * 12);
        let walker = self.mem.u32(WALKER);
        let called = self.host.call(s30, parent, &[local_at, walker, out_at]);
        self.host.call(COPY16_WALK, called.eax, &[]);
        Vec4::load(&self.mem, out_at)
    }

    fn sample_big(&mut self, at: Vec4, value: f32) -> f32 {
        let at_ptr = self.scratch + 0x50;
        at.store(&mut self.mem, at_ptr);
        let drive = self.mem.u32(DRIVE);
        // SampleBig, or NGP's replacement (0x10015fc0) at the call site 0x52caed
        let target = crate::ngp::Call::DriveLineSample.address();
        let called = self.host.call(target, drive, &[at_ptr, value.to_bits()]);
        called.st0 as f32
    }
}

impl IntegrateHost for Pose<'_> {
    fn entity_walk(&mut self, body: &Body, dt: f32) -> Body {
        body.store(&mut self.mem, self.this);
        self.host.call_at(walk_sp(self.esp), INTEGRATE, self.this, &[dt.to_bits()]);
        Body::load(&self.mem, self.this)
    }

    fn sleep_trip(&mut self, body: &Body) -> Result<Body, HostUnavailable> {
        body.store(&mut self.mem, self.this);
        let slot37 = self.mem.u32(self.mem.u32(self.this) + 4 * 37);
        self.host.call(slot37, self.this, &[]);
        Ok(Body::load(&self.mem, self.this))
    }

    fn emit_state(&mut self, state: &StateSignal) -> Result<(), HostUnavailable> {
        let payload = self.scratch + 0x80;
        self.mem.set_u32(payload, state.owner);
        state.position.store(&mut self.mem, payload + 0x10);
        let q = state.orientation;
        for (k, v) in [q.x, q.y, q.z, q.w].into_iter().enumerate() {
            self.mem.set_f32(payload + 0x20 + 4 * k as u32, v);
        }
        let sink = self.mem.u32(SIGNALS) + 0xe438;
        self.host.call(STATE_EMIT, sink, &[payload]);
        Ok(())
    }
}

/// thiscall `(a, b, c)`: the pose step (`CSolid::VfC8`). `b` takes the position and `c` the orientation. The calls outside
/// the body go through the original; the body is stored after the step.
pub(super) fn integrate_pose(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let this = r.ecx;
    let mut body = Body::load(&ctx.mem, this);
    let parent = (body.parent != 0).then(|| ParentFrame::load(&ctx.mem, body.parent));
    let mut calls = Pose { mem: ctx.mem.clone(), host: ctx.host, this, scratch: r.esp - SCRATCH_BELOW_ESP, esp: r.esp };
    pose_step(&mut body, f32::from_bits(a), b != 0, c != 0, parent, &mut calls).map_err(unavailable)?;
    body.store(&mut ctx.mem, this);
    Ok(Return::void(0, 12))
}

/// thiscall `(dt)`: the integrator pass (`CSolid::E43Vf6C`). The calls outside the body go through the original; the body
/// is stored after the pass.
pub(super) fn e43vf6c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let this = r.ecx;
    let def = Def::load(&ctx.mem, ctx.mem.u32(this + 0x34));
    let mut body = Body::load(&ctx.mem, this);
    let parent = (body.parent != 0).then(|| ParentFrame::load(&ctx.mem, body.parent));
    let mut calls = Pose { mem: ctx.mem.clone(), host: ctx.host, this, scratch: r.esp - SCRATCH_BELOW_ESP, esp: r.esp };
    integrate(&mut body, &def, f32::from_bits(r.stack[0]), parent, &mut calls).map_err(unavailable)?;
    body.store(&mut ctx.mem, this);
    Ok(Return::void(0, 4))
}
