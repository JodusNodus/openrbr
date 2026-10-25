//! The hinged body's sequence functions over the store: the loads, the port, the stores, in the original's order
//! (`CHingedRigidBody::SimF950`, `SimE1D0`, `SimEAD0`, `SimCFF0`, `SimD160`, the end stops and the limit sums). The words
//! are the store's ([`HingeStore`]).

use super::hinge_advance;
use super::hinge_limits::{self, LimitStep, RotationBlock};
use super::hinge_pivot::{self, PivotCalls};
use super::hinge_pose::{self, PoseCalls};
use super::hinge_slip::{self, SlipCalls};
use super::hinge_solve::{self, Outcome, SolveCalls};
use super::hinge_stops;
use super::layout::{angle_tweak, pivot_frame, position_at, rate_tweak, scaled_vector, slip_frame, solve_frame};
use super::store::HingeStore;

/// `CHingedRigidBody::SimCF10(idx)` at `this` (0x51cf10): the residual, and the limit state it moves. Any index reads the
/// block and the state word its address gives (SimEA80 passes 0 and 1).
pub fn hinge_limit_residual<S: HingeStore>(store: &mut S, this: u32, idx: u32) -> LimitStep {
    let limit = store.body_limit(this, idx);
    let (angle, rate) = store.angle_rate(this);
    let step = hinge_limits::limit_torque(&limit, store.limit_position(this, idx), angle, rate);
    if let Some(next) = step.new_position {
        store.store_limit_position(this, idx, next);
    }
    step
}

/// `CHingedRigidBody::SimEA80` at `this` (0x51ea80): the sum of the active limits' residuals. The limit states the
/// limits move are stored back (both words, in order).
pub fn hinge_limit_sum<S: HingeStore>(store: &mut S, this: u32) -> f32 {
    let limits = store.body_limits(this);
    let mut state = store.load_hinge_state(this);
    let sum = hinge_limits::limit_torque_sum(&limits, &mut state);
    store.store_limit_positions(this, state.spring_position);
    sum
}

/// `CHingedRigidBody::SimC960` at `this` (0x51c960): the rotation block of the hinge angle.
pub fn hinge_rotation<S: HingeStore>(store: &mut S, this: u32) -> RotationBlock {
    let block = hinge_limits::rotation_block(store.hinge_angle(this));
    store.store_rotation_block(this, &block);
    block
}

/// `CEntity` 0x51f280 on `this`: reads the fields, takes the window, stores it, calls the limit object and stores the
/// residual, in the original's order. Returns eax.
pub fn limit_update_at<S: HingeStore>(store: &mut S, this: u32, calls: &mut impl hinge_stops::Calls) -> u32 {
    let Some(update) = hinge_stops::apply_stops(&store.limit_inputs(this)) else {
        return store.hinge_definition(this);
    };
    store.store_limit_window(this, update.record);
    if let Some(rate) = update.rate {
        calls.mark_dirty();
        store.store_limit_rate(this, rate);
    }
    update.result
}

/// `CHingedRigidBody::SimF950` (0x51f950) on `this` with the step `dt`. `sig` is the state payload's slot in the
/// original's frame (its stack); the payload's padding words are not written, as the original does not write them.
/// Returns eax: the link's flag pointer for a settled body (the asm returns the pointer it loaded), the integrate's for a
/// body with no owner car, else the emit's.
pub fn sim_f950_at<S: HingeStore>(
    store: &mut S,
    this: u32,
    dt: f32,
    sig: u32,
    calls: &mut impl hinge_advance::Calls,
) -> u32 {
    if store.hinge_linked(this) {
        return store.hinge_link(this);
    }
    if !store.hinge_owned(this) {
        return calls.integrate_unowned(this, dt);
    }
    let input = store.advance_input(this);
    let step = hinge_advance::advance(input.solved, dt, input.rate, input.angle);
    calls.mark_dirty(rate_tweak(this));
    store.store_rate(this, step.rate);
    calls.scale_vector(store.rate_axis(this), step.rate, scaled_vector(this));
    store.clear_linear_velocity(this);
    calls.mark_dirty(angle_tweak(this));
    store.store_angle(this, step.angle);
    calls.apply_stops(this);
    calls.update_pose(this);
    let payload = store.state_payload(this);
    store.store_state_payload(sig, &payload);
    calls.emit_state(store.state_signal(), sig)
}

/// `CHingedRigidBody::SimCFF0` (0x51cff0) at `this`: the slip term, through `calls` (its walks and copy).
pub fn sim_cff0_at<S: HingeStore>(store: &S, this: u32, base: u32, calls: &mut impl SlipCalls) -> f32 {
    hinge_slip::slip_torque(calls, &store.slip_input(this), &slip_frame(base))
}

/// `CHingedRigidBody::SimE1D0` (0x51e1d0) at `this`: the pivot and the settle value, through `calls`, stored in the body.
/// Returns eax: `def + 0x2c0` when `def + 0x2b0` is set, else 0 (the asm's last load).
pub fn sim_e1d0_at<S: HingeStore>(store: &mut S, this: u32, base: u32, calls: &mut impl PivotCalls) -> u32 {
    let input = store.pivot_input(this);
    let pivot = hinge_pivot::pivot_terms(calls, &input, &pivot_frame(base, this));
    store.store_pivot(this, &pivot);
    store.settle_axis(this)
}

/// `CHingedRigidBody::SimD160` (0x51d160) at `this`: the frame's products, through `calls`. Returns eax: `this + 0x140`,
/// which `E01Vec4::CopyFrom` returns (the asm's last call).
pub fn sim_d160_at<S: HingeStore>(store: &S, this: u32, calls: &mut impl PoseCalls) -> u32 {
    hinge_pose::update_pose(calls, &store.d160_frame(this));
    position_at(this)
}

/// `CHingedRigidBody::SimEAD0` (0x51ead0) at `this`: the hinge solve, through `calls`. A solved value is stored at +0x624.
pub fn sim_ead0_at<S: HingeStore>(store: &mut S, this: u32, base: u32, calls: &mut impl SolveCalls) -> Outcome {
    let outcome = hinge_solve::hinge_solve(calls, this, &solve_frame(base));
    if let Outcome::Solved(value) = outcome {
        store.store_solved(this, value);
    }
    outcome
}
