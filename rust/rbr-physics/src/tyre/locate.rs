//! 0x528980 `E46_IdxWrite` on the tyre's tables: `Step` searches its 4-, 6- or 8-entry tables with it. The search is
//! `math::table::table_index_frac`; this takes the table's entries and the hint segment, and returns the new one.

use std::cell::Cell;

use super::layout::{Outside, Segment, Table};
use crate::math::table::table_index_frac;

/// `E46_IdxWrite`: searches the sorted `table` for `x`, from the hint `seg.lo`. Returns the new `lo`, `hi` and `frac`.
/// `lo` is -1 when a NaN `x` walks below the table: the word below it is in the `Table`. `Outside` when the hint is
/// outside the window, or when the walk reads an index outside it.
pub fn locate(seg: Segment, table: &Table, x: f32) -> Result<Segment, Outside> {
    let n = table.count() as i32;
    let hint = seg.lo as i32;
    if !(-1..n).contains(&hint) {
        return Err(Outside { index: hint, len: table.count() });
    }
    let beyond = Cell::new(None);
    let at = |i: i32| {
        table.at(i).unwrap_or_else(|| {
            if beyond.get().is_none() {
                beyond.set(Some(i));
            }
            f32::NAN
        })
    };
    let r = table_index_frac(at, n, x, hint);
    if let Some(index) = beyond.get() {
        return Err(Outside { index, len: table.count() });
    }
    Ok(Segment { lo: r.lo as u32, hi: r.hi as u32, frac: r.frac })
}
