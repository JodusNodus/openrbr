//! The curve, bracket and blend tables and the `BlendTerms`/`Relaxation` blocks the engine and drive ports read.

use crate::engine_driveline::sim::BlendRec;
use crate::engine_driveline::store::EngineStore;

/// One knot of an engine curve (`E41_CurveEta`'s records, 0x20 bytes apart): the key at +0x0c, the value at +0x1c.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveKnot {
    pub key: f32,
    pub value: f32,
}

/// The output of `E41_UnkD6F0` (0x50d6f0), a record of four words at the pointer it gets: +0x08 (`at_zero`, a float
/// 1.0 or the int 0), +0x0c (`w_low`), +0x10 (`w_mid`), +0x14 (`w_high`). The meaning of the four is not known.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UnitSplit {
    pub at_zero: f32,
    pub w_low: f32,
    pub w_mid: f32,
    pub w_high: f32,
}

impl UnitSplit {
    /// Writes the record at `at` (the four words; the rest of the object is left alone).
    pub fn store<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_unit_split(at, self)
    }
}

/// One knot of a two-level blend table (`SimE060`, `SimE1F0`): the outer record's key, and the records of its sub-table
/// (the record is the sub-table's descriptor for `BlendX`/`BlendY`, count at +4 and records at +8).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlendKnot {
    pub key: f32,
    pub records: Vec<BlendRec>,
}

// The E40 drive workers (group 6). The offsets of their objects live here; the workers see the typed values.
