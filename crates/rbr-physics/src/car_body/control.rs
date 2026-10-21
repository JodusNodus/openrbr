//! The car's per-step control input: the control payload (`notes/gameplay.md` "Control payload") is `p`, or a neutral
//! block when the override timer is above zero (every word zero but the handbrake at 1.0; the gear word is left as
//! the stack has it). The scratch sim, the engine and the drive take the payload; the car keeps its throttle, brake
//! and steering. Unless the replay object is in its state 2, the payload's flags pick: the reset flag asks the car to
//! reset (after clearing the partial-contact flag) unless the car's gate is closed; otherwise the call-for-help flag
//! probes the car's motion and emits: the pair (out, 1) on the problem signal when the probe returns 0, or the
//! outside-assistance worker when it returns nonzero. Last, the step counter stores, for its first four thousand-step
//! marks, the timer's word and the payload's steering. The order is the original's; the calls are answered from the
//! original's records.

use super::layout::{self, control as word};
use crate::car_body::store::CarStore;
use crate::mem::Mem;
use crate::physics_core::layout::TELEMETRY_GLOBAL;

/// The calls the control update makes out of its own body. The test answers them from the original's records.
pub trait ControlCalls<S = Mem> {
    /// The scratch sim reads the payload (it may write the buffer).
    fn read_packet(&mut self, mem: &mut S, scratch: u32, payload: u32);
    /// The engine's no-op on the payload.
    fn engine_idle(&mut self, mem: &mut S, engine: u32, payload: u32);
    /// The drive reads the payload (handbrake and aux).
    fn drive_control(&mut self, mem: &mut S, drive: u32, payload: u32);
    /// Nonzero when the replay object (the global) is in its state 2.
    fn replay_is_state2(&mut self, replay: u32) -> u32;
    /// The car's gate: nonzero when it is closed.
    fn blocked(&mut self, car: u32) -> u32;
    /// The car's reset.
    fn reset_car(&mut self, car: u32);
    /// The motion probe on the car: its return word, and the out word it writes.
    fn motion_probe(&mut self, car: u32) -> (u32, u32);
    /// The problem signal's emit on `emitter` with the pair (out, 1), passed by pointer.
    fn emit_problem(&mut self, emitter: u32, pair: [u32; 2]);
    /// The outside-assistance worker on the car with the words 1, 0.5 and 4.0.
    fn assist(&mut self, car: u32, words: [u32; 3]);
}

/// The neutral payload the override takes: its words and values (the gear word is never written).
const NEUTRAL: [(u32, u32); 7] = [
    (word::THROTTLE, 0),
    (word::BRAKE, 0),
    (word::HANDBRAKE, 0x3F80_0000),
    (word::CLUTCH, 0),
    (word::STEERING, 0),
    (word::FLAGS, 0),
    (word::AUX, 0),
];

/// The words the outside-assistance worker takes after a call for help: 1, 0.5 and 4.0.
const ASSIST_WORDS: [u32; 3] = [1, 0x3F00_0000, 0x4080_0000];

/// The number of timing marks, at every thousandth step.
const MARKS: u32 = 4;

/// Applies the control payload `p` to `car`. `local` is the 0x20-byte block the original uses for the neutral payload (its
/// stack). Returns the new step count.
pub fn apply_control<S: CarStore>(calls: &mut impl ControlCalls<S>, mem: &mut S, car: u32, p: u32, local: u32) -> u32 {
    let payload = if mem.override_timer(car) > 0.0 {
        for (at, value) in NEUTRAL {
            mem.set_u32(local + at, value);
        }
        local
    } else {
        p
    };
    calls.read_packet(mem, layout::scratch_at(car), payload);
    calls.engine_idle(mem, layout::engine_at(car), payload);
    calls.drive_control(mem, layout::drive_at(car), payload);
    mem.set_u32(car + layout::STEER_INPUT, mem.u32(payload + word::STEERING));
    mem.set_u32(car + layout::THROTTLE_AT, mem.u32(payload + word::THROTTLE));
    mem.set_u32(car + layout::BRAKE_AT, mem.u32(payload + word::BRAKE));

    if calls.replay_is_state2(mem.u32(TELEMETRY_GLOBAL)) == 0 {
        let flags = mem.u32(p + word::FLAGS);
        if flags & word::FLAG_RESET != 0 {
            if calls.blocked(car) == 0 {
                mem.set_u32(car + layout::PARTIAL_CONTACT_FLAG, 0);
                calls.reset_car(car);
            }
        } else if flags & word::FLAG_CALL_FOR_HELP != 0 {
            let (ret, out) = calls.motion_probe(car);
            if ret == 0 {
                calls.emit_problem(layout::signal_at(mem, layout::PROBLEM_EMITTER_AT), [out, 1]);
            } else {
                calls.assist(car, ASSIST_WORDS);
            }
        }
    }

    let steps = mem.u32(car + layout::STEP_COUNT);
    if steps.is_multiple_of(1000) && steps / 1000 < MARKS {
        let mark = steps / 1000;
        mem.set_u32(layout::step_mark_timer_at(car, mark), mem.timer_word());
        mem.set_u32(layout::step_mark_steering_at(car, mark), mem.u32(p + word::STEERING));
    }
    mem.set_u32(car + layout::STEP_COUNT, steps.wrapping_add(1));
    steps.wrapping_add(1)
}
