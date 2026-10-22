//! The wheel's angular sectors. The tyre block keeps a value per sector (four of them) around the wheel; the sector the
//! wheel is in and the next one are picked from its angle, and the values are interpolated between them. The physics path
//! runs the x87 at 24-bit precision (`notes/physics.md` "x87 and float rules"), so each operation is `fpu::round24` of the
//! exact result.

use crate::fpu::round24;
use std::f32::consts::TAU;

/// The angle added before the sector is picked (named by use: it keeps the angle positive; ten turns).
const ANGLE_OFFSET: f32 = 62.831856;
/// `1 / TAU`.
const INV_TAU: f32 = 0.15915494;

/// The pick: the wheel's sector and the next one (both mod the sector count), the weight of the first, and how many times
/// the next one wrapped past the count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectorPick {
    pub first: u32,
    pub next: u32,
    pub first_weight: f32,
    pub next_wraps: u32,
}

/// An unsigned word as the x87 holds it: it loads the word signed, which is exact, and a negative word gets `2^32` added,
/// which rounds to 24 bits.
fn unsigned(x: u32) -> f64 {
    let signed = x as i32;
    if signed < 0 {
        round24(f64::from(signed) + 4_294_967_296.0)
    } else {
        f64::from(signed)
    }
}

/// The sectors for the wheel `angle` (rad) among `count` (nonzero: the original's division faults on zero). With `shifted =
/// angle + ANGLE_OFFSET`, `turns` is `count * shifted / TAU` truncated to 32 bits (exact for `|x| < 2^63`), `width = TAU /
/// count`, and `first_weight = 1 - (shifted - turns * width) / width`.
pub fn pick_sector(angle: f32, count: u32) -> SectorPick {
    let sectors = unsigned(count);
    let shifted = round24(f64::from(angle) + f64::from(ANGLE_OFFSET));
    let width = round24(f64::from(TAU) / sectors);
    let turns = round24(round24(sectors * shifted) * f64::from(INV_TAU)) as i64 as u32;
    let into_sector = round24(shifted - round24(unsigned(turns) * width));
    let first_weight = round24(1.0 - round24(into_sector / width));
    let next = turns.wrapping_add(1);
    SectorPick { first: turns % count, next: next % count, first_weight: first_weight as f32, next_wraps: next / count }
}

/// The value between two sectors' values: `next * (1 - first_weight) + first * first_weight`, each product and sum at 24
/// bits.
pub fn interpolate(first: f32, next: f32, first_weight: f32) -> f32 {
    let (first, next, weight) = (f64::from(first), f64::from(next), f64::from(first_weight));
    let next_weight = round24(1.0 - weight);
    round24(round24(next * next_weight) + round24(first * weight)) as f32
}

/// `(1 - v) * base + v * target`, each product and sum at 24 bits. 1.02's `base` is 0.2; the interpolated value of the
/// first table moves toward 0.15 and the second toward 0.
pub fn mix_toward(v: f32, base: f32, target: f32) -> f32 {
    let (v, base, target) = (f64::from(v), f64::from(base), f64::from(target));
    round24(round24(round24(1.0 - v) * base) + round24(v * target)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_the_originals() {
        assert_eq!(TAU.to_bits(), 0x40C9_0FDB);
        assert_eq!(ANGLE_OFFSET.to_bits(), 0x427B_53D2);
        assert_eq!(INV_TAU.to_bits(), 0x3E22_F983);
    }
}
