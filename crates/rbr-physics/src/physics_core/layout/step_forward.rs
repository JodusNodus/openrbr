//! `CPhysics::StepForward` (0x5273a0) and the stack words its call sites leave.

use crate::mem::Mem;
pub use crate::physics_core::step::step_forward;
use crate::physics_core::store::CoreStore;
use crate::store::Raw;

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

    pub(crate) fn base_local(self) -> u32 {
        self.below(0x24)
    }

    pub(crate) fn scaled_local(self) -> u32 {
        self.below(0x20)
    }

    pub(crate) fn step_local(self) -> u32 {
        self.below(0x28)
    }

    pub(crate) fn loop_saved_ebp(self) -> u32 {
        self.below(0x38)
    }

    pub(crate) fn exception_state(self) -> u32 {
        self.below(4)
    }

    pub(crate) fn outer_prof_return(self) -> u32 {
        self.below(0x2c)
    }

    fn penetration_prof_return(self) -> u32 {
        self.below(0x3c)
    }
}

pub(super) const STEP_STATE_AT: u32 = 0x20;
pub(super) const STEP_SCRATCH_AT: u32 = 0x28;
pub(super) const STEP_NODE_NEXT_AT: u32 = 0x10;

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

pub fn virtual_target(mem: &impl Raw, node: u32, offset: u32) -> u32 {
    mem.u32(mem.u32(node) + offset)
}

/// The current event-list head passed to `Telem70`, reloaded from the physics scratch after `PenCheck`.
pub fn step_scratch_events(mem: &Mem, physics: u32) -> u32 {
    CoreStore::scratch_events(mem, physics)
}
