//! The contact loop: the two contact advances and the collide work, over the physics core's store.

use super::contact::event_before_step_end;
use super::detectors::{fraction_overshoots, step_fraction, window_bounds, window_left};
use super::layout::collide_work::{
    CollideStack, COLLIDE_NAME, COLLIDE_SAMPLES, COLLIDE_SAMPLE_SIZE, EVENT_SOLIDS_AT, SLOT_80,
};
use super::layout::contact_walk::SolidHost;
use super::layout::detector_hosts::CollideHost;
use super::layout::{SLOT_48, SLOT_54, SOLVER_POST_SLOT};
use super::store::CoreStore;

/// The first contact advance on the physics at `this`: over the scratch's event list, each solid of an event
/// that takes part and carries the flag steps back by the event's penetration scalar. The float
/// argument is unused.
pub fn advance_contact_bodies<S: CoreStore>(store: &mut S, host: &mut impl SolidHost, this: u32) {
    let mut event = store.scratch_events(this);
    while event != 0 {
        for at in EVENT_SOLIDS_AT {
            let solid = store.event_solid(event, at);
            if host.takes_part(store.node_slot(solid, SLOT_80), solid) != 0 && store.solid_advanced(solid) {
                let sample = store.event_penetration(event);
                host.back_step(solid, sample);
            }
        }
        event = store.event_next(event);
    }
}

/// The second contact advance on the physics at `this`: over the scratch's event list, an event whose
/// scalar is below 1.0 advances each solid that takes part and carries the flag by `window`.
pub fn advance_contact_bodies_remainder<S: CoreStore>(
    store: &mut S,
    host: &mut impl SolidHost,
    this: u32,
    window: f32,
) {
    let mut event = store.scratch_events(this);
    while event != 0 {
        if event_before_step_end(store.event_penetration(event)) {
            for at in EVENT_SOLIDS_AT {
                let solid = store.event_solid(event, at);
                if host.takes_part(store.node_slot(solid, SLOT_80), solid) != 0 && store.solid_advanced(solid) {
                    host.advance_rest(solid, window);
                }
            }
        }
        event = store.event_next(event);
    }
}

/// The CDef flag that counts the solver's hits.
const COUNT_HITS_FLAG: u32 = 0x20000;

/// The collide work on the physics at `this`. The pass counter and the hit counts restart; while the window is above
/// its floor (once at most: the budget is 1) a profiled detect pass and the solver's sweep give a fraction; at or
/// below 1.1 the contacts step by its clamped value (the first advance, the solver's pass, the bodies' step, the second
/// advance) and the window shrinks, above it the window ends. Then the bodies' post passes, and the pass maximum.
/// Returns the pass count.
pub fn collide_window<S: CoreStore>(
    store: &mut S,
    host: &mut impl CollideHost,
    this: u32,
    window: f32,
    entry: u32,
) -> u32 {
    let stack = CollideStack::new(entry);
    let chan = stack.channel();
    let (offset, floor) = window_bounds(window);
    let mut left = window;
    store.set_pass_count(this, 0);
    store.set_hits(this, 0);
    store.set_hits_last(this, 0);
    if window > floor {
        let mut budget = 1;
        let mut sample = COLLIDE_SAMPLES;
        while budget != 0 {
            budget -= 1;
            store.set_pass_count(this, store.pass_count(this).wrapping_add(1));
            host.chan_enter(stack.enter_sp(), chan, sample, COLLIDE_NAME);
            host.run_collision_detectors(stack.worker_sp(), this, 0);
            host.prof_exit(stack.call_sp(), chan);
            let events = store.scratch_events(this);
            host.telemetry_70(stack.worker_sp(), store.telemetry(), events);
            host.solver_sweep_events(stack.call_sp(), store.solver(this));
            let fraction = host.solver_fraction(stack.call_sp(), store.solver(this));
            // The original's `fst` to its argument slot, which it reuses for the clamped fraction.
            stack.record_fraction(store, fraction as f32);
            if fraction_overshoots(fraction) {
                left = 0.0;
            } else {
                if store.collide_flags() & COUNT_HITS_FLAG != 0 {
                    let hits = host.solver_hit_count(stack.call_sp(), store.solver(this));
                    store.set_hits(this, hits);
                }
                if store.collide_flags() & COUNT_HITS_FLAG != 0 {
                    let hits = host.solver_hit_count(stack.call_sp(), store.solver(this));
                    store.set_hits_last(this, hits);
                    if hits > store.hits_maximum(this) {
                        store.set_hits_maximum(this, hits);
                    }
                }
                let step = step_fraction(fraction as f32, offset, left);
                stack.record_fraction(store, step);
                host.advance_contact_bodies(stack.worker_sp(), this, step);
                host.solver_pass(stack.call_sp(), store.solver(this));
                let mut node = store.bodies_head(store.physics_state(this));
                while node != 0 {
                    host.entity_48(stack.call_sp(), store.node_slot(node, SLOT_48), node);
                    node = store.node_next(node);
                }
                host.advance_contact_bodies_remainder(stack.worker_sp(), this, left);
                left = window_left(step, left);
            }
            host.telemetry_90(stack.call_sp(), store.telemetry());
            let solver = store.solver(this);
            host.solver_post(stack.call_sp(), store.node_slot(solver, SOLVER_POST_SLOT), solver);
            sample += COLLIDE_SAMPLE_SIZE;
            let more = left > floor;
            if !more {
                break;
            }
        }
    }
    for slot in [SLOT_54, SLOT_48] {
        let mut node = store.bodies_head(store.physics_state(this));
        while node != 0 {
            let target = store.node_slot(node, slot);
            if slot == SLOT_54 {
                host.entity_54(stack.slot_sp(), target, node);
            } else {
                host.entity_48(stack.slot_sp(), target, node);
            }
            node = store.node_next(node);
        }
    }
    let passes = store.pass_count(this);
    if store.pass_maximum(this) < passes {
        store.set_pass_maximum(this, passes);
    }
    passes
}
