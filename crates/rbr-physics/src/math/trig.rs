//! The CRT sine and cosine the original calls (`__CIsin` 0x53f3b0 and `__CIcos` 0x53f3a0, both `fsin`/`fcos`), shared
//! by the entities and the tyre ports. `notes/physics.md` "x87 and float rules" has the x87 model.

const X87_BIAS: i32 = 16383;
// The x87's 66-bit pi/2, scaled by 2^65. This is the full 66-bit fraction from Intel's manual.
const X87_PIO2_66_SCALED: u128 = 0x3_243f_6a88_85a3_08d3;

/// The ten-byte value returned by an x87 transcendental instruction.
///
/// The significand includes its explicit integer bit. `sign_exponent` uses the x87 layout: the sign is bit 15 and the
/// biased exponent occupies bits 0 through 14.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct X87Ext80 {
    significand: u64,
    sign_exponent: u16,
}

impl X87Ext80 {
    /// Constructs a value from the x87 significand and sign/exponent words.
    pub const fn from_parts(significand: u64, sign_exponent: u16) -> Self {
        Self { significand, sign_exponent }
    }

    /// Constructs a value from its ten-byte x87 little-endian encoding.
    pub const fn from_le_bytes(bytes: [u8; 10]) -> Self {
        Self {
            significand: u64::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]),
            sign_exponent: u16::from_le_bytes([bytes[8], bytes[9]]),
        }
    }

    /// Returns the ten-byte x87 little-endian encoding.
    pub const fn to_le_bytes(self) -> [u8; 10] {
        let sig = self.significand.to_le_bytes();
        let se = self.sign_exponent.to_le_bytes();
        [sig[0], sig[1], sig[2], sig[3], sig[4], sig[5], sig[6], sig[7], se[0], se[1]]
    }

    /// Loads an `f32` into the x87 extended format without changing its value.
    pub fn from_f32(value: f32) -> Self {
        let bits = value.to_bits();
        let sign = ((bits >> 31) as u16) << 15;
        let exponent = (bits >> 23) & 0xff;
        let fraction = bits & 0x7f_ffff;
        match exponent {
            0xff => Self { significand: (1_u64 << 63) | (u64::from(fraction) << 40), sign_exponent: sign | 0x7fff },
            0 => {
                if fraction == 0 {
                    Self { significand: 0, sign_exponent: sign }
                } else {
                    let top = 31 - fraction.leading_zeros();
                    let unbiased = top as i32 - 149;
                    Self {
                        significand: u64::from(fraction) << (63 - top),
                        sign_exponent: sign | (X87_BIAS + unbiased) as u16,
                    }
                }
            }
            _ => Self {
                significand: (u64::from((1 << 23) | fraction)) << 40,
                sign_exponent: sign | (X87_BIAS + exponent as i32 - 127) as u16,
            },
        }
    }

    /// Rounds this extended value directly to IEEE binary64, ties to even.
    pub fn to_f64(self) -> f64 {
        let sign = u64::from(self.sign_exponent >> 15) << 63;
        let exponent = self.sign_exponent & 0x7fff;
        if exponent == 0x7fff {
            let fraction = (self.significand & !(1_u64 << 63)) >> 11;
            return f64::from_bits(sign | (0x7ff_u64 << 52) | (fraction & ((1_u64 << 52) - 1)));
        }
        if exponent == 0 {
            return f64::from_bits(sign);
        }

        let mut unbiased = i32::from(exponent) - X87_BIAS;
        if unbiased > 1023 {
            return f64::from_bits(sign | (0x7ff_u64 << 52));
        }
        if unbiased >= -1022 {
            let mut significand = round_shift(self.significand, 11);
            if significand == 1_u64 << 53 {
                significand >>= 1;
                unbiased += 1;
            }
            if unbiased > 1023 {
                return f64::from_bits(sign | (0x7ff_u64 << 52));
            }
            let exponent = (unbiased + 1023) as u64;
            return f64::from_bits(sign | (exponent << 52) | (significand & ((1_u64 << 52) - 1)));
        }

        let shift = (-1011 - unbiased) as u32;
        let fraction = round_shift(self.significand, shift);
        if fraction >= 1_u64 << 52 {
            f64::from_bits(sign | (1_u64 << 52))
        } else {
            f64::from_bits(sign | fraction)
        }
    }

    /// Rounds this extended value directly to IEEE binary32, ties to even.
    pub fn to_f32(self) -> f32 {
        let sign = u32::from(self.sign_exponent >> 15) << 31;
        let exponent = self.sign_exponent & 0x7fff;
        if exponent == 0x7fff {
            let fraction = (self.significand & !(1_u64 << 63)) >> 40;
            return f32::from_bits(sign | (0xff_u32 << 23) | (fraction as u32 & 0x7f_ffff));
        }
        if exponent == 0 {
            return f32::from_bits(sign);
        }

        let mut unbiased = i32::from(exponent) - X87_BIAS;
        if unbiased > 127 {
            return f32::from_bits(sign | (0xff_u32 << 23));
        }
        if unbiased >= -126 {
            let mut significand = round_shift(self.significand, 40);
            if significand == 1_u64 << 24 {
                significand >>= 1;
                unbiased += 1;
            }
            if unbiased > 127 {
                return f32::from_bits(sign | (0xff_u32 << 23));
            }
            let exponent = (unbiased + 127) as u32;
            return f32::from_bits(sign | (exponent << 23) | (significand as u32 & 0x7f_ffff));
        }

        let shift = (-86 - unbiased) as u32;
        let fraction = round_shift(self.significand, shift);
        if fraction >= 1_u64 << 23 {
            f32::from_bits(sign | (1_u32 << 23))
        } else {
            f32::from_bits(sign | fraction as u32)
        }
    }

    /// `fmul` of this value by an `f32` at 24-bit precision control, stored to f32 (`fstp`): the exact product rounded
    /// once, ties to even. Finite operands only; a product below the f32 normal range rounds as `to_f32` does.
    pub fn mul_f32_pc24(self, other: f32) -> f32 {
        let other = Self::from_f32(other);
        let sign = (self.sign_exponent ^ other.sign_exponent) & 0x8000;
        if self.is_zero() || other.is_zero() {
            return f32::from_bits(u32::from(sign) << 16);
        }
        let product = u128::from(self.significand) * u128::from(other.significand);
        let top = product >> 127 != 0;
        let normalized = if top { product } else { product << 1 };
        // 64 kept bits, the rest as a sticky bit: enough for one rounding to 24 bits
        let significand = (normalized >> 64) as u64 | u64::from(normalized as u64 != 0);
        let unbiased = i32::from(self.exponent()) + i32::from(other.exponent()) - 2 * X87_BIAS + i32::from(top);
        Self::from_parts(significand, sign | (unbiased + X87_BIAS) as u16).to_f32()
    }

    fn exponent(self) -> u16 {
        self.sign_exponent & 0x7fff
    }

    fn is_negative(self) -> bool {
        self.sign_exponent & 0x8000 != 0
    }

    fn is_zero(self) -> bool {
        self.exponent() == 0 && self.significand == 0
    }

    fn to_dd(self) -> Dd {
        let hi = self.to_f64();
        let exponent = i32::from(self.exponent()) - X87_BIAS;
        let hi_bits = hi.to_bits() & !(1_u64 << 63);
        let hi_exponent = ((hi_bits >> 52) & 0x7ff) as i32 - 1023;
        let hi_significand = (1_u64 << 52) | (hi_bits & ((1_u64 << 52) - 1));
        let shift = (hi_exponent - exponent + 11) as u32;
        let hi_units = i128::from(hi_significand) << shift;
        let difference = i128::from(self.significand) - hi_units;
        let mut lo = (difference as f64) * 2f64.powi(exponent - 63);
        if self.is_negative() {
            lo = -lo;
        }
        Dd { hi, lo }
    }
}

