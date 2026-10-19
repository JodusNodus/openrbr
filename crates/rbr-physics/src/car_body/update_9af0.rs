//! `CCar::Update9AF0(arg)` (exe 0x4e9af0, `ret 4`): the car's per-step signal emitter. It returns at once for `arg` 0,
//! and when the timer gap (`0x4a0290`, an x87 double) is below `GAP_LIMIT`. A set flag word (+0x1668) emits code 1. A
//! speed-like word at +0x9ec (at least 1) takes the flow path: a flow check (`0x566640`) emits 4 on failure; otherwise
//! the S8C gate (slot 35) decides an extra emit (`0x4e8df0`), and +0x9ec is cleared. Otherwise, above argument 5, three
//! tests (`0x4ec0d0`, `0x504840`, `0x50ecf0`) that all pass lead to the probe (MotionProbe, `0x4e7730`); a failed test
//! emits code 1. The probe's out word decides the code (3, or 4 when it is 3): the emit `0x4e88f0` takes the pair
//! (arg, code). Every emit goes to a signal object at the global 0x893600 plus an offset. The asm (not the C++) fixes
//! the order; the calls are answered from the original's records (`car_body/README.md` (git 9c28c0b8)).

use super::layout;
use crate::car_body::layout::FLOW_OBJECT;
use crate::car_body::store::CarStore;
use crate::physics_core::layout::SIGNAL_MANAGER;

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait Update9AF0Calls {
    /// `0x4a0290` (x87 double): the timer gap of the world's timer object (`ecx` = its object, `at` = the argument's
    /// address, pushed).
    fn timer_gap(&mut self, obj: u32, at: u32) -> f64;
    /// `0x566640` on the flow object (`obj`, the word at the global 0x165fa90): 1 when it holds, else 0.
    fn flow_check(&mut self, obj: u32) -> u32;
    /// `S8C` (slot 35) on the car: 0 when the gate is open.
    fn s8c(&mut self, car: u32) -> u32;
    /// `0x4e8df0` (`ret 12`) on the car, with the three words 1, 0x3f000000 (0.5) and 0x40800000 (4.0).
    fn emit_df0(&mut self, car: u32, words: [u32; 3]);
    /// `0x4e8770` on the signal object `obj` with the code word.
    fn emit_770(&mut self, obj: u32, code: u32);
    /// `0x4ec0d0` on the object at this+0x9f4: nonzero when it holds.
    fn test_3b20(&mut self, obj: u32) -> u32;
    /// `0x504840` on this+0x4b0: nonzero when it holds.
    fn test_4b0(&mut self, obj: u32) -> u32;
    /// `0x50ecf0` on this+0x690: nonzero when it holds.
    fn test_690(&mut self, obj: u32) -> u32;
    /// `CCar::MotionProbe` (0x4e7730) on the car: its return word, and the out word it writes.
    fn motion_probe(&mut self, car: u32) -> (u32, u32);
    /// `0x4e88f0` on the signal object `obj` with the pair (argument, code).
    fn emit_8f0(&mut self, obj: u32, pair: [u32; 2]);
}

/// The timer gap below which the function returns (`5.0`, asm 0x72ae44).
const GAP_LIMIT: f64 = 5.0;

/// The speed-like word's limit for the flow path (`1.0`, asm 0x795e14), and the probe gate's (`8.0`, asm 0x7316b4).
const FLOW_LIMIT: f32 = 1.0;
const PROBE_GATE: f32 = 8.0;

/// `CCar::Update9AF0(this, arg)` (name ours).
pub fn emit_signals<S: CarStore>(calls: &mut impl Update9AF0Calls, mem: &mut S, car: u32, arg: u32) {
    if arg == 0 {
        return;
    }
    let world = mem.u32(layout::WORLD);
    let at = mem.u32(world + 0x20) + 0x20;
    let gap = calls.timer_gap(mem.u32(world + 0x40), at);
    if GAP_LIMIT > gap {
        return;
    }
    let signals = mem.u32(SIGNAL_MANAGER);
    if mem.u32(car + layout::FLAG_50G) != 0 {
        return calls.emit_770(signals + 0xabb0, 1);
    }
    if mem.f32(car + layout::WALL_CONTACT) >= FLOW_LIMIT {
        if calls.flow_check(mem.u32(FLOW_OBJECT)) == 0 {
            return calls.emit_770(signals + 0xabb0, 4);
        }
        if calls.s8c(car) == 0 {
            calls.emit_df0(car, [1, 0x3f00_0000, 0x4080_0000]);
        }
        mem.set_f32(car + layout::WALL_CONTACT, 0.0);
        return;
    }
    if arg > 5 {
        let holds = calls.test_3b20(mem.u32(car + layout::DAMAGE_HANDLER)) != 0
            && calls.test_4b0(car + 0x4b0) != 0
            && calls.test_690(car + 0x690) != 0;
        if !holds {
            return calls.emit_8f0(signals + 0xcb18, [arg, 1]);
        }
    } else if !(mem.f32(car + layout::PARTIAL_CONTACT) > PROBE_GATE) {
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
    calls.emit_8f0(signals + 0xcb18, [arg, code]);
}
