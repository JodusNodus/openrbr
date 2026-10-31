//! The detach of a hinged or mounted body: `SimF350`, `SimDA60`, `SimFD00`, `CMountedRigidBody::StepBig`.

use super::hinge_state::HINGE_OWNER;
use crate::entities::layout::ROOT_IDENTITY;
use crate::entities::{hinge_detach, hinge_impulse};
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use crate::physics_core;
use crate::physics_core::layout::{CELL_GRID_GLOBAL, SIGNAL_MANAGER};

/// The original callees of `CHingedRigidBody::SimF350`, at their entry stack pointers.
pub trait F350Calls {
    fn xform(&mut self, sp: u32, car: u32, pose: u32, out: u32);
    fn construct(&mut self, sp: u32, at: u32);
    fn scale(&mut self, sp: u32, vector: u32, scalar: f32, out: u32);
    fn car_impulse(&mut self, sp: u32, car: u32, args: [u32; 8]);
    fn clear_car(&mut self, sp: u32, car: u32, body: u32);
    fn exclude(&mut self, sp: u32, body: u32, car: u32);
}

/// The owned-car path of `CHingedRigidBody::SimF350` (0x51f350). Host calls and local buffers use the original's
/// aligned frame base, which is `(entry_esp - 4) & !0xf`.
pub fn sim_f350(mem: &mut Mem, this: u32, args: [u32; 8], frame: u32, calls: &mut impl F350Calls) {
    let car = mem.u32(this + 0x3d0);
    let def = mem.u32(this + 0x34);
    let transformed = frame - 0x50;
    calls.xform(frame - 0x7c, car, def + 0x2c0, transformed);

    let point = args[0];
    let impulse = args[1];
    let p = Vec4::load(mem, point);
    let b = Vec4::load(mem, impulse);
    let origin = Vec4::load(mem, this + 0x5c0);
    let arm = Vec4 { x: p.x - origin.x, y: p.y - origin.y, z: p.z - origin.z, w: 0.0 };
    let cross = arm.cross3(b);
    for (i, lane) in [arm.x, arm.y, arm.z].into_iter().enumerate() {
        mem.set_f32(frame - 0x40 + 4 * i as u32, lane);
    }
    for (i, lane) in [cross.x, cross.y, cross.z].into_iter().enumerate() {
        mem.set_f32(frame - 0x30 + 4 * i as u32, lane);
    }
    let t = Vec4::load(mem, transformed);
    let c = Vec3::load(mem, frame - 0x30).extend(0.0);
    let projection = hinge_impulse::torque_about_axis(t, c);
    let denominator = mem.f32(def + 0x4bc);
    calls.construct(frame - 0x78, frame - 0x10);
    let angle = mem.f32(this + 0x40c) + projection / denominator;
    mem.set_f32(this + 0x40c, angle);

    let dot_impulse = Vec4::load(mem, impulse);
    let dot = dot_impulse.z * t.z + dot_impulse.y * t.y;
    let dot = dot + dot_impulse.x * t.x;
    let (dot, clamped) = hinge_impulse::clamp_axial_impulse(dot, mem.f32(0x731d00), mem.f32(0x73d3bc));
    let scaled = frame - 0x20;
    calls.scale(frame - 0x7c, transformed, dot, scaled);

    if mem.u32(def + 0x50c) == 0 {
        let passed_dot = hinge_impulse::car_magnitude(dot);
        calls.car_impulse(
            frame - 0x94,
            mem.u32(this + 0x3d0),
            hinge_impulse::car_impulse_args(point, scaled, args[4], passed_dot, args[7]),
        );
    }
    if clamped && mem.u32(def + 0x50c) == 0 {
        let link = mem.u32(this + 0x650);
        mem.set_u32(link, 0);
        mem.set_u32(link + 4, 0);
        calls.clear_car(frame - 0x78, car, this);
        calls.exclude(frame - 0x78, this, car);
    }
}

