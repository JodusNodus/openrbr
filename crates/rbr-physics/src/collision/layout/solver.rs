//! `CCollisionSolver`: its constructor and the offsets of its rows. The passes over them are
//! `collision::solver_pass`, generic over `CollisionStore`; `mem_store.rs` reads and writes the rows.

use crate::mem::Mem;

use super::list;
pub(super) use crate::entities::layout::ROOT_IDENTITY as FRAME_GLOBAL;

// The E38 solver (`CCollisionSolver`, the object at the physics' +0x28 plus 0x10): the event list head at +4 (count at
// +0xc), the contact rows at +0x20 (0x310 bytes each, count at +0x18) and the offset rows at +0xc430 (0x90 bytes each,
// count at +0xc420). An event links through +0xc; its body is at +0xa0 and its partner at +0x98.
pub(super) const SOLVER_EVENTS: u32 = 4;
pub(super) const SOLVER_EVENT_COUNT: u32 = 0xc;
pub(super) const SOLVER_CONTACT_COUNT: u32 = 0x18;
pub(super) const SOLVER_CONTACT_ROWS: u32 = 0x20;
pub(super) const SOLVER_CONTACT_ROW: u32 = 0x310;
pub(super) const SOLVER_OFFSET_COUNT: u32 = 0xc420;
pub(super) const SOLVER_OFFSET_ROWS: u32 = 0xc430;
pub(super) const SOLVER_OFFSET_ROW: u32 = 0x90;
/// A body's contact row (+0x274) and offset row (+0x3b0).
pub(super) const BODY_CONTACT_ROW: u32 = 0x274;
pub(super) const BODY_OFFSET_ROW: u32 = 0x3b0;
/// The vector the offset rows move (+0x140; the solid's position, name ours).
pub(super) const BODY_POSITION: u32 = 0x140;
/// A body's contact rows hold 0x60 bytes each after a 16-byte header.
pub(super) const BODY_CONTACT_ROW_SIZE: u32 = 0x60;

/// `CCollisionSolver`'s vtable, at +0.
const SOLVER_VTABLE: u32 = 0x0073_9978;
/// The solver definition (+0x14).
const SOLVER_DEF: u32 = 0x14;

/// `CCollisionSolver::CtorSolver(def)` (0x4fc930, thiscall, `ret 4`) on the solver at `solver`: the vtable, the event
/// list (+4) and the word after it cleared, the definition, the list drained with the pool flag, then the contact and
/// offset counts cleared. Returns the solver.
pub fn ctor_solver(mem: &mut Mem, solver: u32, def: u32) -> u32 {
    mem.set_u32(solver, SOLVER_VTABLE);
    let head = solver + SOLVER_EVENTS;
    for word in 0..4 {
        mem.set_u32(head + 4 * word, 0);
    }
    mem.set_u32(solver + SOLVER_DEF, def);
    list::drain(mem, head, 1);
    mem.set_u32(solver + SOLVER_CONTACT_COUNT, 0);
    mem.set_u32(solver + SOLVER_OFFSET_COUNT, 0);
    solver
}