fn round_shift(value: u64, shift: u32) -> u64 {
    if shift == 0 {
        return value;
    }
    if shift > 64 {
        return 0;
    }
    if shift == 64 {
        return u64::from(value > (1_u64 << 63));
    }
    let kept = value >> shift;
    let mask = (1_u64 << shift) - 1;
    let remainder = value & mask;
    let half = 1_u64 << (shift - 1);
    kept + u64::from(remainder > half || (remainder == half && kept & 1 != 0))
}

/// The x87 `fsin` operation, including its extended result and its 66-bit argument reduction.
pub fn fsin(value: X87Ext80) -> X87Ext80 {
    x87_sincos(value, false)
}

/// The x87 `fcos` operation, including its extended result and its 66-bit argument reduction.
pub fn fcos(value: X87Ext80) -> X87Ext80 {
    x87_sincos(value, true)
}

fn x87_sincos(value: X87Ext80, cosine: bool) -> X87Ext80 {
    let exponent = value.exponent();
    if exponent == 0x7fff {
        if value.significand & !(1_u64 << 63) == 0 {
            return X87Ext80::from_parts(0xc000_0000_0000_0000, 0xffff);
        }
        return X87Ext80::from_parts(value.significand | (1_u64 << 62), value.sign_exponent);
    }
    if value.is_zero() {
        return if cosine { X87Ext80::from_parts(1_u64 << 63, X87_BIAS as u16) } else { value };
    }
    if exponent == 0 {
        return if cosine { X87Ext80::from_parts(1_u64 << 63, X87_BIAS as u16) } else { value };
    }

    let unbiased = i32::from(exponent) - X87_BIAS;
    if unbiased >= 63 {
        // FSIN and FCOS set C2 and leave ST(0) unchanged outside their reduction range.
        return value;
    }
    if unbiased <= -33 {
        return if cosine { X87Ext80::from_parts(1_u64 << 63, X87_BIAS as u16) } else { value };
    }

    let (remainder, quadrant) = if unbiased >= -2 { reduce_x87(value) } else { (value.to_dd(), 0) };
    let (sin, cos) = sin_cos_dd(remainder);
    let result = if cosine {
        match quadrant {
            0 => cos,
            1 => sin.neg(),
            2 => cos.neg(),
            _ => sin,
        }
    } else {
        match quadrant {
            0 => sin,
            1 => cos,
            2 => sin.neg(),
            _ => cos.neg(),
        }
    };
    dd_to_x87(round_to_64_bits(result))
}

