//! The body's force accumulation and point-velocity write, over the physics core's store.

use super::layout::AccumHost;
use super::math::{gravity_force, point_velocity, Vec4};
use super::store::CoreStore;

/// Accumulates the forces on the body at `body`: the hook when the log flag 0x4000 is set (of the
/// CDef's word or the physics object's), the base, and, when the body's inverse mass is not 0, the gravity walked through
/// the body and scaled into the force. `temp` is the scratch the walk writes (the gravity copy, then the
/// walk's output). Returns None before any call when the gravity pointer is the null sentinel.
pub fn accumulate_forces<S: CoreStore>(store: &mut S, host: &mut impl AccumHost, body: u32, temp: u32) -> Option<()> {
    store.gravity_point()?;
    if store.accumulate_hook() {
        host.hook(body);
    }
    host.base(body);
    if store.inverse_mass(body) == 0.0 {
        return Some(());
    }
    store.copy_gravity(temp);
    let out = store.accumulate_out(temp);
    let point = host.point(body, out);
    host.walk(temp, point, out);
    if let Some(force) = gravity_force(Vec4::load(store, out), store.inverse_mass(body)) {
        force.store(store, out);
        host.accumulate(body, out);
    }
    Some(())
}

/// The point-velocity write on the body at `this`: the first three lanes of `out` are the velocity
/// term of the point at `point` (`math::point_velocity`); its fourth lane is not written.
pub fn offset_540<S: CoreStore>(store: &mut S, this: u32, point: u32, out: u32) {
    let v = point_velocity(&store.body_motion(this), Vec4::load(store, point));
    for (k, lane) in v.into_iter().enumerate() {
        store.set_f32(out + 4 * k as u32, lane);
    }
}
