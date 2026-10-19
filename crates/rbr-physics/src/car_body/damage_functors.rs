//! The leaf functors of the damage dispatch: the functions `ApplyFunctors` (0x4ec560) calls through the handler's functor
//! table (`damage_ops::FUNCTORS`). Their callers are not ported yet, so these are the pure parts: each takes the words it
//! reads as typed values and returns `None` when its gate fails, or the word it writes and the ring event it pushes (a
//! functor that pushes no event returns a `bool`; an unconditional no-op returns its `eax`). The layout (`layout::dmg_*`)
//! reads the words, calls these, and writes
//! the result. The asm decides the order and the rounding: the gates compare with `comiss`/`jbe` (a NaN fails `a > b`),
//! and the clamp keeps a NaN.

use super::aero::clamp;

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

/// The speed the timed functors compare: `a / threshold` when the threshold is above zero, else 0 (the asm clears the
/// register first).
fn speed(a: f32, threshold: f32) -> f32 {
    if threshold > 0.0 {
        a / threshold
    } else {
        0.0
    }
}

/// `CCarDamageHandler::Dmg660` (0x4ec660): at level 2 or more, the wear word (+0x3b00, below 1.0) grows by `(a - b) * c`,
/// clamped to [0, 0.9] at level 3 and [0, 0.6] below it.
pub fn wear(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || !(word < 1.0) {
        return None;
    }
    let cap = if level >= 3 { 0.9 } else { 0.6 };
    pass(clamp((a - b) * c + word, 0.0, cap), 0, tag)
}

/// `CCarDamageHandler::Dmg720` (0x4ec720): the cooling-wear word (+0x3b10) grows by `(a - b) * c`, capped at 1.0.
#[allow(clippy::double_comparisons)] // Keep the original ordered `<= 1.0` then `!= 1.0` gates explicit.
pub fn cooling_wear(a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if !(a > b) || !(word <= 1.0 && word != 1.0) {
        return None;
    }
    pass(clamp((a - b) * c + word, 0.0, 1.0), 3, tag)
}

/// `CCarDamageHandler::DmgD160` (0x4ed160): at signed level 2 or more, the intercooler-wear word (+0x3b0c) grows by
/// `(a - b) * c`, capped at 1.0.
#[allow(clippy::double_comparisons)] // Keep the original ordered `<= 1.0` then `!= 1.0` gates explicit.
pub fn intercooler_wear(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || !(word <= 1.0 && word != 1.0) {
        return None;
    }
    pass(clamp((a - b) * c + word, 0.0, 1.0), 0x19, tag)
}

/// `CCarDamageHandler::DmgA00` (0x4eca00): the wheel's zone float `zones[i]` (i below 4, below 1.0) grows by `(a - b) * c`,
/// clamped to [0, 1].
pub fn zone(zones: &[f32; 4], i: u32, a: f32, b: f32, c: f32) -> Option<Pass<(usize, f32)>> {
    let k = usize::try_from(i).ok().filter(|&k| k < zones.len())?;
    if !(a > b) || !(zones[k] < 1.0) {
        return None;
    }
    pass((k, clamp((a - b) * c + zones[k], 0.0, 1.0)), 10, i)
}

/// `CCarDamageHandler::DmgCF20` (0x4ecf20): with `a > b` and the triple's first word set (`word`, at +0x3a30 + 12i), the
/// layout clears it and sets the flag at +0x3a88.
pub fn triple(a: f32, b: f32, word: u32, i: u32) -> Option<Pass<()>> {
    if !(a > b) || word == 0 {
        return None;
    }
    pass((), 0x11, i)
}

/// `CCarDamageHandler::DmgCFE0` (0x4ecfe0): with `a > b` and the windshield word (`word`, at +0x3dc4 + 4i) at 1, the layout
/// clears it and returns 4. The asm pushes no ring event, so this is a plain gate.
pub fn windshield(a: f32, b: f32, word: u32) -> bool {
    a > b && word == 1
}

/// `CCarDamageHandler::DmgCFA0` (0x4ecfa0, the head gasket): at level 2 or more (a signed compare, `jl`), with `a > b` and
/// the word (`word`, at +0x3dd8 + 4i) at 1, the layout clears it and returns 3. No ring event, so a plain gate.
pub fn head_gasket(level: i32, a: f32, b: f32, word: u32) -> bool {
    level >= 2 && a > b && word == 1
}

/// `CCarDamageHandler::DmgD200` (0x4ed200): the mounted body's state word (`word`, at +0x3db4 + 4i; 0 to 2). The speed
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

/// `CCarDamageHandler::DmgD300` (0x4ed300): ignores all four stack words and always returns zero.
pub fn zero() -> u32 {
    0
}

/// `NPhysics::E35_Zero2F0` (0x4ed2f0): ignores the handler and four stack words, and returns zero.
pub fn zero2f0() -> u32 {
    0
}

/// `CCarDamageHandler::DmgAA0` (0x4ecaa0): with `a > b`, the triple's first word (+0x3d84 + 12i) clear and its second
/// (+0x3d88 + 12i) set, the layout clears the second.
pub fn door(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    if !(a > b) || first != 0 || second == 0 {
        return None;
    }
    pass((), 7, i)
}

