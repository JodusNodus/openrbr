//! The force pass (`CSolid::E43Vf64`) with its own contact list step, and the response (`E43Vf140`, `CMountedRigidBody::Vf128`).

use rbr_physics::mem::Mem;
use rbr_physics::rigid_body::contact_list::{Event, ListHost, Record};
use rbr_physics::rigid_body::force::{force_pass, ForceHost};
use rbr_physics::rigid_body::host::{BodyCalls, HostUnavailable};
use rbr_physics::rigid_body::layout::{self, Body, Def, Vec4};
use rbr_physics::rigid_body::response::ResponseHost;

use super::slots::impulse_run;
use super::{COLLECT_OBJECT, COPY16_WALK, DAMPING_EMIT, E36_COLLECT, MB560, RETIRE_A, SIGNALS, VF81X, WALKER};
use crate::entities::VF64;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

// The force pass, with its own contact list step (its transforms differ from `BigB`'s).

/// The body's virtual slots, as vtable offsets: slot 12 (`S30`, returns the body), slots 28 and 29 (the force and torque
/// applies) and slot 80 (the response to a point).
pub(super) const SLOT_S30: u32 = 0x30;
const SLOT_APPLY_FORCE: u32 = 0x70;
const SLOT_APPLY_TORQUE: u32 = 0x74;
pub(super) const SLOT_RESPONSE: u32 = 0x140;

/// The temporaries' offsets from [`Calls::temp`].
const SRC: u32 = 0x00;
const DST: u32 = 0x10;
const APPLY: u32 = 0x20;
const LIST_CELL: u32 = 0x30;
const COUNT_CELL: u32 = 0x34;
const EVENT: u32 = 0x40;
/// Bytes below the entry the temporaries take (the host calls' frames start 0x1000 below).
const TEMPS: u32 = 0x100;

/// The parent path in `CSolid::E43Vf140`. The original's frame is the aligned local area after its three callee-saves;
/// all nested calls use the exact stack slots and scratch vectors from that frame.
struct ResponseCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    point: u32,
    output: u32,
    frame: u32,
}

impl ResponseHost for ResponseCalls<'_> {
    fn parent_response(&mut self, parent: u32, _point: Vec4) -> Vec4 {
        let target = layout::vtable_target(&self.mem, parent, SLOT_RESPONSE);
        self.host.call_at(self.frame.wrapping_sub(12), target, parent, &[self.point, self.output]);
        Vec4::load(&self.mem, self.output)
    }

    fn compose(&mut self, parent: u32, _point: Vec4) -> Vec4 {
        let output = self.frame + 0x10;
        let walker = self.mem.u32(WALKER);
        self.host.call_at(self.frame.wrapping_sub(16), COPY16_WALK, walker, &[self.point, parent, output]);
        Vec4::load(&self.mem, output)
    }

    fn parent_transform(&mut self, parent: u32, value: Vec4) -> Vec4 {
        let input = self.frame + 0x20;
        let output = self.frame + 0x30;
        let walker = self.mem.u32(WALKER);
        value.store(&mut self.mem, input);
        self.host.call_at(self.frame.wrapping_sub(16), MB560, parent, &[input, walker, output]);
        Vec4::load(&self.mem, output)
    }
}

/// thiscall eight words, `ret 0x20`: `CMountedRigidBody::Vf128`. A body without an owner (`+0x3d0`) jumps to
/// `CSolid::E43Vf128`; an owned one returns at once, with the owner in eax.
pub(super) fn mounted_vf128(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let owner = ctx.mem.u32(r.ecx + 0x3d0);
    if owner == 0 {
        return impulse_run(ctx, r);
    }
    Ok(Return::void(owner, 0x20))
}

/// thiscall `(point*, out*)`: velocity at the point; the parent path follows the original's nested calls and frame slots.
pub(super) fn response_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [point, output, ..] = r.stack;
    let aligned = aligned_frame(r.esp);
    let frame = aligned.wrapping_sub(0x34).wrapping_sub(12);
    let mut calls = ResponseCalls { mem: ctx.mem.clone(), host: ctx.host, point, output, frame };
    let eax = layout::solid_response(&mut ctx.mem, r.ecx, point, output, &mut calls);
    Ok(Return::void(eax, 8))
}

