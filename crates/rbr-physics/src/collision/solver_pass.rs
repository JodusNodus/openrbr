//! The E38 solver's passes over its events, contact rows and offset rows, generic over the store.
//!
//! The model is `contact_solver`; the memory is `CollisionStore`; the calls into the original are `SolverHost` and
//! `ImpulseHost` (`layout/solver_hosts.rs`).

use super::contact_solver::{self, Correction, SolverContact};
use super::layout::solver_hosts::{ImpulseHost, SolverHost};
use super::store::{CollisionStore, Side};
use crate::math::vec::Vec4;

/// The virtual slots the passes call on a body.
const SLOT_INVERSE_MASS: u32 = 0x13c;
const SLOT_TEST_A: u32 = 0x80;
const SLOT_TEST_B: u32 = 0x8c;
const SLOT_WAKE: u32 = 0x90;

/// An offset row holds 8 offsets after its header; the solver has 64 rows. A body's contact row holds 8 rows, the
/// solver's 64.
const OFFSETS_PER_ROW: u32 = 8;
const OFFSET_ROW_LIMIT: u32 = 0x40;
const BODY_CONTACT_ROWS: u32 = 8;
const SOLVER_CONTACT_LIMIT: u32 = 0x40;

/// E38_004FB7F0 (0x4fb7f0): unhooks the body of every contact row (+0x274) and empties the rows. eax is the count as
/// the asm last read it.
pub fn clear_contact_rows<S: CollisionStore>(store: &mut S, solver: u32) -> u32 {
    let mut count = store.solver_contact_count(solver);
    let mut index = 0;
    while index < count {
        let body = store.solver_contact_body(solver, index);
        if body != 0 {
            store.set_body_contact_rows(body, 0);
        }
        count = store.solver_contact_count(solver);
        index += 1;
    }
    store.set_solver_contact_count(solver, 0);
    count
}

/// E38_004FBCA0 (0x4fbca0): the solver's event count.
pub fn event_count<S: CollisionStore>(store: &S, solver: u32) -> u32 {
    store.event_count(solver)
}

/// CCollisionSolver::Vf08 (0x4fc0b0): drains the event list, releasing each event (`list::drain` with the pool flag).
/// `None` (nothing changed) when an event's slot 0x1c is not `CCollisionEvent::Vf1Cx`, which the drain port assumes.
pub fn drain_events<S: CollisionStore>(store: &mut S, solver: u32) -> Option<()> {
    let mut event = store.first_event(solver);
    for _ in 0..store.event_list_len(solver) {
        if event == 0 {
            break;
        }
        if !store.event_releases_to_pool(event) {
            return None;
        }
        event = store.next_event(event);
    }
    store.drain_event_list(solver);
    Some(())
}

/// E38_004FC500 (0x4fc500): the solver's fraction, the earliest of its events' (`CCollisionEvent::Get94`, +0x94) from
/// `FRACTION_START`. An event with a material flag (`E37Test4B20`) is released instead: unlinked from the list when it
/// is on it (`E37Unlink`, the count first) and returned to the pool (slot 0x1c with 1). The next event is read before.
/// `None` (nothing changed) when a released event's slot 0x1c is not `CCollisionEvent::Vf1Cx`, which the port assumes.
pub fn min_contact_time<S: CollisionStore>(store: &mut S, solver: u32) -> Option<f32> {
    let mut event = store.first_event(solver);
    while event != 0 {
        if store.event_suppressed(event) && !store.event_releases_to_pool(event) {
            return None;
        }
        event = store.next_event(event);
    }
    let mut best = contact_solver::FRACTION_START;
    let mut event = store.first_event(solver);
    while event != 0 {
        let next = store.next_event(event);
        if store.event_suppressed(event) {
            store.release_event(solver, event);
            store.pool_event(event);
        } else {
            best = contact_solver::earlier_fraction(best, store.event_time(event));
        }
        event = next;
    }
    Some(best)
}

/// E37Flag4AC0 (0x4f4ac0): sets the event's flag 0x2000 when the material of its id at +0xac has flag bit 3 (+0x158),
/// and 0x4000 for the id at +0x80 (`CMaterialManager::Tag520940` on the manager).
pub fn mark_suppressing_materials<S: CollisionStore>(store: &mut S, event: u32) {
    for (side, flag) in [(Side::Body, 0x2000), (Side::Partner, 0x4000)] {
        if store.side_suppresses(event, side) {
            let flags = store.event_flags(event);
            store.store_event_flags(event, flags | flag);
        }
    }
}

fn set_event_flag<S: CollisionStore>(store: &mut S, event: u32, flag: u32) {
    let flags = store.event_flags(event);
    store.store_event_flags(event, flags | flag);
}

fn inverse_mass<S: CollisionStore>(host: &mut impl SolverHost, store: &S, body: u32) -> f64 {
    host.inverse_mass(store.vtable_slot(body, SLOT_INVERSE_MASS), body)
}

