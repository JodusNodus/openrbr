//! The anti-roll bars: bar `i` of the two (front, rear: `AntiRollBarStiffness0` and `1`) has the travels of its two
//! sides in its cell's first two columns, and its force is `0 - (first - second) * stiffness`, stored in the bar's force
//! cell. The order is the original's: the difference, times the stiffness, then negated. The cell is dirtied first (inside
//! `set_cell`).

use super::accessors::set_cell;
use super::layout::{self, Real};
use crate::car_body::store::CarStore;

/// Updates the force of each anti-roll bar from the travels of its two sides.
pub fn update_roll_bars<S: CarStore>(mem: &mut S, car: u32) {
    for i in 0..2 {
        let (first, second) = mem.roll_bar_travels(car, i);
        let value = 0.0 - (first - second) * mem.roll_bar_stiffness(car, i);
        let at = layout::roll_bar_force_at(car, i);
        set_cell(Real::load(mem, at), value).store(mem, at);
    }
}
