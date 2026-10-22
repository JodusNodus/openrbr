//! `CCollisionSolver`: its contact and offset rows, the event drain, the penetration correction and the impulse
//! solve.

use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use crate::physics_core;

use super::event_pool::{EVENT_BODY, EVENT_FLAGS, EVENT_NEXT, EVENT_PARTNER};
use super::mesh_query::{load_material_pair, material_manager};
use super::solver_hosts::{ImpulseHost, SolverHost};
use super::{list, sort};
use crate::collision::contact_solver::{self, ContactMaterial, ContactRow, Correction, RowEvent, SolverContact};
pub(super) use crate::entities::layout::ROOT_IDENTITY as FRAME_GLOBAL;
use crate::physics_core::layout::vtable_target;

// The E38 solver (`CCollisionSolver`, the object at the physics' +0x28 plus 0x10): the event list head at +4 (count at
// +0xc), the contact rows at +0x20 (0x310 bytes each, count at +0x18) and the offset rows at +0xc430 (0x90 bytes each,
// count at +0xc420). An event links through +0xc; its body is at +0xa0 and its partner at +0x98.
const SOLVER_EVENTS: u32 = 4;
const SOLVER_EVENT_COUNT: u32 = 0xc;
const SOLVER_CONTACT_COUNT: u32 = 0x18;
const SOLVER_CONTACT_ROWS: u32 = 0x20;
const SOLVER_CONTACT_ROW: u32 = 0x310;
const SOLVER_OFFSET_COUNT: u32 = 0xc420;
const SOLVER_OFFSET_ROWS: u32 = 0xc430;
const SOLVER_OFFSET_ROW: u32 = 0x90;
/// The offsets an offset row holds after its 16-byte header (body, count).
const OFFSETS_PER_ROW: u32 = 8;
const OFFSET_ROW_LIMIT: u32 = 0x40;
/// A body's contact row (+0x274) and offset row (+0x3b0).
const BODY_CONTACT_ROW: u32 = 0x274;
const BODY_OFFSET_ROW: u32 = 0x3b0;
/// The virtual slots the passes call on a body.
const SLOT_INVERSE_MASS: u32 = 0x13c;
const SLOT_TEST_A: u32 = 0x80;
const SLOT_TEST_B: u32 = 0x8c;
const SLOT_WAKE: u32 = 0x90;
/// `CCollisionEvent::Vf1Cx` (0x4f52f0), the release `list::drain` runs on each event.
const EVENT_RELEASE: u32 = 0x4f52f0;

/// Whether the body or partner of an event takes part in the solve: byte +0x60 has bit 2.
fn solver_active(mem: &Mem, body: u32) -> bool {
    mem.u8(body + 0x60) & 4 != 0
}

/// E38_004FB7F0 (0x4fb7f0): unhooks the body of every contact row (+0x274) and empties the rows. eax is the count as
/// the asm last read it.
pub fn clear_contact_rows(mem: &mut Mem, solver: u32) -> u32 {
    let mut count = mem.u32(solver + SOLVER_CONTACT_COUNT);
    let mut index = 0;
    while index < count {
        let body = mem.u32(solver + SOLVER_CONTACT_ROWS + index * SOLVER_CONTACT_ROW);
        if body != 0 {
            mem.set_u32(body + BODY_CONTACT_ROW, 0);
        }
        count = mem.u32(solver + SOLVER_CONTACT_COUNT);
        index += 1;
    }
    mem.set_u32(solver + SOLVER_CONTACT_COUNT, 0);
    count
}

/// E38_004FBCA0 (0x4fbca0): the solver's event count.
pub fn event_count(mem: &Mem, solver: u32) -> u32 {
    mem.u32(solver + SOLVER_EVENT_COUNT)
}

/// CCollisionSolver::Vf08 (0x4fc0b0): drains the event list, releasing each event (`list::drain` with the pool flag).
/// `None` (nothing changed) when an event's slot 0x1c is not `CCollisionEvent::Vf1Cx`, which the drain port assumes.
pub fn drain_events(mem: &mut Mem, solver: u32) -> Option<()> {
    let head = solver + SOLVER_EVENTS;
    let mut event = mem.u32(head);
    for _ in 0..mem.u32(head + 8) {
        if event == 0 {
            break;
        }
        if vtable_target(mem, event, 0x1c) != EVENT_RELEASE {
            return None;
        }
        event = mem.u32(event + EVENT_NEXT);
    }
    list::drain(mem, head, 1);
    Some(())
}