/// E38_004FBC20 (0x4fbc20): per event, the material refresh (`event::refresh_materials`) and suppress flags, then flag
/// 0x800 when the body is movable and 0x1000 when the active partner is.
pub fn refresh_events<S: CollisionStore>(host: &mut impl SolverHost, store: &mut S, solver: u32) {
    let mut event = store.first_event(solver);
    while event != 0 {
        store.refresh_event_materials(event);
        mark_suppressing_materials(store, event);
        let body = store.event_body(event);
        if contact_solver::movable(inverse_mass(host, store, body)) {
            set_event_flag(store, event, 0x800);
        }
        let partner = store.event_partner(event);
        if store.body_active(partner) && contact_solver::movable(inverse_mass(host, store, partner)) {
            set_event_flag(store, event, 0x1000);
        }
        event = store.next_event(event);
    }
}

/// E38Ops::M_BFC0X (0x4fbfc0): appends `offset` to the body's offset row (+0x3b0, up to 8), or starts a row for it
/// (up to 64). False when full. The count is written before the copy, as the asm does.
pub fn add_body_offset<S: CollisionStore>(store: &mut S, solver: u32, body: u32, offset: Vec4) -> bool {
    let row = store.body_offset_row(body);
    if row != 0 {
        let count = store.offset_row_count(row);
        if count >= OFFSETS_PER_ROW {
            return false;
        }
        store.set_offset_row_count(row, count + 1);
        store.store_offset(row, count, offset);
        return true;
    }
    let rows = store.solver_offset_count(solver);
    if rows >= OFFSET_ROW_LIMIT {
        return false;
    }
    store.set_solver_offset_count(solver, rows + 1);
    let row = store.solver_offset_row(solver, rows);
    store.store_offset(row, 0, offset);
    store.open_offset_row(row, body);
    true
}

/// E38_004FC590 (0x4fc590): moves the bodies of the events out of their contact planes (`contact_solver::plan_correction`),
/// collecting the moves in offset rows, then applies every row and empties them. eax is the row count as the asm
/// last read it.
pub fn correct_penetrations<S: CollisionStore>(host: &mut impl SolverHost, store: &mut S, solver: u32) -> u32 {
    let mut margin = contact_solver::PENETRATION_MARGIN;
    let mut event = store.first_event(solver);
    while event != 0 {
        let body = store.event_body(event);
        if store.body_active(body) {
            let piece = store.event_partner(event);
            let partner_active = store.body_active(piece);
            let partner = if partner_active { piece } else { 0 };
            let at = store.event_plane(event);
            let contact = SolverContact { flags: at.flags, partner_active, plane: at.plane, point: at.point };
            let (correction, next_margin) = contact_solver::plan_correction(contact, margin);
            margin = next_margin;
            match correction {
                Correction::None | Correction::Split(None) => {}
                Correction::Split(Some(offset)) => {
                    let inv_a = inverse_mass(host, store, body);
                    let inv_b = inverse_mass(host, store, partner);
                    let (body_offset, partner_offset) = contact_solver::split_offsets(offset, inv_a, inv_b);
                    add_body_offset(store, solver, body, body_offset);
                    add_body_offset(store, solver, partner, partner_offset);
                }
                Correction::Body(offset) => {
                    if let Some(offset) = offset {
                        add_body_offset(store, solver, body, offset.scale(-1.0));
                    }
                    host.contact_rows(solver, body, event, 1);
                }
                Correction::Partner(offset) => {
                    if let Some(offset) = offset {
                        add_body_offset(store, solver, partner, offset.scale(1.0));
                    }
                    host.contact_rows(solver, partner, event, 0);
                }
            }
        }
        event = store.next_event(event);
    }
    let mut count = store.solver_offset_count(solver);
    let mut index = 0;
    while index < count {
        host.apply_offsets(store.solver_offset_row(solver, index));
        count = store.solver_offset_count(solver);
        index += 1;
    }
    store.set_solver_offset_count(solver, 0);
    count
}

/// Slot 0x13c gates slots 0x80, 0x8c and 0x90 on a body in E38_004FCA20.
fn wake_if_movable<S: CollisionStore>(host: &mut impl SolverHost, store: &S, body: u32) {
    if contact_solver::movable(inverse_mass(host, store, body))
        && host.body_test(store.vtable_slot(body, SLOT_TEST_A), body) == 0
        && host.body_test(store.vtable_slot(body, SLOT_TEST_B), body) == 0
    {
        host.body_wake(store.vtable_slot(body, SLOT_WAKE), body);
    }
}

/// E38_004FCA20 (0x4fca20): solves each due event (`contact_solver::event_due`, time `CCollisionEvent::Get94`), first waking its
/// movable body and active partner.
pub fn solve_due_events<S: CollisionStore>(host: &mut impl SolverHost, store: &mut S, solver: u32) {
    let mut event = store.first_event(solver);
    while event != 0 {
        let piece = store.event_partner(event);
        let partner = store.body_active(piece).then_some(piece);
        let body = store.event_body(event);
        let time = store.event_time(event);
        let progress = store.body_progress(body);
        if contact_solver::event_due(time, progress, partner.map(|p| store.body_progress(p))) {
            wake_if_movable(host, store, body);
            if let Some(partner) = partner {
                wake_if_movable(host, store, partner);
            }
            host.solve_event(solver, event);
        }
        event = store.next_event(event);
    }
}

