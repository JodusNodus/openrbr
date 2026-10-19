//! [`Car`]: the original's callees a car-body binding makes (walks, compose, edges, drag, probes), and the transform,
//! edge and drag bindings that use it.

use rbr_physics::car_body::aero::Sim7Calls;
use rbr_physics::car_body::drag::DragCalls;
use rbr_physics::car_body::edge::EdgeCalls;
use rbr_physics::car_body::probe::{ProbeCalls, Sample};
use rbr_physics::car_body::transform::{ComposeA, PairWalk, FRAME};
use rbr_physics::car_body::vf54x::Vf54xCalls;
use rbr_physics::car_body::xform_8130::Xform8130Calls;
use rbr_physics::car_body::{self as car, layout};
use rbr_physics::math::vec::{Vec3, Vec4};
use rbr_physics::mem::Mem;

use super::members::{FLOW_OBJECT, M5666B0, SAMPLER_54A0};
use super::step::SLOT_S8C;
use crate::car_body::ADD_EDGE;
use crate::entities::{BODY_COMPOSE, COPY16_WALK, MB560, NODE_OF};
use crate::rigid_body::VF20_JUMP;
use crate::{aligned_frame, Called, Ctx, Decline, Host, Regs, Return};
use rbr_physics::car_body::store::CarStore;

/// Where the calls' vectors and words go: the free stack below the binding's entry, past the return slot. A nested run
/// starts 0x1000 below the emulator's stack pointer, so it does not reach these.
pub(super) fn scratch(r: &Regs) -> u32 {
    (r.esp - 0x100) & !0xf
}

/// Scratch offsets: two vectors for a walk (`A`, `C`), a vector for a send (`SEND`), a second output (`B`), the
/// `Vec4` pair of an edge (`EDGE_IN`, `EDGE_OUT`) and the 28-byte frame `M1` of `Xform8130`.
const A: u32 = 0x00;
pub(super) const C: u32 = 0x10;
pub(super) const SEND: u32 = 0x20;
const B: u32 = 0x30;
const EDGE_IN: u32 = 0x40;
const EDGE_OUT: u32 = 0x50;
const M1: u32 = 0x60;

/// The original's callees a car-body binding makes, on the emulator's memory. The walks (`E43PairWalk::MB560`) and the
/// compose (`ComposeA`) take their vectors by address: the pure ports' `Vec3` is written to scratch and read back.
pub(super) struct Car<'a> {
    pub(super) mem: Mem,
    pub(super) host: &'a dyn Host,
    pub(super) scratch: u32,
}

impl Car<'_> {
    /// Calls the vtable slot `offset` of `obj` (a thiscall with `args`).
    pub(super) fn slot(&self, obj: u32, offset: u32, args: &[u32]) -> Called {
        let target = self.mem.u32(self.mem.u32(obj) + offset);
        self.host.call(target, obj, args)
    }

    /// Lanes 0 to 2 of `v` at `at` (lane 3 is not written, as the port's three lanes).
    fn put(&mut self, at: u32, v: Vec3) {
        self.mem.set_f32(at, v.x);
        self.mem.set_f32(at + 4, v.y);
        self.mem.set_f32(at + 8, v.z);
    }

    fn get(&self, at: u32) -> Vec3 {
        Vec3::load(&self.mem, at)
    }

    /// `E43PairWalk::MB560(this, a, b, c)`: the walk of `a` against `b` on `this`, into the scratch output.
    fn walk(&mut self, this: u32, a: Vec3, b: u32) -> Vec3 {
        let (a_at, c_at) = (self.scratch + A, self.scratch + C);
        self.put(a_at, a);
        self.host.call(MB560, this, &[a_at, b, c_at]);
        self.get(c_at)
    }
}

const TO_WORLD: u32 = 0x4E68E0;
const SLOT_PRE_STEP: u32 = 0x00;
const SLOT_FORCE: u32 = 0x70;
const SLOT_SPIN: u32 = 0x74;
pub(super) const SLOT_RECEIVE: u32 = 0x04;
/// The signal index limit (`emit.rs`): the original logs an error at 0x20.
pub(super) const INDEX_LIMIT: u32 = 0x20;
/// `StepEngine`'s drive step (`CDrive::Step1E0`), the snapshot step (`Snap9360`) and its profiler sample (`g_8ef668`) and
/// name ("CCar::Hook").
pub(super) const DRIVE_STEP: u32 = 0x50A1E0;
pub(super) const SNAP_9360: u32 = 0x4E9360;
pub(super) const STEP_ENGINE_SAMPLE: u32 = 0x8EF668;
pub(super) const STEP_ENGINE_NAME: u32 = 0x738AF4;

impl PairWalk for Car<'_> {
    fn pair_walk(&mut self, this: u32, a: Vec3, b: u32) -> Vec3 {
        self.walk(this, a, b)
    }
}