/// E38_004FC500 (0x4fc500): the solver's fraction, the earliest of its events' (`CCollisionEvent::Get94`, +0x94) from
/// `FRACTION_START`. An event with a material flag (`E37Test4B20`) is released instead: unlinked from the list when it
/// is on it (`E37Unlink`, the count first) and returned to the pool (slot 0x1c with 1). The next event is read before.
/// `None` (nothing changed) when a released event's slot 0x1c is not `CCollisionEvent::Vf1Cx`, which the port assumes.
pub fn min_contact_time(mem: &mut Mem, solver: u32) -> Option<f32> {
    let head = solver + SOLVER_EVENTS;
    let mut event = mem.u32(head);
    while event != 0 {
        if super::event_pool::is_suppressed(mem, event) && vtable_target(mem, event, 0x1c) != EVENT_RELEASE {
            return None;
        }
        event = mem.u32(event + EVENT_NEXT);
    }
    let mut best = contact_solver::FRACTION_START;
    let mut event = mem.u32(head);
    while event != 0 {
        let link = event + EVENT_NEXT;
        let next = mem.u32(link);
        if super::event_pool::is_suppressed(mem, event) {
            if mem.u32(link + 8) == head {
                mem.set_u32(head + 8, mem.u32(head + 8).wrapping_sub(1));
                list::unlink(mem, link);
            }
            super::event_pool::vf1cx(mem, event, 1);
        } else {
            best = contact_solver::earlier_fraction(best, sort::event_get_94(mem, event));
        }
        event = next;
    }
    Some(best)
}

/// E37Flag4AC0 (0x4f4ac0): sets the event's flag 0x2000 when the material of its id at +0xac has flag bit 3 (+0x158),
/// and 0x4000 for the id at +0x80 (`CMaterialManager::Tag520940` on the manager).
pub fn mark_suppressing_materials(mem: &mut Mem, event: u32) {
    let manager = material_manager(mem);
    for (id, flag) in [(0xac, 0x2000), (0x80, 0x4000)] {
        let material = physics_core::layout::material::tag_entry(mem, manager, mem.u32(event + id));
        if mem.u8(material + 0x158) & 8 != 0 {
            mem.set_u32(event + EVENT_FLAGS, mem.u32(event + EVENT_FLAGS) | flag);
        }
    }
}

/// E38_004FBC20 (0x4fbc20): per event, the material refresh (`event::refresh_materials`) and suppress flags, then flag
/// 0x800 when the body is movable and 0x1000 when the active partner is.
pub fn refresh_events(host: &mut impl SolverHost, mem: &mut Mem, solver: u32) {
    let mut event = mem.u32(solver + SOLVER_EVENTS);
    while event != 0 {
        super::event_pool::refresh_materials(mem, event);
        mark_suppressing_materials(mem, event);
        let body = mem.u32(event + EVENT_BODY);
        if contact_solver::movable(host.inverse_mass(vtable_target(mem, body, SLOT_INVERSE_MASS), body)) {
            mem.set_u32(event + EVENT_FLAGS, mem.u32(event + EVENT_FLAGS) | 0x800);
        }
        let partner = mem.u32(event + EVENT_PARTNER);
        if solver_active(mem, partner)
            && contact_solver::movable(host.inverse_mass(vtable_target(mem, partner, SLOT_INVERSE_MASS), partner))
        {
            mem.set_u32(event + EVENT_FLAGS, mem.u32(event + EVENT_FLAGS) | 0x1000);
        }
        event = mem.u32(event + EVENT_NEXT);
    }
}

