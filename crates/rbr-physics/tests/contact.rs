//! The damper workers `CPhysics::Work5640` (0x525640) and `Work5760` (0x525760) on the event list, against the original's
//! order and gates (`physics_core::contact` and `physics_core::layout`).

use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout;
use rbr_physics::physics_core::layout::contact_walk::SolidHost;

/// Records the damper calls in order: (what, solid, the slot's target or the float's bits), and answers slot 0x80
/// with `.1`.
#[derive(Default)]
struct Damper(Vec<(&'static str, u32, Option<u32>)>, u32);

impl SolidHost for Damper {
    fn takes_part(&mut self, target: u32, solid: u32) -> u32 {
        self.0.push(("80", solid, Some(target)));
        self.1
    }

    fn back_step(&mut self, solid: u32, f: f32) {
        self.0.push(("dx", solid, Some(f.to_bits())));
    }

    fn advance_rest(&mut self, solid: u32, f: f32) {
        self.0.push(("ex", solid, Some(f.to_bits())));
    }
}

/// Every solid's slot 0x80 is at this address, through the vtable at `VTABLE`.
const VTABLE: u32 = 0x6000;
const SLOT_80: u32 = 0x7000;

/// The physics at 0x0f00 with the scratch at 0x1000 and the events from 0x2000 (`+0x14`), linked at +0xc. The first
/// event's sample is 0.5 and its solids are 0x3000 (`+0xa0`, flagged) and 0x3100 (`+0x98`, not); the second's is 1.5
/// with 0x3200 and 0x3300, both flagged. A solid's vtable is `VTABLE` and its flag byte is at +0x60.
fn two_events() -> Mem {
    let mut mem = Mem::new();
    mem.set_u32(0x0F28, 0x1000);
    mem.set_u32(0x1014, 0x2000);
    mem.set_u32(0x2000 + 0x94, 0.5f32.to_bits());
    mem.set_u32(0x2000 + 0xa0, 0x3000);
    mem.set_u32(0x2000 + 0x98, 0x3100);
    mem.set_u32(0x2000 + 0xc, 0x2100);
    mem.set_u32(0x2100 + 0x94, 1.5f32.to_bits());
    mem.set_u32(0x2100 + 0xa0, 0x3200);
    mem.set_u32(0x2100 + 0x98, 0x3300);
    mem.set_u32(0x2100 + 0xc, 0);
    mem.set_u32(VTABLE + 0x80, SLOT_80);
    for (solid, flags) in [(0x3000, 4), (0x3100, 0), (0x3200, 4), (0x3300, 4)] {
        mem.set_u32(solid, VTABLE);
        mem.set_u8(solid + 0x60, flags);
    }
    mem
}

/// `Work5640` drives the flagged solids that answer, with each event's scalar, and visits every solid of both events.
#[test]
fn work_5640_drives_flagged_answering_solids() {
    let mut mem = two_events();
    let mut host = Damper(Vec::new(), 1);
    layout::advance_contact_bodies(&mut mem, &mut host, 0x0F00);
    let dx = |solid, f: f32| ("dx", solid, Some(f.to_bits()));
    assert_eq!(
        host.0,
        [
            ("80", 0x3000, Some(SLOT_80)),
            dx(0x3000, 0.5),
            ("80", 0x3100, Some(SLOT_80)),
            ("80", 0x3200, Some(SLOT_80)),
            dx(0x3200, 1.5),
            ("80", 0x3300, Some(SLOT_80)),
            dx(0x3300, 1.5),
        ]
    );
}

/// `Work5760` skips an event whose scalar is 1.0 or more, and gives `VfEx` the window to the flagged solids.
#[test]
fn work_5760_takes_only_samples_below_one() {
    let mut mem = two_events();
    let mut host = Damper(Vec::new(), 1);
    layout::advance_contact_bodies_remainder(&mut mem, &mut host, 0x0F00, 0.25);
    assert_eq!(
        host.0,
        [("80", 0x3000, Some(SLOT_80)), ("ex", 0x3000, Some(0.25f32.to_bits())), ("80", 0x3100, Some(SLOT_80)),]
    );
}

/// A solid that does not answer slot 0x80 is not driven, whatever its flag.
#[test]
fn unanswered_solids_are_not_driven() {
    let mut mem = two_events();
    let mut host = Damper(Vec::new(), 0);
    layout::advance_contact_bodies(&mut mem, &mut host, 0x0F00);
    assert!(host.0.iter().all(|call| call.0 == "80"));
}
