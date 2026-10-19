//! `CCar::Vf150x` (exe 0x4e5f20, slot 84): attaches a wheel to the car. The wheel's flags (+0x60) pick its parts: 0x20
//! puts it in the first empty entry of the car's wheel list (+0x470, its index goes to the wheel at +0xeb4), 0x10 makes it
//! the body (+0x4a0), and 0x400 and 0x800 enter it in the slot lists (+0x480, +0x490) at the index its def gives. Each
//! slot list also makes the two E43VfEC calls: the car's own slot-59 impl on the wheel, then the wheel's on the car. The
//! return is `E43Vf150(wheel)`'s eax. The asm (not the C++ in `src/rbr`) fixes the order.

use super::layout;
use crate::car_body::store::CarStore;

/// The wheel flags (+0x60) the attach tests.
const FLAG_WHEEL_LIST: u32 = 0x20;
const FLAG_BODY: u32 = 0x10;
const FLAG_SLOT_480: u32 = 0x400;
const FLAG_SLOT_490: u32 = 0x800;
/// The def's index fields: +0x4fc picks the entry of the +0x480 list, +0x23c the entry of the +0x490 list.
const DEF_INDEX_480: u32 = 0x4FC;
const DEF_INDEX_490: u32 = 0x23C;
/// The car's lists hold four words each; an index from the def at four or more stores nothing.
const LIST_LEN: u32 = 4;

/// The calls the attach makes. The test answers them from the original's records.
pub trait WheelCalls {
    /// `CEntity::E43VfEC(wheel)` on the car, called directly (0x51a040, ecx = car): the car's own slot-59 impl.
    fn car_vfec(&mut self, car: u32, wheel: u32);
    /// `wheel->E43VfEC(car)` through the wheel's slot 59 (0x51a040, ecx = wheel).
    fn wheel_vfec(&mut self, wheel: u32, car: u32);
    /// `CEntity::E43Vf150(wheel)` on the car (0x51a370): returns eax.
    fn e43vf150(&mut self, car: u32, wheel: u32) -> u32;
}

/// `CCar::Vf150x(wheel)`: returns 0 for a null wheel. Otherwise the flags attach the wheel, and the return is
/// `E43Vf150(wheel)`.
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
    if flags & FLAG_SLOT_480 != 0 {
        enter_slot_list(calls, mem, car, wheel, layout::wheel_slot, DEF_INDEX_480);
    }
    if flags & FLAG_SLOT_490 != 0 {
        enter_slot_list(calls, mem, car, wheel, layout::body_slot, DEF_INDEX_490);
    }
    calls.e43vf150(car, wheel)
}

/// The slot-list half of `Vf150x`: the wheel goes into `slot` (+0x480 or +0x490) at the index the def gives, when that
/// index is below four and the entry is empty. Then the two E43VfEC calls run.
fn enter_slot_list<S: CarStore>(
    calls: &mut impl WheelCalls,
    mem: &mut S,
    car: u32,
    wheel: u32,
    slot: fn(u32, u32) -> u32,
    index_at: u32,
) {
    let index = mem.u32(mem.wheel_def(wheel) + index_at);
    if index < LIST_LEN && mem.u32(slot(car, index)) == 0 {
        mem.set_u32(slot(car, index), wheel);
    }
    calls.car_vfec(car, wheel);
    calls.wheel_vfec(wheel, car);
}