fn reduce_x87(value: X87Ext80) -> (Dd, u8) {
    let unbiased = i32::from(value.exponent()) - X87_BIAS;
    let significand = u128::from(value.significand);
    let scaled = significand << (unbiased + 2) as u32;
    let quotient = scaled / X87_PIO2_66_SCALED;
    let rem = scaled % X87_PIO2_66_SCALED;
    let round_up = rem * 2 > X87_PIO2_66_SCALED || (rem * 2 == X87_PIO2_66_SCALED && quotient & 1 != 0);
    let magnitude = quotient + u128::from(round_up);
    let product = magnitude * X87_PIO2_66_SCALED;
    let (negative_remainder, remainder) =
        if scaled >= product { (false, scaled - product) } else { (true, product - scaled) };
    let negative = value.is_negative() ^ negative_remainder;
    let hi = (remainder as f64) * 2f64.powi(-65);
    let hi_units = (hi * 2f64.powi(65)) as u128;
    let lo_units = remainder as i128 - hi_units as i128;
    let sign = if negative { -1.0 } else { 1.0 };
    let remainder = Dd { hi: sign * hi, lo: sign * (lo_units as f64 * 2f64.powi(-65)) };
    let quadrant =
        if value.is_negative() { (0_u8.wrapping_sub((magnitude & 3) as u8)) & 3 } else { (magnitude & 3) as u8 };
    (remainder, quadrant)
}

fn dd_to_x87(value: Dd) -> X87Ext80 {
    if value.hi == 0.0 {
        return X87Ext80::from_parts(0, if value.hi.is_sign_negative() { 0x8000 } else { 0 });
    }
    let negative = value.hi.is_sign_negative();
    let hi = value.hi.abs();
    let bits = hi.to_bits();
    let mut exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let hi_significand = (1_u64 << 52) | (bits & ((1_u64 << 52) - 1));
    let grid = 2f64.powi(exponent - 63);
    let low_sign = if negative { -value.lo } else { value.lo };
    let low_units = (low_sign / grid).round_ties_even() as i128;
    let mut significand = (i128::from(hi_significand) << 11) + low_units;
    if significand < 1_i128 << 63 {
        significand <<= 1;
        exponent -= 1;
    } else if significand >= 1_i128 << 64 {
        significand >>= 1;
        exponent += 1;
    }
    let sign = if negative { 0x8000 } else { 0 };
    X87Ext80::from_parts(significand as u64, sign | (X87_BIAS + exponent) as u16)
}

