//! The calls the detectors and the contact loop make into the original's code (`RBR_E45_PhysB.cpp`,
//! `RBR_E46_Kernels.cpp`, `RBR_E46_Detectors.cpp`, `RBR_E46_Small.cpp`): `CL2GiganticPenetrationDetector::Vf04`
//! (0x5242c0), `E46L1PenDetector::Vf04` (0x525a20), `E46_Kernel5bd0` (0x525bd0), `CL1CollisionDetector::Vf04`
//! (0x526ad0), `CPhysics::PenCheck` (0x526cc0) and `CPhysics::CollideWork` (0x526d20). The detectors are stack
//! objects handed to the entity walk, so their frames are the original's (`detector_calls`); the calls they make are
//! the host's, at the original's entry stack pointers.

use crate::mem::Mem;
/// The calls the detectors make, each with its callee's entry stack pointer `sp`. The entity slots are virtual (the
/// C++ names them by their vtable offset; their meanings are not all known).
pub trait DetectorHost {
    /// Slot 0x118: the entity's collision grid, 0 for none (name ours).
    fn collision_grid(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0x110 and slot 0x10c: the gates the detectors test (nonzero passes).
    fn entity_110(&mut self, sp: u32, entity: u32) -> u32;
    fn entity_10c(&mut self, sp: u32, entity: u32) -> u32;
    /// Slots 0x30 and 0x34: the entity's two frames.
    fn entity_30(&mut self, sp: u32, entity: u32) -> u32;
    fn entity_34(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0x120: the entity's box, its corners to `lo` and `hi` (name ours).
    fn entity_box(&mut self, sp: u32, entity: u32, lo: u32, hi: u32);
    /// Slot 0xcc: whether the entity overlaps the box of the two pair objects (name ours).
    fn overlaps_box(&mut self, sp: u32, entity: u32, lo: u32, hi: u32) -> u32;
    /// Slot 0x80: the entity's gate on its children and its parent's (nonzero passes) (name ours).
    fn child_gate(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0x30 called with one argument (`E34Call12_1::S30`).
    fn entity_30_with(&mut self, sp: u32, entity: u32, arg: u32) -> u32;
    /// Slot 0xd0: the penetration response with the pair object, the other entity and the detector's word (eax).
    fn entity_d0(&mut self, sp: u32, entity: u32, pair: u32, other: u32, extra: u32) -> u32;
    /// Slot 0xd4: the seven arguments of `CL2Pen`'s pair response (`E45SCC::SD4`).
    fn entity_d4(&mut self, sp: u32, entity: u32, args: [u32; 7]);
    /// `CEntity::E43_ExclF80` (0x519f80): 0 when the entity excludes `other`.
    fn exclusion_test(&mut self, sp: u32, entity: u32, other: u32) -> u32;
    /// `E37GridOps::M6610` (0x4f6610) on the grid ops at `at`.
    fn grid_ops(&mut self, sp: u32, at: u32, grid: u32, frame: u32, other_frame: u32);
    /// `E33Vec4::MinMaxA` (0x4e0ae0) on the grid ops at `at`, with the other grid ops and the two outputs.
    fn min_max(&mut self, sp: u32, at: u32, other: u32, lo: u32, hi: u32);
    /// `E38RayClear` (0x4f6740) on the grid ops at `at`.
    fn ray_clear(&mut self, sp: u32, at: u32);
    /// `E45_M40C0` (0x5240c0): the four cells of the box between `lo` and `hi`, to `out`.
    fn cells(&mut self, sp: u32, lo: u32, hi: u32, out: u32);
    /// `CL2PenetrationDetector::Ctor` (0x5235b0) at `at`.
    fn penetration_ctor(&mut self, sp: u32, at: u32, owner: u32, lo: u32, hi: u32, scale: u32, word: u32);
    /// `CBodyLink::NotifyFiltered` (0x4e0080): the cell's bodies to the detector.
    fn notify_filtered(&mut self, sp: u32, cell: u32, detector: u32);
    /// The detector's slot 1 (`Vf04`) with `node`.
    fn detect(&mut self, sp: u32, detector: u32, node: u32);
    /// `CEntity::E43_Walk1F0` (0x51a1f0): the detector on the entity's children.
    fn walk(&mut self, sp: u32, entity: u32, detector: u32);
    /// `CPhysics::Visit470` (0x526390): the detector on the physics' +0x470 list.
    fn visit_list470(&mut self, sp: u32, physics: u32, detector: u32);
    /// The probe kernel's tests of the entity at the cell node `node` (`kernel_collect`, whose frame is `frame`: the
    /// flags against `mask` and `excluded`, the grid and its solid byte, the overlap with the box of the frame's
    /// pairs), answered by a host that knows how they come out, with the words their calls write written. `None`
    /// runs them.
    fn kernel_tests(
        &mut self,
        _mem: &mut Mem,
        _frame: u32,
        _node: u32,
        _mask: u32,
        _excluded: u32,
    ) -> Option<KernelTest> {
        None
    }
}

/// How the probe kernel's tests of an entity came out ([`DetectorHost::kernel_tests`]): it passes, or it fails and the
/// walk goes on at `next`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelTest {
    Pass,
    Fail { next: u32 },
}

/// The calls `CPhysics::CollideWork` makes, at their entry stack pointers.
pub trait CollideHost {
    /// `E46ProfChan::Enter` (0x528880) and `ProfExit` (0x528690) on the channel at `chan`.
    fn chan_enter(&mut self, sp: u32, chan: u32, sample: u32, name: u32);
    fn prof_exit(&mut self, sp: u32, chan: u32);
    /// `CPhysics::Work6a70` (0x526a70): the collision detectors over the entities.
    fn run_collision_detectors(&mut self, sp: u32, physics: u32, word: u32);
    /// The telemetry hooks (0x499e70 with the event list, 0x499e90).
    fn telemetry_70(&mut self, sp: u32, receiver: u32, events: u32);
    fn telemetry_90(&mut self, sp: u32, receiver: u32);
    /// The solver's sweep (0x4fbc20), its fraction (0x4fc500, the x87 result), its hit count (0x4fbca0), its pass
    /// (0x4fca20) and its slot 2 (`target`).
    fn solver_sweep_events(&mut self, sp: u32, solver: u32);
    fn solver_fraction(&mut self, sp: u32, solver: u32) -> f64;
    fn solver_hit_count(&mut self, sp: u32, solver: u32) -> u32;
    fn solver_pass(&mut self, sp: u32, solver: u32);
    fn solver_post(&mut self, sp: u32, target: u32, solver: u32);
    /// `CPhysics::Work5640` (0x525640) with the fraction and `Work5760` (0x525760) with the window.
    fn advance_contact_bodies(&mut self, sp: u32, physics: u32, fraction: f32);
    fn advance_contact_bodies_remainder(&mut self, sp: u32, physics: u32, window: f32);
    /// The entity slots 0x48 and 0x54 (`target`) on `node`.
    fn entity_48(&mut self, sp: u32, target: u32, node: u32);
    fn entity_54(&mut self, sp: u32, target: u32, node: u32);
}
