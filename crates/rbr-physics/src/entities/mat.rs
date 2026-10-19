//! The material table (`E44MatTable`, filled by `E44MatTable::Build`, 0x520190) and the CMaterial
//! tweak-key dictionary slot (`CMaterial::Unk20`, 0x520820).

/// One source record: the material's tag (its table slot), and the low byte and high word of the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialEntry {
    pub tag: u8,
    pub low: u8,
    pub high: u32,
}

/// The two words stored at `8 * tag` of the table. The low byte is zero-extended to a word: the
/// original's bytes 1-3 of it are zeros it stores itself (see `entities/README.md` (git 9c28c0b8)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialSlot {
    pub low: u32,
    pub high: u32,
}

/// The table `Build` produces: `self_ref` is the table's own address, `count` the number of source
/// entries, and `slots` the stored `(tag, slot)` pairs in order (a later entry with the same tag wins).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialTable {
    pub self_ref: u32,
    pub count: u32,
    pub slots: Vec<(u8, MaterialSlot)>,
}

impl MaterialTable {
    /// Builds the table at address `table` from the source entries, in source order.
    pub fn build(table: u32, entries: &[MaterialEntry]) -> Self {
        let slots = entries.iter().map(|e| (e.tag, MaterialSlot { low: e.low as u32, high: e.high })).collect();
        MaterialTable { self_ref: table, count: entries.len() as u32, slots }
    }
}

/// `CMaterial::Unk20` (0x520820): the CMaterial tweak-key dictionary.
pub const TWEAK_DICT: u32 = 0x01659aec; // dict of NPhysics::CMaterial (notes/physics.md "Renames and tweak keys")

pub fn tweak_dict() -> u32 {
    TWEAK_DICT
}

/// The sheet header in a slot's low word: the width (byte 0), the height (byte 1) and the wrap flags (byte 2: bit 0
/// wraps the column, bit 1 the row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sheet {
    pub width: u8,
    pub height: u8,
    pub wrap_flags: u8,
}

/// A wrapping sheet of width or height 0 divides by zero in the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DivideByZero;

/// `cvttss2si`: truncation, with the hardware's indefinite value for NaN and out-of-range inputs.
fn trunc_i32(f: f32) -> i32 {
    if f.is_nan() || !(-2147483648.0..2147483648.0).contains(&f) {
        i32::MIN
    } else {
        f as i32
    }
}

/// One axis: `f * size` truncated, then wrapped (the remainder; a negative value counts back from `size`, so an
/// exact multiple gives `size`) or clamped to `0..=size - 1`.
fn axis(size: u8, wraps: bool, f: f32) -> Result<i32, DivideByZero> {
    let n = trunc_i32(size as f32 * f);
    let size = size as i32;
    if wraps {
        if size == 0 {
            return Err(DivideByZero);
        }
        Ok(if n >= 0 { n % size } else { size - n.wrapping_neg() % size })
    } else if n < 0 {
        Ok(0)
    } else {
        Ok(n.min(size - 1))
    }
}

/// The byte offset of the texel `f` selects (`E44MatTable` slot lookup 0x520010): `column + row * height`, the
/// original's row stride being the height.
pub fn texel_offset(sheet: Sheet, f: f32) -> Result<u32, DivideByZero> {
    let column = axis(sheet.width, sheet.wrap_flags & 1 != 0, f)?;
    let row = axis(sheet.height, sheet.wrap_flags & 2 != 0, f)?;
    Ok((row.wrapping_mul(sheet.height as i32).wrapping_add(column)) as u32)
}

#[cfg(test)]
mod sheet_tests {
    use super::*;

    const CLAMP: Sheet = Sheet { width: 4, height: 4, wrap_flags: 0 };
    const WRAP: Sheet = Sheet { width: 4, height: 4, wrap_flags: 3 };

    #[test]
    fn clamps_to_the_edges() {
        assert_eq!(texel_offset(CLAMP, 0.5), Ok(2 + 2 * 4));
        assert_eq!(texel_offset(CLAMP, 5.0), Ok(3 + 3 * 4));
        assert_eq!(texel_offset(CLAMP, -1.0), Ok(0));
    }

    #[test]
    fn wraps_and_counts_back_for_negatives() {
        assert_eq!(texel_offset(WRAP, 1.25), Ok(1 + 4));
        assert_eq!(texel_offset(WRAP, -0.25), Ok(3 + 3 * 4));
        // An exact negative multiple gives the size itself, as the original does.
        assert_eq!(texel_offset(WRAP, -1.0), Ok(4 + 4 * 4));
    }

    #[test]
    fn a_wrapping_empty_sheet_divides_by_zero() {
        let empty = Sheet { width: 0, height: 4, wrap_flags: 1 };
        assert_eq!(texel_offset(empty, 0.5), Err(DivideByZero));
    }
}
