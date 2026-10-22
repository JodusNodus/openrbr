//! The car's wiring to its damage object, which it keeps as its damage handler. The damage object's first method runs
//! first, then the attaches on the drive, the engine and the light system (their argument is the address of that
//! part's damage state inside the damage object), then the three lists of bodies (each entry: the wheels get an
//! attach with their damage and a refresh; a non-null hinged or mounted body gets its two calls). Then the control
//! update runs on a local neutral block (every word zero but the gear word, which is left as the stack has it), and
//! the car-init emit takes the pair (the car, its sort key). The order is the original's; the calls are answered from
//! the original's records.

use super::layout::{self, control as word, handler, wiring as callee};
use crate::car_body::store::CarStore;
use crate::mem::Mem;

/// The calls the wiring makes out of its own body. The test answers them from the original's records.
pub trait WiringCalls<S = Mem> {
    /// A call out of the body: `callee` on `this`, with the words `args` (the callee's arguments, first one first). The
    /// callee's return is not used.
    fn call(&mut self, callee: u32, this: u32, args: &[u32]);
    /// The car-init emit on `emitter` with the pair (the car, its sort key), passed by pointer.
    fn emit(&mut self, emitter: u32, pair: [u32; 2]);
}

/// Wires `car` to its damage object. `block` is the local block the control update takes as its payload (0x20 bytes on
/// the original's stack).
pub fn wire_damage<S: CarStore>(calls: &mut impl WiringCalls<S>, mem: &mut S, car: u32, block: u32) {
    let damage = mem.u32(layout::damage_source_at(car));
    mem.set_u32(car + layout::DAMAGE_HANDLER, damage);
    calls.call(layout::virtual_target(mem, damage, callee::DAMAGE_FIRST_SLOT), damage, &[]);
    calls.call(callee::DRIVE_ATTACH, layout::drive_at(car), &[damage + handler::DRIVE]);
    calls.call(callee::ENGINE_ATTACH, layout::engine_at(car), &[damage + handler::ENGINE]);
    calls.call(callee::LIGHTS_ATTACH, layout::light_system_at(car), &[damage + handler::LIGHTS]);
    for i in 0..4 {
        let wheel = mem.u32(layout::wheel_list_at(car, i));
        let attach = [handler::wheel(damage, i), handler::wheel_pair(damage, i), handler::wheel_value8(damage, i)];
        calls.call(callee::WHEEL_ATTACH, wheel, &attach);
        calls.call(callee::WHEEL_REFRESH, mem.u32(layout::wheel_list_at(car, i)), &[]);
    }
    for i in 0..4 {
        let hinged = mem.u32(layout::hinged_slot(car, i));
        if hinged != 0 {
            calls.call(callee::HINGED_ATTACH, hinged, &[handler::hinged(damage, i)]);
            calls.call(callee::HINGED_FRAME_SETUP, hinged, &[]);
        }
    }
    for i in 0..4 {
        let mounted = mem.u32(layout::mounted_slot(car, i));
        if mounted != 0 {
            calls.call(callee::MOUNTED_ATTACH, mounted, &[handler::mounted(damage, i)]);
            calls.call(callee::MOUNTED_REFRESH, mounted, &[]);
        }
    }
    calls.call(callee::DRIVE_HOOK, layout::drive_at(car), &[]);
    for at in [word::THROTTLE, word::BRAKE, word::HANDBRAKE, word::CLUTCH, word::STEERING, word::FLAGS, word::AUX] {
        mem.set_u32(block + at, 0);
    }
    calls.call(callee::APPLY_CONTROL, car, &[block]);
    calls.emit(layout::signal_at(mem, layout::CAR_INIT_EMITTER_AT), [car, mem.sort_key(car)]);
}
