//! The leaf functors of the damage dispatch: the functions the impact path calls through the handler's functor
//! table (`damage_ops::FUNCTORS`). Their callers are not ported yet, so these are the pure parts: each takes the words it
//! reads as typed values and returns `None` when its gate fails, or the word it writes and the ring event it pushes (a
//! functor that pushes no event returns a `bool`; an unconditional no-op returns its value). The layout (`layout::dmg_*`)
//! reads the words, calls these, and writes
//! the result. The original decides the order and the rounding: the gates compare ordered (a NaN fails `a > b`),
//! and the clamp keeps a NaN.

use super::wall_friction::clamp;

/// A ring event: the entry's first word (`kind`) and its second (`value`). The ring (ten entries) is written by the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: u32,
    pub value: u32,
}

/// A functor that passes its gate: the word it writes (`()` when the layout only clears or sets words) and its event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass<T> {
    pub write: T,
    pub event: Event,
}

fn pass<T>(write: T, kind: u32, value: u32) -> Option<Pass<T>> {
    Some(Pass { write, event: Event { kind, value } })
}

/// The speed the timed functors compare: `a / threshold` when the threshold is above zero, else 0.
fn speed(a: f32, threshold: f32) -> f32 {
    if threshold > 0.0 {
        a / threshold
    } else {
        0.0
    }
}

/// Gearbox wear: at level 2 or more, the wear word (below 1.0) grows by `(a - b) * c`,
/// clamped to [0, 0.9] at level 3 and [0, 0.6] below it.
pub fn wear(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || !(word < 1.0) {
        return None;
    }
    let cap = if level >= 3 { 0.9 } else { 0.6 };
    pass(clamp((a - b) * c + word, 0.0, cap), 0, tag)
}

/// Cooling wear: the cooling-wear word grows by `(a - b) * c`, capped at 1.0.
#[allow(clippy::double_comparisons)] // Keep the original ordered `<= 1.0` then `!= 1.0` gates explicit.
pub fn cooling_wear(a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if !(a > b) || !(word <= 1.0 && word != 1.0) {
        return None;
    }
    pass(clamp((a - b) * c + word, 0.0, 1.0), 3, tag)
}

/// Intercooler wear: at signed level 2 or more, the intercooler-wear word grows by
/// `(a - b) * c`, capped at 1.0.
#[allow(clippy::double_comparisons)] // Keep the original ordered `<= 1.0` then `!= 1.0` gates explicit.
pub fn intercooler_wear(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || !(word <= 1.0 && word != 1.0) {
        return None;
    }
    pass(clamp((a - b) * c + word, 0.0, 1.0), 0x19, tag)
}

/// Zone growth: the wheel's zone float `zones[i]` (i below 4, below 1.0) grows by `(a - b) * c`,
/// clamped to [0, 1].
pub fn zone(zones: &[f32; 4], i: u32, a: f32, b: f32, c: f32) -> Option<Pass<(usize, f32)>> {
    let k = usize::try_from(i).ok().filter(|&k| k < zones.len())?;
    if !(a > b) || !(zones[k] < 1.0) {
        return None;
    }
    pass((k, clamp((a - b) * c + zones[k], 0.0, 1.0)), 10, i)
}

/// Triple flag: with `a > b` and the triple's first word set, the
/// layout clears it and sets the damage-event flag.
pub fn triple(a: f32, b: f32, word: u32, i: u32) -> Option<Pass<()>> {
    if !(a > b) || word == 0 {
        return None;
    }
    pass((), 0x11, i)
}

/// Windshield: with `a > b` and the windshield word at 1, the layout
/// clears it and returns 4. No ring event is pushed, so this is a plain gate.
pub fn windshield(a: f32, b: f32, word: u32) -> bool {
    a > b && word == 1
}

/// Head gasket: at level 2 or more (a signed compare), with `a > b` and
/// the word at 1, the layout clears it and returns 3. No ring event, so a plain gate.
pub fn head_gasket(level: i32, a: f32, b: f32, word: u32) -> bool {
    level >= 2 && a > b && word == 1
}

/// Mounted body state: the mounted body's state word. The speed
/// (`a / threshold`, see `speed`) must pass `b`. From 0 the state becomes 2 above `2b` and 1 otherwise; from 1 it becomes
/// 2. Any other state fails.
pub fn state(threshold: f32, a: f32, b: f32, word: u32, i: u32) -> Option<Pass<u32>> {
    let v = speed(a, threshold);
    if !(v > b) {
        return None;
    }
    let next = match word {
        0 if v > 2.0 * b => 2,
        0 => 1,
        1 => 2,
        _ => return None,
    };
    pass(next, 0x1B, i)
}

