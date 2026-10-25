//! The contact loop: `CPhysics::Work5640`/`Work5760` and `CollideWork` (0x526d20), over the physics core's store.

use super::contact::event_before_step_end;
use super::detectors::{fraction_overshoots, step_fraction, window_bounds, window_left};
use super::layout::contact_walk::SolidHost;
use super::layout::detector_hosts::CollideHost;
use super::store::CoreStore;

/// A collision event's two solids (`+0xa0`, then `+0x98`).
const EVENT_SOLIDS_AT: [u32; 2] = [0xa0, 0x98];
/// The solid's slot 0x80 (its vtable entry), which the damper workers ask first.
const SLOT_80: u32 = 0x80;
/// The solver's slot 2 (`solver_post`).
const SOLVER_POST_SLOT: u32 = 0x08;
/// The bodies' slots the contact loop ends with.
const SLOT_48: u32 = 0x48;
const SLOT_54: u32 = 0x54;

/// `CPhysics::Work5640(f)` (0x525640) on the physics at `this`: over the scratch's event list, each solid of an event
/// that answers slot 0x80 and carries the flag gets `VfDx` with the event's penetration scalar (`Get94`). The float
/// argument is unused by the original.
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

/// `CPhysics::Work5760(window)` (0x525760) on the physics at `this`: over the scratch's event list, an event whose
/// scalar is below 1.0 gives each solid that answers slot 0x80 and carries the flag `VfEx(window)`.
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

/// The profiler samples of the contact loop (0x38 bytes each from 0x165afc0) and its name.
const COLLIDE_SAMPLES: u32 = 0x0165_afc0;
const COLLIDE_SAMPLE_SIZE: u32 = 0x38;
const COLLIDE_NAME: u32 = 0x0073_f858;
/// The CDef flag that counts the solver's hits (+0x160e0).
const COUNT_HITS_FLAG: u32 = 0x20000;

/// `CPhysics::CollideWork(window)` (0x526d20) on the physics at `this`; `entry` is its entry esp. The pass counter
/// (+0x18) and the hit counts (+4, +8, the maximum at +0xc) restart; while the window is above `0.05 window` (once at
/// most: the original's budget is 1) a profiled detect pass (`Work6a70`) and the solver's sweep give a fraction; at or
/// below 1.1 the contacts step by its clamped value (`Work5640`, the solver's pass, the bodies' slot 0x48, `Work5760`)
/// and the window shrinks, above it the window ends. Then the bodies' slots 0x54 and 0x48, and the pass maximum
/// (+0x14). Returns the pass count (eax).
pub fn collide_window<S: CoreStore>(
    store: &mut S,
    host: &mut impl CollideHost,
    this: u32,
    window: f32,
    entry: u32,
) -> u32 {
    let chan = entry - 0x14;
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
            host.chan_enter(entry - 0x40, chan, sample, COLLIDE_NAME);
            host.run_collision_detectors(entry - 0x38, this, 0);
            host.prof_exit(entry - 0x34, chan);
            let events = store.scratch_events(this);
            host.telemetry_70(entry - 0x38, store.telemetry(), events);
            host.solver_sweep_events(entry - 0x34, store.solver(this));
            let fraction = host.solver_fraction(entry - 0x34, store.solver(this));
            // The original's `fst` to its argument slot, which it reuses for the clamped fraction.
            store.set_f32(entry + 4, fraction as f32);
            if fraction_overshoots(fraction) {
                left = 0.0;
            } else {
                if store.collide_flags() & COUNT_HITS_FLAG != 0 {
                    let hits = host.solver_hit_count(entry - 0x34, store.solver(this));
                    store.set_hits(this, hits);
                }
                if store.collide_flags() & COUNT_HITS_FLAG != 0 {
                    let hits = host.solver_hit_count(entry - 0x34, store.solver(this));
                    store.set_hits_last(this, hits);
                    if hits > store.hits_maximum(this) {
                        store.set_hits_maximum(this, hits);
                    }
                }
                let step = step_fraction(fraction as f32, offset, left);
                store.set_f32(entry + 4, step);
                host.advance_contact_bodies(entry - 0x38, this, step);
                host.solver_pass(entry - 0x34, store.solver(this));
                let mut node = store.bodies_head(store.physics_state(this));
                while node != 0 {
                    host.entity_48(entry - 0x34, store.node_slot(node, SLOT_48), node);
                    node = store.node_next(node);
                }
                host.advance_contact_bodies_remainder(entry - 0x38, this, left);
                left = window_left(step, left);
            }
            host.telemetry_90(entry - 0x34, store.telemetry());
            let solver = store.solver(this);
            host.solver_post(entry - 0x34, store.node_slot(solver, SOLVER_POST_SLOT), solver);
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
                host.entity_54(entry - 0x30, target, node);
            } else {
                host.entity_48(entry - 0x30, target, node);
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
