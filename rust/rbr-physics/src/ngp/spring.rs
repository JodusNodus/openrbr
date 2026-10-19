//! NGP's wrapper `0x10059f70` of `CSpringDamper::Step` (`0x52e090`, the call at `0x53e547` in `E51Wheel::M3DD30`),
//! thiscall on the strut with 1.02's six arguments (the second the travel `x`, the third the speed `v`). Near the top
//! of the travel it softens the spring rate (the def's `+0x98`) and damping (`+0xd8`) for the call ([`bump_stop`]);
//! it scales the rebound value `+0x118` ([`rebound_scale`]) and may latch the strut's limits ([`latch`]), copies the
//! limits to the car's `+0xe80..0xe8c`, runs 1.02's `Step`, restores the def's words and runs 1.02's
//! `UpdateForces` (`0x52dd70`). All SSE. Names ours.

/// [`bump_stop`]'s result: the ratio (the strut's `+0x30`) and the def's rate and damping for the call; `damping_out`
/// is what the strut's `+0x38` gets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BumpStop {
    pub ratio: f32,
    pub rate: f32,
    pub damping: f32,
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

/// The stop engages when the travel `x` is below the def's top (`+0x28`) by at most the gap `0.05 - g` (`g` the car
/// def's `+0x41c`, 0 outside `[0, 0.05]`). The ratio is the depth over the gap when `progressive` (NGP's setting bit
/// `0x20000` and the game's flag), else 1; the rate scales by it, the damping by its root; then a negative speed
/// divides the damping by 0.3, else a positive `soften` (the car def's `+0x1fc`) multiplies it (and the strut's copy).
pub fn bump_stop(
    progressive: bool,
    top: f32,
    x: f32,
    v: f32,
    g: f32,
    soften: f32,
    rate: f32,
    damping: f32,
) -> Option<BumpStop> {
    if !(top > x) {
        return None;
    }
    let gap = if (0.0..=0.05).contains(&g) { 0.05 - g } else { 0.0 };
    let depth = top - x;
    let gap = max_ss(0.0, gap);
    if !(depth > 0.0 && gap >= depth) {
        return None;
    }
    let ratio = if progressive { depth / gap } else { 1.0 };
    let rate = rate * ratio;
    let damping = (f64::from(ratio).sqrt() as f32) * damping;
    let (damping, damping_out) = if 0.0 > v {
        (damping / 0.3, damping)
    } else if soften > 0.0 {
        (soften * damping, soften * damping)
    } else {
        (damping, damping)
    };
    Some(BumpStop { ratio, rate, damping, damping_out })
}

/// The rebound value for the call: `(1 - 0.4 w) * r` when `r` is above 0 (`w` the strut's `*(+0x10)+4`).
pub fn rebound_scale(r: f32, w: f32) -> f32 {
    if r > 0.0 {
        (1.0 - w * 0.4) * r
    } else {
        r
    }
}

/// What [`latch`] sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Latch {
    /// The strut object's `+0xc` takes its `+0x14` and the def's `+0xe8` is cleared for the call.
    Compression(f32),
    /// The strut object's `+0x10` takes the scaled rebound value and the def's `+0xf8` is cleared for the call.
    Rebound(f32),
    None,
}

/// With the def's `+0xe8` (`c`) and `+0xf8` (`r`): compression when `c > 0`, `-c >= v` and the object's `+0x14` is
/// above 0; else rebound when `r > 0`, `v >= r` and the scaled rebound value is above 0.
pub fn latch(c: f32, r: f32, v: f32, limit: f32, rebound: f32) -> Latch {
    if c > 0.0 && -c >= v && limit > 0.0 {
        Latch::Compression(limit)
    } else if r > 0.0 && v >= r && rebound > 0.0 {
        Latch::Rebound(rebound)
    } else {
        Latch::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_stop_engages_within_the_gap() {
        assert_eq!(bump_stop(true, 0.1, 0.2, 0.0, 0.0, 0.0, 1.0, 1.0), None);
        assert_eq!(bump_stop(true, 0.1, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0), None);
        let b = bump_stop(false, 0.1, 0.08, 1.0, 0.0, 0.0, 2.0, 3.0).unwrap();
        assert_eq!((b.ratio, b.rate, b.damping), (1.0, 2.0, 3.0));
    }

    #[test]
    fn latch_prefers_compression() {
        assert_eq!(latch(1.0, 1.0, -2.0, 5.0, 3.0), Latch::Compression(5.0));
        assert_eq!(latch(1.0, 1.0, 2.0, 5.0, 3.0), Latch::Rebound(3.0));
        assert_eq!(latch(0.0, 0.0, 2.0, 5.0, 3.0), Latch::None);
    }
}
