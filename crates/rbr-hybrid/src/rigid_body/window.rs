//! The window steps (back step, rest, start), wake and sleep, and the `Vf16`/`Vf17x` state helpers.

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::{self as core_layout, Filing};
use rbr_physics::rigid_body::emit::BodyEvent;
use rbr_physics::rigid_body::host::{CellHost, HostUnavailable, StateHost, Vf54Host};
use rbr_physics::rigid_body::layout::{self, Body};

use super::pose::register_cell_at;
use super::{
    unavailable, Callees, BODY_EVENT, BODY_EVENT_EMIT, ENTITY_VF158, ENTITY_VF48, SCRATCH_BELOW_ESP, SCRATCH_RESET,
    SIGNALS,
};
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

/// thiscall, no stack arguments: `Vf17x` on the state (`ecx`), the rotation from the orientation, times the diagonal when
/// the gate is positive. The original's result is no value.
pub(super) fn vf17x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut callees = Callees { mem: ctx.mem.clone(), host: ctx.host, scratch: r.esp - SCRATCH_BELOW_ESP };
    layout::refresh_state_rotation(&mut ctx.mem, r.ecx, &mut callees).map_err(unavailable)?;
    Ok(Return::void(0, 0))
}

/// The calls the window steps make outside the body (`CellHost`, `StateHost`), as the original makes them.
struct Window<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
}

impl CellHost for Window<'_> {
    fn register_cell(&mut self, link: u32, x: f32, y: f32) -> Result<(), HostUnavailable> {
        register_cell_at(&self.mem, self.host, self.this + link, x, y);
        Ok(())
    }
}

impl StateHost for Window<'_> {
    fn refresh_state(&mut self, body: &Body) {
        body.store(&mut self.mem, self.this);
        let slot16 = self.mem.u32(self.mem.u32(self.this) + 4 * 16);
        self.host.call(slot16, self.this, &[]);
    }
}

/// thiscall `(a)`: `CSolid::VfDx`, the back step to fraction `a` of the window. eax is not modelled.
pub(super) fn back_step(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Window { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx };
    let a = f32::from_bits(r.stack[0]);
    layout::solid_back_step(&mut ctx.mem, &mut calls, r.ecx, a).map_err(unavailable)?;
    Ok(Return::void(0, 4))
}

/// thiscall `(a)`: `CSolid::VfEx`, the rest of the window (`a` seconds of its time). eax is not modelled.
pub(super) fn advance_rest(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Window { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx };
    let a = f32::from_bits(r.stack[0]);
    layout::solid_advance_rest(&mut ctx.mem, &mut calls, r.ecx, a).map_err(unavailable)?;
    Ok(Return::void(0, 4))
}

/// thiscall `()`: `CSolid::E43Vf48` (slot 18, `CSolid::S18` in the notes): `CEntity::E43Vf48` on the body (its children,
/// called at the original's depth), then the window's start. eax is the last copy's destination (+0x2e0).
pub(super) fn window_start(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ctx.host.call_at(r.esp - 0x10, ENTITY_VF48, r.ecx, &[]);
    layout::solid_window_start(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.ecx + 0x2e0, 0))
}

/// `CSolid::E43Vf90`'s and `E43Vf94`'s calls (`layout::RestHost`): the body's slot 18 (E43Vf90 only) and the event's emit
/// through the host, and `CEntity::E43Vf90`'s (`E43Vf94`'s) filing from the solid's own slots. `frame` is the original's
/// frame (`(esp - 4) & !0xf`, less 0x64): the calls enter 0x10 below it (0x14 for the emit, after the event's pointer),
/// and the event is at +0x14.
struct Wake<'a> {
    host: &'a dyn Host,
    frame: u32,
    eax: u32,
}

impl core_layout::FilingHost for Wake<'_> {
    fn filing(&mut self, mem: &Mem, object: u32) -> Result<Filing, HostUnavailable> {
        layout::solid_filing(mem, object)
    }
}

impl core_layout::EntityVf158Host for Wake<'_> {
    fn slot158(&mut self, sp: u32, target: u32, parent: u32) -> Result<u32, HostUnavailable> {
        Ok(self.host.call_at(sp, target, parent, &[]).eax)
    }
}

impl layout::RestHost for Wake<'_> {
    fn slot18(&mut self, mem: &mut Mem, this: u32) -> Result<(), HostUnavailable> {
        let slot18 = layout::vtable_target(mem, this, 0x48);
        self.host.call_at(self.frame - 0x10, slot18, this, &[]);
        Ok(())
    }

    fn emit(&mut self, mem: &Mem, event: &BodyEvent) -> Result<(), HostUnavailable> {
        let at = self.frame + 0x14;
        layout::store_body_event(&mut mem.clone(), at, event);
        let signal = mem.u32(SIGNALS) + BODY_EVENT;
        self.eax = self.host.call_at(self.frame - 0x14, BODY_EVENT_EMIT, signal, &[at]).eax;
        Ok(())
    }
}

impl layout::VerySoftHost for Wake<'_> {
    fn slot0(&mut self, mem: &Mem, sp: u32, this: u32) -> Result<(), HostUnavailable> {
        let target = layout::vtable_target(mem, this, 0);
        self.host.call_at(sp, target, this, &[]);
        Ok(())
    }

    fn entity_vf158(&mut self, _mem: &mut Mem, sp: u32, this: u32) -> Result<(), HostUnavailable> {
        self.host.call_at(sp, ENTITY_VF158, this, &[]);
        Ok(())
    }

    fn emit(&mut self, mem: &mut Mem, frame: u32, event: &BodyEvent) -> Result<u32, HostUnavailable> {
        let at = frame + 0x14;
        layout::store_body_event(&mut mem.clone(), at, event);
        let signal = mem.u32(SIGNALS) + BODY_EVENT;
        self.eax = self.host.call_at(frame - 0x14, BODY_EVENT_EMIT, signal, &[at]).eax;
        Ok(self.eax)
    }
}

