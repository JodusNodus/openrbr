//! The contact loop: `CPhysics::Work5640`/`Work5760` and `CollideWork` (0x526d20).

use super::contact_walk::SolidHost;
use super::detector_calls::STATE_BODIES_AT;
use super::detector_hosts::CollideHost;
use super::globals::physics;
use super::passenger_step::PHYSICS_DEF_AT;
use super::step_forward::{step_scratch_events, virtual_target, STEP_NODE_NEXT_AT, STEP_SCRATCH_AT, STEP_STATE_AT};
use super::TELEMETRY_GLOBAL;
use crate::collision::layout::sort::event_get_94;
use crate::mem::Mem;
use crate::physics_core::contact::{advanced_by_contact_step, event_before_step_end};
use crate::physics_core::detectors::{fraction_overshoots, step_fraction, window_bounds, window_left};

/// A collision event's two solids (`+0xa0`, then `+0x98`), and its link to the next event (`+0xc`).
const EVENT_SOLIDS_AT: [u32; 2] = [0xa0, 0x98];
const EVENT_NEXT_AT: u32 = 0xc;
/// The solid's slot 0x80 (its vtable entry) and flag byte, which the damper workers test after the slot 0x80 answered.
const SLOT_80: u32 = 0x80;
const SOLID_FLAGS_AT: u32 = 0x60;

/// Whether the solid at `solid` carries the flag the damper workers test.
fn solid_flagged(mem: &Mem, solid: u32) -> bool {
    advanced_by_contact_step(mem.u8(solid + SOLID_FLAGS_AT))
}

/// `CPhysics::Work5640(f)` (0x525640) on the physics at `this`: over the scratch's event list, each solid of an event
/// that answers slot 0x80 and carries the flag gets `VfDx` with the event's penetration scalar (`Get94`). The float
/// argument is unused by the original.
pub fn advance_contact_bodies(mem: &mut Mem, host: &mut impl SolidHost, this: u32) {
    let mut event = step_scratch_events(mem, this);
    while event != 0 {
        for at in EVENT_SOLIDS_AT {
            let solid = mem.u32(event + at);
            if host.takes_part(virtual_target(mem, solid, SLOT_80), solid) != 0 && solid_flagged(mem, solid) {
                let sample = event_get_94(mem, event);
                host.back_step(solid, sample);
            }
        }
        event = mem.u32(event + EVENT_NEXT_AT);
    }
}

/// `CPhysics::Work5760(window)` (0x525760) on the physics at `this`: over the scratch's event list, an event whose
/// scalar is below 1.0 gives each solid that answers slot 0x80 and carries the flag `VfEx(window)`.
pub fn advance_contact_bodies_remainder(mem: &mut Mem, host: &mut impl SolidHost, this: u32, window: f32) {
    let mut event = step_scratch_events(mem, this);
    while event != 0 {
        if event_before_step_end(event_get_94(mem, event)) {
            for at in EVENT_SOLIDS_AT {
                let solid = mem.u32(event + at);
                if host.takes_part(virtual_target(mem, solid, SLOT_80), solid) != 0 && solid_flagged(mem, solid) {
                    host.advance_rest(solid, window);
                }
            }
        }
        event = mem.u32(event + EVENT_NEXT_AT);
    }
}

/// The profiler samples of the contact loop (0x38 bytes each from 0x165afc0) and its name.
const COLLIDE_SAMPLES: u32 = 0x0165_afc0;
const COLLIDE_SAMPLE_SIZE: u32 = 0x38;
const COLLIDE_NAME: u32 = 0x0073_f858;
/// The CDef flag that counts the solver's hits (+0x160e0).
const COLLIDE_FLAGS_AT: u32 = 0x160e0;
const COUNT_HITS_FLAG: u32 = 0x20000;

/// The solver of a `CPhysics` (its scratch's +0x10).
fn solver_of(mem: &Mem, physics: u32) -> u32 {
    mem.u32(physics + STEP_SCRATCH_AT) + 0x10
}

