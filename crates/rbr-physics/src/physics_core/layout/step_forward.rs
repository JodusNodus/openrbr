//! `CPhysics::StepForward` (0x5273a0) and the stack words its call sites leave.

use super::contact_walk;
use super::globals::physics_global;
use super::lists::{f32, word};
use super::physics_object::StepHost;
use super::prof_calls::{set_word64, word64};
use crate::car_body::layout::CURRENT_CAR;
use crate::mem::Mem;
use crate::physics_core::layout::TELEMETRY_GLOBAL;
use crate::physics_core::pacing;

/// Stack slots at the call sites in `CPhysics::StepForward` (0x5273a0). These depths are from the original's
/// `sub esp, 0x1c`, saved registers, and the single `push ebp` around its substep loop.
#[derive(Clone, Copy, Debug)]
pub struct StepStack {
    entry: u32,
}

impl StepStack {
    pub fn new(entry: u32) -> Self {
        Self { entry }
    }

    fn below(self, bytes: u32) -> u32 {
        self.entry.wrapping_sub(bytes)
    }

    pub fn argument(self) -> u32 {
        self.entry.wrapping_add(4)
    }

    pub fn outer_channel(self) -> u32 {
        self.below(0x14)
    }

    pub fn outer_prof_local(self) -> u32 {
        self.below(0x34)
    }

    pub fn first_slot48(self) -> u32 {
        self.below(0x38)
    }

    pub fn loop_void(self) -> u32 {
        self.below(0x3c)
    }

    pub fn loop_arg(self) -> u32 {
        self.below(0x40)
    }

    pub fn work_entry(self) -> u32 {
        self.below(0x40)
    }

    pub fn work_slot_void(self) -> u32 {
        self.below(0x4c)
    }

    pub fn work_slot_arg(self) -> u32 {
        self.below(0x54)
    }

    pub fn work_saved_ebx(self) -> u32 {
        self.below(0x44)
    }

    pub fn work_saved_esi(self) -> u32 {
        self.below(0x48)
    }

    pub fn work_saved_edi(self) -> u32 {
        self.below(0x4c)
    }

    pub fn penetration_channel(self) -> u32 {
        self.below(0x1c)
    }

    pub fn penetration_prof_local(self) -> u32 {
        self.below(0x44)
    }

    pub fn drive_set4510(self) -> u32 {
        self.below(0x2c)
    }

    fn base_local(self) -> u32 {
        self.below(0x24)
    }

    fn scaled_local(self) -> u32 {
        self.below(0x20)
    }

    fn step_local(self) -> u32 {
        self.below(0x28)
    }

    fn loop_saved_ebp(self) -> u32 {
        self.below(0x38)
    }

    fn exception_state(self) -> u32 {
        self.below(4)
    }

    fn outer_prof_return(self) -> u32 {
        self.below(0x2c)
    }

    fn penetration_prof_return(self) -> u32 {
        self.below(0x3c)
    }
}

const STEP_FORWARD_FLAGS: u32 = 0x160cc;
const STEP_FORWARD_DT: u32 = 0x1608c;
pub(super) const STEP_STATE_AT: u32 = 0x20;
const STEP_DEF_AT: u32 = 0x24;
pub(super) const STEP_SCRATCH_AT: u32 = 0x28;
const STEP_SINGLE_DELAY_AT: u32 = 0x5c;
const STEP_SINGLE_PENDING_AT: u32 = 0x60;
const STEP_CONTACT_HIT_AT: u32 = 0x1c;
const STEP_COUNTER_AT: u32 = 0x18;
const STEP_TIME_AT: u32 = 0x20;
const STEP_LIST450_AT: u32 = 0x450;
const STEP_LIST480_AT: u32 = 0x480;
pub(super) const STEP_NODE_NEXT_AT: u32 = 0x10;
const STEP_INDEX_GLOBAL: u32 = 0x0165_9b78;
const STEP_CALL_COUNT_GLOBAL: u32 = 0x0165_9b74;
/// The collision window in steps: NGP's 20 (`1014fbdc`, the operand at `0x527447`), where 1.02 reads 4 (`0x72d150`).
const STEP_MAX_WINDOW: f32 = 20.0;