/// E38Ops::M_BFC0X (0x4fbfc0): appends `offset` to the body's offset row (+0x3b0, up to 8), or starts a row for it
/// (up to 64). False when full. The count is written before the copy, as the asm does.
pub fn add_body_offset(mem: &mut Mem, solver: u32, body: u32, offset: Vec4) -> bool {
    let row = mem.u32(body + BODY_OFFSET_ROW);
    if row != 0 {
        let count = mem.u32(row + 4);
        if count >= OFFSETS_PER_ROW {
            return false;
        }
        mem.set_u32(row + 4, count + 1);
        offset.store(mem, row + 16 * (count + 1));
        return true;
    }
    let rows = mem.u32(solver + SOLVER_OFFSET_COUNT);
    if rows >= OFFSET_ROW_LIMIT {
        return false;
    }
    mem.set_u32(solver + SOLVER_OFFSET_COUNT, rows + 1);
    let row = solver + SOLVER_OFFSET_ROWS + SOLVER_OFFSET_ROW * rows;
    offset.store(mem, row + 0x10);
    mem.set_u32(row, body);
    mem.set_u32(body + BODY_OFFSET_ROW, row);
    mem.set_u32(row + 4, 1);
    true
}

/// E38_004FC590 (0x4fc590): moves the bodies of the events out of their contact planes (`contact_solver::plan_correction`),
/// collecting the moves in offset rows, then applies every row and empties them. eax is the row count as the asm
/// last read it.
pub fn correct_penetrations(host: &mut impl SolverHost, mem: &mut Mem, solver: u32) -> u32 {
    let mut margin = contact_solver::PENETRATION_MARGIN;
    let mut event = mem.u32(solver + SOLVER_EVENTS);
    while event != 0 {
        let body = mem.u32(event + EVENT_BODY);
        if solver_active(mem, body) {
            let piece = mem.u32(event + EVENT_PARTNER);
            let partner_active = solver_active(mem, piece);
            let partner = if partner_active { piece } else { 0 };
            let contact = SolverContact {
                flags: mem.u32(event + EVENT_FLAGS),
                partner_active,
                plane: Vec4::load(mem, event + 0x70),
                point: Vec3::load(mem, event + 0xc0),
            };
            let (correction, next_margin) = contact_solver::plan_correction(contact, margin);
            margin = next_margin;
            match correction {
                Correction::None | Correction::Split(None) => {}
                Correction::Split(Some(offset)) => {
                    let inv_a = host.inverse_mass(vtable_target(mem, body, SLOT_INVERSE_MASS), body);
                    let inv_b = host.inverse_mass(vtable_target(mem, partner, SLOT_INVERSE_MASS), partner);
                    let (body_offset, partner_offset) = contact_solver::split_offsets(offset, inv_a, inv_b);
                    add_body_offset(mem, solver, body, body_offset);
                    add_body_offset(mem, solver, partner, partner_offset);
                }
                Correction::Body(offset) => {
                    if let Some(offset) = offset {
                        add_body_offset(mem, solver, body, offset.scale(-1.0));
                    }
                    host.contact_rows(solver, body, event, 1);
                }
                Correction::Partner(offset) => {
                    if let Some(offset) = offset {
                        add_body_offset(mem, solver, partner, offset.scale(1.0));
                    }
                    host.contact_rows(solver, partner, event, 0);
                }
            }
        }
        event = mem.u32(event + EVENT_NEXT);
    }
    let mut count = mem.u32(solver + SOLVER_OFFSET_COUNT);
    let mut index = 0;
    while index < count {
        host.apply_offsets(solver + SOLVER_OFFSET_ROWS + index * SOLVER_OFFSET_ROW);
        count = mem.u32(solver + SOLVER_OFFSET_COUNT);
        index += 1;
    }
    mem.set_u32(solver + SOLVER_OFFSET_COUNT, 0);
    count
}

/// Slot 0x13c gates slots 0x80, 0x8c and 0x90 on a body in E38_004FCA20.
fn wake_if_movable(host: &mut impl SolverHost, mem: &Mem, body: u32) {
    if contact_solver::movable(host.inverse_mass(vtable_target(mem, body, SLOT_INVERSE_MASS), body))
        && host.body_test(vtable_target(mem, body, SLOT_TEST_A), body) == 0
        && host.body_test(vtable_target(mem, body, SLOT_TEST_B), body) == 0
    {
        host.body_wake(vtable_target(mem, body, SLOT_WAKE), body);
    }
}

