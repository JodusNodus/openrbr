//! The entities' small virtual slots: `CEntity`, `CDummyObject`, `CGround`, the exclusion test and the limit update.

use rbr_physics::entities::dummy;
use rbr_physics::entities::exclusion::{exclusion_test, ParentTest};
use rbr_physics::entities::ground;
use rbr_physics::entities::hinge_stops;
use rbr_physics::entities::layout;
use rbr_physics::entities::layout::DummyCalls;
use rbr_physics::entities::slots::{self, FlagDispatch};
use rbr_physics::entities::store::EntityStore;
use rbr_physics::math::vec::clear_vec4;
use rbr_physics::mem::Mem;

use super::{E90_BUILD, EXCLUSION_TEST, FETCH_X};
use crate::collision::E01_VEC4_COPY_FROM;
use crate::vcu::SET_DIRTY;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

fn entity_echo(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(rbr_physics::entities::frame::entity_result(r.ecx, r.stack[0], r.stack[1]), 8))
}

pub(super) fn entity_vf9c_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    entity_echo(ctx, r)
}

pub(super) fn entity_vfa0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    entity_echo(ctx, r)
}

pub(super) fn entity_vfb0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    entity_echo(ctx, r)
}

/// thiscall `()`: `CEntity::E43Vf80` (`xor eax, eax; ret`), the zero the walks gate on.
pub(super) fn entity_vf80_run(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::zero_hook(), 0))
}

/// thiscall `(a, b, c, d, e)`, ret 0x14: `CEntity::VfE8` (0x519970, slot 58), `xor eax, eax`: no hit.
pub(super) fn entity_vf_e8(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::zero_hook(), 0x14))
}

/// thiscall: `CEntity::Vf118` and `Vf114` (0x519d60), both `xor eax, eax; ret`.
pub(super) fn vf118(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(0, 0))
}

/// The parent's test `CEntity::E43_ExclF80` makes first, through the host: it enters 16 bytes below the entry (the saved
/// esi and edi and the pushed `other`).
struct ParentCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    sp: u32,
}

impl ParentTest for ParentCalls<'_> {
    fn parent_test(&mut self, parent: u32, other: u32) -> u32 {
        match exclusion_test_direct(self.host, &mut self.mem, self.sp, parent, other) {
            Some(eax) => eax,
            None => self.host.call_at(self.sp, EXCLUSION_TEST, parent, &[other]).eax,
        }
    }
}

/// thiscall `(other)`: `CEntity::E43_ExclF80`, 0 when the entity excludes `other` (eax).
pub(super) fn exclusion_test_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = ParentCalls { mem: ctx.mem.clone(), host: ctx.host, sp: r.esp - 0x10 };
    Ok(Return::void(exclusion_test_at(&mut calls, &ctx.mem, r.ecx, r.stack[0]), 4))
}

fn exclusion_test_at(calls: &mut ParentCalls, mem: &Mem, this: u32, other: u32) -> u32 {
    exclusion_test(calls, &layout::load_excluder(mem, this), &layout::load_excluder(mem, other))
}

/// `CEntity::E43_ExclF80` on `this` with `other` entered at `sp`, run here on the pure host as its binding
/// (`exclusion_test_run`) would, the call's words written as the call writes them; `None` (nothing written) on a host
/// that runs the original.
pub(crate) fn exclusion_test_direct(host: &dyn Host, mem: &mut Mem, sp: u32, this: u32, other: u32) -> Option<u32> {
    if !crate::skip_call(host, mem, sp, &[other]) {
        return None;
    }
    let mut calls = ParentCalls { mem: mem.clone(), host, sp: sp - 0x10 };
    Some(exclusion_test_at(&mut calls, mem, this, other))
}

/// thiscall: `E43PairWalk::MFc10`, the old frame (eax).
pub(super) fn old_frame_of_run(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::old_frame_of(r.ecx), 0))
}

/// thiscall `(lo, hi)`: `CEntity::Vf120`, the jump to the entity's slot 0x11c with the same stack: the slot enters at
/// this entry's esp and pops the two words; eax is its.
pub(crate) const ENTITY_VF120: u32 = 0x519db0;

pub(super) fn vf120_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [lo, hi, ..] = r.stack;
    let target = ctx.mem.vf120_target(r.ecx);
    let eax = ctx.host.call_at(r.esp, target, r.ecx, &[lo, hi]).eax;
    Ok(Return::void(eax, 8))
}

