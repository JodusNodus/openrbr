//! The setup applier. It hands the setup block's words to the drive, the engine, the scratch sim and the four wheels,
//! stores the setup words into the car's setup (the steering lock and the two roll bar stiffnesses), then runs the
//! damage wiring. The order is the original's. The setup block, the wheel list and the setup target are read where the
//! original reads them: after the calls before them, since a call can write memory. The host loads them through `layout`
//! each time (`SetupCalls::setup` and the others).

use super::layout::{self, setup as callee, Setup, SETUP_COPY};

/// The reads, calls and stores the setup applier makes, in the original's order.
pub trait SetupCalls {
    /// The setup block as it is now (`layout::Setup::load`).
    fn setup(&mut self) -> Setup;
    /// The car's four wheel pointers as they are now (`layout::wheel_pointers`).
    fn wheels(&mut self) -> [u32; 4];
    /// The object the setup words land in, as it is now.
    fn setup_target(&mut self) -> u32;
    /// An external call: `callee` on `this`, with `args` on the stack (the first one first).
    fn call(&mut self, callee: u32, this: u32, args: &[u32]);
    /// A word the applier stores (a setup word, at `at`).
    fn store_word(&mut self, at: u32, word: u32);
    /// The damage wiring on the car.
    fn wire_damage(&mut self, car: u32);
}

/// Applies the setup to `car`: the externals, the setup stores into the car's setup, then the damage wiring. Each word is
/// read right before the call or store that uses it.
pub fn apply_setup(calls: &mut impl SetupCalls, car: u32) {
    let drive = calls.setup().drive;
    calls.call(callee::DRIVE_APPLY, layout::drive_at(car), &[drive]);
    let engine = calls.setup().engine;
    calls.call(callee::ENGINE_APPLY, layout::engine_at(car), &[engine]);
    let scratch = calls.setup().scratch;
    calls.call(callee::SCRATCH_APPLY, layout::scratch_at(car), &[scratch]);
    for i in 0..4 {
        let wheel = calls.wheels()[i];
        let args = calls.setup().wheels[i];
        calls.call(callee::WHEEL_APPLY, wheel, &args);
    }
    let target = calls.setup_target();
    let words = calls.setup().setup;
    for (&(at, _), word) in SETUP_COPY.iter().zip(words) {
        calls.store_word(target + at, word);
    }
    calls.wire_damage(car);
}
