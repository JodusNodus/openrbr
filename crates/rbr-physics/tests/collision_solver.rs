//! The E38 solver passes (0x4fb7f0-0x4fca20) on hand-built solvers, with a host that logs the calls. rust_step checks
//! the same passes against the original on the drive (`rust/rbr-hybrid/bound.csv`).

mod common;

use common::shared_mem;
use rbr_physics::collision::e38::{self, Correction, SolverContact, SolverHost};
use rbr_physics::collision::layout::{self, Vec4};
use rbr_physics::math::vec::Vec3;
use rbr_physics::mem::Mem;
use rbr_physics::Variant;

const SOLVER: u32 = 0x1000_0000;
const BODY: u32 = 0x2000_0000;
const PARTNER: u32 = 0x2000_1000;
const EVENT: u32 = 0x3000_0000;
const VTABLE: u32 = 0x4000_0000;

#[derive(Debug, PartialEq)]
enum Call {
    InverseMass(u32),
    Test(u32, u32),
    Wake(u32),
    Rows(u32, u32, u32),
    Apply(u32),
    Solve(u32),
}

/// Inverse masses per body, slot 0x80/0x8c results per body, and the call log.
#[derive(Default)]
struct Log {
    masses: Vec<(u32, f64)>,
    tests: Vec<(u32, u32, u32)>,
    calls: Vec<Call>,
}

impl SolverHost for Log {
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64 {
        assert_eq!(target, VTABLE + 0x13c);
        self.calls.push(Call::InverseMass(body));
        self.masses.iter().find(|m| m.0 == body).unwrap().1
    }

    fn body_test(&mut self, target: u32, body: u32) -> u32 {
        let slot = target - VTABLE;
        self.calls.push(Call::Test(body, slot));
        self.tests.iter().find(|t| t.0 == body && t.1 == slot).map_or(0, |t| t.2)
    }

    fn body_wake(&mut self, target: u32, body: u32) {
        assert_eq!(target, VTABLE + 0x90);
        self.calls.push(Call::Wake(body));
    }

    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32) {
        assert_eq!(solver, SOLVER);
        self.calls.push(Call::Rows(body, event, side));
    }

    fn apply_offsets(&mut self, row: u32) {
        self.calls.push(Call::Apply(row));
    }

    fn solve_event(&mut self, solver: u32, event: u32) {
        assert_eq!(solver, SOLVER);
        self.calls.push(Call::Solve(event));
    }
}

/// A solver with `events` (each 0x100 bytes from EVENT, linked through +0xc) and two active bodies.
fn world(events: u32) -> Mem {
    let mut mem = shared_mem();
    mem.map(SOLVER, &vec![0; 0xc430 + 0x90 * 64]);
    mem.map(BODY, &vec![0; 0x400]);
    mem.map(PARTNER, &vec![0; 0x400]);
    mem.map(VTABLE, &vec![0; 0x200]);
    mem.map(EVENT, &vec![0; 0x100 * events as usize]);
    // Each slot's target is its own address in the table.
    for slot in [0x80, 0x8c, 0x90, 0x13c] {
        mem.set_u32(VTABLE + slot, VTABLE + slot);
    }
    for body in [BODY, PARTNER] {
        mem.set_u32(body, VTABLE);
        mem.set_u8(body + 0x60, 4);
    }
    mem.set_u32(SOLVER + 4, EVENT);
    mem.set_u32(SOLVER + 0xc, events);
    for k in 0..events {
        let event = EVENT + 0x100 * k;
        mem.set_u32(event + 0xa0, BODY);
        mem.set_u32(event + 0x98, PARTNER);
        if k + 1 < events {
            mem.set_u32(event + 0xc, event + 0x100);
        }
    }
    mem
}

fn set_contact(mem: &mut Mem, event: u32, flags: u32, plane: [f32; 4], point: [f32; 3]) {
    mem.set_u32(event + 0xe4, flags);
    for (k, v) in plane.iter().enumerate() {
        mem.set_f32(event + 0x70 + 4 * k as u32, *v);
    }
    for (k, v) in point.iter().enumerate() {
        mem.set_f32(event + 0xc0 + 4 * k as u32, *v);
    }
}