impl ComposeA for Car<'_> {
    fn compose_a(&mut self, this: u32, a: u32, t: u32, out: u32) -> u32 {
        self.host.call(COPY16_WALK, this, &[a, t, out]);
        out
    }
}

impl Xform8130Calls for Car<'_> {
    fn body_compose(&mut self, _mem: &mut Mem, car: u32, a: u32, out: u32) {
        self.host.call(BODY_COMPOSE, car, &[a, out]);
    }

    fn s30(&mut self, car: u32) -> u32 {
        self.host.call(NODE_OF, car, &[]).eax
    }
}

impl EdgeCalls for Car<'_> {
    fn body_compose(&mut self, car: u32, a: Vec4) -> Vec4 {
        let (a_at, out_at) = (self.scratch + EDGE_IN, self.scratch + EDGE_OUT);
        for (k, lane) in [a.x, a.y, a.z, a.w].into_iter().enumerate() {
            self.mem.set_f32(a_at + 4 * k as u32, lane);
        }
        self.host.call(BODY_COMPOSE, car, &[a_at, out_at]);
        Vec4::load(&self.mem, out_at)
    }

    fn add_edge(&mut self, grid: u32, edge: Vec4) {
        let at = self.scratch + EDGE_IN;
        for (k, lane) in [edge.x, edge.y, edge.z, edge.w].into_iter().enumerate() {
            self.mem.set_f32(at + 4 * k as u32, lane);
        }
        self.host.call(ADD_EDGE, grid, &[at]);
    }

    fn vf20(&mut self, car: u32, a: u32) -> u32 {
        self.host.call(VF20_JUMP, car, &[a]).eax
    }
}

impl DragCalls for Car<'_> {
    fn pre_step(&mut self, _mem: &mut Mem, car: u32) {
        self.slot(car, SLOT_PRE_STEP, &[]);
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        let frame = self.mem.u32(FRAME);
        self.walk(frame, a, car)
    }

    fn drag_force(&mut self, _mem: &mut Mem, car: u32, force: Vec3) {
        let at = self.scratch + SEND;
        self.put(at, force);
        self.slot(car, SLOT_FORCE, &[at]);
    }

    fn spin_damping(&mut self, _mem: &mut Mem, car: u32, damping: Vec3) {
        let at = self.scratch + SEND;
        self.put(at, damping);
        self.slot(car, SLOT_SPIN, &[at]);
    }
}

impl Sim7Calls for Car<'_> {
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3 {
        let (a_at, b_at) = (self.scratch + A, self.scratch + B);
        self.put(a_at, v);
        self.host.call(TO_WORLD, car, &[a_at, b_at]);
        self.get(b_at)
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        DragCalls::pair_walk(self, a, car)
    }

    fn aero_force(&mut self, mem: &mut Mem, car: u32, force: Vec3) {
        DragCalls::drag_force(self, mem, car, force);
    }

    fn aero_spin(&mut self, mem: &mut Mem, car: u32, spin: Vec3) {
        DragCalls::spin_damping(self, mem, car, spin);
    }
}

pub(super) fn car_calls<'a>(ctx: &Ctx<'a>, r: &Regs) -> Car<'a> {
    Car { mem: ctx.mem.clone(), host: ctx.host, scratch: scratch(r) }
}

/// MotionProbe's aligned local-frame base: locals begin after its aligned 0x78-byte frame and the saved esi/edi.
pub(super) fn probe_scratch(entry_esp: u32) -> u32 {
    aligned_frame(entry_esp) - 0x78 - 8
}

/// The exact E47 entity-forwarder and `CCar::Vf54x`'s final scratch vtable dispatch.
pub(super) struct CarVf54<'a> {
    pub(super) mem: Mem,
    pub(super) host: &'a dyn Host,
    pub(super) car: u32,
}

impl rbr_physics::rigid_body::host::Vf54Host for CarVf54<'_> {
    fn entity_scratch_tail(&mut self) -> Result<(), rbr_physics::rigid_body::host::HostUnavailable> {
        let entity = self.car + 0x5c;
        let target = self.mem.u32(self.mem.u32(entity) + 4);
        self.host.call(target, entity, &[]);
        Ok(())
    }
}

impl Vf54xCalls for CarVf54<'_> {
    fn scratch_slot1(&mut self, scratch: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(scratch) + 4);
        self.host.call(target, scratch, &[]).eax
    }
}