/// E38Ops::M_BD30 (0x4fbd30): appends the contact row of `body` for `event` (`contact_solver::contact_row`; side 1 is the body's,
/// 0 the partner's) to the body's rows (+0x274), first giving it rows when it has none. 1 when stored; 0 when the
/// solver's 64 rows are taken (nothing else is done) or the body's 8 are. `ComposeA` writes the row's composed point
/// into the original's frame below the entry stack pointer `esp`.
pub fn contact_rows<S: CollisionStore>(
    host: &mut impl ImpulseHost,
    store: &mut S,
    solver: u32,
    body: u32,
    event: u32,
    side: u32,
    esp: u32,
) -> u32 {
    let mut rows = store.body_contact_rows(body);
    if rows == 0 {
        let count = store.solver_contact_count(solver);
        if count >= SOLVER_CONTACT_LIMIT {
            return 0;
        }
        rows = store.solver_contact_row(solver, count);
        store.set_solver_contact_count(solver, count + 1);
        store.open_contact_row(rows, body);
    }
    let (tag, material) = store.event_material(event);
    let mut row = contact_solver::contact_row(store.row_event(event), material, tag, side);
    let frame = ((esp - 4) & !0xf) - 0x80;
    let other = store.event_other_point(event, side);
    host.compose(frame - 16, store.frame_root(), other, body, frame + 0x60);
    row.composed = store.vec4(frame + 0x60);
    let count = store.contact_row_count(rows);
    if count >= BODY_CONTACT_ROWS {
        return 0;
    }
    store.store_contact_row(rows, count, &row);
    store.set_contact_row_count(rows, count + 1);
    1
}

/// E38_004FB890 (0x4fb890): moves the body of an offset row by `contact_solver::offsets_move` of its offsets (+0x140), then
/// unhooks the row from the body (+0x3b0).
pub fn apply_offsets<S: CollisionStore>(store: &mut S, row: u32) {
    let body = store.offset_row_body(row);
    let offsets: Vec<Vec4> = (0..store.offset_row_count(row)).map(|i| store.offset(row, i)).collect();
    if let Some(step) = contact_solver::offsets_move(&offsets) {
        let position = store.body_position(body);
        store.store_body_position(body, step + position);
    }
    store.set_body_offset_row(body, 0);
}

/// E38Ops::M_C1A0 (0x4fc1a0): the impulse of one event, the x87 result. An event `contact_solver::impulse_due` passes gets the
/// normal impulse (`contact_solver::normal_impulse`) and, when it slips, friction (`contact_solver::friction_impulse`); `E37EvOps::Emit4DC0`
/// applies them. The vectors the callees take and the floats they write are in the original's frame below the entry
/// stack pointer `esp`; its last call, the empty `E21Telem::Telem80` (0x499e80), is left out.
pub fn solve_event<S: CollisionStore>(host: &mut impl ImpulseHost, store: &mut S, event: u32, esp: u32) -> f32 {
    let frame = ((esp - 4) & !0xf) - 0x90;
    let (normal_at, velocity_at, direction_at, slip_at, impulse_at) =
        (frame + 0x40, frame + 0x50, frame + 0x60, frame + 0x70, frame + 0x80);
    let (response_at, along_at, across_at, unused_at) = (frame + 0x18, frame + 0x20, frame + 0x1c, frame + 0x24);
    host.relative_velocity(frame - 8, event, velocity_at);
    let velocity = store.vec4(velocity_at);
    let normal = store.event_normal(event);
    let speed = contact_solver::approach_speed(velocity, normal);
    let suppressed = store.event_suppressed(event);
    if !contact_solver::impulse_due(speed, store.event_flags(event), suppressed) {
        return 0.0;
    }
    let slip = contact_solver::slip(velocity, normal, speed);
    let (_, material) = store.event_material(event);
    let k = contact_solver::speed_blend(speed);
    let friction = contact_solver::friction(material, k);
    store.store_vec4(normal_at, normal);
    store.store_vec4(direction_at, slip.direction.unwrap_or_default());
    let slips = u32::from(slip.direction.is_some());
    host.response(frame - 24, event, normal_at, direction_at, slips, response_at, unused_at);
    let impulse = contact_solver::normal_impulse(material, k, store.f32(response_at), speed);
    let scaled = store.vec4(normal_at).scale(impulse);
    store.store_vec4(impulse_at, scaled);
    if slips != 0 {
        host.response(frame - 24, event, direction_at, normal_at, slips, along_at, across_at);
        let vector = contact_solver::friction_impulse(
            store.vec4(impulse_at),
            store.vec4(direction_at),
            impulse,
            slip.speed,
            friction,
            store.f32(along_at),
            store.f32(across_at),
        );
        store.store_vec4(impulse_at, vector);
    }
    store.store_vec4(slip_at, slip.velocity);
    host.apply_impulse(frame - 20, event, impulse, slip.speed, impulse_at, slip_at);
    impulse
}
