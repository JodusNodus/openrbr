//! The car's and body's forwarders to their members, accessors and small slots, the lights and the state object.

use rbr_physics::car_body::probe::{self};
use rbr_physics::car_body::state::StateCalls;
use rbr_physics::car_body::{self as car, layout};
use rbr_physics::math::quat;
use rbr_physics::mem::Mem;

use super::car_calls::{probe_scratch, scratch, Car, CarVf54};
use crate::collision::{BodyCallees, E01_VEC4_COPY_FROM, E37_EDGE_EDGE};
use crate::entities::COPY16_WALK;
use crate::rigid_body::VF16_HELPER;
use crate::{engine_driveline, entities, Ctx, Decline, Host, Regs, Return};
use rbr_physics::car_body::store::CarStore;

struct Clear480Host<'a>(&'a dyn Host);

impl rbr_physics::entities::layout::Clear480Calls for Clear480Host<'_> {
    fn sim_fd00(&mut self, sp: u32, body: u32) -> u32 {
        self.0.call_at(sp, 0x51fd00, body, &[]).eax
    }
}

struct Collide6000Host<'a>(&'a dyn Host);

impl layout::Collide6000Calls for Collide6000Host<'_> {
    fn grid(&mut self, sp: u32, car: u32, target: u32) -> u32 {
        self.0.call_at(sp, target, car, &[]).eax
    }

    fn edge_edge(&mut self, sp: u32, grid: u32, args: [u32; 6]) -> u32 {
        self.0.call_at(sp, E37_EDGE_EDGE, grid, &args).eax
    }
}

struct Vf128xHost<'a>(&'a dyn Host);

impl layout::Vf128xCalls for Vf128xHost<'_> {
    fn compose_a(&mut self, sp: u32, this: u32, a: u32, car: u32, out: u32) {
        self.0.call_at(sp, COPY16_WALK, this, &[a, car, out]);
    }

    fn collide6000(&mut self, sp: u32, car: u32, m: u32, f1: u32, f2: u32) {
        self.0.call_at(sp, 0x4e6000, car, &[m, f1, f2]);
    }

    fn e43vf128(&mut self, sp: u32, car: u32, args: [u32; 8]) {
        self.0.call_at(sp, crate::rigid_body::E43VF128, car, &args);
    }
}

/// thiscall `(tag)`, `ret 4`: clear matching body slots after the nested SimFD00 decision.
pub(super) fn clear480(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Clear480Host(ctx.host);
    rbr_physics::entities::layout::clear_car_bodies(&mut ctx.mem, r.ecx, r.stack[0], r.esp, &mut calls);
    Ok(Return::void(0, 4))
}

/// thiscall three words, `ret 0xc`: the edge probe along the vector, the grid's two flags out.
pub(super) fn collide6000(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, ..] = r.stack;
    let mut calls = Collide6000Host(ctx.host);
    layout::collide6000(&mut ctx.mem, r.ecx, a, b, c, r.esp, &mut calls);
    Ok(Return::void(0, 0xc))
}

/// thiscall eight words, `ret 0x20`: matrix compose, collision probe and solid impulse dispatch.
pub(super) fn vf128x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args: [u32; 8] = r.stack[..8].try_into().expect("Vf128x has eight stack words");
    let mut calls = Vf128xHost(ctx.host);
    layout::vf128x(&mut ctx.mem, r.ecx, args, r.esp, &mut calls);
    Ok(Return::void(0, 0x20))
}

/// The car's drive member and engine sim member.
const DRIVE: u32 = 0x4b0;
const ENGINE_SIM: u32 = 0x690;
/// The forwarder's `jmp` to `Sim8E20` (`0x4e6656`), which NGP retargets to its engine wrapper.
const FWD18E20_JMP: u32 = 0x4e6656;
const SIM8E20: u32 = 0x518e20;
pub(super) const FLOW_OBJECT: u32 = 0x0165fa90;
pub(super) const M5666B0: u32 = 0x5666b0;
pub(super) const SAMPLER_54A0: u32 = 0x4e54a0;

/// `r` with the member as `this`.
fn member(r: &Regs, offset: u32) -> Regs {
    Regs { ecx: r.ecx + offset, ..*r }
}