impl ProbeCalls for Car<'_> {
    fn s8c(&mut self, car: u32) -> u32 {
        self.slot(car, SLOT_S8C, &[]).eax
    }

    fn m5666b0(&mut self) -> u32 {
        self.host.call(M5666B0, self.mem.u32(FLOW_OBJECT), &[]).eax
    }

    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3 {
        let input = self.scratch + 0x10;
        let output = self.scratch + 0x20;
        self.put(input, v);
        self.host.call(TO_WORLD, car, &[input, output]);
        self.get(output)
    }

    fn sampler(&mut self, car: u32) -> Sample {
        let big = self.scratch + 0x30;
        let small = self.scratch + 0x40;
        let found = self.host.call(SAMPLER_54A0, car, &[car + 0x140, big, small]).eax;
        Sample { found, first: self.mem.u32(big + 0x44), second: self.mem.u32(big + 0x48) }
    }
}

/// The vector the original's argument points at (lanes 0 to 2).
fn vec_at(mem: &Mem, at: u32) -> Vec3 {
    Vec3::load(mem, at)
}

fn set_vec(mem: &mut Mem, at: u32, v: Vec3) {
    mem.set_f32(at, v.x);
    mem.set_f32(at + 4, v.y);
    mem.set_f32(at + 8, v.z);
}

/// thiscall `(data*)`: `NTweak::CFloat3::LoadBinary` reads the four words at `data` into `this`; returns `this`.
pub(super) fn load_binary(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let this = car::tweak::load_binary(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(this, 4))
}

/// thiscall: `E36Triple::MinPick` on the triple at `this`; eax is the int result.
pub(super) fn min_pick(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(ctx.mem.min_pick(r.ecx) as u32, 0))
}

/// thiscall `(a, b)`: `CCar::ToWorld` walks the vector at `a` against the world frame into `b`; returns `b`.
pub(super) fn to_world(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    if ctx.mem.u32(FRAME) == 0 {
        return Err(Decline::Unsupported("the frame global is unset (the original halts)"));
    }
    let mut calls = car_calls(ctx, r);
    let v = car::transform::to_world(&mut calls, &ctx.mem, r.ecx, vec_at(&ctx.mem, a));
    set_vec(&mut ctx.mem, b, v);
    Ok(Return::void(b, 8))
}

/// thiscall `(a, b)`: `CCar::WorldCompose`, `ComposeA(S30(car), a, frame, b)`; returns `b`.
pub(super) fn world_compose(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    if ctx.mem.u32(FRAME) == 0 {
        return Err(Decline::Unsupported("the frame global is unset (the original halts)"));
    }
    let mut calls = car_calls(ctx, r);
    car::transform::world_compose(&mut calls, &ctx.mem, r.ecx, a, b);
    Ok(Return::void(b, 8))
}

/// thiscall `(a, b)`: `CCar::BodyCompose`, `ComposeA(S30(body), a, S30(car), b)`; returns `b`.
pub(super) fn body_compose(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    if ctx.mem.body(r.ecx) == 0 {
        return Err(Decline::Unsupported("the body is unset (the original halts)"));
    }
    let mut calls = car_calls(ctx, r);
    car::transform::body_compose(&mut calls, &ctx.mem, r.ecx, a, b);
    Ok(Return::void(b, 8))
}

/// thiscall `(a, b)`: `CCar::Xform8130`, the body frame into the scratch `M1`, then `ComposeA` with the frame global.
pub(super) fn xform_8130(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    if ctx.mem.u32(FRAME) == 0 {
        return Err(Decline::Unsupported("the frame global is unset (the original halts)"));
    }
    let mut calls = car_calls(ctx, r);
    let m1 = calls.scratch + M1;
    car::xform_8130::xform_8130(&mut calls, &mut ctx.mem, r.ecx, a, b, m1);
    Ok(Return::void(b, 8))
}

/// thiscall: `CCar::PushGridEdge` (`Update7270`), the grid edge once and the flag.
pub(super) fn push_grid_edge(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    layout::push_grid_edge(&mut calls, &mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(a)`: `CCar::RefreshEdge` (`Vf20x72D0`); eax is the `Vf20` call's.
pub(super) fn refresh_edge(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    let eax = layout::refresh_edge(&mut calls, &mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(eax, 4))
}

/// thiscall: `CCar::Sim7AD0`, the wheel aero gust.
pub(super) fn sim7ad0(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    car::aero::sim7ad0(&mut calls, &mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall: `CCar::Sim7DC0` (`DragAndSpinDamp`).
pub(super) fn drag_and_spin_damp(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = car_calls(ctx, r);
    let factor = crate::ngp::drag_factor(ctx);
    car::drag::drag_and_spin_damp(&mut calls, &mut ctx.mem, r.ecx, factor, crate::ngp::variant());
    Ok(Return::void(0, 0))
}

/// thiscall: `CCar::UpdateFfbTorque` (`Hook1FF0`), the filtered steering torque for force feedback.
pub(super) fn update_ffb_torque(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::update_ffb_torque(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}
