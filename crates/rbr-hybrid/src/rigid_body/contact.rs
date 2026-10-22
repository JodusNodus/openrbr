//! The contacts: the vote and `BigA` (`rbr_physics::rigid_body::{vote, contact}`), and the contact list step `BigB` and
//! the force accumulate.

use rbr_physics::mem::Mem;
use rbr_physics::rigid_body::contact::{ContactHost, Impulse, SampleHook};
use rbr_physics::rigid_body::contact_list::{contact_list_step, Event, ListHost, Record};
use rbr_physics::rigid_body::force::accumulate_force;
use rbr_physics::rigid_body::host::{BodyCalls, HostUnavailable};
use rbr_physics::rigid_body::layout::{self, Body, Vec4};
use rbr_physics::rigid_body::vote::ContactVoteHost;

use super::force_pass::{SLOT_RESPONSE, SLOT_S30};
use super::slots::Flush;
use super::{
    unavailable, BIG_A, COLLECT_OBJECT, COPY16_WALK, E36_COLLECT, MB560, RETIRE_A, SAMPLE, SCRATCH_BELOW_ESP, SIGNALS,
    SLOT_SAMPLE_HOOK, WALKER,
};
use crate::timekeeper::layout::APP;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

/// The vote's `BigA` calls, at the original's stack: `frame` is the vote's aligned locals after its callee-saves (the pair
/// at +0x20 and +0x30, the force at +0x1c, the depth at +0x18), `range` its third argument.
struct VoteCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    frame: u32,
    range: u32,
}

impl ContactVoteHost for VoteCalls<'_> {
    fn contact(&mut self, k: u32) -> Result<Option<Impulse>, HostUnavailable> {
        let e = layout::contact_record(&self.mem, self.this, k);
        let (a, b) = (self.frame + 0x20, self.frame + 0x30);
        let args = [a, b, self.frame + 0x1c, self.frame + 0x18, e, self.range];
        let hit = self.host.call_at(self.frame - 0x1c, BIG_A, self.this, &args).eax != 0;
        Ok(hit.then(|| Impulse { force: Vec4::load(&self.mem, a), torque: Vec4::load(&self.mem, b) }))
    }
}

/// thiscall `(a, b, range)`, eax: the contact vote over the body's list (`BigA` per record, at the original's stack).
pub(super) fn vf81x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, range, ..] = r.stack;
    let mut calls =
        VoteCalls { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, frame: aligned_frame(r.esp) - 0x40, range };
    let eax = layout::contact_vote(&mut ctx.mem, r.ecx, a, b, &mut calls).expect("the host answers every call");
    Ok(Return::void(eax, 12))
}

/// `BigA`'s callees at the original's stack: `frame` is its aligned locals after its callee-saves (the frame point at
/// +0x40, the response at +0x20, the transform's source at +0x60, the offset at +0x30, the sample's words at +0x14 and
/// +0x18), `e` the contact and `a` the pair's first output.
struct ContactCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    frame: u32,
    e: u32,
    a: u32,
}

impl BodyCalls for ContactCalls<'_> {
    /// Slot 80 `(frame point*, out*)`.
    fn response(&mut self, _point: Vec4) -> Vec4 {
        let target = layout::vtable_target(&self.mem, self.this, SLOT_RESPONSE);
        let args = [self.frame + 0x40, self.frame + 0x20];
        self.host.call_at(self.frame - 0xc, target, self.this, &args);
        Vec4::load(&self.mem, self.frame + 0x20)
    }

    /// Slot 12 (`S30`, `(a*)`), then `MB560 (src*, the body, a*)` on the walker.
    fn transform(&mut self, src: Vec4) -> Vec4 {
        let at = self.frame + 0x60;
        src.store(&mut self.mem, at);
        let target = layout::vtable_target(&self.mem, self.this, SLOT_S30);
        let body = self.host.call_at(self.frame - 8, target, self.this, &[self.a]).eax;
        let walker = self.mem.u32(WALKER);
        self.host.call_at(self.frame - 0x10, MB560, walker, &[at, body, self.a]);
        Vec4::load(&self.mem, self.a)
    }
}

impl ContactHost for ContactCalls<'_> {
    /// `ComposeA (e+0x40, walker, frame point*)` on the body.
    fn compose(&mut self, _vector: Vec4) -> Vec4 {
        let walker = self.mem.u32(WALKER);
        let args = [self.e + 0x40, walker, self.frame + 0x40];
        self.host.call_at(self.frame - 0x10, COPY16_WALK, self.this, &args);
        Vec4::load(&self.mem, self.frame + 0x40)
    }

    fn sample(&mut self) -> Result<(u32, u32), HostUnavailable> {
        let (first, second) = (self.frame + 0x14, self.frame + 0x18);
        self.host.call_at(self.frame - 0x10, SAMPLE, self.this, &[self.e + 0x40, first, second]);
        Ok((self.mem.u32(first), self.mem.u32(second)))
    }

    /// The offset's lanes x, y, z go to `frame + 0x30` (its lane w is the stack's, as the original's).
    fn sample_hook(&mut self, h: &SampleHook) -> Result<(), HostUnavailable> {
        let offset = self.frame + 0x30;
        for (k, v) in [h.offset.x, h.offset.y, h.offset.z].into_iter().enumerate() {
            self.mem.set_f32(offset + 4 * k as u32, v);
        }
        let hook = self.mem.u32(self.this + 0x3cc);
        let target = layout::vtable_target(&self.mem, hook, SLOT_SAMPLE_HOOK);
        let args =
            [h.first, h.second, h.id, h.length.to_bits(), h.reach.to_bits(), self.e + 0x40, self.frame + 0x40, offset];
        self.host.call_at(self.frame - 0x24, target, hook, &args);
        Ok(())
    }
}