/// `CCarDamageHandler::DmgB20` (0x4ecb20): with the speed (see `speed`) above `b`, the triple's first word (+0x3d84 + 12i)
/// set, and index 2 only at level 2 or more, the layout clears that word.
pub fn door_speed(threshold: f32, a: f32, b: f32, first: u32, level: i32, i: u32) -> Option<Pass<()>> {
    if !(speed(a, threshold) > b) || first == 0 || (i == 2 && level < 2) {
        return None;
    }
    pass((), 8, i)
}

/// `CCarDamageHandler::Dmg890` (0x4ec890): at level 2 or more, with `a > b` and the word (+0x3b08) not 0.0 (a NaN passes), the
/// word falls by `(a - b) * c`, clamped to [0, 1].
pub fn drain(level: i32, a: f32, b: f32, c: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || word == 0.0 {
        return None;
    }
    pass(clamp(word - (a - b) * c, 0.0, 1.0), 0xE, tag)
}

/// `CCarDamageHandler::Dmg9A0` (0x4ec9a0): with `a > b` and the flag (+0x3a98) set, the layout clears it.
pub fn clear_flag(a: f32, b: f32, flag: u32, kind: u32, tag: u32) -> Option<Pass<()>> {
    if !(a > b) || flag == 0 {
        return None;
    }
    pass((), kind, tag)
}

/// `CCarDamageHandler::DmgD0F0` (0x4ed0f0): the water-pump flag (+0x3aa8) clears when `a > b` and it is set.
pub fn water_pump(a: f32, b: f32, flag: u32, tag: u32) -> Option<Pass<()>> {
    clear_flag(a, b, flag, 0x1a, tag)
}

/// `CCarDamageHandler::Dmg930` (0x4ec930) and `Dmg820` (0x4ec820): `clear_flag` at level 2 or more.
pub fn clear_flag_level2(level: i32, a: f32, b: f32, flag: u32, kind: u32, tag: u32) -> Option<Pass<()>> {
    if level < 2 {
        return None;
    }
    clear_flag(a, b, flag, kind, tag)
}

/// `CCarDamageHandler::DmgCEB0` (0x4eceb0): at level 2 or more with `a > b`, the word (+0x3a9c) above 0.3 is set to 0.3.
pub fn cap_03(level: i32, a: f32, b: f32, word: f32, tag: u32) -> Option<Pass<f32>> {
    if level < 2 || !(a > b) || !(word > 0.3) {
        return None;
    }
    pass(0.3, 0x10, tag)
}

/// `CCarDamageHandler::Dmg7B0` (0x4ec7b0): with `i` below 3 and `a > b`, the pair's word (`words[i]`, at +0x3a1c + 8i) set,
/// the layout clears it.
pub fn clear_pair_word(a: f32, b: f32, words: &[u32; 3], i: u32) -> Option<Pass<()>> {
    let k = usize::try_from(i).ok().filter(|&k| k < words.len())?;
    if !(a > b) || words[k] == 0 {
        return None;
    }
    pass((), 4, i)
}

/// `CCarDamageHandler::DmgBB0` (0x4ecbb0) and `DmgCC30` (0x4ecc30): with `a > b` and either word of the pair (+0x3b60 or
/// +0x3b40, plus 8i) set, the layout clears both and sets the flag. The asm returns 1 on a pass.
pub fn clear_pair(a: f32, b: f32, first: u32, second: u32, kind: u32, i: u32) -> Option<Pass<()>> {
    if !(a > b) || (first == 0 && second == 0) {
        return None;
    }
    pass((), kind, i)
}

/// `CCarDamageHandler::DmgCCB0` (0x4eccb0): the suspension-arm pair (+0x3ba0 + 8i) clears and sets +0x3b30.
pub fn suspension_arm(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x15, i)
}

/// `CCarDamageHandler::DmgCD30` (0x4ecd30): the steering-rod pair (+0x3b80 + 8i) clears and sets +0x3b3c.
pub fn steering_rod(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x15, i)
}

/// `CCarDamageHandler::DmgCDB0` (0x4ecdb0): the hub pair (+0x3bc0 + 8i) clears and sets +0x3b34.
pub fn hub(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x16, i)
}

/// `CCarDamageHandler::DmgCE30` (0x4ece30): the brake-disc pair (+0x3be0 + 8i) clears and sets +0x3b38.
pub fn brake_disc(a: f32, b: f32, first: u32, second: u32, i: u32) -> Option<Pass<()>> {
    clear_pair(a, b, first, second, 0x18, i)
}

/// `CCarDamageHandler::DmgD020` (0x4ed020): at level 2 or more with `a > b` and the word (+0x3aa0) not 0.0 (a NaN passes),
/// the word falls by `(a - b) * c`, clamped to [lower, 1]; `lower` is 0.4 below level 3 and 0 from level 3. The layout
/// sets the flag at +0x3a88 and returns 5 on a pass.
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
