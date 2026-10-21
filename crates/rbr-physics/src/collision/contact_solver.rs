//! The contact solver's arithmetic: which event is solved when, the penetration correction, the contact rows, the
//! impulse with friction. Every sum keeps the order its instructions gave; the x87 chains run at the thread's
//! precision (`fpu::round_x87`: 24 bits in 1.02, 53 under NGP).

use crate::fpu::{round24, round_x87};
use crate::math::vec::{Vec3, Vec4};
use crate::math::{sign, sse};

/// Where the solver's fraction starts: above every event's.
pub const FRACTION_START: f32 = 2.0;

/// The solver's fraction after an event's: the event's `t` is taken only when the fraction so far is above it, so an
/// unordered pair keeps the fraction. `t` is an f32 the x87 loaded exactly.
pub fn earlier_fraction(best: f32, t: f32) -> f32 {
    if best > t {
        t
    } else {
        best
    }
}

/// The two material tags of a collision object and the weight that picks one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialPair {
    pub low: u32,
    pub high: u32,
    pub weight: f32,
}

/// The material tag of the heavier weight: `high` when the weight is above 0.5.
pub fn pick_material_tag(pair: MaterialPair) -> u32 {
    if 0.5_f32 < pair.weight {
        pair.high
    } else {
        pair.low
    }
}

/// The margin added to a contact's depth, until the first two-sided contact.
pub const PENETRATION_MARGIN: f32 = 0.02;
/// The margin from a contact whose partner takes part too, for that contact and the rest of the pass.
pub const TWO_SIDED_MARGIN: f32 = -0.001;

/// The event flag that moves the body out of the contact.
pub const MOVES_BODY: u32 = 1 << 11;
/// The event flag that moves the partner out of the contact.
pub const MOVES_PARTNER: u32 = 1 << 12;
/// The event flag that suppresses the partner side's row (its restitution is kept and the move is dropped).
pub const SUPPRESSES_PARTNER_SIDE: u32 = 0x2000;
/// The event flag that suppresses the body side's row.
pub const SUPPRESSES_BODY_SIDE: u32 = 0x4000;

/// What the correction pass reads of one contact event.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SolverContact {
    /// The event flags: [`MOVES_BODY`], [`MOVES_PARTNER`]; either suppress flag suppresses the move.
    pub flags: u32,
    /// The partner is active (it takes part in the contact too).
    pub partner_active: bool,
    /// The contact normal and the plane offset (`w`).
    pub plane: Vec4,
    /// The contact point.
    pub point: Vec3,
}

/// The move the correction pass makes for one contact. The offset is `None` when the contact is not below its plane or
/// is suppressed; the rows of the body and the partner are built either way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Correction {
    None,
    /// The body moves by `-offset`, then the body's rows with side 1.
    Body(Option<Vec4>),
    /// The partner moves by `offset`, then the partner's rows with side 0.
    Partner(Option<Vec4>),
    /// The offset is split between the two by inverse mass ([`split_offsets`]); no rows.
    Split(Option<Vec4>),
}

/// The test of one contact: the depth is `n . point + d + margin` (in that order), and the offset the plane times the
/// depth when that is below zero and the contact is not suppressed. Returns the move and the margin for the next
/// contact.
pub fn plan_correction(contact: SolverContact, margin: f32) -> (Correction, f32) {
    let body = contact.flags & MOVES_BODY != 0;
    let partner = contact.flags & MOVES_PARTNER != 0;
    let margin = if body && partner && contact.partner_active { TWO_SIDED_MARGIN } else { margin };
    if !body && !partner {
        return (Correction::None, margin);
    }
    let (n, p) = (contact.plane, contact.point);
    let depth = p.z * n.z + p.y * n.y + p.x * n.x + n.w + margin;
    let suppress = SUPPRESSES_PARTNER_SIDE | SUPPRESSES_BODY_SIDE;
    let offset = (0.0 > depth && contact.flags & suppress == 0).then(|| contact.plane.scale(depth));
    let correction = match (body, partner) {
        (true, true) => Correction::Split(offset),
        (true, false) => Correction::Body(offset),
        _ => Correction::Partner(offset),
    };
    (correction, margin)
}

/// The two-sided move: `f = 1 / (1 + inverse_mass_body / inverse_mass_partner)` on the x87 (at the thread's precision
/// control), the body moves by `-(1 - f) * offset` and the partner by `f * offset`. The body's inverse mass is stored
/// as f32; the partner's divides unstored.
pub fn split_offsets(offset: Vec4, inverse_mass_body: f64, inverse_mass_partner: f64) -> (Vec4, Vec4) {
    let round = round_x87;
    let inverse_mass_body = f64::from(inverse_mass_body as f32);
    let share = round(1.0 / round(round(inverse_mass_body / inverse_mass_partner) + 1.0)) as f32;
    let body = -round(1.0 - f64::from(share)) as f32;
    (offset.scale(body), offset.scale(share))
}

