//! E38's host calls and constant getters: the collision system, the mesh queries and the contact solver call back into
//! the host for the original's virtual calls.

use crate::mem::Mem;

/// CCollisionMeshOpen::IsActive (0x4fb0b0): every open mesh is active, so eax is 1.
pub fn mesh_is_active() -> u32 {
    1
}

/// CCollisionSolver::CDef::Unk20 (0x4fb330): the address of the shared solver table.
pub fn shared_solver_table() -> u32 {
    0x0165_7890
}

/// Calls outside E38RaySeeds (0x4f6550): `E09Vec4::Clear` (0x465d80) on the vector at `at`, which returns `at`.
pub trait RayHost {
    fn vec4_clear(&mut self, at: u32) -> u32;
}

/// Calls outside E38 M_D760X (0x4fd760): `FUN_00520240` (0x520240) on the sample table, which returns the surface byte
/// for `byte` at the sample point (`low` and `high` are the nibble sums; three stack words).
pub trait SampleHost {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32;
}

/// Calls outside E38SegmentSweep (0x4fad90): the cell query E45 (0x5240c0) over the box `lo`..`hi`, which writes the
/// four list heads to `cells`.
pub trait CellQueryHost {
    fn query_cells(&mut self, mem: &mut Mem, lo: u32, hi: u32, cells: u32);
}

/// The material lookup made by E38_004FB720 (0x4fb720): `Tag520940` (name ours) on the material manager.
pub trait MaterialTagHost {
    fn material_tag(&mut self, manager: u32, tag: u32) -> u32;
}

/// Calls out of the E38 solver passes (0x4fbc20, 0x4fc590, 0x4fca20). `target` is the body's virtual slot, which the
/// caller reads from its vtable.
pub trait SolverHost {
    /// Slot 0x13c (`E47Solid::Vf79x`), thiscall on the body: the inverse mass, as st0.
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64;
    /// Slots 0x80 and 0x8c, thiscall on the body: eax (name ours).
    fn body_test(&mut self, target: u32, body: u32) -> u32;
    /// Slot 0x90 (`E47Solid::Vf36x`), thiscall on the body, run when both tests are 0.
    fn body_wake(&mut self, target: u32, body: u32);
    /// `E38Ops::M_BD30` (0x4fbd30, `layout::contact_rows`), thiscall on the solver: the contact rows of `body` for
    /// `event`.
    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32);
    /// `E38_004FB890` (0x4fb890, `layout::apply_offsets`), thiscall on an offset row: the body's move from its
    /// collected offsets.
    fn apply_offsets(&mut self, row: u32);
    /// `E38Ops::M_C1A0` (0x4fc1a0, `layout::solve_event`), thiscall on the solver: the impulse of one event (its st0
    /// result is dropped).
    fn solve_event(&mut self, solver: u32, event: u32);
}

/// Calls out of E38Ops::M_C1A0 (0x4fc1a0) and M_BD30 (0x4fbd30), each entered at `sp` as the original's. The vectors
/// they take and the floats they write are in the caller's frame.
pub trait ImpulseHost {
    /// `E37EvOps::PlaneDiff` (0x4f40f0), thiscall on the event: the velocity of the contact relative to the partner
    /// (slot 0x140 of the body less the partner's); x, y and z to `out`.
    fn relative_velocity(&mut self, sp: u32, event: u32, out: u32);
    /// `E37EvOps::Query4390` (0x4f4390), thiscall on the event with `(direction, other, use_other, along, across)`: the
    /// bodies' response to an impulse along `direction` (names ours), two floats.
    #[allow(clippy::too_many_arguments)]
    fn response(&mut self, sp: u32, event: u32, direction: u32, other: u32, use_other: u32, along: u32, across: u32);
    /// `E37EvOps::Emit4DC0` (0x4f4dc0), thiscall on the event with `(impulse, slip_speed, vector, slip)`: applies the
    /// impulse to both bodies (their slot 0x128) unless the event is suppressed.
    #[allow(clippy::too_many_arguments)]
    fn apply_impulse(&mut self, sp: u32, event: u32, impulse: f32, slip_speed: f32, vector: u32, slip: u32);
    /// `ComposeA` (0x51b4c0), thiscall on `frame` with `(point, body, out)`.
    fn compose(&mut self, sp: u32, frame: u32, point: u32, body: u32, out: u32);
}