/// thiscall `()`: the force pass. The entity walk, its first call, runs before the body is read, since the children's
/// passes it runs may apply forces to the body. The pass's own calls are at the original's stack (`Calls::frame`); `BigB`'s
/// use the temporaries.
pub(super) fn e43vf64(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp) - 0x50;
    ctx.host.call_at(frame - 4, VF64, r.ecx, &[]);
    let def = Def::load(&ctx.mem, ctx.mem.u32(r.ecx + 0x34));
    let mut body = Body::load(&ctx.mem, r.ecx);
    let transforms = if body.body_flags & layout::BODY_FLAG_8 != 0 { Transforms::List } else { Transforms::Force(0) };
    let mut host = Calls::new(ctx, r, frame, transforms);
    force_pass(&mut body, &def, &mut host).expect("the host answers every call");
    body.store(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// A virtual call on `this`: the function at `vtable + slot`.
fn vcall(mem: &Mem, host: &dyn Host, this: u32, slot: u32, args: &[u32]) -> crate::Called {
    vcall_at(mem, host, 0, this, slot, args)
}

/// [`vcall`] entered at `sp`.
pub(super) fn vcall_at(mem: &Mem, host: &dyn Host, sp: u32, this: u32, slot: u32, args: &[u32]) -> crate::Called {
    host.call_at(sp, mem.u32(mem.u32(this) + slot), this, args)
}

/// Which transform the body's `transform` call is. `BigB`'s go through the walker with the body as the matrix, into a
/// temporary. The force pass's own (after `BigB`) go through the body with the walker as the matrix, straight into the
/// linear (`+0x380`, the first) and the angular acceleration (`+0x320`, the second).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Transforms {
    List,
    Force(u32),
}

/// The original callees of `BigB` and the force pass, run in the emulator. `frame` is the pass's aligned locals after its
/// callee-saves (the vote's two vectors at +0x10 and +0x20, the transforms' sources at +0x30 and +0x40); `voted` is set
/// from a vote hit to the read after its applies.
struct Calls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    body: u32,
    temp: u32,
    frame: u32,
    transforms: Transforms,
    voted: bool,
}

impl<'a> Calls<'a> {
    fn new(ctx: &Ctx<'a>, r: &Regs, frame: u32, transforms: Transforms) -> Self {
        Calls {
            mem: ctx.mem.clone(),
            host: ctx.host,
            body: r.ecx,
            temp: (r.esp - TEMPS) & !0xf,
            frame,
            transforms,
            voted: false,
        }
    }

    /// A slot-28 or slot-29 apply of `v`: the vote's from its vector at `at` in the pass's frame, `BigB`'s from a temporary.
    fn apply(&mut self, slot: u32, v: Vec4, at: u32) {
        if self.voted {
            let target = layout::vtable_target(&self.mem, self.body, slot);
            self.host.call_at(self.frame - 8, target, self.body, &[self.frame + at]);
        } else {
            let at = self.put(APPLY, v);
            self.vcall(slot, &[at]);
        }
    }

    fn put(&mut self, at: u32, v: Vec4) -> u32 {
        v.store(&mut self.mem, self.temp + at);
        self.temp + at
    }

    fn vcall(&self, slot: u32, args: &[u32]) -> crate::Called {
        vcall(&self.mem, self.host, self.body, slot, args)
    }
}

