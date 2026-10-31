//! The physics step, over the physics core's store. The stack image its call sites leave (`StepStack`) and the
//! calls into the host are in `layout`.

use crate::physics_core::layout::contact_walk::NodeHost;
use crate::physics_core::layout::physics_object::StepHost;
use crate::physics_core::layout::{StepStack, SLOT_20, SLOT_24, SLOT_48, SLOT_54, SLOT_98, SOLVER_POST_SLOT};
use crate::physics_core::pacing;
use crate::physics_core::store::CoreStore;

/// The collision window in steps: NGP's 20, where 1.02 reads 4.
const MAX_WINDOW: f32 = 20.0;

/// Calls `each` on every node of the list the owner (`this`'s state) holds at its head, linked through the nodes. The
/// owner and the head are read at the start of the walk, and each link after its node's call (the calls may change the
/// list).
fn for_each_node<S: CoreStore>(store: &mut S, this: u32, mut each: impl FnMut(&mut S, u32)) {
    let owner = store.physics_state(this);
    let mut node = store.bodies_head(owner);
    while node != 0 {
        each(store, node);
        node = store.node_next(node);
    }
}

/// The body pass: three walks over the owner's list, force input on each node, then force to acceleration, then the
/// integrate and child walk of each with `p`. The trailing walk reads the list and writes nothing, so it is not here.
pub fn step_bodies<S: CoreStore>(store: &mut S, host: &mut impl NodeHost<S>, this: u32, p: f32) {
    for_each_node(store, this, |store, node| host.force_input(store, node));
    for_each_node(store, this, |store, node| host.force_to_accel(store, node));
    for_each_node(store, this, |store, node| {
        host.integrate(store, node, p);
        host.child_walk(store, node, p);
    });
}

/// Calls `each` with the target of `slot` and the node, for every node of the list at `head`, linked through the nodes
/// (each link is read after its node's call, which may change the list).
fn walk_slot<S: CoreStore>(store: &S, head: u32, slot: u32, mut each: impl FnMut(u32, u32)) {
    let mut node = head;
    while node != 0 {
        each(store.node_slot(node, slot), node);
        node = store.node_next(node);
    }
}

/// The physics step: first-step setup, the physics workers and solver in step order, and the
/// per-substep clock. Ported callees are direct Rust calls; entity callbacks, listeners, collision work and the
/// solver chain use `StepHost`. `stack` is the caller's stack image.
pub fn step_forward<S: CoreStore>(
    store: &mut S,
    host: &mut impl StepHost<S>,
    this: u32,
    requested: u32,
    saved_ebp: u32,
    stack: StepStack,
) {
    stack.enter(store);
    let flags = store.step_flags();
    let (count, delay) = pacing::single_step_count(
        requested,
        flags,
        store.single_step_pending(this) != 0,
        store.single_step_delay(this),
    );
    stack.record_count(store, count);
    store.set_single_step_delay(this, delay);

    let state = store.physics_state(this);
    if store.step_counter(state) == 0 {
        walk_slot(store, store.bodies_head(state), SLOT_48, |target, node| {
            host.entity_48(stack.first_slot48(), target, node)
        });
    }

    let base = store.step_interval(this);
    let scaled = base * MAX_WINDOW;
    stack.record_interval(store, base, scaled);

    if count != 0 {
        stack.record_saved_ebp(store, saved_ebp);
    }
    let mut step = 0;
    stack.record_step(store, step);
    while step < count {
        if store.single_step_pending(this) != 0 && step != 0 {
            break;
        }

        let state = store.physics_state(this);
        stack.seed_substep(store);
        store.mark_state_dirty(state);
        let substeps = store.step_counter(state).wrapping_add(1);
        store.set_step_counter(state, substeps);
        let telemetry = store.telemetry();
        host.telemetry_60(stack.loop_arg(), telemetry, substeps);
        store.set_step_index(step);

        let state = store.physics_state(this);
        walk_slot(store, store.bodies_head(state), SLOT_20, |target, node| {
            host.entity_20(stack.loop_arg(), target, node, base)
        });

        stack.seed_work5500(store, base, this);
        step_bodies(store, host, this, base);

        let state = store.physics_state(this);
        walk_slot(store, store.bodies_head(state), SLOT_24, |target, node| {
            host.entity_24(stack.loop_arg(), target, node, base)
        });

        let flags = store.step_flags();
        let state = store.physics_state(this);
        if pacing::contact_due(flags, store.step_counter(state)) {
            if flags & 0x10 != 0 {
                host.collide_window(stack.loop_arg(), this, scaled);
            }

            if store.step_flags() & 0x20 != 0 {
                let telemetry = store.telemetry();
                host.penetration_scope(stack, this, telemetry);

                host.solver_clear_rows(stack.loop_void(), store.solver(this));
                let hit = host.solver_hit_count(stack.loop_void(), store.solver(this));
                store.set_contact_hit(this, hit);
                if hit != 0 {
                    host.solver_sweep_events(stack.loop_void(), store.solver(this));
                    host.solver_step(stack.loop_void(), store.solver(this));
                    host.solver_pass(stack.loop_void(), store.solver(this));
                }
                let telemetry = store.telemetry();
                host.telemetry_90(stack.loop_void(), telemetry);
                let solver = store.solver(this);
                host.solver_post(stack.loop_void(), store.node_slot(solver, SOLVER_POST_SLOT), solver);
            }

            let solver = store.solver(this);
            host.solver_post(stack.loop_void(), store.node_slot(solver, SOLVER_POST_SLOT), solver);

            let state = store.physics_state(this);
            walk_slot(store, store.bodies_head(state), SLOT_54, |target, node| {
                host.entity_54(stack.loop_void(), target, node)
            });

            let state = store.physics_state(this);
            walk_slot(store, store.bodies_head(state), SLOT_48, |target, node| {
                host.entity_48(stack.loop_void(), target, node)
            });
        }

        let state = store.physics_state(this);
        let mut node = store.second_head(state);
        while node != 0 {
            let next = store.node_next(node);
            let target = store.node_slot(node, SLOT_98);
            host.entity_98(stack.loop_void(), target, node);
            node = next;
        }

        let state = store.physics_state(this);
        let elapsed = store.elapsed(state);
        store.set_elapsed(state, pacing::advance_time(elapsed, base));
        step = step.wrapping_add(1);
        stack.record_step(store, step);
    }

    let player = store.player_car();
    if player != 0 {
        host.drive_set4510(stack.drive_set4510(), store.player_drive(player));
    }
    store.count_step_call();
    stack.seed_exit(store);
}
