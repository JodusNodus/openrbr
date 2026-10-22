//! The car's retire and stand-still check, run each step with the timekeeper's update word `arg`. It returns at once
//! for `arg` 0, and while the timer gap is below `GAP_LIMIT`. A set overload flag (the acceleration passed the dangerous
//! limit) emits the retire code 1. A wall contact time of at least 1 s takes the flow path: the flow check emits the
//! retire code 4 on failure; otherwise an open car gate decides an extra outside-assistance call, and the wall contact
//! time is cleared. Otherwise, with `arg` above 5, three tests (the damage handler's, the drive's, the engine's) that
//! all hold lead to the motion probe; a failed test emits the stand-still code 1. Without them, a partial contact time
//! above 8 s leads to the probe. The probe's out word decides the code (3, or 4 when it is 3): the stand-still emit
//! takes the pair (arg, code). Every emit goes to an emitter of the signal manager. The order is the original's; the
//! calls are answered from the original's records.

use super::layout;
use crate::car_body::store::CarStore;

/// The calls the check makes out of its own body. The test answers them from the original's records.
pub trait RetireCheckCalls {
    /// The timer gap as a double (`obj` is the call's `this`, `at` the address of the timer's double).
    fn timer_gap(&mut self, obj: u32, at: u32) -> f64;
    /// The flow object's check: 1 when it holds, else 0.
    fn flow_holds(&mut self, flow: u32) -> u32;
    /// The car's gate: nonzero when it is closed.
    fn blocked(&mut self, car: u32) -> u32;
    /// The outside-assistance worker on the car, with the three words 1, 0.5 and 4.0.
    fn assist(&mut self, car: u32, words: [u32; 3]);
    /// The retire signal's emit on `emitter` with the code word.
    fn emit_retire(&mut self, emitter: u32, code: u32);
    /// The damage handler's windshield test: nonzero when it holds.
    fn damage_test(&mut self, handler: u32) -> u32;
    /// The drive's test: nonzero when it holds.
    fn drive_test(&mut self, drive: u32) -> u32;
    /// The engine's test: nonzero when it holds.
    fn engine_test(&mut self, engine: u32) -> u32;
    /// The motion probe on the car: its return word, and the out word it writes.
    fn motion_probe(&mut self, car: u32) -> (u32, u32);
    /// The stand-still signal's emit on `emitter` with the pair (arg, code).
    fn emit_stand_still(&mut self, emitter: u32, pair: [u32; 2]);
}

/// The timer gap, s, below which the check returns.
const GAP_LIMIT: f64 = 5.0;

/// The wall contact time, s, from which the flow path runs.
const WALL_CONTACT_LIMIT: f32 = 1.0;

/// The partial contact time, s, above which the probe runs (for `arg` of 5 or less).
const PARTIAL_CONTACT_LIMIT: f32 = 8.0;

/// The words of the outside-assistance call: 1, 0.5 and 4.0.
const ASSIST_WORDS: [u32; 3] = [1, 0x3F00_0000, 0x4080_0000];

/// Checks whether `car` is retired or stands still.
pub fn check_retire<S: CarStore>(calls: &mut impl RetireCheckCalls, mem: &mut S, car: u32, arg: u32) {
    if arg == 0 {
        return;
    }
    let (keeper, at) = mem.timer_gap_args();
    let gap = calls.timer_gap(keeper, at);
    if GAP_LIMIT > gap {
        return;
    }
    let retire = layout::signal_at(mem, layout::RETIRE_EMITTER_AT);
    let stand_still = layout::signal_at(mem, layout::STAND_STILL_EMITTER_AT);
    if mem.overload_flag(car) {
        return calls.emit_retire(retire, 1);
    }
    if mem.wall_contact(car) >= WALL_CONTACT_LIMIT {
        if calls.flow_holds(mem.u32(layout::FLOW_OBJECT)) == 0 {
            return calls.emit_retire(retire, 4);
        }
        if calls.blocked(car) == 0 {
            calls.assist(car, ASSIST_WORDS);
        }
        mem.set_wall_contact(car, 0.0);
        return;
    }
    if arg > 5 {
        let holds = calls.damage_test(mem.damage_handler(car)) != 0
            && calls.drive_test(layout::drive_at(car)) != 0
            && calls.engine_test(layout::engine_at(car)) != 0;
        if !holds {
            return calls.emit_stand_still(stand_still, [arg, 1]);
        }
    } else if !(mem.partial_contact(car) > PARTIAL_CONTACT_LIMIT) {
        return;
    }
    let (ret, out) = calls.motion_probe(car);
    let code = if ret != 0 {
        0
    } else if out == 4 {
        return;
    } else {
        3 + u32::from(out == 3)
    };
    calls.emit_stand_still(stand_still, [arg, code]);
}
