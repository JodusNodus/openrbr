//! `CPassenger::Step` (0x523160) and `E45PSmalls::M2BC0` (0x522bc0), over the physics core's store.

use super::layout::passenger_ctor;
use super::passenger;
use super::store::CoreStore;

/// `CPassenger::Step` (0x523160) on the passenger at `this`: the walk of the gravity point (`host.walk`), then the spring
/// and the pull (`host.spring`, `host.pull`). Returns the eax the original leaves, or None before any call when the
/// gravity pointer is the sentinel.
pub fn passenger_step<S: CoreStore>(
    store: &mut S,
    host: &mut impl passenger_ctor::StepHost,
    this: u32,
    dt: f32,
) -> Option<u32> {
    let gravity = store.gravity_point()?;
    let body = store.passenger_step_host(this);
    let offset = store.host_offset(body);
    host.walk(passenger::walk_point(gravity, offset), body, store.passenger_velocity_at(this));
    host.spring(this, dt);
    Some(host.pull(this, dt))
}

/// `E45PSmalls::M2BC0` (0x522bc0) on the passenger at `this`: the pull on its host's vectors (`passenger::pull_to_rest`).
/// Returns the eax the original leaves, the host's +0x90 vector.
pub fn passenger_pull<S: CoreStore>(store: &mut S, this: u32, dt: f32) -> u32 {
    let host = store.passenger_host(this);
    let (h40, h90) = store.host_pull_vectors(host);
    let (h40_next, h90_next) = passenger::pull_to_rest(h40, h90, store.passenger_velocity(this), dt);
    store.store_host_pull_vectors(host, h40_next, h90_next);
    store.host_pull_at(host)
}
