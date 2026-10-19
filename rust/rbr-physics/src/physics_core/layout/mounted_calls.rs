//! `CMountedRigidBody::Vf64` (0x521740) and `Vf6c` (0x521d90).

use super::globals::{physics_global, signal_manager};
use super::lists::{copy_vec4, set_word, word};
use super::passenger_step::PHYSICS_DEF_AT;
use super::vectors::{offset_540, set_vec4s};
use crate::mem::Mem;
use crate::physics_core::math::{mounted_step, mounted_velocity, Vec4};

/// The calls `CMountedRigidBody::Vf64` makes, each entered at the stack pointer `sp` the original's call has.
pub trait MountedVf64Host {
    /// `CSolid::E43Vf64` (0x52cb20) on the body: the path of a body without an owner. Returns its eax.
    fn solid_vf64(&mut self, sp: u32, body: u32) -> u32;
    /// `CCar::WorldCompose` (0x4e6950) on the owner: the point at `point` into the owner's frame, into `out`.
    fn world_compose(&mut self, sp: u32, owner: u32, point: u32, out: u32);
    /// `E43PairWalk::MB560` on the walker: `src` through the owner, into `out`.
    fn walk(&mut self, sp: u32, src: u32, owner: u32, out: u32);
}

/// The owner (`+0x3d0`) and the mount point (`+0x220` of the definition at `+0x34`) of a mounted body.
const MOUNT_OWNER_AT: u32 = 0x3d0;
const MOUNT_POINT_AT: u32 = 0x220;
/// The CDef's origin (`+0x160b0`; the original's null test is on `+0x160a0`).
const ORIGIN_BASE_AT: u32 = 0x160a0;

/// `CMountedRigidBody::Vf64` (0x521740, thiscall) on the body at `this`; `frame` is the original's aligned frame (its
/// `esp` after the prologue's pushes), where the temporaries are: the mount point composed into the owner's frame
/// (`+0x50`), its velocity term less the CDef's origin (`+0x20`, three lanes; the fourth is the stack's), the origin's
/// copy (`+0x40`), the walk's output (`+0x60`) and the gap between the body and its mount point (`+0x30`, three lanes).
/// Then the velocity at `+0x230` (`math::mounted_velocity`). A body without an owner runs `CSolid::E43Vf64`. Returns
/// the original's eax (the velocity's address, or `E43Vf64`'s), None before any call when the CDef's origin is the
/// null sentinel (the original reads address 0).
pub fn mounted_vf64(mem: &mut Mem, host: &mut impl MountedVf64Host, this: u32, frame: u32) -> Option<u32> {
    let owner = word(mem, this + MOUNT_OWNER_AT);
    if owner == 0 {
        return Some(host.solid_vf64(frame - 4, this));
    }
    let base = word(mem, physics_global(mem) + PHYSICS_DEF_AT).wrapping_add(ORIGIN_BASE_AT);
    if base == 0 {
        return None;
    }
    let mount = word(mem, this + 0x34) + MOUNT_POINT_AT;
    host.world_compose(frame - 0xc, owner, mount, frame + 0x50);
    offset_540(mem, owner, frame + 0x50, frame + 0x20);
    copy_vec4(mem, base + 0x10, frame + 0x40);
    let origin = Vec4::load(mem, frame + 0x40);
    let term = Vec4::load(mem, frame + 0x20);
    mem.set_f32(frame + 0x20, term.x - origin.x);
    mem.set_f32(frame + 0x24, term.y - origin.y);
    mem.set_f32(frame + 0x28, term.z - origin.z);
    host.walk(frame - 0x10, frame + 0x20, owner, frame + 0x60);
    let position = Vec4::load(mem, this + 0x140);
    let point = Vec4::load(mem, word(mem, this + 0x34) + MOUNT_POINT_AT);
    let gap =
        Vec4 { x: position.x - point.x, y: position.y - point.y, z: position.z - point.z, w: mem.f32(frame + 0x3c) };
    mem.set_f32(frame + 0x30, gap.x);
    mem.set_f32(frame + 0x34, gap.y);
    mem.set_f32(frame + 0x38, gap.z);
    let velocity = mounted_velocity(Vec4::load(mem, frame + 0x60), gap, Vec4::load(mem, this + 0x1c0));
    set_vec4s(mem, this + 0x230, &[velocity]);
    Some(this + 0x230)
}

