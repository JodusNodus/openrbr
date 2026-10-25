//! The body's window steps and scratch calls over the store: the back step, the rest of the window, the window start,
//! the scratch accumulate and the state rotation refresh.

use super::host::{CellHost, HostUnavailable, StateHost};
use super::layout::SOLID_STATE;
use super::rotation::{self, MulHost};
use super::scratch::{FlushHost, FLUSH_BIT};
use super::store::RigidStore;
use super::{emit, window};
use crate::collision;
use crate::collision::layout::body_hosts::Vf40Host;

/// `CSolid::S18` (0x529dc0, slot 18) on the body at `this`, after its children's walk (`CEntity::E43Vf48`, the
/// caller's): `emit::window_start`.
pub fn solid_window_start<S: RigidStore>(store: &mut S, this: u32) {
    let mut body = store.body(this);
    emit::window_start(&mut body);
    store.store_body(this, &body);
}

/// `CSolid::VfDx` (0x52cfd0, the back step) on the body at `this`: `window::back_step`. The body is stored when the
/// step stops, as the fields before a failed call are written.
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

/// `CSolid::VfEx` (0x52d110, the rest of the window) on the body at `this`: `window::advance_rest`, stored as
/// `solid_back_step` stores it.
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

/// `E47SolidState::Vf17x` (0x529a50): `state` is the body's state at +0xe0. Refreshes its matrix (which is the body's
/// `+0x110`) from the orientation, scaled by the diagonal when the gate is positive.
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

/// `E47_Vf16Helper` (0x52b6d0), `CSolid::Vf40` in the C++: `Vf17x` on the state at `+0xe0` twice, then the tail jump to
/// `CBody::Vf40` (0x4e1120) on the body. Returns that call's result.
pub fn vf16_helper<S: RigidStore>(
    store: &mut S,
    this: u32,
    host: &mut (impl MulHost + Vf40Host),
) -> Result<u32, HostUnavailable> {
    let state = this + SOLID_STATE;
    refresh_state_rotation(store, state, host)?;
    refresh_state_rotation(store, state, host)?;
    Ok(collision::layout::reset_state_and_base(host, this))
}

/// `CSolid::E43Vf74` (0x529ea0): the scratch accumulator gets `pp * scale` added; then the flush runs when `gate` has its
/// bit (the caller reads the gate from the app). The same sum as `scratch::accumulate`, on the store.
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