/// thiscall `(lo, hi)`: `CDummyObject::Vf11C`, the dummy's box corners.
pub(super) fn dummy_vf11c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [lo, hi, ..] = r.stack;
    layout::dummy_box(&mut ctx.mem, r.ecx, lo, hi);
    Ok(Return::void(0, 8))
}

/// The host calls of `CDummyObject::VfCC`: `FetchX` and the hit handler, each entered at the given stack pointer.
struct DummyCallsAt<'a> {
    host: &'a dyn Host,
}

impl DummyCalls for DummyCallsAt<'_> {
    fn fetch_x(&mut self, sp: u32, this: u32, out: u32, arg: u32) {
        self.host.call_at(sp, FETCH_X, this, &[out, arg]);
    }

    fn group_on_hit(&mut self, sp: u32, group: u32) {
        self.host.call_at(sp, E90_BUILD, group, &[]);
    }
}

/// thiscall `(a, b)`: `CDummyObject::VfCC` (0x6009b0), the overlap of the dummy's box with the span of the two origins.
/// It returns 0 on every path. The original's frame is `FetchX`'s out words: `frame` is its base (see `layout`).
pub(super) fn dummy_vf_cc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    let frame = aligned_frame(r.esp).wrapping_sub(0x3c);
    let mut calls = DummyCallsAt { host: ctx.host };
    layout::dummy_span_test(&mut ctx.mem, r.ecx, a, b, frame, &mut calls);
    Ok(Return::void(0, 8))
}

/// thiscall `(a, b)`, ret 8: `CEntity::VfCC` (0x519e90), eax 1.
pub(super) fn entity_vf_cc(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::vf_cc(), 8))
}

/// thiscall with eight arguments, ret 0x20: `CEntity::VfF0` (0x519eb0), `xor eax, eax`.
pub(super) fn entity_vf_f0(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::zero_hook(), 0x20))
}

/// thiscall `(a, out)`, ret 8: `CEntity::E43Vf140` (0x519e20), `E09Vec4::Clear` of `out`. Eax is `out`.
pub(super) fn entity_vf140(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [_, out, ..] = r.stack;
    clear_vec4().store(&mut ctx.mem, out);
    Ok(Return::void(out, 8))
}

/// thiscall `(a, b, c)`, ret 12: `CEntity::VfD0` (0x519ea0), eax 0.
pub(super) fn entity_vf_d0(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::vf_d0(), 12))
}

/// thiscall: `CDummyObject::Vf110` (0x600c80), the dummy is present (eax 1).
pub(super) fn dummy_present(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(dummy::present() as u32, 0))
}

/// The host calls of `CEntity::E43VfD4`: the flags are read from the game's memory, and the slots are called at the depths
/// the original pushes them (`VfD8` and `VfDC` with five arguments enter 40 bytes below the entry esp, `Vf114` with none
/// enters 20 below).
struct FlagDispatchAt<'a> {
    mem: &'a Mem,
    host: &'a dyn Host,
    this: u32,
    esp: u32,
}

impl FlagDispatch for FlagDispatchAt<'_> {
    fn flags(&mut self) -> (u32, u8) {
        self.mem.entity_flags()
    }

    fn first_call(&mut self, args: [u32; 5]) -> u32 {
        let target = layout::vtable_target(self.mem, self.this, 0xd8);
        self.host.call_at(self.esp.wrapping_sub(40), target, self.this, &args).eax
    }

    fn check(&mut self, e: u32) -> u32 {
        let target = layout::vtable_target(self.mem, e, 0x114);
        self.host.call_at(self.esp.wrapping_sub(20), target, e, &[]).eax
    }

    fn second_call(&mut self, args: [u32; 5]) -> u32 {
        let target = layout::vtable_target(self.mem, self.this, 0xdc);
        self.host.call_at(self.esp.wrapping_sub(40), target, self.this, &args).eax
    }
}

/// thiscall `(a, b, c, d, e, f, g)`: `CEntity::E43VfD4` (0x519ae0), the two gated dispatches on the entity. `a` and `c` are
/// passed to the callees as the addresses of the caller's argument slots.
pub(super) fn e43_vf_d4(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [_, _, _, _, e, f, g, ..] = r.stack;
    let mut calls = FlagDispatchAt { mem: &ctx.mem, host: ctx.host, this: r.ecx, esp: r.esp };
    let eax = slots::dispatch_by_flags(&mut calls, [r.esp + 4, r.esp + 0xc, e, f, g]);
    Ok(Return::void(eax, 0x1c))
}

