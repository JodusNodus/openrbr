//! `CSolid`'s small slots: the point velocity and accessors, the impulse (`E43Vf128`), the ring append, the damping
//! emit, the definition and state constructors, the scratch flush and sleep test, and the row transforms.

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::{layout as core_layout, signal};
use rbr_physics::rigid_body::host::HostUnavailable;
use rbr_physics::rigid_body::impulse::{self, ImpulseHost};
use rbr_physics::rigid_body::layout::{self, Body, Def, Vec4};
use rbr_physics::rigid_body::scratch::FlushHost;
use rbr_physics::rigid_body::sleep;

use super::force_pass::{vcall_at, SLOT_S30};
use super::{unavailable, Original, COPY16_WALK, MB560, RING_APPEND, SLOT_HOOK, WALKER};
use crate::timekeeper::layout::APP;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

/// thiscall `(point*, out*)`, `ret 8`: `CSolid::Step76`, the velocity of a point on the body (`layout::point_velocity`).
pub(super) fn step76(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::point_velocity(&mut ctx.mem, r.ecx, r.stack[0], r.stack[1]);
    Ok(Return::void(0, 8))
}

/// fastcall `()`: `CSolid::Vf85`, the definition's flag as eax (0 or 1).
pub(super) fn vf85(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(u32::from(layout::def_flag(&ctx.mem, r.ecx)), 0))
}

/// thiscall `()`: the inverse-mass accessor through the body's definition, returned in st0.
pub(super) fn inverse_mass(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(f64::from(layout::solid_inverse_mass(&ctx.mem, r.ecx)), 0))
}

/// thiscall `(a, b, c, dp, e*, f*)`, `ret 0x18`: `CSolid::E43Vf138`, the two dot products (`rbr_physics::rigid_body::dot`).
/// Its eax is `e`: the last load before the pops is `mov eax, [esp+0x20]` (0x52b9d0), the fifth stack word.
pub(crate) fn e43vf138(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, dp, e, f, ..] = r.stack;
    layout::e43vf138(&mut ctx.mem, r.ecx, [a, b, c, dp, e, f], crate::ngp::variant());
    Ok(Return::void(e, 0x18))
}

/// The original callees of `E43Vf128`, in the emulator, at the original's stack: the two transforms (`S30`, the body's
/// pose, and `MB560`), then the tail's (`layout::impulse_tail`). `frame` is the original's esp after its prologue.
struct ImpulseCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
    body: u32,
    frame: u32,
}

impl ImpulseHost for ImpulseCalls<'_> {
    /// `S30 (out*)`, then `MB560 (src*, pose, out*)` with the walker as ecx.
    fn transform_by_body(&mut self, v: Vec4) -> Vec4 {
        let (src, out) = (self.frame + 0x40, self.frame + 0x30);
        v.store(&mut self.mem, src);
        let pose = vcall_at(&self.mem, self.host, self.frame - 8, self.body, SLOT_S30, &[out]).eax;
        let walker = self.mem.u32(WALKER);
        self.host.call_at(self.frame - 0x10, MB560, walker, &[src, pose, out]);
        Vec4::load(&self.mem, out)
    }

    /// `S30 (src*, walker, out*)`, then `MB560 (src*, walker, out*)` with the pose as ecx.
    fn transform_by_walker(&mut self, v: Vec4) -> Vec4 {
        let (src, out) = (self.frame + 0x50, self.frame + 0x60);
        v.store(&mut self.mem, src);
        let walker = self.mem.u32(WALKER);
        let args = [src, walker, out];
        let pose = vcall_at(&self.mem, self.host, self.frame - 0x10, self.body, SLOT_S30, &args).eax;
        self.host.call_at(self.frame - 0x10, MB560, pose, &args);
        Vec4::load(&self.mem, out)
    }
}

impl layout::ImpulseTailCalls for ImpulseCalls<'_> {
    fn pose(&mut self, mem: &mut Mem, sp: u32, body: u32, pushed: u32) -> u32 {
        vcall_at(mem, self.host, sp, body, SLOT_S30, &[pushed]).eax
    }

    fn compose_a(&mut self, mem: &mut Mem, sp: u32, point: u32, matrix: u32, out: u32) {
        self.host.call_at(sp, COPY16_WALK, mem.u32(WALKER), &[point, matrix, out]);
    }

    fn hook(&mut self, mem: &mut Mem, sp: u32, hook: u32, args: [u32; 8]) {
        vcall_at(mem, self.host, sp, hook, SLOT_HOOK, &args);
    }

    fn ring_append(&mut self, _: &mut Mem, sp: u32, ring: u32, args: [u32; 4]) {
        self.host.call_at(sp, RING_APPEND, ring, &args);
    }
}

