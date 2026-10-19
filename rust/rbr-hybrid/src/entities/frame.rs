//! The frame hooks of the current and old frames (`rbr_physics::entities::frame`, 0x519530-0x5197c0).

use rbr_physics::entities::frame::{Form, Hook, Owner};
use rbr_physics::entities::layout;
use rbr_physics::mem::Mem;

use super::{FRAME_VF04, FRAME_VF08, FRAME_VF0C, FRAME_VF10, FRAME_VF14};
use crate::rigid_body::{E43VFAC, E43VFB0};
use crate::{Ctx, Decline, Host, Regs, Return};

/// The call depth of a frame hook's owner slot: the original pushes its arguments after the saved esi.
const FRAME_CALL_DEPTH: u32 = 16;
const SOLID_SCALE_ADD_ROWS: u32 = 0x52ac80;
const OLD_FRAME_GATE_DEPTH: u32 = 8;

/// The owner's slot each frame hook calls; old frames choose form X or Y from the owner's gate.
fn frame_slot(hook: Hook, form: Form) -> u32 {
    match (hook, form) {
        (Hook::One, Form::X) => 0x9c,
        (Hook::Two, Form::X) => 0xa0,
        (Hook::Three, Form::X) => 0xac,
        (Hook::Four, Form::X) => 0xb0,
        (Hook::Five, Form::X) => 0xbc,
        (Hook::Six, Form::X) => 0xc0,
        (Hook::One, Form::Y) => 0xa4,
        (Hook::Two, Form::Y) => 0xa8,
        (Hook::Three, Form::Y) => 0xb4,
        (Hook::Four, Form::Y) => 0xb8,
        (Hook::Five, Form::Y) => 0xc4,
        (Hook::Six, Form::Y) => 0xc8,
    }
}

/// The frame's calls on its owner, through the host: the owner's hook slot with `this` the owner.
pub(super) struct FrameOwner<'a> {
    pub(super) mem: &'a mut Mem,
    pub(super) host: &'a dyn Host,
    pub(super) esp: u32,
}

impl Owner for FrameOwner<'_> {
    fn hook(&mut self, owner: u32, hook: Hook, form: Form, a: u32, b: u32) -> u32 {
        let sp = self.esp.wrapping_sub(FRAME_CALL_DEPTH);
        let target = layout::vtable_target(self.mem, owner, frame_slot(hook, form));
        // `CSolid::E43Vf9C`, `E43VfAC` and `E43VfB0` (their bindings in `rigid_body` make no call) run here on the pure
        // host.
        use rbr_physics::rigid_body::layout as solid;
        if matches!(target, SOLID_SCALE_ADD_ROWS | E43VFAC | E43VFB0)
            && crate::skip_call(self.host, self.mem, sp, &[a, b])
        {
            return match target {
                SOLID_SCALE_ADD_ROWS => solid::scale_add_rows(self.mem, owner, a, b),
                E43VFAC => {
                    solid::transform_body_rows(self.mem, owner, a, b);
                    0
                }
                _ => {
                    solid::transform_body_rows3(self.mem, owner, a, b);
                    0
                }
            };
        }
        self.host.call_at(sp, target, owner, &[a, b]).eax
    }

    fn gate(&mut self, owner: u32) -> u32 {
        self.host
            .call_at(
                self.esp.wrapping_sub(OLD_FRAME_GATE_DEPTH),
                layout::vtable_target(self.mem, owner, 0x80),
                owner,
                &[],
            )
            .eax
    }

    fn frame_of(&mut self, _ent: u32) -> u32 {
        unreachable!("a frame hook asks no entity for its frame")
    }

    fn old_frame_of(&mut self, _ent: u32) -> u32 {
        unreachable!("a frame hook asks no entity for its old frame")
    }
}

/// thiscall `(a, b)`: a `CEntityFrame::Vf04..Vf18` (`hook`): the owner's hook when the depth is set; returns `a`.
fn frame_hook(ctx: &mut Ctx, r: &Regs, hook: Hook) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let frame = layout::load_entity_frame(&ctx.mem, r.ecx);
    let mut owner = FrameOwner { mem: &mut ctx.mem, host: ctx.host, esp: r.esp };
    let eax = rbr_physics::entities::frame::entity_hook(&frame, hook, a, b, &mut owner);
    Ok(Return::void(eax, 8))
}

/// The hook of the `CEntityFrame` slot bound at `addr`.
pub(super) fn frame_hook_at(addr: u32) -> Option<Hook> {
    match addr {
        FRAME_VF04 => Some(Hook::One),
        FRAME_VF08 => Some(Hook::Two),
        FRAME_VF0C => Some(Hook::Three),
        FRAME_VF10 => Some(Hook::Four),
        FRAME_VF14 => Some(Hook::Five),
        _ => None,
    }
}

pub(super) fn frame_vf04_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame_hook(ctx, r, Hook::One)
}

pub(super) fn frame_vf08_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame_hook(ctx, r, Hook::Two)
}

pub(super) fn frame_vf0c_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame_hook(ctx, r, Hook::Three)
}

pub(super) fn frame_vf10_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame_hook(ctx, r, Hook::Four)
}

pub(super) fn frame_vf14_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    frame_hook(ctx, r, Hook::Five)
}

/// thiscall `(a, b)`: an old-frame slot uses its owner's gate to choose form X or Y; returns `a`.
fn old_frame_hook(ctx: &mut Ctx, r: &Regs, hook: Hook) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let frame = layout::load_entity_frame(&ctx.mem, r.ecx);
    let mut owner = FrameOwner { mem: &mut ctx.mem, host: ctx.host, esp: r.esp };
    let eax = rbr_physics::entities::frame::old_hook(&frame, hook, a, b, &mut owner);
    Ok(Return::void(eax, 8))
}

pub(super) fn old_frame_vf04_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    old_frame_hook(ctx, r, Hook::One)
}

pub(super) fn old_frame_vf08_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    old_frame_hook(ctx, r, Hook::Two)
}

pub(super) fn old_frame_vf0c_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    old_frame_hook(ctx, r, Hook::Three)
}

pub(super) fn old_frame_vf10_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    old_frame_hook(ctx, r, Hook::Four)
}
