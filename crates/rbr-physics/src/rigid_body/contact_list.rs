//! The contact list step: the collector gathers the body's contacts (none, and then the step ends). A kind-200
//! record on a flagged body is retired. Each record then gives a force from its direction, scaled by its factor and,
//! when its point is off the body's centre, pushed along the point's normal by a spring term from its weight. The
//! force and its torque about the body's position go through the body's transform and are applied. The last record
//! of positive weight goes to the event, which the emitter sends. The calls outside the body go through `ListHost`
//! and `BodyCalls`; the arithmetic is in operation order.

use super::host::{BodyCalls, HostUnavailable};
use super::layout::{Body, Vec4};

/// One contact record of the collected list. Names are by role where the meaning is known, by position otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Record {
    /// The contact point, in world space (the body's position is subtracted for the torque's arm).
    pub point: Vec4,
    /// The force's direction.
    pub direction: Vec4,
    /// Copied into the event as it is.
    pub aux: Vec4,
    /// The weight. The spring term scales with it, and a positive one makes the record the event's.
    pub weight: f32,
    /// The kind. 200 retires the contact on a flagged body.
    pub kind: u32,
    /// The factor the force's scale takes (halved, then clamped to [0, 1]).
    pub factor: f32,
}

/// The event the step emits: the chosen record, with its point on the body's response.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Event {
    /// The record's weight.
    pub weight: f32,
    /// The record's kind.
    pub kind: u32,
    /// The record's `aux`.
    pub aux: Vec4,
    /// The record's point.
    pub contact: Vec4,
    /// The response to the record's point.
    pub point: Vec4,
}

/// The calls the step makes outside the body. `collect` gathers the records: `None` when there are none (the step
/// ends). `retire` retires a kind-200 contact; `apply_force` and `apply_torque` apply the transformed force and
/// torque; `emit` sends the event. The body's response and its transform come through `BodyCalls`.
pub trait ListHost: BodyCalls {
    fn collect(&mut self) -> Result<Option<Vec<Record>>, HostUnavailable>;
    fn retire(&mut self) -> Result<(), HostUnavailable>;
    fn apply_force(&mut self, force: Vec4) -> Result<(), HostUnavailable>;
    fn apply_torque(&mut self, torque: Vec4) -> Result<(), HostUnavailable>;
    fn emit(&mut self, event: &Event) -> Result<(), HostUnavailable>;
}

/// The factor the force's scale takes: `f * 0.5` clamped to [0, 1]. A NaN passes through, as unordered compares do.
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

/// The contact list step. The calls are made in order. With no list the step makes no call after `collect`; with
/// no positive weight the event is the first record's.
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