/// E38_004FCA20 (0x4fca20): solves each due event (`contact_solver::event_due`, time `CCollisionEvent::Get94`), first waking its
/// movable body and active partner.
pub fn solve_due_events(host: &mut impl SolverHost, mem: &mut Mem, solver: u32) {
    let mut event = mem.u32(solver + SOLVER_EVENTS);
    while event != 0 {
        let piece = mem.u32(event + EVENT_PARTNER);
        let partner = solver_active(mem, piece).then_some(piece);
        let body = mem.u32(event + EVENT_BODY);
        let time = sort::event_get_94(mem, event);
        let progress = mem.f32(body + 0x290);
        if contact_solver::event_due(time, progress, partner.map(|p| mem.f32(p + 0x290))) {
            wake_if_movable(host, mem, body);
            if let Some(partner) = partner {
                wake_if_movable(host, mem, partner);
            }
            host.solve_event(solver, event);
        }
        event = mem.u32(event + EVENT_NEXT);
    }
}

/// A body's contact-row limit (8 rows of 0x60 bytes after the 16-byte header) and the solver's (64).
const BODY_CONTACT_ROWS: u32 = 8;
const BODY_CONTACT_ROW_SIZE: u32 = 0x60;
const SOLVER_CONTACT_LIMIT: u32 = 0x40;
/// The vector E38_004FB890 moves (+0x140; the solid's position, name ours).
const BODY_POSITION: u32 = 0x140;

/// `E38_004FB720`'s tag for the event and the reals of its material entry (`CMaterialManager::Tag520940`).
fn event_material(mem: &Mem, event: u32) -> (u32, ContactMaterial) {
    let tag = contact_solver::pick_material_tag(load_material_pair(mem, event));
    let entry = physics_core::layout::material::tag_entry(mem, material_manager(mem), tag);
    let material = ContactMaterial {
        friction_fast: mem.f32(entry + 0x108),
        friction_slow: mem.f32(entry + 0x118),
        restitution_fast: mem.f32(entry + 0x128),
        restitution_slow: mem.f32(entry + 0x138),
    };
    (tag, material)
}

fn store_contact_row(mem: &mut Mem, at: u32, row: &ContactRow) {
    let words = [
        row.suppressed,
        row.tag,
        row.ids[0],
        row.ids[1],
        row.restitution[0].to_bits(),
        row.restitution[1].to_bits(),
        row.friction[0].to_bits(),
        row.friction[1].to_bits(),
    ];
    for (i, word) in words.into_iter().enumerate() {
        mem.set_u32(at + 4 * i as u32, word);
    }
    for (i, v) in [row.point, row.plane, row.composed, row.other].into_iter().enumerate() {
        v.store(mem, at + 0x20 + 0x10 * i as u32);
    }
}

/// E38Ops::M_BD30 (0x4fbd30): appends the contact row of `body` for `event` (`contact_solver::contact_row`; side 1 is the body's,
/// 0 the partner's) to the body's rows (+0x274), first giving it rows when it has none. 1 when stored; 0 when the
/// solver's 64 rows are taken (nothing else is done) or the body's 8 are. `ComposeA` writes the row's composed point
/// into the original's frame below the entry stack pointer `esp`.
pub fn contact_rows(
    host: &mut impl ImpulseHost,
    mem: &mut Mem,
    solver: u32,
    body: u32,
    event: u32,
    side: u32,
    esp: u32,
) -> u32 {
    let mut rows = mem.u32(body + BODY_CONTACT_ROW);
    if rows == 0 {
        let count = mem.u32(solver + SOLVER_CONTACT_COUNT);
        if count >= SOLVER_CONTACT_LIMIT {
            return 0;
        }
        rows = solver + SOLVER_CONTACT_ROWS + count * SOLVER_CONTACT_ROW;
        mem.set_u32(solver + SOLVER_CONTACT_COUNT, count + 1);
        mem.set_u32(rows, body);
        mem.set_u32(rows + 4, 0);
        mem.set_u32(body + BODY_CONTACT_ROW, rows);
    }
    let (tag, material) = event_material(mem, event);
    let contact = RowEvent {
        point_a: Vec4::load(mem, event + 0x60),
        point_b: Vec4::load(mem, event + 0xc0),
        plane: Vec4::load(mem, event + 0x70),
        ids_a: [mem.u32(event + 0x8c), mem.u32(event + 0x90)],
        ids_b: [mem.u32(event + 0xa4), mem.u32(event + 0xa8)],
        flags: mem.u32(event + EVENT_FLAGS),
    };
    let mut row = contact_solver::contact_row(contact, material, tag, side);
    let frame = ((esp - 4) & !0xf) - 0x80;
    let other = event + if side == 0 { 0x60 } else { 0xc0 };
    host.compose(frame - 16, mem.u32(FRAME_GLOBAL), other, body, frame + 0x60);
    row.composed = Vec4::load(mem, frame + 0x60);
    let count = mem.u32(rows + 4);
    if count >= BODY_CONTACT_ROWS {
        return 0;
    }
    store_contact_row(mem, rows + 0x10 + count * BODY_CONTACT_ROW_SIZE, &row);
    mem.set_u32(rows + 4, count + 1);
    1
}

