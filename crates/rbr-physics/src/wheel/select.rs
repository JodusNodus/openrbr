//! `CWheel::Unk9660` (0x539660, `uint_select`) and `CWheel::Unk96e0` (0x5396e0, the table lerp pair that calls it). The
//! physics path runs the x87 at 24-bit precision (`notes/physics.md` "x87 and float rules"), so each operation is `fpu::round24` of the
//! exact result. `Unk96e0` calls `Unk9660` with `n = 4`.

use crate::fpu::round24;

/// `2*pi` as the asm's float constant (0x7427a8).
const TAU: f32 = 6.2831855;
/// The phase offset (0x740bd4), added to the phase.
const PHASE_OFFSET: f32 = 62.831856;
/// `1 / (2*pi)` as the asm's float constant (0x731b1c).
const INV_TAU: f32 = 0.15915494;

/// The pick: the cycle's position `low` and the next one `high` (both mod `n`), the fraction toward `high`, and the
/// wrap count of `high`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pick {
    pub low: u32,
    pub high: u32,
    pub frac: f32,
    pub wraps: u32,
}

/// An unsigned word as the x87 holds it: `FILD` reads it signed, which is exact, and a negative word gets `2^32` added,
/// which rounds to 24 bits.
fn unsigned(x: u32) -> f64 {
    let signed = x as i32;
    if signed < 0 {
        round24(f64::from(signed) + 4_294_967_296.0)
    } else {
        f64::from(signed)
    }
}

/// The pick for `phase` in a cycle of `n` steps (`n` nonzero: the original's `div` faults on zero). With
/// `base = phase + offset`, `r` is `n * base / 2pi` truncated to 32 bits (the low word of `_ftol2`, exact for
/// `|x| < 2^63`), `step = 2pi / n`, and `frac = 1 - (base - r * step) / step`.
pub fn pick(phase: f32, n: u32) -> Pick {
    let nf = unsigned(n);
    let base = round24(f64::from(phase) + f64::from(PHASE_OFFSET));
    let step = round24(f64::from(TAU) / nf);
    let r = round24(round24(nf * base) * f64::from(INV_TAU)) as i64 as u32;
    let swing = round24(base - round24(unsigned(r) * step));
    let frac = round24(1.0 - round24(swing / step));
    let next = r.wrapping_add(1);
    Pick { low: r % n, high: next % n, frac: frac as f32, wraps: next / n }
}

/// The host call `CE36SubBlock::Init` (0x4f3e80, thiscall) that `Unk96e0` makes on its local: it zeroes the block's
/// three words, which `Unk9660` then writes in full.
pub trait SubBlockInit {
    fn init_sub_block(&mut self, block: u32);
}

/// One column of `Unk96e0`'s lerp: `high * (1 - frac) + low * frac`, each product and sum at 24 bits.
pub fn lerp_column(low: f32, high: f32, frac: f32) -> f32 {
    let (low, high, frac) = (f64::from(low), f64::from(high), f64::from(frac));
    let k = round24(1.0 - frac);
    round24(round24(high * k) + round24(low * frac)) as f32
}

/// The output of a lerp column, `(1 - v) * base + v * weight` (1.02's base is the asm's `0.2` at 0x72f428). The first
/// column's weight is `0.15` (0x72d154), the second's is `0`.
pub fn lerp_output(v: f32, base: f32, weight: f32) -> f32 {
    let (v, base, weight) = (f64::from(v), f64::from(base), f64::from(weight));
    round24(round24(round24(1.0 - v) * base) + round24(v * weight)) as f32
}
