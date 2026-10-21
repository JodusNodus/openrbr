//! `CHingedRigidBody`'s simulation (`rbr_physics::entities::hinge_*`, 0x51c960-0x51ff00) and the state emit.

use rbr_physics::entities::hinge_pose::{PoseCalls, PoseState};
use rbr_physics::entities::hinge_car_transform::{self, CarPathCalls};
use rbr_physics::entities::hinge_refresh::{self, RefreshCalls, RefreshState};
use rbr_physics::entities::hinge_advance;
use rbr_physics::entities::hinge_pivot::PivotCalls;
use rbr_physics::entities::hinge_slip::SlipCalls;
use rbr_physics::entities::hinge_solve::{ContactHandles, EntryTerms, Outcome, SolveCalls, SolveTerms};
use rbr_physics::entities::layout;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::{
    self,
    signal::{self, SignalHost},
};

use super::{
    BODY_COMPOSE, CLEAR_ACC, COPY16_WALK, E01_VEC4_COPY_FROM, INIT_BD0, LIMIT_UPDATE, MB560, SIM_C960, SIM_CFF0,
    SIM_DA60, SIM_E1D0, SIM_EA80, STATE_EMIT, TO_QUAT, VF81X, XFORM_8130,
};
use crate::car_body::ADD_EDGE;
use crate::engine_driveline::E01LOG;
use crate::rigid_body::{E43VF64, E43VF6C};
use crate::vcu::SET_DIRTY;
use crate::{aligned_frame, Ctx, Decline, Host, Regs, Return};

/// The state signal's slot count (`CMultiSignal1` has 32 slots).
const SIGNAL_SLOTS: u32 = 0x20;
const SCALE: u32 = 0x473cd0; // CFloat3::Scale
pub(super) const SIM_D160: u32 = 0x51d160;
/// The state emit's messages (the recursion and the range log's text, and the function's name).
const MSG_RECURSIVE: u32 = 0x730358;
const MSG_RANGE: u32 = 0x73032c;
const MSG_NAME: u32 = 0x73d4b0;

/// The calls `SimF950` makes, at the original's depths: the frame is 16-aligned from the entry esp (`push ebp; and esp,
/// -16`), and each call enters where the original's pushes put it below that frame.
struct F950Host<'a> {
    host: &'a dyn Host,
    frame: u32,
}

impl hinge_advance::Calls for F950Host<'_> {
    fn integrate_unowned(&mut self, this: u32, dt: f32) -> u32 {
        self.host.call_at(self.frame - 0x48, E43VF6C, this, &[dt.to_bits()]).eax
    }

    fn mark_dirty(&mut self, at: u32) {
        self.host.call_at(self.frame - 0x44, SET_DIRTY, at, &[]);
    }

    fn scale_vector(&mut self, vec: u32, s: f32, out: u32) {
        self.host.call_at(self.frame - 0x4c, SCALE, vec, &[s.to_bits(), out]);
    }

    fn apply_stops(&mut self, this: u32) {
        self.host.call_at(self.frame - 0x44, LIMIT_UPDATE, this, &[]);
    }

    fn update_pose(&mut self, this: u32) {
        self.host.call_at(self.frame - 0x44, SIM_D160, this, &[]);
    }

    fn emit_state(&mut self, signal: u32, sig: u32) -> u32 {
        self.host.call_at(self.frame - 0x48, STATE_EMIT, signal, &[sig]).eax
    }
}

/// thiscall `(dt)`: `CHingedRigidBody::SimF950` (0x51f950), the hinge's per-step update (see
/// `rbr_physics::entities::hinge_advance`). The payload is written in the frame's slot the original uses (`frame - 0x30`).
pub(super) fn sim_f950_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let dt = f32::from_bits(r.stack[0]);
    let frame = aligned_frame(r.esp);
    let mut calls = F950Host { host: ctx.host, frame };
    let eax = layout::sim_f950_at(&mut ctx.mem, r.ecx, dt, frame - 0x30, &mut calls);
    Ok(Return::void(eax, 4))
}