#[cfg(test)]
mod tests {
    use super::{fcos, fsin, X87Ext80};
    use std::fs;

    fn ulp_distance(a: X87Ext80, b: X87Ext80) -> u128 {
        let a_bytes = a.to_le_bytes();
        let b_bytes = b.to_le_bytes();
        let a_sig = u64::from_le_bytes(a_bytes[..8].try_into().unwrap());
        let b_sig = u64::from_le_bytes(b_bytes[..8].try_into().unwrap());
        let a_se = u16::from_le_bytes([a_bytes[8], a_bytes[9]]);
        let b_se = u16::from_le_bytes([b_bytes[8], b_bytes[9]]);
        let a_negative = a_se & 0x8000 != 0;
        let b_negative = b_se & 0x8000 != 0;
        let a_exp = u128::from(a_se & 0x7fff);
        let b_exp = u128::from(b_se & 0x7fff);
        let a_rank = (a_exp << 63) + u128::from(a_sig & !(1_u64 << 63));
        let b_rank = (b_exp << 63) + u128::from(b_sig & !(1_u64 << 63));
        if a_negative == b_negative {
            a_rank.abs_diff(b_rank)
        } else {
            a_rank + b_rank
        }
    }

    #[test]
    fn f32_round_trip_through_extended() {
        for bits in [
            0,
            0x8000_0000,
            1,
            0x007f_ffff,
            0x0080_0000,
            0x3f80_0000,
            0xbf80_0000,
            0x7f7f_ffff,
            0x7f80_0000,
            0xff80_0000,
        ] {
            let value = f32::from_bits(bits);
            assert_eq!(X87Ext80::from_f32(value).to_f32().to_bits(), bits);
        }
    }

    #[test]
    fn matches_every_fsin_fcos_cache_case() {
        let Some(path) = std::env::var_os("RBR_REPLAY_FPU_CACHE") else {
            return;
        };
        let cache = fs::read(path).expect("read RBR_REPLAY_FPU_CACHE");
        assert_eq!(cache.len() % 46, 0, "malformed replay-fpu cache");
        let mut exact = 0usize;
        let mut one_ulp = 0usize;
        let mut changed_f64 = 0usize;
        let mut changed_f32 = 0usize;
        let mut mismatches_by_op = [0usize; 2];
        let mut mismatches_by_exponent = std::collections::BTreeMap::new();
        for row in cache.chunks_exact(46) {
            let op = row[0];
            if op != 5 && op != 6 {
                continue;
            }
            let input = X87Ext80::from_le_bytes(row[4..14].try_into().unwrap());
            let expected = X87Ext80::from_le_bytes(row[24..34].try_into().unwrap());
            let actual = if op == 5 { fsin(input) } else { fcos(input) };
            let distance = ulp_distance(actual, expected);
            assert!(
                distance <= 1,
                "op={op} input={:02x?}: got {:02x?}, expected {:02x?} ({distance} ulps)",
                input.to_le_bytes(),
                actual.to_le_bytes(),
                expected.to_le_bytes(),
            );
            if distance == 0 {
                exact += 1;
            } else {
                one_ulp += 1;
                mismatches_by_op[usize::from(op == 6)] += 1;
                let exponent = input.exponent();
                let unbiased = if exponent == 0 { -16382 } else { i32::from(exponent) - super::X87_BIAS };
                *mismatches_by_exponent.entry(unbiased).or_insert(0usize) += 1;
                changed_f64 += usize::from(actual.to_f64().to_bits() != expected.to_f64().to_bits());
                changed_f32 += usize::from(actual.to_f32().to_bits() != expected.to_f32().to_bits());
            }
        }
        eprintln!(
            "FSIN/FCOS cache: {exact} exact, {one_ulp} differ by one extended ULP ({changed_f64} also differ as f64, {changed_f32} as f32); by op={mismatches_by_op:?}, by input exponent={mismatches_by_exponent:?}"
        );
    }

