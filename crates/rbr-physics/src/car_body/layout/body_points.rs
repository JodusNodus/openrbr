//! `CCarBody`'s point scatters (`Vf9Cx`, `VfA0x`) and `CCarBody::CState::Vf44x`.

use crate::car_body;
use crate::car_body::body::BodyCalls;
use crate::car_body::body_grid::GridCalls;
use crate::car_body::store::CarStore;

/// `CCarBody::CState::Vf44x` (0x4eb6a0): the flag at +0x1c is set, the owner's grid is cleared over the bounds (their
/// vectors at `scratch` (plus) and `scratch + 0x10` (minus)), then plus is copied to +0x20 and minus to +0x30. Returns
/// the eax the original leaves (the last copy's destination). None when the state has no owner: the original faults
/// there, before anything is written, so the caller declines.
pub fn state_vf44x<S: CarStore>(calls: &mut impl GridCalls, mem: &mut S, state: u32, scratch: u32) -> Option<u32> {
    let owner = mem.u32(state + 0x40);
    if owner == 0 {
        return None;
    }
    mem.set_u32(state + 0x1c, 1);
    let (plus, minus) = car_body::body_grid::grid_bounds();
    plus.store(mem, scratch);
    minus.store(mem, scratch + 0x10);
    let grid = calls.grid(owner);
    calls.clear_grid(owner, grid, scratch, scratch + 0x10);
    calls.copy_from(state + 0x20, scratch);
    Some(calls.copy_from(state + 0x30, scratch + 0x10))
}

/// `CCarBody::Vf9Cx` and `VfA0x` (0x4eb820, 0x4eb8c0) on the `n` points at `out`: the gate and the base are read before
/// the body's slot 0 runs (the port's order), then the points are written back (lanes 0 to 2).
pub fn scatter_points<S: CarStore>(calls: &mut impl BodyCalls, mem: &mut S, body: u32, out: u32, n: u32, sub: bool) {
    let gate = mem.body_pre_step_gate();
    let base = mem.body_base(body);
    let mut points = mem.load_points(out, n);
    if sub {
        car_body::body::subtract_points(calls, gate, body, base, &mut points);
    } else {
        car_body::body::add_points(calls, gate, body, base, &mut points);
    }
    mem.store_points(out, &points);
}