/// The state emit's connections and messages, through the host. `eax` is what the original leaves: the last call's eax
/// (an invoke's, or a log's), 0 when nothing was called.
struct StateEmit<'a> {
    host: &'a dyn Host,
    sp: u32,
    eax: u32,
}

impl SignalHost for StateEmit<'_> {
    /// `Invoke(sig)`: vtable slot 1 of the connection, thiscall, entered 24 bytes below the emit's entry esp.
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32) {
        let slot = mem.u32(mem.u32(conn) + 4);
        self.eax = self.host.call_at(self.sp, slot, conn, &[sig]).eax;
    }

    /// `E01Log(6, 0, ...)` (cdecl): an emission while the signal is emitting.
    fn log_recursive(&mut self) {
        let args = [6, 0, MSG_RECURSIVE, MSG_NAME];
        self.eax = self.host.call_at(self.sp, E01LOG, 0, &args).eax;
    }

    /// `E01Log(7, 0, ...)`: the count is out of range. The binding declines first (the original traps after the log).
    fn log_range(&mut self, count: u32) {
        let args = [7, 0, MSG_RANGE, MSG_NAME, count];
        self.eax = self.host.call_at(self.sp, E01LOG, 0, &args).eax;
    }
}

/// thiscall `(sig)`: `CMultiSignal1<const CSigMrRBodyState&>::Emit` (0x51f700), the physics core's emit (see
/// `rbr_physics::physics_core::signal`). The count's range is checked first: the original logs and traps, which the
/// binding leaves to the original.
pub(super) fn state_emit_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let signal = r.ecx;
    let count_at = physics_core::layout::signal_count_at(&ctx.mem, signal);
    if count_at != 0
        && physics_core::layout::signal_guard(&ctx.mem, signal) == 0
        && ctx.mem.u32(count_at) >= SIGNAL_SLOTS
    {
        return Err(Decline::Unsupported("the signal's slot count is out of range (the original traps)"));
    }
    let mut emit = StateEmit { host: ctx.host, sp: r.esp - 24, eax: 0 };
    signal::emit(&mut ctx.mem, &mut emit, signal, r.stack[0]);
    Ok(Return::void(emit.eax, 4))
}

/// thiscall: `CHingedRigidBody::SimC960` (0x51c960), the rotation block from the hinge angle. Its eax is the sine and
/// cosine's residue, a scratch value no caller reads, so 0.
pub(super) fn sim_c960_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::hinge_rotation(&mut ctx.mem, r.ecx);
    Ok(Return::void(0, 0))
}

/// thiscall `(idx)`: `CHingedRigidBody::SimCF10` (0x51cf10), one limit's residual (st0).
pub(super) fn sim_cf10_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let step = layout::hinge_limit_residual(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::float(step.torque, 4))
}

/// thiscall: `CHingedRigidBody::SimEA80` (0x51ea80), the sum of the active limits' residuals (st0). The inner `SimCF10`
/// calls are the port's own, so they are not made.
pub(super) fn sim_ea80_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::float(f64::from(layout::hinge_limit_sum(&mut ctx.mem, r.ecx)), 0))
}

/// The slip term's calls on the original's objects, at the depths below the frame: `E01Vec4::CopyFrom` (-0x78) and the
/// walks (`E43PairWalk::MB560`, -0x80), as `SimCFF0`'s pushes put them.
struct SlipHost<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
}

impl SlipCalls for SlipHost<'_> {
    fn copy_vector(&mut self, to: u32, from: u32) {
        self.host.call_at(self.frame - 0x78, E01_VEC4_COPY_FROM, to, &[from]);
    }

    fn carry(&mut self, receiver: u32, a: u32, b: u32, out: u32) -> [f32; 3] {
        self.host.call_at(self.frame - 0x80, MB560, receiver, &[a, b, out]);
        layout::vec3(&self.mem, out)
    }

    fn car_speed(&self, state: u32) -> f32 {
        layout::car_state_c00(&self.mem, state)
    }
}

/// thiscall: `CHingedRigidBody::SimCFF0` (0x51cff0), the slip term (st0). Pops nothing.
pub(super) fn sim_cff0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp);
    let mut calls = SlipHost { mem: ctx.mem.clone(), host: ctx.host, frame };
    let term = layout::sim_cff0_at(&ctx.mem, r.ecx, frame, &mut calls);
    Ok(Return::float(term as f64, 0))
}