/// thiscall: `CGround::Vf110`, the ground is present (eax 1).
pub(super) fn ground_present(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ground::present() as u32, 0))
}

/// thiscall `(a, b)`: `CGround::VfCC`, which ignores its arguments: present (eax 1).
pub(super) fn ground_vf_cc(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ground::present() as u32, 8))
}

/// thiscall `(a, b, c, d, e)`, ret 0x14: `CGround::VfE8` (0x51c6f0), the ground's box probe (`layout::box_probe`).
pub(super) fn ground_vf_e8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, _, record, ..] = r.stack;
    let frame = layout::box_probe_frame(r.esp);
    let mut calls = ProbeCallsAt { mem: ctx.mem.clone(), host: ctx.host, frame };
    let eax = layout::box_probe(&mut ctx.mem, r.ecx, a, b, record, frame.scratch, &mut calls);
    Ok(Return::void(eax, 0x14))
}

/// The calls of `CGround::VfE8` in the emulator, each at its entry stack pointer in the frame.
struct ProbeCallsAt<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: layout::ProbeFrame,
}

impl ground::ProbeCalls for ProbeCallsAt<'_> {
    fn transform(&mut self, this: u32, out: u32, arg: u32) -> u32 {
        self.host.call_at(self.frame.fetch_sp, FETCH_X, this, &[out, arg]).eax
    }

    fn query(&mut self, host: u32, first: u32, second: u32, out: u32) -> u32 {
        let target = layout::vtable_target(&self.mem, host, 0);
        self.host.call_at(self.frame.query_sp, target, host, &[first, second, out]).eax
    }

    fn copy_vector(&mut self, to: u32, from: u32) {
        self.host.call_at(self.frame.copy_sp, E01_VEC4_COPY_FROM, to, &[from]);
    }
}

/// thiscall: the empty hook (`ret`); eax is the caller's.
pub(crate) fn empty_hook_run(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(r.eax, 0))
}

/// The flag call enters 12 bytes below the entry esp (the original's push of its float slot and its saved esi).
const LIMIT_FLAG_CALL: u32 = 12;

/// The limit update's call on the limit object (0x52f450).
struct FlagCall<'a> {
    host: &'a dyn Host,
    sp: u32,
    object: u32,
}

impl hinge_stops::Calls for FlagCall<'_> {
    fn mark_dirty(&mut self) {
        self.host.call_at(self.sp, SET_DIRTY, self.object, &[]);
    }
}

/// thiscall: `CEntity` 0x51f280, the limit update (see `rbr_physics::entities::hinge_stops`). Pops nothing.
pub(super) fn limit_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let host = ctx.host;
    let mut flag = FlagCall { host, sp: r.esp - LIMIT_FLAG_CALL, object: layout::limit_object(r.ecx) };
    let eax = layout::limit_update_at(&mut ctx.mem, r.ecx, &mut flag);
    Ok(Return::void(eax, 0))
}

/// thiscall `(index, f, b)`: the material table lookup by index (0x520240). A missing table clears only al of eax.
pub(super) fn sample_indexed_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::ngp::only()?;
    let [index, f, b, ..] = r.stack;
    let found = layout::sample_indexed(&ctx.mem, r.ecx, index as u8, f32::from_bits(f), f32::from_bits(b));
    let eax = match found {
        layout::Indexed::Texel(texel) => texel,
        layout::Indexed::NoTable => r.eax & !0xff,
        layout::Indexed::OutOfRange => 0,
    };
    Ok(Return::void(eax, 12))
}

/// thiscall, no stack arguments: `CSolid::E43ClearAcc` (0x529e60), the two accumulators cleared.
pub(super) fn clear_acc_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.clear_accumulators(r.ecx), 0))
}

/// thiscall, no stack arguments: `CEntity::E43_InitBD0` (0x519bd0), the walker marks.
pub(super) fn init_bd0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::init_bd0(&mut ctx.mem, r.ecx), 0))
}

/// thiscall `(value)`: `CEntity::E43_ExclA060` (0x51a060).
pub(super) fn remove_exclusion_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::remove_exclusion(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(a, b)`, `ret 8`: `CEntity::E43VfAC` (0x51a740), returns `a`.
pub(super) fn vf_ac(_: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(r.stack[0], 8))
}
