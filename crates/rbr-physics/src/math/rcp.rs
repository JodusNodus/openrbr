//! `rcpss`/`rcpps` as the reference CPU computes them: a 12-bit table approximation of `1 / x`. `data/rcp.bin` is the
//! table the reference CPU gives: 2048 little-endian words, the second half of `harness/replay-approx.bin` (made by
//! `harness/replay-approx.c`). `harness/replay.py`'s `hw_rcp` is the same model.

const TABLE_BYTES: &[u8] = include_bytes!("../../data/rcp.bin");

fn table(index: u32) -> u32 {
    let at = 4 * index as usize;
    u32::from_le_bytes([TABLE_BYTES[at], TABLE_BYTES[at + 1], TABLE_BYTES[at + 2], TABLE_BYTES[at + 3]])
}

/// `rcpss` on a float. The result is the bits of the reference CPU's answer, including its NaN, zero and infinity cases.
pub fn rcp(x: f32) -> f32 {
    f32::from_bits(rcp_bits(x.to_bits()))
}

/// The top 11 bits of the mantissa pick the table entry (the reciprocal of `1.m`); the exponent shifts it. A result
/// below the normal range is a signed zero.
fn rcp_bits(x: u32) -> u32 {
    let sign = x & 0x8000_0000;
    let exponent = (x >> 23) & 0xFF;
    let mantissa = x & 0x7F_FFFF;
    if exponent == 0 {
        return sign | 0x7F80_0000;
    }
    if exponent == 0xFF {
        return if mantissa == 0 { sign } else { x | 0x40_0000 };
    }
    let t = table(mantissa >> 12);
    let te = ((t >> 23) & 0xFF) as i32 - (exponent as i32 - 127);
    if te <= 0 {
        sign
    } else {
        sign | (te as u32) << 23 | (t & 0x7F_FFFF)
    }
}

#[cfg(test)]
mod tests {
    use super::rcp;

    #[test]
    fn special_values_follow_the_reference_cpu() {
        assert_eq!(rcp(1.0).to_bits(), 0x3F7F_F000);
        assert_eq!(rcp(0.0), f32::INFINITY);
        assert_eq!(rcp(-0.0), f32::NEG_INFINITY);
        assert_eq!(rcp(f32::INFINITY), 0.0);
    }
}