/// thiscall `(point*, impulse*, c, d, e*, f, g, h)`, `ret 0x20`: `CSolid::E43Vf128`, the impulse at the point, then the
/// hook and the ring tail (`layout::impulse_tail`).
pub(crate) fn impulse_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args: [u32; 8] = r.stack[..8].try_into().expect("E43Vf128 has eight stack words");
    let [point, impulse, ..] = args;
    let def = Def::load(&ctx.mem, ctx.mem.u32(r.ecx + 0x34));
    let mut body = Body::load(&ctx.mem, r.ecx);
    let mut calls =
        ImpulseCalls { mem: ctx.mem.clone(), host: ctx.host, body: r.ecx, frame: aligned_frame(r.esp) - 0x80 };
    impulse::impulse(&mut body, &def, Vec4::load(&ctx.mem, point), Vec4::load(&ctx.mem, impulse), &mut calls);
    body.store(&mut ctx.mem, r.ecx);
    let frame = calls.frame;
    layout::impulse_tail(&mut ctx.mem, r.ecx, args, frame, &mut calls);
    Ok(Return::void(0, 0x20))
}

/// thiscall `(src*, a, b, c)`, `ret 0x10`: `E46_RingAppend`, a ring tail's append (`layout::ring_append`).
pub(super) fn ring_append(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [src, a, b, c, ..] = r.stack;
    Ok(Return::void(layout::ring_append(&mut ctx.mem, r.ecx, src, [a, b, c]), 0x10))
}

/// thiscall `(rows, count)`: subtract the current position, then transform the rows by the current matrix.
pub(super) fn e43vfa0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    Ok(Return::void(layout::solid_e43vfa0(&mut ctx.mem, r.ecx, rows, count), 8))
}

/// thiscall `(dir*, penetration, unused, mu)`: the brake-wall impulse's four-lane accumulation.
pub(super) fn clamp_velocity_along(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dir, penetration, _unused, mu, ..] = r.stack;
    let eax = layout::clamp_velocity_along(&mut ctx.mem, r.ecx, dir, f32::from_bits(penetration), f32::from_bits(mu));
    Ok(Return::void(eax, 16))
}

/// The connection calls and error paths of `CMultiSignal1<CSigDampingCollision>::Emit`.
struct DampingEmit<'a> {
    host: &'a dyn Host,
    sp: u32,
    name: u32,
    eax: u32,
}

impl signal::SignalHost for DampingEmit<'_> {
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32) {
        let target = core_layout::vtable_target(mem, conn, 4);
        self.eax = self.host.call_at(self.sp, target, conn, &[sig]).eax;
    }

    fn log_recursive(&mut self) {
        let log = crate::SignalCalleesAt { host: self.host, sp: self.sp, name: self.name };
        self.eax = log.log_recursive_eax();
    }

    fn log_range(&mut self, _count: u32) {
        unreachable!("the binding declines out-of-range damping emits (the original traps)")
    }
}

/// thiscall `(event*)`: the `CMultiSignal1` emit at `this` (the damping-collision signal's, `CSolid::Emit`, and the body
/// event's, 0x51f7c0: the same template); its listeners are invoked in list order.
pub(super) fn damping_emit<const EMIT: u32>(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let signal_at = r.ecx;
    crate::decline_emit_trap(&ctx.mem, signal_at)?;
    let mut emit = DampingEmit { host: ctx.host, sp: r.esp - 24, name: crate::emit_name(EMIT), eax: 0 };
    signal::emit(&mut ctx.mem, &mut emit, signal_at, r.stack[0]);
    Ok(Return::void(emit.eax, 4))
}

/// cdecl `(outer)`, eax: the definition block, or 0 when the allocation fails (the constructor is not run then).
pub(super) fn new_def(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::new_def(&mut ctx.mem, r.stack[0], &mut Original(ctx.host));
    Ok(Return::void(eax, 0))
}

/// thiscall `(arg)`, eax: the slot-8 jump to the entity walk on the body, which pops the argument.
pub(super) fn vf20_jump(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::entity_vf20_jump(r.ecx, r.stack[0], &mut Original(ctx.host));
    Ok(Return::void(eax, 4))
}

