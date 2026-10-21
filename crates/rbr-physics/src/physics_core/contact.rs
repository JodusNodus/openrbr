//! Which solids and collision events the contact step advances. The walks over the lists are in `layout`.

/// The solid flag bit (bit 2 of its flag byte) that makes the contact step move the solid itself.
const ADVANCED_BY_CONTACT_STEP: u8 = 4;

/// Whether the contact step moves a solid with these flags.
pub fn advanced_by_contact_step(flags: u8) -> bool {
    flags & ADVANCED_BY_CONTACT_STEP != 0
}

/// Whether a collision event's fraction (named by use: the event's position in the step, 1 being the end) lies before
/// the end of the step. An equal or unordered fraction does not.
pub fn event_before_step_end(fraction: f32) -> bool {
    fraction < 1.0
}