    #[test]
    fn matches_fuzzed_x87_cases_of_the_test_machine_within_one_ulp() {
        let mut exact = 0usize;
        let mut one_ulp = 0usize;
        let mut changed_f64 = 0usize;
        let mut changed_f32 = 0usize;
        let mut mismatches_by_op = [0usize; 2];
        let mut mismatches_by_exponent = std::collections::BTreeMap::new();
        for (line_number, line) in include_str!("../../tests/data/x87-trig-fuzz.txt").lines().enumerate() {
            let mut fields = line.split_ascii_whitespace();
            let op = u8::from_str_radix(fields.next().unwrap(), 16).unwrap();
            let input = parse_ext(fields.next().unwrap());
            let expected = parse_ext(fields.next().unwrap());
            assert!(fields.next().is_none());
            let actual = match op {
                5 => fsin(input),
                6 => fcos(input),
                _ => panic!("line {}: unsupported op {op}", line_number + 1),
            };
            let distance = ulp_distance(actual, expected);
            assert!(
                distance <= 1,
                "line {} op={op} input={:02x?}: got {:02x?}, expected {:02x?} ({distance} ulps)",
                line_number + 1,
                input.to_le_bytes(),
                actual.to_le_bytes(),
                expected.to_le_bytes(),
            );
            if distance == 0 {
                exact += 1;
            } else {
                one_ulp += 1;
                mismatches_by_op[usize::from(op == 6)] += 1;
                let exponent = input.exponent();
                let unbiased = if exponent == 0 { -16382 } else { i32::from(exponent) - super::X87_BIAS };
                *mismatches_by_exponent.entry(unbiased).or_insert(0usize) += 1;
                changed_f64 += usize::from(actual.to_f64().to_bits() != expected.to_f64().to_bits());
                changed_f32 += usize::from(actual.to_f32().to_bits() != expected.to_f32().to_bits());
            }
        }
        eprintln!(
            "fuzzed test-machine FSIN/FCOS: {exact} exact, {one_ulp} differ by one extended ULP ({changed_f64} also differ as f64, {changed_f32} as f32); by op={mismatches_by_op:?}, by input exponent={mismatches_by_exponent:?}"
        );
    }

    fn parse_ext(hex: &str) -> X87Ext80 {
        assert_eq!(hex.len(), 20);
        let mut bytes = [0; 10];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        X87Ext80::from_le_bytes(bytes)
    }
}

/// The CRT sine the original calls (`__CIsin`, 0x53f3b0, `fsin`), rounded from the modeled extended result to f64.
pub fn x87_sin(angle: f32) -> f64 {
    fsin(X87Ext80::from_f32(angle)).to_f64()
}

/// The CRT cosine (`__CIcos`, 0x53f3a0), rounded from the modeled extended result to f64. The original's `fcos` result is
/// an extended (64-bit mantissa) value, which this f64 does not keep: a caller that subtracts the cosine from 1 must use
/// `one_minus_x87_cos` instead.
pub fn x87_cos(angle: f32) -> f64 {
    fcos(X87Ext80::from_f32(angle)).to_f64()
}

/// `1 - cos(angle)` from the CRT cosine's extended result (`__CIcos`, then `fsubr` of 1.0), rounded once to f64: the
/// subtraction is exact on the 64-bit mantissa, so a caller at 53-bit precision control gets this value.
pub fn one_minus_x87_cos_f64(angle: f32) -> f64 {
    let c = fcos(X87Ext80::from_f32(angle));
    // c = significand * 2^-shift; 1 - c is exact in units of 2^-shift while 1 fits in an i128.
    let shift = X87_BIAS + 63 - i32::from(c.sign_exponent & 0x7FFF);
    if !(63..=126).contains(&shift) {
        return 1.0 - c.to_f64();
    }
    let units = i128::from(c.significand) * if c.sign_exponent >> 15 != 0 { -1 } else { 1 };
    ((1i128 << shift) - units) as f64 * (-f64::from(shift)).exp2()
}

/// `1 - cos(angle)` as the original stores it. `__CIcos` leaves the extended cosine in st0 (64-bit mantissa); the
/// caller runs `fsubr` of 1.0 at the game's 24-bit precision control and `fstp`s the result to f32, so the subtraction
/// rounds once, to f32. This evaluates the cosine in double-double (`cos_dd`), rounds it to a 64-bit mantissa with ties
/// to even (`round_to_64_bits`), forms 1 minus it exactly, and rounds once to f32. Non-finite angles give NaN.
///
/// The last bit of the x87's own `fcos` is not reproduced: the model is the correctly rounded 64-bit cosine. The angles
/// are small in the callers; `cos_dd` reduces by a three-part pi/2, which is exact enough for |angle| below 2^20.
pub fn one_minus_x87_cos(angle: f32) -> f32 {
    if !angle.is_finite() {
        return f32::NAN;
    }
    let c = round_to_64_bits(cos_dd(angle as f64));
    let s = two_sum(1.0, -c.hi);
    (s.hi + (s.lo - c.lo)) as f32
}

