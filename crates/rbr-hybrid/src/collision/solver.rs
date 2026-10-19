//! The collision solver and its events: contact rows, penetration correction, due events, impulses, event build.

use rbr_physics::collision::e38::{ImpulseHost, SolverHost};
use rbr_physics::collision::event::{self, PlaneDiffHost};
use rbr_physics::collision::layout::EventBodyHost;
use rbr_physics::collision::layout::{self};
use rbr_physics::collision::list;
use rbr_physics::mem::Mem;

use super::Callees;
use super::{COLL_EVENT_INIT_FULL, E36_ALLOC_EVENT};
use crate::entities::{COPY16_WALK, MB560};
use crate::rigid_body::OPERATOR_NEW;
use crate::{Ctx, Decline, Host, Regs, Return};

/// thiscall `(a, b, c, d, e)`, ret 0x14: `E37EvOps::Query4390` (0x4f4390), eax is the second body's `Vf138x` result.
pub(super) fn event_query_bodies(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, e, ..] = r.stack;
    let eax = layout::query_event_bodies(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, [a, b, c, d, e], r.esp);
    Ok(Return::void(eax, 0x14))
}

/// thiscall `(a, b, c, d)`, ret 0x10: `E37EvOps::Emit4DC0` (0x4f4dc0), eax as the original leaves it.
pub(super) fn apply_event_impulse(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, ..] = r.stack;
    let eax = layout::apply_event_impulse(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, [a, b, c, d], r.esp);
    Ok(Return::void(eax, 0x10))
}

/// E38_004FB7F0 (0x4fb7f0): thiscall on the solver; eax is the contact-row count as the asm last read it.
pub(super) fn clear_contact_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::clear_contact_rows(&mut ctx.mem, r.ecx), 0))
}

/// E38_004FBC20 (0x4fbc20): thiscall on the solver; eax is a callee's residue, which the caller does not read.
pub(super) fn refresh_events(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::refresh_events(&mut Callees(ctx.host), &mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// E38_004FBCA0 (0x4fbca0): thiscall on the solver; eax is its event count.
pub(super) fn event_count(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::event_count(&ctx.mem, r.ecx), 0))
}

/// CCollisionSolver::Vf08 (0x4fc0b0): thiscall on the solver; eax is the drain's emptied count, 0. Declines an event
/// whose release is not `CCollisionEvent::Vf1Cx`.
pub(super) fn drain_events(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::drain_events(&mut ctx.mem, r.ecx)
        .ok_or(Decline::Unsupported("an event's slot 0x1c is not CCollisionEvent::Vf1Cx"))?;
    Ok(Return::void(0, 0))
}

/// E38_004FC590X (0x4fc590): thiscall on the solver; eax is the offset-row count as the asm last read it.
pub(super) fn correct_penetrations(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = PenetrationCalls { host: ctx.host, frame: ((r.esp - 4) & !0xF) - 0x80 };
    let eax = layout::correct_penetrations(&mut calls, &mut ctx.mem, r.ecx, crate::ngp::variant());
    Ok(Return::void(eax, 0))
}

/// E38_004FCA20 (0x4fca20): thiscall on the solver; eax is a callee's residue, which no caller reads.
pub(super) fn solve_due_events(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = DueEventCalls { host: ctx.host, esp: r.esp };
    layout::solve_due_events(&mut calls, &mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

const E38_CONTACT_ROWS: u32 = 0x4fbd30;
const E38_APPLY_OFFSETS: u32 = 0x4fb890;
const E38_SOLVE_EVENT: u32 = 0x4fc1a0;

impl SolverHost for Callees<'_> {
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64 {
        self.0.call(target, body, &[]).st0
    }

    fn body_test(&mut self, target: u32, body: u32) -> u32 {
        self.0.call(target, body, &[]).eax
    }

    fn body_wake(&mut self, target: u32, body: u32) {
        self.0.call(target, body, &[]);
    }

    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32) {
        self.0.call(E38_CONTACT_ROWS, solver, &[body, event, side]);
    }

    fn apply_offsets(&mut self, row: u32) {
        self.0.call(E38_APPLY_OFFSETS, row, &[]);
    }

    fn solve_event(&mut self, solver: u32, event: u32) {
        self.0.call(E38_SOLVE_EVENT, solver, &[event]);
    }
}

/// E38_004FC590X's calls at the original's stack pointers. `frame` is its esp after the prologue (`push ebp`, the
/// 16-byte alignment, 0x74 bytes of locals, then `ebx`, `esi`, `edi`): the slot 0x13c and offset-row calls enter just
/// below it, the contact rows below their three pushed words.
struct PenetrationCalls<'a> {
    host: &'a dyn Host,
    frame: u32,
}