/// `add ecx, 0x4b0; jmp CDrive::M6DB0`.
pub(super) fn fwd6db0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    engine_driveline::m6db0(ctx, &member(r, DRIVE))
}

/// `add ecx, 0x4b0; jmp CDrive::M6DD0`.
pub(super) fn fwd6dd0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    engine_driveline::m6dd0(ctx, &member(r, DRIVE))
}

/// `add ecx, 0x4b0; jmp CDrive::SetTwo`.
pub(super) fn fwd43c0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    engine_driveline::set_two(ctx, &member(r, DRIVE))
}

/// `add ecx, 0x690; jmp E43EngineSim::Sim8E20`.
pub(super) fn fwd18e20(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let r = member(r, ENGINE_SIM);
    match crate::ngp::call_target(&ctx.mem, FWD18E20_JMP) {
        SIM8E20 => engine_driveline::engine_step(ctx, &r),
        target if crate::ngp::file(target) == crate::ngp::ENGINE_WRAPPER => crate::ngp::engine_wrapper(ctx, &r),
        target => {
            let called = ctx.host.call(target, r.ecx, &[r.stack[0], r.stack[1]]);
            Ok(Return::float_with_eax(called.eax, called.st0, 8))
        }
    }
}

/// Calls `helper` on `this`, then slot 17 (`+0x44`) of the object at `this + at`, as the original's tail jump does.
fn helper_then_slot17(ctx: &mut Ctx, r: &Regs, helper: u32, at: u32) -> Result<Return, Decline> {
    ctx.host.call(helper, r.ecx, &[]);
    let object = r.ecx + at;
    let slot = ctx.mem.u32(ctx.mem.u32(object) + 0x44);
    Ok(Return::void(ctx.host.call(slot, object, &[]).eax, 0))
}

/// thiscall `()`: `CCar::Vf40x`, `CSolid::Vf40` and then the slot of the object at `+0xa00`.
pub(super) fn car_vf40x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    helper_then_slot17(ctx, r, VF16_HELPER, 0xa00)
}

/// thiscall `()`: `CCarBody::Vf80x` returns 1.
pub(super) fn body_vf80x(_ctx: &mut Ctx, _r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(1, 0))
}

/// thiscall `()`: `CCarBody::Vf40x`, `CBody::Vf40` (0x4e1120) and then the slot of the state at `+0xd0`.
pub(super) fn body_vf40x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    helper_then_slot17(ctx, r, 0x4e1120, 0xd0)
}

/// thiscall `(x)`, `ret 4`.
pub(super) fn set_brake_light(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::set_brake_light(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(eax, 4))
}

/// thiscall `(x)`, `ret 4`.
pub(super) fn set_neutral_light(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::set_neutral_light(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(eax, 4))
}

/// thiscall: `CCar::Get30` returns the word at `this + 0x30`.
pub(super) fn get_30(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.get_30(r.ecx), 0))
}

/// thiscall: `CCar::Get15F8` returns the float at `this + 0x15f8` in st0.
pub(super) fn get_15f8(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(f64::from(ctx.mem.get_15f8(r.ecx)), 0))
}

/// fastcall: `E60Flow::M5666b0` (0x5666b0), the damage-select word `MotionProbe` asks the flow.
pub(super) fn flow_m5666b0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.damage_select(r.ecx), 0))
}

/// thiscall: `E60Flow::M566640` (0x566640), the flow check `Update9AF0` makes: 1 when the flow holds.
pub(super) fn flow_m566640(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.flow_holds(r.ecx), 0))
}

/// thiscall `(out*)`: `out = ToQuat(this)`, the 3x3 matrix at `this`; returns `out`.
pub(super) fn to_quat(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let out = r.stack[0];
    let q = quat::to_quat(&ctx.mem.load_mat3(r.ecx), crate::ngp::variant());
    for (k, lane) in [q.x, q.y, q.z, q.w].into_iter().enumerate() {
        ctx.mem.set_f32(out + 4 * k as u32, lane);
    }
    Ok(Return::void(out, 4))
}

/// The drive member `Vf24x` tail-jumps to: `E39_Nop3860` (0x503860, `ret 4`: it pops the one word the caller pushed).
const DRIVE_NOP: u32 = 0x503860;