/// thiscall `(owner, name, unused)`, eax = this: the state's constructor, the base and the tweak members built in place.
pub(super) fn state_ctor(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [owner, name, ..] = r.stack;
    layout::construct_state(&mut ctx.mem, r.ecx, owner, name);
    Ok(Return::void(r.ecx, 12))
}

/// The flush `E43Vf74` makes: the body's slot 0 (`E34Call0_0::S00`), a thiscall on the body through its vtable.
/// `eax` is the slot's result when it runs.
pub(super) struct Flush<'a> {
    pub(super) mem: Mem,
    pub(super) host: &'a dyn Host,
    pub(super) this: u32,
    pub(super) eax: Option<u32>,
}

impl FlushHost for Flush<'_> {
    fn flush(&mut self) -> Result<(), HostUnavailable> {
        let target = self.mem.u32(self.mem.u32(self.this));
        self.eax = Some(self.host.call(target, self.this, &[]).eax);
        Ok(())
    }
}

/// thiscall `(pp)`: the scratch accumulate (`CSolid::E43Vf74`), then the flush when the app's flags have bit 0x4000.
pub(super) fn e43vf74(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let app = ctx.mem.u32(APP);
    let flags = ctx.mem.u32(app + 0x38) | ctx.mem.u32(ctx.mem.u32(app + 0x24) + 0x160f4);
    let mut flush = Flush { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, eax: None };
    layout::scratch_accumulate(&mut ctx.mem, r.ecx, r.stack[0], flags, &mut flush).map_err(unavailable)?;
    Ok(Return::void(0, 4))
}

/// thiscall `(dt)`, eax = whether the body slept: the sleep test. On a trip the body's slot 37 (`CSolid::E43Vf94`) runs
/// through the host, after the body is stored.
pub(super) fn vf24x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let this = r.ecx;
    let def = Def::load(&ctx.mem, ctx.mem.u32(this + 0x34));
    let mut body = Body::load(&ctx.mem, this);
    let slept = sleep::step(&mut body, &def, f32::from_bits(r.stack[0]));
    body.store(&mut ctx.mem, this);
    if slept {
        let slot37 = ctx.mem.u32(ctx.mem.u32(this) + 4 * 37);
        ctx.host.call(slot37, this, &[]);
    }
    Ok(Return::void(slept as u32, 4))
}

/// thiscall `(rows, count)`: the body's matrix times the rows, stored back in place (`CSolid::E43VfAC`). The port makes no
/// original call.
pub(super) fn transform_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::transform_body_rows(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(0, 8))
}

/// thiscall `(rows, count)`: the three-lane variant (`CSolid::E43VfB0`).
pub(super) fn transform_rows3(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::transform_body_rows3(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(0, 8))
}

/// thiscall `(rows, count)`: the start quaternion plus the start position (`CSolid::E43VfA4`).
pub(super) fn quat_plus_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::quat_plus_body_rows(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(rows, 8))
}

/// thiscall `(rows, count)`: rows in the start pose's local frame (`CSolid::E43VfA8`).
pub(super) fn quat_diff_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::quat_diff_body_rows(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(rows, 8))
}

/// thiscall `(rows, count)`: transform by the start quaternion (`CSolid::E43VfB4`).
pub(super) fn quat_transform_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::quat_transform_body_rows(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(rows, 8))
}

/// thiscall `(rows, count)`: the alternate quaternion row order (`CSolid::E43VfB8`).
pub(super) fn quat_rows_b8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::quat_body_rows_b8(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(rows, 8))
}

/// thiscall `(rows, count)`, eax `rows`: the origin folded into the fourth lane (`CSolid::E43VfBC`).
pub(super) fn fold_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, count, ..] = r.stack;
    layout::fold_body_rows(&mut ctx.mem, r.ecx, rows, count);
    Ok(Return::void(rows, 8))
}

/// thiscall `(rows, n)`, eax: the matrix scales the rows, then the origin is added to them (`CSolid::E43Vf9C`). The
/// port makes no original call; eax is `rows`.
pub(super) fn scale_add_rows(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [rows, n, ..] = r.stack;
    let eax = layout::scale_add_rows(&mut ctx.mem, r.ecx, rows, n);
    Ok(Return::void(eax, 8))
}
