//! Attaching a wheel to the car. The wheel's flags pick its parts: the wheel-list flag puts it in the first empty entry
//! of the car's wheel list (its index goes to the wheel), the body flag makes it the car's body, and the hinged and
//! mounted flags enter it in those slot lists at the index its setup gives. Each slot list also makes the two
//! exclusion calls: the car excludes the wheel, then the wheel the car. The return is the entity's add-child result.
//! The order is the original's (the reconstructed C++ differs).

use super::layout;
use crate::car_body::store::CarStore;

/// The wheel's attach flags.
const FLAG_WHEEL_LIST: u32 = 0x20;
const FLAG_BODY: u32 = 0x10;
const FLAG_HINGED: u32 = 0x400;
const FLAG_MOUNTED: u32 = 0x800;
/// The car's lists hold four entries each; an index from the setup at four or more stores nothing.
const LIST_LEN: u32 = 4;

/// The calls the attach makes. The test answers them from the original's records.
pub trait WheelCalls {
    /// The car excludes `wheel` from its collisions.
    fn car_excludes(&mut self, car: u32, wheel: u32);
    /// The wheel excludes `car` from its collisions.
    fn wheel_excludes(&mut self, wheel: u32, car: u32);
    /// The entity's add-child on the car: returns its result word.
    fn add_child(&mut self, car: u32, wheel: u32) -> u32;
}

/// Attaches `wheel`: returns 0 for a null wheel. Otherwise the flags attach the wheel, and the return is the
/// add-child's.
pub fn attach_wheel<S: CarStore>(calls: &mut impl WheelCalls, mem: &mut S, car: u32, wheel: u32) -> u32 {
    if wheel == 0 {
        return 0;
    }
    let flags = mem.attach_flags(wheel);
    if flags & FLAG_WHEEL_LIST != 0 {
        let free = (0..LIST_LEN).find(|&i| mem.u32(layout::wheel_list_at(car, i)) == 0);
        if let Some(i) = free {
            mem.set_u32(layout::wheel_list_at(car, i), wheel);
            mem.set_wheel_index(wheel, i);
        }
    }
    if flags & FLAG_BODY != 0 {
        mem.set_body(car, wheel);
    }
    if flags & FLAG_HINGED != 0 {
        let index = mem.hinged_index(wheel);
        enter_slot_list(calls, mem, car, wheel, layout::hinged_slot, index);
    }
    if flags & FLAG_MOUNTED != 0 {
        let index = mem.mounted_index(wheel);
        enter_slot_list(calls, mem, car, wheel, layout::mounted_slot, index);
    }
    calls.add_child(car, wheel)
}

/// The slot-list half of the attach: the wheel goes into `slot` at `index`, when that index is below four and the
/// entry is empty. Then the two exclusion calls run.
fn enter_slot_list<S: CarStore>(
    calls: &mut impl WheelCalls,
    mem: &mut S,
    car: u32,
    wheel: u32,
    slot: fn(u32, u32) -> u32,
    index: u32,
) {
    if index < LIST_LEN && mem.u32(slot(car, index)) == 0 {
        mem.set_u32(slot(car, index), wheel);
    }
    calls.car_excludes(car, wheel);
    calls.wheel_excludes(wheel, car);
}