/// The drive's call of `Vf24x`: the tail jump's callee, whose eax the car's `Vf24x` leaves.
struct Drive<'a> {
    host: &'a dyn Host,
    eax: u32,
}

impl car::forward::Vf24xCalls for Drive<'_> {
    fn drive(&mut self, drive: u32) {
        self.eax = self.host.call(DRIVE_NOP, drive, &[]).eax;
    }
}

/// thiscall with one unused word (`ret 4`): `Vf24x` tail-jumps to the drive member at `this+0x4b0`; the car's eax is the callee's.
pub(super) fn vf24x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut drive = Drive { host: ctx.host, eax: 0 };
    car::forward::vf24x(&mut drive, r.ecx);
    Ok(Return::void(drive.eax, 4))
}

/// thiscall `()`: `CCar::Vf54x`, the E47 scratch reset followed by slot 1 of `m_scratch1690`.
pub(super) fn reset_scratch_and_dispatch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = CarVf54 { mem: ctx.mem.clone(), host: ctx.host, car: r.ecx };
    let eax = layout::reset_scratch_and_dispatch(&mut calls, &mut ctx.mem, r.ecx)
        .unwrap_or_else(|_| unreachable!("the emulator always answers host calls"));
    Ok(Return::void(eax, 0))
}

/// thiscall `(out*)`: the car's integer surface probe (`MotionProbe`).
pub(super) fn motion_probe(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = Car { mem: ctx.mem.clone(), host: ctx.host, scratch: probe_scratch(r.esp) };
    let outcome = probe::motion_probe(&mut calls, &ctx.mem, r.ecx);
    ctx.mem.set_u32(r.stack[0], outcome.out());
    Ok(Return::void(outcome.ret(), 4))
}

/// thiscall: `this + 0xa00`, the car's state object (`StateAtb`).
pub(super) fn state_atb(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::state_address(r.ecx), 0))
}

/// thiscall: `GetB0C` returns the float at +0xb0c in st0.
pub(super) fn get_b0c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(ctx.mem.target_b0c(r.ecx) as f64, 0))
}

/// thiscall: `GetB1C` returns the float at +0xb1c in st0.
pub(super) fn get_b1c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(ctx.mem.rate_b1c(r.ecx) as f64, 0))
}

/// thiscall `(v)`: `AddADC` dirties the demand cell and adds `v`.
pub(super) fn add_adc(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::add_adc(&mut ctx.mem, r.ecx, f32::from_bits(r.stack[0]));
    Ok(Return::void(0, 4))
}

/// thiscall `(i, j, v)`: `SetA1C` dirties the matrix cell and sets it.
pub(super) fn set_a1c(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [i, j, v, ..] = r.stack;
    layout::set_a1c(&mut ctx.mem, r.ecx, i, j, f32::from_bits(v));
    Ok(Return::void(0, 12))
}