/// The pivot's calls on the original's objects, at the depths below the frame: the walks (`MB560`, -0x70) and
/// `CCar::BodyCompose` (-0x6c).
struct PivotHost<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
}

impl PivotCalls for PivotHost<'_> {
    fn store_vector(&mut self, at: u32, words: [u32; 3]) {
        layout::store_walk_vector(&mut self.mem, at, words);
    }

    fn carry(&mut self, receiver: u32, a: u32, b: u32, out: u32) -> [f32; 3] {
        self.host.call_at(self.frame - 0x70, MB560, receiver, &[a, b, out]);
        layout::vec3(&self.mem, out)
    }

    fn compose_with_car(&mut self, car: u32, a: u32, out: u32) -> [f32; 3] {
        self.host.call_at(self.frame - 0x6c, BODY_COMPOSE, car, &[a, out]);
        layout::vec3(&self.mem, out)
    }
}

/// thiscall: `CHingedRigidBody::SimE1D0` (0x51e1d0), the pivot and settle. Pops nothing.
pub(super) fn sim_e1d0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp);
    let mut calls = PivotHost { mem: ctx.mem.clone(), host: ctx.host, frame };
    let eax = layout::sim_e1d0_at(&mut ctx.mem, r.ecx, frame, &mut calls);
    Ok(Return::void(eax, 0))
}

/// `SimD160`'s calls on the original's objects, at the depths below the frame: `CCar::Xform8130` (-0x6c), `MB560`
/// (-0x70), `SimC960` (-0x64) and `E34Quat::ToQuat` (-0x68). `load` and `store` read and write the body.
struct D160Host<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
    this: u32,
}

impl PoseCalls for D160Host<'_> {
    fn load(&self) -> PoseState {
        layout::d160_words(&self.mem, self.this)
    }

    fn store(&mut self, state: &PoseState) {
        layout::store_d160_words(&mut self.mem, self.this, state);
    }

    fn car_transform(&mut self, car: u32, pose: u32, out: u32) {
        self.host.call_at(self.frame - 0x6c, XFORM_8130, car, &[pose, out]);
    }

    fn carry_into_car(&mut self, car: u32, vector: u32, identity: u32, out: u32) {
        self.host.call_at(self.frame - 0x70, MB560, car, &[vector, identity, out]);
    }

    fn update_hinge_rotation(&mut self, body: u32) {
        self.host.call_at(self.frame - 0x64, SIM_C960, body, &[]);
    }

    fn matrix_to_quaternion(&mut self, matrix: u32, out: u32) {
        self.host.call_at(self.frame - 0x68, TO_QUAT, matrix, &[out]);
    }
}

/// thiscall: `CHingedRigidBody::SimD160` (0x51d160), the hinge's frame update. Pops nothing.
pub(super) fn sim_d160_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let mut calls = D160Host { mem: ctx.mem.clone(), host: ctx.host, frame: aligned_frame(r.esp), this: r.ecx };
    let eax = layout::sim_d160_at(&ctx.mem, r.ecx, &mut calls);
    Ok(Return::void(eax, 0))
}

/// `SimEAD0`'s calls on the original's objects, at the depths below the frame: `SimE1D0`, `SimCFF0` and `SimEA80`
/// (-0xb4), and the vote (`E47Solid::Vf81x`, -0xc0) with the two vectors and 3.0. `voted` is the vote's answer, which
/// the solve's eax depends on.
struct SolveHost<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
    voted: bool,
}