/// The calls `CMountedRigidBody::Vf6c` makes, each entered at the stack pointer `sp` the original's call has. Each
/// returns its eax.
pub trait MountedVf6cHost {
    /// `CSolid::E43Vf6C(dt)` (0x52cc30) on the body: the path of a body without an owner.
    fn solid_vf6c(&mut self, sp: u32, body: u32, dt: f32) -> u32;
    /// `CMultiSignal1<const CSigMrRBodyEvent&>::Emit` (0x51f7c0) on the signal at `signal` with the payload at `sig`.
    fn emit_event(&mut self, sp: u32, signal: u32, sig: u32) -> u32;
    /// `CMultiSignal1<const CSigMrRBodyState&>::Emit` (0x51f700) on the signal at `signal` with the payload at `sig`.
    fn emit_state(&mut self, sp: u32, signal: u32, sig: u32) -> u32;
}

/// A mounted body's key (`+0x30`), its rotation vector (`+0x100`) and the word that is set once its first step has
/// emitted the body event (`+0x584`); the signal manager's body event and state signals.
const MOUNT_KEY_AT: u32 = 0x30;
const MOUNT_ROTATION_AT: u32 = 0x100;
const MOUNT_EVENT_SENT_AT: u32 = 0x584;
const BODY_EVENT_SIGNAL: u32 = 0xec98;
const BODY_STATE_SIGNAL: u32 = 0xe438;

/// `CMountedRigidBody::Vf6c(dt)` (0x521d90, thiscall, `ret 4`) on the body at `this`; `frame` is the original's aligned
/// frame (its `esp` after the prologue's pushes). A body without an owner runs `CSolid::E43Vf6C`. Else the velocity and
/// the position step (`math::mounted_step`), and the payloads are built in the frame as the asm builds them: on the
/// first step only, the body event at `+0x50` (1, the key, the position at `+0x10` and `+0x30`, the rotation at `+0x20`
/// and `+0x40`; `+0x08` and `+0x0c` are not written), then the state at `+0x20` (the key, the position at `+0x10`, the
/// rotation at `+0x20`; `+0x04..+0x10` not written). The rotation is read at `this + 0x100` (the asm's null test on
/// `this + 0xf0` is for no real body). Returns the original's eax: the state emit's, or `E43Vf6C`'s.
pub fn mounted_vf6c(mem: &mut Mem, host: &mut impl MountedVf6cHost, this: u32, dt: f32, frame: u32) -> u32 {
    if word(mem, this + MOUNT_OWNER_AT) == 0 {
        return host.solid_vf6c(frame - 8, this, dt);
    }
    let mount = Vec4::load(mem, word(mem, this + 0x34) + MOUNT_POINT_AT);
    let step = mounted_step(
        Vec4::load(mem, this + 0x1c0),
        Vec4::load(mem, this + 0x230),
        Vec4::load(mem, this + 0x140),
        mount,
        dt,
    );
    set_vec4s(mem, this + 0x1c0, &[step.linear]);
    set_vec4s(mem, this + 0x140, &[step.position]);
    if word(mem, this + MOUNT_EVENT_SENT_AT) == 0 {
        let sig = frame + 0x50;
        set_word(mem, sig + 4, word(mem, this + MOUNT_KEY_AT));
        set_word(mem, sig, 1);
        copy_vec4(mem, this + MOUNT_ROTATION_AT, sig + 0x20);
        copy_vec4(mem, this + 0x140, sig + 0x10);
        copy_vec4(mem, sig + 0x20, sig + 0x40);
        copy_vec4(mem, sig + 0x10, sig + 0x30);
        host.emit_event(frame - 8, signal_manager(mem) + BODY_EVENT_SIGNAL, sig);
        set_word(mem, this + MOUNT_EVENT_SENT_AT, 1);
    }
    let sig = frame + 0x20;
    set_word(mem, sig, word(mem, this + MOUNT_KEY_AT));
    copy_vec4(mem, this + MOUNT_ROTATION_AT, sig + 0x20);
    copy_vec4(mem, this + 0x140, sig + 0x10);
    host.emit_state(frame - 8, signal_manager(mem) + BODY_STATE_SIGNAL, sig)
}
