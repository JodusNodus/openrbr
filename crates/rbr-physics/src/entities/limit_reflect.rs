//! The limit update `CEntity` 0x51f280 (exe 0x51f280-0x51f342). The tweak block gives two windows of three words, the
//! last word of each being its bound. A value above the first bound takes window one, else a value below the second
//! bound takes window two, else nothing changes. For a positive residual (window one) or a negative one (window two) the
//! residual becomes `0 - factor * residual`, after the call on the limit object (0x52f450, which sets its `+8` to 1).
//! The original's eax is the last word it loaded: the bound of the window taken, else the tweak pointer.

/// The call on the limit object (`0x52f450`).
pub trait Calls {
    fn flag(&mut self);
}

/// What the update reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub value: f32,
    pub residual: f32,
    /// The residual's factor (the tweak block's).
    pub factor: f32,
    pub upper: [u32; 3],
    pub lower: [u32; 3],
}

/// The window taken, the eax the original leaves, and the new residual when it moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Update {
    pub window: [u32; 3],
    pub eax: u32,
    pub residual: Option<f32>,
}

/// `None`: the value is inside both bounds and nothing changes.
pub fn limit_update(l: &Limits) -> Option<Update> {
    let (window, moves) = if l.value > f32::from_bits(l.upper[2]) {
        (l.upper, l.residual > 0.0)
    } else if f32::from_bits(l.lower[2]) > l.value {
        (l.lower, l.residual < 0.0)
    } else {
        return None;
    };
    Some(Update { window, eax: window[2], residual: moves.then_some(0.0 - l.factor * l.residual) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(value: f32, residual: f32) -> Limits {
        Limits {
            value,
            residual,
            factor: 2.0,
            upper: [0x100, 0x101, 1.0f32.to_bits()],
            lower: [0x200, 0x201, (-1.0f32).to_bits()],
        }
    }

    #[test]
    fn above_the_upper_bound_takes_window_one_for_a_positive_residual() {
        let u = limit_update(&limits(3.0, 0.5)).unwrap();
        assert_eq!(u.window, limits(3.0, 0.5).upper);
        assert_eq!(u.eax, 1.0f32.to_bits());
        assert_eq!(u.residual, Some(-1.0));
        assert_eq!(limit_update(&limits(3.0, -0.5)).unwrap().residual, None);
    }

    #[test]
    fn below_the_lower_bound_takes_window_two_for_a_negative_residual() {
        let u = limit_update(&limits(-3.0, -0.5)).unwrap();
        assert_eq!(u.window[0], 0x200);
        assert_eq!(u.residual, Some(1.0));
        assert_eq!(limit_update(&limits(-3.0, 0.5)).unwrap().residual, None);
    }

    #[test]
    fn inside_both_bounds_changes_nothing() {
        assert_eq!(limit_update(&limits(0.0, 0.5)), None);
    }
}
