//! The hinge's end stops. An angle above `HingeMaxAngle` (or below `HingeMinAngle`) is set back to the bound, and a rate
//! heading into the stop is reflected: it becomes `0 - HingeRestitution * rate` (a positive rate at the upper stop, a
//! negative one at the lower). The body keeps the bound's tweak record as its angle block, so taking the record is what
//! sets the angle. The caller marks the rate's tweak object dirty before the rate changes.
//!
//! The bounds are compared as floats; their records are carried as words, because the body copies the whole record.

/// The call that marks the rate's tweak object dirty.
pub trait Calls {
    fn mark_dirty(&mut self);
}

/// What the stops read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// The hinge angle, rad.
    pub angle: f32,
    /// The hinge rate, rad/s.
    pub rate: f32,
    /// `HingeRestitution`.
    pub restitution: f32,
    /// The `HingeMaxAngle` and `HingeMinAngle` records, three words each; the last word of each is the bound (a float).
    pub max_angle: [u32; 3],
    pub min_angle: [u32; 3],
}

/// The record taken, the word the original returns, and the new rate when it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Update {
    /// The bound's record, which becomes the body's angle block.
    pub record: [u32; 3],
    /// The bound's word, as the original returns it.
    pub result: u32,
    pub rate: Option<f32>,
}

/// `None`: the angle is inside both bounds and nothing changes.
pub fn apply_stops(l: &Limits) -> Option<Update> {
    let (record, reflects) = if l.angle > f32::from_bits(l.max_angle[2]) {
        (l.max_angle, l.rate > 0.0)
    } else if f32::from_bits(l.min_angle[2]) > l.angle {
        (l.min_angle, l.rate < 0.0)
    } else {
        return None;
    };
    Some(Update { record, result: record[2], rate: reflects.then_some(0.0 - l.restitution * l.rate) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(angle: f32, rate: f32) -> Limits {
        Limits {
            angle,
            rate,
            restitution: 2.0,
            max_angle: [0x100, 0x101, 1.0f32.to_bits()],
            min_angle: [0x200, 0x201, (-1.0f32).to_bits()],
        }
    }

    #[test]
    fn above_the_maximum_takes_its_record_and_reflects_a_positive_rate() {
        let u = apply_stops(&limits(3.0, 0.5)).unwrap();
        assert_eq!(u.record, limits(3.0, 0.5).max_angle);
        assert_eq!(u.result, 1.0f32.to_bits());
        assert_eq!(u.rate, Some(-1.0));
        assert_eq!(apply_stops(&limits(3.0, -0.5)).unwrap().rate, None);
    }

    #[test]
    fn below_the_minimum_takes_its_record_and_reflects_a_negative_rate() {
        let u = apply_stops(&limits(-3.0, -0.5)).unwrap();
        assert_eq!(u.record[0], 0x200);
        assert_eq!(u.rate, Some(1.0));
        assert_eq!(apply_stops(&limits(-3.0, 0.5)).unwrap().rate, None);
    }

    #[test]
    fn inside_both_bounds_changes_nothing() {
        assert_eq!(apply_stops(&limits(0.0, 0.5)), None);
    }
}