impl SolveCalls for SolveHost<'_> {
    fn linked(&self, this: u32) -> bool {
        layout::hinge_linked(&self.mem, this)
    }

    fn owned(&self, this: u32) -> bool {
        layout::hinge_owned(&self.mem, this)
    }

    fn force_pass(&mut self, this: u32) -> u32 {
        self.host.call_at(self.frame - 0xb4, E43VF64, this, &[]).eax
    }

    fn entry_terms(&self, this: u32) -> EntryTerms {
        layout::solve_entry_terms(&self.mem, this)
    }

    fn update_pivot(&mut self, this: u32) {
        self.host.call_at(self.frame - 0xb4, SIM_E1D0, this, &[]);
    }

    fn solve_terms(&self, this: u32) -> SolveTerms {
        layout::solve_terms(&self.mem, this)
    }

    fn slip_torque(&mut self, this: u32) -> f32 {
        self.host.call_at(self.frame - 0xb4, SIM_CFF0, this, &[]).st0 as f32
    }

    fn limit_torque_sum(&mut self, this: u32) -> f32 {
        self.host.call_at(self.frame - 0xb4, SIM_EA80, this, &[]).st0 as f32
    }

    fn contact_load(&mut self, this: u32, frame: &ContactHandles) -> Option<([f32; 3], [f32; 3])> {
        let args = [frame.force, frame.torque, 3.0f32.to_bits()];
        self.voted = self.host.call_at(self.frame - 0xc0, VF81X, this, &args).eax != 0;
        self.voted.then(|| (layout::vec3(&self.mem, frame.force), layout::vec3(&self.mem, frame.torque)))
    }
}

/// thiscall: `CHingedRigidBody::SimEAD0` (0x51ead0), the hinge solve (writes +0x624); an unowned body gets the force
/// pass (`E43Vf64`) instead. eax: the link when the solve returns early, the pass's eax for an unowned body, the
/// definition after a vote hit, else 0.
pub(super) fn sim_ead0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp);
    let mut calls = SolveHost { mem: ctx.mem.clone(), host: ctx.host, frame, voted: false };
    let eax = match layout::sim_ead0_at(&mut ctx.mem, r.ecx, frame, &mut calls) {
        Outcome::Linked => layout::hinge_link(&ctx.mem, r.ecx),
        Outcome::Unowned { result } => result,
        Outcome::Solved(_) if calls.voted => layout::hinge_definition(&ctx.mem, r.ecx),
        Outcome::Solved(_) => 0,
    };
    Ok(Return::void(eax, 0))
}

/// `SimEFE0`'s calls on the original's objects, each at its depth below the frame (`frame_of`, the asm's aligned frame):
/// the body's slot 36 (-0x44), the two `VfE070` (-0x5c), `CCar::BodyCompose` (-0x4c), `SimD160` (-0x44), `SimD860`
/// (-0x48, its output at -0x20) and `E37GridOps::AddEdge` (-0x48, its vector copied to -0x10 first).
struct EfeHost<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
    this: u32,
}

pub(super) const VF_E070: u32 = 0x51e070;
pub(super) const SIM_D860: u32 = 0x51d860;
/// `SimEFE0`'s locals below its frame: `SimD860`'s output and `AddEdge`'s vector.
const EFE0_D860_OUT: u32 = 0x20;
const EFE0_EDGE: u32 = 0x10;

impl RefreshCalls for EfeHost<'_> {
    fn load(&self) -> RefreshState {
        layout::efe0_state(&self.mem, self.this)
    }

    fn store(&mut self, state: &RefreshState) {
        layout::store_efe0_state(&mut self.mem, self.this, state);
    }

    fn wake_body(&mut self, this: u32) {
        let target = self.mem.u32(self.mem.u32(this) + 0x90);
        self.host.call_at(self.frame - 0x44, target, this, &[]);
    }

    fn build_axes(&mut self, this: u32, args: [u32; 6]) {
        self.host.call_at(self.frame - 0x5c, VF_E070, this, &args);
    }

    fn compose_with_car(&mut self, car: u32, a: u32, out: u32) {
        self.host.call_at(self.frame - 0x4c, BODY_COMPOSE, car, &[a, out]);
    }

    fn update_pose(&mut self, this: u32) {
        self.host.call_at(self.frame - 0x44, SIM_D160, this, &[]);
    }

    fn offset(&mut self, this: u32, out: u32) -> [f32; 3] {
        self.host.call_at(self.frame - 0x48, SIM_D860, this, &[out]);
        [0, 4, 8].map(|k| self.mem.f32(out + k))
    }

    fn add_grid_edge(&mut self, grid: u32, vec: [f32; 4]) {
        let at = self.frame - EFE0_EDGE;
        for (k, lane) in vec.into_iter().enumerate() {
            self.mem.set_f32(at + 4 * k as u32, lane);
        }
        self.host.call_at(self.frame - 0x48, ADD_EDGE, grid, &[at]);
    }
}