/// Mirrors the StepForward prologue and its direct profiler `Enter` call. The exception link at `entry-0xc` is left as
/// captured by the emulator; every other saved/local word below is set or initialized by the original instructions.
pub fn seed_step_forward_entry(mem: &mut Mem, stack: StepStack, ebx: u32, esi: u32, edi: u32, sample: u32, name: u32) {
    mem.set_u32(stack.exception_state(), u32::MAX);
    mem.set_u32(stack.below(8), 0x0071_7080);
    mem.set_u32(stack.below(0x2c), ebx);
    mem.set_u32(stack.below(0x30), esi);
    mem.set_u32(stack.below(0x34), edi);
    mem.set_u32(stack.below(0x38), 0);
    mem.set_u32(stack.below(0x3c), name);
    mem.set_u32(stack.below(0x40), sample);
    mem.set_u32(stack.below(0x44), 0x0052_73d3);
}

/// Mirrors the `Enter` call made for the nested penetration profiler sample.
pub fn seed_penetration_entry(mem: &mut Mem, stack: StepStack, sample: u32, name: u32) {
    mem.set_u32(stack.below(0x3c), 0);
    mem.set_u32(stack.below(0x40), name);
    mem.set_u32(stack.below(0x44), sample);
    mem.set_u32(stack.below(0x48), 0x0052_754e);
}

/// The return addresses and SEH state left by the original's two profiler exits.
pub fn seed_penetration_exit(mem: &mut Mem, stack: StepStack) {
    mem.set_u32(stack.penetration_prof_return(), 0x0052_7579);
}

/// The saved `EDI` word written by Work5500 when its S6c/S58 pass has at least one node. StepForward has `EDI == 0`
/// when it enters Work5500 because its preceding list walk ended at the null link.
pub fn seed_work5500_edi(mem: &mut Mem, stack: StepStack) {
    mem.set_u32(stack.work_saved_edi(), 0);
}

/// The return address and SEH state left by StepForward's final profiler exit.
fn seed_step_forward_exit(mem: &mut Mem, stack: StepStack) {
    mem.set_u32(stack.exception_state(), u32::MAX);
    mem.set_u32(stack.outer_prof_return(), 0x0052_7687);
}

fn step_flags(mem: &Mem) -> u32 {
    let def = word(mem, physics_global(mem) + STEP_DEF_AT);
    word(mem, def + STEP_FORWARD_FLAGS)
}

pub fn virtual_target(mem: &Mem, node: u32, offset: u32) -> u32 {
    word(mem, word(mem, node) + offset)
}

/// The current event-list head passed to `Telem70`, reloaded from the physics scratch after `PenCheck`.
pub fn step_scratch_events(mem: &Mem, physics: u32) -> u32 {
    let scratch = word(mem, physics + STEP_SCRATCH_AT);
    word(mem, scratch + 0x14)
}

