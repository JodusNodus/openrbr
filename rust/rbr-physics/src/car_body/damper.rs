//! `CCar::Damp66D0` (exe 0x4e66d0): two dampers. Each cell's CReal gets `0 - (lane0 - lane4) * gain`, the gain being a
//! setup value (def + 0x4460 for the first cell, + 0x4470 for the second). The asm order: the difference, times the
//! gain, then negated. The CReal is dirtied first (`SetDirty`, inside the port).

use super::accessors::set_cell;
use super::layout::{self, Real};
use crate::car_body::store::CarStore;

/// `CCar::Damp66D0(this)` (name ours): the two damper cells at +0xa28 (stride 0x40).
pub fn update_dampers<S: CarStore>(mem: &mut S, car: u32) {
    for i in 0..2 {
        let (a, b) = mem.damper_lanes(car, i);
        let value = 0.0 - (a - b) * mem.damper_gain(car, i);
        let at = layout::damper_real_at(car, i);
        set_cell(Real::load(mem, at), value).store(mem, at);
    }
}
