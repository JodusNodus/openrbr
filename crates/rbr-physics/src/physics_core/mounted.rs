//! `CMountedRigidBody::Vf64` (0x521740) and `Vf6c` (0x521d90), over the physics core's store. The calls into the original
//! are the host's, at the original's entry stack pointers; `frame` is the original's frame, whose words the walks use.

use super::accumulate::offset_540;
use super::layout::{MountedVf64Host, MountedVf6cHost};
use super::math::{mounted_step, mounted_velocity, Vec4};
use super::store::CoreStore;

/// `CMountedRigidBody::Vf64` (0x521740, thiscall) on the body at `this`; `frame` is the original's aligned frame (its
/// `esp` after the prologue's pushes), where the temporaries are: the mount point composed into the owner's frame
/// (`+0x50`), its velocity term less the CDef's origin (`+0x20`, three lanes; the fourth is the stack's), the origin's
/// copy (`+0x40`), the walk's output (`+0x60`) and the gap between the body and its mount point (`+0x30`, three lanes).
/// Then the velocity at `+0x230` (`math::mounted_velocity`). A body without an owner runs `CSolid::E43Vf64`. Returns
/// the original's eax (the velocity's address, or `E43Vf64`'s), None before any call when the CDef's origin is the
/// null sentinel (the original reads address 0).
pub fn mounted_vf64<S: CoreStore>(
    store: &mut S,
    host: &mut impl MountedVf64Host,
    this: u32,
    frame: u32,
) -> Option<u32> {
    let owner = store.mounted_owner(this);
    if owner == 0 {
        return Some(host.solid_vf64(frame - 4, this));
    }
    let base = store.origin_base();
    if base == 0 {
        return None;
    }
    let mount = store.mount_point_at(store.mounted_car(this));
    host.world_compose(frame - 0xc, owner, mount, frame + 0x50);
    offset_540(store, owner, frame + 0x50, frame + 0x20);
    store.copy_vec4(base + 0x10, frame + 0x40);
    let origin = Vec4::load(store, frame + 0x40);
    let term = Vec4::load(store, frame + 0x20);
    store.set_f32(frame + 0x20, term.x - origin.x);
    store.set_f32(frame + 0x24, term.y - origin.y);
    store.set_f32(frame + 0x28, term.z - origin.z);
    host.walk(frame - 0x10, frame + 0x20, owner, frame + 0x60);
    let position = store.mounted_position(this);
    let point = store.mount_point(store.mounted_car(this));
    let gap =
        Vec4 { x: position.x - point.x, y: position.y - point.y, z: position.z - point.z, w: store.f32(frame + 0x3c) };
    store.set_f32(frame + 0x30, gap.x);
    store.set_f32(frame + 0x34, gap.y);
    store.set_f32(frame + 0x38, gap.z);
    let velocity = mounted_velocity(Vec4::load(store, frame + 0x60), gap, store.mounted_linear(this));
    store.set_mount_velocity(this, velocity);
    Some(store.mount_velocity_at(this))
}

/// The signal manager's body event and state signals.
const BODY_EVENT_SIGNAL: u32 = 0xec98;
const BODY_STATE_SIGNAL: u32 = 0xe438;

/// `CMountedRigidBody::Vf6c(dt)` (0x521d90, thiscall, `ret 4`) on the body at `this`; `frame` is the original's aligned
/// frame (its `esp` after the prologue's pushes). A body without an owner runs `CSolid::E43Vf6C`. Else the velocity and
/// the position step (`math::mounted_step`), and the payloads are built in the frame as the asm builds them: on the
/// first step only, the body event at `+0x50` (1, the key, the position at `+0x10` and `+0x30`, the rotation at `+0x20`
/// and `+0x40`; `+0x08` and `+0x0c` are not written), then the state at `+0x20` (the key, the position at `+0x10`, the
/// rotation at `+0x20`; `+0x04..+0x10` not written). The rotation is read at `this + 0x100` (the asm's null test on
pub fn mounted_vf6c<S: CoreStore>(
    store: &mut S,
    host: &mut impl MountedVf6cHost,
    this: u32,
    dt: f32,
    frame: u32,
) -> u32 {
    if store.mounted_owner(this) == 0 {
        return host.solid_vf6c(frame - 8, this, dt);
    }
    let mount = store.mount_point(store.mounted_car(this));
    let step =
        mounted_step(store.mounted_linear(this), store.mount_velocity(this), store.mounted_position(this), mount, dt);
    store.set_mounted_linear(this, step.linear);
    store.set_mounted_position(this, step.position);
    if store.mounted_event_sent(this) == 0 {
        let sig = frame + 0x50;
        store.set_u32(sig + 4, store.mounted_key(this));
        store.set_u32(sig, 1);
        store.copy_mounted_rotation(this, sig + 0x20);
        store.copy_mounted_position(this, sig + 0x10);
        store.copy_vec4(sig + 0x20, sig + 0x40);
        store.copy_vec4(sig + 0x10, sig + 0x30);
        host.emit_event(frame - 8, store.signal_manager() + BODY_EVENT_SIGNAL, sig);
        store.set_mounted_event_sent(this, 1);
    }
    let sig = frame + 0x20;
    store.set_u32(sig, store.mounted_key(this));
    store.copy_mounted_rotation(this, sig + 0x20);
    store.copy_mounted_position(this, sig + 0x10);
    host.emit_state(frame - 8, store.signal_manager() + BODY_STATE_SIGNAL, sig)
}