#[test]
fn penetration_plan_follows_the_flag_branches() {
    let contact = SolverContact {
        flags: 0x800,
        partner_active: true,
        plane: Vec4 { x: 0.0, y: 0.0, z: 1.0, w: -1.0 },
        point: Vec3 { x: 5.0, y: 5.0, z: 0.5 },
    };
    let depth = 0.5 - 1.0 + e38::PENETRATION_MARGIN;
    let offset = Some(contact.plane.scale(depth));
    let plan = |flags, partner_active| {
        e38::plan_correction(SolverContact { flags, partner_active, ..contact }, e38::PENETRATION_MARGIN)
    };
    assert_eq!(plan(0x800, true), (Correction::Body(offset), e38::PENETRATION_MARGIN));
    assert_eq!(plan(0x1000, true), (Correction::Partner(offset), e38::PENETRATION_MARGIN));
    assert_eq!(plan(0x0, true), (Correction::None, e38::PENETRATION_MARGIN));
    assert_eq!(plan(0x2800, true), (Correction::Body(None), e38::PENETRATION_MARGIN));
    // Both bits with an active partner: the margin becomes -0.001 before the depth.
    let two_sided = Some(contact.plane.scale(0.5 - 1.0 + e38::TWO_SIDED_MARGIN));
    assert_eq!(plan(0x1800, true), (Correction::Split(two_sided), e38::TWO_SIDED_MARGIN));
    assert_eq!(plan(0x1800, false), (Correction::Split(offset), e38::PENETRATION_MARGIN));
    // Above the plane: no offset.
    let above = SolverContact { point: Vec3 { z: 2.0, ..contact.point }, ..contact };
    assert_eq!(e38::plan_correction(above, e38::PENETRATION_MARGIN).0, Correction::Body(None));
    // inv_a = 1, inv_b = 3: f = 1 / (1 + 1/3) = 0.75, the body moves by -0.25.
    let unit = Vec4 { x: 1.0, y: 2.0, z: 3.0, w: 4.0 };
    let (a, b) = e38::split_offsets(unit, 1.0, 3.0, Variant::Stock102);
    assert_eq!(a, unit.scale(-0.25));
    assert_eq!(b, unit.scale(0.75));
}

#[test]
fn correct_penetrations_collects_and_applies_offsets() {
    let mut mem = world(3);
    // Event 0: body only, 0.48 below its plane. Event 1: both sides (margin -0.001 from here on). Event 2: partner only.
    set_contact(&mut mem, EVENT, 0x800, [0.0, 0.0, 1.0, -1.0], [0.0, 0.0, 0.5]);
    set_contact(&mut mem, EVENT + 0x100, 0x1800, [0.0, 1.0, 0.0, 0.0], [0.0, -0.5, 0.0]);
    set_contact(&mut mem, EVENT + 0x200, 0x1000, [1.0, 0.0, 0.0, 0.0], [-0.25, 0.0, 0.0]);
    let mut host = Log { masses: vec![(BODY, 1.0), (PARTNER, 3.0)], ..Log::default() };
    assert_eq!(layout::correct_penetrations(&mut host, &mut mem, SOLVER, Variant::Stock102), 2);
    let rows = [SOLVER + 0xc430, SOLVER + 0xc430 + 0x90];
    assert_eq!(
        host.calls,
        [
            Call::Rows(BODY, EVENT, 1),
            Call::InverseMass(BODY),
            Call::InverseMass(PARTNER),
            Call::Rows(PARTNER, EVENT + 0x200, 0),
            Call::Apply(rows[0]),
            Call::Apply(rows[1]),
        ]
    );
    assert_eq!(mem.u32(SOLVER + 0xc420), 0);
    assert_eq!((mem.u32(BODY + 0x3b0), mem.u32(PARTNER + 0x3b0)), (rows[0], rows[1]));
    assert_eq!((mem.u32(rows[0]), mem.u32(rows[0] + 4)), (BODY, 2), "body row: event 0's and event 1's offsets");
    assert_eq!((mem.u32(rows[1]), mem.u32(rows[1] + 4)), (PARTNER, 2));
    let d0 = 0.5f32 - 1.0 + 0.02;
    assert_eq!(Vec4::load(&mem, rows[0] + 0x10), Vec4 { x: 0.0, y: 0.0, z: 1.0, w: -1.0 }.scale(d0).scale(-1.0));
    let d1 = Vec4 { x: 0.0, y: 1.0, z: 0.0, w: 0.0 }.scale(-0.5f32 + -0.001);
    assert_eq!(Vec4::load(&mem, rows[0] + 0x20), d1.scale(-0.25));
    assert_eq!(Vec4::load(&mem, rows[1] + 0x10), d1.scale(0.75));
    // Event 2 with the margin -0.001: depth -0.251.
    let d2 = Vec4 { x: 1.0, y: 0.0, z: 0.0, w: 0.0 }.scale(-0.25f32 + -0.001);
    assert_eq!(Vec4::load(&mem, rows[1] + 0x20), d2);
}