/// `SimE4F0`'s car path's calls on the original's objects, each at its depth below the frame (`frame_of`): the car's
/// slot 12 (-0x48), the body's slot 12, `ComposeA` and `MB560` (-0x50), the car's slot 0x138 (-0x5c). The car is read from
/// the body (`+0x44`) at each call, as the original reads it.
struct E4F0Host<'a> {
    mem: Mem,
    host: &'a dyn Host,
    this: u32,
    frame: u32,
}

impl E4F0Host<'_> {
    fn car(&self) -> u32 {
        self.mem.u32(self.this + 0x44)
    }
}

impl CarPathCalls for E4F0Host<'_> {
    fn car_frame(&mut self, buf: u32) -> u32 {
        let car = self.car();
        let target = self.mem.u32(self.mem.u32(car) + 0x30);
        self.host.call_at(self.frame - 0x48, target, car, &[buf]).eax
    }

    fn body_frame(&mut self, v: u32, u: u32) -> u32 {
        let target = self.mem.u32(self.mem.u32(self.this) + 0x30);
        self.host.call_at(self.frame - 0x50, target, self.this, &[v, u]).eax
    }

    fn carry_pose(&mut self, x: u32, a: u32, b: u32, c: u32) {
        self.host.call_at(self.frame - 0x50, COPY16_WALK, x, &[a, b, c]);
    }

    fn carry(&mut self, x: u32, a: u32, b: u32, c: u32) {
        self.host.call_at(self.frame - 0x50, MB560, x, &[a, b, c]);
    }

    fn car_forward(&mut self, args: [u32; 6]) -> u32 {
        let car = self.car();
        let target = self.mem.u32(self.mem.u32(car) + 0x138);
        self.host.call_at(self.frame - 0x5c, target, car, &args).eax
    }
}

struct F350Host<'a> {
    mem: Mem,
    host: &'a dyn Host,
}

impl rbr_physics::entities::layout::F350Calls for F350Host<'_> {
    fn xform(&mut self, sp: u32, car: u32, pose: u32, out: u32) {
        self.host.call_at(sp, 0x4e68e0, car, &[pose, out]);
    }

    fn construct(&mut self, sp: u32, at: u32) {
        self.host.call_at(sp, 0x52f090, at, &[0]);
    }

    fn scale(&mut self, sp: u32, vector: u32, scalar: f32, out: u32) {
        self.host.call_at(sp, 0x473cd0, vector, &[scalar.to_bits(), out]);
    }

    fn car_impulse(&mut self, sp: u32, car: u32, args: [u32; 8]) {
        let target = layout::vtable_target(&self.mem, car, 0x128);
        self.host.call_at(sp, target, car, &args);
    }

    fn clear_car(&mut self, sp: u32, car: u32, body: u32) {
        self.host.call_at(sp, 0x4e58f0, car, &[body]);
    }

    fn exclude(&mut self, sp: u32, body: u32, car: u32) {
        self.host.call_at(sp, 0x51a060, body, &[car]);
    }
}

/// thiscall `(a, b, c, d, e, f)`, `ret 0x18`: `CHingedRigidBody::SimE4F0` (0x51e4f0), and `CMountedRigidBody::Vf138`
/// (0x521a50, the same asm). An unowned body forwards its arguments to `CSolid::E43Vf138` (ported in `rigid_body`); an
/// owned one runs the car path (`hinge_car_transform`).
pub(super) fn sim_e4f0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, e, f, ..] = r.stack;
    let args = [a, b, c, d, e, f];
    if !layout::hinge_owned(&ctx.mem, r.ecx) {
        return crate::rigid_body::e43vf138(ctx, r);
    }
    let frame = aligned_frame(r.esp);
    let mut calls = E4F0Host { mem: ctx.mem.clone(), host: ctx.host, this: r.ecx, frame };
    let eax = hinge_car_transform::forward_through_car(&mut calls, args, [frame - 0x30, frame - 0x20, frame - 0x10]);
    Ok(Return::void(eax, 0x18))
}