impl SolverHost for PenetrationCalls<'_> {
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64 {
        self.host.call_at(self.frame - 4, target, body, &[]).st0
    }

    fn body_test(&mut self, target: u32, body: u32) -> u32 {
        Callees(self.host).body_test(target, body)
    }

    fn body_wake(&mut self, target: u32, body: u32) {
        Callees(self.host).body_wake(target, body)
    }

    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32) {
        self.host.call_at(self.frame - 0x10, E38_CONTACT_ROWS, solver, &[body, event, side]);
    }

    fn apply_offsets(&mut self, row: u32) {
        self.host.call_at(self.frame - 4, E38_APPLY_OFFSETS, row, &[]);
    }

    fn solve_event(&mut self, solver: u32, event: u32) {
        Callees(self.host).solve_event(solver, event)
    }
}

/// E38_004FCA20's calls: `M_C1A0` at the original's stack pointer (`esp` is the pass's entry: `sub esp, 8` and three
/// pushes, then the event); the body slots where the harness puts them.
struct DueEventCalls<'a> {
    host: &'a dyn Host,
    esp: u32,
}

impl SolverHost for DueEventCalls<'_> {
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64 {
        Callees(self.host).inverse_mass(target, body)
    }

    fn body_test(&mut self, target: u32, body: u32) -> u32 {
        Callees(self.host).body_test(target, body)
    }

    fn body_wake(&mut self, target: u32, body: u32) {
        Callees(self.host).body_wake(target, body)
    }

    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32) {
        Callees(self.host).contact_rows(solver, body, event, side)
    }

    fn apply_offsets(&mut self, row: u32) {
        Callees(self.host).apply_offsets(row)
    }

    fn solve_event(&mut self, solver: u32, event: u32) {
        self.host.call_at(self.esp - 0x1c, E38_SOLVE_EVENT, solver, &[event]);
    }
}

pub(super) const E37_PLANE_DIFF: u32 = 0x4f40f0;
const E37_QUERY_4390: u32 = 0x4f4390;
const E37_EMIT_4DC0: u32 = 0x4f4dc0;

impl EventBodyHost for Callees<'_> {
    fn vf30(&mut self, sp: u32, target: u32, body: u32) -> u32 {
        self.0.call_at(sp, target, body, &[]).eax
    }

    fn mb560(&mut self, sp: u32, event: u32, a: u32, b: u32, c: u32) {
        self.0.call_at(sp, MB560, event, &[a, b, c]);
    }

    fn compose(&mut self, sp: u32, frame: u32, point: u32, body: u32, out: u32) {
        self.0.call_at(sp, COPY16_WALK, frame, &[point, body, out]);
    }

    fn vf138x(&mut self, sp: u32, target: u32, body: u32, args: [u32; 6]) -> u32 {
        self.0.call_at(sp, target, body, &args).eax
    }

    fn vf128x(&mut self, sp: u32, target: u32, body: u32, args: [u32; 8]) -> u32 {
        self.0.call_at(sp, target, body, &args).eax
    }
}

impl ImpulseHost for Callees<'_> {
    fn relative_velocity(&mut self, sp: u32, event: u32, out: u32) {
        self.0.call_at(sp, E37_PLANE_DIFF, event, &[out]);
    }

    fn response(&mut self, sp: u32, event: u32, direction: u32, other: u32, use_other: u32, along: u32, across: u32) {
        self.0.call_at(sp, E37_QUERY_4390, event, &[direction, other, use_other, along, across]);
    }

    fn apply_impulse(&mut self, sp: u32, event: u32, impulse: f32, slip_speed: f32, vector: u32, slip: u32) {
        let args = [impulse.to_bits(), slip_speed.to_bits(), vector, slip];
        self.0.call_at(sp, E37_EMIT_4DC0, event, &args);
    }

    fn compose(&mut self, sp: u32, frame: u32, point: u32, body: u32, out: u32) {
        self.0.call_at(sp, COPY16_WALK, frame, &[point, body, out]);
    }
}

/// E38_004FB890 (0x4fb890): thiscall on the offset row, plain ret; eax is a residue no caller reads.
pub(super) fn apply_offsets(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::apply_offsets(&mut ctx.mem, r.ecx, crate::ngp::variant());
    Ok(Return::void(0, 0))
}

/// E38Ops::M_BD30 (0x4fbd30): thiscall on the solver with `(body, event, side)`, ret 0xc; eax is 1 when the row was
/// stored.
pub(super) fn contact_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [body, event, side, ..] = r.stack;
    let eax = layout::contact_rows(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, body, event, side, r.esp);
    Ok(Return::void(eax, 12))
}