/// An event is solved when its time less 0.1 is not above the smaller progress of the body and the active partner.
pub fn event_due(time: f32, progress: f32, partner_progress: Option<f32>) -> bool {
    let limit = match partner_progress {
        Some(p) if progress > p => p,
        _ => progress,
    };
    let v = round24(f64::from(time) - f64::from(0.1f32)) as f32;
    !(v > limit)
}

/// The solver passes' gate on a body: its inverse mass is above zero.
pub fn movable(inverse_mass: f64) -> bool {
    inverse_mass > 0.0
}

/// The four reals of a contact's material that the impulse and the contact rows read: friction and restitution, each
/// blended from its slow value to its fast one by the approach speed.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactMaterial {
    pub friction_fast: f32,
    pub friction_slow: f32,
    pub restitution_fast: f32,
    pub restitution_slow: f32,
}

/// What the contact rows read of an event besides its material: the two contact points, the plane, the id pairs of
/// each side and the flags.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowEvent {
    pub point_a: Vec4,
    pub point_b: Vec4,
    pub plane: Vec4,
    pub ids_a: [u32; 2],
    pub ids_b: [u32; 2],
    pub flags: u32,
}

/// One contact row a body appends (up to 8 per body).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactRow {
    /// 1 when the side's suppress flag is set.
    pub suppressed: u32,
    /// The material tag picked by [`pick_material_tag`].
    pub tag: u32,
    /// The side's ids: the second of `ids_a` then the first for the partner side, `ids_b` for the body side.
    pub ids: [u32; 2],
    /// The restitution (slow, fast) of a suppressed side, zero otherwise.
    pub restitution: [f32; 2],
    /// The friction (fast, slow).
    pub friction: [f32; 2],
    /// The side's own contact point.
    pub point: Vec4,
    /// The plane, negated for the partner side.
    pub plane: Vec4,
    /// The other point composed with the body's frame, supplied by the host.
    pub composed: Vec4,
    /// The other contact point.
    pub other: Vec4,
}

/// The row for `side` (1: the body, which sits at `point_a`; 0: the partner, at `point_b` with the plane negated).
/// Returns the row without its `composed` vector and the point that is composed (the other one).
pub fn contact_row(event: RowEvent, material: ContactMaterial, tag: u32, side: u32) -> ContactRow {
    let (ids, point, plane, other, suppress) = if side == 0 {
        let [a, b] = event.ids_a;
        ([b, a], event.point_b, event.plane.scale(-1.0), event.point_a, SUPPRESSES_PARTNER_SIDE)
    } else {
        (event.ids_b, event.point_a, event.plane, event.point_b, SUPPRESSES_BODY_SIDE)
    };
    let suppressed = event.flags & suppress != 0;
    ContactRow {
        suppressed: u32::from(suppressed),
        tag,
        ids,
        restitution: if suppressed { [material.restitution_slow, material.restitution_fast] } else { [0.0; 2] },
        friction: [material.friction_fast, material.friction_slow],
        point,
        plane,
        composed: Vec4::default(),
        other,
    }
}

/// The direction of the offsets' move: the offsets summed in order, normalised on the x87 (at the thread's precision
/// control) when the squared length is above 1e-5 (a double constant). `None` below: the body does not move.
fn offsets_direction(offsets: &[Vec4]) -> Option<Vec4> {
    let sum =
        offsets.iter().fold(Vec4::default(), |acc, o| Vec4 { x: o.x + acc.x, y: o.y + acc.y, z: o.z + acc.z, w: 0.0 });
    let round = round_x87;
    let square = |v: f32| round(f64::from(v) * f64::from(v));
    let length2 = round(round(square(sum.z) + square(sum.y)) + square(sum.x));
    if !(length2 > 1e-5) {
        return None;
    }
    let inverse = round(1.0 / round(length2.sqrt())) as f32;
    Some(sum.scale(inverse))
}

/// The largest distance a body moves from its collected offsets.
const MAX_OFFSET_MOVE: f32 = 0.03;