/// An unevaluated sum `hi + lo` of two f64, with `|lo| <= ulp(hi) / 2`: about 106 bits.
#[derive(Clone, Copy)]
struct Dd {
    hi: f64,
    lo: f64,
}

impl Dd {
    fn neg(self) -> Dd {
        Dd { hi: -self.hi, lo: -self.lo }
    }

    fn add(self, b: Dd) -> Dd {
        let s = two_sum(self.hi, b.hi);
        let t = two_sum(self.lo, b.lo);
        let s = quick_two_sum(s.hi, s.lo + t.hi);
        quick_two_sum(s.hi, s.lo + t.lo)
    }

    fn sub(self, b: Dd) -> Dd {
        self.add(b.neg())
    }

    fn mul(self, b: Dd) -> Dd {
        let p = two_prod(self.hi, b.hi);
        quick_two_sum(p.hi, p.lo + (self.hi * b.lo + self.lo * b.hi))
    }

    fn div_f64(self, b: f64) -> Dd {
        let q1 = self.hi / b;
        let r = self.sub(two_prod(q1, b));
        quick_two_sum(q1, r.hi / b)
    }
}

fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    Dd { hi: s, lo: (a - (s - bb)) + (b - bb) }
}

fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd { hi: s, lo: b - (s - a) }
}

fn two_prod(a: f64, b: f64) -> Dd {
    let p = a * b;
    Dd { hi: p, lo: a.mul_add(b, -p) }
}

/// `cos(x)` in double-double, for a finite `x` of moderate size: `x - k pi/2` in three parts of pi/2, then the Taylor
/// series of the sine and the cosine of the remainder (|remainder| under pi/4 and a little more), by the quadrant `k`.
fn cos_dd(x: f64) -> Dd {
    const PI_2_MID: f64 = 6.123_233_995_736_766e-17;
    const PI_2_LO: f64 = -1.497_384_904_859_169_8e-33;
    let k = (x * std::f64::consts::FRAC_2_PI).round();
    let r = Dd { hi: x, lo: 0.0 }
        .sub(two_prod(k, std::f64::consts::FRAC_PI_2))
        .sub(two_prod(k, PI_2_MID))
        .sub(Dd { hi: k * PI_2_LO, lo: 0.0 });
    let (sin, cos) = sin_cos_dd(r);
    match (k as i64).rem_euclid(4) {
        0 => cos,
        1 => sin.neg(),
        2 => cos.neg(),
        _ => sin,
    }
}

/// The sine and the cosine of `r` (|r| under about 0.8) by their Taylor series, thirteen terms each.
fn sin_cos_dd(r: Dd) -> (Dd, Dd) {
    let r2 = r.mul(r);
    let mut sin = r;
    let mut cos = Dd { hi: 1.0, lo: 0.0 };
    let (mut term_sin, mut term_cos) = (r, cos);
    for n in 1..=13 {
        let n = n as f64;
        term_cos = term_cos.mul(r2).div_f64((2.0 * n - 1.0) * (2.0 * n)).neg();
        term_sin = term_sin.mul(r2).div_f64((2.0 * n) * (2.0 * n + 1.0)).neg();
        cos = cos.add(term_cos);
        sin = sin.add(term_sin);
    }
    (sin, cos)
}

/// Rounds `v` to a 64-bit mantissa, ties to even: the extended format `fcos` returns. The grid is `2^(e - 63)` for the
/// binade `e` of the value; `v.hi` is a multiple of that grid, so only `v.lo` is rounded.
fn round_to_64_bits(v: Dd) -> Dd {
    if v.hi == 0.0 {
        return v;
    }
    let bits = v.hi.to_bits();
    let mut e = ((bits >> 52) & 0x7ff) as i32 - 1023;
    // A power of two with a negative low part lies just below it, in the binade under.
    if bits & ((1 << 52) - 1) == 0 && v.lo < 0.0 {
        e -= 1;
    }
    let grid = 2f64.powi(e - 63);
    quick_two_sum(v.hi, (v.lo / grid).round_ties_even() * grid)
}