/// `CPhysics::CollideWork(window)` (0x526d20) on the physics at `this`; `entry` is its entry esp. The pass counter
/// (+0x18) and the hit counts (+4, +8, the maximum at +0xc) restart; while the window is above `0.05 window` (once at
/// most: the original's budget is 1) a profiled detect pass (`Work6a70`) and the solver's sweep give a fraction; at or
/// below 1.1 the contacts step by its clamped value (`Work5640`, the solver's pass, the bodies' slot 0x48, `Work5760`)
/// and the window shrinks, above it the window ends. Then the bodies' slots 0x54 and 0x48, and the pass maximum
/// (+0x14). Returns the pass count (eax).
pub fn collide_window(mem: &mut Mem, host: &mut impl CollideHost, this: u32, window: f32, entry: u32) -> u32 {
    let chan = entry - 0x14;
    let (offset, floor) = window_bounds(window);
    let mut left = window;
    mem.set_u32(this + 0x18, 0);
    mem.set_u32(this + 4, 0);
    mem.set_u32(this + 8, 0);
    if window > floor {
        let mut budget = 1;
        let mut sample = COLLIDE_SAMPLES;
        while budget != 0 {
            budget -= 1;
            mem.set_u32(this + 0x18, mem.u32(this + 0x18).wrapping_add(1));
            host.chan_enter(entry - 0x40, chan, sample, COLLIDE_NAME);
            host.run_collision_detectors(entry - 0x38, this, 0);
            host.prof_exit(entry - 0x34, chan);
            let events = mem.u32(mem.u32(this + STEP_SCRATCH_AT) + 0x14);
            host.telemetry_70(entry - 0x38, mem.u32(TELEMETRY_GLOBAL), events);
            host.solver_sweep_events(entry - 0x34, solver_of(mem, this));
            let fraction = host.solver_fraction(entry - 0x34, solver_of(mem, this));
            // The original's `fst` to its argument slot, which it reuses for the clamped fraction.
            mem.set_f32(entry + 4, fraction as f32);
            if fraction_overshoots(fraction) {
                left = 0.0;
            } else {
                if collide_flags(mem) & COUNT_HITS_FLAG != 0 {
                    let hits = host.solver_hit_count(entry - 0x34, solver_of(mem, this));
                    mem.set_u32(this + 4, hits);
                }
                if collide_flags(mem) & COUNT_HITS_FLAG != 0 {
                    let hits = host.solver_hit_count(entry - 0x34, solver_of(mem, this));
                    mem.set_u32(this + 8, hits);
                    if hits > mem.u32(this + 0xc) {
                        mem.set_u32(this + 0xc, hits);
                    }
                }
                let step = step_fraction(fraction as f32, offset, left);
                mem.set_f32(entry + 4, step);
                host.advance_contact_bodies(entry - 0x38, this, step);
                host.solver_pass(entry - 0x34, solver_of(mem, this));
                let mut node = mem.u32(mem.u32(this + STEP_STATE_AT) + STATE_BODIES_AT);
                while node != 0 {
                    host.entity_48(entry - 0x34, virtual_target(mem, node, 0x48), node);
                    node = mem.u32(node + STEP_NODE_NEXT_AT);
                }
                host.advance_contact_bodies_remainder(entry - 0x38, this, left);
                left = window_left(step, left);
            }
            host.telemetry_90(entry - 0x34, mem.u32(TELEMETRY_GLOBAL));
            let solver = solver_of(mem, this);
            host.solver_post(entry - 0x34, virtual_target(mem, solver, 8), solver);
            sample += COLLIDE_SAMPLE_SIZE;
            let more = left > floor;
            if !more {
                break;
            }
        }
    }
    for slot in [0x54, 0x48] {
        let mut node = mem.u32(mem.u32(this + STEP_STATE_AT) + STATE_BODIES_AT);
        while node != 0 {
            let target = virtual_target(mem, node, slot);
            if slot == 0x54 {
                host.entity_54(entry - 0x30, target, node);
            } else {
                host.entity_48(entry - 0x30, target, node);
            }
            node = mem.u32(node + STEP_NODE_NEXT_AT);
        }
    }
    let passes = mem.u32(this + 0x18);
    if mem.u32(this + 0x14) < passes {
        mem.set_u32(this + 0x14, passes);
    }
    passes
}

/// The CDef flags the contact loop reads (`[[0x1659b70] + 0x24] + 0x160e0`).
fn collide_flags(mem: &Mem) -> u32 {
    mem.u32(mem.u32(physics(mem) + PHYSICS_DEF_AT) + COLLIDE_FLAGS_AT)
}