/// thiscall `(a, b, force, depth, e, range)`, eax: the contact worker (`CSolid::BigA`), its calls at the original's stack.
pub(super) fn big_a(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, force, depth, e, range, ..] = r.stack;
    let mut calls =
        ContactCalls { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, frame: aligned_frame(r.esp) - 0x70, e, a };
    let out = layout::ContactOut { force_out: a, torque_out: b, force, depth };
    let eax = layout::contact_worker(&mut ctx.mem, r.ecx, out, e, f32::from_bits(range), &mut calls)
        .expect("the host answers every call");
    Ok(Return::void(eax, 0x18))
}

/// The calls the contact list step makes outside the body (`BodyCalls`, `ListHost`), as the original makes them. The
/// body is not changed by the step, so the calls read it in memory. The list and the other temps are in scratch memory
/// below the call.
struct ListCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    scratch: u32,
}

impl BodyCalls for ListCalls<'_> {
    fn response(&mut self, point: Vec4) -> Vec4 {
        let (at, out) = (self.scratch + 0x50, self.scratch + 0x40);
        point.store(&mut self.mem, at);
        let slot80 = self.mem.u32(self.mem.u32(self.this) + 4 * 80);
        self.host.call(slot80, self.this, &[at, out]);
        Vec4::load(&self.mem, out)
    }

    fn transform(&mut self, src: Vec4) -> Vec4 {
        let (a, c) = (self.scratch + 0x60, self.scratch + 0x70);
        src.store(&mut self.mem, a);
        let slot12 = self.mem.u32(self.mem.u32(self.this) + 4 * 12);
        self.host.call(slot12, self.this, &[]);
        self.host.call(MB560, self.mem.u32(WALKER), &[a, self.this, c]);
        Vec4::load(&self.mem, c)
    }
}

impl ListHost for ListCalls<'_> {
    fn collect(&mut self) -> Result<Option<Vec<Record>>, HostUnavailable> {
        let (list, count) = (self.scratch + 0xe0, self.scratch + 0xe4);
        let object = self.mem.u32(COLLECT_OBJECT);
        let found = self.host.call(E36_COLLECT, object, &[self.this, list, count]).eax;
        if found == 0 {
            return Ok(None);
        }
        let records = layout::contact_records(&self.mem, self.mem.u32(list), self.mem.u32(count));
        Ok(Some(records))
    }

    fn retire(&mut self) -> Result<(), HostUnavailable> {
        self.host.call(RETIRE_A, self.this, &[]);
        Ok(())
    }

    fn apply_force(&mut self, force: Vec4) -> Result<(), HostUnavailable> {
        self.apply(28, force, 0x80);
        Ok(())
    }

    fn apply_torque(&mut self, torque: Vec4) -> Result<(), HostUnavailable> {
        self.apply(29, torque, 0x90);
        Ok(())
    }

    fn emit(&mut self, event: &Event) -> Result<(), HostUnavailable> {
        let at = self.scratch + 0xa0;
        layout::store_event(&mut self.mem, at, event);
        let signal = self.mem.u32(SIGNALS) + 0xadc8;
        // BigB's call (0x52c500) of the emit, which NGP retargets (to 0x100edbf0)
        let target = crate::ngp::call_target(&self.mem, 0x52C500);
        self.host.call(target, signal, &[at]);
        Ok(())
    }
}

impl ListCalls<'_> {
    /// The body's slot `slot` with the vector at scratch `at` (the applies: the accumulator's, and the flush's).
    fn apply(&mut self, slot: u32, v: Vec4, at: u32) {
        let at = self.scratch + at;
        v.store(&mut self.mem, at);
        let target = self.mem.u32(self.mem.u32(self.this) + 4 * slot);
        self.host.call(target, self.this, &[at]);
    }
}

/// thiscall, no stack arguments: the contact list step (`CSolid::BigB`). The list comes from `E36Collect` (collision's
/// binding, when it is bound); the step's calls on the body go through the original.
pub(super) fn big_b(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let body = Body::load(&ctx.mem, r.ecx);
    let mut calls = ListCalls { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, scratch: r.esp - SCRATCH_BELOW_ESP };
    contact_list_step(&body, &mut calls).map_err(unavailable)?;
    Ok(Return::void(0, 0))
}

/// thiscall `(pp)`: the force accumulate (`CSolid::E43Vf70`, slot 28), then the flush under `E43Vf74`'s gate. eax is the
/// flush's result when it runs, else the app pointer the gate test left.
pub(super) fn e43vf70(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let app = ctx.mem.u32(APP);
    let flags = ctx.mem.u32(app + 0x38) | ctx.mem.u32(ctx.mem.u32(app + 0x24) + 0x160f4);
    let pp = Vec4::load(&ctx.mem, r.stack[0]);
    let mut body = Body::load(&ctx.mem, r.ecx);
    let mut flush = Flush { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, eax: None };
    accumulate_force(&mut body, pp, flags, &mut flush).map_err(unavailable)?;
    body.store(&mut ctx.mem, r.ecx);
    Ok(Return::void(flush.eax.unwrap_or(app), 4))
}

// The quaternion helpers.