#[test]
fn add_body_offset_stops_at_full_rows() {
    let mut mem = world(0);
    let offset = Vec4 { x: 1.0, y: 2.0, z: 3.0, w: 4.0 };
    for k in 0..8 {
        assert!(layout::add_body_offset(&mut mem, SOLVER, BODY, offset));
        assert_eq!(mem.u32(SOLVER + 0xc430 + 4), k + 1);
    }
    assert!(!layout::add_body_offset(&mut mem, SOLVER, BODY, offset));
    mem.set_u32(SOLVER + 0xc420, 0x40);
    assert!(!layout::add_body_offset(&mut mem, SOLVER, PARTNER, offset));
    assert_eq!(mem.u32(PARTNER + 0x3b0), 0);
}

#[test]
fn solve_due_events_wakes_movable_bodies_first() {
    let mut mem = world(2);
    mem.set_f32(BODY + 0x290, 0.5);
    mem.set_f32(PARTNER + 0x290, 0.3);
    // Event 0 is due (0.35 - 0.1 <= 0.3); event 1 is not (0.45 - 0.1 > 0.3).
    mem.set_f32(EVENT + 0x94, 0.35);
    mem.set_f32(EVENT + 0x194, 0.45);
    let mut host = Log { masses: vec![(BODY, 1.0), (PARTNER, 0.0)], tests: vec![(BODY, 0x80, 0)], ..Log::default() };
    layout::solve_due_events(&mut host, &mut mem, SOLVER);
    assert_eq!(
        host.calls,
        [
            Call::InverseMass(BODY),
            Call::Test(BODY, 0x80),
            Call::Test(BODY, 0x8c),
            Call::Wake(BODY),
            Call::InverseMass(PARTNER),
            Call::Solve(EVENT),
        ]
    );
    assert!(e38::event_due(0.35, 0.5, None));
    assert!(e38::event_due(f32::NAN, 0.5, None), "NaN is not above the limit");
}

#[test]
fn refresh_events_sets_the_mass_and_material_flags() {
    const APP: u32 = 0x5000_0000;
    const MANAGER: u32 = 0x5100_0000;
    let mut mem = world(1);
    mem.map(0x0165_9b70, &APP.to_le_bytes());
    mem.map(APP, &[0; 0x40]);
    mem.set_u32(APP + 0x24, MANAGER - 0x18);
    mem.map(MANAGER, &vec![0; 0x160 * 4]);
    // Material 2's flag byte (+0x158 of its entry): enabled (bit 0) and suppressing (bit 3).
    mem.set_u8(MANAGER + 0xc + 2 * 0x160 + 0x158, 9);
    mem.set_u32(EVENT + 0xac, 2);
    mem.set_u32(EVENT + 0x80, 1);
    mem.set_f32(EVENT + 0x88, 0.0);
    mem.set_f32(EVENT + 0xb4, 0.0);
    let mut host = Log { masses: vec![(BODY, 1.0), (PARTNER, 2.0)], ..Log::default() };
    layout::refresh_events(&mut host, &mut mem, SOLVER);
    assert_eq!(mem.u32(EVENT + 0xe4), 0x2000 | 0x800 | 0x1000);
    mem.set_u32(EVENT + 0xe4, 0);
    mem.set_u8(PARTNER + 0x60, 0);
    host.calls.clear();
    layout::refresh_events(&mut host, &mut mem, SOLVER);
    assert_eq!(mem.u32(EVENT + 0xe4), 0x2000 | 0x800);
    assert_eq!(host.calls, [Call::InverseMass(BODY)]);
}

#[test]
fn contact_rows_clear_and_event_count() {
    let mut mem = world(0);
    mem.set_u32(SOLVER + 0x18, 2);
    mem.set_u32(SOLVER + 0x20, BODY);
    mem.set_u32(BODY + 0x274, SOLVER + 0x20);
    assert_eq!(layout::clear_contact_rows(&mut mem, SOLVER), 2);
    assert_eq!((mem.u32(SOLVER + 0x18), mem.u32(BODY + 0x274)), (0, 0));
    assert_eq!(layout::clear_contact_rows(&mut mem, SOLVER), 0);
    mem.set_u32(SOLVER + 0xc, 7);
    assert_eq!(layout::event_count(&mem, SOLVER), 7);
}

#[test]
fn drain_events_declines_another_release() {
    let mut mem = world(1);
    mem.set_u32(EVENT, VTABLE);
    mem.set_u32(VTABLE + 0x1c, 0x401000);
    assert_eq!(layout::drain_events(&mut mem, SOLVER), None);
    assert_eq!(mem.u32(SOLVER + 0xc), 1, "nothing changed");
}
