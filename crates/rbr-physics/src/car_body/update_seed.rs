//! `CCar::UpdateSeed(p)` (exe 0x4e91f0, `ret 4`): the car's per-step seed. The seed is `p`, or a local block when
//! `+0x1670` is above zero (words 0 to 5 and 7 written, word 6 left as the stack has it). ReadPacket (`0x537f20`, on
//! the scratch sim at +0x850), NopD570 (`0x50d570`, +0x690) and M5970 (`0x505970`, +0x4b0) take the seed; the car
//! keeps seed words 0, 1 and 4 at +0x1694, +0x1698 and +0x16a4. Unless E19IsState2 (`0x499900`, on the object at the
//! global 0x893634) holds, `p`'s flags word (+0x14) picks: bit 0x20 calls S8C (slot 35) and, when that returns 0, S158
//! (slot 86) after clearing +0x9f0; otherwise bit 0x10 calls MotionProbe and emits: the pair (out, 1) to 0x4e8a70 when
//! the probe returns 0, or Assist8DF0 (`0x4e8df0`) when it returns nonzero. Last, the step counter (+0x950) stores, for
//! its first four thousand-step marks, the timer card's word and `p`'s word 4. The asm (not the C++) fixes the order;
//! the calls are answered from the original's records (`car_body/README.md` (git 9c28c0b8)).

use super::layout;
use crate::car_body::store::CarStore;
use crate::mem::Mem;
use crate::physics_core::layout::{SIGNAL_MANAGER, TELEMETRY_GLOBAL};

/// The calls the function makes out of its own body. The test answers them from the original's records.
pub trait UpdateSeedCalls<S = Mem> {
    /// `0x537f20` (`ret 4`) on the scratch sim at car+0x850, with the seed buffer (it may write it).
    fn read_packet(&mut self, mem: &mut S, obj: u32, seed: u32);
    /// `0x50d570` (`ret 4`) on car+0x690, with the seed buffer.
    fn nop_d570(&mut self, mem: &mut S, obj: u32, seed: u32);
    /// `0x505970` (`ret 4`) on car+0x4b0, with the seed buffer.
    fn m5970(&mut self, mem: &mut S, obj: u32, seed: u32);
    /// `E19IsState2` (0x499900) on the object at the global 0x893634: nonzero when its word at +0x10 is 2.
    fn is_state2(&mut self, obj: u32) -> u32;
    /// `S8C` (slot 35) on the car: 0 when the gate is open.
    fn s8c(&mut self, car: u32) -> u32;
    /// `S158` (slot 86) on the car. Runtime dispatch enters the bound slot-86 port; older UpdateSeed records omit its
    /// nested calls, which the legacy vector adapter consumes separately.
    fn s158(&mut self, car: u32);
    /// `CCar::MotionProbe` (0x4e7730) on the car: its return word, and the out word it writes.
    fn motion_probe(&mut self, car: u32) -> (u32, u32);
    /// `0x4e8a70` on the signal object `obj` with the pair (out, 1), passed by pointer.
    fn emit_problem(&mut self, obj: u32, pair: [u32; 2]);
    /// `CCar::Assist8DF0` (0x4e8df0, `ret 12`) on the car with the words 1, 0.5 and 4.0.
    fn assist_df0(&mut self, car: u32, words: [u32; 3]);
}

/// The offset of the problem emit's signal object from the signal global (`+0xd590`).
const PROBLEM_SIGNAL: u32 = 0xD590;

/// The local seed the override takes: its words and values (word 6, at +0x18 of the block, is never written).
const OVERRIDE: [(u32, u32); 7] = [(0, 0), (1, 0), (2, 0x3F80_0000), (3, 0), (4, 0), (5, 0), (7, 0)];

/// The words `Assist8DF0` takes: 1, 0.5 (`0x3f000000`) and 4.0 (`0x40800000`).
const ASSIST_WORDS: [u32; 3] = [1, 0x3F00_0000, 0x4080_0000];

/// `CCar::UpdateSeed(car, p)`. `scratch` is the 0x20-byte block the original uses for the local seed (its stack).
/// Returns the new step count, the eax the original leaves.
pub fn update_seed<S: CarStore>(
    calls: &mut impl UpdateSeedCalls<S>,
    mem: &mut S,
    car: u32,
    p: u32,
    scratch: u32,
) -> u32 {
    let seed = if mem.f32(car + layout::T_OVERRIDE) > 0.0 {
        for (word, value) in OVERRIDE {
            mem.set_u32(scratch + 4 * word, value);
        }
        scratch
    } else {
        p
    };
    calls.read_packet(mem, car + 0x850, seed);
    calls.nop_d570(mem, car + 0x690, seed);
    calls.m5970(mem, car + 0x4B0, seed);
    mem.set_u32(car + layout::STEER_INPUT, mem.u32(seed + 0x10));
    mem.set_u32(car + 0x1694, mem.u32(seed));
    mem.set_u32(car + 0x1698, mem.u32(seed + 4));

    if calls.is_state2(mem.u32(TELEMETRY_GLOBAL)) == 0 {
        let flags = mem.u32(p + 0x14);
        if flags & 0x20 != 0 {
            if calls.s8c(car) == 0 {
                mem.set_u32(car + 0x9F0, 0);
                calls.s158(car);
            }
        } else if flags & 0x10 != 0 {
            let (ret, out) = calls.motion_probe(car);
            if ret == 0 {
                calls.emit_problem(mem.u32(SIGNAL_MANAGER) + PROBLEM_SIGNAL, [out, 1]);
            } else {
                calls.assist_df0(car, ASSIST_WORDS);
            }
        }
    }

    let steps = mem.u32(car + layout::STEP_COUNT);
    if steps.is_multiple_of(1000) && steps / 1000 < 4 {
        let mark = steps / 1000;
        let card = mem.u32(mem.u32(layout::WORLD) + 0x20);
        mem.set_u32(car + 0x954 + 4 * mark, mem.u32(card + 0x18));
        mem.set_u32(car + 0x974 + 0x20 * mark, mem.u32(p + 0x10));
    }
    mem.set_u32(car + layout::STEP_COUNT, steps.wrapping_add(1));
    steps.wrapping_add(1)
}