/// No-op: ignores its arguments and always returns zero.
pub fn zero() -> u32 {
    0
}

/// Zero: ignores its arguments and returns zero.
pub fn zero2f0() -> u32 {
    0
}

/// Door ajar: with `a > b`, the triple's first word clear and its second
/// set, the layout clears the second.
pub fn door(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    if !(a > b) || first != 0 || second == 0 {
        return None;
    }
    pass((), 7, i)
}

/// Door speed: with the speed (see `speed`) above `b`, the triple's first word
/// set, and index 2 only at level 2 or more, the layout clears that word.
pub fn door_speed(threshold: f32, a: f32, b: f32, first: u32, level: i32, i: u32) -> Option<Pass<()>> {
    if !(speed(a, threshold) > b) || first == 0 || (i == 2 && level < 2) {
        return None;
    }
    pass((), 8, i)
}

/// Drain: at level 2 or more, with `a > b` and the word not 0.0 (a NaN passes), the
/// word falls by `(a - b) * c`, clamped to [0, 1].
pub fn drain(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || word == 0.0 {
        return None;
    }
    pass(clamp(word - (a - b) * c, 0.0, 1.0), 0xE, tag)
}

/// Clear flag: with `a > b` and the flag set, the layout clears it.
pub fn clear_flag(a: f32, b: f32, flag: u32, kind: u32, tag: u32) -> Option<Pass<()>> {
    if !(a > b) || flag == 0 {
        return None;
    }
    pass((), kind, tag)
}

/// Water pump: the water-pump flag clears when `a > b` and it is set.
pub fn water_pump(a: f32, b: f32, flag: u32, tag: u32) -> Option<Pass<()>> {
    clear_flag(a, b, flag, 0x1a, tag)
}

/// Level-2 flag and the flag after it: `clear_flag` at level 2 or more.
pub fn clear_flag_level2(level: i32, a: f32, b: f32, flag: u32, kind: u32, tag: u32) -> Option<Pass<()>> {
    if level < 2 {
        return None;
    }
    clear_flag(a, b, flag, kind, tag)
}

/// Cap at 0.3: at level 2 or more with `a > b`, the word above 0.3 is set to 0.3.
pub fn cap_03(level: i32, a: f32, b: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || !(word > 0.3) {
        return None;
    }
    pass(0.3, 0x10, tag)
}

/// Pair word: with `i` below 3 and `a > b`, the pair's word set,
/// the layout clears it.
pub fn clear_pair_word(a: f32, b: f32, words: &[u32; 3], i: u32) -> Option<Pass<()>> {
    let k = usize::try_from(i).ok().filter(|&k| k < words.len())?;
    if !(a > b) || words[k] == 0 {
        return None;
    }
    pass((), 4, i)
}

/// Pair: with `a > b` and either word of the pair set, the layout clears both and sets the flag. It returns 1 on a pass.
pub fn clear_pair(a: f32, b: f32, first: u32, second: u32, kind: u32, i: u32) -> Option<Pass<()>> {
    if !(a > b) || (first == 0 && second == 0) {
        return None;
    }
    pass((), kind, i)
}

/// Suspension arm: the suspension-arm pair clears and sets the suspension-failed flag.
pub fn suspension_arm(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x15, i)
}

/// Steering rod: the steering-rod pair clears and sets the steering-failed flag.
pub fn steering_rod(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x15, i)
}

/// Hub: the hub pair clears and sets the hub-failed flag.
pub fn hub(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x16, i)
}

/// Brake disc: the brake-disc pair clears and sets the brake-failed flag.
pub fn brake_disc(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x18, i)
}

/// Drop to floor: at level 2 or more with `a > b` and the word not 0.0 (a NaN passes),
/// the word falls by `(a - b) * c`, clamped to [lower, 1]; `lower` is 0.4 below level 3 and 0 from level 3. The layout
/// sets the damage-event flag and returns 5 on a pass.
pub fn drop_to_floor(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || word == 0.0 {
        return None;
    }
    let lower = if level >= 3 { 0.0 } else { 0.4 };
    pass(clamp(word - (a - b) * c, lower, 1.0), 1, tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_nan_and_the_bounds() {
        assert_eq!(clamp(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp(2.0, 0.0, 1.0), 1.0);
        assert_eq!(clamp(0.5, 0.0, 1.0), 0.5);
        assert!(clamp(f32::NAN, 0.0, 1.0).is_nan());
    }

    #[test]
    fn speed_is_zero_without_a_positive_threshold() {
        assert_eq!(speed(3.0, 0.0), 0.0);
        assert_eq!(speed(3.0, f32::NAN), 0.0);
        assert_eq!(speed(3.0, 2.0), 1.5);
    }
}
