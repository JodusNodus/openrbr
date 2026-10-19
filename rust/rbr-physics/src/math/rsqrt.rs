//! `rsqrtss` as the reference CPU computes it. The instruction is an approximation with a 12-bit table, not the
//! IEEE square root. `data/rsqrt.bin` is the table the reference CPU gives: 2048 little-endian words, the first
//! half of `harness/replay-approx.bin` (made by `harness/replay-approx.c`, which measured it on that CPU).
//! `harness/replay.py`'s `hw_rsqrt` is the same model.

const TABLE_BYTES: &[u8] = include_bytes!("../../data/rsqrt.bin");

fn table(index: u32) -> u32 {
    let at = 4 * index as usize;
    u32::from_le_bytes([TABLE_BYTES[at], TABLE_BYTES[at + 1], TABLE_BYTES[at + 2], TABLE_BYTES[at + 3]])
}

/// `rsqrtss` on a float. The result is the bits of the reference CPU's answer, including its NaN and zero cases.
pub fn rsqrt(x: f32) -> f32 {
    f32::from_bits(rsqrt_bits(x.to_bits()))
}

/// `rsqrtss` on the bits of its operand. The sign, the exponent and the top 11 bits of the mantissa (with the
/// exponent's parity) pick the table entry; an odd exponent uses the upper half of the table.
fn rsqrt_bits(x: u32) -> u32 {
    let negative = x >> 31 != 0;
    let exponent = (x >> 23) & 0xFF;
    let mantissa = x & 0x7F_FFFF;
    if exponent == 0 {
        return if negative { 0xFF80_0000 } else { 0x7F80_0000 };
    }
    if exponent == 0xFF {
        return if mantissa == 0 {
            if negative {
                0xFFC0_0000
            } else {
                0
            }
        } else {
            x | 0x40_0000
        };
    }
    if negative {
        return 0xFFC0_0000;
    }
    // The unbiased exponent, split into its parity and the even remainder that shifts the result.
    let unbiased = exponent as i32 - 127;
    let odd = unbiased & 1;
    let half = (unbiased - odd) / 2;
    table(((odd as u32) << 10) | (mantissa >> 13)).wrapping_sub((half << 23) as u32)
}

#[cfg(test)]
mod tests {
    use super::{rsqrt, table};

    #[test]
    fn table_entries_match_the_reference_cpu() {
        assert_eq!(table(0), 0x3F7F_F000);
        assert_eq!(table(1024), 0x3F34_F800);
        assert_eq!(table(2047), 0x3F00_0800);
    }

    #[test]
    fn special_values_follow_the_reference_cpu() {
        assert_eq!(rsqrt(1.0).to_bits(), 0x3F7F_F000);
        assert_eq!(rsqrt(4.0).to_bits(), 0x3EFF_F000);
        assert_eq!(rsqrt(0.0), f32::INFINITY);
        assert_eq!(rsqrt(f32::INFINITY), 0.0);
        assert_eq!(rsqrt(-1.0).to_bits(), 0xFFC0_0000);
    }
}