/// E38Ops::M_C1A0 (0x4fc1a0): thiscall on the solver with the event, ret 4; the impulse is the x87 result (eax is a
/// residue its caller does not read).
pub(super) fn solve_event(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let impulse = layout::solve_event(&mut Callees(ctx.host), &mut ctx.mem, r.stack[0], r.esp, crate::ngp::variant());
    Ok(Return::float(f64::from(impulse), 4))
}

/// E38_004FC500 (0x4fc500): thiscall on the solver; the fraction is the x87 result. Declines a released event whose
/// slot 0x1c is not `CCollisionEvent::Vf1Cx`.
pub(super) fn min_contact_time(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let fraction = layout::min_contact_time(&mut ctx.mem, r.ecx)
        .ok_or(Decline::Unsupported("a released event's slot 0x1c is not CCollisionEvent::Vf1Cx"))?;
    Ok(Return::float(f64::from(fraction), 0))
}

impl PlaneDiffHost for Callees<'_> {
    fn point_velocity(&mut self, sp: u32, target: u32, body: u32, point: u32, out: u32) {
        self.0.call_at(sp, target, body, &[point, out]);
    }
}

/// E37EvOps::PlaneDiff (0x4f40f0): thiscall on the event with `out`, ret 4; eax is `out`.
pub(super) fn plane_diff(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let out = r.stack[0];
    let eax = layout::relative_point_velocity(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, out, r.esp);
    Ok(Return::void(eax, 4))
}

/// cdecl with nine words: `E33MakeEvent` (0x4e0d00). Eax is the event, or 0.
pub(super) fn make_event(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2, ..] = r.stack;
    let mut calls = MakeEventCalls { host: ctx.host, s: layout::make_event_frame(r.esp) };
    let words = [kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2];
    let eax = layout::make_event_at(&mut calls, &mut ctx.mem, words, r.esp);
    Ok(Return::void(eax, 0))
}

/// E33MakeEvent's calls, at the original's stack pointers below its frame `s`: the bodies' slot 0x118 (4 below),
/// E36AllocEvent (8 below) and InitFull with the payload in the frame (0x1c below).
struct MakeEventCalls<'a> {
    host: &'a dyn Host,
    s: u32,
}

impl event::MakeEventHost for MakeEventCalls<'_> {
    fn body_grid(&mut self, mem: &Mem, body: u32) -> u32 {
        let target = layout::vtable_slot(mem, body, 0x118);
        self.host.call_at(self.s - 4, target, body, &[]).eax
    }

    fn alloc_event(&mut self, size: u32) -> u32 {
        self.host.call_at(self.s - 8, E36_ALLOC_EVENT, 0, &[size]).eax
    }

    fn init_full(
        &mut self,
        mem: &mut Mem,
        event: u32,
        kind: u32,
        body_a: u32,
        aux_a: u32,
        aux_b: u32,
        data: &[u32; 16],
    ) -> u32 {
        let payload = layout::make_event_payload(self.s);
        layout::store_payload(mem, payload, data);
        let args = [kind, body_a, aux_a, aux_b, payload, 0];
        self.host.call_at(self.s - 0x1c, COLL_EVENT_INIT_FULL, event, &args).eax
    }
}

/// cdecl `(size)`: `E36AllocEvent` (0x4f4f10), the pool's head, or `operator new` (a tail jump, at the entry esp) when
/// the pool is empty.
pub(super) fn alloc_event(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [size, ..] = r.stack;
    let mut calls = AllocAt { host: ctx.host, sp: r.esp };
    Ok(Return::void(event::alloc_event(&mut ctx.mem, size, &mut calls), 0))
}

struct AllocAt<'a> {
    host: &'a dyn Host,
    sp: u32,
}

impl event::AllocEventHost for AllocAt<'_> {
    fn operator_new(&mut self, size: u32) -> u32 {
        self.host.call_at(self.sp, OPERATOR_NEW, 0, &[size]).eax
    }
}

/// thiscall on the event with `(kind, body, point_a, point_b, payload, tag)`, ret 0x18: `CCollisionEvent::InitFull`
/// (0x4f50e0). Eax is the event.
pub(super) fn init_full(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [kind, body, point_a, point_b, payload, tag, ..] = r.stack;
    let args = [kind, body, point_a, point_b, payload, tag];
    let eax = layout::init_full(&mut Callees(ctx.host), &mut ctx.mem, r.ecx, args, r.esp);
    Ok(Return::void(eax, 0x18))
}

/// thiscall on the solver with `(event)`, ret 4: `CCollisionSolver::Vf04` (0x4fc9a0), a tail jump to
/// `E37ListOps::Insert` on its list (+4). Eax is the event.
pub(super) fn solver_add_event(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [event, ..] = r.stack;
    list::insert(&mut ctx.mem, r.ecx + 4, event);
    Ok(Return::void(event, 4))
}