/// The detach signal (`CMultiSignal1<CSigDetachFromParent>`) and the body event signal in the signal object.
const DETACH_SIGNAL: u32 = 0xba58;
const BODY_EVENT_SIGNAL: u32 = 0xec98;
/// The physics object's `CState`.
const PHYSICS_STATE: u32 = 0x20;
/// The hinged body's parent, its entity link (+0x10, its list at +0x18), its owner word, cell link, orientation,
/// position, velocities, accumulators and the start pose.
const BODY_PARENT: u32 = 0x44;
const BODY_LINK: u32 = 0x10;
const BODY_LINK_LIST: u32 = 0x18;
const BODY_OWNER_WORD: u32 = 0x30;
const BODY_CELL_LINK: u32 = 0x160;
const BODY_ORIENTATION: u32 = 0x100;
const BODY_POSITION: u32 = 0x140;
const BODY_ANGULAR: u32 = 0x190;
const BODY_LINEAR: u32 = 0x1c0;
const BODY_START_ORIENTATION: u32 = 0x2a0;
const BODY_START_POSITION: u32 = 0x330;
/// The body's slots `SimFD00` calls: 21 (`E47_Vf54Helper`), 36 (`E43Vf90`) and 18 (`E43Vf48`).
pub const FD00_SLOT_54: u32 = 0x54;
pub const FD00_SLOT_90: u32 = 0x90;
pub const FD00_SLOT_48: u32 = 0x48;

fn copy16(mem: &mut Mem, to: u32, from: u32) {
    mem.copy(to, from, 16);
}

/// The original callees of `CHingedRigidBody::SimDA60`, at their entry stack pointers.
pub trait DA60Calls {
    /// `E33Mat34::ComposeA` (0x51b4c0) with the car as ecx.
    fn compose_a(&mut self, mem: &mut Mem, sp: u32, car: u32, args: [u32; 3]);
    /// `E15Shell::M83DD0` (0x483dd0) with the car's orientation as ecx.
    fn quat_mul(&mut self, mem: &mut Mem, sp: u32, this: u32, b: u32, out: u32);
}

/// `CHingedRigidBody::SimDA60` (0x51da60): the body's position and orientation go to the world through its car, and the
/// start pose takes them. `frame` is the original's esp after its prologue, `((entry - 4) & !0xf) - 0x40`.
pub fn sim_da60(mem: &mut Mem, this: u32, frame: u32, calls: &mut impl DA60Calls) {
    let walker = mem.u32(ROOT_IDENTITY);
    let car = mem.u32(this + HINGE_OWNER);
    calls.compose_a(mem, frame - 0x10, car, [this + BODY_POSITION, walker, frame + 0x20]);
    copy16(mem, this + BODY_POSITION, frame + 0x20);
    let car = mem.u32(this + HINGE_OWNER);
    calls.quat_mul(mem, frame - 0xc, car + BODY_ORIENTATION, this + BODY_ORIENTATION, frame + 0x30);
    copy16(mem, this + BODY_ORIENTATION, frame + 0x30);
    copy16(mem, this + BODY_START_POSITION, this + BODY_POSITION);
    copy16(mem, this + BODY_START_ORIENTATION, this + BODY_ORIENTATION);
}

/// The original callees of `CHingedRigidBody::SimFD00`, at their entry stack pointers.
pub trait FD00Calls {
    /// `CHingedRigidBody::SimDA60` (0x51da60).
    fn sim_da60(&mut self, mem: &mut Mem, sp: u32, body: u32);
    /// `CCar::ToWorld` (0x4e68e0).
    fn to_world(&mut self, mem: &mut Mem, sp: u32, car: u32, local: u32, out: u32);
    /// The body's virtual slot at `slot` (no arguments).
    fn slot(&mut self, mem: &mut Mem, sp: u32, body: u32, slot: u32);
    /// `CSolid::E43ClearAcc` (0x529e60).
    fn clear_accumulators(&mut self, mem: &mut Mem, sp: u32, body: u32);
    /// `E34Link10::UnlinkA` (0x4e41a0) on the body's entity link.
    fn unlink(&mut self, mem: &mut Mem, sp: u32, link: u32);
    /// `CEntity::E43_InitBD0` (0x519bd0).
    fn init_bd0(&mut self, mem: &mut Mem, sp: u32, body: u32);
    /// `CCellGrid::LookupTwin` (0x4e03b0) at the position's x and y words: returns the cell.
    fn lookup_twin(&mut self, mem: &mut Mem, sp: u32, grid: u32, x: u32, y: u32) -> u32;
    /// The cell move (0x4faeb0, cdecl).
    fn move_to_cell(&mut self, mem: &mut Mem, sp: u32, link: u32, cell: u32);
    /// `CPhysics::CState::RegWall` (0x5263d0).
    fn reg_wall(&mut self, mem: &mut Mem, sp: u32, state: u32, body: u32);
    /// `CMultiSignal1<CSigDetachFromParent>::Emit` (0x51f580).
    fn emit_detach(&mut self, mem: &mut Mem, sp: u32, signal: u32, sig: u32);
    /// `CMultiSignal1<CSigMrRBodyEvent>::Emit` (0x51f7c0).
    fn emit_event(&mut self, mem: &mut Mem, sp: u32, signal: u32, event: u32);
}

