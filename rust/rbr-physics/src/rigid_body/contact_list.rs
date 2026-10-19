//! `CSolid::BigB` (0x52c220): the contact list step. `E36Collect` gathers the body's contacts into a list of 0x40-byte
//! records (none when it returns 0, and then the step ends). A kind-200 record on a body with flag 8 is retired. Each
//! record then gives a force from its direction, scaled by its factor and, when its point is off the body's centre,
//! pushed along the point's normal by a spring term from its weight. The force and its torque about the body's position
//! go through the body's transform and are applied through slots 28 and 29. The last record of positive weight goes to
//! the event, which `Emit` sends. The calls outside the body go through `ListHost` and `BodyCalls`; the arithmetic is the
//! asm's, in its order.

use super::host::{BodyCalls, HostUnavailable};
use super::layout::{Body, Vec4};

/// One contact record (`E36Collect`'s list, `0x40` bytes each). Names are by offset: their meaning is open (`README.md`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Record {
    /// `+0x00`: the contact point, in world space (the body's position is subtracted for the torque's arm).
    pub point: Vec4,
    /// `+0x10`: the force's direction.
    pub direction: Vec4,
    /// `+0x20`: copied into the event as it is.
    pub aux: Vec4,
    /// `+0x30`: the weight. The spring term scales with it, and a positive one makes the record the event's.
    pub weight: f32,
    /// `+0x34`: the kind. 200 retires the contact on a body with flag 8.
    pub kind: u32,
    /// `+0x38`: the factor the force's scale takes (halved, then clamped to [0, 1]).
    pub factor: f32,
}

/// The event `BigB` emits: the chosen record, with its point on the body's response.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Event {
    /// `+0x00`: the record's weight.
    pub weight: f32,
    /// `+0x04`: the record's kind.
    pub kind: u32,
    /// `+0x10`: the record's `aux`. (The C++ says `+0x14`; the asm's copies land at `+0x10`, `+0x20`, `+0x30`.)
    pub aux: Vec4,
    /// `+0x20`: the record's point (the original's `src`).
    pub contact: Vec4,
    /// `+0x30`: the response to the record's point.
    pub point: Vec4,
}

/// The calls `BigB` makes outside the body. `collect` is `E36Collect`: `None` when it returns 0 (no list; the step ends).
/// `retire` is `E34_RetireA`, `apply_force` and `apply_torque` the body's slots 28 and 29, `emit` the DampingCollision
/// `Emit`. The body's slot-80 response and its transform come through `BodyCalls`.
pub trait ListHost: BodyCalls {
    fn collect(&mut self) -> Result<Option<Vec<Record>>, HostUnavailable>;
    fn retire(&mut self) -> Result<(), HostUnavailable>;
    fn apply_force(&mut self, force: Vec4) -> Result<(), HostUnavailable>;
    fn apply_torque(&mut self, torque: Vec4) -> Result<(), HostUnavailable>;
    fn emit(&mut self, event: &Event) -> Result<(), HostUnavailable>;
}

/// The factor the force's scale takes: `f * 0.5` clamped to [0, 1]. A NaN passes through, as the asm's compares do.
fn factor(f: f32) -> f32 {
    let f = f * 0.5;
    if 0.0 > f {
        0.0
    } else if f > 1.0 {
        1.0
    } else {
        f
    }
}

/// The contact list step (`CSolid::BigB`). The calls are made in the original's order. With no list the step makes
/// no call after `collect`; with no positive weight the event is the first record's.
pub fn contact_list_step(body: &Body, host: &mut impl ListHost) -> Result<(), HostUnavailable> {
    let Some(records) = host.collect()? else {
        return Ok(());
    };
    for r in &records {
        if r.kind == 200 && body.body_flags & 8 != 0 {
            host.retire()?;
        }
    }
    let best = records.iter().rposition(|r| r.weight > 0.0).unwrap_or(0);
    let mut event = Event::default();
    for (i, r) in records.iter().enumerate() {
        let p = r.point;
        let offset = Vec4 { x: p.x - body.position.x, y: p.y - body.position.y, z: p.z - body.position.z, w: 0.0 };
        let point = host.response(p);
        let len = point.length3();
        let mut force = r.direction.scale(factor(r.factor) * 5000.0);
        if 0.0001 < len {
            // The normal is taken before the length is clamped at 10; the spring term uses the clamped length.
            let normal = point.scale(1.0 / len);
            let len = if len > 10.0 { 10.0 } else { len };
            let spring = r.weight * 5.0;
            let spring = if spring > 1.0 { 1.0 } else { spring };
            let k = spring * len * -50.0;
            force = Vec4 {
                x: normal.x * k + force.x,
                y: normal.y * k + force.y,
                z: normal.z * k + force.z,
                w: normal.w * k + force.w,
            };
        }
        let torque = offset.cross3(force);
        let force_out = host.transform(force);
        let torque_out = host.transform(torque);
        host.apply_force(force_out)?;
        host.apply_torque(torque_out)?;
        if i == best {
            event = Event { weight: r.weight, kind: r.kind, aux: r.aux, contact: p, point };
        }
    }
    host.emit(&event)
}