impl BodyCalls for Calls<'_> {
    /// Slot 80 `(point*, out*)`: the response, three floats into `out` (its fourth lane is the temporary's).
    fn response(&mut self, point: Vec4) -> Vec4 {
        let src = self.put(SRC, point);
        let out = self.temp + DST;
        self.vcall(SLOT_RESPONSE, &[src, out]);
        Vec4::load(&self.mem, out)
    }

    /// Slot 12 (`S30`) on the body, then `MB560 (src*, matrix, out*)`; the pass's own at its frame.
    fn transform(&mut self, v: Vec4) -> Vec4 {
        let walker = self.mem.u32(WALKER);
        let out = match self.transforms {
            Transforms::List => {
                let src = self.put(SRC, v);
                let out = self.temp + DST;
                let body = self.vcall(SLOT_S30, &[out]).eax;
                self.host.call(MB560, walker, &[src, body, out]);
                out
            }
            Transforms::Force(k) => {
                let out = self.body + if k == 0 { 0x380 } else { 0x320 };
                self.transforms = Transforms::Force(k + 1);
                let src = self.frame + 0x30 + 0x10 * k;
                v.store(&mut self.mem, src);
                let target = layout::vtable_target(&self.mem, self.body, SLOT_S30);
                let args = [src, walker, out];
                let body = self.host.call_at(self.frame - 0x10, target, self.body, &args).eax;
                self.host.call_at(self.frame - 0x10, MB560, body, &args);
                out
            }
        };
        Vec4::load(&self.mem, out)
    }
}

impl ListHost for Calls<'_> {
    /// `E36Collect (body, list**, count*)` on the collision world; the list's 0x40-byte records.
    fn collect(&mut self) -> Result<Option<Vec<Record>>, HostUnavailable> {
        let (list_cell, count_cell) = (self.temp + LIST_CELL, self.temp + COUNT_CELL);
        self.mem.set_u32(list_cell, 0);
        self.mem.set_u32(count_cell, 0);
        let world = self.mem.u32(COLLECT_OBJECT);
        if self.host.call(E36_COLLECT, world, &[self.body, list_cell, count_cell]).eax == 0 {
            return Ok(None);
        }
        let list = self.mem.u32(list_cell);
        Ok(Some(layout::contact_records(&self.mem, list, self.mem.u32(count_cell))))
    }

    fn retire(&mut self) -> Result<(), HostUnavailable> {
        self.host.call(RETIRE_A, self.body, &[]);
        Ok(())
    }

    fn apply_force(&mut self, force: Vec4) -> Result<(), HostUnavailable> {
        self.apply(SLOT_APPLY_FORCE, force, 0x10);
        Ok(())
    }

    fn apply_torque(&mut self, torque: Vec4) -> Result<(), HostUnavailable> {
        self.apply(SLOT_APPLY_TORQUE, torque, 0x20);
        Ok(())
    }

    /// The DampingCollision `Emit (event*)`. The event's words at +0x08 and +0x40 are never set (the original's are
    /// its stack's).
    fn emit(&mut self, event: &Event) -> Result<(), HostUnavailable> {
        let at = self.temp + EVENT;
        self.mem.set_f32(at, event.weight);
        self.mem.set_u32(at + 0x4, event.kind);
        event.aux.store(&mut self.mem, at + 0x10);
        event.contact.store(&mut self.mem, at + 0x20);
        event.point.store(&mut self.mem, at + 0x30);
        let signal = self.mem.u32(SIGNALS) + 0xadc8;
        self.host.call(DAMPING_EMIT, signal, &[at]);
        Ok(())
    }
}

impl ForceHost for Calls<'_> {
    /// Made by the binding before the body is read.
    fn entity_walk(&mut self) {}

    /// `Vf81x (frame+0x10, frame+0x20, 1.0)`.
    fn vote(&mut self) -> Result<Option<(Vec4, Vec4)>, HostUnavailable> {
        let (a, b) = (self.frame + 0x10, self.frame + 0x20);
        let args = [a, b, 1.0f32.to_bits()];
        self.voted = self.host.call_at(self.frame - 0x10, VF81X, self.body, &args).eax != 0;
        Ok(self.voted.then(|| (Vec4::load(&self.mem, a), Vec4::load(&self.mem, b))))
    }

    /// The body after the vote's applies (`BigB`, when the body has flag 8, comes next), or after `BigB`'s (the transforms
    /// after it are the pass's own).
    fn reread(&mut self) -> Body {
        if self.voted {
            self.voted = false;
        } else {
            self.transforms = Transforms::Force(0);
        }
        Body::load(&self.mem, self.body)
    }
}