/// `CHingedRigidBody::SimFD00` (0x51fd00), the detach: a body with a parent and an entity list leaves its car. Its pose
/// and velocities go to the world, it leaves the car's list and is filed again in its cell and the physics state, and the
/// detach signal and a body event (kind 3, before and after) are emitted. Returns 1 for a detach, else 0. `frame` is the
/// original's esp after its prologue, `((entry - 4) & !0xf) - 0x90`; the event is at `frame + 0x40`.
pub fn sim_fd00(mem: &mut Mem, this: u32, frame: u32, calls: &mut impl FD00Calls) -> u32 {
    if mem.u32(this + BODY_PARENT) == 0 || mem.u32(this + BODY_LINK_LIST) == 0 {
        return 0;
    }
    let event = frame + 0x40;
    mem.set_u32(event + 4, mem.u32(this + BODY_OWNER_WORD));
    mem.set_u32(event, 3);
    copy16(mem, event + 0x30, this + BODY_POSITION);
    // The original's pointer locals, as it leaves them.
    mem.set_u32(frame + 0x1c, this + 0xf0);
    mem.set_u32(frame + 0x14, this + BODY_ORIENTATION);
    mem.set_u32(frame + 0x18, event + 0x40);
    copy16(mem, event + 0x40, this + BODY_ORIENTATION);
    calls.sim_da60(mem, frame - 4, this);
    detach_tail(mem, this, frame, event, frame + 0x10, calls);
    1
}

/// The calls `CMountedRigidBody::StepBig` makes besides the detach's (`FD00Calls`) and `M83DD0` (`DA60Calls`).
pub trait StepBigCalls: FD00Calls + DA60Calls {
    /// `CCar::WorldCompose` (0x4e6950) with the car as ecx: the point at `point` into the world, into `out`.
    fn world_compose(&mut self, mem: &mut Mem, sp: u32, car: u32, point: u32, out: u32);
    /// `E34Quat::Norm` (0x4e4c70) on the quaternion at `quat`: its unit quaternion into `out`.
    fn quat_norm(&mut self, mem: &mut Mem, sp: u32, quat: u32, out: u32);
}

/// `CMountedRigidBody::StepBig` (0x522100), the mounted body's detach: as `SimFD00`, with its own pose step. The position
/// goes to the world through the car's `WorldCompose`, the orientation through `M83DD0` and `Norm`, and the start pose
/// (+0x330, +0x2a0) takes the new pose; then the detach (`detach_tail`). Returns 1 for a detach, else 0. `frame` is the
/// original's esp after its prologue, `((entry - 4) & !0xf) - 0xb0`; the event is at `frame + 0x60`, the detach payload
/// at `frame + 0x18`, the composed position at `frame + 0x40` and the turned orientation at `frame + 0x50`.
pub fn step_big(mem: &mut Mem, this: u32, frame: u32, calls: &mut impl StepBigCalls) -> u32 {
    if mem.u32(this + BODY_PARENT) == 0 || mem.u32(this + BODY_LINK_LIST) == 0 {
        return 0;
    }
    let event = frame + 0x60;
    mem.set_u32(event, 3);
    mem.set_u32(event + 4, mem.u32(this + BODY_OWNER_WORD));
    copy16(mem, event + 0x30, this + BODY_POSITION);
    // The original's pointer locals, as it leaves them.
    mem.set_u32(frame + 0x14, this + BODY_ORIENTATION);
    mem.set_u32(frame + 0x1c, event + 0x40);
    copy16(mem, event + 0x40, this + BODY_ORIENTATION);
    let car = mem.u32(this + HINGE_OWNER);
    calls.world_compose(mem, frame - 0xc, car, this + BODY_POSITION, frame + 0x40);
    copy16(mem, this + BODY_POSITION, frame + 0x40);
    let car = mem.u32(this + HINGE_OWNER);
    calls.quat_mul(mem, frame - 0xc, car + BODY_ORIENTATION, this + BODY_ORIENTATION, frame + 0x50);
    calls.quat_norm(mem, frame - 8, frame + 0x50, this + BODY_ORIENTATION);
    copy16(mem, this + BODY_START_POSITION, this + BODY_POSITION);
    mem.set_u32(frame + 0x14, this + BODY_ORIENTATION);
    mem.set_u32(frame + 0x1c, this + BODY_START_ORIENTATION);
    copy16(mem, this + BODY_START_ORIENTATION, this + BODY_ORIENTATION);
    detach_tail(mem, this, frame, event, frame + 0x18, calls);
    1
}