/// thiscall eight words, `ret 0x20`: `CHingedRigidBody::SimF350` (0x51f350). The unowned path shares the ported
/// `CSolid::E43Vf128`; the owned path calls the original car and fence-independent helpers at their assembly stack depths.
pub(super) fn sim_f350_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let args: [u32; 8] = r.stack[..8].try_into().expect("SimF350 has eight stack words");
    if !layout::hinge_owned(&ctx.mem, r.ecx) {
        return crate::rigid_body::impulse_run(ctx, r);
    }
    let frame = aligned_frame(r.esp);
    let mut calls = F350Host { mem: ctx.mem.clone(), host: ctx.host };
    layout::sim_f350(&mut ctx.mem, r.ecx, args, frame, &mut calls);
    Ok(Return::void(0, 0x20))
}

/// The original callees of `SimDA60` and `SimFD00`, in the emulator, at the stack pointers the layout gives.
struct DetachHost<'a>(&'a dyn Host);

impl layout::DA60Calls for DetachHost<'_> {
    fn compose_a(&mut self, _: &mut Mem, sp: u32, car: u32, args: [u32; 3]) {
        self.0.call_at(sp, COPY16_WALK, car, &args);
    }

    fn quat_mul(&mut self, _: &mut Mem, sp: u32, this: u32, b: u32, out: u32) {
        self.0.call_at(sp, 0x483dd0, this, &[b, out]);
    }
}

impl layout::FD00Calls for DetachHost<'_> {
    fn sim_da60(&mut self, _: &mut Mem, sp: u32, body: u32) {
        self.0.call_at(sp, SIM_DA60, body, &[]);
    }

    fn to_world(&mut self, _: &mut Mem, sp: u32, car: u32, local: u32, out: u32) {
        self.0.call_at(sp, 0x4e68e0, car, &[local, out]);
    }

    fn slot(&mut self, mem: &mut Mem, sp: u32, body: u32, slot: u32) {
        let target = layout::vtable_target(mem, body, slot);
        self.0.call_at(sp, target, body, &[]);
    }

    fn clear_accumulators(&mut self, _: &mut Mem, sp: u32, body: u32) {
        self.0.call_at(sp, CLEAR_ACC, body, &[]);
    }

    fn unlink(&mut self, _: &mut Mem, sp: u32, link: u32) {
        self.0.call_at(sp, 0x4e41a0, link, &[]);
    }

    fn init_bd0(&mut self, _: &mut Mem, sp: u32, body: u32) {
        self.0.call_at(sp, INIT_BD0, body, &[]);
    }

    fn lookup_twin(&mut self, _: &mut Mem, sp: u32, grid: u32, x: u32, y: u32) -> u32 {
        self.0.call_at(sp, 0x4e03b0, grid, &[x, y]).eax
    }

    fn move_to_cell(&mut self, _: &mut Mem, sp: u32, link: u32, cell: u32) {
        self.0.call_at(sp, 0x4faeb0, 0, &[link, cell]);
    }

    fn reg_wall(&mut self, _: &mut Mem, sp: u32, state: u32, body: u32) {
        self.0.call_at(sp, 0x5263d0, state, &[body]);
    }

    fn emit_detach(&mut self, _: &mut Mem, sp: u32, signal: u32, sig: u32) {
        self.0.call_at(sp, 0x51f580, signal, &[sig]);
    }

    fn emit_event(&mut self, _: &mut Mem, sp: u32, signal: u32, event: u32) {
        self.0.call_at(sp, 0x51f7c0, signal, &[event]);
    }
}

impl layout::StepBigCalls for DetachHost<'_> {
    fn world_compose(&mut self, _: &mut Mem, sp: u32, car: u32, point: u32, out: u32) {
        self.0.call_at(sp, 0x4e6950, car, &[point, out]);
    }

    fn quat_norm(&mut self, _: &mut Mem, sp: u32, quat: u32, out: u32) {
        self.0.call_at(sp, 0x4e4c70, quat, &[out]);
    }
}