/// A body's move from the offsets collected in its row: along their normalised sum, as far as the largest projection
/// of one of them (from 0, at most [`MAX_OFFSET_MOVE`]). The original unrolls the projections by four; the first of
/// each four (and each of the rest) sums `z, x, y`, the other three `z, y, x`.
pub fn offsets_move(offsets: &[Vec4]) -> Option<Vec4> {
    let d = offsets_direction(offsets)?;
    let unrolled = if offsets.len() >= 4 { offsets.len() & !3 } else { 0 };
    let mut best = 0.0f32;
    for (i, o) in offsets.iter().enumerate() {
        let v = if i < unrolled && i % 4 != 0 {
            o.z * d.z + o.y * d.y + o.x * d.x
        } else {
            o.z * d.z + o.x * d.x + o.y * d.y
        };
        if v > best {
            best = v;
        }
    }
    if best > MAX_OFFSET_MOVE {
        best = MAX_OFFSET_MOVE;
    }
    Some(d.scale(best))
}

/// The approach speed: the relative velocity of the contact on the event's normal, summed `z, y, x`.
pub fn approach_speed(velocity: Vec4, normal: Vec4) -> f32 {
    velocity.z * normal.z + velocity.y * normal.y + velocity.x * normal.x
}

/// The impulse gate: the event is solved when it is not suppressed, both sides are movable ([`MOVES_BODY`] and
/// [`MOVES_PARTNER`]) or it approaches faster than 1, and it approaches at all.
pub fn impulse_due(speed: f32, flags: u32, suppressed: bool) -> bool {
    let both = flags & MOVES_BODY != 0 && flags & MOVES_PARTNER != 0;
    !suppressed && (both || -1.0 > speed) && 0.0 > speed
}

/// The slip of an approaching contact: the relative velocity less its normal part (`velocity - n * speed`, all four
/// lanes), its length, and its direction when the length is above 0.001.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slip {
    pub velocity: Vec4,
    pub speed: f32,
    pub direction: Option<Vec4>,
}

/// The slip speed below which a contact takes no friction.
const SLIP_EPSILON: f32 = 0.001;

/// The slip of the relative velocity `velocity` off `normal`, at approach speed `speed`.
pub fn slip(velocity: Vec4, normal: Vec4, speed: f32) -> Slip {
    let velocity = velocity.add_scaled(normal, 0.0 - speed);
    let length = velocity.length3();
    Slip { velocity, speed: length, direction: (length > SLIP_EPSILON).then(|| velocity.scale(1.0 / length)) }
}

/// The blend of the material from slow to fast: the approach speed times -0.05, clamped to 0..1 (an unordered value
/// stays).
pub fn speed_blend(speed: f32) -> f32 {
    let k = speed * -0.05;
    if 0.0 > k {
        0.0
    } else if k > 1.0 {
        1.0
    } else {
        k
    }
}

/// The friction coefficient at blend `k` (SSE).
pub fn friction(material: ContactMaterial, k: f32) -> f32 {
    (material.friction_fast - material.friction_slow) * k + material.friction_slow
}

/// The normal impulse on the x87 (each operation at the precision control): `(1 + e) * (-1 / response) * speed`, `e`
/// the restitution at blend `k` and `response` the bodies' response along the normal.
pub fn normal_impulse(material: ContactMaterial, k: f32, response: f32, speed: f32) -> f32 {
    let round = round_x87;
    let slow = f64::from(material.restitution_slow);
    let rise = round(round(f64::from(material.restitution_fast) - slow) * f64::from(k));
    let scale = round(round(rise + slow) + 1.0);
    let r = round(-1.0 / f64::from(response));
    round(round(scale * r) * f64::from(speed)) as f32
}

/// The friction on the impulse vector: the impulse that stops the slip (`slip_speed + across * impulse` over `along`,
/// the bodies' response along the slip direction), capped by the coefficient times the normal impulse; it opposes the
/// slip unless the stopping impulse's sign differs from the slip speed's. Returns `impulse - direction * friction`.
pub fn friction_impulse(
    impulse: Vec4,
    direction: Vec4,
    normal_impulse: f32,
    slip_speed: f32,
    coefficient: f32,
    along: f32,
    across: f32,
) -> Vec4 {
    let stop = across * normal_impulse + slip_speed;
    let q = stop / along;
    let limit = if q > 0.0 { q } else { 0.0 - q };
    let magnitude = if normal_impulse > 0.0 { normal_impulse } else { 0.0 - normal_impulse };
    let mut f = magnitude * coefficient;
    if f > limit {
        f = limit;
    }
    if sign(stop) != sign(slip_speed) {
        f = 0.0 - f;
    }
    impulse.add_scaled(direction, 0.0 - f)
}

/// The velocity of the event's body at the contact less the partner's, `subss` per lane.
pub fn relative_velocity(body: Vec3, partner: Vec3) -> Vec3 {
    Vec3 { x: sse::sub(body.x, partner.x), y: sse::sub(body.y, partner.y), z: sse::sub(body.z, partner.z) }
}