/// `CPhysics::StepForward(p1)` (0x5273a0): first-step setup, the physics workers and solver in original order, and the
/// per-substep clock. Ported callees are direct Rust calls; unresolved entity slots, listeners, collision work and the
/// solver chain use `StepHost` at the retail call depths. `stack` is the original function's entry ESP.
pub fn step_forward(
    mem: &mut Mem,
    host: &mut impl StepHost,
    this: u32,
    requested: u32,
    saved_ebp: u32,
    stack: StepStack,
) {
    mem.set_u32(stack.exception_state(), 0);
    let flags = step_flags(mem);
    let (count, delay) = pacing::single_step_count(
        requested,
        flags,
        word(mem, this + STEP_SINGLE_PENDING_AT) != 0,
        word(mem, this + STEP_SINGLE_DELAY_AT),
    );
    mem.set_u32(stack.argument(), count);
    mem.set_u32(this + STEP_SINGLE_DELAY_AT, delay);

    let state = word(mem, this + STEP_STATE_AT);
    if word(mem, state + STEP_COUNTER_AT) == 0 {
        let mut node = word(mem, state + STEP_LIST450_AT);
        while node != 0 {
            let target = virtual_target(mem, node, 0x48);
            host.entity_48(stack.first_slot48(), target, node);
            node = word(mem, node + STEP_NODE_NEXT_AT);
        }
    }

    let def = word(mem, this + STEP_DEF_AT);
    let base = f32(mem, def + STEP_FORWARD_DT);
    let scaled = base * STEP_MAX_WINDOW;
    mem.set_f32(stack.base_local(), base);
    mem.set_f32(stack.scaled_local(), scaled);

    if count != 0 {
        mem.set_u32(stack.loop_saved_ebp(), saved_ebp);
    }
    let mut step = 0;
    mem.set_u32(stack.step_local(), step);
    while step < count {
        if word(mem, this + STEP_SINGLE_PENDING_AT) != 0 && step != 0 {
            break;
        }

        let state = word(mem, this + STEP_STATE_AT);
        let dirty = state + 0x0c;
        mem.set_u32(stack.loop_void(), 0x0052_7482);
        crate::ntweak::layout::set_dirty(mem, dirty);
        let substeps = word(mem, state + STEP_COUNTER_AT).wrapping_add(1);
        mem.set_u32(state + STEP_COUNTER_AT, substeps);
        let telemetry = word(mem, TELEMETRY_GLOBAL);
        host.telemetry_60(stack.loop_arg(), telemetry, substeps);
        mem.set_u32(STEP_INDEX_GLOBAL, step);

        let state = word(mem, this + STEP_STATE_AT);
        let mut node = word(mem, state + STEP_LIST450_AT);
        while node != 0 {
            let target = virtual_target(mem, node, 0x20);
            host.entity_20(stack.loop_arg(), target, node, base);
            node = word(mem, node + STEP_NODE_NEXT_AT);
        }

        let work = stack.work_entry();
        mem.set_u32(work, 0x0052_74cf);
        mem.set_u32(work + 4, base.to_bits());
        mem.set_u32(stack.work_saved_ebx(), 0);
        mem.set_u32(stack.work_saved_esi(), this);
        contact_walk::step_bodies(mem, host, this, base);

        let state = word(mem, this + STEP_STATE_AT);
        let mut node = word(mem, state + STEP_LIST450_AT);
        while node != 0 {
            let target = virtual_target(mem, node, 0x24);
            host.entity_24(stack.loop_arg(), target, node, base);
            node = word(mem, node + STEP_NODE_NEXT_AT);
        }

        let flags = step_flags(mem);
        let state = word(mem, this + STEP_STATE_AT);
        if pacing::contact_due(flags, word(mem, state + STEP_COUNTER_AT)) {
            if flags & 0x10 != 0 {
                host.collide_window(stack.loop_arg(), this, scaled);
            }

            if step_flags(mem) & 0x20 != 0 {
                let telemetry = word(mem, TELEMETRY_GLOBAL);
                host.penetration_scope(stack, this, telemetry);

                let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
                host.solver_clear_rows(stack.loop_void(), solver);
                let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
                let hit = host.solver_hit_count(stack.loop_void(), solver);
                mem.set_u32(this + STEP_CONTACT_HIT_AT, hit);
                if hit != 0 {
                    let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
                    host.solver_sweep_events(stack.loop_void(), solver);
                    let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
                    host.solver_step(stack.loop_void(), solver);
                    let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
                    host.solver_pass(stack.loop_void(), solver);
                }
                let telemetry = word(mem, TELEMETRY_GLOBAL);
                host.telemetry_90(stack.loop_void(), telemetry);
                let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
                let target = virtual_target(mem, solver, 0x08);
                host.solver_post(stack.loop_void(), target, solver);
            }

            let solver = word(mem, this + STEP_SCRATCH_AT) + 0x10;
            let target = virtual_target(mem, solver, 0x08);
            host.solver_post(stack.loop_void(), target, solver);

            let state = word(mem, this + STEP_STATE_AT);
            let mut node = word(mem, state + STEP_LIST450_AT);
            while node != 0 {
                let target = virtual_target(mem, node, 0x54);
                host.entity_54(stack.loop_void(), target, node);
                node = word(mem, node + STEP_NODE_NEXT_AT);
            }

            let state = word(mem, this + STEP_STATE_AT);
            let mut node = word(mem, state + STEP_LIST450_AT);
            while node != 0 {
                let target = virtual_target(mem, node, 0x48);
                host.entity_48(stack.loop_void(), target, node);
                node = word(mem, node + STEP_NODE_NEXT_AT);
            }
        }

        let state = word(mem, this + STEP_STATE_AT);
        let mut node = word(mem, state + STEP_LIST480_AT);
        while node != 0 {
            let next = word(mem, node + STEP_NODE_NEXT_AT);
            let target = virtual_target(mem, node, 0x98);
            host.entity_98(stack.loop_void(), target, node);
            node = next;
        }

        let state = word(mem, this + STEP_STATE_AT);
        let elapsed = f64::from_bits(word64(mem, state + STEP_TIME_AT));
        set_word64(mem, state + STEP_TIME_AT, pacing::advance_time(elapsed, base).to_bits());
        step = step.wrapping_add(1);
        mem.set_u32(stack.step_local(), step);
    }

    let player = word(mem, CURRENT_CAR);
    if player != 0 {
        host.drive_set4510(stack.drive_set4510(), player + 0x4b0);
    }
    let calls = word(mem, STEP_CALL_COUNT_GLOBAL).wrapping_add(1);
    mem.set_u32(STEP_CALL_COUNT_GLOBAL, calls);
    seed_step_forward_exit(mem, stack);
}