/// Declines a body with no parent whose filing slots are not `CSolid`'s (`CEntity::E43Vf90` and `E43Vf94` file it again),
/// before any write.
fn check_filing(ctx: &Ctx, this: u32) -> Result<(), Decline> {
    if Body::load(&ctx.mem, this).parent == 0 && layout::solid_filing(&ctx.mem, this).is_err() {
        return Err(Decline::Unsupported("the body's filing slots are not CSolid's"));
    }
    Ok(())
}

/// thiscall `()`: `CSolid::E43Vf90` (slot 36), the body's wake (`layout::solid_wake`). eax is the emit's.
pub(super) fn e43vf90(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let this = r.ecx;
    check_filing(ctx, this)?;
    let mut wake = Wake { host: ctx.host, frame: aligned_frame(r.esp) - 0x64, eax: 0 };
    layout::solid_wake(&mut ctx.mem, &mut wake, this).map_err(unavailable)?;
    Ok(Return::void(wake.eax, 0))
}

/// thiscall `()`: `CSolid::E43Vf94` (slot 37), the body's sleep (`layout::solid_sleep`), which the sleep trip calls. Its
/// frame is E43Vf90's (the same prologue). eax is the emit's.
pub(super) fn e43vf94(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let this = r.ecx;
    check_filing(ctx, this)?;
    let mut sleep = Wake { host: ctx.host, frame: aligned_frame(r.esp) - 0x64, eax: 0 };
    layout::solid_sleep(&mut ctx.mem, &mut sleep, this).map_err(unavailable)?;
    Ok(Return::void(sleep.eax, 0))
}

/// thiscall `()`: `CSolid::E43Vf158` (slot 86), make a parentless body very soft, refile it, then emit SLEEP.
pub(super) fn e43vf158(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let this = r.ecx;
    check_filing(ctx, this)?;
    let frame = ((r.esp - 4) & !0xF) - 0x64;
    let mut soft = Wake { host: ctx.host, frame, eax: 0 };
    let eax = layout::solid_vf158(&mut ctx.mem, &mut soft, this, frame).map_err(unavailable)?;
    Ok(Return::void(eax, 0))
}

/// `E47_Vf54Helper`'s entity call: `E43_EntityVf54Fwd` (0x519ca0) tail-jumps to slot 1 of the body's `+0x5c` object,
/// which is entered where the forwarder is (`sp`, 8 below the helper's entry).
struct EntityVf54<'a> {
    host: &'a dyn Host,
    mem: Mem,
    this: u32,
    sp: u32,
}

impl Vf54Host for EntityVf54<'_> {
    fn entity_scratch_tail(&mut self) -> Result<(), HostUnavailable> {
        let object = self.this + 0x5c;
        let slot1 = layout::vtable_target(&self.mem, object, 4);
        self.host.call_at(self.sp, slot1, object, &[]);
        Ok(())
    }
}

/// thiscall `()`: `E47_Vf54Helper` (0x52b6b0, `layout::scratch_reset_after_entity`): the entity call, then the scratch's slot 1, which
/// the port runs as `E47SolidScratch::Vf04` (declined for another scratch, before the entity call). eax is the reset's
/// last clear's row (the scratch's +0x120).
pub(super) fn scratch_reset_after_entity(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let this = r.ecx;
    if layout::vtable_target(&ctx.mem, this + 0x280, 4) != SCRATCH_RESET {
        return Err(Decline::Unsupported("the scratch's slot 1 is not E47SolidScratch::Vf04"));
    }
    let mut entity = EntityVf54 { host: ctx.host, mem: ctx.mem.clone(), this, sp: r.esp - 8 };
    layout::scratch_reset_after_entity(&mut ctx.mem, this, &mut entity).map_err(unavailable)?;
    Ok(Return::void(this + 0x280 + 0x120, 0))
}

/// thiscall, no stack arguments, eax: `Vf17x` on the state at `+0xe0` twice, then `CBody::Vf40` on the body.
pub(super) fn vf16_helper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = vf16(ctx, r.ecx, r.esp - SCRATCH_BELOW_ESP).map_err(unavailable)?;
    Ok(Return::void(eax, 0))
}

/// `Vf16Helper` (0x52b6d0) on the body, as the port runs it. `scratch` is the free stack for `E46Mat::Mul`'s operands.
/// The result is the eax the original returns.
pub(crate) fn vf16(ctx: &mut Ctx, body: u32, scratch: u32) -> Result<u32, HostUnavailable> {
    let mut callees = Callees { mem: ctx.mem.clone(), host: ctx.host, scratch };
    layout::vf16_helper(&mut ctx.mem, body, &mut callees)
}

/// `E47SolidState::Vf17x` (0x529a50) on the state, as the port runs it (its `Mul` as in `vf16`).
pub(crate) fn vf17x_state(ctx: &mut Ctx, state: u32, scratch: u32) -> Result<(), HostUnavailable> {
    let mut callees = Callees { mem: ctx.mem.clone(), host: ctx.host, scratch };
    layout::refresh_state_rotation(&mut ctx.mem, state, &mut callees)
}