/// E38_004FB890 (0x4fb890): moves the body of an offset row by `contact_solver::offsets_move` of its offsets (+0x140), then
/// unhooks the row from the body (+0x3b0).
pub fn apply_offsets(mem: &mut Mem, row: u32) {
    let body = mem.u32(row);
    let offsets: Vec<Vec4> = (0..mem.u32(row + 4)).map(|i| Vec4::load(mem, row + 0x10 + 0x10 * i)).collect();
    if let Some(step) = contact_solver::offsets_move(&offsets) {
        let at = body + BODY_POSITION;
        (step + Vec4::load(mem, at)).store(mem, at);
    }
    mem.set_u32(body + BODY_OFFSET_ROW, 0);
}

/// E38Ops::M_C1A0 (0x4fc1a0): the impulse of one event, the x87 result. An event `contact_solver::impulse_due` passes gets the
/// normal impulse (`contact_solver::normal_impulse`) and, when it slips, friction (`contact_solver::friction_impulse`); `E37EvOps::Emit4DC0`
/// applies them. The vectors the callees take and the floats they write are in the original's frame below the entry
/// stack pointer `esp`; its last call, the empty `E21Telem::Telem80` (0x499e80), is left out.
pub fn solve_event(host: &mut impl ImpulseHost, mem: &mut Mem, event: u32, esp: u32) -> f32 {
    let frame = ((esp - 4) & !0xf) - 0x90;
    let (normal_at, velocity_at, direction_at, slip_at, impulse_at) =
        (frame + 0x40, frame + 0x50, frame + 0x60, frame + 0x70, frame + 0x80);
    let (response_at, along_at, across_at, unused_at) = (frame + 0x18, frame + 0x20, frame + 0x1c, frame + 0x24);
    host.relative_velocity(frame - 8, event, velocity_at);
    let velocity = Vec4::load(mem, velocity_at);
    let normal = Vec4::load(mem, event + 0x70);
    let speed = contact_solver::approach_speed(velocity, normal);
    let suppressed = super::event_pool::is_suppressed(mem, event);
    if !contact_solver::impulse_due(speed, mem.u32(event + EVENT_FLAGS), suppressed) {
        return 0.0;
    }
    let slip = contact_solver::slip(velocity, normal, speed);
    let (_, material) = event_material(mem, event);
    let k = contact_solver::speed_blend(speed);
    let friction = contact_solver::friction(material, k);
    normal.store(mem, normal_at);
    slip.direction.unwrap_or_default().store(mem, direction_at);
    let slips = u32::from(slip.direction.is_some());
    host.response(frame - 24, event, normal_at, direction_at, slips, response_at, unused_at);
    let impulse = contact_solver::normal_impulse(material, k, mem.f32(response_at), speed);
    Vec4::load(mem, normal_at).scale(impulse).store(mem, impulse_at);
    if slips != 0 {
        host.response(frame - 24, event, direction_at, normal_at, slips, along_at, across_at);
        let vector = contact_solver::friction_impulse(
            Vec4::load(mem, impulse_at),
            Vec4::load(mem, direction_at),
            impulse,
            slip.speed,
            friction,
            mem.f32(along_at),
            mem.f32(across_at),
        );
        vector.store(mem, impulse_at);
    }
    slip.velocity.store(mem, slip_at);
    host.apply_impulse(frame - 20, event, impulse, slip.speed, impulse_at, slip_at);
    impulse
}

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