/// The detach after the pose, shared by `SimFD00` and `CMountedRigidBody::StepBig`: the velocities go to the world, the
/// body leaves its car's list and is filed again in its cell and the physics state, then the detach signal (its payload
/// at `sig`) and the body event at `event` (kind 3; its after pose filled here) are emitted. `frame` is the caller's esp
/// after its prologue: the calls and the locals are at the same depths in both.
fn detach_tail(mem: &mut Mem, this: u32, frame: u32, event: u32, sig: u32, calls: &mut impl FD00Calls) {
    for (at, local) in [(BODY_LINEAR, frame + 0x30), (BODY_ANGULAR, frame + 0x20)] {
        let car = mem.u32(this + HINGE_OWNER);
        calls.to_world(mem, frame - 0xc, car, this + at, local);
    }
    for (at, local) in [(BODY_LINEAR, frame + 0x30), (BODY_ANGULAR, frame + 0x20)] {
        let car = mem.u32(this + HINGE_OWNER);
        for k in 0..3 {
            let lane = hinge_detach::world_lane(mem.f32(car + at + 4 * k), mem.f32(local + 4 * k));
            mem.set_f32(this + at + 4 * k, lane);
        }
    }
    calls.slot(mem, frame - 4, this, FD00_SLOT_54);
    calls.clear_accumulators(mem, frame - 4, this);
    let list = mem.u32(this + BODY_LINK_LIST);
    if list != 0 {
        mem.set_u32(list + 8, mem.u32(list + 8).wrapping_sub(1));
        calls.unlink(mem, frame - 4, this + BODY_LINK);
    }
    mem.set_u32(this + BODY_PARENT, 0);
    mem.set_u32(this + HINGE_OWNER, 0);
    calls.slot(mem, frame - 4, this, FD00_SLOT_90);
    calls.init_bd0(mem, frame - 4, this);
    let (x, y) = (mem.u32(this + BODY_POSITION), mem.u32(this + BODY_POSITION + 4));
    let grid = mem.u32(CELL_GRID_GLOBAL);
    let cell = calls.lookup_twin(mem, frame - 0xc, grid, x, y);
    calls.move_to_cell(mem, frame - 0xc, this + BODY_CELL_LINK, cell);
    let state = mem.u32(physics_core::layout::physics(mem) + PHYSICS_STATE);
    calls.reg_wall(mem, frame - 8, state, this);
    let signals = mem.u32(SIGNAL_MANAGER);
    mem.set_u32(sig, mem.u32(this + BODY_OWNER_WORD));
    calls.emit_detach(mem, frame - 8, signals + DETACH_SIGNAL, sig);
    calls.slot(mem, frame - 4, this, FD00_SLOT_48);
    copy16(mem, event + 0x10, this + BODY_POSITION);
    mem.set_u32(frame + 0x14, this + BODY_ORIENTATION);
    mem.set_u32(frame + 0x1c, event + 0x20);
    copy16(mem, event + 0x20, this + BODY_ORIENTATION);
    let signals = mem.u32(SIGNAL_MANAGER);
    calls.emit_event(mem, frame - 8, signals + BODY_EVENT_SIGNAL, event);
}