/// thiscall: `Damp66D0`, the two damper cells.
pub(super) fn update_dampers(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    car::damper::update_dampers(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(i, neg)`: `GetA48`, the lane word at `+0xa48 + 0x40 * i`, negated when `neg` is nonzero; returns it in st0.
pub(super) fn get_a48(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [i, neg, ..] = r.stack;
    let lane = ctx.mem.lane_a48(r.ecx, i);
    let value = car::accessors::lane_value(lane, neg != 0);
    Ok(Return::float(value as f64, 8))
}

/// thiscall `()`: `CCarBody::CState::Vf44x` (slot 17) sets the flag, clears the owner's grid over the bounds and copies them
/// to the state. The owner's slot 12 is its vtable's (the grid getter), and `CBody::ClearGrid` is the collision port,
/// called directly; the copies are the original's `E01Vec4::CopyFrom` on the host.
pub(super) fn state_vf44x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = StateCallees { mem: ctx.mem.clone(), host: ctx.host };
    let eax = layout::state_vf44x(&mut calls, &mut ctx.mem, r.ecx, scratch(r))
        .ok_or(Decline::Unsupported("the state has no owner (the original faults)"))?;
    Ok(Return::void(eax, 0))
}

/// The state's calls: the owner's slot 12 (a thiscall through its vtable), `CBody::ClearGrid` and `E01Vec4::CopyFrom`.
struct StateCallees<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl StateCalls for StateCallees<'_> {
    fn grid(&mut self, owner: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(owner) + 0x30);
        self.host.call(target, owner, &[]).eax
    }

    fn clear_grid(&mut self, owner: u32, grid: u32, plus: u32, minus: u32) {
        rbr_physics::collision::layout::clear_grid(&mut BodyCallees(self.host), &self.mem, owner, grid, plus, minus);
    }

    fn copy_from(&mut self, dst: u32, src: u32) -> u32 {
        self.host.call(E01_VEC4_COPY_FROM, dst, &[src]).eax
    }
}

/// thiscall `(out, n)`: `CCarBody::Vf9Cx`, subtracts the body's base from the `n` points at `out`; returns `out`.
pub(super) fn subtract_points(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    scatter(ctx, r, true)
}

fn scatter(ctx: &mut Ctx, r: &Regs, sub: bool) -> Result<Return, Decline> {
    let [out, n, ..] = r.stack;
    if n > 0 && ctx.mem.body_base(r.ecx).is_none() {
        return Err(Decline::Unsupported("the body's base is null"));
    }
    let mut calls = BodyCalls { mem: ctx.mem.clone(), host: ctx.host };
    layout::scatter_points(&mut calls, &mut ctx.mem, r.ecx, out, n, sub);
    Ok(Return::void(out, 8))
}

/// The body's slot 0 (`this`'s vtable, offset 0), a getter with no effect on the car.
struct BodyCalls<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl car::body::BodyCalls for BodyCalls<'_> {
    fn slot0(&mut self, body: u32) {
        let target = self.mem.u32(self.mem.u32(body));
        self.host.call(target, body, &[]);
    }
}

/// thiscall `()`: `NTweak::CE36SubBlock::Init` zeroes the sub-block.
pub(super) fn sub_block_init(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    car::light::sub_block_init(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// CCar::VfD8x (0x4eaf10): a naked tail jump to CBody::VfD8, so the car's slot 0xd8 takes the body's arguments and eax.
pub(super) fn car_vf_d8x(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::collision::body_vf_d8(ctx, r)
}

/// thiscall `(sig)`, `ret 4`: the load's `CMultiSignal1::Emit` instances `UpdateA720` reaches (the damage handler's init,
/// 0x4ef350; the car's init `UpdateMrr` emits, 0x4e8cb0), the physics core's emit with each connection's `Invoke`
/// entered where the original calls it (four pushes and the argument below the entry); `EMIT` is the instance. A count
/// out of range declines (the original traps).
pub(super) fn emit_init<const EMIT: u32>(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, r.ecx)?;
    let mut callees = crate::SignalCalleesAt { host: ctx.host, sp: r.esp - 0x18, name: crate::emit_name(EMIT) };
    rbr_physics::physics_core::signal::emit(&mut ctx.mem, &mut callees, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(flags)`, `ret 4`: `CCarLightSystem::Attach` (0x4f1e00), the flag block stored and its seven words set
/// (`layout::attach_lights`). Leaves eax 1.
pub(super) fn light_attach(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::attach_lights(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(1, 4))
}

/// thiscall `(a, b)`, `ret 8`: `CCar::BodyToWorldPair` (0x4e69c0), with a body (`+0x4a0`; the asm logs and breaks
/// without one, which declines). The car's and the body's slot 12 (`ret` with the pushed words left for the walk) give
/// the walk's receiver and its middle argument; then `MB560(a, car's, b)` on the body's. Returns `b`.
pub(super) fn body_to_world_pair(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let car = r.ecx;
    let body = ctx.mem.body(car);
    if body == 0 {
        return Err(Decline::Unsupported("the car has no body (the asm logs)"));
    }
    let [a, b, ..] = r.stack;
    let slot = |mem: &Mem, this: u32| mem.u32(mem.u32(this) + 0x30);
    let own = ctx.host.call(slot(&ctx.mem, car), car, &[]).eax;
    let receiver = ctx.host.call(slot(&ctx.mem, body), body, &[]).eax;
    ctx.host.call(entities::MB560, receiver, &[a, own, b]);
    Ok(Return::void(b, 8))
}
