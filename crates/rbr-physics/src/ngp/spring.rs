//! NGP's changes to the suspension strut's step. Near the top of the travel the bump rubber's stiffness and damping
//! are softened for the step ([`bump_stop`]); the high-speed rebound damping scales with the strut's fade
//! ([`rebound_scale`]); and the high-speed damping may take over the effective damping ([`latch`]). All SSE.
//! Names are by use: the `.lsp` keys are `InnerBumpRubberPos`, `BumpRubberStiffness`, `DampingBumpRubber`,
//! `HighSpeedBreakBump`, `HighSpeedBreakRebound` and `HighSpeedDampingRebound`.

/// The bump rubber of a strut's definition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BumpRubber {
    /// `InnerBumpRubberPos`: the travel the rubber engages at.
    pub pos: f32,
    /// `BumpRubberStiffness`.
    pub stiffness: f32,
    /// `DampingBumpRubber`.
    pub damping: f32,
}

/// What [`bump_stop`] gives for an engaged stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BumpStop {
    /// How far into the engagement gap the travel is (1 when not progressive).
    pub ratio: f32,
    /// The rubber's stiffness and damping for the step.
    pub stiffness: f32,
    pub damping: f32,
    /// The damping the strut's state records: `damping` without the compression divisor.
    pub damping_out: f32,
}

/// `maxss a, b`: `b` when they are unordered.
fn max_ss(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// The stop engages when the travel `x` is below the rubber's position by at most the gap `0.05 - g` (`g` the wheel's
/// `gap_offset`, taken as 0 outside `[0, 0.05]`). The ratio is the depth over the gap when `progressive`, else 1; the
/// stiffness scales by it, the damping by its root; then a negative speed divides the damping by 0.3, else a positive
/// `rebound_ratio` multiplies it (and the recorded damping).
pub fn bump_stop(
    progressive: bool,
    rubber: BumpRubber,
    x: f32,
    v: f32,
    gap_offset: f32,
    rebound_ratio: f32,
) -> Option<BumpStop> {
    if !(rubber.pos > x) {
        return None;
    }
    let gap = if (0.0..=0.05).contains(&gap_offset) { 0.05 - gap_offset } else { 0.0 };
    let depth = rubber.pos - x;
    let gap = max_ss(0.0, gap);
    if !(depth > 0.0 && gap >= depth) {
        return None;
    }
    let ratio = if progressive { depth / gap } else { 1.0 };
    let stiffness = rubber.stiffness * ratio;
    let damping = (f64::from(ratio).sqrt() as f32) * rubber.damping;
    let (damping, damping_out) = if 0.0 > v {
        (damping / 0.3, damping)
    } else if rebound_ratio > 0.0 {
        (rebound_ratio * damping, rebound_ratio * damping)
    } else {
        (damping, damping)
    };
    Some(BumpStop { ratio, stiffness, damping, damping_out })
}

/// The high-speed rebound damping `damping` for the step: `(1 - 0.4 fade) * damping` when it is above 0.
pub fn rebound_scale(damping: f32, fade: f32) -> f32 {
    if damping > 0.0 {
        (1.0 - fade * 0.4) * damping
    } else {
        damping
    }
}

/// Which high-speed damping takes over, and its value; [`latch`] sets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Latch {
    /// The effective bump damping takes the high-speed bump damping, and `HighSpeedBreakBump` is cleared for the step.
    Bump(f32),
    /// The effective rebound damping takes the scaled high-speed rebound damping, and `HighSpeedBreakRebound` is
    /// cleared for the step.
    Rebound(f32),
    None,
}

/// With the break speeds `break_bump` and `break_rebound` and the speed `v`: the bump damping when `break_bump > 0`,
/// `-break_bump >= v` and the high-speed bump damping is above 0; else the rebound damping when `break_rebound > 0`,
/// `v >= break_rebound` and the scaled high-speed rebound damping is above 0.
pub fn latch(break_bump: f32, break_rebound: f32, v: f32, bump_damping: f32, rebound_damping: f32) -> Latch {
    if break_bump > 0.0 && -break_bump >= v && bump_damping > 0.0 {
        Latch::Bump(bump_damping)
    } else if break_rebound > 0.0 && v >= break_rebound && rebound_damping > 0.0 {
        Latch::Rebound(rebound_damping)
    } else {
        Latch::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_stop_engages_within_the_gap() {
        let rubber = |pos, stiffness, damping| BumpRubber { pos, stiffness, damping };
        assert_eq!(bump_stop(true, rubber(0.1, 1.0, 1.0), 0.2, 0.0, 0.0, 0.0), None);
        assert_eq!(bump_stop(true, rubber(0.1, 1.0, 1.0), 0.0, 0.0, 0.0, 0.0), None);
        let b = bump_stop(false, rubber(0.1, 2.0, 3.0), 0.08, 1.0, 0.0, 0.0).unwrap();
        assert_eq!((b.ratio, b.stiffness, b.damping), (1.0, 2.0, 3.0));
    }

    #[test]
    fn latch_prefers_bump() {
        assert_eq!(latch(1.0, 1.0, -2.0, 5.0, 3.0), Latch::Bump(5.0));
        assert_eq!(latch(1.0, 1.0, 2.0, 5.0, 3.0), Latch::Rebound(3.0));
        assert_eq!(latch(0.0, 0.0, 2.0, 5.0, 3.0), Latch::None);
    }
}
