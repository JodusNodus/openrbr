//! The mounted body's velocity pull and position step, over the physics core's store. The calls into the
//! host run through the layout's frames, whose words the walks use.

use super::accumulate::offset_540;
use super::layout::mounted_calls::{PullFrame, StepFrame};
use super::layout::{MountedVf64Host, MountedVf6cHost};
use super::math::{mounted_step, mounted_velocity, Vec3, Vec4};
use super::store::CoreStore;

/// The mounted body's velocity pull on the body at `this`, through the layout's pull frame: the mount point composed
/// into the owner's frame, its velocity term less the CDef's origin, the origin's copy, the walk's output and the gap
/// between the body and its mount point. Then the mount-point velocity (`math::mounted_velocity`). A body without an
/// owner runs the solid body's pull. Returns the velocity's address, None before any call when the CDef's origin is the
/// null sentinel.
pub fn mounted_vf64<S: CoreStore>(
    store: &mut S,
    host: &mut impl MountedVf64Host,
    this: u32,
    frame: u32,
) -> Option<u32> {
    let frame = PullFrame::new(frame);
    let owner = store.mounted_owner(this);
    if owner == 0 {
        return Some(host.solid_vf64(frame.solid_sp(), this));
    }
    let base = store.origin_base();
    if base == 0 {
        return None;
    }
    let mount = store.mount_point_at(store.mounted_car(this));
    host.world_compose(frame.compose_sp(), owner, mount, frame.composed());
    offset_540(store, owner, frame.composed(), frame.term());
    store.copy_origin(base, frame.origin());
    let origin = Vec4::load(store, frame.origin());
    let term = Vec4::load(store, frame.term());
    Vec3 { x: term.x - origin.x, y: term.y - origin.y, z: term.z - origin.z }.store(store, frame.term());
    host.walk(frame.walk_sp(), frame.term(), owner, frame.walked());
    let position = store.mounted_position(this);
    let point = store.mount_point(store.mounted_car(this));
    let gap =
        Vec4 { x: position.x - point.x, y: position.y - point.y, z: position.z - point.z, w: store.f32(frame.gap_w()) };
    Vec3 { x: gap.x, y: gap.y, z: gap.z }.store(store, frame.gap());
    let velocity = mounted_velocity(Vec4::load(store, frame.walked()), gap, store.mounted_linear(this));
    store.set_mount_velocity(this, velocity);
    Some(store.mount_velocity_at(this))
}

/// The mounted body's position step on the body at `this`, through the layout's step frame. A body without an owner
/// runs the solid body's step. Else the velocity and the position step (`math::mounted_step`), and the payloads are
/// built in the frame: on the first step only, the body event, then the state.
pub fn mounted_vf6c<S: CoreStore>(
    store: &mut S,
    host: &mut impl MountedVf6cHost,
    this: u32,
    dt: f32,
    frame: u32,
) -> u32 {
    let frame = StepFrame::new(frame);
    if store.mounted_owner(this) == 0 {
        return host.solid_vf6c(frame.solid_sp(), this, dt);
    }
    let mount = store.mount_point(store.mounted_car(this));
    let step =
        mounted_step(store.mounted_linear(this), store.mount_velocity(this), store.mounted_position(this), mount, dt);
    store.set_mounted_linear(this, step.linear);
    store.set_mounted_position(this, step.position);
    if store.mounted_event_sent(this) == 0 {
        let sig = frame.event();
        store.set_u32(sig.word1(), store.mounted_key(this));
        store.set_u32(sig.word0(), 1);
        store.copy_mounted_rotation(this, sig.rotation());
        store.copy_mounted_position(this, sig.position());
        store.copy_vec4(sig.rotation(), sig.rotation_copy());
        store.copy_vec4(sig.position(), sig.position_copy());
        host.emit_event(frame.emit_sp(), store.body_event_signal(), sig.word0());
        store.set_mounted_event_sent(this, 1);
    }
    let sig = frame.state();
    store.set_u32(sig.word0(), store.mounted_key(this));
    store.copy_mounted_rotation(this, sig.rotation());
    store.copy_mounted_position(this, sig.position());
    host.emit_state(frame.emit_sp(), store.body_state_signal(), sig.word0())
}
