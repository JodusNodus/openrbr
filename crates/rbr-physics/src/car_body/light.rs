//! The light system's pieces: the light block's reset, the sub-block's init, the light def's static table, and the
//! per-light emit and update.

use super::layout::lights as mem_at;
use crate::car_body::store::CarStore;

/// Zeroes the sub-block's two words and its float.
pub fn sub_block_init<S: CarStore>(mem: &mut S, at: u32) {
    mem.set_u32(at + mem_at::SUB_BLOCK_FIRST, 0);
    mem.set_u32(at + mem_at::SUB_BLOCK_SECOND, 0);
    mem.set_f32(at + mem_at::SUB_BLOCK_WEIGHT, 0.0);
}

/// The address of the light def's static table.
pub fn def_table() -> u32 {
    mem_at::DEF_TABLE
}

/// Sets the four groups of words to 1, in the original's order, then the flag word.
pub fn block_reset<S: CarStore>(mem: &mut S, block: u32) {
    for group in 0..mem_at::RESET_GROUPS {
        let base = block + mem_at::RESET_BASE + 8 * group;
        for offset in mem_at::RESET_WORDS {
            mem.set_u32(base.wrapping_add_signed(offset), 1);
        }
    }
    mem.set_u32(block + mem_at::RESET_FLAG, 1);
}

/// The calls the light emit makes: the emitter's emit, answered from the records. The payload is the five words the
/// original stores on the stack: {1, i, s2, s3, kind}.
pub trait LightCalls {
    /// Emits on the light emitter, with the payload words.
    fn emit_light(&mut self, emitter: u32, payload: [u32; 5]);
}

/// For each of the four lights whose head is set, emits the light `i` with its state: `s2` is 1 when `scaled` is
/// nonzero and the light's first state word is set, else 2; `s3` is 2 when the second state word is zero, else 1.
pub fn emit_lights<S: CarStore>(calls: &mut impl LightCalls, mem: &S, kind: u32, lights: u32, states: u32, scaled: u32) {
    for i in 0..4u32 {
        if mem.u32(mem_at::light_head_at(lights, i)) == 0 {
            continue;
        }
        let on = scaled != 0 && mem.u32(mem_at::state_first_at(states, i)) != 0;
        let s2 = if on { 1 } else { 2 };
        let s3 = u32::from(mem.u32(mem_at::state_second_at(states, i)) == 0) + 1;
        let emitter = super::layout::signal_at(mem, mem_at::LIGHT_EMITTER_AT);
        calls.emit_light(emitter, [1, i, s2, s3, kind]);
    }
}

/// For each gate, when its flag word or the first flag word is set, emits its lights with the scale `def word *
/// multiplier` (the multiplier is read once), and clears the gate's flag; the first flag word is cleared at the end. The
/// flag block is read again for each gate.
pub fn update<S: CarStore>(calls: &mut impl LightCalls, mem: &mut S, system: u32) {
    let flags_at = |mem: &S| mem.u32(mem_at::flags_at(system));
    let multiplier = mem.u32(flags_at(mem) + mem_at::FLAGS_MULTIPLIER);
    for gate in mem_at::GATES {
        let flags = flags_at(mem);
        if mem.u32(flags + gate.flag) != 0 || mem.u32(flags + mem_at::FLAGS_ALL) != 0 {
            let scale = mem.u32(mem.u32(mem_at::def_at(system)) + gate.def).wrapping_mul(multiplier);
            let lights = mem.u32(mem_at::lights_at(system)) + gate.lights;
            emit_lights(calls, mem, gate.kind, lights, flags + gate.states, scale);
            let flags = flags_at(mem);
            mem.set_u32(flags + gate.flag, 0);
        }
    }
    let flags = flags_at(mem);
    mem.set_u32(flags + mem_at::FLAGS_ALL, 0);
}