/// thiscall, no stack arguments: `CHingedRigidBody::SimFD00` (0x51fd00), the detach from the car (`layout::sim_fd00`).
pub(super) fn sim_fd00_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp) - 0x90;
    let eax = layout::sim_fd00(&mut ctx.mem, r.ecx, frame, &mut DetachHost(ctx.host));
    Ok(Return::void(eax, 0))
}

/// thiscall, no stack arguments: `CMountedRigidBody::StepBig` (0x522100), the mounted body's detach from the car
/// (`layout::step_big`).
pub(super) fn step_big_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp) - 0xb0;
    let eax = layout::step_big(&mut ctx.mem, r.ecx, frame, &mut DetachHost(ctx.host));
    Ok(Return::void(eax, 0))
}

/// thiscall, no stack arguments: `CHingedRigidBody::SimDA60` (0x51da60), the body's pose through its car.
pub(super) fn sim_da60_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp) - 0x40;
    layout::sim_da60(&mut ctx.mem, r.ecx, frame, &mut DetachHost(ctx.host));
    Ok(Return::void(r.ecx + 0x2a0, 0))
}

/// thiscall: `CHingedRigidBody::SimEFE0` (0x51efe0), the hinged body's frame from its definition. Pops nothing.
pub(super) fn sim_efe0_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let frame = aligned_frame(r.esp);
    let mut calls = EfeHost { mem: ctx.mem.clone(), host: ctx.host, frame, this: r.ecx };
    let at = layout::efe0_frame(&ctx.mem, r.ecx, frame - EFE0_D860_OUT);
    hinge_refresh::refresh_hinge(&mut calls, &at);
    Ok(Return::void(0, 0))
}

/// thiscall `(link)`, `ret 4`: `CHingedRigidBody::Set650`, the hinge's link word.
pub(super) fn set_650_run(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::set_hinge_link(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(a, b, c, d, e, f)`, `ret 0x18`: `CHingedRigidBody::VfE070` (0x51e070, `layout::vf_e070`).
pub(super) fn hinge_vf_e070(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, c, d, e, f, ..] = r.stack;
    layout::vf_e070(&mut ctx.mem, &rbr_physics::entities::hinge_axes::AxesHandles { origin: a, direction: b, reference: c, inverse_origin: d, axes: e, inverse_axes: f });
    Ok(Return::void(0, 0x18))
}

/// `SimD860`'s walk at the original's frame: its vector at `frame - 0x20`, the output at `frame - 0x10`, the call's
/// return slot at `frame - 0x40` (`frame` the asm's aligned frame, `(entry esp - 4) & !0xf`).
struct D860Host<'a> {
    mem: Mem,
    host: &'a dyn Host,
    frame: u32,
}

impl rbr_physics::entities::hinge_offset::OffsetCalls for D860Host<'_> {
    fn carry(&mut self, receiver: u32, a: [f32; 3], b: u32) -> [f32; 3] {
        let (a_at, out) = (self.frame - 0x20, self.frame - 0x10);
        for (k, x) in a.into_iter().enumerate() {
            self.mem.set_f32(a_at + 4 * k as u32, x);
        }
        self.host.call_at(self.frame - 0x40, MB560, receiver, &[a_at, b, out]);
        [0, 4, 8].map(|k| self.mem.f32(out + k))
    }
}

/// thiscall `(out)`, `ret 4`: `CHingedRigidBody::SimD860` (0x51d860), its vector into `out`; returns `out`.
pub(super) fn hinge_sim_d860(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let input = layout::d860_input(&ctx.mem, r.ecx);
    let mut calls = D860Host { mem: ctx.mem.clone(), host: ctx.host, frame: aligned_frame(r.esp) };
    let v = rbr_physics::entities::hinge_offset::hinge_offset(&mut calls, &input);
    for (k, x) in v.into_iter().enumerate() {
        ctx.mem.set_f32(r.stack[0] + 4 * k as u32, x);
    }
    Ok(Return::void(r.stack[0], 4))
}
