//! The body's window steps and scratch calls over the store: the back step, the rest of the window, the window start,
//! the scratch accumulate and the state rotation refresh.

use super::host::{CellHost, HostUnavailable, StateHost};
use super::rotation::{self, MulHost};
use super::scratch::{FlushHost, FLUSH_BIT};
use super::store::RigidStore;
use super::{emit, window};
use crate::collision;
use crate::collision::layout::body_hosts::Vf40Host;

/// The window start on the body at `this`, after its children's walk: `emit::window_start`.
pub fn solid_window_start<S: RigidStore>(store: &mut S, this: u32) {
    let mut body = store.body(this);
    emit::window_start(&mut body);
    store.store_body(this, &body);
}

/// The back step on the body at `this`: `window::back_step`. The body is stored when the step stops, as the fields
/// before a failed call are written.
pub fn solid_back_step<S: RigidStore>(
    store: &mut S,
    host: &mut (impl CellHost + StateHost),
    this: u32,
    a: f32,
) -> Result<(), HostUnavailable> {
    let mut body = store.body(this);
    let done = window::back_step(&mut body, a, host);
    store.store_body(this, &body);
    done
}

/// The rest of the window on the body at `this`: `window::advance_rest`, stored as `solid_back_step` stores it.
pub fn solid_advance_rest<S: RigidStore>(
    store: &mut S,
    host: &mut (impl CellHost + StateHost),
    this: u32,
    a: f32,
) -> Result<(), HostUnavailable> {
    let mut body = store.body(this);
    let done = window::advance_rest(&mut body, a, host);
    store.store_body(this, &body);
    done
}

/// Refreshes the state's matrix from the orientation, scaled by the diagonal when the gate is positive.
pub fn refresh_state_rotation<S: RigidStore>(
    store: &mut S,
    state: u32,
    host: &mut impl MulHost,
) -> Result<(), HostUnavailable> {
    let input = store.state_rotation(state);
    let m = rotation::refresh_rotation(input.orientation, input.gate, input.diagonal, host)?;
    store.store_state_matrix(state, m);
    Ok(())
}

/// Refreshes the state matrix twice, then the body's base reset. Returns that call's result.
pub fn vf16_helper<S: RigidStore>(
    store: &mut S,
    this: u32,
    host: &mut (impl MulHost + Vf40Host),
) -> Result<u32, HostUnavailable> {
    let state = store.state_at(this);
    refresh_state_rotation(store, state, host)?;
    refresh_state_rotation(store, state, host)?;
    Ok(collision::layout::reset_state_and_base(host, this))
}

/// The scratch accumulator gets `pp * scale` added; then the flush runs when `gate` has its bit. The same sum as
/// `scratch::accumulate`, on the store.
pub fn scratch_accumulate<S: RigidStore>(
    store: &mut S,
    this: u32,
    pp: u32,
    gate: u32,
    host: &mut impl FlushHost,
) -> Result<(), HostUnavailable> {
    let scale = store.scratch_scale(this);
    let scratch = store.scratch_accum(this);
    let sum = scratch.add_scaled(store.row(pp), scale);
    store.store_scratch_accum(this, sum);
    if gate & FLUSH_BIT != 0 {
        host.flush()?;
    }
    Ok(())
}
